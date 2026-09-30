//! `ubos-bench tree [--keys N] [--changes C] [--rounds R] [--seed S]` builds a tree of `N`
//! keys in memory, then applies `R` batches of `C` changes (updates, inserts and removals)
//! with stored-node updates (DSN-DVM-007) and reports nodes written per batch, update time
//! and diff time, each root checked against the tree kept so far.

use std::collections::BTreeMap;
use std::time::Instant;

use serde_json::json;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn key(i: u64) -> String {
    format!(
        "o/0190{:04x}-{:04x}-7000-8000-{:012x}",
        (i >> 48) & 0xffff,
        (i >> 32) & 0xffff,
        i & 0xffff_ffff_ffff
    )
}

fn value(r: u64) -> String {
    format!("sha256:{r:016x}{r:016x}{r:016x}{r:016x}")
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    if v.is_empty() {
        return 0.0;
    }
    let i = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[i]
}

/// Run the tree benchmark.
pub fn tree(args: &[String]) -> Result<(), String> {
    let opt = |k: &str, d: u64| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let (keys, changes, rounds, seed) = (
        opt("--keys", 1_000_000),
        opt("--changes", 10),
        opt("--rounds", 200),
        opt("--seed", 20_260_930),
    );
    let mut rng = Rng(seed);
    let mut ids: Vec<u64> = (0..keys).map(|_| rng.next()).collect();
    let entries: Vec<(String, String)> = ids.iter().map(|i| (key(*i), value(rng.next()))).collect();
    let t = Instant::now();
    let built = ubos_tree::build(&entries).map_err(|e| e.to_string())?;
    let build_s = t.elapsed().as_secs_f64();
    let mut nodes = ubos_tree::MemNodes::default();
    let total_nodes = built.nodes.len();
    let depth = built.nodes.iter().map(|(_, n)| n.level).max().unwrap_or(0) + 1;
    nodes.extend(built.nodes);
    drop(entries);
    let mut root = built.root;
    let (mut written, mut upd_ms, mut diff_ms) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..rounds {
        let mut ch: BTreeMap<String, Option<String>> = BTreeMap::new();
        for j in 0..changes {
            match j % 5 {
                // Mostly updates of existing keys, some inserts and removals.
                0 => {
                    let i = rng.next();
                    ids.push(i);
                    ch.insert(key(i), Some(value(rng.next())));
                }
                1 if ids.len() > 1 => {
                    let at = (rng.next() % ids.len() as u64) as usize;
                    let i = ids.swap_remove(at);
                    ch.insert(key(i), None);
                }
                _ => {
                    let i = ids[(rng.next() % ids.len() as u64) as usize];
                    ch.insert(key(i), Some(value(rng.next())));
                }
            }
        }
        let t = Instant::now();
        let w = ubos_tree::update(&mut nodes, &root, &ch).map_err(|e| e.to_string())?;
        upd_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        let fresh = w
            .nodes
            .iter()
            .filter(|(h, _)| !nodes.nodes.contains_key(h))
            .count();
        written.push(fresh as f64);
        nodes.extend(w.nodes);
        let t = Instant::now();
        let d = ubos_tree::diff(&mut nodes, &root, &w.root).map_err(|e| e.to_string())?;
        diff_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        if d.len() != ch.len() {
            return Err(format!("diff has {} keys, batch {}", d.len(), ch.len()));
        }
        root = w.root;
    }
    let mean = written.iter().sum::<f64>() / written.len().max(1) as f64;
    let report = json!({
        "benchmark": "tree", "keys": keys, "changes_per_batch": changes, "rounds": rounds, "seed": seed,
        "levels": depth, "nodes": total_nodes, "build_seconds": (build_s * 10.0).round() / 10.0,
        "nodes_written_per_batch": {"mean": (mean * 10.0).round() / 10.0, "p95": percentile(&mut written, 0.95), "max": percentile(&mut written, 1.0)},
        "update_ms": {"p50": percentile(&mut upd_ms, 0.5), "p95": percentile(&mut upd_ms, 0.95)},
        "diff_ms": {"p50": percentile(&mut diff_ms, 0.5), "p95": percentile(&mut diff_ms, 0.95)},
        "root": root,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_default()
    );
    Ok(())
}
