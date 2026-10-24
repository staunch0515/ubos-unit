//! `ubos-expr` — engine layer (UBS-ARC-07 §2): the L1 expression language of
//! STD-EXPR-001…005 (PH-1 step 5a, DEC-055): lexer, parser, static type checker and
//! evaluator with the ABI 1.0 function library, the conversion between canonical JSON and
//! run-time values, and the normalisation of custom value types (STD-FND-026).
//!
//! Allowed dependencies: `ubos-model` and the crates before it (AR-031). The reference
//! semantics are those of the oracle `standard/tools/bpa/expr.py`.

pub mod ast;
pub mod check;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod value;

/// The model layer below this crate, for crates whose allowed dependency is `ubos-expr`
/// (UBS-ARC-07 §2).
pub use ubos_model as model;

pub use ast::Node;
pub use check::{Env, check};
pub use eval::{Evaluator, MAX_ITEMS, MAX_UNITS, OrderKey, Resolver, order_key};
pub use parser::parse;
pub use value::{Ty, Val, field_type, from_json, to_json, type_name};

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use ubos_model::Registry;
use ubos_model::types::BpaError;
use ubos_model::types::values::validate_value;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The static type of a class: an object of its effective fields; custom value types
/// (the registry's) are their base type.
pub fn class_type(reg: &Registry, name: &str) -> Result<Ty, BpaError> {
    let c = reg.class(name)?;
    Ok(Ty::Obj(
        c.fields
            .iter()
            .map(|(k, f)| (k.clone(), field_type(f, reg.value_types())))
            .collect(),
    ))
}

fn params_type(params: Option<&Value>, custom: &BTreeMap<String, Value>) -> Ty {
    Ty::Obj(
        params
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), field_type(v, custom)))
            .collect(),
    )
}

/// The division target of an expression: `(scale, rounding)` of a decimal or money field
/// (STD-FND-022), and the target type.
fn target_of(target: Option<&Value>) -> (Option<(u32, String)>, Option<String>) {
    let Some(t) = target.filter(|t| !t.is_null()) else {
        return (None, None);
    };
    let ty = s(t, "type").map(str::to_string);
    let c = t.get("constraints").and_then(Value::as_object);
    let rounding = c
        .and_then(|c| c.get("rounding"))
        .and_then(Value::as_str)
        .unwrap_or("half_even")
        .to_string();
    match ty.as_deref() {
        Some("decimal") => match c.and_then(|c| c.get("scale")).and_then(Value::as_u64) {
            Some(scale) => (Some((scale as u32, rounding)), ty),
            None => (None, ty),
        },
        Some("money") => (
            Some((
                c.and_then(|c| c.get("scale"))
                    .and_then(Value::as_u64)
                    .unwrap_or(2) as u32,
                rounding,
            )),
            ty,
        ),
        _ => (None, ty),
    }
}

/// The registry of the `classes` of an expression input.
fn registry(i: &Value) -> Result<Registry, BpaError> {
    let classes: Vec<Value> = i
        .get("classes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    ubos_model::registry_of(&classes, &[])
}

/// `(ast, env, static type)` of an expression input (adapter operations `check_expr` and
/// `eval_expr`) over the registry of its classes.
pub fn prepare(i: &Value, reg: &Registry) -> Result<(Node, Env, Ty), BpaError> {
    let custom = reg.value_types();
    let this_t = match s(i, "class") {
        Some(c) => Some(class_type(reg, c)?),
        None => None,
    };
    let params = i
        .get("params_types")
        .filter(|v| !v.is_null())
        .map(|p| params_type(Some(p), custom));
    let with_old = i.get("old").is_some_and(|v| !v.is_null())
        || i.get("with_old")
            .is_some_and(|v| !v.is_null() && v != &json!(false));
    let mut env = Env::new(
        this_t.clone(),
        params,
        if with_old { this_t } else { None },
        target_of(i.get("target")).0,
    );
    let ast = parse(i.get("expr").unwrap_or(&Value::Null))?;
    let t = check(&ast, &mut env)?;
    Ok((ast, env, t))
}

/// STD-EXPR-001, -002: the static type of an expression.
pub fn check_expr(i: &Value) -> Result<Value, BpaError> {
    let (_, _, t) = prepare(i, &registry(i)?)?;
    Ok(json!({"type": type_name(&t)}))
}

/// STD-EXPR-001…005: evaluate an expression; the result is canonical JSON, assigned to
/// `target` when given (rounded to its scale).
pub fn eval_expr(i: &Value, resolver: Option<&dyn Resolver>) -> Result<Value, BpaError> {
    let reg = registry(i)?;
    let (ast, env, t) = prepare(i, &reg)?;
    let this_t = env.this.clone();
    let ctx: Map<String, Value> = i
        .get("ctx")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut ev = Evaluator::new(&env);
    ev.ctx = from_json(&check::ctx_type(), &Value::Object(ctx));
    ev.objects = i
        .get("objects")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    ev.calendars = i
        .get("calendars")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    ev.rates = i
        .get("rates")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    ev.fx_rates = i.get("fx_rates").and_then(Value::as_array).cloned();
    if let Some(n) = i.get("max_units").and_then(Value::as_u64) {
        ev.max_units = n;
    }
    if let Some(n) = i.get("max_items").and_then(Value::as_u64) {
        ev.max_items = n as usize;
    }
    if let Some(z) = s(i, "business_tz") {
        ev.tz = z.to_string();
    }
    if let Some(t) = &this_t {
        ev.this = i.get("this").map(|v| from_json(t, v));
        if let Some(p) = &env.params {
            ev.params = i.get("params").map(|v| from_json(p, v));
        }
        if env.old.is_some() {
            ev.old = i.get("old").map(|v| from_json(t, v));
        }
    } else if let Some(p) = &env.params {
        ev.params = i.get("params").map(|v| from_json(p, v));
    }
    let typed = ObjectsResolver {
        reg: &reg,
        classes: i
            .get("classes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|d| s(d, "name").map(str::to_string))
            .collect(),
        objects: ev.objects.clone(),
    };
    ev.resolver = match resolver {
        Some(r) => Some(r),
        None if !ev.objects.is_empty() => Some(&typed),
        None => None,
    };
    let v = to_json(&ev.eval(&ast)?)?;
    let v = assign_target(v, i.get("target"))?;
    Ok(json!({"value": v, "type": type_name(&t)}))
}

/// Round and validate a result against the target field, when given.
fn assign_target(v: Value, target: Option<&Value>) -> Result<Value, BpaError> {
    let Some(t) = target.filter(|t| !t.is_null()) else {
        return Ok(v);
    };
    if v.is_null() {
        return Ok(v);
    }
    let c = t.get("constraints").and_then(Value::as_object).cloned();
    let mut v = v;
    if s(t, "type") == Some("decimal")
        && let Some(c) = &c
        && let Some(scale) = c.get("scale").and_then(Value::as_u64)
        && let Some(text) = v.as_str()
    {
        let mode = ubos_model::types::decimal::Rounding::parse(
            c.get("rounding")
                .and_then(Value::as_str)
                .unwrap_or("half_even"),
        )?;
        let d = ubos_model::types::decimal::Decimal::parse(text)?;
        v = Value::String(d.round(scale as u32, mode).format(Some(scale as u32)));
    }
    let cons = c.filter(|c| !c.is_empty()).map(Value::Object);
    validate_value(s(t, "type").unwrap_or_default(), cons.as_ref(), &v)
}

/// Resolver over the `objects` given to `eval_expr`: bodies by canonical URI, typed by the
/// class named in the URI when it is among the given classes.
struct ObjectsResolver<'a> {
    reg: &'a Registry,
    classes: Vec<String>,
    objects: Map<String, Value>,
}

impl Resolver for ObjectsResolver<'_> {
    fn resolve(&self, uri: &str) -> Result<Option<Val>, BpaError> {
        let Some(body) = self.objects.get(uri) else {
            return Ok(None);
        };
        let class = uri.split('/').nth(3).unwrap_or_default();
        let t = if self.classes.iter().any(|c| c == class) {
            class_type(self.reg, class)?
        } else {
            Ty::Any
        };
        Ok(Some(from_json(&t, body)))
    }
}

/// Normalise, then validate, a value of a custom type (STD-FND-026); returns the canonical
/// value. `d` is the declaration (`base`, `constraints`, `normalize`, `validate`).
pub fn custom_value(
    d: &Value,
    value: &Value,
    constraints: Option<&Value>,
) -> Result<Value, BpaError> {
    let base = s(d, "base").unwrap_or_default();
    let mut c = d
        .get("constraints")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(extra) = constraints.and_then(Value::as_object) {
        c.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    let mut v = validate_value(base, None, value)?;
    if let Some(n) = s(d, "normalize") {
        v = run_member(base, n, &v)?;
        if v.is_null() {
            return Err(BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "the normaliser gave null",
            ));
        }
    }
    let cons = if c.is_empty() {
        None
    } else {
        Some(Value::Object(c))
    };
    v = validate_value(base, cons.as_ref(), &v)?;
    if let Some(e) = s(d, "validate")
        && run_member(base, e, &v)? == json!(false)
    {
        return Err(BpaError::new(
            "MODEL.CONSTRAINT_VIOLATED",
            format!("validation of {} failed", s(d, "name").unwrap_or_default()),
        ));
    }
    Ok(v)
}

/// Evaluate a `normalize` or `validate` expression of a custom type over `value`.
fn run_member(base: &str, src: &str, v: &Value) -> Result<Value, BpaError> {
    let t = field_type(&json!({"type": base}), &BTreeMap::new());
    let mut env = Env::new(None, None, None, None);
    env.vars.insert("value".into(), t.clone());
    let ast = parse(&Value::String(src.to_string()))?;
    check(&ast, &mut env)?;
    let mut ev = Evaluator::new(&env);
    ev.vars.insert("value".into(), from_json(&t, v));
    to_json(&ev.eval(&ast)?)
}
