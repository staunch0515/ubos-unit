//! Policy authorization in the kernel (PH-1 step 13b, DEC-073; DSN-DVM-801…807, 820):
//! the entity model of the registry and the compiled policy set of a branch, cached by
//! their hashes (DSN-DVM-803); the decision of a request with the decision log
//! (DSN-DVM-805); the checks of the `Policy` objects a commit writes; the host-facing
//! enforcement points (`commit`, `read`, `query` and `merge` with a `principal`); and
//! the adapter operations `authorize`, `decision_log` and `policy_schema`. The
//! instruction-level enforcement is the executor's (`execution.rs`, `permit`). Without a
//! policy set on the branch the kernel keeps the `grants` of STD-ISA-005, so the released
//! vectors stand; with one, anything not permitted is denied (FR-IAM-041).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use ubos_authz::{Compiled, Decision, Model, Principal, Resource};
use ubos_model::Registry;
use ubos_types::{BpaError, content_id};
use ubos_version::{Repo, State, Tx};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The compiled policy engine state of an engine: the entity model of the current
/// registry and the policy sets by hash (DSN-DVM-803, DSN-DVM-806).
#[derive(Default)]
pub struct Cache {
    model: Option<(String, Arc<Model>)>,
    sets: HashMap<String, Arc<Compiled>>,
}

impl Cache {
    /// Forget everything (the registry changed).
    pub fn clear(&mut self) {
        self.model = None;
        self.sets.clear();
    }
}

/// A policy set in force: the model and the compiled policies.
pub type PolicySet = (Arc<Model>, Arc<Compiled>);

/// The `(name, source)` of the live, enabled `Policy` objects among the given object
/// states.
fn policy_sources(
    repo: &mut Repo<'_>,
    objects: &[(String, ubos_version::ObjectState)],
) -> Result<Vec<(String, String)>, BpaError> {
    let mut out = Vec::new();
    for (_, o) in objects {
        if o.class != "Policy" {
            continue;
        }
        let v = repo.version(&o.head)?;
        if v["tombstone"] == json!(true) || v["body"].get("enabled") == Some(&json!(false)) {
            continue;
        }
        let name = s(&v["body"], "name").unwrap_or(&o.slug).to_string();
        let source = s(&v["body"], "source").unwrap_or_default().to_string();
        out.push((name, source));
    }
    out.sort();
    Ok(out)
}

/// The registry hash the model is keyed by.
fn registry_hash(reg: &Registry) -> Result<String, BpaError> {
    content_id("ubos.chunk.v1", &json!(reg.declarations()))
}

impl Engine {
    /// The entity model of the current registry (DSN-DVM-802), cached by its hash.
    pub(crate) fn authz_model(&mut self) -> Result<Arc<Model>, BpaError> {
        let h = registry_hash(&self.registry)?;
        if let Some((hash, m)) = &self.authz.model
            && *hash == h
        {
            return Ok(m.clone());
        }
        let m = Arc::new(ubos_authz::model_of(&self.registry)?);
        self.authz.model = Some((h, m.clone()));
        Ok(m)
    }

    fn compiled_for(
        &mut self,
        sources: Vec<(String, String)>,
    ) -> Result<Option<PolicySet>, BpaError> {
        if sources.is_empty() {
            return Ok(None);
        }
        let model = self.authz_model()?;
        let key = content_id(
            "ubos.chunk.v1",
            &json!({"schema": model.hash, "policies": sources}),
        )?;
        if let Some(c) = self.authz.sets.get(&key) {
            return Ok(Some((model, c.clone())));
        }
        let c = Arc::new(ubos_authz::compile(&model, &sources)?);
        self.authz.sets.insert(key, c.clone());
        Ok(Some((model, c)))
    }

    /// The policy set in force on a branch at an ordinal (`None` when the branch holds
    /// no enabled policy).
    pub(crate) fn policy_set_at(
        &mut self,
        branch: &str,
        cseq: Option<u64>,
    ) -> Result<Option<PolicySet>, BpaError> {
        let sources = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let c = match cseq {
                Some(c) => c,
                None => repo.ordinal(branch, None)?,
            };
            let objects: Vec<(String, ubos_version::ObjectState)> = repo
                .objects_in(branch, c, Some("Policy"))?
                .into_iter()
                .collect();
            policy_sources(&mut repo, &objects)?
        };
        self.compiled_for(sources)
    }

    /// The policy set in force in a materialised state.
    pub(crate) fn policy_set_in(&mut self, st: &State) -> Result<Option<PolicySet>, BpaError> {
        let objects: Vec<(String, ubos_version::ObjectState)> = st
            .objects
            .iter()
            .filter(|(_, o)| o.class == "Policy")
            .map(|(id, o)| (id.clone(), o.clone()))
            .collect();
        let sources = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            policy_sources(&mut repo, &objects)?
        };
        self.compiled_for(sources)
    }

    /// Decide a request (DSN-DVM-801) and log a denial (DSN-DVM-805; allows of actions
    /// flagged sensitive come with the security log of step 13c).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decide_access(
        &mut self,
        set: &PolicySet,
        branch: &str,
        principal: &Principal,
        action: &str,
        resource: &Resource,
        ctx: &Map<String, Value>,
        process: Option<&str>,
    ) -> Result<Decision, BpaError> {
        let vae = self.store.authority();
        let d = ubos_authz::authorize(&set.0, &set.1, principal, action, resource, &vae, ctx)?;
        if !d.allow {
            let res = match resource {
                Resource::Vae => json!({"kind": "vae"}),
                Resource::Branch { name, .. } => json!({"kind": "branch", "branch": name}),
                Resource::Class { name } => json!({"kind": "class", "class": name}),
                Resource::Object { oid, class, .. } => {
                    json!({"kind": "object", "class": class, "object_id": oid})
                }
            };
            let mut rec = json!({"principal": principal.id, "action": action, "resource": res, "branch": branch,
                "outcome": "deny", "policies": d.policies, "at": self.now});
            if let Some(p) = process {
                rec["process_id"] = json!(p);
            }
            if !d.errors.is_empty() {
                rec["errors"] = json!(d.errors);
            }
            self.security_append("authz.deny", rec.clone())?;
            self.decision_log.push(rec);
        }
        Ok(d)
    }

    /// The principal of a host or adapter input (`principal`, `principal_kind`, `role`
    /// or `roles`, `auth_level`), or none when the input names no principal.
    pub(crate) fn principal_of(input: &Value) -> Option<Principal> {
        let id = s(input, "principal")?;
        let mut roles: Vec<String> = input
            .get("roles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if let Some(r) = s(input, "role") {
            roles.push(r.to_string());
        }
        Some(Principal {
            id: id.to_string(),
            kind: s(input, "principal_kind").unwrap_or("human").to_string(),
            roles,
            auth_level: s(input, "auth_level").map(str::to_string),
        })
    }

    /// The decision context of a host or adapter input.
    pub(crate) fn access_ctx(
        &mut self,
        input: &Value,
        branch: &str,
    ) -> Result<Map<String, Value>, BpaError> {
        let kind = self.branch_meta(branch)?.kind;
        let mut ctx = Map::new();
        ctx.insert(
            "principal_kind".into(),
            json!(s(input, "principal_kind").unwrap_or("human")),
        );
        ctx.insert(
            "channel".into(),
            json!(s(input, "channel").unwrap_or("api")),
        );
        ctx.insert("vae".into(), json!(self.store.authority()));
        ctx.insert("branch_kind".into(), json!(kind));
        for k in ["role", "auth_level"] {
            if let Some(v) = s(input, k) {
                ctx.insert(k.into(), json!(v));
            }
        }
        Ok(ctx)
    }

    /// The object resource of an object state and its head version.
    pub(crate) fn object_resource(
        &mut self,
        branch: &str,
        oid: &str,
        o: &ubos_version::ObjectState,
        version: &Value,
    ) -> Result<Resource, BpaError> {
        let kind = self.branch_meta(branch)?.kind;
        let lineage = self.registry.class(&o.class)?.linearization.clone();
        Ok(Resource::Object {
            oid: oid.to_string(),
            class: o.class.clone(),
            lineage,
            branch: branch.to_string(),
            branch_kind: kind,
            slug: o.slug.clone(),
            state: s(version, "state").map(str::to_string),
            body: version["body"].as_object().cloned().unwrap_or_default(),
        })
    }

    /// Enforce an action of a host input on a resource: `IAM.NOT_FOUND` when the principal
    /// may not read the resource (DSN-DVM-804), `IAM.DENIED` otherwise; nothing when the
    /// branch has no policy set or the input names no principal.
    pub(crate) fn enforce(
        &mut self,
        set: &PolicySet,
        branch: &str,
        principal: &Principal,
        action: &str,
        resource: &Resource,
        ctx: &Map<String, Value>,
    ) -> Result<(), BpaError> {
        let d = self.decide_access(set, branch, principal, action, resource, ctx, None)?;
        if d.allow {
            return Ok(());
        }
        let read_actions = ["read", "read_history", "reflect", "explain"];
        if read_actions.contains(&action) {
            return Err(BpaError::new("IAM.NOT_FOUND", "not visible"));
        }
        let visible = self.decide_access(set, branch, principal, "read", resource, ctx, None)?;
        if !visible.allow {
            return Err(BpaError::new("IAM.NOT_FOUND", "not visible"));
        }
        Err(BpaError::with_details(
            "IAM.DENIED",
            json!({"action": action, "policies": d.policies}),
        ))
    }

    /// The commit of a host input (`principal` given): every change is authorized against
    /// its class or object and the branch (DSN-DVM-807) before anything is written.
    pub(crate) fn enforce_commit(&mut self, input: &Value, branch: &str) -> Result<(), BpaError> {
        let Some(principal) = Engine::principal_of(input) else {
            return Ok(());
        };
        let Some(set) = self.policy_set_at(branch, None)? else {
            return Ok(());
        };
        let ctx = self.access_ctx(input, branch)?;
        let kind = self.branch_meta(branch)?.kind;
        let changes = input
            .get("changes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for ch in &changes {
            let action = match s(ch, "op") {
                Some("create") => "create",
                Some("end") => "end",
                Some("append") => "append",
                Some("reverse" | "adjust") => "correct",
                _ => "update",
            };
            let resource = match s(ch, "object_id")
                .filter(|_| !matches!(s(ch, "op"), Some("create" | "append")))
            {
                Some(oid) => {
                    let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                    let c = repo.ordinal(branch, None)?;
                    match repo.object_at(branch, Some(c), oid)? {
                        Some(o) => {
                            let v = repo.version(&o.head)?;
                            self.object_resource(branch, oid, &o, &v)?
                        }
                        None => Resource::Branch {
                            name: branch.to_string(),
                            kind: kind.clone(),
                        },
                    }
                }
                None => match s(ch, "class") {
                    Some(c) => Resource::Class {
                        name: c.to_string(),
                    },
                    None => Resource::Branch {
                        name: branch.to_string(),
                        kind: kind.clone(),
                    },
                },
            };
            self.enforce(&set, branch, &principal, action, &resource, &ctx)?;
            let on_branch = Resource::Branch {
                name: branch.to_string(),
                kind: kind.clone(),
            };
            self.enforce(&set, branch, &principal, action, &on_branch, &ctx)?;
        }
        Ok(())
    }

    /// `authorize` (adapter operation, kernel-only): decide `{branch, principal, action,
    /// resource: {class | object_id}, …}` against the policy set of the branch.
    pub fn authorize_op(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let Some(principal) = Engine::principal_of(input) else {
            return Err(BpaError::new("MODEL.DECLARATION_INCOMPLETE", "principal"));
        };
        let action = s(input, "action").unwrap_or_default().to_string();
        let kind = self.branch_meta(&branch)?.kind;
        let res_in = input.get("resource").cloned().unwrap_or(json!({}));
        let resource = if let Some(oid) = s(&res_in, "object_id") {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let c = repo.ordinal(&branch, None)?;
            let Some(o) = repo.object_at(&branch, Some(c), oid)? else {
                return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
            };
            let v = repo.version(&o.head)?;
            self.object_resource(&branch, oid, &o, &v)?
        } else if let Some(c) = s(&res_in, "class") {
            Resource::Class {
                name: c.to_string(),
            }
        } else if s(&res_in, "kind") == Some("vae") {
            Resource::Vae
        } else {
            Resource::Branch {
                name: branch.clone(),
                kind,
            }
        };
        let ctx = self.access_ctx(input, &branch)?;
        match self.policy_set_at(&branch, None)? {
            None => Ok(json!({"allow": true, "policies": [], "policy_set": null})),
            Some(set) => {
                let d =
                    self.decide_access(&set, &branch, &principal, &action, &resource, &ctx, None)?;
                let mut out =
                    json!({"allow": d.allow, "policies": d.policies, "policy_set": set.1.hash});
                if !d.errors.is_empty() {
                    out["errors"] = json!(d.errors);
                }
                Ok(out)
            }
        }
    }

    /// `decision_log` (adapter operation): the decision records since the last call
    /// (DSN-DVM-805; the host delivers them to the security log).
    pub fn drain_decision_log(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.decision_log)
    }

    /// `policy_schema` (adapter operation): the Cedar schema of the current model.
    pub fn policy_schema(&mut self) -> Result<Value, BpaError> {
        let m = self.authz_model()?;
        Ok(json!({"schema": m.text, "hash": m.hash, "actions": m.actions}))
    }
}

/// STD-CLS-004-like check of the `Policy` objects a commit writes (DSN-DVM-803): the
/// source parses and validates against the entity model of the registry.
pub(crate) fn check_written(
    repo: &mut Repo<'_>,
    tx: &Tx,
    reg: &Registry,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    let mut model: Option<Model> = None;
    for (oid, vh) in written {
        if tx.state.objects.get(oid).map(|o| o.class.as_str()) != Some("Policy") {
            continue;
        }
        let v = repo.version_in(tx, vh)?;
        if v["tombstone"] == json!(true) {
            continue;
        }
        let name = s(&v["body"], "name").unwrap_or_default().to_string();
        let source = s(&v["body"], "source").unwrap_or_default().to_string();
        if model.is_none() {
            model = Some(ubos_authz::model_of(reg)?);
        }
        ubos_authz::check_policy(model.as_ref().expect("model"), &name, &source)?;
    }
    Ok(())
}
