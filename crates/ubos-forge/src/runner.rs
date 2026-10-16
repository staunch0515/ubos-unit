//! Vector execution and comparison (STD-CONF-001, STD-CONF-005).

use crate::json::parse_strict;
use serde_json::{Value, json};
use std::collections::HashMap;
use ubos_dvm::{Adapter, BpaError, STATEFUL_OPERATIONS};

/// Members removed before an exact comparison of a stateful result (STD-CONF-005, rule 6,
/// as of 1.0-rc1).
const UNSUPPORTED: &str = "ADAPTER.UNSUPPORTED";

pub use ubos_dvm::vectors::{
    IMPLEMENTATION_DEPENDENT, canonical_eq, pointer, resolve, strip, subset,
};

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

/// Run one vector on the adapter after resetting it.
pub fn run_vector(adapter: &mut Adapter, vector: &Value, tag: Option<&str>) -> Outcome {
    if let Some(t) = tag {
        let tagged = vector["tags"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(t)));
        if !tagged {
            return Outcome::NotApplicable(format!("not tagged {t}"));
        }
    }
    if let Err(e) = adapter.execute("reset", &json!({})) {
        return Outcome::Fail(format!("reset: {e}"));
    }
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
                // a vector selected by tag must be executable: unsupported is then a failure
                let detail = format!("{label}: {}", e.message);
                return if tag.is_some() {
                    Outcome::Fail(detail)
                } else {
                    Outcome::NotApplicable(detail)
                };
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

/// The vectors of a released BPA version that the release in development supersedes
/// (`superseded.txt` next to the vectors, DEC-067): kept as released, not run.
pub fn superseded(dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    std::fs::read_to_string(dir.join("superseded.txt"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}
