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

pub mod db;
pub mod identity;
pub mod keys;
pub mod leases;
pub mod platform;
pub mod server;
pub mod services;

use db::Db;
use serde_json::json;
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
}

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

    /// The `(tenant slug, authority)` of the active VAEs with work for the Server: effects
    /// released but not acknowledged, or became-effective timers due within `horizon_secs`
    /// (NR-DUR-006, NR-PERF-012). The owner role reads across tenants.
    pub fn vaes_with_work(&self, horizon_secs: i64) -> Result<Vec<(String, String)>, BpaError> {
        let rows = self.db.query(
            "SELECT t.slug, v.authority FROM vae v JOIN tenant t ON t.tenant_id = v.tenant_id \
             WHERE t.state = 'active' AND ( \
               EXISTS (SELECT 1 FROM outbox o WHERE o.tenant_id = v.tenant_id AND o.vae_id = v.vae_id AND o.released_at IS NULL) \
               OR EXISTS (SELECT 1 FROM timer m WHERE m.tenant_id = v.tenant_id AND m.vae_id = v.vae_id AND m.fired_at IS NULL \
                          AND m.due_at < now() + make_interval(secs => $1::bigint))) \
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
        let scoped = Arc::new(Db::tenant(&self.url, &tenant.id)?);
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
        let scoped = Arc::new(Db::tenant(&self.url, &tenant.id)?);
        Ok(Some(services::JobQueue::new(scoped, &tenant.id, &vae_id)))
    }

    /// The key hierarchy of a tenant.
    pub fn keys(&self, tenant: &Tenant) -> Result<Arc<keys::Hierarchy>, BpaError> {
        keys::Hierarchy::open(self.db.clone(), self.root, &tenant.id).map(Arc::new)
    }

    /// The host services of a tenant's VAE: real clock and entropy, the node's connectors
    /// (with the tenant's secrets), no models, the event log, the job table and telemetry.
    pub fn vae_services(&self, tenant: &Tenant, vae_id: &str) -> Result<Services, BpaError> {
        let scoped = Arc::new(Db::tenant(&self.url, &tenant.id)?);
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
        let scoped = Arc::new(Db::tenant(&self.url, &tenant.id)?);
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
        let scoped = Arc::new(Db::tenant(&self.url, &tenant.id)?);
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
        let scoped = Arc::new(Db::tenant(&self.url, &tenant.id)?);
        Ok(keys::SecretStore::new(scoped, self.keys(tenant)?))
    }

    /// The scoped database of a tenant (services and suites).
    pub fn tenant_db(&self, tenant: &Tenant) -> Result<Arc<Db>, BpaError> {
        Db::tenant(&self.url, &tenant.id).map(Arc::new)
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
