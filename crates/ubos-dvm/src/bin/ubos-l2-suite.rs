//! `ubos-l2-suite`: runs the L2 escape suite (VER-SEC-0100, EXIT-1-10, DEC-058) against
//! the L2 host of the engine, and the measurements of NR-PERF-010 and W-4.
//!
//! Usage: `ubos-l2-suite [--cases verify/sandbox/l2-escape-cases.json] [--report out.json]
//!        [--key <file with 64 hex chars>] [--bench] [--verbose]`
//!
//! Every case is a script run as the port `run` of an `Invoice` object on a draft branch
//! of a fresh in-memory engine with the L2 host enabled, twice: a case is blocked when
//! the first run's outcome equals the expected one and the second run agrees (same result
//! or error code, same units unless stopped by a clock or by cancellation). Scripts see
//! `ctx`, `sys`, `this` (the invoice: `number` text, `amount` decimal, `qty` integer,
//! `paid` boolean), `params` and `self_id`; `{{R:n:text}}` repeats `text` `n` times and
//! `{{N:n:text}}` does the same with `$i` replaced by the index. Function definitions in
//! a script are hoisted to the top level; the rest becomes the body of `run`.

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::Instant;
use ubos_dvm::{Adapter, BpaError, canonical};

/// Live bytes are counted for the memory limit of the L2 host (DSN-DVM-612, W-2).
#[global_allocator]
static ALLOC: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

const NOW: &str = "2026-01-01T00:00:00.000000Z";
const SEED: &str = "2026093020260930202609302026093020260930202609302026093020260930";
const INVOICE: &str = "01890a5d-ac96-7001-8000-000000000001";
const ASSET: &str = "01890a5d-ac96-705a-8000-00000000005a";
const PROCESS: &str = "01890a5d-ac96-71f5-8000-0000000001f5";

fn classes() -> Value {
    let f =
        |name: &str, t: &str| json!({"name": name, "label": name, "description": name, "type": t});
    json!([{
        "name": "Invoice", "label": "Invoice", "description": "Invoice", "namespace": "acme.l2",
        "kind": "definition", "parent": "DataObject", "schema_version": 1,
        "fields": [f("number", "text"), f("amount", "decimal"), f("qty", "integer"), f("paid", "boolean")],
        "ports": [{"name": "run", "description": "run", "kind": "command", "params": [],
                   "impl": "acme.l2e#run", "permission": "run"}]
    }])
}

/// Expand `{{R:n:text}}` and `{{N:n:text}}`.
fn expand(script: &str) -> String {
    let mut out = String::new();
    let mut rest = script;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 2..];
        let Some(end) = tail.find("}}") else {
            out.push_str(&rest[i..]);
            return out;
        };
        let spec = &tail[..end];
        let mut parts = spec.splitn(3, ':');
        let (kind, n, text) = (
            parts.next().unwrap_or(""),
            parts
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0),
            parts.next().unwrap_or(""),
        );
        for k in 0..n {
            if kind == "N" {
                out.push_str(&text.replace("$i", &k.to_string()));
            } else {
                out.push_str(text);
            }
        }
        rest = &tail[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Hoist top-level `fn` definitions; the rest becomes the body of `run`.
fn source_of(script: &str) -> String {
    let b: Vec<char> = script.chars().collect();
    let (mut defs, mut body) = (String::new(), String::new());
    let mut i = 0;
    while i < b.len() {
        let at_fn = b[i..].iter().collect::<String>().starts_with("fn ")
            && (i == 0 || !(b[i - 1].is_alphanumeric() || b[i - 1] == '_'));
        if at_fn {
            // find the opening brace, then its match
            let mut j = i;
            while j < b.len() && b[j] != '{' {
                j += 1;
            }
            let mut depth = 0;
            let mut in_str: Option<char> = None;
            let mut k = j;
            while k < b.len() {
                let c = b[k];
                if let Some(q) = in_str {
                    if c == '\\' {
                        k += 2;
                        continue;
                    }
                    if c == q {
                        in_str = None;
                    }
                } else if c == '"' || c == '\'' {
                    in_str = Some(c);
                } else if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                k += 1;
            }
            let end = (k + 1).min(b.len());
            defs.push_str(&b[i..end].iter().collect::<String>());
            defs.push('\n');
            i = end;
            continue;
        }
        body.push(b[i]);
        i += 1;
    }
    format!("{defs}fn run() {{\n{body}\n}}\n")
}

struct Run {
    outcome: Result<Value, String>,
    detail: String,
    units: u64,
    ms: f64,
    stopped_by_clock: bool,
}

fn run_case(case: &Value, source: &str) -> Result<Run, BpaError> {
    let mut a = Adapter::new()?;
    a.enable_l2();
    a.execute("set_now", &json!({"now": NOW}))?;
    a.execute("load_genesis", &json!({"classes": classes()}))?;
    a.execute(
        "create_branch",
        &json!({"name": "feature/l2e", "from": "main"}),
    )?;
    a.execute("commit", &json!({"branch": "feature/l2e", "changes": [
        {"op": "create", "object_id": INVOICE, "class": "Invoice", "slug": "inv-1",
         "body": {"number": "INV-1", "amount": "10.50", "qty": 2, "paid": false}},
        {"op": "create", "object_id": ASSET, "class": "Logic", "slug": "acme-l2e",
         "body": {"name": "acme.l2e", "tier": "l2", "abi": "1.0", "source": source, "exports": ["run"],
                  "capabilities": ["commit", "read", "run", "emit"], "profile": "strict", "state": "committed"}}
    ]}))?;
    let profile = case
        .get("profile")
        .and_then(Value::as_str)
        .unwrap_or("strict");
    let mut input = json!({"branch": "feature/l2e", "target": format!("ubos://acme/Invoice/inv-1"), "port": "run",
        "params": {}, "process": {"process_id": PROCESS}, "entropy_seed": SEED, "ctx": {"profile": profile}});
    if let Some(l) = case.get("limits").and_then(Value::as_object) {
        let mut limits = Map::new();
        for (k, v) in l {
            let key = if k == "operations" {
                "units"
            } else {
                k.as_str()
            };
            limits.insert(key.to_string(), v.clone());
        }
        input["limits"] = Value::Object(limits);
    }
    if let Some(c) = case.get("cancel_after_units").and_then(Value::as_u64) {
        input["cancel_after_units"] = json!(c);
    }
    let t = Instant::now();
    let r = a.execute("execute", &input);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let mut detail = String::new();
    let (outcome, units) = match r {
        Ok(v) => (
            Ok(v["result"].clone()),
            v["metering"]["units"].as_u64().unwrap_or_default(),
        ),
        Err(e) => {
            let m = a
                .execute("metering", &json!({"process_id": PROCESS}))
                .ok()
                .and_then(|m| m["units"].as_u64())
                .unwrap_or_default();
            detail = e.message.clone();
            (Err(e.code.to_string()), m)
        }
    };
    let stopped_by_clock =
        matches!(&outcome, Err(c) if c == "LOGIC.LIMIT_EXCEEDED" || c == "LOGIC.CANCELLED");
    Ok(Run {
        outcome,
        detail,
        units,
        ms,
        stopped_by_clock,
    })
}

fn expected_matches(expect: &Value, r: &Run) -> bool {
    if let Some(code) = expect.get("error").and_then(Value::as_str) {
        return matches!(&r.outcome, Err(c) if c == code);
    }
    if let Some(v) = expect.get("value") {
        return matches!(&r.outcome, Ok(x) if x == v);
    }
    // `deterministic` (and the PH-0 `journaled`): any outcome that completes the run
    r.outcome.is_ok() || expect.get("deterministic").is_some()
}

fn agree(a: &Run, b: &Run) -> bool {
    let same = match (&a.outcome, &b.outcome) {
        (Ok(x), Ok(y)) => x == y,
        (Err(x), Err(y)) => x == y,
        _ => false,
    };
    same && (a.stopped_by_clock || a.units == b.units)
}

fn bench() -> Result<Value, BpaError> {
    // NR-PERF-010: L1 expression evaluation on the kernel path (parse, check and
    // evaluate a guard against a typed `this`; median of 20,000), then with the checked
    // tree reused (evaluation only; DSN-DVM-610).
    let classes = classes();
    let reg = ubos_model::registry_of(classes.as_array().map(Vec::as_slice).unwrap_or(&[]), &[])?;
    let this_t = ubos_expr::class_type(&reg, "Invoice")?;
    let this_v = json!({"number": "INV-1", "amount": "10.50", "qty": 2, "paid": false});
    let expr = json!("this.qty > 0 and this.amount > 100 and not this.paid");
    let mut times = Vec::with_capacity(20_000);
    let mut env = ubos_expr::Env::new(Some(this_t.clone()), None, None, None);
    for _ in 0..20_000 {
        let t = Instant::now();
        let ast = ubos_expr::parse(&expr)?;
        ubos_expr::check(&ast, &mut env)?;
        let mut ev = ubos_expr::Evaluator::new(&env);
        ev.this = Some(ubos_expr::from_json(&this_t, &this_v));
        ev.eval(&ast)?;
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let l1_median_us = times[times.len() / 2];
    let ast = ubos_expr::parse(&expr)?;
    ubos_expr::check(&ast, &mut env)?;
    let this_val = ubos_expr::from_json(&this_t, &this_v);
    let mut times = Vec::with_capacity(20_000);
    for _ in 0..20_000 {
        let t = Instant::now();
        let mut ev = ubos_expr::Evaluator::new(&env);
        ev.this = Some(this_val.clone());
        ev.eval(&ast)?;
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let l1_cached_median_us = times[times.len() / 2];
    // NR-PERF-010: warm L2 invocation (p95 of 2,000 invocations of a trivial port).
    let mut a = Adapter::new()?;
    a.enable_l2();
    a.execute("set_now", &json!({"now": NOW}))?;
    a.execute("load_genesis", &json!({"classes": classes}))?;
    a.execute(
        "create_branch",
        &json!({"name": "feature/l2e", "from": "main"}),
    )?;
    a.execute("commit", &json!({"branch": "feature/l2e", "changes": [
        {"op": "create", "object_id": INVOICE, "class": "Invoice", "slug": "inv-1",
         "body": {"number": "INV-1", "amount": "10.50", "qty": 2, "paid": false}},
        {"op": "create", "object_id": ASSET, "class": "Logic", "slug": "acme-l2e",
         "body": {"name": "acme.l2e", "tier": "l2", "abi": "1.0", "source": "fn run() { this.qty * 2 }", "exports": ["run"],
                  "capabilities": ["read"], "profile": "standard", "state": "committed"}}
    ]}))?;
    let input = json!({"branch": "feature/l2e", "target": "ubos://acme/Invoice/inv-1", "port": "run",
        "params": {}, "entropy_seed": SEED, "ctx": {"profile": "standard"}});
    let mut times = Vec::with_capacity(2_000);
    for _ in 0..2_000 {
        let t = Instant::now();
        a.execute("execute", &input)?;
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let l2_p95_us = times[times.len() * 95 / 100];
    let l2_median_us = times[times.len() / 2];
    // W-4: element-wise growth under the engine's size checks.
    let mut growth = Map::new();
    for (what, n) in [
        ("array", 1_000),
        ("array", 10_000),
        ("array", 65_536),
        ("string", 1_000),
        ("string", 10_000),
        ("string", 100_000),
    ] {
        let src = if what == "array" {
            format!("fn run() {{ let a = []; for i in 0..{n} {{ a.push(i); }} a.len() }}")
        } else {
            format!("fn run() {{ let s = \"\"; for i in 0..{n} {{ s += \"x\"; }} s.len() }}")
        };
        let head = a.execute(
            "read",
            &json!({"branch": "feature/l2e", "object_id": ASSET}),
        )?;
        a.execute("commit", &json!({"branch": "feature/l2e", "changes": [
            {"op": "update", "object_id": ASSET, "base": head["version_hash"], "body": {"source": src}}]}))?;
        let t = Instant::now();
        let r = a.execute("execute", &input);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        growth.insert(
            format!("{what}:{n}"),
            json!({"ms": (ms * 100.0).round() / 100.0, "outcome": r.map(|_| "ok".to_string()).unwrap_or_else(|e| e.code.to_string())}),
        );
    }
    Ok(
        json!({"l1_median_us": (l1_median_us * 100.0).round() / 100.0,
        "l1_cached_median_us": (l1_cached_median_us * 100.0).round() / 100.0,
        "l2_median_us": (l2_median_us * 10.0).round() / 10.0, "l2_p95_us": (l2_p95_us * 10.0).round() / 10.0,
        "growth": growth}),
    )
}

fn main() -> ExitCode {
    ubos_dvm::memory::install_probe(|| ALLOC.allocated());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let verbose = args.iter().any(|a| a == "--verbose");
    let cases_path = arg("--cases").unwrap_or_else(|| "verify/sandbox/l2-escape-cases.json".into());
    let suite: Value = match std::fs::read_to_string(&cases_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{cases_path}: {e}");
            return ExitCode::from(2);
        }
    };
    let list = suite["cases"].as_array().cloned().unwrap_or_default();
    let start = Instant::now();
    let mut results = Vec::new();
    let mut cats: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut codes: BTreeMap<String, u64> = BTreeMap::new();
    let mut blocked = 0u64;
    for case in &list {
        let id = case["id"].as_str().unwrap_or_default().to_string();
        let cat = case["category"].as_str().unwrap_or("other").to_string();
        let source = source_of(&expand(case["script"].as_str().unwrap_or_default()));
        let (r1, r2) = match (run_case(case, &source), run_case(case, &source)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("{id}: setup failed: {e}");
                return ExitCode::from(2);
            }
        };
        let ok = expected_matches(&case["expect"], &r1) && agree(&r1, &r2);
        let e = cats.entry(cat.clone()).or_insert((0, 0));
        e.0 += 1;
        if ok {
            e.1 += 1;
            blocked += 1;
        }
        let actual = match &r1.outcome {
            Ok(v) => json!({"value": v}),
            Err(c) => json!({"error": c}),
        };
        *codes
            .entry(match &r1.outcome {
                Ok(_) => "value".into(),
                Err(c) => c.clone(),
            })
            .or_insert(0) += 1;
        if verbose || !ok {
            println!(
                "{} {id} [{cat}] expected {} actual {} units {} ({:.1} ms){}",
                if ok { "ok  " } else { "FAIL" },
                case["expect"],
                actual,
                r1.units,
                r1.ms,
                if agree(&r1, &r2) {
                    ""
                } else {
                    " NONDETERMINISTIC"
                }
            );
            if !r1.detail.is_empty() {
                println!("     {}", r1.detail.lines().next().unwrap_or_default());
            }
        }
        results.push(
            json!({"id": id, "category": cat, "title": case["title"], "expect": case["expect"],
            "actual": actual, "units": r1.units, "ms": (r1.ms * 10.0).round() / 10.0, "blocked": ok,
            "deterministic": agree(&r1, &r2)}),
        );
    }
    let bench_out = if args.iter().any(|a| a == "--bench") {
        match bench() {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("bench: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let mut report = json!({
        "suite": suite["suite"], "version": suite["version"], "verifies": "VER-SEC-0100",
        "engine": "rhai 1.26.1", "host": "ubos-runtime::l2 (DEC-058)", "profile_default": "strict",
        "implementation": {"name": "ubos-dvm", "version": env!("CARGO_PKG_VERSION")},
        "environment": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                        "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)},
        "cases": list.len(), "blocked": blocked,
        "categories": cats.iter().map(|(k, (n, b))| (k.clone(), json!({"cases": n, "blocked": b}))).collect::<Map<_, _>>(),
        "outcomes": codes, "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "results": results,
    });
    if let Some(b) = bench_out {
        report["bench"] = b;
    }
    // signed by an ephemeral runner key, like the forge reports
    let seed: [u8; 32] = match arg("--key") {
        Some(p) => {
            let t = std::fs::read_to_string(&p).unwrap_or_default();
            let mut s = [0u8; 32];
            for (i, b) in s.iter_mut().enumerate() {
                *b = u8::from_str_radix(t.trim().get(2 * i..2 * i + 2).unwrap_or("00"), 16)
                    .unwrap_or(0);
            }
            s
        }
        None => ubos_crypto::random_bytes::<32>().unwrap_or([7u8; 32]),
    };
    let body = canonical(&report).unwrap_or_default();
    let sig = ubos_crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] = json!({"alg": "ed25519", "key": hex(&ubos_crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    println!(
        "l2 escape suite: {blocked} of {} cases blocked{}",
        list.len(),
        report
            .get("bench")
            .map(|b| format!("; bench {b}"))
            .unwrap_or_default()
    );
    if let Some(path) = arg("--report") {
        let text = serde_json::to_string_pretty(&report).unwrap_or_default() + "\n";
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    }
    if blocked == list.len() as u64 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
