//! Lifecycles of BPA 1.0-rc3 (PH-1 step 12b, DEC-071; DSN-DVM-730…732): the shape of a
//! lifecycle declaration (`DAT-Lifecycle`, STD-CLS-004), the extension rule for subclass
//! lifecycles (FR-FLOW-015), the reachability findings of the BPU class verification
//! (STD-BPU-011), the evaluation of guards and `set` effects as L1 expressions over `this`,
//! `params` and `ctx`, the availability of a port in a lifecycle state (STD-ISA-050) and the
//! action list of an object (STD-ISA-036). The engine is pure: the lifecycle objects, the
//! object versions, permissions and decisions are the kernel's (`ubos-dvm`).

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_rules::expr::{Env, Evaluator, Ty, Val, field_type, from_json, to_json};
use ubos_rules::model::types::{BpaError, names};
use ubos_rules::model::{Port, Registry};

/// The kinds of an effect: `{set: {field: expression}}` or `{logic: "<asset>#<function>"}`.
pub const EFFECT_KINDS: &[&str] = &["set", "logic"];
/// The field rules of a state (STD-CLS-004).
pub const FIELD_RULES: &[&str] = &["read_only", "required", "hidden"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

fn member(v: &Value) -> bool {
    v.as_str().is_some_and(|t| names::matches("member-name", t))
}

fn texts(v: Option<&Value>) -> Vec<&str> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// The transitions of a lifecycle body.
pub fn transitions(body: &Value) -> impl Iterator<Item = &Value> {
    body.get("transitions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// The transition of a lifecycle by name.
pub fn transition<'a>(body: &'a Value, name: &str) -> Option<&'a Value> {
    transitions(body).find(|t| s(t, "name") == Some(name))
}

/// The states a transition leaves.
pub fn leaves(t: &Value, state: Option<&str>) -> bool {
    state.is_some_and(|st| texts(t.get("from")).contains(&st))
}

/// STD-CLS-004: the shape of a `DAT-Lifecycle` body; returns the transitions by name.
pub fn check_lifecycle(body: &Value) -> Result<BTreeMap<String, Value>, BpaError> {
    let states: Vec<&str> = texts(body.get("states"));
    let distinct: BTreeSet<&str> = states.iter().copied().collect();
    if !body["states"].is_array()
        || states.is_empty()
        || body["states"]
            .as_array()
            .is_some_and(|a| a.len() != states.len())
        || states.iter().any(|st| !names::matches("member-name", st))
        || distinct.len() != states.len()
    {
        return Err(invalid(
            "states is a non-empty list of distinct state names",
        ));
    }
    if !s(body, "initial").is_some_and(|i| distinct.contains(i)) {
        return Err(invalid("initial names a state"));
    }
    if let Some(f) = body.get("final").filter(|v| !v.is_null())
        && (!f.is_array()
            || f.as_array().is_some_and(|a| {
                a.iter()
                    .any(|x| !x.as_str().is_some_and(|t| distinct.contains(t)))
            }))
    {
        return Err(invalid("final names states"));
    }
    let Some(trans) = body.get("transitions").and_then(Value::as_array) else {
        return Err(invalid("transitions is a list"));
    };
    let mut by_name: BTreeMap<String, Value> = BTreeMap::new();
    for t in trans {
        let (Some(name), Some(from), Some(to)) = (
            t.get("name").and_then(Value::as_str),
            t.get("from").and_then(Value::as_array),
            t.get("to").and_then(Value::as_str),
        ) else {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "a transition has name, from and to",
            ));
        };
        if !names::matches("member-name", name) {
            return Err(BpaError::new(
                "MODEL.INVALID_NAME",
                format!("transition {name}"),
            ));
        }
        if by_name.contains_key(name) {
            return Err(invalid(&format!("duplicate transition {name}")));
        }
        if from.is_empty()
            || from
                .iter()
                .any(|f| !f.as_str().is_some_and(|t| distinct.contains(t)))
            || !distinct.contains(to)
        {
            return Err(invalid("a transition leaves and enters declared states"));
        }
        for g in t
            .get("guards")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let ok = g.is_object()
                && s(g, "name").is_some()
                && s(g, "rule").is_some()
                && g.get("message").is_none_or(Value::is_string);
            if !ok {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "a guard has name, rule and message",
                ));
            }
            ubos_rules::expr::parse(&g["rule"])?;
        }
        by_name.insert(name.to_string(), t.clone());
    }
    let effects = body.get("effects").filter(|v| !v.is_null());
    if let Some(e) = effects {
        let Some(m) = e.as_object() else {
            return Err(invalid("effects maps declared states"));
        };
        if m.keys().any(|k| !distinct.contains(k.as_str())) {
            return Err(invalid("effects maps declared states"));
        }
        for v in m.values() {
            let Some(kinds) = v.as_object() else {
                return Err(invalid("effects of a state are {entry, exit}"));
            };
            if kinds.keys().any(|k| k != "entry" && k != "exit") {
                return Err(invalid("effects of a state are {entry, exit}"));
            }
            for items in kinds.values() {
                let Some(list) = items.as_array() else {
                    return Err(invalid("effects are lists"));
                };
                for ef in list {
                    check_effect(ef)?;
                }
            }
        }
    }
    let rules = body.get("field_rules").filter(|v| !v.is_null());
    if let Some(r) = rules {
        let Some(m) = r.as_object() else {
            return Err(invalid("field_rules maps declared states"));
        };
        if m.keys().any(|k| !distinct.contains(k.as_str())) {
            return Err(invalid("field_rules maps declared states"));
        }
        for v in m.values() {
            let ok = v.as_object().is_some_and(|o| {
                o.iter().all(|(k, list)| {
                    FIELD_RULES.contains(&k.as_str())
                        && list.as_array().is_some_and(|a| a.iter().all(member))
                })
            });
            if !ok {
                return Err(invalid("field rules are lists of field names"));
            }
        }
    }
    Ok(by_name)
}

fn check_effect(ef: &Value) -> Result<(), BpaError> {
    let Some(o) = ef.as_object() else {
        return Err(invalid("an effect is {set} or {logic}"));
    };
    let kinds: Vec<&str> = EFFECT_KINDS
        .iter()
        .copied()
        .filter(|k| o.contains_key(*k))
        .collect();
    if kinds.len() != 1 || o.len() != 1 {
        return Err(invalid("an effect is {set} or {logic}"));
    }
    if kinds[0] == "set" {
        let Some(sets) = o["set"].as_object().filter(|m| !m.is_empty()) else {
            return Err(invalid("set maps fields to expressions"));
        };
        for (k, v) in sets {
            if !names::matches("member-name", k) || !v.is_string() {
                return Err(invalid("set maps fields to expressions"));
            }
            ubos_rules::expr::parse(v)?;
        }
    } else if !o["logic"].as_str().is_some_and(|t| t.contains('#')) {
        return Err(invalid("logic names <asset>#<function>"));
    }
    Ok(())
}

/// FR-FLOW-015: `child` keeps the initial state, every state and every transition of
/// `parent` (a transition keeps its name and target and may leave more states).
pub fn extends(parent: &Value, child: &Value) -> bool {
    let child_states = texts(child.get("states"));
    if s(child, "initial") != s(parent, "initial")
        || texts(parent.get("states"))
            .iter()
            .any(|st| !child_states.contains(st))
    {
        return false;
    }
    transitions(parent).all(|t| {
        transition(child, s(t, "name").unwrap_or_default()).is_some_and(|c| {
            s(c, "to") == s(t, "to") && {
                let cf = texts(c.get("from"));
                texts(t.get("from")).iter().all(|f| cf.contains(f))
            }
        })
    })
}

/// STD-BPU-011 item 2: unreachable states and dead ends other than final states.
pub fn findings(body: &Value) -> Vec<Value> {
    let initial = s(body, "initial").unwrap_or_default();
    let mut reach: BTreeSet<&str> = BTreeSet::new();
    let mut todo = vec![initial];
    reach.insert(initial);
    while let Some(st) = todo.pop() {
        for t in transitions(body) {
            if let Some(to) = s(t, "to")
                && texts(t.get("from")).contains(&st)
                && reach.insert(to)
            {
                todo.push(to);
            }
        }
    }
    let finals = texts(body.get("final"));
    let mut out = Vec::new();
    for st in texts(body.get("states")) {
        if !reach.contains(st) {
            out.push(json!({"state": st, "finding": "unreachable"}));
        } else if !finals.contains(&st)
            && !transitions(body).any(|t| texts(t.get("from")).contains(&st))
        {
            out.push(json!({"state": st, "finding": "dead_end"}));
        }
    }
    out
}

/// The value of an L1 expression over `this`, `params` (typed by the port's parameter
/// declarations) and `ctx`; a failure to evaluate is an error.
pub fn eval_value(
    reg: &Registry,
    class: &str,
    text: &str,
    this: &Value,
    params: Option<(&[Value], &Value)>,
    ctx: Option<&Map<String, Value>>,
) -> Result<Value, BpaError> {
    let ast = ubos_rules::expr::parse(&Value::String(text.to_string()))?;
    let this_t = ubos_rules::expr::class_type(reg, class)?;
    let params_t = params.map(|(decls, _)| {
        Ty::Obj(
            decls
                .iter()
                .map(|q| {
                    (
                        s(q, "name").unwrap_or_default().to_string(),
                        field_type(q, reg.value_types()),
                    )
                })
                .collect(),
        )
    });
    let mut env = Env::new(Some(this_t.clone()), params_t.clone(), None, None);
    ubos_rules::expr::check(&ast, &mut env)?;
    let mut ev = Evaluator::new(&env);
    ev.this = Some(from_json(&this_t, this));
    if let (Some(t), Some((_, p))) = (&params_t, params) {
        ev.params = Some(from_json(t, p));
    }
    if let Some(c) = ctx {
        ev.ctx = from_json(
            &ubos_rules::expr::check::ctx_type(),
            &Value::Object(c.clone()),
        );
    }
    to_json(&ev.eval(&ast)?)
}

/// A guard holds when its rule evaluates to true; a failure to evaluate is a failed guard.
pub fn guard_ok(
    reg: &Registry,
    class: &str,
    rule: &str,
    this: &Value,
    params: Option<(&[Value], &Value)>,
    ctx: Option<&Map<String, Value>>,
) -> Result<bool, BpaError> {
    match eval_value(reg, class, rule, this, params, ctx) {
        Ok(v) => Ok(v == Value::Bool(true)),
        Err(e) if matches!(e.code, "LOGIC.TYPE_ERROR" | "LOGIC.NULL_ACCESS") => Ok(false),
        Err(e) => Err(e),
    }
}

/// The parameter declarations of a port.
pub fn param_decls(port: &Value) -> Vec<Value> {
    port.get("params")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// STD-ISA-050: whether a port is available in a lifecycle state: its `states`, when
/// declared, include the state, and the transition it names leaves the state.
pub fn available(port: &Value, lifecycle: Option<&Value>, state: Option<&str>) -> bool {
    if let Some(states) = port.get("states").and_then(Value::as_array)
        && !states.is_empty()
        && !state.is_some_and(|st| states.iter().any(|x| x.as_str() == Some(st)))
    {
        return false;
    }
    if let (Some(name), Some(lc)) = (s(port, "transition"), lifecycle) {
        return transition(lc, name).is_some_and(|t| leaves(t, state));
    }
    true
}

/// The port guards that fail (STD-ISA-050): `(index, text)`.
pub fn failed_port_guards(
    reg: &Registry,
    class: &str,
    port: &Value,
    this: &Value,
    params: &Value,
    ctx: &Map<String, Value>,
) -> Result<Vec<(usize, String)>, BpaError> {
    let decls = param_decls(port);
    let mut out = Vec::new();
    for (i, g) in texts(port.get("guards")).into_iter().enumerate() {
        if !guard_ok(reg, class, g, this, Some((&decls, params)), Some(ctx))? {
            out.push((i, g.to_string()));
        }
    }
    Ok(out)
}

/// The guards of a transition that fail (STD-ISA-017): `{name, message}` each.
pub fn failed_transition_guards(
    reg: &Registry,
    class: &str,
    t: &Value,
    decls: &[Value],
    this: &Value,
    params: &Value,
    ctx: &Map<String, Value>,
) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for g in t
        .get("guards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let rule = s(g, "rule").unwrap_or_default();
        if !guard_ok(reg, class, rule, this, Some((decls, params)), Some(ctx))? {
            let name = s(g, "name").unwrap_or_default();
            out.push(json!({"name": name, "message": s(g, "message").unwrap_or(name)}));
        }
    }
    Ok(out)
}

/// The field rules of a state: `{read_only?, required?, hidden?}`.
pub fn field_rules<'a>(lifecycle: &'a Value, state: &str) -> Option<&'a Value> {
    lifecycle.get("field_rules").and_then(|r| r.get(state))
}

/// The effects of a state of a kind (`entry` or `exit`).
pub fn effects<'a>(lifecycle: &'a Value, state: &str, kind: &str) -> Vec<&'a Value> {
    lifecycle
        .get("effects")
        .and_then(|e| e.get(state))
        .and_then(|e| e.get(kind))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect()
}

/// STD-ISA-036: the ports of an object with their availability and reasons, in port-name
/// order. `permit(action, class)` is the principal's permission check and `decide(port)` the
/// decisions of a command port (STD-LGS-031), both the kernel's.
#[allow(clippy::too_many_arguments)]
pub fn actions(
    reg: &Registry,
    class: &str,
    ports: &BTreeMap<String, Port>,
    lifecycle: Option<&Value>,
    state: Option<&str>,
    body: &Value,
    ctx: &Map<String, Value>,
    permit: &mut dyn FnMut(&str, &str) -> bool,
    decide: &mut dyn FnMut(&str) -> Result<Value, BpaError>,
) -> Result<Value, BpaError> {
    let mut out = Vec::new();
    let none = json!({});
    for (name, p) in ports {
        let d = &p.decl;
        let mut a = json!({"port": name, "kind": d.get("kind").cloned().unwrap_or(Value::Null)});
        let tr = s(d, "transition").and_then(|n| lifecycle.and_then(|lc| transition(lc, n)));
        if let Some(n) = s(d, "transition") {
            a["transition"] = json!(n);
            if let Some(to) = tr.and_then(|t| s(t, "to")) {
                a["to"] = json!(to);
            }
        }
        let mut reason: Option<&str> = None;
        if !available(d, lifecycle, state) {
            reason = Some("state");
        } else if !permit(s(d, "permission").unwrap_or("none"), class) {
            reason = Some("permission");
        } else if d.get("kind") == Some(&json!("command")) {
            let r = decide(name)?;
            if s(&r, "outcome") != Some("allow") {
                reason = Some("decision");
                a["decision"] = r["outcome"].clone();
            } else {
                let decls = param_decls(d);
                let mut failed: Vec<Value> = failed_port_guards(reg, class, d, body, &none, ctx)?
                    .into_iter()
                    .map(|(_, g)| json!(g))
                    .collect();
                if let Some(t) = tr {
                    let tname = s(d, "transition").unwrap_or_default();
                    failed.extend(
                        failed_transition_guards(reg, class, t, &decls, body, &none, ctx)?
                            .iter()
                            .map(|g| {
                                json!(format!("{tname}/{}", s(g, "name").unwrap_or_default()))
                            }),
                    );
                }
                if !failed.is_empty() {
                    reason = Some("guard");
                    a["guards"] = Value::Array(failed);
                }
            }
        }
        a["available"] = json!(reason.is_none());
        if let Some(r) = reason {
            a["reason"] = json!(r);
        }
        out.push(a);
    }
    Ok(json!({"state": state, "actions": out}))
}

/// The `{from, to}` of a transition record kept by a process for its commit: the state the
/// object leaves, the state it enters and the fields its effects set (exempt from the
/// `read_only` rule of the state left).
#[derive(Debug, Clone, Default)]
pub struct Moved {
    /// State left.
    pub from: String,
    /// State entered.
    pub to: String,
    /// Fields set by the effects.
    pub fields: BTreeSet<String>,
}

/// What a process hands to its commit (STD-ISA-017, STD-TIME-040): the transitions by
/// object and the guards passed, as `{guard, result}`.
#[derive(Debug, Clone, Default)]
pub struct Flow {
    /// Transitions by object identifier.
    pub transitions: BTreeMap<String, Moved>,
    /// Guards evaluated, in order.
    pub guards: Vec<Value>,
}

impl Flow {
    /// Record a guard passed.
    pub fn passed(&mut self, guard: &str) {
        self.guards.push(json!({"guard": guard, "result": true}));
    }
}

/// `Val` of the kernel's typed parameters as JSON (for guards over the frame's parameters).
pub fn params_json(params: &BTreeMap<String, Val>) -> Result<Value, BpaError> {
    let mut m = Map::new();
    for (k, v) in params {
        m.insert(k.clone(), to_json(v)?);
    }
    Ok(Value::Object(m))
}
