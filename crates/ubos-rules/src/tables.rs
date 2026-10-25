//! Decision tables (STD-LGS-040, STD-ISA-053; BPA 1.0-rc8; PH-2 step 5b, DEC-113): the
//! declaration check, evaluation under the hit policies, and the authoring check for gaps and
//! overlaps (FR-RULE-042, DSN-DVM-722).

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use ubos_expr::Node;
use ubos_expr::value::Val;
use ubos_model::types::decimal::{Decimal, Rounding};
use ubos_model::types::values::{minor_unit, validate_value};
use ubos_model::types::{BpaError, canonical};

const HIT_POLICIES: &[&str] = &["unique", "first", "priority", "collect"];
const AGGREGATORS: &[&str] = &["sum", "min", "max", "count"];
/// The largest number of input combinations the authoring check enumerates.
const MAX_POINTS: usize = 10_000;
const ANY: &str = "-";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn incomplete(what: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.into())
}

fn invalid(what: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", what.into())
}

fn columns<'a>(t: &'a Value, k: &str) -> &'a [Value] {
    t[k].as_array().map_or(&[], Vec::as_slice)
}

/// The members of a decision table (`DAT-DecisionTable`).
pub fn check_decl(t: &Value) -> Result<(), BpaError> {
    if !t.is_object() {
        return Err(incomplete("a decision table is a map"));
    }
    for k in ["inputs", "outputs", "rows", "hit_policy"] {
        if t.get(k).is_none_or(Value::is_null) {
            return Err(incomplete(format!("decision table member {k}")));
        }
    }
    let pol = s(t, "hit_policy").unwrap_or_default();
    if !HIT_POLICIES.contains(&pol) {
        return Err(invalid("hit_policy is unique, first, priority or collect"));
    }
    let agg = t.get("aggregator").filter(|v| !v.is_null());
    if let Some(a) = agg
        && (!a.as_str().is_some_and(|a| AGGREGATORS.contains(&a)) || pol != "collect")
    {
        return Err(invalid(
            "an aggregator (sum, min, max, count) goes with collect",
        ));
    }
    for col in ["inputs", "outputs"] {
        let items = t[col].as_array().filter(|a| !a.is_empty());
        let ok = items.is_some_and(|a| {
            a.iter()
                .all(|f| s(f, "name").is_some() && s(f, "type").is_some())
        });
        if !ok {
            return Err(incomplete(format!("{col} are {{name, type}} items")));
        }
        let names: BTreeSet<&str> = columns(t, col)
            .iter()
            .filter_map(|f| s(f, "name"))
            .collect();
        if names.len() != columns(t, col).len() {
            return Err(invalid(format!("{col} have unique names")));
        }
    }
    if matches!(agg.and_then(Value::as_str), Some("sum" | "min" | "max"))
        && columns(t, "outputs")
            .iter()
            .any(|o| !matches!(s(o, "type"), Some("integer" | "decimal" | "money")))
    {
        return Err(invalid("sum, min and max aggregate numeric outputs"));
    }
    let names: BTreeSet<&str> = columns(t, "inputs")
        .iter()
        .filter_map(|f| s(f, "name"))
        .collect();
    let outs: BTreeMap<&str, &Value> = columns(t, "outputs")
        .iter()
        .filter_map(|f| s(f, "name").map(|n| (n, f)))
        .collect();
    let Some(rows) = t["rows"].as_array() else {
        return Err(incomplete("rows"));
    };
    for r in rows {
        let Some(then) = r.get("then").and_then(Value::as_object) else {
            return Err(incomplete("a row is {when, then, priority}"));
        };
        let when = r.get("when").filter(|v| !v.is_null());
        let when_ok = match when {
            None => true,
            Some(w) => w
                .as_object()
                .is_some_and(|w| w.keys().all(|k| names.contains(k.as_str()))),
        };
        if !when_ok {
            return Err(invalid("a row condition names an input"));
        }
        if then.keys().any(|k| !outs.contains_key(k.as_str()))
            || outs.keys().any(|k| !then.contains_key(*k))
        {
            return Err(invalid("a row gives every output"));
        }
        for (k, v) in then {
            let o = outs[k.as_str()];
            validate_value(s(o, "type").unwrap_or_default(), o.get("constraints"), v)?;
        }
        if pol == "priority" && !r["priority"].is_i64() {
            return Err(incomplete("rows of a priority table have a priority"));
        }
    }
    Ok(())
}

/// The inputs checked against their declarations, as canonical values.
fn check_inputs(t: &Value, inputs: &Value) -> Result<Map<String, Value>, BpaError> {
    let Some(given) = inputs.as_object() else {
        return Err(invalid("inputs are a map"));
    };
    let mut out = Map::new();
    for f in columns(t, "inputs") {
        let n = s(f, "name").unwrap_or_default();
        let Some(v) = given.get(n).filter(|v| !v.is_null()) else {
            return Err(incomplete(format!("input {n}")));
        };
        out.insert(
            n.to_string(),
            validate_value(s(f, "type").unwrap_or_default(), f.get("constraints"), v)?,
        );
    }
    if given.keys().any(|k| !out.contains_key(k)) {
        return Err(invalid("unknown input"));
    }
    Ok(out)
}

type Cache = HashMap<(String, String), bool>;

/// A cell: an L1 boolean over `params` (the inputs); `-` matches any value.
fn cell(
    t: &Value,
    c: Option<&Value>,
    inputs: &Map<String, Value>,
    cache: &mut Cache,
) -> Result<bool, BpaError> {
    let Some(text) = c.and_then(Value::as_str).filter(|x| *x != ANY) else {
        return Ok(true);
    };
    let key = (text.to_string(), canonical(&Value::Object(inputs.clone()))?);
    if let Some(v) = cache.get(&key) {
        return Ok(*v);
    }
    let types: Map<String, Value> = columns(t, "inputs")
        .iter()
        .filter_map(|f| s(f, "name").map(|n| (n.to_string(), f.clone())))
        .collect();
    let out = ubos_expr::eval_expr(
        &json!({"expr": text, "params_types": types, "params": inputs}),
        None,
    )?;
    let Some(v) = out["value"].as_bool() else {
        return Err(BpaError::new(
            "RULE.PARSE_ERROR",
            "a condition is an L1 boolean",
        ));
    };
    cache.insert(key, v);
    Ok(v)
}

fn matching_rows(
    t: &Value,
    inputs: &Map<String, Value>,
    cache: &mut Cache,
) -> Result<Vec<usize>, BpaError> {
    let mut out = Vec::new();
    for (n, r) in columns(t, "rows").iter().enumerate() {
        let mut hit = true;
        for f in columns(t, "inputs") {
            let name = s(f, "name").unwrap_or_default();
            if !cell(t, r.get("when").and_then(|w| w.get(name)), inputs, cache)? {
                hit = false;
                break;
            }
        }
        if hit {
            out.push(n);
        }
    }
    Ok(out)
}

/// `(currency, amount)` of a numeric output value.
fn num(v: &Value) -> Result<(String, Decimal), BpaError> {
    Ok(match v {
        Value::Object(_) => (
            s(v, "currency").unwrap_or_default().to_string(),
            Decimal::parse(s(v, "amount").unwrap_or("?"))?,
        ),
        Value::String(t) => (String::new(), Decimal::parse(t)?),
        _ => (String::new(), Decimal::parse(&v.to_string())?),
    })
}

/// STD-LGS-040: `{outputs, rows}` of a table under its hit policy.
pub fn evaluate(t: &Value, inputs: &Value) -> Result<Value, BpaError> {
    check_decl(t)?;
    let inputs = check_inputs(t, inputs)?;
    let rows = matching_rows(t, &inputs, &mut Cache::new())?;
    let then = |n: usize| t["rows"][n]["then"].clone();
    let pol = s(t, "hit_policy").unwrap_or_default();
    if matches!(pol, "unique" | "first" | "priority") && rows.is_empty() {
        return Err(BpaError::with_details(
            "RULE.NO_MATCH",
            json!({"inputs": inputs}),
        ));
    }
    match pol {
        "unique" => {
            if rows.len() > 1 {
                return Err(BpaError::with_details(
                    "RULE.MULTIPLE_MATCH",
                    json!({"rows": rows}),
                ));
            }
            return Ok(json!({"outputs": then(rows[0]), "rows": rows}));
        }
        "first" => return Ok(json!({"outputs": then(rows[0]), "rows": [rows[0]]})),
        "priority" => {
            let best = *rows
                .iter()
                .max_by_key(|n| {
                    (
                        t["rows"][**n]["priority"].as_i64().unwrap_or_default(),
                        -(**n as i64),
                    )
                })
                .expect("a matching row");
            return Ok(json!({"outputs": then(best), "rows": [best]}));
        }
        _ => {}
    }
    match s(t, "aggregator") {
        None => {
            Ok(json!({"outputs": rows.iter().map(|n| then(*n)).collect::<Vec<_>>(), "rows": rows}))
        }
        Some("count") => Ok(json!({"outputs": {"count": rows.len()}, "rows": rows})),
        Some(agg) => {
            let mut out = Map::new();
            for f in columns(t, "outputs") {
                let name = s(f, "name").unwrap_or_default();
                let vals: Vec<(String, Decimal)> = rows
                    .iter()
                    .map(|n| num(&t["rows"][*n]["then"][name]))
                    .collect::<Result<_, _>>()?;
                if vals.is_empty() {
                    out.insert(name.into(), Value::Null);
                    continue;
                }
                let cur: BTreeSet<&String> = vals.iter().map(|(c, _)| c).collect();
                if cur.len() > 1 {
                    return Err(BpaError::new(
                        "MODEL.CURRENCY_MISMATCH",
                        "outputs in several currencies",
                    ));
                }
                let mut it = vals.iter().map(|(_, d)| d.clone());
                let first = it.next().expect("a value");
                let r = it.fold(first, |acc, d| match agg {
                    "sum" => acc.add(&d),
                    "min" => {
                        if d < acc {
                            d
                        } else {
                            acc
                        }
                    }
                    _ => {
                        if d > acc {
                            d
                        } else {
                            acc
                        }
                    }
                });
                let v = match s(f, "type") {
                    Some("money") => {
                        let c = vals[0].0.clone();
                        json!({"amount": r.format(minor_unit(&c)), "currency": c})
                    }
                    Some("integer") => {
                        json!(
                            r.round(0, Rounding::Down)
                                .format(None)
                                .parse::<i64>()
                                .unwrap_or_default()
                        )
                    }
                    _ => json!(r.format(None)),
                };
                out.insert(name.into(), v);
            }
            Ok(json!({"outputs": out, "rows": rows}))
        }
    }
}

// ---- the authoring check (FR-RULE-042) ----

/// The value of a literal node: a literal, `date("…")` or a negated number.
fn lit(n: &Node) -> Option<Value> {
    match n {
        Node::Lit(_, v, _) => val_json(v),
        Node::Call(_, name, args) if name == "date" && args.len() == 1 => match &args[0] {
            Node::Lit(_, Val::Text(t), _) => Some(json!(t)),
            _ => None,
        },
        Node::Neg(_, inner) => match inner.as_ref() {
            Node::Lit(_, Val::Int(i), _) => Some(json!((-i).to_string())),
            Node::Lit(_, Val::Dec(d), _) => Some(json!(d.neg().format(None))),
            _ => None,
        },
        _ => None,
    }
}

fn val_json(v: &Val) -> Option<Value> {
    match v {
        Val::Int(i) => Some(json!(i.to_string())),
        Val::Dec(d) => Some(json!(d.format(None))),
        Val::Text(t) => Some(json!(t)),
        Val::Bool(b) => Some(json!(b)),
        Val::Date(d) => Some(json!(d.format("%Y-%m-%d").to_string())),
        _ => None,
    }
}

fn is_param(n: &Node, name: &str) -> bool {
    matches!(n, Node::Member(_, base, m, _) if m == name && matches!(base.as_ref(), Node::Path(_, p) if p == "params"))
}

/// The literals compared with `params.<name>` in a condition.
fn literals(n: &Node, name: &str, out: &mut Vec<Value>) {
    match n {
        Node::Cmp(_, _, a, b) => {
            for (x, y) in [(a, b), (b, a)] {
                if is_param(x, name)
                    && let Some(v) = lit(y)
                {
                    out.push(v);
                }
            }
        }
        Node::In(_, a, b) if is_param(a, name) => {
            if let Node::List(_, items) = b.as_ref() {
                out.extend(items.iter().filter_map(lit));
            }
        }
        _ => {}
    }
    let children: Vec<&Node> = match n {
        Node::List(_, xs) | Node::Call(_, _, xs) => xs.iter().collect(),
        Node::Member(_, a, ..)
        | Node::Neg(_, a)
        | Node::Not(_, a)
        | Node::IsNull(_, a, _)
        | Node::Matches(_, a, _) => {
            vec![a.as_ref()]
        }
        Node::Index(_, a, b)
        | Node::Arith(_, _, a, b)
        | Node::Cmp(_, _, a, b)
        | Node::In(_, a, b)
        | Node::And(_, a, b)
        | Node::Or(_, a, b) => vec![a.as_ref(), b.as_ref()],
        Node::Cond(_, a, b, c) => vec![a.as_ref(), b.as_ref(), c.as_ref()],
        Node::Lambda(_, _, a, _, b) => vec![a.as_ref(), b.as_ref()],
        _ => vec![],
    };
    for c in children {
        literals(c, name, out);
    }
}

/// The representative values of an input: its domain when enumerable, the boundaries of its
/// conditions and the values between and around them when range-based; `None` otherwise.
fn points(t: &Value, f: &Value) -> Result<Option<Vec<Value>>, BpaError> {
    let typ = s(f, "type").unwrap_or_default();
    match typ {
        "boolean" => return Ok(Some(vec![json!(true), json!(false)])),
        "enum" => {
            return Ok(Some(
                f["constraints"]["allowed_values"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            ));
        }
        "integer" | "decimal" | "date" => {}
        _ => return Ok(None),
    }
    let name = s(f, "name").unwrap_or_default();
    let mut lits = Vec::new();
    for r in columns(t, "rows") {
        if let Some(c) = r
            .get("when")
            .and_then(|w| w.get(name))
            .filter(|c| c.as_str().is_some_and(|x| x != ANY))
        {
            literals(&ubos_expr::parser::parse(c)?, name, &mut lits);
        }
    }
    if typ == "date" {
        let mut ds = BTreeSet::new();
        for x in &lits {
            let text = x
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| x.to_string());
            ds.insert(ubos_model::types::time::parse_date(&text)?);
        }
        let Some(first) = ds.iter().next().copied() else {
            return Ok(Some(vec![json!("2000-01-01")]));
        };
        let mut pts = BTreeSet::new();
        pts.extend(first.pred_opt());
        for d in &ds {
            pts.extend(d.pred_opt());
            pts.insert(*d);
            pts.extend(d.succ_opt());
        }
        return Ok(Some(
            pts.iter()
                .map(|d| json!(d.format("%Y-%m-%d").to_string()))
                .collect(),
        ));
    }
    let mut cs: Vec<Decimal> = Vec::new();
    for x in &lits {
        let text = x
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| x.to_string());
        let d = Decimal::parse(&text)?;
        if !cs.contains(&d) {
            cs.push(d);
        }
    }
    cs.sort();
    if cs.is_empty() {
        return Ok(Some(vec![if typ == "integer" {
            json!(0)
        } else {
            json!("0")
        }]));
    }
    let step = Decimal::parse(if typ == "integer" { "1" } else { "0.01" })?;
    let two = Decimal::parse("2")?;
    let mut pts: Vec<Decimal> = Vec::new();
    let push = |d: Decimal, pts: &mut Vec<Decimal>| {
        if !pts.contains(&d) {
            pts.push(d);
        }
    };
    for c in &cs {
        push(c.sub(&step), &mut pts);
        push(c.clone(), &mut pts);
        push(c.add(&step), &mut pts);
    }
    for w in cs.windows(2) {
        let sum = w[0].add(&w[1]);
        let mid = if typ == "integer" {
            sum.div(&two, 0, Rounding::Floor)?
        } else {
            sum.div(&two, sum.scale() + 1, Rounding::HalfEven)?
        };
        push(mid, &mut pts);
    }
    pts.sort();
    Ok(Some(
        pts.iter()
            .map(|p| {
                if typ == "integer" {
                    json!(p.format(None).parse::<i64>().unwrap_or_default())
                } else {
                    json!(p.format(None))
                }
            })
            .collect(),
    ))
}

/// FR-RULE-042: the gaps (input combinations no row matches) and the overlaps the hit policy
/// forbids, over the representative values of the enumerable and range-based inputs; other
/// inputs are listed as unchecked.
pub fn check_table(t: &Value) -> Result<Value, BpaError> {
    check_decl(t)?;
    let mut axes: Vec<(String, Vec<Value>)> = Vec::new();
    let mut unchecked = Vec::new();
    for f in columns(t, "inputs") {
        let name = s(f, "name").unwrap_or_default().to_string();
        match points(t, f)? {
            Some(p) if !p.is_empty() => axes.push((name, p)),
            _ => unchecked.push(name),
        }
    }
    if !unchecked.is_empty() {
        return Ok(json!({"gaps": [], "overlaps": [], "unchecked": unchecked, "points": 0}));
    }
    let total = axes.iter().map(|(_, p)| p.len()).product::<usize>();
    if total > MAX_POINTS {
        let names: Vec<&String> = axes.iter().map(|(n, _)| n).collect();
        return Ok(json!({"gaps": [], "overlaps": [], "unchecked": names, "points": 0}));
    }
    let pol = s(t, "hit_policy").unwrap_or_default();
    let (mut gaps, mut overlaps, mut cache) = (Vec::new(), Vec::new(), Cache::new());
    for k in 0..total {
        // the k-th combination, the last axis varying fastest (itertools.product order)
        let mut point = Map::new();
        let mut rest = k;
        let mut picks = vec![0usize; axes.len()];
        for (i, (_, p)) in axes.iter().enumerate().rev() {
            picks[i] = rest % p.len();
            rest /= p.len();
        }
        for (i, (n, p)) in axes.iter().enumerate() {
            point.insert(n.clone(), p[picks[i]].clone());
        }
        let inputs = check_inputs(t, &Value::Object(point.clone()))?;
        let rows = matching_rows(t, &inputs, &mut cache)?;
        if rows.is_empty() && pol != "collect" {
            gaps.push(Value::Object(point.clone()));
        }
        if rows.len() > 1 {
            if pol == "unique" {
                overlaps.push(json!({"point": point, "rows": rows}));
            } else if pol == "priority" {
                let pr = |n: &usize| t["rows"][*n]["priority"].as_i64().unwrap_or_default();
                let top = rows.iter().map(pr).max().unwrap_or_default();
                let tied: Vec<usize> = rows.iter().copied().filter(|n| pr(n) == top).collect();
                if tied.len() > 1 {
                    overlaps.push(json!({"point": point, "rows": tied}));
                }
            }
        }
    }
    Ok(json!({"gaps": gaps, "overlaps": overlaps, "unchecked": [], "points": total}))
}
