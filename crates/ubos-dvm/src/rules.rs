//! Rules in the kernel (STD-LGS-011, -030, -031; DSN-DVM-707…709, -720, -721, -725): the
//! defaults of a create, the commit enforcement of the commit properties, validations and
//! invariants, the validation preview and the decisions before a command port.

use crate::sheets::{cascade_state, version_of};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use ubos_expr::{Env, Evaluator, Ty, Val, field_type, from_json};
use ubos_model::Registry;
use ubos_rules::Cache;
use ubos_types::BpaError;
use ubos_version::{Repo, State, Tx};

/// Default of `set_limits.max_violations` (FR-RULE-034).
pub const MAX_VIOLATIONS: u64 = 100;

/// The steps the action properties require (STD-LGS-031).
const STEP_OF: &[(&str, &str)] = &[
    ("approvals", "approval"),
    ("require_signature", "signature"),
    ("maker_checker", "second_person"),
];

/// The context the declarations of a commit see (STD-LGS-020; DEC-067): the branch kind,
/// the channel, the principal kind and the role of the process, and the VAE.
pub(crate) fn rule_ctx(p: &Value, branch_kind: &str, vae: &str) -> Map<String, Value> {
    let g = |k: &str, d: &str| -> Value {
        p.get(k)
            .filter(|v| v.is_string())
            .cloned()
            .unwrap_or_else(|| json!(d))
    };
    let mut ctx = Map::new();
    ctx.insert("branch_kind".into(), json!(branch_kind));
    ctx.insert("channel".into(), g("channel", "api"));
    ctx.insert("principal_kind".into(), g("principal_kind", "human"));
    ctx.insert("vae".into(), json!(vae));
    if let Some(r) = p.get("role").filter(|v| !v.is_null()) {
        ctx.insert("role".into(), r.clone());
    }
    ctx
}

/// Business objects only: the objects of `ConfigAsset` and `LogicAsset` classes (sheets,
/// extensions, logic) are governed by write policy and change sets, not by the
/// declarations they carry (STD-LGS-011 notes).
fn governed(reg: &Registry, class: &str) -> bool {
    reg.class(class).is_ok_and(|c| {
        c.kind.as_deref() == Some("definition")
            && !c
                .linearization
                .iter()
                .any(|l| l == "ConfigAsset" || l == "LogicAsset")
    })
}

/// True when an L1 rule holds on `this` (with `old`, `params` typed by a port's parameter
/// declarations, and `ctx`); a type error or a null access counts as not holding.
pub(crate) fn eval_rule(
    reg: &Registry,
    class: &str,
    rule: &str,
    this: &Value,
    old: Option<&Value>,
    params: Option<(&[Value], &Value)>,
    ctx: Option<&Map<String, Value>>,
) -> Result<bool, BpaError> {
    let run = || -> Result<Val, BpaError> {
        let ast = ubos_expr::parse(&Value::String(rule.to_string()))?;
        let this_t = ubos_expr::class_type(reg, class)?;
        let params_t = params.map(|(decls, _)| {
            Ty::Obj(
                decls
                    .iter()
                    .map(|q| {
                        (
                            q.get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            field_type(q, reg.value_types()),
                        )
                    })
                    .collect(),
            )
        });
        let mut env = Env::new(
            Some(this_t.clone()),
            params_t.clone(),
            old.map(|_| this_t.clone()),
            None,
        );
        ubos_expr::check(&ast, &mut env)?;
        let mut ev = Evaluator::new(&env);
        ev.this = Some(from_json(&this_t, this));
        if let Some(o) = old {
            ev.old = Some(from_json(&this_t, o));
        }
        if let (Some(t), Some((_, p))) = (&params_t, params) {
            ev.params = Some(from_json(t, p));
        }
        if let Some(c) = ctx {
            ev.ctx = from_json(&ubos_expr::check::ctx_type(), &Value::Object(c.clone()));
        }
        ev.eval(&ast)
    };
    match run() {
        Ok(Val::Bool(true)) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) if matches!(e.code, "LOGIC.TYPE_ERROR" | "LOGIC.NULL_ACCESS") => Ok(false),
        Err(e) => Err(e),
    }
}

/// STD-LGS-011 `field_default`: the defaults of the cascade fill the absent fields of a
/// created object before its validation (DEC-067).
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_defaults(
    repo: &mut Repo<'_>,
    tx: &Tx,
    reg_ext: &Registry,
    cache: &mut Cache,
    class: &str,
    body: &mut Map<String, Value>,
    asof: &str,
    ctx: &Map<String, Value>,
    state: Option<&str>,
) -> Result<(), BpaError> {
    if !governed(reg_ext, class) {
        return Ok(());
    }
    let given = Value::Object(body.clone());
    let (eff, _, _) = cascade_state(
        repo,
        Some(tx),
        &tx.state,
        reg_ext,
        cache,
        class,
        &given,
        asof,
        ctx,
        state,
    )?;
    for (path, (v, _)) in &eff {
        if let Some(f) = path.strip_prefix("field_default.")
            && body.get(f).is_none_or(Value::is_null)
        {
            body.insert(f.to_string(), v.clone());
        }
    }
    Ok(())
}

fn unsatisfied(property: &str, oid: &str, extra: &[(&str, Value)]) -> BpaError {
    let mut d = json!({"property": property, "object_id": oid});
    for (k, v) in extra {
        d[*k] = v.clone();
    }
    BpaError::with_details("RULE.DECLARATION_UNSATISFIED", d)
}

fn item(code: &str, oid: &str, r: &Value) -> Value {
    json!({
        "code": code, "object_id": oid, "name": r["name"], "severity": r["severity"],
        "message": r.get("message").filter(|m| m.is_string()).cloned().unwrap_or_else(|| r["name"].clone()),
    })
}

/// What the enforcement of a commit needs besides the transaction.
pub(crate) struct Enforce<'a> {
    /// The process of the commit as given, with defaults.
    pub process: &'a Value,
    /// The commit comes through a change set.
    pub via_change_set: bool,
    /// Kind of the branch.
    pub branch_kind: &'a str,
    /// The branch is protected (STD-VER-012).
    pub protected: bool,
    /// Objects written `(object, version)` in change order, ended ones included.
    pub written: &'a [(String, String)],
    /// Objects ended `(object, date)`.
    pub ended: &'a [(String, String)],
    /// Default valid-time start per class.
    pub default_valid: &'a dyn Fn(&str) -> String,
    /// Largest number of violations reported.
    pub max_violations: u64,
}

/// STD-LGS-011 commit properties, STD-LGS-030 validations and invariants (DSN-DVM-707,
/// -725): `RULE.DECLARATION_UNSATISFIED`, `VER.CHANGE_SET_REQUIRED` or
/// `RULE.VALIDATION_FAILED` with every error-level violation; returns the warnings and the
/// versions of the sheets the cascades bound (the `sheets` of the rule binding, 1.0-rc7).
pub(crate) fn enforce(
    repo: &mut Repo<'_>,
    tx: &Tx,
    reg_ext: &Registry,
    cache: &mut Cache,
    e: &Enforce<'_>,
) -> Result<(Vec<Value>, BTreeSet<String>), BpaError> {
    let ctx = rule_ctx(e.process, e.branch_kind, &repo.store.authority());
    let mut bound: BTreeSet<String> = BTreeSet::new();
    let mut written: Vec<(String, String, Value, Option<String>)> = Vec::new();
    for (oid, vh) in e.written {
        let Some(o) = tx.state.objects.get(oid) else {
            continue;
        };
        if !governed(reg_ext, &o.class) {
            continue;
        }
        let v = version_of(repo, Some(tx), vh)?;
        if v["tombstone"] != json!(true) {
            written.push((
                oid.clone(),
                vh.clone(),
                v["body"].clone(),
                crate::flow::state_of(&v),
            ));
        }
    }
    let written_ids: BTreeSet<&str> = written.iter().map(|(o, ..)| o.as_str()).collect();
    for (oid, _) in e.ended {
        let Some(o) = tx.state.objects.get(oid) else {
            continue;
        };
        if written_ids.contains(oid.as_str()) || !governed(reg_ext, &o.class) {
            continue;
        }
        let v = version_of(repo, Some(tx), &o.head)?;
        let body = v["body"].clone();
        let asof = (e.default_valid)(&o.class);
        let (eff, b, _) = cascade_state(
            repo,
            Some(tx),
            &tx.state,
            reg_ext,
            cache,
            &o.class,
            &body,
            &asof,
            &ctx,
            crate::flow::state_of(&v).as_deref(),
        )?;
        bound.extend(b);
        if eff.get("read_only").map(|(v, _)| v) == Some(&json!(true)) {
            return Err(unsatisfied("read_only", oid, &[]));
        }
    }
    let given = |k: &str| {
        e.process
            .get(k)
            .and_then(Value::as_str)
            .is_some_and(|t| !t.is_empty())
    };
    let (message_ok, reason_ok) = (given("message"), given("reason"));
    let mut violations: Vec<Value> = Vec::new();
    let mut warnings: Vec<Value> = Vec::new();
    let mut invariants: BTreeMap<String, Value> = BTreeMap::new();
    for (oid, _, body, state) in &written {
        let class = tx.state.objects[oid].class.clone();
        let existed = tx.loaded.get(oid).is_some_and(Option::is_some);
        let old = match tx.loaded.get(oid) {
            Some(Some(prev)) => version_of(repo, Some(tx), &prev.head)?["body"].clone(),
            _ => json!({}),
        };
        let asof = (e.default_valid)(&class);
        let (eff, b, _) = cascade_state(
            repo,
            Some(tx),
            &tx.state,
            reg_ext,
            cache,
            &class,
            body,
            &asof,
            &ctx,
            state.as_deref(),
        )?;
        bound.extend(b);
        let value = |p: &str| eff.get(p).map(|(v, _)| v.clone()).unwrap_or(Value::Null);
        if value("read_only") == json!(true) && existed {
            return Err(unsatisfied("read_only", oid, &[]));
        }
        for f in value("field_required").as_array().into_iter().flatten() {
            let f = f.as_str().unwrap_or_default();
            if body.get(f).is_none_or(Value::is_null) {
                return Err(unsatisfied("field_required", oid, &[("field", json!(f))]));
            }
        }
        if value("audit_level") == json!("strict") && !(message_ok && reason_ok) {
            return Err(unsatisfied(
                "audit_level",
                oid,
                &[(
                    "message",
                    json!("a strict commit carries message and reason"),
                )],
            ));
        }
        if value("write_policy") == json!("change_set") && !e.via_change_set && e.protected {
            return Err(BpaError::new("VER.CHANGE_SET_REQUIRED", class.clone()));
        }
        let mut rules: Vec<Value> = reg_ext.class(&class)?.validations.clone();
        rules.extend(value("validations").as_array().cloned().unwrap_or_default());
        for r in &rules {
            let rule = r["rule"].as_str().unwrap_or_default();
            if !eval_rule(reg_ext, &class, rule, body, Some(&old), None, None)? {
                let it = item("RULE.VALIDATION_FAILED", oid, r);
                if r["severity"] == json!("error") {
                    violations.push(it);
                } else {
                    warnings.push(it);
                }
            }
        }
        for inv in value("invariants").as_array().into_iter().flatten() {
            let name = inv["name"].as_str().unwrap_or_default().to_string();
            invariants.entry(name).or_insert_with(|| inv.clone());
        }
    }
    let empty = json!({});
    for inv in invariants.values() {
        let scope: Vec<&str> = inv["scope"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for (oid, _, body, _) in &written {
            let class = tx.state.objects[oid].class.clone();
            let lin = &reg_ext.class(&class)?.linearization;
            if !lin.iter().any(|l| scope.contains(&l.as_str())) {
                continue;
            }
            let rule = inv["rule"].as_str().unwrap_or_default();
            if !eval_rule(reg_ext, &class, rule, body, Some(&empty), None, None)? {
                let it = item("RULE.INVARIANT_VIOLATED", oid, inv);
                if inv["severity"] == json!("error") {
                    violations.push(it);
                } else {
                    warnings.push(it);
                }
            }
        }
    }
    if !violations.is_empty() {
        violations.truncate(e.max_violations as usize);
        return Err(BpaError::with_details(
            "RULE.VALIDATION_FAILED",
            json!({"violations": violations}),
        ));
    }
    Ok((warnings, bound))
}

impl Engine {
    /// `DAT-DecisionResult` of a port call on an object of a state (STD-LGS-031,
    /// DSN-DVM-720): any `deny` denies, otherwise the union of the steps requires.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decide_in(
        &mut self,
        st: &State,
        branch: &str,
        oid: &str,
        port: &str,
        params: &Value,
        ctx: &Map<String, Value>,
        lazy: Option<(String, Option<u64>)>,
    ) -> Result<Value, BpaError> {
        let kind = self.branch_meta(branch)?.kind;
        let Some(o) = st.objects.get(oid).cloned() else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        };
        let asof = self.default_valid_of(&o.class);
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.version(&o.head)?;
        if head["tombstone"] == json!(true) {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        }
        let exts = crate::relations::extensions_of(&mut repo, st)?;
        let reg_ext: Cow<Registry> = if exts.is_empty() {
            Cow::Borrowed(&self.registry)
        } else {
            Cow::Owned(self.registry.with_extensions(&exts)?)
        };
        let mut repo = Repo::new(self.store.as_mut(), &reg_ext);
        let Some(decl) = reg_ext
            .class(&o.class)?
            .ports
            .get(port)
            .map(|p| p.decl.clone())
        else {
            return Err(BpaError::new(
                "MODEL.PORT_NOT_IMPLEMENTED",
                format!("no port {port}"),
            ));
        };
        let body = head["body"].clone();
        let mut ctx = ctx.clone();
        let vae = repo.store.authority();
        for (k, d) in [
            ("principal_kind", "human"),
            ("branch_kind", kind.as_str()),
            ("vae", vae.as_str()),
            ("channel", "api"),
        ] {
            ctx.entry(k.to_string()).or_insert_with(|| json!(d));
        }
        // over the partial view of a process, a `:rel` qualifier reads at its snapshot
        let (eff, bindings, _) = crate::sheets::cascade_lazy(
            &mut repo,
            None,
            st,
            &reg_ext,
            &mut self.rules,
            &o.class,
            &body,
            &asof,
            &ctx,
            crate::flow::state_of(&head).as_deref(),
            lazy,
        )?;
        if decl.get("kind") == Some(&json!("query")) {
            return Ok(json!({"outcome": "allow", "explanations": [], "bindings": bindings}));
        }
        let value = |p: &str| eff.get(p).map(|(v, _)| v.clone()).unwrap_or(Value::Null);
        let ptypes: Vec<Value> = decl
            .get("params")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let empty = json!({});
        let mut denies = 0;
        let mut steps: Vec<Value> = Vec::new();
        let mut explanations: Vec<Value> = Vec::new();
        for d in value("decisions").as_array().into_iter().flatten() {
            if let Some(when) = d
                .get("when")
                .and_then(Value::as_str)
                .filter(|w| !w.is_empty())
                && !eval_rule(
                    &reg_ext,
                    &o.class,
                    when,
                    &body,
                    Some(&empty),
                    Some((&ptypes, params)),
                    Some(&ctx),
                )?
            {
                continue;
            }
            let outcome = d["outcome"].as_str().unwrap_or_default();
            if outcome == "allow" {
                continue;
            }
            let d_steps: Vec<Value> = d
                .get("steps")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            explanations.push(json!({
                "name": d["name"], "outcome": outcome,
                "message": d.get("message").filter(|m| m.is_string()).cloned().unwrap_or_else(|| d["name"].clone()),
                "steps": d_steps,
            }));
            if outcome == "deny" {
                denies += 1;
            } else {
                for st in d_steps {
                    if !steps.contains(&st) {
                        steps.push(st);
                    }
                }
            }
        }
        for (prop, step) in STEP_OF {
            let v = value(prop);
            let on = v == json!(true) || v.get("count").and_then(Value::as_i64).unwrap_or(0) > 0;
            if on {
                explanations.push(json!({"name": prop, "outcome": "require", "message": prop, "steps": [step], "value": v}));
                if !steps.contains(&json!(step)) {
                    steps.push(json!(step));
                }
            }
        }
        let outcome = if denies > 0 {
            "deny"
        } else if !steps.is_empty() {
            "require"
        } else {
            "allow"
        };
        let mut out =
            json!({"outcome": outcome, "explanations": explanations, "bindings": bindings});
        if !steps.is_empty() && denies == 0 {
            out["steps"] = Value::Array(steps);
        }
        Ok(out)
    }

    /// `decide` (STD-CONF-005): the decisions of a port call without running it.
    pub fn decide(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = input
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let oid = input
            .get("object_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let port = input
            .get("port")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let (_, _, _, st) = self.object_view(&branch, input, &oid)?;
        let st = match st {
            Some(s) => s,
            None => {
                let time = input.get("time").and_then(Value::as_u64);
                Repo::new(self.store.as_mut(), &self.registry).state_at(&branch, time)?
            }
        };
        let params = input.get("params").cloned().unwrap_or_else(|| json!({}));
        let ctx = input
            .get("ctx")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        self.decide_in(&st, &branch, &oid, &port, &params, &ctx, None)
    }

    /// `validate` (STD-CONF-005, FR-RULE-035): the violations and warnings a commit of the
    /// changes would produce, without committing (DSN-DVM-709).
    pub fn validate(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = input
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let mut next = self.next_process;
        let p = crate::versioning::process(
            self.store.as_mut(),
            input.get("process"),
            "action",
            &mut next,
            &self.principal,
        )?;
        let meta = self.branch_meta(&branch)?;
        if ubos_version::expires_of(&meta)?.is_some_and(|exp| self.now >= exp) {
            return Err(BpaError::new("VER.BRANCH_ARCHIVED", branch));
        }
        let refs = self.refs_indexed(&branch)?;
        let r = crate::stateful::commit_on(
            self.store.as_mut(),
            &self.registry,
            &self.now,
            input,
            &p,
            false,
            &meta,
            self.max_inputs,
            None,
            true,
            self.max_violations,
            &mut self.rules,
            None,
            refs,
        );
        match r {
            Ok((result, _, _)) => Ok(json!({
                "violations": [],
                "warnings": result.get("warnings").cloned().unwrap_or_else(|| json!([])),
            })),
            Err(e) if e.code == "RULE.VALIDATION_FAILED" => Ok(json!({
                "violations": e.details.and_then(|d| d.get("violations").cloned()).unwrap_or_else(|| json!([])),
                "warnings": [],
            })),
            Err(e) => Err(e),
        }
    }
}
