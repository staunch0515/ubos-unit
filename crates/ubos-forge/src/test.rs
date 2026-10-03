//! `forge test` (DSN-FRG-201…204): unit, port, scenario and property tests in YAML, run
//! against an in-memory VAE built from the model and seed under the deterministic host
//! services (fixed clock, seeded entropy, no connectors: DSN-FRG-202, -203); reports as
//! JSON and JUnit XML (DSN-FRG-204).
//!
//! Test shapes (one list under `tests:`):
//!
//! ```yaml
//! - name: …            # every test
//!   kind: scenario     # steps of adapter operations with vector-style expectations
//!   clock: 2026-03-15T10:00:00.000000Z   # optional start; `advance: 30d|12h|5m|10s` steps move it
//!   steps:
//!     - {op: commit, input: {…}, expect: {match: subset, result: {…}}, label: c1}
//!     - {advance: 30d}
//! - name: …
//!   kind: unit         # a logic function
//!   asset: acme.app, function: main, args: [], expect: {result: …} | {error: CODE}
//! - name: …
//!   kind: port         # a port of a seeded object
//!   target: <object id or URI>, port: describe, params: {}, expect: {result: …} | {error: CODE}
//! - name: …
//!   kind: property     # a function over generated arguments; an L1 invariant over `result`
//!   asset: …, function: …, cases: 50, params: [{type: integer, min: 0, max: 100}], result_type: integer,
//!   invariant: "params.result >= 0"
//! ```

use crate::check::{CLOCK, seed_vae};
use crate::project::{Finding, Project};
use crate::runner::{resolve, strip, subset};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;
use ubos_dvm::host::{Context, FixedClock};
use ubos_dvm::{BpaError, canonical};

/// The outcome of one test.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name.
    pub name: String,
    /// Test kind.
    pub kind: String,
    /// File.
    pub file: String,
    /// Passed.
    pub pass: bool,
    /// Failure: expected, actual, step.
    pub failure: Option<Value>,
    /// Milliseconds.
    pub ms: u128,
    /// Steps executed.
    pub steps: usize,
}

/// The report of a run.
#[derive(Debug, Clone)]
pub struct Report {
    /// Results in order.
    pub results: Vec<TestResult>,
    /// Functions invoked per asset, for coverage.
    pub invoked: BTreeMap<String, Vec<String>>,
    /// Genesis and seed roots (DSN-FRG-203).
    pub roots: Value,
    /// Findings of the fixture build.
    pub findings: Vec<Finding>,
}

fn ctx() -> Context {
    Context {
        principal: "did:ubos:acme:01900000-0000-7000-8000-000000000002".into(),
        on_behalf_of: None,
    }
}

fn advance(now: &str, by: &str) -> Result<String, BpaError> {
    let (n, unit) = by.split_at(by.len().saturating_sub(1));
    let n: i64 = n
        .parse()
        .map_err(|_| BpaError::new("MODEL.INVALID_VALUE", "advance is <n>d|h|m|s"))?;
    let secs = match unit {
        "d" => n * 86_400,
        "h" => n * 3_600,
        "m" => n * 60,
        "s" => n,
        _ => {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "advance is <n>d|h|m|s",
            ));
        }
    };
    let t = ubos_dvm::types::time::parse_instant(now)? + chrono_secs(secs);
    Ok(ubos_dvm::types::time::format_instant(&t))
}

fn chrono_secs(secs: i64) -> ubos_dvm::types::time::Duration {
    ubos_dvm::types::time::Duration::seconds(secs)
}

/// A deterministic value for a generated parameter.
fn gen_value(spec: &Value, seed: u64, i: u64) -> Value {
    let mut z = seed ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    match spec
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("integer")
    {
        "integer" => {
            let min = spec.get("min").and_then(Value::as_i64).unwrap_or(0);
            let max = spec.get("max").and_then(Value::as_i64).unwrap_or(1000);
            json!(min + (z % ((max - min + 1).max(1)) as u64) as i64)
        }
        "boolean" => json!(z.is_multiple_of(2)),
        "decimal" => json!(format!("{}.{:02}", z % 1000, (z >> 10) % 100)),
        _ => json!(format!("t{}", z % 100_000)),
    }
}

/// Run every test of the project.
pub fn run(project: &Project) -> Result<Report, BpaError> {
    let (mut vae, findings) = seed_vae(project)?;
    let genesis_root =
        vae.call("export_state", &json!({"branch": "main", "time": 0}))?["root"].clone();
    let seed_root = vae.call("export_state", &json!({"branch": "main"}))?["root"].clone();
    let roots = json!({"genesis": genesis_root, "seed": seed_root});
    let clock: std::sync::Arc<FixedClock> = std::sync::Arc::new(FixedClock::new(CLOCK));
    {
        let s = vae.services().clone();
        let new = ubos_dvm::host::Services {
            clock: clock.clone(),
            ..s
        };
        vae.engine_mut().services = Some(new.clone());
    }
    let mut results = Vec::new();
    let mut invoked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let c = ctx();
    for item in &project.tests {
        let t = &item.value;
        let name = t
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed")
            .to_string();
        let kind = t
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("scenario")
            .to_string();
        let started = Instant::now();
        let start_clock = t
            .get("clock")
            .and_then(Value::as_str)
            .unwrap_or(CLOCK)
            .to_string();
        clock.set(&start_clock);
        let mut now = start_clock;
        let mut steps = 0;
        let outcome: Result<Option<Value>, BpaError> = (|| match kind.as_str() {
            "scenario" => {
                let mut labels: HashMap<String, Value> = HashMap::new();
                for step in t
                    .get("steps")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    steps += 1;
                    if let Some(by) = step.get("advance").and_then(Value::as_str) {
                        now = advance(&now, by)?;
                        clock.set(&now);
                        continue;
                    }
                    let op = step.get("op").and_then(Value::as_str).unwrap_or("read");
                    let input = resolve(step.get("input").unwrap_or(&json!({})), &labels)
                        .map_err(|e| BpaError::new("MODEL.INVALID_VALUE", e))?;
                    let input = if ["commit", "execute", "merge", "read", "read_ledger", "query"]
                        .contains(&op)
                    {
                        let mut i = input;
                        if i.get("principal").is_none() {
                            i["principal"] = json!(c.principal);
                        }
                        i
                    } else {
                        input
                    };
                    let out = vae.call(op, &input);
                    if op == "execute"
                        && let Ok(o) = &out
                    {
                        let _ = o;
                        note_invocation(&mut invoked, &input);
                    }
                    if let Ok(v) = &out
                        && let Some(l) = step.get("label").and_then(Value::as_str)
                    {
                        labels.insert(l.to_string(), v.clone());
                    }
                    if let Some(d) = expectation(step.get("expect"), &out, &labels, steps) {
                        return Ok(Some(d));
                    }
                }
                Ok(None)
            }
            "unit" | "port" => {
                steps = 1;
                let input = if kind == "unit" {
                    let asset = t.get("asset").and_then(Value::as_str).unwrap_or_default();
                    json!({"asset": asset, "function": t.get("function").cloned().unwrap_or(json!("main")),
                            "args": t.get("args").cloned().unwrap_or(json!([])), "branch": "feature/forge", "principal": c.principal})
                } else {
                    json!({"target": t.get("target").cloned().or_else(|| t.get("object_id").cloned()), "port": t.get("port"),
                            "params": t.get("params").cloned().unwrap_or(json!({})), "branch": "feature/forge", "principal": c.principal})
                };
                let out = vae.call("execute", &input);
                if out.is_ok() {
                    note_invocation(&mut invoked, &input);
                }
                let expect = t.get("expect").cloned().unwrap_or(json!({}));
                let d = match (&out, expect.get("error").and_then(Value::as_str)) {
                    (Err(e), Some(code)) if e.code == code => None,
                    (Err(e), _) => Some(
                        json!({"expected": expect, "actual": {"error": e.code, "message": e.message}}),
                    ),
                    (Ok(v), Some(_)) => Some(json!({"expected": expect, "actual": v})),
                    (Ok(v), None) => {
                        let want = expect.get("result").cloned().unwrap_or(Value::Null);
                        if strip(&v["result"]) == strip(&want) || subset(&want, &v["result"]) {
                            None
                        } else {
                            Some(json!({"expected": expect, "actual": v["result"]}))
                        }
                    }
                };
                Ok(d)
            }
            "property" => {
                let asset = t.get("asset").and_then(Value::as_str).unwrap_or_default();
                let cases = t.get("cases").and_then(Value::as_u64).unwrap_or(25);
                let params = t
                    .get("params")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let seed = t.get("seed").and_then(Value::as_u64).unwrap_or(1);
                for i in 0..cases {
                    steps += 1;
                    let args: Vec<Value> = params.iter().map(|p| gen_value(p, seed, i)).collect();
                    let input = json!({"asset": asset, "function": t.get("function").cloned().unwrap_or(json!("main")),
                            "args": args, "branch": "feature/forge", "principal": c.principal});
                    let out = match vae.call("execute", &input) {
                        Ok(v) => v,
                        Err(e) => {
                            return Ok(Some(
                                json!({"case": i, "args": args, "actual": {"error": e.code, "message": e.message}}),
                            ));
                        }
                    };
                    note_invocation(&mut invoked, &input);
                    if let Some(inv) = t.get("invariant").and_then(Value::as_str) {
                        let rt = t
                            .get("result_type")
                            .and_then(Value::as_str)
                            .unwrap_or("text");
                        let ok = vae.call("eval_expr", &json!({"expr": inv, "params_types": {"result": {"type": rt}}, "params": {"result": out["result"]}}))?;
                        if ok["value"] != json!(true) {
                            return Ok(Some(
                                json!({"case": i, "args": args, "invariant": inv, "result": out["result"], "evaluated": ok}),
                            ));
                        }
                    }
                }
                Ok(None)
            }
            other => Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                format!("unknown test kind {other}"),
            )),
        })();
        let (pass, failure) = match outcome {
            Ok(None) => (true, None),
            Ok(Some(d)) => (false, Some(d)),
            Err(e) => (false, Some(json!({"error": e.code, "message": e.message}))),
        };
        results.push(TestResult {
            name,
            kind,
            file: item.file.clone(),
            pass,
            failure,
            ms: started.elapsed().as_millis(),
            steps,
        });
    }
    Ok(Report {
        results,
        invoked,
        roots,
        findings,
    })
}

fn note_invocation(invoked: &mut BTreeMap<String, Vec<String>>, input: &Value) {
    if let Some(asset) = input.get("asset").and_then(Value::as_str) {
        let f = input
            .get("function")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let e = invoked.entry(asset.to_string()).or_default();
        if !e.contains(&f) {
            e.push(f);
        }
    }
}

/// Compare a step's outcome with its vector-style expectation; `Some(detail)` on a failure.
fn expectation(
    exp: Option<&Value>,
    out: &Result<Value, BpaError>,
    labels: &HashMap<String, Value>,
    step: usize,
) -> Option<Value> {
    let Some(exp) = exp else {
        return match out {
            Ok(_) => None,
            Err(e) => {
                Some(json!({"step": step, "actual": {"error": e.code, "message": e.message}}))
            }
        };
    };
    if let Some(code) = exp.get("error_code").and_then(Value::as_str) {
        return match out {
            Err(e) if e.code == code => None,
            Err(e) => Some(
                json!({"step": step, "expected": exp, "actual": {"error": e.code, "message": e.message}}),
            ),
            Ok(v) => Some(json!({"step": step, "expected": exp, "actual": v})),
        };
    }
    let v = match out {
        Ok(v) => v,
        Err(e) => {
            return Some(
                json!({"step": step, "expected": exp, "actual": {"error": e.code, "message": e.message}}),
            );
        }
    };
    if let Some(r) = exp.get("same_as").and_then(Value::as_str) {
        let same = crate::runner::pointer(labels, r).is_some_and(|x| x == v);
        return if same {
            None
        } else {
            Some(json!({"step": step, "expected": exp, "actual": v}))
        };
    }
    let want = resolve(exp.get("result").unwrap_or(&Value::Null), labels).unwrap_or(Value::Null);
    let ok = match exp.get("match").and_then(Value::as_str).unwrap_or("exact") {
        "subset" => subset(&want, v),
        _ => canonical(&strip(v)).ok() == canonical(&strip(&want)).ok(),
    };
    if ok {
        None
    } else {
        Some(json!({"step": step, "expected": want, "actual": v}))
    }
}

impl Report {
    /// The JSON report (DSN-FRG-204).
    pub fn to_json(&self, project: &Project) -> Value {
        let mut coverage = Map::new();
        for item in &project.logic {
            let name = item
                .value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let exports: Vec<String> = item.value["exports"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect();
            let hit = self
                .invoked
                .get(name)
                .map(|v| v.iter().filter(|f| exports.contains(f)).count())
                .unwrap_or(0);
            coverage.insert(name.to_string(), json!({"functions": exports.len(), "invoked": hit,
                "fraction": if exports.is_empty() { 1.0 } else { hit as f64 / exports.len() as f64 }}));
        }
        json!({
            "project": project.manifest.name, "tests": self.results.len(),
            "passed": self.results.iter().filter(|r| r.pass).count(),
            "failed": self.results.iter().filter(|r| !r.pass).count(),
            "roots": self.roots, "coverage": coverage,
            "results": self.results.iter().map(|r| json!({"name": r.name, "kind": r.kind, "file": r.file,
                "result": if r.pass { "pass" } else { "fail" }, "ms": r.ms, "steps": r.steps, "failure": r.failure})).collect::<Vec<_>>(),
            "fixture_findings": self.findings.iter().map(Finding::to_json).collect::<Vec<_>>(),
        })
    }

    /// JUnit XML (DSN-FRG-204).
    pub fn to_junit(&self, project: &Project) -> String {
        fn esc(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        }
        let failed = self.results.iter().filter(|r| !r.pass).count();
        let time: f64 = self.results.iter().map(|r| r.ms as f64 / 1000.0).sum();
        let mut x = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites tests=\"{}\" failures=\"{failed}\" time=\"{time:.3}\">\n  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failed}\" time=\"{time:.3}\">\n",
            self.results.len(),
            esc(&project.manifest.name),
            self.results.len()
        );
        for r in &self.results {
            x.push_str(&format!(
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\"",
                esc(&r.name),
                esc(&r.file),
                r.ms as f64 / 1000.0
            ));
            match &r.failure {
                None => x.push_str(" />\n"),
                Some(f) => x.push_str(&format!(
                    ">\n      <failure message=\"{}\">{}</failure>\n    </testcase>\n",
                    esc(&r.kind),
                    esc(&serde_json::to_string_pretty(f).unwrap_or_default())
                )),
            }
        }
        x.push_str("  </testsuite>\n</testsuites>\n");
        x
    }
}
