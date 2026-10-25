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
    fn delegation_state(&self, body: &Value) -> &'static str {
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
