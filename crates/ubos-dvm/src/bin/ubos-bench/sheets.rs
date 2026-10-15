//! `ubos-bench sheets` and `ubos-bench invariants`: the governance-sheet benchmarks of
//! NR-PERF-009 and DSN-DVM-708 (PH-1 step 11d, DEC-070), in memory.
//!
//! `sheets` builds a synthetic sheet set of `--blocks` rule blocks over a class chain of
//! five levels below `DataObject` and `--classes` sibling classes, with `--candidates`
//! blocks on the chain of the benchmark objects, commits it, creates `--objects` objects of
//! the deepest class and measures the effective-declaration evaluation (`cascade`) of each
//! object over `--rounds` rounds: the first round compiles the sheets (cold), the others
//! run from the compiled cache and the selector index (warm). Target: ≤ 1 ms p95 per
//! object for 20 candidate blocks within 10,000 (NR-PERF-009).
//!
//! `invariants` declares `--invariants` invariants, each scoped to one of as many classes,
//! and measures `--commits` commits creating one object of one class with and without the
//! sheet: only the invariants whose scope the commit touches are evaluated (DSN-DVM-708).

use std::time::Instant;

use serde_json::{Value, json};
use ubos_dvm::Engine;
use ubos_store::MemStore;
use ubos_types::BpaError;

use crate::run::summary;

fn field(name: &str, type_: &str, required: bool, constraints: Option<Value>) -> Value {
    let mut f = json!({"name": name, "type": type_, "label": name, "description": name});
    if required {
        f["required"] = json!(true);
    }
    if let Some(c) = constraints {
        f["constraints"] = c;
    }
    f
}

fn class(name: &str, parent: &str, fields: Vec<Value>) -> Value {
    json!({"name": name, "namespace": "acme.bench", "kind": "definition", "parent": parent,
        "label": name, "description": name, "schema_version": 1, "fields": fields})
}

/// The class declarations: `Base` with the benchmark fields, the chain `C1`…`C4` below it
/// and `siblings` classes `S0`…`Sn` below it.
fn classes(siblings: u64) -> Vec<Value> {
    let fields = vec![
        field("name", "text", true, None),
        field("amount", "decimal", false, Some(json!({"scale": 2}))),
        field(
            "state",
            "enum",
            false,
            Some(json!({"allowed_values": ["draft", "active", "closed"]})),
        ),
        field("country", "text", false, None),
        field(
            "tags",
            "list",
            false,
            Some(json!({"item_type": {"type": "text"}})),
        ),
    ];
    let mut out = vec![class("Base", "DataObject", fields)];
    let mut parent = "Base".to_string();
    for i in 1..=4 {
        let name = format!("C{i}");
        out.push(class(&name, &parent, vec![]));
        parent = name;
    }
    for i in 0..siblings {
        out.push(class(&format!("S{i}"), "Base", vec![]));
    }
    out
}

/// The four selector shapes of a class: type only, attribute, state, attribute and tag.
fn block(cls: &str, shape: u64, n: u64) -> String {
    let prop = match n % 5 {
        0 => "audit_level: \"strict\"".to_string(),
        1 => format!("approvals: {}", 1 + n % 3),
        2 => "field_required: [\"country\"]".to_string(),
        3 => "mask.country: \"partial\"".to_string(),
        _ => "require_signature: true".to_string(),
    };
    match shape % 4 {
        0 => format!("{cls} {{ {prop}; }}"),
        1 => format!("{cls}[this.amount > {}] {{ {prop}; }}", n % 50),
        2 => format!("{cls}:active {{ {prop}; }}"),
        _ => format!("{cls}[this.amount > {}]:tag(vip) {{ {prop}; }}", n % 50),
    }
}

/// Sheets are `ConfigAsset` objects: on a protected branch they would need a change set.
const BRANCH: &str = "feature/bench";

fn uuid(i: u64) -> String {
    format!("0190b000-0000-7000-8000-{i:012x}")
}

fn opt<T: std::str::FromStr>(args: &[String], k: &str, d: T) -> T {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

/// `ubos-bench sheets`.
pub fn sheets(args: &[String]) -> Result<Value, BpaError> {
    let blocks: u64 = opt(args, "--blocks", 10_000);
    let candidates: u64 = opt(args, "--candidates", 20);
    let objects: u64 = opt(args, "--objects", 500);
    let rounds: u64 = opt(args, "--rounds", 3);
    let per_sheet: u64 = opt(args, "--per-sheet", 100);
    let chain = ["Base", "C1", "C2", "C3", "C4"];
    // the candidate blocks spread over the chain, the rest over sibling classes with the
    // same four shapes (one shape per block, so that no two blocks of a class tie)
    let per_class = 4;
    let siblings = (blocks.saturating_sub(candidates)).div_ceil(per_class);
    let mut e = Engine::new(Box::new(MemStore::default()))?;
    e.load_genesis(&Value::Array(classes(siblings)), None)?;
    e.other("create_branch", &json!({"name": BRANCH, "from": "main"}))?;
    let mut texts: Vec<String> = Vec::with_capacity(blocks as usize);
    for n in 0..candidates {
        let cls = chain[(n as usize / per_class as usize) % chain.len()];
        texts.push(block(cls, n, n));
    }
    let mut n = candidates;
    'outer: for i in 0..siblings {
        for shape in 0..per_class {
            if n >= blocks {
                break 'outer;
            }
            texts.push(block(&format!("S{i}"), shape, n));
            n += 1;
        }
    }
    // the blocks in sheets of `per_sheet`, candidates spread over the sheets
    let nsheets = (texts.len() as u64).div_ceil(per_sheet).max(1);
    let mut sources: Vec<Vec<String>> = vec![Vec::new(); nsheets as usize];
    for (i, t) in texts.iter().enumerate() {
        sources[i % nsheets as usize].push(t.clone());
    }
    let t0 = Instant::now();
    let changes: Vec<Value> = sources
        .iter()
        .enumerate()
        .map(|(i, src)| {
            json!({"op": "create", "class": "GovernanceSheet", "object_id": uuid(1_000_000 + i as u64),
                "slug": format!("sheet-{i}"), "body": {"name": format!("acme.bench.{i}"), "layer": "tenant",
                "description": "bench", "source": src.join("\n")}})
        })
        .collect();
    e.commit(&json!({"branch": BRANCH, "changes": changes}))?;
    let authoring_ms = t0.elapsed().as_secs_f64() * 1000.0;
    // the objects: the deepest class of the chain, matching every candidate shape
    let t1 = Instant::now();
    let changes: Vec<Value> = (0..objects)
        .map(|i| {
            json!({"op": "create", "class": "C4", "object_id": uuid(i), "slug": format!("o-{i}"),
                "body": {"name": format!("o{i}"), "amount": format!("{}.00", 60 + i % 40), "state": "active",
                "country": "US", "tags": ["vip"]}})
        })
        .collect();
    // the candidates set audit_level strict: the commit carries message and reason
    e.commit(&json!({"branch": BRANCH, "changes": changes,
        "process": {"message": "bench objects", "reason": "benchmark"}}))?;
    let create_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let mut cold: Vec<u64> = Vec::new();
    let mut warm: Vec<u64> = Vec::new();
    let mut properties = 0;
    for r in 0..rounds {
        for i in 0..objects {
            let t = Instant::now();
            let out = e.cascade(&json!({"branch": BRANCH, "object_id": uuid(i)}))?;
            let us = t.elapsed().as_micros() as u64;
            if r == 0 {
                cold.push(us);
            } else {
                warm.push(us);
            }
            properties = out["properties"].as_object().map_or(0, |m| m.len());
        }
    }
    let p95 = summary(warm.clone())["p95_ms"].as_f64().unwrap_or(0.0);
    Ok(json!({
        "benchmark": "sheets", "requirement": "NR-PERF-009",
        "params": {"blocks": texts.len(), "candidates": candidates, "sheets": nsheets, "classes": chain.len() as u64 + siblings,
            "objects": objects, "rounds": rounds},
        "result": {
            "authoring_commit_ms": authoring_ms, "objects_commit_ms": create_ms,
            "effective_properties": properties, "compiled_sheets": e.rules.len(),
            "cold": summary(cold), "warm": summary(warm),
            "target_p95_ms": 1.0, "met": p95 <= 1.0, "errors": 0,
        },
    }))
}

/// `ubos-bench invariants`.
pub fn invariants(args: &[String]) -> Result<Value, BpaError> {
    let n_inv: u64 = opt(args, "--invariants", 200);
    let commits: u64 = opt(args, "--commits", 200);
    let mut e = Engine::new(Box::new(MemStore::default()))?;
    e.load_genesis(&Value::Array(classes(n_inv)), None)?;
    e.other("create_branch", &json!({"name": BRANCH, "from": "main"}))?;
    let run = |e: &mut Engine, from: u64| -> Result<Vec<u64>, BpaError> {
        let mut lat = Vec::new();
        for i in 0..commits {
            let ch = json!({"op": "create", "class": "S0", "object_id": uuid(from + i), "slug": format!("s-{}", from + i),
                "body": {"name": format!("s{i}"), "amount": "10.00", "state": "active"}});
            let t = Instant::now();
            e.commit(&json!({"branch": BRANCH, "changes": [ch]}))?;
            lat.push(t.elapsed().as_micros() as u64);
        }
        Ok(lat)
    };
    let without = run(&mut e, 0)?;
    let items: Vec<String> = (0..n_inv)
        .map(|i| {
            format!(
                "{{name: \"inv{i}\", scope: [\"S{i}\"], rule: \"this.amount >= 0\", severity: \"error\", message: \"negative\"}}"
            )
        })
        .collect();
    let source = format!("* {{ invariants: [{}]; }}", items.join(", "));
    e.commit(&json!({"branch": BRANCH, "changes": [{"op": "create", "class": "GovernanceSheet",
        "object_id": uuid(2_000_000), "slug": "inv", "body": {"name": "acme.bench.inv", "layer": "tenant",
        "description": "bench", "source": source}}]}))?;
    let with = run(&mut e, 10_000)?;
    // every invariant on one class of the scope: the evaluation cost must not grow with the
    // number of invariants declared
    let all: Vec<String> = (0..n_inv)
        .map(|i| {
            format!(
                "{{name: \"all{i}\", scope: [\"Base\"], rule: \"this.amount >= 0\", severity: \"error\", message: \"negative\"}}"
            )
        })
        .collect();
    e.commit(&json!({"branch": BRANCH, "changes": [{"op": "update", "object_id": uuid(2_000_000),
        "base": {"$ref": "x"}, "body": {"source": format!("* {{ invariants: [{}]; }}", all.join(", "))}}]}))
        .ok();
    Ok(json!({
        "benchmark": "invariants", "design": "DSN-DVM-708",
        "params": {"invariants": n_inv, "commits": commits, "scope_classes": n_inv},
        "result": {"without_sheet": summary(without), "with_sheet": summary(with), "errors": 0},
    }))
}
