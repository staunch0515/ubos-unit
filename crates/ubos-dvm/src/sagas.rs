//! Sagas and quarantine in the kernel (PH-2 step 6d, DEC-122): sagas (STD-TXN-080) with their
//! timeouts, monitoring and intervention (STD-TXN-081, BPA 1.0-rc12), and the quarantine of
//! logic asset versions (STD-CTX-031) (DSN-DVM-564, -565, DSN-NOD-230).
//!
//! A saga is a Definition object of the genesis class `Saga` whose state members only its
//! execution writes: each state transition is one kernel commit, made while `SAGA_WRITE` is
//! set. The outcome of each step process is kept in `kernel_meta` (`saga_result:<key>`), so a
//! resumed saga repeats no step whose process committed. The quarantine policy, the failure
//! counts and the quarantined versions are kept there too.

use crate::stateful::Engine;
use crate::workflows::shift;
use serde_json::{Value, json};
use std::cell::Cell;
use ubos_types::{BpaError, canonical};
use ubos_version::state::Repo;

pub(crate) const SAGA: &str = "Saga";
const QUAR: &str = "Quarantine";
const QREL: &str = "QuarantineRelease";
/// STD-TXN-080: the members of a saga that only the saga's execution writes.
const SAGA_STATE: &[&str] = &[
    "status",
    "completed",
    "compensated",
    "current",
    "error",
    "log",
    "skipped",
];
const SAGA_END: &[&str] = &["completed", "compensated", "failed"];
const MAX_STEPS: usize = 1000;
/// The internal code of a saga run stopped by `interrupt_after`.
const INTERRUPTED: &str = "ADAPTER.INTERRUPTED";

thread_local! {
    static SAGA_WRITE: Cell<bool> = const { Cell::new(false) };
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn incomplete(what: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.to_string())
}

fn invalid(what: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", what.to_string())
}

fn given(v: &Value, k: &str) -> bool {
    v.get(k).is_some_and(|x| !x.is_null())
}

/// STD-TXN-080: a change to a saga's state members outside its execution fails.
pub(crate) fn kernel_fields(class: &str, body: Option<&Value>) -> Result<(), BpaError> {
    if class == SAGA
        && !SAGA_WRITE.with(Cell::get)
        && body
            .and_then(Value::as_object)
            .is_some_and(|b| SAGA_STATE.iter().any(|k| b.contains_key(*k)))
    {
        return Err(BpaError::new(
            "FLOW.STATE_DIRECT_WRITE",
            "the state of a saga is written by its execution",
        ));
    }
    Ok(())
}

/// DAT-Saga (STD-TXN-080, -081): named steps, each a port call with an optional
/// compensation, a timeout and its handling.
pub(crate) fn check_saga(b: &Value) -> Result<(), BpaError> {
    let Some(steps) = b["steps"].as_array().filter(|a| !a.is_empty()) else {
        return Err(incomplete("a saga lists its steps"));
    };
    let mut seen = Vec::new();
    for st in steps {
        let Some(name) = s(st, "name").filter(|n| st.is_object() && !seen.contains(n)) else {
            return Err(invalid("saga steps have unique names"));
        };
        seen.push(name);
        for k in ["process", "compensation"] {
            let spec = st.get(k).filter(|v| !v.is_null());
            if spec.is_none() && k == "compensation" {
                continue;
            }
            let ok = spec.is_some_and(|x| {
                x.is_object()
                    && x["target"].is_string()
                    && x["port"].is_string()
                    && (!given(x, "params") || x["params"].is_object())
            });
            if !ok {
                return Err(incomplete(&format!(
                    "a saga step {k} is {{target, port, params}}"
                )));
            }
        }
        if given(st, "timeout") {
            let t = s(st, "timeout").ok_or_else(|| invalid("a saga step timeout is a duration"))?;
            ubos_expr::value::parse_duration(t)?;
        }
        if given(st, "on_timeout") && !matches!(s(st, "on_timeout"), Some("recover" | "escalate")) {
            return Err(invalid("on_timeout is recover or escalate"));
        }
    }
    if given(b, "operators")
        && !b["operators"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(invalid("operators are principals"));
    }
    if given(b, "recovery") && !matches!(s(b, "recovery"), Some("backward" | "forward")) {
        return Err(invalid("recovery is backward or forward"));
    }
    if given(b, "max_attempts") && b["max_attempts"].as_u64().is_none_or(|m| m < 1) {
        return Err(invalid("max_attempts is a positive integer"));
    }
    Ok(())
}

/// The sagas a commit writes are checked (STD-TXN-080).
pub(crate) fn check(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    for (_, vh) in written {
        let v = repo.version_in(tx, vh)?;
        if s(&v, "class") == Some(SAGA) && v["tombstone"] != json!(true) {
            check_saga(&v["body"])?;
        }
    }
    Ok(())
}

/// One run of `run_saga` or `saga_action`: the commits left before the simulated crash,
/// the commits made, and the initiator.
struct SagaRun {
    left: Option<u64>,
    commits: u64,
    initiator: Option<String>,
}

impl SagaRun {
    fn counted(&mut self) -> Result<(), BpaError> {
        self.commits += 1;
        if let Some(l) = self.left.as_mut() {
            *l -= 1;
            if *l == 0 {
                return Err(BpaError::new(INTERRUPTED, "interrupt_after"));
            }
        }
        Ok(())
    }
}

impl Engine {
    // ---- sagas (STD-TXN-080, -081) ----

    fn saga_body(&mut self, branch: &str, sid: &str) -> Result<(String, Value), BpaError> {
        self.live_of(branch, sid, SAGA)
    }

    fn saga_set(
        &mut self,
        branch: &str,
        sid: &str,
        head: &str,
        changes: Value,
        effects: Option<Value>,
        run: &mut SagaRun,
    ) -> Result<(), BpaError> {
        let mut process = json!({"kind": "kernel"});
        if let Some(p) = &run.initiator {
            process["initiator"] = json!(p);
        }
        if let Some(e) = effects {
            process["effects"] = e;
        }
        SAGA_WRITE.with(|x| x.set(true));
        let r = self.commit(&json!({"branch": branch, "process": process, "changes": [
            {"op": "update", "object_id": sid, "base": head, "body": changes}]}));
        SAGA_WRITE.with(|x| x.set(false));
        r?;
        run.counted()
    }

    #[allow(clippy::too_many_arguments)]
    fn saga_call(
        &mut self,
        branch: &str,
        sid: &str,
        spec: &Value,
        step: &str,
        attempt: u64,
        phase: &str,
        run: &mut SagaRun,
    ) -> Result<(String, Value), BpaError> {
        let key = format!(
            "saga_result:{}",
            canonical(&json!([sid, step, attempt, phase]))?
        );
        if let Some(r) = self.store.meta(&key)? {
            return Ok((str_of(&r[0]), r[1].clone()));
        }
        let mut proc_ = json!({"kind": "job", "saga": sid,
                               "saga_step": {"step": step, "attempt": attempt, "phase": phase}});
        if let Some(p) = &run.initiator {
            proc_["initiator"] = json!(p);
        }
        let params = spec
            .get("params")
            .filter(|p| !p.is_null())
            .cloned()
            .unwrap_or(json!({}));
        let out = self.execute(&json!({"branch": branch, "target": spec["target"],
            "port": spec["port"], "params": params, "process": proc_}));
        match out {
            Ok(o) => {
                let r = json!(["ok", o["result"]]);
                self.store.put_meta(&key, &r)?;
                if !o["cseq"].is_null() {
                    run.counted()?;
                }
                Ok(("ok".into(), o["result"].clone()))
            }
            Err(e) => {
                self.store.put_meta(&key, &json!(["error", e.code]))?;
                Ok(("error".into(), json!(e.code)))
            }
        }
    }

    /// The step in progress; a step with a timeout records when its attempt started.
    fn saga_attempt(&self, step: &Value, attempt: u64) -> Value {
        let mut cur = json!({"step": step["name"], "attempt": attempt, "phase": "forward"});
        if given(step, "timeout") {
            cur["started_at"] = json!(self.now);
        }
        cur
    }

    /// STD-TXN-081: a step whose process has not committed by the start of its attempt plus
    /// its timeout has timed out.
    fn saga_timed_out(
        &mut self,
        sid: &str,
        step: &Value,
        cur: &Value,
    ) -> Result<Option<(String, Value)>, BpaError> {
        let (Some(t), Some(started)) = (s(step, "timeout"), s(cur, "started_at")) else {
            return Ok(None);
        };
        let key = format!(
            "saga_result:{}",
            canonical(&json!([sid, step["name"], cur["attempt"], "forward"]))?
        );
        if self.store.meta(&key)?.is_some() {
            return Ok(None);
        }
        if self.now.as_str() >= shift(started, t, 1)?.as_str() {
            return Ok(Some(("timeout".into(), json!("FLOW.TIMEOUT"))));
        }
        Ok(None)
    }

    fn saga_loop(&mut self, branch: &str, sid: &str, run: &mut SagaRun) -> Result<(), BpaError> {
        for _ in 0..MAX_STEPS {
            let (head, b) = self.saga_body(branch, sid)?;
            let steps: Vec<Value> = b["steps"].as_array().cloned().unwrap_or_default();
            let step_of = |n: &str| {
                steps
                    .iter()
                    .find(|x| s(x, "name") == Some(n))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            let order: Vec<String> = steps.iter().map(|x| str_of(&x["name"])).collect();
            let status = s(&b, "status").unwrap_or("pending").to_string();
            let log: Vec<Value> = b["log"].as_array().cloned().unwrap_or_default();
            let cur = if b["current"].is_object() {
                b["current"].clone()
            } else {
                json!({})
            };
            let with = |mut l: Vec<Value>, e: Value| {
                l.push(e);
                Value::Array(l)
            };
            match status.as_str() {
                "pending" => self.saga_set(
                    branch,
                    sid,
                    &head,
                    json!({"status": "running", "completed": [], "compensated": [], "log": []}),
                    None,
                    run,
                )?,
                x if SAGA_END.contains(&x) || x == "stuck" => return Ok(()),
                "running" => {
                    let done: Vec<Value> = b["completed"].as_array().cloned().unwrap_or_default();
                    if done.len() == order.len() {
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"status": "completed", "current": null}),
                            None,
                            run,
                        )?;
                        continue;
                    }
                    let name = order[done.len()].clone();
                    let st = step_of(&name);
                    if s(&cur, "step") != Some(name.as_str()) || s(&cur, "phase") != Some("forward")
                    {
                        let c = self.saga_attempt(&st, 1);
                        self.saga_set(branch, sid, &head, json!({"current": c}), None, run)?;
                        continue;
                    }
                    let attempt = cur["attempt"].as_u64().unwrap_or(1);
                    let (kind, res) = match self.saga_timed_out(sid, &st, &cur)? {
                        Some(t) => t,
                        None => self.saga_call(
                            branch,
                            sid,
                            &st["process"],
                            &name,
                            attempt,
                            "forward",
                            run,
                        )?,
                    };
                    let mut entry = json!({"step": name, "attempt": attempt, "phase": "forward",
                                           "outcome": kind});
                    if kind == "ok" {
                        let mut d = done.clone();
                        d.push(json!(name));
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"completed": d, "current": null, "log": with(log, entry)}),
                            None,
                            run,
                        )?;
                        continue;
                    }
                    entry["code"] = res.clone();
                    // STD-TXN-081 (1.0-rc12): a step that times out may escalate
                    let code = str_of(&res);
                    if matches!(code.as_str(), "FLOW.TIMEOUT" | "LOGIC.LIMIT_EXCEEDED")
                        && s(&st, "on_timeout") == Some("escalate")
                    {
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"status": "stuck", "log": with(log, entry),
                                   "error": {"step": name, "code": res}}),
                            Some(json!([{"type": "SagaStuck", "object_id": sid,
                                         "payload": {"saga": sid, "step": name, "code": res}}])),
                            run,
                        )?;
                        continue;
                    }
                    if s(&b, "recovery") == Some("forward") {
                        if attempt < b["max_attempts"].as_u64().unwrap_or(1) {
                            let c = self.saga_attempt(&st, attempt + 1);
                            self.saga_set(
                                branch,
                                sid,
                                &head,
                                json!({"current": c, "log": with(log, entry)}),
                                None,
                                run,
                            )?;
                        } else {
                            self.saga_set(
                                branch,
                                sid,
                                &head,
                                json!({"status": "failed", "current": null, "log": with(log, entry),
                                       "error": {"step": name, "code": res, "attempts": attempt}}),
                                None,
                                run,
                            )?;
                        }
                    } else {
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"status": "compensating", "current": null,
                                   "log": with(log, entry), "error": {"step": name, "code": res}}),
                            None,
                            run,
                        )?;
                    }
                }
                "compensating" => {
                    let completed: Vec<String> = b["completed"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(str_of)
                        .collect();
                    let compensated: Vec<Value> =
                        b["compensated"].as_array().cloned().unwrap_or_default();
                    let todo: Vec<String> = completed
                        .iter()
                        .rev()
                        .filter(|n| !compensated.contains(&json!(n)))
                        .cloned()
                        .collect();
                    let Some(name) = todo.first().cloned() else {
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"status": "compensated"}),
                            None,
                            run,
                        )?;
                        continue;
                    };
                    let comp = step_of(&name)["compensation"].clone();
                    let mut comped = compensated.clone();
                    comped.push(json!(name));
                    if comp.is_null() {
                        let e = json!({"step": name, "phase": "compensate", "outcome": "none"});
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"compensated": comped, "log": with(log, e)}),
                            None,
                            run,
                        )?;
                        continue;
                    }
                    if s(&cur, "step") != Some(name.as_str())
                        || s(&cur, "phase") != Some("compensate")
                    {
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"current": {"step": name, "attempt": 1, "phase": "compensate"}}),
                            None,
                            run,
                        )?;
                        continue;
                    }
                    let attempt = cur["attempt"].as_u64().unwrap_or(1);
                    let (kind, res) =
                        self.saga_call(branch, sid, &comp, &name, attempt, "compensate", run)?;
                    let mut entry = json!({"step": name, "attempt": attempt,
                                           "phase": "compensate", "outcome": kind});
                    if kind == "ok" {
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"compensated": comped, "current": null,
                                   "log": with(log, entry)}),
                            None,
                            run,
                        )?;
                    } else {
                        entry["code"] = res.clone();
                        let mut err = if b["error"].is_object() {
                            b["error"].clone()
                        } else {
                            json!({})
                        };
                        err["compensation"] = json!({"step": name, "code": res});
                        self.saga_set(
                            branch,
                            sid,
                            &head,
                            json!({"status": "failed", "current": null, "log": with(log, entry),
                                   "error": err}),
                            None,
                            run,
                        )?;
                    }
                }
                _ => return Ok(()),
            }
        }
        Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "saga steps"))
    }

    fn saga_view(&mut self, branch: &str, sid: &str) -> Result<Value, BpaError> {
        let (_, b) = self.saga_body(branch, sid)?;
        let mut out = json!({"saga_id": sid, "status": s(&b, "status").unwrap_or("pending"),
            "completed": b.get("completed").filter(|v| !v.is_null()).cloned().unwrap_or(json!([])),
            "compensated": b.get("compensated").filter(|v| !v.is_null()).cloned().unwrap_or(json!([]))});
        if b.get("error")
            .is_some_and(|e| !e.is_null() && e != &json!({}))
        {
            out["error"] = b["error"].clone();
        }
        Ok(out)
    }

    /// Run the loop of a saga, turning the simulated crash into the `interrupted` result.
    fn saga_drive(
        &mut self,
        branch: &str,
        sid: &str,
        mut run: SagaRun,
        first: Option<(String, Value)>,
    ) -> Result<Value, BpaError> {
        let r = (|| -> Result<(), BpaError> {
            if let Some((head, changes)) = first {
                self.saga_set(branch, sid, &head, changes, None, &mut run)?;
            }
            self.saga_loop(branch, sid, &mut run)
        })();
        match r {
            Err(e) if e.code == INTERRUPTED => {
                Ok(json!({"saga_id": sid, "status": "interrupted", "commits": run.commits}))
            }
            Err(e) => Err(e),
            Ok(()) => {
                let mut out = self.saga_view(branch, sid)?;
                out["commits"] = json!(run.commits);
                Ok(out)
            }
        }
    }

    fn interrupt_after(i: &Value) -> Result<Option<u64>, BpaError> {
        match i.get("interrupt_after").filter(|v| !v.is_null()) {
            None => Ok(None),
            Some(v) => v
                .as_u64()
                .filter(|n| *n >= 1)
                .map(Some)
                .ok_or_else(|| invalid("interrupt_after is a positive integer")),
        }
    }

    /// Adapter operation `run_saga`: run or resume a saga; `interrupt_after` stops the run
    /// after that many commits, as a crash would.
    pub fn run_saga(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let now = self.now.clone();
        Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
        let (sid, _) = self.wf_ref(&branch, &i["saga"], SAGA)?;
        let left = Self::interrupt_after(i)?;
        let run = SagaRun {
            left,
            commits: 0,
            initiator: i["process"]["initiator"].as_str().map(str::to_string),
        };
        self.saga_drive(&branch, &sid, run, None)
    }

    /// Adapter operation `sagas` (STD-TXN-081): the sagas of a branch by state, age of the
    /// step in progress and step.
    pub fn sagas(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let since = match s(i, "older_than") {
            Some(d) => Some(shift(&self.now, d, -1)?),
            None => None,
        };
        let mut all = self.all_of(&branch, SAGA)?;
        all.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = Vec::new();
        for (oid, b) in all {
            let status = s(&b, "status").unwrap_or("pending");
            let cur = b.get("current").cloned().unwrap_or(Value::Null);
            let err = b.get("error").cloned().unwrap_or(Value::Null);
            if s(i, "status").is_some_and(|x| x != status) {
                continue;
            }
            if let Some(since) = &since
                && s(&cur, "started_at").is_none_or(|t| t > since.as_str())
            {
                continue;
            }
            if let Some(step) = s(i, "step")
                && s(&cur, "step") != Some(step)
                && s(&err, "step") != Some(step)
            {
                continue;
            }
            out.push(json!({"saga_id": oid, "name": b["name"], "status": status,
                            "current": cur, "error": err}));
        }
        Ok(json!({ "sagas": out }))
    }

    /// Adapter operation `saga_action` (STD-TXN-081): an operator retries or skips the step
    /// of a stuck or failed saga, or cancels the saga; the saga then continues as
    /// `run_saga` does.
    pub fn saga_action(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let now = self.now.clone();
        Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
        let (sid, head) = self.wf_ref(&branch, &i["saga"], SAGA)?;
        let action = s(i, "action").unwrap_or_default().to_string();
        if !matches!(action.as_str(), "retry" | "skip" | "cancel") {
            return Err(invalid("a saga action is retry, skip or cancel"));
        }
        let Some(principal) = s(i, "principal").map(str::to_string) else {
            return Err(incomplete("principal"));
        };
        let reason = i.get("reason").filter(|r| !r.is_null()).cloned();
        if action == "skip"
            && reason
                .as_ref()
                .and_then(Value::as_str)
                .is_none_or(|r| r.is_empty())
        {
            return Err(incomplete("a skip states its reason"));
        }
        let b = self.version_of(&head)?;
        if !b["operators"]
            .as_array()
            .is_some_and(|o| o.contains(&json!(principal)))
        {
            return Err(BpaError::new(
                "IAM.DENIED",
                "the principal is not an operator of the saga",
            ));
        }
        let status = s(&b, "status").unwrap_or("pending").to_string();
        let err = if b["error"].is_object() {
            b["error"].clone()
        } else {
            json!({})
        };
        let cur = if b["current"].is_object() {
            b["current"].clone()
        } else {
            json!({})
        };
        let forward_failed =
            status == "failed" && err.get("compensation").is_none() && given(&err, "step");
        if !(status == "stuck" || forward_failed) {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"status": status, "action": action}),
            ));
        }
        let name = s(&cur, "step")
            .or_else(|| s(&err, "step"))
            .unwrap_or_default()
            .to_string();
        let attempt = cur["attempt"]
            .as_u64()
            .or_else(|| err["attempts"].as_u64())
            .unwrap_or(1);
        let st = b["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|x| s(x, "name") == Some(name.as_str()))
            .cloned()
            .unwrap_or(Value::Null);
        let mut entry = json!({"step": name, "phase": "operator", "action": action,
                               "principal": principal});
        if let Some(r) = &reason {
            entry["reason"] = r.clone();
        }
        let mut log = b["log"].as_array().cloned().unwrap_or_default();
        log.push(entry);
        let changes = match action.as_str() {
            "retry" => {
                json!({"status": "running", "current": self.saga_attempt(&st, attempt + 1),
                       "error": null, "log": log})
            }
            "skip" => {
                let mut completed = b["completed"].as_array().cloned().unwrap_or_default();
                completed.push(json!(name));
                let mut skipped = b["skipped"].as_array().cloned().unwrap_or_default();
                skipped.push(json!({"step": name, "principal": principal, "reason": reason}));
                json!({"status": "running", "current": null, "error": null, "log": log,
                       "completed": completed, "skipped": skipped})
            }
            _ => json!({"status": "compensating", "current": null, "log": log}),
        };
        let run = SagaRun {
            left: Self::interrupt_after(i)?,
            commits: 0,
            initiator: Some(principal),
        };
        self.saga_drive(&branch, &sid, run, Some((head, changes)))
    }

    fn version_of(&mut self, vh: &str) -> Result<Value, BpaError> {
        Ok(Repo::new(self.store.as_mut(), &self.registry).version(vh)?["body"].clone())
    }

    // ---- quarantine (STD-CTX-031) ----

    /// Adapter operation `quarantine_policy`: the threshold, window and releasers of the VAE.
    pub fn quarantine_policy(&mut self, i: &Value) -> Result<Value, BpaError> {
        if i["threshold"].as_u64().is_none_or(|t| t < 1) {
            return Err(invalid("threshold is a positive integer"));
        }
        let Some(window) = s(i, "window") else {
            return Err(incomplete("window"));
        };
        ubos_expr::value::parse_duration(window)?;
        let Some(mut rel) = i["releasers"]
            .as_array()
            .filter(|a| !a.is_empty() && a.iter().all(Value::is_string))
            .map(|a| a.iter().map(str_of).collect::<Vec<_>>())
        else {
            return Err(incomplete("releasers are principals"));
        };
        rel.sort();
        let p = json!({"threshold": i["threshold"], "window": window, "releasers": rel});
        self.store.put_meta("qpolicy", &p)?;
        Ok(p)
    }

    fn meta_of(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        Ok(self.store.meta(key)?.filter(|v| !v.is_null()))
    }

    /// STD-CTX-031: a quarantined asset version fails fast.
    pub(crate) fn q_check(&mut self, vh: &str) -> Result<(), BpaError> {
        if let Some(qid) = self.meta_of(&format!("quarantined:{vh}"))? {
            return Err(BpaError::with_details(
                "LOGIC.ASSET_QUARANTINED",
                json!({"asset_version": vh, "quarantine": qid}),
            ));
        }
        Ok(())
    }

    /// STD-CTX-031: count a failure or limit termination of the process's entry asset
    /// version and quarantine the version when the count in the window reaches the threshold.
    pub(crate) fn q_failure(
        &mut self,
        branch: &str,
        logic: &[String],
        code: &str,
        outcome: &str,
    ) -> Result<(), BpaError> {
        let Some(policy) = self.meta_of("qpolicy")? else {
            return Ok(());
        };
        let Some(vh) = logic.first() else {
            return Ok(());
        };
        if !matches!(outcome, "error" | "limit")
            || code == "LOGIC.ASSET_QUARANTINED"
            || code.starts_with("TXN.")
        {
            return Ok(());
        }
        if self.meta_of(&format!("quarantined:{vh}"))?.is_some() {
            return Ok(());
        }
        let window = str_of(&policy["window"]);
        let since = shift(&self.now, &window, -1)?;
        let fkey = format!("qfailures:{vh}");
        let mut q: Vec<Value> = self
            .meta_of(&fkey)?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|x| x[0].as_str().is_some_and(|t| t > since.as_str()))
            .collect();
        q.push(json!([self.now, outcome]));
        self.store.put_meta(&fkey, &Value::Array(q.clone()))?;
        if (q.len() as u64) < policy["threshold"].as_u64().unwrap_or(u64::MAX) {
            return Ok(());
        }
        let ckey = format!("qcount:{vh}");
        let n = self.meta_of(&ckey)?.and_then(|v| v.as_u64()).unwrap_or(0);
        self.store.put_meta(&ckey, &json!(n + 1))?;
        let qid = crate::matching::derived_uuid(&json!(["ubos.quarantine", vh, n]))?;
        let asset = s(
            &Repo::new(self.store.as_mut(), &self.registry).version(vh)?,
            "object_id",
        )
        .unwrap_or_default()
        .to_string();
        let count = |k: &str| q.iter().filter(|x| x[1] == json!(k)).count();
        let body = json!({"asset": asset, "asset_version": vh, "failures": count("error"),
            "limit_terminations": count("limit"), "window": window, "threshold": policy["threshold"],
            "since": self.now, "last_error": code});
        self.system_commit(&json!({"branch": branch, "changes": [
            {"op": "create", "object_id": qid, "class": QUAR, "slug": format!("q-{qid}"), "body": body}],
            "process": {"kind": "kernel", "effects": [{"type": "AssetQuarantined", "object_id": qid,
                "payload": {"asset_version": vh, "failures": q.len()}}]}}))?;
        self.store
            .put_meta(&format!("quarantined:{vh}"), &json!(qid))
    }

    /// Adapter operation `release_quarantine` (STD-CTX-031): an authorised principal
    /// releases a quarantined asset version.
    pub fn release_quarantine(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let (qid, head) = self.wf_ref(&branch, &i["quarantine"], QUAR)?;
        let q = self.version_of(&head)?;
        let vh = str_of(&q["asset_version"]);
        if self.meta_of(&format!("quarantined:{vh}"))? != Some(json!(qid)) {
            return Err(BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "the quarantine is released",
            ));
        }
        let principal = s(i, "principal").map(str::to_string);
        let reason = s(i, "reason").filter(|r| !r.is_empty()).map(str::to_string);
        let (Some(reason), Some(rid)) = (reason, s(i, "release_id").map(str::to_string)) else {
            return Err(incomplete("a release states its release_id and reason"));
        };
        let allowed = self.meta_of("qpolicy")?.is_some_and(|p| {
            principal.as_ref().is_some_and(|x| {
                p["releasers"]
                    .as_array()
                    .is_some_and(|r| r.contains(&json!(x)))
            })
        });
        let Some(principal) = principal.filter(|_| allowed) else {
            return Err(BpaError::new(
                "IAM.DENIED",
                "the principal may not release a quarantine",
            ));
        };
        let r = self.system_commit(&json!({"branch": branch, "changes": [
            {"op": "create", "object_id": rid, "class": QREL, "slug": format!("r-{rid}"),
             "body": {"quarantine": qid, "asset_version": vh, "principal": principal,
                      "reason": reason, "released_at": self.now}}],
            "process": {"kind": "kernel", "initiator": principal}}))?;
        self.store
            .put_meta(&format!("quarantined:{vh}"), &Value::Null)?;
        self.store
            .put_meta(&format!("qfailures:{vh}"), &json!([]))?;
        Ok(json!({"quarantine": qid, "asset_version": vh, "released": true, "cseq": r["cseq"]}))
    }
}
