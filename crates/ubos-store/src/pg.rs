//! PostgreSQL adapter on sqlx (DSN-DVM-001, -004, -005, -006, -007, -010, -402; DEC-047).
//!
//! [`PgBackend`] implements the backend-neutral traits [`StoreRead`] and (through [`PgTx`])
//! [`StoreTx`]. [`PgStore`] is the synchronous engine view ([`Store`]) over it: it runs the
//! asynchronous operations on its own Tokio runtime and maps the engine's records to the
//! kernel rows (canonical bytes, 32-byte content IDs, `cseq` intervals).

use crate::kernel::{
    Allocated, BranchRow, ChunkRow, CommitRow, CounterRow, EntryFilter, EntryMeta, Hash, HeadRow,
    IdempotencyRow, IndexRow, LedgerRow, NodeRow, OPEN, OutboxRow, Placement, Scope,
    SequenceRequest, StateRow, StoreRead, StoreTx, TemporalRow, VersionRow, hash_of,
};
use crate::physical::{Chunks, LocalBlobs, Physical};
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
    ledger_entry, ledger_sequence, merkle_node, object_state, head_index, temporal_index, \
    derivation_edge, idempotency_record, outbox";

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
            "SET ROLE ubos_app; SET ubos.tenant = '{}'; SET synchronous_commit = on; SET plan_cache_mode = force_custom_plan;",
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
        head_cseq: uint(r.try_get("head_cseq").map_err(db)?),
        parent: parent.map(|p| (p, uint(fork.unwrap_or_default()))),
        depth: u16::try_from(r.try_get::<i16, _>("depth").map_err(db)?).unwrap_or_default(),
    })
}

const BRANCH_COLS: &str = "branch_id, name, head_no, head_cseq, parent_branch, fork_cseq, depth";

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
    async fn object(
        &self,
        branch_id: i64,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Vec<u8>>>, BpaError> {
        let r = self
            .opt(
                sqlx::query("SELECT state FROM object_state WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = $4 AND cseq_from <= $5 AND cseq_to > $5")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(object_id)
                    .bind(int(cseq)),
            )
            .await?;
        r.map(|r| r.try_get::<Option<Vec<u8>>, _>(0).map_err(db))
            .transpose()
    }
    async fn objects(
        &self,
        branch_id: i64,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Vec<u8>>)>, BpaError> {
        let q = match class {
            Some(c) => sqlx::query("SELECT object_id, state FROM object_state WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq_from <= $4 AND cseq_to > $4 AND class_name = $5")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(cseq))
                .bind(c),
            None => sqlx::query("SELECT object_id, state FROM object_state WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq_from <= $4 AND cseq_to > $4")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(cseq)),
        };
        let rows = q.fetch_all(&self.pool).await.map_err(db)?;
        rows.iter()
            .map(|r| Ok((r.try_get(0).map_err(db)?, r.try_get(1).map_err(db)?)))
            .collect()
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
            EntryFilter::Id(_) => {
                sqlx::query(const_format(BASE, " AND entry_id = $5 ORDER BY cseq, idx"))
            }
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
    async fn state_rows(
        &self,
        branch_id: i64,
        from: u64,
        to: u64,
    ) -> Result<Vec<StateRow>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT object_id, cseq_from, cseq_to, class_name, state FROM object_state WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq_from BETWEEN $4 AND $5")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id)
                    .bind(int(from))
                    .bind(int(to)),
            )
            .await?;
        rows.iter().map(state_row).collect()
    }
    async fn head_rows(&self, branch_id: i64) -> Result<Vec<HeadRow>, BpaError> {
        let rows = self
            .rows(
                sqlx::query("SELECT object_id, version_id, class_name, hidden FROM head_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
                    .bind(&self.scope.tenant)
                    .bind(&self.scope.vae)
                    .bind(branch_id),
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(HeadRow {
                    object_id: r.try_get(0).map_err(db)?,
                    version: r
                        .try_get::<Option<Vec<u8>>, _>(1)
                        .map_err(db)?
                        .map(hash_col)
                        .transpose()?,
                    class: r.try_get(2).map_err(db)?,
                    hidden: r.try_get(3).map_err(db)?,
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
        let rows = self
            .rows(
                sqlx::query("SELECT object_id, valid_from, valid_to, cseq_from, cseq_to, version_id, class_name FROM temporal_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND cseq_from BETWEEN $4 AND $5")
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
                    valid_from: r.try_get(1).map_err(db)?,
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
                sqlx::query("SELECT ledger_class, ledger_key, seq, entry_id, prev_hash, entry_hash FROM ledger_entry WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 ORDER BY ledger_class, ledger_key, seq")
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
}

impl PgBackend {
    async fn rows(
        &self,
        q: sqlx::query::Query<'_, Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<Vec<PgRow>, BpaError> {
        q.fetch_all(&self.pool).await.map_err(db)
    }
}

fn state_row(r: &PgRow) -> Result<StateRow, BpaError> {
    Ok(StateRow {
        object_id: r.try_get(0).map_err(db)?,
        cseq_from: uint(r.try_get(1).map_err(db)?),
        cseq_to: uint(r.try_get(2).map_err(db)?),
        class: r.try_get(3).map_err(db)?,
        state: r.try_get(4).map_err(db)?,
    })
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
        let env: Vec<&[u8]> = rows.iter().map(|r| r.envelope.as_slice()).collect();
        sqlx::query("INSERT INTO object_version (tenant_id, version_id, object_id, class_name, envelope) SELECT $1::uuid, t.i, t.o, t.c, t.e FROM unnest($2::bytea[], $3::text[], $4::text[], $5::bytea[]) AS t(i, o, c, e) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(ids)
            .bind(oids)
            .bind(classes)
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
        let cseq: Vec<i64> = rows.iter().map(|r| int(r.cseq)).collect();
        let idx: Vec<i32> = rows
            .iter()
            .map(|r| i32::try_from(r.idx).unwrap_or(i32::MAX))
            .collect();
        let prev: Vec<Vec<u8>> = rows.iter().map(|r| r.prev_hash.to_vec()).collect();
        let eh: Vec<Vec<u8>> = rows.iter().map(|r| r.entry_hash.to_vec()).collect();
        let body: Vec<&[u8]> = rows.iter().map(|r| r.body.as_slice()).collect();
        sqlx::query("INSERT INTO ledger_entry (tenant_id, vae_id, branch_id, ledger_class, ledger_key, seq, entry_id, cseq, idx, prev_hash, entry_hash, body) SELECT $1::uuid, $2::uuid, $3, t.c, t.k, t.s, t.i, t.q, t.x, t.p, t.h, t.b FROM unnest($4::text[], $5::bytea[], $6::bigint[], $7::text[], $8::bigint[], $9::integer[], $10::bytea[], $11::bytea[], $12::bytea[]) AS t(c, k, s, i, q, x, p, h, b)")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(class)
            .bind(key)
            .bind(seq)
            .bind(id)
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
        sqlx::query("INSERT INTO commit (tenant_id, vae_id, commit_no, commit_id, branch_id, cseq, state_root, body) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8)")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(int(row.commit_no))
            .bind(&row.commit_id[..])
            .bind(branch_id)
            .bind(int(row.cseq))
            .bind(&row.state_root[..])
            .bind(&row.body)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        if !row.parents.is_empty() {
            let ord: Vec<i16> = (0..row.parents.len())
                .map(|i| i16::try_from(i).unwrap_or(i16::MAX))
                .collect();
            let parents: Vec<i64> = row.parents.iter().map(|p| int(*p)).collect();
            sqlx::query("INSERT INTO commit_parent (tenant_id, vae_id, commit_no, ord, parent_no) SELECT $1::uuid, $2::uuid, $3, t.o, t.p FROM unnest($4::smallint[], $5::bigint[]) AS t(o, p)")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(int(row.commit_no))
                .bind(ord)
                .bind(parents)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }
    async fn cas_branch_head(
        &mut self,
        branch_id: i64,
        expected: u64,
        head_no: u64,
        head_cseq: u64,
    ) -> Result<bool, BpaError> {
        let r = sqlx::query("UPDATE branch_ref SET head_no = $4, head_cseq = $5 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND head_no = $6")
            .bind(&self.scope.tenant)
            .bind(&self.scope.vae)
            .bind(branch_id)
            .bind(int(head_no))
            .bind(int(head_cseq))
            .bind(int(expected))
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() == 1)
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
        let states: Vec<Option<&[u8]>> = rows.iter().map(|r| r.state.as_deref()).collect();
        let open = int(OPEN);
        for table in ["object_state", "temporal_index"] {
            let sql = format!(
                "UPDATE {table} SET cseq_to = $4 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = ANY($5::text[]) AND cseq_to = $6"
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
        sqlx::query("INSERT INTO object_state (tenant_id, vae_id, branch_id, object_id, cseq_from, cseq_to, class_name, state) SELECT $1::uuid, $2::uuid, $3, t.o, $4, $5, t.c, t.s FROM unnest($6::text[], $7::text[], $8::bytea[]) AS t(o, c, s)")
            .bind(t)
            .bind(v)
            .bind(branch_id)
            .bind(c)
            .bind(open)
            .bind(&ids)
            .bind(&classes)
            .bind(&states)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        let heads: Vec<Option<Vec<u8>>> = rows.iter().map(|r| r.head.map(|h| h.to_vec())).collect();
        let hidden: Vec<bool> = rows.iter().map(|r| r.state.is_none()).collect();
        sqlx::query("INSERT INTO head_index (tenant_id, vae_id, branch_id, object_id, version_id, class_name, hidden) SELECT $1::uuid, $2::uuid, $3, t.o, t.h, t.c, t.x FROM unnest($4::text[], $5::bytea[], $6::text[], $7::boolean[]) AS t(o, h, c, x) ON CONFLICT (tenant_id, vae_id, branch_id, object_id) DO UPDATE SET version_id = EXCLUDED.version_id, class_name = EXCLUDED.class_name, hidden = EXCLUDED.hidden")
            .bind(t)
            .bind(v)
            .bind(branch_id)
            .bind(&ids)
            .bind(heads)
            .bind(&classes)
            .bind(hidden)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        let (mut so, mut sc, mut sf, mut st, mut sv) = (vec![], vec![], vec![], vec![], vec![]);
        for r in rows {
            for (from, to, ver) in &r.segments {
                so.push(r.object_id.as_str());
                sc.push(r.class.as_str());
                sf.push(from.as_str());
                st.push(to.as_deref());
                sv.push(ver.to_vec());
            }
        }
        if !so.is_empty() {
            sqlx::query("INSERT INTO temporal_index (tenant_id, vae_id, branch_id, object_id, valid_from, valid_to, cseq_from, cseq_to, version_id, class_name) SELECT $1::uuid, $2::uuid, $3, t.o, t.f, t.t, $4, $5, t.v, t.c FROM unnest($6::text[], $7::text[], $8::text[], $9::bytea[], $10::text[]) AS t(o, f, t, v, c)")
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
            sqlx::query("INSERT INTO outbox (tenant_id, vae_id, branch_id, cseq, idx, kind, aggregate_key, payload) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8)")
                .bind(&self.scope.tenant)
                .bind(&self.scope.vae)
                .bind(branch_id)
                .bind(int(r.cseq))
                .bind(i32::try_from(r.idx).unwrap_or(i32::MAX))
                .bind(i16::try_from(r.kind).unwrap_or(i16::MAX))
                .bind(r.aggregate_key.as_deref())
                .bind(&r.payload)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
        }
        Ok(())
    }
    async fn idempotency_put(&mut self, row: &IdempotencyRow) -> Result<bool, BpaError> {
        let r = sqlx::query("INSERT INTO idempotency_record (tenant_id, scope, key, request_hash, outcome, commit_no) VALUES ($1::uuid, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING")
            .bind(&self.scope.tenant)
            .bind(&row.scope)
            .bind(&row.key)
            .bind(&row.request_hash[..])
            .bind(row.outcome.as_deref())
            .bind(row.commit_no.map(int))
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
    /// `(object_id, cseq_from, cseq_to, class, state)` object rows.
    pub objects: Vec<(String, u64, Option<u64>, String, Value)>,
    /// `(hash, node)` tree nodes.
    pub nodes: Vec<(String, Value)>,
}

/// Entries kept per cache before it is cleared.
const CACHE_LIMIT: usize = 200_000;

fn remember<K: std::hash::Hash + Eq>(cache: &mut HashMap<K, Value>, k: K, v: &Value) {
    if cache.len() >= CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(k, v.clone());
}

/// The engine view of a PostgreSQL store ([`Store`]), synchronous over [`PgBackend`].
pub struct PgStore {
    rt: tokio::runtime::Runtime,
    /// `reset` moves to a fresh VAE (conformance runs) instead of purging all tables.
    fresh_on_reset: bool,
    backend: PgBackend,
    physical: Physical,
    chunks: Chunks,
    ids: Vec<u64>,
    branches: HashMap<String, i64>,
    names: HashMap<i64, String>,
    /// Immutable, content-addressed records (DSN-DVM-007 node cache, DSN-DVM-009).
    nodes: HashMap<String, Value>,
    versions: HashMap<String, Value>,
    commits: HashMap<u64, Value>,
}

impl PgStore {
    /// Connect (a `postgres://` URL or `host=… user=… dbname=…`) in the default scope and
    /// apply the migrations.
    pub fn connect(url: &str) -> Result<PgStore, BpaError> {
        let scope = Scope {
            tenant: DEFAULT_SCOPE.0.into(),
            vae: DEFAULT_SCOPE.1.into(),
        };
        PgStore::connect_scope(url, scope)
    }

    /// Connect in a given scope.
    pub fn connect_scope(url: &str, scope: Scope) -> Result<PgStore, BpaError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(db)?;
        let keys: Arc<dyn KeyService> = Arc::new(LocalKeys::from_env()?);
        let physical = Physical::new(keys, &scope.tenant);
        let dir = std::env::var_os("UBOS_BLOB_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("ubos-blobs"));
        let chunks = Chunks::new(physical.clone(), Arc::new(LocalBlobs::new(dir)?));
        let backend = rt.block_on(PgBackend::connect(url, scope))?;
        Ok(PgStore {
            rt,
            fresh_on_reset: true,
            backend,
            physical,
            chunks,
            ids: Vec::new(),
            branches: HashMap::new(),
            names: HashMap::new(),
            nodes: HashMap::new(),
            versions: HashMap::new(),
            commits: HashMap::new(),
        })
    }

    /// Keep the scope on `reset` and purge every table instead (benchmark datasets, which
    /// later runs open in the same scope).
    pub fn with_fixed_scope(mut self) -> PgStore {
        self.fresh_on_reset = false;
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
            let oid = r
                .get("object_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let class = r.get("class").and_then(Value::as_str).unwrap_or_default();
            copy_line(
                &mut ver,
                &[
                    &t,
                    &hex(&vid),
                    oid,
                    class,
                    &hex(&self.physical.seal(&vid, &bytes(r)?)?),
                ],
            );
        }
        let (mut st, mut hi, mut ti) = (String::new(), String::new(), String::new());
        let bs = bid.to_string();
        for (oid, from, to, class, state) in &rows.objects {
            let to_s = to.map(int).unwrap_or(int(OPEN)).to_string();
            let from_s = from.to_string();
            copy_line(
                &mut st,
                &[
                    &t,
                    &v,
                    &bs,
                    oid,
                    &from_s,
                    &to_s,
                    class,
                    &hex(&bytes(state)?),
                ],
            );
            let row = crate::index_row(&crate::ObjectRow {
                object_id: oid.clone(),
                class: class.clone(),
                state: Some(state.clone()),
            })?;
            if to.is_none() {
                let head = row.head.map(|h| hex(&h)).unwrap_or_else(|| "\\N".into());
                copy_line(&mut hi, &[&t, &v, &bs, oid, &head, class, "f"]);
            }
            for (f, e, h) in &row.segments {
                let e = e.clone().unwrap_or_else(|| "\\N".into());
                copy_line(
                    &mut ti,
                    &[&t, &v, &bs, oid, f, &e, &from_s, &to_s, &hex(h), class],
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
            for (sql, data) in [
                ("COPY object_version (tenant_id, version_id, object_id, class_name, envelope) FROM STDIN", ver),
                ("COPY object_state (tenant_id, vae_id, branch_id, object_id, cseq_from, cseq_to, class_name, state) FROM STDIN", st),
                ("COPY head_index (tenant_id, vae_id, branch_id, object_id, version_id, class_name, hidden) FROM STDIN", hi),
                ("COPY temporal_index (tenant_id, vae_id, branch_id, object_id, valid_from, valid_to, cseq_from, cseq_to, version_id, class_name) FROM STDIN", ti),
                ("COPY merkle_node (tenant_id, node_id, level, body) FROM STDIN", nodes),
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
        let last = commits.last().map(|(id, cseq, _)| (*id, *cseq));
        self.rt.block_on(async move {
            let mut tx = backend.begin().await?;
            for r in &rows {
                tx.put_commit(bid, r).await?;
            }
            if let Some((id, cseq)) = last {
                sqlx::query("UPDATE branch_ref SET head_no = $4, head_cseq = $5 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3")
                    .bind(&backend.scope.tenant)
                    .bind(&backend.scope.vae)
                    .bind(bid)
                    .bind(int(id))
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
            plans.push(Plan::of(f, ids[&f.branch], &self.physical)?);
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
                tx.put_tree_nodes(&p.nodes).await?;
                if !tx
                    .cas_branch_head(p.branch_id, f.expected_head, f.commit_id, f.cseq)
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
    let commit_id = hash_of(&ubos_types::content_id("ubos.commit.v1", rec)?)?;
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
    Ok(CommitRow {
        commit_no: id,
        commit_id,
        cseq,
        state_root: root.unwrap_or(ZERO),
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
    nodes: Vec<NodeRow>,
}

impl Plan {
    fn of(f: &Flush, branch_id: i64, physical: &Physical) -> Result<Plan, BpaError> {
        let versions = f
            .versions
            .iter()
            .map(|(h, r)| {
                let version_id = hash_of(h)?;
                Ok(VersionRow {
                    version_id,
                    object_id: r
                        .get("object_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    class: r
                        .get("class")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
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
            commit: commit_row(f.commit_id, f.cseq, &f.commit)?,
            requests,
            expected,
            slot,
            entries,
            index,
            nodes,
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
        // identifiers restart so that runs are reproducible.
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
        self.rt
            .block_on(
                sqlx::query("SELECT setval('commit_no_seq', 1, false)").execute(&self.backend.pool),
            )
            .map_err(db)?;
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
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        let Some(bid) = self.branch_id(branch)? else {
            return Ok(None);
        };
        self.rt.block_on(self.backend.commit_at(bid, cseq))
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
        self.write(std::slice::from_ref(f))
    }
    fn flush_many(&mut self, fs: &[Flush]) -> Result<bool, BpaError> {
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
        r.map(|s| s.as_deref().map(json).transpose()).transpose()
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
        rows.into_iter()
            .map(|(id, s)| Ok((id, s.as_deref().map(json).transpose()?)))
            .collect()
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
    fn backend(&self) -> &'static str {
        "postgresql"
    }
    fn scope(&self) -> Option<Scope> {
        Some(self.backend.scope.clone())
    }
}
