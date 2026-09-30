//! Benchmarks of the PH-0 prototype (SUITE-DVM-BENCH, VER-BENCH-0001, PH-0 step 5).
//!
//! ```text
//! ubos-bench tree  [--keys N] [--changes C] [--rounds R] [--seed S]
//! ubos-bench load  --database URL [--objects N] [--months M] [--seed S] [--threads T]
//! ubos-bench check --database URL [--objects N] [--months M] [--seed S]
//! ubos-bench run   --database URL --what read|commit|diff [--objects N] [--months M]
//!                  [--seed S] [--seconds T] [--rate R] [--workers W] [--per-commit K]
//!                  [--group G]
//! ```
//!
//! Every command prints a JSON report (`--report FILE` also writes it). `--objects`,
//! `--months` and `--seed` describe the dataset (WL-GENERIC-10M: 10,000,000 objects,
//! 24 months, seed 20260930) and must be the same for `load` and `run`.

mod load;
mod run;
mod tree;
mod workload;

use serde_json::Value;
use workload::Spec;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let num = |k: &str, d: u64| opt(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let spec = Spec {
        objects: num("--objects", 10_000_000),
        months: num("--months", 24),
        seed: num("--seed", 20_260_930),
    };
    let url = opt("--database").unwrap_or_else(|| "host=127.0.0.1 user=postgres dbname=bpa".into());
    let threads = num(
        "--threads",
        std::thread::available_parallelism().map_or(4, |n| n.get() as u64),
    );
    let r: Result<Value, String> = match args.first().map(String::as_str) {
        Some("tree") => tree::tree(&args[1..]).map(|_| Value::Null),
        Some("load") => load::load(&url, spec, threads).map_err(|e| e.to_string()),
        Some("check") => load::check(&url, spec).map_err(|e| e.to_string()),
        Some("run") => {
            let o = run::Opts {
                what: opt("--what").unwrap_or_else(|| "read".into()),
                seconds: num("--seconds", 60),
                rate: opt("--rate").and_then(|v| v.parse().ok()).unwrap_or(500.0),
                workers: num("--workers", 8),
                per_commit: num("--per-commit", 10),
                group: num("--group", 64),
            };
            run::run(&url, spec, &o).map_err(|e| e.to_string())
        }
        _ => Err("usage: ubos-bench tree|load|check|run [options] (see the source header)".into()),
    };
    match r {
        Ok(Value::Null) => {}
        Ok(v) => {
            let text = serde_json::to_string_pretty(&v).unwrap_or_default();
            println!("{text}");
            if let Some(p) = opt("--report")
                && let Err(e) = std::fs::write(&p, text + "\n")
            {
                eprintln!("{p}: {e}");
                std::process::exit(2);
            }
            let errors = v["result"]["errors"].as_u64().unwrap_or(0);
            if errors > 0 {
                eprintln!("{errors} operations failed");
                std::process::exit(3);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
