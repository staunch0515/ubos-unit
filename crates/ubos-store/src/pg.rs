//! PostgreSQL backend (DEC-024, DEC-033; DSN-DVM-004, -005, -007, -010).

use crate::{BranchRef, Flush, Store};
use postgres::types::ToSql;
use postgres::{Client, NoTls, Statement};
use serde_json::Value;
use std::collections::HashMap;
use ubos_types::BpaError;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS bpa_version (hash text PRIMARY KEY, record jsonb NOT NULL);
CREATE TABLE IF NOT EXISTS bpa_commit (
    id bigint PRIMARY KEY, branch text NOT NULL, cseq bigint NOT NULL, record jsonb NOT NULL,
    UNIQUE (branch, cseq));
CREATE TABLE IF NOT EXISTS bpa_branch (
    name text PRIMARY KEY, head bigint NOT NULL, head_cseq bigint NOT NULL,
    parent text, fork_cseq bigint);
CREATE TABLE IF NOT EXISTS bpa_object (
    branch text NOT NULL, object_id text NOT NULL, cseq_from bigint NOT NULL, cseq_to bigint,
    class text NOT NULL, state jsonb,
    PRIMARY KEY (branch, object_id, cseq_from));
CREATE INDEX IF NOT EXISTS bpa_object_class ON bpa_object (branch, class, cseq_from);
CREATE TABLE IF NOT EXISTS bpa_entry (
    ord bigserial PRIMARY KEY, branch text NOT NULL, cseq bigint NOT NULL, entry_id text NOT NULL,
    class text NOT NULL, ledger_key text NOT NULL, entry jsonb NOT NULL);
CREATE INDEX IF NOT EXISTS bpa_entry_key ON bpa_entry (branch, class, ledger_key, ord);
CREATE INDEX IF NOT EXISTS bpa_entry_id ON bpa_entry (branch, entry_id);
CREATE TABLE IF NOT EXISTS bpa_tree_node (hash text PRIMARY KEY, node jsonb NOT NULL);
CREATE TABLE IF NOT EXISTS bpa_meta (key text PRIMARY KEY, value jsonb NOT NULL);
CREATE SEQUENCE IF NOT EXISTS bpa_commit_id;
";

const TABLES: &str =
    "bpa_version, bpa_commit, bpa_branch, bpa_object, bpa_entry, bpa_tree_node, bpa_meta";

fn db(e: postgres::Error) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

fn int(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A store in a PostgreSQL database.
pub struct PgStore {
    client: Client,
    statements: HashMap<&'static str, Statement>,
    /// Commit identifiers fetched in advance (identifiers need not be dense).
    ids: Vec<u64>,
    /// Immutable, content-addressed records read or written through this connection
    /// (DSN-DVM-007 node cache, DSN-DVM-009).
    nodes: HashMap<String, Value>,
    versions: HashMap<String, Value>,
    commits: HashMap<u64, Value>,
}

/// Entries kept per cache before it is cleared.
const CACHE_LIMIT: usize = 200_000;

fn remember<K: std::hash::Hash + Eq>(cache: &mut HashMap<K, Value>, k: K, v: &Value) {
    if cache.len() >= CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(k, v.clone());
}

/// Rows for a bulk load of one branch's history (benchmark datasets, PH-0 step 5).
#[derive(Debug, Default)]
pub struct BulkRows {
    /// `(hash, record)` of version records.
    pub versions: Vec<(String, Value)>,
    /// `(object_id, cseq_from, cseq_to, class, state)` object-index rows.
    pub objects: Vec<(String, u64, Option<u64>, String, Value)>,
    /// `(hash, node)` tree nodes.
    pub nodes: Vec<(String, Value)>,
}

/// Escape a field for the COPY text format.
fn copy_field(out: &mut String, v: &str) {
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

impl PgStore {
    /// Write bulk rows with COPY (no per-row statements). Tree nodes and versions already
    /// present must not be repeated by the caller.
    pub fn bulk_rows(&mut self, branch: &str, rows: &BulkRows) -> Result<(), BpaError> {
        use std::io::Write;
        let mut buf = String::new();
        let mut w = self
            .client
            .copy_in("COPY bpa_version (hash, record) FROM STDIN")
            .map_err(db)?;
        for (h, r) in &rows.versions {
            buf.clear();
            copy_field(&mut buf, h);
            buf.push('\t');
            copy_field(&mut buf, &r.to_string());
            buf.push('\n');
            w.write_all(buf.as_bytes())
                .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?;
        }
        w.finish().map_err(db)?;
        let mut w = self
            .client
            .copy_in(
                "COPY bpa_object (branch, object_id, cseq_from, cseq_to, class, state) FROM STDIN",
            )
            .map_err(db)?;
        for (id, from, to, class, state) in &rows.objects {
            buf.clear();
            copy_field(&mut buf, branch);
            buf.push('\t');
            copy_field(&mut buf, id);
            buf.push_str(&format!("\t{from}\t"));
            match to {
                Some(t) => buf.push_str(&t.to_string()),
                None => buf.push_str("\\N"),
            }
            buf.push('\t');
            copy_field(&mut buf, class);
            buf.push('\t');
            copy_field(&mut buf, &state.to_string());
            buf.push('\n');
            w.write_all(buf.as_bytes())
                .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?;
        }
        w.finish().map_err(db)?;
        let mut w = self
            .client
            .copy_in("COPY bpa_tree_node (hash, node) FROM STDIN")
            .map_err(db)?;
        for (h, n) in &rows.nodes {
            buf.clear();
            copy_field(&mut buf, h);
            buf.push('\t');
            copy_field(&mut buf, &n.to_string());
            buf.push('\n');
            w.write_all(buf.as_bytes())
                .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?;
        }
        w.finish().map_err(db)?;
        Ok(())
    }

    /// Record bulk-loaded commits and move the branch head to the last one; the commit
    /// identifier sequence continues after it. `commits` are `(id, cseq, record)`.
    pub fn bulk_commits(
        &mut self,
        branch: &str,
        commits: &[(u64, u64, Value)],
    ) -> Result<(), BpaError> {
        let mut tx = self.client.transaction().map_err(db)?;
        for (id, cseq, rec) in commits {
            tx.execute(W_COMMIT, &[&int(*id), &branch, &int(*cseq), rec])
                .map_err(db)?;
        }
        if let Some((id, cseq, _)) = commits.last() {
            tx.execute(
                "UPDATE bpa_branch SET head = $1, head_cseq = $2 WHERE name = $3",
                &[&int(*id), &int(*cseq), &branch],
            )
            .map_err(db)?;
            tx.execute("SELECT setval('bpa_commit_id', $1)", &[&int(*id)])
                .map_err(db)?;
        }
        tx.commit().map_err(db)?;
        self.client.batch_execute("ANALYZE").map_err(db)
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
            let r = self
                .client
                .query_one("SELECT current_setting($1)", &[&name])
                .map_err(db)?;
            out.insert(name.to_string(), Value::String(r.get(0)));
        }
        Ok(Value::Object(out))
    }

    /// Connect (for example `host=127.0.0.1 user=postgres dbname=bpa`) and create the schema.
    ///
    /// Tables of the step-4 layout (full state per commit) are not migrated: drop them first.
    pub fn connect(url: &str) -> Result<PgStore, BpaError> {
        let mut client = Client::connect(url, NoTls).map_err(db)?;
        client.batch_execute(SCHEMA).map_err(db)?;
        let old = client
            .query_opt(
                "SELECT 1 FROM information_schema.columns WHERE table_name = 'bpa_branch' AND column_name = 'head_cseq'",
                &[],
            )
            .map_err(db)?;
        if old.is_none() {
            return Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                "tables of an older prototype layout exist; drop the bpa_* tables",
            ));
        }
        Ok(PgStore {
            client,
            statements: HashMap::new(),
            ids: Vec::new(),
            nodes: HashMap::new(),
            versions: HashMap::new(),
            commits: HashMap::new(),
        })
    }

    /// A prepared statement, cached per connection.
    fn stmt(&mut self, sql: &'static str) -> Result<Statement, BpaError> {
        if let Some(s) = self.statements.get(sql) {
            return Ok(s.clone());
        }
        let s = self.client.prepare(sql).map_err(db)?;
        self.statements.insert(sql, s.clone());
        Ok(s)
    }

    fn opt(
        &mut self,
        sql: &'static str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<postgres::Row>, BpaError> {
        let s = self.stmt(sql)?;
        self.client.query_opt(&s, params).map_err(db)
    }

    fn all(
        &mut self,
        sql: &'static str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<postgres::Row>, BpaError> {
        let s = self.stmt(sql)?;
        self.client.query(&s, params).map_err(db)
    }
}

const Q_OBJECT: &str = "SELECT state FROM bpa_object WHERE branch = $1 AND object_id = $3 AND cseq_from <= $2 AND (cseq_to IS NULL OR cseq_to > $2)";
const Q_OBJECTS: &str = "SELECT object_id, state FROM bpa_object WHERE branch = $1 AND cseq_from <= $2 AND (cseq_to IS NULL OR cseq_to > $2)";
const Q_OBJECTS_CLASS: &str = "SELECT object_id, state FROM bpa_object WHERE branch = $1 AND class = $3 AND cseq_from <= $2 AND (cseq_to IS NULL OR cseq_to > $2)";
const Q_ENTRIES: &str = "SELECT entry FROM bpa_entry WHERE branch = $1 AND cseq <= $2 ORDER BY ord";
const Q_ENTRIES_KEY: &str = "SELECT entry FROM bpa_entry WHERE branch = $1 AND cseq <= $2 AND class = $3 AND ledger_key = $4 ORDER BY ord";
const Q_ENTRY_ID: &str =
    "SELECT entry FROM bpa_entry WHERE branch = $1 AND cseq <= $2 AND entry_id = $3 ORDER BY ord";
/// The whole flush as one statement: the head compare-and-set gates every insert, so the
/// statement either writes everything or nothing (DSN-DVM-208), in one round trip.
const W_FLUSH: &str = "
WITH h AS (
  UPDATE bpa_branch SET head = $1, head_cseq = $2 WHERE name = $3 AND head = $4 RETURNING 1),
v AS (
  INSERT INTO bpa_version (hash, record)
  SELECT t.h, t.r FROM unnest($5::text[], $6::jsonb[]) AS t(h, r) WHERE EXISTS (SELECT 1 FROM h)
  ON CONFLICT DO NOTHING),
c AS (
  INSERT INTO bpa_commit (id, branch, cseq, record)
  SELECT $1, $3, $2, $7 WHERE EXISTS (SELECT 1 FROM h)),
x AS (
  UPDATE bpa_object SET cseq_to = $2
  WHERE branch = $3 AND object_id = ANY($8::text[]) AND cseq_to IS NULL AND EXISTS (SELECT 1 FROM h)),
o AS (
  INSERT INTO bpa_object (branch, object_id, cseq_from, class, state)
  SELECT $3, t.id, $2, t.class, t.state FROM unnest($8::text[], $9::text[], $10::jsonb[]) AS t(id, class, state)
  WHERE EXISTS (SELECT 1 FROM h)),
e AS (
  INSERT INTO bpa_entry (branch, cseq, entry_id, class, ledger_key, entry)
  SELECT $3, $2, t.id, t.class, t.k, t.e
  FROM unnest($11::text[], $12::text[], $13::text[], $14::jsonb[]) WITH ORDINALITY AS t(id, class, k, e, n)
  WHERE EXISTS (SELECT 1 FROM h) ORDER BY t.n),
n AS (
  INSERT INTO bpa_tree_node (hash, node)
  SELECT t.h, t.n FROM unnest($15::text[], $16::jsonb[]) AS t(h, n) WHERE EXISTS (SELECT 1 FROM h)
  ON CONFLICT DO NOTHING)
SELECT count(*) FROM h";
const W_COMMIT: &str = "INSERT INTO bpa_commit (id, branch, cseq, record) VALUES ($1, $2, $3, $4)";

/// Parameters of [`W_FLUSH`] for one flush.
struct FlushParams<'a> {
    id: i64,
    cseq: i64,
    head: i64,
    f: &'a Flush,
    vh: Vec<&'a String>,
    vr: Vec<&'a Value>,
    oi: Vec<&'a String>,
    oc: Vec<&'a String>,
    os: Vec<Option<&'a Value>>,
    ei: Vec<&'a String>,
    ec: Vec<&'a String>,
    ek: Vec<&'a String>,
    ee: Vec<&'a Value>,
    nh: Vec<&'a String>,
    nn: Vec<&'a Value>,
}

impl FlushParams<'_> {
    fn refs(&self) -> Vec<&(dyn ToSql + Sync)> {
        vec![
            &self.id,
            &self.cseq,
            &self.f.branch,
            &self.head,
            &self.vh,
            &self.vr,
            &self.f.commit,
            &self.oi,
            &self.oc,
            &self.os,
            &self.ei,
            &self.ec,
            &self.ek,
            &self.ee,
            &self.nh,
            &self.nn,
        ]
    }
}

fn flush_params(f: &Flush) -> FlushParams<'_> {
    FlushParams {
        id: int(f.commit_id),
        cseq: int(f.cseq),
        head: int(f.expected_head),
        f,
        vh: f.versions.iter().map(|v| &v.0).collect(),
        vr: f.versions.iter().map(|v| &v.1).collect(),
        oi: f.objects.iter().map(|o| &o.object_id).collect(),
        oc: f.objects.iter().map(|o| &o.class).collect(),
        os: f.objects.iter().map(|o| o.state.as_ref()).collect(),
        ei: f.entries.iter().map(|e| &e.entry_id).collect(),
        ec: f.entries.iter().map(|e| &e.class).collect(),
        ek: f.entries.iter().map(|e| &e.ledger_key).collect(),
        ee: f.entries.iter().map(|e| &e.entry).collect(),
        nh: f.nodes.iter().map(|n| &n.0).collect(),
        nn: f.nodes.iter().map(|n| &n.1).collect(),
    }
}

impl Store for PgStore {
    fn reset(&mut self) -> Result<(), BpaError> {
        self.ids.clear();
        self.nodes.clear();
        self.versions.clear();
        self.commits.clear();
        let sql =
            format!("TRUNCATE {TABLES} RESTART IDENTITY; ALTER SEQUENCE bpa_commit_id RESTART;");
        self.client.batch_execute(&sql).map_err(db)
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        if let Some(v) = self.versions.get(hash) {
            return Ok(v.clone());
        }
        let v: Value = self
            .opt("SELECT record FROM bpa_version WHERE hash = $1", &[&hash])?
            .map(|r| r.get(0))
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing version"))?;
        remember(&mut self.versions, hash.to_string(), &v);
        Ok(v)
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        if let Some(v) = self.commits.get(&id) {
            return Ok(v.clone());
        }
        let v: Value = self
            .opt("SELECT record FROM bpa_commit WHERE id = $1", &[&int(id)])?
            .map(|r| r.get(0))
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing commit"))?;
        remember(&mut self.commits, id, &v);
        Ok(v)
    }
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        Ok(self
            .opt(
                "SELECT id FROM bpa_commit WHERE branch = $1 AND cseq = $2",
                &[&branch, &int(cseq)],
            )?
            .map(|r| r.get::<_, i64>(0) as u64))
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        if self.ids.is_empty() {
            let rows = self.all(
                "SELECT nextval('bpa_commit_id') FROM generate_series(1, 64)",
                &[],
            )?;
            self.ids = rows
                .iter()
                .rev()
                .map(|r| r.get::<_, i64>(0) as u64)
                .collect();
        }
        self.ids
            .pop()
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "no commit identifier"))
    }
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError> {
        Ok(self
            .opt(
                "SELECT head, head_cseq, parent, fork_cseq FROM bpa_branch WHERE name = $1",
                &[&name],
            )?
            .map(|r| BranchRef {
                head: r.get::<_, i64>(0) as u64,
                head_cseq: r.get::<_, i64>(1) as u64,
                parent: r
                    .get::<_, Option<String>>(2)
                    .map(|p| (p, r.get::<_, Option<i64>>(3).unwrap_or_default() as u64)),
            }))
    }
    fn create_branch(&mut self, name: &str, r: &BranchRef) -> Result<bool, BpaError> {
        let parent = r.parent.as_ref().map(|p| p.0.clone());
        let fork = r.parent.as_ref().map(|p| int(p.1));
        let s = self.stmt("INSERT INTO bpa_branch (name, head, head_cseq, parent, fork_cseq) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING")?;
        let n = self
            .client
            .execute(
                &s,
                &[&name, &int(r.head), &int(r.head_cseq), &parent, &fork],
            )
            .map_err(db)?;
        Ok(n == 1)
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        let st = self.stmt(W_FLUSH)?;
        let row = self
            .client
            .query_one(&st, &flush_params(f).refs())
            .map_err(db)?;
        if row.get::<_, i64>(0) != 1 {
            return Ok(false);
        }
        remember(&mut self.commits, f.commit_id, &f.commit);
        for (h, n) in &f.nodes {
            remember(&mut self.nodes, h.clone(), n);
        }
        Ok(true)
    }
    fn flush_many(&mut self, fs: &[Flush]) -> Result<bool, BpaError> {
        let st = self.stmt(W_FLUSH)?;
        let mut tx = self.client.transaction().map_err(db)?;
        for f in fs {
            let row = tx.query_one(&st, &flush_params(f).refs()).map_err(db)?;
            if row.get::<_, i64>(0) != 1 {
                tx.rollback().map_err(db)?;
                return Ok(false);
            }
        }
        tx.commit().map_err(db)?;
        for f in fs {
            remember(&mut self.commits, f.commit_id, &f.commit);
            for (h, n) in &f.nodes {
                remember(&mut self.nodes, h.clone(), n);
            }
        }
        Ok(true)
    }
    fn object(
        &mut self,
        branch: &str,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Value>>, BpaError> {
        Ok(self
            .opt(Q_OBJECT, &[&branch, &int(cseq), &object_id])?
            .map(|r| r.get::<_, Option<Value>>(0)))
    }
    fn objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let rows = match class {
            Some(c) => self.all(Q_OBJECTS_CLASS, &[&branch, &int(cseq), &c])?,
            None => self.all(Q_OBJECTS, &[&branch, &int(cseq)])?,
        };
        Ok(rows.into_iter().map(|r| (r.get(0), r.get(1))).collect())
    }
    fn entries(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Value>, BpaError> {
        let c = int(cseq);
        let rows = match (key, entry_id) {
            (_, Some(id)) => self.all(Q_ENTRY_ID, &[&branch, &c, &id])?,
            (Some((class, k)), None) => self.all(Q_ENTRIES_KEY, &[&branch, &c, &class, &k])?,
            (None, None) => self.all(Q_ENTRIES, &[&branch, &c])?,
        };
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }
    fn node(&mut self, hash: &str) -> Result<Value, BpaError> {
        if let Some(v) = self.nodes.get(hash) {
            return Ok(v.clone());
        }
        let v: Value = self
            .opt("SELECT node FROM bpa_tree_node WHERE hash = $1", &[&hash])?
            .map(|r| r.get(0))
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing tree node"))?;
        remember(&mut self.nodes, hash.to_string(), &v);
        Ok(v)
    }
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError> {
        let s = self.stmt("INSERT INTO bpa_meta (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")?;
        self.client.execute(&s, &[&key, value]).map_err(db)?;
        Ok(())
    }
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        Ok(self
            .opt("SELECT value FROM bpa_meta WHERE key = $1", &[&key])?
            .map(|r| r.get(0)))
    }
    fn backend(&self) -> &'static str {
        "postgresql"
    }
}
