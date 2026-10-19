//! PostgreSQL adapter on sqlx (DSN-DVM-001, -004, -005, -006, -007, -010, -402; DEC-047).
//!
//! [`PgBackend`] implements the backend-neutral traits [`StoreRead`] and (through [`PgTx`])
//! [`StoreTx`]. [`PgStore`] is the synchronous engine view ([`Store`]) over it: it runs the
//! asynchronous operations on its own Tokio runtime and maps the engine's records to the
//! kernel rows (canonical bytes, 32-byte content IDs, `cseq` intervals).

use crate::kernel::{
    Allocated, BranchMetaRow, BranchRow, ChangeSetRow, ChunkRow, CommitRow, CounterRow,
    DerivationEdge, EntryFilter, EntryMeta, FieldRow, Hash, HeadRow, IdempotencyRow, IndexRow,
    LedgerRow, NodeRow, OPEN, ObjectFacts, OutboxRow, Placement, ProcessRow, Scope,
    SequenceRequest, StoreRead, StoreTx, TagRow, TemporalRow, TimerRow, VersionRow, hash_of,
    hash_text, kind_code, kind_name,
};
use crate::physical::{BlobStore, Chunks, LocalBlobs, Physical};
use crate::{BranchRef, Flush, Store};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgRow};
use sqlx::{Executor, Postgres, Row, Transaction};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;
use ubos_crypto::{KeyService, LocalKeys};
use ubos_types::{BpaError, canonical};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Tables emptied by `reset` (adapter operation, STD-CONF-002).
const TABLES: &str = "kernel_meta, branch_ref, commit, commit_parent, chunk, object_version, \
    ledger_entry, ledger_sequence, merkle_node, head_index, temporal_index, field_index, \
    derivation_edge, idempotency_record, outbox, tag, process_record, change_set, timer, \
    blob, secret, node_key, event_log, job";

/// Canonical instant text of a `timestamptz` column (NULL stays NULL).
fn instant_text(col: &str) -> String {
    format!("to_char({col} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
}

/// A random UUID (version 4) in text form.
fn random_uuid() -> Result<String, BpaError> {
    let r: [u8; 16] = ubos_crypto::random_bytes()?;
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

/// Canonical text of a valid-time column (`date` or `instant` per the row's granularity;
/// `'infinity'` reads as NULL) — the inverse of the `timestamptz` cast of the input text.
fn time_text(col: &str, granularity: &str) -> String {
    format!(
        "CASE WHEN {col} = 'infinity' THEN NULL WHEN {granularity} = 1 THEN to_char({col} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') ELSE to_char({col} AT TIME ZONE 'UTC', 'YYYY-MM-DD') END"
    )
}

/// Objects of a branch at a commit ordinal: head rows joined with their segments.
fn facts_query(filter: &str) -> String {
    format!(
        "SELECT h.object_id::text, h.version_id, h.class_name, h.kind, h.slug, h.hidden, {vf}, {vt}, t.version_id \
         FROM head_index h LEFT JOIN temporal_index t ON t.tenant_id = h.tenant_id AND t.vae_id = h.vae_id \
         AND t.branch_id = h.branch_id AND t.object_id = h.object_id AND t.cseq_from <= $4 AND t.cseq_to > $4 \
         WHERE h.tenant_id = $1::uuid AND h.vae_id = $2::uuid AND h.branch_id = $3 AND h.cseq_from <= $4 AND h.cseq_to > $4{filter} \
         ORDER BY h.object_id, t.valid_from",
        vf = time_text("t.valid_from", "t.granularity"),
        vt = time_text("t.valid_to", "t.granularity"),
    )
}

/// Group the rows of [`facts_query`] by object.
fn facts_of(rows: &[PgRow]) -> Result<Vec<(String, Option<ObjectFacts>)>, BpaError> {
    let mut out: Vec<(String, Option<ObjectFacts>)> = Vec::new();
    for r in rows {
        let id: String = r.try_get(0).map_err(db)?;
        if out.last().map(|x| x.0.as_str()) != Some(id.as_str()) {
            let head = r
                .try_get::<Option<Vec<u8>>, _>(1)
                .map_err(db)?
                .map(hash_col)
                .transpose()?;
            let hidden: bool = r.try_get(5).map_err(db)?;
            let facts = if hidden {
                None
            } else {
                Some(ObjectFacts {
                    class: r.try_get(2).map_err(db)?,
                    kind: kind_name(
                        u16::try_from(r.try_get::<i16, _>(3).map_err(db)?).unwrap_or_default(),
                    )
                    .into(),
                    slug: r.try_get(4).map_err(db)?,
                    head,
                    segments: Vec::new(),
                })
            };
            out.push((id, facts));
        }
        if let Some((_, Some(f))) = out.last_mut()
            && let Some(v) = r.try_get::<Option<Vec<u8>>, _>(8).map_err(db)?
        {
            f.segments.push((
                r.try_get::<Option<String>, _>(6)
                    .map_err(db)?
                    .unwrap_or_default(),
                r.try_get(7).map_err(db)?,
                hash_col(v)?,
            ));
        }
    }
    Ok(out)
}

/// Default scope of the conformance adapter and the benchmark tools.
pub const DEFAULT_SCOPE: (&str, &str) = (
    "01900000-0000-7000-8000-0000000000a1",
    "01900000-0000-7000-8000-0000000000a2",
);

const ZERO: Hash = [0u8; 32];

fn db(e: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

fn int(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn uint(n: i64) -> u64 {
    u64::try_from(n).unwrap_or_default()
}

fn hash_col(v: Vec<u8>) -> Result<Hash, BpaError> {
    v.try_into().map_err(|_| db("stored hash is not 32 bytes"))
}

/// Hash of a canonical ledger key (the `ledger_key` column).
pub fn key_hash(key: &str) -> Hash {
    Sha256::digest(key.as_bytes()).into()
}

/// Connection options from a `postgres://` URL or a libpq `key=value` string.
pub fn options(url: &str) -> Result<PgConnectOptions, BpaError> {
    if url.contains("://") {
        return PgConnectOptions::from_str(url).map_err(db);
    }
    let mut o = PgConnectOptions::new();
    for part in url.split_whitespace() {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| db(format!("malformed connection parameter {part}")))?;
        o = match k {
            "host" => o.host(v),
            "port" => o.port(v.parse().map_err(db)?),
            "user" => o.username(v),
            "password" => o.password(v),
            "dbname" => o.database(v),
            other => return Err(db(format!("unsupported connection parameter {other}"))),
        };
    }
    Ok(o)
}

/// The PostgreSQL backend of one scope (tenant and VAE).
#[derive(Clone)]
pub struct PgBackend {
    pool: PgPool,
    scope: Scope,
    /// Connection options of the owner role (migrations, bulk loads).
    owner: PgConnectOptions,
}

/// One PostgreSQL transaction ([`StoreTx`]).
pub struct PgTx {
    tx: Transaction<'static, Postgres>,
    scope: Scope,
}

impl PgBackend {
    /// Connect, apply the migrations and fix the scope. Every connection sets
    /// `ubos.tenant` for row-level security (DSN-DVM-010).
    pub async fn connect(url: &str, scope: Scope) -> Result<PgBackend, BpaError> {
        for id in [&scope.tenant, &scope.vae] {
            let ok = id.len() == 36
                && id.chars().enumerate().all(|(i, c)| {
                    matches!(i, 8 | 13 | 18 | 23) == (c == '-')
                        && (c == '-' || c.is_ascii_hexdigit())
                });
            if !ok {
                return Err(db(format!("scope identifier {id} is not a UUID")));
            }
        }
        // Migrations run as the connecting (owner) role; the pool then works as `ubos_app`,
        // so that row-level security applies (DSN-DVM-010, DEC-048).
        {
            use sqlx::Connection;
            let mut owner = sqlx::PgConnection::connect_with(&options(url)?)
                .await
                .map_err(db)?;
            MIGRATOR.run(&mut owner).await.map_err(db)?;
            owner.close().await.map_err(db)?;
        }
        let setup = format!(
            "SET ROLE ubos_app; SET ubos.tenant = '{}'; SET TIME ZONE 'UTC'; SET synchronous_commit = on; SET plan_cache_mode = force_custom_plan;",
            scope.tenant
        );
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |conn, _| {
                let sql = setup.clone();
                Box::pin(async move {
                    conn.execute(sql.as_str()).await?;
                    Ok(())
                })
            })
            .connect_with(options(url)?)
            .await
            .map_err(db)?;
        Ok(PgBackend {
            pool,
            scope,
            owner: options(url)?,
        })
    }

    /// Apply the migrations as the connecting (owner) role without opening a scope: the
    /// node host runs this once at start (DSN-NOD-602), before any tenant is opened.
    pub fn migrate(url: &str) -> Result<(), BpaError> {
        use sqlx::Connection;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(db)?;
        let opts = options(url)?;
        rt.block_on(async move {
            let mut owner = sqlx::PgConnection::connect_with(&opts).await.map_err(db)?;
            MIGRATOR.run(&mut owner).await.map_err(db)?;
            owner.close().await.map_err(db)
        })
    }

    /// The connection pool (for tools such as the benchmark driver).
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The scope.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Begin a transaction.
    pub async fn begin(&self) -> Result<PgTx, BpaError> {
        Ok(PgTx {
            tx: self.pool.begin().await.map_err(db)?,
            scope: self.scope.clone(),
        })
    }

    /// Empty every table of every tenant and restart the commit identifiers (tools only;
    /// the adapter's `reset` moves to a fresh VAE instead).
    pub async fn purge_all(&self) -> Result<(), BpaError> {
        let sql = format!("TRUNCATE {TABLES}; SELECT setval('commit_no_seq', 1, false);");
        self.pool.execute(sql.as_str()).await.map_err(db)?;
        Ok(())
    }

    async fn opt(
        &self,
        q: sqlx::query::Query<'_, Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<Option<PgRow>, BpaError> {
        q.fetch_optional(&self.pool).await.map_err(db)
    }
}

fn branch_row(r: &PgRow) -> Result<BranchRow, BpaError> {
    let parent: Option<i64> = r.try_get("parent_branch").map_err(db)?;
    let fork: Option<i64> = r.try_get("fork_cseq").map_err(db)?;
    Ok(BranchRow {
        branch_id: r.try_get("branch_id").map_err(db)?,
        name: r.try_get("name").map_err(db)?,
        head_no: uint(r.try_get("head_no").map_err(db)?),
        head_commit: r
            .try_get::<Option<Vec<u8>>, _>("head_commit")
            .map_err(db)?
            .map(hash_col)
            .transpose()?,
        head_cseq: uint(r.try_get("head_cseq").map_err(db)?),
        parent: parent.map(|p| (p, uint(fork.unwrap_or_default()))),
        depth: u16::try_from(r.try_get::<i16, _>("depth").map_err(db)?).unwrap_or_default(),
    })
}

const BRANCH_COLS: &str =
    "branch_id, name, head_no, head_commit, head_cseq, parent_branch, fork_cseq, depth";

impl StoreRead for PgBackend {
    async fn branch(&self, name: &str) -> Result<Option<BranchRow>, BpaError> {
        let sql = format!(
            "SELECT {BRANCH_COLS} FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND name = $3"
        );
        let r = self
            .opt(
                sqlx::query(&sql)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(name),
            )
            .await?;
        r.as_ref().map(branch_row).transpose()
    }
    async fn branch_names(&self) -> Result<Vec<String>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT name FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid ORDER BY name")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae),
            )
            .await?;
        rows.iter()
            .map(|r| r.try_get::<String, _>(0).map_err(db))
            .collect()
    }
    async fn commit(&self, commit_no: u64) -> Result<Option<(i64, u64, Vec<u8>)>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT branch_id, cseq, body FROM commit WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND commit_no = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(int(commit_no)),
            )
            .await?;
        r.map(|r| -> Result<_, BpaError> {
            Ok((
                r.try_get(0).map_err(db)?,
                uint(r.try_get(1).map_err(db)?),
                r.try_get(2).map_err(db)?,
            ))
        })
        .transpose()
    }
    async fn commit_at(&self, branch_id: i64, cseq: u64) -> Result<Option<u64>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT commit_no FROM commit WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq = $4")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(int(cseq)),
            )
            .await?;
        r.map(|r| r.try_get::<i64, _>(0).map(uint).map_err(db))
            .transpose()
    }
    async fn commit_numbers(&self, n: u32) -> Result<Vec<u64>, BpaError> {
        let rows = sqlx::query("SELECT nextval('commit_no_seq') FROM generate_series(1, $1)")
            .bind(i32::try_from(n).unwrap_or(i32::MAX))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| r.try_get::<i64, _>(0).map(uint).map_err(db))
            .collect()
    }
    async fn version(&self, id: &Hash) -> Result<Option<Vec<u8>>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT envelope FROM object_version WHERE tenant_id = $1::uuid AND version_id = $2")
                    .bind(&self.scope.tenant)
                    .bind(&id[..]),
            )
            .await?;
        r.map(|r| r.try_get(0).map_err(db)).transpose()
    }
    async fn chunk(&self, id: &Hash) -> Result<Option<ChunkRow>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT placement, size_logical, body FROM chunk WHERE tenant_id = $1::uuid AND content_id = $2")
                    .bind(&self.scope.tenant)
                    .bind(&id[..]),
            )
            .await?;
        r.map(|r| -> Result<ChunkRow, BpaError> {
            let placement = match r.try_get::<i16, _>(0).map_err(db)? {
                0 => Placement::Inline,
                1 => Placement::ObjectStore,
                p => return Err(db(format!("unsupported chunk placement {p}"))),
            };
            Ok(ChunkRow {
                content_id: *id,
                size_logical: u32::try_from(r.try_get::<i32, _>(1).map_err(db)?)
                    .unwrap_or_default(),
                placement,
                body: r.try_get(2).map_err(db)?,
            })
        })
        .transpose()
    }
    async fn node(&self, id: &Hash) -> Result<Option<Vec<u8>>, BpaError> {
        let r = self
            .opt(
                sqlx::query(
                    "SELECT body FROM merkle_node WHERE tenant_id = $1::uuid AND node_id = $2",
                )
                .bind(&self.scope.tenant)
                .bind(&id[..]),
            )
            .await?;
        r.map(|r| r.try_get(0).map_err(db)).transpose()
    }
    /// The field index (DSN-DVM-902): objects of the classes whose head at `cseq` carries a
    /// key in `[lo, hi]`, in key order then object order.
    #[allow(clippy::too_many_arguments)]
    async fn field_scan(
        &self,
        branch_id: i64,
        cseq: u64,
        classes: &[String],
        field: &str,
        lo: Option<&[u8]>,
        hi: Option<&[u8]>,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>, BpaError> {
        let mut sql = String::from(
            "SELECT object_id::text, value_key FROM field_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid \
             AND branch_id = $3 AND class_name = ANY($4::text[]) AND field = $5 AND cseq_from <= $6 AND cseq_to > $6",
        );
        if lo.is_some() {
            sql.push_str(" AND value_key >= $7");
        }
        if hi.is_some() {
            sql.push_str(if lo.is_some() {
                " AND value_key <= $8"
            } else {
                " AND value_key <= $7"
            });
        }
        sql.push_str(" ORDER BY value_key, object_id LIMIT ");
        sql.push_str(&int(limit as u64).to_string());
        let mut q = sqlx::query(&sql)
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(classes.to_vec())
            .bind(field)
            .bind(int(cseq));
        if let Some(l) = lo {
            q = q.bind(l.to_vec());
        }
        if let Some(h) = hi {
            q = q.bind(h.to_vec());
        }
        let rows = self.rows(q).await?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.try_get::<String, _>(0).map_err(db)?,
                    r.try_get::<Vec<u8>, _>(1).map_err(db)?,
                ))
            })
            .collect()
    }
    /// The number of objects present at `cseq`, optionally of one class.
    async fn count_objects(
        &self,
        branch_id: i64,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<u64, BpaError> {
        let base = "SELECT count(*) FROM head_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 \
                    AND cseq_from <= $4 AND cseq_to > $4 AND NOT hidden";
        let q = match class {
            Some(c) => sqlx::query(const_format(base, " AND class_name = $5"))
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(cseq))
                .bind(c),
            None => sqlx::query(base)
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(cseq)),
        };
        let r = q.fetch_one(&self.pool).await.map_err(db)?;
        Ok(uint(r.try_get::<i64, _>(0).map_err(db)?))
    }
    async fn object(
        &self,
        branch_id: i64,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<ObjectFacts>>, BpaError> {
        // Two direct lookups rather than one join: planning a join over the partitioned
        // `temporal_index` costs more than both executions (DEC-053).
        let Some(h) = self
            .opt(
                sqlx::query("SELECT version_id, class_name, kind, slug, hidden FROM head_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = $4::uuid AND cseq_from <= $5 AND cseq_to > $5")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(object_id)
                    .bind(int(cseq)),
            )
            .await?
        else {
            return Ok(None);
        };
        if h.try_get::<bool, _>(4).map_err(db)? {
            return Ok(Some(None));
        }
        static SEGMENTS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "SELECT {vf}, {vt}, version_id FROM temporal_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = $4::uuid AND cseq_from <= $5 AND cseq_to > $5 ORDER BY valid_from",
                vf = time_text("valid_from", "granularity"),
                vt = time_text("valid_to", "granularity"),
            )
        });
        let rows = self
            .rows(
                sqlx::query(&SEGMENTS)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(object_id)
                    .bind(int(cseq)),
            )
            .await?;
        let mut segments = Vec::with_capacity(rows.len());
        for r in &rows {
            segments.push((
                r.try_get::<Option<String>, _>(0)
                    .map_err(db)?
                    .unwrap_or_default(),
                r.try_get(1).map_err(db)?,
                hash_col(r.try_get(2).map_err(db)?)?,
            ));
        }
        Ok(Some(Some(ObjectFacts {
            class: h.try_get(1).map_err(db)?,
            kind: kind_name(u16::try_from(h.try_get::<i16, _>(2).map_err(db)?).unwrap_or_default())
                .into(),
            slug: h.try_get(3).map_err(db)?,
            head: h
                .try_get::<Option<Vec<u8>>, _>(0)
                .map_err(db)?
                .map(hash_col)
                .transpose()?,
            segments,
        })))
    }
    async fn objects(
        &self,
        branch_id: i64,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<ObjectFacts>)>, BpaError> {
        static ALL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| facts_query(""));
        static OF_CLASS: std::sync::LazyLock<String> =
            std::sync::LazyLock::new(|| facts_query(" AND h.class_name = $5"));
        let q = match class {
            Some(c) => sqlx::query(&OF_CLASS)
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(cseq))
                .bind(c),
            None => sqlx::query(&ALL)
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(cseq)),
        };
        let rows = self.rows(q).await?;
        facts_of(&rows)
    }
    async fn objects_by_id(
        &self,
        branch_id: i64,
        cseq: u64,
        ids: &[String],
    ) -> Result<Vec<(String, Option<ObjectFacts>)>, BpaError> {
        // Two lookups by identifier list rather than one join, as in `object` (DEC-053):
        // the planner expects one head row and loops over the whole temporal index.
        let heads = self
            .rows(
                sqlx::query("SELECT object_id::text, version_id, class_name, kind, slug, hidden FROM head_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = ANY($4::uuid[]) AND cseq_from <= $5 AND cseq_to > $5 ORDER BY object_id")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(ids.to_vec())
                    .bind(int(cseq)),
            )
            .await?;
        if heads.is_empty() {
            return Ok(Vec::new());
        }
        static SEGMENTS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "SELECT object_id::text, {vf}, {vt}, version_id FROM temporal_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = ANY($4::uuid[]) AND cseq_from <= $5 AND cseq_to > $5 ORDER BY object_id, valid_from",
                vf = time_text("valid_from", "granularity"),
                vt = time_text("valid_to", "granularity"),
            )
        });
        let segs = self
            .rows(
                sqlx::query(&SEGMENTS)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(ids.to_vec())
                    .bind(int(cseq)),
            )
            .await?;
        let mut out: Vec<(String, Option<ObjectFacts>)> = Vec::with_capacity(heads.len());
        for h in &heads {
            let id: String = h.try_get(0).map_err(db)?;
            let facts = if h.try_get::<bool, _>(5).map_err(db)? {
                None
            } else {
                Some(ObjectFacts {
                    class: h.try_get(2).map_err(db)?,
                    kind: kind_name(
                        u16::try_from(h.try_get::<i16, _>(3).map_err(db)?).unwrap_or_default(),
                    )
                    .into(),
                    slug: h.try_get(4).map_err(db)?,
                    head: h
                        .try_get::<Option<Vec<u8>>, _>(1)
                        .map_err(db)?
                        .map(hash_col)
                        .transpose()?,
                    segments: Vec::new(),
                })
            };
            out.push((id, facts));
        }
        let mut i = 0;
        for r in &segs {
            let id: String = r.try_get(0).map_err(db)?;
            while i < out.len() && out[i].0 < id {
                i += 1;
            }
            if let Some((oid, Some(f))) = out.get_mut(i)
                && *oid == id
            {
                f.segments.push((
                    r.try_get::<Option<String>, _>(1)
                        .map_err(db)?
                        .unwrap_or_default(),
                    r.try_get(2).map_err(db)?,
                    hash_col(r.try_get(3).map_err(db)?)?,
                ));
            }
        }
        Ok(out)
    }
    async fn entries(
        &self,
        branch_id: i64,
        cseq: u64,
        filter: &EntryFilter,
    ) -> Result<Vec<Vec<u8>>, BpaError> {
        const BASE: &str = "SELECT body FROM ledger_entry WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq <= $4";
        let q = match filter {
            EntryFilter::All => sqlx::query(const_format(BASE, " ORDER BY cseq, idx")),
            EntryFilter::Key(..) => sqlx::query(const_format(
                BASE,
                " AND ledger_class = $5 AND ledger_key = $6 ORDER BY cseq, idx",
            )),
            EntryFilter::Id(_) => sqlx::query(const_format(
                BASE,
                " AND entry_id = $5::uuid ORDER BY cseq, idx",
            )),
        }
        .bind(&self.scope.tenant)
        .bind(&self.scope.vae)
        .bind(branch_id)
        .bind(int(cseq));
        let q = match filter {
            EntryFilter::All => q,
            EntryFilter::Key(c, k) => q.bind(c.clone()).bind(k.to_vec()),
            EntryFilter::Id(id) => q.bind(id.clone()),
        };
        let rows = q.fetch_all(&self.pool).await.map_err(db)?;
        rows.iter().map(|r| r.try_get(0).map_err(db)).collect()
    }
    async fn branch_commits(&self, branch_id: i64) -> Result<Vec<(u64, u64)>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT commit_no, cseq FROM commit WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 ORDER BY cseq")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok((
                    uint(r.try_get(0).map_err(db)?),
                    uint(r.try_get(1).map_err(db)?),
                ))
            })
            .collect()
    }
    async fn commit_bodies(&self, branch_id: i64) -> Result<Vec<(u64, Vec<u8>)>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT cseq, body FROM commit WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 ORDER BY cseq")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id),
            )
            .await?;
        rows.iter()
            .map(|r| Ok((uint(r.try_get(0).map_err(db)?), r.try_get(1).map_err(db)?)))
            .collect()
    }
    async fn head_rows(
        &self,
        branch_id: i64,
        from: u64,
        to: u64,
    ) -> Result<Vec<HeadRow>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT object_id::text, cseq_from, cseq_to, version_id, class_name, kind, slug, granularity, hidden FROM head_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq_from BETWEEN $4 AND $5")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(int(from))
                    .bind(int(to)),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(HeadRow {
                    object_id: r.try_get(0).map_err(db)?,
                    cseq_from: uint(r.try_get(1).map_err(db)?),
                    cseq_to: uint(r.try_get(2).map_err(db)?),
                    version: r
                        .try_get::<Option<Vec<u8>>, _>(3)
                        .map_err(db)?
                        .map(hash_col)
                        .transpose()?,
                    class: r.try_get(4).map_err(db)?,
                    kind: kind_name(
                        u16::try_from(r.try_get::<i16, _>(5).map_err(db)?).unwrap_or_default(),
                    )
                    .into(),
                    slug: r.try_get(6).map_err(db)?,
                    instant: r.try_get::<i16, _>(7).map_err(db)? == 1,
                    hidden: r.try_get(8).map_err(db)?,
                })
            })
            .collect()
    }
    async fn temporal_rows(
        &self,
        branch_id: i64,
        from: u64,
        to: u64,
    ) -> Result<Vec<TemporalRow>, BpaError> {
        static SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "SELECT object_id::text, {vf}, {vt}, cseq_from, cseq_to, version_id, class_name FROM temporal_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq_from BETWEEN $4 AND $5",
                vf = time_text("valid_from", "granularity"),
                vt = time_text("valid_to", "granularity"),
            )
        });
        let rows = self
            .rows(
                sqlx::query(&SQL)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(int(from))
                    .bind(int(to)),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(TemporalRow {
                    object_id: r.try_get(0).map_err(db)?,
                    valid_from: r
                        .try_get::<Option<String>, _>(1)
                        .map_err(db)?
                        .unwrap_or_default(),
                    valid_to: r.try_get(2).map_err(db)?,
                    cseq_from: uint(r.try_get(3).map_err(db)?),
                    cseq_to: uint(r.try_get(4).map_err(db)?),
                    version: hash_col(r.try_get(5).map_err(db)?)?,
                    class: r.try_get(6).map_err(db)?,
                })
            })
            .collect()
    }
    async fn entry_meta(&self, branch_id: i64) -> Result<Vec<EntryMeta>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT ledger_class, ledger_key, seq, entry_id::text, prev_hash, entry_hash FROM ledger_entry WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 ORDER BY ledger_class, ledger_key, seq")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(EntryMeta {
                    class: r.try_get(0).map_err(db)?,
                    ledger_key: hash_col(r.try_get(1).map_err(db)?)?,
                    seq: uint(r.try_get(2).map_err(db)?),
                    entry_id: r.try_get(3).map_err(db)?,
                    prev_hash: hash_col(r.try_get(4).map_err(db)?)?,
                    entry_hash: hash_col(r.try_get(5).map_err(db)?)?,
                })
            })
            .collect()
    }
    async fn counters(&self, branch_id: i64) -> Result<Vec<CounterRow>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT ledger_class, ledger_key, next_seq, last_hash FROM ledger_sequence WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(CounterRow {
                    class: r.try_get(0).map_err(db)?,
                    ledger_key: hash_col(r.try_get(1).map_err(db)?)?,
                    next_seq: uint(r.try_get(2).map_err(db)?),
                    last_hash: hash_col(r.try_get(3).map_err(db)?)?,
                })
            })
            .collect()
    }
    async fn inherited(
        &self,
        branch_id: i64,
        class: &str,
        key: &Hash,
    ) -> Result<(u64, Hash), BpaError> {
        let mut conn = self.pool.acquire().await.map_err(db)?;
        inherited_on(&mut conn, &self.scope, branch_id, class, key).await
    }
    async fn meta(&self, key: &str) -> Result<Option<Vec<u8>>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT value::text FROM kernel_meta WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND key = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(key),
            )
            .await?;
        r.map(|r| {
            r.try_get::<String, _>(0)
                .map(String::into_bytes)
                .map_err(db)
        })
        .transpose()
    }

    async fn commit_no_of(&self, commit_id: &Hash) -> Result<Option<u64>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT commit_no FROM commit WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND commit_id = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(&commit_id[..]),
            )
            .await?;
        r.map(|r| r.try_get::<i64, _>(0).map(uint).map_err(db))
            .transpose()
    }
    async fn commit_id_of(&self, commit_no: u64) -> Result<Option<Hash>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT commit_id FROM commit WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND commit_no = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(int(commit_no)),
            )
            .await?;
        r.map(|r| hash_col(r.try_get(0).map_err(db)?)).transpose()
    }
    async fn object_versions(&self, object_id: &str) -> Result<Vec<(Hash, Vec<u8>)>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT version_id, envelope FROM object_version WHERE tenant_id = $1::uuid AND object_id = $2::uuid")
                    .bind(&self.scope.tenant)
                    .bind(object_id),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok((
                    hash_col(r.try_get(0).map_err(db)?)?,
                    r.try_get(1).map_err(db)?,
                ))
            })
            .collect()
    }
    async fn branch_meta(&self, branch_id: i64) -> Result<Option<BranchMetaRow>, BpaError> {
        static SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "SELECT kind, {}, {}, parent_line::text, created_from FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3",
                instant_text("created_at"),
                instant_text("active_at")
            )
        });
        let r = self
            .opt(
                sqlx::query(&SQL)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id),
            )
            .await?;
        r.map(|r| {
            Ok(BranchMetaRow {
                kind: r.try_get(0).map_err(db)?,
                created_at: r.try_get(1).map_err(db)?,
                active_at: r.try_get(2).map_err(db)?,
                parent_line: r
                    .try_get::<Option<String>, _>(3)
                    .map_err(db)?
                    .map(String::into_bytes),
                created_from: r
                    .try_get::<Option<Vec<u8>>, _>(4)
                    .map_err(db)?
                    .map(hash_col)
                    .transpose()?,
            })
        })
        .transpose()
    }
    async fn tag_names(&self) -> Result<Vec<String>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT name FROM tag WHERE tenant_id = $1::uuid AND vae_id = $2::uuid ORDER BY name")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae),
            )
            .await?;
        rows.iter()
            .map(|r| r.try_get::<String, _>(0).map_err(db))
            .collect()
    }
    async fn tag(&self, name: &str) -> Result<Option<TagRow>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT commit_id, branch, signer, signature, tombstoned FROM tag WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND name = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(name),
            )
            .await?;
        r.map(|r| {
            Ok(TagRow {
                name: name.to_string(),
                commit_id: hash_col(r.try_get(0).map_err(db)?)?,
                branch: r.try_get(1).map_err(db)?,
                signer: r.try_get(2).map_err(db)?,
                signature: r.try_get(3).map_err(db)?,
                tombstoned: r.try_get(4).map_err(db)?,
            })
        })
        .transpose()
    }
    async fn process(&self, process_id: &str) -> Result<Option<ProcessRow>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT kind, state, initiator, trigger_kind, trigger_ref, parent_process::text, bulk::text, idempotency_key, effects, commits::text, bulk_state::text, journal_id, metering FROM process_record WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND process_id = $3::uuid")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(process_id),
            )
            .await?;
        r.map(|r| {
            let commits: Vec<(String, u64)> =
                serde_json::from_str::<Vec<(String, u64)>>(&r.try_get::<String, _>(9).map_err(db)?)
                    .map_err(db)?;
            Ok(ProcessRow {
                process_id: process_id.to_string(),
                kind: r.try_get(0).map_err(db)?,
                state: r.try_get(1).map_err(db)?,
                initiator: r.try_get(2).map_err(db)?,
                trigger_kind: r.try_get(3).map_err(db)?,
                trigger_ref: r.try_get(4).map_err(db)?,
                parent_process: r.try_get(5).map_err(db)?,
                bulk: r.try_get(6).map_err(db)?,
                idempotency_key: r.try_get(7).map_err(db)?,
                effects: u32::try_from(r.try_get::<i32, _>(8).map_err(db)?).unwrap_or_default(),
                commits,
                bulk_state: r
                    .try_get::<Option<String>, _>(10)
                    .map_err(db)?
                    .map(String::into_bytes),
                journal_id: r
                    .try_get::<Option<Vec<u8>>, _>(11)
                    .map_err(db)?
                    .map(hash_col)
                    .transpose()?,
                metering: r
                    .try_get::<Option<Vec<u8>>, _>(12)
                    .map_err(db)?
                    .map(hash_col)
                    .transpose()?,
            })
        })
        .transpose()
    }
    async fn process_ids(&self) -> Result<Vec<String>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT process_id::text FROM process_record WHERE tenant_id = $1::uuid AND vae_id = $2::uuid ORDER BY seq")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae),
            )
            .await?;
        rows.iter().map(|r| r.try_get(0).map_err(db)).collect()
    }
    async fn change_set(&self, id: &str) -> Result<Option<ChangeSetRow>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT target, merge_commit FROM change_set WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND change_set_id = $3::uuid")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(id),
            )
            .await?;
        r.map(|r| {
            Ok(ChangeSetRow {
                change_set_id: id.to_string(),
                target: r.try_get(0).map_err(db)?,
                merge_commit: r
                    .try_get::<Option<Vec<u8>>, _>(1)
                    .map_err(db)?
                    .map(hash_col)
                    .transpose()?,
            })
        })
        .transpose()
    }
    async fn derivation_edges(&self, binding_id: &Hash) -> Result<Vec<DerivationEdge>, BpaError> {
        static SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "SELECT output_version, input_object::text, input_version, {}, granularity FROM derivation_edge WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND binding_id = $3 ORDER BY input_object, valid_at, input_version",
                time_text("valid_at", "granularity")
            )
        });
        let rows = self
            .rows(
                sqlx::query(&SQL)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(&binding_id[..]),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(DerivationEdge {
                    binding_id: *binding_id,
                    output_version: hash_col(r.try_get(0).map_err(db)?)?,
                    input_object: r.try_get(1).map_err(db)?,
                    input_version: hash_col(r.try_get(2).map_err(db)?)?,
                    valid_at: r
                        .try_get::<Option<String>, _>(3)
                        .map_err(db)?
                        .unwrap_or_default(),
                    instant: r.try_get::<i16, _>(4).map_err(db)? == 1,
                })
            })
            .collect()
    }
    async fn outbox_pending(&self) -> Result<Vec<Vec<u8>>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT payload FROM outbox WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND released_at IS NULL ORDER BY id")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae),
            )
            .await?;
        rows.iter().map(|r| r.try_get(0).map_err(db)).collect()
    }
    async fn outbox_of_commit(&self, branch_id: i64, cseq: i64) -> Result<Vec<Vec<u8>>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT payload FROM outbox WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq = $4 ORDER BY id")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(cseq),
            )
            .await?;
        rows.iter().map(|r| r.try_get(0).map_err(db)).collect()
    }
    async fn timers(&self) -> Result<Vec<TimerRow>, BpaError> {
        static SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "SELECT id::text, {}, (step->>'instant')::boolean, target::text, step::text FROM timer WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND fired_at IS NULL ORDER BY due_at, id",
                time_text(
                    "due_at",
                    "CASE WHEN (step->>'instant')::boolean THEN 1 ELSE 0 END"
                )
            )
        });
        let rows = self
            .rows(
                sqlx::query(&SQL)
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(TimerRow {
                    id: r.try_get(0).map_err(db)?,
                    due_at: r
                        .try_get::<Option<String>, _>(1)
                        .map_err(db)?
                        .unwrap_or_default(),
                    instant: r
                        .try_get::<Option<bool>, _>(2)
                        .map_err(db)?
                        .unwrap_or(false),
                    target: r.try_get(3).map_err(db)?,
                    step: r.try_get::<String, _>(4).map_err(db)?.into_bytes(),
                })
            })
            .collect()
    }
    async fn idempotency(
        &self,
        scope: &str,
        key: &str,
    ) -> Result<Option<(Hash, Option<Vec<u8>>)>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT request_hash, outcome FROM idempotency_record WHERE tenant_id = $1::uuid AND scope = $2 AND key = $3")
                    .bind(&self.scope.tenant)
                    .bind(scope)
                    .bind(key),
            )
            .await?;
        r.map(|r| {
            Ok((
                hash_col(r.try_get(0).map_err(db)?)?,
                r.try_get(1).map_err(db)?,
            ))
        })
        .transpose()
    }
}

impl PgBackend {
    /// The rows of the objects of `class` whose row visible at `cseq` carries `slug`
    /// (index `head_slug`, migration 0016; PH-2 step 0b).
    async fn objects_by_slug(
        &self,
        branch_id: i64,
        cseq: u64,
        class: &str,
        slug: &str,
    ) -> Result<Vec<(String, Option<ObjectFacts>)>, BpaError> {
        static BY_SLUG: std::sync::LazyLock<String> =
            std::sync::LazyLock::new(|| facts_query(" AND h.class_name = $5 AND h.slug = $6"));
        let q = sqlx::query(&BY_SLUG)
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(int(cseq))
            .bind(class)
            .bind(slug);
        let rows = self.rows(q).await?;
        facts_of(&rows)
    }

    async fn objects_by_slugs(
        &self,
        branch_id: i64,
        cseq: u64,
        class: &str,
        slugs: &[String],
    ) -> Result<Vec<(String, Option<ObjectFacts>)>, BpaError> {
        static BY_SLUGS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            facts_query(" AND h.class_name = $5 AND h.slug = ANY($6::text[])")
        });
        let q = sqlx::query(&BY_SLUGS)
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(int(cseq))
            .bind(class)
            .bind(slugs.to_vec());
        let rows = self.rows(q).await?;
        facts_of(&rows)
    }

    async fn field_keys(
        &self,
        branch_id: i64,
        cseq: u64,
        classes: &[String],
        field: &str,
        keys: &[Vec<u8>],
    ) -> Result<Vec<(String, Vec<u8>)>, BpaError> {
        let q = sqlx::query(
            "SELECT object_id::text, value_key FROM field_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid \
             AND branch_id = $3 AND class_name = ANY($4::text[]) AND field = $5 AND value_key = ANY($6::bytea[]) \
             AND cseq_from <= $7 AND cseq_to > $7",
        )
        .bind(&self.scope.tenant)
        .bind(&self.scope.vae)
        .bind(branch_id)
        .bind(classes.to_vec())
        .bind(field)
        .bind(keys.to_vec())
        .bind(int(cseq));
        let rows = self.rows(q).await?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.try_get::<String, _>(0).map_err(db)?,
                    r.try_get::<Vec<u8>, _>(1).map_err(db)?,
                ))
            })
            .collect()
    }

    async fn rows(
        &self,
        q: sqlx::query::Query<'_, Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<Vec<PgRow>, BpaError> {
        q.fetch_all(&self.pool).await.map_err(db)
    }
}

/// The ledger queries differ only in their tail; the combined texts are leaked once each.
fn const_format(base: &'static str, tail: &'static str) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    let mut m = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    m.entry(tail)
        .or_insert_with(|| Box::leak(format!("{base}{tail}").into_boxed_str()))
}

/// Entries a branch inherits for one ledger key through its read-through parents: their
/// number and the hash of the last one (DSN-DVM-402).
async fn inherited_on(
    conn: &mut sqlx::PgConnection,
    scope: &Scope,
    branch_id: i64,
    class: &str,
    key: &Hash,
) -> Result<(u64, Hash), BpaError> {
    // Entries a new branch inherits through its read-through parents (DSN-DVM-402): the
    // counter of a key starts after them.
    let mut count = 0u64;
    let mut last: Option<Hash> = None;
    let mut cur = sqlx::query("SELECT parent_branch, fork_cseq FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
        .bind(&scope.tenant)
        .bind(&scope.vae)
        .bind(branch_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(db)?
        .and_then(|r| {
            let p: Option<i64> = r.try_get(0).ok()?;
            let f: Option<i64> = r.try_get(1).ok()?;
            Some((p?, f.unwrap_or_default()))
        });
    while let Some((pid, fork)) = cur {
        let r = sqlx::query("SELECT count(*), (SELECT entry_hash FROM ledger_entry WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND ledger_class = $4 AND ledger_key = $5 AND cseq <= $6 ORDER BY seq DESC LIMIT 1) FROM ledger_entry WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND ledger_class = $4 AND ledger_key = $5 AND cseq <= $6")
            .bind(&scope.tenant)
            .bind(&scope.vae)
            .bind(pid)
            .bind(class)
            .bind(&key[..])
            .bind(fork)
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
        count += uint(r.try_get(0).map_err(db)?);
        if last.is_none()
            && let Some(h) = r.try_get::<Option<Vec<u8>>, _>(1).map_err(db)?
        {
            last = Some(hash_col(h)?);
        }
        cur = sqlx::query("SELECT parent_branch, fork_cseq FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
            .bind(&scope.tenant)
            .bind(&scope.vae)
            .bind(pid)
            .fetch_optional(&mut *conn)
            .await
            .map_err(db)?
            .and_then(|r| {
                let p: Option<i64> = r.try_get(0).ok()?;
                let f: Option<i64> = r.try_get(1).ok()?;
                Some((p?, f.unwrap_or_default()))
            });
    }
    Ok((count, last.unwrap_or(ZERO)))
}

impl PgTx {
    async fn chain_count(
        &mut self,
        branch_id: i64,
        class: &str,
        key: &Hash,
    ) -> Result<(u64, Hash), BpaError> {
        inherited_on(&mut self.tx, &self.scope, branch_id, class, key).await
    }
}

impl StoreTx for PgTx {
    async fn lock_branch(&mut self, name: &str) -> Result<Option<BranchRow>, BpaError> {
        let sql = format!(
            "SELECT {BRANCH_COLS} FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND name = $3 FOR UPDATE"
        );
        let r = sqlx::query(&sql)
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(name)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db)?;
        r.as_ref().map(branch_row).transpose()
    }
    async fn create_branch(&mut self, row: &BranchRow) -> Result<Option<i64>, BpaError> {
        let (parent, fork) = match row.parent {
            Some((p, f)) => (Some(p), Some(int(f))),
            None => (None, None),
        };
        let r = sqlx::query("INSERT INTO branch_ref (tenant_id, vae_id, branch_id, name, head_no, head_cseq, parent_branch, fork_cseq, depth) SELECT $1::uuid, $2::uuid, coalesce(max(branch_id), 0) + 1, $3, $4, $5, $6, $7, $8 FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid ON CONFLICT DO NOTHING RETURNING branch_id")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(&row.name)
            .bind(int(row.head_no))
            .bind(int(row.head_cseq))
            .bind(parent)
            .bind(fork)
            .bind(i16::try_from(row.depth).unwrap_or(i16::MAX))
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db)?;
        r.map(|r| r.try_get(0).map_err(db)).transpose()
    }
    async fn put_chunks(&mut self, rows: &[ChunkRow]) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let ids: Vec<Vec<u8>> = rows.iter().map(|r| r.content_id.to_vec()).collect();
        let placement: Vec<i16> = rows
            .iter()
            .map(|r| match r.placement {
                Placement::Inline => 0,
                Placement::ObjectStore => 1,
            })
            .collect();
        let size: Vec<i32> = rows
            .iter()
            .map(|r| i32::try_from(r.size_logical).unwrap_or(i32::MAX))
            .collect();
        let body: Vec<Option<Vec<u8>>> = rows.iter().map(|r| r.body.clone()).collect();
        sqlx::query("INSERT INTO chunk (tenant_id, content_id, placement, size_logical, body) SELECT $1::uuid, t.i, t.p, t.s, t.b FROM unnest($2::bytea[], $3::smallint[], $4::integer[], $5::bytea[]) AS t(i, p, s, b) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(ids)
            .bind(placement)
            .bind(size)
            .bind(body)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_versions(&mut self, rows: &[VersionRow]) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let ids: Vec<Vec<u8>> = rows.iter().map(|r| r.version_id.to_vec()).collect();
        let oids: Vec<&str> = rows.iter().map(|r| r.object_id.as_str()).collect();
        let classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
        let cv: Vec<Vec<u8>> = rows.iter().map(|r| r.class_version.to_vec()).collect();
        let kinds: Vec<i16> = rows
            .iter()
            .map(|r| i16::try_from(r.kind).unwrap_or_default())
            .collect();
        let from: Vec<&str> = rows.iter().map(|r| r.valid_from.as_str()).collect();
        let to: Vec<Option<&str>> = rows.iter().map(|r| r.valid_to.as_deref()).collect();
        let env: Vec<&[u8]> = rows.iter().map(|r| r.envelope.as_slice()).collect();
        sqlx::query("INSERT INTO object_version (tenant_id, version_id, object_id, vae_id, class_name, class_version, kind, valid_from, valid_to, envelope) SELECT $1::uuid, t.i, t.o::uuid, $2::uuid, t.c, t.cv, t.k, t.f::timestamptz, coalesce(t.t::timestamptz, 'infinity'), t.e FROM unnest($3::bytea[], $4::text[], $5::text[], $6::bytea[], $7::smallint[], $8::text[], $9::text[], $10::bytea[]) AS t(i, o, c, cv, k, f, t, e) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(ids)
            .bind(oids)
            .bind(classes)
            .bind(cv)
            .bind(kinds)
            .bind(from)
            .bind(to)
            .bind(env)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_entries(&mut self, branch_id: i64, rows: &[LedgerRow]) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let class: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
        let key: Vec<Vec<u8>> = rows.iter().map(|r| r.ledger_key.to_vec()).collect();
        let seq: Vec<i64> = rows.iter().map(|r| int(r.seq)).collect();
        let id: Vec<&str> = rows.iter().map(|r| r.entry_id.as_str()).collect();
        let at: Vec<&str> = rows.iter().map(|r| r.valid_at.as_str()).collect();
        let cseq: Vec<i64> = rows.iter().map(|r| int(r.cseq)).collect();
        let idx: Vec<i32> = rows
            .iter()
            .map(|r| i32::try_from(r.idx).unwrap_or(i32::MAX))
            .collect();
        let prev: Vec<Vec<u8>> = rows.iter().map(|r| r.prev_hash.to_vec()).collect();
        let eh: Vec<Vec<u8>> = rows.iter().map(|r| r.entry_hash.to_vec()).collect();
        let body: Vec<&[u8]> = rows.iter().map(|r| r.body.as_slice()).collect();
        sqlx::query("INSERT INTO ledger_entry (tenant_id, vae_id, branch_id, ledger_class, ledger_key, seq, entry_id, valid_at, cseq, idx, prev_hash, entry_hash, body) SELECT $1::uuid, $2::uuid, $3, t.c, t.k, t.s, t.i::uuid, t.a::timestamptz, t.q, t.x, t.p, t.h, t.b FROM unnest($4::text[], $5::bytea[], $6::bigint[], $7::text[], $8::text[], $9::bigint[], $10::integer[], $11::bytea[], $12::bytea[], $13::bytea[]) AS t(c, k, s, i, a, q, x, p, h, b)")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(class)
            .bind(key)
            .bind(seq)
            .bind(id)
            .bind(at)
            .bind(cseq)
            .bind(idx)
            .bind(prev)
            .bind(eh)
            .bind(body)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_commit(&mut self, branch_id: i64, row: &CommitRow) -> Result<(), BpaError> {
        sqlx::query("INSERT INTO commit (tenant_id, commit_id, vae_id, branch_id, cseq, commit_no, tx_time, state_root, process_id, author, body) VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, $7::timestamptz, $8, $9::uuid, $10, $11)")
            .bind(&self.scope.tenant)
            .bind(&row.commit_id[..])
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(int(row.cseq))
            .bind(int(row.commit_no))
            .bind(row.tx_time.as_deref())
            .bind(&row.state_root[..])
            .bind(row.process_id.as_deref())
            .bind(row.author.as_deref())
            .bind(&row.body)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        if !row.parents.is_empty() {
            let ord: Vec<i16> = (0..row.parents.len())
                .map(|i| i16::try_from(i).unwrap_or(i16::MAX))
                .collect();
            let parents: Vec<i64> = row.parents.iter().map(|p| int(*p)).collect();
            // Parents are recorded by content ID (UBS-SYS-DVM-04); the engine names them
            // by handle.
            let r = sqlx::query("INSERT INTO commit_parent (tenant_id, vae_id, commit_id, ord, parent_id) SELECT $1::uuid, $2::uuid, $3, t.o, c.commit_id FROM unnest($4::smallint[], $5::bigint[]) AS t(o, p) JOIN commit c ON c.tenant_id = $1::uuid AND c.vae_id = $2::uuid AND c.commit_no = t.p")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(&row.commit_id[..])
                .bind(ord)
                .bind(parents)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
            if r.rows_affected() != row.parents.len() as u64 {
                return Err(db("a parent commit is unknown"));
            }
        }
        Ok(())
    }
    async fn cas_branch_head(
        &mut self,
        branch_id: i64,
        expected: u64,
        head_no: u64,
        head_commit: &Hash,
        head_cseq: u64,
    ) -> Result<bool, BpaError> {
        let r = sqlx::query("UPDATE branch_ref SET head_no = $4, head_commit = $5, head_cseq = $6 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND head_no = $7")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(int(head_no))
            .bind(&head_commit[..])
            .bind(int(head_cseq))
            .bind(int(expected))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() == 1)
    }
    /// Field index rows of a flush (DSN-DVM-902): the open rows of every changed object
    /// close at `cseq`, the rows of the new heads open at `cseq`.
    async fn upsert_field_rows(
        &mut self,
        branch_id: i64,
        cseq: u64,
        changed: &[String],
        rows: &[FieldRow],
    ) -> Result<(), BpaError> {
        if changed.is_empty() {
            return Ok(());
        }
        let (t, v, c) = (&self.scope.tenant, &self.scope.vae, int(cseq));
        sqlx::query("UPDATE field_index SET cseq_to = $4 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = ANY($5::uuid[]) AND cseq_to = $6")
            .bind(t)
            .bind(v)
            .bind(branch_id)
            .bind(c)
            .bind(changed.to_vec())
            .bind(int(OPEN))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        if rows.is_empty() {
            return Ok(());
        }
        let ids: Vec<&str> = rows.iter().map(|r| r.object_id.as_str()).collect();
        let classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
        let fields: Vec<&str> = rows.iter().map(|r| r.field.as_str()).collect();
        let keys: Vec<Vec<u8>> = rows.iter().map(|r| r.key.clone()).collect();
        sqlx::query("INSERT INTO field_index (tenant_id, vae_id, branch_id, class_name, field, value_key, object_id, cseq_from, cseq_to) SELECT $1::uuid, $2::uuid, $3, t.c, t.f, t.k, t.o::uuid, $4, $5 FROM unnest($6::text[], $7::text[], $8::bytea[], $9::text[]) AS t(c, f, k, o)")
            .bind(t)
            .bind(v)
            .bind(branch_id)
            .bind(c)
            .bind(int(OPEN))
            .bind(&classes)
            .bind(&fields)
            .bind(keys)
            .bind(&ids)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn upsert_index_rows(
        &mut self,
        branch_id: i64,
        cseq: u64,
        rows: &[IndexRow],
    ) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let (t, v, c) = (&self.scope.tenant, &self.scope.vae, int(cseq));
        let ids: Vec<&str> = rows.iter().map(|r| r.object_id.as_str()).collect();
        let classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
        let open = int(OPEN);
        for table in ["head_index", "temporal_index"] {
            let sql = format!(
                "UPDATE {table} SET cseq_to = $4 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = ANY($5::uuid[]) AND cseq_to = $6"
            );
            sqlx::query(&sql)
                .bind(t)
                .bind(v)
                .bind(branch_id)
                .bind(c)
                .bind(&ids)
                .bind(open)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
        }
        let heads: Vec<Option<Vec<u8>>> = rows.iter().map(|r| r.head.map(|h| h.to_vec())).collect();
        let kinds: Vec<i16> = rows
            .iter()
            .map(|r| i16::try_from(kind_code(&r.kind)).unwrap_or_default())
            .collect();
        let slugs: Vec<&str> = rows.iter().map(|r| r.slug.as_str()).collect();
        let gran: Vec<i16> = rows.iter().map(|r| i16::from(r.instant)).collect();
        let hidden: Vec<bool> = rows.iter().map(|r| !r.present).collect();
        sqlx::query("INSERT INTO head_index (tenant_id, vae_id, branch_id, object_id, cseq_from, cseq_to, version_id, class_name, kind, slug, granularity, hidden) SELECT $1::uuid, $2::uuid, $3, t.o::uuid, $4, $5, t.h, t.c, t.k, t.s, t.g, t.x FROM unnest($6::text[], $7::bytea[], $8::text[], $9::smallint[], $10::text[], $11::smallint[], $12::boolean[]) AS t(o, h, c, k, s, g, x)")
            .bind(t)
            .bind(v)
            .bind(branch_id)
            .bind(c)
            .bind(open)
            .bind(&ids)
            .bind(heads)
            .bind(&classes)
            .bind(kinds)
            .bind(slugs)
            .bind(&gran)
            .bind(hidden)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        let (mut so, mut sc, mut sf, mut st, mut sv, mut sg) =
            (vec![], vec![], vec![], vec![], vec![], vec![]);
        for r in rows {
            for (from, to, ver) in &r.segments {
                so.push(r.object_id.as_str());
                sc.push(r.class.as_str());
                sf.push(from.as_str());
                st.push(to.as_deref());
                sv.push(ver.to_vec());
                sg.push(i16::from(r.instant));
            }
        }
        if !so.is_empty() {
            sqlx::query("INSERT INTO temporal_index (tenant_id, vae_id, branch_id, object_id, valid_from, valid_to, cseq_from, cseq_to, version_id, class_name, granularity) SELECT $1::uuid, $2::uuid, $3, t.o::uuid, t.f::timestamptz, coalesce(t.t::timestamptz, 'infinity'), $4, $5, t.v, t.c, t.g FROM unnest($6::text[], $7::text[], $8::text[], $9::bytea[], $10::text[], $11::smallint[]) AS t(o, f, t, v, c, g)")
                .bind(t)
                .bind(v)
                .bind(branch_id)
                .bind(c)
                .bind(open)
                .bind(so)
                .bind(sf)
                .bind(st)
                .bind(sv)
                .bind(sc)
                .bind(sg)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }
    async fn put_tree_nodes(&mut self, rows: &[NodeRow]) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let ids: Vec<Vec<u8>> = rows.iter().map(|r| r.node_id.to_vec()).collect();
        let lv: Vec<i16> = rows
            .iter()
            .map(|r| i16::try_from(r.level).unwrap_or(i16::MAX))
            .collect();
        let body: Vec<&[u8]> = rows.iter().map(|r| r.body.as_slice()).collect();
        sqlx::query("INSERT INTO merkle_node (tenant_id, node_id, level, body) SELECT $1::uuid, t.i, t.l, t.b FROM unnest($2::bytea[], $3::smallint[], $4::bytea[]) AS t(i, l, b) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(ids)
            .bind(lv)
            .bind(body)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn alloc_sequences(
        &mut self,
        branch_id: i64,
        requests: &[SequenceRequest],
    ) -> Result<Vec<Allocated>, BpaError> {
        let mut out = Vec::with_capacity(requests.len());
        for q in requests {
            let n = q.entry_hashes.len() as u64;
            let Some(last) = q.entry_hashes.last() else {
                out.push(Allocated {
                    first_seq: 0,
                    prev_hash: ZERO,
                });
                continue;
            };
            let row = sqlx::query("UPDATE ledger_sequence SET next_seq = next_seq + $6, last_hash = $7 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND ledger_class = $4 AND ledger_key = $5 RETURNING next_seq - $6, (SELECT s.last_hash FROM ledger_sequence s WHERE s.tenant_id = $1::uuid AND s.vae_id = $2::uuid AND s.branch_id = $3 AND s.ledger_class = $4 AND s.ledger_key = $5)")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(&q.class)
                .bind(&q.ledger_key[..])
                .bind(int(n))
                .bind(&last[..])
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(db)?;
            if let Some(r) = row {
                out.push(Allocated {
                    first_seq: uint(r.try_get(0).map_err(db)?),
                    prev_hash: hash_col(r.try_get(1).map_err(db)?)?,
                });
                continue;
            }
            let (inherited, prev) = self.chain_count(branch_id, &q.class, &q.ledger_key).await?;
            sqlx::query("INSERT INTO ledger_sequence (tenant_id, vae_id, branch_id, ledger_class, ledger_key, next_seq, last_hash) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7)")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(&q.class)
                .bind(&q.ledger_key[..])
                .bind(int(inherited + n + 1))
                .bind(&last[..])
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
            out.push(Allocated {
                first_seq: inherited + 1,
                prev_hash: prev,
            });
        }
        Ok(out)
    }
    async fn outbox_append(&mut self, branch_id: i64, rows: &[OutboxRow]) -> Result<(), BpaError> {
        for r in rows {
            sqlx::query("INSERT INTO outbox (tenant_id, vae_id, branch_id, cseq, idx, kind, aggregate_key, payload, effect_id) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9)")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(r.cseq))
                .bind(i32::try_from(r.idx).unwrap_or(i32::MAX))
                .bind(i16::try_from(r.kind).unwrap_or(i16::MAX))
                .bind(r.aggregate_key.as_deref())
                .bind(&r.payload)
                .bind(&r.effect_id)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }
    async fn idempotency_put(&mut self, row: &IdempotencyRow) -> Result<bool, BpaError> {
        let r = sqlx::query("INSERT INTO idempotency_record (tenant_id, scope, key, request_hash, outcome, commit_id) VALUES ($1::uuid, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(&row.scope)
            .bind(&row.key)
            .bind(&row.request_hash[..])
            .bind(row.outcome.as_deref())
            .bind(row.commit_id.map(|h| h.to_vec()))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() == 1)
    }
    async fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<(), BpaError> {
        let text = std::str::from_utf8(value).map_err(db)?;
        sqlx::query("INSERT INTO kernel_meta (tenant_id, vae_id, key, value) VALUES ($1::uuid, $2::uuid, $3, $4::jsonb) ON CONFLICT (tenant_id, vae_id, key) DO UPDATE SET value = EXCLUDED.value")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(key)
            .bind(text)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_branch_meta(
        &mut self,
        branch_id: i64,
        row: &BranchMetaRow,
    ) -> Result<(), BpaError> {
        let pl = row
            .parent_line
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned());
        sqlx::query("UPDATE branch_ref SET kind = $4, created_at = $5::timestamptz, active_at = $6::timestamptz, parent_line = $7::jsonb, created_from = $8 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(&row.kind)
            .bind(row.created_at.as_deref())
            .bind(row.active_at.as_deref())
            .bind(pl)
            .bind(row.created_from.map(|h| h.to_vec()))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_tag(&mut self, row: &TagRow) -> Result<(), BpaError> {
        sqlx::query("INSERT INTO tag (tenant_id, vae_id, name, commit_id, branch, signer, signature, tombstoned) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8) ON CONFLICT (tenant_id, vae_id, name) DO UPDATE SET commit_id = EXCLUDED.commit_id, branch = EXCLUDED.branch, signer = EXCLUDED.signer, signature = EXCLUDED.signature, tombstoned = EXCLUDED.tombstoned")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(&row.name)
            .bind(&row.commit_id[..])
            .bind(&row.branch)
            .bind(&row.signer)
            .bind(&row.signature)
            .bind(row.tombstoned)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_process(&mut self, row: &ProcessRow) -> Result<(), BpaError> {
        let commits = serde_json::to_string(&row.commits).map_err(db)?;
        let bulk_state = row
            .bulk_state
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned());
        sqlx::query("INSERT INTO process_record (tenant_id, process_id, vae_id, kind, state, initiator, trigger_kind, trigger_ref, parent_process, bulk, idempotency_key, effects, commits, bulk_state, journal_id, metering) VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9::uuid, $10::uuid, $11, $12, $13::jsonb, $14::jsonb, $15, $16) ON CONFLICT (tenant_id, vae_id, process_id) DO UPDATE SET state = EXCLUDED.state, effects = EXCLUDED.effects, commits = EXCLUDED.commits, bulk_state = EXCLUDED.bulk_state, journal_id = EXCLUDED.journal_id, metering = EXCLUDED.metering")
            .bind(&self.scope.tenant)
            .bind(&row.process_id)
            .bind(&self.scope.vae)
            .bind(&row.kind)
            .bind(&row.state)
            .bind(&row.initiator)
            .bind(&row.trigger_kind)
            .bind(row.trigger_ref.as_deref())
            .bind(row.parent_process.as_deref())
            .bind(row.bulk.as_deref())
            .bind(row.idempotency_key.as_deref())
            .bind(i32::try_from(row.effects).unwrap_or(i32::MAX))
            .bind(commits)
            .bind(bulk_state)
            .bind(row.journal_id.as_ref().map(|h| h.to_vec()))
            .bind(row.metering.as_ref().map(|h| h.to_vec()))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_change_set(&mut self, row: &ChangeSetRow) -> Result<(), BpaError> {
        sqlx::query("INSERT INTO change_set (tenant_id, vae_id, change_set_id, target, merge_commit) VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5) ON CONFLICT (tenant_id, vae_id, change_set_id) DO UPDATE SET target = EXCLUDED.target, merge_commit = EXCLUDED.merge_commit")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(&row.change_set_id)
            .bind(&row.target)
            .bind(row.merge_commit.map(|h| h.to_vec()))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_derivation_edges(&mut self, rows: &[DerivationEdge]) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let b: Vec<Vec<u8>> = rows.iter().map(|r| r.binding_id.to_vec()).collect();
        let o: Vec<Vec<u8>> = rows.iter().map(|r| r.output_version.to_vec()).collect();
        let io: Vec<&str> = rows.iter().map(|r| r.input_object.as_str()).collect();
        let iv: Vec<Vec<u8>> = rows.iter().map(|r| r.input_version.to_vec()).collect();
        let at: Vec<&str> = rows.iter().map(|r| r.valid_at.as_str()).collect();
        let g: Vec<i16> = rows.iter().map(|r| i16::from(r.instant)).collect();
        sqlx::query("INSERT INTO derivation_edge (tenant_id, vae_id, binding_id, output_version, input_object, input_version, valid_at, granularity) SELECT $1::uuid, $2::uuid, t.b, t.o, t.io::uuid, t.iv, t.a::timestamptz, t.g FROM unnest($3::bytea[], $4::bytea[], $5::text[], $6::bytea[], $7::text[], $8::smallint[]) AS t(b, o, io, iv, a, g) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(b)
            .bind(o)
            .bind(io)
            .bind(iv)
            .bind(at)
            .bind(g)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn put_timers(&mut self, rows: &[TimerRow]) -> Result<(), BpaError> {
        for r in rows {
            let step = String::from_utf8_lossy(&r.step).into_owned();
            sqlx::query("INSERT INTO timer (tenant_id, vae_id, id, due_at, target, step) VALUES ($1::uuid, $2::uuid, $3::uuid, $4::timestamptz, $5::uuid, $6::jsonb) ON CONFLICT DO NOTHING")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(&r.id)
                .bind(&r.due_at)
                .bind(&r.target)
                .bind(step)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }
    async fn fire_timers(&mut self, ids: &[String]) -> Result<(), BpaError> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query("UPDATE timer SET fired_at = now() WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND id = ANY($3::uuid[])")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(ids)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn outbox_claim(
        &mut self,
        holder: &str,
        lease_ms: i64,
        n: usize,
    ) -> Result<Vec<(Vec<u8>, i64)>, BpaError> {
        // the pending rows in release order, locked: releasers serialise here, which keeps
        // the per-object order (a row skipped by SKIP LOCKED could be overtaken)
        let rows = sqlx::query("SELECT id, payload, coalesce(claimed_by, ''), coalesce(claim_until > now(), false) FROM outbox WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND released_at IS NULL ORDER BY id LIMIT 10000 FOR UPDATE")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db)?;
        let mut blocked: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut pick: Vec<i64> = Vec::new();
        for r in &rows {
            if pick.len() >= n {
                break;
            }
            let id: i64 = r.try_get(0).map_err(db)?;
            let payload: Vec<u8> = r.try_get(1).map_err(db)?;
            let by: String = r.try_get(2).map_err(db)?;
            let live: bool = r.try_get(3).map_err(db)?;
            let v: Value = serde_json::from_slice(&payload).unwrap_or(Value::Null);
            let key = v
                .get("object_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("#{id}"));
            if live && by != holder {
                blocked.insert(key);
                continue;
            }
            if blocked.contains(&key) {
                continue;
            }
            pick.push(id);
        }
        if pick.is_empty() {
            return Ok(Vec::new());
        }
        let out = sqlx::query("UPDATE outbox SET claimed_by = $3, claim_until = now() + make_interval(secs => $4::float8 / 1000), attempts = attempts + 1 WHERE tenant_id = $1::uuid AND id = ANY($5::bigint[]) RETURNING id, payload, attempts")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(holder)
            .bind(lease_ms as f64)
            .bind(&pick)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db)?;
        let mut rows: Vec<(i64, Vec<u8>, i64)> = Vec::new();
        for r in out {
            let id: i64 = r.try_get(0).map_err(db)?;
            let p: Vec<u8> = r.try_get(1).map_err(db)?;
            let a: i32 = r.try_get(2).map_err(db)?;
            rows.push((id, p, i64::from(a)));
        }
        rows.sort_by_key(|x| x.0);
        Ok(rows.into_iter().map(|(_, p, a)| (p, a)).collect())
    }
    async fn outbox_ack(&mut self, ids: &[String]) -> Result<bool, BpaError> {
        let n: i64 = sqlx::query("SELECT count(*) FROM outbox WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND effect_id = ANY($3::text[])")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(ids)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db)?
            .try_get(0)
            .map_err(db)?;
        let distinct: std::collections::BTreeSet<&String> = ids.iter().collect();
        if uint(n) != distinct.len() as u64 {
            return Ok(false);
        }
        sqlx::query("UPDATE outbox SET released_at = now() WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND effect_id = ANY($3::text[]) AND released_at IS NULL")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(ids)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(true)
    }
    async fn commit(self) -> Result<(), BpaError> {
        self.tx.commit().await.map_err(db)
    }
    async fn rollback(self) -> Result<(), BpaError> {
        self.tx.rollback().await.map_err(db)
    }
}

// ---- engine view --------------------------------------------------------------------

fn bytes(v: &Value) -> Result<Vec<u8>, BpaError> {
    Ok(canonical(v)?.into_bytes())
}

fn json(b: &[u8]) -> Result<Value, BpaError> {
    serde_json::from_slice(b).map_err(db)
}

/// Rows for a bulk load of one branch's history (benchmark datasets, PH-0 step 5).
#[derive(Debug, Default)]
pub struct BulkRows {
    /// `(hash, record)` of version records.
    pub versions: Vec<(String, Value)>,
    /// `(cseq_from, cseq_to, row)` object rows.
    pub objects: Vec<(u64, Option<u64>, crate::ObjectRow)>,
    /// `(hash, node)` tree nodes.
    pub nodes: Vec<(String, Value)>,
}

/// Entries kept per cache before it is cleared.
/// Approximate byte budget of each in-process cache of immutable records (versions, commits,
/// tree nodes); a count bound let long-lived writers grow to gigabytes (PH-1 step 8b,
/// DEC-061).
const CACHE_BYTES: usize = 64 << 20;

/// Approximate heap size of a cached JSON value.
fn approx_size(v: &Value) -> usize {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => 16,
        Value::String(s) => 24 + s.len(),
        Value::Array(a) => 24 + a.iter().map(approx_size).sum::<usize>(),
        Value::Object(m) => {
            32 + m
                .iter()
                .map(|(k, x)| 40 + k.len() + approx_size(x))
                .sum::<usize>()
        }
    }
}

/// A bounded cache of immutable records: cleared when its approximate size passes the budget.
#[derive(Default)]
struct Cache<K> {
    map: HashMap<K, Value>,
    bytes: usize,
}

impl<K: std::hash::Hash + Eq> Cache<K> {
    fn get<Q: ?Sized + std::hash::Hash + Eq>(&self, k: &Q) -> Option<&Value>
    where
        K: std::borrow::Borrow<Q>,
    {
        self.map.get(k)
    }
    fn clear(&mut self) {
        self.map.clear();
        self.bytes = 0;
    }
}

fn remember<K: std::hash::Hash + Eq>(cache: &mut Cache<K>, k: K, v: &Value) {
    let size = 64 + approx_size(v);
    if cache.bytes + size > CACHE_BYTES {
        cache.clear();
    }
    cache.map.insert(k, v.clone());
    cache.bytes += size;
}

/// The engine view of a PostgreSQL store ([`Store`]), synchronous over [`PgBackend`].
pub struct PgStore {
    rt: tokio::runtime::Runtime,
    /// `reset` moves to a fresh VAE (conformance runs) instead of purging all tables.
    fresh_on_reset: bool,
    /// The node signer of the host, if any (DSN-NOD-134).
    signer: Option<Arc<dyn crate::node::NodeSigner>>,
    backend: PgBackend,
    physical: Physical,
    chunks: Chunks,
    ids: Vec<u64>,
    branches: HashMap<String, i64>,
    names: HashMap<i64, String>,
    /// Immutable, content-addressed records (DSN-DVM-007 node cache, DSN-DVM-009).
    nodes: Cache<String>,
    versions: Cache<String>,
    commits: Cache<u64>,
    /// The small members of the commit records read so far (`commit_meta`, step 13d).
    commit_metas: HashMap<u64, Value>,
}

impl PgStore {
    /// Connect (a `postgres://` URL or `host=… user=… dbname=…`) in the default scope and
    /// apply the migrations.
    pub fn connect(url: &str) -> Result<PgStore, BpaError> {
        PgStore::connect_scope(url, Scope::new(DEFAULT_SCOPE.0, DEFAULT_SCOPE.1))
    }

    /// Connect in a given scope with the local key provider and blob directory of
    /// development and CI (DEC-047, DEC-048).
    pub fn connect_scope(url: &str, scope: Scope) -> Result<PgStore, BpaError> {
        let keys: Arc<dyn KeyService> = Arc::new(LocalKeys::from_env()?);
        let dir = std::env::var_os("UBOS_BLOB_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("ubos-blobs"));
        PgStore::connect_with(url, scope, keys, Arc::new(LocalBlobs::new(dir)?))
    }

    /// Connect in a given scope with the key service and blob store of a host (CTR-002:
    /// the node host's key hierarchy and blob service, PH-1 step 7a).
    pub fn connect_with(
        url: &str,
        scope: Scope,
        keys: Arc<dyn KeyService>,
        blobs: Arc<dyn BlobStore>,
    ) -> Result<PgStore, BpaError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(db)?;
        let physical = Physical::new(keys, &scope.tenant);
        let chunks = Chunks::new(physical.clone(), blobs);
        let backend = rt.block_on(PgBackend::connect(url, scope))?;
        Ok(PgStore {
            rt,
            fresh_on_reset: true,
            signer: None,
            backend,
            physical,
            chunks,
            ids: Vec::new(),
            branches: HashMap::new(),
            names: HashMap::new(),
            nodes: Cache::default(),
            versions: Cache::default(),
            commits: Cache::default(),
            commit_metas: HashMap::new(),
        })
    }

    /// Keep the scope on `reset` and purge every table instead (benchmark datasets, which
    /// later runs open in the same scope).
    pub fn with_fixed_scope(mut self) -> PgStore {
        self.fresh_on_reset = false;
        self
    }

    /// Install the node signer of the host (DSN-NOD-134): commit and verification records
    /// are then signed by the key hierarchy instead of the node record.
    pub fn with_signer(mut self, signer: Arc<dyn crate::node::NodeSigner>) -> PgStore {
        self.signer = Some(signer);
        self
    }

    /// The backend (for tools and the storage suite).
    pub fn backend_ref(&self) -> &PgBackend {
        &self.backend
    }

    /// The physical form of this store's tenant (DSN-DVM-002).
    pub fn physical(&self) -> &Physical {
        &self.physical
    }

    /// The chunk store of this store's tenant (DSN-DVM-003).
    pub fn chunks(&self) -> &Chunks {
        &self.chunks
    }

    /// Run a future of the backend on this store's runtime.
    pub fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
        self.rt.block_on(f)
    }

    fn branch_id(&mut self, name: &str) -> Result<Option<i64>, BpaError> {
        if let Some(id) = self.branches.get(name) {
            return Ok(Some(*id));
        }
        let r = self.rt.block_on(self.backend.branch(name))?;
        Ok(r.map(|b| {
            self.branches.insert(name.to_string(), b.branch_id);
            self.names.insert(b.branch_id, name.to_string());
            b.branch_id
        }))
    }

    fn branch_name(&mut self, id: i64) -> Result<String, BpaError> {
        if let Some(n) = self.names.get(&id) {
            return Ok(n.clone());
        }
        let n: String = self
            .rt
            .block_on(
                sqlx::query("SELECT name FROM branch_ref WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
                    .bind(&self.backend.scope.tenant)
                    .bind(&self.backend.scope.vae)
                    .bind(id)
                    .fetch_one(&self.backend.pool),
            )
            .map_err(db)?
            .try_get(0)
            .map_err(db)?;
        self.names.insert(id, n.clone());
        Ok(n)
    }

    /// Write bulk rows with COPY (no per-row statements); the kernel indexes are derived
    /// as in a flush. Tree nodes and versions already present must not be repeated.
    pub fn bulk_rows(&mut self, branch: &str, rows: &BulkRows) -> Result<(), BpaError> {
        let bid = self
            .branch_id(branch)?
            .ok_or_else(|| db(format!("unknown branch {branch}")))?;
        let (t, v) = (
            self.backend.scope.tenant.clone(),
            self.backend.scope.vae.clone(),
        );
        let mut ver = String::new();
        for (h, r) in &rows.versions {
            let vid = hash_of(h)?;
            let text = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default();
            let valid = r.get("valid").cloned().unwrap_or(Value::Null);
            let from = valid
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let to = valid
                .get("to")
                .and_then(Value::as_str)
                .unwrap_or("infinity");
            copy_line(
                &mut ver,
                &[
                    &t,
                    &hex(&vid),
                    text("object_id"),
                    &v,
                    text("class"),
                    &hex(&hash_of(text("class_version"))?),
                    &kind_code(text("kind")).to_string(),
                    from,
                    to,
                    &hex(&self.physical.seal(&vid, &bytes(r)?)?),
                ],
            );
        }
        let (mut hi, mut ti, mut fi) = (String::new(), String::new(), String::new());
        let bs = bid.to_string();
        for (from, to, o) in &rows.objects {
            let to_s = to.map(int).unwrap_or(int(OPEN)).to_string();
            let from_s = from.to_string();
            let row = crate::index_row(o)?;
            if o.state.is_some() {
                for (field, key) in &o.index {
                    copy_line(
                        &mut fi,
                        &[
                            &t,
                            &v,
                            &bs,
                            &o.class,
                            field,
                            &hex(key),
                            &o.object_id,
                            &from_s,
                            &to_s,
                        ],
                    );
                }
            }
            let head = row.head.map(|h| hex(&h)).unwrap_or_else(|| "\\N".into());
            let (kind, gran) = (
                kind_code(&row.kind).to_string(),
                i16::from(row.instant).to_string(),
            );
            copy_line(
                &mut hi,
                &[
                    &t,
                    &v,
                    &bs,
                    &o.object_id,
                    &from_s,
                    &to_s,
                    &head,
                    &o.class,
                    &kind,
                    &row.slug,
                    &gran,
                    if row.present { "f" } else { "t" },
                ],
            );
            for (f, e, h) in &row.segments {
                let e = e.clone().unwrap_or_else(|| "infinity".into());
                copy_line(
                    &mut ti,
                    &[
                        &t,
                        &v,
                        &bs,
                        &o.object_id,
                        f,
                        &e,
                        &from_s,
                        &to_s,
                        &hex(h),
                        &o.class,
                        &gran,
                    ],
                );
            }
        }
        let mut nodes = String::new();
        for (h, n) in &rows.nodes {
            let level = n
                .get("level")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                .to_string();
            copy_line(
                &mut nodes,
                &[&t, &hex(&hash_of(h)?), &level, &hex(&bytes(n)?)],
            );
        }
        // COPY is not allowed under row-level security; bulk loads of benchmark datasets
        // are an operator task and run as the owner role (DEC-048).
        let owner = self.backend.owner.clone();
        self.rt.block_on(async move {
            use sqlx::Connection;
            let mut conn = sqlx::PgConnection::connect_with(&owner).await.map_err(db)?;
            conn.execute("SET TIME ZONE 'UTC'").await.map_err(db)?;
            for (sql, data) in [
                ("COPY object_version (tenant_id, version_id, object_id, vae_id, class_name, class_version, kind, valid_from, valid_to, envelope) FROM STDIN", ver),
                ("COPY head_index (tenant_id, vae_id, branch_id, object_id, cseq_from, cseq_to, version_id, class_name, kind, slug, granularity, hidden) FROM STDIN", hi),
                ("COPY temporal_index (tenant_id, vae_id, branch_id, object_id, valid_from, valid_to, cseq_from, cseq_to, version_id, class_name, granularity) FROM STDIN", ti),
                ("COPY merkle_node (tenant_id, node_id, level, body) FROM STDIN", nodes),
                ("COPY field_index (tenant_id, vae_id, branch_id, class_name, field, value_key, object_id, cseq_from, cseq_to) FROM STDIN", fi),
            ] {
                let mut w = conn.copy_in_raw(sql).await.map_err(db)?;
                w.send(data.into_bytes()).await.map_err(db)?;
                w.finish().await.map_err(db)?;
            }
            Ok::<(), BpaError>(())
        })
    }

    /// Record bulk-loaded commits and move the branch head to the last one; the commit
    /// identifiers continue after it. `commits` are `(id, cseq, record)`.
    pub fn bulk_commits(
        &mut self,
        branch: &str,
        commits: &[(u64, u64, Value)],
    ) -> Result<(), BpaError> {
        let bid = self
            .branch_id(branch)?
            .ok_or_else(|| db(format!("unknown branch {branch}")))?;
        let rows = commits
            .iter()
            .map(|(id, cseq, rec)| commit_row(*id, *cseq, rec))
            .collect::<Result<Vec<_>, _>>()?;
        let backend = self.backend.clone();
        let last = rows.last().map(|r| (r.commit_no, r.commit_id, r.cseq));
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            for r in &rows {
                tx.put_commit(bid, r).await?;
            }
            if let Some((id, commit_id, cseq)) = last {
                sqlx::query("UPDATE branch_ref SET head_no = $4, head_commit = $5, head_cseq = $6 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
                    .bind(&backend.scope.tenant)
                    .bind(&backend.scope.vae)
                    .bind(bid)
                    .bind(int(id))
                    .bind(commit_id.to_vec())
                    .bind(int(cseq))
                    .execute(&mut *tx.tx)
                    .await
                    .map_err(db)?;
                sqlx::query("SELECT setval('commit_no_seq', $1)")
                    .bind(int(id))
                    .execute(&mut *tx.tx)
                    .await
                    .map_err(db)?;
            }
            tx.commit().await?;
            backend.pool.execute("ANALYZE").await.map_err(db)?;
            Ok(())
        })
    }

    /// Server facts for benchmark reports: version and settings that affect latency.
    pub fn server_facts(&mut self) -> Result<Value, BpaError> {
        let mut out = serde_json::Map::new();
        for name in [
            "server_version",
            "shared_buffers",
            "synchronous_commit",
            "fsync",
            "wal_level",
            "max_connections",
        ] {
            let r: String = self
                .rt
                .block_on(
                    sqlx::query("SELECT current_setting($1)")
                        .bind(name)
                        .fetch_one(&self.backend.pool),
                )
                .map_err(db)?
                .try_get(0)
                .map_err(db)?;
            out.insert(name.to_string(), Value::String(r));
        }
        Ok(Value::Object(out))
    }

    /// Write consecutive flushes in one transaction under the branch lock (DSN-DVM-006,
    /// DEC-046): validate each head, allocate ledger sequences from the counter rows,
    /// write versions, commits, entries, indexes and tree nodes, and move the head.
    fn write(&mut self, fs: &[Flush]) -> Result<bool, BpaError> {
        let mut ids = HashMap::new();
        for f in fs {
            if !ids.contains_key(&f.branch) {
                let Some(id) = self.branch_id(&f.branch)? else {
                    return Ok(false);
                };
                ids.insert(f.branch.clone(), id);
            }
        }
        let mut plans = Vec::with_capacity(fs.len());
        for f in fs {
            plans.push(Plan::of(
                f,
                ids[&f.branch],
                &self.physical,
                &self.chunks,
                &self.backend.scope.vae,
            )?);
        }
        let backend = self.backend.clone();
        let ok = self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            let mut heads: BTreeMap<String, u64> = BTreeMap::new();
            for (f, p) in fs.iter().zip(plans.iter_mut()) {
                let head = match heads.get(&f.branch) {
                    Some(h) => *h,
                    None => match tx.lock_branch(&f.branch).await? {
                        Some(b) => b.head_no,
                        None => {
                            tx.rollback().await?;
                            return Ok(false);
                        }
                    },
                };
                if head != f.expected_head {
                    tx.rollback().await?;
                    return Ok(false);
                }
                tx.put_versions(&p.versions).await?;
                tx.put_commit(p.branch_id, &p.commit).await?;
                let alloc = tx.alloc_sequences(p.branch_id, &p.requests).await?;
                p.chain(&alloc)?;
                tx.put_entries(p.branch_id, &p.entries).await?;
                tx.upsert_index_rows(p.branch_id, f.cseq, &p.index).await?;
                tx.upsert_field_rows(p.branch_id, f.cseq, &p.changed, &p.fields)
                    .await?;
                tx.put_tree_nodes(&p.nodes).await?;
                // Metadata written with the commit (DEC-054): effects, the idempotency
                // record, derivation records and edges, timers.
                tx.outbox_append(p.branch_id, &p.outbox).await?;
                // a request whose idempotency record another transaction wrote meanwhile
                // is not committed twice: the flush fails like a moved head and the caller
                // replays (STD-TXN-040; the fencing of PH-1 step 15c, DEC-076)
                if let Some(r) = &p.idempotency
                    && !tx.idempotency_put(r).await?
                {
                    tx.rollback().await?;
                    return Ok(false);
                }
                tx.put_chunks(&p.chunks).await?;
                tx.put_derivation_edges(&p.edges).await?;
                tx.put_timers(&p.timers).await?;
                if !tx
                    .cas_branch_head(
                        p.branch_id,
                        f.expected_head,
                        f.commit_id,
                        &p.commit.commit_id,
                        f.cseq,
                    )
                    .await?
                {
                    tx.rollback().await?;
                    return Ok(false);
                }
                heads.insert(f.branch.clone(), f.commit_id);
            }
            tx.commit().await?;
            Ok::<bool, BpaError>(true)
        })?;
        if ok {
            for f in fs {
                remember(&mut self.commits, f.commit_id, &f.commit);
                for (h, n) in &f.nodes {
                    remember(&mut self.nodes, h.clone(), n);
                }
            }
        }
        Ok(ok)
    }
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(2 + 2 * b.len());
    s.push_str("\\\\x");
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

/// Append one COPY text-format line; fields are escaped unless they are `\N` or a
/// `\\x…` bytea literal produced by [`hex`].
fn copy_line(out: &mut String, fields: &[&str]) {
    for (i, v) in fields.iter().enumerate() {
        if i > 0 {
            out.push('\t');
        }
        if *v == "\\N" || v.starts_with("\\\\x") {
            out.push_str(v);
            continue;
        }
        for c in v.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
    }
    out.push('\n');
}

fn commit_row(id: u64, cseq: u64, rec: &Value) -> Result<CommitRow, BpaError> {
    let body = bytes(rec)?;
    // The identity is the record's `commit_id` (STD-VER-001); records without one (bulk
    // loads of benchmark datasets) are identified by their content.
    let commit_id = match rec.get("commit_id").and_then(Value::as_str) {
        Some(c) if !c.is_empty() => hash_of(c)?,
        _ => hash_of(&ubos_types::content_id("ubos.commit.v1", rec)?)?,
    };
    let root = rec
        .get("root")
        .and_then(Value::as_str)
        .map(hash_of)
        .transpose()?;
    let parents = rec
        .get("parents")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    let text = |k: &str| rec.get(k).and_then(Value::as_str).map(str::to_string);
    Ok(CommitRow {
        commit_no: id,
        commit_id,
        cseq,
        tx_time: text("tx_time"),
        state_root: root.unwrap_or(ZERO),
        process_id: text("process_id"),
        author: text("author"),
        parents,
        body,
    })
}

/// The rows of one flush, prepared before the transaction (W-8: preparation outside the
/// lock); only sequences and the hash chain are completed under the lock.
struct Plan {
    branch_id: i64,
    versions: Vec<VersionRow>,
    commit: CommitRow,
    requests: Vec<SequenceRequest>,
    /// Engine sequence of each entry, checked against the allocation.
    expected: Vec<u64>,
    /// Request of each entry.
    slot: Vec<usize>,
    entries: Vec<LedgerRow>,
    index: Vec<IndexRow>,
    /// Objects whose field index rows change, and the new rows.
    changed: Vec<String>,
    fields: Vec<FieldRow>,
    nodes: Vec<NodeRow>,
    outbox: Vec<OutboxRow>,
    idempotency: Option<IdempotencyRow>,
    chunks: Vec<ChunkRow>,
    edges: Vec<DerivationEdge>,
    timers: Vec<TimerRow>,
}

impl Plan {
    fn of(
        f: &Flush,
        branch_id: i64,
        physical: &Physical,
        chunks: &Chunks,
        vae: &str,
    ) -> Result<Plan, BpaError> {
        let text = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let outbox = f
            .outbox
            .iter()
            .enumerate()
            .map(|(i, e)| {
                Ok(OutboxRow {
                    effect_id: text(e, "effect_id"),
                    cseq: f.cseq,
                    idx: u32::try_from(i).unwrap_or(u32::MAX),
                    kind: 0,
                    aggregate_key: None,
                    payload: bytes(e)?,
                })
            })
            .collect::<Result<Vec<_>, BpaError>>()?;
        let commit = commit_row(f.commit_id, f.cseq, &f.commit)?;
        let idempotency = match &f.idempotency {
            Some((scope, key, hash, outcome)) => Some(IdempotencyRow {
                scope: format!("{vae}:{scope}"),
                key: key.clone(),
                request_hash: hash_of(hash)?,
                outcome: Some(bytes(outcome)?),
                commit_id: Some(commit.commit_id),
            }),
            None => None,
        };
        let (mut chunk_rows, mut edges) = (Vec::new(), Vec::new());
        for d in &f.derivations {
            let rid = hash_of(&text(d, "id"))?;
            let row = chunks.prepare(&bytes(&d["record"])?)?;
            if row.content_id != rid {
                return Err(db("derivation record identifier differs from its chunk"));
            }
            chunk_rows.push(row);
            for out in d["outputs"].as_array().into_iter().flatten() {
                let output = hash_of(out.as_str().unwrap_or_default())?;
                for i in d["inputs"].as_array().into_iter().flatten() {
                    let asof = text(i, "asof");
                    edges.push(DerivationEdge {
                        binding_id: rid,
                        output_version: output,
                        input_object: text(i, "object"),
                        input_version: hash_of(&text(i, "version"))?,
                        instant: asof.len() > 10,
                        valid_at: asof,
                    });
                }
            }
        }
        let mut timers = Vec::new();
        for t in &f.timers {
            let key = t["key"].as_array().cloned().unwrap_or_default();
            let (branch, oid, from, vh) = (
                key.first().and_then(Value::as_str).unwrap_or_default(),
                key.get(1).and_then(Value::as_str).unwrap_or_default(),
                key.get(2).and_then(Value::as_str).unwrap_or_default(),
                key.get(3).and_then(Value::as_str).unwrap_or_default(),
            );
            let instant = t
                .get("instant")
                .and_then(Value::as_bool)
                .unwrap_or(from.len() > 10);
            timers.push(TimerRow {
                id: random_uuid()?,
                due_at: from.to_string(),
                instant,
                target: oid.to_string(),
                step: bytes(
                    &serde_json::json!({"branch": branch, "from": from, "version": vh,
                    "class": t["class"], "instant": instant}),
                )?,
            });
        }
        let versions = f
            .versions
            .iter()
            .map(|(h, r)| {
                let version_id = hash_of(h)?;
                let text = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default();
                Ok(VersionRow {
                    version_id,
                    object_id: text("object_id").into(),
                    class: text("class").into(),
                    class_version: hash_of(text("class_version"))?,
                    kind: kind_code(text("kind")),
                    valid_from: r
                        .get("valid")
                        .and_then(|v| v.get("from"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    valid_to: r
                        .get("valid")
                        .and_then(|v| v.get("to"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    envelope: physical.seal(&version_id, &bytes(r)?)?,
                })
            })
            .collect::<Result<Vec<_>, BpaError>>()?;
        let mut requests: Vec<SequenceRequest> = Vec::new();
        let (mut expected, mut slot, mut entries) = (vec![], vec![], vec![]);
        for (i, e) in f.entries.iter().enumerate() {
            let key = key_hash(&e.ledger_key);
            let eh = hash_of(
                e.entry
                    .get("entry_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?;
            let k = match requests
                .iter()
                .position(|q| q.class == e.class && q.ledger_key == key)
            {
                Some(k) => k,
                None => {
                    requests.push(SequenceRequest {
                        class: e.class.clone(),
                        ledger_key: key,
                        entry_hashes: vec![],
                    });
                    requests.len() - 1
                }
            };
            requests[k].entry_hashes.push(eh);
            expected.push(
                e.entry
                    .get("seq")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            );
            slot.push(k);
            entries.push(LedgerRow {
                class: e.class.clone(),
                ledger_key: key,
                seq: 0,
                entry_id: e.entry_id.clone(),
                valid_at: e
                    .entry
                    .get("valid_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                cseq: f.cseq,
                idx: u32::try_from(i).unwrap_or(u32::MAX),
                prev_hash: ZERO,
                entry_hash: eh,
                body: bytes(&e.entry)?,
            });
        }
        let index = f
            .objects
            .iter()
            .map(crate::index_row)
            .collect::<Result<Vec<_>, _>>()?;
        let changed: Vec<String> = f.objects.iter().map(|o| o.object_id.clone()).collect();
        let fields: Vec<FieldRow> = f
            .objects
            .iter()
            .filter(|o| o.state.is_some())
            .flat_map(|o| {
                o.index.iter().map(|(field, key)| FieldRow {
                    object_id: o.object_id.clone(),
                    class: o.class.clone(),
                    field: field.clone(),
                    key: key.clone(),
                })
            })
            .collect();
        let nodes = f
            .nodes
            .iter()
            .map(|(h, n)| {
                Ok(NodeRow {
                    node_id: hash_of(h)?,
                    level: n
                        .get("level")
                        .and_then(Value::as_u64)
                        .and_then(|l| u16::try_from(l).ok())
                        .unwrap_or_default(),
                    body: bytes(n)?,
                })
            })
            .collect::<Result<Vec<_>, BpaError>>()?;
        Ok(Plan {
            branch_id,
            versions,
            commit,
            requests,
            expected,
            slot,
            entries,
            index,
            changed,
            fields,
            nodes,
            outbox,
            idempotency,
            chunks: chunk_rows,
            edges,
            timers,
        })
    }

    /// Complete sequences and the hash chain from the allocation; the engine's sequences
    /// must agree, since its view is the locked head (DSN-DVM-402).
    fn chain(&mut self, alloc: &[Allocated]) -> Result<(), BpaError> {
        let mut next: Vec<(u64, Hash)> = alloc.iter().map(|a| (a.first_seq, a.prev_hash)).collect();
        for (i, e) in self.entries.iter_mut().enumerate() {
            let k = self.slot[i];
            let (seq, prev) = next[k];
            if seq != self.expected[i] {
                return Err(BpaError::new(
                    "NOD.STORAGE_UNAVAILABLE",
                    format!(
                        "ledger sequence {seq} allocated, {} expected",
                        self.expected[i]
                    ),
                ));
            }
            e.seq = seq;
            e.prev_hash = prev;
            next[k] = (seq + 1, e.entry_hash);
        }
        Ok(())
    }
}

impl Store for PgStore {
    fn reset(&mut self) -> Result<(), BpaError> {
        self.ids.clear();
        self.branches.clear();
        self.names.clear();
        self.nodes.clear();
        self.versions.clear();
        self.commits.clear();
        if !self.fresh_on_reset {
            return self.rt.block_on(self.backend.purge_all());
        }
        // A fresh, empty VAE of the same tenant: the adapter's state is empty without
        // truncating the partitioned tables (STD-CONF-002 `reset`, DEC-048). Commit
        // identifiers are not restarted: they are internal, and other VAEs of the database
        // keep theirs (DEC-051).
        let r: [u8; 16] = ubos_crypto::random_bytes()?;
        let h: String = r.iter().map(|b| format!("{b:02x}")).collect();
        self.backend.scope.vae = format!(
            "{}-{}-4{}-8{}-{}",
            &h[0..8],
            &h[8..12],
            &h[13..16],
            &h[17..20],
            &h[20..32]
        );
        Ok(())
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        if let Some(v) = self.versions.get(hash) {
            return Ok(v.clone());
        }
        let id = hash_of(hash)?;
        let b = self
            .rt
            .block_on(self.backend.version(&id))?
            .ok_or_else(|| db("missing version"))?;
        let v = json(&self.physical.open(&id, &b)?)?;
        remember(&mut self.versions, hash.to_string(), &v);
        Ok(v)
    }
    fn versions(&mut self, hashes: &[String]) -> Result<Vec<(String, Value)>, BpaError> {
        let mut out = Vec::with_capacity(hashes.len());
        let mut want: HashMap<Vec<u8>, String> = HashMap::new();
        for h in hashes {
            if let Some(v) = self.versions.get(h) {
                out.push((h.clone(), v.clone()));
            } else {
                want.insert(hash_of(h)?.to_vec(), h.clone());
            }
        }
        if want.is_empty() {
            return Ok(out);
        }
        let ids: Vec<Vec<u8>> = want.keys().cloned().collect();
        let tenant = self.backend.scope.tenant.clone();
        let pool = self.backend.pool.clone();
        let rows: Vec<(Vec<u8>, Vec<u8>)> = self.rt.block_on(async move {
            let rows = sqlx::query("SELECT version_id, envelope FROM object_version WHERE tenant_id = $1::uuid AND version_id = ANY($2::bytea[])")
                .bind(&tenant)
                .bind(&ids)
                .fetch_all(&pool)
                .await
                .map_err(db)?;
            rows.into_iter()
                .map(|r| Ok((r.try_get(0).map_err(db)?, r.try_get(1).map_err(db)?)))
                .collect::<Result<Vec<_>, BpaError>>()
        })?;
        for (id_bytes, b) in rows {
            let Some(h) = want.get(&id_bytes) else {
                continue;
            };
            let id = hash_of(h)?;
            let v = json(&self.physical.open(&id, &b)?)?;
            remember(&mut self.versions, h.clone(), &v);
            out.push((h.clone(), v));
        }
        Ok(out)
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        if let Some(v) = self.commits.get(&id) {
            return Ok(v.clone());
        }
        let (_, _, b) = self
            .rt
            .block_on(self.backend.commit(id))?
            .ok_or_else(|| db("missing commit"))?;
        let v = json(&b)?;
        remember(&mut self.commits, id, &v);
        Ok(v)
    }
    fn commit_meta(&mut self, id: u64) -> Result<Value, BpaError> {
        if let Some(m) = self.commit_metas.get(&id) {
            return Ok(m.clone());
        }
        let c = self.commit(id)?;
        let mut out = serde_json::Map::new();
        for k in ["branch", "cseq", "commit_id", "tx_time", "parents"] {
            if let Some(v) = c.get(k) {
                out.insert(k.to_string(), v.clone());
            }
        }
        let m = Value::Object(out);
        self.commit_metas.insert(id, m.clone());
        Ok(m)
    }
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(None);
        };
        self.rt.block_on(self.backend.commit_at(bid, cseq))
    }
    fn commit_by_id(&mut self, commit_id: &str) -> Result<Option<u64>, BpaError> {
        self.rt
            .block_on(self.backend.commit_no_of(&hash_of(commit_id)?))
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        if self.ids.is_empty() {
            let mut v = self.rt.block_on(self.backend.commit_numbers(64))?;
            v.reverse();
            self.ids = v;
        }
        self.ids.pop().ok_or_else(|| db("no commit identifier"))
    }
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError> {
        let Some(b) = self.rt.block_on(self.backend.branch(name))? else {
            return Ok(None);
        };
        self.branches.insert(name.to_string(), b.branch_id);
        self.names.insert(b.branch_id, name.to_string());
        let parent = match b.parent {
            Some((p, f)) => Some((self.branch_name(p)?, f)),
            None => None,
        };
        Ok(Some(BranchRef {
            head: b.head_no,
            head_cseq: b.head_cseq,
            parent,
        }))
    }
    fn branch_names(&mut self) -> Result<Vec<String>, BpaError> {
        self.rt.block_on(self.backend.branch_names())
    }
    fn create_branch(&mut self, name: &str, r: &BranchRef) -> Result<bool, BpaError> {
        let (parent, depth) = match &r.parent {
            Some((p, f)) => {
                let pid = self
                    .branch_id(p)?
                    .ok_or_else(|| db(format!("unknown parent branch {p}")))?;
                let d = self
                    .rt
                    .block_on(self.backend.branch(p))?
                    .map(|b| b.depth)
                    .unwrap_or_default();
                (Some((pid, *f)), d + 1)
            }
            None => (None, 0),
        };
        let row = BranchRow {
            branch_id: 0,
            name: name.to_string(),
            head_no: r.head,
            head_commit: None,
            head_cseq: r.head_cseq,
            parent,
            depth,
        };
        let backend = self.backend.clone();
        let id = self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            let id = tx.create_branch(&row).await?;
            tx.commit().await?;
            Ok::<_, BpaError>(id)
        })?;
        if let Some(id) = id {
            self.branches.insert(name.to_string(), id);
            self.names.insert(id, name.to_string());
        }
        Ok(id.is_some())
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        let _phase = ubos_types::trace::span("store.flush");
        self.write(std::slice::from_ref(f))
    }
    fn flush_many(&mut self, fs: &[Flush]) -> Result<bool, BpaError> {
        let _phase = ubos_types::trace::span("store.flush");
        self.write(fs)
    }
    fn object(
        &mut self,
        branch: &str,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Value>>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(None);
        };
        let r = self
            .rt
            .block_on(self.backend.object(bid, cseq, object_id))?;
        Ok(r.map(|f| f.as_ref().and_then(crate::state_of)))
    }
    fn objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Vec::new());
        };
        let rows = self.rt.block_on(self.backend.objects(bid, cseq, class))?;
        Ok(rows
            .into_iter()
            .map(|(id, f)| (id, f.as_ref().and_then(crate::state_of)))
            .collect())
    }
    fn objects_by_slug(
        &mut self,
        branch: &str,
        cseq: u64,
        class: &str,
        slug: &str,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Vec::new());
        };
        let rows = self
            .rt
            .block_on(self.backend.objects_by_slug(bid, cseq, class, slug))?;
        Ok(rows
            .into_iter()
            .map(|(id, f)| (id, f.as_ref().and_then(crate::state_of)))
            .collect())
    }
    fn has_field_index(&self) -> bool {
        true
    }
    fn objects_by_slugs(
        &mut self,
        branch: &str,
        cseq: u64,
        class: &str,
        slugs: &[String],
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Vec::new());
        };
        let rows = self
            .rt
            .block_on(self.backend.objects_by_slugs(bid, cseq, class, slugs))?;
        Ok(rows
            .into_iter()
            .map(|(id, f)| (id, f.as_ref().and_then(crate::state_of)))
            .collect())
    }
    fn field_keys(
        &mut self,
        branch: &str,
        cseq: u64,
        classes: &[String],
        field: &str,
        keys: &[Vec<u8>],
    ) -> Result<Option<crate::FieldRows>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Some(Vec::new()));
        };
        self.rt
            .block_on(self.backend.field_keys(bid, cseq, classes, field, keys))
            .map(Some)
    }
    fn count_objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<u64, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(0);
        };
        self.rt
            .block_on(self.backend.count_objects(bid, cseq, class))
    }
    fn put_field_rows(
        &mut self,
        branch: &str,
        cseq: u64,
        rows: &[(String, String, Vec<u8>, String)],
    ) -> Result<(), BpaError> {
        if rows.is_empty() {
            return Ok(());
        }
        let Some(bid) = self.branch_id(branch)? else {
            return Err(db(format!("unknown branch {branch}")));
        };
        let backend = self.backend.clone();
        let classes: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
        let fields: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
        let keys: Vec<Vec<u8>> = rows.iter().map(|r| r.2.clone()).collect();
        let ids: Vec<String> = rows.iter().map(|r| r.3.clone()).collect();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            for start in (0..ids.len()).step_by(10_000) {
                let end = (start + 10_000).min(ids.len());
                sqlx::query("INSERT INTO field_index (tenant_id, vae_id, branch_id, class_name, field, value_key, object_id, cseq_from, cseq_to) SELECT $1::uuid, $2::uuid, $3, t.c, t.f, t.k, t.o::uuid, $4, $5 FROM unnest($6::text[], $7::text[], $8::bytea[], $9::text[]) AS t(c, f, k, o) ON CONFLICT DO NOTHING")
                    .bind(&backend.scope.tenant)
                    .bind(&backend.scope.vae)
                    .bind(bid)
                    .bind(int(cseq))
                    .bind(int(OPEN))
                    .bind(&classes[start..end])
                    .bind(&fields[start..end])
                    .bind(&keys[start..end])
                    .bind(&ids[start..end])
                    .execute(&mut *tx.tx)
                    .await
                    .map_err(db)?;
            }
            tx.commit().await
        })
    }
    fn field_scan(
        &mut self,
        branch: &str,
        cseq: u64,
        classes: &[String],
        field: &str,
        lo: Option<&[u8]>,
        hi: Option<&[u8]>,
        limit: usize,
    ) -> Result<Option<crate::FieldRows>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Some(Vec::new()));
        };
        self.rt
            .block_on(
                self.backend
                    .field_scan(bid, cseq, classes, field, lo, hi, limit),
            )
            .map(Some)
    }
    fn objects_by_id(
        &mut self,
        branch: &str,
        cseq: u64,
        ids: &[String],
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Vec::new());
        };
        let rows = self
            .rt
            .block_on(self.backend.objects_by_id(bid, cseq, ids))?;
        Ok(rows
            .into_iter()
            .map(|(id, f)| (id, f.as_ref().and_then(crate::state_of)))
            .collect())
    }
    fn entries(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Value>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Vec::new());
        };
        let filter = match (key, entry_id) {
            (_, Some(id)) => EntryFilter::Id(id.to_string()),
            (Some((c, k)), None) => EntryFilter::Key(c.to_string(), key_hash(k)),
            (None, None) => EntryFilter::All,
        };
        let rows = self.rt.block_on(self.backend.entries(bid, cseq, &filter))?;
        rows.iter().map(|b| json(b)).collect()
    }
    fn node(&mut self, hash: &str) -> Result<Value, BpaError> {
        if let Some(v) = self.nodes.get(hash) {
            return Ok(v.clone());
        }
        let b = self
            .rt
            .block_on(self.backend.node(&hash_of(hash)?))?
            .ok_or_else(|| db("missing tree node"))?;
        let v = json(&b)?;
        remember(&mut self.nodes, hash.to_string(), &v);
        Ok(v)
    }
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError> {
        let b = serde_json::to_vec(value).map_err(db)?;
        let backend = self.backend.clone();
        let key = key.to_string();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.put_meta(&key, &b).await?;
            tx.commit().await
        })
    }
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        let r = self.rt.block_on(self.backend.meta(key))?;
        r.as_deref().map(json).transpose()
    }
    fn authority(&self) -> String {
        self.backend.scope.authority.clone()
    }
    fn signer(&self) -> Option<Arc<dyn crate::node::NodeSigner>> {
        self.signer.clone()
    }
    fn object_slugs(&mut self, object_id: &str) -> Result<Vec<String>, BpaError> {
        let rows = self.rt.block_on(self.backend.object_versions(object_id))?;
        let mut out = Vec::new();
        for (id, b) in rows {
            let v = json(&self.physical.open(&id, &b)?)?;
            if let Some(s) = v.get("slug").and_then(Value::as_str) {
                out.push(s.to_string());
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }
    fn branch_meta_rec(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        let Some(bid) = self.branch_id(name)? else {
            return Ok(None);
        };
        let Some(m) = self.rt.block_on(self.backend.branch_meta(bid))? else {
            return Ok(None);
        };
        let created_from = match m.created_from {
            Some(h) => self.rt.block_on(self.backend.commit_no_of(&h))?,
            None => None,
        };
        let pl = match &m.parent_line {
            Some(b) => json(b)?,
            None => Value::Null,
        };
        Ok(Some(
            serde_json::json!({"kind": m.kind, "created_at": m.created_at, "active_at": m.active_at,
            "parent_line": pl, "created_from": created_from}),
        ))
    }
    fn put_branch_meta_rec(&mut self, name: &str, rec: &Value) -> Result<(), BpaError> {
        let bid = self
            .branch_id(name)?
            .ok_or_else(|| db(format!("unknown branch {name}")))?;
        let created_from = match rec.get("created_from").and_then(Value::as_u64) {
            Some(no) => self.rt.block_on(self.backend.commit_id_of(no))?,
            None => None,
        };
        let text = |k: &str| rec.get(k).and_then(Value::as_str).map(str::to_string);
        let row = BranchMetaRow {
            kind: text("kind").unwrap_or_else(|| "main".into()),
            created_at: text("created_at"),
            active_at: text("active_at"),
            parent_line: rec
                .get("parent_line")
                .filter(|p| p.is_object())
                .map(bytes)
                .transpose()?,
            created_from,
        };
        let backend = self.backend.clone();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.put_branch_meta(bid, &row).await?;
            tx.commit().await
        })
    }
    fn tag_rec(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        let Some(t) = self.rt.block_on(self.backend.tag(name))? else {
            return Ok(None);
        };
        let commit = self.rt.block_on(self.backend.commit_no_of(&t.commit_id))?;
        Ok(Some(
            serde_json::json!({"commit": commit, "commit_id": hash_text(&t.commit_id), "branch": t.branch,
            "signer": t.signer, "signature": t.signature, "deleted": t.tombstoned}),
        ))
    }
    fn tag_names(&mut self) -> Result<Vec<String>, BpaError> {
        self.rt.block_on(self.backend.tag_names())
    }
    fn put_tag_rec(&mut self, name: &str, rec: &Value) -> Result<(), BpaError> {
        let text = |k: &str| {
            rec.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let row = TagRow {
            name: name.to_string(),
            commit_id: hash_of(&text("commit_id"))?,
            branch: text("branch"),
            signer: text("signer"),
            signature: text("signature"),
            tombstoned: rec.get("deleted") == Some(&Value::Bool(true)),
        };
        let backend = self.backend.clone();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.put_tag(&row).await?;
            tx.commit().await
        })
    }
    fn process_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        let Some(p) = self.rt.block_on(self.backend.process(id))? else {
            return Ok(None);
        };
        let mut trigger = serde_json::json!({"type": p.trigger_kind});
        if let Some(r) = &p.trigger_ref {
            trigger["ref"] = Value::String(r.clone());
        }
        let mut out = serde_json::json!({"process_id": p.process_id, "kind": p.kind, "initiator": p.initiator,
            "trigger": trigger, "state": p.state, "commits": p.commits, "effects": p.effects, "explicit": true});
        for (k, v) in [
            ("parent_process", &p.parent_process),
            ("bulk", &p.bulk),
            ("idempotency_key", &p.idempotency_key),
        ] {
            if let Some(v) = v {
                out[k] = Value::String(v.clone());
            }
        }
        if let Some(b) = &p.bulk_state {
            out["bulk_state"] = json(b)?;
        }
        if let Some(h) = &p.journal_id {
            out["journal_id"] = Value::String(hash_text(h));
        }
        if let Some(h) = &p.metering {
            out["metering"] = Value::String(hash_text(h));
        }
        Ok(Some(out))
    }
    fn put_process_rec(&mut self, id: &str, rec: &Value) -> Result<(), BpaError> {
        let text = |k: &str| rec.get(k).and_then(Value::as_str).map(str::to_string);
        let commits: Vec<(String, u64)> = rec
            .get("commits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c[0].as_str().unwrap_or_default().to_string(),
                    c[1].as_u64().unwrap_or_default(),
                )
            })
            .collect();
        let row = ProcessRow {
            process_id: id.to_string(),
            kind: text("kind").unwrap_or_else(|| "action".into()),
            state: text("state").unwrap_or_default(),
            initiator: text("initiator").unwrap_or_default(),
            trigger_kind: rec["trigger"]["type"]
                .as_str()
                .unwrap_or("user")
                .to_string(),
            trigger_ref: rec["trigger"]
                .get("ref")
                .and_then(Value::as_str)
                .map(str::to_string),
            parent_process: text("parent_process"),
            bulk: text("bulk"),
            idempotency_key: rec
                .get("idempotency_key")
                .filter(|v| !v.is_null())
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                }),
            effects: rec
                .get("effects")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or_default(),
            commits,
            journal_id: text("journal_id").map(|t| hash_of(&t)).transpose()?,
            metering: text("metering").map(|t| hash_of(&t)).transpose()?,
            bulk_state: rec
                .get("bulk_state")
                .filter(|v| !v.is_null())
                .map(bytes)
                .transpose()?,
        };
        let backend = self.backend.clone();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.put_process(&row).await?;
            tx.commit().await
        })
    }
    fn process_ids(&mut self) -> Result<Vec<String>, BpaError> {
        self.rt.block_on(self.backend.process_ids())
    }
    fn change_set_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        let Some(c) = self.rt.block_on(self.backend.change_set(id))? else {
            return Ok(None);
        };
        let merge = match c.merge_commit {
            Some(h) => self.rt.block_on(self.backend.commit_no_of(&h))?,
            None => None,
        };
        Ok(Some(
            serde_json::json!({"target": c.target, "merge": merge}),
        ))
    }
    fn put_change_set_rec(&mut self, id: &str, rec: &Value) -> Result<(), BpaError> {
        let merge_commit = match rec.get("merge").and_then(Value::as_u64) {
            Some(no) => self.rt.block_on(self.backend.commit_id_of(no))?,
            None => None,
        };
        let row = ChangeSetRow {
            change_set_id: id.to_string(),
            target: rec
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            merge_commit,
        };
        let backend = self.backend.clone();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.put_change_set(&row).await?;
            tx.commit().await
        })
    }
    fn record(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        let rid = hash_of(id)?;
        let Some(row) = self.rt.block_on(self.backend.chunk(&rid))? else {
            return Ok(None);
        };
        json(&self.chunks.read(&row)?).map(Some)
    }
    fn put_record(&mut self, value: &Value) -> Result<String, BpaError> {
        let row = self.chunks.prepare(canonical(value)?.as_bytes())?;
        let id = hash_text(&row.content_id);
        let backend = self.backend.clone();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.put_chunks(&[row]).await?;
            tx.commit().await
        })?;
        Ok(id)
    }
    fn derivation_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        let rid = hash_of(id)?;
        let Some(row) = self.rt.block_on(self.backend.chunk(&rid))? else {
            return Ok(None);
        };
        let record = json(&self.chunks.read(&row)?)?;
        let edges = self.rt.block_on(self.backend.derivation_edges(&rid))?;
        let mut inputs: Vec<Value> = Vec::new();
        for e in edges {
            let item = serde_json::json!({"object": e.input_object, "version": hash_text(&e.input_version), "asof": e.valid_at});
            if !inputs.contains(&item) {
                inputs.push(item);
            }
        }
        Ok(Some(
            serde_json::json!({"record": record, "inputs": inputs}),
        ))
    }
    fn outbox_pending(&mut self) -> Result<Vec<Value>, BpaError> {
        let rows = self.rt.block_on(self.backend.outbox_pending())?;
        rows.iter().map(|b| json(b)).collect()
    }
    fn outbox_claim(
        &mut self,
        holder: &str,
        _now_ms: i64,
        lease_ms: i64,
        n: usize,
    ) -> Result<Vec<Value>, BpaError> {
        let backend = self.backend.clone();
        let holder = holder.to_string();
        let rows = self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            let rows = tx.outbox_claim(&holder, lease_ms, n).await?;
            tx.commit().await?;
            Ok::<_, BpaError>(rows)
        })?;
        rows.iter()
            .map(|(b, a)| {
                let mut v = json(b)?;
                v["attempts"] = Value::from(*a);
                Ok(v)
            })
            .collect()
    }
    fn outbox_released(&mut self, branch: &str, cseq: u64) -> Result<Vec<Value>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(Vec::new());
        };
        let rows = self
            .rt
            .block_on(self.backend.outbox_of_commit(bid, cseq as i64))?;
        rows.iter().map(|b| json(b)).collect()
    }
    fn outbox_push(&mut self, items: &[Value]) -> Result<(), BpaError> {
        let mut by_branch: BTreeMap<i64, Vec<OutboxRow>> = BTreeMap::new();
        for e in items {
            let branch = e.get("branch").and_then(Value::as_str).unwrap_or("main");
            let bid = self.branch_id(branch)?.unwrap_or_default();
            let rows = by_branch.entry(bid).or_default();
            rows.push(OutboxRow {
                effect_id: e
                    .get("effect_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                cseq: e.get("cseq").and_then(Value::as_u64).unwrap_or_default(),
                idx: u32::try_from(rows.len()).unwrap_or(u32::MAX),
                kind: 1,
                aggregate_key: None,
                payload: bytes(e)?,
            });
        }
        let backend = self.backend.clone();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            for (bid, rows) in &by_branch {
                tx.outbox_append(*bid, rows).await?;
            }
            tx.commit().await
        })
    }
    fn outbox_ack(&mut self, ids: &[String]) -> Result<bool, BpaError> {
        let backend = self.backend.clone();
        let ids = ids.to_vec();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            let ok = tx.outbox_ack(&ids).await?;
            if ok {
                tx.commit().await?;
            } else {
                tx.rollback().await?;
            }
            Ok(ok)
        })
    }
    fn timers(&mut self) -> Result<Vec<Value>, BpaError> {
        let rows = self.rt.block_on(self.backend.timers())?;
        rows.iter()
            .map(|t| {
                let step = json(&t.step)?;
                Ok(serde_json::json!({"id": t.id, "key": [step["branch"], t.target, t.due_at, step["version"]],
                    "class": step["class"], "instant": t.instant}))
            })
            .collect()
    }
    fn remove_timers(&mut self, ids: &[String]) -> Result<(), BpaError> {
        let backend = self.backend.clone();
        let ids = ids.to_vec();
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            tx.fire_timers(&ids).await?;
            tx.commit().await
        })
    }
    fn idempotency_rec(&mut self, scope: &str, key: &str) -> Result<Option<Value>, BpaError> {
        // Scoped by tenant (the table), VAE and initiator (STD-TXN-040).
        let scope = format!("{}:{scope}", self.backend.scope.vae);
        let Some((hash, outcome)) = self.rt.block_on(self.backend.idempotency(&scope, key))? else {
            return Ok(None);
        };
        let result = match outcome {
            Some(b) => json(&b)?,
            None => Value::Null,
        };
        Ok(Some(
            serde_json::json!({"hash": hash_text(&hash), "result": result}),
        ))
    }
    fn backend(&self) -> &'static str {
        "postgresql"
    }
    fn scope(&self) -> Option<Scope> {
        Some(self.backend.scope.clone())
    }
}
