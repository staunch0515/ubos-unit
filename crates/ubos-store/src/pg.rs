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
}

impl PgStore {
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
const W_HEAD: &str =
    "UPDATE bpa_branch SET head = $1, head_cseq = $2 WHERE name = $3 AND head = $4";
const W_VERSION: &str =
    "INSERT INTO bpa_version (hash, record) VALUES ($1, $2) ON CONFLICT DO NOTHING";
const W_COMMIT: &str = "INSERT INTO bpa_commit (id, branch, cseq, record) VALUES ($1, $2, $3, $4)";
const W_CLOSE: &str =
    "UPDATE bpa_object SET cseq_to = $1 WHERE branch = $2 AND object_id = $3 AND cseq_to IS NULL";
const W_OBJECT: &str = "INSERT INTO bpa_object (branch, object_id, cseq_from, class, state) VALUES ($1, $2, $3, $4, $5)";
const W_ENTRY: &str = "INSERT INTO bpa_entry (branch, cseq, entry_id, class, ledger_key, entry) VALUES ($1, $2, $3, $4, $5, $6)";
const W_NODE: &str =
    "INSERT INTO bpa_tree_node (hash, node) VALUES ($1, $2) ON CONFLICT DO NOTHING";

impl Store for PgStore {
    fn reset(&mut self) -> Result<(), BpaError> {
        let sql =
            format!("TRUNCATE {TABLES} RESTART IDENTITY; ALTER SEQUENCE bpa_commit_id RESTART;");
        self.client.batch_execute(&sql).map_err(db)
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.opt("SELECT record FROM bpa_version WHERE hash = $1", &[&hash])?
            .map(|r| r.get(0))
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing version"))
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        self.opt("SELECT record FROM bpa_commit WHERE id = $1", &[&int(id)])?
            .map(|r| r.get(0))
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing commit"))
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
        let r = self.opt("SELECT nextval('bpa_commit_id')", &[])?;
        Ok(r.map(|r| r.get::<_, i64>(0) as u64).unwrap_or_default())
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
        let st: Vec<Statement> = [
            W_HEAD, W_VERSION, W_COMMIT, W_CLOSE, W_OBJECT, W_ENTRY, W_NODE,
        ]
        .into_iter()
        .map(|q| self.stmt(q))
        .collect::<Result<_, _>>()?;
        let cseq = int(f.cseq);
        let mut tx = self.client.transaction().map_err(db)?;
        let moved = tx
            .execute(
                &st[0],
                &[&int(f.commit_id), &cseq, &f.branch, &int(f.expected_head)],
            )
            .map_err(db)?;
        if moved != 1 {
            tx.rollback().map_err(db)?;
            return Ok(false);
        }
        for (h, v) in &f.versions {
            tx.execute(&st[1], &[h, v]).map_err(db)?;
        }
        tx.execute(&st[2], &[&int(f.commit_id), &f.branch, &cseq, &f.commit])
            .map_err(db)?;
        for o in &f.objects {
            tx.execute(&st[3], &[&cseq, &f.branch, &o.object_id])
                .map_err(db)?;
            tx.execute(
                &st[4],
                &[&f.branch, &o.object_id, &cseq, &o.class, &o.state],
            )
            .map_err(db)?;
        }
        for e in &f.entries {
            tx.execute(
                &st[5],
                &[
                    &f.branch,
                    &cseq,
                    &e.entry_id,
                    &e.class,
                    &e.ledger_key,
                    &e.entry,
                ],
            )
            .map_err(db)?;
        }
        for (h, n) in &f.nodes {
            tx.execute(&st[6], &[h, n]).map_err(db)?;
        }
        tx.commit().map_err(db)?;
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
        self.opt("SELECT node FROM bpa_tree_node WHERE hash = $1", &[&hash])?
            .map(|r| r.get(0))
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing tree node"))
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
