//! The property registry (STD-LGS-010) with the kernel properties of ABI 1.0 (STD-LGS-011).

use crate::ast::Declaration;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ubos_model::types::error::invalid;
use ubos_model::types::values::{LEVELS, validate_value};
use ubos_model::types::{BpaError, names};

/// Layers in rank order (STD-LGS-022).
pub const LAYERS: &[&str] = &[
    "platform", "industry", "vendor", "tenant", "vae", "unit", "object",
];
/// Enforcement points of a property.
pub const ENFORCEMENTS: &[&str] = &["commit", "action", "read", "view", "retention", "notify"];
/// Combination rules of a property.
pub const COMBINES: &[&str] = &["override", "max", "min", "union", "all_true", "any_true"];

/// A registered property (`DAT-PropertyDecl`).
#[derive(Debug, Clone)]
pub struct Property {
    /// Property name (kernel names are plain, Buk names namespaced).
    pub name: String,
    /// Field declaration `{type, constraints}` of the value.
    pub schema: Value,
    /// Enforcement point.
    pub enforcement: String,
    /// Combination rule.
    pub combine: String,
    /// `kernel` or the Buk namespace.
    pub owner: String,
}

fn enumeration(values: &[&str]) -> Value {
    json!({"type": "enum", "constraints": {"allowed_values": values}})
}

fn map_of(value: Value) -> Value {
    json!({"type": "map", "constraints": {"value_type": value, "key_pattern": "[a-z][a-z0-9_]{0,63}"}})
}

fn severity() -> Value {
    json!({"type": "enum", "required": true, "constraints": {"allowed_values": ["error", "warning", "info"]}})
}

/// STD-LGS-011: the kernel properties of ABI 1.0 as `(name, enforcement, combine, schema)`.
pub fn kernel_properties() -> Vec<Property> {
    let list_of = |item: Value| json!({"type": "list", "constraints": {"item_type": item}});
    let text = json!({"type": "text"});
    let req_text = json!({"type": "text", "required": true});
    let composite =
        |members: Value| json!({"type": "composite", "constraints": {"members": members}});
    let entries: Vec<(&str, &str, &str, Value)> = vec![
        (
            "audit_level",
            "commit",
            "override",
            enumeration(&["basic", "standard", "strict"]),
        ),
        (
            "require_signature",
            "action",
            "override",
            json!({"type": "boolean"}),
        ),
        (
            "approvals",
            "action",
            "max",
            composite(json!({
                "count": {"type": "integer", "required": true, "constraints": {"min": 1}},
                "roles": list_of(text.clone()),
                "order": enumeration(&["any", "sequence"]),
            })),
        ),
        (
            "maker_checker",
            "action",
            "override",
            json!({"type": "boolean"}),
        ),
        (
            "read_only",
            "commit",
            "override",
            json!({"type": "boolean"}),
        ),
        ("field_required", "commit", "union", list_of(text.clone())),
        (
            "field_default",
            "commit",
            "override",
            map_of(json!({"type": "json"})),
        ),
        (
            "field_visibility",
            "view",
            "override",
            map_of(enumeration(&["visible", "hidden", "read_only"])),
        ),
        (
            "mask",
            "read",
            "override",
            map_of(enumeration(&["hide", "partial", "hash"])),
        ),
        (
            "retention",
            "retention",
            "override",
            composite(json!({
                "min": {"type": "duration"}, "max": {"type": "duration"}, "trigger": {"type": "text"},
            })),
        ),
        (
            "classification",
            "commit",
            "max",
            map_of(enumeration(LEVELS)),
        ),
        (
            "notify",
            "notify",
            "union",
            list_of(json!({"type": "json"})),
        ),
        (
            "write_policy",
            "commit",
            "max",
            enumeration(&["direct", "change_set"]),
        ),
        (
            "rate_limit",
            "action",
            "override",
            composite(json!({
                "per": {"type": "enum", "required": true, "constraints": {"allowed_values": ["second", "minute", "hour", "day"]}},
                "count": {"type": "integer", "required": true, "constraints": {"min": 1}},
            })),
        ),
        (
            "validations",
            "commit",
            "union",
            list_of(composite(json!({
                "name": req_text, "rule": req_text, "severity": severity(), "message": text,
            }))),
        ),
        (
            "invariants",
            "commit",
            "union",
            list_of(composite(json!({
                "name": req_text,
                "scope": {"type": "list", "required": true, "constraints": {"item_type": text, "min_items": 1}},
                "rule": req_text, "severity": severity(), "message": text,
            }))),
        ),
        (
            "decisions",
            "action",
            "union",
            list_of(composite(json!({
                "name": req_text, "when": text,
                "outcome": {"type": "enum", "required": true, "constraints": {"allowed_values": ["allow", "deny", "require"]}},
                "steps": list_of(enumeration(&["approval", "signature", "reason", "step_up", "second_person"])),
                "message": text,
            }))),
        ),
    ];
    entries
        .into_iter()
        .map(|(name, enforcement, combine, schema)| Property {
            name: name.into(),
            schema,
            enforcement: enforcement.into(),
            combine: combine.into(),
            owner: "kernel".into(),
        })
        .collect()
}

/// The order of the `max` properties whose values are not numbers.
pub fn order_of(root: &str) -> Option<&'static [&'static str]> {
    match root {
        "classification" => Some(LEVELS),
        "write_policy" => Some(&["direct", "change_set"]),
        _ => None,
    }
}

/// The property registry at a coordinate: the kernel properties plus the live
/// `DAT-PropertyDecl` objects (STD-LGS-010).
#[derive(Debug, Clone)]
pub struct Registry {
    props: BTreeMap<String, Property>,
    /// A hash of the Buk property declarations: two registries with the same fingerprint
    /// normalise declarations identically (the cache of compiled sheets keys on it).
    fingerprint: u64,
}

impl Registry {
    /// The kernel properties plus the given `DAT-PropertyDecl` items; a Buk property is
    /// namespaced and cannot shadow a kernel property (`MODEL.INVALID_VALUE`).
    pub fn new(properties: &[Value]) -> Result<Registry, BpaError> {
        let mut props = BTreeMap::new();
        for p in kernel_properties() {
            props.insert(p.name.clone(), p);
        }
        for d in properties {
            let name = d.get("name").and_then(Value::as_str).unwrap_or_default();
            if !name.contains('.') || !names::matches("namespace", name) {
                return Err(invalid("a Buk property is namespaced"));
            }
            if props.contains_key(name) {
                return Err(invalid(format!("duplicate property {name}")));
            }
            let schema = d.get("schema").cloned().unwrap_or(Value::Null);
            if !schema.is_object() || schema.get("type").is_none() {
                return Err(invalid("a property schema is a field declaration"));
            }
            let enforcement = d
                .get("enforcement")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let combine = d.get("combine").and_then(Value::as_str).unwrap_or_default();
            if !ENFORCEMENTS.contains(&enforcement) || !COMBINES.contains(&combine) {
                return Err(invalid("enforcement or combine rule"));
            }
            let owner = d
                .get("owner")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    name.rsplit_once('.')
                        .map(|(a, _)| a)
                        .unwrap_or(name)
                        .to_string()
                });
            props.insert(
                name.to_string(),
                Property {
                    name: name.to_string(),
                    schema,
                    enforcement: enforcement.into(),
                    combine: combine.into(),
                    owner,
                },
            );
        }
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for d in properties {
            ubos_model::types::canonical(d)?.hash(&mut h);
        }
        Ok(Registry {
            props,
            fingerprint: h.finish(),
        })
    }

    /// The fingerprint of the Buk property declarations.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// A registered property.
    pub fn get(&self, name: &str) -> Option<&Property> {
        self.props.get(name)
    }

    /// Every registered property.
    pub fn properties(&self) -> impl Iterator<Item = &Property> {
        self.props.values()
    }

    /// `(property name, map key)` of a property path: the registered name is the path itself
    /// or its longest registered prefix followed by one key (`RULE.UNKNOWN_PROPERTY`).
    pub fn split_path<'a>(&self, path: &'a str) -> Result<(&'a str, Option<&'a str>), BpaError> {
        if self.props.contains_key(path) {
            return Ok((path, None));
        }
        if let Some((root, key)) = path.rsplit_once('.')
            && !key.is_empty()
            && self.props.contains_key(root)
        {
            return Ok((root, Some(key)));
        }
        Err(BpaError::new("RULE.UNKNOWN_PROPERTY", path))
    }

    /// The combination rule of the property of a path.
    pub fn combine_of(&self, path: &str) -> Result<&str, BpaError> {
        let (root, _) = self.split_path(path)?;
        Ok(&self.props[root].combine)
    }

    /// A declared value validated against the property's schema, as canonical JSON
    /// (STD-LGS-010): a keyed path validates against the map's value type, and an integer
    /// `approvals` is the shorthand of `{count}`.
    pub fn check_declaration(&self, property: &str, value: &Value) -> Result<Value, BpaError> {
        let (root, key) = self.split_path(property)?;
        let schema = &self.props[root].schema;
        let type_ = schema
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if key.is_some() {
            if type_ != "map" {
                return Err(invalid("a dotted path names one key of a map property"));
            }
            let json_t = json!({"type": "json"});
            let vt = schema
                .get("constraints")
                .and_then(|c| c.get("value_type"))
                .filter(|v| v.is_object())
                .unwrap_or(&json_t);
            return validate_value(
                vt.get("type").and_then(Value::as_str).unwrap_or_default(),
                vt.get("constraints"),
                value,
            );
        }
        let shorthand;
        let value = if root == "approvals" && value.is_i64() {
            shorthand = json!({"count": value});
            &shorthand
        } else {
            value
        };
        validate_value(type_, schema.get("constraints"), value)
    }

    /// The `(path, value)` pairs a declaration contributes: a whole-map declaration
    /// contributes one pair per key (DEC-066 item 3).
    pub fn normalise(&self, d: &Declaration) -> Result<Vec<(String, Value)>, BpaError> {
        let (root, key) = self.split_path(&d.property)?;
        let value = self.check_declaration(&d.property, &d.value.to_json())?;
        let is_map = self.props[root].schema.get("type").and_then(Value::as_str) == Some("map");
        if key.is_none() && is_map {
            return Ok(value
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, v)| (format!("{root}.{k}"), v.clone()))
                .collect());
        }
        Ok(vec![(d.property.clone(), value)])
    }

    /// The combined value of a combinable property over the candidate values in position
    /// order (STD-LGS-022).
    pub fn combine(&self, path: &str, values: &[Value]) -> Result<Value, BpaError> {
        let (root, _) = self.split_path(path)?;
        let rule = self.props[root].combine.as_str();
        match rule {
            "max" | "min" => {
                let key = |v: &Value| -> i128 {
                    if let Some(order) = order_of(root) {
                        return order
                            .iter()
                            .position(|x| v.as_str() == Some(x))
                            .map(|p| p as i128)
                            .unwrap_or(-1);
                    }
                    let n = v.get("count").unwrap_or(v);
                    n.as_i64().map(i128::from).unwrap_or(-1)
                };
                let mut best: Option<&Value> = None;
                for v in values {
                    let better = match best {
                        None => true,
                        Some(b) => {
                            if rule == "max" {
                                key(v) > key(b)
                            } else {
                                key(v) < key(b)
                            }
                        }
                    };
                    if better {
                        best = Some(v);
                    }
                }
                best.cloned().ok_or_else(|| invalid("no candidates"))
            }
            "union" => {
                let mut out: Vec<Value> = Vec::new();
                for v in values {
                    for x in v.as_array().into_iter().flatten() {
                        if !out.contains(x) {
                            out.push(x.clone());
                        }
                    }
                }
                Ok(Value::Array(out))
            }
            "all_true" => Ok(Value::Bool(values.iter().all(|v| v == &Value::Bool(true)))),
            "any_true" => Ok(Value::Bool(values.iter().any(|v| v == &Value::Bool(true)))),
            _ => Err(invalid("combine rule")),
        }
    }
}
