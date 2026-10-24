//! Evaluator of L1 expressions (STD-EXPR-003 null semantics, STD-EXPR-004 totality and
//! limits, STD-EXPR-005 ABI 1.0 function library): a port of the oracle
//! `expr.py::Evaluator`.

use crate::ast::Node;
use crate::check::Env;
use crate::lexer::terr;
use crate::value::{Ty, Val, from_json, to_json};
use chrono::{DateTime, Datelike, Duration as CDuration, NaiveDate, Offset, Utc};
use chrono_tz::Tz;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use ubos_model::types::BpaError;
use ubos_model::types::decimal::{Decimal, Rounding};
use ubos_model::types::time::{local_to_instant, parse_instant};
use ubos_model::types::values::{is_reference, minor_unit, validate_value};

/// Operation budget of the strict profile (STD-CTX-004).
pub const MAX_UNITS: u64 = 1_000_000;
/// Maximum number of items of a list.
pub const MAX_ITEMS: usize = 65_536;
/// `ref()` and `exists()` are charged as `read.get` (STD-ISA-030).
pub const REF_COST: u64 = 10;

/// The rate a conversion applies (STD-FND-029): multiply by a rate, or divide by the rate
/// of the opposite key.
#[derive(Debug, Clone)]
pub enum FxRate {
    /// The rate of the key.
    Mul(Decimal),
    /// The rate of the opposite key, applied as its inverse.
    Div(Decimal),
}

/// Resolves a canonical object URI to a typed run-time value.
pub trait Resolver {
    /// The object at `uri`, or `None`.
    fn resolve(&self, uri: &str) -> Result<Option<Val>, BpaError>;
    /// The rate of `(source, base, quote, rate_type)` at the evaluation date (STD-FND-029,
    /// BPA 1.0-rc9); `None` when the evaluation has no FX rates.
    fn fx_rate(
        &self,
        _base: &str,
        _quote: &str,
        _rate_type: &str,
        _source: Option<&str>,
    ) -> Result<Option<FxRate>, BpaError> {
        Ok(None)
    }
    /// The `FxRate` object a rate reference names, as `{from, to, rate}` (STD-FND-029).
    fn rate_object(&self, _rate_ref: &str) -> Result<Option<Value>, BpaError> {
        Ok(None)
    }
}

/// STD-FND-029 (BPA 1.0-rc9): the rate of a key in a list of `{source, base, quote,
/// rate_type, rate}` (the `fx_rates` of `eval_expr`), with the rules of `FxRate` objects.
pub fn fx_in_list(
    list: &[Value],
    base: &str,
    quote: &str,
    rate_type: &str,
    source: Option<&str>,
) -> Result<FxRate, BpaError> {
    let mut cands: Vec<(Option<&str>, bool, &Value)> = Vec::new();
    for r in list {
        let g = |k: &str| r.get(k).and_then(Value::as_str);
        if g("rate_type") != Some(rate_type) || source.is_some_and(|x| g("source") != Some(x)) {
            continue;
        }
        if (g("base"), g("quote")) == (Some(base), Some(quote)) {
            cands.push((g("source"), false, r));
        } else if (g("base"), g("quote")) == (Some(quote), Some(base)) {
            cands.push((g("source"), true, r));
        }
    }
    pick_rate(&cands, base, quote, rate_type)
}

/// The rate among candidates `(source, inverse, {rate})`: one source, the direct key first.
pub fn pick_rate(
    cands: &[(Option<&str>, bool, &Value)],
    base: &str,
    quote: &str,
    rate_type: &str,
) -> Result<FxRate, BpaError> {
    if cands.is_empty() {
        return Err(BpaError::with_details(
            "MODEL.CURRENCY_MISMATCH",
            serde_json::json!({"base": base, "quote": quote, "rate_type": rate_type}),
        ));
    }
    let first = cands[0].0;
    if cands.iter().any(|c| c.0 != first) {
        return Err(invalid("several sources have the rate; name one"));
    }
    let (_, inverse, r) = cands.iter().find(|c| !c.1).unwrap_or(&cands[0]);
    let rate = Decimal::parse(r.get("rate").and_then(Value::as_str).unwrap_or("0"))?;
    Ok(if *inverse {
        FxRate::Div(rate)
    } else {
        FxRate::Mul(rate)
    })
}

/// Ordering key of a value: the category (variant order) then the value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderKey {
    /// Numbers.
    Num(Decimal),
    /// Money, by currency then amount.
    Money(String, Decimal),
    /// Text and times.
    Text(String),
    /// Instants.
    Instant(DateTime<Utc>),
    /// Dates.
    Date(NaiveDate),
    /// Durations.
    Duration(i128),
}

/// The ordering key of a value, or `None` for values that are not ordered.
pub fn order_key(v: &Val) -> Option<OrderKey> {
    Some(match v {
        Val::Int(i) => OrderKey::Num(dec_of_int(*i)),
        Val::Dec(d) => OrderKey::Num(d.clone()),
        Val::Money(a, c) => OrderKey::Money(c.clone(), a.clone()),
        Val::Text(s) => OrderKey::Text(s.clone()),
        Val::Instant(t) => OrderKey::Instant(*t),
        Val::Date(d) => OrderKey::Date(*d),
        Val::Duration(us) => OrderKey::Duration(*us),
        _ => return None,
    })
}

type Key = OrderKey;

impl Key {
    fn cat(&self) -> u8 {
        match self {
            Key::Num(_) => 0,
            Key::Money(..) => 1,
            Key::Text(_) => 2,
            Key::Instant(_) => 3,
            Key::Date(_) => 4,
            Key::Duration(_) => 5,
        }
    }
}

fn dec_of_int(i: i128) -> Decimal {
    Decimal::parse(&i.to_string()).expect("integer text")
}

fn dec_to_int(d: &Decimal) -> Result<i128, BpaError> {
    d.format(None)
        .parse::<i128>()
        .map_err(|_| BpaError::new("MODEL.INVALID_VALUE", "integer out of range"))
}

fn invalid(msg: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", msg)
}

/// The evaluator: roots, resources, limits and the budget.
pub struct Evaluator<'a> {
    env: &'a Env,
    /// The evaluation context (`ctx`), with `now` as an instant.
    pub ctx: Val,
    /// Object bodies by URI (used when no resolver is installed).
    pub objects: Map<String, Value>,
    /// Business calendars by name (`{weekend, holidays}`).
    pub calendars: Map<String, Value>,
    /// Exchange rates by reference (`{from, to, rate}`).
    pub rates: Map<String, Value>,
    /// FX rates by key (`eval_expr` input `fx_rates`, STD-FND-029).
    pub fx_rates: Option<Vec<Value>>,
    /// Operation budget.
    pub max_units: u64,
    /// Maximum number of list items.
    pub max_items: usize,
    /// Business time zone of `today()`.
    pub tz: String,
    /// `this`.
    pub this: Option<Val>,
    /// `params`.
    pub params: Option<Val>,
    /// `old`.
    pub old: Option<Val>,
    /// The reference resolver.
    pub resolver: Option<&'a dyn Resolver>,
    /// Variables in scope.
    pub vars: BTreeMap<String, Val>,
    units: u64,
}

impl<'a> Evaluator<'a> {
    /// A new evaluator with the default limits.
    pub fn new(env: &'a Env) -> Evaluator<'a> {
        Evaluator {
            env,
            ctx: Val::Map(BTreeMap::new()),
            objects: Map::new(),
            calendars: Map::new(),
            rates: Map::new(),
            fx_rates: None,
            max_units: MAX_UNITS,
            max_items: MAX_ITEMS,
            tz: "UTC".into(),
            this: None,
            params: None,
            old: None,
            resolver: None,
            vars: BTreeMap::new(),
            units: 0,
        }
    }

    /// Units charged so far.
    pub fn units(&self) -> u64 {
        self.units
    }

    fn charge(&mut self, n: u64) -> Result<(), BpaError> {
        self.units = self.units.saturating_add(n);
        if self.units > self.max_units {
            return Err(BpaError::new(
                "LOGIC.LIMIT_EXCEEDED",
                "operation budget exhausted",
            ));
        }
        Ok(())
    }

    fn items(&self, n: usize) -> Result<(), BpaError> {
        if n > self.max_items {
            return Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "too many items"));
        }
        Ok(())
    }

    fn root(&self, name: &str) -> Val {
        match name {
            "this" => self.this.clone(),
            "params" => self.params.clone(),
            "old" => self.old.clone(),
            "ctx" => Some(self.ctx.clone()),
            _ => None,
        }
        .unwrap_or(Val::Null)
    }

    /// Evaluate `node`.
    pub fn eval(&mut self, node: &Node) -> Result<Val, BpaError> {
        self.charge(1)?;
        match node {
            Node::Lit(_, v, _) => Ok(v.clone()),
            Node::List(_, items) => {
                let out = items
                    .iter()
                    .map(|x| self.eval(x))
                    .collect::<Result<Vec<_>, _>>()?;
                self.items(out.len())?;
                Ok(Val::List(out))
            }
            Node::Path(_, name) => Ok(match self.vars.get(name) {
                Some(v) => v.clone(),
                None => self.root(name),
            }),
            Node::Member(pos, obj, name, safe) => {
                let r = self.eval(obj)?;
                match r {
                    Val::Null => {
                        if *safe {
                            Ok(Val::Null)
                        } else {
                            Err(terr(*pos, &format!("member {name} of null; use ?.")))
                        }
                    }
                    Val::Map(m) => Ok(m.get(name).cloned().unwrap_or(Val::Null)),
                    Val::Money(a, c) => Ok(if name == "amount" {
                        Val::Dec(a)
                    } else {
                        Val::Text(c)
                    }),
                    Val::Period(lo, hi) => Ok(if name == "from" {
                        *lo
                    } else {
                        hi.map(|h| *h).unwrap_or(Val::Null)
                    }),
                    _ => Err(terr(*pos, &format!("no member {name}"))),
                }
            }
            Node::Index(pos, obj, idx) => {
                let r = self.eval(obj)?;
                let i = self.eval(idx)?;
                match (r, i) {
                    (Val::Null, _) | (_, Val::Null) => Ok(Val::Null),
                    (Val::List(items), Val::Int(i)) => {
                        Ok(if i >= 0 && (i as usize) < items.len() {
                            items[i as usize].clone()
                        } else {
                            Val::Null
                        })
                    }
                    (Val::Map(m), Val::Text(k)) => Ok(m.get(&k).cloned().unwrap_or(Val::Null)),
                    _ => Err(terr(*pos, "invalid index")),
                }
            }
            Node::Neg(pos, e) => match self.eval(e)? {
                Val::Null => Ok(Val::Null),
                Val::Money(a, c) => Ok(Val::Money(a.neg(), c)),
                Val::Duration(us) => Ok(Val::Duration(-us)),
                Val::Int(i) => Ok(Val::Int(-i)),
                Val::Dec(d) => Ok(Val::Dec(d.neg())),
                _ => Err(terr(*pos, "cannot negate")),
            },
            Node::Arith(pos, op, a, b) => {
                let x = self.eval(a)?;
                let y = self.eval(b)?;
                self.arith(*op, &x, &y, *pos)
            }
            Node::Cmp(pos, op, a, b) => {
                let x = self.eval(a)?;
                let y = self.eval(b)?;
                Ok(opt_bool(self.compare(op, &x, &y, *pos)?))
            }
            Node::In(pos, a, lst) => {
                let x = self.eval(a)?;
                let l = self.eval(lst)?;
                if x.is_null() || l.is_null() {
                    return Ok(Val::Null);
                }
                let Val::List(items) = l else {
                    return Err(terr(*pos, "in needs a list"));
                };
                self.charge(items.len() as u64)?;
                let mut unknown = false;
                for y in &items {
                    match self.compare("==", &x, y, *pos)? {
                        Some(true) => return Ok(Val::Bool(true)),
                        Some(false) => {}
                        None => unknown = true,
                    }
                }
                Ok(if unknown { Val::Null } else { Val::Bool(false) })
            }
            Node::IsNull(_, e, neg) => {
                let v = self.eval(e)?.is_null();
                Ok(Val::Bool(if *neg { !v } else { v }))
            }
            Node::Matches(pos, e, pat) => match self.eval(e)? {
                Val::Null => Ok(Val::Null),
                Val::Text(s) => {
                    let r = regex::Regex::new(&format!("^(?:{pat})$"))
                        .map_err(|_| terr(*pos, "invalid pattern"))?;
                    Ok(Val::Bool(r.is_match(&s)))
                }
                _ => Err(terr(*pos, "matches needs text")),
            },
            Node::And(pos, a, b) => {
                let x = self.eval(a)?;
                let x = self.boolean(&x, *pos)?;
                if x == Some(false) {
                    return Ok(Val::Bool(false));
                }
                let y = self.eval(b)?;
                let y = self.boolean(&y, *pos)?;
                Ok(match (x, y) {
                    (_, Some(false)) => Val::Bool(false),
                    (Some(true), Some(true)) => Val::Bool(true),
                    _ => Val::Null,
                })
            }
            Node::Or(pos, a, b) => {
                let x = self.eval(a)?;
                let x = self.boolean(&x, *pos)?;
                if x == Some(true) {
                    return Ok(Val::Bool(true));
                }
                let y = self.eval(b)?;
                let y = self.boolean(&y, *pos)?;
                Ok(match (x, y) {
                    (_, Some(true)) => Val::Bool(true),
                    (Some(false), Some(false)) => Val::Bool(false),
                    _ => Val::Null,
                })
            }
            Node::Not(pos, e) => {
                let x = self.eval(e)?;
                Ok(opt_bool(self.boolean(&x, *pos)?.map(|b| !b)))
            }
            Node::Cond(pos, c, a, b) => {
                let x = self.eval(c)?;
                if self.boolean(&x, *pos)? == Some(true) {
                    self.eval(a)
                } else {
                    self.eval(b)
                }
            }
            Node::Call(pos, name, args) => {
                let vals = args
                    .iter()
                    .map(|a| self.eval(a))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(name, vals, *pos)
            }
            Node::Lambda(pos, name, lst, var, body) => self.lam(name, lst, var, body, *pos),
        }
    }

    fn boolean(&self, v: &Val, pos: usize) -> Result<Option<bool>, BpaError> {
        match v {
            Val::Null => Ok(None),
            Val::Bool(b) => Ok(Some(*b)),
            _ => Err(terr(pos, "a boolean is needed")),
        }
    }

    fn arith(&self, op: char, a: &Val, b: &Val, pos: usize) -> Result<Val, BpaError> {
        if a.is_null() || b.is_null() {
            return Ok(Val::Null);
        }
        if a.is_num() && b.is_num() {
            if let (Val::Int(x), Val::Int(y)) = (a, b) {
                let r = match op {
                    '+' => x.checked_add(*y),
                    '-' => x.checked_sub(*y),
                    '*' => x.checked_mul(*y),
                    '%' => {
                        if *y == 0 {
                            return Err(BpaError::new(
                                "LOGIC.DIVISION_BY_ZERO",
                                "division by zero",
                            ));
                        }
                        let r = x.abs() % y.abs();
                        Some(if *x < 0 { -r } else { r })
                    }
                    _ => None,
                };
                if let Some(r) = r {
                    return Ok(Val::Int(r));
                }
            }
            let x = a.as_decimal().expect("number");
            let y = b.as_decimal().expect("number");
            match op {
                '+' => return Ok(Val::Dec(x.add(&y))),
                '-' => return Ok(Val::Dec(x.sub(&y))),
                '*' => return Ok(Val::Dec(x.mul(&y))),
                '/' => {
                    if y.is_zero() {
                        return Err(BpaError::new("LOGIC.DIVISION_BY_ZERO", "division by zero"));
                    }
                    let Some((scale, rounding)) = &self.env.target else {
                        return Err(terr(pos, "division needs a scale"));
                    };
                    return Ok(Val::Dec(x.div(&y, *scale, Rounding::parse(rounding)?)?));
                }
                _ => {}
            }
        }
        match (op, a, b) {
            ('+' | '-', Val::Money(x, cx), Val::Money(y, cy)) => {
                if cx != cy {
                    return Err(BpaError::new(
                        "MODEL.CURRENCY_MISMATCH",
                        "currencies differ",
                    ));
                }
                Ok(Val::Money(
                    if op == '+' { x.add(y) } else { x.sub(y) },
                    cx.clone(),
                ))
            }
            ('*', Val::Money(x, c), n) | ('*', n, Val::Money(x, c)) if n.is_num() => Ok(
                Val::Money(x.mul(&n.as_decimal().expect("number")), c.clone()),
            ),
            ('-', Val::Date(x), Val::Date(y)) => Ok(Val::Int(i128::from((*x - *y).num_days()))),
            ('-', Val::Instant(x), Val::Instant(y)) => Ok(Val::Duration(i128::from(
                (*x - *y).num_microseconds().unwrap_or_default(),
            ))),
            ('+' | '-', Val::Duration(x), Val::Duration(y)) => {
                Ok(Val::Duration(if op == '+' { x + y } else { x - y }))
            }
            _ => Err(terr(
                pos,
                &format!("operator {op} does not apply to these values"),
            )),
        }
    }

    fn order_key(&self, v: &Val, pos: usize) -> Result<Key, BpaError> {
        order_key(v).ok_or_else(|| terr(pos, "values are not ordered"))
    }

    /// Deep equality with numeric cross-type comparison.
    fn eq(a: &Val, b: &Val) -> bool {
        match (a, b) {
            (Val::Int(_) | Val::Dec(_), Val::Int(_) | Val::Dec(_)) => {
                a.as_decimal() == b.as_decimal()
            }
            (Val::List(x), Val::List(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(p, q)| Self::eq(p, q))
            }
            (Val::Map(x), Val::Map(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .all(|(k, p)| y.get(k).is_some_and(|q| Self::eq(p, q)))
            }
            (Val::Period(l1, h1), Val::Period(l2, h2)) => {
                Self::eq(l1, l2)
                    && match (h1, h2) {
                        (None, None) => true,
                        (Some(p), Some(q)) => Self::eq(p, q),
                        _ => false,
                    }
            }
            _ => a == b,
        }
    }

    fn compare(&self, op: &str, a: &Val, b: &Val, pos: usize) -> Result<Option<bool>, BpaError> {
        if a.is_null() || b.is_null() {
            return Ok(None);
        }
        if let (Val::Money(_, cx), Val::Money(_, cy)) = (a, b)
            && cx != cy
            && !matches!(op, "==" | "!=")
        {
            return Err(BpaError::new(
                "MODEL.CURRENCY_MISMATCH",
                "currencies differ",
            ));
        }
        if matches!(op, "==" | "!=") {
            let eq = Self::eq(a, b);
            return Ok(Some(if op == "==" { eq } else { !eq }));
        }
        let ka = self.order_key(a, pos)?;
        let kb = self.order_key(b, pos)?;
        if ka.cat() != kb.cat() {
            return Err(terr(pos, "values of different types"));
        }
        let o = ka.cmp(&kb);
        Ok(Some(match op {
            "<" => o == Ordering::Less,
            "<=" => o != Ordering::Greater,
            ">" => o == Ordering::Greater,
            _ => o != Ordering::Less,
        }))
    }

    // -- collections --

    fn lam(
        &mut self,
        name: &str,
        lst: &Node,
        var: &str,
        body: &Node,
        pos: usize,
    ) -> Result<Val, BpaError> {
        let items = match self.eval(lst)? {
            Val::Null => return Ok(Val::Null),
            Val::List(items) => items,
            _ => return Err(terr(pos, &format!("{name} needs a list"))),
        };
        self.items(items.len())?;
        let mut out: Vec<(Val, Val)> = Vec::with_capacity(items.len());
        for x in items {
            self.charge(1)?;
            self.vars.insert(var.to_string(), x.clone());
            let v = self.eval(body);
            self.vars.remove(var);
            let v = v?;
            let t = v == Val::Bool(true);
            if name == "first" && t {
                return Ok(x);
            }
            if name == "any" && t {
                return Ok(Val::Bool(true));
            }
            if name == "all" && v == Val::Bool(false) {
                return Ok(Val::Bool(false));
            }
            out.push((x, v));
        }
        let has_null = out.iter().any(|(_, v)| v.is_null());
        match name {
            "any" => Ok(if has_null {
                Val::Null
            } else {
                Val::Bool(false)
            }),
            "all" => Ok(if has_null { Val::Null } else { Val::Bool(true) }),
            "count" => Ok(Val::Int(
                out.iter().filter(|(_, v)| *v == Val::Bool(true)).count() as i128,
            )),
            "filter" => Ok(Val::List(
                out.into_iter()
                    .filter(|(_, v)| *v == Val::Bool(true))
                    .map(|(x, _)| x)
                    .collect(),
            )),
            "map" => {
                let vals: Vec<Val> = out.into_iter().map(|(_, v)| v).collect();
                self.items(vals.len())?;
                Ok(Val::List(vals))
            }
            "first" => Ok(Val::Null),
            "sum" => {
                let vals: Vec<Val> = out
                    .into_iter()
                    .map(|(_, v)| v)
                    .filter(|v| !v.is_null())
                    .collect();
                self.total(&vals, pos)
            }
            "min" | "max" => {
                let mut best: Option<(Key, Val)> = None;
                for (_, v) in out.into_iter().filter(|(_, v)| !v.is_null()) {
                    let k = self.order_key(&v, pos)?;
                    let better = match &best {
                        None => true,
                        Some((bk, _)) => {
                            if name == "min" {
                                k < *bk
                            } else {
                                k > *bk
                            }
                        }
                    };
                    if better {
                        best = Some((k, v));
                    }
                }
                Ok(best.map(|(_, v)| v).unwrap_or(Val::Null))
            }
            "sort_by" => {
                let mut nulls = Vec::new();
                let mut rest: Vec<(Key, Val)> = Vec::new();
                for (x, v) in out {
                    if v.is_null() {
                        nulls.push(x);
                    } else {
                        rest.push((self.order_key(&v, pos)?, x));
                    }
                }
                rest.sort_by(|a, b| a.0.cmp(&b.0));
                nulls.extend(rest.into_iter().map(|(_, x)| x));
                Ok(Val::List(nulls))
            }
            _ => Err(terr(pos, "unknown collection function")),
        }
    }

    fn total(&self, vals: &[Val], pos: usize) -> Result<Val, BpaError> {
        let Some(first) = vals.first() else {
            return Ok(Val::Null);
        };
        let mut acc = first.clone();
        for v in &vals[1..] {
            acc = self.arith('+', &acc, v, pos)?;
        }
        Ok(acc)
    }

    // -- functions --

    fn call(&mut self, name: &str, a: Vec<Val>, pos: usize) -> Result<Val, BpaError> {
        let n = a.len();
        if matches!(
            name,
            "any" | "all" | "count" | "sum" | "min" | "max" | "first" | "distinct"
        ) && n == 1
        {
            let lst = match a.into_iter().next() {
                Some(Val::Null) => return Ok(Val::Null),
                Some(Val::List(l)) => l,
                _ => return Err(terr(pos, &format!("{name} needs a list"))),
            };
            self.items(lst.len())?;
            self.charge(lst.len() as u64)?;
            return match name {
                "count" => Ok(Val::Int(lst.len() as i128)),
                "first" => Ok(lst.into_iter().next().unwrap_or(Val::Null)),
                "distinct" => {
                    let mut out: Vec<Val> = Vec::new();
                    for x in lst {
                        let mut dup = false;
                        for y in &out {
                            if (x.is_null() && y.is_null())
                                || self.compare("==", &x, y, pos)? == Some(true)
                            {
                                dup = true;
                                break;
                            }
                        }
                        if !dup {
                            out.push(x);
                        }
                    }
                    Ok(Val::List(out))
                }
                "any" | "all" => {
                    let mut vals = Vec::with_capacity(lst.len());
                    for x in &lst {
                        vals.push(self.boolean(x, pos)?);
                    }
                    let has_null = vals.iter().any(Option::is_none);
                    Ok(if name == "any" {
                        if vals.contains(&Some(true)) {
                            Val::Bool(true)
                        } else if has_null {
                            Val::Null
                        } else {
                            Val::Bool(false)
                        }
                    } else if vals.contains(&Some(false)) {
                        Val::Bool(false)
                    } else if has_null {
                        Val::Null
                    } else {
                        Val::Bool(true)
                    })
                }
                _ => {
                    let vals: Vec<Val> = lst.into_iter().filter(|v| !v.is_null()).collect();
                    if name == "sum" {
                        return self.total(&vals, pos);
                    }
                    let mut best: Option<(Key, Val)> = None;
                    for v in vals {
                        let k = self.order_key(&v, pos)?;
                        let better = match &best {
                            None => true,
                            Some((bk, _)) => {
                                if name == "min" {
                                    k < *bk
                                } else {
                                    k > *bk
                                }
                            }
                        };
                        if better {
                            best = Some((k, v));
                        }
                    }
                    Ok(best.map(|(_, v)| v).unwrap_or(Val::Null))
                }
            };
        }
        let nullable = !matches!(
            name,
            "today" | "now" | "exists" | "concat" | "format" | "period"
        );
        if nullable && a.iter().any(Val::is_null) {
            return Ok(Val::Null);
        }
        if matches!(name, "min" | "max") {
            let ka = self.order_key(&a[0], pos)?;
            let kb = self.order_key(&a[1], pos)?;
            if ka.cat() != kb.cat() {
                return Err(terr(pos, "values of different types"));
            }
            if let (Val::Money(_, cx), Val::Money(_, cy)) = (&a[0], &a[1])
                && cx != cy
            {
                return Err(BpaError::new(
                    "MODEL.CURRENCY_MISMATCH",
                    "currencies differ",
                ));
            }
            let mut it = a.into_iter();
            let (x, y) = (it.next().expect("two"), it.next().expect("two"));
            return Ok(if (ka <= kb) == (name == "min") { x } else { y });
        }
        self.func(name, &a, pos)
    }

    fn func(&mut self, name: &str, a: &[Val], pos: usize) -> Result<Val, BpaError> {
        let bad = || terr(pos, &format!("argument of {name} has the wrong type"));
        let text = |i: usize| -> Result<&str, BpaError> {
            match a.get(i) {
                Some(Val::Text(s)) => Ok(s),
                _ => Err(bad()),
            }
        };
        let int = |i: usize| -> Result<i128, BpaError> {
            match a.get(i) {
                Some(Val::Int(n)) => Ok(*n),
                _ => Err(bad()),
            }
        };
        let dec = |i: usize| -> Result<Decimal, BpaError> {
            a.get(i).and_then(Val::as_decimal).ok_or_else(bad)
        };
        let date = |i: usize| -> Result<NaiveDate, BpaError> {
            match a.get(i) {
                Some(Val::Date(d)) => Ok(*d),
                _ => Err(bad()),
            }
        };
        let money = |i: usize| -> Result<(Decimal, String), BpaError> {
            match a.get(i) {
                Some(Val::Money(x, c)) => Ok((x.clone(), c.clone())),
                _ => Err(bad()),
            }
        };
        let period = |i: usize| -> Result<(&Val, Option<&Val>), BpaError> {
            match a.get(i) {
                Some(Val::Period(lo, hi)) => Ok((lo, hi.as_deref())),
                _ => Err(bad()),
            }
        };
        Ok(match name {
            // text
            "len" => Val::Int(text(0)?.chars().count() as i128),
            "lower" => Val::Text(ubos_model::types::nfc(&text(0)?.to_lowercase())),
            "upper" => Val::Text(ubos_model::types::nfc(&text(0)?.to_uppercase())),
            "trim" => Val::Text(text(0)?.trim().to_string()),
            "substr" => {
                let s = text(0)?;
                let start = int(1)?.max(0) as usize;
                let it = s.chars().skip(start);
                Val::Text(if a.len() > 2 {
                    it.take(int(2)?.max(0) as usize).collect()
                } else {
                    it.collect()
                })
            }
            "concat" => {
                if a.iter().any(Val::is_null) {
                    return Ok(Val::Null);
                }
                let mut s = String::new();
                for i in 0..a.len() {
                    s.push_str(text(i)?);
                }
                Val::Text(s)
            }
            "contains" => match &a[0] {
                Val::Period(lo, hi) => {
                    opt_bool(Some(self.in_period(lo, hi.as_deref(), &a[1], pos)?))
                }
                _ => Val::Bool(text(0)?.contains(text(1)?)),
            },
            "starts_with" => Val::Bool(text(0)?.starts_with(text(1)?)),
            "ends_with" => Val::Bool(text(0)?.ends_with(text(1)?)),
            "replace" => {
                let old = text(1)?;
                if old.is_empty() {
                    return Err(terr(pos, "replace needs a non-empty text to find"));
                }
                Val::Text(text(0)?.replace(old, text(2)?))
            }
            "pad_left" => {
                let s = text(0)?;
                let width = int(1)?;
                let fill = text(2)?;
                if fill.chars().count() != 1 {
                    return Err(invalid("the fill is one character"));
                }
                let n = s.chars().count() as i128;
                Val::Text(if n >= width {
                    s.to_string()
                } else {
                    fill.repeat((width - n) as usize) + s
                })
            }
            "format" => {
                let Val::Text(template) = &a[0] else {
                    if a[0].is_null() {
                        return Ok(Val::Null);
                    }
                    return Err(bad());
                };
                let args = &a[1..];
                let r = regex::Regex::new(r"\{([0-9]+)\}").expect("grammar");
                let mut out = String::new();
                let mut last = 0;
                for m in r.captures_iter(template) {
                    let whole = m.get(0).expect("match");
                    out.push_str(&template[last..whole.start()]);
                    let i: usize = m[1].parse().map_err(|_| {
                        invalid(&format!("format placeholder {{{}}} has no argument", &m[1]))
                    })?;
                    let Some(v) = args.get(i) else {
                        return Err(invalid(&format!(
                            "format placeholder {{{i}}} has no argument"
                        )));
                    };
                    out.push_str(&canon_text(v)?);
                    last = whole.end();
                }
                out.push_str(&template[last..]);
                Val::Text(out)
            }
            // numbers
            "abs" => match &a[0] {
                Val::Money(x, c) => Val::Money(
                    if *x < Decimal::parse("0")? {
                        x.neg()
                    } else {
                        x.clone()
                    },
                    c.clone(),
                ),
                Val::Int(i) => Val::Int(i.abs()),
                Val::Dec(d) => Val::Dec(if *d < Decimal::parse("0")? {
                    d.neg()
                } else {
                    d.clone()
                }),
                _ => return Err(bad()),
            },
            "round" => {
                let scale = scale_of(int(1)?)?;
                let mode = if a.len() > 2 {
                    Rounding::parse(text(2)?)?
                } else {
                    Rounding::HalfEven
                };
                match &a[0] {
                    Val::Money(x, c) => Val::Money(x.round(scale, mode), c.clone()),
                    _ => Val::Dec(dec(0)?.round(scale, mode)),
                }
            }
            "floor" => Val::Int(dec_to_int(&dec(0)?.round(0, Rounding::Floor))?),
            "ceil" => Val::Int(dec_to_int(&dec(0)?.round(0, Rounding::Ceiling))?),
            "div" => {
                let (x, y) = (dec(0)?, dec(1)?);
                if y.is_zero() {
                    return Err(BpaError::new("LOGIC.DIVISION_BY_ZERO", "division by zero"));
                }
                let scale = scale_of(int(2)?)?;
                let mode = if a.len() > 3 {
                    Rounding::parse(text(3)?)?
                } else {
                    Rounding::HalfEven
                };
                Val::Dec(x.div(&y, scale, mode)?)
            }
            "clamp" => {
                let (x, lo, hi) = (&a[0], &a[1], &a[2]);
                if self.compare(">", lo, hi, pos)? == Some(true) {
                    return Err(invalid("clamp bounds are reversed"));
                }
                if self.compare("<", x, lo, pos)? == Some(true) {
                    lo.clone()
                } else if self.compare(">", x, hi, pos)? == Some(true) {
                    hi.clone()
                } else {
                    x.clone()
                }
            }
            // money
            "amount" => Val::Dec(money(0)?.0),
            "currency" => Val::Text(money(0)?.1),
            "money" => {
                let cur = text(1)?;
                if minor_unit(cur).is_none() {
                    return Err(invalid("unknown ISO 4217 currency"));
                }
                // a dynamically typed amount may be a decimal string (STD-FND-022), as the
                // canonical JSON of parameters carries decimals
                let amount = match &a[0] {
                    Val::Text(t) => Decimal::parse(t).map_err(|_| bad())?,
                    _ => dec(0)?,
                };
                Val::Money(amount, cur.to_string())
            }
            "convert" => {
                let (x, from) = money(0)?;
                let cur = text(1)?;
                let rate_ref = text(2)?;
                // against a branch the reference names an FxRate object (STD-FND-029)
                let object = match (self.rates.get(rate_ref), self.resolver) {
                    (None, Some(r)) => r.rate_object(rate_ref)?,
                    _ => None,
                };
                let Some(r) = self.rates.get(rate_ref).or(object.as_ref()) else {
                    return Err(BpaError::new(
                        "MODEL.REFERENCE_INVALID",
                        format!("unknown rate {rate_ref}"),
                    ));
                };
                let rf = r.get("from").and_then(Value::as_str).unwrap_or_default();
                let rt = r.get("to").and_then(Value::as_str).unwrap_or_default();
                if rf != from || rt != cur {
                    return Err(BpaError::new(
                        "MODEL.CURRENCY_MISMATCH",
                        format!("the rate converts {rf} to {rt}"),
                    ));
                }
                let rate = match r.get("rate") {
                    Some(Value::String(s)) => Decimal::parse(s)?,
                    Some(Value::Number(n)) => Decimal::parse(&n.to_string())?,
                    _ => return Err(invalid("a rate is a decimal")),
                };
                Val::Money(x.mul(&rate), cur.to_string())
            }
            "fx_convert" => {
                let (x, from) = money(0)?;
                let quote = text(1)?.to_string();
                let rate_type = text(2)?.to_string();
                let source = if a.len() > 3 {
                    Some(text(3)?.to_string())
                } else {
                    None
                };
                let Some(unit) = minor_unit(&quote) else {
                    return Err(invalid("unknown ISO 4217 currency"));
                };
                if from == quote {
                    return Ok(Val::Money(x, quote));
                }
                let rate = match (&self.fx_rates, self.resolver) {
                    (Some(list), _) => Some(fx_in_list(
                        list,
                        &from,
                        &quote,
                        &rate_type,
                        source.as_deref(),
                    )?),
                    (None, Some(r)) => r.fx_rate(&from, &quote, &rate_type, source.as_deref())?,
                    _ => None,
                };
                let amount = match rate {
                    Some(FxRate::Mul(r)) => x.mul(&r),
                    Some(FxRate::Div(r)) => x.div(&r, 60, Rounding::HalfEven)?,
                    None => {
                        return Err(BpaError::with_details(
                            "MODEL.CURRENCY_MISMATCH",
                            serde_json::json!({"base": from, "quote": quote, "rate_type": rate_type}),
                        ));
                    }
                };
                Val::Money(amount.round(unit, Rounding::HalfEven), quote)
            }
            "allocate" => {
                let (x, cur) = money(0)?;
                let Val::List(weights) = &a[1] else {
                    return Err(bad());
                };
                self.allocate(&x, &cur, weights)?
            }
            // dates
            "today" => Val::Date(self.today()?),
            "now" => self.ctx_now(),
            "date" => {
                let (y, m, d) = (int(0)?, int(1)?, int(2)?);
                let ok = i32::try_from(y)
                    .ok()
                    .zip(u32::try_from(m).ok())
                    .zip(u32::try_from(d).ok())
                    .and_then(|((y, m), d)| NaiveDate::from_ymd_opt(y, m, d));
                match ok {
                    Some(d) if (1..=9999).contains(&y) => Val::Date(d),
                    _ => return Err(invalid("not a calendar date")),
                }
            }
            "year" => Val::Int(i128::from(date(0)?.year())),
            "month" => Val::Int(i128::from(date(0)?.month())),
            "day" => Val::Int(i128::from(date(0)?.day())),
            "weekday" => Val::Int(i128::from(date(0)?.weekday().number_from_monday())),
            "add_days" => Val::Date(add_days(date(0)?, int(1)?)?),
            "add_months" => Val::Date(add_months(date(0)?, int(1)?)?),
            "days_between" => Val::Int(i128::from((date(1)? - date(0)?).num_days())),
            "start_of_month" => Val::Date(date(0)?.with_day(1).expect("day 1")),
            "end_of_month" => {
                let d = date(0)?;
                Val::Date(
                    d.with_day(days_in_month(d.year(), d.month()))
                        .expect("last day"),
                )
            }
            "to_zone" => {
                let Val::Instant(i) = &a[0] else {
                    return Err(bad());
                };
                let z: Tz = text(1)?.parse().map_err(|_| invalid("unknown time zone"))?;
                let local = i.with_timezone(&z);
                let mins = local.offset().fix().local_minus_utc() / 60;
                let sign = if mins >= 0 { '+' } else { '-' };
                Val::Text(format!(
                    "{}{sign}{:02}:{:02}",
                    local.format("%Y-%m-%dT%H:%M:%S%.6f"),
                    mins.abs() / 60,
                    mins.abs() % 60
                ))
            }
            // calendars
            "is_business_day" => Val::Bool(self.is_bd(date(0)?, text(1)?)?),
            "add_business_days" => {
                let mut d = date(0)?;
                let n = int(1)?;
                let cal = text(2)?;
                self.charge(n.unsigned_abs().min(u64::MAX as u128) as u64)?;
                for _ in 0..n.abs() {
                    d = self.step_bd(d, cal, if n > 0 { 1 } else { -1 })?;
                }
                Val::Date(d)
            }
            "next_business_day" => Val::Date(self.step_bd(date(0)?, text(1)?, 1)?),
            "previous_business_day" => Val::Date(self.step_bd(date(0)?, text(1)?, -1)?),
            "cutoff" => {
                let mut d = date(0)?;
                let cal = text(3)?;
                if !self.is_bd(d, cal)? {
                    d = self.step_bd(d, cal, 1)?;
                }
                let t = validate_value("time", None, &Value::String(text(1)?.to_string()))?;
                let s = local_to_instant(
                    &d.format("%Y-%m-%d").to_string(),
                    t.as_str().unwrap_or_default(),
                    text(2)?,
                    Some("earlier"),
                )?;
                Val::Instant(parse_instant(&s)?)
            }
            // periods
            "overlaps" => {
                let (plo, phi) = period(0)?;
                let (qlo, qhi) = period(1)?;
                let before = |x: &Val, y: &Val| -> Result<bool, BpaError> {
                    Ok(self.compare("<", x, y, pos)? == Some(true))
                };
                let left = match qhi {
                    None => true,
                    Some(h) => before(plo, h)?,
                };
                let right = match phi {
                    None => true,
                    Some(h) => before(qlo, h)?,
                };
                Val::Bool(left && right)
            }
            "period" => {
                let (lo, hi) = (&a[0], &a[1]);
                if lo.is_null() {
                    return Ok(Val::Null);
                }
                if !hi.is_null()
                    && (std::mem::discriminant(lo) != std::mem::discriminant(hi)
                        || self.compare("<", lo, hi, pos)? != Some(true))
                {
                    return Err(invalid(
                        "a period runs from an earlier to a later time of one granularity",
                    ));
                }
                Val::Period(
                    Box::new(lo.clone()),
                    if hi.is_null() {
                        None
                    } else {
                        Some(Box::new(hi.clone()))
                    },
                )
            }
            // references
            "ref" => {
                let uri = text(0)?.to_string();
                match self.lookup(&uri)? {
                    Some(v) => v,
                    None => {
                        return Err(BpaError::new(
                            "IAM.NOT_FOUND",
                            format!("no object at {uri}"),
                        ));
                    }
                }
            }
            "exists" => {
                if a[0].is_null() {
                    return Ok(Val::Null);
                }
                let uri = text(0)?.to_string();
                Val::Bool(self.lookup(&uri)?.is_some())
            }
            // identifiers
            "luhn_valid" => Val::Bool(luhn_valid(text(0)?)),
            "iso7064_mod97_10" => Val::Bool(mod97(text(0)?)),
            "isin_valid" => Val::Bool(isin_valid(text(0)?)),
            "lei_valid" => Val::Bool(lei_valid(text(0)?)),
            "iban_valid" => Val::Bool(iban_valid(text(0)?)),
            _ => return Err(terr(pos, &format!("unknown function {name}"))),
        })
    }

    fn allocate(&self, amount: &Decimal, cur: &str, weights: &[Val]) -> Result<Val, BpaError> {
        let zero = Decimal::parse("0")?;
        let ws: Vec<Decimal> = weights.iter().filter_map(Val::as_decimal).collect();
        let wsum = ws.iter().fold(zero.clone(), |acc, w| acc.add(w));
        if ws.len() != weights.len()
            || ws.is_empty()
            || ws.iter().any(|w| *w < zero)
            || wsum.is_zero()
        {
            return Err(invalid(
                "weights are non-negative numbers with a positive sum",
            ));
        }
        let scale = minor_unit(cur).unwrap_or(2);
        let total = amount.round(scale, Rounding::HalfEven);
        let ten_pow = Decimal::parse(&format!("1{}", "0".repeat(scale as usize)))?;
        let units = dec_of_int(dec_to_int(&total.mul(&ten_pow))?);
        let exact: Vec<Decimal> = ws
            .iter()
            .map(|w| units.mul(w).div(&wsum, 60, Rounding::Down))
            .collect::<Result<_, _>>()?;
        let mut base: Vec<i128> = exact
            .iter()
            .map(|e| dec_to_int(&e.round(0, Rounding::Floor)))
            .collect::<Result<_, _>>()?;
        let rest = dec_to_int(&units)? - base.iter().sum::<i128>();
        let mut order: Vec<usize> = (0..ws.len()).collect();
        order.sort_by(|&i, &j| {
            let fi = exact[i].sub(&dec_of_int(base[i]));
            let fj = exact[j].sub(&dec_of_int(base[j]));
            fj.cmp(&fi).then(i.cmp(&j))
        });
        for &i in order.iter().take(rest.max(0) as usize) {
            base[i] += 1;
        }
        let unit = Decimal::parse("1")?.div(&ten_pow, scale, Rounding::Down)?;
        Ok(Val::List(
            base.into_iter()
                .map(|b| Val::Money(dec_of_int(b).mul(&unit), cur.to_string()))
                .collect(),
        ))
    }

    fn ctx_now(&self) -> Val {
        match &self.ctx {
            Val::Map(m) => m.get("now").cloned().unwrap_or(Val::Null),
            _ => Val::Null,
        }
    }

    fn today(&self) -> Result<NaiveDate, BpaError> {
        let Val::Instant(now) = self.ctx_now() else {
            return Err(BpaError::new("LOGIC.TYPE_ERROR", "the context has no now"));
        };
        let z: Tz = self.tz.parse().map_err(|_| invalid("unknown time zone"))?;
        Ok(now.with_timezone(&z).date_naive())
    }

    fn cal(&self, name: &str) -> Result<(Vec<u32>, Vec<String>), BpaError> {
        let Some(c) = self.calendars.get(name) else {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                format!("unknown calendar {name}"),
            ));
        };
        let weekend = match c.get("weekend").and_then(Value::as_array) {
            Some(a) => a
                .iter()
                .filter_map(Value::as_u64)
                .map(|x| x as u32)
                .collect(),
            None => vec![6, 7],
        };
        let holidays = c
            .get("holidays")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        Ok((weekend, holidays))
    }

    fn is_bd(&self, d: NaiveDate, name: &str) -> Result<bool, BpaError> {
        let (weekend, holidays) = self.cal(name)?;
        Ok(!weekend.contains(&d.weekday().number_from_monday())
            && !holidays.contains(&d.format("%Y-%m-%d").to_string()))
    }

    fn step_bd(&self, mut d: NaiveDate, name: &str, dir: i64) -> Result<NaiveDate, BpaError> {
        for _ in 0..3660 {
            d = add_days(d, i128::from(dir))?;
            if self.is_bd(d, name)? {
                return Ok(d);
            }
        }
        Err(invalid("the calendar has no business day within ten years"))
    }

    fn in_period(&self, lo: &Val, hi: Option<&Val>, d: &Val, pos: usize) -> Result<bool, BpaError> {
        if std::mem::discriminant(d) != std::mem::discriminant(lo) {
            return Err(terr(
                pos,
                "the period and the value have different granularity",
            ));
        }
        let ge = self.compare("<=", lo, d, pos)? == Some(true);
        let lt = match hi {
            None => true,
            Some(h) => self.compare("<", d, h, pos)? == Some(true),
        };
        Ok(ge && lt)
    }

    fn lookup(&mut self, uri: &str) -> Result<Option<Val>, BpaError> {
        self.charge(REF_COST)?;
        if !is_reference(uri) {
            return Err(invalid("a reference is a canonical URI"));
        }
        match self.resolver {
            Some(r) => r.resolve(uri),
            None => Ok(self.objects.get(uri).map(|v| from_json(&Ty::Any, v))),
        }
    }
}

fn opt_bool(b: Option<bool>) -> Val {
    match b {
        Some(b) => Val::Bool(b),
        None => Val::Null,
    }
}

fn scale_of(n: i128) -> Result<u32, BpaError> {
    u32::try_from(n)
        .ok()
        .filter(|s| *s <= 100)
        .ok_or_else(|| invalid("the scale is a small non-negative integer"))
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn add_days(d: NaiveDate, n: i128) -> Result<NaiveDate, BpaError> {
    i64::try_from(n)
        .ok()
        .and_then(|n| d.checked_add_signed(CDuration::days(n)))
        .filter(|d| (1..=9999).contains(&d.year()))
        .ok_or_else(|| invalid("date out of range"))
}

fn add_months(d: NaiveDate, n: i128) -> Result<NaiveDate, BpaError> {
    let m = i128::from(d.month()) - 1 + n;
    let y = i128::from(d.year()) + m.div_euclid(12);
    let m = (m.rem_euclid(12) + 1) as u32;
    let y = i32::try_from(y)
        .ok()
        .filter(|y| (1..=9999).contains(y))
        .ok_or_else(|| invalid("date out of range"))?;
    NaiveDate::from_ymd_opt(y, m, d.day().min(days_in_month(y, m)))
        .ok_or_else(|| invalid("date out of range"))
}

/// The text of a value in `format()`: text as is, lists and objects as JSON, the rest as
/// their canonical text (money as `amount currency`, booleans lower-case).
fn canon_text(v: &Val) -> Result<String, BpaError> {
    Ok(match v {
        Val::Null => "null".into(),
        Val::Text(s) => s.clone(),
        Val::List(_) | Val::Map(_) => py_json(&to_json(v)?),
        Val::Money(..) => {
            let j = to_json(v)?;
            format!(
                "{} {}",
                j["amount"].as_str().unwrap_or_default(),
                j["currency"].as_str().unwrap_or_default()
            )
        }
        Val::Period(..) => {
            let j = to_json(v)?;
            let r = |x: &Value| -> String {
                match x.as_str() {
                    Some(s) => format!("'{s}'"),
                    None => "none".into(),
                }
            };
            format!("{{'from': {}, 'to': {}}}", r(&j["from"]), r(&j["to"]))
        }
        _ => match to_json(v)? {
            Value::String(s) => s,
            other => other.to_string().to_lowercase(),
        },
    })
}

/// JSON text with Python's default separators (`, ` and `: `).
fn py_json(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(py_json).collect::<Vec<_>>().join(", ")),
        Value::Object(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, x)| format!("{}: {}", Value::String(k.clone()), py_json(x)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => other.to_string(),
    }
}

// identifiers

fn luhn(digits: &str) -> bool {
    let mut total = 0u32;
    for (i, ch) in digits.chars().rev().enumerate() {
        let mut d = ch.to_digit(10).unwrap_or(0);
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        total += d;
    }
    total.is_multiple_of(10)
}

fn base36_digits(s: &str) -> Option<String> {
    let mut out = String::new();
    for c in s.chars() {
        out.push_str(&c.to_digit(36)?.to_string());
    }
    Some(out)
}

fn mod97(s: &str) -> bool {
    if s.is_empty() || !s.is_ascii() || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let Some(n) = base36_digits(&s.to_ascii_uppercase()) else {
        return false;
    };
    let mut r = 0u32;
    for c in n.chars() {
        r = (r * 10 + c.to_digit(10).unwrap_or(0)) % 97;
    }
    r == 1
}

fn luhn_valid(s: &str) -> bool {
    s.len() >= 2 && s.chars().all(|c| c.is_ascii_digit()) && luhn(s)
}

fn isin_valid(s: &str) -> bool {
    let b = s.as_bytes();
    let ok = b.len() == 12
        && b[..2].iter().all(u8::is_ascii_uppercase)
        && b[2..11]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && b[11].is_ascii_digit();
    ok && base36_digits(s).is_some_and(|d| luhn(&d))
}

fn lei_valid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 20
        && b[..18]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && b[18..].iter().all(u8::is_ascii_digit)
        && mod97(s)
}

fn iban_valid(s: &str) -> bool {
    let b = s.as_bytes();
    let ok = (15..=34).contains(&b.len())
        && b[..2].iter().all(u8::is_ascii_uppercase)
        && b[2..4].iter().all(u8::is_ascii_digit)
        && b[4..]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    ok && mod97(&format!("{}{}", &s[4..], &s[..4]))
}
