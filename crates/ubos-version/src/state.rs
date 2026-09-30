//! Branch state, commits and ledgers.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;
use ubos_model::Registry;
use ubos_model::types::decimal::Decimal;
use ubos_model::types::values::minor_unit;
use ubos_model::types::{BpaError, canonical, content_id, names};
use ubos_store::{BranchRef, EntryRow, Flush, ObjectRow, Store};
use ubos_tree::{Node, NodeSource};

/// A valid-time segment `[from, to)` of a timeline; `to = None` is an open end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// Start (date or instant).
    pub from: String,
    /// End, exclusive; `None` for an open end.
    pub to: Option<String>,
    /// Version hash valid in the segment.
    pub version: String,
}

/// `a < b` where `None` is +infinity.
pub fn lt(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(x), Some(y)) => x < y,
    }
}

/// The segment of a timeline that contains `asof`.
pub fn at<'a>(tl: &'a [Segment], asof: &str) -> Option<&'a Segment> {
    tl.iter()
        .find(|s| s.from.as_str() <= asof && lt(Some(asof), s.to.as_deref()))
}

/// State of a Definition, RawDocument or System object on a branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectState {
    /// Class name.
    pub class: String,
    /// Kind of the class.
    pub kind: String,
    /// Current slug.
    pub slug: String,
    /// Last version written on the branch.
    pub head: String,
    /// Valid-time segments, sorted and non-overlapping.
    pub timeline: Vec<Segment>,
}

/// A ledger entry on a branch (STD-KIND-020).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Entry identifier.
    pub entry_id: String,
    /// Ledger class.
    pub class: String,
    /// Ledger key values.
    pub ledger_key: Value,
    /// Business effect date or instant.
    pub valid_at: String,
    /// Field values.
    pub body: Value,
    /// Corrected entries.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub corrects: Option<Vec<String>>,
    /// `reversal` or `adjustment`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub correction_type: Option<String>,
    /// Double-entry group.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub posting_group: Option<String>,
    /// Branch sequence within the ledger key.
    pub seq: u64,
    /// Commit that made the entry visible on this branch.
    pub cseq: u64,
    /// Entry hash (sequence excluded).
    pub entry_hash: String,
    /// `{branch, seq}` where the entry was first appended, after merges.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_seq: Option<Value>,
}

/// The state of a branch at a commit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Objects by identifier.
    pub objects: BTreeMap<String, ObjectState>,
    /// Ledgers in insertion order: `(class, canonical ledger key)` and its entries.
    pub ledger: Vec<(String, String, Vec<Entry>)>,
}

impl State {
    /// Entries of a ledger key, created on demand.
    pub fn queue(&mut self, class: &str, key: &str) -> &mut Vec<Entry> {
        let i = match self
            .ledger
            .iter()
            .position(|(c, k, _)| c == class && k == key)
        {
            Some(i) => i,
            None => {
                self.ledger
                    .push((class.to_string(), key.to_string(), Vec::new()));
                self.ledger.len() - 1
            }
        };
        &mut self.ledger[i].2
    }

    /// The entries of a ledger key.
    pub fn entries(&self, class: &str, key: &str) -> &[Entry] {
        self.ledger
            .iter()
            .find(|(c, k, _)| c == class && k == key)
            .map(|x| x.2.as_slice())
            .unwrap_or(&[])
    }

    /// An entry by identifier.
    pub fn find_entry(&self, entry_id: &str) -> Option<&Entry> {
        self.ledger
            .iter()
            .flat_map(|x| x.2.iter())
            .find(|e| e.entry_id == entry_id)
    }
}

/// A commit as stored: parents, ordinal, owning branch and state-tree root. The branch
/// state itself lives in the object and ledger indexes (DSN-DVM-004, -005).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRecord {
    /// Parent commits; the first is the previous head of the branch.
    pub parents: Vec<u64>,
    /// Commit ordinal on its branch.
    pub cseq: u64,
    /// Branch the commit was made on.
    pub branch: String,
    /// Root of the state tree after the commit (STD-PROOF-001).
    pub root: String,
}

fn storage(e: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, BpaError> {
    serde_json::from_value(v).map_err(storage)
}

/// Hash and stored record of a new version (STD-OBJ-002); used by commits and by bulk
/// loaders, which must produce identical versions.
#[allow(clippy::too_many_arguments)]
pub fn version_record(
    reg: &Registry,
    oid: &str,
    class: &str,
    slug: &str,
    body: &Map<String, Value>,
    from: &str,
    to: Option<&str>,
    predecessor: Option<&str>,
    tombstone: bool,
) -> Result<(String, Value), BpaError> {
    let c = reg.class(class)?;
    let valid = json!({"from": from, "to": to});
    let mut env = json!({
        "object_id": oid, "class": class, "class_version": c.class_version, "schema_version": c.schema_version,
        "kind": c.kind, "slug": slug, "valid": valid, "body_ref": content_id("ubos.chunk.v1", &Value::Object(body.clone()))?,
    });
    if let Some(p) = predecessor {
        env["predecessor"] = json!(p);
    }
    if tombstone {
        env["tombstone"] = json!(true);
    }
    let vh = content_id("ubos.version.v1", &env)?;
    let rec = json!({"object_id": oid, "class": class, "slug": slug, "body": body, "valid": valid, "tombstone": tombstone});
    Ok((vh, rec))
}

/// Parsed tree nodes shared by the views of this thread, keyed by hash (the process-local
/// node cache of DSN-DVM-007; nodes are immutable, so the cache is never stale, AR-028).
const NODE_CACHE_LIMIT: usize = 500_000;

thread_local! {
    static NODES: RefCell<HashMap<String, Rc<Node>>> = RefCell::new(HashMap::new());
}

/// Tree nodes read from the store through the node cache.
struct StoreNodes<'s>(&'s mut dyn Store);

impl NodeSource for StoreNodes<'_> {
    fn node(&mut self, hash: &str) -> Result<Rc<Node>, BpaError> {
        if let Some(n) = NODES.with(|c| c.borrow().get(hash).cloned()) {
            return Ok(n);
        }
        let n = Rc::new(Node::from_json(&self.0.node(hash)?)?);
        NODES.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= NODE_CACHE_LIMIT {
                c.clear();
            }
            c.insert(hash.to_string(), n.clone());
        });
        Ok(n)
    }
}

/// Access to the stored branches, commits and versions.
pub struct Repo<'a> {
    /// The store.
    pub store: &'a mut dyn Store,
    /// The class registry.
    pub reg: &'a Registry,
    cache: HashMap<u64, CommitRecord>,
    /// Branch references as first read by this view, so that one operation sees one
    /// snapshot of every branch (DSN-DVM-008).
    branches: HashMap<String, BranchRef>,
}

impl<'a> Repo<'a> {
    /// A repository view over a store and a class registry.
    pub fn new(store: &'a mut dyn Store, reg: &'a Registry) -> Self {
        Repo {
            store,
            reg,
            cache: HashMap::new(),
            branches: HashMap::new(),
        }
    }

    /// A commit record.
    pub fn commit(&mut self, id: u64) -> Result<CommitRecord, BpaError> {
        if let Some(c) = self.cache.get(&id) {
            return Ok(c.clone());
        }
        let c: CommitRecord = parse(self.store.commit(id)?)?;
        self.cache.insert(id, c.clone());
        Ok(c)
    }

    /// Initialise `main` with the empty genesis commit (cseq 0).
    pub fn init(&mut self) -> Result<(), BpaError> {
        let id = self.store.next_commit_id()?;
        let empty = ubos_tree::build(&[])?;
        let rec = CommitRecord {
            parents: vec![],
            cseq: 0,
            branch: "main".into(),
            root: empty.root,
        };
        let r = BranchRef {
            head: 0,
            head_cseq: 0,
            parent: None,
        };
        self.store.create_branch("main", &r)?;
        let f = Flush {
            commit_id: id,
            commit: serde_json::to_value(&rec).map_err(storage)?,
            branch: "main".into(),
            cseq: 0,
            expected_head: 0,
            nodes: empty
                .nodes
                .iter()
                .map(|(h, n)| (h.clone(), n.to_json()))
                .collect(),
            ..Flush::default()
        };
        if !self.store.flush(&f)? {
            return Err(storage("cannot initialise main"));
        }
        Ok(())
    }

    fn branch_ref(&mut self, branch: &str) -> Result<BranchRef, BpaError> {
        if let Some(r) = self.branches.get(branch) {
            return Ok(r.clone());
        }
        let r = self
            .store
            .branch(branch)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown branch"))?;
        self.branches.insert(branch.to_string(), r.clone());
        Ok(r)
    }

    /// The head commit of a branch.
    pub fn head(&mut self, branch: &str) -> Result<u64, BpaError> {
        Ok(self.branch_ref(branch)?.head)
    }

    /// The branches and ordinals whose local rows make up `branch` at `cseq`, nearest first:
    /// a branch at or before its fork point is its parent (DSN-DVM-008).
    fn chain(&mut self, branch: &str, cseq: u64) -> Result<Vec<(String, u64)>, BpaError> {
        let mut out = Vec::new();
        let (mut b, mut c) = (branch.to_string(), cseq);
        loop {
            let r = self.branch_ref(&b)?;
            match r.parent {
                Some((p, fork)) if c <= fork => b = p,
                Some((p, fork)) => {
                    out.push((b, c));
                    b = p;
                    c = fork;
                }
                None => {
                    out.push((b, c));
                    return Ok(out);
                }
            }
        }
    }

    /// The ordinal a read of `branch` at `time` uses (default: head), checked to exist.
    pub(crate) fn ordinal(&mut self, branch: &str, time: Option<u64>) -> Result<u64, BpaError> {
        let r = self.branch_ref(branch)?;
        match time {
            None => Ok(r.head_cseq),
            Some(t) if t <= r.head_cseq => Ok(t),
            Some(_) => Err(BpaError::new("IAM.NOT_FOUND", "no commit with this cseq")),
        }
    }

    /// The commit of a branch with the given cseq (default: head).
    pub fn commit_at(&mut self, branch: &str, time: Option<u64>) -> Result<u64, BpaError> {
        let c = self.ordinal(branch, time)?;
        let (b, c) = self.chain(branch, c)?.remove(0);
        self.store
            .commit_at(&b, c)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "no commit with this cseq"))
    }

    /// One object of a branch at an ordinal, from the object index.
    fn object_in(
        &mut self,
        branch: &str,
        cseq: u64,
        oid: &str,
    ) -> Result<Option<ObjectState>, BpaError> {
        for (b, c) in self.chain(branch, cseq)? {
            if let Some(row) = self.store.object(&b, c, oid)? {
                return row.map(parse).transpose();
            }
        }
        Ok(None)
    }

    /// Objects of a branch at an ordinal, optionally of one class.
    pub(crate) fn objects_in(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<BTreeMap<String, ObjectState>, BpaError> {
        let mut out = BTreeMap::new();
        for (b, c) in self.chain(branch, cseq)?.into_iter().rev() {
            for (id, row) in self.store.objects(&b, c, class)? {
                match row {
                    Some(v) => {
                        out.insert(id, parse(v)?);
                    }
                    None => {
                        out.remove(&id);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Ledger entries of a branch at an ordinal, optionally of one key or one identifier.
    fn entries_in(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Entry>, BpaError> {
        let mut out = Vec::new();
        for (b, c) in self.chain(branch, cseq)?.into_iter().rev() {
            for v in self.store.entries(&b, c, key, entry_id)? {
                out.push(parse(v)?);
            }
        }
        Ok(out)
    }

    fn materialize(&mut self, branch: &str, cseq: u64) -> Result<State, BpaError> {
        let mut st = State {
            objects: self.objects_in(branch, cseq, None)?,
            ledger: Vec::new(),
        };
        for e in self.entries_in(branch, cseq, None, None)? {
            let key = canonical(&e.ledger_key)?;
            st.queue(&e.class, &key).push(e);
        }
        Ok(st)
    }

    /// The state of a branch at a cseq (default: head).
    pub fn state_at(&mut self, branch: &str, time: Option<u64>) -> Result<State, BpaError> {
        let c = self.ordinal(branch, time)?;
        self.materialize(branch, c)
    }

    /// The state after a commit.
    pub fn commit_state(&mut self, id: u64) -> Result<State, BpaError> {
        let c = self.commit(id)?;
        self.materialize(&c.branch, c.cseq)
    }

    /// One object of a branch at a cseq (default: head), read from the index.
    pub fn object_at(
        &mut self,
        branch: &str,
        time: Option<u64>,
        oid: &str,
    ) -> Result<Option<ObjectState>, BpaError> {
        let c = self.ordinal(branch, time)?;
        self.object_in(branch, c, oid)
    }

    /// The entries of one ledger key of a branch at a cseq (default: head).
    pub fn ledger_at(
        &mut self,
        branch: &str,
        time: Option<u64>,
        class: &str,
        key: &str,
    ) -> Result<Vec<Entry>, BpaError> {
        let c = self.ordinal(branch, time)?;
        self.entries_in(branch, c, Some((class, key)), None)
    }

    /// The stored state-tree root of a branch at a cseq (default: head).
    pub fn root_at(&mut self, branch: &str, time: Option<u64>) -> Result<String, BpaError> {
        let id = self.commit_at(branch, time)?;
        Ok(self.commit(id)?.root)
    }

    /// Keys whose values differ between two state-tree roots.
    pub fn tree_diff(&mut self, a: &str, b: &str) -> Result<Vec<String>, BpaError> {
        Ok(ubos_tree::diff(&mut StoreNodes(&mut *self.store), a, b)?
            .into_iter()
            .map(|c| c.0)
            .collect())
    }

    /// A version record by hash.
    pub fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.store.version(hash)
    }

    /// Create a branch at a commit of another branch (STD-VER-010, -013).
    pub fn create_branch(
        &mut self,
        name: &str,
        source: &str,
        time: Option<u64>,
    ) -> Result<Value, BpaError> {
        names::check_name("branch-name", name)?;
        let id = self.commit_at(source, time)?;
        let c = self.commit(id)?;
        let r = BranchRef {
            head: id,
            head_cseq: c.cseq,
            parent: Some((c.branch.clone(), c.cseq)),
        };
        if !self.store.create_branch(name, &r)? {
            return Err(BpaError::new("TXN.ALREADY_EXISTS", "branch exists"));
        }
        Ok(json!({"head": c.cseq}))
    }

    /// Begin a commit on a branch. The state of the transaction starts empty and is filled
    /// from the indexes as objects and ledgers are read.
    pub fn begin(&mut self, branch: &str) -> Result<Tx, BpaError> {
        let r = self.branch_ref(branch)?;
        Ok(Tx {
            branch: branch.to_string(),
            head: r.head,
            base: r.head_cseq,
            cseq: r.head_cseq + 1,
            state: State::default(),
            loaded: HashMap::new(),
            loaded_entries: HashSet::new(),
            loaded_queues: HashSet::new(),
            versions: Vec::new(),
            appended: Vec::new(),
        })
    }

    /// An object as the transaction sees it, loaded from the index on first use.
    pub fn obj(&mut self, tx: &mut Tx, oid: &str) -> Result<Option<ObjectState>, BpaError> {
        if !tx.loaded.contains_key(oid) {
            let o = self.object_in(&tx.branch, tx.base, oid)?;
            if let Some(o) = &o {
                tx.state.objects.insert(oid.to_string(), o.clone());
            }
            tx.loaded.insert(oid.to_string(), o);
        }
        Ok(tx.state.objects.get(oid).cloned())
    }

    /// Load every object of a class into the transaction.
    pub fn load_class(&mut self, tx: &mut Tx, class: &str) -> Result<(), BpaError> {
        for (id, o) in self.objects_in(&tx.branch, tx.base, Some(class))? {
            if !tx.loaded.contains_key(&id) {
                tx.state.objects.insert(id.clone(), o.clone());
                tx.loaded.insert(id, Some(o));
            }
        }
        Ok(())
    }

    /// Load the entries of a ledger key into the transaction.
    pub fn load_queue(&mut self, tx: &mut Tx, class: &str, key: &str) -> Result<(), BpaError> {
        let k = (class.to_string(), key.to_string());
        if tx.loaded_queues.contains(&k) {
            return Ok(());
        }
        let entries = self.entries_in(&tx.branch, tx.base, Some((class, key)), None)?;
        let q = tx.state.queue(class, key);
        let fresh = std::mem::take(q);
        for e in &entries {
            tx.loaded_entries.insert(e.entry_id.clone());
        }
        q.extend(entries);
        q.extend(fresh);
        tx.loaded_queues.insert(k);
        Ok(())
    }

    /// Load the whole branch state into a fresh transaction (merge).
    pub fn load_all(&mut self, tx: &mut Tx) -> Result<(), BpaError> {
        let st = self.materialize(&tx.branch.clone(), tx.base)?;
        for (id, o) in &st.objects {
            tx.loaded.insert(id.clone(), Some(o.clone()));
        }
        for (c, k, q) in &st.ledger {
            tx.loaded_queues.insert((c.clone(), k.clone()));
            tx.loaded_entries
                .extend(q.iter().map(|e| e.entry_id.clone()));
        }
        tx.state = st;
        Ok(())
    }

    /// An entry by identifier as the transaction sees it.
    pub fn find_entry(&mut self, tx: &Tx, entry_id: &str) -> Result<Option<Entry>, BpaError> {
        if let Some(e) = tx.state.find_entry(entry_id) {
            return Ok(Some(e.clone()));
        }
        Ok(self
            .entries_in(&tx.branch, tx.base, None, Some(entry_id))?
            .into_iter()
            .next())
    }

    /// State-tree key of an entry (STD-PROOF-001).
    fn entry_key(e: &Entry) -> Result<String, BpaError> {
        let ch = content_id("ubos.chunk.v1", &json!(e.class))?[7..].to_string();
        let kh = content_id("ubos.chunk.v1", &e.ledger_key)?[7..].to_string();
        Ok(format!("l/{ch}/{kh}/{:020}", e.seq))
    }

    /// Write a commit with the given parents: the changed objects, the new entries, the
    /// tree nodes and the head compare-and-set in one flush; `TXN.HEAD_MOVED` if the head
    /// moved.
    pub fn write(&mut self, tx: Tx, parents: Vec<u64>) -> Result<u64, BpaError> {
        let mut objects = Vec::new();
        let mut tree: BTreeMap<String, Option<String>> = BTreeMap::new();
        let ids: BTreeSet<&String> = tx.loaded.keys().chain(tx.state.objects.keys()).collect();
        for id in ids {
            let old = tx.loaded.get(id).cloned().flatten();
            let new = tx.state.objects.get(id);
            if old.as_ref() == new {
                continue;
            }
            let any = new.or(old.as_ref()).expect("changed object");
            if any.kind != "system" {
                tree.insert(format!("o/{id}"), new.map(|o| o.head.clone()));
            }
            objects.push(ObjectRow {
                object_id: id.clone(),
                class: any.class.clone(),
                state: new.map(serde_json::to_value).transpose().map_err(storage)?,
            });
        }
        let mut entries = Vec::new();
        for (class, key, q) in &tx.state.ledger {
            for e in q
                .iter()
                .filter(|e| !tx.loaded_entries.contains(&e.entry_id))
            {
                tree.insert(Self::entry_key(e)?, Some(e.entry_hash.clone()));
                entries.push(EntryRow {
                    entry_id: e.entry_id.clone(),
                    class: class.clone(),
                    ledger_key: key.clone(),
                    entry: serde_json::to_value(e).map_err(storage)?,
                });
            }
        }
        let prev = self.commit(tx.head)?.root;
        let w = ubos_tree::update(&mut StoreNodes(&mut *self.store), &prev, &tree)?;
        let id = self.store.next_commit_id()?;
        let rec = CommitRecord {
            parents,
            cseq: tx.cseq,
            branch: tx.branch.clone(),
            root: w.root,
        };
        let f = Flush {
            versions: tx.versions,
            commit_id: id,
            commit: serde_json::to_value(&rec).map_err(storage)?,
            branch: tx.branch,
            cseq: tx.cseq,
            expected_head: tx.head,
            objects,
            entries,
            nodes: w
                .nodes
                .iter()
                .map(|(h, n)| (h.clone(), n.to_json()))
                .collect(),
        };
        let branch = f.branch.clone();
        self.branches.remove(&branch);
        if !self.store.flush(&f)? {
            return Err(BpaError::new("TXN.HEAD_MOVED", "the branch head moved"));
        }
        self.cache.insert(id, rec);
        Ok(id)
    }

    /// Create a version record and remember it for the flush (STD-OBJ-002).
    #[allow(clippy::too_many_arguments)]
    pub fn new_version(
        &self,
        tx: &mut Tx,
        oid: &str,
        class: &str,
        slug: &str,
        body: &Map<String, Value>,
        from: &str,
        to: Option<&str>,
        predecessor: Option<&str>,
        tombstone: bool,
    ) -> Result<String, BpaError> {
        let (vh, rec) = version_record(
            self.reg,
            oid,
            class,
            slug,
            body,
            from,
            to,
            predecessor,
            tombstone,
        )?;
        tx.versions.push((vh.clone(), rec));
        Ok(vh)
    }

    /// A version record, looking first at the versions of an open transaction.
    pub fn version_in(&mut self, tx: &Tx, hash: &str) -> Result<Value, BpaError> {
        match tx.versions.iter().find(|(h, _)| h == hash) {
            Some((_, v)) => Ok(v.clone()),
            None => self.version(hash),
        }
    }

    /// Entry hash: the entry without `seq`, `source_seq` and `cseq`, with class version and
    /// body reference (STD-KIND-020).
    pub fn entry_hash(&self, e: &Entry) -> Result<String, BpaError> {
        let mut rec = json!({"entry_id": e.entry_id, "class": e.class, "ledger_key": e.ledger_key, "valid_at": e.valid_at});
        if let Some(c) = &e.corrects {
            rec["corrects"] = json!(c);
        }
        if let Some(c) = &e.correction_type {
            rec["correction_type"] = json!(c);
        }
        if let Some(g) = &e.posting_group {
            rec["posting_group"] = json!(g);
        }
        rec["class_version"] = json!(self.reg.class(&e.class)?.class_version);
        rec["body_ref"] = json!(content_id("ubos.chunk.v1", &e.body)?);
        content_id("ubos.entry.v1", &rec)
    }

    fn check_period(&mut self, tx: &mut Tx, class: &str, today: &str) -> Result<(), BpaError> {
        let Some(scope) = self
            .reg
            .class(class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("period_scope"))
            .and_then(Value::as_str)
        else {
            return Ok(());
        };
        self.load_class(tx, scope)?;
        let objs: Vec<ObjectState> = tx
            .state
            .objects
            .values()
            .filter(|o| o.class == scope)
            .cloned()
            .collect();
        for o in objs {
            let hash = at(&o.timeline, today)
                .map(|s| s.version.clone())
                .unwrap_or(o.head.clone());
            let body = self.version_in(tx, &hash)?["body"].clone();
            let (start, end) = (
                body["start"].as_str().unwrap_or_default(),
                body["end"].as_str().unwrap_or_default(),
            );
            if body["state"] == "closed" && start <= today && today < end {
                return Err(BpaError::new(
                    "LEDG.PERIOD_CLOSED",
                    "posting date in a closed period",
                ));
            }
        }
        Ok(())
    }

    fn amounts(&self, class: &str) -> Result<Vec<String>, BpaError> {
        Ok(self
            .reg
            .class(class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("amounts"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Append, reverse or adjust (STD-KIND-020…025); returns the new entry's id and seq.
    pub fn append(
        &mut self,
        tx: &mut Tx,
        ch: &Value,
        op: &str,
        today: &str,
        default_valid: &dyn Fn(&str) -> String,
    ) -> Result<(String, u64), BpaError> {
        let s = |k: &str| ch.get(k).and_then(Value::as_str).map(str::to_string);
        let mut entry = if op == "append" {
            let class = s("class").unwrap_or_default();
            let c = self.reg.class(&class)?.clone();
            if c.kind.as_deref() != Some("ledger") {
                return Err(BpaError::new(
                    "LOGIC.FORBIDDEN_TARGET",
                    "not a ledger class",
                ));
            }
            let spec = c.ledger.clone().unwrap_or_default();
            let key_fields: Vec<String> = spec
                .get("key")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            let lk = ch
                .get("ledger_key")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut have: Vec<&String> = lk.keys().collect();
            have.sort();
            let mut want: Vec<&String> = key_fields.iter().collect();
            want.sort();
            if have != want {
                return Err(BpaError::new("MODEL.INVALID_VALUE", "ledger key fields"));
            }
            let mut key = Map::new();
            for k in &key_fields {
                let f = &c.fields[k];
                key.insert(
                    k.clone(),
                    ubos_model::types::values::validate_value(
                        f["type"].as_str().unwrap_or_default(),
                        f.get("constraints"),
                        &lk[k],
                    )?,
                );
            }
            let body = self
                .reg
                .check_body(&class, ch.get("body").unwrap_or(&json!({})), false)?;
            Entry {
                entry_id: s("entry_id").unwrap_or_default(),
                class: class.clone(),
                ledger_key: Value::Object(key),
                valid_at: s("valid_at").unwrap_or_else(|| default_valid(&class)),
                body: Value::Object(body),
                corrects: None,
                correction_type: None,
                posting_group: s("posting_group"),
                seq: 0,
                cseq: 0,
                entry_hash: String::new(),
                source_seq: None,
            }
        } else {
            let target = self
                .find_entry(tx, &s("entry_id").unwrap_or_default())?
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown entry"))?;
            let key = canonical(&target.ledger_key)?;
            self.load_queue(tx, &target.class, &key)?;
            let q = tx.state.entries(&target.class, &key);
            let body = if op == "reverse" {
                if target.correction_type.as_deref() == Some("reversal") {
                    return Err(BpaError::new(
                        "LEDG.REVERSAL_NOT_REVERSIBLE",
                        "reversal of a reversal",
                    ));
                }
                if q.iter().any(|e| {
                    e.correction_type.as_deref() == Some("reversal")
                        && e.corrects
                            .as_ref()
                            .is_some_and(|c| c.contains(&target.entry_id))
                }) {
                    return Err(BpaError::new("LEDG.ALREADY_REVERSED", "already reversed"));
                }
                let mut b = target.body.as_object().cloned().unwrap_or_default();
                for a in self.amounts(&target.class)? {
                    if let Some(v) = b.get(&a).cloned() {
                        b.insert(a, negate(&v)?);
                    }
                }
                Value::Object(b)
            } else {
                Value::Object(self.reg.check_body(
                    &target.class,
                    ch.get("body").unwrap_or(&json!({})),
                    false,
                )?)
            };
            Entry {
                entry_id: s("new_entry_id").unwrap_or_default(),
                class: target.class.clone(),
                ledger_key: target.ledger_key.clone(),
                valid_at: s("valid_at").unwrap_or(target.valid_at.clone()),
                body,
                corrects: Some(vec![target.entry_id.clone()]),
                correction_type: Some(
                    if op == "reverse" {
                        "reversal"
                    } else {
                        "adjustment"
                    }
                    .into(),
                ),
                posting_group: if op == "reverse" {
                    target
                        .posting_group
                        .as_ref()
                        .map(|g| format!("{g}/reversal"))
                } else {
                    None
                },
                seq: 0,
                cseq: 0,
                entry_hash: String::new(),
                source_seq: None,
            }
        };
        if !names::matches("uuid", &entry.entry_id) {
            return Err(BpaError::new("MODEL.INVALID_VALUE", "entry_id is a UUID"));
        }
        if self.find_entry(tx, &entry.entry_id)?.is_some()
            || self.obj(tx, &entry.entry_id)?.is_some()
        {
            return Err(BpaError::new("TXN.ALREADY_EXISTS", "entry exists"));
        }
        self.check_period(tx, &entry.class.clone(), today)?;
        let key = canonical(&entry.ledger_key)?;
        entry.cseq = tx.cseq;
        entry.entry_hash = self.entry_hash(&entry)?;
        self.load_queue(tx, &entry.class, &key)?;
        let q = tx.state.queue(&entry.class, &key);
        entry.seq = q.len() as u64 + 1;
        let out = (entry.entry_id.clone(), entry.seq);
        q.push(entry.clone());
        tx.appended.push(entry);
        Ok(out)
    }

    /// Double entry: each posting group of a double-entry class balances per currency (STD-KIND-024).
    pub fn check_double_entry(&self, tx: &Tx) -> Result<(), BpaError> {
        let mut groups: BTreeMap<(String, String), BTreeMap<String, Decimal>> = BTreeMap::new();
        for e in &tx.appended {
            let double = self
                .reg
                .class(&e.class)?
                .ledger
                .as_ref()
                .and_then(|l| l.get("double_entry"))
                == Some(&Value::Bool(true));
            if !double {
                continue;
            }
            let g = groups
                .entry((
                    e.class.clone(),
                    e.posting_group.clone().unwrap_or(e.entry_id.clone()),
                ))
                .or_default();
            for a in self.amounts(&e.class)? {
                let v = &e.body[&a];
                let (cur, amt) = match v {
                    Value::Object(o) => (
                        o["currency"].as_str().unwrap_or_default().to_string(),
                        o["amount"].as_str().unwrap_or("0"),
                    ),
                    other => (String::new(), other.as_str().unwrap_or("0")),
                };
                let d = Decimal::parse(amt)?;
                let sum = g.remove(&cur).map_or(d.clone(), |x| x.add(&d));
                g.insert(cur, sum);
            }
        }
        if groups.values().any(|t| t.values().any(|x| !x.is_zero())) {
            return Err(BpaError::new(
                "LEDG.UNBALANCED",
                "posting group does not balance",
            ));
        }
        Ok(())
    }

    /// State-tree entries of a state (STD-PROOF-001).
    pub fn state_entries(&self, st: &State) -> Result<Vec<(String, String)>, BpaError> {
        let mut out: Vec<(String, String)> = st
            .objects
            .iter()
            .filter(|(_, o)| o.kind != "system")
            .map(|(id, o)| (format!("o/{id}"), o.head.clone()))
            .collect();
        for (class, _, q) in &st.ledger {
            let ch = content_id("ubos.chunk.v1", &json!(class))?[7..].to_string();
            let kh = match q.first() {
                Some(e) => content_id("ubos.chunk.v1", &e.ledger_key)?[7..].to_string(),
                None => String::new(),
            };
            for e in q {
                out.push((format!("l/{ch}/{kh}/{:020}", e.seq), e.entry_hash.clone()));
            }
        }
        out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        Ok(out)
    }
}

fn negate(v: &Value) -> Result<Value, BpaError> {
    Ok(match v {
        Value::Object(o) => {
            let cur = o["currency"].as_str().unwrap_or_default();
            let d = Decimal::parse(o["amount"].as_str().unwrap_or("0"))?.neg();
            json!({"amount": d.format(minor_unit(cur)), "currency": cur})
        }
        Value::String(s) => json!(Decimal::parse(s)?.neg().format(None)),
        other => other.clone(),
    })
}

/// An open commit on a branch.
#[derive(Debug, Clone)]
pub struct Tx {
    /// Branch name.
    pub branch: String,
    /// Head the commit is built on.
    pub head: u64,
    /// Ordinal of the head (the snapshot the transaction reads).
    pub base: u64,
    /// cseq the commit will get.
    pub cseq: u64,
    /// The loaded part of the branch state, with the changes of this commit.
    pub state: State,
    /// Objects as loaded from the index (`None`: absent).
    pub loaded: HashMap<String, Option<ObjectState>>,
    /// Entries that existed before this commit.
    pub loaded_entries: HashSet<String>,
    /// Ledger keys whose entries are loaded.
    pub loaded_queues: HashSet<(String, String)>,
    /// New version records.
    pub versions: Vec<(String, Value)>,
    /// Entries appended in this commit.
    pub appended: Vec<Entry>,
}
