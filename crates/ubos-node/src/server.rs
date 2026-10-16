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

use crate::identity::{Authenticated, Identity};
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

struct Call {
    op: String,
    input: Value,
    reply: tokio::sync::oneshot::Sender<Result<Value, BpaError>>,
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
                    let r = vae.call(&call.op, &call.input);
                    let _ = call.reply.send(r);
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
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Call {
                op: op.to_string(),
                input,
                reply,
            })
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?;
        rx.await
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?
    }

    /// Run one operation from a thread outside the runtime (suites and setup).
    pub fn call_blocking(&self, op: &str, input: Value) -> Result<Value, BpaError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Call {
                op: op.to_string(),
                input,
                reply,
            })
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?;
        rx.blocking_recv()
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?
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
}

/// One VAE already open (suites, the memory preset): every tenant and authority maps to
/// it, and one memory identity serves every tenant name.
pub struct SingleVae {
    /// The VAE.
    pub handle: VaeHandle,
    /// Its identity (memory).
    pub identity: Arc<Identity>,
}

impl VaeSource for SingleVae {
    fn open(&self, _tenant: &str, _authority: &str) -> Result<Option<VaeHandle>, BpaError> {
        Ok(Some(self.handle.clone()))
    }
    fn identity(&self, _tenant: &str) -> Result<Option<Identity>, BpaError> {
        // the Server's fallback identity (`Server::single`) serves every tenant name
        Ok(None)
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
    vae: String,
    tenant: String,
    handle: VaeHandle,
    push: Option<tokio::sync::mpsc::Sender<Value>>,
    /// Frames the Server produced itself (errors before the binding), numbered apart.
    own: u64,
    auth: Option<Auth>,
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
        Arc::new(Server {
            source: Box::new(SingleVae {
                handle,
                identity: identity.clone(),
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
        })
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
            .route("/ubtp/v1", get(ws_upgrade).post(http_messages))
            .route("/ubtp/v1/stream", get(sse_stream))
            .route("/api/v1/{vae}/{class}", get(rest_query))
            .route("/api/v1/{vae}/{class}/{slug}", get(rest_read))
            .route("/api/v1/{vae}/{class}/{slug}/{port}", post(rest_mutate))
            .route("/api/v1/identity/{*rest}", axum::routing::any(identity_api))
            .layer(DefaultBodyLimit::max(MAX_FRAME))
            .with_state(self.clone())
    }

    /// Serve on `listen` until the task is cancelled.
    pub async fn serve(self: Shared, listen: std::net::SocketAddr) -> std::io::Result<()> {
        let listener = tokio::net::TcpListener::bind(listen).await?;
        let poll = self.clone();
        tokio::spawn(async move { poll.poll_loop().await });
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
        self.vaes.lock().expect("vaes").insert(key, h.clone());
        Ok(h)
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
        let out = handle
            .call("ubtp", json!({"session": key, "messages": [message]}))
            .await;
        let mut frames = match out {
            Ok(v) => v["responses"].as_array().cloned().unwrap_or_default(),
            Err(e) => vec![self.own_error(&key, &e, rid.as_deref())],
        };
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

async fn ws_upgrade(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let tenant = header(&headers, "ubtp-tenant");
    let source = peer.ip().to_string();
    ws.protocols([SUBPROTOCOL])
        .max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| ws_session(s, tenant, source, socket))
}

async fn ws_session(s: Shared, tenant: Option<String>, source: String, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (push_tx, mut push_rx) = tokio::sync::mpsc::channel::<Value>(QUEUE);
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
                    s.attach_push(&key, push_tx.clone());
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
    for m in messages {
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
    use ubos_dvm::host::{NoConnectors, NoModels, OsEntropy, Recorder, Services, SystemClock};
    let mut engine = ubos_dvm::Engine::new(Box::new(ubos_dvm::store::MemStore::default()))?;
    if let Some(c) = classes {
        engine.load_genesis(c, None)?;
    }
    let rec = Arc::new(Recorder::default());
    let services = Services {
        clock: Arc::new(SystemClock),
        entropy: Arc::new(OsEntropy),
        connectors: Arc::new(NoConnectors),
        models: Arc::new(NoModels),
        outbox: rec.clone(),
        jobs: rec.clone(),
        telemetry: rec,
    };
    Ok(Vae::open(engine, services))
}

/// Unused-import guard for `Map` in builds without the facade helpers.
#[allow(dead_code)]
fn _map_type(_: &Map<String, Value>) {}
