//! Schema evolution in the kernel (PH-2 step 7a, DEC-127): the computed classification of
//! class changes (STD-CLS-050), lenses of the nine declarative operations (STD-CLS-060),
//! their application to reads, writes and merges (STD-CLS-061) and the totality check
//! (STD-CLS-062) of BPA 1.0-rc8 (DSN-DVM-120…124).
//!
//! A class change (`change_class`) records its step in `kernel_meta` as
//! `schema_step:<class>:<to>`: the classification, the lens and the effective fields before
//! and after. Bodies stay stored in the schema version they were written in; a read, an
//! update's base and a merge see them through the lenses of the steps.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use ubos_types::{BpaError, canonical};
use ubos_version::state::Repo;

pub(crate) const LENS: &str = "Lens";
const ARCHIVE: &str = "_archived";
const ORDER: &[&str] = &["additive", "lens_required", "breaking"];
const SAMPLE: usize = 10_000;
const MAX_FAILURES: usize = 10;
const ANNOTATIONS: &[&str] = &["label", "description"];
/// STD-CLS-060: the parameters of each operation.
const OPS: &[(&str, &[&str])] = &[
    ("rename", &["from", "to"]),
    ("split", &["from", "to", "forward", "backward"]),
    ("join", &["from", "to", "forward", "backward"]),
    (
        "convert",
        &[
            "field",
            "from_type",
            "to_type",
            "forward_expr",
            "backward_expr",
        ],
    ),
    ("default", &["field", "value"]),
    ("nest", &["fields", "into"]),
    ("unnest", &["from", "fields"]),
    ("map_enum", &["field", "mapping"]),
    ("drop_with_archive", &["field"]),
];

type Body = Map<String, Value>;
type Fields = Map<String, Value>;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(what: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", what.to_string())
}

fn incomplete(what: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.to_string())
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(x)) => !x.is_empty(),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn not_null<'a>(b: &'a Body, k: &str) -> Option<&'a Value> {
    b.get(k).filter(|v| !v.is_null())
}

/// DAT-Lens: the operations and their parameters; a lens whose backward mapping is not
/// total is declared one-way with a reason.
pub fn check_lens(lens: &Value) -> Result<(), BpaError> {
    if !lens.is_object() {
        return Err(invalid("a lens is a map"));
    }
    for k in ["class", "from", "to", "ops"] {
        if lens.get(k).is_none_or(Value::is_null) {
            return Err(incomplete(&format!("lens member {k}")));
        }
    }
    let from = lens["from"].as_i64().filter(|f| *f >= 1);
    if from.is_none() || lens["to"].as_i64() != from.map(|f| f + 1) {
        return Err(invalid("a lens maps schema version from to from + 1"));
    }
    if lens.get("residual_forward").is_some_and(|v| !v.is_null())
        || lens.get("residual_backward").is_some_and(|v| !v.is_null())
    {
        return Err(BpaError::new(
            "LOGIC.NOT_AVAILABLE",
            "L2 residual functions of lenses come after BPA 1.0-rc8",
        ));
    }
    let Some(ops) = lens["ops"].as_array().filter(|o| !o.is_empty()) else {
        return Err(incomplete("a lens has operations"));
    };
    let mut needs_one_way = false;
    for op in ops {
        let Some((k, params)) = s(op, "op").and_then(|k| OPS.iter().find(|(n, _)| *n == k)) else {
            let names: Vec<&str> = OPS.iter().map(|(n, _)| *n).collect();
            return Err(invalid(&format!(
                "a lens operation is one of {}",
                names.join(", ")
            )));
        };
        for p in *params {
            if op.get(*p).is_none() {
                return Err(incomplete(&format!("{k} needs {p}")));
            }
        }
        if matches!(*k, "split" | "join") {
            let (many, exprs) = if *k == "split" {
                (&op["to"], &op["forward"])
            } else {
                (&op["from"], &op["backward"])
            };
            let ok = matches!((many.as_array(), exprs.as_array()),
                (Some(m), Some(e)) if !m.is_empty() && m.len() == e.len());
            if !ok {
                return Err(invalid(&format!("{k} gives one expression per part")));
            }
        }
        if matches!(*k, "nest" | "unnest") && op["fields"].as_array().is_none_or(|f| f.is_empty()) {
            return Err(invalid(&format!("{k} names its fields")));
        }
        if *k == "map_enum" {
            let Some(m) = op["mapping"].as_object().filter(|m| !m.is_empty()) else {
                return Err(invalid("a mapping is a map of values"));
            };
            let mut vals: Vec<String> = m.values().map(canonical).collect::<Result<_, _>>()?;
            vals.sort();
            vals.dedup();
            if vals.len() != m.len() {
                needs_one_way = true;
            }
        }
    }
    if needs_one_way && !truthy(lens.get("one_way")) {
        return Err(invalid(
            "a mapping that is not injective makes the lens one-way",
        ));
    }
    if truthy(lens.get("one_way")) && !truthy(lens.get("one_way_reason")) {
        return Err(incomplete("a one-way lens states one_way_reason"));
    }
    Ok(())
}

/// An L1 expression over the members of a body typed by `types`.
fn x(text: &Value, types: &Fields, body: &Body) -> Result<Value, BpaError> {
    let mut params = Map::new();
    for (k, v) in body {
        if types.contains_key(k) && !v.is_null() {
            params.insert(k.clone(), v.clone());
        }
    }
    let out = ubos_expr::eval_expr(
        &json!({"expr": text, "params_types": types, "params": params}),
        None,
    )?;
    Ok(out["value"].clone())
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

fn typed(field: &Value, type_: &Value) -> Fields {
    let mut t = Map::new();
    t.insert(
        field.as_str().unwrap_or_default().to_string(),
        json!({"name": field, "type": type_}),
    );
    t
}

fn op_forward(op: &Value, mut b: Body, types: &Fields) -> Result<Body, BpaError> {
    let k = s(op, "op").unwrap_or_default();
    let name = |p: &str| s(op, p).unwrap_or_default().to_string();
    match k {
        "rename" => {
            if let Some(v) = b.remove(&name("from")) {
                b.insert(name("to"), v);
            }
        }
        "split" => {
            if not_null(&b, &name("from")).is_some() {
                let mut vals = Vec::new();
                for e in op["forward"].as_array().into_iter().flatten() {
                    vals.push(x(e, types, &b)?);
                }
                b.remove(&name("from"));
                for (t, v) in names(&op["to"]).into_iter().zip(vals) {
                    b.insert(t, v);
                }
            }
        }
        "join" => {
            let from = names(&op["from"]);
            if from.iter().all(|f| not_null(&b, f).is_some()) {
                let v = x(&op["forward"], types, &b)?;
                for f in &from {
                    b.remove(f);
                }
                b.insert(name("to"), v);
            }
        }
        "convert" => {
            let f = name("field");
            if not_null(&b, &f).is_some() {
                let v = x(
                    &op["forward_expr"],
                    &typed(&op["field"], &op["from_type"]),
                    &b,
                )?;
                b.insert(f, v);
            }
        }
        "default" => {
            let f = name("field");
            if not_null(&b, &f).is_none() {
                b.insert(f, op["value"].clone());
            }
        }
        "nest" => {
            let mut inner = Map::new();
            for f in names(&op["fields"]) {
                if let Some(v) = b.remove(&f) {
                    inner.insert(f, v);
                }
            }
            if !inner.is_empty() {
                b.insert(name("into"), Value::Object(inner));
            }
        }
        "unnest" => {
            if let Some(Value::Object(inner)) = b.remove(&name("from")) {
                for f in names(&op["fields"]) {
                    if let Some(v) = inner.get(&f) {
                        b.insert(f, v.clone());
                    }
                }
            }
        }
        "map_enum" => {
            let f = name("field");
            if let Some(to) = b
                .get(&f)
                .and_then(Value::as_str)
                .and_then(|v| op["mapping"].get(v))
                .cloned()
            {
                b.insert(f, to);
            }
        }
        "drop_with_archive" => {
            let f = name("field");
            if let Some(v) = b.remove(&f) {
                let arch = b.entry(ARCHIVE.to_string()).or_insert_with(|| json!({}));
                if !arch.is_object() {
                    *arch = json!({});
                }
                arch[&f] = v;
            }
        }
        _ => {}
    }
    Ok(b)
}

fn op_backward(op: &Value, mut b: Body, types: &Fields) -> Result<Body, BpaError> {
    let k = s(op, "op").unwrap_or_default();
    let name = |p: &str| s(op, p).unwrap_or_default().to_string();
    match k {
        "rename" => {
            if let Some(v) = b.remove(&name("to")) {
                b.insert(name("from"), v);
            }
        }
        "split" => {
            let to = names(&op["to"]);
            if to.iter().all(|f| not_null(&b, f).is_some()) {
                let v = x(&op["backward"], types, &b)?;
                for f in &to {
                    b.remove(f);
                }
                b.insert(name("from"), v);
            }
        }
        "join" => {
            if not_null(&b, &name("to")).is_some() {
                let mut vals = Vec::new();
                for e in op["backward"].as_array().into_iter().flatten() {
                    vals.push(x(e, types, &b)?);
                }
                b.remove(&name("to"));
                for (f, v) in names(&op["from"]).into_iter().zip(vals) {
                    b.insert(f, v);
                }
            }
        }
        "convert" => {
            let f = name("field");
            if not_null(&b, &f).is_some() {
                let v = x(
                    &op["backward_expr"],
                    &typed(&op["field"], &op["to_type"]),
                    &b,
                )?;
                b.insert(f, v);
            }
        }
        "default" => {
            let f = name("field");
            if let Some(v) = b.get(&f)
                && canonical(v)? == canonical(&op["value"])?
            {
                b.remove(&f);
            }
        }
        "nest" => {
            if let Some(Value::Object(inner)) = b.remove(&name("into")) {
                for f in names(&op["fields"]) {
                    if let Some(v) = inner.get(&f) {
                        b.insert(f, v.clone());
                    }
                }
            }
        }
        "unnest" => {
            let mut inner = Map::new();
            for f in names(&op["fields"]) {
                if let Some(v) = b.remove(&f) {
                    inner.insert(f, v);
                }
            }
            if !inner.is_empty() {
                b.insert(name("from"), Value::Object(inner));
            }
        }
        "map_enum" => {
            let f = name("field");
            if let Some(v) = not_null(&b, &f).cloned() {
                let cv = canonical(&v)?;
                let mut inv = None;
                for (from, to) in op["mapping"].as_object().into_iter().flatten() {
                    if canonical(to)? == cv {
                        inv = Some(json!(from));
                    }
                }
                if let Some(back) = inv {
                    b.insert(f, back);
                }
            }
        }
        "drop_with_archive" => {
            let f = name("field");
            let mut empty = false;
            let mut restored = None;
            if let Some(Value::Object(arch)) = b.get_mut(ARCHIVE)
                && let Some(v) = arch.remove(&f)
            {
                restored = Some(v);
                empty = arch.is_empty();
            }
            if let Some(v) = restored {
                b.insert(f, v);
                if empty {
                    b.remove(ARCHIVE);
                }
            }
        }
        _ => {}
    }
    Ok(b)
}

fn object(v: &Value) -> Body {
    v.as_object().cloned().unwrap_or_default()
}

/// A body through a lens forward.
pub fn lens_forward(
    lens: &Value,
    body: &Body,
    old: &Fields,
    new: &Fields,
) -> Result<Body, BpaError> {
    let mut types = new.clone();
    for (k, v) in old {
        types.insert(k.clone(), v.clone());
    }
    let mut b = body.clone();
    for op in lens["ops"].as_array().into_iter().flatten() {
        b = op_forward(op, b, &types)?;
    }
    Ok(b)
}

/// A body through a lens backward; a one-way lens refuses.
pub fn lens_backward(
    lens: &Value,
    body: &Body,
    old: &Fields,
    new: &Fields,
) -> Result<Body, BpaError> {
    if truthy(lens.get("one_way")) {
        return Err(BpaError::with_details(
            "MODEL.LENS_REQUIRED",
            json!({"one_way": lens.get("one_way_reason").cloned().unwrap_or(Value::Null)}),
        ));
    }
    let mut types = old.clone();
    for (k, v) in new {
        types.insert(k.clone(), v.clone());
    }
    let mut b = body.clone();
    for op in lens["ops"].as_array().into_iter().flatten().rev() {
        b = op_backward(op, b, &types)?;
    }
    Ok(b)
}

fn fields_of(decl: &Value) -> Fields {
    let mut out = Map::new();
    for f in decl["fields"].as_array().into_iter().flatten() {
        if let Some(n) = s(f, "name") {
            out.insert(n.to_string(), f.clone());
        }
    }
    out
}

fn satisfies(f: &Value, v: Option<&Value>) -> bool {
    match v.filter(|v| !v.is_null()) {
        None => !truthy(f.get("required")),
        Some(v) => ubos_types::values::validate_value(
            s(f, "type").unwrap_or_default(),
            f.get("constraints").filter(|c| !c.is_null()),
            v,
        )
        .is_ok(),
    }
}

/// Why a body does not conform to the fields of a schema version (empty when it does).
fn body_problems(fields: &Fields, body: &Body) -> Vec<Value> {
    let mut out = Vec::new();
    for (k, v) in body {
        if k == ARCHIVE {
            continue;
        }
        match fields.get(k) {
            None => out.push(json!({"field": k, "problem": "undeclared"})),
            Some(f) if !satisfies(f, Some(v)) => {
                out.push(json!({"field": k, "problem": "invalid"}));
            }
            _ => {}
        }
    }
    for (k, f) in fields {
        if truthy(f.get("required")) && not_null(body, k).is_none() {
            out.push(json!({"field": k, "problem": "missing"}));
        }
    }
    out
}

/// Generated values per field type (STD-CLS-062); values a field's constraints refuse are
/// skipped.
fn generated(f: &Value) -> Vec<Value> {
    let t = s(f, "type").unwrap_or_default();
    let vals: Vec<Value> = match t {
        "enum" => f["constraints"]["allowed_values"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        "text" => vec![json!(""), json!("a"), json!("Zürich 1")],
        "integer" => vec![
            json!(0),
            json!(1),
            json!(-7),
            json!(9_007_199_254_740_991_i64),
        ],
        "decimal" => vec![
            json!("0"),
            json!("1.5"),
            json!("-2.25"),
            json!("1000000.01"),
        ],
        "boolean" => vec![json!(true), json!(false)],
        "date" => vec![json!("1970-01-01"), json!("2026-02-28")],
        "instant" => vec![json!("2026-01-01T00:00:00Z")],
        "money" => vec![
            json!({"amount": "0.00", "currency": "USD"}),
            json!({"amount": "12.34", "currency": "EUR"}),
            json!({"amount": "-5", "currency": "JPY"}),
        ],
        "json" => vec![json!({}), json!({"a": [1, "b"]})],
        _ => Vec::new(),
    };
    vals.iter()
        .filter_map(|v| {
            ubos_types::values::validate_value(t, f.get("constraints").filter(|c| !c.is_null()), v)
                .ok()
        })
        .collect()
}

fn affected(lens: &Value, old: &Fields) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for op in lens["ops"].as_array().into_iter().flatten() {
        for p in ["from", "field", "fields"] {
            let v = &op[p];
            let list: Vec<&Value> = match v.as_array() {
                Some(a) => a.iter().collect(),
                None => vec![v],
            };
            for n in list.into_iter().filter_map(Value::as_str) {
                if old.contains_key(n) && !out.iter().any(|x| x == n) {
                    out.push(n.to_string());
                }
            }
        }
    }
    out
}

/// Bodies of the source schema version that vary each field a lens touches over the
/// generated values of its type, the other fields set to their first generated value.
fn generated_bodies(lens: &Value, old: &Fields) -> Result<(Vec<Body>, Vec<String>), BpaError> {
    let mut base = Map::new();
    for (n, f) in old {
        match generated(f).into_iter().next() {
            Some(v) => {
                base.insert(n.clone(), v);
            }
            None if truthy(f.get("required")) => return Ok((Vec::new(), vec![n.clone()])),
            None => {}
        }
    }
    let mut out: Vec<Body> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut ungenerated = Vec::new();
    for n in affected(lens, old) {
        let f = &old[&n];
        let vals = generated(f);
        if vals.is_empty() {
            ungenerated.push(n.clone());
        }
        let mut all: Vec<Option<Value>> = vals.into_iter().map(Some).collect();
        if !truthy(f.get("required")) {
            all.push(None);
        }
        for v in all {
            let mut b = base.clone();
            match v {
                None => {
                    b.remove(&n);
                }
                Some(v) => {
                    b.insert(n.clone(), v);
                }
            }
            let c = canonical(&Value::Object(b.clone()))?;
            if !seen.contains(&c) {
                seen.push(c);
                out.push(b);
            }
        }
    }
    if !seen.contains(&canonical(&Value::Object(base.clone()))?) {
        out.insert(0, base);
    }
    Ok((out, ungenerated))
}

/// STD-CLS-062: round trips backward(forward(x)) = x and conformance of forward(x) for
/// generated values and the stored current versions; failures carry the sample values.
fn totality(lens: &Value, old: &Fields, new: &Fields, stored: &[Body]) -> Result<Value, BpaError> {
    let (gen_, ungenerated) = generated_bodies(lens, old)?;
    let stored: Vec<Body> = stored.iter().take(SAMPLE).cloned().collect();
    let mut failures = Vec::new();
    let mut any = false;
    for (source, bodies) in [("generated", &gen_), ("stored", &stored)] {
        for xb in bodies {
            let mut fail = json!({"source": source, "value": xb});
            let r = (|| -> Result<(), BpaError> {
                let fx = lens_forward(lens, xb, old, new)?;
                let probs = body_problems(new, &fx);
                if !probs.is_empty() {
                    fail["reason"] = json!("forward_invalid");
                    fail["forward"] = json!(fx);
                    fail["problems"] = json!(probs);
                } else if !truthy(lens.get("one_way")) {
                    let bx = lens_backward(lens, &fx, old, new)?;
                    if canonical(&Value::Object(bx.clone()))?
                        != canonical(&Value::Object(xb.clone()))?
                    {
                        fail["reason"] = json!("round_trip");
                        fail["forward"] = json!(fx);
                        fail["backward"] = json!(bx);
                    }
                }
                Ok(())
            })();
            if let Err(e) = r {
                fail["reason"] = json!("error");
                fail["code"] = json!(e.code);
            }
            if fail.get("reason").is_some() {
                any = true;
                if failures.len() < MAX_FAILURES {
                    failures.push(fail);
                }
            }
        }
    }
    let mut out = json!({"total": !any, "generated": gen_.len(), "stored": stored.len(),
                         "failures": failures});
    if !ungenerated.is_empty() {
        out["ungenerated"] = json!(ungenerated);
    }
    Ok(out)
}

fn strip(f: &Value, keys: &[&str]) -> Value {
    let mut m = object(f);
    for k in keys {
        m.remove(*k);
    }
    Value::Object(m)
}

/// STD-CLS-050: the computed classification of a class change. With a lens, constraints
/// are checked on the stored versions mapped forward.
fn classify(
    old: &Value,
    new: &Value,
    old_f: &Fields,
    new_f: &Fields,
    stored: &[Body],
    lens: Option<&Value>,
) -> Result<Value, BpaError> {
    let mut changes: Vec<Value> = Vec::new();
    let mut mapped: Vec<Body> = stored.to_vec();
    if let Some(l) = lens {
        check_lens(l)?;
        mapped = stored
            .iter()
            .map(|xb| lens_forward(l, xb, old_f, new_f).unwrap_or_else(|_| xb.clone()))
            .collect();
    }
    let mut add = |change: &str, cls: &str, extra: Value| {
        let mut c = json!({"change": change, "classification": cls});
        if let (Some(m), Some(e)) = (c.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                m.insert(k.clone(), v.clone());
            }
        }
        changes.push(c);
    };
    let mut names: Vec<&String> = old_f.keys().chain(new_f.keys()).collect();
    names.sort();
    names.dedup();
    for n in names {
        match (old_f.get(n), new_f.get(n)) {
            (None, Some(b)) => {
                let req = truthy(b.get("required")) && b.get("default").is_none_or(Value::is_null);
                add(
                    "field_added",
                    if req { "lens_required" } else { "additive" },
                    json!({"field": n}),
                );
            }
            (Some(_), None) => add("field_removed", "lens_required", json!({"field": n})),
            (Some(a), Some(b)) => {
                let cons = |f: &Value| {
                    f.get("constraints")
                        .filter(|c| !c.is_null() && c != &&json!({}))
                        .cloned()
                        .unwrap_or(json!({}))
                };
                if a["type"] != b["type"] {
                    add("type_changed", "lens_required", json!({"field": n}));
                } else if cons(a) != cons(b)
                    || truthy(a.get("required")) != truthy(b.get("required"))
                {
                    let bad = mapped.iter().filter(|xb| !satisfies(b, xb.get(n))).count();
                    let raw = stored.iter().filter(|xb| !satisfies(b, xb.get(n))).count();
                    if bad > 0 {
                        add(
                            "constraint_changed",
                            "breaking",
                            json!({"field": n, "violations": bad}),
                        );
                    } else if raw > 0 {
                        add(
                            "constraint_changed",
                            "lens_required",
                            json!({"field": n, "mapped": raw}),
                        );
                    } else {
                        add("constraint_changed", "additive", json!({"field": n}));
                    }
                } else if strip(a, &["label", "description", "default"])
                    != strip(b, &["label", "description", "default"])
                {
                    add("field_changed", "breaking", json!({"field": n}));
                } else if a != b {
                    add("annotation", "additive", json!({"field": n}));
                }
            }
            (None, None) => {}
        }
    }
    let ports = |d: &Value| {
        let mut m = Map::new();
        for p in d["ports"].as_array().into_iter().flatten() {
            if let Some(n) = s(p, "name") {
                m.insert(n.to_string(), p.clone());
            }
        }
        m
    };
    let (po, pn) = (ports(old), ports(new));
    let mut pnames: Vec<&String> = po.keys().chain(pn.keys()).collect();
    pnames.sort();
    pnames.dedup();
    for n in pnames {
        match (po.get(n), pn.get(n)) {
            (None, Some(_)) => add("port_added", "additive", json!({"port": n})),
            (Some(_), None) => add("port_removed", "breaking", json!({"port": n})),
            (Some(a), Some(b)) if a != b => add("port_changed", "breaking", json!({"port": n})),
            _ => {}
        }
    }
    let get = |d: &Value, k: &str| d.get(k).cloned().unwrap_or(Value::Null);
    for m in ["kind", "parent", "namespace", "abstract"] {
        if get(old, m) != get(new, m) {
            add(&format!("{m}_changed"), "breaking", json!({}));
        }
    }
    let skip = [
        "fields",
        "ports",
        "kind",
        "parent",
        "namespace",
        "abstract",
        "schema_version",
        "name",
    ];
    let mut rest: Vec<String> = object(old)
        .keys()
        .chain(object(new).keys())
        .filter(|k| !skip.contains(&k.as_str()))
        .cloned()
        .collect();
    rest.sort();
    rest.dedup();
    for k in rest {
        if get(old, &k) != get(new, &k) {
            if ANNOTATIONS.contains(&k.as_str()) {
                add("annotation", "additive", json!({"member": k}));
            } else {
                add("member_changed", "breaking", json!({"member": k}));
            }
        }
    }
    let level = changes
        .iter()
        .filter_map(|c| ORDER.iter().position(|o| c["classification"] == json!(o)))
        .max()
        .unwrap_or(0);
    let mut out = json!({"classification": ORDER[level], "changes": changes});
    if let Some(l) = lens
        && level < 2
    {
        check_lens(l)?;
        let t = totality(l, old_f, new_f, stored)?;
        if t["total"] != json!(true) {
            out["classification"] = json!("breaking");
        }
        out["totality"] = t;
    }
    Ok(out)
}

fn bodies(v: &Value) -> Vec<Body> {
    v.as_array().into_iter().flatten().map(object).collect()
}

/// Adapter operation `classify_change` (STD-CLS-050, pure form): `from` and `to` are class
/// declarations with their own fields; `stored` are bodies of current versions.
pub fn classify_change(i: &Value) -> Result<Value, BpaError> {
    let (old, new) = (&i["from"], &i["to"]);
    if !old.is_object() || !new.is_object() {
        return Err(incomplete("from and to are class declarations"));
    }
    let lens = i.get("lens").filter(|l| !l.is_null());
    classify(
        old,
        new,
        &fields_of(old),
        &fields_of(new),
        &bodies(&i["stored"]),
        lens,
    )
}

/// Adapter operation `apply_lens` (STD-CLS-061, pure form for one lens).
pub fn apply_lens(i: &Value) -> Result<Value, BpaError> {
    let lens = &i["lens"];
    check_lens(lens)?;
    let (old, new) = (fields_of(&i["from"]), fields_of(&i["to"]));
    let body = object(&i["body"]);
    match s(i, "direction").unwrap_or("forward") {
        "forward" => Ok(json!({"body": lens_forward(lens, &body, &old, &new)?})),
        "backward" => Ok(json!({"body": lens_backward(lens, &body, &old, &new)?})),
        _ => Err(invalid("direction is forward or backward")),
    }
}

/// Adapter operation `check_lens` (STD-CLS-062, pure form).
pub fn lens_check(i: &Value) -> Result<Value, BpaError> {
    let lens = &i["lens"];
    check_lens(lens)?;
    totality(
        lens,
        &fields_of(&i["from"]),
        &fields_of(&i["to"]),
        &bodies(&i["stored"]),
    )
}

/// The lenses a commit writes are checked (STD-CLS-060).
pub(crate) fn check(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    for (_, vh) in written {
        let v = repo.version_in(tx, vh)?;
        if s(&v, "class") == Some(LENS) && v["tombstone"] != json!(true) {
            check_lens(&v["body"])?;
        }
    }
    Ok(())
}

fn step_key(class: &str, to: i64) -> String {
    format!("schema_step:{class}:{to}")
}

/// A body stored at schema version `s` in schema version `t`: the lenses s→s+1→…→t forward,
/// or backward for t < s; a step without a lens was additive.
pub(crate) fn lens_body(
    repo: &mut Repo,
    class: &str,
    body: &Body,
    s_: i64,
    t: i64,
) -> Result<Body, BpaError> {
    let cur = repo.reg.class(class)?.schema_version;
    if t < 1 || t > cur {
        return Err(invalid("schema_version is a schema version of the class"));
    }
    let mut b = body.clone();
    let mut v = s_;
    while v < t {
        if let Some(step) = repo.store.meta(&step_key(class, v + 1))?
            && step["lens"].is_object()
        {
            b = lens_forward(
                &step["lens"],
                &b,
                &object(&step["from_fields"]),
                &object(&step["to_fields"]),
            )?;
        }
        v += 1;
    }
    while v > t {
        if let Some(step) = repo.store.meta(&step_key(class, v))? {
            if step["lens"].is_object() {
                b = lens_backward(
                    &step["lens"],
                    &b,
                    &object(&step["from_fields"]),
                    &object(&step["to_fields"]),
                )?;
            } else {
                let from = object(&step["from_fields"]);
                b.retain(|k, _| from.contains_key(k) || k == ARCHIVE);
            }
        }
        v -= 1;
    }
    Ok(b)
}

/// The body of a version record in its class's current schema version (STD-CLS-061).
pub fn current_body(repo: &mut Repo, v: &Value) -> Result<Body, BpaError> {
    let body = object(&v["body"]);
    let class = s(v, "class").unwrap_or_default();
    let Some(sv) = v["schema_version"].as_i64() else {
        return Ok(body);
    };
    let Ok(c) = repo.reg.class(class) else {
        return Ok(body);
    };
    if c.schema_version == sv {
        return Ok(body);
    }
    let cur = c.schema_version;
    lens_body(repo, class, &body, sv, cur)
}

/// STD-CLS-061: a read in the schema version asked for (default: the current one); the
/// result is not stored.
pub(crate) fn lens_read(
    repo: &mut Repo,
    out: &mut Value,
    v: &Value,
    input: &Value,
) -> Result<(), BpaError> {
    let Some(sv) = v["schema_version"].as_i64() else {
        return Ok(());
    };
    let class = s(v, "class").unwrap_or_default();
    let Ok(c) = repo.reg.class(class) else {
        return Ok(());
    };
    let cur = c.schema_version;
    let asked = input.get("schema_version").filter(|x| !x.is_null());
    let t = match asked {
        None => cur,
        Some(x) => match x.as_i64() {
            Some(t) if !x.is_boolean() => t,
            _ => return Err(invalid("schema_version is a schema version of the class")),
        },
    };
    if sv != t {
        out["body"] = Value::Object(lens_body(repo, class, &object(&v["body"]), sv, t)?);
    }
    if sv != t || asked.is_some() {
        out["schema_version"] = json!(t);
    }
    Ok(())
}

/// STD-CLS-061: a change written in an older schema version is lensed forward before it is
/// stored; an update's patch applies to its base seen in that version.
pub(crate) fn lens_change(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    ch: &Value,
) -> Result<Value, BpaError> {
    let sv = ch["schema_version"]
        .as_i64()
        .ok_or_else(|| invalid("schema_version is a schema version of the class"))?;
    let mut out = object(ch);
    out.remove("schema_version");
    let body = if s(ch, "op") == Some("create") {
        let class = s(ch, "class").unwrap_or_default().to_string();
        let cur = repo.reg.class(&class)?.schema_version;
        lens_body(repo, &class, &object(&ch["body"]), sv, cur)?
    } else {
        let oid = s(ch, "object_id").unwrap_or_default();
        let Some(o) = repo.obj(tx, oid)? else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        };
        let cur = repo.reg.class(&o.class)?.schema_version;
        let head = repo.version_in(tx, &o.head)?;
        let current = current_body(repo, &head)?;
        let mut seen = lens_body(repo, &o.class, &current, cur, sv)?;
        for (k, v) in object(&ch["body"]) {
            seen.insert(k, v);
        }
        lens_body(repo, &o.class, &seen, sv, cur)?
    };
    out.insert("body".into(), Value::Object(body));
    Ok(Value::Object(out))
}

/// The aligner of merges (STD-CLS-061): a version's body in the current schema version.
fn align(repo: &mut Repo, v: &Value) -> Result<Body, BpaError> {
    current_body(repo, v)
}

/// Install the aligner of merges once per process.
pub(crate) fn install() {
    ubos_version::state::set_aligner(align);
}

impl Engine {
    /// The current versions of a class at its current schema version, sampled in object
    /// order (STD-CLS-062).
    fn stored_bodies(&mut self, branch: &str, class: &str) -> Result<Vec<Body>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let cseq = repo.ordinal(branch, None)?;
        let mut objs: Vec<(String, String)> = repo
            .objects_in(branch, cseq, Some(class))?
            .into_iter()
            .filter(|(_, o)| o.class == class)
            .map(|(id, o)| (id, o.head))
            .collect();
        objs.sort();
        let mut out = Vec::new();
        for (_, head) in objs {
            let v = repo.version(&head)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            out.push(current_body(&mut repo, &v)?);
            if out.len() == SAMPLE {
                break;
            }
        }
        Ok(out)
    }

    /// Adapter operation `change_class` (STD-CLS-050): change a class declaration by one
    /// schema version. The change is classified by the implementation; a lens-required
    /// change needs a total lens, a breaking change is refused.
    pub fn change_class(&mut self, i: &Value) -> Result<Value, BpaError> {
        let new = &i["class"];
        let name = s(new, "name").unwrap_or_default().to_string();
        let declared = new.is_object() && self.registry.decl(&name).is_ok();
        if !declared {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                "a change names a declared class",
            ));
        }
        let old = self.registry.decl(&name)?.clone();
        let cur = self.registry.class(&name)?.schema_version;
        if new["schema_version"].as_i64() != Some(cur + 1) || new["schema_version"].is_boolean() {
            return Err(invalid("a class change increments schema_version by one"));
        }
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let lens = match i.get("lens").filter(|l| !l.is_null()) {
            None => None,
            Some(r) => {
                let (_, head) = self.wf_ref(&branch, r, LENS)?;
                let l =
                    Repo::new(self.store.as_mut(), &self.registry).version(&head)?["body"].clone();
                let uri = format!("ubos://{}/Class/{name}", self.store.authority());
                let mine = l["class"] == json!(name) || l["class"] == json!(uri);
                if !mine || l["from"].as_i64() != Some(cur) {
                    return Err(invalid(
                        "the lens maps this class from its current schema version",
                    ));
                }
                Some(l)
            }
        };
        let old_f: Fields = self
            .registry
            .class(&name)?
            .fields
            .clone()
            .into_iter()
            .collect();
        let next = self.registry.load(std::slice::from_ref(new), None)?;
        let new_f: Fields = next.class(&name)?.fields.clone().into_iter().collect();
        let stored = self.stored_bodies(&branch, &name)?;
        let res = classify(&old, new, &old_f, &new_f, &stored, lens.as_ref())?;
        let mut out = json!({"class": name, "from": cur, "to": cur + 1,
                             "classification": res["classification"], "changes": res["changes"]});
        if let Some(t) = res.get("totality") {
            out["totality"] = t.clone();
        }
        if res["classification"] == "breaking" {
            return Err(BpaError::with_details("MODEL.MIGRATION_REQUIRED", out));
        }
        if res["classification"] == "lens_required" && lens.is_none() {
            let req: Vec<Value> = res["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["classification"] == "lens_required")
                .cloned()
                .collect();
            return Err(BpaError::with_details(
                "MODEL.LENS_REQUIRED",
                json!({ "changes": req }),
            ));
        }
        self.load_genesis(&json!([new]), None)?;
        let to_f: Fields = self
            .registry
            .class(&name)?
            .fields
            .clone()
            .into_iter()
            .collect();
        self.store.put_meta(
            &step_key(&name, cur + 1),
            &json!({"classification": res["classification"], "lens": lens,
                    "from_fields": old_f, "to_fields": to_f}),
        )?;
        Ok(out)
    }
}

// ---- upstream changes (STD-VER-082; BPA 1.0-rc8, PH-2 step 7b) ----

/// Metadata key of a branch's overrides: object ID → the parent-line version the override
/// was based on, or the one last resolved.
fn overrides_key(branch: &str) -> String {
    format!("overrides:{branch}")
}

thread_local! {
    /// The resolution a `resolve_upstream` merge commits: `(branch, object ID, parent
    /// head)`, recorded as the new base in the same commit.
    static RESOLVING: std::cell::RefCell<Option<(String, String, Value)>> =
        const { std::cell::RefCell::new(None) };
}

/// An object of the parent line becomes an override when the branch first commits its own
/// version; the parent version it was based on is recorded with the commit (STD-VER-082).
pub(crate) fn note_overrides(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    let resolving = RESOLVING.with(|r| r.borrow().clone());
    let mut added: Vec<(String, Value)> = Vec::new();
    for (oid, _) in written {
        if let Some(o) = tx.inherited.get(oid) {
            added.push((oid.clone(), json!(o.head)));
        }
    }
    let resolved = resolving.filter(|(b, _, _)| b == &tx.branch);
    if added.is_empty() && resolved.is_none() {
        return Ok(());
    }
    let key = overrides_key(&tx.branch);
    let mut map = repo
        .store
        .meta(&key)?
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let before = map.clone();
    for (oid, base) in added {
        map.entry(oid).or_insert(base);
    }
    if let Some((_, oid, after)) = resolved {
        map.insert(oid, after);
    }
    if map != before {
        tx.marks.push((key, Value::Object(map)));
    }
    Ok(())
}

impl Engine {
    /// The overrides of a branch whose parent-line head differs from the recorded version:
    /// object ID → `{object_id, state, before, after}`.
    fn upstream(&mut self, branch: &str) -> Result<Map<String, Value>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        repo.head(branch)?;
        let map = repo
            .store
            .meta(&overrides_key(branch))?
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let mut out = Map::new();
        if map.is_empty() {
            return Ok(out);
        }
        let pv = repo.parent_line_view(branch)?;
        for (oid, before) in map {
            let after = pv
                .as_ref()
                .and_then(|v| v.state.objects.get(&oid))
                .map(|o| json!(o.head))
                .unwrap_or(Value::Null);
            if after != before {
                out.insert(
                    oid.clone(),
                    json!({"object_id": oid, "state": "upstream_changed",
                           "before": before, "after": after}),
                );
            }
        }
        Ok(out)
    }

    /// Adapter operation `upstream_changes` (STD-VER-082): the overrides of a branch whose
    /// parent-line version changed since the override or its last resolution.
    pub fn upstream_changes(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let changes: Vec<Value> = self.upstream(&branch)?.into_values().collect();
        Ok(json!({ "changes": changes }))
    }

    /// Adapter operation `resolve_upstream` (STD-VER-082): resolve an upstream change by
    /// a three-way merge (base = the parent version at the override) or by
    /// acknowledgement; both record the parent head as the new base.
    pub fn resolve_upstream(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let oid = s(i, "object_id").unwrap_or_default().to_string();
        let mode = s(i, "mode").unwrap_or_default().to_string();
        let mark = self.upstream(&branch)?.remove(&oid).ok_or_else(|| {
            BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "the object has no upstream change",
            )
        })?;
        let (before, after) = (mark["before"].clone(), mark["after"].clone());
        let mut out = json!({"object_id": oid, "mode": mode, "before": before, "after": after});
        if mode == "acknowledge" {
            let key = overrides_key(&branch);
            let mut map = self
                .store
                .meta(&key)?
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            map.insert(oid, after);
            self.store.put_meta(&key, &Value::Object(map))?;
            return Ok(out);
        }
        if mode != "merge" {
            return Err(invalid("mode is merge or acknowledge"));
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let ended = match after.as_str() {
            None => true,
            Some(h) => repo.version(h)?["tombstone"] == json!(true),
        };
        if ended {
            return Err(BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "the parent line ended the object; acknowledge it",
            ));
        }
        let mut tx = repo.begin_view(&branch)?;
        let head = tx
            .state
            .objects
            .get(&oid)
            .map(|o| o.head.clone())
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown object"))?;
        let res: Vec<Value> = i
            .get("resolutions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (merged, conflicts) = repo.merge_object_body(
            &oid,
            before.as_str(),
            after.as_str().unwrap_or_default(),
            &head,
            &res,
            &mut tx,
        )?;
        if !conflicts.is_empty() {
            return Err(BpaError::with_details(
                "VER.MERGE_CONFLICT",
                json!({ "conflicts": conflicts }),
            ));
        }
        let hv = repo.version(&head)?;
        let current = current_body(&mut repo, &hv)?;
        out["body"] = Value::Object(merged.clone());
        if canonical(&Value::Object(merged.clone())) != canonical(&Value::Object(current)) {
            let mut input = json!({"branch": branch, "changes": [
                {"op": "update", "object_id": oid, "base": head, "body": merged}]});
            if let Some(p) = i.get("process").filter(|p| !p.is_null()) {
                input["process"] = p.clone();
            }
            RESOLVING.with(|r| *r.borrow_mut() = Some((branch.clone(), oid.clone(), after)));
            let r = self.commit(&input);
            RESOLVING.with(|r| *r.borrow_mut() = None);
            out["cseq"] = r?["cseq"].clone();
        } else {
            let key = overrides_key(&branch);
            let mut map = self
                .store
                .meta(&key)?
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            map.insert(oid, after);
            self.store.put_meta(&key, &Value::Object(map))?;
        }
        Ok(out)
    }
}
