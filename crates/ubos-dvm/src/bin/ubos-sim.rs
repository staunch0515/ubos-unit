//! `ubos-sim` — the deterministic concurrency and crash simulation of the kernel
//! (SUITE-DVM-SIM, VER-SIM-0001, EXIT-1-05, DSN-DVM-516; PH-1 step 8b, DEC-061).
//!
//! `ubos-sim [--seeds N] [--from S] [--threads T] [--report FILE] [--only SEED] [--verbose]
//!           [--inject lost_ack]`
//!
//! `--inject lost_ack` seeds a defect for the checker's sensitivity: the store acknowledges a
//! flush it did not apply (the failure NR-DUR-001 forbids); the run must then report
//! violations.
//!
//! Every seed builds one simulated cluster: 2–4 nodes (engines) over one shared in-memory
//! store, 1–3 branches, six contended objects and two contended ledger keys. A seeded
//! scheduler picks the node, the branch and the transaction of every step: reads of one or
//! two objects, updates against the versions read (optionally in serializable mode with the
//! read set declared), appends, clock jumps forwards and backwards on one node, and crashes
//! injected into the storage flush — an aborted flush (the storage transaction rolls back,
//! the node sees a storage error) or a crash after the flush applied (the commit exists, the
//! acknowledgement is lost, the node restarts with empty caches). A node retries
//! `TXN.HEAD_MOVED` and `TXN.READ_SET_CHANGED` up to five times (DSN-DVM-513).
//!
//! After every step and at the end the checker verifies: a linear history per branch (dense
//! `cseq` from 1 to the head, every commit's parent is the previous commit), no lost
//! acknowledged commit (every acknowledged commit is in the store at its `cseq` with its
//! versions readable), unacknowledged commits are at most the crashes after apply, gap-free
//! ledger sequences per key, and the serializable-history check in the manner of Elle: per
//! branch the written versions of the commits in `cseq` order form one chain per object
//! (every version's predecessor is the version the previous write of that object produced),
//! and a serializable transaction's read set was the head when it committed. A failing seed
//! is reproduced with `--only SEED`.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use ubos_dvm::Adapter;
use ubos_dvm::stateful::Engine;
use ubos_store::{BranchRef, Flush, MemStore, Store};
use ubos_types::BpaError;

// ---- deterministic generator ----

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x5DEE_CE66_D1CE_4E5D)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

// ---- shared store with fault injection ----

/// The fault the next flush suffers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fault {
    /// The storage transaction rolls back: nothing is applied, the node sees an error.
    AbortBeforeApply,
    /// The storage transaction committed, the node crashes before acknowledging.
    CrashAfterApply,
}

#[derive(Default)]
struct Cluster {
    store: MemStore,
    next_fault: Option<Fault>,
    aborted: u64,
    crashed_after_apply: u64,
    /// Seeded defect: acknowledge without applying (sensitivity check).
    inject_lost_ack: bool,
}

/// One node's handle on the shared store: every call takes the cluster lock, which plays the
/// branch sequencer's lock (DSN-DVM-006) in the simulation.
#[derive(Clone)]
struct Shared(Arc<Mutex<Cluster>>);

impl Shared {
    fn with<T>(&self, f: impl FnOnce(&mut Cluster) -> T) -> T {
        let mut c = self.0.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut c)
    }
}

fn unavailable() -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", "simulated crash")
}

impl Store for Shared {
    fn reset(&mut self) -> Result<(), BpaError> {
        self.with(|c| c.store.reset())
    }
    fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.with(|c| c.store.version(hash))
    }
    fn commit(&mut self, id: u64) -> Result<Value, BpaError> {
        self.with(|c| c.store.commit(id))
    }
    fn commit_at(&mut self, branch: &str, cseq: u64) -> Result<Option<u64>, BpaError> {
        self.with(|c| c.store.commit_at(branch, cseq))
    }
    fn commit_by_id(&mut self, commit_id: &str) -> Result<Option<u64>, BpaError> {
        self.with(|c| c.store.commit_by_id(commit_id))
    }
    fn next_commit_id(&mut self) -> Result<u64, BpaError> {
        self.with(|c| c.store.next_commit_id())
    }
    fn branch(&mut self, name: &str) -> Result<Option<BranchRef>, BpaError> {
        self.with(|c| c.store.branch(name))
    }
    fn branch_names(&mut self) -> Result<Vec<String>, BpaError> {
        self.with(|c| c.store.branch_names())
    }
    fn tag_names(&mut self) -> Result<Vec<String>, BpaError> {
        self.with(|c| c.store.tag_names())
    }
    fn create_branch(&mut self, name: &str, r: &BranchRef) -> Result<bool, BpaError> {
        self.with(|c| c.store.create_branch(name, r))
    }
    fn flush(&mut self, f: &Flush) -> Result<bool, BpaError> {
        self.with(|c| match c.next_fault.take() {
            Some(Fault::AbortBeforeApply) => {
                c.aborted += 1;
                Err(unavailable())
            }
            Some(Fault::CrashAfterApply) => {
                let applied = c.store.flush(f)?;
                if applied {
                    c.crashed_after_apply += 1;
                    Err(unavailable())
                } else {
                    Ok(false)
                }
            }
            None if c.inject_lost_ack && f.cseq >= 7 && f.cseq.is_multiple_of(7) => Ok(true),
            None => c.store.flush(f),
        })
    }
    fn object_slugs(&mut self, object_id: &str) -> Result<Vec<String>, BpaError> {
        self.with(|c| c.store.object_slugs(object_id))
    }
    fn object(
        &mut self,
        branch: &str,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Value>>, BpaError> {
        self.with(|c| c.store.object(branch, cseq, object_id))
    }
    fn objects(
        &mut self,
        branch: &str,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Value>)>, BpaError> {
        self.with(|c| c.store.objects(branch, cseq, class))
    }
    fn entries(
        &mut self,
        branch: &str,
        cseq: u64,
        key: Option<(&str, &str)>,
        entry_id: Option<&str>,
    ) -> Result<Vec<Value>, BpaError> {
        self.with(|c| c.store.entries(branch, cseq, key, entry_id))
    }
    fn node(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.with(|c| c.store.node(hash))
    }
    fn put_meta(&mut self, key: &str, value: &Value) -> Result<(), BpaError> {
        self.with(|c| c.store.put_meta(key, value))
    }
    fn meta(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        self.with(|c| c.store.meta(key))
    }
    fn backend(&self) -> &'static str {
        "memory"
    }
}

// ---- the model ----

const OBJECTS: u64 = 6;
const KEYS: [&str; 2] = ["bolt", "nut"];
const BRANCHES: [&str; 3] = ["main", "feature/a", "feature/b"];

fn uid(n: u64) -> String {
    format!("01890a5d-ac96-7{:03x}-8000-{:012x}", n % 4096, n)
}

fn classes() -> Value {
    let f = |n: &str| json!({"name": n, "type": "text", "label": n, "description": n});
    json!([
        {"name": "Item", "namespace": "sim", "kind": "definition", "parent": "DataObject", "label": "Item",
         "description": "Item", "schema_version": 1, "fields": [f("a"), f("b"), f("c")]},
        {"name": "StockMove", "namespace": "sim", "kind": "ledger", "parent": "DataObject", "label": "StockMove",
         "description": "StockMove", "schema_version": 1,
         "fields": [{"name": "item", "type": "text", "label": "item", "description": "item", "required": true},
                    {"name": "quantity", "type": "decimal", "label": "quantity", "description": "quantity", "required": true}],
         "ledger": {"key": ["item"], "amounts": ["quantity"], "double_entry": false}}
    ])
}

/// An acknowledged commit as the client saw it.
#[derive(Debug, Clone)]
struct Ack {
    node: usize,
    branch: String,
    cseq: u64,
    commit_id: String,
    /// Written versions by object.
    versions: BTreeMap<String, String>,
    /// Bases of the updates by object.
    bases: BTreeMap<String, String>,
    /// Appended entry ids by (key).
    entries: Vec<(String, String)>,
    /// The read set of a serializable transaction.
    read_set: Option<BTreeMap<String, String>>,
}

struct Node {
    adapter: Adapter,
    now: String,
    restarts: u64,
}

struct Sim {
    seed: u64,
    rng: Rng,
    shared: Shared,
    nodes: Vec<Node>,
    branches: Vec<&'static str>,
    acks: Vec<Ack>,
    next_entry: u64,
    steps: u64,
    retries: u64,
    head_moved: u64,
    read_set_changed: u64,
    clock_jumps: u64,
    rejected_clock: u64,
    log: Vec<String>,
}

fn instant(day: u64, hour: u64) -> String {
    format!("2026-03-{:02}T{:02}:00:00.000000Z", 1 + day % 28, hour % 24)
}

impl Sim {
    fn new(seed: u64, inject: Option<&str>) -> Result<Sim, BpaError> {
        let mut rng = Rng::new(seed);
        let shared = Shared(Arc::new(Mutex::new(Cluster {
            inject_lost_ack: inject == Some("lost_ack"),
            ..Cluster::default()
        })));
        let n_nodes = 2 + rng.below(3) as usize;
        let n_branches = 1 + rng.below(3) as usize;
        let mut nodes = Vec::new();
        for i in 0..n_nodes {
            let engine = if i == 0 {
                Engine::new(Box::new(shared.clone()))?
            } else {
                Engine::attach(Box::new(shared.clone()), &classes())?
            };
            nodes.push(Node {
                adapter: Adapter::from_engine(engine),
                now: instant(0, 0),
                restarts: 0,
            });
        }
        let mut sim = Sim {
            seed,
            rng,
            shared,
            nodes,
            branches: BRANCHES[..n_branches].to_vec(),
            acks: Vec::new(),
            next_entry: 1000,
            steps: 0,
            retries: 0,
            head_moved: 0,
            read_set_changed: 0,
            clock_jumps: 0,
            rejected_clock: 0,
            log: Vec::new(),
        };
        let n0 = &mut sim.nodes[0].adapter;
        n0.execute("set_now", &json!({"now": instant(0, 0)}))?;
        n0.execute("load_genesis", &json!({"classes": classes()}))?;
        let creates: Vec<Value> = (0..OBJECTS)
            .map(|o| {
                json!({"op": "create", "object_id": uid(o), "class": "Item", "slug": format!("i{o}"),
                       "body": {"a": "0", "b": "0", "c": "0"}, "valid": {"from": "2026-01-01"}})
            })
            .collect();
        let r = n0.execute("commit", &json!({"branch": "main", "changes": creates}))?;
        sim.record_ack(0, "main", &r, &BTreeMap::new(), Vec::new(), None);
        for b in sim.branches.clone().into_iter().skip(1) {
            sim.nodes[0]
                .adapter
                .execute("create_branch", &json!({"name": b, "from": "main"}))?;
        }
        Ok(sim)
    }

    fn record_ack(
        &mut self,
        node: usize,
        branch: &str,
        r: &Value,
        bases: &BTreeMap<String, String>,
        entries: Vec<(String, String)>,
        read_set: Option<BTreeMap<String, String>>,
    ) {
        let versions = r["versions"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        self.acks.push(Ack {
            node,
            branch: branch.to_string(),
            cseq: r["cseq"].as_u64().unwrap_or_default(),
            commit_id: r["commit_id"].as_str().unwrap_or_default().to_string(),
            versions,
            bases: bases.clone(),
            entries,
            read_set,
        });
    }

    /// Restart a node: a fresh engine on the shared store, caches empty (DSN-DVM-508).
    fn restart(&mut self, n: usize) -> Result<(), BpaError> {
        let engine = Engine::attach(Box::new(self.shared.clone()), &classes())?;
        let now = self.nodes[n].now.clone();
        let mut adapter = Adapter::from_engine(engine);
        adapter.execute("set_now", &json!({"now": now}))?;
        self.nodes[n].adapter = adapter;
        self.nodes[n].restarts += 1;
        Ok(())
    }

    /// One scheduled step.
    fn step(&mut self) -> Result<(), BpaError> {
        self.steps += 1;
        let n = self.rng.below(self.nodes.len() as u64) as usize;
        let branch = self.branches[self.rng.below(self.branches.len() as u64) as usize];
        // clock jump on this node: forwards or backwards by hours or days
        if self.rng.chance(10) {
            let now = instant(self.rng.below(28), self.rng.below(24));
            self.clock_jumps += 1;
            match self.nodes[n]
                .adapter
                .execute("set_now", &json!({"now": now}))
            {
                Ok(_) => self.nodes[n].now = now,
                Err(_) => self.rejected_clock += 1,
            }
        }
        // the transaction: reads, then updates against what was read, and appends
        let n_reads = 1 + self.rng.below(2);
        let serializable = self.rng.chance(30);
        let mut attempts = 0;
        loop {
            attempts += 1;
            let mut reads: BTreeMap<String, String> = BTreeMap::new();
            for _ in 0..n_reads {
                let o = uid(self.rng.below(OBJECTS));
                let r = self.nodes[n]
                    .adapter
                    .execute("read", &json!({"branch": branch, "object_id": o}))?;
                reads.insert(
                    o,
                    r["version_hash"].as_str().unwrap_or_default().to_string(),
                );
            }
            let mut changes = Vec::new();
            let mut bases = BTreeMap::new();
            let mut entries = Vec::new();
            for (o, v) in &reads {
                if self.rng.chance(70) {
                    let field = ["a", "b", "c"][self.rng.below(3) as usize];
                    changes.push(json!({"op": "update", "object_id": o, "base": v,
                                        "body": {field: format!("n{}s{}", n, self.steps)}}));
                    bases.insert(o.clone(), v.clone());
                }
            }
            if changes.is_empty() || self.rng.chance(40) {
                self.next_entry += 1;
                let key = KEYS[self.rng.below(2) as usize];
                let eid = uid(self.next_entry);
                changes.push(json!({"op": "append", "class": "StockMove", "entry_id": eid, "ledger_key": {"item": key},
                                    "body": {"item": key, "quantity": (self.rng.below(20) as i64 - 5).to_string()},
                                    "valid_at": "2026-03-15"}));
                entries.push((key.to_string(), eid));
            }
            let mut input = json!({"branch": branch, "changes": changes});
            if serializable {
                input["process"] = json!({"serializable": true, "reads": reads});
            }
            // a fault on this flush?
            let fault = match self.rng.below(100) {
                0..=9 => Some(Fault::AbortBeforeApply),
                10..=14 => Some(Fault::CrashAfterApply),
                _ => None,
            };
            self.shared.with(|c| c.next_fault = fault);
            let out = self.nodes[n].adapter.execute("commit", &input);
            self.shared.with(|c| c.next_fault = None);
            match out {
                Ok(r) => {
                    let rs = serializable.then(|| reads.clone());
                    self.record_ack(n, branch, &r, &bases, entries, rs);
                    return Ok(());
                }
                Err(e) if e.code == "TXN.HEAD_MOVED" || e.code == "TXN.READ_SET_CHANGED" => {
                    if e.code == "TXN.HEAD_MOVED" {
                        self.head_moved += 1;
                    } else {
                        self.read_set_changed += 1;
                    }
                    if attempts >= 5 {
                        return Ok(());
                    }
                    self.retries += 1;
                }
                Err(e) if e.code == "NOD.STORAGE_UNAVAILABLE" => {
                    // the node crashed: it restarts and the transaction is lost (DSN-DVM-508)
                    self.restart(n)?;
                    return Ok(());
                }
                Err(e) if e.code.starts_with("TIME.") => return Ok(()),
                Err(e) => {
                    return Err(BpaError::new(
                        e.code,
                        format!(
                            "unexpected commit outcome at step {}: {}",
                            self.steps, e.message
                        ),
                    ));
                }
            }
        }
    }

    // ---- the checker ----

    fn check(&mut self, final_: bool) -> Result<(), String> {
        let mut store = self.shared.clone();
        let fail = |m: String| Err(format!("seed {}: {m}", self.seed));
        let crashes_after_apply = self.shared.with(|c| c.crashed_after_apply);
        let mut store_commits = 0u64;
        for &b in &self.branches {
            let Some(head) = store.branch(b).map_err(|e| e.to_string())? else {
                return fail(format!("branch {b} missing"));
            };
            // 1. linear history: dense cseq from the fork point, parent chain
            let first = head.parent.as_ref().map(|p| p.1 + 1).unwrap_or(1);
            let mut prev: Option<u64> = None;
            for c in first..=head.head_cseq {
                let Some(id) = store.commit_at(b, c).map_err(|e| e.to_string())? else {
                    return fail(format!(
                        "{b}: no commit at cseq {c} (head {})",
                        head.head_cseq
                    ));
                };
                let rec = store.commit(id).map_err(|e| e.to_string())?;
                if rec["cseq"].as_u64() != Some(c) {
                    return fail(format!("{b}@{c}: record carries cseq {}", rec["cseq"]));
                }
                if let Some(p) = prev {
                    let parents: Vec<u64> = rec["parents"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    if !parents.contains(&p) {
                        return fail(format!("{b}@{c}: parents {parents:?} do not include {p}"));
                    }
                }
                prev = Some(id);
                store_commits += 1;
            }
            if store
                .commit_at(b, head.head_cseq + 1)
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return fail(format!(
                    "{b}: a commit beyond the head cseq {}",
                    head.head_cseq
                ));
            }
            if prev.is_some() && prev != Some(head.head) {
                return fail(format!("{b}: head {} is not the last commit", head.head));
            }
            // 5. serializable history: one version chain per object, read sets valid at commit
            let mut heads: BTreeMap<String, String> = BTreeMap::new();
            if let Some(parent) = &head.parent {
                // a branch starts from its fork point: replay the parent's write sets up to it
                fork_heads(&mut store, &parent.0, parent.1, &mut heads)?;
            }
            for c in first..=head.head_cseq {
                let id = store
                    .commit_at(b, c)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                let rec = store.commit(id).map_err(|e| e.to_string())?;
                let ack = self.acks.iter().find(|a| a.branch == b && a.cseq == c);
                if let Some(a) = ack {
                    if a.commit_id != rec["commit_id"].as_str().unwrap_or_default() {
                        return fail(format!(
                            "{b}@{c}: acknowledged commit {} but the store holds {}",
                            a.commit_id, rec["commit_id"]
                        ));
                    }
                    for (o, base) in &a.bases {
                        if heads.get(o).is_some_and(|h| h != base) {
                            return fail(format!(
                                "{b}@{c}: update of {o} based on {base} but the previous write produced {}",
                                heads[o]
                            ));
                        }
                    }
                    if let Some(rs) = &a.read_set {
                        for (o, v) in rs {
                            if heads.get(o).is_some_and(|h| h != v) {
                                return fail(format!(
                                    "{b}@{c}: serializable read of {o} at {v} but the head was {}",
                                    heads[o]
                                ));
                            }
                        }
                    }
                    for (o, v) in &a.versions {
                        heads.insert(o.clone(), v.clone());
                    }
                } else {
                    // an unacknowledged commit: the versions of its write set must still chain
                    for (o, v) in written(&rec) {
                        let vrec = store.version(&v).map_err(|e| e.to_string())?;
                        if let (Some(p), Some(h)) = (vrec["predecessor"].as_str(), heads.get(&o))
                            && p != h
                        {
                            return fail(format!(
                                "{b}@{c}: version of {o} has predecessor {p} but the previous write produced {h}"
                            ));
                        }
                        heads.insert(o, v);
                    }
                }
            }
            // 4. gap-free ledger sequences per key
            for key in KEYS {
                let r = self.nodes[0]
                    .adapter
                    .execute(
                        "read_ledger",
                        &json!({"branch": b, "class": "StockMove", "ledger_key": {"item": key}}),
                    )
                    .map_err(|e| e.to_string())?;
                let seqs: Vec<u64> = r["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e["seq"].as_u64())
                    .collect();
                for (i, s) in seqs.iter().enumerate() {
                    if *s != i as u64 + 1 {
                        return fail(format!("{b} {key}: ledger sequence {s} at position {i}"));
                    }
                }
            }
        }
        // 2. no lost acknowledged commit
        for a in &self.acks {
            let at = store
                .commit_at(&a.branch, a.cseq)
                .map_err(|e| e.to_string())?;
            let Some(id) = at else {
                return fail(format!(
                    "lost acknowledged commit {}@{} ({}) of node {}",
                    a.branch, a.cseq, a.commit_id, a.node
                ));
            };
            let rec = store.commit(id).map_err(|e| e.to_string())?;
            if rec["commit_id"].as_str() != Some(a.commit_id.as_str()) {
                return fail(format!(
                    "{}@{}: acknowledged {} but the store holds {}",
                    a.branch, a.cseq, a.commit_id, rec["commit_id"]
                ));
            }
            for (o, v) in &a.versions {
                if store.version(v).is_err() {
                    return fail(format!(
                        "{}@{}: acknowledged version {v} of {o} missing",
                        a.branch, a.cseq
                    ));
                }
            }
            for (key, eid) in &a.entries {
                let r = self.nodes[0]
                    .adapter
                    .execute("read_ledger", &json!({"branch": a.branch, "class": "StockMove", "ledger_key": {"item": key}}))
                    .map_err(|e| e.to_string())?;
                if !r["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|e| e["entry_id"].as_str() == Some(eid.as_str()))
                {
                    return fail(format!(
                        "{}@{}: acknowledged entry {eid} missing",
                        a.branch, a.cseq
                    ));
                }
            }
        }
        // 3. unacknowledged commits are the crashes after apply
        let unacked = store_commits.saturating_sub(self.acks.len() as u64);
        if unacked > crashes_after_apply {
            return fail(format!(
                "{unacked} commits without acknowledgement but only {crashes_after_apply} crashes after apply"
            ));
        }
        if final_ && unacked != crashes_after_apply {
            return fail(format!(
                "{unacked} commits without acknowledgement, {crashes_after_apply} crashes after apply"
            ));
        }
        Ok(())
    }

    fn run(seed: u64, verbose: bool, inject: Option<&str>) -> Result<Value, String> {
        let mut sim = Sim::new(seed, inject).map_err(|e| format!("seed {seed}: setup: {e}"))?;
        let steps = 30 + sim.rng.below(90);
        for _ in 0..steps {
            sim.step().map_err(|e| format!("seed {seed}: {e}"))?;
            if sim.rng.chance(4) {
                sim.check(false)?;
            }
        }
        sim.check(true)?;
        let (aborted, crashed) = sim.shared.with(|c| (c.aborted, c.crashed_after_apply));
        let restarts: u64 = sim.nodes.iter().map(|n| n.restarts).sum();
        if verbose {
            for l in &sim.log {
                eprintln!("{l}");
            }
        }
        Ok(
            json!({"seed": seed, "nodes": sim.nodes.len(), "branches": sim.branches.len(), "steps": sim.steps,
                  "commits": sim.acks.len(), "head_moved": sim.head_moved, "read_set_changed": sim.read_set_changed,
                  "retries": sim.retries, "aborted_flushes": aborted, "crashes_after_apply": crashed,
                  "restarts": restarts, "clock_jumps": sim.clock_jumps, "rejected_clock_jumps": sim.rejected_clock}),
        )
    }
}

/// The head versions written by a commit, from its write set (`objects`, DEC-053).
fn written(rec: &Value) -> Vec<(String, String)> {
    rec["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            Some((
                o["object_id"].as_str()?.to_string(),
                o["state"]["head"].as_str()?.to_string(),
            ))
        })
        .collect()
}

/// The object heads of `branch` at `cseq`, replayed from the write sets of its lineage.
fn fork_heads(
    store: &mut Shared,
    branch: &str,
    cseq: u64,
    heads: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let Some(b) = store.branch(branch).map_err(|e| e.to_string())? else {
        return Err(format!("fork parent {branch} missing"));
    };
    let first = match &b.parent {
        Some(p) => {
            fork_heads(store, &p.0, p.1, heads)?;
            p.1 + 1
        }
        None => 1,
    };
    for c in first..=cseq {
        if let Some(id) = store.commit_at(branch, c).map_err(|e| e.to_string())? {
            let rec = store.commit(id).map_err(|e| e.to_string())?;
            for (o, v) in written(&rec) {
                heads.insert(o, v);
            }
        }
    }
    Ok(())
}

// ---- driver ----

static TOTALS: [AtomicU64; 9] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
const KEYS_T: [&str; 9] = [
    "steps",
    "commits",
    "head_moved",
    "read_set_changed",
    "retries",
    "aborted_flushes",
    "crashes_after_apply",
    "restarts",
    "clock_jumps",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let verbose = args.iter().any(|a| a == "--verbose");
    let seeds: u64 = opt("--seeds").and_then(|v| v.parse().ok()).unwrap_or(1_000);
    let from: u64 = opt("--from").and_then(|v| v.parse().ok()).unwrap_or(1);
    let threads: u64 = opt("--threads").and_then(|v| v.parse().ok()).unwrap_or(4);
    let inject = opt("--inject");
    if let Some(only) = opt("--only").and_then(|v| v.parse::<u64>().ok()) {
        match Sim::run(only, true, inject.as_deref()) {
            Ok(v) => println!("{v}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let inject = inject.clone();
            std::thread::spawn(move || {
                let mut failures = Vec::new();
                let mut seed = from + t;
                while seed < from + seeds {
                    match Sim::run(seed, false, inject.as_deref()) {
                        Ok(v) => {
                            for (i, k) in KEYS_T.iter().enumerate() {
                                TOTALS[i].fetch_add(
                                    v[k].as_u64().unwrap_or_default(),
                                    Ordering::Relaxed,
                                );
                            }
                        }
                        Err(e) => {
                            if verbose {
                                eprintln!("{e}");
                            }
                            failures.push(json!({"seed": seed, "error": e}));
                            if failures.len() >= 10 {
                                break;
                            }
                        }
                    }
                    seed += threads;
                }
                failures
            })
        })
        .collect();
    let failures: Vec<Value> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap_or_default())
        .collect();
    let secs = start.elapsed().as_secs_f64();
    let mut totals = serde_json::Map::new();
    for (i, k) in KEYS_T.iter().enumerate() {
        totals.insert(k.to_string(), json!(TOTALS[i].load(Ordering::Relaxed)));
    }
    println!(
        "SUITE-DVM-SIM: seeds {from}..{}: {} violations, {secs:.1} s; {}",
        from + seeds - 1,
        failures.len(),
        Value::Object(totals.clone())
    );
    if let Some(path) = opt("--report") {
        let report = json!({"suite": "SUITE-DVM-SIM", "item": "VER-SIM-0001", "criterion": "EXIT-1-05",
                            "seeds": {"from": from, "to": from + seeds - 1, "count": seeds}, "threads": threads, "inject": inject,
                            "violations": failures, "totals": totals, "seconds": (secs * 10.0).round() / 10.0});
        std::fs::write(
            path,
            serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
        )
        .expect("write report");
    }
    std::process::exit(if failures.is_empty() { 0 } else { 1 });
}
