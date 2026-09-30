//! `ubos-store` — engine layer (UBS-ARC-07 §2).
//!
//! Storage traits; the PostgreSQL adapter (SQLite follows in PH-4, DEC-024); chunk store;
//! kernel index maintenance. Allowed dependencies: `ubos-types`, `ubos-crypto` (AR-031).
//!
//! Two layers (DSN-DVM-001, DEC-047):
//!
//! * [`kernel`]: the backend-neutral storage traits `StoreTx` (one storage transaction with
//!   the write operations of the DVM) and `StoreRead` (snapshot reads by commit ordinal),
//!   implemented for PostgreSQL on sqlx ([`PgBackend`], migrations in `migrations/`);
//! * [`Store`]: the synchronous engine view used by the version layer, with the in-memory
//!   reference [`MemStore`], the PostgreSQL view [`PgStore`] over the kernel traits, and the
//!   commit [`Group`] overlay.
//!
//! A flush writes versions, the commit, ledger entries with sequences from the counter rows
//! (DSN-DVM-402), the kernel indexes (DSN-DVM-005) and tree nodes (DSN-DVM-007), and moves
//! the branch head under the sequencer lock in one storage transaction (DSN-DVM-006, -208).
//! Rows are local to a branch; reads of a branch fall back to its parent at the fork point,
//! which the version layer resolves.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use ubos_types::BpaError;

pub mod kernel;
mod pg;

pub use pg::{BulkRows, DEFAULT_SCOPE, PgBackend, PgStore, PgTx, key_hash};

/// A branch reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRef {
    /// Head commit identifier.
    pub head: u64,
    /// Ordinal of the head commit.
    pub head_cseq: u64,
    /// Parent branch and the ordinal it was forked at.
    pub parent: Option<(String, u64)>,
}

/// An object-index row written by a flush: the object's state after the commit, or `None`
/// when the object no longer exists on the branch.
#[derive(Debug, Clone)]
pub struct ObjectRow {
    /// Object identifier.
    pub object_id: String,
    /// Class name (for class-scoped reads).
    pub class: String,
    /// Object state (class, kind, slug, head, timeline).
    pub state: Option<Value>,
}

/// A ledger entry written by a flush.
#[derive(Debug, Clone)]
pub struct EntryRow {
    /// Entry identifier.
    pub entry_id: String,
    /// Ledger class.
    pub class: String,
    /// Canonical ledger key.
    pub ledger_key: String,
    /// The entry.
    pub entry: Value,
}

/// Everything a flush writes in one storage transaction.
#[derive(Debug, Clone, Default)]
pub struct Flush {
    /// New version records by hash.
    pub versions: Vec<(String, Value)>,
    /// Identifier of the new commit.
    pub commit_id: u64,
    /// The commit record.
    pub commit: Value,
    /// Branch whose head moves.
    pub branch: String,
    /// Ordinal of the new commit on the branch.
    pub cseq: u64,
    /// Head the flush was built on (compare-and-set).
    pub expected_head: u64,
    /// Changed objects.
    pub objects: Vec<ObjectRow>,
    /// Appended ledger entries, in order.
    pub entries: Vec<EntryRow>,
    /// New tree nodes by hash.
    pub nodes: Vec<(String, Value)>,
}

/// Storage of versions, commits, branches, kernel indexes and tree nodes.
///
/// "Local" reads see only the rows written on the named branch with `cseq` at most the
/// given ordinal.
pub trait Store {
    /// Remove all data (adapter `reset`, STD-CONF-002).
    fn reset(&mut self) -> Result<(), BpaError>;
    /// A version record by hash.
    fn version(&mut self, hash: &str) -> Result<Value, BpaError>;
    /// A commit record by identifier.
    fn commit(&mut self, id: u64) -> Result<Value, BpaError>;
    /// The commit of a branch with the given ordinal, among the branch's own commits.
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError>;
    /// A fresh commit identifier.
    fn next_commit_id(&mut self) -> Result<u64, BpaError>;
    /// A branch reference.
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError>;
    /// Create a branch; false when the name exists.
    fn create_branch(&mut self, name: &str, r: &BranchRef) -> Result<bool, BpaError>;
    /// Write a flush atomically; false (nothing written) when the head moved.
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError>;
    /// Write consecutive flushes (group commit, DSN-DVM-006); false when a head moved.
    /// The default writes them one by one; PostgreSQL writes them in one transaction.
    fn flush_many(&mut self, fs: &[Flush]) -> Result<bool, BpaError> {
        for f in fs {
            if !self.flush(f)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// The local row of one object: `Some(state or None)` when the branch has one.
    fn object(
        &mut self,
        branch: &str,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Value>>, BpaError>;
    /// All local object rows visible at `cseq`, optionally of one class.
    fn objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError>;
    /// Local ledger entries up to `cseq` in append order, optionally of one ledger key or
    /// with one entry identifier.
    fn entries(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Value>, BpaError>;
    /// A stored tree node.
    fn node(&mut self, hash: &str) -> Result<Value, BpaError>;
    /// Store a value under a key (class declarations, adapter clock).
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError>;
    /// Read a value stored with `put_meta`.
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError>;
    /// Backend name for reports.
    fn backend(&self) -> &'static str;
}

/// An object-index row as kept in memory.
#[derive(Debug, Clone)]
struct MemObject {
    class: String,
    cseq_from: u64,
    cseq_to: Option<u64>,
    state: Option<Value>,
}

/// A ledger row as kept in memory.
#[derive(Debug, Clone)]
struct MemEntry {
    cseq: u64,
    entry_id: String,
    class: String,
    ledger_key: String,
    entry: Value,
}

/// In-memory backend with the same row model as PostgreSQL.
#[derive(Default)]
pub struct MemStore {
    versions: HashMap<String, Value>,
    commits: HashMap<u64, Value>,
    by_cseq: HashMap<(String, u64), u64>,
    branches: HashMap<String, BranchRef>,
    objects: HashMap<String, BTreeMap<String, Vec<MemObject>>>,
    entries: HashMap<String, Vec<MemEntry>>,
    nodes: HashMap<String, Value>,
    meta: HashMap<String, Value>,
    next: u64,
}

fn missing(what: &str) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", format!("missing {what}"))
}

fn visible(r: &MemObject, cseq: u64) -> bool {
    r.cseq_from <= cseq && r.cseq_to.is_none_or(|t| t > cseq)
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
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        Ok(self.by_cseq.get(&(branch.to_string(), cseq)).copied())
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        self.next += 1;
        Ok(self.next)
    }
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError> {
        Ok(self.branches.get(name).cloned())
    }
    fn create_branch(&mut self, name: &str, r: &BranchRef) -> Result<bool, BpaError> {
        if self.branches.contains_key(name) {
            return Ok(false);
        }
        self.branches.insert(name.to_string(), r.clone());
        Ok(true)
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        let Some(b) = self.branches.get_mut(&f.branch) else {
            return Ok(false);
        };
        if b.head != f.expected_head {
            return Ok(false);
        }
        b.head = f.commit_id;
        b.head_cseq = f.cseq;
        for (h, v) in &f.versions {
            self.versions.insert(h.clone(), v.clone());
        }
        self.commits.insert(f.commit_id, f.commit.clone());
        self.by_cseq.insert((f.branch.clone(), f.cseq), f.commit_id);
        let objects = self.objects.entry(f.branch.clone()).or_default();
        for o in &f.objects {
            let rows = objects.entry(o.object_id.clone()).or_default();
            if let Some(last) = rows.last_mut()
                && last.cseq_to.is_none()
            {
                last.cseq_to = Some(f.cseq);
            }
            rows.push(MemObject {
                class: o.class.clone(),
                cseq_from: f.cseq,
                cseq_to: None,
                state: o.state.clone(),
            });
        }
        let entries = self.entries.entry(f.branch.clone()).or_default();
        for e in &f.entries {
            entries.push(MemEntry {
                cseq: f.cseq,
                entry_id: e.entry_id.clone(),
                class: e.class.clone(),
                ledger_key: e.ledger_key.clone(),
                entry: e.entry.clone(),
            });
        }
        for (h, n) in &f.nodes {
            self.nodes.insert(h.clone(), n.clone());
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
            .objects
            .get(branch)
            .and_then(|m| m.get(object_id))
            .and_then(|rows| rows.iter().find(|r| visible(r, cseq)))
            .map(|r| r.state.clone()))
    }
    fn objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let mut out = Vec::new();
        for (id, rows) in self.objects.get(branch).into_iter().flatten() {
            if let Some(r) = rows.iter().find(|r| visible(r, cseq))
                && class.is_none_or(|c| c == r.class)
            {
                out.push((id.clone(), r.state.clone()));
            }
        }
        Ok(out)
    }
    fn entries(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Value>, BpaError> {
        Ok(self
            .entries
            .get(branch)
            .into_iter()
            .flatten()
            .filter(|e| e.cseq <= cseq)
            .filter(|e| key.is_none_or(|(c, k)| e.class == c && e.ledger_key == k))
            .filter(|e| entry_id.is_none_or(|i| e.entry_id == i))
            .map(|e| e.entry.clone())
            .collect())
    }
    fn node(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.nodes
            .get(hash)
            .cloned()
            .ok_or_else(|| missing("tree node"))
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

/// A group of commits built on top of a store (DSN-DVM-006): flushes go to an in-memory
/// overlay that later commits of the group read through, and [`Group::finish`] writes them
/// all with [`Store::flush_many`].
pub struct Group {
    inner: Box<dyn Store>,
    overlay: MemStore,
    pending: Vec<Flush>,
}

impl Group {
    /// Start a group over a store.
    pub fn new(inner: Box<dyn Store>) -> Group {
        Group {
            inner,
            overlay: MemStore::default(),
            pending: Vec::new(),
        }
    }

    /// Write the group; returns the store and whether the flushes were written.
    pub fn finish(mut self) -> (Box<dyn Store>, Result<bool, BpaError>) {
        let r = if self.pending.is_empty() {
            Ok(true)
        } else {
            self.inner.flush_many(&self.pending)
        };
        (self.inner, r)
    }

    /// Number of flushes in the group.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// True when no flush is pending.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

fn unsupported(what: &str) -> BpaError {
    BpaError::new(
        "ADAPTER.UNSUPPORTED",
        format!("{what} inside a commit group"),
    )
}

impl Store for Group {
    fn reset(&mut self) -> Result<(), BpaError> {
        Err(unsupported("reset"))
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        match self.overlay.versions.get(hash) {
            Some(v) => Ok(v.clone()),
            None => self.inner.version(hash),
        }
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        match self.overlay.commits.get(&id) {
            Some(v) => Ok(v.clone()),
            None => self.inner.commit(id),
        }
    }
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        match self.overlay.commit_at(branch, cseq)? {
            Some(id) => Ok(Some(id)),
            None => self.inner.commit_at(branch, cseq),
        }
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        self.inner.next_commit_id()
    }
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError> {
        match self.overlay.branches.get(name) {
            Some(r) => Ok(Some(r.clone())),
            None => self.inner.branch(name),
        }
    }
    fn create_branch(&mut self, _name: &str, _r: &BranchRef) -> Result<bool, BpaError> {
        Err(unsupported("create_branch"))
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        if !self.overlay.branches.contains_key(&f.branch) {
            let Some(r) = self.inner.branch(&f.branch)? else {
                return Ok(false);
            };
            self.overlay.branches.insert(f.branch.clone(), r);
        }
        if !self.overlay.flush(f)? {
            return Ok(false);
        }
        self.pending.push(f.clone());
        Ok(true)
    }
    fn object(
        &mut self,
        branch: &str,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Value>>, BpaError> {
        match self.overlay.object(branch, cseq, object_id)? {
            Some(row) => Ok(Some(row)),
            None => self.inner.object(branch, cseq, object_id),
        }
    }
    fn objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let mut out: BTreeMap<String, Option<Value>> = self
            .inner
            .objects(branch, cseq, class)?
            .into_iter()
            .collect();
        out.extend(self.overlay.objects(branch, cseq, class)?);
        Ok(out.into_iter().collect())
    }
    fn entries(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Value>, BpaError> {
        let mut out = self.inner.entries(branch, cseq, key, entry_id)?;
        out.extend(self.overlay.entries(branch, cseq, key, entry_id)?);
        Ok(out)
    }
    fn node(&mut self, hash: &str) -> Result<Value, BpaError> {
        match self.overlay.nodes.get(hash) {
            Some(v) => Ok(v.clone()),
            None => self.inner.node(hash),
        }
    }
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError> {
        self.inner.put_meta(key, value)
    }
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        self.inner.meta(key)
    }
    fn backend(&self) -> &'static str {
        self.inner.backend()
    }
}
