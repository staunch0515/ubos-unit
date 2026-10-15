//! Property checks of the PH-0 prototype (SUITE-DVM-PROP, EXIT-0-04, DEC-033).
//!
//! `ubos-props [--cases N] [--seed S] [--threads T] [--only NAME] [--report FILE]`
//!
//! Every case is generated from `(seed, property, case index)` by a SplitMix64 generator, so a
//! failing case is replayed by its index. Properties:
//! - `root_history_independence` (VER-PROP-0001): two operation sequences with inserts,
//!   updates, deletes and re-inserts that reach the same key–value state give equal roots;
//!   different states give different roots.
//! - `timeline_non_overlap` (VER-PROP-0002): after every accepted timeline operation the
//!   segments are sorted, non-empty and non-overlapping (STD-TIME-011).
//! - `merge_determinism` (VER-PROP-0003): merging R into L and L into R from the same histories
//!   gives the same outcome — the same conflicts, or the same body and valid period of every
//!   object at every date (STD-VER-040, STD-VER-050).
//! - `tree_incremental` (VER-PROP-0004): batches of inserts, updates and removals applied to
//!   stored nodes give the root of a full rebuild, and the tree diff of two roots equals the
//!   key-wise difference of their entry sets (DSN-DVM-007).
//! - VER-PROP-0100 (PH-1 step 8a): `index_rebuild_equality` (reads through the kernel indexes
//!   equal reads through the state rebuilt from the commit records, root verifier clean),
//!   `diff_cost_bound` (a tree diff reads at most `2·(h+1)·(3·c+1)` nodes for `c` changes),
//!   `merge_ledger_completeness` (a merge appends every entry of the branch in order with
//!   dense sequences and the same balance), `balance_bitemporal` (the balance at any
//!   `(asof, cseq)` is the sum of the included entries) and `cursor_completeness` (pages of a
//!   query under concurrent commits return the snapshot exactly once, in order).
//! - VER-PROP-0100 (PH-1 step 12c): `process_state_machine` (only the transitions of
//!   UBS-SYS-DVM-05 §1 are possible, and every execution records a path of the table ending
//!   in the terminal phase its outcome implies; DSN-DVM-501).

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Merge outcomes seen: conflicts, merges that changed the target, merges without change.
static MERGE_OUTCOMES: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
use ubos_dvm::Adapter;
use ubos_dvm::process::{Event, Phase};
use ubos_time::Segment;

struct Rng(u64);

impl Rng {
    fn new(seed: u64, property: u64, case: u64) -> Rng {
        Rng(seed
            ^ property.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ case.wrapping_mul(0xD1B5_4A32_D192_ED03))
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
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}

// ---- VER-PROP-0001 ----

fn key(i: u64) -> String {
    if i.is_multiple_of(3) {
        format!("l/{:064x}/{:064x}/{:020}", i % 7, i % 5, i)
    } else {
        format!("o/01890a5d-ac96-7{:03x}-8000-{:012x}", i % 4096, i)
    }
}

fn value(i: u64) -> String {
    format!("sha256:{:064x}", i.wrapping_mul(0x2545_F491_4F6C_DD1D))
}

/// Apply a random history that ends in `target`; return the order in which the final
/// entries were last written.
fn history(rng: &mut Rng, target: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut state: BTreeMap<String, String> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut keys: Vec<&String> = target.keys().collect();
    rng.shuffle(&mut keys);
    for k in keys {
        if rng.chance(30) {
            state.insert(k.clone(), value(rng.next()));
        }
        if rng.chance(20) {
            state.remove(k);
        }
        let noise = key(1_000_000 + rng.below(50));
        if rng.chance(25) {
            state.insert(noise.clone(), value(rng.next()));
        }
        state.insert(k.clone(), target[k].clone());
        order.retain(|x| x != k);
        order.push(k.clone());
        if rng.chance(25) {
            state.remove(&noise);
        }
    }
    let extra: Vec<String> = state
        .keys()
        .filter(|k| !target.contains_key(*k))
        .cloned()
        .collect();
    for k in extra {
        state.remove(&k);
    }
    assert_eq!(&state, target, "history must reach the target state");
    order
        .into_iter()
        .map(|k| (k.clone(), state[&k].clone()))
        .collect()
}

fn prop_roots(rng: &mut Rng) -> Result<(), String> {
    let n = 1 + rng.below(40);
    let mut target = BTreeMap::new();
    for _ in 0..n {
        target.insert(key(rng.below(200)), value(rng.next()));
    }
    let a = history(rng, &target);
    let b = history(rng, &target);
    let ra = ubos_tree::tree_root(&a).map_err(|e| e.to_string())?;
    let rb = ubos_tree::tree_root(&b).map_err(|e| e.to_string())?;
    if ra != rb {
        return Err(format!("roots differ for equal states: {ra} {rb}"));
    }
    let mut other = target.clone();
    let k = other.keys().next().cloned().expect("non-empty");
    other.insert(k, value(rng.next() | 1));
    let rc =
        ubos_tree::tree_root(&other.into_iter().collect::<Vec<_>>()).map_err(|e| e.to_string())?;
    if rc == ra {
        return Err("different states give equal roots".into());
    }
    Ok(())
}

// ---- VER-PROP-0002 ----

fn date(d: u64) -> String {
    format!("2026-01-{:02}", 1 + d)
}

fn check_timeline(tl: &[Segment]) -> Result<(), String> {
    for s in tl {
        if !ubos_time::lt(Some(&s.from), s.to.as_deref()) {
            return Err(format!("empty segment {s:?}"));
        }
    }
    for w in tl.windows(2) {
        if w[0].from >= w[1].from || ubos_time::lt(Some(&w[1].from), w[0].to.as_deref()) {
            return Err(format!("overlapping or unsorted segments {w:?}"));
        }
    }
    Ok(())
}

fn prop_timeline(rng: &mut Rng) -> Result<(), String> {
    let mut tl = ubos_time::apply(&[], "create", "v0", &date(rng.below(20)), None)
        .map_err(|e| e.0.to_string())?;
    let ops = 1 + rng.below(8);
    for i in 1..=ops {
        let v = format!("v{i}");
        let a = rng.below(20);
        let (kind, b) = match rng.below(4) {
            0 => ("supersede_from", None),
            1 => ("set_period", Some(date(a + 1 + rng.below(20 - a.min(19))))),
            2 => ("replace_future", None),
            _ => ("end", None),
        };
        match ubos_time::apply(&tl, kind, &v, &date(a), b.as_deref()) {
            Ok(next) => {
                check_timeline(&next).map_err(|e| format!("after {kind} {}: {e}", date(a)))?;
                tl = next;
            }
            Err(_) => continue,
        }
        if tl.is_empty() {
            break;
        }
    }
    Ok(())
}

// ---- VER-PROP-0003 ----

fn uid(n: u64) -> String {
    format!("01890a5d-ac96-7{:03x}-8000-{:012x}", n, n)
}

fn run(a: &mut Adapter, op: &str, input: Value) -> Result<Value, String> {
    a.execute(op, &input).map_err(|e| {
        format!(
            "{}{}",
            e.code,
            e.details.map(|d| d.to_string()).unwrap_or_default()
        )
    })
}

struct Side {
    heads: BTreeMap<String, String>,
}

fn random_change(rng: &mut Rng, side: &Side) -> Option<Value> {
    let objs: Vec<&String> = side.heads.keys().collect();
    let oid = objs[rng.below(objs.len() as u64) as usize].clone();
    let base = side.heads[&oid].clone();
    let field = ["a", "b", "c"][rng.below(3) as usize];
    let valid = json!({"kind": "supersede_from", "from": date(rng.below(10))});
    Some(match rng.below(10) {
        0 => {
            json!({"op": "end", "object_id": oid, "base": base, "valid": {"from": date(5 + rng.below(10))}})
        }
        1 => json!({"op": "update", "object_id": oid, "base": base, "body": {field: null}}),
        _ => {
            json!({"op": "update", "object_id": oid, "base": base, "body": {field: format!("x{}", rng.below(3))}, "valid": valid})
        }
    })
}

fn build(a: &mut Adapter, seed_case: (u64, u64)) -> Result<(), String> {
    let mut rng = Rng::new(seed_case.0, 3, seed_case.1);
    run(a, "set_now", json!({"now": "2026-01-15T00:00:00.000000Z"}))?;
    let f = |n: &str| json!({"name": n, "type": "text", "label": n, "description": n});
    run(
        a,
        "load_genesis",
        json!({"classes": [{"name": "Item", "namespace": "p", "kind": "definition", "parent": "DataObject",
        "label": "Item", "description": "Item", "schema_version": 1, "fields": [f("a"), f("b"), f("c")]}]}),
    )?;
    let n = 1 + rng.below(3);
    let creates: Vec<Value> = (0..n)
        .map(|i| {
            json!({"op": "create", "object_id": uid(i), "class": "Item", "slug": format!("i{i}"),
                        "body": {"a": "0", "b": "0"}, "valid": {"from": date(0)}})
        })
        .collect();
    let r = run(a, "commit", json!({"branch": "main", "changes": creates}))?;
    let base: BTreeMap<String, String> = r["versions"]
        .as_object()
        .expect("versions")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect();
    run(
        a,
        "create_branch",
        json!({"name": "feature/l", "from": "main"}),
    )?;
    run(
        a,
        "create_branch",
        json!({"name": "feature/r", "from": "main"}),
    )?;
    for branch in ["feature/l", "feature/r"] {
        let mut side = Side {
            heads: base.clone(),
        };
        for _ in 0..(1 + rng.below(4)) {
            let Some(ch) = random_change(&mut rng, &side) else {
                continue;
            };
            if let Ok(r) = run(a, "commit", json!({"branch": branch, "changes": [ch]})) {
                for (k, v) in r["versions"].as_object().into_iter().flatten() {
                    side.heads
                        .insert(k.clone(), v.as_str().unwrap_or_default().to_string());
                }
            }
        }
    }
    Ok(())
}

fn outcome(a: &mut Adapter, source: &str, target: &str, objects: u64) -> Result<Value, String> {
    match a.execute("merge", &json!({"source": source, "target": target})) {
        Err(e) if e.code == "VER.MERGE_CONFLICT" => {
            let mut set: Vec<String> = e
                .details
                .and_then(|d| d["conflicts"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .map(|c| c.to_string())
                .collect();
            set.sort();
            Ok(json!({"conflicts": set}))
        }
        Err(e) => Err(e.to_string()),
        Ok(_) => {
            let mut reads = Vec::new();
            for o in 0..objects {
                for d in 0..20 {
                    let r = a.execute(
                        "read",
                        &json!({"branch": target, "object_id": uid(o), "asof": date(d)}),
                    );
                    reads.push(match r {
                        Ok(v) => json!([v["body"], v["valid"]]),
                        Err(e) => json!(e.code),
                    });
                }
            }
            Ok(json!({"reads": reads}))
        }
    }
}

fn outcome_reads(a: &mut Adapter, branch: &str, objects: u64) -> Option<Value> {
    let mut reads = Vec::new();
    for o in 0..objects {
        for d in 0..20 {
            let r = a.execute(
                "read",
                &json!({"branch": branch, "object_id": uid(o), "asof": date(d)}),
            );
            reads.push(match r {
                Ok(v) => json!([v["body"], v["valid"]]),
                Err(e) => json!(e.code),
            });
        }
    }
    Some(Value::Array(reads))
}

fn prop_merge(seed: u64, case: u64) -> Result<(), String> {
    let objects = 1 + Rng::new(seed, 3, case).below(3);
    let mut a = Adapter::new().map_err(|e| e.to_string())?;
    build(&mut a, (seed, case))?;
    let x = outcome(&mut a, "feature/r", "feature/l", objects)?;
    let mut b = Adapter::new().map_err(|e| e.to_string())?;
    build(&mut b, (seed, case))?;
    let y = outcome(&mut b, "feature/l", "feature/r", objects)?;
    let before = outcome_reads(&mut b, "main", objects);
    let kind = if x.get("conflicts").is_some() {
        0
    } else if Some(&x["reads"]) != before.as_ref() {
        1
    } else {
        2
    };
    MERGE_OUTCOMES[kind].fetch_add(1, Ordering::Relaxed);
    if x != y {
        return Err(format!("merge outcome depends on direction: {x} vs {y}"));
    }
    Ok(())
}

// ---- VER-PROP-0004 ----

fn prop_tree(rng: &mut Rng) -> Result<(), String> {
    let err = |e: ubos_types::BpaError| e.to_string();
    let size = if rng.chance(3) {
        rng.below(20_000)
    } else {
        rng.below(3_000)
    };
    let space = 1 + size * 2;
    let mut state: BTreeMap<String, String> = BTreeMap::new();
    for _ in 0..size {
        state.insert(key(rng.below(space)), value(rng.next()));
    }
    let entries: Vec<(String, String)> = state.clone().into_iter().collect();
    let mut nodes = ubos_tree::MemNodes::default();
    let built = ubos_tree::build(&entries).map_err(err)?;
    if built.root != ubos_tree::tree_root(&entries).map_err(err)? {
        return Err("build differs from tree_root".into());
    }
    nodes.extend(built.nodes);
    let mut root = built.root;
    for _ in 0..1 + rng.below(4) {
        let n = match rng.below(10) {
            0 => 200 + rng.below(800),
            1 => state.len() as u64,
            _ => 1 + rng.below(20),
        };
        let wipe = rng.chance(3);
        let mut changes: BTreeMap<String, Option<String>> = BTreeMap::new();
        let existing: Vec<String> = state.keys().cloned().collect();
        for _ in 0..n {
            let k = if !existing.is_empty() && rng.chance(60) {
                existing[rng.below(existing.len() as u64) as usize].clone()
            } else {
                key(rng.below(space + 100))
            };
            let v = if wipe || rng.chance(35) {
                None
            } else {
                Some(value(rng.next()))
            };
            changes.insert(k, v);
        }
        if wipe {
            for k in &existing {
                changes.insert(k.clone(), None);
            }
        }
        let before = state.clone();
        for (k, v) in &changes {
            match v {
                Some(v) => state.insert(k.clone(), v.clone()),
                None => state.remove(k),
            };
        }
        let w = ubos_tree::update(&mut nodes, &root, &changes).map_err(err)?;
        let expect =
            ubos_tree::tree_root(&state.clone().into_iter().collect::<Vec<_>>()).map_err(err)?;
        if w.root != expect {
            return Err(format!(
                "incremental root {} differs from rebuild {expect} ({} keys, {} changes)",
                w.root,
                state.len(),
                changes.len()
            ));
        }
        nodes.extend(w.nodes);
        let d = ubos_tree::diff(&mut nodes, &root, &w.root).map_err(err)?;
        let mut want = Vec::new();
        for k in before.keys().chain(state.keys()) {
            let (a, b) = (before.get(k), state.get(k));
            if a != b {
                want.push((k.clone(), a.cloned(), b.cloned()));
            }
        }
        want.sort_by(|x, y| x.0.as_bytes().cmp(y.0.as_bytes()));
        want.dedup();
        if d != want {
            return Err(format!(
                "tree diff has {} keys, expected {}",
                d.len(),
                want.len()
            ));
        }
        root = w.root;
    }
    Ok(())
}

// ---- VER-PROP-0100: kernel property set (PH-1 step 8a, DEC-061) ----

/// Diff cost seen: node reads, and the bound `2·(h+1)·(3·c+1)` they are checked against.
static DIFF_COST: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

fn text_field(n: &str) -> Value {
    json!({"name": n, "type": "text", "label": n, "description": n})
}

fn item_class() -> Value {
    json!({"name": "Item", "namespace": "p", "kind": "definition", "parent": "DataObject",
        "label": "Item", "description": "Item", "schema_version": 1,
        "fields": [text_field("a"), text_field("b"), text_field("c")]})
}

fn stock_class() -> Value {
    json!({"name": "StockMove", "namespace": "p", "kind": "ledger", "parent": "DataObject",
        "label": "StockMove", "description": "StockMove", "schema_version": 1,
        "fields": [json!({"name": "item", "type": "text", "label": "item", "description": "item", "required": true}),
                   json!({"name": "quantity", "type": "decimal", "label": "quantity", "description": "quantity", "required": true})],
        "ledger": {"key": ["item"], "amounts": ["quantity"], "double_entry": false}})
}

fn genesis(a: &mut Adapter) -> Result<(), String> {
    run(a, "set_now", json!({"now": "2026-01-15T00:00:00.000000Z"}))?;
    run(
        a,
        "load_genesis",
        json!({"classes": [item_class(), stock_class()]}),
    )?;
    Ok(())
}

fn stock(eid: u64, item: &str, qty: i64, valid_at: &str) -> Value {
    json!({"op": "append", "class": "StockMove", "entry_id": uid(eid), "ledger_key": {"item": item},
        "valid_at": valid_at, "body": {"item": item, "quantity": qty.to_string()}})
}

fn qty(e: &Value) -> i64 {
    e["body"]["quantity"]
        .as_str()
        .and_then(|q| q.parse().ok())
        .unwrap_or(i64::MIN / 4)
}

/// Index rebuild equality (DSN-DVM-005, DSN-DVM-092): a random history of creates, updates,
/// ends and appends on one or two branches; every read of every object and ledger key at
/// every commit and several `asof` dates through the kernel indexes (`time: cseq`) equals the
/// read through the state rebuilt from the commit records (`commit: id`); the root verifier
/// finds nothing.
fn prop_index(rng: &mut Rng) -> Result<(), String> {
    let mut a = Adapter::new().map_err(|e| e.to_string())?;
    genesis(&mut a)?;
    let objects = 1 + rng.below(5);
    let mut branches = vec!["main".to_string()];
    let mut heads: BTreeMap<(String, u64), String> = BTreeMap::new();
    let mut commits: Vec<(String, String, u64)> = Vec::new();
    let mut eid = 100;
    let n_commits = 1 + rng.below(12);
    for c in 0..n_commits {
        if c > 0 && branches.len() == 1 && rng.chance(30) {
            run(
                &mut a,
                "create_branch",
                json!({"name": "feature/x", "from": "main"}),
            )?;
            for o in 0..objects {
                if let Some(h) = heads.get(&("main".to_string(), o)).cloned() {
                    heads.insert(("feature/x".to_string(), o), h);
                }
            }
            branches.push("feature/x".to_string());
        }
        let branch = branches[rng.below(branches.len() as u64) as usize].clone();
        let o = rng.below(objects);
        let mut changes = Vec::new();
        match heads.get(&(branch.clone(), o)).cloned() {
            None => changes.push(json!({"op": "create", "object_id": uid(o), "class": "Item",
                "slug": format!("i{o}"), "body": {"a": format!("a{}", rng.below(4))},
                "valid": {"from": date(rng.below(14))}})),
            Some(base) => {
                let field = ["a", "b", "c"][rng.below(3) as usize];
                changes.push(match rng.below(6) {
                    0 => {
                        json!({"op": "end", "object_id": uid(o), "base": base, "valid": {"from": date(5 + rng.below(15))}})
                    }
                    1 => {
                        json!({"op": "update", "object_id": uid(o), "base": base, "body": {field: format!("v{}", rng.below(5))}})
                    }
                    _ => {
                        json!({"op": "update", "object_id": uid(o), "base": base, "body": {field: format!("v{}", rng.below(5))},
                            "valid": {"kind": "supersede_from", "from": date(rng.below(20))}})
                    }
                });
            }
        }
        if rng.chance(40) {
            eid += 1;
            changes.push(stock(
                eid,
                ["bolt", "nut"][rng.below(2) as usize],
                rng.below(20) as i64 - 5,
                &date(rng.below(20)),
            ));
        }
        let r = match run(
            &mut a,
            "commit",
            json!({"branch": branch, "changes": changes}),
        ) {
            Ok(r) => r,
            // A rejected change (no segment at the date, a stale base, …) is not the property.
            Err(_) => continue,
        };
        for (k, v) in r["versions"].as_object().into_iter().flatten() {
            if let Some(o) = (0..objects).find(|o| uid(*o) == *k) {
                heads.insert(
                    (branch.clone(), o),
                    v.as_str().unwrap_or_default().to_string(),
                );
            }
        }
        commits.push((
            branch.clone(),
            r["commit_id"].as_str().unwrap_or_default().to_string(),
            r["cseq"].as_u64().unwrap_or_default(),
        ));
    }
    if commits.is_empty() {
        return Ok(());
    }
    let dates: Vec<String> = (0..4).map(|_| date(rng.below(20))).collect();
    for (branch, cid, cseq) in &commits {
        for o in 0..objects {
            for d in &dates {
                let by_index = a.execute(
                    "read",
                    &json!({"branch": branch, "object_id": uid(o), "time": cseq, "asof": d}),
                );
                let by_rebuild = a.execute(
                    "read",
                    &json!({"branch": branch, "object_id": uid(o), "commit": cid, "asof": d}),
                );
                let strip = |r: Result<Value, ubos_types::BpaError>| match r {
                    Ok(mut v) => {
                        v.as_object_mut().map(|m| m.remove("evaluated_at"));
                        v
                    }
                    Err(e) => json!(e.code),
                };
                let (x, y) = (strip(by_index), strip(by_rebuild));
                if x != y {
                    return Err(format!(
                        "index read differs from rebuild at {branch}@{cseq} {} asof {d}: {x} vs {y}",
                        uid(o)
                    ));
                }
            }
        }
        for item in ["bolt", "nut"] {
            for d in &dates {
                let x = run(&mut a, "read_ledger", json!({"branch": branch, "class": "StockMove", "ledger_key": {"item": item}, "time": cseq, "asof": d}))
                    .map_err(|e| format!("ledger read by index at {branch}@{cseq} asof {d}: {e}"))?;
                let y = run(&mut a, "read_ledger", json!({"branch": branch, "class": "StockMove", "ledger_key": {"item": item}, "commit": cid, "asof": d}))
                    .map_err(|e| format!("ledger read by rebuild at {branch}@{cseq} ({cid}) asof {d}: {e}"))?;
                if x["entries"] != y["entries"] {
                    return Err(format!(
                        "ledger index read differs from rebuild at {branch}@{cseq} {item}: {} vs {}",
                        x["entries"], y["entries"]
                    ));
                }
            }
        }
    }
    for b in &branches {
        let r = ubos_dvm::verify::verify_roots(a.engine_mut(), b, 0, u64::MAX >> 1)
            .map_err(|e| e.to_string())?;
        if !r.findings.is_empty() {
            return Err(format!("root verifier on {b}: {}", r.to_json()));
        }
    }
    Ok(())
}

/// A node source that counts the nodes it serves.
struct Counting<'a> {
    inner: &'a mut ubos_tree::MemNodes,
    reads: u64,
}

impl ubos_tree::NodeSource for Counting<'_> {
    fn node(&mut self, hash: &str) -> Result<std::rc::Rc<ubos_tree::Node>, ubos_types::BpaError> {
        self.reads += 1;
        self.inner.node(hash)
    }
}

/// Diff cost bound (DSN-DVM-230, STD-VER-031): the tree diff of a tree of `n` keys and the
/// tree after `c` changes reads at most `2·(h+1)·(3·c+1)` nodes, `h` the tree height, so the
/// cost is O(changes × log n); the diff lists exactly the changed keys.
fn prop_diff_cost(rng: &mut Rng) -> Result<(), String> {
    let err = |e: ubos_types::BpaError| e.to_string();
    let size = match rng.below(10) {
        0 => 5_000 + rng.below(15_000),
        1..=3 => 500 + rng.below(4_500),
        _ => 1 + rng.below(500),
    };
    let space = size * 3;
    let mut state: BTreeMap<String, String> = BTreeMap::new();
    for _ in 0..size {
        state.insert(key(rng.below(space)), value(rng.next()));
    }
    let entries: Vec<(String, String)> = state.clone().into_iter().collect();
    let built = ubos_tree::build(&entries).map_err(err)?;
    let mut nodes = ubos_tree::MemNodes::default();
    nodes.extend(built.nodes);
    let root = built.root;
    let height = ubos_tree::NodeSource::node(&mut nodes, &root)
        .map_err(err)?
        .level as u64;
    let c = match rng.below(4) {
        0 => 1,
        1 => 1 + rng.below(5),
        _ => 1 + rng.below(50),
    };
    let existing: Vec<String> = state.keys().cloned().collect();
    let mut changes: BTreeMap<String, Option<String>> = BTreeMap::new();
    while (changes.len() as u64) < c {
        let k = if rng.chance(60) {
            existing[rng.below(existing.len() as u64) as usize].clone()
        } else {
            key(rng.below(space + 100))
        };
        let v = if rng.chance(30) && state.contains_key(&k) {
            None
        } else {
            Some(value(rng.next()))
        };
        if state.get(&k) == v.as_ref() {
            continue;
        }
        changes.insert(k, v);
    }
    let w = ubos_tree::update(&mut nodes, &root, &changes).map_err(err)?;
    nodes.extend(w.nodes);
    let mut counting = Counting {
        inner: &mut nodes,
        reads: 0,
    };
    let d = ubos_tree::diff(&mut counting, &root, &w.root).map_err(err)?;
    let reads = counting.reads;
    let c = changes.len() as u64;
    let bound = 2 * (height + 1) * (3 * c + 1);
    DIFF_COST[0].fetch_max(reads, Ordering::Relaxed);
    DIFF_COST[1].fetch_max(reads * 1000 / bound.max(1), Ordering::Relaxed);
    if reads > bound {
        return Err(format!(
            "diff read {reads} nodes for {c} changes in a tree of {size} keys and height {height}; bound {bound}"
        ));
    }
    let want: Vec<String> = changes.keys().cloned().collect();
    let got: Vec<String> = d.into_iter().map(|x| x.0).collect();
    if got != want {
        return Err(format!("diff lists {} keys, expected {}", got.len(), c));
    }
    Ok(())
}

/// Merge ledger completeness (DSN-DVM-242, STD-VER-052, STD-KIND-022): after merging a
/// branch, the target's ledger holds its own entries followed by every entry of the branch
/// in branch order, with dense sequences, the same bodies, and the sum of the quantities.
fn prop_merge_ledger(rng: &mut Rng) -> Result<(), String> {
    let mut a = Adapter::new().map_err(|e| e.to_string())?;
    genesis(&mut a)?;
    let items = ["bolt", "nut", "gear"];
    let mut eid = 0;
    let mut model: BTreeMap<&str, Vec<(String, i64)>> = BTreeMap::new();
    let mut append = |a: &mut Adapter,
                      rng: &mut Rng,
                      branch: &str,
                      model: &mut BTreeMap<&str, Vec<(String, i64)>>,
                      n: u64|
     -> Result<(), String> {
        for _ in 0..n {
            let mut changes = Vec::new();
            let mut added = Vec::new();
            for _ in 0..1 + rng.below(3) {
                eid += 1;
                let item = items[rng.below(3) as usize];
                let q = rng.below(40) as i64 - 10;
                changes.push(stock(eid, item, q, &date(rng.below(20))));
                added.push((item, uid(eid), q));
            }
            run(a, "commit", json!({"branch": branch, "changes": changes}))?;
            for (item, id, q) in added {
                model.entry(item).or_default().push((id, q));
            }
        }
        Ok(())
    };
    let n0 = rng.below(4);
    append(&mut a, rng, "main", &mut model, n0)?;
    run(
        &mut a,
        "create_branch",
        json!({"name": "feature/l", "from": "main"}),
    )?;
    let mut branch_model: BTreeMap<&str, Vec<(String, i64)>> = BTreeMap::new();
    let n1 = 1 + rng.below(5);
    append(&mut a, rng, "feature/l", &mut branch_model, n1)?;
    let n2 = rng.below(3);
    append(&mut a, rng, "main", &mut model, n2)?;
    run(
        &mut a,
        "merge",
        json!({"source": "feature/l", "target": "main"}),
    )?;
    for item in items {
        let mut want = model.remove(item).unwrap_or_default();
        want.extend(branch_model.remove(item).unwrap_or_default());
        let r = run(
            &mut a,
            "read_ledger",
            json!({"branch": "main", "class": "StockMove", "ledger_key": {"item": item}}),
        )?;
        let got = r["entries"].as_array().cloned().unwrap_or_default();
        if got.len() != want.len() {
            return Err(format!(
                "{item}: {} entries after the merge, expected {}",
                got.len(),
                want.len()
            ));
        }
        for (i, (e, (id, q))) in got.iter().zip(&want).enumerate() {
            if e["seq"].as_u64() != Some(i as u64 + 1) {
                return Err(format!("{item}: seq {} at position {i}", e["seq"]));
            }
            if e["entry_id"].as_str() != Some(id.as_str()) || qty(e) != *q {
                return Err(format!(
                    "{item}: entry {} ({}) at position {i}, expected {id} ({q})",
                    e["entry_id"],
                    qty(e)
                ));
            }
        }
        let sum: i64 = got.iter().map(qty).sum();
        let want_sum: i64 = want.iter().map(|x| x.1).sum();
        if sum != want_sum {
            return Err(format!(
                "{item}: balance {sum} after the merge, expected {want_sum}"
            ));
        }
    }
    Ok(())
}

/// Bitemporal balance (DSN-DVM-420, DSN-DVM-421, STD-TIME-020): the balance of a ledger key
/// read at any `(asof, cseq)` equals the sum of the entries with `valid_at ≤ asof` committed
/// at or before `cseq`, backdated entries included; the entries are listed in sequence order.
fn prop_balance(rng: &mut Rng) -> Result<(), String> {
    let mut a = Adapter::new().map_err(|e| e.to_string())?;
    genesis(&mut a)?;
    let mut model: Vec<(u64, String, i64)> = Vec::new();
    let mut eid = 0;
    let n = 1 + rng.below(10);
    for _ in 0..n {
        let mut changes = Vec::new();
        let mut added = Vec::new();
        for _ in 0..1 + rng.below(3) {
            eid += 1;
            let at = date(rng.below(20));
            let q = rng.below(1000) as i64 - 300;
            changes.push(stock(eid, "bolt", q, &at));
            added.push((at, q));
        }
        if rng.chance(30) {
            eid += 1;
            changes.push(stock(eid, "nut", 1, &date(rng.below(20))));
        }
        let r = run(
            &mut a,
            "commit",
            json!({"branch": "main", "changes": changes}),
        )?;
        let cseq = r["cseq"].as_u64().unwrap_or_default();
        for (at, q) in added {
            model.push((cseq, at, q));
        }
    }
    let head = model.last().map(|m| m.0).unwrap_or_default();
    for _ in 0..12 {
        let asof = date(rng.below(20));
        let time = 1 + rng.below(head);
        let input = if rng.chance(20) {
            json!({"branch": "main", "class": "StockMove", "ledger_key": {"item": "bolt"}, "asof": asof})
        } else {
            json!({"branch": "main", "class": "StockMove", "ledger_key": {"item": "bolt"}, "asof": asof, "time": time})
        };
        let time = if input.get("time").is_some() {
            time
        } else {
            head
        };
        let r = run(&mut a, "read_ledger", input)?;
        let got = r["entries"].as_array().cloned().unwrap_or_default();
        let want: Vec<&(u64, String, i64)> = model
            .iter()
            .filter(|(c, at, _)| *c <= time && at.as_str() <= asof.as_str())
            .collect();
        let sum: i64 = got.iter().map(qty).sum();
        let want_sum: i64 = want.iter().map(|x| x.2).sum();
        if got.len() != want.len() || sum != want_sum {
            return Err(format!(
                "balance at asof {asof} cseq {time}: {} entries summing to {sum}, expected {} summing to {want_sum}",
                got.len(),
                want.len()
            ));
        }
        let seqs: Vec<u64> = got.iter().filter_map(|e| e["seq"].as_u64()).collect();
        if seqs.windows(2).any(|w| w[0] >= w[1]) {
            return Err(format!(
                "entries out of sequence order at asof {asof} cseq {time}: {seqs:?}"
            ));
        }
    }
    Ok(())
}

/// Cursor completeness (DSN-DVM-908, FR-QRY-071, FR-QRY-072): paging a query with its cursor
/// while commits land between the pages returns every object of the first page's snapshot
/// exactly once, in the query order with ties broken by object id.
fn prop_cursor(rng: &mut Rng) -> Result<(), String> {
    let mut a = Adapter::new().map_err(|e| e.to_string())?;
    genesis(&mut a)?;
    let mut model: BTreeMap<u64, String> = BTreeMap::new();
    let mut heads: BTreeMap<u64, String> = BTreeMap::new();
    let mut next = 0;
    let mut create = |a: &mut Adapter,
                      model: &mut BTreeMap<u64, String>,
                      heads: &mut BTreeMap<u64, String>,
                      n: u64,
                      rng: &mut Rng|
     -> Result<(), String> {
        let mut changes = Vec::new();
        let mut added = Vec::new();
        for _ in 0..n {
            let v = format!("k{}", rng.below(6));
            changes.push(json!({"op": "create", "object_id": uid(next), "class": "Item", "slug": format!("i{next}"), "body": {"a": v}}));
            added.push((next, v));
            next += 1;
        }
        if changes.is_empty() {
            return Ok(());
        }
        let r = run(a, "commit", json!({"branch": "main", "changes": changes}))?;
        for (o, v) in added {
            model.insert(o, v);
            heads.insert(
                o,
                r["versions"][uid(o)]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        Ok(())
    };
    let rounds = 1 + rng.below(3);
    for _ in 0..rounds {
        let n = rng.below(14);
        create(&mut a, &mut model, &mut heads, n, rng)?;
    }
    let desc = rng.chance(50);
    let limit = 1 + rng.below(7);
    let mut want: Vec<(String, u64)> = model.iter().map(|(o, v)| (v.clone(), *o)).collect();
    want.sort_by(|x, y| {
        let o = x.0.cmp(&y.0);
        (if desc { o.reverse() } else { o }).then(uid(x.1).cmp(&uid(y.1)))
    });
    let want: Vec<String> = want.into_iter().map(|x| uid(x.1)).collect();
    let spec = json!({"branch": "main", "from": "Item", "order_by": [{"path": "this.a", "dir": if desc { "desc" } else { "asc" }}], "limit": limit});
    let mut got: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let mut input = spec.clone();
        if let Some(c) = &cursor {
            input["cursor"] = json!(c);
        }
        let r = run(&mut a, "query", input)?;
        let page = r["objects"].as_array().cloned().unwrap_or_default();
        if page.len() as u64 > limit {
            return Err(format!(
                "page of {} objects over the limit {limit}",
                page.len()
            ));
        }
        got.extend(page.iter().filter_map(|v| v.as_str().map(str::to_string)));
        pages += 1;
        match r.get("cursor").and_then(Value::as_str) {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
        if pages > 200 {
            return Err("paging does not end".into());
        }
        // Concurrent commits between the pages: creates, updates of the ordering field, ends.
        match rng.below(3) {
            0 => {
                let n = 1 + rng.below(3);
                create(&mut a, &mut model, &mut heads, n, rng)?
            }
            1 if !heads.is_empty() => {
                let o = *heads
                    .keys()
                    .nth(rng.below(heads.len() as u64) as usize)
                    .expect("key");
                let base = heads[&o].clone();
                let r = run(
                    &mut a,
                    "commit",
                    json!({"branch": "main", "changes": [{"op": "update", "object_id": uid(o), "base": base, "body": {"a": format!("k{}", rng.below(6))}}]}),
                )?;
                heads.insert(
                    o,
                    r["versions"][uid(o)]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            _ if !heads.is_empty() => {
                let o = *heads
                    .keys()
                    .nth(rng.below(heads.len() as u64) as usize)
                    .expect("key");
                let base = heads.remove(&o).expect("head");
                run(
                    &mut a,
                    "commit",
                    json!({"branch": "main", "changes": [{"op": "end", "object_id": uid(o), "base": base, "valid": {"from": "2026-01-15"}}]}),
                )?;
            }
            _ => {}
        }
    }
    if got != want {
        return Err(format!(
            "pages return {} objects {:?}, expected {} {:?}",
            got.len(),
            got,
            want.len(),
            want
        ));
    }
    Ok(())
}

// ---- VER-PROP-0100: process state machine (PH-1 step 12c, DEC-071) ----

/// The transition table of UBS-SYS-DVM-05 §1 as the property states it, independently of
/// `ubos_dvm::process::step`.
const PROCESS_TABLE: &[(Phase, Event, Phase)] = &[
    (Phase::Created, Event::FirstInstruction, Phase::Running),
    (Phase::Running, Event::CommitEmpty, Phase::Committed),
    (Phase::Running, Event::CommitStaged, Phase::Staged),
    (Phase::Staged, Event::ValidationFailed, Phase::Aborted),
    (Phase::Staged, Event::Requires, Phase::Deferred),
    (Phase::Staged, Event::Enqueued, Phase::Flushing),
    (Phase::Flushing, Event::Flushed, Phase::Committed),
    (
        Phase::Flushing,
        Event::Conflict { retry: true },
        Phase::Running,
    ),
    (
        Phase::Flushing,
        Event::Conflict { retry: false },
        Phase::Aborted,
    ),
    (Phase::Running, Event::Fault, Phase::Aborted),
    (Phase::Staged, Event::Fault, Phase::Aborted),
    (Phase::Flushing, Event::StorageFailure, Phase::Aborted),
];
const PHASES: &[Phase] = &[
    Phase::Created,
    Phase::Running,
    Phase::Staged,
    Phase::Flushing,
    Phase::Committed,
    Phase::Aborted,
    Phase::Deferred,
];
const EVENTS: &[Event] = &[
    Event::FirstInstruction,
    Event::CommitEmpty,
    Event::CommitStaged,
    Event::ValidationFailed,
    Event::Requires,
    Event::Enqueued,
    Event::Flushed,
    Event::Conflict { retry: true },
    Event::Conflict { retry: false },
    Event::Fault,
    Event::StorageFailure,
];

fn phase_of(name: &str) -> Option<Phase> {
    PHASES.iter().copied().find(|p| p.name() == name)
}

/// A recorded phase list is a path of the table from `created` to a terminal phase.
fn check_path(names: &[Value]) -> Result<Phase, String> {
    let mut cur = Phase::Created;
    if names.first().and_then(Value::as_str) != Some("created") {
        return Err(format!("phases start with created: {names:?}"));
    }
    for n in &names[1..] {
        let to = n
            .as_str()
            .and_then(phase_of)
            .ok_or_else(|| format!("unknown phase {n}"))?;
        if !PROCESS_TABLE.iter().any(|(f, _, t)| *f == cur && *t == to) {
            return Err(format!(
                "{} → {} is not in the table ({names:?})",
                cur.name(),
                to.name()
            ));
        }
        cur = to;
    }
    if !cur.is_terminal() {
        return Err(format!(
            "the process did not end in a terminal phase ({names:?})"
        ));
    }
    Ok(cur)
}

/// Process state machine (DSN-DVM-501, UBS-SYS-DVM-05 §1): `step` lists exactly the table
/// (every other pair is illegal, terminal phases absorb); a random walk through `Machine`
/// never leaves the table; and the kernel's executions — empty, with writes, re-executed
/// after a head move, aborted after a conflict, aborted by a limit, aborted by a validation
/// failure — record a phase path of the table that ends in the terminal phase their outcome
/// implies.
fn prop_process(rng: &mut Rng) -> Result<(), String> {
    // 1. the table, both ways
    for &from in PHASES {
        for &ev in EVENTS {
            let want = PROCESS_TABLE
                .iter()
                .find(|(f, e, _)| *f == from && *e == ev)
                .map(|(_, _, t)| *t);
            let got = ubos_dvm::process::step(from, ev);
            if got != want {
                return Err(format!("step({from:?}, {ev:?}) = {got:?}, table {want:?}"));
            }
            if from.is_terminal() && want.is_some() {
                return Err(format!("terminal phase {from:?} leaves on {ev:?}"));
            }
        }
    }
    // 2. a random walk: legal events are applied, illegal ones rejected and ignored
    let mut m = ubos_dvm::process::Machine::new();
    for _ in 0..(1 + rng.below(12)) {
        let ev = EVENTS[rng.below(EVENTS.len() as u64) as usize];
        let before = m.phase;
        let legal = PROCESS_TABLE
            .iter()
            .any(|(f, e, _)| *f == before && *e == ev);
        match m.fire(ev) {
            Ok(to) if legal => {
                if !PROCESS_TABLE.contains(&(before, ev, to)) {
                    return Err(format!(
                        "walk {before:?} --{ev:?}--> {to:?} is not in the table"
                    ));
                }
            }
            Ok(to) => return Err(format!("illegal {before:?} --{ev:?}--> {to:?} applied")),
            Err(_) if !legal => {
                if m.phase != before {
                    return Err("a rejected event changed the phase".into());
                }
            }
            Err(e) => return Err(format!("legal {before:?} --{ev:?} rejected: {e}")),
        }
    }
    let names: Vec<Value> = m.phases.iter().map(|p| json!(p.name())).collect();
    if m.phase.is_terminal() {
        check_path(&names)?;
    }
    // 3. the kernel's executions
    let mut a = Adapter::new().map_err(|e| e.to_string())?;
    genesis(&mut a)?;
    run(
        &mut a,
        "create_branch",
        json!({"name": "feature/p", "from": "main"}),
    )?;
    let target = "\"ubos://acme/Item/i-1\"";
    let kinds = ["empty", "write", "retry", "conflict", "limit", "invalid"];
    let kind = kinds[rng.below(kinds.len() as u64) as usize];
    let body = match kind {
        "empty" => vec![json!({"return": "1"})],
        "invalid" => vec![
            json!({"do": "read.get", "args": {"target": target}}),
            json!({"do": "commit.update", "args": {"target": target, "set": {"a": "\"x\"", "zz": "1"}}}),
            json!({"return": "1"}),
        ],
        _ => vec![
            json!({"do": "read.get", "args": {"target": target}}),
            json!({"do": "commit.update", "args": {"target": target, "set": {"a": "\"x\""}}}),
            json!({"return": "1"}),
        ],
    };
    let program = json!({"op": "create", "class": "Logic", "object_id": uid(80), "slug": "acme-p",
        "body": {"name": "acme.p", "tier": "host", "abi": "1.0",
            "source": {"functions": {"main": {"params": [], "body": body, "retryable": kind == "retry"}}},
            "exports": ["main"], "capabilities": ["commit", "read", "run"], "profile": "standard",
            "state": "committed"}});
    let item = json!({"op": "create", "class": "Item", "object_id": uid(1), "slug": "i-1",
        "body": {"a": "0", "b": "0", "c": "0"}});
    run(
        &mut a,
        "commit",
        json!({"branch": "feature/p", "changes": [program, item]}),
    )?;
    let pid = uid(500);
    let mut input = json!({"branch": "feature/p", "asset": "acme.p", "function": "main", "args": [],
        "process": {"process_id": pid}, "entropy_seed": "11".repeat(32)});
    if kind == "retry" || kind == "conflict" {
        // a commit of another process between the read and the commit moves the head
        let head = run(
            &mut a,
            "read",
            json!({"branch": "feature/p", "object_id": uid(1)}),
        )?["version_hash"]
            .clone();
        input["interfere"] = json!([{"attempt": 1, "changes": [
            {"op": "update", "object_id": uid(1), "base": head, "body": {"c": "9"}}]}]);
    }
    if kind == "limit" {
        input["limits"] = json!({"units": 3});
    }
    let r = run(&mut a, "execute", input);
    let info = run(&mut a, "process_info", json!({"process_id": pid}))?;
    let phases = info["phases"].as_array().cloned().unwrap_or_default();
    let end = check_path(&phases)?;
    let (want_end, want_ok, want_attempts): (Phase, bool, u64) = match kind {
        "empty" | "write" => (Phase::Committed, true, 1),
        "retry" => (Phase::Committed, true, 2),
        _ => (Phase::Aborted, false, 1),
    };
    if end != want_end || r.is_ok() != want_ok {
        return Err(format!(
            "{kind}: ended {end:?} (ok {}), wanted {want_end:?} (ok {want_ok}): {phases:?} {r:?}",
            r.is_ok()
        ));
    }
    if info["attempts"].as_u64() != Some(want_attempts) {
        return Err(format!(
            "{kind}: attempts {}, wanted {want_attempts}",
            info["attempts"]
        ));
    }
    let staged = phases.iter().any(|p| p == "staged");
    if staged == (kind == "empty" || kind == "limit") {
        return Err(format!("{kind}: staged phase {staged} ({phases:?})"));
    }
    let state = info["state"].as_str().unwrap_or_default();
    if (end == Phase::Committed) != (state == "committed") {
        return Err(format!("{kind}: recorded state {state} with phase {end:?}"));
    }
    Ok(())
}

// ---- driver ----

const PROPERTIES: &[(&str, &str, u64)] = &[
    ("root_history_independence", "VER-PROP-0001", 1),
    ("timeline_non_overlap", "VER-PROP-0002", 2),
    ("merge_determinism", "VER-PROP-0003", 3),
    ("tree_incremental", "VER-PROP-0004", 4),
    ("index_rebuild_equality", "VER-PROP-0100", 5),
    ("diff_cost_bound", "VER-PROP-0100", 6),
    ("merge_ledger_completeness", "VER-PROP-0100", 7),
    ("balance_bitemporal", "VER-PROP-0100", 8),
    ("cursor_completeness", "VER-PROP-0100", 9),
    ("process_state_machine", "VER-PROP-0100", 10),
];

fn check(name: &str, id: u64, seed: u64, case: u64) -> Result<(), String> {
    match name {
        "merge_determinism" => prop_merge(seed, case),
        _ => {
            let mut rng = Rng::new(seed, id, case);
            match name {
                "root_history_independence" => prop_roots(&mut rng),
                "tree_incremental" => prop_tree(&mut rng),
                "index_rebuild_equality" => prop_index(&mut rng),
                "diff_cost_bound" => prop_diff_cost(&mut rng),
                "merge_ledger_completeness" => prop_merge_ledger(&mut rng),
                "balance_bitemporal" => prop_balance(&mut rng),
                "cursor_completeness" => prop_cursor(&mut rng),
                "process_state_machine" => prop_process(&mut rng),
                _ => prop_timeline(&mut rng),
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let cases: u64 = opt("--cases")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let seed: u64 = opt("--seed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_260_930);
    let threads: u64 = opt("--threads").and_then(|v| v.parse().ok()).unwrap_or(4);
    let only = opt("--only");
    let mut results = Vec::new();
    let mut failed = false;
    for &(name, ver, id) in PROPERTIES {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let start = Instant::now();
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                std::thread::spawn(move || {
                    let mut failures = Vec::new();
                    let mut case = t;
                    while case < cases {
                        if let Err(e) = check(name, id, seed, case) {
                            failures.push(json!({"case": case, "error": e}));
                            if failures.len() >= 5 {
                                break;
                            }
                        }
                        case += threads;
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
        println!(
            "{ver} {name}: {cases} cases, {} failures, {secs:.1} s",
            failures.len()
        );
        failed |= !failures.is_empty();
        let mut r = json!({"item": ver, "property": name, "cases": cases, "seed": seed, "failures": failures,
                           "seconds": (secs * 10.0).round() / 10.0});
        if name == "merge_determinism" {
            let n = |i: usize| MERGE_OUTCOMES[i].load(Ordering::Relaxed);
            r["outcomes"] =
                json!({"conflict": n(0), "merged_with_changes": n(1), "merged_unchanged": n(2)});
            println!(
                "  outcomes: conflict {}, merged with changes {}, merged unchanged {}",
                n(0),
                n(1),
                n(2)
            );
        }
        if name == "diff_cost_bound" {
            let (reads, permille) = (
                DIFF_COST[0].load(Ordering::Relaxed),
                DIFF_COST[1].load(Ordering::Relaxed),
            );
            r["cost"] = json!({"max_node_reads": reads, "max_reads_per_bound_permille": permille});
            println!(
                "  cost: at most {reads} node reads; at most {permille}‰ of the bound 2·(h+1)·(3·c+1)"
            );
        }
        results.push(r);
    }
    if let Some(path) = opt("--report") {
        let report = json!({"suite": "SUITE-DVM-PROP", "generator": "SplitMix64", "threads": threads, "results": results});
        std::fs::write(
            path,
            serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
        )
        .expect("write report");
    }
    std::process::exit(if failed { 1 } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every property holds on a small sample (the test body of the mutation score,
    /// VER-INSP-0001); the million-case runs are `ubos-props --cases 1000000`.
    #[test]
    fn properties_hold_on_a_sample() {
        for &(name, _, id) in PROPERTIES {
            let cases = if name == "merge_determinism" { 8 } else { 25 };
            for case in 0..cases {
                if let Err(e) = check(name, id, 20_260_930, case) {
                    panic!("{name} case {case}: {e}");
                }
            }
        }
    }
}
