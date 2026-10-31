//! Workflows in the kernel (PH-2 step 6b, DEC-120): workflow definitions and instances
//! (STD-TXN-090), events, timers and human tasks (STD-TXN-091) and migration (STD-TXN-092)
//! of BPA 1.0-rc8 (DSN-DVM-750, -752, -753).
//!
//! A definition is a Definition object of the genesis class `WorkflowDefinition`; an instance
//! is a System object of `WorkflowInstance`, pinned to the definition version current when it
//! started and advanced only here, one kernel-written commit per step. The kernel's own state
//! lives in `kernel_meta`: the instances in start order (`wf_instances`), the outcome of each
//! action attempt (`wf_result:<key>`, so that a step is not run twice), the delivered events
//! (`wf_event:<id>`) and the events buffered per branch (`wf_buffer:<branch>`).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use ubos_types::{BpaError, canonical, content_id};
use ubos_version::state::Repo;

pub(crate) const WF_DEF: &str = "WorkflowDefinition";
pub(crate) const WF_INST: &str = "WorkflowInstance";
const STEP_TYPES: &[&str] = &[
    "action",
    "wait_event",
    "timer",
    "human_task",
    "decision",
    "parallel",
    "join",
    "sub_workflow",
    "end",
];
const MAX_STEPS: usize = 1000;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn incomplete(what: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.to_string())
}

fn invalid(what: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", what.to_string())
}

/// `now` shifted by a duration, forwards or backwards.
pub(crate) fn shift(now: &str, duration: &str, sign: i64) -> Result<String, BpaError> {
    let us = ubos_expr::value::parse_duration(duration)?;
    let us = i64::try_from(us).unwrap_or(i64::MAX).saturating_mul(sign);
    let t = ubos_types::time::parse_instant(now)? + chrono::Duration::microseconds(us);
    Ok(ubos_types::time::format_instant(&t))
}

fn duration(v: &Value, what: &str) -> Result<(), BpaError> {
    let d = v.as_str().ok_or_else(|| invalid(what))?;
    ubos_expr::value::parse_duration(d)?;
    Ok(())
}

fn is_int(v: &Value) -> bool {
    v.is_i64() || v.is_u64()
}

/// DAT-WorkflowDefinition (STD-TXN-090, and the step members of STD-TXN-093, -094): typed
/// variables, a start step and steps whose successors exist.
pub(crate) fn check_definition(d: &Value) -> Result<(), BpaError> {
    let vs = d.get("variables").filter(|v| !v.is_null());
    let vars: Vec<Value> = match vs {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .filter(|a| {
                a.iter()
                    .all(|x| x.is_object() && x["name"].is_string() && x["type"].is_string())
            })
            .cloned()
            .ok_or_else(|| incomplete("variables are {name, type} items"))?,
    };
    let names: Vec<&str> = vars.iter().filter_map(|v| s(v, "name")).collect();
    let Some(steps) = d
        .get("steps")
        .and_then(Value::as_object)
        .filter(|m| !m.is_empty())
    else {
        return Err(incomplete("steps"));
    };
    if !s(d, "start").is_some_and(|x| steps.contains_key(x)) {
        return Err(invalid("start names a step"));
    }
    let succ = |st: &Value, k: &str, required: bool| -> Result<(), BpaError> {
        let v = st.get(k).filter(|v| !v.is_null());
        if v.is_none() && !required {
            return Ok(());
        }
        if !v
            .and_then(Value::as_str)
            .is_some_and(|x| steps.contains_key(x))
        {
            return Err(invalid(&format!("{k} names a step")));
        }
        Ok(())
    };
    let given = |st: &Value, k: &str| st.get(k).is_some_and(|v| !v.is_null());
    for (name, st) in steps {
        let t = s(st, "type")
            .filter(|t| STEP_TYPES.contains(t))
            .ok_or_else(|| {
                invalid(&format!(
                    "step {name}: the type is one of {}",
                    STEP_TYPES.join(", ")
                ))
            })?;
        if given(st, "result") && !s(st, "result").is_some_and(|r| names.contains(&r)) {
            return Err(invalid(&format!("step {name}: result names a variable")));
        }
        if matches!(
            t,
            "action" | "wait_event" | "timer" | "human_task" | "join" | "sub_workflow"
        ) {
            succ(st, "next", true)?;
        }
        match t {
            "action" => {
                if s(st, "target").is_none() || s(st, "port").is_none() {
                    return Err(incomplete(&format!(
                        "step {name}: an action names target and port"
                    )));
                }
                if given(st, "params") && !st["params"].is_object() {
                    return Err(invalid(&format!(
                        "step {name}: params are a map of expressions"
                    )));
                }
                if given(st, "retries") && !st["retries"].is_u64() {
                    return Err(invalid(&format!(
                        "step {name}: retries is a non-negative integer"
                    )));
                }
                succ(st, "on_error", false)?;
                if given(st, "defer") && !st["defer"].is_boolean() {
                    return Err(invalid(&format!("step {name}: defer is a boolean")));
                }
                succ(st, "on_reject", false)?;
            }
            "wait_event" => {
                if s(st, "event_type").is_none() || s(st, "correlation").is_none() {
                    return Err(incomplete(&format!(
                        "step {name}: a wait names event_type and correlation"
                    )));
                }
                if given(st, "buffer") {
                    duration(&st["buffer"], &format!("step {name}: buffer is a duration"))?;
                }
            }
            "timer" => {
                let cals = st.get("calendars").filter(|v| !v.is_null());
                if cals.is_some_and(|c| {
                    !c.as_object()
                        .is_some_and(|m| m.values().all(Value::is_object))
                }) {
                    return Err(invalid(&format!(
                        "step {name}: calendars map names to {{weekend, holidays}}"
                    )));
                }
                if given(st, "after") == given(st, "at") {
                    return Err(incomplete(&format!(
                        "step {name}: a timer names after or at"
                    )));
                }
                if given(st, "at") {
                    if !st["at"].is_string() {
                        return Err(invalid(&format!("step {name}: at is an expression")));
                    }
                } else {
                    duration(&st["after"], &format!("step {name}: after is a duration"))?;
                }
            }
            "human_task" => {
                if given(st, "assignee") && !st["assignee"].is_string() {
                    return Err(invalid(&format!("step {name}: assignee is a principal")));
                }
                for k in ["queue", "task_type", "subject"] {
                    if given(st, k) && !st[k].is_string() {
                        return Err(invalid(&format!("step {name}: {k} is text")));
                    }
                }
                if given(st, "priority") && !is_int(&st["priority"]) {
                    return Err(invalid(&format!("step {name}: priority is an integer")));
                }
                if given(st, "data")
                    && !st["data"].as_array().is_some_and(|a| {
                        a.iter()
                            .all(|x| x.as_str().is_some_and(|x| names.contains(&x)))
                    })
                {
                    return Err(invalid(&format!("step {name}: data lists variables")));
                }
                succ(st, "on_cancel", false)?;
            }
            "decision" => {
                succ(st, "default", true)?;
                if given(st, "table") {
                    if !st["inputs"].is_object()
                        || !st["output"].is_string()
                        || !st["branches"].is_object()
                    {
                        return Err(incomplete(&format!(
                            "step {name}: a table decision names inputs, output and branches"
                        )));
                    }
                    for nxt in st["branches"].as_object().into_iter().flatten() {
                        if !nxt.1.as_str().is_some_and(|x| steps.contains_key(x)) {
                            return Err(invalid(&format!("step {name}: a branch names a step")));
                        }
                    }
                } else {
                    let ok = st["cases"].as_array().is_some_and(|cs| {
                        !cs.is_empty() && cs.iter().all(|c| c.is_object() && c["when"].is_string())
                    });
                    if !ok {
                        return Err(incomplete(&format!(
                            "step {name}: a decision has cases {{when, next}} or a table"
                        )));
                    }
                    for c in st["cases"].as_array().into_iter().flatten() {
                        succ(c, "next", true)?;
                    }
                }
            }
            "parallel" => {
                let ok = st["branches"].as_array().is_some_and(|b| {
                    b.len() >= 2
                        && b.iter()
                            .all(|x| x.as_str().is_some_and(|x| steps.contains_key(x)))
                });
                if !ok {
                    return Err(invalid(&format!(
                        "step {name}: a parallel step names two or more branch steps"
                    )));
                }
                let join = s(st, "join").and_then(|j| steps.get(j));
                if join.is_none_or(|j| s(j, "type") != Some("join")) {
                    return Err(invalid(&format!(
                        "step {name}: a parallel step names its join step"
                    )));
                }
            }
            "sub_workflow" => {
                if s(st, "definition").is_none() {
                    return Err(incomplete(&format!(
                        "step {name}: a sub-workflow names its definition"
                    )));
                }
                for k in ["inputs", "outputs"] {
                    if given(st, k) && !st[k].is_object() {
                        return Err(invalid(&format!("step {name}: {k} are a map")));
                    }
                }
            }
            "end" => {
                if given(st, "outcome") && !matches!(s(st, "outcome"), Some("completed" | "failed"))
                {
                    return Err(invalid(&format!(
                        "step {name}: the outcome is completed or failed"
                    )));
                }
            }
            _ => {}
        }
        if matches!(t, "wait_event" | "human_task")
            && (given(st, "timeout") || given(st, "on_timeout"))
        {
            if !given(st, "timeout") || !given(st, "on_timeout") {
                return Err(incomplete(&format!(
                    "step {name}: a timeout names on_timeout"
                )));
            }
            duration(
                &st["timeout"],
                &format!("step {name}: timeout is a duration"),
            )?;
            succ(st, "on_timeout", true)?;
        }
    }
    Ok(())
}

/// The workflow definitions a commit writes are checked (STD-TXN-090).
pub(crate) fn check(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    for (_, vh) in written {
        let v = repo.version_in(tx, vh)?;
        if s(&v, "class") == Some(WF_DEF) && v["tombstone"] != json!(true) {
            check_definition(&v["body"])?;
        }
    }
    Ok(())
}

/// A due workflow timer: due instant, branch, instance, token and the token's visit
/// sequence when it began to wait.
pub(crate) type WfTimer = (String, String, String, String, u64);

/// DSN-DVM-751 (PH-2 step 6c): the timer rows of the workflow instances a commit writes,
/// one per token that waits for a due time, written with the commit. The key carries
/// `wf:<token>:<seq>` in place of a version, which every store keeps.
pub(crate) fn timer_rows(
    repo: &mut Repo,
    tx: &ubos_version::state::Tx,
    branch: &str,
    versions: &Value,
) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for (oid, vh) in versions.as_object().into_iter().flatten() {
        let v = repo.version_in(tx, vh.as_str().unwrap_or_default())?;
        if s(&v, "class") != Some(WF_INST) || s(&v["body"], "status") != Some("running") {
            continue;
        }
        for t in v["body"]["tokens"].as_array().into_iter().flatten() {
            if let Some(due) = s(&t["wait"], "due") {
                let mark = format!("wf:{}:{}", str_of(&t["id"]), t["seq"].as_u64().unwrap_or(0));
                out.push(
                    json!({"key": [branch, oid, due, mark], "class": WF_INST, "instant": true}),
                );
            }
        }
    }
    Ok(out)
}

/// The token and visit sequence of a workflow timer row's mark, `wf:<token>:<seq>`.
pub(crate) fn timer_mark(mark: &str) -> Option<(String, u64)> {
    let rest = mark.strip_prefix("wf:")?;
    let (tid, seq) = rest.rsplit_once(':')?;
    Some((tid.to_string(), seq.parse().ok()?))
}

/// The token of an instance body with `id`.
fn tok_mut<'a>(body: &'a mut Value, id: &str) -> &'a mut Value {
    let toks = body["tokens"].as_array_mut().expect("tokens");
    let k = toks
        .iter()
        .position(|t| s(t, "id") == Some(id))
        .expect("token");
    &mut toks[k]
}

fn remove_tok(body: &mut Value, id: &str) {
    if let Some(toks) = body["tokens"].as_array_mut() {
        toks.retain(|t| s(t, "id") != Some(id));
    }
}

/// A new visit of `step` by a token (the visit counter keeps steps in loops apart).
fn visit(body: &mut Value, id: &str, step: &str) {
    let n = body["visits"][step].as_u64().unwrap_or(0) + 1;
    body["visits"][step] = json!(n);
    let seq = body["seq"].as_u64().unwrap_or(0) + 1;
    body["seq"] = json!(seq);
    let t = tok_mut(body, id);
    t["step"] = json!(step);
    t["visit"] = json!(n);
    t["attempt"] = json!(1);
    t["seq"] = json!(seq);
    if let Some(m) = t.as_object_mut() {
        m.remove("wait");
    }
}

fn new_token(body: &mut Value, step: &str, forks: Vec<Value>) -> String {
    let n = body["next_token"].as_u64().unwrap_or(1);
    body["next_token"] = json!(n + 1);
    let id = format!("t{n}");
    body["tokens"]
        .as_array_mut()
        .expect("tokens")
        .push(json!({"id": id, "step": step, "attempt": 1, "forks": forks}));
    visit(body, &id, step);
    id
}

fn log(body: &mut Value, tok: &Value, outcome: &str, extra: Value) {
    let mut e = json!({"step": tok["step"], "visit": tok["visit"], "token": tok["id"],
                       "outcome": outcome});
    if let (Some(m), Some(x)) = (e.as_object_mut(), extra.as_object()) {
        for (k, v) in x {
            m.insert(k.clone(), v.clone());
        }
    }
    body["log"].as_array_mut().expect("log").push(e);
}

fn end(body: &mut Value, status: &str, error: Option<Value>) {
    body["status"] = json!(status);
    body["tokens"] = json!([]);
    if let Some(e) = error {
        body["error"] = e;
    }
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

impl Engine {
    // ---- kernel state ----

    fn wf_instances(&mut self) -> Result<Vec<(String, String)>, BpaError> {
        Ok(self
            .store
            .meta("wf_instances")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|x| (str_of(&x[0]), str_of(&x[1])))
            .collect())
    }

    fn wf_result(&mut self, key: &str) -> Result<Option<Value>, BpaError> {
        self.store.meta(&format!("wf_result:{key}"))
    }

    // ---- objects ----

    /// The object of `class` that `r` names: an object ID or a canonical URI.
    fn wf_ref(
        &mut self,
        branch: &str,
        r: &Value,
        class: &str,
    ) -> Result<(String, String), BpaError> {
        let unknown = || BpaError::new("IAM.NOT_FOUND", format!("unknown {class}"));
        let Some(r) = r.as_str() else {
            return Err(unknown());
        };
        let oid = if r.starts_with("ubos://") {
            let parts: Vec<&str> = r.split('/').collect();
            let (Some(c), Some(slug)) = (parts.get(3), parts.get(4)) else {
                return Err(unknown());
            };
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let cseq = repo.ordinal(branch, None)?;
            repo.object_by_slug(branch, cseq, c, slug)?
                .map(|(id, _)| id)
                .ok_or_else(unknown)?
        } else {
            r.to_string()
        };
        let (head, _) = self.live_of(branch, &oid, class).map_err(|_| unknown())?;
        Ok((oid, head))
    }

    fn version_body(&mut self, vh: &str) -> Result<Value, BpaError> {
        Ok(Repo::new(self.store.as_mut(), &self.registry).version(vh)?["body"].clone())
    }

    fn wf_body(&mut self, branch: &str, iid: &str) -> Result<(String, Value), BpaError> {
        self.live_of(branch, iid, WF_INST)
            .map_err(|_| BpaError::new("IAM.NOT_FOUND", "unknown workflow instance"))
    }

    fn wf_save(
        &mut self,
        branch: &str,
        iid: &str,
        head: &str,
        body: &Value,
        initiator: Option<&str>,
    ) -> Result<(), BpaError> {
        self.kernel_write(
            branch,
            vec![json!({"op": "update", "object_id": iid, "base": head, "body": body})],
            initiator,
        )?;
        Ok(())
    }

    // ---- expressions and variables ----

    /// An L1 expression of a workflow over its variables (`params`).
    fn wx(&self, d: &Value, text: &str, vars: &Value) -> Result<Value, BpaError> {
        let mut types = Map::new();
        for v in d["variables"].as_array().into_iter().flatten() {
            types.insert(str_of(&v["name"]), v.clone());
        }
        let mut params = Map::new();
        for (k, v) in vars.as_object().into_iter().flatten() {
            if !v.is_null() && types.contains_key(k) {
                params.insert(k.clone(), v.clone());
            }
        }
        let out = ubos_expr::eval_expr(
            &json!({"expr": text, "params_types": types, "params": params}),
            None,
        )?;
        Ok(out["value"].clone())
    }

    fn wf_set(d: &Value, vars: &mut Value, name: &str, value: &Value) -> Result<(), BpaError> {
        let f = d["variables"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|v| s(v, "name") == Some(name))
            .cloned()
            .ok_or_else(|| invalid("unknown variable"))?;
        vars[name] = if value.is_null() {
            Value::Null
        } else {
            ubos_types::values::validate_value(
                s(&f, "type").unwrap_or_default(),
                f.get("constraints").filter(|c| !c.is_null()),
                value,
            )?
        };
        Ok(())
    }

    fn wf_vars(d: &Value, given: &Value) -> Result<Value, BpaError> {
        let Some(g) = given.as_object() else {
            return Err(invalid("variables are a map"));
        };
        let mut decl: Vec<Value> = d["variables"].as_array().cloned().unwrap_or_default();
        if g.keys()
            .any(|k| !decl.iter().any(|v| s(v, "name") == Some(k.as_str())))
        {
            return Err(invalid("unknown variable"));
        }
        decl.sort_by(|a, b| str_of(&a["name"]).cmp(&str_of(&b["name"])));
        let mut out = json!({});
        for v in &decl {
            let name = str_of(&v["name"]);
            let val = match g.get(&name) {
                Some(x) => x.clone(),
                None => v.get("default").cloned().unwrap_or(Value::Null),
            };
            Self::wf_set(d, &mut out, &name, &val)?;
        }
        Ok(out)
    }

    // ---- instances (STD-TXN-090) ----

    /// Adapter operation `start_workflow`: start an instance pinned to the definition's
    /// current version and advance it until every token waits or it ends.
    pub fn start_workflow(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let now = self.now.clone();
        Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
        let initiator = i["process"]["initiator"].as_str().map(str::to_string);
        let vars = i
            .get("variables")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(json!({}));
        let iid = self.wf_create(
            &branch,
            &i["definition"],
            &i["instance_id"],
            &vars,
            initiator.as_deref(),
            None,
        )?;
        self.wf_run(&branch, &iid)?;
        self.wf_view(&branch, &iid)
    }

    fn wf_create(
        &mut self,
        branch: &str,
        r: &Value,
        iid: &Value,
        variables: &Value,
        initiator: Option<&str>,
        parent: Option<Value>,
    ) -> Result<String, BpaError> {
        let (doid, dv) = self.wf_ref(branch, r, WF_DEF)?;
        let d = self.version_body(&dv)?;
        check_definition(&d)?;
        let Some(iid) = iid.as_str().map(str::to_string) else {
            return Err(incomplete("instance_id"));
        };
        let vars = Self::wf_vars(&d, variables)?;
        let mut body = json!({"definition": doid, "definition_version": dv, "status": "running",
            "variables": vars, "tokens": [], "forks": {}, "visits": {}, "done": {}, "log": [],
            "next_token": 1, "initiator": initiator.unwrap_or(&self.principal), "migrations": []});
        if let Some(p) = parent {
            body["parent"] = p;
        }
        new_token(&mut body, s(&d, "start").unwrap_or_default(), Vec::new());
        self.kernel_write(
            branch,
            vec![json!({"op": "create", "object_id": iid, "class": WF_INST,
                        "slug": format!("wf-{iid}"), "body": body})],
            None,
        )?;
        let mut all = self
            .store
            .meta("wf_instances")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        all.push(json!([branch, iid]));
        self.store.put_meta("wf_instances", &Value::Array(all))?;
        Ok(iid)
    }

    /// Advance an instance until each of its tokens waits or it ends; every step is one
    /// committed state change.
    fn wf_run(&mut self, branch: &str, iid: &str) -> Result<(), BpaError> {
        let mut ended = None;
        for _ in 0..MAX_STEPS {
            let (head, body) = self.wf_body(branch, iid)?;
            if s(&body, "status") != Some("running") {
                ended = Some(body);
                break;
            }
            let Some(tok) = body["tokens"]
                .as_array()
                .and_then(|t| t.iter().find(|t| t.get("wait").is_none()))
                .map(|t| str_of(&t["id"]))
            else {
                return Ok(());
            };
            self.wf_step(branch, iid, &head, body, &tok)?;
        }
        let Some(body) = ended else {
            return Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "workflow steps"));
        };
        if body.get("parent").is_some_and(|p| !p.is_null()) {
            self.wf_child_done(branch, iid, &body)?;
        }
        Ok(())
    }

    fn wf_step(
        &mut self,
        branch: &str,
        iid: &str,
        head: &str,
        mut body: Value,
        tid: &str,
    ) -> Result<(), BpaError> {
        let d = self.version_body(&str_of(&body["definition_version"]))?;
        let since = Repo::new(self.store.as_mut(), &self.registry).ordinal(branch, None)? + 1;
        let tok = tok_mut(&mut body, tid).clone();
        let step = str_of(&tok["step"]);
        let st = d["steps"][&step].clone();
        let vars = body["variables"].clone();
        match s(&st, "type").unwrap_or_default() {
            "action" => {
                let key = canonical(&json!([iid, tid, step, tok["visit"], tok["attempt"]]))?;
                let res = match self.wf_result(&key)? {
                    Some(r) => r,
                    None => {
                        let mut params = Map::new();
                        for (k, e) in st["params"].as_object().into_iter().flatten() {
                            params.insert(
                                k.clone(),
                                self.wx(&d, e.as_str().unwrap_or_default(), &vars)?,
                            );
                        }
                        let proc_ = json!({"kind": "job", "initiator": body["initiator"],
                            "workflow": {"instance": iid, "step": step, "visit": tok["visit"],
                                         "attempt": tok["attempt"]}});
                        let r = match self
                            .execute(&json!({"branch": branch, "target": st["target"],
                            "port": st["port"], "params": params, "process": proc_}))
                        {
                            Ok(out) => json!(["ok", out["result"]]),
                            Err(e) => json!(["error", e.code]),
                        };
                        self.store.put_meta(&format!("wf_result:{key}"), &r)?;
                        r
                    }
                };
                if res[0] == "ok" {
                    if let Some(rv) = s(&st, "result") {
                        let mut vars = body["variables"].clone();
                        Self::wf_set(&d, &mut vars, rv, &res[1])?;
                        body["variables"] = vars;
                    }
                    log(&mut body, &tok, "ok", json!({"attempt": tok["attempt"]}));
                    visit(&mut body, tid, s(&st, "next").unwrap_or_default());
                } else {
                    log(
                        &mut body,
                        &tok,
                        "error",
                        json!({"attempt": tok["attempt"], "code": res[1]}),
                    );
                    let attempt = tok["attempt"].as_u64().unwrap_or(1);
                    if attempt <= st["retries"].as_u64().unwrap_or(0) {
                        tok_mut(&mut body, tid)["attempt"] = json!(attempt + 1);
                    } else if let Some(h) = s(&st, "on_error") {
                        visit(&mut body, tid, h);
                    } else {
                        end(
                            &mut body,
                            "failed",
                            Some(json!({"step": step, "code": res[1]})),
                        );
                    }
                }
            }
            "decision" => {
                let nxt = if st.get("table").is_some_and(|t| !t.is_null()) {
                    let mut inputs = Map::new();
                    for (k, e) in st["inputs"].as_object().into_iter().flatten() {
                        inputs.insert(
                            k.clone(),
                            self.wx(&d, e.as_str().unwrap_or_default(), &vars)?,
                        );
                    }
                    let out = self.decide_table(
                        &json!({"branch": branch, "table": st["table"], "inputs": inputs}),
                    )?;
                    let value = &out["outputs"][s(&st, "output").unwrap_or_default()];
                    let k = match value.as_str() {
                        Some(x) => x.to_string(),
                        None => canonical(value)?,
                    };
                    st["branches"]
                        .get(&k)
                        .and_then(Value::as_str)
                        .or_else(|| s(&st, "default"))
                        .unwrap_or_default()
                        .to_string()
                } else {
                    let mut nxt = str_of(&st["default"]);
                    for c in st["cases"].as_array().into_iter().flatten() {
                        if self.wx(&d, s(c, "when").unwrap_or_default(), &vars)? == json!(true) {
                            nxt = str_of(&c["next"]);
                            break;
                        }
                    }
                    nxt
                };
                log(&mut body, &tok, "ok", json!({"next": nxt}));
                visit(&mut body, tid, &nxt);
            }
            "parallel" => {
                let fid = format!("{tid}/{}", tok["visit"]);
                let branches: Vec<String> = st["branches"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(str_of)
                    .collect();
                body["forks"][&fid] = json!({"join": st["join"], "n": branches.len(),
                                             "arrived": [], "fired": false});
                log(&mut body, &tok, "ok", json!({"fork": fid}));
                remove_tok(&mut body, tid);
                let mut forks = tok["forks"].as_array().cloned().unwrap_or_default();
                forks.push(json!(fid));
                for b in branches {
                    new_token(&mut body, &b, forks.clone());
                }
            }
            "join" => {
                let forks = tok["forks"].as_array().cloned().unwrap_or_default();
                let fid = forks.last().map(str_of);
                remove_tok(&mut body, tid);
                let fork = fid.as_ref().and_then(|f| body["forks"].get(f)).cloned();
                match fork {
                    Some(f) if s(&f, "join") == Some(step.as_str()) => {
                        let fid = fid.unwrap_or_default();
                        body["forks"][&fid]["arrived"]
                            .as_array_mut()
                            .expect("arrived")
                            .push(json!(tid));
                        log(&mut body, &tok, "arrived", json!({"fork": fid}));
                        let fk = &body["forks"][&fid];
                        let n = fk["arrived"].as_array().map_or(0, Vec::len) as u64;
                        if Some(n) == fk["n"].as_u64() && fk["fired"] != json!(true) {
                            body["forks"][&fid]["fired"] = json!(true);
                            let outer = forks[..forks.len() - 1].to_vec();
                            new_token(&mut body, s(&st, "next").unwrap_or_default(), outer);
                        }
                    }
                    _ => end(
                        &mut body,
                        "failed",
                        Some(json!({"step": step, "code": "MODEL.INVALID_VALUE"})),
                    ),
                }
            }
            "end" => {
                let outcome = s(&st, "outcome").unwrap_or("completed").to_string();
                log(&mut body, &tok, &outcome, json!({}));
                end(&mut body, &outcome, None);
            }
            "wait_event" => {
                let key = self.wx(&d, s(&st, "correlation").unwrap_or_default(), &vars)?;
                match self.wf_buffered(branch, &st, &key)? {
                    None => {
                        tok_mut(&mut body, tid)["wait"] =
                            json!({"event_type": st["event_type"], "key": key, "since": since});
                    }
                    Some(ev) => {
                        if let Some(rv) = s(&st, "result") {
                            let mut vars = body["variables"].clone();
                            Self::wf_set(&d, &mut vars, rv, &ev["payload"])?;
                            body["variables"] = vars;
                        }
                        log(
                            &mut body,
                            &tok,
                            "event",
                            json!({"event_id": ev["event_id"], "buffered": true}),
                        );
                        visit(&mut body, tid, s(&st, "next").unwrap_or_default());
                    }
                }
            }
            "timer" => {
                let due = shift(&self.now, s(&st, "after").unwrap_or_default(), 1)?;
                tok_mut(&mut body, tid)["wait"] = json!({"due": due, "since": since});
            }
            "human_task" => {
                let mut w = json!({"task": true, "since": since});
                if let Some(a) = s(&st, "assignee") {
                    w["assignee"] = json!(a);
                }
                tok_mut(&mut body, tid)["wait"] = w;
            }
            "sub_workflow" => {
                let child = crate::matching::derived_uuid(&json!([
                    "ubos.workflow.child",
                    iid,
                    tid,
                    step,
                    tok["visit"]
                ]))?;
                tok_mut(&mut body, tid)["wait"] = json!({"child": child, "since": since});
                let initiator = str_of(&body["initiator"]);
                self.wf_save(branch, iid, head, &body, None)?;
                let mut inputs = Map::new();
                for (k, e) in st["inputs"].as_object().into_iter().flatten() {
                    inputs.insert(
                        k.clone(),
                        self.wx(&d, e.as_str().unwrap_or_default(), &vars)?,
                    );
                }
                self.wf_create(
                    branch,
                    &st["definition"],
                    &json!(child),
                    &Value::Object(inputs),
                    Some(&initiator),
                    Some(json!({"instance": iid, "token": tid})),
                )?;
                return self.wf_run(branch, &child);
            }
            _ => {}
        }
        self.wf_save(branch, iid, head, &body, None)
    }

    /// STD-TXN-091: the oldest event of the step's type and key that arrived before the
    /// wait within the step's buffer window; it is consumed.
    fn wf_buffered(
        &mut self,
        branch: &str,
        st: &Value,
        key: &Value,
    ) -> Result<Option<Value>, BpaError> {
        let Some(buffer) = s(st, "buffer") else {
            return Ok(None);
        };
        let since = shift(&self.now, buffer, -1)?;
        let bkey = format!("wf_buffer:{branch}");
        let mut evs = self
            .store
            .meta(&bkey)?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let ck = canonical(key)?;
        for ev in evs.iter_mut() {
            if ev["consumed"] != json!(true)
                && ev["event_type"] == st["event_type"]
                && canonical(&ev["key"])? == ck
                && s(ev, "at").unwrap_or_default() >= since.as_str()
            {
                ev["consumed"] = json!(true);
                let found = ev.clone();
                self.store.put_meta(&bkey, &Value::Array(evs))?;
                let ekey = format!("wf_event:{}", str_of(&found["event_id"]));
                if let Some(mut rec) = self.store.meta(&ekey)? {
                    rec["result"] = json!({"woken": null, "buffered": true, "consumed": true});
                    self.store.put_meta(&ekey, &rec)?;
                }
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// A sub-workflow that ended moves its parent's waiting token.
    fn wf_child_done(&mut self, branch: &str, cid: &str, child: &Value) -> Result<(), BpaError> {
        let pid = str_of(&child["parent"]["instance"]);
        let (head, mut body) = self.wf_body(branch, &pid)?;
        let Some(tid) = body["tokens"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| s(&t["wait"], "child") == Some(cid))
            .map(|t| str_of(&t["id"]))
        else {
            return Ok(());
        };
        if s(&body, "status") != Some("running") {
            return Ok(());
        }
        let d = self.version_body(&str_of(&body["definition_version"]))?;
        let tok = tok_mut(&mut body, &tid).clone();
        let st = d["steps"][str_of(&tok["step"])].clone();
        if s(child, "status") == Some("completed") {
            let mut vars = body["variables"].clone();
            for (pv, cv) in st["outputs"].as_object().into_iter().flatten() {
                Self::wf_set(&d, &mut vars, pv, &child["variables"][str_of(cv)])?;
            }
            body["variables"] = vars;
            log(&mut body, &tok, "ok", json!({"child": cid}));
            visit(&mut body, &tid, s(&st, "next").unwrap_or_default());
        } else {
            log(&mut body, &tok, "error", json!({"child": cid}));
            end(
                &mut body,
                "failed",
                Some(json!({"step": tok["step"], "child": cid})),
            );
        }
        self.wf_save(branch, &pid, &head, &body, None)?;
        self.wf_run(branch, &pid)
    }

    fn wf_view(&mut self, branch: &str, iid: &str) -> Result<Value, BpaError> {
        let (_, body) = self.wf_body(branch, iid)?;
        let d = self.version_body(&str_of(&body["definition_version"]))?;
        let mut waiting = Vec::new();
        for t in body["tokens"].as_array().into_iter().flatten() {
            let mut w = t.get("wait").cloned().unwrap_or(json!({}));
            if let Some(m) = w.as_object_mut() {
                m.remove("task");
                m.remove("entered");
            }
            let step = str_of(&t["step"]);
            w["step"] = json!(step);
            w["type"] = d["steps"][&step]["type"].clone();
            w["visit"] = t["visit"].clone();
            waiting.push(w);
        }
        let mut out = json!({"instance_id": iid, "status": body["status"],
            "definition_version": body["definition_version"], "variables": body["variables"],
            "waiting": waiting});
        if body.get("error").is_some_and(|e| !e.is_null()) {
            out["error"] = body["error"].clone();
        }
        Ok(out)
    }

    // ---- events, tasks and timers (STD-TXN-091) ----

    /// Adapter operation `deliver_event`: an event wakes at most one step waiting for its
    /// type and correlation key, the one waiting longest; a repeated event ID returns the
    /// first result.
    pub fn deliver_event(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let (Some(eid), Some(etype)) = (s(i, "event_id"), s(i, "event_type")) else {
            return Err(incomplete("an event has event_id, event_type and key"));
        };
        if i.get("key").is_none() {
            return Err(incomplete("an event has event_id, event_type and key"));
        }
        let (eid, etype) = (eid.to_string(), etype.to_string());
        let mut asked = Map::new();
        for k in ["branch", "event_type", "key", "payload"] {
            asked.insert(k.to_string(), i.get(k).cloned().unwrap_or(Value::Null));
        }
        let h = content_id("ubos.chunk.v1", &Value::Object(asked))?;
        let ekey = format!("wf_event:{eid}");
        if let Some(known) = self.store.meta(&ekey)? {
            if known["hash"] != json!(h) {
                return Err(BpaError::new("TXN.IDEMPOTENCY_MISMATCH", "event_id"));
            }
            let mut r = known["result"].clone();
            r["duplicate"] = json!(true);
            return Ok(r);
        }
        let now = self.now.clone();
        Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
        let ck = canonical(&i["key"])?;
        let mut best: Option<(u64, String, String)> = None;
        for (b, iid) in self.wf_instances()? {
            if b != branch {
                continue;
            }
            let (_, body) = self.wf_body(&branch, &iid)?;
            if s(&body, "status") != Some("running") {
                continue;
            }
            for t in body["tokens"].as_array().into_iter().flatten() {
                let w = &t["wait"];
                if s(w, "event_type") == Some(etype.as_str())
                    && w.get("key").is_some()
                    && canonical(&w["key"])? == ck
                {
                    let order = (
                        w["since"].as_u64().unwrap_or(0),
                        iid.clone(),
                        str_of(&t["id"]),
                    );
                    if best.as_ref().is_none_or(|b| order < *b) {
                        best = Some(order);
                    }
                }
            }
        }
        let mut result = json!({"woken": null, "buffered": true});
        match best {
            None => {
                let bkey = format!("wf_buffer:{branch}");
                let mut evs = self
                    .store
                    .meta(&bkey)?
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                evs.push(
                    json!({"event_id": eid, "event_type": etype, "key": i["key"],
                                "payload": i.get("payload").cloned().unwrap_or(Value::Null),
                                "at": now}),
                );
                self.store.put_meta(&bkey, &Value::Array(evs))?;
            }
            Some((_, iid, tid)) => {
                let (head, mut body) = self.wf_body(&branch, &iid)?;
                let d = self.version_body(&str_of(&body["definition_version"]))?;
                let tok = tok_mut(&mut body, &tid).clone();
                let st = d["steps"][str_of(&tok["step"])].clone();
                if let Some(rv) = s(&st, "result") {
                    let mut vars = body["variables"].clone();
                    Self::wf_set(&d, &mut vars, rv, i.get("payload").unwrap_or(&Value::Null))?;
                    body["variables"] = vars;
                }
                result = json!({"woken": {"instance_id": iid, "step": tok["step"],
                                          "visit": tok["visit"]}});
                log(&mut body, &tok, "event", json!({"event_id": eid}));
                visit(&mut body, &tid, s(&st, "next").unwrap_or_default());
                self.wf_save(&branch, &iid, &head, &body, None)?;
                self.wf_run(&branch, &iid)?;
            }
        }
        self.store
            .put_meta(&ekey, &json!({"hash": h, "result": result}))?;
        Ok(result)
    }

    /// Adapter operation `complete_task`: complete a human task; the completion of
    /// (instance, step, visit) is idempotent.
    pub fn complete_task(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let iid = str_of(&i["instance"]);
        let step = str_of(&i["step"]);
        let Some(principal) = s(i, "principal").map(str::to_string) else {
            return Err(incomplete("principal"));
        };
        let (head, mut body) = self.wf_body(&branch, &iid)?;
        let toks = body["tokens"].as_array().cloned().unwrap_or_default();
        // STD-TXN-093 (1.0-rc12): a step that waits for a Task is completed through the task
        if let Some(t) = toks
            .iter()
            .find(|t| s(t, "step") == Some(step.as_str()) && t["wait"].get("task_id").is_some())
        {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"task_id": t["wait"]["task_id"]}),
            ));
        }
        let h = content_id(
            "ubos.chunk.v1",
            &json!({"principal": principal, "data": i.get("data").cloned().unwrap_or(Value::Null)}),
        )?;
        let given_visit = i.get("visit").filter(|v| !v.is_null());
        let tok = toks
            .iter()
            .find(|t| {
                s(t, "step") == Some(step.as_str())
                    && t["wait"]["task"] == json!(true)
                    && given_visit.is_none_or(|v| *v == t["visit"])
            })
            .cloned();
        let visit_no = match &tok {
            Some(t) => t["visit"].clone(),
            None => given_visit
                .cloned()
                .unwrap_or_else(|| body["visits"].get(&step).cloned().unwrap_or(Value::Null)),
        };
        let vtext = if visit_no.is_null() {
            "None".to_string()
        } else {
            visit_no.to_string()
        };
        let key = format!("{step}/{vtext}");
        if let Some(done) = body["done"].get(&key) {
            if done["hash"] != json!(h) {
                return Err(BpaError::new("TXN.IDEMPOTENCY_MISMATCH", "task"));
            }
            let mut v = self.wf_view(&branch, &iid)?;
            v["duplicate"] = json!(true);
            return Ok(v);
        }
        let Some(tok) = tok.filter(|_| s(&body, "status") == Some("running")) else {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"step": step, "waiting": false}),
            ));
        };
        if s(&tok["wait"], "assignee").is_some_and(|a| a != principal) {
            return Err(BpaError::new(
                "IAM.DENIED",
                "the task is assigned to another principal",
            ));
        }
        let d = self.version_body(&str_of(&body["definition_version"]))?;
        let st = d["steps"][&step].clone();
        if let Some(rv) = s(&st, "result") {
            let mut vars = body["variables"].clone();
            Self::wf_set(&d, &mut vars, rv, i.get("data").unwrap_or(&Value::Null))?;
            body["variables"] = vars;
        }
        body["done"][&key] = json!({"principal": principal, "hash": h});
        log(
            &mut body,
            &tok,
            "completed",
            json!({"principal": principal}),
        );
        let tid = str_of(&tok["id"]);
        visit(&mut body, &tid, s(&st, "next").unwrap_or_default());
        self.wf_save(&branch, &iid, &head, &body, Some(&principal))?;
        self.wf_run(&branch, &iid)?;
        self.wf_view(&branch, &iid)
    }

    /// STD-TXN-091: fire the workflow timers whose rows the tick found due (DSN-DVM-751), in
    /// the order of their due times; a row whose token no longer waits for that due time at
    /// that visit is stale and fires nothing.
    pub(crate) fn fire_wf_timers(&mut self, mut due: Vec<WfTimer>) -> Result<(), BpaError> {
        let now = self.now.clone();
        due.sort();
        for (at, branch, iid, tid, seq) in due {
            if Repo::new(self.store.as_mut(), &self.registry)
                .check_active(&branch, &now)
                .is_err()
            {
                continue;
            }
            let Ok((head, mut body)) = self.wf_body(&branch, &iid) else {
                continue;
            };
            if s(&body, "status") != Some("running") {
                continue;
            }
            let Some(tok) = body["tokens"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| {
                    s(t, "id") == Some(tid.as_str())
                        && s(&t["wait"], "due") == Some(at.as_str())
                        && t["seq"].as_u64() == Some(seq)
                })
                .cloned()
            else {
                continue;
            };
            let d = self.version_body(&str_of(&body["definition_version"]))?;
            log(&mut body, &tok, "fired", json!({"due": tok["wait"]["due"]}));
            let next = str_of(&d["steps"][str_of(&tok["step"])]["next"]);
            visit(&mut body, &tid, &next);
            self.wf_save(&branch, &iid, &head, &body, None)?;
            self.wf_run(&branch, &iid)?;
        }
        Ok(())
    }

    // ---- migration (STD-TXN-092) ----

    /// Adapter operation `migrate_workflow`: move a running instance to another version of
    /// its definition; every active step is mapped explicitly.
    pub fn migrate_workflow(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let iid = str_of(&i["instance"]);
        let (head, mut body) = self.wf_body(&branch, &iid)?;
        if s(&body, "status") != Some("running") {
            return Err(BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "the instance has ended",
            ));
        }
        let def = str_of(&body["definition"]);
        let vh = match s(i, "version") {
            Some(v) => v.to_string(),
            None => {
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                repo.object_at(&branch, None, &def)?
                    .map(|o| o.head)
                    .unwrap_or_default()
            }
        };
        let v = Repo::new(self.store.as_mut(), &self.registry)
            .version(&vh)
            .unwrap_or(Value::Null);
        if s(&v, "object_id") != Some(def.as_str()) || v["tombstone"] == json!(true) {
            return Err(invalid("version is a version of the instance definition"));
        }
        let old = self.version_body(&str_of(&body["definition_version"]))?;
        let new = v["body"].clone();
        check_definition(&new)?;
        let smap = i
            .get("step_map")
            .filter(|m| !m.is_null())
            .cloned()
            .unwrap_or(json!({}));
        let Some(smap) = smap.as_object() else {
            return Err(invalid("step_map is a map"));
        };
        let mut active: Vec<String> = body["tokens"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| str_of(&t["step"]))
            .collect();
        for f in body["forks"].as_object().into_iter().flatten() {
            if f.1["fired"] != json!(true) {
                active.push(str_of(&f.1["join"]));
            }
        }
        active.sort();
        active.dedup();
        let unmapped: Vec<&String> = active.iter().filter(|x| !smap.contains_key(*x)).collect();
        if !unmapped.is_empty() {
            return Err(BpaError::with_details(
                "MODEL.MIGRATION_REQUIRED",
                json!({"unmapped": unmapped}),
            ));
        }
        for a in &active {
            let to = str_of(&smap[a.as_str()]);
            let same = new["steps"]
                .get(&to)
                .is_some_and(|ns| ns["type"] == old["steps"][a]["type"]);
            if !same {
                return Err(invalid(&format!(
                    "step {a} maps to a step of the same type"
                )));
            }
        }
        let n = body["tokens"].as_array().map_or(0, Vec::len);
        for k in 0..n {
            let from = str_of(&body["tokens"][k]["step"]);
            let to = str_of(&smap[from.as_str()]);
            body["tokens"][k]["step"] = json!(to);
            let tv = body["tokens"][k]["visit"].as_u64().unwrap_or(0);
            let cur = body["visits"][&to].as_u64().unwrap_or(0);
            body["visits"][&to] = json!(cur.max(tv));
        }
        let fids: Vec<String> = body["forks"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        for f in fids {
            if body["forks"][&f]["fired"] != json!(true) {
                let j = str_of(&body["forks"][&f]["join"]);
                body["forks"][&f]["join"] = smap[j.as_str()].clone();
            }
        }
        let mut sm = Map::new();
        for a in &active {
            sm.insert(a.clone(), smap[a.as_str()].clone());
        }
        let entry = json!({"from": body["definition_version"], "to": vh, "step_map": sm});
        body["migrations"]
            .as_array_mut()
            .expect("migrations")
            .push(entry);
        body["definition_version"] = json!(vh);
        self.wf_save(&branch, &iid, &head, &body, None)?;
        self.wf_run(&branch, &iid)?;
        self.wf_view(&branch, &iid)
    }
}
