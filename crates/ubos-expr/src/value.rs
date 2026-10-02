//! Static types and run-time values of L1 (STD-EXPR-002), and their canonical JSON forms.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use ubos_model::types::BpaError;
use ubos_model::types::decimal::{Decimal, Rounding};
use ubos_model::types::time::{format_instant, parse_date, parse_instant};
use ubos_model::types::values::minor_unit;

/// A static type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// `text` (also `enum` and `reference` values).
    Text,
    /// `integer`.
    Integer,
    /// `decimal`.
    Decimal,
    /// `money`.
    Money,
    /// `boolean`.
    Boolean,
    /// `date`.
    Date,
    /// `instant`.
    Instant,
    /// `time`.
    Time,
    /// `duration`.
    Duration,
    /// `period`.
    Period,
    /// The type of `null`.
    Null,
    /// Dynamic: checked at run time.
    Any,
    /// `list<T>`.
    List(Box<Ty>),
    /// An object with typed members.
    Obj(BTreeMap<String, Ty>),
}

impl Ty {
    /// The item type of a list.
    pub fn item(&self) -> Option<&Ty> {
        match self {
            Ty::List(t) => Some(t),
            _ => None,
        }
    }

    /// True for `any` or `null`.
    pub fn is_dyn(&self) -> bool {
        matches!(self, Ty::Any | Ty::Null)
    }

    /// True for the numeric types.
    pub fn is_num(&self) -> bool {
        matches!(self, Ty::Integer | Ty::Decimal)
    }

    /// True for the ordered types.
    pub fn is_ordered(&self) -> bool {
        matches!(
            self,
            Ty::Integer
                | Ty::Decimal
                | Ty::Text
                | Ty::Date
                | Ty::Instant
                | Ty::Time
                | Ty::Duration
                | Ty::Money
        )
    }

    /// Short name for messages (`list` and `object` for the compound types).
    pub fn short(&self) -> String {
        match self {
            Ty::List(_) => "list".into(),
            Ty::Obj(_) => "obj".into(),
            other => type_name(other),
        }
    }
}

/// The name of a type as `check_expr` reports it.
pub fn type_name(t: &Ty) -> String {
    match t {
        Ty::Text => "text".into(),
        Ty::Integer => "integer".into(),
        Ty::Decimal => "decimal".into(),
        Ty::Money => "money".into(),
        Ty::Boolean => "boolean".into(),
        Ty::Date => "date".into(),
        Ty::Instant => "instant".into(),
        Ty::Time => "time".into(),
        Ty::Duration => "duration".into(),
        Ty::Period => "period".into(),
        Ty::Null => "null".into(),
        Ty::Any => "any".into(),
        Ty::List(t) => format!("list<{}>", type_name(t)),
        Ty::Obj(_) => "object".into(),
    }
}

/// The common type of two compatible types, or `None`.
pub fn compat(a: &Ty, b: &Ty) -> Option<Ty> {
    if *a == Ty::Any || *b == Ty::Any {
        return Some(Ty::Any);
    }
    if *a == Ty::Null {
        return Some(b.clone());
    }
    if *b == Ty::Null {
        return Some(a.clone());
    }
    if a == b {
        return Some(a.clone());
    }
    if a.is_num() && b.is_num() {
        return Some(Ty::Decimal);
    }
    match (a, b) {
        (Ty::List(x), Ty::List(y)) => compat(x, y).map(|c| Ty::List(Box::new(c))),
        (Ty::Obj(_), Ty::Obj(_)) => Some(Ty::Any),
        _ => None,
    }
}

/// Static type of a field declaration `{type, constraints}` or of a type name; custom
/// value types are their base type.
pub fn field_type(f: &Value, custom: &BTreeMap<String, Value>) -> Ty {
    let decl;
    let f = if let Some(name) = f.as_str() {
        decl = json!({"type": name});
        &decl
    } else {
        f
    };
    let mut t = f.get("type").and_then(Value::as_str).unwrap_or_default();
    if let Some(c) = custom.get(t) {
        t = c.get("base").and_then(Value::as_str).unwrap_or_default();
    }
    match t {
        "text" | "enum" | "reference" => Ty::Text,
        "integer" => Ty::Integer,
        "decimal" => Ty::Decimal,
        "money" => Ty::Money,
        "boolean" => Ty::Boolean,
        "date" => Ty::Date,
        "instant" => Ty::Instant,
        "time" => Ty::Time,
        "duration" => Ty::Duration,
        "period" => Ty::Period,
        "list" => {
            let item = f
                .get("constraints")
                .and_then(|c| c.get("item_type"))
                .cloned()
                .unwrap_or(json!({"type": "json"}));
            Ty::List(Box::new(field_type(&item, custom)))
        }
        "composite" => Ty::Obj(
            f.get("constraints")
                .and_then(|c| c.get("members"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|m| {
                    (
                        m.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        field_type(m, custom),
                    )
                })
                .collect(),
        ),
        _ => Ty::Any,
    }
}

/// A run-time value.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i128),
    /// An exact decimal.
    Dec(Decimal),
    /// Money: amount and ISO 4217 code.
    Money(Decimal, String),
    /// Text (also `time` values, which stay text).
    Text(String),
    /// A date.
    Date(NaiveDate),
    /// An instant (UTC).
    Instant(DateTime<Utc>),
    /// A duration in microseconds.
    Duration(i128),
    /// A period `[from, to)` of dates or instants; `to` may be open.
    Period(Box<Val>, Option<Box<Val>>),
    /// A list.
    List(Vec<Val>),
    /// A map or object.
    Map(BTreeMap<String, Val>),
}

impl Val {
    /// True for `null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }

    /// The decimal value of a number.
    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            Val::Int(i) => Decimal::parse(&i.to_string()).ok(),
            Val::Dec(d) => Some(d.clone()),
            _ => None,
        }
    }

    /// True for integers and decimals.
    pub fn is_num(&self) -> bool {
        matches!(self, Val::Int(_) | Val::Dec(_))
    }
}

/// Canonical text of a duration (`P[nD][T[nH][nM][n.nS]]`).
pub fn fmt_duration(us: i128) -> String {
    let (sign, us) = if us < 0 { ("-", -us) } else { ("", us) };
    let (days, rest) = (us / (86_400 * 1_000_000), us % (86_400 * 1_000_000));
    let (h, rest) = (rest / (3_600 * 1_000_000), rest % (3_600 * 1_000_000));
    let (m, rest) = (rest / (60 * 1_000_000), rest % (60 * 1_000_000));
    let (s, frac) = (rest / 1_000_000, rest % 1_000_000);
    let mut out = format!("{sign}P");
    if days > 0 {
        out.push_str(&format!("{days}D"));
    }
    let mut t = String::new();
    if h > 0 {
        t.push_str(&format!("{h}H"));
    }
    if m > 0 {
        t.push_str(&format!("{m}M"));
    }
    if s > 0 || frac > 0 {
        let mut x = format!("{s}.{frac:06}");
        while x.ends_with('0') {
            x.pop();
        }
        if x.ends_with('.') {
            x.pop();
        }
        t.push_str(&x);
        t.push('S');
    }
    if !t.is_empty() {
        out.push('T');
        out.push_str(&t);
    }
    if out == "P" || out == "-P" {
        "PT0S".into()
    } else {
        out
    }
}

/// Parse a duration with days, hours, minutes and seconds only.
pub fn parse_duration(s: &str) -> Result<i128, BpaError> {
    let bad = || {
        BpaError::new(
            "MODEL.INVALID_VALUE",
            "durations with days, hours, minutes and seconds only",
        )
    };
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let r = R.get_or_init(|| {
        regex::Regex::new(
            r"^(-)?P(?:([0-9]+)D)?(?:T(?:([0-9]+)H)?(?:([0-9]+)M)?(?:([0-9]+(?:\.[0-9]{1,6})?)S)?)?$",
        )
        .expect("grammar")
    });
    let m = r.captures(s).ok_or_else(bad)?;
    if matches!(s, "P" | "-P" | "PT" | "-PT") {
        return Err(bad());
    }
    let n = |i: usize| -> i128 {
        m.get(i)
            .map(|x| x.as_str().parse::<i128>().unwrap_or_default())
            .unwrap_or_default()
    };
    let secs = m.get(5).map(|x| x.as_str()).unwrap_or("0");
    let (whole, frac) = match secs.split_once('.') {
        Some((w, f)) => (w.parse::<i128>().unwrap_or_default(), {
            let mut f = f.to_string();
            while f.len() < 6 {
                f.push('0');
            }
            f.parse::<i128>().unwrap_or_default()
        }),
        None => (secs.parse::<i128>().unwrap_or_default(), 0),
    };
    let us = ((n(2) * 24 + n(3)) * 60 + n(4)) * 60 * 1_000_000 + whole * 1_000_000 + frac;
    Ok(if m.get(1).is_some() { -us } else { us })
}

/// Run-time value of canonical JSON `v` of static type `t`.
pub fn from_json(t: &Ty, v: &Value) -> Val {
    if v.is_null() {
        return Val::Null;
    }
    match t {
        Ty::Integer => match v {
            Value::Number(n) => n
                .as_i64()
                .map(|i| Val::Int(i128::from(i)))
                .unwrap_or_else(|| any_of(v)),
            Value::String(s) => s
                .parse::<i128>()
                .map(Val::Int)
                .unwrap_or_else(|_| any_of(v)),
            _ => any_of(v),
        },
        Ty::Decimal => match v.as_str().and_then(|s| Decimal::parse(s).ok()) {
            Some(d) => Val::Dec(d),
            None => any_of(v),
        },
        Ty::Money => match (
            v.get("amount")
                .and_then(Value::as_str)
                .and_then(|a| Decimal::parse(a).ok()),
            v.get("currency").and_then(Value::as_str),
        ) {
            (Some(a), Some(c)) => Val::Money(a, c.to_string()),
            _ => any_of(v),
        },
        Ty::Date => match v.as_str().and_then(|s| parse_date(s).ok()) {
            Some(d) => Val::Date(d),
            None => any_of(v),
        },
        Ty::Instant => match v.as_str().and_then(|s| parse_instant(s).ok()) {
            Some(d) => Val::Instant(d),
            None => any_of(v),
        },
        Ty::Time => any_of(v),
        Ty::Duration => match v.as_str().and_then(|s| parse_duration(s).ok()) {
            Some(us) => Val::Duration(us),
            None => any_of(v),
        },
        Ty::Period => {
            let g = |x: &Value| -> Val {
                match x.as_str() {
                    Some(s) if s.len() == 10 => from_json(&Ty::Date, x),
                    Some(_) => from_json(&Ty::Instant, x),
                    None => Val::Null,
                }
            };
            let lo = g(v.get("from").unwrap_or(&Value::Null));
            let hi = v.get("to").filter(|x| !x.is_null()).map(|x| Box::new(g(x)));
            Val::Period(Box::new(lo), hi)
        }
        Ty::List(item) => match v.as_array() {
            Some(a) => Val::List(a.iter().map(|x| from_json(item, x)).collect()),
            None => any_of(v),
        },
        Ty::Obj(members) => match v.as_object() {
            Some(m) => Val::Map(
                m.iter()
                    .map(|(k, x)| (k.clone(), from_json(members.get(k).unwrap_or(&Ty::Any), x)))
                    .collect(),
            ),
            None => any_of(v),
        },
        _ => any_of(v),
    }
}

/// A JSON value as a dynamic run-time value (`json` fields, `any`).
fn any_of(v: &Value) -> Val {
    match v {
        Value::Null => Val::Null,
        Value::Bool(b) => Val::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Val::Int(i128::from(i)),
            None => Decimal::parse(&n.to_string())
                .map(Val::Dec)
                .unwrap_or(Val::Text(n.to_string())),
        },
        Value::String(s) => Val::Text(s.clone()),
        Value::Array(a) => Val::List(a.iter().map(any_of).collect()),
        Value::Object(m) => Val::Map(m.iter().map(|(k, x)| (k.clone(), any_of(x))).collect()),
    }
}

/// The canonical JSON form of a run-time value.
pub fn to_json(v: &Val) -> Result<Value, BpaError> {
    Ok(match v {
        Val::Null => Value::Null,
        Val::Bool(b) => Value::Bool(*b),
        Val::Int(i) => {
            if i.abs() < (1i128 << 53) {
                json!(*i as i64)
            } else {
                Value::String(i.to_string())
            }
        }
        Val::Dec(d) => Value::String(d.format(None)),
        Val::Money(a, c) => {
            let scale = minor_unit(c).unwrap_or(2);
            json!({"amount": a.round(scale, Rounding::HalfEven).format(Some(scale)), "currency": c})
        }
        Val::Text(s) => Value::String(s.clone()),
        Val::Date(d) => Value::String(d.format("%Y-%m-%d").to_string()),
        Val::Instant(t) => Value::String(format_instant(t)),
        Val::Duration(us) => Value::String(fmt_duration(*us)),
        Val::Period(lo, hi) => {
            let to = match hi {
                Some(h) => to_json(h)?,
                None => Value::Null,
            };
            json!({"from": to_json(lo)?, "to": to})
        }
        Val::List(items) => Value::Array(items.iter().map(to_json).collect::<Result<_, _>>()?),
        Val::Map(m) => {
            let mut out = Map::new();
            for (k, x) in m {
                out.insert(k.clone(), to_json(x)?);
            }
            Value::Object(out)
        }
    })
}
