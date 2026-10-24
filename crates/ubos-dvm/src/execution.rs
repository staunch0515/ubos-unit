//! Execution of logic (PH-1 step 5b, DEC-056): the store-facing half of the instructions
//! of ABI 1.0 ([`EngineHost`], STD-ISA-010…), the `execute` operation (one process per
//! invocation, STD-TXN-001; automatic retry on `TXN.HEAD_MOVED` and
//! `TXN.READ_SET_CHANGED`, STD-TXN-023), its journal and metering records
//! (STD-CTX-005, -011) and the logic-asset rules of STD-CTX-022 and -030. The interpreter
//! and the gateway live in `ubos-runtime`.

use crate::relations::{self, extensions_of};
use crate::stateful::{Engine, commit_on};
use crate::transactions::effects_of;
use crate::versioning::{Process, process, record};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use ubos_expr::{Ty, Val, from_json};
use ubos_model::Registry;
use ubos_runtime::isa;
use ubos_runtime::program::{self, PROTECTED};
use ubos_runtime::{Core, Frame, Host, Instruction, Pending, Replay};
use ubos_types::{BpaError, canonical, content_id, names};
use ubos_version::{ObjectState, Repo, Segment, State, at};

/// Codes that start another attempt (STD-TXN-023).
const RETRY_CODES: &[&str] = &["TXN.HEAD_MOVED", "TXN.READ_SET_CHANGED"];
/// Attempts per invocation unless `max_attempts` is given.
const MAX_ATTEMPTS: u64 = 5;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

fn not_found(m: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", m)
}

fn last_segment(v: &Value) -> String {
    v.as_str()
        .map(|c| c.rsplit('/').next().unwrap_or(c).to_string())
        .unwrap_or_default()
}

/// 32 bytes from lowercase hexadecimal.
fn unhex32(v: &Value, what: &str) -> Result<[u8; 32], BpaError> {
    let bad = || invalid(&format!("{what} is 32 bytes in hexadecimal"));
    let s = v.as_str().ok_or_else(bad)?;
    if s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
    }
    Ok(out)
}

/// The engine as the host of a run: the snapshot of the branch, the pending objects and
/// the instructions that read or change the store.
pub struct EngineHost<'a> {
    e: &'a mut Engine,
    /// The part of the pinned snapshot the process has read (PH-2 step 0b, DEC-087): the
    /// governance objects and logic assets, then each object on its first use; the whole
    /// snapshot on a branch with a parent line.
    view: Option<State>,
    /// `(branch, cseq)` of the pinned snapshot.
    coord: Option<(String, u64)>,
    /// The view holds the whole snapshot.
    full: bool,
    /// Objects already looked up in the index (present or not).
    tried: HashSet<String>,
    /// The transitions and guards of the process for its commit (STD-ISA-017, since 1.0-rc3).
    pub(crate) flow: ubos_flow::Flow,
    /// The policy set of the branch at the snapshot, resolved on the first enforcement
    /// (`None` inside: no policies, grants only; DSN-DVM-801, step 13b).
    policies: Option<Option<crate::authz::PolicySet>>,
    /// Decisions of this attempt by `(action, class, object)` (DSN-DVM-806).
    decisions: BTreeMap<(String, String, Option<String>), Option<&'static str>>,
}

impl<'a> EngineHost<'a> {
    pub(crate) fn new(e: &'a mut Engine) -> EngineHost<'a> {
        EngineHost {
            e,
            view: None,
            coord: None,
            full: false,
            tried: HashSet::new(),
            flow: ubos_flow::Flow::default(),
            policies: None,
            decisions: BTreeMap::new(),
        }
    }

    // ---- authorization (STD-ISA-005; DSN-DVM-801, -804, -806, -807; step 13b) ----

    /// Authorize an action of the process on a class or an object: the grants of the
    /// process (STD-ISA-005) and, when the branch holds a policy set, the policies — on
    /// the resource and on the branch (DSN-DVM-807). A denial is `IAM.DENIED`, or
    /// `IAM.NOT_FOUND` for a read-type action and for a resource the principal may not
    /// read (DSN-DVM-804). Decisions are cached per attempt (DSN-DVM-806).
    fn permit(
        &mut self,
        core: &mut Core,
        action: &str,
        cls: &str,
        oid: Option<&str>,
    ) -> Result<(), BpaError> {
        core.authorize(action, cls)?;
        if action == "none" {
            return Ok(());
        }
        if self.policies.is_none() {
            let st = self.state(core)?.clone();
            let set = self.e.policy_set_in(&st)?;
            self.policies = Some(set);
        }
        let Some(set) = self.policies.as_ref().and_then(|p| p.clone()) else {
            return Ok(());
        };
        let key = (action.to_string(), cls.to_string(), oid.map(str::to_string));
        if let Some(code) = self.decisions.get(&key) {
            return match code {
                None => Ok(()),
                Some(c) => Err(BpaError::new(c, "policy decision")),
            };
        }
        let principal = ubos_authz::Principal {
            id: s(&core.context, "principal")
                .unwrap_or_default()
                .to_string(),
            kind: s(&core.context, "principal_kind")
                .unwrap_or("human")
                .to_string(),
            roles: s(&core.context, "role")
                .map(|r| vec![r.to_string()])
                .unwrap_or_default(),
            auth_level: s(&core.context, "auth_level").map(str::to_string),
        };
        let mut ctx = self
            .decision_ctx(core)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(a) = s(&core.context, "auth_level") {
            ctx.insert("auth_level".into(), json!(a));
        }
        let branch = core.branch.clone();
        let kind = self.e.branch_meta(&branch)?.kind;
        let resource = match oid {
            Some(oid) => {
                let body = self.head_body(core, oid)?;
                let state = self.object_state(core, oid)?;
                let slug = self.slug_of(core, oid)?;
                ubos_authz::Resource::Object {
                    oid: oid.to_string(),
                    class: cls.to_string(),
                    lineage: self.registry(core)?.class(cls)?.linearization.clone(),
                    branch: branch.clone(),
                    branch_kind: kind.clone(),
                    slug,
                    state,
                    body,
                }
            }
            None => ubos_authz::Resource::Class {
                name: cls.to_string(),
            },
        };
        let pid = core.process_id.clone();
        let outcome = (|| -> Result<(), BpaError> {
            for res in [
                &resource,
                &ubos_authz::Resource::Branch {
                    name: branch.clone(),
                    kind: kind.clone(),
                },
            ] {
                let d = self.e.decide_access(
                    &set,
                    &branch,
                    &principal,
                    action,
                    res,
                    &ctx,
                    Some(&pid),
                )?;
                if d.allow {
                    continue;
                }
                if program::READ_ACTIONS.contains(&action) {
                    return Err(BpaError::new("IAM.NOT_FOUND", "not visible"));
                }
                let visible = self.e.decide_access(
                    &set,
                    &branch,
                    &principal,
                    "read",
                    res,
                    &ctx,
                    Some(&pid),
                )?;
                if !visible.allow {
                    return Err(BpaError::new("IAM.NOT_FOUND", "not visible"));
                }
                return Err(BpaError::with_details(
                    "IAM.DENIED",
                    json!({"action": action, "policies": d.policies}),
                ));
            }
            Ok(())
        })();
        let code: Option<&'static str> = match &outcome {
            Ok(()) => None,
            Err(e) if e.code == "IAM.NOT_FOUND" => Some("IAM.NOT_FOUND"),
            Err(_) => Some("IAM.DENIED"),
        };
        self.decisions.insert(key, code);
        outcome
    }

    // ---- lifecycles (STD-CLS-004, since 1.0-rc3) ----

    /// The lifecycle in force for a class on the snapshot.
    fn lifecycle_of(
        &mut self,
        core: &mut Core,
        cls: &str,
    ) -> Result<Option<crate::flow::Lifecycle>, BpaError> {
        let st = self.state(core)?.clone();
        let reg = self.registry(core)?;
        let mut repo = self.repo();
        crate::flow::Lifecycles::of(&repo).lifecycle(&mut repo, None, &st, &reg, cls)
    }

    /// The lifecycle state of an object as this process sees it: after its pending
    /// transition, the initial state of a pending create, else the head version's.
    fn object_state(&mut self, core: &mut Core, oid: &str) -> Result<Option<String>, BpaError> {
        if let Some(m) = self.flow.transitions.get(oid) {
            return Ok(Some(m.to.clone()));
        }
        if let Some(p) = core.pending.get(oid).filter(|p| p.change.is_some()) {
            let cls = p.class.clone();
            return Ok(self
                .lifecycle_of(core, &cls)?
                .and_then(|(_, _, b)| s(&b, "initial").map(str::to_string)));
        }
        let head = self
            .object(core, oid)?
            .map(|o| o.head.clone())
            .ok_or_else(|| not_found("unknown object"))?;
        Ok(crate::flow::state_of(&self.version(&head)?))
    }

    /// STD-ISA-017: a lifecycle transition as the effect of the current port.
    fn commit_transition(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        let name = s(&Value::Object(a.clone()), "transition")
            .unwrap_or_default()
            .to_string();
        let frame = core.frames.last().cloned().unwrap_or_default();
        let reg = self.registry(core)?;
        let decl = frame.port.as_deref().and_then(|p| {
            reg.class(&cls)
                .ok()
                .and_then(|c| c.ports.get(p).map(|q| q.decl.clone()))
        });
        let Some(decl) = decl.filter(|p| s(p, "transition") == Some(name.as_str())) else {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"transition": name, "message": "the port does not name the transition"}),
            ));
        };
        let Some((_, _, lc)) = self.lifecycle_of(core, &cls)? else {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"transition": name, "message": "the class has no lifecycle"}),
            ));
        };
        let current = self.object_state(core, &oid)?;
        let Some(t) = ubos_flow::transition(&lc, &name)
            .filter(|t| ubos_flow::leaves(t, current.as_deref()))
            .cloned()
        else {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"transition": name, "from": current}),
            ));
        };
        let current = current.unwrap_or_default();
        let to = s(&t, "to").unwrap_or_default().to_string();
        let guards = t
            .get("guards")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let n_effects = ubos_flow::effects(&lc, &current, "exit").len()
            + ubos_flow::effects(&lc, &to, "entry").len();
        core.per_item(d, (guards.len() + n_effects) as u64)?;
        let mut body = Value::Object(self.head_body(core, &oid)?);
        let decls = ubos_flow::param_decls(&decl);
        let params = ubos_flow::params_json(&frame.params)?;
        let ctx = self.decision_ctx(core)?;
        let ctx = ctx.as_object().cloned().unwrap_or_default();
        let failed =
            ubos_flow::failed_transition_guards(&reg, &cls, &t, &decls, &body, &params, &ctx)?;
        if !failed.is_empty() {
            return Err(BpaError::with_details(
                "FLOW.GUARD_FAILED",
                json!({"transition": name, "failed": failed}),
            ));
        }
        let mut sets: Map<String, Value> = Map::new();
        for (state, kind) in [(current.as_str(), "exit"), (to.as_str(), "entry")] {
            for ef in ubos_flow::effects(&lc, state, kind) {
                let r = (|| -> Result<(), BpaError> {
                    if let Some(set) = ef.get("set").and_then(Value::as_object) {
                        for (f, text) in set {
                            if !reg.class(&cls)?.fields.contains_key(f) {
                                return Err(invalid(&format!("undeclared field {f}")));
                            }
                            let text = text.as_str().unwrap_or_default();
                            let v = ubos_flow::eval_value(
                                &reg,
                                &cls,
                                text,
                                &body,
                                Some((&decls, &params)),
                                Some(&ctx),
                            )?;
                            body[f] = v.clone();
                            sets.insert(f.clone(), v);
                        }
                    } else {
                        let logic = s(ef, "logic").unwrap_or_default();
                        let (asset_name, fname) = logic.split_once('#').unwrap_or((logic, ""));
                        let (asset, vh) = self.asset(core, asset_name)?;
                        let fr = Frame {
                            asset,
                            vh: Some(vh),
                            fname: Some(fname.to_string()),
                            oid: Some(oid.clone()),
                            this: Some(body.clone()),
                            cls: Some(cls.clone()),
                            port: frame.port.clone(),
                            impl_class: None,
                            params: BTreeMap::new(),
                        };
                        program::run_function(core, self, fr, &params)?;
                        body = Value::Object(self.head_body(core, &oid)?);
                    }
                    Ok(())
                })();
                if let Err(e) = r {
                    return Err(BpaError::with_details(
                        "FLOW.EFFECT_FAILED",
                        json!({"state": state, "effect": kind, "code": e.code}),
                    ));
                }
            }
        }
        for f in ubos_flow::field_rules(&lc, &to)
            .and_then(|r| r.get("required"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if body.get(f).is_none_or(Value::is_null) {
                return Err(BpaError::with_details(
                    "FLOW.FIELD_RULE_VIOLATED",
                    json!({"state": to, "field": f, "rule": "required"}),
                ));
            }
        }
        self.stage_update(core, &oid, sets.clone(), None, None, None)?;
        let moved = self.flow.transitions.entry(oid.clone()).or_default();
        moved.from = current.clone();
        moved.to = to.clone();
        moved.fields.extend(sets.keys().cloned());
        for g in &guards {
            self.flow
                .passed(&format!("{name}/{}", s(g, "name").unwrap_or_default()));
        }
        Ok(json!({"from": current, "to": to}))
    }

    /// STD-ISA-036: the ports of an object with their availability for this principal.
    fn read_actions(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        self.object(core, &oid)?;
        let st = self.state(core)?.clone();
        let lazy = self.lazy();
        let dctx = self.decision_ctx(core)?;
        let dctx = dctx.as_object().cloned().unwrap_or_default();
        let branch = core.branch.clone();
        let grants = core.grants.clone();
        // the principal's permission as `Core::authorize` decides it (STD-ISA-005)
        let mut permit = |action: &str, c: &str| match &grants {
            None => true,
            Some(g) => {
                action == "none"
                    || g.iter()
                        .any(|x| x == action || *x == format!("{action}:{c}"))
            }
        };
        let out = self
            .e
            .actions_in(&st, &branch, &oid, &dctx, Some(&mut permit), lazy)?;
        let n = out["actions"].as_array().map_or(0, Vec::len) as u64;
        core.per_item(d, n)?;
        Ok(out)
    }

    fn repo(&mut self) -> Repo<'_> {
        Repo::new(self.e.store.as_mut(), &self.e.registry)
    }

    /// The pinned snapshot of the branch (STD-TXN-002), as far as the process has read it:
    /// the objects a process reads as a whole (policies, lifecycles, class extensions,
    /// sheets and property declarations, logic assets) are loaded first, every other object
    /// on its first use ([`Self::object`]), so that a process costs what it reads and not
    /// the size of its branch (PH-2 step 0b, DEC-087). A branch with a parent line
    /// (STD-VER-080) is read as a whole view.
    fn state(&mut self, core: &mut Core) -> Result<&State, BpaError> {
        if !core.pins.contains_key(&core.branch) {
            let mut repo = self.repo();
            let head = repo.head(&core.branch)?;
            let id = repo.meta(head)?.commit_id;
            core.pins.insert(core.branch.clone(), id);
        }
        if self.view.is_none() {
            let branch = core.branch.clone();
            let parent_line = self.e.branch_meta(&branch)?.parent_line.is_some();
            let classes = self.whole_classes();
            let mut repo = self.repo();
            let snap = repo.select(&branch, &json!({"commit": core.pins[&branch]}))?;
            let cseq = repo.meta(snap.commit)?.cseq;
            let st = if parent_line {
                repo.view(&branch, &json!({"evaluated_at": core.pins}))?
                    .0
                    .state
            } else {
                let mut st = State::default();
                for c in &classes {
                    st.objects.extend(repo.objects_in(&branch, cseq, Some(c))?);
                }
                st
            };
            self.full = parent_line;
            self.coord = Some((branch, cseq));
            self.view = Some(st);
        }
        Ok(self.view.as_ref().expect("view"))
    }

    /// The classes a process reads as a whole: the governance classes and the logic
    /// asset classes.
    fn whole_classes(&self) -> Vec<String> {
        let mut out: Vec<String> = [
            "Policy",
            "Lifecycle",
            "ClassExtension",
            "GovernanceSheet",
            "PropertyDecl",
        ]
        .iter()
        .map(|c| c.to_string())
        .collect();
        out.extend(
            self.e
                .registry
                .classes()
                .filter(|c| c.linearization.iter().any(|x| x == "LogicAsset"))
                .map(|c| c.name.clone()),
        );
        out
    }

    /// The objects of a class at the pinned snapshot.
    fn snapshot_class(
        &mut self,
        core: &mut Core,
        class: &str,
    ) -> Result<Vec<(String, ObjectState)>, BpaError> {
        self.state(core)?;
        if self.full {
            return Ok(self
                .view
                .as_ref()
                .map(|v| {
                    v.objects
                        .iter()
                        .filter(|(_, o)| o.class == class)
                        .map(|(id, o)| (id.clone(), o.clone()))
                        .collect()
                })
                .unwrap_or_default());
        }
        let (branch, cseq) = self.coord.clone().expect("coordinate");
        Ok(self
            .repo()
            .objects_in(&branch, cseq, Some(class))?
            .into_iter()
            .collect())
    }

    /// An object of the pinned snapshot, read from the index on its first use.
    fn object(&mut self, core: &mut Core, oid: &str) -> Result<Option<ObjectState>, BpaError> {
        self.state(core)?;
        if !self.full
            && !self.tried.contains(oid)
            && !self
                .view
                .as_ref()
                .is_some_and(|v| v.objects.contains_key(oid))
        {
            let (branch, cseq) = self.coord.clone().expect("coordinate");
            let found = self
                .repo()
                .objects_in_ids(&branch, cseq, &[oid.to_string()])?;
            if let (Some(v), Some(o)) = (self.view.as_mut(), found.get(oid)) {
                v.objects.insert(oid.to_string(), o.clone());
            }
            self.tried.insert(oid.to_string());
        }
        Ok(self.view.as_ref().and_then(|v| v.objects.get(oid).cloned()))
    }

    /// The `:rel` coordinate of a cascade over the view: the snapshot, unless the view is
    /// whole.
    fn lazy(&self) -> Option<(String, Option<u64>)> {
        match (&self.coord, self.full) {
            (Some((b, c)), false) => Some((b.clone(), Some(*c))),
            _ => None,
        }
    }

    fn version(&mut self, vh: &str) -> Result<Value, BpaError> {
        self.repo().version(vh)
    }

    /// The registry with the extensions in force on the snapshot.
    fn registry(&mut self, core: &mut Core) -> Result<Registry, BpaError> {
        let st = self.state(core)?.clone();
        let exts = extensions_of(&mut self.repo(), &st)?;
        if exts.is_empty() {
            Ok(self.e.registry.clone())
        } else {
            self.e.registry.with_extensions(&exts)
        }
    }

    // ---- targets ----

    fn target_id(
        &mut self,
        core: &mut Core,
        t: &Value,
        missing_ok: bool,
    ) -> Result<Option<String>, BpaError> {
        if let Some(id) = t.as_str().filter(|x| names::matches("uuid", x)) {
            if core.pending.contains_key(id) || self.object(core, id)?.is_some() {
                return Ok(Some(id.to_string()));
            }
            if missing_ok {
                return Ok(None);
            }
            return Err(not_found("unknown object"));
        }
        if !t.as_str().is_some_and(|x| x.starts_with("ubos://")) {
            return Err(invalid("a target is a URI or an object id"));
        }
        let p = ubos_types::addr::parse_uri(t)?;
        if s(&p, "tenant") != Some(self.e.store.authority().as_str()) || p.get("vae").is_some() {
            return Err(not_found(
                "objects of other VAEs are reached only as published objects",
            ));
        }
        let (class, slug) = (
            s(&p, "class").unwrap_or_default(),
            s(&p, "slug").unwrap_or_default(),
        );
        if let Some((oid, _)) = core
            .pending
            .iter()
            .find(|(_, o)| o.class == class && o.slug == slug)
        {
            return Ok(Some(oid.clone()));
        }
        self.state(core)?;
        let found = if self.full {
            self.view.as_ref().and_then(|st| {
                st.objects
                    .iter()
                    .find(|(_, o)| o.class == class && o.slug == slug)
                    .map(|(id, o)| (id.clone(), o.clone()))
            })
        } else {
            let (branch, cseq) = self.coord.clone().expect("coordinate");
            self.repo().object_by_slug(&branch, cseq, class, slug)?
        };
        if let Some((oid, o)) = found {
            if let Some(v) = self.view.as_mut() {
                v.objects.insert(oid.clone(), o);
            }
            self.tried.insert(oid.clone());
            return Ok(Some(oid));
        }
        if missing_ok {
            return Ok(None);
        }
        Err(not_found(&format!(
            "no object at {}",
            s(&p, "canonical").unwrap_or_default()
        )))
    }

    fn target(&mut self, core: &mut Core, t: &Value) -> Result<String, BpaError> {
        Ok(self.target_id(core, t, false)?.expect("present"))
    }

    fn segment(&mut self, core: &mut Core, oid: &str) -> Result<Option<Segment>, BpaError> {
        let asof = core.asof();
        Ok(self
            .object(core, oid)?
            .and_then(|o| at(&o.timeline, &asof).cloned()))
    }

    fn class_of(&mut self, core: &mut Core, oid: &str) -> Result<String, BpaError> {
        if let Some(p) = core.pending.get(oid) {
            return Ok(p.class.clone());
        }
        Ok(self
            .object(core, oid)?
            .map(|o| o.class.clone())
            .unwrap_or_default())
    }

    fn head_body(&mut self, core: &mut Core, oid: &str) -> Result<Map<String, Value>, BpaError> {
        if let Some(p) = core.pending.get(oid) {
            return Ok(p.body.clone());
        }
        let head = self
            .object(core, oid)?
            .map(|o| o.head.clone())
            .ok_or_else(|| not_found("unknown object"))?;
        Ok(self.version(&head)?["body"]
            .as_object()
            .cloned()
            .unwrap_or_default())
    }

    fn slug_of(&mut self, core: &mut Core, oid: &str) -> Result<String, BpaError> {
        if let Some(p) = core.pending.get(oid) {
            return Ok(p.slug.clone());
        }
        Ok(self
            .object(core, oid)?
            .map(|o| o.slug.clone())
            .unwrap_or_default())
    }

    // ---- assets (STD-CTX-022, -023, -030) ----

    /// `(body, version hash)` of a logic asset by object id or name, pinned for the process.
    pub(crate) fn asset(
        &mut self,
        core: &mut Core,
        key: &str,
    ) -> Result<(Value, String), BpaError> {
        if names::matches("uuid", key) {
            self.object(core, key)?;
        }
        let objects: Vec<(String, String, String)> = self
            .state(core)?
            .objects
            .iter()
            .map(|(id, o)| (id.clone(), o.class.clone(), o.head.clone()))
            .collect();
        let mut cands = Vec::new();
        for (oid, class, head) in &objects {
            let is_logic = self
                .e
                .registry
                .class(class)
                .is_ok_and(|c| c.linearization.iter().any(|x| x == "LogicAsset"));
            let v = self.version(head)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            if oid == key || (is_logic && s(&v["body"], "name") == Some(key)) {
                cands.push((oid.clone(), class.clone(), head.clone()));
            }
        }
        let Some((oid, class, head)) = cands.into_iter().next() else {
            return Err(not_found(&format!("no logic asset {key}")));
        };
        if !self
            .e
            .registry
            .class(&class)
            .is_ok_and(|c| c.linearization.iter().any(|x| x == "LogicAsset"))
        {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                "not a logic asset",
            ));
        }
        // STD-TIME-041: under a restatement, the version of the re-evaluation mode
        let vh = match core.pinned.get(&oid) {
            Some(v) => v.clone(),
            None => {
                let v = match self.object(core, &oid)? {
                    Some(o) => crate::restate::logic_version(&o),
                    None => head,
                };
                core.pinned.insert(oid, v.clone());
                v
            }
        };
        let body = self.version(&vh)?["body"].clone();
        self.check_asset(core, &body)?;
        if !core.logic.contains(&vh) {
            core.logic.push(vh.clone());
        }
        Ok((body, vh))
    }

    fn check_asset(&mut self, core: &mut Core, body: &Value) -> Result<(), BpaError> {
        let (major, minor) = isa::abi_version(body.get("abi"))?;
        if major != 1 || minor > 0 {
            return Err(BpaError::new(
                "LOGIC.ABI_UNSUPPORTED",
                "this implementation supports ABI 1.0",
            ));
        }
        match s(body, "tier") {
            Some("host") => {}
            Some("l2") if self.e.l2 => {}
            Some("l2") => {
                return Err(BpaError::new(
                    "LOGIC.NOT_AVAILABLE",
                    "L2 execution is not enabled",
                ));
            }
            _ => {
                return Err(BpaError::new(
                    "LOGIC.NOT_AVAILABLE",
                    "L3 components come with a later release",
                ));
            }
        }
        let kind = self.e.branch_meta(&core.branch)?.kind;
        if PROTECTED.contains(&kind.as_str()) {
            let ok = matches!(s(body, "state"), Some("verified" | "deprecated"))
                && self.e.asset_verified(body)?;
            if !ok {
                return Err(BpaError::new("LOGIC.UNVERIFIED_ASSET", "unverified asset"));
            }
        } else if s(body, "state") == Some("retired") {
            return Err(BpaError::new("LOGIC.UNVERIFIED_ASSET", "retired"));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn call_impl(
        &mut self,
        core: &mut Core,
        found: &Value,
        oid: &str,
        cls: &str,
        body: Value,
        port: &str,
        params: &Value,
    ) -> Result<Value, BpaError> {
        let implementation = s(found, "impl").unwrap_or_default();
        let (name, fname) = implementation
            .split_once('#')
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .unwrap_or((implementation.to_string(), String::new()));
        let (asset, vh) = self.asset(core, &name)?;
        let frame = Frame {
            asset,
            vh: Some(vh),
            fname: Some(fname),
            oid: Some(oid.to_string()),
            this: Some(body),
            cls: Some(cls.to_string()),
            port: Some(port.to_string()),
            impl_class: s(found, "class").map(str::to_string),
            params: BTreeMap::new(),
        };
        program::run_function(core, self, frame, params)
    }

    // ---- commit.* ----

    fn commit_create(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let cls = last_segment(&a["class"]);
        self.permit(core, &d.authorization, &cls, None)?;
        let fields = a
            .get("fields")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        core.per_item(d, fields.len() as u64)?;
        let given = a.get("object_id").filter(|v| !v.is_null());
        if given.is_some() && core.process_kind != "import" {
            return Err(invalid("object_id is given by import processes only"));
        }
        let oid = given
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| core.uuid());
        let slug = s(&Value::Object(a.clone()), "slug")
            .map(str::to_string)
            .unwrap_or_else(|| oid.clone());
        let mut ch =
            json!({"op": "create", "object_id": oid, "class": cls, "slug": slug, "body": fields});
        if let Some(v) = a.get("valid").filter(|v| v.is_object()) {
            ch["valid"] = json!({"from": v.get("from").cloned().unwrap_or(Value::Null)});
        }
        let idx = core.add(ch)?;
        core.pending.insert(
            oid.clone(),
            Pending {
                class: cls,
                slug,
                body: fields,
                change: Some(idx),
                update: None,
            },
        );
        Ok(json!({"object_id": oid, "version": null}))
    }

    fn base_of(&self, core: &Core, oid: &str, given: Option<&Value>) -> Result<Value, BpaError> {
        if let Some(b) = given.filter(|v| !v.is_null()) {
            return Ok(b.clone());
        }
        core.reads.get(oid).map(|h| json!(h)).ok_or_else(|| {
            BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "the base is required when the object was not read",
            )
        })
    }

    fn commit_update(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
        slug: Option<&str>,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let sets = a
            .get("set")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        core.per_item(d, sets.len() as u64)?;
        self.stage_update(core, &oid, sets, slug, a.get("base"), a.get("timeline"))
    }

    /// Merge field sets into the pending change of an object, or stage an update.
    fn stage_update(
        &mut self,
        core: &mut Core,
        oid: &str,
        sets: Map<String, Value>,
        slug: Option<&str>,
        base: Option<&Value>,
        timeline: Option<&Value>,
    ) -> Result<Value, BpaError> {
        let oid = oid.to_string();
        if let Some(p) = core.pending.get_mut(&oid) {
            p.body
                .extend(sets.iter().map(|(k, v)| (k.clone(), v.clone())));
            let idx = p.change.or(p.update).expect("a pending change");
            if let Some(sl) = slug {
                p.slug = sl.to_string();
                core.uow[idx]["slug"] = json!(sl);
            }
            if let Some(b) = core.uow[idx]["body"].as_object_mut() {
                b.extend(sets);
            }
            return Ok(json!({"version": null}));
        }
        let mut ch = json!({"op": "update", "object_id": oid, "base": self.base_of(core, &oid, base)?, "body": sets});
        if let Some(sl) = slug {
            ch["slug"] = json!(sl);
        }
        if let Some(tl) = timeline.filter(|v| v.is_object()) {
            ch["valid"] = json!({"kind": tl.get("op"), "from": tl.get("from")});
            if let Some(to) = tl.get("to").filter(|v| !v.is_null()) {
                ch["valid"]["to"] = to.clone();
            }
        }
        let idx = core.add(ch)?;
        let (class, old_slug) = {
            let o = self
                .object(core, &oid)?
                .ok_or_else(|| not_found("unknown object"))?;
            (o.class.clone(), o.slug.clone())
        };
        let mut body = self.head_body(core, &oid)?;
        body.extend(sets);
        core.pending.insert(
            oid,
            Pending {
                class,
                slug: slug.map(str::to_string).unwrap_or(old_slug),
                body,
                change: None,
                update: Some(idx),
            },
        );
        Ok(json!({"version": null}))
    }

    fn commit_end(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let mut ch = json!({"op": "end", "object_id": oid, "base": self.base_of(core, &oid, a.get("base"))?});
        if let Some(f) = a.get("from").filter(|v| !v.is_null()) {
            ch["valid"] = json!({"from": f});
        }
        core.add(ch)?;
        Ok(json!({"version": null}))
    }

    fn commit_append(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let cls = last_segment(&a["class"]);
        self.permit(core, &d.authorization, &cls, None)?;
        let fields = a.get("fields").cloned().unwrap_or(Value::Null);
        core.per_item(d, fields.as_object().map(Map::len).unwrap_or(0) as u64)?;
        let eid = core.uuid();
        let mut ch = json!({"op": "append", "class": cls, "entry_id": eid, "ledger_key": a.get("ledger_key"), "body": fields});
        for k in ["valid_at", "posting_group", "source_refs"] {
            if let Some(v) = a.get(k).filter(|v| !v.is_null()) {
                ch[k] = v.clone();
            }
        }
        // STD-TIME-043 (1.0-rc9): a re-run's appends are its restated postings
        if crate::restate::capture(self.posting(&cls, &ch)?) {
            return Ok(json!({"entry_id": eid}));
        }
        core.add(ch)?;
        Ok(json!({"entry_id": eid}))
    }

    /// A restated posting `{class, ledger_key, valid_at, body}`, validated as an append
    /// would be (STD-TIME-043); empty outside a restatement.
    fn posting(&mut self, cls: &str, ch: &Value) -> Result<Value, BpaError> {
        let c = self.e.registry.class(cls)?.clone();
        let keys: Vec<String> = c
            .ledger
            .as_ref()
            .and_then(|l| l.get("key"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
        let mut key = Map::new();
        for k in &keys {
            let f = c.fields.get(k).cloned().unwrap_or(Value::Null);
            let v = ubos_types::values::validate_value(
                s(&f, "type").unwrap_or("text"),
                f.get("constraints"),
                &ch["ledger_key"][k],
            )?;
            key.insert(k.clone(), v);
        }
        let body = self.e.registry.check_body(cls, &ch["body"], false)?;
        let valid = ch
            .get("valid_at")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.e.default_valid_of(cls));
        Ok(json!({"class": cls, "ledger_key": key, "valid_at": valid, "body": body}))
    }

    fn entry_class(&mut self, core: &mut Core, eid: &Value) -> Result<String, BpaError> {
        let eid = eid.as_str().unwrap_or_default();
        self.state(core)?;
        if let Some(e) = self.view.as_ref().and_then(|v| v.find_entry(eid)) {
            return Ok(e.class.clone());
        }
        let (branch, cseq) = self.coord.clone().expect("coordinate");
        self.repo()
            .entries_in(&branch, cseq, None, Some(eid))?
            .into_iter()
            .next()
            .map(|e| e.class)
            .ok_or_else(|| not_found("unknown entry"))
    }

    fn commit_reverse(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let cls = self.entry_class(core, &a["entry"])?;
        self.permit(core, &d.authorization, &cls, None)?;
        let nid = core.uuid();
        let mut ch = json!({"op": "reverse", "entry_id": a["entry"], "new_entry_id": nid});
        if let Some(v) = a.get("valid_at").filter(|v| !v.is_null()) {
            ch["valid_at"] = v.clone();
        }
        core.add(ch)?;
        Ok(json!({"entry_id": nid}))
    }

    fn commit_adjust(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let corrects = a.get("corrects").and_then(Value::as_array);
        let Some(first) = corrects.filter(|c| c.len() == 1).and_then(|c| c.first()) else {
            return Err(invalid("an adjustment corrects one entry in BPA 1.0-rc1"));
        };
        let cls = self.entry_class(core, first)?;
        self.permit(core, &d.authorization, &cls, None)?;
        let nid = core.uuid();
        let mut ch = json!({"op": "adjust", "entry_id": first, "new_entry_id": nid, "body": a.get("fields")});
        if let Some(v) = a.get("valid_at").filter(|v| !v.is_null()) {
            ch["valid_at"] = v.clone();
        }
        core.add(ch)?;
        Ok(json!({"entry_id": nid}))
    }

    fn link(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
        on: bool,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["from"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let rel = s(&Value::Object(a.clone()), "relationship")
            .unwrap_or_default()
            .to_string();
        let f = self
            .e
            .registry
            .class(&cls)?
            .fields
            .get(&rel)
            .filter(|f| f.get("relationship").is_some_and(|r| !r.is_null()))
            .cloned()
            .ok_or_else(|| {
                BpaError::new(
                    "MODEL.REFERENCE_INVALID",
                    format!("{rel} is not a relationship of {cls}"),
                )
            })?;
        let tid = self.target(core, &a["to"])?;
        let tcls = self.class_of(core, &tid)?;
        let tslug = self.slug_of(core, &tid)?;
        let uri = relations::uri_of(&self.e.store.authority(), &tcls, &tslug);
        let body = self.head_body(core, &oid)?;
        let cur = body.get(&rel).cloned().unwrap_or(Value::Null);
        let new = if s(&f, "type") == Some("list") {
            let mut vals: Vec<Value> = cur.as_array().cloned().unwrap_or_default();
            if on {
                if !vals.iter().any(|x| x == &json!(uri)) {
                    vals.push(json!(uri));
                }
            } else {
                vals.retain(|x| x != &json!(uri));
            }
            Value::Array(vals)
        } else if on {
            json!(uri)
        } else {
            Value::Null
        };
        if !core.pending.contains_key(&oid) && !core.reads.contains_key(&oid) {
            let head = self
                .object(core, &oid)?
                .map(|o| o.head.clone())
                .unwrap_or_default();
            core.reads.insert(oid.clone(), head);
        }
        let mut args = Map::new();
        args.insert("target".into(), json!(oid));
        args.insert("set".into(), json!({rel: new}));
        let upd = isa::by_name("commit.update").expect("catalogue");
        self.commit_update(core, &args, upd, None)?;
        Ok(json!({"link": null}))
    }

    // ---- read.* ----

    fn read_get(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let t = a["target"].clone();
        let mut out = if t.as_str().is_some_and(|x| x.contains('?')) {
            let r = self.e.resolve(&t)?;
            let vh = s(&r, "version_hash").unwrap_or_default().to_string();
            let cls = s(&self.version(&vh)?, "class")
                .unwrap_or_default()
                .to_string();
            self.permit(core, &d.authorization, &cls, None)?;
            let branch = s(&r, "branch").unwrap_or_default().to_string();
            json!({"object_id": r["object_id"], "class": cls, "version": vh, "fields": r["body"], "valid": r["valid"],
                "resolved_from": {"branch": branch, "commit": r["evaluated_at"][&branch]}})
        } else {
            let oid = self.target(core, &t)?;
            let cls = self.class_of(core, &oid)?;
            self.permit(core, &d.authorization, &cls, Some(&oid))?;
            if let Some(p) = core.pending.get(&oid) {
                json!({"object_id": oid, "class": cls, "version": null, "fields": p.body, "valid": null,
                    "resolved_from": {"branch": core.branch, "commit": null}})
            } else {
                let seg = self
                    .segment(core, &oid)?
                    .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
                let head = self
                    .object(core, &oid)?
                    .map(|o| o.head.clone())
                    .unwrap_or_default();
                core.reads.entry(oid.clone()).or_insert(head);
                let trip = json!({"object": oid, "version": seg.version, "asof": core.asof()});
                if !core.triples.contains(&trip) {
                    core.triples.push(trip);
                }
                let v = self.version(&seg.version)?;
                let mut out = json!({"object_id": oid, "class": cls, "version": seg.version, "fields": v["body"],
                    "valid": {"from": seg.from, "to": seg.to},
                    "resolved_from": {"branch": core.branch, "commit": core.pins.get(&core.branch)}});
                if let Some(st) = v.get("state").filter(|x| !x.is_null()) {
                    out["state"] = st.clone();
                }
                out
            }
        };
        if let Some(keep) = a.get("fields").and_then(Value::as_array) {
            let fields: Map<String, Value> = out["fields"]
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| keep.iter().any(|x| x.as_str() == Some(k.as_str())))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            out["fields"] = Value::Object(fields);
        }
        let bytes = canonical(&out["fields"])?.len() as u64;
        core.per_item(d, bytes.div_ceil(1024))?;
        Ok(out)
    }

    fn read_history(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let h = self
            .e
            .history(&json!({"branch": core.branch, "object_id": oid,
            "from": a.get("from_time"), "to": a.get("to_time")}))?;
        let commits: Vec<Value> = h["commits"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| json!({"cseq": c["cseq"], "tx_time": c["tx_time"], "ops": c["ops"]}))
            .collect();
        core.per_item(d, commits.len() as u64)?;
        Ok(json!({"commits": commits}))
    }

    fn read_timeline(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let first = self
            .object(core, &oid)?
            .and_then(|o| o.timeline.iter().map(|x| x.from.clone()).min());
        let lo = a
            .get("valid_from")
            .filter(|v| !v.is_null())
            .cloned()
            .or(first.map(Value::String));
        let segs: Vec<Value> = match lo {
            Some(lo) => self
                .e
                .read_range(&json!({"branch": core.branch, "object_id": oid, "from": lo,
                "to": a.get("valid_to"), "evaluated_at": core.pins}))?["segments"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            None => Vec::new(),
        };
        core.per_item(d, segs.len() as u64)?;
        Ok(
            json!({"segments": segs.iter().map(|x| json!({"from": x["valid"]["from"], "to": x["valid"]["to"], "fields": x["body"]})).collect::<Vec<_>>()}),
        )
    }

    fn read_diff(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["scope"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let coord = |v: &Value| -> Value {
            let mut c = v.clone();
            if c.get("branch").is_none() {
                c["branch"] = json!(core.branch);
            }
            if let Some(t) = c
                .get("time")
                .and_then(Value::as_str)
                .and_then(|t| t.parse::<u64>().ok())
            {
                c["time"] = json!(t);
            }
            c
        };
        let (ca, cb) = (coord(&a["a"]), coord(&a["b"]));
        let entries: Vec<Value> = self
            .repo()
            .diff(&ca, &cb)?
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["object_id"] == json!(oid))
            .map(|e| {
                let mut e = e.clone();
                e.as_object_mut().map(|m| m.remove("versions"));
                e
            })
            .collect();
        core.per_item(d, entries.len() as u64)?;
        Ok(json!({"entries": entries}))
    }

    fn read_effective(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let cls = last_segment(&a["target"]);
        self.permit(core, &d.authorization, &cls, None)?;
        match s(&Value::Object(a.clone()), "what") {
            Some("class") => {
                let mut i = json!({"branch": core.branch, "class": cls});
                if !core.pins.is_empty() {
                    i["evaluated_at"] = json!(core.pins);
                }
                self.e.other("class_at", &i)
            }
            Some("descriptor") => {
                let branch = core.branch.clone();
                self.e.bpu_descriptor(&branch, &cls)
            }
            Some("declarations") => {
                // since 1.0-rc2 (DEC-067): the effective sheet declarations of an object
                let oid = self.target(core, &a["target"])?;
                let mut i = json!({"branch": core.branch, "object_id": oid, "ctx": self.decision_ctx(core)?});
                if !core.pins.is_empty() {
                    i["evaluated_at"] = json!(core.pins);
                }
                let r = self.e.cascade(&i)?;
                Ok(json!({"properties": r["properties"], "bindings": r["bindings"]}))
            }
            _ => Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "what is class, declarations or descriptor",
            )),
        }
    }

    /// The context the decisions and the cascade see (STD-LGS-020, -031; DEC-067).
    fn decision_ctx(&mut self, core: &Core) -> Result<Value, BpaError> {
        let c = &core.context;
        let mut out = json!({
            "principal_kind": c["principal_kind"], "channel": c["channel"], "vae": c["vae"],
            "branch_kind": self.e.branch_meta(&core.branch)?.kind,
        });
        if let Some(r) = c.get("role").filter(|v| !v.is_null()) {
            out["role"] = r.clone();
        }
        Ok(out)
    }

    /// STD-ISA-054: the validation preview of proposed changes or of the unit of work on a
    /// target (FR-RULE-035).
    fn run_validate(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        _d: &Instruction,
    ) -> Result<Value, BpaError> {
        let changes: Vec<Value> = if let Some(c) = a.get("changes").filter(|v| !v.is_null()) {
            c.as_array().cloned().unwrap_or_default()
        } else {
            let Some(t) = a.get("target").filter(|v| !v.is_null()) else {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "run.validate takes changes or a target",
                ));
            };
            let oid = self.target(core, t)?;
            core.uow
                .iter()
                .filter(|c| s(c, "object_id") == Some(&oid))
                .map(|c| {
                    let mut c = c.clone();
                    c.as_object_mut().map(|m| m.remove("derivation"));
                    c
                })
                .collect()
        };
        for c in &changes {
            if let Some(oid) = s(c, "object_id") {
                let cls = self.class_of(core, oid)?;
                self.permit(core, "read", &cls, Some(oid))?;
            }
        }
        let process = json!({
            "channel": core.context["channel"], "principal_kind": core.context["principal_kind"],
        });
        self.e
            .validate(&json!({"branch": core.branch, "changes": changes, "process": process}))
    }

    /// STD-ISA-037: a projection at the process snapshot (BPA 1.0-rc9, PH-2 step 3c).
    fn read_projection(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        self.state(core)?;
        let (branch, cseq) = self.coord.clone().expect("coordinate");
        let name = a
            .get("projection")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let decl = self.e.projection_body(&branch, cseq, name)?;
        let source = decl["source"].as_str().unwrap_or_default().to_string();
        self.permit(core, &d.authorization, &source, None)?;
        let mut input = Value::Object(a.clone());
        input["time"] = json!(cseq);
        let r = self.e.projection_at(&branch, &input)?;
        core.per_item(d, r["groups"].as_array().map_or(0, Vec::len) as u64)?;
        Ok(r)
    }

    /// STD-ISA-035: explanations of a cascade, a validation outcome or a decision.
    /// STD-ISA-080: a criteria query at the process snapshot (since 1.0-rc2; step 13a).
    fn search_query(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let q = match a.get("query") {
            Some(Value::String(t)) => json!({"text": t}),
            Some(v @ Value::Object(_)) => v.clone(),
            _ => {
                return Err(BpaError::new(
                    "QRY.INVALID_QUERY",
                    "query is a DAT-Query or its text",
                ));
            }
        };
        if a.get("coordinates").is_some_and(|c| !c.is_null()) {
            return Err(BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                "explicit coordinates come with PH-2",
            ));
        }
        self.state(core)?;
        // `limit` and `cursor` are members of the query object (DAT-Query): the ABI 1.0
        // catalogue stays as released
        let mut input = q;
        input["branch"] = json!(core.branch);
        input["commit"] = json!(core.pins.get(&core.branch));
        input["profile"] = json!(core.profile);
        // row filters (DSN-DVM-820): the principal of the process
        for k in [
            "principal",
            "principal_kind",
            "role",
            "auth_level",
            "channel",
        ] {
            if let Some(v) = core.context.get(k).filter(|v| v.is_string()) {
                input[k] = v.clone();
            }
        }
        let from = input
            .get("from")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                input
                    .get("text")
                    .and_then(|t| ubos_query::parse_query(t).ok())
                    .and_then(|p| p.get("from").and_then(Value::as_str).map(str::to_string))
            })
            .unwrap_or_default();
        self.permit(core, &d.authorization, &from, None)?;
        let r = self.e.query(&input)?;
        let items = r["objects"].clone();
        core.per_item(d, items.as_array().map_or(0, Vec::len) as u64)?;
        let mut out = json!({"items": items, "evaluated_at": r["evaluated_at"]});
        if let Some(c) = r.get("cursor").filter(|c| !c.is_null()) {
            out["cursor"] = c.clone();
        }
        Ok(out)
    }

    fn read_explain(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        match s(&Value::Object(a.clone()), "kind") {
            Some("cascade") => {
                let Some(property) = a.get("property").and_then(Value::as_str) else {
                    return Err(BpaError::new(
                        "MODEL.DECLARATION_INCOMPLETE",
                        "a cascade explanation names the property",
                    ));
                };
                let mut i = json!({"branch": core.branch, "object_id": oid, "property": property,
                    "ctx": self.decision_ctx(core)?});
                if !core.pins.is_empty() {
                    i["evaluated_at"] = json!(core.pins);
                }
                let mut r = self.e.explain_cascade(&i)?;
                r.as_object_mut().map(|m| m.remove("evaluated_at"));
                Ok(r)
            }
            Some("validation") => {
                let mut b = Map::new();
                b.insert("target".into(), a["target"].clone());
                self.run_validate(core, &b, d)
            }
            Some("decision") => {
                let Some(port) = a.get("property").and_then(Value::as_str) else {
                    return Err(BpaError::new(
                        "MODEL.DECLARATION_INCOMPLETE",
                        "a decision explanation names the port",
                    ));
                };
                self.object(core, &oid)?;
                let st = self.state(core)?.clone();
                let lazy = self.lazy();
                let dctx = self.decision_ctx(core)?;
                let branch = core.branch.clone();
                self.e.decide_in(
                    &st,
                    &branch,
                    &oid,
                    port,
                    &json!({}),
                    dctx.as_object().expect("context"),
                    lazy,
                )
            }
            _ => Err(BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                "lineage explanations come with PH-2",
            )),
        }
    }

    // ---- run.* and emit.* ----

    fn run_port(&mut self, core: &mut Core, a: &Map<String, Value>) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["target"])?;
        let cls = self.class_of(core, &oid)?;
        let port = s(&Value::Object(a.clone()), "port")
            .unwrap_or_default()
            .to_string();
        let reg = self.registry(core)?;
        let Some(p) = reg.class(&cls)?.ports.get(&port) else {
            return Err(BpaError::new(
                "MODEL.PORT_NOT_IMPLEMENTED",
                format!("no port {port}"),
            ));
        };
        let decl = p.decl.clone();
        // STD-ISA-050 (1.0-rc3): the port is available in the object's lifecycle state
        let state = self.object_state(core, &oid)?;
        let lc = self.lifecycle_of(core, &cls)?;
        if !ubos_flow::available(&decl, lc.as_ref().map(|l| &l.2), state.as_deref()) {
            return Err(BpaError::with_details(
                "FLOW.PORT_UNAVAILABLE",
                json!({"port": port, "state": state}),
            ));
        }
        let permission = s(&decl, "permission").unwrap_or("none").to_string();
        self.permit(core, &permission, &cls, Some(&oid))?;
        if !core.pending.contains_key(&oid) {
            // STD-LGS-031: the decisions of a command port run before it (DSN-DVM-720) and
            // their result is journaled (DSN-DVM-721)
            let params = a.get("params").cloned().unwrap_or_else(|| json!({}));
            self.object(core, &oid)?;
            let st = self.state(core)?.clone();
            let lazy = self.lazy();
            let dctx = self.decision_ctx(core)?;
            let branch = core.branch.clone();
            let args = json!({"target": oid, "port": port, "params": params});
            let r = ubos_runtime::program::journaled(core, "decision", &args, |_| {
                self.e.decide_in(
                    &st,
                    &branch,
                    &oid,
                    &port,
                    &params,
                    dctx.as_object().expect("context"),
                    lazy.clone(),
                )
            })?;
            match s(&r, "outcome") {
                Some("deny") => return Err(BpaError::with_details("RULE.DECISION_DENIED", r)),
                // STD-TXN-060 (1.0-rc8): a process called with `defer` goes on, and its
                // changes become a pending change
                Some("require") if !crate::approvals::defer(&oid, &port, &r) => {
                    return Err(BpaError::with_details("RULE.DECISION_REQUIRES", r));
                }
                _ => {}
            }
        }
        let found = reg.dispatch(&cls, &port, None)?;
        let body = Value::Object(self.head_body(core, &oid)?);
        let params = a.get("params").cloned().unwrap_or_else(|| json!({}));
        // STD-ISA-050 (1.0-rc3): the port's guards, L1 booleans over this, params and ctx
        let dctx = self.decision_ctx(core)?;
        let dctx = dctx.as_object().cloned().unwrap_or_default();
        let failed = ubos_flow::failed_port_guards(&reg, &cls, &decl, &body, &params, &dctx)?;
        if !failed.is_empty() {
            let failed: Vec<Value> = failed.iter().map(|(_, g)| json!({"guard": g})).collect();
            return Err(BpaError::with_details(
                "FLOW.GUARD_FAILED",
                json!({"port": port, "failed": failed}),
            ));
        }
        let n = decl
            .get("guards")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        for i in 0..n {
            self.flow.passed(&format!("{port}/{i}"));
        }
        self.call_impl(core, &found, &oid, &cls, body, &port, &params)
    }

    fn run_super(&mut self, core: &mut Core) -> Result<Value, BpaError> {
        let f = core.frame().clone();
        let Some(port) = f.port.clone() else {
            return Err(BpaError::new(
                "MODEL.PORT_NOT_IMPLEMENTED",
                "run.super outside a port",
            ));
        };
        let cls = f.cls.clone().unwrap_or_default();
        let reg = self.registry(core)?;
        let found = reg.dispatch(&cls, &port, f.impl_class.as_deref())?;
        let mut params = Map::new();
        for (k, v) in &f.params {
            params.insert(k.clone(), ubos_expr::to_json(v)?);
        }
        self.call_impl(
            core,
            &found,
            f.oid.as_deref().unwrap_or_default(),
            &cls,
            f.this.clone().unwrap_or(Value::Null),
            &port,
            &Value::Object(params),
        )
    }

    fn run_logic(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let f = core.frame().clone();
        let name = s(&Value::Object(a.clone()), "asset")
            .unwrap_or_default()
            .to_string();
        let pin = f.asset["imports"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|x| s(x, "name") == Some(name.as_str()))
            .map(|x| x.get("version").cloned().unwrap_or(Value::Null));
        let Some(vh) = pin else {
            return Err(BpaError::new(
                "LOGIC.FORBIDDEN_CAPABILITY",
                "the asset is not among the imports",
            ));
        };
        let vh = vh.as_str().unwrap_or_default().to_string();
        let v = self.version(&vh).ok();
        let Some(v) = v.filter(|v| s(&v["body"], "name") == Some(name.as_str())) else {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                format!("the pinned version of {name} is unknown"),
            ));
        };
        let asset_class = s(&v, "class").unwrap_or_default().to_string();
        self.permit(core, &d.authorization, &asset_class, None)?;
        self.check_asset(core, &v["body"])?;
        if !core.logic.contains(&vh) {
            core.logic.push(vh.clone());
        }
        let frame = Frame {
            asset: v["body"].clone(),
            vh: Some(vh),
            fname: s(&Value::Object(a.clone()), "function").map(str::to_string),
            ..Frame::default()
        };
        let args = a.get("args").cloned().unwrap_or_else(|| json!([]));
        program::run_function(core, self, frame, &args)
    }

    fn run_async(&mut self, core: &mut Core, a: &Map<String, Value>) -> Result<Value, BpaError> {
        let job = core.uuid();
        let oid = self.target(core, a.get("target").unwrap_or(&Value::Null))?;
        core.effects.push(json!({"type": "Job", "object_id": oid,
            "payload": {"job_id": job, "port": a.get("port"), "params": a.get("params").cloned().unwrap_or_else(|| json!({})),
                "not_before": a.get("not_before"), "parent_process": core.process_id}}));
        Ok(json!({"job_id": job}))
    }

    fn emit_event(
        &mut self,
        core: &mut Core,
        a: &Map<String, Value>,
        d: &Instruction,
    ) -> Result<Value, BpaError> {
        let oid = self.target(core, &a["subject"])?;
        let cls = self.class_of(core, &oid)?;
        self.permit(core, &d.authorization, &cls, Some(&oid))?;
        let lin = self.e.registry.class(&cls)?.linearization.clone();
        let mut declared = Vec::new();
        for c in &lin {
            for e in self.e.registry.decl(c)?["events"]
                .as_array()
                .into_iter()
                .flatten()
            {
                declared.push(s(e, "name").unwrap_or_default().to_string());
            }
        }
        let t = s(&Value::Object(a.clone()), "type")
            .unwrap_or_default()
            .to_string();
        if !declared.contains(&t) {
            return Err(BpaError::new("EVT.UNKNOWN_EVENT_TYPE", t));
        }
        core.effects.push(json!({"type": t, "object_id": oid, "payload": a.get("payload").cloned().unwrap_or_else(|| json!({}))}));
        let before = self
            .e
            .store
            .process_rec(&core.process_id)?
            .and_then(|r| r["effects"].as_u64())
            .unwrap_or_default();
        Ok(
            json!({"event_id": format!("{}/{}", core.process_id, before + core.effects.len() as u64)}),
        )
    }
}

impl Host for EngineHost<'_> {
    fn instruction(
        &mut self,
        core: &mut Core,
        d: &Instruction,
        a: &Map<String, Value>,
    ) -> Result<Value, BpaError> {
        match d.name.as_str() {
            "commit.create" => self.commit_create(core, a, d),
            "commit.update" => self.commit_update(core, a, d, None),
            "commit.end" => self.commit_end(core, a, d),
            "commit.move" => {
                let mut b = Map::new();
                b.insert("target".into(), a["target"].clone());
                b.insert("set".into(), json!({}));
                if let Some(x) = a.get("base") {
                    b.insert("base".into(), x.clone());
                }
                let slug = s(&Value::Object(a.clone()), "slug").map(str::to_string);
                self.commit_update(core, &b, d, slug.as_deref())
            }
            "commit.append" => self.commit_append(core, a, d),
            "commit.reverse" => self.commit_reverse(core, a, d),
            "commit.adjust" => self.commit_adjust(core, a, d),
            "commit.link" => self.link(core, a, d, true),
            "commit.unlink" => self.link(core, a, d, false),
            "commit.transition" => self.commit_transition(core, a, d),
            "read.get" => self.read_get(core, a, d),
            "read.actions" => self.read_actions(core, a, d),
            "read.history" => self.read_history(core, a, d),
            "read.timeline" => self.read_timeline(core, a, d),
            "read.diff" => self.read_diff(core, a, d),
            "read.effective" => self.read_effective(core, a, d),
            "read.explain" => self.read_explain(core, a, d),
            "search.query" => self.search_query(core, a, d),
            "read.projection" => self.read_projection(core, a, d),
            "run.validate" => self.run_validate(core, a, d),
            "run.port" => self.run_port(core, a),
            "run.super" => self.run_super(core),
            "run.logic" => self.run_logic(core, a, d),
            "run.async" => self.run_async(core, a),
            "emit.event" => self.emit_event(core, a, d),
            "call.connector" | "ask.model" => self.host_call(core, d, a),
            other => Err(BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                format!("{other} is not available in BPA 1.0-rc1"),
            )),
        }
    }

    fn implements(&self, name: &str) -> bool {
        self.e.services.is_some() && matches!(name, "call.connector" | "ask.model")
    }

    fn typed_ref(&mut self, core: &mut Core, uri: &str) -> Result<Option<Val>, BpaError> {
        let Some(oid) = self.target_id(core, &json!(uri), true)? else {
            return Ok(None);
        };
        let (body, cls) = if let Some(p) = core.pending.get(&oid) {
            (Value::Object(p.body.clone()), p.class.clone())
        } else {
            let Some(seg) = self.segment(core, &oid)? else {
                return Ok(None);
            };
            let cls = self.class_of(core, &oid)?;
            (self.version(&seg.version)?["body"].clone(), cls)
        };
        Ok(Some(from_json(&self.class_type(&cls)?, &body)))
    }

    fn class_type(&mut self, class: &str) -> Result<Ty, BpaError> {
        ubos_expr::class_type(&self.e.registry, class)
    }

    fn fx_rate(
        &mut self,
        core: &mut Core,
        base: &str,
        quote: &str,
        rate_type: &str,
        source: Option<&str>,
    ) -> Result<Option<ubos_expr::eval::FxRate>, BpaError> {
        let objects = self.snapshot_class(core, "FxRate")?;
        let rows = crate::finance::rates_at(&mut self.repo(), &objects, &core.asof())?;
        let u = crate::finance::pick_rate(&rows, base, quote, rate_type, source)?;
        use_rate(core, &u.object_id, &u.version_hash);
        Ok(Some(u.rate))
    }

    fn rate_object(&mut self, core: &mut Core, rate_ref: &str) -> Result<Option<Value>, BpaError> {
        let oid = if rate_ref.starts_with("ubos://") {
            match self.target_id(core, &json!(rate_ref), true)? {
                Some(o) => o,
                None => return Ok(None),
            }
        } else {
            rate_ref.to_string()
        };
        let Some(o) = self.object(core, &oid)? else {
            return Ok(None);
        };
        if o.class != "FxRate" {
            return Ok(None);
        }
        let rows = crate::finance::rates_at(&mut self.repo(), &[(oid.clone(), o)], &core.asof())?;
        let Some((_, vh, b)) = rows.into_iter().next() else {
            return Ok(None);
        };
        use_rate(core, &oid, &vh);
        Ok(Some(
            json!({"from": b["base"], "to": b["quote"], "rate": b["rate"]}),
        ))
    }
}

/// Record the rate version an evaluation used, once, in order (STD-FND-029).
fn use_rate(core: &mut Core, oid: &str, vh: &str) {
    let rec = json!({"object_id": oid, "version_hash": vh});
    if !core.rates.contains(&rec) {
        core.rates.push(rec);
    }
}

impl EngineHost<'_> {
    /// `call.connector` and `ask.model` through the host services (CTR-002), journaled
    /// before use (STD-CTX-011): a replay serves the recorded result and never calls the
    /// host (SCN-011 A1).
    fn host_call(
        &mut self,
        core: &mut Core,
        d: &Instruction,
        a: &Map<String, Value>,
    ) -> Result<Value, BpaError> {
        let Some(sv) = self.e.services.clone() else {
            return Err(BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                format!("{} needs a host", d.name),
            ));
        };
        let args = Value::Object(a.clone());
        program::journaled(core, &d.name, &args, |_| {
            if d.name == "ask.model" {
                return sv.models.ask(&args);
            }
            let text = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
            sv.connectors.call(
                text("connector"),
                text("operation"),
                a.get("request").unwrap_or(&Value::Null),
            )
        })
    }
}

/// Content ID of the code of an asset (STD-CTX-023).
pub fn code_identity(body: &Value) -> Result<String, BpaError> {
    let mut m = Map::new();
    for k in [
        "tier",
        "abi",
        "source",
        "exports",
        "imports",
        "capabilities",
    ] {
        m.insert(k.into(), body.get(k).cloned().unwrap_or(Value::Null));
    }
    content_id("ubos.chunk.v1", &Value::Object(m))
}

fn seed_of(i: &Value, pid: &str) -> Result<[u8; 32], BpaError> {
    if let Some(s) = i.get("entropy_seed").filter(|v| !v.is_null()) {
        return unhex32(s, "entropy_seed");
    }
    let mut h = Sha256::new();
    h.update(b"ubos.entropy.v1\0");
    h.update(pid.as_bytes());
    Ok(h.finalize().into())
}

pub(crate) fn changes_hash(uow: &[Value]) -> Result<String, BpaError> {
    let list: Vec<Value> = uow
        .iter()
        .map(|c| {
            let mut c = c.clone();
            c.as_object_mut().map(|m| m.remove("derivation"));
            c
        })
        .collect();
    content_id("ubos.chunk.v1", &Value::Array(list))
}

impl Engine {
    /// `execute`: run a logic function or a port as one process (STD-TXN-001) and commit
    /// its unit of work; retried on head movement or read-set change when the function is
    /// retryable (STD-TXN-023).
    pub fn execute(&mut self, i: &Value) -> Result<Value, BpaError> {
        self.run(i, None)
    }

    /// One invocation, or its replay from journal records (STD-CTX-012): a replay stops
    /// after the run and compares the outputs; nothing is committed or recorded.
    pub(crate) fn run(&mut self, i: &Value, replay: Option<Replay>) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let now = self.now.clone();
        Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
        let mut next = self.next_process;
        let p = process(
            self.store.as_mut(),
            i.get("process"),
            "action",
            &mut next,
            &self.principal,
        )?;
        self.next_process = next;
        // STD-TXN-040 for executions (PH-1 step 15c, DEC-076): a process naming an
        // idempotency key commits once; a repeated request returns the stored result. The
        // request hash is over what was asked, not over the changes it produced.
        let idem = match (
            replay.is_none(),
            p.p.get("idempotency_key").filter(|k| !k.is_null()),
        ) {
            (true, Some(k)) => {
                let mut req = Map::new();
                for key in [
                    "branch", "target", "port", "params", "asset", "function", "args",
                ] {
                    if let Some(v) = i.get(key) {
                        req.insert(key.into(), v.clone());
                    }
                }
                let h = content_id("ubos.chunk.v1", &Value::Object(req))?;
                let scope = s(&p.p, "initiator").unwrap_or_default().to_string();
                Some((scope, canonical(k)?, h))
            }
            _ => None,
        };
        if let Some((scope, key, hash)) = &idem
            && let Some(result) = self.replay(scope, key, hash)?
        {
            return Ok(result);
        }
        let seed0 = match (
            &self.services,
            i.get("entropy_seed").filter(|v| !v.is_null()),
        ) {
            (Some(sv), None) => {
                // a host seeds the process from its entropy (CTR-002); the seed is recorded
                // with the process, so a replay needs no host
                let mut seed = [0u8; 32];
                sv.entropy.fill(&mut seed)?;
                seed
            }
            _ => seed_of(i, &p.id)?,
        };
        let interfere: BTreeMap<u64, Value> = i
            .get("interfere")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|x| {
                x.get("attempt")
                    .and_then(Value::as_u64)
                    .map(|a| (a, x["changes"].clone()))
            })
            .collect();
        let max_attempts = i
            .get("max_attempts")
            .and_then(Value::as_u64)
            .unwrap_or(MAX_ATTEMPTS);
        let mut attempt = 0u64;
        // the process state machine of DSN-DVM-501 (UBS-SYS-DVM-05 §1)
        let mut machine = crate::process::Machine::new();
        let invariant = |e: String| BpaError::new("LOGIC.NOT_AVAILABLE", e);
        loop {
            attempt += 1;
            machine.attempts = attempt;
            if machine.phase == crate::process::Phase::Created {
                machine
                    .fire(crate::process::Event::FirstInstruction)
                    .map_err(invariant)?;
            }
            let seed = if attempt == 1 {
                seed0
            } else {
                let mut h = Sha256::new();
                h.update(seed0);
                h.update((attempt as u32).to_be_bytes());
                h.finalize().into()
            };
            let mut core = Core::new(i, seed, &p.id, s(&p.p, "kind").unwrap_or("action"))?;
            core.replay = replay.clone();
            if let Some(r) = &replay {
                core.pins = r.pins.clone();
            }
            let today = self.now[..10].to_string();
            core.build_context(
                i,
                s(&p.p, "initiator").unwrap_or_default(),
                &self.store.authority(),
                &today,
                &self.now,
                &p.p["trigger"],
            )?;
            let mut retryable = false;
            let mut pending: Option<Value> = None;
            let mut duty: Option<String> = None;
            crate::approvals::begin(
                replay.is_none() && i.get("defer").and_then(Value::as_bool) == Some(true),
            );
            let r = (|| -> Result<(Value, Option<u64>), BpaError> {
                let mut host = EngineHost::new(self);
                let started = start(&mut core, &mut host, i);
                let deferred = crate::approvals::take();
                let (result, rt) = started?;
                if i.get("port").is_some() {
                    duty = Some(host.target(&mut core, i.get("target").unwrap_or(&Value::Null))?);
                }
                let flow = std::mem::take(&mut host.flow);
                retryable = rt;
                if replay.is_some() {
                    return Ok((result, None));
                }
                if !deferred.is_empty() {
                    // STD-TXN-060: the changes wait for approval; the process ends committed
                    // without effect on the target
                    machine
                        .fire(crate::process::Event::CommitEmpty)
                        .map_err(invariant)?;
                    let initiator = s(&p.p, "initiator").unwrap_or_default().to_string();
                    pending =
                        Some(self.defer_changes(&branch, &p.id, &initiator, &core.uow, &deferred)?);
                    return Ok((result, None));
                }
                if let Some(changes) = interfere.get(&attempt) {
                    self.commit(&json!({"branch": branch, "changes": changes}))?;
                }
                let mut cseq = None;
                if core.uow.is_empty() {
                    machine
                        .fire(crate::process::Event::CommitEmpty)
                        .map_err(invariant)?;
                } else {
                    machine
                        .fire(crate::process::Event::CommitStaged)
                        .map_err(invariant)?;
                    let mut pc = p.p.clone();
                    pc["serializable"] = i.get("serializable").cloned().unwrap_or(json!(false));
                    pc["reads"] = json!(core.reads);
                    pc["effects"] = json!(core.effects);
                    let pr = Process {
                        id: p.id.clone(),
                        explicit: p.explicit,
                        p: pc,
                    };
                    let meta = self.branch_meta(&branch)?;
                    let input = json!({"branch": branch, "changes": core.uow, "process": pr.p});
                    let refs = self.refs_indexed(&branch)?;
                    let committed = commit_on(
                        self.store.as_mut(),
                        &self.registry,
                        &self.now,
                        &input,
                        &pr,
                        false,
                        &meta,
                        self.max_inputs,
                        idem.clone(),
                        false,
                        self.max_violations,
                        &mut self.rules,
                        Some(&flow),
                        refs,
                    );

                    // staged → flushing → committed; a conflict re-runs or aborts, a storage
                    // failure aborts, any other error is the pre-lock validation failing
                    use crate::process::Event;
                    let events: Vec<Event> = match &committed {
                        Ok(_) => vec![Event::Enqueued, Event::Flushed],
                        Err(e) if RETRY_CODES.contains(&e.code) => vec![
                            Event::Enqueued,
                            Event::Conflict {
                                retry: retryable && attempt < max_attempts,
                            },
                        ],
                        Err(e) if e.code.starts_with("NOD.") => {
                            vec![Event::Enqueued, Event::StorageFailure]
                        }
                        Err(_) => vec![Event::ValidationFailed],
                    };
                    for ev in events {
                        machine.fire(ev).map_err(invariant)?;
                    }
                    let (r, classes, effects) = committed?;
                    self.touch_branch(&branch)?;
                    let n = r["cseq"].as_u64().unwrap_or_default();
                    record(
                        self.store.as_mut(),
                        &pr,
                        "committed",
                        Some((&branch, n)),
                        effects,
                    )?;
                    self.instances.extend(classes);
                    cseq = Some(n);
                }
                Ok((result, cseq))
            })();
            match r {
                Ok((result, cseq)) => {
                    if replay.is_some() {
                        let j = self.journal_full(&p.id)?;
                        return Engine::compare_replay(&core, &result, &j);
                    }
                    // STD-LGS-042: a committed port call is a duty of its initiator
                    if let (Some(oid), Some(port)) = (&duty, s(i, "port")) {
                        let initiator = s(&p.p, "initiator").unwrap_or_default().to_string();
                        self.record_duty(oid, port, &initiator)?;
                    }
                    let mut out =
                        self.finish(&core, i, &p, Some(result), cseq, "ok", attempt, &machine)?;
                    if let Some(v) = pending {
                        out["pending"] = v;
                    }
                    return Ok(out);
                }
                Err(e) => {
                    if replay.is_some() {
                        return Err(e);
                    }
                    // another execution of the same request committed meanwhile (its
                    // idempotency record was written first): its result stands, and this
                    // execution is not committed (DEC-076)
                    if RETRY_CODES.contains(&e.code)
                        && let Some((scope, key, hash)) = &idem
                        && let Some(result) = self.replay(scope, key, hash)?
                    {
                        return Ok(result);
                    }
                    if RETRY_CODES.contains(&e.code) && retryable && attempt < max_attempts {
                        continue;
                    }
                    if !machine.phase.is_terminal() {
                        // a limit, a cancellation or a fault while running (or staged)
                        machine
                            .fire(crate::process::Event::Fault)
                            .map_err(invariant)?;
                    }
                    let outcome = match e.code {
                        "LOGIC.LIMIT_EXCEEDED" => "limit",
                        "LOGIC.CANCELLED" => "cancelled",
                        _ => "error",
                    };
                    self.finish(&core, i, &p, None, None, outcome, attempt, &machine)?;
                    return Err(e);
                }
            }
        }
    }

    /// Record the process, its effects, the metering record and the journal.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        core: &Core,
        i: &Value,
        p: &Process,
        result: Option<Value>,
        cseq: Option<u64>,
        outcome: &str,
        attempt: u64,
        machine: &crate::process::Machine,
    ) -> Result<Value, BpaError> {
        let pid = p.id.clone();
        if outcome == "ok" {
            if cseq.is_none() {
                let mut pc = p.p.clone();
                pc["effects"] = json!(core.effects);
                let pr = Process {
                    id: pid.clone(),
                    explicit: p.explicit,
                    p: pc,
                };
                record(self.store.as_mut(), &pr, "committed", None, 0)?;
                if !core.effects.is_empty() {
                    let head = {
                        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                        let h = repo.head(&core.branch)?;
                        repo.meta(h)?.cseq
                    };
                    let rows = effects_of(self.store.as_mut(), &pr, &core.branch, head)?;
                    let n = rows.len() as u32;
                    self.store.outbox_push(&rows)?;
                    record(self.store.as_mut(), &pr, "committed", None, n)?;
                }
            }
        } else {
            record(self.store.as_mut(), p, "failed", None, 0)?;
        }
        crate::versioning::record_phases(self.store.as_mut(), p, machine)?;
        let m = core.metering(outcome);
        let commit_ids: Vec<String> = match cseq {
            Some(_) => {
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                let h = repo.head(&core.branch)?;
                vec![repo.meta(h)?.commit_id]
            }
            None => Vec::new(),
        };
        let journal = json!({
            "context": core.context,
            "snapshots": core.pins,
            "logic": core.logic,
            "entropy_seed": ubos_types::addr::b64u(&core.seed),
            "tzdb": program::TZDB,
            "records": core.records,
            "outputs": {
                "commit_ids": commit_ids,
                "result_hash": content_id("ubos.chunk.v1", result.as_ref().unwrap_or(&Value::Null))?,
                "changes_hash": changes_hash(&core.uow)?,
                "effects": core.effects.iter().map(|e| e["type"].clone()).collect::<Vec<_>>(),
            },
        });
        let invocation: Map<String, Value> = i
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| !matches!(k.as_str(), "interfere" | "process" | "world"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut journal = journal;
        if !core.rates.is_empty() {
            journal["rates"] = json!(core.rates); // STD-FND-029 (BPA 1.0-rc9)
        }
        journal["invocation"] = Value::Object(invocation);
        let journal_id = self.store.put_record(&journal)?;
        let metering_id = self.store.put_record(&m)?;
        let refs = json!({"journal_id": journal_id, "metering": metering_id});
        match self.store.process_rec(&pid)? {
            Some(mut rec) => {
                rec["journal_id"] = refs["journal_id"].clone();
                rec["metering"] = refs["metering"].clone();
                self.store.put_process_rec(&pid, &rec)?;
            }
            None => {
                self.store.put_meta(&format!("run:{pid}"), &refs)?;
                let mut runs = self
                    .store
                    .meta("runs")?
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                if !runs.iter().any(|x| x.as_str() == Some(pid.as_str())) {
                    runs.push(json!(pid));
                    self.store.put_meta("runs", &Value::Array(runs))?;
                }
            }
        }
        if outcome != "ok" {
            return Ok(Value::Null);
        }
        let mut out =
            json!({"result": result.unwrap_or(Value::Null), "attempts": attempt, "metering": m});
        if let Some(n) = cseq {
            out["cseq"] = json!(n);
        }
        if p.explicit || self.services.is_some() {
            // a host always learns the process identifier (CTR-001 `invoke_port`)
            out["process_id"] = json!(pid);
        }
        Ok(out)
    }

    /// The journal and metering references of a process, from its record or, for a
    /// process that was not named, from the run index.
    fn run_refs(&mut self, process_id: &str) -> Result<Option<Value>, BpaError> {
        if let Some(rec) = self.store.process_rec(process_id)?
            && rec.get("journal_id").is_some_and(|v| v.is_string())
        {
            return Ok(Some(rec));
        }
        self.store.meta(&format!("run:{process_id}"))
    }

    /// The stored journal of a process, with the observed results and the invocation.
    pub(crate) fn journal_full(&mut self, process_id: &str) -> Result<Value, BpaError> {
        let id = self
            .run_refs(process_id)?
            .and_then(|r| s(&r, "journal_id").map(str::to_string))
            .ok_or_else(|| not_found("no journal for the process"))?;
        self.store
            .record(&id)?
            .ok_or_else(|| not_found("no journal for the process"))
    }

    /// `journal` (STD-CTX-011): the journal of a process, without the observed results.
    pub fn journal(&mut self, process_id: &str) -> Result<Value, BpaError> {
        let mut j = self.journal_full(process_id)?;
        if let Some(recs) = j["records"].as_array_mut() {
            for r in recs {
                r.as_object_mut().map(|m| m.remove("result"));
            }
        }
        let logic = j["logic"].take();
        if let Some(m) = j.as_object_mut() {
            m.remove("logic");
            m.remove("invocation");
        }
        j["logic_versions"] = logic;
        Ok(j)
    }

    /// `metering` (STD-CTX-005): the metering record of a process.
    pub fn metering_record(&mut self, process_id: &str) -> Result<Value, BpaError> {
        let id = self
            .run_refs(process_id)?
            .and_then(|r| s(&r, "metering").map(str::to_string))
            .ok_or_else(|| not_found("no metering record for the process"))?;
        self.store
            .record(&id)?
            .ok_or_else(|| not_found("no metering record for the process"))
    }

    /// `metering_total` (STD-CTX-006): units and processes per tenant.
    pub fn metering_total(&mut self) -> Result<Value, BpaError> {
        let mut ids: Vec<String> = self.store.process_ids()?;
        ids.extend(
            self.store
                .meta("runs")?
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string)),
        );
        let (mut units, mut n) = (0u64, 0usize);
        for id in &ids {
            let Some(mid) = self
                .run_refs(id)?
                .and_then(|r| s(&r, "metering").map(str::to_string))
            else {
                continue;
            };
            if let Some(m) = self.store.record(&mid)? {
                units += m["units"].as_u64().unwrap_or_default();
                n += 1;
            }
        }
        Ok(json!({"tenants": {self.store.authority(): {"units": units, "processes": n}}}))
    }
}

/// Start a run: a port on a target, or an exported function of an asset. Returns the
/// result and whether the function is retryable.
fn start(core: &mut Core, host: &mut EngineHost<'_>, i: &Value) -> Result<(Value, bool), BpaError> {
    let (res, asset, fname) = if let Some(port) = s(i, "port") {
        core.frames.push(Frame {
            asset: json!({"capabilities": ["run"], "abi": "1.0"}),
            ..Frame::default()
        });
        let mut a = Map::new();
        a.insert(
            "target".into(),
            i.get("target").cloned().unwrap_or(Value::Null),
        );
        a.insert("port".into(), json!(port));
        a.insert(
            "params".into(),
            i.get("params").cloned().unwrap_or_else(|| json!({})),
        );
        let res = host.run_port(core, &a);
        core.frames.pop();
        let res = res?;
        let oid = host.target(core, i.get("target").unwrap_or(&Value::Null))?;
        let cls = host.class_of(core, &oid)?;
        let found = host.registry(core)?.dispatch(&cls, port, None)?;
        let implementation = s(&found, "impl").unwrap_or_default().to_string();
        let (name, fname) = implementation
            .split_once('#')
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .unwrap_or((implementation.clone(), String::new()));
        let (asset, _) = host.asset(core, &name)?;
        (res, asset, fname)
    } else {
        let (asset, vh) = host.asset(core, s(i, "asset").unwrap_or_default())?;
        let fname = s(i, "function").unwrap_or_default().to_string();
        let frame = Frame {
            asset: asset.clone(),
            vh: Some(vh),
            fname: Some(fname.clone()),
            ..Frame::default()
        };
        let args = i.get("args").cloned().unwrap_or_else(|| json!([]));
        let res = program::run_function(core, host, frame, &args)?;
        (res, asset, fname)
    };
    let retryable = asset["source"]["functions"]
        .get(&fname)
        .is_some_and(|f| f["retryable"] == json!(true));
    Ok((res, retryable))
}
