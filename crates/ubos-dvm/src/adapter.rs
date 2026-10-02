//! Adapter operations of release 0.1 (STD-CONF-005).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use ubos_store::{MemStore, PgStore};
use ubos_types::error::invalid;
use ubos_types::{BpaError, content_id, names, nfc, time, values};

/// Operations that read or change adapter state; their hashes are compared only through
/// `same_as` (STD-CONF-005, rule 6).
pub const STATEFUL_OPERATIONS: &[&str] = &[
    "reset",
    "set_now",
    "load_genesis",
    "create_branch",
    "commit",
    "read",
    "read_ledger",
    "merge",
    "diff",
    "export_state",
    "set_limits",
    "list_objects",
    "traverse",
    "class_at",
    "merge_preview",
    "change_set",
    "tag",
    "delete_tag",
    "tag_info",
    "release",
    "revert",
    "branch_info",
    "set_parent_line",
    "commit_info",
    "tree_diff",
    "process_info",
    "lineage",
];

const ENVELOPE_REQUIRED: &[&str] = &[
    "object_id",
    "class",
    "class_version",
    "schema_version",
    "kind",
    "slug",
    "valid",
];
const ENTRY_REQUIRED: &[&str] = &[
    "entry_id",
    "class",
    "class_version",
    "ledger_key",
    "valid_at",
];

/// Error code of an operation the adapter does not implement (outside the claimed scope).
pub const UNSUPPORTED: &str = "ADAPTER.UNSUPPORTED";

fn field<'a>(input: &'a Value, name: &str) -> Result<&'a Value, BpaError> {
    input
        .get(name)
        .ok_or_else(|| invalid(format!("missing input {name}")))
}

fn text<'a>(input: &'a Value, name: &str) -> Result<&'a str, BpaError> {
    field(input, name)?
        .as_str()
        .ok_or_else(|| invalid(format!("{name} is text")))
}

fn opt_text<'a>(input: &'a Value, name: &str) -> Option<&'a str> {
    input.get(name).and_then(Value::as_str)
}

fn object(v: &Value) -> Result<&Map<String, Value>, BpaError> {
    v.as_object().ok_or_else(|| invalid("object expected"))
}

/// `{body_ref, version_hash}` of an object version (STD-OBJ-002).
pub fn version_hash(envelope: &Value, body: &Value) -> Result<Value, BpaError> {
    let env = object(envelope)?;
    for k in ENVELOPE_REQUIRED {
        if !env.contains_key(*k) {
            return Err(invalid(format!("envelope member {k}")));
        }
    }
    let body_ref = content_id("ubos.chunk.v1", body)?;
    let mut env = env.clone();
    env.insert("body_ref".into(), Value::String(body_ref.clone()));
    Ok(
        json!({"body_ref": body_ref, "version_hash": content_id("ubos.version.v1", &Value::Object(env))?}),
    )
}

/// `{body_ref, entry_hash}` of a ledger entry; `seq` and `source_seq` are excluded (STD-KIND-020).
pub fn entry_hash(entry: &Value, body: &Value) -> Result<Value, BpaError> {
    let e = object(entry)?;
    for k in ENTRY_REQUIRED {
        if !e.contains_key(*k) {
            return Err(invalid(format!("entry member {k}")));
        }
    }
    let mut rec: Map<String, Value> = e
        .iter()
        .filter(|(k, _)| *k != "seq" && *k != "source_seq")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let body_ref = content_id("ubos.chunk.v1", body)?;
    rec.insert("body_ref".into(), Value::String(body_ref.clone()));
    Ok(
        json!({"body_ref": body_ref, "entry_hash": content_id("ubos.entry.v1", &Value::Object(rec))?}),
    )
}

/// Commit identifier: content identifier of the commit without `signatures` (STD-VER-001).
pub fn commit_id(commit: &Value) -> Result<String, BpaError> {
    let mut c = object(commit)?.clone();
    c.remove("signatures");
    content_id("ubos.commit.v1", &Value::Object(c))
}

fn tree_entries(v: &Value) -> Result<Vec<(String, String)>, BpaError> {
    v.as_array()
        .ok_or_else(|| invalid("entries is a list"))?
        .iter()
        .map(|e| match e.as_array().map(Vec::as_slice) {
            Some([Value::String(k), Value::String(h)]) => Ok((k.clone(), h.clone())),
            _ => Err(invalid("entry is [key, hash]")),
        })
        .collect()
}

fn list(i: &Value, k: &str) -> Result<Vec<Value>, BpaError> {
    field(i, k)?
        .as_array()
        .cloned()
        .ok_or_else(|| invalid(format!("{k} is a list")))
}

fn opt_list(i: &Value, k: &str) -> Vec<Value> {
    i.get(k)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The conformance adapter (STD-CONF-002, STD-CONF-005).
pub struct Adapter {
    engine: Engine,
}

impl Adapter {
    /// A new adapter over an in-memory store.
    pub fn new() -> Result<Self, BpaError> {
        Ok(Adapter {
            engine: Engine::new(Box::new(MemStore::default()))?,
        })
    }

    /// A new adapter over a PostgreSQL database (for example `host=127.0.0.1 user=postgres`).
    pub fn postgres(url: &str) -> Result<Self, BpaError> {
        Ok(Adapter {
            engine: Engine::new(Box::new(PgStore::connect(url)?))?,
        })
    }

    /// The engine (storage suite and verifiers).
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Name of the storage backend.
    pub fn backend(&self) -> &'static str {
        self.engine.store.backend()
    }

    /// Execute one operation.
    pub fn execute(&mut self, op: &str, input: &Value) -> Result<Value, BpaError> {
        let i = input;
        Ok(match op {
            "reset" => {
                self.engine.reset()?;
                Value::Bool(true)
            }
            "set_now" => {
                ubos_types::time::parse_instant(text(i, "now")?)?;
                self.engine.now = text(i, "now")?.to_string();
                Value::Bool(true)
            }
            "load_genesis" => self
                .engine
                .load_genesis(field(i, "classes")?, i.get("value_types"))?,
            "commit" => self.engine.commit(i)?,
            "read" => self.engine.read(i)?,
            "read_ledger" => self.engine.read_ledger(i)?,
            "create_branch" | "merge" | "diff" | "export_state" | "merge_preview"
            | "change_set" | "tag" | "delete_tag" | "tag_info" | "release" | "revert"
            | "branch_info" | "set_parent_line" | "commit_info" | "tree_diff" | "process_info"
            | "lineage" => self.engine.other(op, i)?,
            "canonical_json" => Value::String(ubos_types::canonical(field(i, "value")?)?),
            "content_id" => Value::String(content_id(text(i, "domain")?, field(i, "value")?)?),
            "normalize_text" => Value::String(nfc(text(i, "text")?)),
            "check_name" => {
                let t = field(i, "text")?
                    .as_str()
                    .ok_or_else(|| BpaError::new("MODEL.INVALID_NAME", "not text"))?;
                names::check_name(text(i, "grammar")?, t)?;
                Value::Bool(true)
            }
            "validate_value" => {
                if i.get("value_types").is_some_and(|v| !v.is_null()) {
                    return Err(BpaError::new(
                        UNSUPPORTED,
                        "custom value types need the L1 evaluator (PH-1 step 5)",
                    ));
                }
                values::validate_value(text(i, "type")?, i.get("constraints"), field(i, "value")?)?
            }
            "linearize" => json!(ubos_model::linearize_raw(
                &list(i, "classes")?,
                text(i, "class")?
            )?),
            "effective_class" => {
                let r = ubos_model::registry_of(&list(i, "classes")?, &opt_list(i, "extensions"))?;
                r.class(text(i, "class")?)?.to_json()
            }
            "dispatch" => {
                let r = ubos_model::registry_of(&list(i, "classes")?, &opt_list(i, "extensions"))?;
                r.dispatch(text(i, "class")?, text(i, "port")?, opt_text(i, "after"))?
            }
            "type_selector" => {
                let r = ubos_model::registry_of(&list(i, "classes")?, &[])?;
                r.type_selector(
                    text(i, "class")?,
                    text(i, "selector")?,
                    i.get("only").and_then(Value::as_bool).unwrap_or(false),
                )?
            }
            "blind_index" => {
                let key = text(i, "key")?;
                let hex_ok = key.len() == 64
                    && key
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
                if !hex_ok {
                    return Err(invalid("a lookup key is 32 bytes in lowercase hexadecimal"));
                }
                let k: Vec<u8> = (0..32)
                    .map(|j| u8::from_str_radix(&key[2 * j..2 * j + 2], 16).unwrap_or_default())
                    .collect();
                let norm = values::validate_value(
                    text(i, "type")?,
                    i.get("constraints"),
                    field(i, "value")?,
                )?;
                let mac = ubos_crypto::hmac_sha256(&k, ubos_types::canonical(&norm)?.as_bytes());
                Value::String(format!(
                    "hmac-sha256:{}",
                    mac.iter().map(|b| format!("{b:02x}")).collect::<String>()
                ))
            }
            "set_limits" => {
                for k in ["max_body_bytes", "max_derivation_inputs"] {
                    if let Some(v) = i.get(k)
                        && !v.is_null()
                        && !v.as_u64().is_some_and(|n| n >= 1 && !v.is_boolean())
                    {
                        return Err(invalid("a limit is a positive integer"));
                    }
                }
                if let Some(n) = i.get("max_body_bytes").and_then(Value::as_u64) {
                    self.engine.registry.max_body = n as usize;
                }
                if let Some(n) = i.get("max_derivation_inputs").and_then(Value::as_u64) {
                    self.engine.max_inputs = n;
                }
                Value::Bool(true)
            }
            "list_objects" | "traverse" | "class_at" => self.engine.other(op, i)?,
            "decimal" => Value::String(values::decimal_op(
                text(i, "op")?,
                text(i, "a")?,
                opt_text(i, "b"),
                i.get("scale").and_then(Value::as_u64).map(|s| s as u32),
                opt_text(i, "rounding"),
            )?),
            "money" => values::money_op(text(i, "op")?, field(i, "a")?, field(i, "b")?)?,
            "local_to_instant" => Value::String(time::local_to_instant(
                text(i, "date")?,
                text(i, "time")?,
                text(i, "tz")?,
                opt_text(i, "disambiguation"),
            )?),
            "classification_covers" => {
                let list: Vec<String> = field(i, "clearances")?
                    .as_array()
                    .ok_or_else(|| invalid("clearances is a list"))?
                    .iter()
                    .map(|v| v.as_str().unwrap_or("?").to_string())
                    .collect();
                Value::Bool(values::classification_covers(&list, text(i, "level")?)?)
            }
            "version_hash" => version_hash(field(i, "envelope")?, field(i, "body")?)?,
            "entry_hash" => entry_hash(field(i, "entry")?, field(i, "body")?)?,
            "commit_id" => Value::String(commit_id(field(i, "commit")?)?),
            "tree_root" => {
                Value::String(ubos_tree::tree_root(&tree_entries(field(i, "entries")?)?)?)
            }
            _ => {
                return Err(BpaError::new(
                    UNSUPPORTED,
                    format!("operation {op} is not implemented"),
                ));
            }
        })
    }
}
