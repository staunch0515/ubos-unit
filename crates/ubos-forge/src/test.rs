//! `forge test` (DSN-FRG-201…204): unit, port, scenario and property tests in YAML, run
//! against a VAE built from the model and seed under the deterministic host services (fixed
//! clock, seeded entropy, mocked connectors: DSN-FRG-202, -203), in memory or on a fresh
//! VAE of a PostgreSQL database; reports as JSON and JUnit XML (DSN-FRG-204).
//!
//! Test shapes (one list under `tests:`):
//!
//! ```yaml
//! - name: …            # every test
//!   kind: scenario     # steps of adapter operations with vector-style expectations
//!   clock: 2026-03-15T10:00:00.000000Z   # optional start; `advance: 30d|12h|5m|10s` steps move it
//!   connectors: {rate: {get: {usd: "1.10"}}}  # mocked connector responses (operation or `*`)
//!   steps:
//!     - {op: commit, input: {…}, expect: {match: subset, result: {…}}, label: c1}
//!     - {advance: 30d}
//!     - {op: released_effects, expect: {match: subset, result: {effects: [{type: BecameEffective}]}}}
//!     - {concurrent: 100, op: commit, input: {…}, expect: {match: subset, result: {ok: 1}}}
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

use crate::check::{Backend, CLOCK, seed_vae_with};
use crate::project::{Finding, Project};
use crate::runner::{resolve, strip, subset};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use ubos_dvm::host::{Connectors, Context, FixedClock, Services, Vae};
use ubos_dvm::{BpaError, canonical};

/// Mocked connectors of a test (DSN-FRG-202): `connectors: {name: {operation: response}}`
/// in the test; an operation named `*` answers every operation.
#[derive(Default)]
pub struct MockConnectors {
    responses: Mutex<BTreeMap<String, Value>>,
    /// Calls made `(connector, operation)`.
    pub calls: Mutex<Vec<(String, String)>>,
}

impl MockConnectors {
    fn set(&self, spec: Option<&Value>) {
        if let Ok(mut g) = self.responses.lock() {
            g.clear();
            for (name, ops) in spec.and_then(Value::as_object).into_iter().flatten() {
                g.insert(name.clone(), ops.clone());
            }
        }
    }
}

impl Connectors for MockConnectors {
    fn call(&self, connector: &str, operation: &str, _request: &Value) -> Result<Value, BpaError> {
        if let Ok(mut c) = self.calls.lock() {
            c.push((connector.to_string(), operation.to_string()));
        }
        let g = self
            .responses
            .lock()
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "lock"))?;
        let Some(ops) = g.get(connector) else {
            return Err(BpaError::new(
                "LOGIC.CONNECTOR_UNAVAILABLE",
                format!("no mocked connector {connector}"),
            ));
        };
        ops.get(operation)
            .or_else(|| ops.get("*"))
            .cloned()
            .ok_or_else(|| {
                BpaError::new(
                    "LOGIC.CONNECTOR_ERROR",
                    format!("{connector} has no mocked {operation}"),
                )
            })
    }
}

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
    let t =
        ubos_dvm::types::time::parse_instant(now)? + ubos_dvm::types::time::Duration::seconds(secs);
    Ok(ubos_dvm::types::time::format_instant(&t))
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

/// Run `op` `n` times at once; `{ok, errors: {code: count}, parallel}`. On PostgreSQL every
/// writer has its own adapter on the VAE; in memory the writers run one after the other.
fn concurrent(
    vae: &mut Vae,
    backend: &Backend,
    scope: Option<&ubos_dvm::store::kernel::Scope>,
    n: u64,
    op: &str,
    input: &Value,
) -> Result<Value, BpaError> {
    let mut ok = 0u64;
    let mut errors: BTreeMap<String, u64> = BTreeMap::new();
    match (&backend.database, scope) {
        (Some(url), Some(sc)) => {
            let now = vae.engine_mut().now.clone();
            // waves of writers, so that their connections stay within the database's limit
            const WAVE: u64 = 16;
            let mut started = 0;
            while started < n {
                let wave = WAVE.min(n - started);
                let handles: Vec<_> = (0..wave)
                    .map(|_| {
                        let (url, sc, op, input, now) = (
                            url.clone(),
                            sc.clone(),
                            op.to_string(),
                            input.clone(),
                            now.clone(),
                        );
                        std::thread::spawn(move || -> Result<Result<Value, BpaError>, BpaError> {
                            let mut a = ubos_dvm::Adapter::postgres_existing(
                                &url,
                                &sc.tenant,
                                &sc.vae,
                                &sc.authority,
                            )?;
                            a.engine_mut().now = now;
                            Ok(a.execute(&op, &input))
                        })
                    })
                    .collect();
                for h in handles {
                    match h.join().map_err(|_| {
                        BpaError::new("NOD.STORAGE_UNAVAILABLE", "writer thread panicked")
                    })?? {
                        Ok(_) => ok += 1,
                        Err(e) => *errors.entry(e.code.to_string()).or_default() += 1,
                    }
                }
                started += wave;
            }
            vae.engine_mut().branch_cache.clear();
            Ok(json!({"ok": ok, "errors": errors, "parallel": true}))
        }
        _ => {
            for _ in 0..n {
                match vae.call(op, input) {
                    Ok(_) => ok += 1,
                    Err(e) => *errors.entry(e.code.to_string()).or_default() += 1,
                }
            }
            Ok(json!({"ok": ok, "errors": errors, "parallel": false}))
        }
    }
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
pub(crate) fn expectation(
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
            Err(e) if e.code == code => match exp.get("details") {
                Some(d) if !e.details.as_ref().is_some_and(|x| subset(d, x)) => Some(
                    json!({"step": step, "expected": exp, "actual": {"error": e.code, "details": e.details}}),
                ),
                _ => None,
            },
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

/// Operations that carry the principal of the context.
const WITH_PRINCIPAL: &[&str] = &[
    "commit",
    "execute",
    "merge",
    "read",
    "read_ledger",
    "query",
    "bulk",
    "revert",
];

/// Run every test of the project on the given backend.
pub fn run(project: &Project, backend: &Backend) -> Result<Report, BpaError> {
    // every test runs on its own fixture built from the model and seed (DSN-FRG-203), so
    // that tests cannot see each other's state; the roots of the first fixture are reported
    let decls = project.declarations();
    let clock: Arc<FixedClock> = Arc::new(FixedClock::new(CLOCK));
    let mocks: Arc<MockConnectors> = Arc::new(MockConnectors::default());
    let fixture = |clock: &Arc<FixedClock>, mocks: &Arc<MockConnectors>| -> Result<_, BpaError> {
        let (mut vae, findings, recorder) = seed_vae_with(project, &decls, backend)?;
        let s = vae.services().clone();
        vae.set_services(Services {
            clock: clock.clone(),
            connectors: mocks.clone(),
            ..s
        });
        let scope = vae.engine_mut().store.scope();
        Ok((vae, findings, recorder, scope))
    };
    let (mut vae, findings, mut recorder, mut scope) = fixture(&clock, &mocks)?;
    let genesis_root =
        vae.call("export_state", &json!({"branch": "main", "time": 0}))?["root"].clone();
    let seed_root = vae.call("export_state", &json!({"branch": "main"}))?["root"].clone();
    let roots = json!({"genesis": genesis_root, "seed": seed_root});
    let mut results = Vec::new();
    let mut invoked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let c = ctx();
    let mut first = true;
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
        if !first {
            let (v, _, r, sc) = fixture(&clock, &mocks)?;
            vae = v;
            recorder = r;
            scope = sc;
        }
        first = false;
        let start_clock = t
            .get("clock")
            .and_then(Value::as_str)
            .unwrap_or(CLOCK)
            .to_string();
        clock.set(&start_clock);
        mocks.set(t.get("connectors"));
        if let Ok(mut g) = recorder.effects.lock() {
            g.clear();
        }
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
                        // the clock moves even without an operation: due timers fire
                        vae.call("outbox", &json!({}))?;
                        continue;
                    }
                    let op = step.get("op").and_then(Value::as_str).unwrap_or("read");
                    let mut input = resolve(step.get("input").unwrap_or(&json!({})), &labels)
                        .map_err(|e| BpaError::new("MODEL.INVALID_VALUE", e))?;
                    if WITH_PRINCIPAL.contains(&op)
                        && input.get("principal").is_none()
                        && input.is_object()
                    {
                        input["principal"] = json!(c.principal);
                    }
                    let out = if op == "released_effects" {
                        let effects: Vec<Value> = recorder
                            .effects
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        Ok(json!({"effects": effects}))
                    } else if let Some(n) = step.get("concurrent").and_then(Value::as_u64) {
                        concurrent(&mut vae, backend, scope.as_ref(), n, op, &input)
                    } else {
                        let out = vae.call(op, &input);
                        if op == "execute" && out.is_ok() {
                            note_invocation(&mut invoked, &input);
                        }
                        out
                    };
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
                        let ok = vae.call(
                            "eval_expr",
                            &json!({"expr": inv, "params_types": {"result": {"type": rt}}, "params": {"result": out["result"]}}),
                        )?;
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
            coverage.insert(
                name.to_string(),
                json!({"functions": exports.len(), "invoked": hit,
                    "fraction": if exports.is_empty() { 1.0 } else { hit as f64 / exports.len() as f64 }}),
            );
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
