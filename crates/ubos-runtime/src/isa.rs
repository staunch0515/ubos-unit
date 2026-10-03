//! The instruction catalogue of ABI 1.0 (STD-ISA-001), the error registry (STD-ERR-002)
//! and the profile table (STD-CTX-004), embedded from `isa.json`, which
//! `standard/tools/gen_isa_json.py` writes from the clause text of the standard so that the
//! kernel and the specification cannot drift.

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use ubos_expr::model::types::BpaError;

const ISA_JSON: &str = include_str!("isa.json");

/// An instruction declaration (DAT-InstructionDecl).
#[derive(Debug, Clone)]
pub struct Instruction {
    /// `family.op`.
    pub name: String,
    /// Family (`commit`, `read`, `run`, `emit`, `search`, `call`, `ask`).
    pub family: String,
    /// ABI version that introduced it (`MAJOR.MINOR`).
    pub since: String,
    /// Parameters: `(name, type, optional)`.
    pub params: Vec<(String, String, bool)>,
    /// Base cost in units.
    pub base_cost: u64,
    /// Cost per item.
    pub per_item: u64,
    /// Required authorization action, or `none`.
    pub authorization: String,
    /// The declaration as `instructions` reports it (without the clause).
    pub decl: Value,
}

/// The limits of a profile (STD-CTX-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Operation units.
    pub units: u64,
    /// Instruction calls.
    pub instruction_calls: u64,
    /// Changes per commit.
    pub changes: u64,
    /// Nesting depth of function calls.
    pub depth: u64,
    /// Connector calls.
    pub connector_calls: u64,
    /// Model calls.
    pub model_calls: u64,
}

impl Limits {
    /// The limit named `k`.
    pub fn get(&self, k: &str) -> Option<u64> {
        Some(match k {
            "units" => self.units,
            "instruction_calls" => self.instruction_calls,
            "changes" => self.changes,
            "depth" => self.depth,
            "connector_calls" => self.connector_calls,
            "model_calls" => self.model_calls,
            _ => return None,
        })
    }

    /// Set the limit named `k`.
    pub fn set(&mut self, k: &str, v: u64) {
        match k {
            "units" => self.units = v,
            "instruction_calls" => self.instruction_calls = v,
            "changes" => self.changes = v,
            "depth" => self.depth = v,
            "connector_calls" => self.connector_calls = v,
            "model_calls" => self.model_calls = v,
            _ => {}
        }
    }

    fn to_json(self) -> Value {
        json!({"units": self.units, "instruction_calls": self.instruction_calls, "changes": self.changes,
            "depth": self.depth, "connector_calls": self.connector_calls, "model_calls": self.model_calls})
    }
}

struct Catalogue {
    abi: String,
    instructions: Vec<Instruction>,
    by_name: BTreeMap<String, usize>,
    catchable: BTreeMap<String, bool>,
    limits: BTreeMap<String, Limits>,
    families: BTreeMap<String, Vec<String>>,
}

fn catalogue() -> &'static Catalogue {
    static C: OnceLock<Catalogue> = OnceLock::new();
    C.get_or_init(|| {
        let v: Value = serde_json::from_str(ISA_JSON).expect("isa.json");
        let mut instructions = Vec::new();
        let mut by_name = BTreeMap::new();
        for d in v["instructions"].as_array().into_iter().flatten() {
            let name = d["name"].as_str().unwrap_or_default().to_string();
            let mut decl = d.clone();
            decl.as_object_mut().map(|m| m.remove("clause"));
            by_name.insert(name.clone(), instructions.len());
            instructions.push(Instruction {
                family: name.split('.').next().unwrap_or_default().to_string(),
                since: d["since"].as_str().unwrap_or("1.0").to_string(),
                params: d["params"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        (
                            p["name"].as_str().unwrap_or_default().to_string(),
                            p["type"].as_str().unwrap_or_default().to_string(),
                            p["optional"] == json!(true),
                        )
                    })
                    .collect(),
                base_cost: d["cost"]["base"].as_u64().unwrap_or_default(),
                per_item: d["cost"]["per_item"].as_u64().unwrap_or_default(),
                authorization: d["authorization"].as_str().unwrap_or("none").to_string(),
                name,
                decl,
            });
        }
        let catchable = v["catchable"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, x)| (k.clone(), x == &json!(true)))
            .collect();
        let limits = v["profile_limits"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, l)| {
                let g = |n: &str| l[n].as_u64().unwrap_or_default();
                (
                    k.clone(),
                    Limits {
                        units: g("units"),
                        instruction_calls: g("instruction_calls"),
                        changes: g("changes"),
                        depth: g("depth"),
                        connector_calls: g("connector_calls"),
                        model_calls: g("model_calls"),
                    },
                )
            })
            .collect();
        let families = v["profile_families"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, f)| {
                (
                    k.clone(),
                    f.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect(),
                )
            })
            .collect();
        Catalogue {
            abi: v["abi"].as_str().unwrap_or("1.0").to_string(),
            instructions,
            by_name,
            catchable,
            limits,
            families,
        }
    })
}

/// The ABI version of the catalogue.
pub fn abi() -> &'static str {
    &catalogue().abi
}

/// The instruction named `name`, if declared.
pub fn by_name(name: &str) -> Option<&'static Instruction> {
    let c = catalogue();
    c.by_name.get(name).map(|&i| &c.instructions[i])
}

/// Whether an error code is registered, and catchable (`Some(true)`), registered and not
/// catchable (`Some(false)`), or unknown (`None`) (STD-ERR-002).
pub fn catchable(code: &str) -> Option<bool> {
    catalogue().catchable.get(code).copied()
}

/// The registered form of an error code, with static lifetime (STD-ERR-002).
pub fn registered(code: &str) -> Option<&'static str> {
    catalogue()
        .catchable
        .get_key_value(code)
        .map(|(k, _)| k.as_str())
}

/// The limits of a profile, if known.
pub fn limits(profile: &str) -> Option<Limits> {
    catalogue().limits.get(profile).copied()
}

/// The instruction families a profile allows.
pub fn families(profile: &str) -> &'static [String] {
    catalogue()
        .families
        .get(profile)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Adapter operation `instructions`: the catalogue of ABI 1.0.
pub fn instructions_op() -> Value {
    let c = catalogue();
    json!({"abi": c.abi, "instructions": c.instructions.iter().map(|i| i.decl.clone()).collect::<Vec<_>>()})
}

/// Adapter operation `profile_limits`.
pub fn profile_limits_op(i: &Value) -> Result<Value, BpaError> {
    let p = i.get("profile").and_then(Value::as_str).unwrap_or_default();
    let l = limits(p).ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "unknown profile"))?;
    let mut out: Map<String, Value> = l.to_json().as_object().cloned().unwrap_or_default();
    out.insert("families".into(), json!(families(p)));
    Ok(Value::Object(out))
}

/// `(major, minor)` of an ABI version `MAJOR.MINOR`.
pub fn abi_version(v: Option<&Value>) -> Result<(u64, u64), BpaError> {
    let bad = || BpaError::new("MODEL.INVALID_VALUE", "an ABI version is MAJOR.MINOR");
    let s = v.and_then(Value::as_str).ok_or_else(bad)?;
    let (a, b) = s.split_once('.').ok_or_else(bad)?;
    let num = |x: &str| -> Result<u64, BpaError> {
        if x.is_empty()
            || (x.len() > 1 && x.starts_with('0'))
            || !x.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(bad());
        }
        x.parse().map_err(|_| bad())
    };
    Ok((num(a)?, num(b)?))
}
