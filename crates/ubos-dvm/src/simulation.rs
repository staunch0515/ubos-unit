//! Simulation of rule and logic changes (PH-2 step 5c, DEC-114): historical port calls re-run
//! on sandbox branches with the candidate rules (STD-LGS-044, BPA 1.0-rc8, DSN-DVM-760, -761,
//! -762) and, with `include: ["logic"]`, the candidate logic (STD-LGS-074, BPA 1.0-rc11).
//!
//! A re-run is an execution in the `simulation` profile while `SIMULATING` is set: its effects
//! are dropped instead of entering the outbox, and it is not recorded among the processes of
//! the VAE, so a later simulation, `process_info` or `metering_total` does not see it. The
//! sandbox branches and their commits stay as evidence.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::cell::Cell;
use ubos_types::{BpaError, content_id};
use ubos_version::state::Repo;

thread_local! {
    static SIMULATING: Cell<bool> = const { Cell::new(false) };
}

const RULE_CLASSES: &[&str] = &["GovernanceSheet", "PropertyDecl", "DecisionTable"];
const CLASS: &str = "SimulationRun";

/// Whether the running execution is a simulated re-run.
pub(crate) fn active() -> bool {
    SIMULATING.with(Cell::get)
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn incomplete(what: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.to_string())
}

/// A historical port call: its date, the cseq of its first commit on the target, its process.
type Call = (String, Option<u64>, String);

impl Engine {
    /// The outcome of a historical process: committed with its result hash, or failed.
    fn outcome_of(&mut self, pid: &str, journal: &Value) -> Result<Value, BpaError> {
        let state = match self.store.process_rec(pid)? {
            Some(r) => s(&r, "state").unwrap_or_default().to_string(),
            None => match self.metering_record(pid) {
                Ok(m) if s(&m, "outcome") == Some("ok") => "committed".into(),
                _ => "failed".into(),
            },
        };
        Ok(if state == "committed" {
            json!({"outcome": "committed", "result_hash": journal["outputs"]["result_hash"]})
        } else {
            json!({"outcome": "failed"})
        })
    }

    /// The historical port calls of `target` in the window, by an optional port, in commit
    /// order.
    fn calls(
        &mut self,
        target: &str,
        from: Option<&str>,
        to: Option<&str>,
        port: Option<&str>,
    ) -> Result<Vec<(Call, Value)>, BpaError> {
        let mut ids = self.store.process_ids()?;
        for r in self
            .store
            .meta("runs")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
        {
            if let Some(r) = r.as_str()
                && !ids.iter().any(|x| x == r)
            {
                ids.push(r.to_string());
            }
        }
        let mut out = Vec::new();
        for pid in ids {
            let Ok(j) = self.journal_full(&pid) else {
                continue;
            };
            let inv = &j["invocation"];
            let Some(p) = s(inv, "port") else { continue };
            if s(inv, "branch").unwrap_or("main") != target || port.is_some_and(|x| x != p) {
                continue;
            }
            let when = s(&j["context"], "now")
                .and_then(|n| n.get(..10))
                .unwrap_or_default()
                .to_string();
            if from.is_some_and(|f| when.as_str() < f) || to.is_some_and(|t| when.as_str() >= t) {
                continue;
            }
            let cseq = self.store.process_rec(&pid)?.and_then(|r| {
                r["commits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|c| c[0].as_str() == Some(target))
                    .and_then(|c| c[1].as_u64())
            });
            out.push(((when, cseq, pid), j));
        }
        out.sort_by(|a, b| {
            let key = |c: &Call| (c.0.clone(), c.1.map_or(-1, |n| n as i64), c.2.clone());
            key(&a.0).cmp(&key(&b.0))
        });
        Ok(out)
    }

    /// Adapter operation `simulate` (STD-LGS-044, -074): re-run the historical port calls of
    /// a window on sandbox branches with the candidates of the source branch, compare the
    /// outcomes with the actual ones within a budget of processes, and store the run.
    pub fn simulate(&mut self, i: &Value) -> Result<Value, BpaError> {
        let source = s(i, "source").unwrap_or_default().to_string();
        let target = s(i, "target").unwrap_or("main").to_string();
        Repo::new(self.store.as_mut(), &self.registry).head(&source)?;
        let (from, to) = (s(i, "from"), s(i, "to"));
        let budget = i["budget"]["processes"].as_u64().filter(|b| *b >= 1);
        let Some(budget) = budget else {
            return Err(incomplete("a simulation has a budget of processes"));
        };
        let Some(rid) = s(i, "run_id").map(str::to_string) else {
            return Err(incomplete("run_id"));
        };
        let include = i.get("include").filter(|v| !v.is_null());
        let logic = match include {
            None => false,
            Some(v) => {
                let list = v.as_array().filter(|l| l.iter().all(|x| x == "logic"));
                let Some(list) = list else {
                    return Err(BpaError::new("MODEL.INVALID_VALUE", "include lists logic"));
                };
                !list.is_empty()
            }
        };
        let port = s(&i["select"], "port");
        let calls = self.calls(&target, from, to, port)?;
        // the candidates of the source branch
        let mut kinds: Vec<&str> = RULE_CLASSES.to_vec();
        if logic {
            kinds.push("Logic");
        }
        let mut candidates: Vec<(String, String, String, String, Value)> = Vec::new();
        {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let src = repo.state_at(&source, None)?;
            for (oid, o) in &src.objects {
                if kinds.contains(&o.class.as_str()) {
                    let body = repo.version(&o.head)?["body"].clone();
                    candidates.push((
                        oid.clone(),
                        o.class.clone(),
                        o.slug.clone(),
                        o.head.clone(),
                        body,
                    ));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        let rule_versions: Vec<&String> = candidates.iter().map(|c| &c.3).collect();
        let mut results = Vec::new();
        let mut covered: Vec<String> = Vec::new();
        let short = rid.get(..8).unwrap_or(&rid).to_string();
        for (n, ((when, cseq, pid), j)) in calls.iter().take(budget as usize).enumerate() {
            let actual = self.outcome_of(pid, j)?;
            let name = format!("sandbox/sim-{short}-{}", n + 1);
            let at = match cseq {
                Some(c) => c - 1,
                None => Repo::new(self.store.as_mut(), &self.registry).ordinal(&target, None)?,
            };
            self.other(
                "create_branch",
                &json!({"name": name, "from": target, "time": at}),
            )?;
            let changes = {
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                let st = repo.state_at(&name, None)?;
                let mut changes = Vec::new();
                for (oid, class, slug, head, body) in &candidates {
                    match st.objects.get(oid) {
                        None => changes
                            .push(json!({"op": "create", "object_id": oid, "class": class,
                                                    "slug": slug, "body": body})),
                        Some(cur) if cur.head != *head => changes.push(json!({"op": "update",
                            "object_id": oid, "base": cur.head, "body": body})),
                        _ => {}
                    }
                }
                changes
            };
            if !changes.is_empty() {
                self.commit(&json!({"branch": name, "changes": changes}))?;
            }
            let mut inv: Map<String, Value> =
                j["invocation"].as_object().cloned().unwrap_or_default();
            inv.insert("branch".into(), json!(name));
            let mut ctx = inv
                .get("ctx")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            ctx.insert("profile".into(), json!("simulation"));
            inv.insert("ctx".into(), Value::Object(ctx));
            // a re-run is a new process: the original keeps its record
            let initiator = j["context"]["principal"].clone();
            inv.insert("process".into(), json!({"initiator": initiator}));
            SIMULATING.with(|x| x.set(true));
            let out = self.execute(&Value::Object(inv));
            SIMULATING.with(|x| x.set(false));
            let sim = match out {
                Ok(o) => json!({"outcome": "committed",
                                "result_hash": content_id("ubos.chunk.v1", &o["result"])?}),
                Err(e) => json!({"outcome": "failed", "error": e.code}),
            };
            let changed = actual != sim;
            results.push(json!({"process_id": pid, "date": when, "actual": actual,
                                "simulated": sim, "changed": changed}));
            covered.push(when.clone());
        }
        let mut coverage = json!({"processes": results.len(), "selected": calls.len(),
                                  "complete": results.len() == calls.len()});
        if let (Some(f), Some(l)) = (covered.first(), covered.last()) {
            coverage["from"] = json!(f);
            coverage["to"] = json!(l);
        }
        let changed = results
            .iter()
            .filter(|r| r["changed"] == json!(true))
            .count();
        let mut body = json!({"source": source, "target": target,
                              "window": {"from": from, "to": to}, "budget": {"processes": budget},
                              "rule_versions": rule_versions, "coverage": coverage,
                              "results": results, "changed": changed});
        if let Some(cs) = i.get("change_set").filter(|v| !v.is_null()) {
            body["change_set"] = cs.clone();
        }
        self.system_commit(
            &json!({"branch": target, "changes": [{"op": "create", "object_id": rid,
            "class": CLASS, "slug": format!("sim-{short}"), "body": body}]}),
        )?;
        if let Some(cs) = s(i, "change_set") {
            let (_, _, mut cbody) = self.cs_body(cs)?;
            let mut sims = cbody
                .get("simulations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            sims.push(json!(rid));
            cbody.insert("simulations".into(), Value::Array(sims));
            self.cs_write(cs, &cbody)?;
        }
        let mut out = body;
        if let Some(m) = out.as_object_mut() {
            m.remove("rule_versions");
        }
        out["run_id"] = json!(rid);
        Ok(out)
    }
}
