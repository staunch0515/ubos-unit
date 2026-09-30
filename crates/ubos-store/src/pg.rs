//! PostgreSQL backend (DEC-024, DEC-033).

use crate::{Flush, Store};
use postgres::{Client, NoTls};
use serde_json::Value;
use ubos_types::BpaError;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS bpa_version (hash text PRIMARY KEY, record jsonb NOT NULL);
CREATE TABLE IF NOT EXISTS bpa_commit (id bigint PRIMARY KEY, record jsonb NOT NULL);
CREATE TABLE IF NOT EXISTS bpa_branch (name text PRIMARY KEY, head bigint NOT NULL);
CREATE TABLE IF NOT EXISTS bpa_meta (key text PRIMARY KEY, value jsonb NOT NULL);
CREATE SEQUENCE IF NOT EXISTS bpa_commit_id;
";

fn db(e: postgres::Error) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

/// A store in a PostgreSQL database.
pub struct PgStore {
    client: Client,
}

impl PgStore {
    /// Connect (for example `host=127.0.0.1 user=postgres dbname=bpa`) and create the schema.
    pub fn connect(url: &str) -> Result<PgStore, BpaError> {
        let mut client = Client::connect(url, NoTls).map_err(db)?;
        client.batch_execute(SCHEMA).map_err(db)?;
        Ok(PgStore { client })
    }
}

impl Store for PgStore {
    fn reset(&mut self) -> Result<(), BpaError> {
        self.client
            .batch_execute("TRUNCATE bpa_version, bpa_commit, bpa_branch, bpa_meta; ALTER SEQUENCE bpa_commit_id RESTART;")
            .map_err(db)
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        let row = self
            .client
            .query_one("SELECT record FROM bpa_version WHERE hash = $1", &[&hash])
            .map_err(db)?;
        Ok(row.get(0))
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        let row = self
            .client
            .query_one(
                "SELECT record FROM bpa_commit WHERE id = $1",
                &[&(id as i64)],
            )
            .map_err(db)?;
        Ok(row.get(0))
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        let row = self
            .client
            .query_one("SELECT nextval('bpa_commit_id')", &[])
            .map_err(db)?;
        Ok(row.get::<_, i64>(0) as u64)
    }
    fn head(&mut self, branch: &str) -> Result<Option<u64>, BpaError> {
        let row = self
            .client
            .query_opt("SELECT head FROM bpa_branch WHERE name = $1", &[&branch])
            .map_err(db)?;
        Ok(row.map(|r| r.get::<_, i64>(0) as u64))
    }
    fn create_branch(&mut self, branch: &str, head: u64) -> Result<bool, BpaError> {
        let n = self
            .client
            .execute(
                "INSERT INTO bpa_branch (name, head) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                &[&branch, &(head as i64)],
            )
            .map_err(db)?;
        Ok(n == 1)
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        let mut tx = self.client.transaction().map_err(db)?;
        for (h, v) in &f.versions {
            tx.execute(
                "INSERT INTO bpa_version (hash, record) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                &[h, v],
            )
            .map_err(db)?;
        }
        tx.execute(
            "INSERT INTO bpa_commit (id, record) VALUES ($1, $2)",
            &[&(f.commit_id as i64), &f.commit],
        )
        .map_err(db)?;
        let moved = tx
            .execute(
                "UPDATE bpa_branch SET head = $1 WHERE name = $2 AND head = $3",
                &[&(f.commit_id as i64), &f.branch, &(f.expected_head as i64)],
            )
            .map_err(db)?;
        if moved != 1 {
            tx.rollback().map_err(db)?;
            return Ok(false);
        }
        tx.commit().map_err(db)?;
        Ok(true)
    }
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError> {
        self.client
            .execute(
                "INSERT INTO bpa_meta (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
                &[&key, value],
            )
            .map_err(db)?;
        Ok(())
    }
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        let row = self
            .client
            .query_opt("SELECT value FROM bpa_meta WHERE key = $1", &[&key])
            .map_err(db)?;
        Ok(row.map(|r| r.get(0)))
    }
    fn backend(&self) -> &'static str {
        "postgresql"
    }
}
