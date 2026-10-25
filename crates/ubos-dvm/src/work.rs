//! Delegation and human work (PH-2 step 4d, DEC-108): delegations (STD-LGS-060, BPA 1.0-rc10,
//! DSN-DVM-745) and the roles of the adapter's directory they rest on.

use crate::stateful::Engine;
use serde_json::{Value, json};
use ubos_types::BpaError;
use ubos_types::decimal::Decimal;
use ubos_version::state::Repo;

pub(crate) const DELEGATION: &str = "Delegation";
const SCOPE_KEYS: &[&str] = &["ports", "classes", "task_types", "limit"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn incomplete(what: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.to_string())
}

fn invalid(what: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", what.to_string())
}

fn denied(details: Value) -> BpaError {
    BpaError::with_details("IAM.DENIED", details)
}

fn instant<'a>(i: &'a Value, k: &str) -> Result<&'a str, BpaError> {
    let v = s(i, k).ok_or_else(|| invalid(&format!("{k} is an instant")))?;
    ubos_types::time::parse_instant(v).map_err(|_| invalid(&format!("{k} is an instant")))?;
    Ok(v)
}

fn names(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

/// What an action through a delegation is about (STD-LGS-060 scope).
#[derive(Default)]
pub(crate) struct Use<'a> {
    pub port: Option<&'a str>,
    pub class: Option<&'a str>,
    pub task_type: Option<&'a str>,
    pub params: Option<&'a Value>,
}

impl Engine {
    /// The roles of a principal in the adapter's directory (`principal`).
    pub(crate) fn roles_of(&mut self, did: &str) -> Result<Vec<String>, BpaError> {
        Ok(self
            .store
            .meta(&format!("principal:{did}"))?
            .and_then(|e| names(&e["roles"]))
            .unwrap_or_default())
    }

    /// The head body of a live object of `class` on a branch, with its head version.
    pub(crate) fn live_of(
        &mut self,
        branch: &str,
        oid: &str,
        class: &str,
    ) -> Result<(String, Value), BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let o = repo
            .object_at(branch, None, oid)?
            .filter(|o| o.class == class)
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("unknown {class}")))?;
        let v = repo.version(&o.head)?;
        if v["tombstone"] == json!(true) {
            return Err(BpaError::new("IAM.NOT_FOUND", format!("unknown {class}")));
        }
        Ok((o.head, v["body"].clone()))
    }

    /// `[(object id, head body)]` of the live objects of a class on a branch.
    pub(crate) fn all_of(
        &mut self,
        branch: &str,
        class: &str,
    ) -> Result<Vec<(String, Value)>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let cseq = repo.ordinal(branch, None)?;
        let mut out = Vec::new();
        for (oid, o) in repo.objects_in(branch, cseq, Some(class))? {
            let v = repo.version(&o.head)?;
            if v["tombstone"] != json!(true) {
                out.push((oid, v["body"].clone()));
            }
        }
        Ok(out)
    }

    /// A kernel-written commit of System objects with `principal` as initiator.
    pub(crate) fn kernel_write(
        &mut self,
        branch: &str,
        changes: Vec<Value>,
        principal: Option<&str>,
    ) -> Result<Value, BpaError> {
        let mut process = json!({"kind": "kernel"});
        if let Some(p) = principal {
            process["initiator"] = json!(p);
        }
        self.system_commit(&json!({"branch": branch, "changes": changes, "process": process}))
    }

    /// Adapter operation `delegate` (STD-LGS-060).
    pub fn delegate(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let (Some(id), Some(delegator), Some(delegate)) =
            (s(i, "delegation_id"), s(i, "delegator"), s(i, "delegate"))
        else {
            return Err(incomplete(
                "a delegation has its id, delegator and delegate",
            ));
        };
        if delegator == delegate {
            return Err(BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "a principal does not delegate to itself",
            ));
        }
        let scope = i.get("scope").cloned().unwrap_or(Value::Null);
        let Some(sc) = scope.as_object() else {
            return Err(incomplete("a delegation has a scope"));
        };
        if !SCOPE_KEYS
            .iter()
            .any(|k| sc.get(*k).is_some_and(|v| !v.is_null()))
        {
            return Err(incomplete("a delegation has a scope"));
        }
        if sc.keys().any(|k| !SCOPE_KEYS.contains(&k.as_str())) {
            return Err(invalid("a scope has ports, classes, task_types and limit"));
        }
        for k in ["ports", "classes", "task_types"] {
            if let Some(v) = sc.get(k).filter(|v| !v.is_null())
                && names(v).is_none()
            {
                return Err(invalid(&format!("{k} is a list of names")));
            }
        }
        if let Some(lim) = sc.get("limit").filter(|v| !v.is_null()) {
            if s(lim, "param").is_none() {
                return Err(invalid("a limit is {param, max}"));
            }
            Decimal::parse(s(lim, "max").unwrap_or("?"))?;
        }
        let from = instant(i, "valid_from")?;
        let to = instant(i, "valid_to")?;
        if from >= to {
            return Err(invalid("valid_from is before valid_to"));
        }
        let mut roles = match i.get("roles").filter(|v| !v.is_null()) {
            None => Vec::new(),
            Some(v) => names(v).ok_or_else(|| invalid("roles are a list of names"))?,
        };
        roles.sort();
        roles.dedup();
        let held = self.roles_of(delegator)?;
        let extra: Vec<&String> = roles.iter().filter(|r| !held.contains(r)).collect();
        if !extra.is_empty() {
            return Err(denied(
                json!({"delegation": "exceeds_authority", "roles": extra}),
            ));
        }
        let body = json!({"delegator": delegator, "delegate": delegate, "roles": roles,
                          "scope": scope, "valid_from": from, "valid_to": to, "state": "active"});
        let slug = format!("delegation-{id}");
        let res = self.kernel_write(
            &branch,
            vec![
                json!({"op": "create", "object_id": id, "class": DELEGATION, "slug": slug,
                        "body": body}),
            ],
            Some(delegator),
        )?;
        let mut out = body;
        out["delegation_id"] = json!(id);
        out["cseq"] = res["cseq"].clone();
        Ok(out)
    }

    /// Adapter operation `revoke_delegation` (STD-LGS-060): only the delegator revokes.
    pub fn revoke_delegation(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let id = s(i, "delegation_id").unwrap_or_default().to_string();
        let (head, mut body) = self.live_of(&branch, &id, DELEGATION)?;
        if s(i, "principal") != s(&body, "delegator") {
            return Err(denied(json!({"delegation": "not_delegator"})));
        }
        if s(&body, "state") == Some("revoked") {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"state": "revoked"}),
            ));
        }
        body["state"] = json!("revoked");
        body["revoked_at"] = json!(self.now);
        if let Some(r) = i.get("reason").filter(|v| !v.is_null()) {
            body["reason"] = r.clone();
        }
        let delegator = s(&body, "delegator").unwrap_or_default().to_string();
        self.kernel_write(
            &branch,
            vec![json!({"op": "update", "object_id": id, "base": head, "body": body})],
            Some(&delegator),
        )?;
        body["delegation_id"] = json!(id);
        Ok(body)
    }

    /// The state of a delegation at the current time.
    pub(crate) fn delegation_state(&self, body: &Value) -> &'static str {
        if s(body, "state") == Some("revoked") {
            "revoked"
        } else if self.now.as_str() < s(body, "valid_from").unwrap_or_default() {
            "scheduled"
        } else if self.now.as_str() < s(body, "valid_to").unwrap_or_default() {
            "active"
        } else {
            "expired"
        }
    }

    /// Adapter operation `delegations` (STD-LGS-060, FR-FLOW-093): the delegations a
    /// principal gives or holds, with their state, for access reviews.
    pub fn delegations(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let p = s(i, "principal");
        let mut out = Vec::new();
        for (oid, mut body) in self.all_of(&branch, DELEGATION)? {
            if p.is_none_or(|p| s(&body, "delegator") == Some(p) || s(&body, "delegate") == Some(p))
            {
                body["state"] = json!(self.delegation_state(&body));
                body["delegation_id"] = json!(oid);
                out.push(body);
            }
        }
        Ok(json!({"delegations": out}))
    }

    /// STD-LGS-060: the delegation an action names, checked for the acting principal and
    /// what the action is about; returns its body.
    pub(crate) fn use_delegation(
        &mut self,
        branch: &str,
        id: &str,
        principal: &str,
        on_behalf_of: Option<&str>,
        u: Use<'_>,
    ) -> Result<Value, BpaError> {
        let (_, body) = self.live_of(branch, id, DELEGATION)?;
        if s(&body, "delegate") != Some(principal) {
            return Err(denied(json!({"delegation": "not_delegate"})));
        }
        if on_behalf_of.is_some_and(|b| s(&body, "delegator") != Some(b)) {
            return Err(denied(json!({"delegation": "not_delegator"})));
        }
        match self.delegation_state(&body) {
            "expired" => {
                return Err(BpaError::with_details(
                    "IAM.DELEGATION_EXPIRED",
                    json!({"delegation_id": id, "valid_to": body["valid_to"]}),
                ));
            }
            "revoked" => return Err(denied(json!({"delegation": "revoked"}))),
            "scheduled" => return Err(denied(json!({"delegation": "not_yet_valid"}))),
            _ => {}
        }
        let sc = &body["scope"];
        let within = |k: &str, v: Option<&str>| -> bool {
            match names(&sc[k]) {
                None => true,
                Some(list) => v.is_some_and(|v| list.iter().any(|x| x == v)),
            }
        };
        let mut ok = within("ports", u.port)
            && within("classes", u.class)
            && within("task_types", u.task_type);
        if u.task_type.is_some() && sc["task_types"].is_null() {
            ok = false; // task authority is delegated by type only
        }
        if let Some(lim) = sc.get("limit").filter(|v| !v.is_null())
            && u.task_type.is_none()
        {
            let param = s(lim, "param").unwrap_or_default();
            let v = u.params.and_then(|p| p.get(param));
            let text = match v {
                Some(Value::String(t)) => Some(t.clone()),
                Some(Value::Number(n)) => Some(n.to_string()),
                _ => None,
            };
            let max = Decimal::parse(s(lim, "max").unwrap_or("0"))?;
            ok = ok
                && text
                    .and_then(|t| Decimal::parse(&t).ok())
                    .is_some_and(|d| d.cmp(&max) != std::cmp::Ordering::Greater);
        }
        if !ok {
            return Err(denied(json!({"delegation": "out_of_scope"})));
        }
        Ok(body)
    }
}

pub(crate) const TASK: &str = "Task";
pub(crate) const QUEUE: &str = "WorkQueue";
const ESCALATION: &[&str] = &["notify", "reassign", "raise_priority"];
const FAR: &str = "9999-12-31T23:59:59.999999Z";

/// A task target `{principal}`, `{role}` or `{queue}` (the kinds allowed).
fn target(to: Option<&Value>, kinds: &[&str]) -> Result<Value, BpaError> {
    let ok = to.and_then(Value::as_object).is_some_and(|m| {
        m.len() == 1
            && m.iter()
                .all(|(k, v)| kinds.contains(&k.as_str()) && v.is_string())
    });
    if !ok {
        return Err(invalid(&format!(
            "a target is one of {}",
            kinds
                .iter()
                .map(|k| format!("{{{k}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(to.cloned().unwrap_or(Value::Null))
}

/// STD-LGS-062, -063 (BPA 1.0-rc10): the declaration of a work queue.
fn check_queue(b: &Value) -> Result<(), BpaError> {
    if names(&b["members"]).is_none() {
        return Err(invalid("members are principals"));
    }
    if !b["managers"].is_null() && names(&b["managers"]).is_none() {
        return Err(invalid("managers are roles"));
    }
    if !b["skills"].is_null()
        && !b["skills"]
            .as_object()
            .is_some_and(|m| m.values().all(|v| v.is_array()))
    {
        return Err(invalid("skills map a member to a list of skills"));
    }
    if !b["due_rule"].is_null() && !b["due_rule"].is_string() {
        return Err(invalid("a due rule is an L1 expression"));
    }
    for step in b["escalation"].as_array().into_iter().flatten() {
        let action = s(step, "action").unwrap_or_default();
        let Some(after) = s(step, "after").filter(|_| ESCALATION.contains(&action)) else {
            return Err(invalid(
                "an escalation step is {after, action} with action notify, reassign or raise_priority",
            ));
        };
        ubos_expr::value::parse_duration(after)?;
        match action {
            "notify" => {
                target(step.get("to"), &["principal", "role"])?;
            }
            "reassign" => {
                target(step.get("to"), &["principal", "role", "queue"])?;
            }
            _ => {
                if !step["priority"].is_i64() {
                    return Err(invalid("raise_priority names the priority"));
                }
            }
        }
    }
    Ok(())
}

/// STD-LGS-062 (BPA 1.0-rc10): the work queues the commit wrote.
pub(crate) fn check(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    for (_, vh) in written {
        let v = repo.version_in(tx, vh)?;
        if s(&v, "class") == Some(QUEUE) && v["tombstone"] != json!(true) {
            check_queue(&v["body"])?;
        }
    }
    Ok(())
}

fn log_entry(action: &str, principal: &str, behalf: Option<&str>, now: &str) -> Value {
    let mut e = json!({"action": action, "principal": principal, "at": now});
    if let Some(b) = behalf {
        e["on_behalf_of"] = json!(b);
    }
    e
}

fn with_id(mut body: Value, tid: &str) -> Value {
    body["task_id"] = json!(tid);
    body
}

impl Engine {
    /// The open or claimed tasks assigned to or owned by a member.
    fn load(tasks: &[(String, Value)], member: &str) -> usize {
        tasks
            .iter()
            .filter(|(_, b)| {
                matches!(s(b, "state"), Some("open" | "claimed"))
                    && (s(b, "owner") == Some(member)
                        || (b["owner"].is_null() && s(&b["assignee"], "principal") == Some(member)))
            })
            .count()
    }

    /// STD-LGS-062: the member a queue's rule assigns a new task to.
    fn assign(tasks: &[(String, Value)], qid: &str, q: &Value, task: &Value) -> Option<String> {
        let rule = s(q, "assignment").unwrap_or("manual");
        let members = names(&q["members"]).unwrap_or_default();
        if rule == "manual" || members.is_empty() {
            return None;
        }
        if rule == "round_robin" {
            let n = tasks
                .iter()
                .filter(|(_, b)| {
                    s(b, "queue") == Some(qid) && s(b, "assigned_by") == Some("round_robin")
                })
                .count();
            return Some(members[n % members.len()].clone());
        }
        let mut cands = members;
        if rule == "skill" {
            let need = names(&task["skills"]).unwrap_or_default();
            cands.retain(|m| {
                let has = names(&q["skills"][m.as_str()]).unwrap_or_default();
                need.iter().all(|x| has.contains(x))
            });
        }
        cands
            .iter()
            .enumerate()
            .min_by_key(|(k, m)| (Self::load(tasks, m), *k))
            .map(|(_, m)| m.clone())
    }

    /// STD-LGS-063: the due instant (and date) of a queue's due rule for a new task.
    fn due_of(&self, q: &Value, task: &Value) -> Result<(String, Option<String>), BpaError> {
        let types = json!({"created": {"name": "created", "type": "date"},
                           "created_at": {"name": "created_at", "type": "instant"},
                           "type": {"name": "type", "type": "text"},
                           "priority": {"name": "priority", "type": "integer"}});
        let out = ubos_expr::eval_expr(
            &json!({"expr": q["due_rule"], "params_types": types,
                    "calendars": q.get("calendars").filter(|c| !c.is_null()).cloned().unwrap_or(json!({})),
                    "params": {"created": &self.now[..10], "created_at": self.now,
                               "type": task["type"], "priority": task["priority"]}}),
            None,
        )?;
        let v = s(&out, "value").unwrap_or_default().to_string();
        match s(&out, "type") {
            Some("date") => {
                let d = ubos_types::time::parse_date(&v)? + chrono::Duration::days(1);
                Ok((
                    format!("{}T00:00:00.000000Z", d.format("%Y-%m-%d")),
                    Some(v),
                ))
            }
            Some("instant") => Ok((v, None)),
            _ => Err(invalid("a due rule gives a date or an instant")),
        }
    }

    /// Adapter operation `task` (STD-LGS-061): create, claim, release, reassign, complete
    /// or cancel a task, each in one commit of a kernel process with the principal as
    /// initiator.
    pub fn task(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let action = s(i, "action").unwrap_or_default().to_string();
        if ![
            "create", "claim", "release", "reassign", "complete", "cancel",
        ]
        .contains(&action.as_str())
        {
            return Err(invalid(
                "a task action is one of create, claim, release, reassign, complete, cancel",
            ));
        }
        let Some(principal) = s(i, "principal").map(str::to_string) else {
            return Err(incomplete("principal"));
        };
        let Some(tid) = s(i, "task_id").map(str::to_string) else {
            return Err(incomplete("task_id"));
        };
        if action == "create" {
            return self.task_create(&branch, &tid, &principal, i);
        }
        let (head, mut body) = self.live_of(&branch, &tid, TASK)?;
        if let Some(b) = s(i, "base")
            && b != head
        {
            return Err(BpaError::with_details(
                "TXN.HEAD_MOVED",
                json!({"object_id": tid}),
            ));
        }
        let q = match s(&body, "queue") {
            Some(qid) => Some(self.live_of(&branch, qid, QUEUE)?.1),
            None => None,
        };
        let own_roles = self.roles_of(&principal)?;
        let manager = q.as_ref().is_some_and(|q| {
            names(&q["managers"])
                .unwrap_or_default()
                .iter()
                .any(|m| own_roles.contains(m))
        });
        let (acting, behalf, roles) = match s(i, "delegation")
            .filter(|_| matches!(action.as_str(), "claim" | "release" | "complete"))
        {
            Some(d) => {
                let tt = s(&body, "type").unwrap_or_default().to_string();
                let del = self.use_delegation(
                    &branch,
                    d,
                    &principal,
                    None,
                    Use {
                        task_type: Some(&tt),
                        ..Use::default()
                    },
                )?;
                let who = s(&del, "delegator").unwrap_or_default().to_string();
                (
                    who.clone(),
                    Some(who),
                    names(&del["roles"]).unwrap_or_default(),
                )
            }
            None => (principal.clone(), None, own_roles),
        };
        let state = s(&body, "state").unwrap_or_default().to_string();
        let need = |states: &[&str]| -> Result<(), BpaError> {
            if states.contains(&state.as_str()) {
                Ok(())
            } else {
                Err(BpaError::with_details(
                    "FLOW.TRANSITION_NOT_ALLOWED",
                    json!({"state": state, "action": action}),
                ))
            }
        };
        let now = self.now.clone();
        let m = body.as_object_mut().expect("task body");
        let mut log = m
            .get("log")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        match action.as_str() {
            "claim" => {
                need(&["open"])?;
                let a = m.get("assignee").cloned().unwrap_or(Value::Null);
                let ok = s(&a, "principal") == Some(acting.as_str())
                    || s(&a, "role").is_some_and(|r| roles.iter().any(|x| x == r))
                    || (a.get("principal").is_none()
                        && a.get("role").is_none()
                        && q.as_ref().is_some_and(|q| {
                            names(&q["members"]).unwrap_or_default().contains(&acting)
                        }));
                if !ok {
                    return Err(denied(
                        json!({"task": "not_eligible", "principal": principal}),
                    ));
                }
                m.insert("state".into(), json!("claimed"));
                m.insert("owner".into(), json!(principal));
                if let Some(b) = &behalf {
                    m.insert("on_behalf_of".into(), json!(b));
                }
                log.push(log_entry("claim", &principal, behalf.as_deref(), &now));
            }
            "release" | "complete" => {
                need(&["claimed"])?;
                if m.get("owner").and_then(Value::as_str) != Some(principal.as_str()) {
                    return Err(denied(json!({"task": "not_owner", "principal": principal})));
                }
                if action == "release" {
                    m.insert("state".into(), json!("open"));
                    m.remove("owner");
                    m.remove("on_behalf_of");
                } else {
                    m.insert("state".into(), json!("completed"));
                    m.insert("completed_at".into(), json!(now));
                    if let Some(o) = i.get("outcome").filter(|v| !v.is_null()) {
                        m.insert("outcome".into(), o.clone());
                    }
                }
                log.push(log_entry(&action, &principal, behalf.as_deref(), &now));
            }
            "reassign" => {
                need(&["open", "claimed"])?;
                if !manager {
                    return Err(denied(
                        json!({"task": "not_manager", "principal": principal}),
                    ));
                }
                let to = target(i.get("to"), &["principal", "role", "queue"])?;
                if let Some(qid) = s(&to, "queue") {
                    self.live_of(&branch, qid, QUEUE)?;
                    m.insert("queue".into(), json!(qid));
                    m.remove("assignee");
                } else {
                    m.insert("assignee".into(), to.clone());
                }
                m.insert("state".into(), json!("open"));
                m.remove("owner");
                m.remove("on_behalf_of");
                m.remove("assigned_by");
                let mut e = log_entry("reassign", &principal, None, &now);
                e["to"] = to;
                if let Some(r) = i.get("reason").filter(|v| !v.is_null()) {
                    e["reason"] = r.clone();
                }
                log.push(e);
            }
            _ => {
                need(&["open", "claimed"])?;
                if !manager
                    && m.get("created_by").and_then(Value::as_str) != Some(principal.as_str())
                {
                    return Err(denied(
                        json!({"task": "not_manager", "principal": principal}),
                    ));
                }
                m.insert("state".into(), json!("cancelled"));
                let mut e = log_entry("cancel", &principal, None, &now);
                if let Some(r) = i.get("reason").filter(|v| !v.is_null()) {
                    e["reason"] = r.clone();
                }
                log.push(e);
            }
        }
        m.insert("log".into(), Value::Array(log));
        self.kernel_write(
            &branch,
            vec![json!({"op": "update", "object_id": tid, "base": head, "body": body})],
            Some(&principal),
        )?;
        Ok(with_id(body, &tid))
    }

    fn task_create(
        &mut self,
        branch: &str,
        tid: &str,
        principal: &str,
        i: &Value,
    ) -> Result<Value, BpaError> {
        let Some(ty) = s(i, "type") else {
            return Err(incomplete("a task has a type"));
        };
        let queue = s(i, "queue");
        if queue.is_none() && i.get("assignee").is_none_or(Value::is_null) {
            return Err(incomplete("a task has a queue or an assignee"));
        }
        let prio = match i.get("priority") {
            None | Some(Value::Null) => 0,
            Some(v) => v
                .as_i64()
                .ok_or_else(|| invalid("a priority is an integer"))?,
        };
        let mut body = json!({"type": ty, "state": "open", "priority": prio,
                              "created_by": principal, "created_at": self.now,
                              "escalations": [],
                              "log": [log_entry("create", principal, None, &self.now)]});
        for k in ["subject", "data"] {
            if let Some(v) = i.get(k).filter(|v| !v.is_null()) {
                body[k] = v.clone();
            }
        }
        if let Some(v) = i.get("skills").filter(|v| !v.is_null()) {
            names(v).ok_or_else(|| invalid("skills are a list of names"))?;
            body["skills"] = v.clone();
        }
        if let Some(a) = i.get("assignee").filter(|v| !v.is_null()) {
            body["assignee"] = target(Some(a), &["principal", "role"])?;
        }
        let mut q = None;
        if let Some(qid) = queue {
            let qb = self.live_of(branch, qid, QUEUE)?.1;
            body["queue"] = json!(qid);
            if body.get("assignee").is_none() {
                let tasks = self.all_of(branch, TASK)?;
                if let Some(m) = Self::assign(&tasks, qid, &qb, &body) {
                    body["assignee"] = json!({"principal": m});
                    body["assigned_by"] = qb["assignment"].clone();
                }
            }
            q = Some(qb);
        }
        if i.get("due").is_some_and(|v| !v.is_null()) {
            body["due"] = json!(instant(i, "due")?);
        } else if let Some(qb) = q.as_ref().filter(|qb| qb["due_rule"].is_string()) {
            let (due, date) = self.due_of(qb, &body)?;
            body["due"] = json!(due);
            if let Some(d) = date {
                body["due_date"] = json!(d);
            }
        }
        // the branches the escalation sweep looks at (STD-LGS-063)
        let mut branches = self
            .store
            .meta("work_branches")?
            .and_then(|v| names(&v))
            .unwrap_or_default();
        if !branches.iter().any(|b| b == branch) {
            branches.push(branch.to_string());
            self.store.put_meta("work_branches", &json!(branches))?;
        }
        self.kernel_write(
            branch,
            vec![json!({"op": "create", "object_id": tid, "class": TASK,
                        "slug": format!("task-{tid}"), "body": body})],
            Some(principal),
        )?;
        Ok(with_id(body, tid))
    }

    /// Adapter operation `inbox` (STD-LGS-062): the tasks a principal may work on and the
    /// pending changes they may decide, with counts.
    pub fn inbox(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let Some(p) = s(i, "principal").map(str::to_string) else {
            return Err(incomplete("principal"));
        };
        let roles = self.roles_of(&p)?;
        let queues: std::collections::BTreeMap<String, Value> =
            self.all_of(&branch, QUEUE)?.into_iter().collect();
        let member = |qid: Option<&str>, who: &str| -> bool {
            qid.and_then(|q| queues.get(q)).is_some_and(|q| {
                names(&q["members"])
                    .unwrap_or_default()
                    .iter()
                    .any(|m| m == who)
            })
        };
        let mut delegated = Vec::new();
        for (_, d) in self.all_of(&branch, DELEGATION)? {
            if s(&d, "delegate") == Some(p.as_str())
                && self.delegation_state(&d) == "active"
                && !d["scope"]["task_types"].is_null()
            {
                delegated.push(d);
            }
        }
        let mut tasks = Vec::new();
        for (tid, b) in self.all_of(&branch, TASK)? {
            let state = s(&b, "state").unwrap_or_default();
            if state != "open" && state != "claimed" {
                continue;
            }
            let a = &b["assignee"];
            let open = state == "open";
            let unassigned = a.is_null();
            let mine = s(&b, "owner") == Some(p.as_str())
                || (b["owner"].is_null() && s(a, "principal") == Some(p.as_str()));
            if mine
                || (open && s(a, "role").is_some_and(|r| roles.iter().any(|x| x == r)))
                || (open && unassigned && member(s(&b, "queue"), &p))
            {
                tasks.push(with_id(b, &tid));
                continue;
            }
            for d in &delegated {
                let ty = s(&b, "type").unwrap_or_default();
                if !names(&d["scope"]["task_types"])
                    .unwrap_or_default()
                    .iter()
                    .any(|t| t == ty)
                {
                    continue;
                }
                let who = s(d, "delegator").unwrap_or_default();
                let droles = names(&d["roles"]).unwrap_or_default();
                if open
                    && (s(a, "principal") == Some(who)
                        || s(a, "role").is_some_and(|r| droles.iter().any(|x| x == r))
                        || (unassigned && member(s(&b, "queue"), who)))
                {
                    let mut item = with_id(b.clone(), &tid);
                    item["on_behalf_of"] = json!(who);
                    tasks.push(item);
                    break;
                }
            }
        }
        let due = |t: &Value| s(t, "due").unwrap_or(FAR).to_string();
        let prio = |t: &Value| t["priority"].as_i64().unwrap_or_default();
        let id = |t: &Value| s(t, "task_id").unwrap_or_default().to_string();
        if s(i, "sort") == Some("priority") {
            tasks.sort_by_key(|t| (-prio(t), due(t), id(t)));
        } else {
            tasks.sort_by_key(|t| (due(t), -prio(t), id(t)));
        }
        let mut approvals = Vec::new();
        let mut ids: Vec<String> = self
            .store
            .meta("pending_changes")?
            .and_then(|v| names(&v))
            .unwrap_or_default();
        ids.sort();
        let proles = json!({"roles": roles});
        for pcid in ids {
            let rec = self.pc(&pcid)?;
            if s(&rec, "target_branch") != Some(branch.as_str())
                || s(&rec, "state") != Some("pending")
            {
                continue;
            }
            let reqs = rec["requirements"].as_array().cloned().unwrap_or_default();
            let step = if reqs.iter().any(|r| s(r, "step") == Some("approval")) {
                "approval"
            } else {
                reqs.first()
                    .and_then(|r| s(r, "step"))
                    .unwrap_or("approval")
            };
            let req = reqs
                .iter()
                .find(|r| s(r, "step") == Some(step))
                .cloned()
                .unwrap_or_else(|| json!({"step": step, "object_id": null}));
            if self.eligible(&rec, &req, &proles, &p).is_ok() {
                approvals.push(
                    json!({"pending_change_id": pcid, "requirements": rec["requirements"],
                                      "preview": rec["preview"]}),
                );
            }
        }
        let overdue = tasks
            .iter()
            .filter(|t| s(t, "due").is_some_and(|d| self.now.as_str() >= d))
            .count();
        let counts =
            json!({"tasks": tasks.len(), "overdue": overdue, "approvals": approvals.len()});
        Ok(json!({"tasks": tasks, "approvals": approvals, "counts": counts}))
    }
}

impl Engine {
    /// STD-LGS-063 (BPA 1.0-rc10, PH-2 step 4f): apply the escalation steps whose time the
    /// clock has reached to the open and claimed tasks, in the order of their times, then
    /// of branch and task, once per task and step; each step is a kernel commit and emits
    /// `task.escalated`.
    pub(crate) fn task_escalations(&mut self) -> Result<(), BpaError> {
        let branches = self
            .store
            .meta("work_branches")?
            .and_then(|v| names(&v))
            .unwrap_or_default();
        let mut due: Vec<(String, String, String, usize)> = Vec::new();
        for branch in branches {
            let active = Repo::new(self.store.as_mut(), &self.registry)
                .head(&branch)
                .is_ok();
            if !active {
                continue;
            }
            let queues: std::collections::BTreeMap<String, Value> =
                self.all_of(&branch, QUEUE)?.into_iter().collect();
            for (tid, b) in self.all_of(&branch, TASK)? {
                if !matches!(s(&b, "state"), Some("open" | "claimed")) {
                    continue;
                }
                let (Some(d), Some(q)) = (s(&b, "due"), s(&b, "queue").and_then(|q| queues.get(q)))
                else {
                    continue;
                };
                let done: Vec<u64> = b["escalations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e["step"].as_u64())
                    .collect();
                for (k, step) in q["escalation"].as_array().into_iter().flatten().enumerate() {
                    let at = crate::approvals::plus(d, s(step, "after").unwrap_or("PT0S"))?;
                    if !done.contains(&(k as u64)) && at.as_str() <= self.now.as_str() {
                        due.push((at, branch.clone(), tid.clone(), k));
                    }
                }
            }
        }
        due.sort();
        for (at, branch, tid, k) in due {
            let (head, mut b) = self.live_of(&branch, &tid, TASK)?;
            let done = b["escalations"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|e| e["step"].as_u64() == Some(k as u64));
            if done || !matches!(s(&b, "state"), Some("open" | "claimed")) {
                continue;
            }
            let qid = s(&b, "queue").unwrap_or_default().to_string();
            let q = self.live_of(&branch, &qid, QUEUE)?.1;
            let step = q["escalation"][k].clone();
            let action = s(&step, "action").unwrap_or_default().to_string();
            let mut rec = json!({"step": k, "action": action, "at": at});
            let m = b.as_object_mut().expect("task body");
            match action.as_str() {
                "reassign" => {
                    if let Some(q2) = s(&step["to"], "queue") {
                        m.insert("queue".into(), json!(q2));
                        m.remove("assignee");
                    } else {
                        m.insert("assignee".into(), step["to"].clone());
                    }
                    m.insert("state".into(), json!("open"));
                    m.remove("owner");
                    m.remove("on_behalf_of");
                    m.remove("assigned_by");
                }
                "raise_priority" => {
                    m.insert("priority".into(), step["priority"].clone());
                }
                _ => {}
            }
            if !step["to"].is_null() {
                rec["to"] = step["to"].clone();
            }
            let mut escalations = m
                .get("escalations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            escalations.push(rec);
            m.insert("escalations".into(), Value::Array(escalations));
            let mut log = m
                .get("log")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            log.push(json!({"action": "escalate", "step": k, "escalation": action, "at": at}));
            m.insert("log".into(), Value::Array(log));
            let res = self.kernel_write(
                &branch,
                vec![json!({"op": "update", "object_id": tid, "base": head, "body": b})],
                None,
            )?;
            let mut payload = json!({"task_id": tid, "step": k, "action": action});
            if !step["to"].is_null() {
                payload["to"] = step["to"].clone();
            }
            self.store.outbox_push(&[json!({
                "effect_id": format!("task-escalation/{tid}/{k}"), "type": "task.escalated",
                "branch": branch, "cseq": res["cseq"], "object_id": tid, "payload": payload})])?;
        }
        Ok(())
    }

    /// Adapter operation `sla_report` (STD-LGS-064): per task type and queue, the tasks with
    /// a due time completed in `[from, to)` on time or late, the attainment and the open
    /// overdue tasks, with a total.
    pub fn sla_report(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let (Some(from), Some(to)) = (s(i, "from"), s(i, "to")) else {
            return Err(invalid("from and to are dates"));
        };
        for d in [from, to] {
            ubos_types::time::parse_date(d).map_err(|_| invalid("from and to are dates"))?;
        }
        let mut rows: std::collections::BTreeMap<(String, String), Value> =
            std::collections::BTreeMap::new();
        for (_, b) in self.all_of(&branch, TASK)? {
            let Some(due) = s(&b, "due") else { continue };
            let ty = s(&b, "type").unwrap_or_default().to_string();
            let queue = s(&b, "queue").map(str::to_string);
            let key = (ty.clone(), queue.clone().unwrap_or_default());
            let row = |rows: &mut std::collections::BTreeMap<(String, String), Value>| {
                rows.entry(key.clone())
                    .or_insert_with(|| {
                        json!({"type": ty, "queue": queue, "completed": 0, "on_time": 0,
                               "late": 0, "open_overdue": 0})
                    })
                    .clone()
            };
            let state = s(&b, "state").unwrap_or_default();
            let done_at = s(&b, "completed_at").unwrap_or_default();
            let bump = |r: &mut Value, k: &str| {
                r[k] = json!(r[k].as_u64().unwrap_or_default() + 1);
            };
            if state == "completed" && done_at.get(..10).is_some_and(|d| d >= from && d < to) {
                let mut r = row(&mut rows);
                bump(&mut r, "completed");
                bump(&mut r, if done_at <= due { "on_time" } else { "late" });
                rows.insert(key, r);
            } else if matches!(state, "open" | "claimed") && self.now.as_str() >= due {
                let mut r = row(&mut rows);
                bump(&mut r, "open_overdue");
                rows.insert(key, r);
            }
        }
        let rate = |r: &Value| -> Value {
            let ok = r["on_time"].as_u64().unwrap_or_default();
            let n = ok + r["late"].as_u64().unwrap_or_default();
            if n == 0 {
                return Value::Null;
            }
            let q = Decimal::parse(&ok.to_string())
                .and_then(|a| {
                    a.div(
                        &Decimal::parse(&n.to_string())?,
                        4,
                        ubos_types::decimal::Rounding::HalfEven,
                    )
                })
                .map(|d| d.format(Some(4)));
            q.map(Value::String).unwrap_or(Value::Null)
        };
        let mut out = Vec::new();
        let mut total = json!({"completed": 0, "on_time": 0, "late": 0, "open_overdue": 0});
        for (_, mut r) in rows {
            r["attainment"] = rate(&r);
            for k in ["completed", "on_time", "late", "open_overdue"] {
                total[k] = json!(
                    total[k].as_u64().unwrap_or_default() + r[k].as_u64().unwrap_or_default()
                );
            }
            out.push(r);
        }
        total["attainment"] = rate(&total);
        Ok(json!({"from": from, "to": to, "rows": out, "total": total}))
    }
}
