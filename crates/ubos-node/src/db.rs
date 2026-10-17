//! Database access of the node services: one connection pool per role and tenant, driven
//! by a runtime of its own so that a synchronous service call never nests inside another
//! runtime.

use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Executor, Row};
use std::future::Future;
use std::pin::Pin;
use ubos_dvm::store::pg_options;
use ubos_dvm::types::BpaError;

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

/// Isolation defects detected (DSN-NOD-405): a tenant-scoped pool asked for without a
/// valid tenant. The self-checks report them.
pub static ISOLATION_DEFECTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Raise an isolation defect alert.
pub fn isolation_alert(what: &str) {
    ISOLATION_DEFECTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    eprintln!("ubos: ISOLATION DEFECT: {what}");
}

/// A future a database job returns.
pub type Job<R> = Pin<Box<dyn Future<Output = Result<R, BpaError>> + Send>>;

/// A pool with its runtime.
pub struct Db {
    rt: tokio::runtime::Runtime,
    pool: PgPool,
}

impl Db {
    fn open(url: &str, setup: Option<String>, max: u32) -> Result<Db, BpaError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(err)?;
        let opts = pg_options(url)?;
        let pool = rt.block_on(async {
            let mut b = PgPoolOptions::new().max_connections(max);
            if let Some(sql) = setup {
                b = b.after_connect(move |conn, _| {
                    let sql = sql.clone();
                    Box::pin(async move {
                        conn.execute(sql.as_str()).await?;
                        Ok(())
                    })
                });
            }
            b.connect_with(opts).await.map_err(err)
        })?;
        Ok(Db { rt, pool })
    }

    /// The owner role: the registry tables (`tenant`, `vae`), which row-level security
    /// hides from `ubos_app`.
    pub fn owner(url: &str) -> Result<Db, BpaError> {
        Db::open(url, Some("SET TIME ZONE 'UTC'".into()), 4)
    }

    /// The application role in one tenant: the tenant tables under row-level security.
    pub fn tenant(url: &str, tenant_id: &str) -> Result<Db, BpaError> {
        Db::tenant_with(url, tenant_id, 4)
    }

    /// The application role in one tenant with a pool of at most `max` connections. The
    /// pool's connections are bound to the tenant for their life (DSN-NOD-405): a pool
    /// serves one tenant and is never shared, and a pool without a valid tenant is refused
    /// with an isolation alert (row-level security gives a connection without a tenant no
    /// rows).
    pub fn tenant_with(url: &str, tenant_id: &str, max: u32) -> Result<Db, BpaError> {
        let ok =
            tenant_id.len() == 36 && tenant_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if !ok {
            isolation_alert("a tenant pool without a valid tenant was refused");
            return Err(BpaError::with_details(
                "NOD.STORAGE_UNAVAILABLE",
                serde_json::json!({"reason": "tenant_not_set"}),
            ));
        }
        Db::open(
            url,
            Some(format!(
                "SET ROLE ubos_app; SET ubos.tenant = '{tenant_id}'; SET TIME ZONE 'UTC'"
            )),
            max,
        )
    }

    /// Run a database job to completion.
    pub fn run<R>(&self, f: impl FnOnce(PgPool) -> Job<R>) -> Result<R, BpaError> {
        self.rt.block_on(f(self.pool.clone()))
    }

    /// Run statements in one transaction, in order; the rows of each. A failing statement
    /// rolls the transaction back.
    pub fn transaction(&self, stmts: &[(&str, &[Param<'_>])]) -> Result<Vec<Vec<Value>>, BpaError> {
        let stmts: Vec<(String, Vec<Owned>)> = stmts
            .iter()
            .map(|(sql, params)| (sql.to_string(), params.iter().map(Param::owned).collect()))
            .collect();
        self.run(move |pool| {
            Box::pin(async move {
                let mut tx = pool.begin().await.map_err(err)?;
                let mut out = Vec::new();
                for (sql, params) in &stmts {
                    let mut q = sqlx::query(sql);
                    for p in params {
                        q = p.bind(q);
                    }
                    let rows = q.fetch_all(&mut *tx).await.map_err(err)?;
                    out.push(rows.iter().map(row_json).collect::<Result<Vec<_>, _>>()?);
                }
                tx.commit().await.map_err(err)?;
                Ok(out)
            })
        })
    }

    /// Execute a statement with text and byte parameters; returns the rows affected.
    pub fn execute(&self, sql: &str, params: &[Param<'_>]) -> Result<u64, BpaError> {
        let sql = sql.to_string();
        let params: Vec<Owned> = params.iter().map(Param::owned).collect();
        self.run(move |pool| {
            Box::pin(async move {
                let mut q = sqlx::query(&sql);
                for p in &params {
                    q = p.bind(q);
                }
                q.execute(&pool)
                    .await
                    .map(|r| r.rows_affected())
                    .map_err(err)
            })
        })
    }

    /// Query rows as JSON objects of the named columns (text, integer, bigint, boolean,
    /// jsonb, bytea as lowercase hex; cast uuid and timestamptz columns to text in the statement).
    pub fn query(&self, sql: &str, params: &[Param<'_>]) -> Result<Vec<Value>, BpaError> {
        let sql = sql.to_string();
        let params: Vec<Owned> = params.iter().map(Param::owned).collect();
        self.run(move |pool| {
            Box::pin(async move {
                let mut q = sqlx::query(&sql);
                for p in &params {
                    q = p.bind(q);
                }
                let rows = q.fetch_all(&pool).await.map_err(err)?;
                rows.iter().map(row_json).collect()
            })
        })
    }
}

/// A query parameter.
pub enum Param<'a> {
    /// Text (also for `::uuid` casts in the statement).
    Text(&'a str),
    /// Bytes.
    Bytes(&'a [u8]),
    /// Integer.
    Int(i64),
    /// JSON.
    Json(&'a Value),
}

enum Owned {
    Text(String),
    Bytes(Vec<u8>),
    Int(i64),
    Json(Value),
}

impl Param<'_> {
    fn owned(&self) -> Owned {
        match self {
            Param::Text(t) => Owned::Text(t.to_string()),
            Param::Bytes(b) => Owned::Bytes(b.to_vec()),
            Param::Int(i) => Owned::Int(*i),
            Param::Json(v) => Owned::Json((*v).clone()),
        }
    }
}

impl Owned {
    fn bind<'q>(
        &'q self,
        q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
        match self {
            Owned::Text(t) => q.bind(t.as_str()),
            Owned::Bytes(b) => q.bind(b.as_slice()),
            Owned::Int(i) => q.bind(*i),
            Owned::Json(v) => q.bind(v),
        }
    }
}

fn row_json(r: &sqlx::postgres::PgRow) -> Result<Value, BpaError> {
    use sqlx::{Column, TypeInfo};
    let mut out = serde_json::Map::new();
    for c in r.columns() {
        let i = c.ordinal();
        let v = match c.type_info().name() {
            "TEXT" | "VARCHAR" | "NAME" => r
                .try_get::<Option<String>, _>(i)
                .map_err(err)?
                .map(Value::String),
            "INT4" => r
                .try_get::<Option<i32>, _>(i)
                .map_err(err)?
                .map(Value::from),
            "INT8" => r
                .try_get::<Option<i64>, _>(i)
                .map_err(err)?
                .map(Value::from),
            "BOOL" => r
                .try_get::<Option<bool>, _>(i)
                .map_err(err)?
                .map(Value::Bool),
            "JSONB" | "JSON" => r.try_get::<Option<Value>, _>(i).map_err(err)?,
            "BYTEA" => r
                .try_get::<Option<Vec<u8>>, _>(i)
                .map_err(err)?
                .map(|b| Value::String(b.iter().map(|x| format!("{x:02x}")).collect())),
            other => {
                return Err(err(format!(
                    "column {} of type {other} is not mapped",
                    c.name()
                )));
            }
        };
        out.insert(c.name().to_string(), v.unwrap_or(Value::Null));
    }
    Ok(Value::Object(out))
}
