//! `ubos-node` — host layer (UBS-ARC-07 §2). The in-process node host of PH-1 gate 1a
//! (DEC-060, step 7a): the CTR-002 host services on PostgreSQL behind the host API of
//! `ubos-dvm::host` (CTR-001). Allowed dependencies: `ubos-dvm`, `ubos-proto` (AR-031); the
//! engine crates are reached through `ubos_dvm::{store, crypto, types}`.
//!
//! * [`Node`]: the tenant registry (DSN-NOD-401, -402) and `open_vae`.
//! * [`keys`]: the key hierarchy root KEK → tenant KEK → data keys and node signing keys
//!   (DSN-NOD-131, -134), the secret store (DSN-NOD-130).
//! * [`identity`]: local identity (step 14c): principals, Argon2id and TOTP credentials,
//!   sessions, API keys and scopes, principal keys, the node's security log
//!   (DSN-NOD-101, -105, -106, -108…-111, -150, -153).
//! * [`services`]: the blob service, connectors with an allow-list, the event log that
//!   receives released effects, jobs with leases and retries, telemetry without business
//!   data (DSN-NOD-201, -202, -220, -221, -223, -301, -320, -610, -611).
//!
//! The Server of gate 1b (UBTP, sessions, schedules, backup) builds on these services.

pub mod config;
pub mod db;
pub mod genesis;
pub mod identity;
pub mod keys;
pub mod leases;
pub mod platform;
pub mod schedule;
pub mod server;
pub mod services;

use db::Db;
use serde_json::{Value, json};
use std::sync::Arc;
use ubos_dvm::host::{Context, Services, Vae};
use ubos_dvm::store::kernel::Scope;
use ubos_dvm::store::{PgStore, Store};
use ubos_dvm::types::BpaError;
use ubos_dvm::{Engine, crypto};

/// Environment variable holding the root key-encryption key (64 hex digits), or
/// `UBOS_ROOT_KEK_FILE` naming a file with it. Absent both, a fixed development key is used.
pub const ROOT_KEK_ENV: &str = "UBOS_ROOT_KEK";

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

/// A UUID (version 4 form) from random bytes.
pub fn uuid() -> Result<String, BpaError> {
    let r: [u8; 16] = crypto::random_bytes()?;
    let h: String = r.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-4{}-8{}-{}",
        &h[0..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    ))
}

/// The root KEK of the node (DSN-NOD-131): `UBOS_ROOT_KEK`, `UBOS_ROOT_KEK_FILE`, or the
/// development key when neither is set.
pub fn root_kek() -> Result<[u8; 32], BpaError> {
    let text = match std::env::var(ROOT_KEK_ENV) {
        Ok(v) => Some(v),
        Err(_) => match std::env::var("UBOS_ROOT_KEK_FILE") {
            Ok(p) => Some(std::fs::read_to_string(&p).map_err(err)?.trim().to_string()),
            Err(_) => None,
        },
    };
    match text {
        Some(t) => crypto::unhex32(&json!(t), "UBOS_ROOT_KEK")
            .map_err(|_| BpaError::new("IAM.KEY_UNAVAILABLE", "UBOS_ROOT_KEK is 64 hex digits")),
        None => Ok(*b"ubos-development-root-kek-000000"),
    }
}

/// A tenant of the registry.
#[derive(Debug, Clone)]
pub struct Tenant {
    /// Tenant identifier.
    pub id: String,
    /// Slug.
    pub slug: String,
    /// State (`active`, `suspended`).
    pub state: String,
}

/// The node: registry, key hierarchy and services over one PostgreSQL database.
pub struct Node {
    url: String,
    db: Arc<Db>,
    root: [u8; 32],
    services: NodeServices,
    scoped: std::sync::Mutex<std::collections::HashMap<String, Arc<Db>>>,
}

/// The oldest PostgreSQL version a node runs on (`server_version_num`).
pub const MIN_POSTGRES: i64 = 160_000;
/// The clock skew a bootstrap and the self-checks accept against the database, in ms.
pub const MAX_CLOCK_SKEW_MS: i64 = 2_000;
/// The PostgreSQL extensions the migrations need.
pub const EXTENSIONS: &[&str] = &["btree_gist"];

/// The connections of a node's pool of one tenant.
pub const TENANT_POOL: u32 = 8;

/// The service implementations shared by the VAEs of a node.
#[derive(Clone)]
pub struct NodeServices {
    /// Connector registry (allow-list).
    pub connectors: Arc<services::ConnectorRegistry>,
    /// Telemetry sink.
    pub telemetry: Arc<services::LogTelemetry>,
}

impl Node {
    /// Connect to the database as the owner role for the registry; the root KEK comes from
    /// `root_kek`.
    pub fn connect(url: &str) -> Result<Node, BpaError> {
        Node::connect_with_root(url, root_kek()?)
    }

    /// Connect with a given root KEK.
    pub fn connect_with_root(url: &str, root: [u8; 32]) -> Result<Node, BpaError> {
        ubos_dvm::store::PgBackend::migrate(url)?;
        let db = Arc::new(Db::owner(url)?);
        Ok(Node {
            url: url.to_string(),
            db,
            root,
            services: NodeServices {
                connectors: Arc::new(services::ConnectorRegistry::default()),
                telemetry: Arc::new(services::LogTelemetry::default()),
            },
            scoped: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// The services shared by the node's VAEs (register connectors here).
    pub fn services(&self) -> &NodeServices {
        &self.services
    }

    /// Create a tenant (DSN-NOD-402): a fresh KEK wrapped by the root KEK. `TXN.ALREADY_EXISTS`
    /// when the slug is taken.
    pub fn create_tenant(&self, slug: &str) -> Result<Tenant, BpaError> {
        if self.tenant(slug)?.is_some() {
            return Err(BpaError::new("TXN.ALREADY_EXISTS", "tenant exists"));
        }
        let id = uuid()?;
        let kek: [u8; 32] = crypto::random_bytes()?;
        let nonce: [u8; crypto::NONCE_LEN] = crypto::random_bytes()?;
        let wrapped = crypto::seal(&self.root, &nonce, id.as_bytes(), &kek)?;
        let (id2, slug2) = (id.clone(), slug.to_string());
        self.db.run(move |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO tenant (tenant_id, slug, kek_wrapped, kek_nonce) VALUES ($1::uuid, $2, $3, $4)")
                    .bind(&id2)
                    .bind(&slug2)
                    .bind(wrapped.as_slice())
                    .bind(nonce.as_slice())
                    .execute(&pool)
                    .await
                    .map(|_| ())
                    .map_err(err)
            })
        })?;
        self.services
            .telemetry
            .emit_now(&json!({"name": "node.tenant_created", "tenant": id}));
        Ok(Tenant {
            id,
            slug: slug.to_string(),
            state: "active".into(),
        })
    }

    /// A tenant by slug.
    pub fn tenant(&self, slug: &str) -> Result<Option<Tenant>, BpaError> {
        let slug = slug.to_string();
        self.db.run(move |pool| {
            Box::pin(async move {
                let r: Option<(String, String, String)> = sqlx::query_as(
                    "SELECT tenant_id::text, slug, state FROM tenant WHERE slug = $1",
                )
                .bind(&slug)
                .fetch_optional(&pool)
                .await
                .map_err(err)?;
                Ok(r.map(|(id, slug, state)| Tenant { id, slug, state }))
            })
        })
    }

    /// The tenants of the node, by slug.
    pub fn tenants(&self) -> Result<Vec<Tenant>, BpaError> {
        self.db.run(move |pool| {
            Box::pin(async move {
                let rows: Vec<(String, String, String)> =
                    sqlx::query_as("SELECT tenant_id::text, slug, state FROM tenant ORDER BY slug")
                        .fetch_all(&pool)
                        .await
                        .map_err(err)?;
                Ok(rows
                    .into_iter()
                    .map(|(id, slug, state)| Tenant { id, slug, state })
                    .collect())
            })
        })
    }

    /// Create a VAE of a tenant with the given authority (STD-ADDR-001); returns its
    /// identifier. `TXN.ALREADY_EXISTS` when the authority is taken in the tenant.
    pub fn create_vae(&self, tenant: &Tenant, authority: &str) -> Result<String, BpaError> {
        if self.vae(tenant, authority)?.is_some() {
            return Err(BpaError::new("TXN.ALREADY_EXISTS", "VAE exists"));
        }
        let id = uuid()?;
        let (t, v, a) = (tenant.id.clone(), id.clone(), authority.to_string());
        self.db.run(move |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO vae (tenant_id, vae_id, authority) VALUES ($1::uuid, $2::uuid, $3)")
                    .bind(&t)
                    .bind(&v)
                    .bind(&a)
                    .execute(&pool)
                    .await
                    .map(|_| ())
                    .map_err(err)
            })
        })?;
        Ok(id)
    }

    /// The identifier of a VAE by authority.
    pub fn vae(&self, tenant: &Tenant, authority: &str) -> Result<Option<String>, BpaError> {
        let (t, a) = (tenant.id.clone(), authority.to_string());
        self.db.run(move |pool| {
            Box::pin(async move {
                let r: Option<(String,)> = sqlx::query_as(
                    "SELECT vae_id::text FROM vae WHERE tenant_id = $1::uuid AND authority = $2",
                )
                .bind(&t)
                .bind(&a)
                .fetch_optional(&pool)
                .await
                .map_err(err)?;
                Ok(r.map(|(id,)| id))
            })
        })
    }

    /// The authorities of a tenant's VAEs.
    pub fn vaes(&self, tenant: &Tenant) -> Result<Vec<String>, BpaError> {
        let rows = self.db.query(
            "SELECT authority FROM vae WHERE tenant_id = $1::uuid ORDER BY authority",
            &[db::Param::Text(&tenant.id)],
        )?;
        Ok(rows
            .iter()
            .filter_map(|r| r["authority"].as_str().map(str::to_string))
            .collect())
    }

    // ---- the tenant registry and tenant creation (DSN-NOD-401, -402; step 16b) ----

    /// The registry record of a tenant: `{tenant_id, slug, state, plan, placement,
    /// residency, keys_ref, vaes}` (DSN-NOD-401); `None` when unknown.
    pub fn tenant_record(&self, slug: &str) -> Result<Option<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT tenant_id::text AS tenant_id, slug, state, plan, placement, residency, keys_ref FROM tenant WHERE slug = $1",
            &[db::Param::Text(slug)],
        )?;
        let Some(mut r) = rows.into_iter().next() else {
            return Ok(None);
        };
        let t = Tenant {
            id: r["tenant_id"].as_str().unwrap_or_default().to_string(),
            slug: slug.to_string(),
            state: r["state"].as_str().unwrap_or("active").to_string(),
        };
        r["vaes"] = json!(self.vaes(&t)?);
        if r["keys_ref"].is_null() {
            r["keys_ref"] = json!(format!("kek:{}:{}", t.id, 1));
        }
        Ok(Some(r))
    }

    /// Change the platform settings of a tenant (`plan`, `placement`, `residency`; CTL in
    /// later phases); returns the new record. The caller projects it into the VAEs.
    pub fn set_tenant_settings(&self, slug: &str, patch: &Value) -> Result<Value, BpaError> {
        let cur = self
            .tenant_record(slug)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant"))?;
        let pick = |k: &str| -> Result<String, BpaError> {
            match patch.get(k) {
                Some(Value::String(v)) if !v.is_empty() && v.len() <= 100 => Ok(v.clone()),
                Some(Value::Null) | None => Ok(cur[k].as_str().unwrap_or_default().to_string()),
                Some(_) => Err(BpaError::new(
                    "MODEL.INVALID_VALUE",
                    format!("{k} is a text of 1 to 100 characters"),
                )),
            }
        };
        let (plan, placement, residency) = (pick("plan")?, pick("placement")?, pick("residency")?);
        self.db.execute(
            "UPDATE tenant SET plan = $2, placement = $3, residency = nullif($4, '') WHERE slug = $1",
            &[db::Param::Text(slug), db::Param::Text(&plan), db::Param::Text(&placement), db::Param::Text(&residency)],
        )?;
        self.tenant_record(slug)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant"))
    }

    fn setup_done(&self, slug: &str) -> Result<Vec<String>, BpaError> {
        Ok(self
            .db
            .query(
                "SELECT step FROM tenant_setup WHERE slug = $1",
                &[db::Param::Text(slug)],
            )?
            .iter()
            .filter_map(|r| r["step"].as_str().map(str::to_string))
            .collect())
    }

    fn setup_mark(&self, slug: &str, step: &str, detail: &Value) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO tenant_setup (slug, step, detail) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            &[db::Param::Text(slug), db::Param::Text(step), db::Param::Json(detail)],
        )?;
        // a fault injected after a step (the resumption checks of SUITE-NOD-OPS)
        if std::env::var("UBOS_FAULT_AFTER_STEP").is_ok_and(|s| s == step) {
            eprintln!("ubos: fault injected after the tenant setup step {step}");
            std::process::exit(86);
        }
        Ok(())
    }

    /// Create a tenant and its first VAE, idempotently and resumably (DSN-NOD-402): the
    /// tenant KEK and the registry record, the VAE (`create_vae`), the genesis Buks, the
    /// `Tenant` projection, the first administrator's invitation (when `admin` names one
    /// and the tenant has no administrator) and the creation event. Each step is recorded;
    /// a run after an interruption does what is missing, never twice. Returns `{steps,
    /// invitation?}`.
    pub fn provision(
        &self,
        slug: &str,
        authority: &str,
        admin: Option<&str>,
    ) -> Result<Value, BpaError> {
        let done = self.setup_done(slug)?;
        let mut steps = Vec::new();
        let mut step = |name: &str, ran: bool| {
            steps.push(json!({"step": name, "status": if ran { "done" } else { "already done" }}))
        };
        // 1. the KEK and the registry record, in one statement
        let t = match self.tenant(slug)? {
            Some(t) => t,
            None => self.create_tenant(slug)?,
        };
        let ran = !done.contains(&"register".to_string());
        if ran {
            self.setup_mark(slug, "register", &json!({"tenant_id": t.id}))?;
        }
        step("register", ran);
        // 2. the first VAE
        let ran = self.vae(&t, authority)?.is_none();
        if ran {
            self.create_vae(&t, authority)?;
        }
        if !done.contains(&"vae".to_string()) {
            self.setup_mark(slug, "vae", &json!({"authority": authority}))?;
        }
        step("vae", ran);
        // 3. the genesis Buks, 4. the Tenant projection
        let mut vae = self.open_vae(&t, authority)?;
        let installed = genesis::install_all(&mut vae)?;
        let ran = installed.iter().any(|x| x["status"] == "installed");
        if !done.contains(&"genesis".to_string()) {
            self.setup_mark(slug, "genesis", &json!({"buks": installed}))?;
        }
        step("genesis", ran);
        let record = self
            .tenant_record(slug)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant"))?;
        let ran = platform::project_tenant(&mut vae, &record)?;
        if !done.contains(&"projection".to_string()) {
            self.setup_mark(slug, "projection", &json!({}))?;
        }
        step("projection", ran);
        drop(vae);
        // 5. the first administrator's invitation
        let mut invitation = Value::Null;
        if let Some(user) = admin {
            let id = self.identity(&t)?;
            let has_admin = id
                .principals()?
                .iter()
                .any(|p| p["role"] == "admin" && p["kind"] == "human");
            let ran = !has_admin;
            if ran {
                invitation = id.invite(user, "admin", 24 * 3600)?;
            }
            if !done.contains(&"invitation".to_string()) {
                self.setup_mark(slug, "invitation", &json!({"username": user}))?;
            }
            step("invitation", ran);
        }
        // 6. the creation event, once
        let ran = !done.contains(&"event".to_string());
        if ran {
            self.identity(&t)?.log(
                "tenant.created",
                json!({"tenant": t.id, "slug": slug, "vae": authority}),
            )?;
            self.services
                .telemetry
                .emit_now(&json!({"name": "node.tenant_provisioned", "tenant": t.id}));
            self.setup_mark(slug, "event", &json!({}))?;
        }
        step("event", ran);
        Ok(json!({"tenant": t.id, "slug": slug, "steps": steps, "invitation": invitation}))
    }

    /// Record an applied configuration version (FR-OPS-093): `{node, hash, author, source,
    /// document, diff, restart_required}`; returns its sequence number.
    pub fn record_config(&self, v: &Value) -> Result<i64, BpaError> {
        let t = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        let (node, hash, author, source, doc) = (
            t("node"),
            t("hash"),
            t("author"),
            t("source"),
            t("document"),
        );
        let rows = self.db.query(
            "INSERT INTO node_config (node_id, hash, author, source, document, diff, restart_required) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING seq",
            &[
                db::Param::Text(&node),
                db::Param::Text(&hash),
                db::Param::Text(&author),
                db::Param::Text(&source),
                db::Param::Text(&doc),
                db::Param::Json(&v["diff"]),
                db::Param::Json(&v["restart_required"]),
            ],
        )?;
        Ok(rows.first().and_then(|r| r["seq"].as_i64()).unwrap_or(0))
    }

    /// The configuration versions applied, newest first (all nodes, or one).
    pub fn config_versions(&self, node: Option<&str>, limit: i64) -> Result<Vec<Value>, BpaError> {
        self.db.query(
            "SELECT seq, node_id, to_char(applied_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS applied_at, \
             hash, author, source, diff, restart_required FROM node_config WHERE ($1 = '' OR node_id = $1) ORDER BY seq DESC LIMIT $2",
            &[db::Param::Text(node.unwrap_or_default()), db::Param::Int(limit.clamp(1, 1000))],
        )
    }

    /// The prerequisites of a bootstrap (DSN-NOD-602): the PostgreSQL version, the
    /// extensions the migrations need, the clock against the database's and whether the
    /// root key is the development one. Each check is `{check, ok, detail, fix?}`.
    pub fn prerequisites(url: &str, extensions: &[&str]) -> Vec<Value> {
        let mut out = Vec::new();
        let db = match Db::owner(url) {
            Ok(d) => d,
            Err(e) => {
                out.push(
                    json!({"check": "database", "ok": false, "detail": e.to_string(),
                    "fix": "start PostgreSQL 16 or later and check database.url"}),
                );
                return out;
            }
        };
        match db.query("SELECT current_setting('server_version_num')::int AS v, current_setting('server_version') AS t", &[]) {
            Ok(r) => {
                let v = r.first().and_then(|x| x["v"].as_i64()).unwrap_or(0);
                let t = r.first().and_then(|x| x["t"].as_str()).unwrap_or("?").to_string();
                out.push(json!({"check": "postgresql_version", "ok": v >= MIN_POSTGRES, "detail": t,
                    "fix": if v >= MIN_POSTGRES { Value::Null } else { json!("upgrade PostgreSQL to version 16 or later") }}));
            }
            Err(e) => out.push(json!({"check": "postgresql_version", "ok": false, "detail": e.to_string()})),
        }
        for ext in extensions {
            let rows = db
                .query(
                    "SELECT (SELECT count(*) FROM pg_available_extensions WHERE name = $1)::int AS available, \
                     (SELECT count(*) FROM pg_extension WHERE extname = $1)::int AS installed",
                    &[db::Param::Text(ext)],
                )
                .unwrap_or_default();
            let r = rows.first().cloned().unwrap_or(Value::Null);
            let ok = r["available"].as_i64().unwrap_or(0) > 0
                || r["installed"].as_i64().unwrap_or(0) > 0;
            out.push(json!({"check": format!("extension {ext}"), "ok": ok,
                "detail": if ok { format!("{ext} is available") } else { format!("the PostgreSQL extension {ext} is not available") },
                "fix": if ok { Value::Null } else { json!(format!("install the PostgreSQL contrib package (postgresql-16 contrib, e.g. postgresql-contrib), then run bootstrap again; it runs CREATE EXTENSION {ext}")) }}));
        }
        match db.query(
            "SELECT (extract(epoch FROM clock_timestamp()) * 1000)::bigint AS ms",
            &[],
        ) {
            Ok(r) => {
                let theirs = r.first().and_then(|x| x["ms"].as_i64()).unwrap_or(0);
                let ours = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or_default();
                let skew = ours - theirs;
                out.push(json!({"check": "clock", "ok": skew.abs() <= MAX_CLOCK_SKEW_MS, "detail": format!("{skew} ms against the database"),
                    "fix": if skew.abs() <= MAX_CLOCK_SKEW_MS { Value::Null } else { json!("synchronise the clocks (NTP) of this host and the database host") }}));
            }
            Err(e) => out.push(json!({"check": "clock", "ok": false, "detail": e.to_string()})),
        }
        out
    }

    /// The `(tenant slug, authority)` of the active VAEs with work for the Server: effects
    /// released but not acknowledged, became-effective timers due within `horizon_secs`
    /// (NR-DUR-006, NR-PERF-012), jobs queued or running, or schedules (step 15d: missed
    /// slots are computed after a full outage, DSN-NOD-226). The owner role reads across
    /// tenants.
    pub fn vaes_with_work(&self, horizon_secs: i64) -> Result<Vec<(String, String)>, BpaError> {
        let rows = self.db.query(
            "SELECT t.slug, v.authority FROM vae v JOIN tenant t ON t.tenant_id = v.tenant_id \
             WHERE t.state = 'active' AND ( \
               EXISTS (SELECT 1 FROM outbox o WHERE o.tenant_id = v.tenant_id AND o.vae_id = v.vae_id AND o.released_at IS NULL) \
               OR EXISTS (SELECT 1 FROM timer m WHERE m.tenant_id = v.tenant_id AND m.vae_id = v.vae_id AND m.fired_at IS NULL \
                          AND m.due_at < now() + make_interval(secs => $1::bigint)) \
               OR EXISTS (SELECT 1 FROM job j WHERE j.tenant_id = v.tenant_id AND j.vae_id = v.vae_id AND j.state IN ('queued', 'running')) \
               OR EXISTS (SELECT 1 FROM schedule_cursor c WHERE c.tenant_id = v.tenant_id AND c.vae_id = v.vae_id)) \
             ORDER BY t.slug, v.authority",
            &[db::Param::Int(horizon_secs)],
        )?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r["slug"].as_str().unwrap_or_default().to_string(),
                    r["authority"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect())
    }

    /// The leadership leases of the node (DSN-NOD-228).
    pub fn leases(&self) -> Arc<leases::PgLeases> {
        Arc::new(leases::PgLeases::new(self.db.clone()))
    }

    /// The event log of a tenant's VAE (DSN-NOD-202); `None` when the VAE is unknown.
    pub fn events(
        &self,
        tenant: &Tenant,
        authority: &str,
    ) -> Result<Option<services::EventLog>, BpaError> {
        let Some(vae_id) = self.vae(tenant, authority)? else {
            return Ok(None);
        };
        let scoped = self.scoped(tenant)?;
        Ok(Some(services::EventLog::new(scoped, &tenant.id, &vae_id)))
    }

    /// The jobs of a tenant's VAE (deliveries and dead letters); `None` when the VAE is
    /// unknown.
    pub fn jobs(
        &self,
        tenant: &Tenant,
        authority: &str,
    ) -> Result<Option<services::JobQueue>, BpaError> {
        let Some(vae_id) = self.vae(tenant, authority)? else {
            return Ok(None);
        };
        let scoped = self.scoped(tenant)?;
        Ok(Some(services::JobQueue::new(scoped, &tenant.id, &vae_id)))
    }

    /// The schedule state of a tenant's VAE (step 15d); `None` when the VAE is unknown.
    pub fn schedules(
        &self,
        tenant: &Tenant,
        authority: &str,
    ) -> Result<Option<schedule::PgScheduleStore>, BpaError> {
        let Some(vae_id) = self.vae(tenant, authority)? else {
            return Ok(None);
        };
        let scoped = self.scoped(tenant)?;
        Ok(Some(schedule::PgScheduleStore::new(
            scoped, &tenant.id, &vae_id,
        )))
    }

    /// The key hierarchy of a tenant.
    pub fn keys(&self, tenant: &Tenant) -> Result<Arc<keys::Hierarchy>, BpaError> {
        keys::Hierarchy::open(self.db.clone(), self.root, &tenant.id).map(Arc::new)
    }

    /// The host services of a tenant's VAE: real clock and entropy, the node's connectors
    /// (with the tenant's secrets), no models, the event log, the job table and telemetry.
    pub fn vae_services(&self, tenant: &Tenant, vae_id: &str) -> Result<Services, BpaError> {
        let scoped = self.scoped(tenant)?;
        let hierarchy = self.keys(tenant)?;
        let secrets = Arc::new(keys::SecretStore::new(scoped.clone(), hierarchy));
        let connectors = Arc::new(services::BoundConnectors {
            registry: self.services.connectors.clone(),
            secrets,
        });
        Ok(Services {
            clock: Arc::new(ubos_dvm::host::SystemClock),
            entropy: Arc::new(ubos_dvm::host::OsEntropy),
            connectors,
            models: Arc::new(ubos_dvm::host::NoModels),
            outbox: Arc::new(services::EventLog::new(scoped.clone(), &tenant.id, vae_id)),
            jobs: Arc::new(services::JobQueue::new(scoped, &tenant.id, vae_id)),
            telemetry: self.services.telemetry.clone(),
        })
    }

    /// `open_vae` (CTR-001): the store of the VAE under the tenant's key hierarchy, blob
    /// service and node signer, the engine over it (initialised when new) and the services.
    pub fn open_vae(&self, tenant: &Tenant, authority: &str) -> Result<Vae, BpaError> {
        let vae_id = match self.vae(tenant, authority)? {
            Some(v) => v,
            None => self.create_vae(tenant, authority)?,
        };
        let hierarchy = self.keys(tenant)?;
        let scoped = self.scoped(tenant)?;
        let blobs = Arc::new(services::PgBlobs::new(scoped.clone(), &tenant.id));
        let signer = Arc::new(keys::Signer::new(
            scoped,
            hierarchy.clone(),
            &tenant.id,
            &vae_id,
            authority,
        ));
        let scope = Scope {
            tenant: tenant.id.clone(),
            vae: vae_id.clone(),
            authority: authority.to_string(),
        };
        let store = PgStore::connect_with(&self.url, scope, hierarchy, blobs)?.with_signer(signer);
        let engine = Engine::open_existing(Box::new(store))?;
        let services = self.vae_services(tenant, &vae_id)?;
        Ok(Vae::open(engine, services))
    }

    /// The local identity of a tenant (DSN-NOD-101…-153): records in the tenant tables,
    /// TOTP secrets and custodial key seeds under a key derived from the tenant KEK; the
    /// authority of its registered DIDs is the tenant slug.
    pub fn identity(&self, tenant: &Tenant) -> Result<identity::Identity, BpaError> {
        let scoped = self.scoped(tenant)?;
        let master = self.keys(tenant)?.derive("ubos.identity.v1", "master");
        Ok(identity::Identity::new(
            Box::new(identity::PgBackend::new(scoped, &tenant.id)),
            &tenant.slug,
            &tenant.slug,
            master,
        ))
    }

    /// The secret store of a tenant (DSN-NOD-130; the operator interface of the Server).
    pub fn secrets(&self, tenant: &Tenant) -> Result<keys::SecretStore, BpaError> {
        let scoped = self.scoped(tenant)?;
        Ok(keys::SecretStore::new(scoped, self.keys(tenant)?))
    }

    /// The scoped database of a tenant (services and suites).
    pub fn tenant_db(&self, tenant: &Tenant) -> Result<Arc<Db>, BpaError> {
        self.scoped(tenant)
    }

    /// The node's one pool of a tenant (opened once, `TENANT_POOL` connections), shared by
    /// the tenant's VAEs, stores and services, so that the connections of a node grow with
    /// its tenants rather than with every store of every VAE.
    fn scoped(&self, tenant: &Tenant) -> Result<Arc<Db>, BpaError> {
        let mut pools = self
            .scoped
            .lock()
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "pools lock"))?;
        if let Some(d) = pools.get(&tenant.id) {
            return Ok(d.clone());
        }
        let d = Arc::new(Db::tenant_with(&self.url, &tenant.id, TENANT_POOL)?);
        pools.insert(tenant.id.clone(), d.clone());
        Ok(d)
    }

    /// A context for `principal`.
    pub fn context(principal: &str) -> Context {
        Context {
            principal: principal.to_string(),
            on_behalf_of: None,
        }
    }
}

/// Whether a store has the node signer installed (suites).
pub fn has_signer(store: &dyn Store) -> bool {
    store.signer().is_some()
}
