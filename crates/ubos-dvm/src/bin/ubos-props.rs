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

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Merge outcomes seen: conflicts, merges that changed the target, merges without change.
static MERGE_OUTCOMES: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
use ubos_dvm::Adapter;
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

// ---- driver ----

const PROPERTIES: &[(&str, &str, u64)] = &[
    ("root_history_independence", "VER-PROP-0001", 1),
    ("timeline_non_overlap", "VER-PROP-0002", 2),
    ("merge_determinism", "VER-PROP-0003", 3),
    ("tree_incremental", "VER-PROP-0004", 4),
];

fn check(name: &str, id: u64, seed: u64, case: u64) -> Result<(), String> {
    match name {
        "merge_determinism" => prop_merge(seed, case),
        _ => {
            let mut rng = Rng::new(seed, id, case);
            if name == "root_history_independence" {
                prop_roots(&mut rng)
            } else if name == "tree_incremental" {
                prop_tree(&mut rng)
            } else {
                prop_timeline(&mut rng)
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
