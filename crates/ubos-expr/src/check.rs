//! Static type checker of L1 expressions (STD-EXPR-002): the rules of the oracle
//! `expr.py::check`, `arith_type` and the `FUNCTIONS` signature table.

use crate::ast::{COLL_FN, Node, ROOTS};
use crate::lexer::terr;
use crate::value::{Ty, compat};
use std::collections::BTreeMap;
use ubos_model::types::BpaError;

/// The static type of the evaluation context `ctx` (STD-CTX-001).
pub fn ctx_type() -> Ty {
    let t = |s: &str| -> Ty {
        match s {
            "text" => Ty::Text,
            "instant" => Ty::Instant,
            "boolean" => Ty::Boolean,
            "list<text>" => Ty::List(Box::new(Ty::Text)),
            _ => Ty::Any,
        }
    };
    Ty::Obj(
        [
            ("principal", "text"),
            ("principal_kind", "text"),
            ("on_behalf_of", "list<text>"),
            ("auth_level", "text"),
            ("tenant", "text"),
            ("vae", "text"),
            ("branch", "text"),
            ("asof", "any"),
            ("time", "text"),
            ("now", "instant"),
            ("locale", "text"),
            ("time_zone", "text"),
            ("process_id", "text"),
            ("trigger", "any"),
            ("channel", "text"),
            ("role", "text"),
            ("input", "any"),
            ("profile", "text"),
            ("budget_remaining", "any"),
            ("replay", "boolean"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), t(v)))
        .collect(),
    )
}

/// The static environment of an expression: the types of the roots and of the lambda
/// variables, and the division target.
#[derive(Debug, Clone)]
pub struct Env {
    /// Type of `this`, when available.
    pub this: Option<Ty>,
    /// Type of `params`, when available.
    pub params: Option<Ty>,
    /// Type of `old`, when available.
    pub old: Option<Ty>,
    /// Type of `ctx`.
    pub ctx: Ty,
    /// Variables in scope (lambda variables, `value` of custom types).
    pub vars: BTreeMap<String, Ty>,
    /// `(scale, rounding)` for `/`, or `None`.
    pub target: Option<(u32, String)>,
}

impl Env {
    /// A new environment.
    pub fn new(
        this: Option<Ty>,
        params: Option<Ty>,
        old: Option<Ty>,
        target: Option<(u32, String)>,
    ) -> Env {
        Env {
            this,
            params,
            old,
            ctx: ctx_type(),
            vars: BTreeMap::new(),
            target,
        }
    }

    fn root(&self, name: &str) -> Option<&Ty> {
        match name {
            "this" => self.this.as_ref(),
            "params" => self.params.as_ref(),
            "old" => self.old.as_ref(),
            "ctx" => Some(&self.ctx),
            _ => None,
        }
    }
}

const NUM: &[Ty] = &[Ty::Integer, Ty::Decimal];
const NUMM: &[Ty] = &[Ty::Integer, Ty::Decimal, Ty::Money];
const DATEI: &[Ty] = &[Ty::Date, Ty::Instant];

fn is_ordered_or_dyn(t: &Ty) -> bool {
    t.is_ordered() || t.is_dyn()
}

fn one_of(t: &Ty, p: &[Ty]) -> bool {
    t.is_dyn() || p.contains(t)
}

/// Fixed-arity signature: each parameter is a set of accepted types.
fn sig(name: &str, ts: &[Ty], pos: usize, params: &[&[Ty]]) -> Result<(), BpaError> {
    if ts.len() != params.len() {
        return Err(terr(
            pos,
            &format!("{name} takes {} arguments", params.len()),
        ));
    }
    for (t, p) in ts.iter().zip(params) {
        if !one_of(t, p) {
            return Err(terr(
                pos,
                &format!("argument of {name} has type {}", t.short()),
            ));
        }
    }
    Ok(())
}

/// Variadic signature of one type.
fn var(name: &str, ts: &[Ty], pos: usize, min: usize, p: &Ty) -> Result<(), BpaError> {
    if ts.len() < min {
        return Err(terr(pos, &format!("{name} takes at least {min} arguments")));
    }
    for t in ts {
        if !(t.is_dyn() || t == p) {
            return Err(terr(
                pos,
                &format!("argument of {name} has type {}", t.short()),
            ));
        }
    }
    Ok(())
}

fn minmax(name: &str, ts: &[Ty], pos: usize) -> Result<Ty, BpaError> {
    if ts.len() != 2 {
        return Err(terr(
            pos,
            &format!("{name} takes two values, or a list and a lambda"),
        ));
    }
    match compat(&ts[0], &ts[1]) {
        Some(c) if is_ordered_or_dyn(&c) => Ok(c),
        _ => Err(terr(pos, &format!("{name} needs two comparable values"))),
    }
}

fn contains(ts: &[Ty], pos: usize) -> Result<Ty, BpaError> {
    if ts.len() != 2 {
        return Err(terr(pos, "contains takes two arguments"));
    }
    if ts[0] == Ty::Period {
        if !one_of(&ts[1], DATEI) {
            return Err(terr(pos, "contains(period, d) takes a date or an instant"));
        }
    } else if !one_of(&ts[0], &[Ty::Text]) || !one_of(&ts[1], &[Ty::Text]) {
        return Err(terr(pos, "contains takes two texts or a period and a time"));
    }
    Ok(Ty::Boolean)
}

fn list_of(name: &str, ts: &[Ty], pos: usize) -> Result<Ty, BpaError> {
    if ts.len() != 1 || !(ts[0].item().is_some() || ts[0].is_dyn()) {
        return Err(terr(pos, &format!("{name} takes a list")));
    }
    Ok(if ts[0].item().is_some() {
        ts[0].clone()
    } else {
        Ty::List(Box::new(Ty::Any))
    })
}

fn plain_coll(name: &str, ts: &[Ty], pos: usize) -> Result<Ty, BpaError> {
    if ts.len() == 2 && (name == "min" || name == "max") {
        return minmax(name, ts, pos);
    }
    if ts.len() != 1 || !(ts[0].item().is_some() || ts[0].is_dyn()) {
        return Err(terr(
            pos,
            &format!("{name} takes a list, or a list and a lambda"),
        ));
    }
    let item = ts[0].item().cloned().unwrap_or(Ty::Any);
    match name {
        "count" => Ok(Ty::Integer),
        "any" | "all" => {
            if !one_of(&item, &[Ty::Boolean]) {
                return Err(terr(pos, &format!("{name} of a list needs boolean items")));
            }
            Ok(Ty::Boolean)
        }
        "sum" => {
            if !one_of(&item, NUMM) && item != Ty::Duration {
                return Err(terr(pos, "sum needs numbers or money"));
            }
            Ok(item)
        }
        "min" | "max" => {
            if !is_ordered_or_dyn(&item) {
                return Err(terr(pos, &format!("{name} needs comparable items")));
            }
            Ok(item)
        }
        "first" => Ok(item),
        _ => Err(terr(pos, &format!("{name} needs a lambda"))),
    }
}

/// The result type of a call, per the ABI 1.0 signature table.
fn call_type(name: &str, ts: &[Ty], pos: usize) -> Result<Ty, BpaError> {
    const T: &[Ty] = &[Ty::Text];
    const I: &[Ty] = &[Ty::Integer];
    const M: &[Ty] = &[Ty::Money];
    const D: &[Ty] = &[Ty::Date];
    const INS: &[Ty] = &[Ty::Instant];
    const P: &[Ty] = &[Ty::Period];
    let fixed = |params: &[&[Ty]], result: Ty| -> Result<Ty, BpaError> {
        sig(name, ts, pos, params)?;
        Ok(result)
    };
    match name {
        "len" => fixed(&[T], Ty::Integer),
        "lower" | "upper" | "trim" => fixed(&[T], Ty::Text),
        "substr" => {
            if ts.len() == 2 {
                fixed(&[T, I], Ty::Text)
            } else {
                fixed(&[T, I, I], Ty::Text)
            }
        }
        "concat" => {
            var(name, ts, pos, 1, &Ty::Text)?;
            Ok(Ty::Text)
        }
        "contains" => contains(ts, pos),
        "starts_with" | "ends_with" => fixed(&[T, T], Ty::Boolean),
        "replace" => fixed(&[T, T, T], Ty::Text),
        "pad_left" => fixed(&[T, I, T], Ty::Text),
        "format" => {
            if ts.is_empty() || !matches!(ts[0], Ty::Text | Ty::Any) {
                return Err(terr(pos, "format takes a template"));
            }
            Ok(Ty::Text)
        }
        "abs" => {
            sig(name, ts, pos, &[NUMM])?;
            Ok(ts[0].clone())
        }
        "round" => {
            if ts.len() == 2 {
                sig(name, ts, pos, &[NUMM, I])?;
            } else {
                sig(name, ts, pos, &[NUMM, I, T])?;
            }
            Ok(if ts[0] == Ty::Money {
                Ty::Money
            } else {
                Ty::Decimal
            })
        }
        "floor" | "ceil" => fixed(&[NUM], Ty::Integer),
        "div" => {
            if ts.len() == 3 {
                fixed(&[NUM, NUM, I], Ty::Decimal)
            } else {
                fixed(&[NUM, NUM, I, T], Ty::Decimal)
            }
        }
        "clamp" => {
            if ts.len() != 3 {
                return Err(terr(pos, "clamp takes three values"));
            }
            match compat(&ts[0], &ts[1]) {
                Some(c) => minmax(name, &[c, ts[2].clone()], pos),
                None => minmax(name, &[ts[0].clone(), Ty::Boolean], pos),
            }
        }
        "amount" => fixed(&[M], Ty::Decimal),
        "currency" => fixed(&[M], Ty::Text),
        "money" => fixed(&[NUM, T], Ty::Money),
        "convert" => fixed(&[M, T, T], Ty::Money),
        // STD-FND-029 (BPA 1.0-rc9): conversion by rate key
        "fx_convert" => {
            if ts.len() == 3 {
                fixed(&[M, T, T], Ty::Money)
            } else {
                fixed(&[M, T, T, T], Ty::Money)
            }
        }
        "allocate" => {
            let ok = ts.len() == 2
                && one_of(&ts[0], M)
                && (ts[1].is_dyn() || ts[1].item().is_some_and(|i| one_of(i, NUM)));
            if !ok {
                return Err(terr(pos, "allocate takes money and a list of weights"));
            }
            Ok(Ty::List(Box::new(Ty::Money)))
        }
        "today" => fixed(&[], Ty::Date),
        "now" => fixed(&[], Ty::Instant),
        "date" => fixed(&[I, I, I], Ty::Date),
        "year" | "month" | "day" | "weekday" => fixed(&[D], Ty::Integer),
        "add_days" | "add_months" => fixed(&[D, I], Ty::Date),
        "days_between" => fixed(&[D, D], Ty::Integer),
        "start_of_month" | "end_of_month" => fixed(&[D], Ty::Date),
        "to_zone" => fixed(&[INS, T], Ty::Text),
        "is_business_day" => fixed(&[D, T], Ty::Boolean),
        "add_business_days" => fixed(&[D, I, T], Ty::Date),
        "next_business_day" | "previous_business_day" => fixed(&[D, T], Ty::Date),
        "cutoff" => fixed(&[D, T, T, T], Ty::Instant),
        "overlaps" => fixed(&[P, P], Ty::Boolean),
        "period" => fixed(&[DATEI, DATEI], Ty::Period),
        "distinct" => list_of(name, ts, pos),
        "ref" => fixed(&[T], Ty::Any),
        "exists" | "luhn_valid" | "iso7064_mod97_10" | "isin_valid" | "lei_valid"
        | "iban_valid" => fixed(&[T], Ty::Boolean),
        "any" | "all" | "count" | "sum" | "min" | "max" | "first" => plain_coll(name, ts, pos),
        _ => Err(terr(pos, &format!("unknown function {name}"))),
    }
}

/// The result type of an arithmetic operator.
pub fn arith_type(op: char, a: &Ty, b: &Ty, pos: usize, env: &Env) -> Result<Ty, BpaError> {
    if *a == Ty::Any || *b == Ty::Any {
        return Ok(Ty::Any);
    }
    if *a == Ty::Null {
        return Ok(b.clone());
    }
    if *b == Ty::Null {
        return Ok(a.clone());
    }
    let both_num = a.is_num() && b.is_num();
    let num_result = || {
        if *a == Ty::Integer && *b == Ty::Integer {
            Ty::Integer
        } else {
            Ty::Decimal
        }
    };
    match op {
        '+' | '-' => {
            if both_num {
                return Ok(num_result());
            }
            if *a == Ty::Money && *b == Ty::Money {
                return Ok(Ty::Money);
            }
            if op == '-' && *a == Ty::Date && *b == Ty::Date {
                return Ok(Ty::Integer);
            }
            if op == '-' && *a == Ty::Instant && *b == Ty::Instant {
                return Ok(Ty::Duration);
            }
            if *a == Ty::Duration && *b == Ty::Duration {
                return Ok(Ty::Duration);
            }
        }
        '*' => {
            if both_num {
                return Ok(num_result());
            }
            if (*a == Ty::Money && b.is_num()) || (*b == Ty::Money && a.is_num()) {
                return Ok(Ty::Money);
            }
        }
        '/' => {
            if both_num {
                if env.target.is_none() {
                    return Err(terr(
                        pos,
                        "division needs the scale of a target field; use div(a, b, scale, rounding)",
                    ));
                }
                return Ok(Ty::Decimal);
            }
        }
        '%' => {
            if *a == Ty::Integer && *b == Ty::Integer {
                return Ok(Ty::Integer);
            }
        }
        _ => {}
    }
    Err(terr(
        pos,
        &format!(
            "operator {op} does not apply to {} and {}",
            a.short(),
            b.short()
        ),
    ))
}

/// The static type of `node` in `env` (STD-EXPR-002).
pub fn check(node: &Node, env: &mut Env) -> Result<Ty, BpaError> {
    match node {
        Node::Lit(_, _, t) => Ok(t.clone()),
        Node::List(_, items) => {
            let mut t = Ty::Null;
            for it in items {
                let x = check(it, env)?;
                t = compat(&t, &x).unwrap_or(Ty::Any);
            }
            Ok(Ty::List(Box::new(t)))
        }
        Node::Path(pos, name) => {
            if let Some(t) = env.vars.get(name) {
                return Ok(t.clone());
            }
            if ROOTS.contains(&name.as_str()) {
                return env
                    .root(name)
                    .cloned()
                    .ok_or_else(|| terr(*pos, &format!("{name} is not available here")));
            }
            Err(terr(*pos, &format!("unknown name {name}")))
        }
        Node::Member(pos, obj, name, _) => {
            let t = check(obj, env)?;
            if t.is_dyn() {
                return Ok(Ty::Any);
            }
            if let Ty::Obj(members) = &t {
                return members
                    .get(name)
                    .cloned()
                    .ok_or_else(|| terr(*pos, &format!("unknown member {name}")));
            }
            if t == Ty::Money && (name == "amount" || name == "currency") {
                return Ok(if name == "amount" {
                    Ty::Decimal
                } else {
                    Ty::Text
                });
            }
            if t == Ty::Period && (name == "from" || name == "to") {
                return Ok(Ty::Any);
            }
            Err(terr(*pos, &format!("no member {name} on {}", t.short())))
        }
        Node::Index(pos, obj, idx) => {
            let t = check(obj, env)?;
            let i = check(idx, env)?;
            if let Some(item) = t.item() {
                if !one_of(&i, &[Ty::Integer]) {
                    return Err(terr(idx.pos(), "a list index is an integer"));
                }
                return Ok(item.clone());
            }
            if t.is_dyn() || matches!(t, Ty::Obj(_)) {
                if !one_of(&i, &[Ty::Text, Ty::Integer]) {
                    return Err(terr(idx.pos(), "an index is text or an integer"));
                }
                return Ok(match (&t, &**idx) {
                    (Ty::Obj(members), Node::Lit(_, crate::value::Val::Text(k), _)) => {
                        members.get(k).cloned().unwrap_or(Ty::Any)
                    }
                    _ => Ty::Any,
                });
            }
            Err(terr(*pos, &format!("cannot index {}", t.short())))
        }
        Node::Neg(pos, e) => {
            let t = check(e, env)?;
            if !one_of(&t, &[Ty::Integer, Ty::Decimal, Ty::Money, Ty::Duration]) {
                return Err(terr(*pos, &format!("cannot negate {}", t.short())));
            }
            Ok(t)
        }
        Node::Arith(pos, op, a, b) => {
            let ta = check(a, env)?;
            let tb = check(b, env)?;
            arith_type(*op, &ta, &tb, *pos, env)
        }
        Node::Cmp(pos, op, a, b) => {
            let ta = check(a, env)?;
            let tb = check(b, env)?;
            let c = compat(&ta, &tb).ok_or_else(|| {
                terr(
                    *pos,
                    &format!("cannot compare {} with {}", ta.short(), tb.short()),
                )
            })?;
            if !matches!(*op, "==" | "!=") && !is_ordered_or_dyn(&c) {
                return Err(terr(*pos, &format!("{} values are not ordered", c.short())));
            }
            Ok(Ty::Boolean)
        }
        Node::In(pos, a, lst) => {
            let ta = check(a, env)?;
            let lt = check(lst, env)?;
            if !lt.is_dyn() && lt.item().is_none() {
                return Err(terr(lst.pos(), "in needs a list"));
            }
            if let Some(item) = lt.item()
                && compat(&ta, item).is_none()
            {
                return Err(terr(*pos, "value and list items are not comparable"));
            }
            Ok(Ty::Boolean)
        }
        Node::IsNull(_, e, _) => {
            check(e, env)?;
            Ok(Ty::Boolean)
        }
        Node::Matches(pos, e, _) => {
            if !one_of(&check(e, env)?, &[Ty::Text]) {
                return Err(terr(*pos, "matches needs text"));
            }
            Ok(Ty::Boolean)
        }
        Node::And(_, a, b) | Node::Or(_, a, b) => {
            let k = if matches!(node, Node::And(..)) {
                "and"
            } else {
                "or"
            };
            for x in [a, b] {
                if !one_of(&check(x, env)?, &[Ty::Boolean]) {
                    return Err(terr(x.pos(), &format!("{k} needs boolean operands")));
                }
            }
            Ok(Ty::Boolean)
        }
        Node::Not(pos, e) => {
            if !one_of(&check(e, env)?, &[Ty::Boolean]) {
                return Err(terr(*pos, "not needs a boolean operand"));
            }
            Ok(Ty::Boolean)
        }
        Node::Cond(pos, c, a, b) => {
            if !one_of(&check(c, env)?, &[Ty::Boolean]) {
                return Err(terr(c.pos(), "a condition is boolean"));
            }
            let ta = check(a, env)?;
            let tb = check(b, env)?;
            compat(&ta, &tb).ok_or_else(|| terr(*pos, "branches of different types"))
        }
        Node::Call(pos, name, args) => {
            let ts = args
                .iter()
                .map(|a| check(a, env))
                .collect::<Result<Vec<_>, _>>()?;
            call_type(name, &ts, *pos)
        }
        Node::Lambda(pos, name, lst, var, body) => {
            let lt = check(lst, env)?;
            if !lt.is_dyn() && lt.item().is_none() {
                return Err(terr(lst.pos(), &format!("{name} needs a list")));
            }
            if ROOTS.contains(&var.as_str()) || env.vars.contains_key(var) {
                return Err(terr(
                    *pos,
                    &format!("the lambda variable {var} hides another name"),
                ));
            }
            let item = lt.item().cloned().unwrap_or(Ty::Any);
            env.vars.insert(var.clone(), item.clone());
            let bt = check(body, env);
            env.vars.remove(var);
            let bt = bt?;
            debug_assert!(COLL_FN.contains(&name.as_str()));
            match name.as_str() {
                "any" | "all" | "count" | "filter" | "first" => {
                    if !one_of(&bt, &[Ty::Boolean]) {
                        return Err(terr(body.pos(), &format!("{name} needs a boolean lambda")));
                    }
                    Ok(match name.as_str() {
                        "any" | "all" => Ty::Boolean,
                        "count" => Ty::Integer,
                        "filter" => lt,
                        _ => item,
                    })
                }
                "sum" => {
                    if !one_of(&bt, NUMM) && bt != Ty::Duration {
                        return Err(terr(body.pos(), "sum needs numbers or money"));
                    }
                    Ok(bt)
                }
                "min" | "max" | "sort_by" => {
                    if !is_ordered_or_dyn(&bt) {
                        return Err(terr(body.pos(), &format!("{name} needs comparable keys")));
                    }
                    Ok(if name == "sort_by" {
                        if lt.item().is_some() {
                            lt
                        } else {
                            Ty::List(Box::new(Ty::Any))
                        }
                    } else {
                        bt
                    })
                }
                _ => Ok(Ty::List(Box::new(bt))),
            }
        }
    }
}
