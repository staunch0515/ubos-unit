//! `ubos-store` — engine layer (UBS-ARC-07 §2).
//!
//! Storage traits; the PostgreSQL adapter (SQLite follows in PH-4, DEC-024); chunk store;
//! kernel index maintenance. Allowed dependencies: `ubos-types`, `ubos-crypto` (AR-031).
//!
//! PH-0 prototype scope (DEC-033): a store of content-addressed version records, commit
//! records and branch heads, with an in-memory backend and a PostgreSQL backend. A flush
//! writes the new versions, the commit and the head compare-and-set in one storage
//! transaction (DSN-DVM-208). Commit records hold the full branch state; the layout with
//! kernel indexes and stored tree nodes for the benchmarks (DSN-DVM-001…011) is PH-0 step 5.

use serde_json::Value;
use std::collections::HashMap;
use ubos_types::BpaError;

mod pg;

pub use pg::PgStore;

/// Everything a flush writes in one storage transaction.
#[derive(Debug, Clone)]
pub struct Flush {
    /// New version records by hash.
    pub versions: Vec<(String, Value)>,
    /// Identifier of the new commit.
    pub commit_id: u64,
    /// The commit record.
    pub commit: Value,
    /// Branch whose head moves.
    pub branch: String,
    /// Head the flush was built on (compare-and-set).
    pub expected_head: u64,
}

/// Storage of versions, commits and branch heads.
pub trait Store {
    /// Remove all data (adapter `reset`, STD-CONF-002).
    fn reset(&mut self) -> Result<(), BpaError>;
    /// A version record by hash.
    fn version(&mut self, hash: &str) -> Result<Value, BpaError>;
    /// A commit record by identifier.
    fn commit(&mut self, id: u64) -> Result<Value, BpaError>;
    /// A fresh commit identifier.
    fn next_commit_id(&mut self) -> Result<u64, BpaError>;
    /// The head of a branch.
    fn head(&mut self, branch: &str) -> Result<Option<u64>, BpaError>;
    /// Create a branch pointing at `head`; false when the name exists.
    fn create_branch(&mut self, branch: &str, head: u64) -> Result<bool, BpaError>;
    /// Write a flush atomically; false (nothing written) when the head moved.
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError>;
    /// Store a value under a key (class declarations, adapter clock).
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError>;
    /// Read a value stored with `put_meta`.
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError>;
    /// Backend name for reports.
    fn backend(&self) -> &'static str;
}

/// In-memory backend.
#[derive(Default)]
pub struct MemStore {
    versions: HashMap<String, Value>,
    commits: HashMap<u64, Value>,
    heads: HashMap<String, u64>,
    meta: HashMap<String, Value>,
    next: u64,
}

fn missing(what: &str) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", format!("missing {what}"))
}

impl Store for MemStore {
    fn reset(&mut self) -> Result<(), BpaError> {
        *self = MemStore::default();
        Ok(())
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.versions
            .get(hash)
            .cloned()
            .ok_or_else(|| missing("version"))
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        self.commits
            .get(&id)
            .cloned()
            .ok_or_else(|| missing("commit"))
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        self.next += 1;
        Ok(self.next)
    }
    fn head(&mut self, branch: &str) -> Result<Option<u64>, BpaError> {
        Ok(self.heads.get(branch).copied())
    }
    fn create_branch(&mut self, branch: &str, head: u64) -> Result<bool, BpaError> {
        if self.heads.contains_key(branch) {
            return Ok(false);
        }
        self.heads.insert(branch.to_string(), head);
        Ok(true)
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        if self.heads.get(&f.branch) != Some(&f.expected_head) {
            return Ok(false);
        }
        for (h, v) in &f.versions {
            self.versions.insert(h.clone(), v.clone());
        }
        self.commits.insert(f.commit_id, f.commit.clone());
        self.heads.insert(f.branch.clone(), f.commit_id);
        Ok(true)
    }
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError> {
        self.meta.insert(key.to_string(), value.clone());
        Ok(())
    }
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        Ok(self.meta.get(key).cloned())
    }
    fn backend(&self) -> &'static str {
        "memory"
    }
}
