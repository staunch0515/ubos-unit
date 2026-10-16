//! Helpers of conformance-vector runners (the forge's `conform` and the node's protocol
//! suite): the implementation-dependent members a stateful result is stripped of before an
//! exact comparison, subset matching, references between steps and canonical equality.

use serde_json::{Map, Value};
use std::collections::HashMap;
use ubos_types::canonical;

/// Members of stateful results whose values are implementation-dependent (hashes,
/// signatures, timestamps, cursors) and not compared exactly.
pub const IMPLEMENTATION_DEPENDENT: &[&str] = &[
    "version_hash",
    "entry_hash",
    "root",
    "head",
    "versions",
    "commit_id",
    "tx_time",
    "parents",
    "created_from",
    "evaluated_at",
    "preview",
    "base",
    "source_head",
    "target_head",
    "merged_commit",
    "signature",
    "provenance_ref",
    "inputs_commitment",
    "class_version",
    "signatures",
    "proof",
    "records",
    "heads",
    "archive",
    "from_commit",
    "to_commit",
    "cursor",
    "snapshots",
    "logic_versions",
    "commit_ids",
    "changes_hash",
    "cpu_us",
    "peak_memory",
    "wall_us",
    "asset",
    "record_id",
    "json_schema",
];

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

/// A `label/path` reference into earlier results.
pub fn pointer<'a>(results: &'a HashMap<String, Value>, reference: &str) -> Option<&'a Value> {
    let (label, ptr) = reference.split_once('/').unwrap_or((reference, ""));
    let doc = results.get(label)?;
    if ptr.is_empty() {
        Some(doc)
    } else {
        doc.pointer(&format!("/{ptr}"))
    }
}

/// Replace `{"$ref": "label/path"}` members by earlier results.
pub fn resolve(v: &Value, results: &HashMap<String, Value>) -> Result<Value, String> {
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

/// Canonical-form equality (STD-FND-003).
pub fn canonical_eq(a: &Value, b: &Value) -> bool {
    matches!((canonical(a), canonical(b)), (Ok(x), Ok(y)) if x == y)
}
