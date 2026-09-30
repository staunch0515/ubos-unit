//! Vector execution and comparison (STD-CONF-001, STD-CONF-005).

use crate::json::parse_strict;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use ubos_dvm::{Adapter, BpaError, STATEFUL_OPERATIONS, canonical};

const IMPLEMENTATION_DEPENDENT: &[&str] =
    &["version_hash", "entry_hash", "root", "head", "versions"];
const UNSUPPORTED: &str = "ADAPTER.UNSUPPORTED";

/// Outcome of one vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every expectation held.
    Pass,
    /// An expectation failed; the detail says where.
    Fail(String),
    /// The vector lies outside the claimed scope (profile, tag or operation).
    NotApplicable(String),
}

/// Remove implementation-dependent members of a stateful result (STD-CONF-005, rule 6).
pub fn strip(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| !IMPLEMENTATION_DEPENDENT.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), strip(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(strip).collect()),
        other => other.clone(),
    }
}

/// `expected` is a subset of `actual` (STD-CONF-005).
pub fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|x| subset(v, x))),
        (Value::Array(e), Value::Array(a)) => {
            e.len() == a.len() && e.iter().zip(a).all(|(x, y)| subset(x, y))
        }
        (Value::Object(_) | Value::Array(_), _) => false,
        (e, a) => e == a,
    }
}

fn pointer<'a>(results: &'a HashMap<String, Value>, reference: &str) -> Option<&'a Value> {
    let (label, ptr) = reference.split_once('/').unwrap_or((reference, ""));
    let doc = results.get(label)?;
    if ptr.is_empty() {
        Some(doc)
    } else {
        doc.pointer(&format!("/{ptr}"))
    }
}

fn resolve(v: &Value, results: &HashMap<String, Value>) -> Result<Value, String> {
    match v {
        Value::Object(o) if o.len() == 1 && o.contains_key("$ref") => {
            let r = o["$ref"].as_str().unwrap_or_default();
            pointer(results, r)
                .cloned()
                .ok_or_else(|| format!("unresolved reference {r}"))
        }
        Value::Object(o) => Ok(Value::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, results)?)))
                .collect::<Result<Map<_, _>, String>>()?,
        )),
        Value::Array(a) => Ok(Value::Array(
            a.iter()
                .map(|x| resolve(x, results))
                .collect::<Result<_, _>>()?,
        )),
        other => Ok(other.clone()),
    }
}

fn canonical_eq(a: &Value, b: &Value) -> bool {
    matches!((canonical(a), canonical(b)), (Ok(x), Ok(y)) if x == y)
}

/// Run one vector on a fresh adapter.
pub fn run_vector(vector: &Value, tag: Option<&str>) -> Outcome {
    if let Some(t) = tag {
        let tagged = vector["tags"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(t)));
        if !tagged {
            return Outcome::NotApplicable(format!("not tagged {t}"));
        }
    }
    let mut adapter = Adapter::new();
    let mut results: HashMap<String, Value> = HashMap::new();
    for phase in ["setup", "steps"] {
        let steps = vector[phase].as_array().cloned().unwrap_or_default();
        for (n, step) in steps.iter().enumerate() {
            let label = step["label"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}{}", &phase[..1], n + 1));
            let op = step["op"].as_str().unwrap_or_default();
            let out: Result<Value, BpaError> =
                if let Some(t) = step.get("input_text").and_then(Value::as_str) {
                    match parse_strict(t) {
                        Ok(input) => adapter.execute(op, &input),
                        Err(e) => Err(BpaError::new("MODEL.INVALID_VALUE", e)),
                    }
                } else {
                    match resolve(step.get("input").unwrap_or(&json!({})), &results) {
                        Ok(input) => adapter.execute(op, &input),
                        Err(e) => return Outcome::Fail(format!("{label}: {e}")),
                    }
                };
            if let Err(e) = &out
                && e.code == UNSUPPORTED
            {
                return Outcome::NotApplicable(format!("{label}: {}", e.message));
            }
            if let Ok(v) = &out {
                results.insert(label.clone(), v.clone());
            }
            let Some(exp) = step.get("expect") else {
                if let Err(e) = out {
                    return Outcome::Fail(format!("{label}: unexpected error {e}"));
                }
                continue;
            };
            let stateful = STATEFUL_OPERATIONS.contains(&op);
            let problem = if let Some(code) = exp.get("error_code").and_then(Value::as_str) {
                match &out {
                    Err(e) if e.code == code => match exp.get("details") {
                        Some(d) if !e.details.as_ref().is_some_and(|x| subset(d, x)) => {
                            Some(format!("error details differ: {:?}", e.details))
                        }
                        _ => None,
                    },
                    Err(e) => Some(format!("expected {code}, got {e}")),
                    Ok(v) => Some(format!("expected {code}, got result {v}")),
                }
            } else {
                match &out {
                    Err(e) => Some(format!("unexpected error {e}")),
                    Ok(v) => {
                        if let Some(r) = exp.get("same_as").and_then(Value::as_str) {
                            match pointer(&results, r) {
                                Some(x) if x == v => None,
                                _ => Some(format!("not the same as {r}")),
                            }
                        } else if exp["match"] == "subset" {
                            (!subset(&exp["result"], v))
                                .then(|| format!("subset mismatch: got {v}"))
                        } else {
                            let actual = if stateful { strip(v) } else { v.clone() };
                            (!canonical_eq(&exp["result"], &actual))
                                .then(|| format!("mismatch: got {actual}"))
                        }
                    }
                }
            };
            if let Some(p) = problem {
                return Outcome::Fail(format!("{label}: {p}"));
            }
        }
    }
    Outcome::Pass
}
