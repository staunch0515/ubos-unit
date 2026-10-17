//! The single-node Server over UBTP (PH-1 step 14b, DEC-075; DSN-NOD-001, -003, -006,
//! -008, -010…-018; STD-PROTO-020, -021).
//!
//! The Server gives the kernel's in-process binding (`ubos-dvm::ubtp`, the adapter
//! operation `ubtp`) its network transports: every message that arrives over WebSocket,
//! HTTP or the REST facade is decoded, authenticated, routed to its tenant and VAE, handed
//! to that one binding and answered with the frames it produced, so that the semantics are
//! the binding's whatever the transport (FR-INT-015). Each VAE runs on one actor thread
//! ([`VaeHandle`]: the kernel is single-threaded and its store is not `Send`), the Tokio
//! runtime serves the connections (DSN-NOD-001), and after a mutate the Server fans the
//! emits of the commit out to the other sessions of the VAE and polls once a second as the
//! fallback (DSN-NOD-014).
//!
//! Bindings (STD-PROTO-020): WebSocket at `GET /ubtp/v1` with the subprotocol `ubtp.v1`,
//! one message per frame, one session per connection; HTTP at `POST /ubtp/v1` taking a
//! message or a list of messages with the session in the `Ubtp-Session` header (the
//! handshake opens the session and returns its id), and `GET /ubtp/v1/stream?session=` as
//! server-sent events carrying the `emit` frames; the REST facade of STD-PROTO-021 under
//! `/api/v1`. Frames are limited to 16 MB (DSN-NOD-018) and each session's push queue to
//! 1,000 frames (DSN-NOD-016): a queue that overflows closes the stream, and the client
//! resumes its subscriptions from their cursors (DSN-NOD-015).
//!
//! Credentials (step 14c, [`crate::identity`]): a handshake over the network carries
//! `{kind: "password", username, password, totp?, device?}` (a sign-in that opens an
//! identity session, whose token the result frame returns under `credentials`),
//! `{kind: "bearer", token}` (an identity session) or `{kind: "api_key", key}`; the
//! Server validates them with the tenant's identity (the token names the tenant) and turns
//! them into the binding's in-process credentials: the principal, its kind and role, the
//! credential's grants (scopes intersected with the principal's grants, DSN-NOD-153) and
//! the authentication level. In the `dev` preset `{kind: "dev", principal?, principal_kind?,
//! role?, grants?}` is accepted as well. In-process credentials arriving over the network
//! are refused. A session's credential is re-validated at least every `RECHECK_SECS` on
//! its next message (revocation, timeouts, principal state; DSN-NOD-106), and a revoked
//! identity session closes the connections bound to it at once. The REST facade takes
//! `Authorization: Bearer <token>`; the identity API lives under `/api/v1/identity`
//! ([`identity_api`]). The node's security log receives the kernel's decision and
//! security logs once a second (DSN-NOD-111).

use crate::config::Config;
use crate::identity::{Authenticated, Identity};
use crate::leases::{Leases, MemLeases};
use crate::schedule::{MemScheduleStore, ScheduleStore};
use crate::services::{EventFilter, EventStore, JobFilter, JobStore, MemEventLog, MemJobQueue};
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use ubos_dvm::host::Vae;
use ubos_dvm::types::BpaError;

/// Largest frame (DSN-NOD-018).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
/// Bounded push queue per session (DSN-NOD-016).
pub const QUEUE: usize = 1_000;
/// The WebSocket subprotocol (STD-PROTO-020).
pub const SUBPROTOCOL: &str = "ubtp.v1";
/// Effects a releaser claims at once (DSN-NOD-201).
pub const RELEASE_BATCH: usize = 500;
/// The releaser's claim lease (DSN-NOD-201: 30 s by default).
pub const RELEASE_LEASE_MS: i64 = 30_000;
/// The releaser's poll interval; a commit through the Server wakes it at once.
pub const RELEASE_POLL_MS: u64 = 200;
/// Deliveries a worker runs per VAE per round.
pub const DELIVERY_BATCH: usize = 100;
/// The lease of a claimed job (heartbeated every third of it).
pub const DELIVERY_LEASE_MS: i64 = 60_000;
/// The job kinds the worker runs.
pub const JOB_KINDS: &[&str] = &["delivery", "port", "bulk", "schedule"];
/// The interval of the job sweeper (FR-EVT-032).
pub const JOB_SWEEP_SECS: u64 = 30;
/// The interval of the scan for VAEs with work.
pub const WORK_SCAN_SECS: u64 = 30;
/// Timers due within this horizon open their VAE (a scan interval and a margin).
pub const WORK_HORIZON_SECS: i64 = 60;
/// The retention sweep's interval (DSN-NOD-202).
pub const SWEEP_SECS: u64 = 600;
/// Seconds between re-validations of a session's credential (DSN-NOD-106: a local cache
/// no longer than the NR-SEC-005 bound).
pub const RECHECK_SECS: i64 = 5;

/// The HTTP status of each registered error code (STD-ERR-002, the `HTTP` column).
const STATUS: &[(&str, u16)] = &[
    ("MODEL.DECLARATION_INCOMPLETE", 422),
    ("MODEL.CONSTRAINT_VIOLATED", 422),
    ("MODEL.NARROWING_VIOLATION", 422),
    ("MODEL.LINEARIZATION_INCONSISTENT", 422),
    ("MODEL.INHERITANCE_CYCLE", 422),
    ("MODEL.KIND_MISMATCH", 422),
    ("MODEL.KIND_IMMUTABLE", 409),
    ("MODEL.ABSTRACT_INSTANTIATION", 422),
    ("MODEL.KEY_CONFLICT", 409),
    ("MODEL.REFERENCE_INVALID", 422),
    ("MODEL.REFERENCE_RESTRICTED", 409),
    ("MODEL.CARDINALITY_VIOLATED", 422),
    ("MODEL.PORT_SIGNATURE_INCOMPATIBLE", 422),
    ("MODEL.PORT_NOT_IMPLEMENTED", 501),
    ("MODEL.LENS_REQUIRED", 422),
    ("MODEL.MIGRATION_REQUIRED", 422),
    ("MODEL.CURRENCY_MISMATCH", 422),
    ("MODEL.OBJECT_TOO_LARGE", 413),
    ("MODEL.INVALID_VALUE", 422),
    ("MODEL.UNKNOWN_TYPE", 422),
    ("MODEL.INVALID_NAME", 422),
    ("VER.CHANGE_SET_REQUIRED", 409),
    ("VER.MERGE_CONFLICT", 409),
    ("VER.INVALID_BRANCH_NAME", 422),
    ("VER.TAG_IMMUTABLE", 409),
    ("VER.INHERITANCE_CYCLE", 422),
    ("VER.SELECTION_NOT_CLOSED", 422),
    ("VER.MERGE_NOT_ALLOWED", 409),
    ("VER.BRANCH_ARCHIVED", 409),
    ("TIME.OVERLAPPING_VALIDITY", 409),
    ("TIME.RESERVED_FIELD", 422),
    ("LEDG.IMMUTABLE_ENTRY", 409),
    ("LEDG.ALREADY_REVERSED", 409),
    ("LEDG.REVERSAL_NOT_REVERSIBLE", 409),
    ("LEDG.UNBALANCED", 422),
    ("LEDG.PERIOD_CLOSED", 409),
    ("LEDG.CORRECTION_TARGET_MISSING", 409),
    ("TXN.HEAD_MOVED", 409),
    ("TXN.READ_SET_CHANGED", 409),
    ("TXN.ALREADY_EXISTS", 409),
    ("TXN.IDEMPOTENCY_MISMATCH", 422),
    ("LOGIC.LIMIT_EXCEEDED", 429),
    ("LOGIC.CANCELLED", 499),
    ("LOGIC.FORBIDDEN_CAPABILITY", 403),
    ("LOGIC.FORBIDDEN_TARGET", 403),
    ("LOGIC.CONTEXT_IMMUTABLE", 500),
    ("LOGIC.PARAMETER_FORBIDDEN", 400),
    ("LOGIC.TYPE_ERROR", 422),
    ("LOGIC.UNKNOWN_FIELD", 422),
    ("LOGIC.DIVISION_BY_ZERO", 422),
    ("LOGIC.FLOAT_TO_DECIMAL", 422),
    ("LOGIC.ABI_UNSUPPORTED", 501),
    ("LOGIC.UNVERIFIED_ASSET", 403),
    ("LOGIC.ASSET_QUARANTINED", 503),
    ("LOGIC.SCRIPT_TOO_COMPLEX", 422),
    ("LOGIC.REPLAY_INPUT_MISSING", 500),
    ("LOGIC.CONNECTOR_UNAVAILABLE", 503),
    ("LOGIC.CONNECTOR_ERROR", 502),
    ("LOGIC.CLASSIFICATION_EGRESS_DENIED", 403),
    ("LOGIC.EGRESS_NOT_ALLOWED", 403),
    ("LOGIC.NOT_AVAILABLE", 501),
    ("RULE.PARSE_ERROR", 422),
    ("RULE.UNKNOWN_PROPERTY", 422),
    ("RULE.AMBIGUOUS_CASCADE", 422),
    ("RULE.LOCKED_OVERRIDE", 409),
    ("RULE.DECLARATION_UNSATISFIED", 422),
    ("RULE.INVARIANT_VIOLATED", 422),
    ("RULE.VALIDATION_FAILED", 422),
    ("RULE.DECISION_DENIED", 403),
    ("RULE.DECISION_REQUIRES", 202),
    ("RULE.NO_MATCH", 422),
    ("RULE.MULTIPLE_MATCH", 422),
    ("RULE.UNTYPED_ACCESS", 422),
    ("RULE.TESTS_FAILED", 409),
    ("FLOW.TRANSITION_NOT_ALLOWED", 409),
    ("FLOW.GUARD_FAILED", 409),
    ("FLOW.STATE_DIRECT_WRITE", 409),
    ("FLOW.PORT_UNAVAILABLE", 409),
    ("FLOW.EFFECT_FAILED", 422),
    ("FLOW.FIELD_RULE_VIOLATED", 422),
    ("EVT.UNKNOWN_EVENT_TYPE", 422),
    ("QRY.SCAN_LIMIT", 422),
    ("QRY.INVALID_QUERY", 400),
    ("QRY.NOT_FOUND_AT_TIME", 404),
    ("QRY.CLASSIFICATION_NOT_EMBEDDABLE", 422),
    ("IAM.DENIED", 403),
    ("IAM.NOT_FOUND", 404),
    ("IAM.DELEGATION_EXPIRED", 401),
    ("IAM.KEY_UNAVAILABLE", 503),
    ("TEN.SUSPENDED", 423),
    ("TEN.OVERRIDE_LOCKED", 409),
    ("PKG.SIGNATURE_INVALID", 422),
    ("PKG.MANIFEST_INVALID", 422),
    ("PKG.NAMESPACE_VIOLATION", 409),
    ("PKG.CONFIGURATION_INCOMPLETE", 409),
    ("PROOF.SIGNATURE_INVALID", 422),
    ("PROOF.VALUE_ERASED", 410),
    ("PROOF.VERIFICATION_FAILED", 422),
    ("SYNC.INTEGRITY_FAILURE", 422),
    ("SYNC.UPSTREAM_UNAVAILABLE", 502),
    ("AI.CLASSIFICATION_BLOCKED", 403),
    ("AI.BUDGET_EXHAUSTED", 429),
    ("AI.OUTPUT_INVALID", 502),
    ("AI.PROVIDER_UNAVAILABLE", 503),
    ("AUD.LEGAL_HOLD_ACTIVE", 423),
    ("BILL.LICENCE_INACTIVE", 402),
    ("BILL.NOT_ENTITLED", 402),
    ("OPS.QUOTA_EXCEEDED", 429),
    ("INT.PROTOCOL_ERROR", 400),
    ("INT.VERSION_UNSUPPORTED", 426),
    ("INT.CURSOR_EXPIRED", 410),
    ("ADDR.CONFLICTING_SELECTORS", 400),
    ("NOD.STORAGE_UNAVAILABLE", 503),
    ("NOD.BLOB_UNAVAILABLE", 503),
];

/// Log levels of the node (`log.level`, reloadable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Errors only.
    Error = 0,
    /// And warnings.
    Warn = 1,
    /// And information.
    Info = 2,
    /// And each request.
    Debug = 3,
    /// Everything.
    Trace = 4,
}

static LOG_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);

/// Set the log level by name (`error` … `trace`).
pub fn set_log_level(name: &str) {
    let l = crate::config::LEVELS
        .iter()
        .position(|x| *x == name)
        .unwrap_or(2) as u8;
    LOG_LEVEL.store(l, std::sync::atomic::Ordering::Relaxed);
}

/// Whether messages of a level are logged.
pub fn log_enabled(level: LogLevel) -> bool {
    level as u8 <= LOG_LEVEL.load(std::sync::atomic::Ordering::Relaxed)
}

/// The HTTP status of an error code: the registered status, else 500.
pub fn status_of(code: &str) -> u16 {
    STATUS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, s)| *s)
        .unwrap_or(500)
}

/// An RFC 9457 problem with the `code` and `explanation` extensions (STD-PROTO-021).
pub fn problem(e: &BpaError) -> Value {
    let status = status_of(e.code);
    let explanation = match &e.details {
        Some(d) => d.clone(),
        None => json!(e.message),
    };
    json!({"type": format!("urn:ubos:error:{}", e.code), "title": e.code, "status": status,
        "detail": e.message, "code": e.code, "explanation": explanation})
}

// ---- the VAE actor ----

/// The result of an operation with the kernel's phases when it was traced.
type Traced = (Result<Value, BpaError>, Vec<ubos_dvm::types::trace::Phase>);

struct Call {
    op: String,
    input: Value,
    trace: bool,
    reply: tokio::sync::oneshot::Sender<Traced>,
}

/// A VAE on its own thread: the kernel's operations run there in order (the kernel is
/// single-threaded), callers send operations and await their results.
#[derive(Clone)]
pub struct VaeHandle {
    tx: std::sync::mpsc::Sender<Call>,
}

impl VaeHandle {
    /// Start the actor; `open` runs on the actor's thread and opens the VAE.
    pub fn spawn<F>(open: F) -> Result<VaeHandle, BpaError>
    where
        F: FnOnce() -> Result<Vae, BpaError> + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel::<Call>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), BpaError>>();
        std::thread::Builder::new()
            .name("ubos-vae".into())
            .spawn(move || {
                let mut vae = match open() {
                    Ok(v) => {
                        let _ = ready_tx.send(Ok(()));
                        v
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(call) = rx.recv() {
                    if call.trace {
                        ubos_dvm::types::trace::begin();
                    }
                    let r = host_call(&mut vae, &call.op, &call.input);
                    let phases = if call.trace {
                        ubos_dvm::types::trace::take()
                    } else {
                        Vec::new()
                    };
                    let _ = call.reply.send((r, phases));
                }
            })
            .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(VaeHandle { tx }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                "the VAE thread ended before it opened",
            )),
        }
    }

    /// Run one operation on the VAE and await its result.
    pub async fn call(&self, op: &str, input: Value) -> Result<Value, BpaError> {
        self.send(op, input, false).await.0
    }

    /// Run one operation and return the kernel's phases with its result (DSN-NOD-610).
    pub async fn call_traced(&self, op: &str, input: Value) -> Traced {
        self.send(op, input, true).await
    }

    async fn send(&self, op: &str, input: Value, trace: bool) -> Traced {
        let (reply, rx) = tokio::sync::oneshot::channel();
        let gone = || BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended");
        if self
            .tx
            .send(Call {
                op: op.to_string(),
                input,
                trace,
                reply,
            })
            .is_err()
        {
            return (Err(gone()), Vec::new());
        }
        rx.await.unwrap_or_else(|_| (Err(gone()), Vec::new()))
    }

    /// Run one operation from a thread outside the runtime (suites and setup).
    pub fn call_blocking(&self, op: &str, input: Value) -> Result<Value, BpaError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Call {
                op: op.to_string(),
                input,
                trace: false,
                reply,
            })
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?;
        rx.blocking_recv()
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?
            .0
    }
}

/// The operations of the VAE actor: the host's own (`host.mode`, `host.tick`,
/// `host.release`; step 15a) and every operation of the host API.
fn host_call(vae: &mut Vae, op: &str, input: &Value) -> Result<Value, BpaError> {
    match op {
        "host.mode" => {
            vae.set_server_mode(
                input["claimed_release"].as_bool().unwrap_or(true),
                input["auto_tick"].as_bool().unwrap_or(false),
            );
            // the platform classes of step 15b (DEC-076)
            let platform = crate::platform::ensure(vae).err().map(|e| e.to_string());
            Ok(json!({"ok": true, "platform_error": platform}))
        }
        "host.tick" => vae.tick().map(|n| json!({"pending": n})),
        "host.release" => {
            // claim, deliver to the event log, enqueue the subscriptions' deliveries, then
            // acknowledge: a crash before the acknowledgement repeats the round, and the
            // dedupe keys keep each delivery single (DSN-NOD-201, -204)
            let rows = vae.call(
                "outbox_claim",
                &json!({"holder": input["holder"].as_str().unwrap_or("node"),
                    "n": input["n"].as_u64().unwrap_or(RELEASE_BATCH as u64),
                    "lease_ms": input["lease_ms"].as_i64().unwrap_or(RELEASE_LEASE_MS)}),
            )?;
            let mut effects: Vec<Value> = rows["rows"].as_array().cloned().unwrap_or_default();
            if effects.is_empty() {
                return Ok(json!({"released": 0, "deliveries": 0}));
            }
            for e in effects.iter_mut() {
                if let Some(m) = e.as_object_mut() {
                    m.remove("attempts");
                }
            }
            vae.services().outbox.release(&effects)?;
            let deliveries = crate::platform::dispatch(vae, &effects)?;
            let ids: Vec<Value> = effects.iter().map(|e| e["effect_id"].clone()).collect();
            vae.call("outbox_ack", &json!({"ids": ids}))?;
            Ok(json!({"released": effects.len(), "deliveries": deliveries}))
        }
        "host.deliver" => crate::platform::deliver(vae, &input["job"], &input["principal"]),
        "host.run_port" => crate::platform::run_port(vae, &input["job"], &input["principal"]),
        "host.run_schedule" => {
            crate::platform::run_schedule(vae, &input["job"], &input["principal"])
        }
        "host.schedules" => crate::platform::schedules(vae, input["known"].as_str()),
        "host.verify_roots" => {
            let head = vae.call("branch_info", &json!({"branch": "main"}))?["cseq"]
                .as_u64()
                .unwrap_or(0);
            let last = input["last"].as_u64().unwrap_or(20);
            let r = ubos_dvm::verify::verify_roots(
                vae.engine_mut(),
                "main",
                head.saturating_sub(last.saturating_sub(1)).max(1),
                head,
            )?;
            Ok(
                json!({"checked": r.checked, "findings": r.findings.iter().map(|f| json!({"check": f.check, "key": f.key, "detail": f.detail})).collect::<Vec<_>>()}),
            )
        }
        "host.field_classification" => {
            // the classification of a field, declared or inherited (STD-FND-027)
            let field = input["field"].as_str().unwrap_or_default();
            let mut class = input["class"].as_str().unwrap_or_default().to_string();
            let mut default: Option<String> = None;
            for _ in 0..64 {
                let d = vae.engine_mut().registry.decl(&class)?;
                if default.is_none() {
                    default = d["classification_default"].as_str().map(str::to_string);
                }
                if let Some(f) = d["fields"]
                    .as_array()
                    .and_then(|a| a.iter().find(|f| f["name"] == json!(field)))
                {
                    let c = f["classification"]
                        .as_str()
                        .map(str::to_string)
                        .or(default)
                        .unwrap_or_else(|| "internal".into());
                    return Ok(json!({"classification": c, "type": f["type"]}));
                }
                match d["parent"].as_str() {
                    Some(p) => class = p.to_string(),
                    None => break,
                }
            }
            Err(BpaError::new(
                "LOGIC.UNKNOWN_FIELD",
                format!("{class} has no field {field}"),
            ))
        }
        "host.project_tenant" => {
            crate::platform::project_tenant(vae, input).map(|w| json!({"written": w}))
        }
        "host.set_paused" => crate::platform::set_paused(
            vae,
            input["schedule"].as_str().unwrap_or_default(),
            input["paused"].as_bool().unwrap_or(false),
            input["by"].as_str().unwrap_or_default(),
        ),
        "host.run_bulk" => crate::platform::run_bulk(
            vae,
            &input["job"],
            input["max_chunks"].as_u64().unwrap_or(1),
        ),
        "host.initiator" => Ok(json!(crate::platform::initiator_of(
            vae,
            input["process_id"].as_str().unwrap_or_default()
        ))),
        "host.job_state" => crate::platform::job_state(
            vae,
            input["job_id"].as_str().unwrap_or_default(),
            &input["set"],
        )
        .map(|_| json!(true)),
        "host.jobs_open" => {
            let known: std::collections::HashSet<String> = input["known"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            crate::platform::open_jobs(vae, &known).map(|j| json!({"items": j}))
        }
        "host.enqueue" => {
            let specs = input["specs"].as_array().cloned().unwrap_or_default();
            crate::platform::create_jobs(vae, &specs).map(|n| json!({"created": n}))
        }
        "host.listeners" => crate::platform::listeners(
            vae,
            input["branch"].as_str().unwrap_or("main"),
            input["type"].as_str(),
            input["handler"].as_str(),
        )
        .map(|l| json!({"items": l})),
        _ => vae.call(op, input),
    }
}

// ---- the server ----

/// The edition preset (DSN-NOD-004): `dev` accepts `dev` credentials besides those of
/// local identity and lets the facade open sessions for the default principal; `server`
/// accepts the credentials of local identity only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Development: `dev` credentials, the facade opens sessions for the default principal.
    Dev,
    /// Server: the credentials of local identity only.
    Server,
}

/// What the Server serves: the VAEs it can open, by `(tenant, authority)`, and the
/// identity of each tenant.
pub trait VaeSource: Send + Sync {
    /// Open the VAE of a tenant and authority as an actor; `None` when there is none.
    fn open(&self, tenant: &str, authority: &str) -> Result<Option<VaeHandle>, BpaError>;
    /// The local identity of a tenant; `None` when the tenant is unknown.
    fn identity(&self, tenant: &str) -> Result<Option<Identity>, BpaError>;
    /// The secret store of a tenant (the operator interface, DSN-NOD-130); `None` when
    /// the source has none.
    fn secrets(&self, _tenant: &str) -> Result<Option<crate::keys::SecretStore>, BpaError> {
        Ok(None)
    }
    /// The `(tenant, authority)` of the VAEs with work: unacknowledged effects or timers due
    /// within `horizon_secs`. The Server opens them at start and on every rescan, so that
    /// the releaser delivers what a crash left pending (NR-DUR-006) and due timers fire on
    /// time (NR-PERF-012) without a session. `None` when the source cannot tell (the
    /// Server then opens its default VAE).
    fn with_work(&self, _horizon_secs: i64) -> Result<Option<Vec<(String, String)>>, BpaError> {
        Ok(None)
    }
    /// The leadership leases (DSN-NOD-228).
    fn leases(&self) -> Arc<dyn Leases>;
    /// The event log of a VAE (DSN-NOD-202); `None` when the source keeps none.
    fn events(
        &self,
        _tenant: &str,
        _authority: &str,
    ) -> Result<Option<Arc<dyn EventStore>>, BpaError> {
        Ok(None)
    }
    /// The jobs of a VAE (deliveries and dead letters); `None` when the source keeps none.
    fn jobs(&self, _tenant: &str, _authority: &str) -> Result<Option<Arc<dyn JobStore>>, BpaError> {
        Ok(None)
    }
    /// Probe the database (DSN-NOD-613): `{ok, writable, clock_skew_ms}`; `None` when
    /// the source has none.
    fn probe_database(&self) -> Option<Value> {
        None
    }
    /// Probe a tenant's keys; `None` when the source has no key service.
    fn probe_keys(&self, _tenant: &str) -> Option<Value> {
        None
    }
    /// The database part of a diagnostic bundle (DSN-NOD-615); `None` when the source has
    /// no database.
    fn diagnostics(&self) -> Option<Value> {
        None
    }
    /// Create a named restore point with the heads recorded (step 16e); `None` when the
    /// source has no database.
    fn restore_point(
        &self,
        _target: &crate::backup::Target,
        _name: &str,
    ) -> Option<Result<Value, BpaError>> {
        None
    }
    /// The WAL archiver's state (`pg_stat_archiver`); `None` when the source has none.
    fn archiver(&self) -> Option<Value> {
        None
    }
    /// The file service of a tenant (step 17a); `None` when the source keeps files in
    /// memory (the Server then keeps them per tenant itself).
    fn files(&self, _tenant: &str) -> Result<Option<crate::files::Files>, BpaError> {
        Ok(None)
    }
    /// The registry record of a tenant (DSN-NOD-401); `None` when the source keeps no
    /// registry.
    fn tenant_record(&self, _slug: &str) -> Result<Option<Value>, BpaError> {
        Ok(None)
    }
    /// Change a tenant's platform settings; `None` when the source keeps no registry.
    fn set_tenant_settings(&self, _slug: &str, _patch: &Value) -> Result<Option<Value>, BpaError> {
        Ok(None)
    }
    /// Record an applied configuration version (FR-OPS-093); `false` when the source keeps
    /// no log (the Server then keeps it in memory).
    fn record_config(&self, _version: &Value) -> Result<bool, BpaError> {
        Ok(false)
    }
    /// The configuration versions applied by this node, newest first; `None` when the
    /// source keeps no log.
    fn config_versions(&self, _node: &str, _limit: i64) -> Result<Option<Vec<Value>>, BpaError> {
        Ok(None)
    }
    /// The schedule state of a VAE (step 15d); `None` when the source keeps none (the
    /// Server then keeps it in memory).
    fn schedules(
        &self,
        _tenant: &str,
        _authority: &str,
    ) -> Result<Option<Arc<dyn ScheduleStore>>, BpaError> {
        Ok(None)
    }
}

/// One VAE already open (suites, the memory preset): every tenant and authority maps to
/// it, and one memory identity serves every tenant name.
pub struct SingleVae {
    /// The VAE.
    pub handle: VaeHandle,
    /// Its identity (memory).
    pub identity: Arc<Identity>,
    /// Its event log, when the VAE releases into one (the memory preset).
    pub events: Option<Arc<MemEventLog>>,
    /// Its job queue, when the VAE enqueues into one (the memory preset).
    pub jobs: Option<Arc<MemJobQueue>>,
    /// Leases in memory.
    pub leases: Arc<MemLeases>,
}

impl VaeSource for SingleVae {
    fn open(&self, _tenant: &str, _authority: &str) -> Result<Option<VaeHandle>, BpaError> {
        Ok(Some(self.handle.clone()))
    }
    fn identity(&self, _tenant: &str) -> Result<Option<Identity>, BpaError> {
        // the Server's fallback identity (`Server::single`) serves every tenant name
        Ok(None)
    }
    fn leases(&self) -> Arc<dyn Leases> {
        self.leases.clone()
    }
    fn events(
        &self,
        _tenant: &str,
        _authority: &str,
    ) -> Result<Option<Arc<dyn EventStore>>, BpaError> {
        Ok(self.events.clone().map(|e| e as Arc<dyn EventStore>))
    }
    fn jobs(&self, _tenant: &str, _authority: &str) -> Result<Option<Arc<dyn JobStore>>, BpaError> {
        Ok(self.jobs.clone().map(|j| j as Arc<dyn JobStore>))
    }
}

/// The VAEs of the node's tenancy registry (DSN-NOD-008).
pub struct NodeVaes(pub Arc<crate::Node>);

impl VaeSource for NodeVaes {
    fn open(&self, tenant: &str, authority: &str) -> Result<Option<VaeHandle>, BpaError> {
        let Some(t) = self.0.tenant(tenant)? else {
            return Ok(None);
        };
        if self.0.vae(&t, authority)?.is_none() {
            return Ok(None);
        }
        let node = self.0.clone();
        let authority = authority.to_string();
        VaeHandle::spawn(move || node.open_vae(&t, &authority)).map(Some)
    }
    fn identity(&self, tenant: &str) -> Result<Option<Identity>, BpaError> {
        match self.0.tenant(tenant)? {
            Some(t) => self.0.identity(&t).map(Some),
            None => Ok(None),
        }
    }
    fn secrets(&self, tenant: &str) -> Result<Option<crate::keys::SecretStore>, BpaError> {
        match self.0.tenant(tenant)? {
            Some(t) => self.0.secrets(&t).map(Some),
            None => Ok(None),
        }
    }
    fn with_work(&self, horizon_secs: i64) -> Result<Option<Vec<(String, String)>>, BpaError> {
        self.0.vaes_with_work(horizon_secs).map(Some)
    }
    fn leases(&self) -> Arc<dyn Leases> {
        self.0.leases()
    }
    fn events(
        &self,
        tenant: &str,
        authority: &str,
    ) -> Result<Option<Arc<dyn EventStore>>, BpaError> {
        let Some(t) = self.0.tenant(tenant)? else {
            return Ok(None);
        };
        Ok(self
            .0
            .events(&t, authority)?
            .map(|e| Arc::new(e) as Arc<dyn EventStore>))
    }
    fn jobs(&self, tenant: &str, authority: &str) -> Result<Option<Arc<dyn JobStore>>, BpaError> {
        let Some(t) = self.0.tenant(tenant)? else {
            return Ok(None);
        };
        Ok(self
            .0
            .jobs(&t, authority)?
            .map(|j| Arc::new(j) as Arc<dyn JobStore>))
    }
    fn record_config(&self, version: &Value) -> Result<bool, BpaError> {
        self.0.record_config(version).map(|_| true)
    }
    fn tenant_record(&self, slug: &str) -> Result<Option<Value>, BpaError> {
        self.0.tenant_record(slug)
    }
    fn probe_database(&self) -> Option<Value> {
        Some(self.0.probe_database())
    }
    fn probe_keys(&self, tenant: &str) -> Option<Value> {
        Some(self.0.probe_keys(tenant))
    }
    fn diagnostics(&self) -> Option<Value> {
        Some(self.0.diagnostics())
    }
    fn restore_point(
        &self,
        target: &crate::backup::Target,
        name: &str,
    ) -> Option<Result<Value, BpaError>> {
        Some(crate::backup::restore_point(
            self.0.owner_db(),
            target,
            name,
        ))
    }
    fn files(&self, tenant: &str) -> Result<Option<crate::files::Files>, BpaError> {
        let Some(t) = self.0.tenant(tenant)? else {
            return Ok(None);
        };
        let db = self.0.tenant_db(&t)?;
        let keys = self.0.keys(&t)?;
        let link_key = keys.derive("ubos.files.v1", "links");
        Ok(Some(crate::files::Files::new(
            Arc::new(crate::files::PgRows::new(db.clone(), &t.id)),
            keys,
            Arc::new(crate::services::PgBlobs::new(db, &t.id)),
            &t.id,
            link_key,
        )))
    }
    fn archiver(&self) -> Option<Value> {
        Some(
            self.0
                .owner_db()
                .query(
                    "SELECT archived_count, failed_count, last_archived_wal, \
                     (last_failed_time IS NOT NULL AND (last_archived_time IS NULL OR last_failed_time > last_archived_time)) AS failing, \
                     current_setting('archive_mode') AS archive_mode FROM pg_stat_archiver",
                    &[],
                )
                .map(|r| r.into_iter().next().unwrap_or(Value::Null))
                .unwrap_or_else(|e| json!({"error": e.to_string()})),
        )
    }
    fn set_tenant_settings(&self, slug: &str, patch: &Value) -> Result<Option<Value>, BpaError> {
        self.0.set_tenant_settings(slug, patch).map(Some)
    }
    fn config_versions(&self, node: &str, limit: i64) -> Result<Option<Vec<Value>>, BpaError> {
        self.0.config_versions(Some(node), limit).map(Some)
    }
    fn schedules(
        &self,
        tenant: &str,
        authority: &str,
    ) -> Result<Option<Arc<dyn ScheduleStore>>, BpaError> {
        let Some(t) = self.0.tenant(tenant)? else {
            return Ok(None);
        };
        Ok(self
            .0
            .schedules(&t, authority)?
            .map(|j| Arc::new(j) as Arc<dyn ScheduleStore>))
    }
}

/// This node's view of a VAE's leadership (DSN-NOD-228).
#[derive(Debug, Clone)]
struct Leader {
    held: bool,
    token: i64,
    renewed: std::time::Instant,
}

/// The identity credential a Server session was opened with.
#[derive(Clone)]
struct Auth {
    token: String,
    source: String,
    tenant: String,
    principal: String,
    credential_id: String,
    scopes: Option<Vec<String>>,
    checked_at: i64,
}

struct SessionEntry {
    /// The degraded-mode epoch this session has been told of (DSN-NOD-616).
    banner_epoch: u64,
    vae: String,
    tenant: String,
    handle: VaeHandle,
    push: Option<tokio::sync::mpsc::Sender<Value>>,
    /// Frames the Server produced itself (errors before the binding), numbered apart.
    own: u64,
    auth: Option<Auth>,
}

/// The degraded modes of a node (DSN-NOD-616): read-only when the database refuses
/// writes, effects paused when the release of effects fails.
#[derive(Debug, Clone, Default)]
struct Modes {
    read_only: Option<String>,
    effects_paused: Option<String>,
    epoch: u64,
}

impl Modes {
    fn json(&self) -> Value {
        json!({"read_only": self.read_only.is_some(), "read_only_reason": self.read_only,
            "effects_paused": self.effects_paused.is_some(), "effects_paused_reason": self.effects_paused, "epoch": self.epoch})
    }
}

/// The Server's state.
pub struct Server {
    source: Box<dyn VaeSource>,
    preset: Preset,
    /// Default tenant and authority of requests that name none.
    default_tenant: String,
    default_authority: String,
    vaes: Mutex<HashMap<String, VaeHandle>>,
    sessions: Mutex<HashMap<String, SessionEntry>>,
    next_session: Mutex<u64>,
    /// The identities by tenant (opened once).
    identities: Mutex<HashMap<String, Arc<Identity>>>,
    /// The identity of a single-VAE Server, serving every tenant name.
    fallback_identity: Option<Arc<Identity>>,
    /// Facade sessions by credential (`bearer:<token hash>` or `dev:<tenant>/<vae>`).
    facade_sessions: Mutex<HashMap<String, String>>,
    /// The kernel security-log sequence mirrored so far, per VAE.
    mirrored: Mutex<HashMap<String, u64>>,
    /// This node's identity as a lease and claim holder.
    node_id: String,
    /// Leadership per VAE.
    leaders: Mutex<HashMap<String, Leader>>,
    /// Event logs per VAE (opened once).
    event_stores: Mutex<HashMap<String, Arc<dyn EventStore>>>,
    /// Job stores per VAE (opened once).
    job_stores: Mutex<HashMap<String, Arc<dyn JobStore>>>,
    /// Schedule stores per VAE (opened once).
    schedule_stores: Mutex<HashMap<String, Arc<dyn ScheduleStore>>>,
    /// Traces, metrics and logs (DSN-NOD-610).
    pub tracer: Arc<crate::telemetry::Tracer>,
    /// The trace of the request that started each recent process (jobs join it).
    process_traces: Mutex<HashMap<String, crate::telemetry::Context>>,
    /// The degraded modes (DSN-NOD-616).
    modes: Mutex<Modes>,
    /// Whether the Server's loops run (the startup probe).
    started: std::sync::atomic::AtomicBool,
    /// The last self-check results (DSN-NOD-614).
    checks: Mutex<Vec<Value>>,
    /// Consecutive failed release rounds.
    release_failures: std::sync::atomic::AtomicU64,
    /// Security-log entries the database refused (a read-only node), written with the next.
    audit_backlog: Mutex<Vec<(String, Value)>>,
    /// The file service of each tenant (step 17a).
    files: Mutex<HashMap<String, Arc<crate::files::Files>>>,
    /// The running configuration (DSN-NOD-603).
    config: std::sync::RwLock<Arc<Config>>,
    /// The file the configuration is read from, if any.
    config_file: Mutex<Option<std::path::PathBuf>>,
    /// The applied versions when the source keeps no log.
    config_log: Mutex<Vec<Value>>,
    /// Request budgets per principal: tokens and the time of the last refill.
    budgets: Mutex<HashMap<String, (f64, std::time::Instant)>>,
    /// The schedules of each VAE's `main` as last read, with main's head (FR-EVT-042).
    schedule_cache: Mutex<HashMap<String, (String, Vec<Value>)>>,
    /// Wakes the delivery worker after a release that enqueued deliveries.
    deliver_now: tokio::sync::Notify,
    /// Wakes the releaser after a commit.
    release_now: tokio::sync::Notify,
}

type Shared = Arc<Server>;

impl Server {
    /// A Server over a VAE source.
    pub fn new(
        source: Box<dyn VaeSource>,
        preset: Preset,
        tenant: &str,
        authority: &str,
    ) -> Shared {
        Arc::new(Server {
            source,
            preset,
            default_tenant: tenant.to_string(),
            default_authority: authority.to_string(),
            vaes: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            next_session: Mutex::new(0),
            identities: Mutex::new(HashMap::new()),
            fallback_identity: None,
            facade_sessions: Mutex::new(HashMap::new()),
            mirrored: Mutex::new(HashMap::new()),
            node_id: crate::uuid().unwrap_or_else(|_| "node".into()),
            leaders: Mutex::new(HashMap::new()),
            event_stores: Mutex::new(HashMap::new()),
            job_stores: Mutex::new(HashMap::new()),
            schedule_stores: Mutex::new(HashMap::new()),
            config: std::sync::RwLock::new(Arc::new(Config::defaults())),
            tracer: Arc::new(crate::telemetry::Tracer::new("node")),
            process_traces: Mutex::new(HashMap::new()),
            modes: Mutex::new(Modes::default()),
            started: std::sync::atomic::AtomicBool::new(false),
            checks: Mutex::new(Vec::new()),
            release_failures: std::sync::atomic::AtomicU64::new(0),
            audit_backlog: Mutex::new(Vec::new()),
            files: Mutex::new(HashMap::new()),
            config_file: Mutex::new(None),
            config_log: Mutex::new(Vec::new()),
            budgets: Mutex::new(HashMap::new()),
            schedule_cache: Mutex::new(HashMap::new()),
            deliver_now: tokio::sync::Notify::new(),
            release_now: tokio::sync::Notify::new(),
        })
    }

    /// A Server over one open VAE with one identity for every tenant name (the `memory`
    /// preset, suites).
    pub fn single(
        handle: VaeHandle,
        identity: Arc<Identity>,
        preset: Preset,
        tenant: &str,
        authority: &str,
    ) -> Shared {
        Server::single_with(handle, identity, None, None, preset, tenant, authority)
    }

    /// A single-VAE Server whose VAE releases into a memory event log.
    pub fn single_with(
        handle: VaeHandle,
        identity: Arc<Identity>,
        events: Option<Arc<MemEventLog>>,
        jobs: Option<Arc<MemJobQueue>>,
        preset: Preset,
        tenant: &str,
        authority: &str,
    ) -> Shared {
        Arc::new(Server {
            source: Box::new(SingleVae {
                handle,
                identity: identity.clone(),
                events,
                jobs,
                leases: Arc::new(MemLeases::default()),
            }),
            preset,
            default_tenant: tenant.to_string(),
            default_authority: authority.to_string(),
            vaes: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            next_session: Mutex::new(0),
            identities: Mutex::new(HashMap::new()),
            fallback_identity: Some(identity),
            facade_sessions: Mutex::new(HashMap::new()),
            mirrored: Mutex::new(HashMap::new()),
            node_id: crate::uuid().unwrap_or_else(|_| "node".into()),
            leaders: Mutex::new(HashMap::new()),
            event_stores: Mutex::new(HashMap::new()),
            job_stores: Mutex::new(HashMap::new()),
            schedule_stores: Mutex::new(HashMap::new()),
            config: std::sync::RwLock::new(Arc::new(Config::defaults())),
            tracer: Arc::new(crate::telemetry::Tracer::new("node")),
            process_traces: Mutex::new(HashMap::new()),
            modes: Mutex::new(Modes::default()),
            started: std::sync::atomic::AtomicBool::new(false),
            checks: Mutex::new(Vec::new()),
            release_failures: std::sync::atomic::AtomicU64::new(0),
            audit_backlog: Mutex::new(Vec::new()),
            files: Mutex::new(HashMap::new()),
            config_file: Mutex::new(None),
            config_log: Mutex::new(Vec::new()),
            budgets: Mutex::new(HashMap::new()),
            schedule_cache: Mutex::new(HashMap::new()),
            deliver_now: tokio::sync::Notify::new(),
            release_now: tokio::sync::Notify::new(),
        })
    }

    /// This node's identifier (lease and claim holder).
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    // ---- configuration (DSN-NOD-603…-605, step 16a) ----

    /// The running configuration.
    pub fn config(&self) -> Arc<Config> {
        self.config.read().expect("config").clone()
    }

    /// Start with a configuration (read from `file`, if any): it becomes the running one
    /// and is recorded as applied by `author`.
    pub fn configure(
        self: &Shared,
        cfg: Config,
        file: Option<std::path::PathBuf>,
        author: &str,
    ) -> Result<Value, BpaError> {
        *self.config_file.lock().expect("config file") = file;
        let old = Config::defaults();
        let diff = cfg.diff(&old);
        *self.config.write().expect("config") = Arc::new(cfg.clone());
        set_log_level(cfg.text("log.level").unwrap_or("info"));
        self.configure_telemetry(&cfg);
        self.record_version(&cfg, author, "start", diff, Vec::new())
    }

    /// Apply a new configuration (FR-OPS-091, -092, DSN-NOD-604): its reloadable keys take
    /// effect at once and the others at the next start (reported as `restart_required`).
    /// The version is recorded with its hash, author and diff (FR-OPS-093).
    pub fn apply_config(
        self: &Shared,
        new: Config,
        author: &str,
        source: &str,
    ) -> Result<Value, BpaError> {
        let cur = self.config();
        let diff = new.diff(&cur);
        let restart: Vec<Value> = diff
            .iter()
            .filter(|d| d["reloadable"] == json!(false))
            .map(|d| d["key"].clone())
            .collect();
        let running = cur.reloaded(&new);
        set_log_level(running.text("log.level").unwrap_or("info"));
        self.configure_telemetry(&running);
        *self.config.write().expect("config") = Arc::new(running);
        self.budgets.lock().expect("budgets").clear();
        self.record_version(&new, author, source, diff, restart)
    }

    /// Read the configuration file again and apply it (SIGHUP); an invalid file is
    /// rejected and the running configuration stays.
    pub fn reload(self: &Shared, author: &str) -> Result<Value, BpaError> {
        let path = self
            .config_file
            .lock()
            .expect("config file")
            .clone()
            .ok_or_else(|| {
                BpaError::new(
                    "MODEL.INVALID_VALUE",
                    "the node runs without a configuration file",
                )
            })?;
        let text = std::fs::read_to_string(&path).map_err(|e| {
            BpaError::new("MODEL.INVALID_VALUE", format!("{}: {e}", path.display()))
        })?;
        match Config::parse(&text) {
            Ok(c) => self.apply_config(c, author, "file"),
            Err(e) => {
                self.audit(
                    "config.rejected",
                    json!({"author": author, "source": "file", "errors": e.details}),
                );
                Err(e)
            }
        }
    }

    fn record_version(
        &self,
        cfg: &Config,
        author: &str,
        source: &str,
        diff: Vec<Value>,
        restart: Vec<Value>,
    ) -> Result<Value, BpaError> {
        let v = json!({"node": self.node_id, "hash": cfg.hash(), "author": author, "source": source,
            "document": cfg.document(), "diff": diff, "restart_required": restart});
        if !self.source.record_config(&v)? {
            let mut log = self.config_log.lock().expect("config log");
            let mut e = v.clone();
            e["seq"] = json!(log.len() + 1);
            e["applied_at"] = json!(ubos_dvm::types::time::format_instant(&chrono::DateTime::<
                chrono::Utc,
            >::from(
                std::time::SystemTime::now()
            )));
            log.push(e);
        }
        self.audit(
            "config.applied",
            json!({"node": self.node_id, "hash": cfg.hash(), "author": author,
            "source": source, "diff": diff, "restart_required": restart}),
        );
        Ok(json!({"hash": cfg.hash(), "diff": diff, "restart_required": restart}))
    }

    /// The configuration versions applied by this node, newest first.
    pub fn config_versions(&self, limit: i64) -> Result<Vec<Value>, BpaError> {
        if let Some(v) = self.source.config_versions(&self.node_id, limit)? {
            return Ok(v);
        }
        let log = self.config_log.lock().expect("config log");
        Ok(log
            .iter()
            .rev()
            .take(limit.max(1) as usize)
            .map(|e| {
                let mut e = e.clone();
                if let Some(m) = e.as_object_mut() {
                    m.remove("document");
                }
                e
            })
            .collect())
    }

    /// An entry of the default tenant's security log (node events: configuration).
    fn audit(&self, kind: &str, record: Value) {
        let write = || {
            let mut backlog = self.audit_backlog.lock().expect("audit backlog");
            backlog.push((kind.to_string(), record));
            let Ok(id) = self.identity(&self.default_tenant) else {
                return;
            };
            let mut written = 0;
            for (k, r) in backlog.iter() {
                if id.log(k, r.clone()).is_err() {
                    break;
                }
                written += 1;
            }
            backlog.drain(..written);
            // a node that cannot write for long keeps the newest entries
            let excess = backlog.len().saturating_sub(1000);
            backlog.drain(..excess);
        };
        // the database runtime cannot nest inside the Server's: from async code the write
        // runs on a thread of its own
        if tokio::runtime::Handle::try_current().is_ok() {
            std::thread::scope(|sc| {
                let _ = sc.spawn(write).join();
            });
        } else {
            write();
        }
    }

    /// Whether a principal may make one more request (DSN-NOD-604): a token bucket of
    /// `limits.requests_per_second` and `limits.burst`; `limits.requests_per_second = 0`
    /// has no limit.
    fn allow(&self, principal: &str) -> bool {
        let cfg = self.config();
        let rate = cfg.int("limits.requests_per_second") as f64;
        if rate <= 0.0 {
            return true;
        }
        let burst = cfg.int("limits.burst").max(1) as f64;
        let now = std::time::Instant::now();
        let mut b = self.budgets.lock().expect("budgets");
        let e = b.entry(principal.to_string()).or_insert((burst, now));
        e.0 = (e.0 + now.duration_since(e.1).as_secs_f64() * rate).min(burst);
        e.1 = now;
        if e.0 >= 1.0 {
            e.0 -= 1.0;
            true
        } else {
            false
        }
    }

    /// The identity of a tenant (opened once; `IAM.NOT_FOUND` for an unknown tenant).
    pub fn identity(&self, tenant: &str) -> Result<Arc<Identity>, BpaError> {
        if let Some(i) = self.identities.lock().expect("identities").get(tenant) {
            return Ok(i.clone());
        }
        let id = match self.source.identity(tenant)? {
            Some(i) => Arc::new(i),
            None => self
                .fallback_identity
                .clone()
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant"))?,
        };
        self.identities
            .lock()
            .expect("identities")
            .insert(tenant.to_string(), id.clone());
        Ok(id)
    }

    /// The router of the bindings and the facade.
    pub fn router(self: &Shared) -> Router {
        Router::new()
            .route("/livez", get(livez))
            .route("/readyz", get(readyz))
            .route("/startupz", get(startupz))
            .route("/ubtp/v1", get(ws_upgrade).post(http_messages))
            .route("/ubtp/v1/stream", get(sse_stream))
            .route("/api/v1/{vae}/{class}", get(rest_query))
            .route("/api/v1/{vae}/{class}/{slug}", get(rest_read))
            .route("/api/v1/{vae}/{class}/{slug}/{port}", post(rest_mutate))
            .route(
                "/files/v1/download/{token}",
                get(file_download).head(file_download),
            )
            .route(
                "/files/v1/{vae}",
                post(upload_create).options(upload_options),
            )
            .route("/files/v1/{vae}/links", post(file_link))
            .route(
                "/files/v1/{vae}/uploads/{id}",
                get(upload_state)
                    .head(upload_head)
                    .patch(upload_patch)
                    .delete(upload_delete)
                    .layer(DefaultBodyLimit::max(UPLOAD_PATCH_MAX)),
            )
            .route("/api/v1/identity/{*rest}", axum::routing::any(identity_api))
            .route("/api/v1/admin/{*rest}", axum::routing::any(admin_api))
            .layer(DefaultBodyLimit::max(MAX_FRAME))
            .layer(axum::middleware::from_fn_with_state(self.clone(), cors))
            .with_state(self.clone())
    }

    /// Serve on `listen` until the task is cancelled.
    pub async fn serve(self: Shared, listen: std::net::SocketAddr) -> std::io::Result<()> {
        let listener = tokio::net::TcpListener::bind(listen).await?;
        self.serve_with(listener).await
    }

    /// Serve on a bound listener (suites pick an ephemeral port).
    pub async fn serve_with(
        self: Shared,
        listener: tokio::net::TcpListener,
    ) -> std::io::Result<()> {
        let poll = self.clone();
        tokio::spawn(async move { poll.poll_loop().await });
        let ops = self.clone();
        tokio::spawn(async move { ops.operations_loop().await });
        let me = self.clone();
        tokio::spawn(async move {
            // the VAEs with work open at start and on every rescan, so that what a crash
            // left in the outboxes is delivered without waiting for a session (NR-DUR-006)
            let m2 = me.clone();
            let _ = tokio::task::spawn_blocking(move || m2.open_with_work()).await;
            let r = me.clone();
            tokio::spawn(async move { r.release_loop().await });
            let d = me.clone();
            tokio::spawn(async move { d.delivery_loop().await });
            let w = me.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(WORK_SCAN_SECS)).await;
                    let w2 = w.clone();
                    let _ = tokio::task::spawn_blocking(move || w2.open_with_work()).await;
                }
            });
            me.leader_loop().await;
        });
        axum::serve(
            listener,
            self.router()
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    }

    /// The VAE of a tenant and authority (opened once; DSN-NOD-008).
    fn vae(&self, tenant: &str, authority: &str) -> Result<VaeHandle, BpaError> {
        let key = format!("{tenant}/{authority}");
        if let Some(h) = self.vaes.lock().expect("vaes").get(&key) {
            return Ok(h.clone());
        }
        let h = self
            .source
            .open(tenant, authority)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant or VAE"))?;
        // server mode: the releaser claims the outbox; only the leader fires timers
        let held = self.source.leases().acquire(
            &lease_name(&key),
            &self.node_id,
            crate::leases::LEASE_MS,
        )?;
        h.call_blocking(
            "host.mode",
            json!({"claimed_release": true, "auto_tick": held.is_some()}),
        )?;
        self.leaders.lock().expect("leaders").insert(
            key.clone(),
            Leader {
                held: held.is_some(),
                token: held.unwrap_or(0),
                renewed: std::time::Instant::now(),
            },
        );
        self.vaes.lock().expect("vaes").insert(key, h.clone());
        Ok(h)
    }

    /// Open the VAEs with work (or, when the source cannot tell, the default one).
    fn open_with_work(&self) {
        let list = match self.source.with_work(WORK_HORIZON_SECS) {
            Ok(Some(l)) => l,
            Ok(None) => vec![(self.default_tenant.clone(), self.default_authority.clone())],
            Err(e) => {
                self.log_error("work scan", "work scan".to_string(), &e);
                return;
            }
        };
        for (t, a) in list {
            if self
                .vaes
                .lock()
                .expect("vaes")
                .contains_key(&format!("{t}/{a}"))
            {
                continue;
            }
            if let Err(e) = self.vae(&t, &a) {
                self.log_error("open", format!("cannot open {t}/{a}"), &e);
            }
        }
    }

    /// The event log of a VAE (opened once).
    fn events_of(&self, key: &str) -> Result<Option<Arc<dyn EventStore>>, BpaError> {
        if let Some(e) = self.event_stores.lock().expect("events").get(key) {
            return Ok(Some(e.clone()));
        }
        let (t, a) = key.split_once('/').unwrap_or((key, ""));
        let e = self.source.events(t, a)?;
        if let Some(e) = &e {
            self.event_stores
                .lock()
                .expect("events")
                .insert(key.to_string(), e.clone());
        }
        Ok(e)
    }

    /// The releaser (DSN-NOD-201): every open VAE's outbox is claimed with a lease,
    /// delivered to its event log and acknowledged, every `RELEASE_POLL_MS` and at once
    /// after a commit through the Server.
    async fn release_loop(self: Shared) {
        loop {
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(RELEASE_POLL_MS),
                self.release_now.notified(),
            )
            .await;
            let vaes: Vec<VaeHandle> = self.vaes.lock().expect("vaes").values().cloned().collect();
            for h in vaes {
                loop {
                    let r = h
                        .call("host.release", json!({"holder": self.node_id, "n": RELEASE_BATCH, "lease_ms": RELEASE_LEASE_MS}))
                        .await;
                    if r.as_ref()
                        .is_ok_and(|v| v["deliveries"].as_u64().unwrap_or(0) > 0)
                    {
                        self.deliver_now.notify_one();
                    }
                    match r {
                        Ok(v) if v["released"].as_u64().unwrap_or(0) as usize == RELEASE_BATCH => {
                            continue;
                        }
                        Ok(_) => {
                            self.release_ok();
                            break;
                        }
                        Err(e) => {
                            self.log_error("release", "release".to_string(), &e);
                            self.release_failed(&e);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// The job store of a VAE (opened once).
    fn jobs_of(&self, key: &str) -> Result<Option<Arc<dyn JobStore>>, BpaError> {
        if let Some(j) = self.job_stores.lock().expect("jobs").get(key) {
            return Ok(Some(j.clone()));
        }
        let (t, a) = key.split_once('/').unwrap_or((key, ""));
        let j = self.source.jobs(t, a)?;
        if let Some(j) = &j {
            self.job_stores
                .lock()
                .expect("jobs")
                .insert(key.to_string(), j.clone());
        }
        Ok(j)
    }

    /// The job worker (DSN-NOD-204, -209, -220…-223, -229, -230; FR-EVT-031…-034,
    /// -051…-053): every `RELEASE_POLL_MS` and at once after a release with jobs, it takes
    /// one ready job from each open VAE in turn (tenants round-robin, within their queue and
    /// tenant limits) until none is ready. A claimed job holds a lease whose token fences its
    /// result and which is heartbeated every lease/3; deliveries run as their subscription's
    /// service principal, port jobs as the parent process's initiator, bulk jobs batch by
    /// batch so that a cancellation stops them in between. A failed attempt waits for its
    /// backoff; one that spends its policy, repeats the same error `POISON_AFTER` times or
    /// whose principal cannot act is dead. Every outcome is committed to the `Job` object.
    async fn delivery_loop(self: Shared) {
        loop {
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(RELEASE_POLL_MS),
                self.deliver_now.notified(),
            )
            .await;
            let vaes: Vec<(String, VaeHandle)> = self
                .vaes
                .lock()
                .expect("vaes")
                .iter()
                .map(|(k, h)| (k.clone(), h.clone()))
                .collect();
            for _ in 0..DELIVERY_BATCH {
                let mut any = false;
                for (key, h) in &vaes {
                    match self.run_one(key, h).await {
                        Ok(true) => any = true,
                        Ok(false) => {}
                        Err(e) => self.log_error("job", format!("job {key}"), &e),
                    }
                }
                if !any {
                    break;
                }
            }
        }
    }

    /// The principal a job runs as: the subscription's service principal (`required`:
    /// it must exist, be active and be a service or agent, DSN-NOD-110), or the parent
    /// process's initiator when the tenant knows it.
    async fn job_principal(
        self: &Shared,
        tenant: &str,
        did: &str,
        required: bool,
    ) -> Result<Value, (BpaError, bool)> {
        let id = self.identity_async(tenant).await.map_err(|e| (e, true))?;
        let d = did.to_string();
        match blocking(move || id.principal(&d)).await {
            Ok(p)
                if p["state"] == "active"
                    && (!required || matches!(p["kind"].as_str(), Some("service" | "agent"))) =>
            {
                Ok(
                    json!({"did": p["did"], "kind": p["kind"], "role": p.get("role").cloned().unwrap_or(Value::Null),
                    "grants": p.get("grants").cloned().unwrap_or(Value::Null)}),
                )
            }
            Ok(p) => Err((
                BpaError::with_details(
                    "IAM.DENIED",
                    json!({"reason": "run_as_not_an_active_service_principal", "run_as": did, "state": p["state"], "kind": p["kind"]}),
                ),
                true,
            )),
            Err(_) if !required && !did.is_empty() => Ok(json!({"did": did, "kind": "human"})),
            Err(e) => Err((e, true)),
        }
    }

    /// Claim and run one job of a VAE; `false` when none is ready.
    async fn run_one(self: &Shared, key: &str, h: &VaeHandle) -> Result<bool, BpaError> {
        let (me, k) = (self.clone(), key.to_string());
        let Some(jobs) = blocking(move || me.jobs_of(&k)).await? else {
            return Ok(false);
        };
        let (j2, holder) = (jobs.clone(), self.node_id.clone());
        let Some(job) = blocking(move || j2.claim(JOB_KINDS, &holder, DELIVERY_LEASE_MS)).await?
        else {
            return Ok(false);
        };
        let job_id = job["job_id"].as_str().unwrap_or_default().to_string();
        let token = job["lease_token"].as_i64().unwrap_or(0);
        let tenant = key.split('/').next().unwrap_or_default().to_string();
        let kind = job["kind"].as_str().unwrap_or_default().to_string();
        // a cancellation asked before the attempt started
        if job["cancel_requested"] == json!(true) {
            let (j2, jid) = (jobs.clone(), job_id.clone());
            if blocking(move || j2.finish_cancelled(&jid, token)).await? {
                let _ = h
                    .call(
                        "host.job_state",
                        json!({"job_id": job_id, "set": {"state": "cancelled"}}),
                    )
                    .await;
            }
            return Ok(true);
        }
        // heartbeats every lease/3 while the attempt runs (DSN-NOD-221)
        let hb = {
            let (j2, jid) = (jobs.clone(), job_id.clone());
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        (DELIVERY_LEASE_MS / 3) as u64,
                    ))
                    .await;
                    let (j3, jid2) = (j2.clone(), jid.clone());
                    if !blocking(move || j3.heartbeat(&jid2, token, DELIVERY_LEASE_MS))
                        .await
                        .unwrap_or(false)
                    {
                        break;
                    }
                }
            })
        };
        // the job joins the trace of the request whose process started it (FR-OPS-041)
        let origin = match kind.as_str() {
            "delivery" => job["payload"]["event"]["event_id"]
                .as_str()
                .and_then(|e| e.split('/').next())
                .map(str::to_string),
            "port" => job["payload"]["parent_process"]
                .as_str()
                .map(str::to_string),
            _ => None,
        };
        let parent = origin
            .as_deref()
            .and_then(|p| self.process_traces.lock().expect("traces").get(p).cloned());
        let jctx = self.tracer.child(parent.as_ref());
        let jt0 = crate::telemetry::now_ns();
        let outcome: Result<Value, (BpaError, bool)> = match kind.as_str() {
            "delivery" => {
                let run_as = job["payload"]["run_as"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                match self.job_principal(&tenant, &run_as, true).await {
                    Ok(pr) => h
                        .call("host.deliver", json!({"job": job, "principal": pr}))
                        .await
                        .map_err(|e| (e, false)),
                    Err(e) => Err(e),
                }
            }
            "port" => {
                let parent = job["payload"]["parent_process"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let did = match h
                    .call("host.initiator", json!({"process_id": parent}))
                    .await
                {
                    Ok(v) => v.as_str().unwrap_or_default().to_string(),
                    Err(_) => String::new(),
                };
                // the parent's initiator, or the operator who enqueued the job
                let did = if did.is_empty() {
                    job["payload"]["initiator"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                } else {
                    did
                };
                match self.job_principal(&tenant, &did, false).await {
                    Ok(pr) => h
                        .call("host.run_port", json!({"job": job, "principal": pr}))
                        .await
                        .map_err(|e| (e, false)),
                    Err(e) => Err(e),
                }
            }
            "bulk" => self.run_bulk_job(&jobs, h, &job, token).await,
            "schedule" => {
                let run_as = job["payload"]["run_as"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                match self.job_principal(&tenant, &run_as, true).await {
                    Ok(pr) => h
                        .call("host.run_schedule", json!({"job": job, "principal": pr}))
                        .await
                        .map_err(|e| (e, false)),
                    Err(e) => Err(e),
                }
            }
            other => Err((
                BpaError::new(
                    "LOGIC.NOT_AVAILABLE",
                    format!("no runner for jobs of kind {other}"),
                ),
                true,
            )),
        };
        hb.abort();
        {
            let mut a = Map::new();
            a.insert("ubos.job_id".into(), json!(job_id));
            a.insert("ubos.job_kind".into(), json!(kind));
            a.insert("ubos.tenant".into(), json!(tenant));
            if let Ok(r) = &outcome
                && let Some(pid) = r["process_id"].as_str()
            {
                a.insert("ubos.process_id".into(), json!(pid));
            }
            let err = outcome.as_ref().err().map(|(e, _)| e.code);
            self.tracer.record(
                &jctx,
                parent.as_ref().map(|p| p.span_id.as_str()),
                &format!("job.{kind}"),
                jt0,
                crate::telemetry::now_ns(),
                &a,
                err,
            );
        }
        let policy = crate::services::RetryPolicy::from_json(&job["payload"]["policy"]);
        let attempts = job["attempts"].as_i64().unwrap_or(1);
        let jid = job_id.clone();
        match outcome {
            Ok(r) if r["status"] == "cancelled" => {
                let j2 = jobs.clone();
                if blocking(move || j2.finish_cancelled(&jid, token)).await? {
                    let _ = h.call("host.job_state", json!({"job_id": job_id, "set": {"state": "cancelled", "attempts": attempts}})).await;
                }
            }
            Ok(r) => {
                let (j2, r2) = (jobs.clone(), r.clone());
                if blocking(move || j2.complete(&jid, Some(token), &r2)).await? {
                    let _ = h.call("host.job_state", json!({"job_id": job_id, "set": {"state": "succeeded", "attempts": attempts, "result": r}})).await;
                } else {
                    self.fenced(&tenant, key, &job_id, token).await;
                }
            }
            Err((e, final_)) => {
                let text = match &e.details {
                    Some(d) => format!("{}: {d}", e.code),
                    None => format!("{}: {}", e.code, e.message),
                };
                let delay = policy.delay_ms(attempts);
                let (j2, t2) = (jobs.clone(), text.clone());
                match blocking(move || j2.fail(&jid, Some(token), &t2, delay, final_)).await? {
                    None => self.fenced(&tenant, key, &job_id, token).await,
                    Some(f) => {
                        let set = json!({"state": crate::platform::object_state(&f.state), "attempts": attempts,
                            "last_error": text, "poison": f.poison});
                        let _ = h
                            .call("host.job_state", json!({"job_id": job_id, "set": set}))
                            .await;
                        if f.state == "dead"
                            && let Ok(id) = self.identity_async(&tenant).await
                        {
                            let rec = json!({"vae": key, "job_id": job_id, "kind": kind, "code": e.code, "attempts": attempts,
                                "poison": f.poison, "subscription": job["payload"]["subscription"],
                                "event_id": job["payload"]["event"]["event_id"]});
                            let name = if kind == "delivery" {
                                "delivery.dead"
                            } else {
                                "job.dead"
                            };
                            let _ = blocking(move || id.log(name, rec)).await;
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    /// A result refused because the lease's token no longer holds (DSN-NOD-221).
    async fn fenced(self: &Shared, tenant: &str, key: &str, job_id: &str, token: i64) {
        if let Ok(id) = self.identity_async(tenant).await {
            let rec = json!({"vae": key, "job_id": job_id, "token": token, "node": self.node_id});
            let _ = blocking(move || id.log("job.fenced", rec)).await;
        }
    }

    /// Run a bulk job batch by batch: between batches the lease is checked and an
    /// operator's cancellation stops the job (`{status: "cancelled"}`).
    async fn run_bulk_job(
        self: &Shared,
        jobs: &Arc<dyn JobStore>,
        h: &VaeHandle,
        job: &Value,
        token: i64,
    ) -> Result<Value, (BpaError, bool)> {
        let job_id = job["job_id"].as_str().unwrap_or_default().to_string();
        loop {
            let r = h
                .call("host.run_bulk", json!({"job": job, "max_chunks": 1}))
                .await
                .map_err(|e| (e, false))?;
            match r["state"].as_str() {
                Some("committed") => return Ok(r),
                Some("failed") => {
                    return Err((
                        BpaError::with_details(
                            "LOGIC.NOT_AVAILABLE",
                            json!({"reason": "bulk_failed", "error": r["error"]}),
                        ),
                        false,
                    ));
                }
                _ => {}
            }
            let (j2, jid) = (jobs.clone(), job_id.clone());
            let now = blocking(move || j2.job(&jid))
                .await
                .map_err(|e| (e, false))?;
            match now {
                Some(j) if j["cancel_requested"] == json!(true) => {
                    return Ok(json!({"status": "cancelled", "bulk": r}));
                }
                Some(j) if j["lease_token"].as_i64() == Some(token) && j["state"] == "running" => {}
                // the lease was lost: stop; the new holder resumes the bulk process
                _ => {
                    return Err((
                        BpaError::with_details("LOGIC.CANCELLED", json!({"reason": "lease_lost"})),
                        false,
                    ));
                }
            }
        }
    }

    /// The leader's loop (DSN-NOD-208, -228): every second each VAE's lease is renewed
    /// when due (every `RENEW_MS`), the holder fires the due became-effective timers, and
    /// sweeps the event log every `SWEEP_SECS`. A node whose renewal fails stops acting
    /// once its lease would have expired.
    async fn leader_loop(self: Shared) {
        let mut last_sweep = std::time::Instant::now();
        let mut last_jobs = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let vaes: Vec<(String, VaeHandle)> = self
                .vaes
                .lock()
                .expect("vaes")
                .iter()
                .map(|(k, h)| (k.clone(), h.clone()))
                .collect();
            let sweep = last_sweep.elapsed().as_secs() >= SWEEP_SECS;
            if sweep {
                last_sweep = std::time::Instant::now();
            }
            let sweep_jobs = last_jobs.elapsed().as_secs() >= JOB_SWEEP_SECS;
            if sweep_jobs {
                last_jobs = std::time::Instant::now();
            }
            for (key, h) in vaes {
                let held = self.renew(&key, &h).await;
                if !held {
                    continue;
                }
                if let Err(e) = h.call("host.tick", json!({})).await {
                    self.log_error("tick", format!("tick {key}"), &e);
                }
                if let Err(e) = self.evaluate_schedules(&key, &h).await {
                    self.log_error("schedules", format!("schedules {key}"), &e);
                }
                if sweep_jobs && let Err(e) = self.sweep_jobs(&key, &h).await {
                    self.log_error("job sweep", format!("job sweep {key}"), &e);
                }
                if sweep {
                    let (me, k) = (self.clone(), key.clone());
                    let now = ubos_dvm::types::time::format_instant(
                        &chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()),
                    );
                    let _ = blocking(move || match me.events_of(&k)? {
                        Some(e) => e.sweep(&now).map(|_| ()),
                        None => Ok(()),
                    })
                    .await;
                }
            }
        }
    }

    /// The schedule store of a VAE (opened once; in memory when the source keeps none).
    fn schedules_of(&self, key: &str) -> Result<Arc<dyn ScheduleStore>, BpaError> {
        if let Some(st) = self.schedule_stores.lock().expect("schedules").get(key) {
            return Ok(st.clone());
        }
        let (t, a) = key.split_once('/').unwrap_or((key, ""));
        let st = match self.source.schedules(t, a)? {
            Some(st) => st,
            None => Arc::new(MemScheduleStore::default()) as Arc<dyn ScheduleStore>,
        };
        self.schedule_stores
            .lock()
            .expect("schedules")
            .insert(key.to_string(), st.clone());
        Ok(st)
    }

    /// The schedules of a VAE's `main`, read again only when main's head moved.
    async fn schedules_now(
        self: &Shared,
        key: &str,
        h: &VaeHandle,
    ) -> Result<Vec<Value>, BpaError> {
        let known = self
            .schedule_cache
            .lock()
            .expect("schedule cache")
            .get(key)
            .map(|c| c.0.clone());
        let r = h.call("host.schedules", json!({"known": known})).await?;
        if r["unchanged"] != json!(true) {
            let items = r["items"].as_array().cloned().unwrap_or_default();
            // the cursors of deleted and disabled schedules are forgotten
            let live: Vec<String> = items
                .iter()
                .filter(|it| it["body"]["enabled"] != json!(false))
                .filter_map(|it| it["schedule_id"].as_str().map(str::to_string))
                .collect();
            let (me, k) = (self.clone(), key.to_string());
            blocking(move || me.schedules_of(&k)?.retain(&live)).await?;
            self.schedule_cache.lock().expect("schedule cache").insert(
                key.to_string(),
                (r["head"].as_str().unwrap_or_default().to_string(), items),
            );
        }
        Ok(self
            .schedule_cache
            .lock()
            .expect("schedule cache")
            .get(key)
            .map(|c| c.1.clone())
            .unwrap_or_default())
    }

    /// Evaluate a VAE's schedules as its leader (DSN-NOD-224…-226; FR-EVT-041…-044): each
    /// second, the schedules of `main` as committed now (a change takes effect at the next
    /// evaluation), their slots since the cursor recorded in the schedule state before their
    /// jobs are enqueued, missed and paused slots by the misfire policy. Returns the jobs
    /// enqueued.
    pub async fn evaluate_schedules(
        self: &Shared,
        key: &str,
        h: &VaeHandle,
    ) -> Result<usize, BpaError> {
        let items = self.schedules_now(key, h).await?;
        if items.is_empty() {
            return Ok(0);
        }
        let (me, k) = (self.clone(), key.to_string());
        let id = self
            .identity_async(key.split('/').next().unwrap_or_default())
            .await
            .ok();
        let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
        let done = blocking(move || {
            let store = me.schedules_of(&k)?;
            let mut out = Vec::new();
            for it in &items {
                let sid = it["schedule_id"].as_str().unwrap_or_default();
                match crate::schedule::evaluate(store.as_ref(), sid, &it["body"], now) {
                    Ok(ev) => {
                        if ev.dropped > 0
                            && let Some(id) = &id
                        {
                            let _ = id.log(
                                "schedule.slots_dropped",
                                json!({"vae": k, "schedule": sid, "dropped": ev.dropped}),
                            );
                        }
                        if !ev.specs.is_empty() {
                            out.push((sid.to_string(), ev));
                        }
                    }
                    // a schedule whose rule does not parse has no slots until it is fixed
                    Err(e) => me.log_error("schedule", format!("schedule {k} {sid}"), &e),
                }
            }
            Ok((store, out))
        })
        .await?;
        let (store, fired) = done;
        let mut n = 0;
        for (sid, ev) in fired {
            h.call("host.enqueue", json!({"specs": ev.specs})).await?;
            n += ev.specs.len();
            let (st, slots) = (store.clone(), ev.firing);
            blocking(move || crate::schedule::mark_fired(st.as_ref(), &sid, &slots)).await?;
        }
        if n > 0 {
            self.deliver_now.notify_one();
        }
        Ok(n)
    }

    /// The job sweeper (FR-EVT-032, DSN-NOD-220): every pending `Job` object without a row
    /// gets one again; expired leases are claimable anyway. Returns the rows restored.
    pub async fn sweep_jobs(self: &Shared, key: &str, h: &VaeHandle) -> Result<usize, BpaError> {
        let (me, k) = (self.clone(), key.to_string());
        let Some(jobs) = blocking(move || me.jobs_of(&k)).await? else {
            return Ok(0);
        };
        let j2 = jobs.clone();
        let known: Vec<String> = blocking(move || j2.ids()).await?.into_iter().collect();
        let open = h.call("host.jobs_open", json!({"known": known})).await?;
        let items = open["items"].as_array().cloned().unwrap_or_default();
        let restored = blocking(move || {
            let mut n = 0;
            for j in &items {
                if jobs.restore(j)? {
                    n += 1;
                }
            }
            Ok(n)
        })
        .await?;
        if restored > 0 {
            self.deliver_now.notify_one();
        }
        Ok(restored)
    }

    /// Renew (or try to take) a VAE's leadership when due; returns whether this node may act.
    async fn renew(self: &Shared, key: &str, h: &VaeHandle) -> bool {
        let cur = self.leaders.lock().expect("leaders").get(key).cloned();
        let ms = |c: &Leader| c.renewed.elapsed().as_millis() as i64;
        let due = cur
            .as_ref()
            .is_none_or(|c| !c.held || ms(c) >= crate::leases::RENEW_MS);
        if !due {
            return true;
        }
        let (leases, name, holder) = (self.source.leases(), lease_name(key), self.node_id.clone());
        let r = blocking(move || leases.acquire(&name, &holder, crate::leases::LEASE_MS)).await;
        let was = cur.as_ref().is_some_and(|c| c.held);
        let now = match r {
            Ok(Some(token)) => Leader {
                held: true,
                token,
                renewed: std::time::Instant::now(),
            },
            Ok(None) => Leader {
                held: false,
                token: cur.as_ref().map_or(0, |c| c.token),
                renewed: std::time::Instant::now(),
            },
            // the store did not answer: keep acting only while the lease cannot have expired
            Err(_) => match cur {
                Some(c) if c.held && ms(&c) < crate::leases::LEASE_MS => return true,
                Some(c) => Leader { held: false, ..c },
                None => return false,
            },
        };
        let held = now.held;
        if held != was {
            let _ = h
                .call(
                    "host.mode",
                    json!({"claimed_release": true, "auto_tick": held}),
                )
                .await;
        }
        self.leaders
            .lock()
            .expect("leaders")
            .insert(key.to_string(), now);
        held
    }

    /// The identity of a tenant, opened off the runtime (the node's registry blocks on
    /// its database).
    async fn identity_async(self: &Shared, tenant: &str) -> Result<Arc<Identity>, BpaError> {
        if let Some(i) = self.identities.lock().expect("identities").get(tenant) {
            return Ok(i.clone());
        }
        let (me, t) = (self.clone(), tenant.to_string());
        blocking(move || me.identity(&t)).await
    }

    /// A VAE, opened off the runtime.
    async fn vae_async(
        self: &Shared,
        tenant: &str,
        authority: &str,
    ) -> Result<VaeHandle, BpaError> {
        let (me, t, a) = (self.clone(), tenant.to_string(), authority.to_string());
        blocking(move || me.vae(&t, &a)).await
    }

    /// The secret store of a tenant, opened off the runtime.
    async fn secrets_async(self: &Shared, tenant: &str) -> Option<crate::keys::SecretStore> {
        let (me, t) = (self.clone(), tenant.to_string());
        blocking(move || me.source.secrets(&t)).await.ok().flatten()
    }

    /// Open a session on the VAE a handshake names.
    async fn open_session(
        self: &Shared,
        tenant: Option<&str>,
        authority: Option<&str>,
    ) -> Result<(String, VaeHandle), BpaError> {
        let tenant = tenant.unwrap_or(&self.default_tenant);
        let authority = authority.unwrap_or(&self.default_authority);
        let handle = {
            let (me, t, a) = (self.clone(), tenant.to_string(), authority.to_string());
            blocking(move || me.vae(&t, &a)).await?
        };
        let key = {
            let mut n = self.next_session.lock().expect("sessions");
            *n += 1;
            format!("sess-{}", *n)
        };
        self.sessions.lock().expect("sessions").insert(
            key.clone(),
            SessionEntry {
                banner_epoch: 0,
                vae: format!("{tenant}/{authority}"),
                tenant: tenant.to_string(),
                handle: handle.clone(),
                push: None,
                own: 0,
                auth: None,
            },
        );
        Ok((key, handle))
    }

    /// Close the Server sessions bound to an identity session or to a principal: their
    /// push channels close, which ends the WebSocket and event-stream connections
    /// (DSN-NOD-106).
    fn close_bound(
        &self,
        tenant: &str,
        session_id: Option<&str>,
        principal: Option<&str>,
    ) -> usize {
        let mut sessions = self.sessions.lock().expect("sessions");
        let keys: Vec<String> = sessions
            .iter()
            .filter(|(_, e)| {
                e.tenant == tenant
                    && e.auth.as_ref().is_some_and(|a| {
                        session_id.is_some_and(|id| a.credential_id == id)
                            || principal.is_some_and(|p| a.principal == p)
                    })
            })
            .map(|(k, _)| k.clone())
            .collect();
        for k in &keys {
            sessions.remove(k);
        }
        keys.len()
    }

    fn session(&self, key: &str) -> Option<(String, VaeHandle)> {
        self.sessions
            .lock()
            .expect("sessions")
            .get(key)
            .map(|s| (s.vae.clone(), s.handle.clone()))
    }

    fn close_session(&self, key: &str) {
        self.sessions.lock().expect("sessions").remove(key);
    }

    fn attach_push(&self, key: &str, tx: tokio::sync::mpsc::Sender<Value>) -> bool {
        match self.sessions.lock().expect("sessions").get_mut(key) {
            Some(s) => {
                s.push = Some(tx);
                true
            }
            None => false,
        }
    }

    /// A frame the Server produces itself, numbered apart from the binding's.
    fn own_error(&self, key: &str, e: &BpaError, reply_to: Option<&str>) -> Value {
        let n = match self.sessions.lock().expect("sessions").get_mut(key) {
            Some(s) => {
                s.own += 1;
                s.own
            }
            None => 0,
        };
        ubos_proto::error_frame(&format!("n{n}"), e, reply_to, None)
    }

    /// The request pipeline of one message (DSN-NOD-006): decode (done by the transport),
    /// authenticate (the handshake's credentials), construct the context (the session's
    /// VAE), call the binding, encode the frames. `session` is `None` for a message that
    /// opens a session: the handshake.
    async fn handle(
        self: &Shared,
        session: Option<String>,
        tenant: Option<String>,
        source: &str,
        message: Value,
    ) -> (String, Vec<Value>) {
        let mut message = message;
        let rid = ubos_proto::id_of(&message);
        let type_ = message
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let is_handshake = type_ == "handshake";
        let authority = message
            .get("body")
            .and_then(|b| b.get("vae"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // the tenant of an identity credential is the one its token names
        let tenant = match (tenant, is_handshake) {
            (t, true) => token_tenant(&message).or(t),
            (t, false) => t,
        };
        let (key, handle) = match session {
            Some(k) => match self.session(&k) {
                Some((_, h)) => (k, h),
                None => {
                    let e = BpaError::new("IAM.NOT_FOUND", "unknown session");
                    return (
                        k.clone(),
                        vec![ubos_proto::error_frame("n0", &e, rid.as_deref(), None)],
                    );
                }
            },
            None => match self
                .open_session(tenant.as_deref(), authority.as_deref())
                .await
            {
                Ok(x) => x,
                Err(e) => {
                    return (
                        "".into(),
                        vec![ubos_proto::error_frame("n0", &e, rid.as_deref(), None)],
                    );
                }
            },
        };
        let mut signed_in: Option<Value> = None;
        if is_handshake {
            let t = tenant
                .clone()
                .unwrap_or_else(|| self.default_tenant.clone());
            let a = authority
                .clone()
                .unwrap_or_else(|| self.default_authority.clone());
            match self.authenticate(&mut message, &t, &a, source).await {
                Ok(Some((auth, token))) => {
                    signed_in = token;
                    if let Some(e) = self.sessions.lock().expect("sessions").get_mut(&key) {
                        e.auth = Some(auth);
                    }
                }
                Ok(None) => {}
                Err(e) => return (key.clone(), vec![self.own_error(&key, &e, rid.as_deref())]),
            }
        } else if let Err(e) = self.recheck(&key, &type_).await {
            self.close_session(&key);
            return (key.clone(), vec![self.own_error(&key, &e, rid.as_deref())]);
        }
        // the request limit per principal (DSN-NOD-604): the session's principal, else
        // the session
        let principal = self
            .sessions
            .lock()
            .expect("sessions")
            .get(&key)
            .and_then(|e| e.auth.as_ref().map(|a| a.principal.clone()))
            .unwrap_or_else(|| key.clone());
        if !is_handshake && !self.allow(&principal) {
            let e = BpaError::with_details(
                "LOGIC.LIMIT_EXCEEDED",
                json!({"reason": "rate_limited", "requests_per_second": self.config().int("limits.requests_per_second")}),
            );
            return (key.clone(), vec![self.own_error(&key, &e, rid.as_deref())]);
        }
        // a read-only node refuses writes and keeps serving reads (DSN-NOD-616)
        let read_only = self.modes.lock().expect("modes").read_only.clone();
        if type_ == "mutate"
            && let Some(reason) = read_only
        {
            let e = BpaError::with_details(
                "NOD.STORAGE_UNAVAILABLE",
                json!({"reason": "read_only", "detail": reason,
                    "message": "the node is read-only: the database does not accept writes; reads work"}),
            );
            let mut frames = vec![self.own_error(&key, &e, rid.as_deref())];
            self.banner_for(&key, &mut frames);
            return (key.clone(), frames);
        }
        // the edge span of the request, child of the client's trace (DSN-NOD-610)
        let incoming = message
            .get("trace")
            .and_then(Value::as_str)
            .and_then(crate::telemetry::Context::parse);
        let edge = self.tracer.child(incoming.as_ref());
        let dvm = self.tracer.child(Some(&edge));
        let started = std::time::Instant::now();
        let t0 = crate::telemetry::now_ns();
        let (out, phases) = handle
            .call_traced("ubtp", json!({"session": key, "messages": [message]}))
            .await;
        let t1 = crate::telemetry::now_ns();
        if log_enabled(LogLevel::Debug) {
            eprintln!(
                "ubos-server: debug {type_} session={key} {} us",
                started.elapsed().as_micros()
            );
        }
        let mut frames = match out {
            Ok(v) => v["responses"].as_array().cloned().unwrap_or_default(),
            Err(e) => vec![self.own_error(&key, &e, rid.as_deref())],
        };
        self.trace_request(
            &key,
            &type_,
            authority.as_deref(),
            incoming.as_ref(),
            &edge,
            &dvm,
            (t0, t1),
            &phases,
            &frames,
        );
        self.banner_for(&key, &mut frames);
        if let Some(t) = signed_in {
            match frames.first_mut() {
                Some(f) if f["type"] == json!("result") => f["body"]["credentials"] = t,
                // the binding refused the handshake: the identity session is not kept
                _ => {
                    let auth = self
                        .sessions
                        .lock()
                        .expect("sessions")
                        .get(&key)
                        .and_then(|e| e.auth.clone());
                    if let Some(a) = auth
                        && let Ok(id) = self.identity_async(&a.tenant).await
                    {
                        let _ = tokio::task::spawn_blocking(move || id.sign_out(&a.token)).await;
                    }
                }
            }
        }
        if type_ == "mutate" {
            self.release_now.notify_one();
            let me = self.clone();
            let k = key.clone();
            tokio::spawn(async move { me.fan_out(Some(&k)).await });
        }
        (key, frames)
    }

    /// The credentials a handshake over the network may carry: `password`, `bearer` and
    /// `api_key` of local identity in every preset, `dev` in the `dev` preset; each becomes
    /// the binding's in-process credentials. In-process credentials from the network are
    /// refused. Returns the identity credential bound to the session and, for a sign-in,
    /// the token to return.
    async fn authenticate(
        self: &Shared,
        message: &mut Value,
        tenant: &str,
        authority: &str,
        source: &str,
    ) -> Result<Option<(Auth, Option<Value>)>, BpaError> {
        let Some(body) = message.get_mut("body").and_then(Value::as_object_mut) else {
            return Ok(None);
        };
        let cred = body.get("credentials").cloned().unwrap_or(Value::Null);
        let kind = cred["kind"].as_str().unwrap_or("").to_string();
        let (auth, token): (Authenticated, Option<Value>) = match (kind.as_str(), self.preset) {
            ("dev", Preset::Dev) => {
                if let Some(c) = body.get_mut("credentials").and_then(Value::as_object_mut) {
                    c.insert("kind".into(), json!("in_process"));
                }
                return Ok(None);
            }
            ("in_process", _) => {
                return Err(BpaError::new(
                    "IAM.DENIED",
                    "in-process credentials are not accepted over the network",
                ));
            }
            ("password", _) => {
                let id = self.identity_async(tenant).await?;
                let src = source.to_string();
                let (username, password) = (
                    cred["username"].as_str().unwrap_or_default().to_string(),
                    cred["password"].as_str().unwrap_or_default().to_string(),
                );
                let totp = cred["totp"].as_str().map(str::to_string);
                let device = cred["device"].as_str().map(str::to_string);
                let id2 = id.clone();
                let signed = blocking(move || {
                    id2.sign_in(
                        &username,
                        &password,
                        totp.as_deref(),
                        &src,
                        device.as_deref(),
                    )
                })
                .await?;
                let src = source.to_string();
                let tok = signed.token.clone();
                let a = blocking(move || id.validate(&tok, &src)).await?;
                let t = json!({"token": signed.token, "session_id": signed.session_id,
                    "expires_at": crate::identity::iso(signed.expires_at), "level": signed.level});
                (a, Some(t))
            }
            ("bearer", _) | ("api_key", _) => {
                let token = cred[if kind == "bearer" { "token" } else { "key" }]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let id = self.identity_async(tenant).await?;
                let src = source.to_string();
                let tok = token.clone();
                (blocking(move || id.validate(&tok, &src)).await?, None)
            }
            (k, _) => {
                return Err(BpaError::new(
                    "IAM.DENIED",
                    format!("credentials of kind {k:?} are not accepted by this server"),
                ));
            }
        };
        if !crate::identity::scope_allows_vae(auth.scopes.as_deref(), authority) {
            return Err(BpaError::with_details(
                "IAM.DENIED",
                json!({"reason": "vae_not_in_scope", "vae": authority}),
            ));
        }
        body.insert("credentials".into(), auth.in_process());
        let secret = match &token {
            Some(t) => t["token"].as_str().unwrap_or_default().to_string(),
            None => cred[if kind == "bearer" { "token" } else { "key" }]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        };
        Ok(Some((
            Auth {
                token: secret,
                source: source.to_string(),
                tenant: tenant.to_string(),
                principal: auth.did,
                credential_id: auth.credential_id,
                scopes: auth.scopes,
                checked_at: now_secs(),
            },
            token,
        )))
    }

    /// Re-validate a session's identity credential when its last check is older than
    /// `RECHECK_SECS`, and refuse a mutate a read-only scope does not allow (DSN-NOD-153).
    async fn recheck(self: &Shared, key: &str, type_: &str) -> Result<(), BpaError> {
        let auth = self
            .sessions
            .lock()
            .expect("sessions")
            .get(key)
            .and_then(|e| e.auth.clone());
        let Some(a) = auth else { return Ok(()) };
        if type_ == "mutate" && !crate::identity::scope_allows_write(a.scopes.as_deref()) {
            return Err(BpaError::with_details(
                "IAM.DENIED",
                json!({"reason": "scope_read_only"}),
            ));
        }
        if now_secs() - a.checked_at < RECHECK_SECS {
            return Ok(());
        }
        let id = self.identity_async(&a.tenant).await?;
        let (tok, src) = (a.token.clone(), a.source.clone());
        blocking(move || id.validate(&tok, &src)).await?;
        if let Some(e) = self.sessions.lock().expect("sessions").get_mut(key)
            && let Some(x) = e.auth.as_mut()
        {
            x.checked_at = now_secs();
        }
        Ok(())
    }

    /// Push the emits of new commits to the other sessions of the VAE a session committed
    /// on (DSN-NOD-014): each session with a live stream gets the binding's catch-up.
    async fn fan_out(self: &Shared, from: Option<&str>) {
        let targets: Vec<(String, VaeHandle, tokio::sync::mpsc::Sender<Value>)> = {
            let sessions = self.sessions.lock().expect("sessions");
            let vae = from.and_then(|k| sessions.get(k).map(|s| s.vae.clone()));
            sessions
                .iter()
                .filter(|(k, s)| {
                    Some(k.as_str()) != from && vae.as_ref().is_none_or(|v| *v == s.vae)
                })
                .filter_map(|(k, s)| s.push.clone().map(|p| (k.clone(), s.handle.clone(), p)))
                .collect()
        };
        for (key, handle, push) in targets {
            let r = handle
                .call("ubtp", json!({"session": key, "messages": []}))
                .await;
            if let Ok(v) = r {
                for f in v["responses"].as_array().into_iter().flatten() {
                    if push.try_send(f.clone()).is_err() {
                        // the queue is full (DSN-NOD-016): the stream closes, the client
                        // resumes from its cursors
                        if let Some(entry) = self.sessions.lock().expect("sessions").get_mut(&key) {
                            entry.push = None;
                        }
                        break;
                    }
                }
            }
        }
    }

    /// The poll fallback of DSN-NOD-014: once a second, every streaming session catches up.
    async fn poll_loop(self: Shared) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tick.tick().await;
            self.fan_out(None).await;
            self.mirror_logs().await;
        }
    }

    /// Feed the kernel's decision records and new security-log entries of every open VAE
    /// into the tenant's node security log (DSN-NOD-111).
    async fn mirror_logs(self: &Shared) {
        let vaes: Vec<(String, VaeHandle)> = self
            .vaes
            .lock()
            .expect("vaes")
            .iter()
            .map(|(k, h)| (k.clone(), h.clone()))
            .collect();
        for (key, handle) in vaes {
            let tenant = key.split('/').next().unwrap_or_default().to_string();
            let Ok(id) = self.identity_async(&tenant).await else {
                continue;
            };
            let from = self
                .mirrored
                .lock()
                .expect("mirrored")
                .get(&key)
                .copied()
                .unwrap_or(0)
                + 1;
            let decisions = handle
                .call("decision_log", json!({}))
                .await
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            let security = handle
                .call("security_log", json!({"from": from, "limit": 1000}))
                .await
                .ok()
                .and_then(|v| v["entries"].as_array().cloned())
                .unwrap_or_default();
            if decisions.is_empty() && security.is_empty() {
                continue;
            }
            let last = security
                .last()
                .and_then(|e| e["seq"].as_u64())
                .unwrap_or(from - 1);
            let vae = key.clone();
            let r = blocking(move || id.mirror(&vae, &decisions, &security)).await;
            if r.is_ok() {
                self.mirrored.lock().expect("mirrored").insert(key, last);
            }
        }
    }
}

// ---- WebSocket (STD-PROTO-020) ----

#[derive(serde::Deserialize)]
struct WsQuery {
    tenant: Option<String>,
}

/// The tenant is `Ubtp-Tenant`, else the `tenant` query parameter (browsers cannot set
/// headers on a WebSocket).
async fn ws_upgrade(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(q): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let tenant = header(&headers, "ubtp-tenant").or(q.tenant);
    let source = peer.ip().to_string();
    ws.protocols([SUBPROTOCOL])
        .max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| ws_session(s, tenant, source, socket))
}

async fn ws_session(s: Shared, tenant: Option<String>, source: String, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    // the session entry holds the only sender: when the Server drops the entry (a revoked
    // or expired credential, DSN-NOD-106) the channel closes and so does the connection
    let (push_tx, mut push_rx) = tokio::sync::mpsc::channel::<Value>(QUEUE);
    let mut push_tx = Some(push_tx);
    let mut session: Option<String> = None;
    loop {
        tokio::select! {
            m = stream.next() => {
                let Some(Ok(m)) = m else { break };
                let text = match m {
                    Message::Text(t) => t.to_string(),
                    Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let message: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => {
                        let e = BpaError::new("INT.PROTOCOL_ERROR", "a frame is a JSON envelope");
                        let f = ubos_proto::error_frame("n0", &e, None, None);
                        if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
                        continue;
                    }
                };
                let (key, frames) = s.handle(session.clone(), tenant.clone(), &source, message).await;
                if session.is_none() && !key.is_empty() {
                    session = Some(key.clone());
                    if let Some(tx) = push_tx.take() {
                        s.attach_push(&key, tx);
                    }
                }
                for f in frames {
                    if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
                }
                // a session the Server closed (a revoked or expired credential) ends the connection
                if session.as_ref().is_some_and(|k| s.session(k).is_none()) { break; }
            }
            f = push_rx.recv() => {
                let Some(f) = f else { break };
                if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
            }
        }
    }
    if let Some(k) = session {
        s.close_session(&k);
    }
}

// ---- browsers (CORS) ----

/// The response headers a browser application reads: the session, tus and download headers.
const CORS_EXPOSE: &str = "ubtp-session, location, upload-offset, upload-length, upload-metadata, \
    tus-resumable, tus-version, tus-extension, tus-max-size, ubos-content-sha256, content-range, \
    content-length, accept-ranges";

/// Cross-origin requests from the origins of `server.cors_origins` (none by default): a
/// preflight is answered here, other responses carry the allowed origin and the headers
/// the SDK reads. Credentials travel as bearer headers, never as cookies.
async fn cors(
    State(s): State<Shared>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let origin = header(req.headers(), "origin").filter(|o| {
        s.config()
            .get("server.cors_origins")
            .and_then(Value::as_array)
            .is_some_and(|l| l.iter().any(|x| x.as_str() == Some(o.as_str())))
    });
    let Some(origin) = origin else {
        return next.run(req).await;
    };
    let preflight = req.method() == Method::OPTIONS
        && req.headers().contains_key("access-control-request-method");
    let mut resp = if preflight {
        let mut r = StatusCode::NO_CONTENT.into_response();
        let h = r.headers_mut();
        h.insert(
            "access-control-allow-methods",
            header::HeaderValue::from_static("GET, POST, PATCH, HEAD, DELETE, OPTIONS"),
        );
        if let Some(v) = req.headers().get("access-control-request-headers") {
            h.insert("access-control-allow-headers", v.clone());
        }
        h.insert(
            "access-control-max-age",
            header::HeaderValue::from_static("600"),
        );
        r
    } else {
        next.run(req).await
    };
    let h = resp.headers_mut();
    if let Ok(v) = origin.parse() {
        h.insert("access-control-allow-origin", v);
    }
    h.insert(
        "access-control-expose-headers",
        header::HeaderValue::from_static(CORS_EXPOSE),
    );
    h.append(header::VARY, header::HeaderValue::from_static("origin"));
    resp
}

// ---- HTTP (STD-PROTO-020) ----

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

async fn http_messages(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let tenant = header(&headers, "ubtp-tenant");
    let source = peer.ip().to_string();
    let mut session = header(&headers, "ubtp-session");
    let messages: Vec<Value> = match serde_json::from_slice::<Value>(&body) {
        Ok(Value::Array(a)) => a,
        Ok(v) => vec![v],
        Err(_) => {
            let e = BpaError::new(
                "INT.PROTOCOL_ERROR",
                "the body is a message or a list of messages",
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"responses": [ubos_proto::error_frame("n0", &e, None, None)]})),
            )
                .into_response();
        }
    };
    let mut out = Vec::new();
    // an empty list on an open session polls the binding for pending emits
    if messages.is_empty()
        && let Some(k) = &session
        && let Some((_, h)) = s.session(k)
        && let Ok(v) = h.call("ubtp", json!({"session": k, "messages": []})).await
    {
        out.extend(v["responses"].as_array().cloned().unwrap_or_default());
    }
    let traceparent = header(&headers, "traceparent");
    for mut m in messages {
        // the HTTP traceparent stands for a message without its own trace
        if let (Some(tp), Some(o)) = (&traceparent, m.as_object_mut())
            && !o.contains_key("trace")
        {
            o.insert("trace".into(), json!(tp));
        }
        let (key, frames) = s.handle(session.clone(), tenant.clone(), &source, m).await;
        if session.is_none() && !key.is_empty() {
            session = Some(key);
        }
        out.extend(frames);
    }
    let mut resp = Json(json!({"responses": out})).into_response();
    if let Some(k) = session
        && let Ok(v) = k.parse()
    {
        resp.headers_mut().insert("ubtp-session", v);
    }
    resp
}

#[derive(serde::Deserialize)]
struct StreamQuery {
    session: String,
}

async fn sse_stream(State(s): State<Shared>, Query(q): Query<StreamQuery>) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Value>(QUEUE);
    if !s.attach_push(&q.session, tx) {
        let e = BpaError::new("IAM.NOT_FOUND", "unknown session");
        return (StatusCode::NOT_FOUND, Json(problem(&e))).into_response();
    }
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|f| {
            let ev: Result<Event, std::convert::Infallible> =
                Ok(Event::default().event("emit").data(f.to_string()));
            (ev, rx)
        })
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ---- REST facade (STD-PROTO-021) ----

/// The session of a facade request: the `Ubtp-Session` header; else a session opened for
/// the `Authorization: Bearer` token (an identity session or an API key) and kept for its
/// next requests; else, in the `dev` preset, a session for the default principal.
async fn facade_session(
    s: &Shared,
    headers: &HeaderMap,
    vae: &str,
    source: &str,
) -> Result<String, BpaError> {
    if let Some(k) = header(headers, "ubtp-session") {
        return match s.session(&k) {
            Some(_) => Ok(k),
            None => Err(BpaError::new("IAM.NOT_FOUND", "unknown session")),
        };
    }
    let tenant = header(headers, "ubtp-tenant");
    let bearer = bearer_token(headers);
    let (cache_key, credentials) = match &bearer {
        Some(t) => (
            format!("bearer:{}:{vae}", crate::identity::token_hash(t)),
            json!({"kind": if t.contains(".ak.") { "api_key" } else { "bearer" },
                "token": t, "key": t}),
        ),
        None if s.preset == Preset::Dev => (
            format!("dev:{}/{vae}", tenant.clone().unwrap_or_default()),
            json!({"kind": "dev"}),
        ),
        None => {
            return Err(BpaError::new(
                "IAM.DENIED",
                "the facade needs a session (Ubtp-Session) or a bearer token (Authorization)",
            ));
        }
    };
    let cached = s
        .facade_sessions
        .lock()
        .expect("facade")
        .get(&cache_key)
        .cloned();
    if let Some(k) = cached {
        if s.session(&k).is_some() {
            return Ok(k);
        }
        s.facade_sessions.lock().expect("facade").remove(&cache_key);
    }
    let hs = json!({"v": ubos_proto::VERSION, "id": "facade", "type": "handshake",
        "body": {"vae": vae, "versions": [ubos_proto::VERSION], "credentials": credentials}});
    let (key, frames) = s.handle(None, tenant, source, hs).await;
    if frames.first().is_some_and(|f| f["type"] == json!("result")) {
        s.facade_sessions
            .lock()
            .expect("facade")
            .insert(cache_key, key.clone());
    }
    match frames.first() {
        Some(f) if f["type"] == json!("result") => Ok(key),
        Some(f) => Err(BpaError::with_details(
            f["body"]["code"]
                .as_str()
                .unwrap_or("IAM.DENIED")
                .to_string()
                .leak(),
            f["body"]["details"].clone(),
        )),
        None => Err(BpaError::new("IAM.DENIED", "no session")),
    }
}

fn facade_response(frames: Vec<Value>) -> Response {
    let Some(f) = frames.into_iter().next() else {
        let e = BpaError::new("NOD.STORAGE_UNAVAILABLE", "no response");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(problem(&e))).into_response();
    };
    if f["type"] == json!("error") {
        let code: &'static str = f["body"]["code"]
            .as_str()
            .unwrap_or("NOD.STORAGE_UNAVAILABLE")
            .to_string()
            .leak();
        let mut e = BpaError::with_details(code, f["body"]["details"].clone());
        e.message = f["body"]["details"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let status =
            StatusCode::from_u16(status_of(code)).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem(&e)),
        )
            .into_response();
    }
    Json(f["body"].clone()).into_response()
}

async fn facade(
    s: &Shared,
    headers: &HeaderMap,
    source: &str,
    vae: &str,
    id: &str,
    type_: &str,
    body: Value,
) -> Response {
    let key = match facade_session(s, headers, vae, source).await {
        Ok(k) => k,
        Err(e) => {
            let status = StatusCode::from_u16(status_of(e.code))
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            return (
                status,
                [(header::CONTENT_TYPE, "application/problem+json")],
                Json(problem(&e)),
            )
                .into_response();
        }
    };
    let n = {
        let mut sessions = s.sessions.lock().expect("sessions");
        match sessions.get_mut(&key) {
            Some(e) => {
                e.own += 1;
                e.own
            }
            None => 0,
        }
    };
    let message =
        json!({"v": ubos_proto::VERSION, "id": format!("{id}-{n}"), "type": type_, "body": body});
    let (_, frames) = s.handle(Some(key), None, source, message).await;
    facade_response(frames)
}

#[derive(serde::Deserialize)]
struct RestQuery {
    q: Option<String>,
    branch: Option<String>,
}

async fn rest_read(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, class, slug)): Path<(String, String, String)>,
    Query(q): Query<RestQuery>,
) -> Response {
    let mut body = json!({"vae": vae, "read": format!("ubos://{vae}/{class}/{slug}")});
    if let Some(b) = q.branch {
        body["branch"] = json!(b);
    }
    facade(
        &s,
        &headers,
        &peer.ip().to_string(),
        &vae,
        "read",
        "query",
        body,
    )
    .await
}

async fn rest_query(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, class)): Path<(String, String)>,
    Query(q): Query<RestQuery>,
) -> Response {
    let text = match q.q {
        Some(t) if t.trim_start().starts_with("from ") => t,
        Some(t) => format!("from {class} where {t}"),
        None => format!("from {class}"),
    };
    let mut body = json!({"vae": vae, "query": text});
    if let Some(b) = q.branch {
        body["branch"] = json!(b);
    }
    facade(
        &s,
        &headers,
        &peer.ip().to_string(),
        &vae,
        "query",
        "query",
        body,
    )
    .await
}

async fn rest_mutate(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, class, slug, port)): Path<(String, String, String, String)>,
    Query(q): Query<RestQuery>,
    body: Bytes,
) -> Response {
    let params: Value = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => {
                let e = BpaError::new(
                    "MODEL.INVALID_VALUE",
                    "the body is the JSON map of the port's parameters",
                );
                return (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/problem+json")],
                    Json(problem(&e)),
                )
                    .into_response();
            }
        }
    };
    let mut b = json!({"vae": vae, "port": {"target": format!("ubos://{vae}/{class}/{slug}"), "name": port, "params": params}});
    if let Some(br) = q.branch {
        b["branch"] = json!(br);
    }
    if let Some(k) = header(&headers, "idempotency-key") {
        b["idempotency_key"] = json!(k);
    }
    facade(
        &s,
        &headers,
        &peer.ip().to_string(),
        &vae,
        "mutate",
        "mutate",
        b,
    )
    .await
}

// ---- identity helpers and API (step 14c) ----

/// Run identity work (which may block on the database) off the runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, BpaError> + Send + 'static,
) -> Result<T, BpaError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// The tenant a handshake's bearer token or API key names (its prefix).
fn token_tenant(message: &Value) -> Option<String> {
    let c = &message["body"]["credentials"];
    let t = match c["kind"].as_str()? {
        "bearer" => c["token"].as_str()?,
        "api_key" => c["key"].as_str()?,
        _ => return None,
    };
    t.split('.').next().map(str::to_string)
}

/// The `Authorization: Bearer` token of a request.
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    header(headers, "authorization")
        .and_then(|a| a.strip_prefix("Bearer ").map(str::trim).map(str::to_string))
        .filter(|t| !t.is_empty())
}

fn problem_response(e: &BpaError) -> Response {
    let status =
        StatusCode::from_u16(status_of(e.code)).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut r = (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(problem(e)),
    )
        .into_response();
    if let Some(d) = e.details.as_ref().and_then(|d| d["retry_after"].as_i64())
        && let Ok(v) = d.to_string().parse()
    {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

/// The identity API under `/api/v1/identity` (JSON in, JSON out; RFC 9457 problems):
///
/// | Method and path | Who | Body |
/// |---|---|---|
/// | `POST sign-in` | anyone | `{username, password, totp?, device?}` → `{token, session_id, expires_at, principal, level}` |
/// | `POST sign-out` | bearer | — |
/// | `GET me` | bearer | — |
/// | `GET did/{did}` | anyone | the DID document (STD-ADDR-010) |
/// | `GET principals`, `POST principals` | admin | `{kind, name, role?, grants?}` |
/// | `GET principals/{did}` | admin or self | — |
/// | `POST principals/{did}/state` | admin | `{state}` |
/// | `POST principals/{did}/password` | admin or self | `{username, password}` |
/// | `POST credentials/{username}/totp`, `…/totp/confirm` | admin or owner | — / `{code}` |
/// | `GET`, `POST principals/{did}/api-keys` | admin or self | `{name, scopes, ip_ranges?, ttl_secs?}` |
/// | `DELETE api-keys/{key_id}` | admin or owner | — |
/// | `GET sessions`, `DELETE sessions/{id}` | admin (all) or self (own) | — |
/// | `POST principals/{did}/keys/rotate`, `POST principals/{did}/keys` | admin or self | — / `{public_key}` |
/// | `POST principals/{did}/sign` | self | `{message}` (base64url) |
/// | `POST principals/{did}/verify` | bearer | `{key, message, signature, at?}` |
/// | `GET security-log?from&to&limit`, `GET security-log/verify`, `GET security-log/export?from` | admin | — |
/// | `PUT session-policy` | admin | `{kind, idle_secs?, absolute_secs?, max_sessions?}` |
/// | `PUT secrets/{handle}`, `DELETE secrets/{handle}` | admin | `{value}` (never logged) |
///
/// The tenant is the bearer token's, else `Ubtp-Tenant`, else the default.
async fn identity_api(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    Path(rest): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let source = peer.ip().to_string();
    let bearer = bearer_token(&headers);
    let tenant = bearer
        .as_ref()
        .and_then(|t| t.split('.').next().map(str::to_string))
        // an invitation link names its tenant in its token
        .or_else(|| {
            rest.strip_prefix("invitations/")
                .and_then(|t| t.split('.').next())
                .map(str::to_string)
        })
        .or_else(|| header(&headers, "ubtp-tenant"))
        .unwrap_or_else(|| s.default_tenant.clone());
    let body: Value = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => {
                return problem_response(&BpaError::new("MODEL.INVALID_VALUE", "the body is JSON"));
            }
        }
    };
    let id = match s.identity_async(&tenant).await {
        Ok(i) => i,
        Err(e) => return problem_response(&e),
    };
    let me = match &bearer {
        Some(t) => {
            let (id2, t2, src) = (id.clone(), t.clone(), source.clone());
            match blocking(move || id2.validate(&t2, &src)).await {
                Ok(a) => Some(a),
                Err(e) => return problem_response(&e),
            }
        }
        None => None,
    };
    let secrets = s.secrets_async(&tenant).await;
    let segs: Vec<String> = rest.split('/').map(str::to_string).collect();
    let (id2, m2, me2, b2, q2, tok, src) = (
        id.clone(),
        method.clone(),
        me.clone(),
        body,
        q,
        bearer,
        source.clone(),
    );
    let out = blocking(move || {
        identity_call(
            &id2,
            secrets.as_ref(),
            &m2,
            &segs,
            me2.as_ref(),
            tok.as_deref(),
            &src,
            &b2,
            &q2,
        )
    })
    .await;
    match out {
        Ok(v) => {
            // revocations close the connections bound to what was revoked
            if let Some(closed) = v.get("_close") {
                s.close_bound(
                    &tenant,
                    closed["session_id"].as_str(),
                    closed["principal"].as_str(),
                );
                let mut v = v;
                v.as_object_mut().map(|m| m.remove("_close"));
                return Json(v).into_response();
            }
            if let Some(text) = v.get("_ndjson").and_then(Value::as_str) {
                return (
                    [(header::CONTENT_TYPE, "application/x-ndjson")],
                    text.to_string(),
                )
                    .into_response();
            }
            Json(v).into_response()
        }
        Err(e) => problem_response(&e),
    }
}

/// What an operator's job action answers: the reply, the object state to commit after it
/// and the job specification to create.
type AdminJobOutcome = (Value, Option<(String, Value)>, Option<Value>);

/// The lease name of a VAE's leadership.
fn lease_name(key: &str) -> String {
    format!("vae-leader:{key}")
}

/// The operator API under `/api/v1/admin` (an administrator's bearer token; the tenant is
/// the token's):
///
/// | Method and path | Body or query |
/// |---|---|
/// | `GET events` | `?vae&type&object&from&to&after&limit` — the event log (DSN-NOD-202) |
/// | `GET events/retention`, `PUT events/retention` | `{type, days}` (`days: null` removes the rule; `*` for the other types) |
/// | `POST events/sweep` | `{now?}` — apply the retention now |
/// | `GET leaders` | this node's leadership of each open VAE (DSN-NOD-228) |
/// | `GET listeners` | `?vae&branch&type&handler` — the listener registry (FR-EVT-022) |
/// | `GET jobs`, `GET jobs/{id}` | `?vae&state&kind&limit` — jobs and dead letters |
/// | `POST jobs/{id}/retry`, `…/cancel`, `…/annotate` | `{text}` for a note — operator actions on dead letters (FR-EVT-052), audited |
/// | `POST jobs` | `{kind: bulk \| port, payload, queue?, priority?, max_attempts?}` — an operator's job (DSN-NOD-230) |
/// | `POST jobs/{id}/priority` | `{priority}` — re-prioritise (DSN-NOD-229) |
/// | `GET queues`, `POST queues/{q}/pause`, `…/resume`, `PUT queues/{q}` | `{concurrency}` — queue controls and limits |
/// | `PUT jobs-limit` | `{concurrency}` — the tenant's limit of running jobs (FR-EVT-034) |
/// | `GET schedules`, `GET schedules/{id}/slots` | `?status&limit` — schedules with their cursor and next slots, and their recorded slots |
/// | `POST schedules/{id}/pause`, `…/resume` | pause or resume a schedule (FR-EVT-044), committed and audited |
async fn admin_api(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    Path(rest): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return problem_response(&BpaError::with_details(
            "IAM.DENIED",
            json!({"reason": "bearer_required"}),
        ));
    };
    let tenant = token.split('.').next().unwrap_or_default().to_string();
    let id = match s.identity_async(&tenant).await {
        Ok(i) => i,
        Err(e) => return problem_response(&e),
    };
    let src = peer.ip().to_string();
    let t2 = token.clone();
    let me = match blocking(move || id.validate(&t2, &src)).await {
        Ok(a) => a,
        Err(e) => return problem_response(&e),
    };
    if let Err(e) = need_admin(Some(&me)) {
        return problem_response(&e);
    }
    let body: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body).unwrap_or(json!({}))
    };
    let vae = q
        .get("vae")
        .cloned()
        .unwrap_or_else(|| s.default_authority.clone());
    let key = format!("{tenant}/{vae}");
    let segs: Vec<String> = rest.split('/').map(str::to_string).collect();
    let p: Vec<&str> = segs.iter().map(String::as_str).collect();
    if (method.as_str(), p.as_slice()) == ("GET", &["leaders"][..]) {
        let items: Vec<Value> = s
            .leaders
            .lock()
            .expect("leaders")
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("{tenant}/")))
            .map(|(k, l)| json!({"vae": k, "leader": l.held, "token": l.token, "node": s.node_id}))
            .collect();
        return Json(json!({"items": items})).into_response();
    }
    if (method.as_str(), p.as_slice()) == ("GET", &["listeners"][..]) {
        let h = match s.vae_async(&tenant, &vae).await {
            Ok(h) => h,
            Err(e) => return problem_response(&e),
        };
        let input = json!({"branch": q.get("branch").cloned().unwrap_or_else(|| "main".into()),
            "type": q.get("type"), "handler": q.get("handler")});
        return match h.call("host.listeners", input).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => problem_response(&e),
        };
    }
    if (method.as_str(), p.as_slice()) == ("POST", &["diagnostics"][..]) {
        return match s.diagnostics(&me.did).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => problem_response(&e),
        };
    }
    if matches!(p.first(), Some(&"health" | &"telemetry")) {
        return match admin_ops(&s, &method, &p, &q).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => problem_response(&e),
        };
    }
    if p.first() == Some(&"tenant") {
        return match admin_tenant(&s, &tenant, &method, &q, &body, &me.did).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => problem_response(&e),
        };
    }
    if p.first() == Some(&"config") {
        return match admin_config(&s, &method, &p, &q, &body, &me.did).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => problem_response(&e),
        };
    }
    if p.first() == Some(&"schedules") {
        return match admin_schedules(&s, &tenant, &vae, &key, &method, &p, &q, &me.did).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => problem_response(&e),
        };
    }
    if matches!(p.first(), Some(&"jobs" | &"queues" | &"jobs-limit")) {
        let h = match s.vae_async(&tenant, &vae).await {
            Ok(h) => h,
            Err(e) => return problem_response(&e),
        };
        let (me2, key2, m, segs2, by) = (
            s.clone(),
            key.clone(),
            method.clone(),
            segs.clone(),
            me.did.clone(),
        );
        let id2 = s.identity_async(&tenant).await.ok();
        let body2 = body.clone();
        // the table action; the object's new state, if any, is committed after it
        let out = blocking(move || -> Result<AdminJobOutcome, BpaError> {
            let jobs = me2
                .jobs_of(&key2)?
                .ok_or_else(|| BpaError::new("LOGIC.NOT_AVAILABLE", "this server keeps no jobs for the VAE"))?;
            let p: Vec<&str> = segs2.iter().map(String::as_str).collect();
            let audit = |action: &str, job: &str, ok: bool, extra: Value| {
                if let Some(id) = &id2 {
                    let _ = id.log(&format!("admin.job_{action}"), json!({"vae": key2, "job_id": job, "by": by, "ok": ok, "detail": extra}));
                }
            };
            match (m.as_str(), p.as_slice()) {
                ("GET", ["jobs"]) => {
                    let f = JobFilter {
                        state: q.get("state").cloned(),
                        kind: q.get("kind").cloned(),
                        queue: q.get("queue").cloned(),
                        limit: q.get("limit").and_then(|x| x.parse().ok()).unwrap_or(100),
                    };
                    Ok((json!({"items": jobs.list(&f)?}), None, None))
                }
                ("POST", ["jobs"]) => {
                    // an operator's job: a bulk process or a port call (DSN-NOD-230)
                    let kind = text(&body2, "kind")?.to_string();
                    if kind != "bulk" && kind != "port" {
                        return Err(BpaError::new("MODEL.INVALID_VALUE", "an operator enqueues bulk or port jobs"));
                    }
                    let id = crate::uuid()?;
                    let mut payload = body2.get("payload").cloned().unwrap_or_else(|| json!({}));
                    // an operator's port job runs as the operator (it has no parent process)
                    if kind == "port" && payload.is_object() {
                        payload["initiator"] = json!(by);
                    }
                    let spec = json!({"job_id": id, "kind": kind, "queue": body2.get("queue").cloned().unwrap_or_else(|| json!("default")),
                        "priority": body2.get("priority").cloned().unwrap_or_else(|| json!(0)),
                        "max_attempts": body2.get("max_attempts").cloned().unwrap_or_else(|| json!(3)),
                        "payload": payload});
                    audit("enqueue", &id, true, json!({"kind": kind}));
                    Ok((json!({"job_id": id}), None, Some(spec)))
                }
                ("GET", ["jobs", j]) => Ok((jobs.job(j)?.ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown job"))?, None, None)),
                ("POST", ["jobs", j, "retry"]) => {
                    let ok = jobs.retry(j)?;
                    audit("retry", j, ok, Value::Null);
                    Ok((json!({"ok": ok}), ok.then(|| (j.to_string(), json!({"state": "pending", "attempts": 0, "poison": false}))), None))
                }
                ("POST", ["jobs", j, "cancel"]) => {
                    let before = jobs.job(j)?.map(|x| x["state"].clone());
                    let ok = jobs.cancel(j)?;
                    audit("cancel", j, ok, json!({"was": before}));
                    // a running job is cancelled when its attempt ends (or between batches)
                    let set = (ok && before != Some(json!("running"))).then(|| (j.to_string(), json!({"state": "cancelled"})));
                    Ok((json!({"ok": ok, "running": before == Some(json!("running"))}), set, None))
                }
                ("POST", ["jobs", j, "annotate"]) => {
                    let ok = jobs.annotate(j, &by, text(&body2, "text")?)?;
                    audit("annotate", j, ok, Value::Null);
                    Ok((json!({"ok": ok}), None, None))
                }
                ("POST", ["jobs", j, "priority"]) => {
                    let pr = body2["priority"].as_i64().ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "priority is an integer"))?;
                    let ok = jobs.reprioritise(j, pr)?;
                    audit("priority", j, ok, json!({"priority": pr}));
                    Ok((json!({"ok": ok}), ok.then(|| (j.to_string(), json!({"priority": pr}))), None))
                }
                ("GET", ["queues"]) => Ok((json!({"items": jobs.queues()?}), None, None)),
                ("POST", ["queues", qn, action @ ("pause" | "resume")]) => {
                    jobs.set_queue(qn, Some(*action == "pause"), None)?;
                    audit(action, qn, true, json!({"queue": qn}));
                    Ok((json!({"ok": true}), None, None))
                }
                ("PUT", ["queues", qn]) => {
                    let c = body2["concurrency"].as_i64();
                    jobs.set_queue(qn, None, Some(c))?;
                    audit("queue_limit", qn, true, json!({"queue": qn, "concurrency": c}));
                    Ok((json!({"ok": true}), None, None))
                }
                ("PUT", ["jobs-limit"]) => {
                    let c = body2["concurrency"].as_i64();
                    jobs.set_tenant_limit(c)?;
                    audit("tenant_limit", "*", true, json!({"concurrency": c}));
                    Ok((json!({"ok": true}), None, None))
                }
                _ => Err(BpaError::new("IAM.NOT_FOUND", "unknown admin route")),
            }
        })
        .await;
        let (v, set, spec) = match out {
            Ok(x) => x,
            Err(e) => return problem_response(&e),
        };
        if let Some((job_id, set)) = set
            && let Err(e) = h
                .call("host.job_state", json!({"job_id": job_id, "set": set}))
                .await
        {
            return problem_response(&e);
        }
        if let Some(spec) = spec
            && let Err(e) = h.call("host.enqueue", json!({"specs": [spec]})).await
        {
            return problem_response(&e);
        }
        s.deliver_now.notify_one();
        return Json(v).into_response();
    }
    let me2 = s.clone();
    let m = method.clone();
    let out = blocking(move || {
        let ev = me2.events_of(&key)?.ok_or_else(|| {
            BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                "this server keeps no event log for the VAE",
            )
        })?;
        let p: Vec<&str> = segs.iter().map(String::as_str).collect();
        match (m.as_str(), p.as_slice()) {
            ("GET", ["events"]) => {
                let f = EventFilter {
                    type_: q.get("type").cloned(),
                    object_id: q.get("object").cloned(),
                    from: q.get("from").cloned(),
                    to: q.get("to").cloned(),
                    after: q.get("after").and_then(|x| x.parse().ok()).unwrap_or(0),
                    limit: q.get("limit").and_then(|x| x.parse().ok()).unwrap_or(100),
                };
                Ok(json!({"items": ev.query(&f)?}))
            }
            ("GET", ["events", "retention"]) => {
                let rules: Vec<Value> = ev
                    .retention()?
                    .into_iter()
                    .map(|(t, d)| json!({"type": t, "days": d}))
                    .collect();
                Ok(json!({"rules": rules}))
            }
            ("PUT", ["events", "retention"]) => {
                ev.set_retention(text(&body, "type")?, body["days"].as_i64())?;
                Ok(json!({"ok": true}))
            }
            ("POST", ["events", "sweep"]) => {
                let now = body["now"].as_str().map(str::to_string).unwrap_or_else(|| {
                    ubos_dvm::types::time::format_instant(&chrono::DateTime::<chrono::Utc>::from(
                        std::time::SystemTime::now(),
                    ))
                });
                Ok(json!({"removed": ev.sweep(&now)?}))
            }
            _ => Err(BpaError::new("IAM.NOT_FOUND", "unknown admin route")),
        }
    })
    .await;
    match out {
        Ok(v) => Json(v).into_response(),
        Err(e) => problem_response(&e),
    }
}

/// The tenant routes of the operator API (step 16b, DSN-NOD-401): `GET tenant` (the
/// registry record) and `PUT tenant` `{plan?, placement?, residency?}` — the change of a
/// platform setting (by CTL in later phases), projected at once into the `Tenant` object of
/// each of the tenant's VAEs and audited. `?slug=` names another tenant for an
/// administrator of the node's default tenant.
async fn admin_tenant(
    s: &Shared,
    own: &str,
    method: &Method,
    q: &HashMap<String, String>,
    body: &Value,
    by: &str,
) -> Result<Value, BpaError> {
    let slug = q.get("slug").cloned().unwrap_or_else(|| own.to_string());
    if slug != own && own != s.default_tenant {
        return Err(BpaError::with_details(
            "IAM.DENIED",
            json!({"reason": "another_tenant"}),
        ));
    }
    let (me, sl) = (s.clone(), slug.clone());
    match method.as_str() {
        "GET" => blocking(move || me.source.tenant_record(&sl))
            .await?
            .ok_or_else(|| {
                BpaError::new(
                    "LOGIC.NOT_AVAILABLE",
                    "this server keeps no tenant registry",
                )
            }),
        "PUT" => {
            let patch = body.clone();
            let rec = blocking(move || me.source.set_tenant_settings(&sl, &patch))
                .await?
                .ok_or_else(|| {
                    BpaError::new(
                        "LOGIC.NOT_AVAILABLE",
                        "this server keeps no tenant registry",
                    )
                })?;
            let mut projected = Vec::new();
            for a in rec["vaes"].as_array().cloned().unwrap_or_default() {
                let a = a.as_str().unwrap_or_default().to_string();
                let h = s.vae_async(&slug, &a).await?;
                h.call("host.project_tenant", rec.clone()).await?;
                projected.push(a);
            }
            if let Ok(id) = s.identity_async(&slug).await {
                let r = json!({"tenant": slug, "by": by, "plan": rec["plan"], "placement": rec["placement"], "residency": rec["residency"]});
                let _ = blocking(move || id.log("tenant.settings", r)).await;
            }
            Ok(json!({"tenant": rec, "projected": projected}))
        }
        _ => Err(BpaError::new("IAM.NOT_FOUND", "unknown admin route")),
    }
}

/// The configuration routes of the operator API (step 16a): `GET config` (the running
/// configuration, references only, and the schema), `GET config/versions`, `POST config`
/// `{document}` (validate, write the file when the node has one, apply).
async fn admin_config(
    s: &Shared,
    method: &Method,
    p: &[&str],
    q: &HashMap<String, String>,
    body: &Value,
    by: &str,
) -> Result<Value, BpaError> {
    match (method.as_str(), p) {
        ("GET", ["config"]) => {
            let c = s.config();
            Ok(json!({"hash": c.hash(), "values": c.values(), "schema": crate::config::schema()}))
        }
        ("GET", ["config", "versions"]) => {
            let limit = q.get("limit").and_then(|x| x.parse().ok()).unwrap_or(50);
            let me = s.clone();
            Ok(json!({"items": blocking(move || me.config_versions(limit)).await?}))
        }
        ("POST", ["config"]) => {
            let text = body["document"]
                .as_str()
                .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "document is the TOML text"))?
                .to_string();
            let (me, by) = (s.clone(), by.to_string());
            blocking(move || {
                let cfg = match Config::parse(&text) {
                    Ok(c) => c,
                    Err(e) => {
                        me.audit(
                            "config.rejected",
                            json!({"author": by, "source": "api", "errors": e.details}),
                        );
                        return Err(e);
                    }
                };
                // the file stays the source of truth: it is replaced first, atomically
                if let Some(path) = me.config_file.lock().expect("config file").clone() {
                    let tmp = path.with_extension("toml.new");
                    std::fs::write(&tmp, &text)
                        .and_then(|_| std::fs::rename(&tmp, &path))
                        .map_err(|e| {
                            BpaError::new(
                                "NOD.STORAGE_UNAVAILABLE",
                                format!("{}: {e}", path.display()),
                            )
                        })?;
                }
                me.apply_config(cfg, &by, "api")
            })
            .await
        }
        _ => Err(BpaError::new("IAM.NOT_FOUND", "unknown admin route")),
    }
}

/// The schedule routes of the operator API (step 15d).
#[allow(clippy::too_many_arguments)]
async fn admin_schedules(
    s: &Shared,
    tenant: &str,
    vae: &str,
    key: &str,
    method: &Method,
    p: &[&str],
    q: &HashMap<String, String>,
    by: &str,
) -> Result<Value, BpaError> {
    let h = s.vae_async(tenant, vae).await?;
    match (method.as_str(), p) {
        ("GET", ["schedules"]) => {
            let items = s.schedules_now(key, &h).await?;
            let (me, k) = (s.clone(), key.to_string());
            blocking(move || {
                let store = me.schedules_of(&k)?;
                let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
                let mut out = Vec::new();
                for it in &items {
                    let sid = it["schedule_id"].as_str().unwrap_or_default();
                    let b = &it["body"];
                    let cursor = store.cursor(sid)?;
                    let (next, error) = match crate::schedule::Rule::from_body(b) {
                        Ok(r) => {
                            let mut t = cursor.unwrap_or(now).max(now);
                            let mut n = Vec::new();
                            for _ in 0..3 {
                                match r.next_after(t) {
                                    Some(x) => {
                                        n.push(json!(ubos_dvm::types::time::format_instant(&x)));
                                        t = x;
                                    }
                                    None => break,
                                }
                            }
                            (n, Value::Null)
                        }
                        Err(e) => (Vec::new(), json!(e.to_string())),
                    };
                    out.push(json!({"schedule_id": sid, "name": b["name"], "rule": b["rule"], "time_zone": b["time_zone"],
                        "misfire": crate::schedule::misfire_of(b).unwrap_or("fire_once"),
                        "enabled": b.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                        "paused": b.get("paused").and_then(Value::as_bool).unwrap_or(false),
                        "cursor": cursor.map(|c| ubos_dvm::types::time::format_instant(&c)), "next": next, "error": error}));
                }
                Ok(json!({"items": out}))
            })
            .await
        }
        ("GET", ["schedules", sid, "slots"]) => {
            let (me, k, sid) = (s.clone(), key.to_string(), sid.to_string());
            let status = q.get("status").cloned();
            let limit = q.get("limit").and_then(|x| x.parse().ok()).unwrap_or(100);
            blocking(move || {
                let store = me.schedules_of(&k)?;
                Ok(json!({"items": store.slots(&sid, status.as_deref(), limit)?}))
            })
            .await
        }
        ("POST", ["schedules", sid, action @ ("pause" | "resume")]) => {
            let paused = *action == "pause";
            let r = h
                .call(
                    "host.set_paused",
                    json!({"schedule": sid, "paused": paused, "by": by}),
                )
                .await;
            if let Ok(id) = s.identity_async(tenant).await {
                let rec = json!({"vae": key, "schedule": sid, "by": by, "ok": r.is_ok()});
                let name = format!("admin.schedule_{action}");
                let _ = blocking(move || id.log(&name, rec)).await;
            }
            r.map(|c| json!({"ok": true, "commit_id": c["commit_id"]}))
        }
        _ => Err(BpaError::new("IAM.NOT_FOUND", "unknown admin route")),
    }
}

fn need_admin(me: Option<&Authenticated>) -> Result<&Authenticated, BpaError> {
    match me {
        Some(a) if a.is_admin() => Ok(a),
        Some(_) => Err(BpaError::with_details(
            "IAM.DENIED",
            json!({"reason": "admin_required"}),
        )),
        None => Err(BpaError::with_details(
            "IAM.DENIED",
            json!({"reason": "bearer_required"}),
        )),
    }
}

fn need_self_or_admin<'a>(
    me: Option<&'a Authenticated>,
    did: &str,
) -> Result<&'a Authenticated, BpaError> {
    match me {
        Some(a) if a.is_admin() || a.did == did => Ok(a),
        Some(_) => Err(BpaError::with_details(
            "IAM.DENIED",
            json!({"reason": "not_self"}),
        )),
        None => Err(BpaError::with_details(
            "IAM.DENIED",
            json!({"reason": "bearer_required"}),
        )),
    }
}

fn need_bearer(me: Option<&Authenticated>) -> Result<&Authenticated, BpaError> {
    me.ok_or_else(|| BpaError::with_details("IAM.DENIED", json!({"reason": "bearer_required"})))
}

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, BpaError> {
    v[k].as_str()
        .filter(|x| !x.is_empty())
        .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", format!("{k} is required")))
}

#[allow(clippy::too_many_arguments)]
fn identity_call(
    id: &Identity,
    secrets: Option<&crate::keys::SecretStore>,
    method: &Method,
    segs: &[String],
    me: Option<&Authenticated>,
    token: Option<&str>,
    source: &str,
    body: &Value,
    q: &HashMap<String, String>,
) -> Result<Value, BpaError> {
    let p: Vec<&str> = segs.iter().map(String::as_str).collect();
    let num = |k: &str| q.get(k).and_then(|v| v.parse::<u64>().ok());
    let not_found = || BpaError::new("IAM.NOT_FOUND", "unknown identity route");
    match (method.as_str(), p.as_slice()) {
        // the one-time administrator link of a bootstrap (DSN-NOD-602)
        ("POST", ["invitations", token]) => id.accept_invitation(token, text(body, "password")?),
        ("POST", ["sign-in"]) => {
            let signed = id.sign_in(
                text(body, "username")?,
                text(body, "password")?,
                body["totp"].as_str(),
                source,
                body["device"].as_str(),
            )?;
            Ok(
                json!({"token": signed.token, "session_id": signed.session_id, "principal": signed.principal,
                "level": signed.level, "expires_at": crate::identity::iso(signed.expires_at)}),
            )
        }
        ("POST", ["sign-out"]) => {
            let a = need_bearer(me)?;
            let ok = token.is_some_and(|t| id.sign_out(t).unwrap_or(false));
            Ok(json!({"ok": ok, "_close": {"session_id": a.credential_id}}))
        }
        ("GET", ["me"]) => {
            let a = need_bearer(me)?;
            Ok(
                json!({"principal": a.principal, "credential": a.credential, "credential_id": a.credential_id,
                "level": a.level, "scopes": a.scopes}),
            )
        }
        ("GET", ["did", did]) => id.did_document(did),
        ("GET", ["principals"]) => {
            need_admin(me)?;
            Ok(json!({"items": id.principals()?}))
        }
        ("POST", ["principals"]) => {
            need_admin(me)?;
            id.create_principal(
                text(body, "kind")?,
                text(body, "name")?,
                body["role"].as_str(),
                strings(&body["grants"]),
            )
        }
        ("GET", ["principals", did]) => {
            need_self_or_admin(me, did)?;
            id.principal(did)
        }
        ("POST", ["principals", did, "state"]) => {
            need_admin(me)?;
            let state = text(body, "state")?;
            let p = id.set_principal_state(did, state)?;
            if state == "suspended" || state == "deprovisioned" {
                return Ok(json!({"principal": p, "_close": {"principal": did}}));
            }
            Ok(p)
        }
        ("POST", ["principals", did, "password"]) => {
            need_self_or_admin(me, did)?;
            id.set_password(text(body, "username")?, did, text(body, "password")?)?;
            Ok(json!({"ok": true}))
        }
        ("POST", ["credentials", username, "totp"]) => {
            let owner = id.credential_principal(username)?;
            need_self_or_admin(me, &owner)?;
            id.enrol_totp(username)
        }
        ("POST", ["credentials", username, "totp", "confirm"]) => {
            let owner = id.credential_principal(username)?;
            need_self_or_admin(me, &owner)?;
            id.confirm_totp(username, text(body, "code")?)?;
            Ok(json!({"ok": true}))
        }
        ("GET", ["principals", did, "api-keys"]) => {
            need_self_or_admin(me, did)?;
            Ok(json!({"items": id.api_keys(Some(did))?}))
        }
        ("POST", ["principals", did, "api-keys"]) => {
            need_self_or_admin(me, did)?;
            id.issue_api_key(
                did,
                text(body, "name")?,
                strings(&body["scopes"]).unwrap_or_default(),
                strings(&body["ip_ranges"]).unwrap_or_default(),
                body["ttl_secs"].as_i64(),
            )
        }
        ("DELETE", ["api-keys", key_id]) => {
            let a = need_bearer(me)?;
            let owner = id
                .api_keys(None)?
                .into_iter()
                .find(|k| k["key_id"].as_str() == Some(key_id))
                .and_then(|k| k["principal"].as_str().map(str::to_string))
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown API key"))?;
            if !a.is_admin() && a.did != owner {
                return Err(BpaError::with_details(
                    "IAM.DENIED",
                    json!({"reason": "not_self"}),
                ));
            }
            Ok(json!({"ok": id.revoke_api_key(key_id)?}))
        }
        ("GET", ["sessions"]) => {
            let a = need_bearer(me)?;
            let items = if a.is_admin() {
                id.sessions(None)?
            } else {
                id.sessions(Some(&a.did))?
            };
            Ok(json!({"items": items}))
        }
        ("DELETE", ["sessions", session_id]) => {
            let a = need_bearer(me)?;
            let owner = id
                .session_principal(session_id)?
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown session"))?;
            if !a.is_admin() && a.did != owner {
                return Err(BpaError::with_details(
                    "IAM.DENIED",
                    json!({"reason": "not_self"}),
                ));
            }
            let ok = id.revoke_session(session_id, "revoked")?;
            Ok(json!({"ok": ok, "_close": {"session_id": session_id}}))
        }
        ("POST", ["principals", did, "keys", "rotate"]) => {
            need_self_or_admin(me, did)?;
            id.rotate_key(did)
        }
        ("POST", ["principals", did, "keys"]) => {
            need_self_or_admin(me, did)?;
            id.register_key(did, &body["public_key"])
        }
        ("GET", ["principals", did, "keys"]) => {
            need_self_or_admin(me, did)?;
            id.ensure_key(did)
        }
        ("POST", ["principals", did, "sign"]) => {
            let a = need_bearer(me)?;
            if a.did != *did {
                return Err(BpaError::with_details(
                    "IAM.DENIED",
                    json!({"reason": "not_self"}),
                ));
            }
            let msg = crate::identity::unb64u_any(&body["message"], "message")?;
            id.sign(did, &msg)
        }
        ("POST", ["principals", did, "verify"]) => {
            need_bearer(me)?;
            let msg = crate::identity::unb64u_any(&body["message"], "message")?;
            id.verify(
                did,
                text(body, "key")?,
                &msg,
                &body["signature"],
                body["at"].as_i64(),
            )
        }
        ("GET", ["security-log"]) => {
            need_admin(me)?;
            id.security_log(num("from"), num("to"), num("limit"))
        }
        ("GET", ["security-log", "verify"]) => {
            need_admin(me)?;
            id.verify_log()
        }
        ("GET", ["security-log", "export"]) => {
            need_admin(me)?;
            let v = id.security_log(num("from"), None, Some(10_000))?;
            let lines: Vec<String> = v["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .map(Value::to_string)
                .collect();
            Ok(json!({"_ndjson": lines.join("\n") + "\n"}))
        }
        ("PUT", ["session-policy"]) => {
            need_admin(me)?;
            id.set_session_policy(text(body, "kind")?, body)
        }
        ("PUT", ["secrets", handle]) => {
            need_admin(me)?;
            let store = secrets.ok_or_else(|| {
                BpaError::new("LOGIC.NOT_AVAILABLE", "this server has no secret store")
            })?;
            store.put(handle, text(body, "value")?.as_bytes())?;
            id.log(
                "secret.put",
                json!({"handle": handle, "by": me.map(|a| a.did.clone())}),
            )?;
            Ok(json!({"ok": true, "handle": handle}))
        }
        ("DELETE", ["secrets", handle]) => {
            need_admin(me)?;
            let store = secrets.ok_or_else(|| {
                BpaError::new("LOGIC.NOT_AVAILABLE", "this server has no secret store")
            })?;
            let ok = store.delete(handle)?;
            id.log(
                "secret.deleted",
                json!({"handle": handle, "by": me.map(|a| a.did.clone())}),
            )?;
            Ok(json!({"ok": ok}))
        }
        _ => Err(not_found()),
    }
}

/// A memory VAE for the `memory` preset and the suites: an engine over the memory store
/// with the system clock and OS entropy, the genesis classes loaded when given.
pub fn memory_vae(classes: Option<&Value>) -> Result<Vae, BpaError> {
    memory_vae_with(classes, None, None)
}

/// A memory VAE whose outbox is released into `events` (the memory preset's event log).
pub fn memory_vae_with(
    classes: Option<&Value>,
    events: Option<Arc<MemEventLog>>,
    jobs: Option<Arc<MemJobQueue>>,
) -> Result<Vae, BpaError> {
    use ubos_dvm::host::{
        NoConnectors, NoModels, OsEntropy, OutboxSink, Recorder, Services, SystemClock,
    };
    let mut engine = ubos_dvm::Engine::new(Box::new(ubos_dvm::store::MemStore::default()))?;
    if let Some(c) = classes {
        engine.load_genesis(c, None)?;
    }
    let rec = Arc::new(Recorder::default());
    let outbox: Arc<dyn OutboxSink> = match events {
        Some(e) => e,
        None => rec.clone(),
    };
    let services = Services {
        clock: Arc::new(SystemClock),
        entropy: Arc::new(OsEntropy),
        connectors: Arc::new(NoConnectors),
        models: Arc::new(NoModels),
        outbox,
        jobs: match jobs {
            Some(j) => j,
            None => rec.clone(),
        },
        telemetry: rec,
    };
    Ok(Vae::open(engine, services))
}

/// Unused-import guard for `Map` in builds without the facade helpers.
#[allow(dead_code)]
fn _map_type(_: &Map<String, Value>) {}

// ---- operations: traces, health, self-checks and degraded modes (step 16c) ----

impl Server {
    fn configure_telemetry(&self, cfg: &Config) {
        self.tracer.set_node(&self.node_id);
        self.tracer.configure(crate::telemetry::Export {
            mode: cfg.text("telemetry.export").unwrap_or("none").to_string(),
            endpoint: cfg.text("telemetry.otlp_endpoint").map(str::to_string),
            ratio: cfg.float("telemetry.sample_ratio"),
        });
    }

    /// Record the spans of a request: the edge span `ubtp.<type>`, the DVM operation and
    /// the kernel's phases; a process the request started is remembered with its trace.
    #[allow(clippy::too_many_arguments)]
    fn trace_request(
        &self,
        key: &str,
        type_: &str,
        authority: Option<&str>,
        incoming: Option<&crate::telemetry::Context>,
        edge: &crate::telemetry::Context,
        dvm: &crate::telemetry::Context,
        (t0, t1): (u64, u64),
        phases: &[ubos_dvm::types::trace::Phase],
        frames: &[Value],
    ) {
        let first = frames.first().cloned().unwrap_or(Value::Null);
        let pid = first["body"]["process_id"].as_str().map(str::to_string);
        let err = (first["type"] == "error").then(|| {
            first["body"]["code"]
                .as_str()
                .unwrap_or("error")
                .to_string()
        });
        let mut a = Map::new();
        a.insert("ubos.message_type".into(), json!(type_));
        a.insert("ubos.session".into(), json!(key));
        if let Some(v) = authority {
            a.insert("ubos.vae".into(), json!(v));
        }
        if let Some(p) = &pid {
            a.insert("ubos.process_id".into(), json!(p));
            let mut m = self.process_traces.lock().expect("traces");
            if m.len() >= 10_000 {
                m.clear();
            }
            m.insert(p.clone(), edge.clone());
        }
        if let Some(e) = &err {
            a.insert("ubos.error_code".into(), json!(e));
        }
        let end = crate::telemetry::now_ns();
        let mut d = a.clone();
        d.insert("ubos.op".into(), json!("ubtp"));
        self.tracer.record(
            dvm,
            Some(&edge.span_id),
            "dvm.ubtp",
            t0,
            t1,
            &d,
            err.as_deref(),
        );
        for ph in phases {
            let c = self.tracer.child(Some(dvm));
            self.tracer.record(
                &c,
                Some(&dvm.span_id),
                ph.name,
                ph.start_ns,
                ph.end_ns,
                &a,
                None,
            );
        }
        self.tracer.record(
            edge,
            incoming.map(|c| c.span_id.as_str()),
            &format!("ubtp.{type_}"),
            t0.saturating_sub(1),
            end,
            &a,
            err.as_deref(),
        );
    }

    /// The emit frame of the current degraded modes (DSN-NOD-616).
    fn banner_frame(&self, n: u64) -> Value {
        let m = self.modes.lock().expect("modes").clone();
        let message = match (&m.read_only, &m.effects_paused) {
            (Some(_), _) => {
                "The node is read-only: the database does not accept writes. Reads work; writes wait until it recovers."
            }
            (None, Some(_)) => {
                "Effects are paused: events and jobs are delivered once the node can release them again."
            }
            (None, None) => "The node works normally again.",
        };
        json!({"v": "1.0", "id": format!("b{n}"), "type": "emit",
            "body": {"banner": {"read_only": m.read_only.is_some(), "effects_paused": m.effects_paused.is_some(), "message": message, "epoch": m.epoch}}})
    }

    /// Add the banner to a session's frames when its last banner is older than the modes.
    fn banner_for(&self, key: &str, frames: &mut Vec<Value>) {
        let (epoch, active) = {
            let m = self.modes.lock().expect("modes");
            (m.epoch, m.read_only.is_some() || m.effects_paused.is_some())
        };
        if epoch == 0 {
            return;
        }
        // a session hears of a degraded mode, and of the recovery when it heard of the mode
        let due = match self.sessions.lock().expect("sessions").get_mut(key) {
            Some(e) if e.banner_epoch < epoch => {
                let tell = active || e.banner_epoch > 0;
                e.banner_epoch = epoch;
                tell
            }
            _ => false,
        };
        if due {
            frames.push(self.banner_frame(epoch));
        }
    }

    /// Enter or leave a degraded mode; a change is audited, logged and pushed to the
    /// streaming sessions as a banner.
    fn set_mode(&self, mode: &str, reason: Option<String>) {
        let changed = {
            let mut m = self.modes.lock().expect("modes");
            let slot = if mode == "read_only" {
                &mut m.read_only
            } else {
                &mut m.effects_paused
            };
            if slot.is_some() == reason.is_some() {
                false
            } else {
                *slot = reason.clone();
                m.epoch += 1;
                true
            }
        };
        if !changed {
            return;
        }
        let rec = json!({"mode": mode, "active": reason.is_some(), "reason": reason});
        eprintln!(
            "ubos-server: mode {mode} {}",
            if reason.is_some() { "entered" } else { "left" }
        );
        self.tracer.log(
            if reason.is_some() { "WARN" } else { "INFO" },
            json!({"name": "node.mode", "mode": mode, "active": reason.is_some()}),
        );
        self.audit("node.mode", rec);
        let epoch = self.modes.lock().expect("modes").epoch;
        let frame = self.banner_frame(epoch);
        let mut sessions = self.sessions.lock().expect("sessions");
        for e in sessions.values_mut() {
            if let Some(p) = &e.push
                && p.try_send(frame.clone()).is_ok()
            {
                e.banner_epoch = epoch;
            }
        }
    }

    fn release_ok(&self) {
        if self
            .release_failures
            .swap(0, std::sync::atomic::Ordering::Relaxed)
            > 0
        {
            self.set_mode("effects_paused", None);
        }
    }

    fn release_failed(&self, e: &BpaError) {
        let n = self
            .release_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        if n >= 3 {
            self.set_mode(
                "effects_paused",
                Some(format!("{n} release rounds failed: {}", e.code)),
            );
        }
    }

    /// The readiness of the node (DSN-NOD-613): the database, the key service and the
    /// degraded modes; `(ready, report)`.
    fn readiness(&self) -> (bool, Value) {
        let mut checks = Vec::new();
        let mut ready = true;
        if let Some(db) = self.source.probe_database() {
            ready &= db["ok"] == json!(true);
            checks.push(json!({"check": "database", "ok": db["ok"], "detail": db.get("detail").cloned().unwrap_or(json!({"writable": db["writable"]}))}));
        }
        if !ready {
            // the key service is reached through the database
            checks.push(json!({"check": "key service", "ok": Value::Null, "detail": "not checked: the database is unavailable"}));
        } else if let Some(k) = self.source.probe_keys(&self.default_tenant) {
            ready &= k["ok"] == json!(true);
            checks.push(json!({"check": "key service", "ok": k["ok"], "detail": k.get("detail").cloned().unwrap_or(Value::Null)}));
        }
        checks.push(json!({"check": "object store", "ok": true, "detail": "none in PH-1: blobs live in PostgreSQL (DEC-077)"}));
        let modes = self.modes.lock().expect("modes").json();
        let status = if !ready {
            "not ready"
        } else if modes["read_only"] == json!(true) || modes["effects_paused"] == json!(true) {
            "degraded"
        } else {
            "ready"
        };
        (
            ready,
            json!({"status": status, "checks": checks, "modes": modes}),
        )
    }

    /// The self-checks (DSN-NOD-614): clock, certificate, keys, disk, backups, the DVM root
    /// verifier and isolation; a check that turns to warning or error raises an alert.
    async fn run_selfchecks(self: &Shared) -> Vec<Value> {
        let cfg = self.config();
        let mut out = Vec::new();
        let check = |name: &str, status: &str, detail: Value| json!({"check": name, "status": status, "detail": detail});
        let me = self.clone();
        let db = blocking(move || Ok(me.source.probe_database()))
            .await
            .ok()
            .flatten();
        match &db {
            Some(d) if d["ok"] == json!(true) => {
                let skew = d["clock_skew_ms"].as_i64().unwrap_or(0);
                out.push(check(
                    "clock",
                    if skew.abs() <= crate::MAX_CLOCK_SKEW_MS {
                        "ok"
                    } else {
                        "warning"
                    },
                    json!(format!("{skew} ms against the database")),
                ));
            }
            Some(d) => out.push(check(
                "clock",
                "error",
                json!(format!("the database does not answer: {}", d["detail"])),
            )),
            None => out.push(check("clock", "ok", json!("no database (memory store)"))),
        }
        match cfg.text("server.tls_certificate") {
            None => out.push(check(
                "certificate",
                "ok",
                json!("no certificate configured"),
            )),
            Some(p) => match std::fs::read_to_string(p)
                .ok()
                .and_then(|t| cert_not_after(&t))
            {
                None => out.push(check(
                    "certificate",
                    "error",
                    json!(format!("{p}: not a readable PEM certificate")),
                )),
                Some(t) => {
                    let days =
                        (t - chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()))
                            .num_days();
                    let st = if days < 0 {
                        "error"
                    } else if days <= 30 {
                        "warning"
                    } else {
                        "ok"
                    };
                    out.push(check(
                        "certificate",
                        st,
                        json!(format!(
                            "{p} expires {} ({days} days)",
                            ubos_dvm::types::time::format_instant(&t)
                        )),
                    ));
                }
            },
        }
        let (me, t) = (self.clone(), self.default_tenant.clone());
        match blocking(move || Ok(me.source.probe_keys(&t)))
            .await
            .ok()
            .flatten()
        {
            Some(k) if k["ok"] == json!(true) => out.push(check(
                "keys",
                "ok",
                json!("the root key opens the tenant key"),
            )),
            Some(k) => out.push(check("keys", "error", k["detail"].clone())),
            None => out.push(check("keys", "ok", json!("no key service (memory store)"))),
        }
        let min = cfg.int("selfcheck.disk_min_free_mb").max(0) as u64;
        let mut paths = vec![".".to_string()];
        if let Some(b) = cfg.text("backup.dir") {
            paths.push(b.to_string());
        }
        for p in paths {
            match disk_free_mb(&p) {
                Some(f) => out.push(check(
                    "disk",
                    if f < min { "warning" } else { "ok" },
                    json!(format!("{p}: {f} MB free (minimum {min})")),
                )),
                None => out.push(check(
                    "disk",
                    "warning",
                    json!(format!("{p}: free space unknown")),
                )),
            }
        }
        let me = self.clone();
        let archiver = blocking(move || Ok(me.source.archiver()))
            .await
            .ok()
            .flatten();
        match crate::backup::Target::of(&cfg) {
            Ok(Some(t)) => {
                let st = crate::backup::status(&t);
                match st["base_age_secs"].as_u64() {
                    Some(age) if age <= 26 * 3600 => out.push(check(
                        "backup age",
                        "ok",
                        json!(format!("the newest base backup is {} h old", age / 3600)),
                    )),
                    Some(age) => out.push(check(
                        "backup age",
                        "warning",
                        json!(format!("the newest base backup is {} h old", age / 3600)),
                    )),
                    None => out.push(check(
                        "backup age",
                        "warning",
                        json!(format!("{}: no base backup yet", t.dir.display())),
                    )),
                }
                match &archiver {
                    Some(a) if a["archive_mode"] == "off" => out.push(check(
                        "wal archive",
                        "warning",
                        json!("archive_mode is off: no point-in-time restore"),
                    )),
                    Some(a) if a["failing"] == json!(true) => out.push(check(
                        "wal archive",
                        "error",
                        json!(format!("archiving fails ({} failures)", a["failed_count"])),
                    )),
                    Some(a) if a.get("error").is_none() => out.push(check(
                        "wal archive",
                        "ok",
                        json!(format!("{} segments archived", a["archived_count"])),
                    )),
                    _ => {}
                }
            }
            Err(e) => out.push(check("backup age", "error", json!(e.to_string()))),
            Ok(None) if db.is_some() && cfg.text("server.preset") == Some("server") => {
                out.push(check(
                    "backup age",
                    "warning",
                    json!("no backup directory configured (backup.dir)"),
                ))
            }
            Ok(None) => out.push(check(
                "backup age",
                "ok",
                json!("no backups expected (development)"),
            )),
        }
        match self
            .vae_async(&self.default_tenant, &self.default_authority)
            .await
        {
            Ok(h) => match h.call("host.verify_roots", json!({"last": 20})).await {
                Ok(r) if r["findings"].as_array().is_some_and(Vec::is_empty) => out.push(check(
                    "dvm verifier",
                    "ok",
                    json!(format!("{} recent commits of main verified", r["checked"])),
                )),
                Ok(r) => out.push(check("dvm verifier", "error", r["findings"].clone())),
                Err(e) => out.push(check("dvm verifier", "warning", json!(e.to_string()))),
            },
            Err(e) => out.push(check("dvm verifier", "warning", json!(e.to_string()))),
        }
        let defects = crate::db::ISOLATION_DEFECTS.load(std::sync::atomic::Ordering::Relaxed);
        out.push(check(
            "isolation",
            if defects == 0 { "ok" } else { "error" },
            json!(format!("{defects} isolation defects")),
        ));
        let errors = self
            .tracer
            .export_errors
            .load(std::sync::atomic::Ordering::Relaxed);
        let last = self.tracer.last_error.lock().expect("last").clone();
        out.push(check(
            "telemetry export",
            if errors == 0 { "ok" } else { "warning" },
            json!(format!(
                "{errors} failed exports{}",
                last.map(|l| format!("; last: {l}")).unwrap_or_default()
            )),
        ));
        // alerts on changes to warning or error
        let before: HashMap<String, String> = self
            .checks
            .lock()
            .expect("checks")
            .iter()
            .map(|c| {
                (
                    format!("{}|{}", c["check"], c["detail"]),
                    c["status"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        for c in &out {
            let st = c["status"].as_str().unwrap_or("ok");
            let k = format!("{}|{}", c["check"], c["detail"]);
            if st != "ok" && before.get(&k).map(String::as_str) != Some(st) {
                self.audit("selfcheck.alert", c.clone());
                self.tracer.log(
                    if st == "error" { "ERROR" } else { "WARN" },
                    json!({"name": "node.selfcheck", "check": c["check"], "status": st}),
                );
                eprintln!(
                    "ubos-server: self-check {}: {st}: {}",
                    c["check"], c["detail"]
                );
            }
        }
        *self.checks.lock().expect("checks") = out.clone();
        out
    }

    /// The operations loop: telemetry export every second (metrics every 10 s), the
    /// database's writability every 5 s (read-only mode), the self-checks every
    /// `selfcheck.interval_secs`.
    async fn operations_loop(self: Shared) {
        self.started
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut tick: u64 = 0;
        let mut last_checks = std::time::Instant::now() - std::time::Duration::from_secs(3600);
        let mut last_point = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            tick += 1;
            let gauges = self.gauges();
            let t = self.tracer.clone();
            let with_metrics = tick.is_multiple_of(10);
            let _ = tokio::task::spawn_blocking(move || t.flush(&gauges, with_metrics)).await;
            if tick.is_multiple_of(5) {
                let me = self.clone();
                if let Ok(Some(db)) = blocking(move || Ok(me.source.probe_database())).await {
                    if db["ok"] == json!(true) && db["writable"] == json!(false) {
                        let why = if db["in_recovery"] == json!(true) {
                            "the database is in recovery"
                        } else {
                            "the database refuses writes"
                        };
                        self.set_mode("read_only", Some(why.to_string()));
                    } else if db["ok"] == json!(true) {
                        self.set_mode("read_only", None);
                    }
                }
            }
            let cfg = self.config();
            let minutes = cfg.int("backup.restore_point_minutes");
            if minutes > 0 && last_point.elapsed().as_secs() >= minutes as u64 * 60 {
                last_point = std::time::Instant::now();
                if let Ok(Some(t)) = crate::backup::Target::of(&cfg) {
                    let me = self.clone();
                    let name = format!(
                        "ubos-{}",
                        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                            .format("%Y%m%dT%H%M%SZ")
                    );
                    match blocking(move || Ok(me.source.restore_point(&t, &name))).await {
                        Ok(Some(Err(e))) => {
                            self.log_error("restore point", "restore point".to_string(), &e)
                        }
                        Ok(Some(Ok(p))) => self
                            .tracer
                            .log("INFO", json!({"name": "backup.point", "vaes": p["vaes"]})),
                        _ => {}
                    }
                }
            }
            let every = self.config().int("selfcheck.interval_secs").max(5) as u64;
            if last_checks.elapsed().as_secs() >= every {
                last_checks = std::time::Instant::now();
                self.run_selfchecks().await;
            }
        }
    }
}

/// The free space of the file system of a path in MB (`df -Pk`).
fn disk_free_mb(path: &str) -> Option<u64> {
    let o = std::process::Command::new("df")
        .args(["-Pk", path])
        .output()
        .ok()?;
    let t = String::from_utf8_lossy(&o.stdout);
    let line = t.lines().nth(1)?;
    line.split_whitespace()
        .nth(3)?
        .parse::<u64>()
        .ok()
        .map(|k| k / 1024)
}

/// The `notAfter` of the first certificate of a PEM text (DER: Certificate → tbsCertificate
/// → validity).
pub fn cert_not_after(pem: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let b64: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END"))
        .collect();
    let der = base64_decode(&b64)?;
    // (tag, content start, content end)
    let tlv = |b: &[u8], at: usize| -> Option<(u8, usize, usize)> {
        let tag = *b.get(at)?;
        let l0 = *b.get(at + 1)? as usize;
        let (len, hdr) = if l0 < 0x80 {
            (l0, 2)
        } else {
            let n = l0 & 0x7f;
            let mut len = 0usize;
            for i in 0..n {
                len = (len << 8) | *b.get(at + 2 + i)? as usize;
            }
            (len, 2 + n)
        };
        Some((tag, at + hdr, at + hdr + len))
    };
    let (_, cs, _) = tlv(&der, 0)?; // Certificate
    let (_, ts, _) = tlv(&der, cs)?; // tbsCertificate
    let mut at = ts;
    let (tag, _, end) = tlv(&der, at)?;
    if tag == 0xa0 {
        at = end; // [0] version
    }
    for _ in 0..3 {
        let (_, _, end) = tlv(&der, at)?; // serial, signature, issuer
        at = end;
    }
    let (_, vs, _) = tlv(&der, at)?; // validity
    let (_, _, nb_end) = tlv(&der, vs)?; // notBefore
    let (tag, s, e) = tlv(&der, nb_end)?; // notAfter
    let text = std::str::from_utf8(der.get(s..e)?).ok()?;
    let full = if tag == 0x17 {
        // UTCTime YYMMDDHHMMSSZ
        let yy: i32 = text.get(0..2)?.parse().ok()?;
        format!(
            "{}{}",
            if yy >= 50 { 1900 + yy } else { 2000 + yy },
            text.get(2..)?
        )
    } else {
        text.to_string()
    };
    chrono::NaiveDateTime::parse_from_str(full.trim_end_matches('Z'), "%Y%m%d%H%M%S")
        .ok()
        .map(|n| n.and_utc())
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The liveness probe (DSN-NOD-613): the process answers.
async fn livez() -> Response {
    Json(json!({"status": "live"})).into_response()
}

/// The startup probe: the Server's loops run.
async fn startupz(State(s): State<Shared>) -> Response {
    if s.started.load(std::sync::atomic::Ordering::Relaxed) {
        Json(json!({"status": "started"})).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "starting"})),
        )
            .into_response()
    }
}

/// The readiness probe (DSN-NOD-613): the database and the key service answer; degraded
/// modes are reported.
async fn readyz(State(s): State<Shared>) -> Response {
    let me = s.clone();
    let (ready, report) = tokio::task::spawn_blocking(move || me.readiness())
        .await
        .unwrap_or((false, json!({"status": "not ready"})));
    if ready {
        Json(report).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(report)).into_response()
    }
}

/// The operator's view of operations (step 16c): `GET health` (readiness, modes and the
/// last self-checks), `POST health/run` (run the self-checks now), `GET telemetry/spans`
/// `?trace&limit` (recent spans), `GET telemetry/metrics`.
async fn admin_ops(
    s: &Shared,
    method: &Method,
    p: &[&str],
    q: &HashMap<String, String>,
) -> Result<Value, BpaError> {
    match (method.as_str(), p) {
        ("GET", ["health"]) => {
            let me = s.clone();
            let (_, r) = blocking(move || Ok(me.readiness())).await?;
            Ok(json!({"readiness": r, "selfchecks": *s.checks.lock().expect("checks")}))
        }
        ("POST", ["health", "run"]) => Ok(json!({"selfchecks": s.run_selfchecks().await})),
        ("GET", ["telemetry", "spans"]) => {
            let limit = q.get("limit").and_then(|x| x.parse().ok()).unwrap_or(200);
            Ok(json!({"items": s.tracer.recent(q.get("trace").map(String::as_str), limit)}))
        }
        ("GET", ["telemetry", "metrics"]) => Ok(json!({"spans": s.tracer.metrics(),
            "dropped": s.tracer.dropped.load(std::sync::atomic::Ordering::Relaxed),
            "export_errors": s.tracer.export_errors.load(std::sync::atomic::Ordering::Relaxed)})),
        _ => Err(BpaError::new("IAM.NOT_FOUND", "unknown admin route")),
    }
}

// ---- diagnostic bundles (step 16d) ----

impl Server {
    /// An operational error: to stderr in full, to the telemetry logs as its operation and
    /// code only (no message, which may quote a value: DSN-NOD-611).
    fn log_error(&self, operation: &str, what: String, e: &BpaError) {
        eprintln!("ubos-server: {what}: {e}");
        self.tracer.log(
            "ERROR",
            json!({"name": "node.error", "operation": operation, "code": e.code}),
        );
    }

    /// The node gauges of the metrics export and the diagnostic bundle.
    fn gauges(&self) -> Value {
        let modes = self.modes.lock().expect("modes").clone();
        json!({"sessions": self.sessions.lock().expect("sessions").len(), "vaes_open": self.vaes.lock().expect("vaes").len(),
            "isolation_defects": crate::db::ISOLATION_DEFECTS.load(std::sync::atomic::Ordering::Relaxed),
            "spans_dropped": self.tracer.dropped.load(std::sync::atomic::Ordering::Relaxed),
            "release_failures": self.release_failures.load(std::sync::atomic::Ordering::Relaxed),
            "read_only": modes.read_only.is_some() as i64, "effects_paused": modes.effects_paused.is_some() as i64})
    }

    /// A diagnostic bundle of the running node (DSN-NOD-615, NR-OPER-005): operational
    /// sources only, checked against the configured secrets ([`crate::diagnostics`]);
    /// audited as `diagnostics.bundle`.
    async fn diagnostics(self: &Shared, by: &str) -> Result<Value, BpaError> {
        let started = std::time::Instant::now();
        let cfg = self.config();
        let selfchecks = self.run_selfchecks().await;
        let me = self.clone();
        let (history, readiness, database, log) = blocking(move || {
            let history = me.config_versions(20).unwrap_or_default();
            let (_, readiness) = me.readiness();
            let database = me.source.diagnostics();
            let log = me
                .identity(&me.default_tenant)
                .and_then(|id| id.security_log(None, None, Some(10_000)))
                .map(|l| crate::diagnostics::log_summary(&l))
                .unwrap_or_else(|e| json!({"error": e.to_string()}));
            Ok((history, readiness, database, log))
        })
        .await?;
        let export = self.tracer.export();
        let mut m = Map::new();
        m.insert("versions".into(), crate::diagnostics::versions());
        m.insert("host".into(), crate::diagnostics::host());
        m.insert(
            "configuration".into(),
            json!({"hash": cfg.hash(), "values": cfg.values(), "versions": history}),
        );
        m.insert(
            "health".into(),
            json!({"readiness": readiness, "selfchecks": selfchecks}),
        );
        m.insert("telemetry".into(), json!({"export": {"mode": export.mode, "endpoint": export.endpoint, "ratio": export.ratio},
            "metrics": self.tracer.metrics(),
            "dropped": self.tracer.dropped.load(std::sync::atomic::Ordering::Relaxed),
            "export_errors": self.tracer.export_errors.load(std::sync::atomic::Ordering::Relaxed)}));
        m.insert("spans".into(), json!(self.tracer.recent(None, 500)));
        m.insert("logs".into(), json!(self.tracer.history(500)));
        m.insert(
            "runtime".into(),
            json!({"node": self.node_id, "preset": format!("{:?}", self.preset),
            "gauges": self.gauges(), "leaders": self.leaders.lock().expect("leaders").len()}),
        );
        m.insert("database".into(), database.unwrap_or(Value::Null));
        m.insert("security_log".into(), log);
        let bundle =
            crate::diagnostics::finish(m, &crate::config::secret_values(&cfg), "online", started);
        let mf = &bundle["manifest"];
        self.audit(
            "diagnostics.bundle",
            json!({"author": by, "content_hash": mf["content_hash"], "duration_ms": mf["duration_ms"], "redactions": mf["redactions"]}),
        );
        Ok(bundle)
    }
}

// ---- files: tus-compatible uploads, links and downloads (step 17a, DEC-078) ----

/// The largest body of one upload request.
const UPLOAD_PATCH_MAX: usize = 64 << 20;
/// The tus protocol version served.
const TUS: &str = "1.0.0";

impl Server {
    /// The file service of a tenant: the source's (PostgreSQL) or one in memory.
    fn files_of(&self, tenant: &str) -> Result<Arc<crate::files::Files>, BpaError> {
        if let Some(f) = self.files.lock().expect("files").get(tenant) {
            return Ok(f.clone());
        }
        let f = Arc::new(match self.source.files(tenant)? {
            Some(f) => f,
            None => crate::files::Files::memory(tenant)?,
        });
        self.files
            .lock()
            .expect("files")
            .insert(tenant.to_string(), f.clone());
        Ok(f)
    }

    /// The tenant and principal of a session (`dev` without authentication).
    fn session_principal(&self, key: &str) -> Option<(String, String)> {
        self.sessions.lock().expect("sessions").get(key).map(|e| {
            (
                e.tenant.clone(),
                e.auth
                    .as_ref()
                    .map(|a| a.principal.clone())
                    .unwrap_or_else(|| "dev".into()),
            )
        })
    }
}

/// A file-API error: the problem body with the tus header.
fn file_problem(e: &BpaError) -> Response {
    let status =
        StatusCode::from_u16(status_of(e.code)).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        [
            (header::CONTENT_TYPE, "application/problem+json".to_string()),
            (
                header::HeaderName::from_static("tus-resumable"),
                TUS.to_string(),
            ),
        ],
        Json(problem(e)),
    )
        .into_response()
}

/// The session, tenant and principal of a file request (the facade's authentication).
async fn file_caller(
    s: &Shared,
    headers: &HeaderMap,
    vae: &str,
    peer: &SocketAddr,
) -> Result<(String, String, String), BpaError> {
    let key = facade_session(s, headers, vae, &peer.ip().to_string()).await?;
    let (tenant, principal) = s
        .session_principal(&key)
        .ok_or_else(|| BpaError::new("IAM.DENIED", "no session"))?;
    Ok((key, tenant, principal))
}

/// `OPTIONS /files/v1/{vae}`: the tus capabilities.
async fn upload_options(State(s): State<Shared>) -> Response {
    let max = (s.config().int("files.max_upload_mb").max(1) as u64) << 20;
    (
        StatusCode::NO_CONTENT,
        [
            ("tus-resumable", TUS.to_string()),
            ("tus-version", TUS.to_string()),
            ("tus-extension", "creation,termination".to_string()),
            ("tus-max-size", max.to_string()),
        ],
    )
        .into_response()
}

/// tus metadata `key base64,key base64`.
fn tus_metadata(h: Option<String>) -> HashMap<String, String> {
    h.unwrap_or_default()
        .split(',')
        .filter_map(|kv| {
            let mut it = kv.trim().splitn(2, ' ');
            let k = it.next()?.to_string();
            let v = it
                .next()
                .and_then(base64_decode)
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
            (!k.is_empty()).then_some((k, v))
        })
        .collect()
}

/// `POST /files/v1/{vae}`: create an upload (`Upload-Length`, `Upload-Metadata` with
/// `filename` and `filetype`); 201 with its `Location`.
async fn upload_create(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(vae): Path<String>,
) -> Response {
    let (_, tenant, principal) = match file_caller(&s, &headers, &vae, &peer).await {
        Ok(c) => c,
        Err(e) => return file_problem(&e),
    };
    let Some(length) = header(&headers, "upload-length").and_then(|l| l.parse::<u64>().ok()) else {
        return file_problem(&BpaError::new(
            "MODEL.INVALID_VALUE",
            "Upload-Length is the size of the file in bytes",
        ));
    };
    let meta = tus_metadata(header(&headers, "upload-metadata"));
    let max = (s.config().int("files.max_upload_mb").max(1) as u64) << 20;
    let me = s.clone();
    let r = blocking(move || {
        let f = me.files_of(&tenant)?;
        f.create(
            &principal,
            length,
            meta.get("filetype").map(String::as_str),
            meta.get("filename").map(String::as_str),
            max,
        )
    })
    .await;
    match r {
        Ok(u) => {
            let mut resp = (
                StatusCode::CREATED,
                [
                    ("tus-resumable", TUS.to_string()),
                    ("location", format!("/files/v1/{vae}/uploads/{}", u.id)),
                    ("upload-offset", u.received.to_string()),
                ],
                Json(u.json()),
            )
                .into_response();
            if let Some(f) = &u.file_id
                && let Ok(v) = f.parse()
            {
                resp.headers_mut().insert("ubos-file", v);
            }
            resp
        }
        Err(e) => file_problem(&e),
    }
}

/// `HEAD /files/v1/{vae}/uploads/{id}`: the offset to resume from.
async fn upload_head(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, id)): Path<(String, String)>,
) -> Response {
    match upload_get(&s, &headers, &vae, &id, &peer).await {
        Ok(u) => (
            StatusCode::OK,
            [
                ("tus-resumable", TUS.to_string()),
                ("upload-offset", u.received.to_string()),
                ("upload-length", u.length.to_string()),
                ("cache-control", "no-store".to_string()),
            ],
        )
            .into_response(),
        Err(e) => file_problem(&e).status().into_response(),
    }
}

async fn upload_get(
    s: &Shared,
    headers: &HeaderMap,
    vae: &str,
    id: &str,
    peer: &SocketAddr,
) -> Result<crate::files::Upload, BpaError> {
    let (_, tenant, principal) = file_caller(s, headers, vae, peer).await?;
    let (me, id) = (s.clone(), id.to_string());
    blocking(move || me.files_of(&tenant)?.upload(&id, &principal)).await
}

/// `GET /files/v1/{vae}/uploads/{id}`: the upload's state and, once complete, the file
/// reference to put into an object field.
async fn upload_state(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, id)): Path<(String, String)>,
) -> Response {
    let (_, tenant, principal) = match file_caller(&s, &headers, &vae, &peer).await {
        Ok(c) => c,
        Err(e) => return file_problem(&e),
    };
    let me = s.clone();
    let r = blocking(move || {
        let f = me.files_of(&tenant)?;
        let u = f.upload(&id, &principal)?;
        let mut v = u.json();
        if let Some(fid) = &u.file_id {
            let rec = f.file(fid)?;
            v["reference"] = rec.reference(u.name.as_deref());
            v["sha256"] = json!(rec.sha256);
        }
        Ok(v)
    })
    .await;
    match r {
        Ok(v) => ([("tus-resumable", TUS)], Json(v)).into_response(),
        Err(e) => file_problem(&e),
    }
}

/// `PATCH /files/v1/{vae}/uploads/{id}`: bytes at `Upload-Offset`
/// (`application/offset+octet-stream`); 204 with the new offset, and `Ubos-File` when the
/// upload completed.
async fn upload_patch(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    if header(&headers, "content-type").as_deref() != Some("application/offset+octet-stream") {
        return file_problem(&BpaError::new(
            "MODEL.INVALID_VALUE",
            "Content-Type is application/offset+octet-stream",
        ));
    }
    let Some(offset) = header(&headers, "upload-offset").and_then(|o| o.parse::<u64>().ok()) else {
        return file_problem(&BpaError::new(
            "MODEL.INVALID_VALUE",
            "Upload-Offset is required",
        ));
    };
    let (_, tenant, principal) = match file_caller(&s, &headers, &vae, &peer).await {
        Ok(c) => c,
        Err(e) => return file_problem(&e),
    };
    let me = s.clone();
    match blocking(move || me.files_of(&tenant)?.append(&id, &principal, offset, &body)).await {
        Ok(u) => {
            let mut resp = (
                StatusCode::NO_CONTENT,
                [
                    ("tus-resumable", TUS.to_string()),
                    ("upload-offset", u.received.to_string()),
                ],
            )
                .into_response();
            if let Some(f) = &u.file_id
                && let Ok(v) = f.parse()
            {
                resp.headers_mut().insert("ubos-file", v);
            }
            resp
        }
        Err(e) => file_problem(&e),
    }
}

/// `DELETE /files/v1/{vae}/uploads/{id}`: abandon an upload (tus termination).
async fn upload_delete(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((vae, id)): Path<(String, String)>,
) -> Response {
    let (_, tenant, principal) = match file_caller(&s, &headers, &vae, &peer).await {
        Ok(c) => c,
        Err(e) => return file_problem(&e),
    };
    let me = s.clone();
    match blocking(move || me.files_of(&tenant)?.terminate(&id, &principal)).await {
        Ok(()) => (StatusCode::NO_CONTENT, [("tus-resumable", TUS)]).into_response(),
        Err(e) => file_problem(&e),
    }
}

/// `POST /files/v1/{vae}/links` `{object, field}`: a short-lived signed download link for
/// the file an object version's `blob` field refers to (DSN-NOD-327). The object is read with the
/// caller's session, so the caller's authorization applies; the link is bound to the
/// principal and the version read.
async fn file_link(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(vae): Path<String>,
    body: Bytes,
) -> Response {
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let (Some(object), Some(field)) = (req["object"].as_str(), req["field"].as_str()) else {
        return file_problem(&BpaError::new(
            "MODEL.INVALID_VALUE",
            "the body is {object: ubos:// URI, field}",
        ));
    };
    let (key, tenant, principal) = match file_caller(&s, &headers, &vae, &peer).await {
        Ok(c) => c,
        Err(e) => return file_problem(&e),
    };
    // message identifiers are unique within the (shared facade) session
    let mid = format!("link-{}", crate::uuid().unwrap_or_default());
    let msg = json!({"v": ubos_proto::VERSION, "id": mid, "type": "query", "body": {"vae": vae, "read": object}});
    let (_, frames) = s
        .handle(Some(key.clone()), None, &peer.ip().to_string(), msg)
        .await;
    let item = match frames.first() {
        Some(f) if f["type"] == json!("result") => f["body"]["items"][0].clone(),
        Some(f) => {
            return file_problem(&BpaError::with_details(
                f["body"]["code"]
                    .as_str()
                    .unwrap_or("IAM.DENIED")
                    .to_string()
                    .leak(),
                f["body"]["details"].clone(),
            ));
        }
        None => return file_problem(&BpaError::new("IAM.NOT_FOUND", "no object")),
    };
    let reference = item["body"][field].clone();
    let Some(file) = reference["content_id"].as_str().map(str::to_string) else {
        return file_problem(&BpaError::with_details(
            "MODEL.REFERENCE_INVALID",
            json!({"reason": "not_a_file", "field": field}),
        ));
    };
    let class = item["class"].as_str().unwrap_or_default().to_string();
    let classification = match s.session(&key) {
        Some((_, h)) => h
            .call(
                "host.field_classification",
                json!({"class": class, "field": field}),
            )
            .await
            .ok()
            .and_then(|c| c["classification"].as_str().map(str::to_string))
            .unwrap_or_else(|| "internal".into()),
        None => "internal".into(),
    };
    let ttl = s
        .config()
        .int("files.download_link_seconds")
        .clamp(30, 3600);
    let me = s.clone();
    let r = blocking(move || {
        let f = me.files_of(&tenant)?;
        let rec = f.file(&file)?;
        let exp = crate::files::unix_now() + ttl;
        let claims = json!({"t": tenant, "f": rec.id, "p": principal, "o": item["object_id"],
            "v": item["version_hash"], "c": classification, "n": reference["name"],
            "m": rec.media_type, "e": exp});
        let token = f.sign_link(&claims);
        Ok(
            json!({"url": format!("/files/v1/download/{token}"), "expires_in": ttl, "file": rec.id,
            "size": rec.size, "sha256": rec.sha256, "classification": claims["c"]}),
        )
    })
    .await;
    match r {
        Ok(v) => Json(v).into_response(),
        Err(e) => file_problem(&e),
    }
}

/// `GET /files/v1/download/{token}`: the file of a valid link, streamed chunk by chunk,
/// with `Range` support; a download of personal or sensitive personal content is written
/// to the tenant's security log (DSN-NOD-327).
async fn file_download(
    State(s): State<Shared>,
    method: Method,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Response {
    let Some(tenant) = crate::files::link_tenant(&token) else {
        return file_problem(&BpaError::new("IAM.DENIED", "malformed link"));
    };
    let me = s.clone();
    let tok = token.clone();
    let r = blocking(move || {
        let f = me.files_of(&tenant)?;
        let claims = f.verify_link(&tok)?;
        let rec = f.file(claims["f"].as_str().unwrap_or_default())?;
        Ok((f, claims, rec, tenant))
    })
    .await;
    let (files, claims, rec, tenant) = match r {
        Ok(x) => x,
        Err(e) => return file_problem(&e),
    };
    // Range: bytes=a-b | a- | -n
    let (start, end, partial) = match header(&headers, "range") {
        None => (0, rec.size, false),
        Some(r) => {
            let spec = r.strip_prefix("bytes=").unwrap_or("");
            let (a, b) = spec.split_once('-').unwrap_or(("", ""));
            let (start, end) = match (a.parse::<u64>().ok(), b.parse::<u64>().ok()) {
                (Some(a), Some(b)) => (a, b.saturating_add(1).min(rec.size)),
                (Some(a), None) => (a, rec.size),
                (None, Some(n)) => (rec.size.saturating_sub(n), rec.size),
                _ => (u64::MAX, 0),
            };
            if start >= end || start >= rec.size {
                return (
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    [("content-range", format!("bytes */{}", rec.size))],
                )
                    .into_response();
            }
            (start, end, true)
        }
    };
    let personal = matches!(
        claims["c"].as_str(),
        Some("personal") | Some("sensitive_personal")
    );
    if personal && method == Method::GET {
        let me = s.clone();
        let rec_log = json!({"principal": claims["p"], "object": claims["o"], "version": claims["v"],
            "file": rec.id, "classification": claims["c"], "range": [start, end]});
        let _ = blocking(move || me.identity(&tenant)?.log("file.download", rec_log)).await;
    }
    let mut hdrs = vec![
        ("accept-ranges", "bytes".to_string()),
        ("content-length", (end - start).to_string()),
        (
            "content-type",
            rec.media_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into()),
        ),
        ("etag", format!("\"{}\"", rec.id)),
        ("ubos-content-sha256", rec.sha256.clone()),
        ("cache-control", "private, no-store".to_string()),
    ];
    if let Some(n) = claims["n"].as_str() {
        let safe: String = n
            .chars()
            .filter(|c| c.is_ascii_graphic() && *c != '"' && *c != '\\' || *c == ' ')
            .collect();
        hdrs.push((
            "content-disposition",
            format!("attachment; filename=\"{safe}\""),
        ));
    }
    if partial {
        hdrs.push((
            "content-range",
            format!("bytes {start}-{}/{}", end - 1, rec.size),
        ));
    }
    let status = if partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let mut builder = Response::builder().status(status);
    for (k, v) in hdrs {
        builder = builder.header(k, v);
    }
    if method == Method::HEAD {
        return builder
            .body(axum::body::Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    tokio::task::spawn_blocking(move || {
        let r = files.read_range(&rec, start, end, |b| {
            tx.blocking_send(Ok(Bytes::from(b))).is_ok()
        });
        if let Err(e) = r {
            let _ = tx.blocking_send(Err(std::io::Error::other(e.to_string())));
        }
    });
    let stream =
        futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|b| (b, rx)) });
    builder
        .body(axum::body::Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
