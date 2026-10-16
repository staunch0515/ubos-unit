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

/// What an event-log query selects (DSN-NOD-202): every member optional.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Event type.
    pub type_: Option<String>,
    /// Object identifier.
    pub object_id: Option<String>,
    /// Released at or after this instant.
    pub from: Option<String>,
    /// Released before this instant.
    pub to: Option<String>,
    /// Sequence numbers after this one (paging).
    pub after: i64,
    /// At most this many (1…1,000; 100 by default).
    pub limit: i64,
}

impl EventFilter {
    fn limit(&self) -> i64 {
        if self.limit <= 0 {
            100
        } else {
            self.limit.min(1_000)
        }
    }
}

/// The event log of a VAE: the sink of the releaser, queries and the retention sweep.
pub trait EventStore: OutboxSink {
    /// Events matching a filter, by sequence number.
    fn query(&self, f: &EventFilter) -> Result<Vec<Value>, BpaError>;
    /// The retention rules `(type, days)` of the tenant (`*` for the other types).
    fn retention(&self) -> Result<Vec<(String, i64)>, BpaError>;
    /// Set (or with `None` remove) the retention of an event type.
    fn set_retention(&self, type_: &str, days: Option<i64>) -> Result<(), BpaError>;
    /// Remove the events older than their type's retention at `now` (canonical instant);
    /// returns how many were removed (FR-EVT-015, DSN-NOD-202 acceptance 1).
    fn sweep(&self, now: &str) -> Result<u64, BpaError>;
}

fn check_retention(type_: &str, days: Option<i64>) -> Result<(), BpaError> {
    if type_.is_empty() || type_.len() > 200 {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "an event type is 1 to 200 characters",
        ));
    }
    if days.is_some_and(|d| d <= 0 || d > 36_500) {
        return Err(BpaError::new("MODEL.INVALID_VALUE", "days is 1 to 36,500"));
    }
    Ok(())
}

/// The instant `days` before `now`.
fn cutoff(now: &str, days: i64) -> Result<String, BpaError> {
    let t = ubos_dvm::types::time::parse_instant(now)?;
    Ok(ubos_dvm::types::time::format_instant(
        &(t - chrono::Duration::days(days)),
    ))
}

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

const RELEASED_AT: &str =
    "to_char(released_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS released_at";

impl EventStore for EventLog {
    fn query(&self, f: &EventFilter) -> Result<Vec<Value>, BpaError> {
        let (t, o) = (
            f.type_.clone().unwrap_or_default(),
            f.object_id.clone().unwrap_or_default(),
        );
        let (from, to) = (
            f.from.clone().unwrap_or_default(),
            f.to.clone().unwrap_or_default(),
        );
        self.db.query(
            &format!("SELECT seq, effect_id, type, branch, cseq, object_id, payload, {RELEASED_AT} FROM event_log \
              WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND seq > $3 \
                AND ($4 = '' OR type = $4) AND ($5 = '' OR object_id = $5) \
                AND ($6 = '' OR released_at >= nullif($6, '')::timestamptz) \
                AND ($7 = '' OR released_at < nullif($7, '')::timestamptz) \
              ORDER BY seq LIMIT $8"),
            &[
                Param::Text(&self.tenant),
                Param::Text(&self.vae),
                Param::Int(f.after),
                Param::Text(&t),
                Param::Text(&o),
                Param::Text(&from),
                Param::Text(&to),
                Param::Int(f.limit()),
            ],
        )
    }
    fn retention(&self) -> Result<Vec<(String, i64)>, BpaError> {
        Ok(self
            .db
            .query(
                "SELECT type, days FROM event_retention WHERE tenant_id = $1::uuid ORDER BY type",
                &[Param::Text(&self.tenant)],
            )?
            .iter()
            .map(|r| {
                (
                    r["type"].as_str().unwrap_or_default().to_string(),
                    r["days"].as_i64().unwrap_or(0),
                )
            })
            .collect())
    }
    fn set_retention(&self, type_: &str, days: Option<i64>) -> Result<(), BpaError> {
        check_retention(type_, days)?;
        match days {
            Some(d) => self.db.execute(
                "INSERT INTO event_retention (tenant_id, type, days) VALUES ($1::uuid, $2, $3) ON CONFLICT (tenant_id, type) DO UPDATE SET days = EXCLUDED.days",
                &[Param::Text(&self.tenant), Param::Text(type_), Param::Int(d)],
            )?,
            None => self.db.execute(
                "DELETE FROM event_retention WHERE tenant_id = $1::uuid AND type = $2",
                &[Param::Text(&self.tenant), Param::Text(type_)],
            )?,
        };
        Ok(())
    }
    fn sweep(&self, now: &str) -> Result<u64, BpaError> {
        let rules = self.retention()?;
        let named: Vec<String> = rules
            .iter()
            .filter(|(t, _)| t != "*")
            .map(|(t, _)| t.clone())
            .collect();
        let mut removed = 0;
        for (t, days) in &rules {
            let before = cutoff(now, *days)?;
            removed += if t == "*" {
                self.db.execute(
                    "DELETE FROM event_log WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND released_at < $3::timestamptz AND NOT (type = ANY(string_to_array($4, chr(31))))",
                    &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(&before), Param::Text(&named.join("\u{1f}"))],
                )?
            } else {
                self.db.execute(
                    "DELETE FROM event_log WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND type = $3 AND released_at < $4::timestamptz",
                    &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(t), Param::Text(&before)],
                )?
            };
        }
        Ok(removed)
    }
}

/// An event log in memory (the `memory` preset, suites): the same queries and sweep.
#[derive(Default)]
pub struct MemEventLog {
    rows: Mutex<Vec<Value>>,
    retention: Mutex<BTreeMap<String, i64>>,
    fixed_now: Mutex<Option<String>>,
}

impl MemEventLog {
    /// Fix the release time stamps (suites); `None` returns to the system clock.
    pub fn set_now(&self, at: Option<&str>) {
        if let Ok(mut g) = self.fixed_now.lock() {
            *g = at.map(str::to_string);
        }
    }

    fn now(&self) -> String {
        if let Ok(g) = self.fixed_now.lock()
            && let Some(t) = g.clone()
        {
            return t;
        }
        ubos_dvm::types::time::format_instant(&chrono::DateTime::<chrono::Utc>::from(
            std::time::SystemTime::now(),
        ))
    }

    /// Every event, in order (suites).
    pub fn all(&self) -> Vec<Value> {
        self.rows.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

fn mem_err() -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", "event log lock")
}

impl OutboxSink for MemEventLog {
    fn release(&self, effects: &[Value]) -> Result<(), BpaError> {
        let now = self.now();
        let mut rows = self.rows.lock().map_err(|_| mem_err())?;
        for e in effects {
            if rows.iter().any(|r| r["effect_id"] == e["effect_id"]) {
                continue;
            }
            let seq = rows.last().and_then(|r| r["seq"].as_i64()).unwrap_or(0) + 1;
            rows.push(json!({"seq": seq, "effect_id": e["effect_id"], "type": e["type"], "branch": e["branch"],
                "cseq": e.get("cseq").cloned().unwrap_or(Value::Null), "object_id": e.get("object_id").cloned().unwrap_or(Value::Null),
                "payload": e.get("payload").cloned().unwrap_or(Value::Null), "released_at": now}));
        }
        Ok(())
    }
}

impl EventStore for MemEventLog {
    fn query(&self, f: &EventFilter) -> Result<Vec<Value>, BpaError> {
        let rows = self.rows.lock().map_err(|_| mem_err())?;
        Ok(rows
            .iter()
            .filter(|r| r["seq"].as_i64().unwrap_or(0) > f.after)
            .filter(|r| f.type_.as_ref().is_none_or(|t| r["type"] == json!(t)))
            .filter(|r| {
                f.object_id
                    .as_ref()
                    .is_none_or(|o| r["object_id"] == json!(o))
            })
            .filter(|r| {
                f.from
                    .as_ref()
                    .is_none_or(|t| r["released_at"].as_str().unwrap_or_default() >= t.as_str())
            })
            .filter(|r| {
                f.to.as_ref()
                    .is_none_or(|t| r["released_at"].as_str().unwrap_or_default() < t.as_str())
            })
            .take(f.limit() as usize)
            .cloned()
            .collect())
    }
    fn retention(&self) -> Result<Vec<(String, i64)>, BpaError> {
        Ok(self
            .retention
            .lock()
            .map_err(|_| mem_err())?
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }
    fn set_retention(&self, type_: &str, days: Option<i64>) -> Result<(), BpaError> {
        check_retention(type_, days)?;
        let mut r = self.retention.lock().map_err(|_| mem_err())?;
        match days {
            Some(d) => r.insert(type_.to_string(), d),
            None => r.remove(type_),
        };
        Ok(())
    }
    fn sweep(&self, now: &str) -> Result<u64, BpaError> {
        let rules = self.retention()?;
        let mut cut: BTreeMap<String, String> = BTreeMap::new();
        for (t, d) in &rules {
            cut.insert(t.clone(), cutoff(now, *d)?);
        }
        let mut rows = self.rows.lock().map_err(|_| mem_err())?;
        let before = rows.len();
        rows.retain(|r| {
            let t = r["type"].as_str().unwrap_or_default();
            let at = r["released_at"].as_str().unwrap_or_default();
            match cut.get(t).or_else(|| cut.get("*")) {
                Some(c) => at >= c.as_str(),
                None => true,
            }
        });
        Ok((before - rows.len()) as u64)
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

// ---- jobs (DSN-NOD-220, -221, -223; step 15b: deliveries and dead letters) ----

/// The retry policy of a job (FR-EVT-051): `max_attempts` attempts with an exponential
/// backoff from `backoff_ms`, at most `max_backoff_ms`, with jitter. The standard default
/// is 5 attempts from 1 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts in all.
    pub max_attempts: i64,
    /// The first delay.
    pub backoff_ms: i64,
    /// The longest delay.
    pub max_backoff_ms: i64,
    /// Whether delays are jittered (between half and the whole delay).
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_attempts: 5,
            backoff_ms: 1_000,
            max_backoff_ms: 300_000,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// A policy from `{max_attempts?, backoff_ms?, max_backoff_ms?, jitter?}`.
    pub fn from_json(v: &Value) -> RetryPolicy {
        let d = RetryPolicy::default();
        let int = |k: &str, dv: i64, lo: i64, hi: i64| {
            v.get(k).and_then(Value::as_i64).unwrap_or(dv).clamp(lo, hi)
        };
        RetryPolicy {
            max_attempts: int("max_attempts", d.max_attempts, 1, 100),
            backoff_ms: int("backoff_ms", d.backoff_ms, 0, 3_600_000),
            max_backoff_ms: int("max_backoff_ms", d.max_backoff_ms, 0, 86_400_000),
            jitter: v.get("jitter").and_then(Value::as_bool).unwrap_or(d.jitter),
        }
    }

    /// The delay before the attempt after `attempts` failed ones.
    pub fn delay_ms(&self, attempts: i64) -> i64 {
        let exp = attempts.saturating_sub(1).clamp(0, 30) as u32;
        let base = self
            .backoff_ms
            .saturating_mul(1i64 << exp)
            .min(self.max_backoff_ms);
        if !self.jitter || base == 0 {
            return base;
        }
        let r: [u8; 2] = ubos_dvm::crypto::random_bytes().unwrap_or([0, 0]);
        let frac = f64::from(u16::from_le_bytes(r)) / f64::from(u16::MAX);
        (base as f64 * (0.5 + 0.5 * frac)) as i64
    }
}

/// What a job listing selects.
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    /// State (`queued`, `running`, `done`, `dead`, `cancelled`).
    pub state: Option<String>,
    /// Kind.
    pub kind: Option<String>,
    /// At most this many (1…1,000; 100 by default).
    pub limit: i64,
}

/// The jobs of a VAE: enqueue (with an optional dedupe key and earliest start), claim under a
/// lease, complete, fail with a backoff or into `dead`, and the operator actions on dead
/// letters (FR-EVT-052: re-drive, cancel, annotate; payloads are never edited).
pub trait JobStore: Jobs {
    /// Claim the next ready job of `kind` (any kind when `None`) for `worker`.
    fn claim(
        &self,
        kind: Option<&str>,
        worker: &str,
        lease_ms: i64,
    ) -> Result<Option<Value>, BpaError>;
    /// Complete a job with its result.
    fn complete(&self, job_id: &str, result: &Value) -> Result<(), BpaError>;
    /// Fail an attempt: requeue after `retry_ms`, or `dead` when the attempts are spent or
    /// `final_` is set; returns the new state.
    fn fail(
        &self,
        job_id: &str,
        error: &str,
        retry_ms: i64,
        final_: bool,
    ) -> Result<String, BpaError>;
    /// A job.
    fn job(&self, job_id: &str) -> Result<Option<Value>, BpaError>;
    /// Jobs by filter, newest first.
    fn list(&self, f: &JobFilter) -> Result<Vec<Value>, BpaError>;
    /// Re-drive a dead or cancelled job: queued now with a fresh attempt budget.
    fn retry(&self, job_id: &str) -> Result<bool, BpaError>;
    /// Cancel a queued or dead job.
    fn cancel(&self, job_id: &str) -> Result<bool, BpaError>;
    /// Add an operator's note.
    fn annotate(&self, job_id: &str, by: &str, text: &str) -> Result<bool, BpaError>;
}

const JOB_COLUMNS: &str = "job_id::text AS job_id, kind, payload, state, attempts, max_attempts, worker, result, error, dedupe_key, annotations, to_char(not_before AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS not_before, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at";

/// Jobs of a VAE in the `job` table.
pub struct JobQueue {
    db: Arc<Db>,
    tenant: String,
    vae: String,
}

impl JobQueue {
    /// The jobs of a VAE.
    pub fn new(db: Arc<Db>, tenant: &str, vae: &str) -> JobQueue {
        JobQueue {
            db,
            tenant: tenant.to_string(),
            vae: vae.to_string(),
        }
    }

    /// Claim the next ready job of any kind for `worker` under a lease of `lease_seconds`.
    pub fn claim(&self, worker: &str, lease_seconds: i64) -> Result<Option<Value>, BpaError> {
        JobStore::claim(self, None, worker, lease_seconds * 1000)
    }

    /// Complete a job.
    pub fn complete(&self, job_id: &str, result: &Value) -> Result<(), BpaError> {
        JobStore::complete(self, job_id, result)
    }

    /// Fail an attempt with no delay (gate 1a): requeued, or dead when spent.
    pub fn fail(&self, job_id: &str, error: &str) -> Result<String, BpaError> {
        JobStore::fail(self, job_id, error, 0, false)
    }

    /// A job.
    pub fn job(&self, job_id: &str) -> Result<Option<Value>, BpaError> {
        JobStore::job(self, job_id)
    }
}

impl JobStore for JobQueue {
    fn claim(
        &self,
        kind: Option<&str>,
        worker: &str,
        lease_ms: i64,
    ) -> Result<Option<Value>, BpaError> {
        let k = kind.unwrap_or_default().to_string();
        let rows = self.db.query(
            &format!("UPDATE job SET state = 'running', worker = $3, attempts = attempts + 1, \
             lease_until = now() + make_interval(secs => $4::bigint / 1000.0), updated_at = now() \
             WHERE (tenant_id, job_id) = (SELECT tenant_id, job_id FROM job \
               WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND ($5 = '' OR kind = $5) \
                 AND ((state = 'queued' AND not_before <= now()) OR (state = 'running' AND lease_until < now())) \
               ORDER BY not_before, created_at LIMIT 1 FOR UPDATE SKIP LOCKED) \
             RETURNING {JOB_COLUMNS}"),
            &[
                Param::Text(&self.tenant),
                Param::Text(&self.vae),
                Param::Text(worker),
                Param::Int(lease_ms),
                Param::Text(&k),
            ],
        )?;
        Ok(rows.into_iter().next())
    }

    fn complete(&self, job_id: &str, result: &Value) -> Result<(), BpaError> {
        self.db.execute(
            "UPDATE job SET state = 'done', result = $3, error = NULL, lease_until = NULL, updated_at = now() WHERE tenant_id = $1::uuid AND job_id = $2::uuid",
            &[Param::Text(&self.tenant), Param::Text(job_id), Param::Json(result)],
        )?;
        Ok(())
    }

    fn fail(
        &self,
        job_id: &str,
        error: &str,
        retry_ms: i64,
        final_: bool,
    ) -> Result<String, BpaError> {
        let rows = self.db.query(
            "UPDATE job SET state = CASE WHEN $5 = 'true' OR attempts >= max_attempts THEN 'dead' ELSE 'queued' END, \
             error = $3, lease_until = NULL, not_before = now() + make_interval(secs => $4::bigint / 1000.0), updated_at = now() \
             WHERE tenant_id = $1::uuid AND job_id = $2::uuid RETURNING state",
            &[Param::Text(&self.tenant), Param::Text(job_id), Param::Text(error), Param::Int(retry_ms), Param::Text(if final_ { "true" } else { "false" })],
        )?;
        Ok(rows
            .first()
            .and_then(|r| r["state"].as_str())
            .unwrap_or_default()
            .to_string())
    }

    fn job(&self, job_id: &str) -> Result<Option<Value>, BpaError> {
        let rows = self.db.query(
            &format!(
                "SELECT {JOB_COLUMNS} FROM job WHERE tenant_id = $1::uuid AND job_id = $2::uuid"
            ),
            &[Param::Text(&self.tenant), Param::Text(job_id)],
        )?;
        Ok(rows.into_iter().next())
    }

    fn list(&self, f: &JobFilter) -> Result<Vec<Value>, BpaError> {
        let (st, k) = (
            f.state.clone().unwrap_or_default(),
            f.kind.clone().unwrap_or_default(),
        );
        let limit = if f.limit <= 0 {
            100
        } else {
            f.limit.min(1_000)
        };
        self.db.query(
            &format!("SELECT {JOB_COLUMNS} FROM job WHERE tenant_id = $1::uuid AND vae_id = $2::uuid \
              AND ($3 = '' OR state = $3) AND ($4 = '' OR kind = $4) ORDER BY created_at DESC LIMIT $5"),
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(&st), Param::Text(&k), Param::Int(limit)],
        )
    }

    fn retry(&self, job_id: &str) -> Result<bool, BpaError> {
        let n = self.db.execute(
            "UPDATE job SET state = 'queued', attempts = 0, not_before = now(), lease_until = NULL, updated_at = now() \
             WHERE tenant_id = $1::uuid AND job_id = $2::uuid AND state IN ('dead', 'cancelled')",
            &[Param::Text(&self.tenant), Param::Text(job_id)],
        )?;
        Ok(n == 1)
    }

    fn cancel(&self, job_id: &str) -> Result<bool, BpaError> {
        let n = self.db.execute(
            "UPDATE job SET state = 'cancelled', lease_until = NULL, updated_at = now() \
             WHERE tenant_id = $1::uuid AND job_id = $2::uuid AND state IN ('queued', 'dead')",
            &[Param::Text(&self.tenant), Param::Text(job_id)],
        )?;
        Ok(n == 1)
    }

    fn annotate(&self, job_id: &str, by: &str, text: &str) -> Result<bool, BpaError> {
        let note = json!([{"at": ubos_dvm::types::time::format_instant(&chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())), "by": by, "text": text}]);
        let n = self.db.execute(
            "UPDATE job SET annotations = annotations || $3, updated_at = now() WHERE tenant_id = $1::uuid AND job_id = $2::uuid",
            &[Param::Text(&self.tenant), Param::Text(job_id), Param::Json(&note)],
        )?;
        Ok(n == 1)
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
        let dedupe = job
            .get("dedupe_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let rows = self.db.query(
            "INSERT INTO job (tenant_id, job_id, vae_id, kind, payload, max_attempts, dedupe_key) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, nullif($7, '')) \
             ON CONFLICT (tenant_id, vae_id, dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING RETURNING job_id::text AS job_id",
            &[Param::Text(&self.tenant), Param::Text(&id), Param::Text(&self.vae), Param::Text(&kind), Param::Json(&payload), Param::Int(max), Param::Text(&dedupe)],
        )?;
        if rows.is_empty() {
            // a job with this dedupe key exists: its identifier
            let r = self.db.query(
                "SELECT job_id::text AS job_id FROM job WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND dedupe_key = $3",
                &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(&dedupe)],
            )?;
            return Ok(r
                .first()
                .and_then(|x| x["job_id"].as_str())
                .unwrap_or_default()
                .to_string());
        }
        Ok(id)
    }
}

/// Jobs in memory (the `memory` preset, suites): the same behaviour as the `job` table.
#[derive(Default)]
pub struct MemJobQueue {
    rows: Mutex<Vec<Value>>,
    fixed_now: Mutex<Option<i64>>,
}

impl MemJobQueue {
    /// Fix the clock in milliseconds (suites); `None` returns to the system clock.
    pub fn set_now(&self, at: Option<i64>) {
        if let Ok(mut g) = self.fixed_now.lock() {
            *g = at;
        }
    }

    fn now_ms(&self) -> i64 {
        if let Ok(g) = self.fixed_now.lock()
            && let Some(t) = *g
        {
            return t;
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default()
    }

    fn with<R>(
        &self,
        job_id: &str,
        f: impl FnOnce(&mut Value) -> R,
    ) -> Result<Option<R>, BpaError> {
        let mut rows = self.rows.lock().map_err(|_| mem_err())?;
        Ok(rows
            .iter_mut()
            .find(|r| r["job_id"] == json!(job_id))
            .map(f))
    }
}

fn public_job(r: &Value) -> Value {
    let mut v = r.clone();
    if let Some(m) = v.as_object_mut() {
        m.remove("not_before_ms");
        m.remove("lease_until_ms");
    }
    v
}

impl Jobs for MemJobQueue {
    fn enqueue(&self, job: &Value) -> Result<String, BpaError> {
        let dedupe = job
            .get("dedupe_key")
            .and_then(Value::as_str)
            .map(str::to_string);
        let now = self.now_ms();
        let mut rows = self.rows.lock().map_err(|_| mem_err())?;
        if let Some(d) = &dedupe
            && let Some(r) = rows.iter().find(|r| r["dedupe_key"] == json!(d))
        {
            return Ok(r["job_id"].as_str().unwrap_or_default().to_string());
        }
        let id = crate::uuid()?;
        rows.push(json!({"job_id": id, "kind": job.get("kind").and_then(Value::as_str).unwrap_or("job"),
            "payload": job.get("payload").cloned().unwrap_or(json!({})), "state": "queued", "attempts": 0,
            "max_attempts": job.get("max_attempts").and_then(Value::as_i64).unwrap_or(3), "worker": null,
            "result": null, "error": null, "dedupe_key": dedupe, "annotations": [], "not_before_ms": now,
            "lease_until_ms": 0, "created_ms": now}));
        Ok(id)
    }
}

impl JobStore for MemJobQueue {
    fn claim(
        &self,
        kind: Option<&str>,
        worker: &str,
        lease_ms: i64,
    ) -> Result<Option<Value>, BpaError> {
        let now = self.now_ms();
        let mut rows = self.rows.lock().map_err(|_| mem_err())?;
        let mut ready: Vec<&mut Value> = rows
            .iter_mut()
            .filter(|r| kind.is_none_or(|k| r["kind"] == json!(k)))
            .filter(|r| {
                (r["state"] == "queued" && r["not_before_ms"].as_i64().unwrap_or(0) <= now)
                    || (r["state"] == "running" && r["lease_until_ms"].as_i64().unwrap_or(0) < now)
            })
            .collect();
        ready.sort_by_key(|r| {
            (
                r["not_before_ms"].as_i64().unwrap_or(0),
                r["created_ms"].as_i64().unwrap_or(0),
            )
        });
        let Some(r) = ready.into_iter().next() else {
            return Ok(None);
        };
        r["state"] = json!("running");
        r["worker"] = json!(worker);
        r["attempts"] = json!(r["attempts"].as_i64().unwrap_or(0) + 1);
        r["lease_until_ms"] = json!(now + lease_ms);
        Ok(Some(public_job(r)))
    }
    fn complete(&self, job_id: &str, result: &Value) -> Result<(), BpaError> {
        self.with(job_id, |r| {
            r["state"] = json!("done");
            r["result"] = result.clone();
            r["error"] = Value::Null;
        })?;
        Ok(())
    }
    fn fail(
        &self,
        job_id: &str,
        error: &str,
        retry_ms: i64,
        final_: bool,
    ) -> Result<String, BpaError> {
        let now = self.now_ms();
        Ok(self
            .with(job_id, |r| {
                let spent =
                    r["attempts"].as_i64().unwrap_or(0) >= r["max_attempts"].as_i64().unwrap_or(0);
                let st = if final_ || spent { "dead" } else { "queued" };
                r["state"] = json!(st);
                r["error"] = json!(error);
                r["not_before_ms"] = json!(now + retry_ms);
                st.to_string()
            })?
            .unwrap_or_default())
    }
    fn job(&self, job_id: &str) -> Result<Option<Value>, BpaError> {
        let rows = self.rows.lock().map_err(|_| mem_err())?;
        Ok(rows
            .iter()
            .find(|r| r["job_id"] == json!(job_id))
            .map(public_job))
    }
    fn list(&self, f: &JobFilter) -> Result<Vec<Value>, BpaError> {
        let rows = self.rows.lock().map_err(|_| mem_err())?;
        let limit = if f.limit <= 0 {
            100
        } else {
            f.limit.min(1_000)
        } as usize;
        Ok(rows
            .iter()
            .rev()
            .filter(|r| f.state.as_ref().is_none_or(|s| r["state"] == json!(s)))
            .filter(|r| f.kind.as_ref().is_none_or(|k| r["kind"] == json!(k)))
            .take(limit)
            .map(public_job)
            .collect())
    }
    fn retry(&self, job_id: &str) -> Result<bool, BpaError> {
        let now = self.now_ms();
        Ok(self
            .with(job_id, |r| {
                if r["state"] == "dead" || r["state"] == "cancelled" {
                    r["state"] = json!("queued");
                    r["attempts"] = json!(0);
                    r["not_before_ms"] = json!(now);
                    true
                } else {
                    false
                }
            })?
            .unwrap_or(false))
    }
    fn cancel(&self, job_id: &str) -> Result<bool, BpaError> {
        Ok(self
            .with(job_id, |r| {
                if r["state"] == "queued" || r["state"] == "dead" {
                    r["state"] = json!("cancelled");
                    true
                } else {
                    false
                }
            })?
            .unwrap_or(false))
    }
    fn annotate(&self, job_id: &str, by: &str, text: &str) -> Result<bool, BpaError> {
        let at = ubos_dvm::types::time::format_instant(&chrono::DateTime::<chrono::Utc>::from(
            std::time::SystemTime::now(),
        ));
        Ok(self
            .with(job_id, |r| {
                if let Some(a) = r["annotations"].as_array_mut() {
                    a.push(json!({"at": at, "by": by, "text": text}));
                }
                true
            })?
            .unwrap_or(false))
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
