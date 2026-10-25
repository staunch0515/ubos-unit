//! Deferred commits and approvals (PH-2 step 4b, DEC-106): pending changes of port calls
//! whose decisions require steps (STD-TXN-060, BPA 1.0-rc8, DSN-DVM-550, -551, -552), their
//! requirements (STD-LGS-041, DSN-DVM-740), separation of duties (STD-LGS-042, DSN-DVM-741)
//! and approval evidence (STD-LGS-043, DSN-DVM-742).
//!
//! A pending change is kept outside the branch state, as the reference model keeps it: a
//! record under `kernel_meta` key `pending_change:<id>`, listed under `pending_changes`. The
//! committed port calls of an object (the duties of STD-LGS-042) are listed under
//! `duties:<object id>`.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use ubos_types::{BpaError, content_id};
use ubos_version::state::Repo;

thread_local! {
    /// The `require` decisions of the running process when it was called with `defer`.
    static DEFERRING: RefCell<Option<Vec<(String, String, Value)>>> = const { RefCell::new(None) };
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// Start collecting the `require` decisions of a process (`on`: it was called with
/// `defer` and is not a replay), or stop.
pub(crate) fn begin(on: bool) {
    DEFERRING.with(|d| *d.borrow_mut() = on.then(Vec::new));
}

/// The `require` decisions collected since `begin`; collecting stops.
pub(crate) fn take() -> Vec<(String, String, Value)> {
    DEFERRING.with(|d| d.borrow_mut().take().unwrap_or_default())
}

/// STD-TXN-060: record a `require` decision of a deferring process; `false` when the
/// process does not defer, and the decision stops the call.
pub(crate) fn defer(oid: &str, port: &str, r: &Value) -> bool {
    DEFERRING.with(|d| match d.borrow_mut().as_mut() {
        Some(list) => {
            list.push((oid.to_string(), port.to_string(), r.clone()));
            true
        }
        None => false,
    })
}

/// STD-LGS-041: the requirements of the `require` outcomes of a deferred process, one item
/// per step and object, the approval items with their specification.
pub(crate) fn requirements(decisions: &[(String, String, Value)]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for (oid, port, r) in decisions {
        for step in r["steps"].as_array().into_iter().flatten() {
            let Some(step) = step.as_str() else { continue };
            let mut item = json!({"step": step, "object_id": oid, "port": port});
            if step == "approval" {
                let spec = r["explanations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|e| s(e, "name") == Some("approvals") && e["value"].is_object())
                    .map(|e| e["value"].clone())
                    .unwrap_or_else(|| json!({}));
                item["count"] = spec.get("count").cloned().unwrap_or(json!(1));
                item["order"] = spec.get("order").cloned().unwrap_or(json!("any"));
                for k in ["roles", "principals", "incompatible", "expires_in"] {
                    if let Some(v) = spec.get(k).filter(|v| !v.is_null()) {
                        item[k] = v.clone();
                    }
                }
            }
            if !out
                .iter()
                .any(|x| x["step"] == item["step"] && s(x, "object_id") == Some(oid))
            {
                out.push(item);
            }
        }
    }
    out
}

/// `now` plus a duration, as an instant.
fn plus(now: &str, duration: &str) -> Result<String, BpaError> {
    let us = ubos_expr::value::parse_duration(duration)?;
    let t = ubos_types::time::parse_instant(now)?
        + chrono::Duration::microseconds(i64::try_from(us).unwrap_or(i64::MAX));
    Ok(ubos_types::time::format_instant(&t))
}

fn denied(details: Value) -> BpaError {
    BpaError::with_details("IAM.DENIED", details)
}

/// The members of a pending change shown by `pending_change` and `approve`.
fn view(rec: &Value) -> Value {
    let mut out = Map::new();
    for k in [
        "pending_change_id",
        "target_branch",
        "preview",
        "requirements",
        "approvals",
        "state",
    ] {
        if let Some(v) = rec.get(k) {
            out.insert(k.into(), v.clone());
        }
    }
    for k in [
        "expires_at",
        "voided",
        "changed_since",
        "invalid_reason",
        "cseq",
    ] {
        if let Some(v) = rec.get(k).filter(|v| !v.is_null()) {
            out.insert(k.into(), v.clone());
        }
    }
    Value::Object(out)
}

impl Engine {
    /// Adapter operation `principal` (STD-ISA-071, BPA 1.0-rc8): register a principal's
    /// clearances and roles in the adapter's directory, under `kernel_meta`
    /// `principal:<did>`. The roles are those of STD-LGS-060…062 (BPA 1.0-rc10).
    pub fn principal_entry(&mut self, i: &Value) -> Result<Value, BpaError> {
        let Some(did) = s(i, "did") else {
            return Err(BpaError::new("MODEL.DECLARATION_INCOMPLETE", "did"));
        };
        let clearances = i.get("clearances").cloned().unwrap_or(json!(["internal"]));
        let ok = clearances.as_array().is_some_and(|l| {
            l.iter().all(|c| {
                c.as_str()
                    .is_some_and(|c| ubos_types::values::LEVELS.contains(&c))
            })
        });
        if !ok {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "clearances are classification levels",
            ));
        }
        let mut roles: Vec<String> = i["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect();
        roles.sort();
        let entry = json!({"clearances": clearances, "roles": roles});
        self.store.put_meta(&format!("principal:{did}"), &entry)?;
        let mut out = entry;
        out["did"] = json!(did);
        Ok(out)
    }

    fn pc_key(id: &str) -> String {
        format!("pending_change:{id}")
    }

    fn pc_put(&mut self, rec: &Value) -> Result<(), BpaError> {
        let id = s(rec, "pending_change_id").unwrap_or_default().to_string();
        self.store.put_meta(&Self::pc_key(&id), rec)
    }

    /// STD-TXN-060: store the changes of a deferring process as a pending change; returns
    /// its view.
    pub(crate) fn defer_changes(
        &mut self,
        branch: &str,
        process_id: &str,
        initiator: &str,
        changes: &[Value],
        decisions: &[(String, String, Value)],
        extra: Value,
    ) -> Result<Value, BpaError> {
        let reqs = requirements(decisions);
        let stripped: Vec<Value> = changes
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c.as_object_mut().map(|m| m.remove("derivation"));
                c
            })
            .collect();
        let preview = content_id(
            "ubos.chunk.v1",
            &json!({"branch": branch, "changes": stripped}),
        )?;
        let mut rec = json!({"pending_change_id": process_id, "target_branch": branch,
                             "changes": changes, "preview": preview, "requirements": reqs,
                             "approvals": [], "state": "pending", "initiator": initiator,
                             "process_id": process_id});
        let mut exp: Option<String> = None;
        for r in &reqs {
            if let Some(d) = s(r, "expires_in") {
                let at = plus(&self.now, d)?;
                if exp.as_ref().is_none_or(|e| at < *e) {
                    exp = Some(at);
                }
            }
        }
        if let Some(e) = exp {
            rec["expires_at"] = json!(e);
        }
        for (k, v) in extra.as_object().into_iter().flatten() {
            rec[k] = v.clone();
        }
        self.pc_put(&rec)?;
        let mut ids = self
            .store
            .meta("pending_changes")?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if !ids.iter().any(|x| x.as_str() == Some(process_id)) {
            ids.push(json!(process_id));
            self.store.put_meta("pending_changes", &Value::Array(ids))?;
        }
        Ok(view(&rec))
    }

    /// A pending change, refreshed when still `pending` (FR-FLOW-045): expired at its
    /// expiry, invalidated when a subject changed since the request.
    pub(crate) fn pc(&mut self, id: &str) -> Result<Value, BpaError> {
        let Some(mut rec) = self.store.meta(&Self::pc_key(id))? else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown pending change"));
        };
        if s(&rec, "state") == Some("pending") && self.pc_refresh(&mut rec)? {
            self.pc_put(&rec)?;
        }
        Ok(rec)
    }

    /// Expiry and invalidation of a pending change; whether it changed.
    fn pc_refresh(&mut self, rec: &mut Value) -> Result<bool, BpaError> {
        if let Some(e) = s(rec, "expires_at")
            && self.now.as_str() >= e
        {
            rec["state"] = json!("expired");
            return Ok(true);
        }
        let branch = s(rec, "target_branch").unwrap_or("main").to_string();
        let mut changed = Vec::new();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        for c in rec["changes"].as_array().cloned().unwrap_or_default() {
            let (Some(base), Some(oid)) = (s(&c, "base"), s(&c, "object_id")) else {
                continue;
            };
            let o = repo.object_at(&branch, None, oid)?;
            if o.as_ref().is_some_and(|o| o.head == base) {
                continue;
            }
            let approved = repo.version(base)?["body"].clone();
            let current = match &o {
                Some(o) => repo.version(&o.head)?["body"].clone(),
                None => Value::Null,
            };
            changed.push(json!({"object_id": oid, "approved": approved, "current": current}));
        }
        if changed.is_empty() {
            return Ok(false);
        }
        rec["state"] = json!("invalidated");
        rec["invalid_reason"] = json!("TXN.HEAD_MOVED");
        rec["changed_since"] = Value::Array(changed);
        if rec["approvals"].as_array().is_some_and(|a| !a.is_empty()) {
            rec["voided"] = rec["approvals"].take();
            rec["approvals"] = json!([]);
        }
        Ok(true)
    }

    /// Adapter operation `pending_change` (STD-TXN-060).
    pub fn pending_change(&mut self, i: &Value) -> Result<Value, BpaError> {
        let id = s(i, "pending_change_id").unwrap_or_default();
        Ok(view(&self.pc(id)?))
    }

    /// STD-LGS-042: record a committed port call of an object (a duty of its initiator).
    pub(crate) fn record_duty(
        &mut self,
        oid: &str,
        port: &str,
        initiator: &str,
    ) -> Result<(), BpaError> {
        let key = format!("duties:{oid}");
        let mut list = self
            .store
            .meta(&key)?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let d = json!({"port": port, "initiator": initiator});
        if !list.contains(&d) {
            list.push(d);
            self.store.put_meta(&key, &Value::Array(list))?;
        }
        Ok(())
    }

    /// STD-LGS-041 eligibility and STD-LGS-042 separation of duties of an approval by
    /// `approver`, acting with `i` (`on_behalf_of`, `roles`).
    pub(crate) fn eligible(
        &mut self,
        rec: &Value,
        req: &Value,
        i: &Value,
        approver: &str,
    ) -> Result<(), BpaError> {
        let mut actors = vec![approver.to_string()];
        if let Some(b) = s(i, "on_behalf_of") {
            actors.push(b.to_string());
        }
        // STD-LGS-060 (1.0-rc10): through a delegation, eligibility is the delegator's
        let who = match (i.get("delegation"), s(i, "on_behalf_of")) {
            (Some(d), Some(b)) if !d.is_null() => b.to_string(),
            _ => approver.to_string(),
        };
        let initiator = s(rec, "initiator").unwrap_or_default();
        if actors.iter().any(|a| a == initiator) {
            return Err(denied(
                json!({"sod": "maker_checker", "principal": initiator}),
            ));
        }
        let approvals = rec["approvals"].as_array().cloned().unwrap_or_default();
        for a in &actors {
            if approvals
                .iter()
                .any(|x| s(x, "approver") == Some(a) || s(x, "on_behalf_of") == Some(a))
            {
                return Err(denied(
                    json!({"sod": "one_decision_per_principal", "principal": a}),
                ));
            }
        }
        if let Some(oid) = s(req, "object_id") {
            let duties = self
                .store
                .meta(&format!("duties:{oid}"))?
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            for port in req["incompatible"].as_array().into_iter().flatten() {
                for d in &duties {
                    let who = s(d, "initiator").unwrap_or_default();
                    if d["port"] == *port && actors.iter().any(|a| a == who) {
                        return Err(denied(
                            json!({"sod": "incompatible_duty", "duty": port, "principal": who}),
                        ));
                    }
                }
            }
        }
        let principals: Vec<&str> = req["principals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let roles: Vec<&str> = req["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !principals.is_empty() || !roles.is_empty() {
            let held = i["roles"].as_array().cloned().unwrap_or_default();
            let ok = principals.contains(&who.as_str())
                || held
                    .iter()
                    .any(|r| r.as_str().is_some_and(|r| roles.contains(&r)));
            if !ok {
                return Err(denied(
                    json!({"approval": "not_eligible", "principal": approver}),
                ));
            }
        }
        if s(req, "order") == Some("sequence") && !principals.is_empty() {
            let n = approvals
                .iter()
                .filter(|a| s(a, "step") == Some("approval"))
                .count();
            if principals.get(n) != Some(&who.as_str()) {
                return Err(denied(
                    json!({"approval": "out_of_sequence", "principal": approver}),
                ));
            }
        }
        Ok(())
    }

    /// STD-LGS-060 (1.0-rc10): an approval through a delegation, checked against the port
    /// and class of the requirement and the parameters of the deferred call; returns the
    /// delegator and the delegated roles.
    fn approval_delegation(
        &mut self,
        rec: &Value,
        req: &Value,
        id: &str,
        i: &Value,
        approver: &str,
    ) -> Result<(String, Vec<String>), BpaError> {
        let branch = s(rec, "target_branch").unwrap_or("main").to_string();
        let class = match s(req, "object_id") {
            Some(oid) => Repo::new(self.store.as_mut(), &self.registry)
                .object_at(&branch, None, oid)?
                .map(|o| o.class),
            None => None,
        };
        let params = match s(rec, "process_id") {
            Some(pid) => self
                .journal_full(pid)
                .ok()
                .map(|j| j["invocation"]["params"].clone()),
            None => None,
        };
        let body = self.use_delegation(
            &branch,
            id,
            approver,
            s(i, "on_behalf_of"),
            crate::work::Use {
                port: s(req, "port"),
                class: class.as_deref(),
                task_type: None,
                params: params.as_ref(),
            },
        )?;
        let roles = body["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect();
        Ok((s(&body, "delegator").unwrap_or_default().to_string(), roles))
    }

    /// Whether every requirement of a pending change is met.
    fn satisfied(rec: &Value) -> bool {
        let approvals: Vec<&Value> = rec["approvals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| s(a, "decision") == Some("approve"))
            .collect();
        rec["requirements"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|r| match s(r, "step") {
                Some("approval") => {
                    let n = approvals
                        .iter()
                        .filter(|a| s(a, "step") == Some("approval"))
                        .count() as u64;
                    n >= r["count"].as_u64().unwrap_or(1)
                }
                Some("second_person") => !approvals.is_empty(),
                Some("signature") => approvals
                    .iter()
                    .any(|a| a.get("signature").is_some_and(|x| !x.is_null())),
                _ => true,
            })
    }

    /// Adapter operation `approve` (STD-LGS-043): record an approval or a rejection of a
    /// pending change; the final approval applies the change in a new process (STD-TXN-060).
    pub fn approve(&mut self, i: &Value) -> Result<Value, BpaError> {
        let id = s(i, "pending_change_id").unwrap_or_default().to_string();
        let mut rec = self.pc(&id)?;
        if s(&rec, "state") != Some("pending") {
            return Ok(view(&rec));
        }
        let Some(approver) = s(i, "approver")
            .filter(|a| !a.is_empty())
            .map(str::to_string)
        else {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "an approval names its approver",
            ));
        };
        let decision = s(i, "decision").unwrap_or("approve").to_string();
        if decision != "approve" && decision != "reject" {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "decision is approve or reject",
            ));
        }
        let reqs = rec["requirements"].as_array().cloned().unwrap_or_default();
        let step = if reqs.iter().any(|r| s(r, "step") == Some("approval")) {
            "approval".to_string()
        } else {
            reqs.first()
                .and_then(|r| s(r, "step"))
                .unwrap_or("approval")
                .to_string()
        };
        let req = reqs
            .iter()
            .find(|r| s(r, "step") == Some(step.as_str()))
            .cloned()
            .unwrap_or_else(|| json!({"step": step, "object_id": null}));
        let i = match s(i, "delegation") {
            Some(d) => {
                let (behalf, roles) = self.approval_delegation(&rec, &req, d, i, &approver)?;
                let mut x = i.clone();
                x["on_behalf_of"] = json!(behalf);
                x["roles"] = json!(roles);
                x
            }
            None => i.clone(),
        };
        let i = &i;
        self.eligible(&rec, &req, i, &approver)?;
        let mut a = json!({"approver": approver, "decision": decision, "preview": rec["preview"],
                           "at": self.now, "auth_level": s(i, "auth_level").unwrap_or("basic"),
                           "step": step});
        for k in ["on_behalf_of", "delegation", "comment", "signature"] {
            if let Some(v) = i.get(k).filter(|v| !v.is_null()) {
                a[k] = v.clone();
            }
        }
        rec["approvals"].as_array_mut().expect("approvals").push(a);
        if decision == "reject" {
            rec["state"] = json!("rejected");
        } else if Self::satisfied(&rec) {
            self.pc_apply(&mut rec)?;
        }
        self.pc_put(&rec)?;
        Ok(view(&rec))
    }

    /// The final approval: a new process re-validates and commits the changes with the
    /// approvals referenced, or the pending change becomes invalidated (DSN-DVM-551).
    fn pc_apply(&mut self, rec: &mut Value) -> Result<(), BpaError> {
        let approvers: Vec<Value> = rec["approvals"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| a["approver"].clone())
            .collect();
        let mut process = rec.get("process").cloned().unwrap_or_else(|| json!({}));
        process["kind"] = json!("action");
        process["initiator"] = rec["initiator"].clone();
        process["approvals"] = json!([{"pending_change_id": rec["pending_change_id"],
                                       "preview": rec["preview"], "approvers": approvers}]);
        let input = json!({"branch": rec["target_branch"], "changes": rec["changes"],
                           "process": process});
        // STD-KIND-025 (1.0-rc10): a governed reopening leaves `closed` as the period
        // operation does
        let committed = if rec["period_op"] == json!(true) {
            crate::periods::as_operation("closed", "open", || self.commit(&input))
        } else {
            self.commit(&input)
        };
        match committed {
            Ok(res) => {
                rec["state"] = json!("applied");
                rec["cseq"] = res["cseq"].clone();
            }
            Err(e) => {
                if e.code == "TXN.HEAD_MOVED" {
                    self.pc_refresh(rec)?;
                }
                rec["state"] = json!("invalidated");
                rec["invalid_reason"] = json!(e.code);
            }
        }
        Ok(())
    }
}
