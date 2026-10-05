//! `ubos-replay` — the replay corpus and its cross-platform check (SUITE-DVM-REPLAY,
//! VER-CONF-8500, EXIT-1-08; PH-1 step 8c, DEC-061).
//!
//! ```text
//! ubos-replay record --expected <file> [--count 10000] [--seed 20261005]
//! ubos-replay check  --expected <file> [--report <file>] [--replay-every 10] [--verbose]
//! ```
//!
//! The corpus is generated, not stored: process `i` of seed `s` is a host program (seven in
//! ten) or an L2 script (three in ten) over seeded inputs — decimals of random scale and
//! sign, integers, dates, texts with non-ASCII letters, ligatures, combining marks and
//! surrounding spaces, money in currencies of different minor units, lists — exercising
//! the determinism-sensitive parts of the kernel: exact decimal arithmetic and rounding,
//! money allocation, calendar arithmetic and time zones, Unicode case mapping, NFC and
//! ordering, canonical formatting, seeded entropy (`sys.uuid`) and the clock. Every process
//! runs on a fresh in-memory VAE through the adapter with a fixed clock and entropy seed;
//! its outcome — the result or the error code, the metering units and instruction counts —
//! is hashed (SHA-256 of the canonical JSON). `record` writes the hashes; `check` recomputes
//! them on the platform it runs on, compares, and replays every n-th process from its
//! journal (NR-DET-002). The report is signed by an ephemeral runner key.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Instant;
use ubos_dvm::Adapter;
use ubos_types::{BpaError, canonical};

const DEFAULT_SEED: u64 = 20_261_005;
const NOW: &str = "2026-10-05T08:30:00.000000Z";

struct Rng(u64);

impl Rng {
    fn new(seed: u64, i: u64) -> Rng {
        Rng(seed ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x2545_F491_4F6C_DD1D)
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
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

// ---- seeded inputs ----

const TEXTS: &[&str] = &[
    "Ångström",
    "straße",
    "İstanbul",
    "ﬁnance",
    "café",
    "cafe\u{301}",
    "ǅemal",
    "ΣΊΣΥΦΟΣ",
    "北京市",
    "東京",
    "naïve",
    "Łódź",
    "ÉCOLE",
    " padded ",
    "tab\tbed",
    "emoji 🙂",
    "Ω≠ω",
    "Zürich",
    "zurich",
    "zürich",
    "ÆSIR",
    "ﬀ",
    "ß",
    "Æbleskiver",
    "Ďáblice",
    "Çà",
    "ı",
];
const CURRENCIES: &[&str] = &["USD", "EUR", "JPY", "KWD", "CHF", "BHD"];
const ZONES: &[&str] = &[
    "Europe/Berlin",
    "America/New_York",
    "Asia/Tokyo",
    "Australia/Lord_Howe",
    "UTC",
    "Pacific/Chatham",
    "America/St_Johns",
];

fn decimal(rng: &mut Rng) -> String {
    let scale = rng.below(7);
    let digits = 1 + rng.below(12);
    let mut s = String::new();
    for k in 0..digits {
        let d = if k == 0 {
            1 + rng.below(9)
        } else {
            rng.below(10)
        };
        s.push(char::from(b'0' + d as u8));
    }
    if scale > 0 {
        while (s.len() as u64) <= scale {
            s.insert(0, '0');
        }
        let p = s.len() - scale as usize;
        s.insert(p, '.');
    }
    if rng.below(4) == 0 {
        s.insert(0, '-');
    }
    s
}

fn date(rng: &mut Rng) -> String {
    let y = 2020 + rng.below(11);
    let m = 1 + rng.below(12);
    let d = 1 + rng.below(if m == 2 { 28 } else { 30 });
    format!("{y:04}-{m:02}-{d:02}")
}

fn text(rng: &mut Rng) -> String {
    let a = *rng.pick(TEXTS);
    if rng.below(3) == 0 {
        format!("{a} {}", rng.pick(TEXTS))
    } else {
        a.to_string()
    }
}

/// The inputs of process `i`: named values passed as the arguments.
fn inputs(rng: &mut Rng) -> Vec<(&'static str, Value)> {
    let cur = *rng.pick(CURRENCIES);
    let xs: Vec<Value> = (0..3 + rng.below(6)).map(|_| json!(decimal(rng))).collect();
    let ts: Vec<Value> = (0..2 + rng.below(5)).map(|_| json!(text(rng))).collect();
    vec![
        ("a", json!(decimal(rng))),
        ("b", json!(decimal(rng))),
        ("c", json!(decimal(rng))),
        ("i", json!(rng.below(400) as i64 - 200)),
        ("j", json!(rng.below(60) as i64 - 30)),
        ("d1", json!(date(rng))),
        ("d2", json!(date(rng))),
        ("s1", json!(text(rng))),
        ("s2", json!(text(rng))),
        ("m", json!({"amount": decimal(rng), "currency": cur})),
        ("xs", Value::Array(xs)),
        ("ts", Value::Array(ts)),
        ("z", json!(*rng.pick(ZONES))),
    ]
}

/// Expression templates over the inputs (STD-EXPR-005), grouped by what they exercise.
const EXPRESSIONS: &[&str] = &[
    // exact decimal arithmetic and rounding
    "params.a + params.b * params.c",
    "params.a - div(params.b, abs(params.c) + 1, 4, \"half_even\")",
    "round(params.a * params.b, 2, \"half_even\")",
    "div(params.a, abs(params.b) + 0.5, 4, \"half_up\")",
    "round(params.c, 0, \"half_even\")",
    "floor(params.a) + ceil(params.b)",
    "div(params.a, abs(params.b) + 1, 6, \"half_even\")",
    "clamp(params.a, -100.5, 100.5)",
    "min(params.a, params.b) * max(params.b, params.c)",
    "params.i % (abs(params.j) + 1) + params.j % 7",
    "div(params.i, abs(params.j) + 1, 2, \"half_even\")",
    "div(params.a + params.b + params.c, 3, 3, \"half_even\")",
    // money
    "amount(params.m)",
    "currency(params.m)",
    "money(round(params.a, 2, \"half_even\"), \"EUR\")",
    "allocate(params.m, [1, 2, 3])",
    "allocate(params.m, [3, 3, 3, 1])",
    "format(\"{0} and {1}\", params.m, params.a)",
    // dates and time zones (calendars need calendar objects of the VAE: not in the corpus)
    "add_days(params.d1, params.i)",
    "add_months(params.d1, params.j)",
    "days_between(params.d1, params.d2)",
    "weekday(params.d2)",
    "end_of_month(params.d1)",
    "start_of_month(add_months(params.d2, params.j))",
    "year(params.d1) * 100 + month(params.d2)",
    "day(end_of_month(add_days(params.d1, params.i)))",
    "to_zone(ctx.now, params.z)",
    "to_zone(ctx.now, \"Europe/Berlin\")",
    // text: Unicode case mapping, NFC, code points
    "upper(params.s1)",
    "lower(params.s2)",
    "trim(params.s1)",
    "len(params.s2)",
    "substr(params.s1, 1, 3)",
    "concat(params.s1, \"·\", params.s2)",
    "replace(params.s2, \"a\", \"ä\")",
    "pad_left(params.s1, 12, \"·\")",
    "contains(upper(params.s1), \"SS\")",
    "starts_with(lower(params.s2), \"ı\")",
    "format(\"{0}|{1}|{2}\", params.s1, params.i, params.d1)",
    "upper(params.s1) == upper(lower(params.s1))",
    "lower(\"ΣΊΣΥΦΟΣ\") == lower(params.s2)",
    // collections: sums, ordering, distinct
    "sum(params.xs, x -> x * 2)",
    "sum(params.xs, x -> round(x, 1, \"half_even\"))",
    "sort_by(params.ts, t -> t)",
    "sort_by(params.ts, t -> lower(t))",
    "sort_by(params.xs, x -> x)",
    "map(params.xs, x -> div(x, 3, 3, \"half_even\"))",
    "filter(params.xs, x -> x > params.a)",
    "max(params.xs, x -> x) - min(params.xs, x -> x)",
    "count(params.ts, t -> len(t) > 5)",
    "distinct(map(params.ts, t -> upper(t)))",
    "any(params.xs, x -> x < 0) and all(params.ts, t -> len(t) > 0)",
    "map(sort_by(params.ts, t -> len(t)), t -> format(\"{0}:{1}\", t, len(t)))",
];

/// The L2 script of a process (Rhai; decimals through `dec`, strings, arrays, `sys.uuid`).
fn l2_script(rng: &mut Rng) -> String {
    let mut body = Vec::new();
    for _ in 0..3 + rng.below(5) {
        body.push(match rng.below(14) {
            0 => "out.push(to_string(dec(a) + dec(b) * dec(c)));",
            1 => "out.push(to_string(dec(a) - dec(b)));",
            2 => "out.push(to_string(dec(a) * dec(b) * dec(c)));",
            3 => "out.push(if dec(a) < dec(b) { \"lt\" } else { \"ge\" });",
            4 => "out.push(to_string(-dec(c)));",
            5 => "out.push(s1.to_upper());",
            6 => "out.push(s2.to_lower());",
            7 => "out.push(s1.len().to_string());",
            8 => "out.push(s1 + \"·\" + s2);",
            9 => "out.push(s1.trim());",
            10 => "let t = [s1, s2, \"zürich\", \"Zurich\"]; t.sort(); let j = \"\"; for x in t { j += x + \",\"; } out.push(j);",
            11 => "out.push(sys.uuid());",
            12 => "out.push(s2.sub_string(1, 3));",
            _ => "out.push(if s1.contains(\"ß\") { \"sharp s\" } else { \"plain\" });",
        });
    }
    format!(
        "fn main(a, b, c, s1, s2) {{ let out = []; {} out }}",
        body.join(" ")
    )
}

/// A text as an L1 string literal.
fn quote(t: &str) -> String {
    let mut out = String::from("\"");
    for ch in t.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An input as an L1 literal (STD-EXPR-001): decimals and integers as numbers, dates as
/// `d'…'`, money as `12.50USD`, lists in brackets, texts quoted.
fn literal(name: &str, v: &Value) -> String {
    match (name, v) {
        ("d1" | "d2", Value::String(d)) => format!("d'{d}'"),
        ("m", Value::Object(m)) => format!(
            "{}{}",
            m["amount"].as_str().unwrap_or("0"),
            m["currency"].as_str().unwrap_or("USD")
        ),
        ("xs", Value::Array(a)) => format!(
            "[{}]",
            a.iter()
                .map(|x| x.as_str().unwrap_or("0").to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ("ts", Value::Array(a)) => format!(
            "[{}]",
            a.iter()
                .map(|x| quote(x.as_str().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ("a" | "b" | "c", Value::String(d)) => d.clone(),
        (_, Value::Number(n)) => n.to_string(),
        (_, Value::String(t)) => quote(t),
        (_, other) => other.to_string(),
    }
}

/// Process `i`: the asset, the function and the arguments. Host programs carry their inputs
/// as literals in the expressions (a parameter's type is the type of its JSON argument, and
/// decimals travel as text in JSON); L2 scripts take them as arguments.
fn process(seed: u64, i: u64) -> (Value, &'static str, Vec<Value>, bool) {
    let mut rng = Rng::new(seed, i);
    let inputs = inputs(&mut rng);
    let l2 = rng.below(10) < 3;
    let name = format!("corpus.p{i}");
    if l2 {
        let script = l2_script(&mut rng);
        let args: Vec<Value> = ["a", "b", "c", "s1", "s2"]
            .iter()
            .map(|k| {
                inputs
                    .iter()
                    .find(|(n, _)| n == k)
                    .map(|x| x.1.clone())
                    .unwrap_or(Value::Null)
            })
            .collect();
        let asset = json!({"name": name, "abi": "1.0", "tier": "l2", "profile": "standard", "state": "committed",
                           "capabilities": [], "exports": ["main"], "source": script});
        return (asset, "main", args, true);
    }
    let literals: Vec<(String, String)> = inputs
        .iter()
        .map(|(n, v)| (format!("params.{n}"), literal(n, v)))
        .collect();
    let mut body = Vec::new();
    let n = 2 + rng.below(6);
    let mut lets = Vec::new();
    for k in 0..n {
        let mut e = (*rng.pick(EXPRESSIONS)).to_string();
        for (from, to) in &literals {
            e = e.replace(from.as_str(), to);
        }
        body.push(json!({"let": format!("v{k}"), "value": e}));
        lets.push(format!("v{k}"));
    }
    if rng.below(2) == 0 {
        body.push(json!({"do": "sys.uuid", "as": "u"}));
        lets.push("u".into());
    }
    if rng.below(3) == 0 {
        let cond = format!("{} > {}", literals[0].1, literals[1].1);
        body.push(json!({"if": cond, "then": [{"let": "w", "value": "\"a wins\""}], "else": [{"let": "w", "value": "\"b wins\""}]}));
        lets.push("w".into());
    }
    if rng.below(3) == 0 {
        body.push(json!({"let": "acc", "value": "0"}));
        let step = format!("acc + div({}, 7, 3, \"half_even\")", literals[0].1);
        body.push(
            json!({"repeat": 3 + rng.below(4) as i64, "body": [{"let": "acc", "value": step}]}),
        );
        lets.push("acc".into());
    }
    lets.push("ctx.now".into());
    body.push(json!({"return": format!("[{}]", lets.join(", "))}));
    let asset = json!({"name": name, "abi": "1.0", "tier": "host", "profile": "standard", "state": "committed",
                       "capabilities": [], "exports": ["main"],
                       "source": {"functions": {"main": {"params": [], "body": body}}}});
    (asset, "main", Vec::new(), false)
}

fn uid(n: u64) -> String {
    format!("01890a5d-ac96-7{:03x}-8000-{:012x}", n % 4096, n)
}

/// Run process `i` on a fresh VAE; the outcome and, when asked, the replay verdict.
fn run(seed: u64, i: u64, replay: bool) -> Result<(Value, Option<String>), BpaError> {
    let (asset, function, args, l2) = process(seed, i);
    let mut a = Adapter::new()?;
    if l2 {
        a.enable_l2();
    }
    a.execute("set_now", &json!({"now": NOW}))?;
    a.execute("load_genesis", &json!({"classes": []}))?;
    // a feature branch: `main` runs verified assets only (DSN-DVM-610)
    a.execute(
        "create_branch",
        &json!({"name": "feature/corpus", "from": "main"}),
    )?;
    a.execute("commit", &json!({"branch": "feature/corpus", "changes": [{"op": "create", "class": "Logic", "object_id": uid(1), "slug": "corpus", "body": asset}]}))?;
    let mut entropy = [0u8; 32];
    for (k, b) in entropy.iter_mut().enumerate() {
        *b = ((seed >> (k % 8 * 8)) as u8) ^ (i as u8).wrapping_mul(k as u8 + 1);
    }
    let hex: String = entropy.iter().map(|b| format!("{b:02x}")).collect();
    let pid = uid(4096 + i % 4096);
    let out = a.execute("execute", &json!({"branch": "feature/corpus", "asset": asset["name"], "function": function, "args": args,
                                           "entropy_seed": hex, "process": {"process_id": pid}}));
    let outcome = match out {
        Ok(v) => {
            json!({"result": v["result"], "units": v["metering"]["units"], "instructions": v["metering"]["instructions"], "outcome": v["metering"]["outcome"]})
        }
        Err(e) => json!({"error": e.code, "message": e.message, "details": e.details}),
    };
    // a replay must reproduce the outcome: the result, or the same error
    let verdict = if replay {
        Some(match a.execute("replay", &json!({"process_id": pid})) {
            Ok(r) => r["outcome"].as_str().unwrap_or("?").to_string(),
            Err(e) if outcome["error"].as_str() == Some(e.code) => "identical".to_string(),
            Err(e) => format!("error {}", e.code),
        })
    } else {
        None
    };
    Ok((outcome, verdict))
}

fn hash(v: &Value) -> Result<String, BpaError> {
    let c = canonical(v)?;
    Ok(format!("sha256:{:x}", Sha256::digest(c.as_bytes())))
}

fn opt(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().unwrap_or_default();
    let seed: u64 = opt(&args, "--seed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SEED);
    let verbose = args.iter().any(|a| a == "--verbose");
    let Some(expected_path) = opt(&args, "--expected") else {
        eprintln!(
            "usage: ubos-replay record|check --expected <file> [--count N] [--seed S] [--report <file>] [--replay-every N]"
        );
        std::process::exit(2);
    };
    let start = Instant::now();
    match mode.as_str() {
        "diagnose" => {
            // every template once, with the inputs of process 0, through eval_expr
            let mut rng = Rng::new(seed, 0);
            let inputs = inputs(&mut rng);
            let literals: Vec<(String, String)> = inputs
                .iter()
                .map(|(n, v)| (format!("params.{n}"), literal(n, v)))
                .collect();
            let mut a = Adapter::new().expect("adapter");
            a.execute("set_now", &json!({"now": NOW})).expect("now");
            for e in EXPRESSIONS {
                let mut x = e.to_string();
                for (from, to) in &literals {
                    x = x.replace(from.as_str(), to);
                }
                let input = json!({"expr": x, "ctx": {"now": NOW}, "calendars": {"weekdays": {"weekend": [6, 7], "holidays": []}}});
                match a.execute("eval_expr", &input) {
                    Ok(v) => println!("ok   {e}\n     = {}", v),
                    Err(er) => println!(
                        "FAIL {e}\n     {x}\n     {} {} {}",
                        er.code,
                        er.message,
                        er.details.map(|d| d.to_string()).unwrap_or_default()
                    ),
                }
            }
        }
        "record" => {
            let count: u64 = opt(&args, "--count")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000);
            let mut hashes = Vec::new();
            let mut kinds = (0u64, 0u64, 0u64);
            for i in 0..count {
                let (outcome, _) = run(seed, i, false).unwrap_or_else(|e| {
                    eprintln!("process {i}: {e}");
                    std::process::exit(1);
                });
                if outcome.get("error").is_some() {
                    kinds.2 += 1;
                } else if process(seed, i).3 {
                    kinds.1 += 1;
                } else {
                    kinds.0 += 1;
                }
                if verbose && (i < 5 || (outcome.get("error").is_some() && i < 400)) {
                    let (asset, _, _, _) = process(seed, i);
                    eprintln!("process {i}: {outcome}\n    {}", asset["source"]);
                }
                hashes.push(json!(hash(&outcome).expect("hash")));
            }
            let expected = json!({"suite": "SUITE-DVM-REPLAY", "item": "VER-CONF-8500", "generator": "ubos-replay",
                                  "seed": seed, "count": count, "clock": NOW,
                                  "processes": {"host_programs": kinds.0, "l2_scripts": kinds.1, "ending_in_an_error": kinds.2},
                                  "recorded_on": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
                                  "hashes": hashes});
            std::fs::write(
                &expected_path,
                serde_json::to_string_pretty(&expected).expect("json") + "\n",
            )
            .expect("write");
            eprintln!(
                "recorded {count} outcome hashes ({} host programs, {} L2 scripts, {} ending in an error) in {:.1} s",
                kinds.0,
                kinds.1,
                kinds.2,
                start.elapsed().as_secs_f64()
            );
        }
        "check" => {
            let expected: Value =
                serde_json::from_str(&std::fs::read_to_string(&expected_path).expect("read"))
                    .expect("json");
            let seed = expected["seed"].as_u64().unwrap_or(seed);
            let hashes = expected["hashes"].as_array().cloned().unwrap_or_default();
            let every: u64 = opt(&args, "--replay-every")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10);
            let mut mismatches = Vec::new();
            let mut replays = (0u64, 0u64);
            for (i, h) in hashes.iter().enumerate() {
                let i = i as u64;
                let replay = every > 0 && i.is_multiple_of(every);
                let (outcome, verdict) = run(seed, i, replay).unwrap_or_else(|e| {
                    eprintln!("process {i}: {e}");
                    std::process::exit(1);
                });
                let actual = hash(&outcome).expect("hash");
                if Some(actual.as_str()) != h.as_str() {
                    if verbose {
                        eprintln!("process {i}: expected {h}, got {actual}: {outcome}");
                    }
                    mismatches.push(
                        json!({"process": i, "expected": h, "actual": actual, "outcome": outcome}),
                    );
                }
                if let Some(v) = verdict {
                    replays.0 += 1;
                    if v == "identical" {
                        replays.1 += 1;
                    } else if verbose {
                        eprintln!("process {i}: replay {v}");
                    }
                }
            }
            let passed = mismatches.is_empty() && replays.0 == replays.1;
            let mut report = json!({"suite": "SUITE-DVM-REPLAY", "item": "VER-CONF-8500", "criterion": "EXIT-1-08",
                                    "expected": {"seed": seed, "count": hashes.len(), "recorded_on": expected["recorded_on"]},
                                    "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH, "family": std::env::consts::FAMILY},
                                    "identical": hashes.len() - mismatches.len(), "mismatches": mismatches,
                                    "replays": {"sampled": replays.0, "identical": replays.1},
                                    "passed": passed, "ms": start.elapsed().as_millis() as u64});
            let key: [u8; 32] = ubos_crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
            let body = canonical(&report).expect("canonical");
            let sig = ubos_crypto::ed25519_sign(&key, body.as_bytes());
            let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
            report["signature"] = json!({"alg": "ed25519", "key": hex(&ubos_crypto::ed25519_public(&key)), "sig": hex(&sig)});
            let text = serde_json::to_string_pretty(&report).expect("json");
            if let Some(p) = opt(&args, "--report") {
                if let Some(dir) = std::path::Path::new(&p).parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                std::fs::write(p, text + "\n").expect("write");
            }
            eprintln!(
                "replay check on {}-{}: {} of {} outcomes identical, {} of {} replays identical: {}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                report["identical"],
                hashes.len(),
                replays.1,
                replays.0,
                if passed { "pass" } else { "FAIL" }
            );
            if !passed {
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: ubos-replay record|check …");
            std::process::exit(2);
        }
    }
}
