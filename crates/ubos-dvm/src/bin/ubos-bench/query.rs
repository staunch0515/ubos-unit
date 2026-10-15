//! `ubos-bench query`: the indexed-query benchmark of NR-PERF-008 / MET-PERF-016 (PH-1
//! step 13d, DEC-073), in memory or on PostgreSQL.
//!
//! The dataset is one class `Account` with the indexed fields `country` (100 values),
//! `balance` (a decimal in 0…9,999.99) and the unique key `number`, plus two unindexed
//! fields, created in commits of `--per-commit` objects. The suite runs `--rounds` rounds
//! of each query with a parameter drawn per round and measures the latency of the first
//! page (`limit 100`) of:
//!
//! - `eq`: an equality on `country` (1% of the objects), ordered by `country` — the
//!   order-by pushdown reads `offset + limit` index rows and stops;
//! - `range`: a range on `balance` (1% of the objects), ordered by `balance`;
//! - `eq_filter`: the equality with a principal and a policy set on the branch: the
//!   residual row filter of DSN-DVM-820 (the policy denies one country in ten);
//! - `page2`: the second page of `eq` through its cursor;
//! - `key`: a lookup by the unique key.
//!
//! Target: p95 ≤ 100 ms for the first 100 rows over 10,000,000 objects on ENV-REF-SERVER
//! (NR-PERF-008); the workstation runs smaller datasets and the report extrapolates.

use std::time::Instant;

use serde_json::{Value, json};
use ubos_dvm::Engine;
use ubos_store::{MemStore, PgStore};
use ubos_types::BpaError;

use crate::run::summary;

const BRANCH: &str = "feature/bench";
const PRINCIPAL: &str = "did:ubos:acme:01900000-0000-7000-8000-000000000002";

fn opt<T: std::str::FromStr>(args: &[String], k: &str, d: T) -> T {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn text(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn uuid(i: u64) -> String {
    format!("01890a5d-ac96-7000-8000-{i:012x}")
}

fn classes() -> Value {
    let f = |n: &str, t: &str, required: bool| json!({"name": n, "type": t, "label": n, "description": n, "required": required});
    json!([{
        "name": "Account", "namespace": "bench.query", "kind": "definition", "label": "Account",
        "description": "Account", "parent": "DataObject", "schema_version": 1,
        "fields": [f("number", "text", true), f("name", "text", true), f("country", "text", true),
                   f("balance", "decimal", true), f("region", "text", false), f("note", "text", false)],
        "keys": [["number"]],
    }])
}

/// A deterministic pseudo-random stream (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// `ubos-bench query`.
pub fn query(args: &[String], url: &str) -> Result<Value, BpaError> {
    let objects: u64 = opt(args, "--objects", 100_000);
    let per_commit: u64 = opt(args, "--per-commit", 5_000);
    let rounds: u64 = opt(args, "--rounds", 50);
    let backend = text(args, "--backend").unwrap_or_else(|| "memory".into());
    let seed: u64 = opt(args, "--seed", 20_261_015);
    // `--reuse`: the dataset a previous PostgreSQL run loaded (same `--objects`, `--seed`)
    let reuse = args.iter().any(|a| a == "--reuse") && backend == "postgres";
    if reuse {
        let mut e = Engine::open_existing(Box::new(PgStore::connect(url)?.with_fixed_scope()))?;
        return suite(&mut e, objects, per_commit, rounds, seed, &backend, 0.0);
    }
    let mut e = match backend.as_str() {
        "postgres" => Engine::new(Box::new(PgStore::connect(url)?.with_fixed_scope()))?,
        _ => Engine::new(Box::new(MemStore::default()))?,
    };
    e.load_genesis(&classes(), None)?;
    e.declare_index("Account", "country")?;
    e.declare_index("Account", "balance")?;
    e.other("create_branch", &json!({"name": BRANCH, "from": "main"}))?;
    // the policy set of `eq_filter`: one country in ten is not readable
    e.commit(&json!({"branch": BRANCH, "changes": [{"op": "create", "class": "Policy",
        "object_id": uuid(0xffff_0001), "slug": "readable", "body": {"name": "readable", "description": "bench",
        "source": "permit(principal, action, resource) when { !(resource is UBOS::Model::Account) || (resource has country && !(resource.country like \"C*0\")) };"}}]}))?;
    let mut rng = Rng(seed | 1);
    let t0 = Instant::now();
    let mut i = 0u64;
    while i < objects {
        let n = per_commit.min(objects - i);
        let changes: Vec<Value> = (i..i + n)
            .map(|k| {
                let r = rng.next();
                json!({"op": "create", "class": "Account", "object_id": uuid(k), "slug": format!("a-{k}"),
                    "body": {"number": format!("A-{k:09}"), "name": format!("Account {k}"),
                        "country": format!("C{:02}", r % 100), "balance": format!("{}.{:02}", (r >> 8) % 10_000, (r >> 24) % 100),
                        "region": format!("R{}", (r >> 32) % 7), "note": "bench"}})
            })
            .collect();
        e.commit(&json!({"branch": BRANCH, "changes": changes}))?;
        i += n;
    }
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    suite(&mut e, objects, per_commit, rounds, seed, &backend, load_ms)
}

/// The query suite over a loaded dataset.
fn suite(
    e: &mut Engine,
    objects: u64,
    per_commit: u64,
    rounds: u64,
    seed: u64,
    backend: &str,
    load_ms: f64,
) -> Result<Value, BpaError> {
    let mut lat: std::collections::BTreeMap<&str, Vec<u64>> = Default::default();
    let mut rows: std::collections::BTreeMap<&str, u64> = Default::default();
    let mut errors = 0u64;
    let mut last_error: std::collections::BTreeMap<&str, String> = Default::default();
    let mut run = |name: &'static str,
                   input: Value,
                   lat: &mut std::collections::BTreeMap<&str, Vec<u64>>|
     -> Result<Value, BpaError> {
        let t = Instant::now();
        let r = e.query(&input);
        let us = t.elapsed().as_micros() as u64;
        lat.entry(name).or_default().push(us);
        if let Err(e) = &r {
            last_error.insert(name, format!("{}: {} {:?}", e.code, e.message, e.details));
        }
        r
    };
    for r in 0..rounds {
        let c = format!("C{:02}", (seed + r * 7) % 100);
        let lo = ((seed + r * 13) % 99) * 100;
        let q = |text: String| json!({"branch": BRANCH, "text": text, "profile": "standard"});
        let eq = q(format!(
            "from Account where this.country == \"{c}\" order by this.country limit 100"
        ));
        match run("eq", eq.clone(), &mut lat) {
            Ok(v) => {
                *rows.entry("eq").or_default() +=
                    v["objects"].as_array().map_or(0, Vec::len) as u64;
                if let Some(cur) = v.get("cursor").and_then(Value::as_str) {
                    let mut p2 = eq.clone();
                    p2["cursor"] = json!(cur);
                    match run("page2", p2, &mut lat) {
                        Ok(v) => {
                            *rows.entry("page2").or_default() +=
                                v["objects"].as_array().map_or(0, Vec::len) as u64
                        }
                        Err(_) => errors += 1,
                    }
                }
            }
            Err(_) => errors += 1,
        }
        let range = q(format!(
            "from Account where this.balance >= {lo}.00 and this.balance < {}.00 order by this.balance limit 100",
            lo + 100
        ));
        match run("range", range, &mut lat) {
            Ok(v) => {
                *rows.entry("range").or_default() +=
                    v["objects"].as_array().map_or(0, Vec::len) as u64
            }
            Err(_) => errors += 1,
        }
        let mut f = q(format!(
            "from Account where this.country == \"{c}\" order by this.country limit 100"
        ));
        f["principal"] = json!(PRINCIPAL);
        match run("eq_filter", f, &mut lat) {
            Ok(v) => {
                *rows.entry("eq_filter").or_default() +=
                    v["objects"].as_array().map_or(0, Vec::len) as u64
            }
            Err(_) => errors += 1,
        }
        let k = (seed + r * 101) % objects;
        let key = q(format!(
            "from Account where this.number == \"A-{k:09}\" limit 100"
        ));
        match run("key", key, &mut lat) {
            Ok(v) => {
                *rows.entry("key").or_default() +=
                    v["objects"].as_array().map_or(0, Vec::len) as u64
            }
            Err(_) => errors += 1,
        }
    }
    let mut result = json!({"load_ms": load_ms, "errors": errors, "target_p95_ms": 100.0});
    let mut met = true;
    for (name, l) in lat {
        let s = summary(l);
        met &= s["p95_ms"].as_f64().unwrap_or(f64::MAX) <= 100.0;
        result[name] = json!({"latency": s, "rows": rows.get(name).copied().unwrap_or(0)});
    }
    result["met"] = json!(met);
    if !last_error.is_empty() {
        result["last_error"] = json!(last_error);
    }
    Ok(json!({
        "benchmark": "query", "requirement": "NR-PERF-008",
        "params": {"objects": objects, "per_commit": per_commit, "rounds": rounds, "backend": backend, "seed": seed},
        "result": result,
    }))
}
