//! The host boundary of the DVM (PH-1 step 7a, DEC-060): the host services the kernel must
//! not own (CTR-002) and the host API a host calls (CTR-001).
//!
//! * [`Services`] bundles the CTR-002 interfaces: `clock.now`, `entropy.fill`, `blob.put/get`
//!   ([`BlobStore`]), the key service ([`KeyService`]), `secret.resolve` (used only inside a
//!   connector), `connector.call`, `model.ask`, `outbox.release`, `jobs.enqueue` and
//!   `telemetry.emit`. Every non-deterministic result crosses into the kernel through a
//!   journaled instruction or the recorded entropy seed, so a process replays without the
//!   host (STD-CTX-011, -012).
//! * [`Vae`] is the in-process host API over one VAE: it owns the engine, reads the clock
//!   before every operation, seeds every process from the host's entropy and releases the
//!   outbox to the host's event log. Forge's test host and the node host of `ubos-node`
//!   provide the services.

use crate::adapter::Adapter;
use crate::stateful::Engine;
use serde_json::{Value, json};
use std::sync::Arc;
pub use ubos_crypto::KeyService;
pub use ubos_store::physical::BlobStore;
use ubos_types::BpaError;

/// `clock.now`: the physical clock as a canonical instant.
pub trait Clock: Send + Sync {
    /// The current instant, canonical text (STD-TIME-001).
    fn now(&self) -> String;
}

/// `entropy.fill`: random bytes for process seeds.
pub trait Entropy: Send + Sync {
    /// Fill `buf` with random bytes.
    fn fill(&self, buf: &mut [u8]) -> Result<(), BpaError>;
}

/// `secret.resolve`: a secret by handle, for connector execution only (DSN-NOD-130). The
/// bytes never reach logic or the journal.
pub trait Secrets: Send + Sync {
    /// The secret behind a handle, or `IAM.NOT_FOUND`.
    fn resolve(&self, handle: &str) -> Result<Vec<u8>, BpaError>;
}

/// `connector.call`: a call of a registered connector (DSN-NOD-301). The result is
/// journaled by the kernel before use.
pub trait Connectors: Send + Sync {
    /// Call `operation` of `connector` with `request`; `LOGIC.CONNECTOR_UNAVAILABLE` for an
    /// unknown connector, `LOGIC.CONNECTOR_ERROR` for a failed call.
    fn call(&self, connector: &str, operation: &str, request: &Value) -> Result<Value, BpaError>;
}

/// `model.ask`: a model call; journaled like a connector.
pub trait Models: Send + Sync {
    /// Ask a model; `AI.PROVIDER_UNAVAILABLE` when no provider is configured.
    fn ask(&self, request: &Value) -> Result<Value, BpaError>;
}

/// `outbox.release`: effects released after their commit (DSN-NOD-201).
pub trait OutboxSink: Send + Sync {
    /// Deliver released effects (`{effect_id, type, branch, cseq, object_id?, payload?}`);
    /// a delivered effect is then acknowledged in the store.
    fn release(&self, effects: &[Value]) -> Result<(), BpaError>;
}

/// `jobs.enqueue`: background work (DSN-NOD-220).
pub trait Jobs: Send + Sync {
    /// Enqueue a job `{kind, payload, max_attempts?}`; returns its identifier.
    fn enqueue(&self, job: &Value) -> Result<String, BpaError>;
}

/// `telemetry.emit`: operational events without business data (DSN-NOD-610, -611).
pub trait Telemetry: Send + Sync {
    /// Emit an event `{name, …}`.
    fn emit(&self, event: &Value);
}

/// The CTR-002 services of a host.
#[derive(Clone)]
pub struct Services {
    /// `clock.now`.
    pub clock: Arc<dyn Clock>,
    /// `entropy.fill`.
    pub entropy: Arc<dyn Entropy>,
    /// `connector.call`.
    pub connectors: Arc<dyn Connectors>,
    /// `model.ask`.
    pub models: Arc<dyn Models>,
    /// `outbox.release`.
    pub outbox: Arc<dyn OutboxSink>,
    /// `jobs.enqueue`.
    pub jobs: Arc<dyn Jobs>,
    /// `telemetry.emit`.
    pub telemetry: Arc<dyn Telemetry>,
}

// ---- deterministic defaults (tests, the conformance adapter when a host is absent) ----

/// A clock that reads a fixed instant until set.
pub struct FixedClock(std::sync::Mutex<String>);

impl FixedClock {
    /// A clock at `now`.
    pub fn new(now: &str) -> FixedClock {
        FixedClock(std::sync::Mutex::new(now.to_string()))
    }
    /// Set the clock.
    pub fn set(&self, now: &str) {
        if let Ok(mut g) = self.0.lock() {
            *g = now.to_string();
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> String {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Seeded entropy: a deterministic stream from one seed (test hosts, DSN-FRG-202).
pub struct SeededEntropy(std::sync::Mutex<([u8; 32], u64)>);

impl SeededEntropy {
    /// A stream from `seed`.
    pub fn new(seed: [u8; 32]) -> SeededEntropy {
        SeededEntropy(std::sync::Mutex::new((seed, 0)))
    }
}

impl Entropy for SeededEntropy {
    fn fill(&self, buf: &mut [u8]) -> Result<(), BpaError> {
        use sha2::{Digest, Sha256};
        let mut g = self
            .0
            .lock()
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "entropy poisoned"))?;
        for chunk in buf.chunks_mut(32) {
            let mut h = Sha256::new();
            h.update(b"ubos.seeded-entropy.v1\0");
            h.update(g.0);
            h.update(g.1.to_be_bytes());
            g.1 += 1;
            let d = h.finalize();
            chunk.copy_from_slice(&d[..chunk.len()]);
        }
        Ok(())
    }
}

/// Entropy from the operating system.
pub struct OsEntropy;

impl Entropy for OsEntropy {
    fn fill(&self, buf: &mut [u8]) -> Result<(), BpaError> {
        getrandom::getrandom(buf)
            .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))
    }
}

/// The system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        ubos_types::time::format_instant(&chrono::DateTime::<chrono::Utc>::from(
            std::time::SystemTime::now(),
        ))
    }
}

/// No connectors: every call is `LOGIC.CONNECTOR_UNAVAILABLE`.
pub struct NoConnectors;

impl Connectors for NoConnectors {
    fn call(&self, connector: &str, _operation: &str, _request: &Value) -> Result<Value, BpaError> {
        Err(BpaError::new(
            "LOGIC.CONNECTOR_UNAVAILABLE",
            format!("no connector {connector} is registered"),
        ))
    }
}

/// No model provider: every call is `AI.PROVIDER_UNAVAILABLE`.
pub struct NoModels;

impl Models for NoModels {
    fn ask(&self, _request: &Value) -> Result<Value, BpaError> {
        Err(BpaError::new(
            "AI.PROVIDER_UNAVAILABLE",
            "no model provider is configured",
        ))
    }
}

/// Released effects, jobs and telemetry kept in memory (test hosts).
#[derive(Default)]
pub struct Recorder {
    /// Released effects in order.
    pub effects: std::sync::Mutex<Vec<Value>>,
    /// Enqueued jobs in order.
    pub jobs: std::sync::Mutex<Vec<Value>>,
    /// Emitted telemetry events in order.
    pub events: std::sync::Mutex<Vec<Value>>,
}

impl OutboxSink for Recorder {
    fn release(&self, effects: &[Value]) -> Result<(), BpaError> {
        if let Ok(mut g) = self.effects.lock() {
            g.extend(effects.iter().cloned());
        }
        Ok(())
    }
}

impl Jobs for Recorder {
    fn enqueue(&self, job: &Value) -> Result<String, BpaError> {
        let id = format!("job-{}", self.jobs.lock().map(|g| g.len() + 1).unwrap_or(0));
        if let Ok(mut g) = self.jobs.lock() {
            let mut j = job.clone();
            j["job_id"] = json!(id);
            g.push(j);
        }
        Ok(id)
    }
}

impl Telemetry for Recorder {
    fn emit(&self, event: &Value) {
        if let Ok(mut g) = self.events.lock() {
            g.push(event.clone());
        }
    }
}

impl Services {
    /// Deterministic services for tests: a fixed clock, seeded entropy, no connectors or
    /// models, and a recorder for effects, jobs and telemetry (DSN-FRG-202).
    pub fn deterministic(now: &str, seed: [u8; 32]) -> (Services, Arc<Recorder>) {
        let rec = Arc::new(Recorder::default());
        let s = Services {
            clock: Arc::new(FixedClock::new(now)),
            entropy: Arc::new(SeededEntropy::new(seed)),
            connectors: Arc::new(NoConnectors),
            models: Arc::new(NoModels),
            outbox: rec.clone(),
            jobs: rec.clone(),
            telemetry: rec.clone(),
        };
        (s, rec)
    }
}

// ---- the host API (CTR-001) ----

/// The execution context a host supplies (AR-007): identity parameters never come from
/// arguments (STD-CTX-002).
#[derive(Debug, Clone)]
pub struct Context {
    /// Principal DID.
    pub principal: String,
    /// Principal acted for, if any.
    pub on_behalf_of: Option<String>,
}

/// One open VAE: the engine behind the host API, with the host's services.
pub struct Vae {
    adapter: Adapter,
    services: Services,
}

impl Vae {
    /// Open a VAE over an engine with the given services; the engine reads the clock and
    /// seeds processes from the entropy of the services from now on.
    pub fn open(mut engine: Engine, services: Services) -> Vae {
        engine.services = Some(services.clone());
        Vae {
            adapter: Adapter::from_engine(engine),
            services,
        }
    }

    /// The services.
    pub fn services(&self) -> &Services {
        &self.services
    }

    /// Replace the services (a test host swapping its clock or connectors).
    pub fn set_services(&mut self, services: Services) {
        self.adapter.engine_mut().services = Some(services.clone());
        self.services = services;
    }

    /// The engine (verifiers and suites).
    pub fn engine_mut(&mut self) -> &mut Engine {
        self.adapter.engine_mut()
    }

    /// Read the clock, fire due timers, run one operation and release the outbox.
    pub fn call(&mut self, op: &str, input: &Value) -> Result<Value, BpaError> {
        let now = self.services.clock.now();
        let e = self.adapter.engine_mut();
        if e.now != now {
            e.now = now;
            e.tick()?;
        }
        let started = std::time::Instant::now();
        let out = self.adapter.execute(op, input);
        self.services.telemetry.emit(&json!({
            "name": "dvm.operation", "op": op,
            "duration_us": started.elapsed().as_micros() as u64,
            "code": out.as_ref().err().map(|e| e.code),
        }));
        let out = out?;
        self.release_outbox()?;
        Ok(out)
    }

    /// Release the pending effects to the host (`outbox.release`) and acknowledge them.
    pub fn release_outbox(&mut self) -> Result<usize, BpaError> {
        let pending = self.adapter.engine_mut().outbox()?;
        let effects: Vec<Value> = pending["effects"].as_array().cloned().unwrap_or_default();
        if effects.is_empty() {
            return Ok(0);
        }
        self.services.outbox.release(&effects)?;
        let ids: Vec<Value> = effects.iter().map(|e| e["effect_id"].clone()).collect();
        self.adapter.engine_mut().ack(&Value::Array(ids))?;
        Ok(effects.len())
    }

    fn with_ctx(ctx: &Context, mut input: Value) -> Value {
        input["principal"] = json!(ctx.principal);
        if let Some(o) = &ctx.on_behalf_of {
            input["on_behalf_of"] = json!(o);
        }
        input
    }

    /// `begin_process(ctx)`: a recorded process for the commits and invocations that follow
    /// (`{process_id, kind}`).
    pub fn begin_process(&mut self, ctx: &Context, kind: &str) -> Result<Value, BpaError> {
        let e = self.adapter.engine_mut();
        let mut next = e.next_process;
        let p = crate::versioning::process(
            e.store.as_mut(),
            Some(&json!({"kind": kind, "author": ctx.principal})),
            kind,
            &mut next,
            &ctx.principal,
        )?;
        e.next_process = next;
        // recorded now, so that a reopened VAE continues after it (STD-TXN-010)
        let mut rec = p.p.clone();
        rec["process_id"] = json!(p.id);
        rec["state"] = json!("open");
        e.store.put_process_rec(&p.id, &rec)?;
        Ok(json!({"process_id": p.id, "kind": kind}))
    }

    /// `commit`.
    pub fn commit(&mut self, ctx: &Context, input: Value) -> Result<Value, BpaError> {
        self.call("commit", &Self::with_ctx(ctx, input))
    }

    /// `read` (an object) or `read_ledger` when the input names a ledger key.
    pub fn read(&mut self, ctx: &Context, input: Value) -> Result<Value, BpaError> {
        let op = if input.get("ledger_key").is_some() {
            "read_ledger"
        } else {
            "read"
        };
        self.call(op, &Self::with_ctx(ctx, input))
    }

    /// `query` (STD-EXPR-011).
    pub fn query(&mut self, ctx: &Context, input: Value) -> Result<Value, BpaError> {
        self.call("query", &Self::with_ctx(ctx, input))
    }

    /// `invoke_port`: execute a port or a logic function as a process (STD-CTX-001).
    pub fn invoke_port(&mut self, ctx: &Context, input: Value) -> Result<Value, BpaError> {
        self.call("execute", &Self::with_ctx(ctx, input))
    }

    /// `merge`.
    pub fn merge(&mut self, ctx: &Context, input: Value) -> Result<Value, BpaError> {
        self.call("merge", &Self::with_ctx(ctx, input))
    }

    /// `preview`: a merge preview.
    pub fn preview(&mut self, ctx: &Context, input: Value) -> Result<Value, BpaError> {
        self.call("merge_preview", &Self::with_ctx(ctx, input))
    }

    /// `replay` of a recorded process (STD-CTX-012).
    pub fn replay(&mut self, process_id: &str) -> Result<Value, BpaError> {
        self.call("replay", &json!({"process_id": process_id}))
    }

    /// `export`: the portable archive (STD-PROOF-070).
    pub fn export(&mut self) -> Result<Value, BpaError> {
        self.call("export_archive", &json!({}))
    }

    /// `import` of a portable archive into this empty VAE.
    pub fn import(&mut self, archive: &Value) -> Result<Value, BpaError> {
        self.call("import_archive", &json!({"archive": archive}))
    }

    /// `verify`: the verification gate of a logic asset (STD-CTX-023).
    pub fn verify(&mut self, input: Value) -> Result<Value, BpaError> {
        self.call("verify_asset", &input)
    }

    /// `explain`: the history of an object and, when a process is named, its lineage.
    pub fn explain(&mut self, input: Value) -> Result<Value, BpaError> {
        let mut out = json!({});
        if input.get("object_id").is_some() {
            out["history"] = self.call("history", &input)?;
        }
        if let Some(p) = input.get("process_id") {
            out["process"] = self.call("process_info", &json!({"process_id": p}))?;
            out["lineage"] = self.call("lineage", &json!({"process_id": p}))?;
        }
        Ok(out)
    }

    /// `reflect`: the effective class, or the declarations when none is named.
    pub fn reflect(&mut self, input: Value) -> Result<Value, BpaError> {
        match input.get("class").and_then(Value::as_str) {
            Some(class) => self.call("class_at", &input).or_else(|_| {
                let e = self.adapter.engine_mut();
                e.registry.class(class).map(|c| c.to_json())
            }),
            None => Ok(json!({"classes": self.adapter.engine_mut().registry.declarations()})),
        }
    }
}
