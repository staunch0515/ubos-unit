//! The host services on PostgreSQL (CTR-002): blob service, connectors with an allow-list
//! and the tenant's secrets, the event log that receives released effects, jobs with leases
//! and retries, telemetry without business data.

use crate::db::{Db, Param};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};
use ubos_dvm::host::{Connectors, Jobs, OutboxSink, Secrets, Telemetry};
use ubos_dvm::store::physical::BlobStore;
use ubos_dvm::types::BpaError;

// ---- blob service (DSN-NOD-320, -321) ----

/// Blobs in the `blob` table of the tenant (content-addressed paths deduplicate, -321).
pub struct PgBlobs {
    db: Arc<Db>,
    tenant: String,
}

impl PgBlobs {
    /// The blob service of a tenant.
    pub fn new(db: Arc<Db>, tenant: &str) -> PgBlobs {
        PgBlobs {
            db,
            tenant: tenant.to_string(),
        }
    }

    /// Number of blobs of the tenant.
    pub fn count(&self) -> Result<u64, BpaError> {
        let rows = self.db.query(
            "SELECT count(*)::bigint AS n FROM blob WHERE tenant_id = $1::uuid",
            &[Param::Text(&self.tenant)],
        )?;
        Ok(rows.first().and_then(|r| r["n"].as_u64()).unwrap_or(0))
    }
}

impl BlobStore for PgBlobs {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO blob (tenant_id, path, bytes) VALUES ($1::uuid, $2, $3) ON CONFLICT DO NOTHING",
            &[Param::Text(&self.tenant), Param::Text(path), Param::Bytes(bytes)],
        )?;
        Ok(())
    }
    fn get(&self, path: &str) -> Result<Vec<u8>, BpaError> {
        let rows = self.db.query(
            "SELECT bytes FROM blob WHERE tenant_id = $1::uuid AND path = $2",
            &[Param::Text(&self.tenant), Param::Text(path)],
        )?;
        let Some(r) = rows.first() else {
            let mut e = BpaError::new("NOD.BLOB_UNAVAILABLE", format!("blob {path} missing"));
            e.details = Some(json!({"integrity": false}));
            return Err(e);
        };
        let hex = r["bytes"].as_str().unwrap_or_default();
        Ok((0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap_or_default())
            .collect())
    }
}

// ---- connectors (DSN-NOD-301, -302) ----

/// A connector implemented in the node: `(operation, request, secrets) -> response`.
pub type Connector =
    Arc<dyn Fn(&str, &Value, &dyn Secrets) -> Result<Value, BpaError> + Send + Sync>;

/// The registered connectors: the allow-list of the node (DSN-NOD-302). An unregistered
/// connector is `LOGIC.CONNECTOR_UNAVAILABLE`.
#[derive(Default)]
pub struct ConnectorRegistry {
    connectors: RwLock<BTreeMap<String, Connector>>,
    /// Calls made, for suites: `(connector, operation)`.
    pub calls: Mutex<Vec<(String, String)>>,
}

impl ConnectorRegistry {
    /// Register a connector under its name (`ubos://…` URIs are matched on the whole text).
    pub fn register(&self, name: &str, connector: Connector) {
        if let Ok(mut g) = self.connectors.write() {
            g.insert(name.to_string(), connector);
        }
    }

    /// Names of the registered connectors.
    pub fn names(&self) -> Vec<String> {
        self.connectors
            .read()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// The connectors of a node bound to one tenant's secrets.
pub struct BoundConnectors {
    /// The node's registry.
    pub registry: Arc<ConnectorRegistry>,
    /// The tenant's secrets, resolved only here (never returned to logic).
    pub secrets: Arc<dyn Secrets>,
}

impl Connectors for BoundConnectors {
    fn call(&self, connector: &str, operation: &str, request: &Value) -> Result<Value, BpaError> {
        let c = self
            .registry
            .connectors
            .read()
            .ok()
            .and_then(|g| g.get(connector).cloned())
            .ok_or_else(|| {
                BpaError::new(
                    "LOGIC.CONNECTOR_UNAVAILABLE",
                    format!("connector {connector} is not registered"),
                )
            })?;
        if let Ok(mut g) = self.registry.calls.lock() {
            g.push((connector.to_string(), operation.to_string()));
        }
        c(operation, request, self.secrets.as_ref()).map_err(|e| {
            if e.code == "LOGIC.CONNECTOR_UNAVAILABLE" || e.code == "LOGIC.CONNECTOR_ERROR" {
                e
            } else {
                let mut out = BpaError::new("LOGIC.CONNECTOR_ERROR", e.message);
                out.details = Some(json!({"code": e.code}));
                out
            }
        })
    }
}

// ---- event log (DSN-NOD-201, -202) ----

/// Released effects of a VAE in the `event_log` table.
pub struct EventLog {
    db: Arc<Db>,
    tenant: String,
    vae: String,
}

impl EventLog {
    /// The event log of a VAE.
    pub fn new(db: Arc<Db>, tenant: &str, vae: &str) -> EventLog {
        EventLog {
            db,
            tenant: tenant.to_string(),
            vae: vae.to_string(),
        }
    }

    /// Events after `after` (sequence number), in order.
    pub fn events(&self, after: i64, limit: i64) -> Result<Vec<Value>, BpaError> {
        self.db.query(
            "SELECT seq, effect_id, type, branch, cseq, object_id, payload FROM event_log WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND seq > $3 ORDER BY seq LIMIT $4",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Int(after), Param::Int(limit)],
        )
    }
}

impl OutboxSink for EventLog {
    fn release(&self, effects: &[Value]) -> Result<(), BpaError> {
        for e in effects {
            let text = |k: &str| {
                e.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let cseq = e.get("cseq").and_then(Value::as_i64).unwrap_or(0);
            let object = e
                .get("object_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let payload = e.get("payload").cloned().unwrap_or(Value::Null);
            let mut params = vec![Param::Text(&self.tenant), Param::Text(&self.vae)];
            let (effect_id, kind, branch) = (text("effect_id"), text("type"), text("branch"));
            params.push(Param::Text(&effect_id));
            params.push(Param::Text(&kind));
            params.push(Param::Text(&branch));
            params.push(Param::Int(cseq));
            let object_text = object.unwrap_or_default();
            params.push(Param::Text(&object_text));
            params.push(Param::Json(&payload));
            self.db.execute(
                "INSERT INTO event_log (tenant_id, vae_id, effect_id, type, branch, cseq, object_id, payload) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, nullif($7, ''), $8) ON CONFLICT (tenant_id, vae_id, effect_id) DO NOTHING",
                &params,
            )?;
        }
        Ok(())
    }
}

// ---- jobs (DSN-NOD-220, -221, -223) ----

/// Jobs of a VAE in the `job` table: enqueue, claim under a lease, complete or fail with
/// retries until `max_attempts`, after which the job is poison.
pub struct JobQueue {
    db: Arc<Db>,
    tenant: String,
    vae: String,
}

impl JobQueue {
    /// The job queue of a VAE.
    pub fn new(db: Arc<Db>, tenant: &str, vae: &str) -> JobQueue {
        JobQueue {
            db,
            tenant: tenant.to_string(),
            vae: vae.to_string(),
        }
    }

    /// Claim the next queued job (or one whose lease expired) for `worker` with a lease of
    /// `lease_seconds`; `None` when nothing is due.
    pub fn claim(&self, worker: &str, lease_seconds: i64) -> Result<Option<Value>, BpaError> {
        let rows = self.db.query(
            "UPDATE job SET state = 'running', worker = $3, attempts = attempts + 1, \
             lease_until = now() + make_interval(secs => $4), updated_at = now() \
             WHERE (tenant_id, job_id) = (SELECT tenant_id, job_id FROM job \
               WHERE tenant_id = $1::uuid AND vae_id = $2::uuid \
                 AND (state = 'queued' OR (state = 'running' AND lease_until < now())) \
               ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) \
             RETURNING job_id::text AS job_id, kind, payload, attempts, max_attempts",
            &[
                Param::Text(&self.tenant),
                Param::Text(&self.vae),
                Param::Text(worker),
                Param::Int(lease_seconds),
            ],
        )?;
        Ok(rows.into_iter().next())
    }

    /// Complete a job with its result.
    pub fn complete(&self, job_id: &str, result: &Value) -> Result<(), BpaError> {
        self.db.execute(
            "UPDATE job SET state = 'done', result = $3, lease_until = NULL, updated_at = now() WHERE tenant_id = $1::uuid AND job_id = $2::uuid",
            &[Param::Text(&self.tenant), Param::Text(job_id), Param::Json(result)],
        )?;
        Ok(())
    }

    /// Fail an attempt: the job is queued again until `max_attempts`, then poison
    /// (DSN-NOD-223). Returns the new state.
    pub fn fail(&self, job_id: &str, error: &str) -> Result<String, BpaError> {
        let rows = self.db.query(
            "UPDATE job SET state = CASE WHEN attempts >= max_attempts THEN 'poison' ELSE 'queued' END, \
             error = $3, lease_until = NULL, updated_at = now() \
             WHERE tenant_id = $1::uuid AND job_id = $2::uuid RETURNING state",
            &[Param::Text(&self.tenant), Param::Text(job_id), Param::Text(error)],
        )?;
        Ok(rows
            .first()
            .and_then(|r| r["state"].as_str())
            .unwrap_or_default()
            .to_string())
    }

    /// A job by identifier.
    pub fn job(&self, job_id: &str) -> Result<Option<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT job_id::text AS job_id, kind, payload, state, attempts, max_attempts, worker, result, error FROM job WHERE tenant_id = $1::uuid AND job_id = $2::uuid",
            &[Param::Text(&self.tenant), Param::Text(job_id)],
        )?;
        Ok(rows.into_iter().next())
    }
}

impl Jobs for JobQueue {
    fn enqueue(&self, job: &Value) -> Result<String, BpaError> {
        let id = crate::uuid()?;
        let kind = job
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("job")
            .to_string();
        let payload = job.get("payload").cloned().unwrap_or(json!({}));
        let max = job.get("max_attempts").and_then(Value::as_i64).unwrap_or(3);
        self.db.execute(
            "INSERT INTO job (tenant_id, job_id, vae_id, kind, payload, max_attempts) VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6)",
            &[Param::Text(&self.tenant), Param::Text(&id), Param::Text(&self.vae), Param::Text(&kind), Param::Json(&payload), Param::Int(max)],
        )?;
        Ok(id)
    }
}

// ---- telemetry (DSN-NOD-610, -611) ----

/// Members a telemetry event may carry: names, identifiers, codes and measures, never
/// business data (DSN-NOD-611). Other members are dropped before the event is kept.
pub const TELEMETRY_FIELDS: &[&str] = &[
    "name",
    "op",
    "code",
    "duration_us",
    "tenant",
    "vae",
    "process_id",
    "branch",
    "count",
    "bytes",
    "units",
    "worker",
    "kind",
];

/// Telemetry kept in a bounded ring (and written to standard error when `UBOS_TELEMETRY`
/// is `stderr`); OpenTelemetry export follows with the Server of gate 1b.
#[derive(Default)]
pub struct LogTelemetry {
    /// The last events, redacted.
    pub events: Mutex<std::collections::VecDeque<Value>>,
}

impl LogTelemetry {
    /// Redact an event to `TELEMETRY_FIELDS`.
    pub fn redact(event: &Value) -> Value {
        let mut out = serde_json::Map::new();
        for (k, v) in event.as_object().into_iter().flatten() {
            if TELEMETRY_FIELDS.contains(&k.as_str()) && !v.is_object() && !v.is_array() {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out)
    }

    /// Emit (the trait method, for callers holding the concrete type).
    pub fn emit_now(&self, event: &Value) {
        self.emit(event);
    }
}

impl Telemetry for LogTelemetry {
    fn emit(&self, event: &Value) {
        let e = LogTelemetry::redact(event);
        if std::env::var("UBOS_TELEMETRY").is_ok_and(|v| v == "stderr") {
            eprintln!("telemetry {e}");
        }
        if let Ok(mut g) = self.events.lock() {
            if g.len() >= 10_000 {
                g.pop_front();
            }
            g.push_back(e);
        }
    }
}
