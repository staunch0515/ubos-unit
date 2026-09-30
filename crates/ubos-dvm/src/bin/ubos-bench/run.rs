//! Benchmark driver of VER-BENCH-0001 (MET-PERF-001…003) against a loaded dataset.
//!
//! - `read`: point reads of random objects at the head (MET-PERF-001), open loop at the
//!   given rate, spread over worker connections.
//! - `commit`: commits of 1–10 random objects (NR-PERF-002) submitted at the given rate to
//!   one sequencer connection, which serialises commits to `main` and flushes the commits
//!   that are ready together in one transaction (group commit, DSN-DVM-006, at most
//!   `--group` per group); latency counts from the scheduled submission, so queueing is
//!   included (MET-PERF-002).
//! - `diff`: diffs between two commits ten single-object commits apart (MET-PERF-003); the
//!   run first appends the single-object commits it needs.
//!
//! Latencies are measured from the scheduled start of each operation, so a slow system
//! cannot hide queueing (no coordinated omission).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use ubos_dvm::Engine;
use ubos_store::PgStore;
use ubos_types::{BpaError, content_id};
use ubos_version::Repo;

use crate::workload::{self, Spec};

/// Options of one run.
#[derive(Debug, Clone)]
pub struct Opts {
    /// `read`, `commit` or `diff`.
    pub what: String,
    /// Duration of the measured phase.
    pub seconds: u64,
    /// Operations per second (all workers together).
    pub rate: f64,
    /// Worker connections for reads and diffs.
    pub workers: u64,
    /// Objects per commit: uniform in `1..=per_commit`.
    pub per_commit: u64,
    /// Largest commit group (DSN-DVM-006 default 64; 1 disables group commit).
    pub group: u64,
}

fn attach(url: &str, spec: &Spec) -> Result<Engine, BpaError> {
    Engine::attach(
        Box::new(PgStore::connect(url)?),
        &workload::classes(spec.seed),
    )
}

/// Percentiles and a histogram of latencies given in microseconds.
pub fn summary(mut lat: Vec<u64>) -> Value {
    lat.sort_unstable();
    let pct = |p: f64| -> f64 {
        if lat.is_empty() {
            return 0.0;
        }
        let i = ((lat.len() as f64 - 1.0) * p).round() as usize;
        lat[i] as f64 / 1000.0
    };
    let bounds = [
        0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.25, 8.0, 10.0, 15.0, 19.0, 25.0, 50.0, 62.5, 100.0,
        250.0, 1000.0,
    ];
    let mut hist = Map::new();
    let mut prev = 0.0;
    for b in bounds {
        let n = lat
            .iter()
            .filter(|x| {
                let ms = **x as f64 / 1000.0;
                ms > prev && ms <= b
            })
            .count();
        hist.insert(format!("<= {b} ms"), json!(n));
        prev = b;
    }
    hist.insert(
        format!("> {prev} ms"),
        json!(lat.iter().filter(|x| **x as f64 / 1000.0 > prev).count()),
    );
    json!({"count": lat.len(), "p50_ms": pct(0.5), "p90_ms": pct(0.9), "p95_ms": pct(0.95),
           "p99_ms": pct(0.99), "p999_ms": pct(0.999), "max_ms": pct(1.0), "histogram": hist})
}

/// Facts about the machine for the report and its environment hash.
pub fn environment(url: &str) -> Result<Value, BpaError> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let field = |k: &str| {
        cpuinfo
            .lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split(':').nth(1))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let flags = field("flags");
    let mem = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let env = json!({
        "cpu_model": field("model name"),
        "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "sha_extensions": flags.split_whitespace().any(|f| f == "sha_ni"),
        "memory": mem,
        "kernel": std::fs::read_to_string("/proc/version").unwrap_or_default().trim(),
        "postgresql": PgStore::connect(url)?.server_facts()?,
        "build": option_env!("UBOS_BUILD").unwrap_or("local"),
    });
    let hash = content_id("ubos.chunk.v1", &env)?;
    Ok(json!({"facts": env, "hash": hash}))
}

/// A body change of object `i` for a benchmark commit numbered `n`.
fn change(engine: &mut Engine, spec: &Spec, i: u64, n: u64) -> Result<Value, BpaError> {
    let oid = workload::object_id(spec.seed, i);
    let mut repo = Repo::new(engine.store.as_mut(), &engine.registry);
    let o = repo
        .object_at("main", None, &oid)?
        .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", oid.clone()))?;
    let fields: Vec<(String, String)> = engine
        .registry
        .class(&o.class)?
        .fields
        .iter()
        .map(|(k, d)| (k.clone(), d["type"].as_str().unwrap_or("text").to_string()))
        .collect();
    let b = workload::body(spec.seed ^ 0xbe7c, i, 100 + n, &fields, Some(&Map::new()));
    Ok(json!({"op": "update", "object_id": oid, "base": o.head, "body": b}))
}

fn reads(url: &str, spec: &Spec, o: &Opts) -> Result<Value, BpaError> {
    let start = Instant::now() + Duration::from_millis(500);
    let end = start + Duration::from_secs(o.seconds);
    let per = o.rate / o.workers as f64;
    let results: Vec<Result<(Vec<u64>, u64), BpaError>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..o.workers)
            .map(|w| {
                s.spawn(move || {
                    let mut e = attach(url, spec)?;
                    let (mut lat, mut errors, mut k) = (Vec::new(), 0u64, 0u64);
                    loop {
                        let at = start + Duration::from_secs_f64(k as f64 / per);
                        if at >= end {
                            break;
                        }
                        if let Some(d) = at.checked_duration_since(Instant::now()) {
                            std::thread::sleep(d);
                        }
                        let i = workload::mix(spec.seed ^ 0x7ead, w, k) % spec.objects;
                        let q = json!({"object_id": workload::object_id(spec.seed, i)});
                        if e.read(&q).is_err() {
                            errors += 1;
                        }
                        lat.push(at.elapsed().as_micros() as u64);
                        k += 1;
                    }
                    Ok((lat, errors))
                })
            })
            .collect();
        hs.into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    Err(BpaError::new("NOD.STORAGE_UNAVAILABLE", "worker panicked"))
                })
            })
            .collect()
    });
    let (mut lat, mut errors) = (Vec::new(), 0);
    for r in results {
        let (l, e) = r?;
        lat.extend(l);
        errors += e;
    }
    let n = lat.len();
    Ok(
        json!({"latency": summary(lat), "errors": errors, "achieved_rate": n as f64 / o.seconds as f64}),
    )
}

fn commits(url: &str, spec: &Spec, o: &Opts) -> Result<Value, BpaError> {
    let (tx, rx) = mpsc::channel::<(Instant, u64)>();
    let start = Instant::now() + Duration::from_millis(500);
    let end = start + Duration::from_secs(o.seconds);
    let rate = o.rate;
    let producer = std::thread::spawn(move || {
        let mut k = 0u64;
        loop {
            let at = start + Duration::from_secs_f64(k as f64 / rate);
            if at >= end {
                break;
            }
            if let Some(d) = at.checked_duration_since(Instant::now()) {
                std::thread::sleep(d);
            }
            if tx.send((at, k)).is_err() {
                break;
            }
            k += 1;
        }
    });
    let mut e = attach(url, spec)?;
    let (mut lat, mut service, mut errors, mut objects) = (Vec::new(), Vec::new(), 0u64, 0u64);
    let mut sizes = Vec::new();
    let (mut prep_s, mut group_s) = (0f64, 0f64);
    let mut carry: Option<(Instant, u64)> = None;
    loop {
        // A group: the next request (waiting if none), then whatever else is ready, up to
        // `group` requests, cut before a request that touches an object already in the group.
        let first = match carry.take() {
            Some(r) => r,
            None => match rx.recv() {
                Ok(r) => r,
                Err(_) => break,
            },
        };
        let began = Instant::now();
        let mut reqs = vec![first];
        while reqs.len() < o.group.max(1) as usize {
            match rx.try_recv() {
                Ok(r) => reqs.push(r),
                Err(_) => break,
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut inputs = Vec::new();
        let mut taken = Vec::new();
        for (at, k) in reqs {
            let n = 1 + workload::mix(spec.seed ^ 0xc0, k, 1) % o.per_commit.max(1);
            let ids: Vec<u64> = (0..n)
                .map(|j| workload::mix(spec.seed ^ 0xc0, k, 10 + j) % spec.objects)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if ids.iter().any(|i| seen.contains(i)) {
                carry = Some((at, k));
                break;
            }
            let mut changes = Vec::new();
            for i in &ids {
                seen.insert(*i);
                changes.push(change(&mut e, spec, *i, k)?);
            }
            objects += changes.len() as u64;
            inputs.push(json!({"branch": "main", "changes": changes}));
            taken.push(at);
        }
        prep_s += began.elapsed().as_secs_f64();
        let g0 = Instant::now();
        match e.commit_group(&inputs) {
            Ok(rs) => errors += rs.iter().filter(|r| r.is_err()).count() as u64,
            Err(_) => errors += inputs.len() as u64,
        }
        group_s += g0.elapsed().as_secs_f64();
        sizes.push(inputs.len() as u64);
        for at in taken {
            lat.push(at.elapsed().as_micros() as u64);
            service.push(began.elapsed().as_micros() as u64);
        }
    }
    let _ = producer.join();
    let n = lat.len();
    let groups = sizes.len().max(1) as f64;
    Ok(
        json!({"latency": summary(lat), "service_time": summary(service), "errors": errors,
              "objects_committed": objects, "groups": sizes.len(),
              "mean_group_size": sizes.iter().sum::<u64>() as f64 / groups,
              "busy_ms_per_commit": {"prepare": prep_s * 1000.0 / n.max(1) as f64,
                                     "commit_group": group_s * 1000.0 / n.max(1) as f64},
              "achieved_rate": n as f64 / o.seconds as f64}),
    )
}

fn diffs(url: &str, spec: &Spec, o: &Opts) -> Result<Value, BpaError> {
    let mut e = attach(url, spec)?;
    let head = |e: &mut Engine| -> Result<u64, BpaError> {
        let id = Repo::new(e.store.as_mut(), &e.registry).commit_at("main", None)?;
        Ok(Repo::new(e.store.as_mut(), &e.registry).commit(id)?.cseq)
    };
    let first = head(&mut e)?;
    for k in 0..200u64 {
        let i = workload::mix(spec.seed ^ 0xd1ff, k, 0) % spec.objects;
        let c = change(&mut e, spec, i, 10_000 + k)?;
        e.commit(&json!({"branch": "main", "changes": [c]}))?;
    }
    let last = head(&mut e)?;
    let start = Instant::now() + Duration::from_millis(500);
    let end = start + Duration::from_secs(o.seconds);
    let per = o.rate / o.workers as f64;
    type Outcome = Result<(Vec<u64>, u64, u64), BpaError>;
    let results: Vec<Outcome> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..o.workers)
            .map(|w| {
                s.spawn(move || {
                    let mut e = attach(url, spec)?;
                    let (mut lat, mut errors, mut changed, mut k) = (Vec::new(), 0u64, 0u64, 0u64);
                    loop {
                        let at = start + Duration::from_secs_f64(k as f64 / per);
                        if at >= end {
                            break;
                        }
                        if let Some(d) = at.checked_duration_since(Instant::now()) {
                            std::thread::sleep(d);
                        }
                        let a = first + workload::mix(spec.seed ^ 0xd1f, w, k) % (last - first - 10);
                        let r = e.other(
                            "diff",
                            &json!({"from": {"branch": "main", "time": a}, "to": {"branch": "main", "time": a + 10}}),
                        );
                        match r {
                            Ok(v) => changed += v.as_array().map_or(0, Vec::len) as u64,
                            Err(_) => errors += 1,
                        }
                        lat.push(at.elapsed().as_micros() as u64);
                        k += 1;
                    }
                    Ok((lat, errors, changed))
                })
            })
            .collect();
        hs.into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    Err(BpaError::new("NOD.STORAGE_UNAVAILABLE", "worker panicked"))
                })
            })
            .collect()
    });
    let (mut lat, mut errors, mut changed) = (Vec::new(), 0, 0);
    for r in results {
        let (l, e, c) = r?;
        lat.extend(l);
        errors += e;
        changed += c;
    }
    let n = lat.len();
    Ok(json!({"latency": summary(lat), "errors": errors,
              "mean_changes_per_diff": changed as f64 / n.max(1) as f64,
              "achieved_rate": n as f64 / o.seconds as f64}))
}

/// Run one benchmark and return its report.
pub fn run(url: &str, spec: Spec, o: &Opts) -> Result<Value, BpaError> {
    let result = match o.what.as_str() {
        "read" => reads(url, &spec, o)?,
        "commit" => commits(url, &spec, o)?,
        "diff" => diffs(url, &spec, o)?,
        w => {
            return Err(BpaError::new(
                "ADAPTER.UNSUPPORTED",
                format!("benchmark {w}"),
            ));
        }
    };
    Ok(json!({
        "item": "VER-BENCH-0001", "benchmark": o.what, "spec": spec.to_json(),
        "options": {"seconds": o.seconds, "rate": o.rate, "workers": o.workers, "per_commit": o.per_commit, "group": o.group},
        "environment": environment(url)?, "result": result,
    }))
}
