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
pub mod node;
mod pg;
pub mod physical;
pub mod verify;

pub use pg::{BulkRows, DEFAULT_SCOPE, PgBackend, PgStore, PgTx, key_hash, options as pg_options};

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
/// when the object no longer exists on the branch. The commit record carries these rows as
/// its write set (`objects`), from which the kernel indexes are rebuilt (DEC-053).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObjectRow {
    /// Object identifier.
    pub object_id: String,
    /// Class name (for class-scoped reads).
    pub class: String,
    /// Valid-time granularity of the class: true for `instant`, false for `date`.
    #[serde(default)]
    pub instant: bool,
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
    /// Effects released with the commit (STD-TXN-050): `{effect_id, type, branch, cseq,
    /// object_id?, payload?}`.
    pub outbox: Vec<Value>,
    /// Idempotency record of the commit (STD-TXN-040): `(scope, key, request hash,
    /// outcome)`.
    pub idempotency: Option<(String, String, String, Value)>,
    /// Derivation records of the commit (STD-TXN-030): `{id, record, inputs, outputs}`.
    pub derivations: Vec<Value>,
    /// Future segment starts to emit BecameEffective for (STD-TIME-050): `{key: [branch,
    /// object_id, from, version], class, instant}`.
    pub timers: Vec<Value>,
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
    /// The commit identifier (content ID) of a commit, without the rest of its record
    /// (the evaluation evidence of a read, STD-TIME-015).
    fn commit_id(&mut self, id: u64) -> Result<String, BpaError> {
        Ok(self.commit(id)?["commit_id"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }
    /// The commit of a branch with the given ordinal, among the branch's own commits.
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError>;
    /// The commit with a commit identifier (content ID), on any branch.
    fn commit_by_id(&mut self, commit_id: &str) -> Result<Option<u64>, BpaError>;
    /// A fresh commit identifier.
    fn next_commit_id(&mut self) -> Result<u64, BpaError>;
    /// A branch reference.
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError>;
    /// Names of every branch, sorted (tenant roots and key rotation, STD-PROOF-030).
    fn branch_names(&mut self) -> Result<Vec<String>, BpaError>;
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
    /// The local rows of the given objects visible at `cseq`, in one round trip where the
    /// backend allows it (objects without a local row are left out).
    fn objects_by_id(
        &mut self,
        branch: &str,
        cseq: u64,
        ids: &[String],
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        let mut out = Vec::new();
        for id in ids {
            if let Some(row) = self.object(branch, cseq, id)? {
                out.push((id.clone(), row));
            }
        }
        Ok(out)
    }
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
    /// Authority of the VAE (STD-ADDR-001); `acme` for the conformance vectors.
    fn authority(&self) -> String {
        "acme".into()
    }
    /// Slugs the versions of an object ever carried (STD-ADDR-001 redirects).
    fn object_slugs(&mut self, object_id: &str) -> Result<Vec<String>, BpaError>;

    // ---- metadata records (DEC-054). The defaults keep them under settings keys; the
    // PostgreSQL adapter keeps them in their tables. ----

    /// Branch metadata record (`kind`, `created_at`, `active_at`, `parent_line`,
    /// `created_from`).
    fn branch_meta_rec(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("branch:{name}"))
    }
    /// Store a branch metadata record.
    fn put_branch_meta_rec(&mut self, name: &str, rec: &Value) -> Result<(), BpaError> {
        self.put_meta(&format!("branch:{name}"), rec)
    }
    /// A tag record.
    fn tag_rec(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("tag:{name}"))
    }
    /// Store a tag record.
    fn put_tag_rec(&mut self, name: &str, rec: &Value) -> Result<(), BpaError> {
        self.put_meta(&format!("tag:{name}"), rec)
    }
    /// Names of every tag, sorted, deleted ones included (archive export, STD-PROOF-070).
    fn tag_names(&mut self) -> Result<Vec<String>, BpaError>;
    /// A process record.
    fn process_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("process:{id}"))
    }
    /// Store a process record (new or updated).
    fn put_process_rec(&mut self, id: &str, rec: &Value) -> Result<(), BpaError> {
        if self.meta(&format!("process:{id}"))?.is_none() {
            let mut ids: Vec<Value> = self
                .meta("processes")?
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            ids.push(Value::String(id.to_string()));
            self.put_meta("processes", &Value::Array(ids))?;
        }
        self.put_meta(&format!("process:{id}"), rec)
    }
    /// Identifiers of the recorded processes, in order of recording.
    fn process_ids(&mut self) -> Result<Vec<String>, BpaError> {
        Ok(self
            .meta("processes")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect())
    }
    /// A change-set record (`target`, `merge`).
    fn change_set_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("cs:{id}"))
    }
    /// Store a change-set record.
    fn put_change_set_rec(&mut self, id: &str, rec: &Value) -> Result<(), BpaError> {
        self.put_meta(&format!("cs:{id}"), rec)
    }
    /// A derivation record with its inputs (`{record, inputs}`), written by a flush.
    fn derivation_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("derivation:{id}"))
    }
    /// A record stored as a chunk under its content ID (journals, metering and
    /// verification records; DSN-DVM-630).
    fn record(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("record:{id}"))
    }
    /// Store a record as a chunk; returns its content ID.
    fn put_record(&mut self, value: &Value) -> Result<String, BpaError> {
        let id = ubos_types::content_id("ubos.chunk.v1", value)?;
        self.put_meta(&format!("record:{id}"), value)?;
        Ok(id)
    }
    /// The released effects not yet acknowledged, in release order.
    fn outbox_pending(&mut self) -> Result<Vec<Value>, BpaError> {
        let acked: std::collections::HashSet<String> = self
            .meta("acked")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        Ok(self
            .meta("outbox")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|e| {
                !acked.contains(
                    e.get("effect_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
            })
            .collect())
    }
    /// Release effects outside a flush (BecameEffective events).
    fn outbox_push(&mut self, items: &[Value]) -> Result<(), BpaError> {
        let mut all = self
            .meta("outbox")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        all.extend(items.iter().cloned());
        self.put_meta("outbox", &Value::Array(all))
    }
    /// Acknowledge effects; false when one is unknown (nothing acknowledged then).
    fn outbox_ack(&mut self, ids: &[String]) -> Result<bool, BpaError> {
        let known: std::collections::HashSet<String> = self
            .meta("outbox")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|e| {
                e.get("effect_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect();
        if ids.iter().any(|x| !known.contains(x)) {
            return Ok(false);
        }
        let mut acked: Vec<Value> = self
            .meta("acked")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        acked.extend(ids.iter().map(|x| Value::String(x.clone())));
        self.put_meta("acked", &Value::Array(acked))?;
        Ok(true)
    }
    /// All pending timers (`{id, key, class, instant}`).
    fn timers(&mut self) -> Result<Vec<Value>, BpaError> {
        Ok(self
            .meta("schedule")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default())
    }
    /// Remove timers by identifier.
    fn remove_timers(&mut self, ids: &[String]) -> Result<(), BpaError> {
        let keep: Vec<Value> = self
            .timers()?
            .into_iter()
            .filter(|t| {
                !ids.iter()
                    .any(|i| t.get("id").and_then(Value::as_str) == Some(i))
            })
            .collect();
        self.put_meta("schedule", &Value::Array(keep))
    }
    /// An idempotency record (`hash`, `result`), written by a flush.
    fn idempotency_rec(&mut self, scope: &str, key: &str) -> Result<Option<Value>, BpaError> {
        self.meta(&format!("idem:{scope}:{key}"))
    }
    /// Tenant and VAE of a kernel store (verifiers and the storage suite), if any.
    fn scope(&self) -> Option<kernel::Scope> {
        None
    }
    /// The node signer a host installed (DSN-NOD-134); none in the conformance adapter,
    /// which signs with the node record of STD-CONF-005.
    fn signer(&self) -> Option<std::sync::Arc<dyn node::NodeSigner>> {
        None
    }
}

/// Kernel index rows of one engine object row: head, kind and slug and the valid-time
/// segments come from the engine state (`head`, `kind`, `slug`, `timeline[].{from, to,
/// version}`).
pub fn index_row(o: &ObjectRow) -> Result<kernel::IndexRow, BpaError> {
    let text = |k: &str| {
        o.state
            .as_ref()
            .and_then(|s| s.get(k))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (head, segments) = match &o.state {
        None => (None, Vec::new()),
        Some(s) => {
            let head = s
                .get("head")
                .and_then(Value::as_str)
                .map(kernel::hash_of)
                .transpose()?;
            let mut segs = Vec::new();
            for seg in s
                .get("timeline")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let from = seg.get("from").and_then(Value::as_str).unwrap_or_default();
                let to = seg.get("to").and_then(Value::as_str).map(str::to_string);
                let ver = seg
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                segs.push((from.to_string(), to, kernel::hash_of(ver)?));
            }
            (head, segs)
        }
    };
    Ok(kernel::IndexRow {
        object_id: o.object_id.clone(),
        class: o.class.clone(),
        present: o.state.is_some(),
        head,
        kind: text("kind"),
        slug: text("slug"),
        instant: o.instant,
        segments,
    })
}

/// The engine's object state from the facts of the kernel indexes (the inverse of
/// [`index_row`]): `None` when the branch holds no version of the object.
pub fn state_of(facts: &kernel::ObjectFacts) -> Option<Value> {
    let head = facts.head.as_ref()?;
    let timeline: Vec<Value> = facts
        .segments
        .iter()
        .map(|(f, t, v)| serde_json::json!({"from": f, "to": t, "version": kernel::hash_text(v)}))
        .collect();
    Some(serde_json::json!({
        "class": facts.class, "kind": facts.kind, "slug": facts.slug,
        "head": kernel::hash_text(head), "timeline": timeline,
    }))
}

/// An object-index row as kept in memory.
#[derive(Debug, Clone)]
struct MemObject {
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
    commit_ids: HashMap<u64, String>,
    /// Object identifiers by branch and class (the class of an object never changes).
    by_class: HashMap<String, HashMap<String, Vec<String>>>,
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
    fn commit_id(&mut self, id: u64) -> Result<String, BpaError> {
        self.commit_ids
            .get(&id)
            .cloned()
            .ok_or_else(|| missing("commit"))
    }
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        Ok(self.by_cseq.get(&(branch.to_string(), cseq)).copied())
    }
    fn commit_by_id(&mut self, commit_id: &str) -> Result<Option<u64>, BpaError> {
        Ok(self
            .commits
            .iter()
            .find(|(_, c)| c.get("commit_id").and_then(Value::as_str) == Some(commit_id))
            .map(|(id, _)| *id))
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        self.next += 1;
        Ok(self.next)
    }
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError> {
        Ok(self.branches.get(name).cloned())
    }
    fn branch_names(&mut self) -> Result<Vec<String>, BpaError> {
        let mut names: Vec<String> = self.branches.keys().cloned().collect();
        names.sort();
        Ok(names)
    }
    fn tag_names(&mut self) -> Result<Vec<String>, BpaError> {
        let mut names: Vec<String> = self
            .meta
            .keys()
            .filter_map(|k| k.strip_prefix("tag:").map(str::to_string))
            .collect();
        names.sort();
        Ok(names)
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
        self.commit_ids.insert(
            f.commit_id,
            f.commit["commit_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        self.by_cseq.insert((f.branch.clone(), f.cseq), f.commit_id);
        let objects = self.objects.entry(f.branch.clone()).or_default();
        let by_class = self.by_class.entry(f.branch.clone()).or_default();
        for o in &f.objects {
            let rows = objects.entry(o.object_id.clone()).or_default();
            if rows.is_empty() {
                by_class
                    .entry(o.class.clone())
                    .or_default()
                    .push(o.object_id.clone());
            }
            if let Some(last) = rows.last_mut()
                && last.cseq_to.is_none()
            {
                last.cseq_to = Some(f.cseq);
            }
            rows.push(MemObject {
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
        apply_metadata(self, f)?;
        Ok(true)
    }
    fn object_slugs(&mut self, object_id: &str) -> Result<Vec<String>, BpaError> {
        let mut out: Vec<String> = self
            .versions
            .values()
            .filter(|v| v.get("object_id").and_then(Value::as_str) == Some(object_id))
            .filter_map(|v| v.get("slug").and_then(Value::as_str).map(str::to_string))
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
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
        let Some(objects) = self.objects.get(branch) else {
            return Ok(out);
        };
        if let Some(c) = class {
            // the class index: only the objects of the class are visited
            let ids = self
                .by_class
                .get(branch)
                .and_then(|m| m.get(c))
                .map(Vec::as_slice)
                .unwrap_or_default();
            for id in ids {
                if let Some(r) = objects
                    .get(id)
                    .and_then(|rows| rows.iter().find(|r| visible(r, cseq)))
                {
                    out.push((id.clone(), r.state.clone()));
                }
            }
            out.sort_by(|a, b| a.0.cmp(&b.0));
            return Ok(out);
        }
        for (id, rows) in objects {
            if let Some(r) = rows.iter().find(|r| visible(r, cseq)) {
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

/// Apply the metadata of a flush through the settings keys of the default methods
/// (memory store and overlays): released effects, the idempotency record, derivation
/// records and timers.
fn apply_metadata(s: &mut dyn Store, f: &Flush) -> Result<(), BpaError> {
    if !f.outbox.is_empty() {
        s.outbox_push(&f.outbox)?;
    }
    if let Some((scope, key, hash, result)) = &f.idempotency {
        s.put_meta(
            &format!("idem:{scope}:{key}"),
            &serde_json::json!({"hash": hash, "result": result}),
        )?;
    }
    for d in &f.derivations {
        let id = d.get("id").and_then(Value::as_str).unwrap_or_default();
        s.put_meta(
            &format!("derivation:{id}"),
            &serde_json::json!({"record": d["record"], "inputs": d["inputs"]}),
        )?;
    }
    if !f.timers.is_empty() {
        let mut all = s.timers()?;
        for t in &f.timers {
            if !all.iter().any(|x| x["key"] == t["key"]) {
                let mut t = t.clone();
                t["id"] = Value::String(format!("timer-{}", all.len() + 1));
                all.push(t);
            }
        }
        s.put_meta("schedule", &Value::Array(all))?;
    }
    Ok(())
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
    fn commit_id(&mut self, id: u64) -> Result<String, BpaError> {
        match self.overlay.commit_ids.get(&id) {
            Some(v) => Ok(v.clone()),
            None => self.inner.commit_id(id),
        }
    }
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        match self.overlay.commit_at(branch, cseq)? {
            Some(id) => Ok(Some(id)),
            None => self.inner.commit_at(branch, cseq),
        }
    }
    fn commit_by_id(&mut self, commit_id: &str) -> Result<Option<u64>, BpaError> {
        match self.overlay.commit_by_id(commit_id)? {
            Some(id) => Ok(Some(id)),
            None => self.inner.commit_by_id(commit_id),
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
    fn branch_names(&mut self) -> Result<Vec<String>, BpaError> {
        let mut names = self.inner.branch_names()?;
        names.extend(self.overlay.branches.keys().cloned());
        names.sort();
        names.dedup();
        Ok(names)
    }
    fn tag_names(&mut self) -> Result<Vec<String>, BpaError> {
        let mut names = self.inner.tag_names()?;
        names.extend(self.overlay.tag_names()?);
        names.sort();
        names.dedup();
        Ok(names)
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
    fn authority(&self) -> String {
        self.inner.authority()
    }
    fn object_slugs(&mut self, object_id: &str) -> Result<Vec<String>, BpaError> {
        let mut out = self.inner.object_slugs(object_id)?;
        out.extend(self.overlay.object_slugs(object_id)?);
        out.sort();
        out.dedup();
        Ok(out)
    }
    fn branch_meta_rec(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        self.inner.branch_meta_rec(name)
    }
    fn put_branch_meta_rec(&mut self, name: &str, rec: &Value) -> Result<(), BpaError> {
        self.inner.put_branch_meta_rec(name, rec)
    }
    fn tag_rec(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        self.inner.tag_rec(name)
    }
    fn put_tag_rec(&mut self, name: &str, rec: &Value) -> Result<(), BpaError> {
        self.inner.put_tag_rec(name, rec)
    }
    fn process_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.inner.process_rec(id)
    }
    fn put_process_rec(&mut self, id: &str, rec: &Value) -> Result<(), BpaError> {
        self.inner.put_process_rec(id, rec)
    }
    fn process_ids(&mut self) -> Result<Vec<String>, BpaError> {
        self.inner.process_ids()
    }
    fn record(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.inner.record(id)
    }
    fn put_record(&mut self, value: &Value) -> Result<String, BpaError> {
        self.inner.put_record(value)
    }
    fn change_set_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        self.inner.change_set_rec(id)
    }
    fn put_change_set_rec(&mut self, id: &str, rec: &Value) -> Result<(), BpaError> {
        self.inner.put_change_set_rec(id, rec)
    }
    fn derivation_rec(&mut self, id: &str) -> Result<Option<Value>, BpaError> {
        match self.overlay.derivation_rec(id)? {
            Some(v) => Ok(Some(v)),
            None => self.inner.derivation_rec(id),
        }
    }
    fn outbox_pending(&mut self) -> Result<Vec<Value>, BpaError> {
        self.inner.outbox_pending()
    }
    fn outbox_push(&mut self, items: &[Value]) -> Result<(), BpaError> {
        self.inner.outbox_push(items)
    }
    fn outbox_ack(&mut self, ids: &[String]) -> Result<bool, BpaError> {
        self.inner.outbox_ack(ids)
    }
    fn timers(&mut self) -> Result<Vec<Value>, BpaError> {
        self.inner.timers()
    }
    fn remove_timers(&mut self, ids: &[String]) -> Result<(), BpaError> {
        self.inner.remove_timers(ids)
    }
    fn idempotency_rec(&mut self, scope: &str, key: &str) -> Result<Option<Value>, BpaError> {
        match self.overlay.idempotency_rec(scope, key)? {
            Some(v) => Ok(Some(v)),
            None => self.inner.idempotency_rec(scope, key),
        }
    }
    fn backend(&self) -> &'static str {
        self.inner.backend()
    }
}
