//! `ubos-sandbox-spike`: runs the initial L2 escape suite (VER-SEC-0001, EXIT-0-06).
//!
//! ```text
//! ubos-sandbox-spike --cases verify/sandbox/l2-escape-cases.json [--report out.json]
//! ```
//!
//! Every case runs twice, each time in a freshly built sandbox, the second time on
//! another thread. A case is blocked when its outcome matches the expectation and
//! both runs agree (no entropy or host state leaks into the result).

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use cap::Cap;
use serde_json::{Value, json};
use ubos_runtime::{Context, FieldType, Limits, Outcome, Sandbox};

const CLASS: (&str, &[(&str, FieldType)]) = (
    "Invoice",
    &[
        ("number", FieldType::String),
        ("amount", FieldType::Decimal),
        ("qty", FieldType::Integer),
        ("paid", FieldType::Boolean),
    ],
);

/// Counting allocator: the memory probe of the sandbox. Executions run one at a
/// time, so the process-wide count is the execution's count.
#[global_allocator]
static ALLOCATOR: Cap<std::alloc::System> = Cap::new(std::alloc::System, usize::MAX);

fn allocated() -> usize {
    ALLOCATOR.allocated()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cases = None;
    let mut report = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--cases" => cases = it.next().cloned(),
            "--report" => report = it.next().cloned(),
            _ => {
                eprintln!("usage: ubos-sandbox-spike --cases <file> [--report <file>]");
                return ExitCode::from(2);
            }
        }
    }
    let Some(cases) = cases else {
        eprintln!("--cases is required");
        return ExitCode::from(2);
    };
    let suite: Value = match std::fs::read_to_string(&cases)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{cases}: {e}");
            return ExitCode::from(2);
        }
    };
    let list = suite["cases"].as_array().cloned().unwrap_or_default();

    let started = Instant::now();
    let mut rows = Vec::new();
    let mut by_category: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut blocked = 0u64;
    for case in &list {
        let row = run_case(case);
        let ok = row["blocked"].as_bool() == Some(true);
        let cat = case["category"].as_str().unwrap_or("?").to_string();
        let e = by_category.entry(cat).or_default();
        e.0 += 1;
        if ok {
            e.1 += 1;
            blocked += 1;
        } else {
            eprintln!(
                "NOT BLOCKED {}: {}",
                row["id"],
                serde_json::to_string(&row).unwrap_or_default()
            );
        }
        rows.push(row);
    }
    let total = list.len() as u64;
    let categories: Value = by_category
        .iter()
        .map(|(k, (n, b))| (k.clone(), json!({"cases": n, "blocked": b})))
        .collect::<serde_json::Map<_, _>>()
        .into();
    let out = json!({
        "suite": suite["suite"],
        "version": suite["version"],
        "engine": format!("rhai {}", rhai_version()),
        "profile_default": "strict",
        "cases": total,
        "blocked": blocked,
        "categories": categories,
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "results": rows,
    });
    for (k, (n, b)) in &by_category {
        println!("{k:<20} {b:>3} / {n:<3} blocked");
    }
    println!("total                {blocked:>3} / {total:<3} blocked");
    if let Some(path) = report {
        let text = serde_json::to_string_pretty(&out).unwrap_or_default() + "\n";
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    }
    if blocked == total && total > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn rhai_version() -> &'static str {
    // Pinned by Cargo.lock; kept in step by hand for the report.
    "1.26.1"
}

/// Expands `{{R:n:text}}` (text repeated n times) and `{{N:n:text}}` (text repeated
/// n times with `$i` replaced by the index).
fn expand(script: &str) -> String {
    let mut out = String::new();
    let mut rest = script;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        let body = &rest[start + 2..start + end];
        let mut parts = body.splitn(3, ':');
        let (kind, n, text) = (parts.next(), parts.next(), parts.next());
        let n: usize = n.and_then(|n| n.parse().ok()).unwrap_or(0);
        let text = text.unwrap_or("");
        for i in 0..n {
            if kind == Some("N") {
                out.push_str(&text.replace("$i", &i.to_string()));
            } else {
                out.push_str(text);
            }
        }
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

fn limits_for(case: &Value) -> Limits {
    let profile = case["profile"].as_str().unwrap_or("strict");
    let mut l = Limits::profile(profile).unwrap_or_else(|| Limits::profile("strict").unwrap());
    if let Some(o) = case["limits"].as_object() {
        if let Some(v) = o.get("operations").and_then(Value::as_u64) {
            l.operations = v;
        }
        if let Some(v) = o.get("cpu_ms").and_then(Value::as_u64) {
            l.cpu_ms = v;
        }
        if let Some(v) = o.get("wall_ms").and_then(Value::as_u64) {
            l.wall_ms = v;
        }
        if let Some(v) = o.get("memory").and_then(Value::as_u64) {
            l.memory = usize::try_from(v).unwrap_or(usize::MAX);
        }
    }
    l
}

fn execute(case: &Value, script: &str) -> (Outcome, u128) {
    let sandbox = Sandbox::new(limits_for(case), CLASS.0, CLASS.1).with_memory_probe(allocated);
    if let Some(ms) = case["cancel_after_ms"].as_u64() {
        let flag = sandbox.cancel_handle();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            flag.store(true, Ordering::Relaxed);
        });
    }
    let t = Instant::now();
    let outcome = sandbox.run(script, &Context::default());
    (outcome, t.elapsed().as_millis())
}

fn run_case(case: &Value) -> Value {
    let script = expand(case["script"].as_str().unwrap_or(""));
    let (first, ms) = execute(case, &script);
    let (c2, s2) = (case.clone(), script.clone());
    let second = std::thread::spawn(move || execute(&c2, &s2).0).join().ok();
    // A termination by elapsed time or cancellation stops at a point that depends on
    // host speed: there the code must agree, the operation count may differ.
    let timed = |o: &Outcome| {
        o.result
            .as_ref()
            .is_err_and(|f| f.detail.starts_with("Script terminated"))
    };
    let same = match &second {
        Some(o) if timed(&first) && timed(o) => {
            o.result.as_ref().err().map(|f| f.code) == first.result.as_ref().err().map(|f| f.code)
                && o.journal == first.journal
        }
        Some(o) => *o == first,
        None => false,
    };
    let expect = &case["expect"];
    let matches = match &first.result {
        Err(f) => expect["error"].as_str() == Some(f.code),
        Ok(v) => {
            expect.get("value") == Some(v)
                || expect["journaled"].as_u64() == Some(first.journal.len() as u64)
                || expect["deterministic"].as_bool() == Some(true)
        }
    } || (expect["deterministic"].as_bool() == Some(true) && first.result.is_err());
    let actual = match &first.result {
        Ok(v) => json!({"value": v}),
        Err(f) => json!({"error": f.code, "detail": f.detail}),
    };
    json!({
        "id": case["id"],
        "category": case["category"],
        "title": case["title"],
        "expect": expect,
        "actual": actual,
        "operations": first.operations,
        "instruction_calls": first.instruction_calls,
        "journal": first.journal.len(),
        "milliseconds": ms,
        "replay_identical": same,
        "blocked": matches && same,
    })
}
