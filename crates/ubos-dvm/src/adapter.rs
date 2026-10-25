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
    "query",
    "execute",
    "journal",
    "metering",
    "metering_total",
    "replay",
    "verify_asset",
    "bpu_descriptor",
    "bpu_state",
    "verify_bpu",
    "node_did",
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
    "read_range",
    "history",
    "derivation",
    "impact",
    "outbox",
    "ack",
    "outbox_claim",
    "outbox_ack",
    "due_effective",
    "system_commit",
    "bulk",
    "resolve",
    "did_document",
    "rotate_node_key",
    "commit_record",
    "prove",
    "consistency_proof",
    "tenant_root",
    "export_archive",
    "import_archive",
    "cascade",
    "explain_cascade",
    "actions",
    "declare_index",
    "authorize",
    "decision_log",
    "policy_schema",
    "changes",
    "transitions",
    "audit",
    "view",
    "currency",
    "fx_rate",
    "read_projection",
    "projection_lag",
    "rebuild_projection",
    "close_period",
    "reopen_period",
    "match",
    "unmatch",
    "ingest_statement",
    "restate",
    "decide_table",
    "simulate",
    "rule_impact",
    "run_rule_tests",
    "pending_change",
    "approve",
    "principal",
    "delegate",
    "revoke_delegation",
    "delegations",
    "task",
    "inbox",
    "sla_report",
    "security_log",
    "verify_security_log",
    "ubtp",
    "validate",
    "decide",
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

fn s_or<'a>(i: &'a Value, k: &str, default: &'a str) -> &'a str {
    i.get(k).and_then(Value::as_str).unwrap_or(default)
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

    /// Enable the L2 host (STD-CTX-020); off by default, see `Engine::l2`.
    pub fn enable_l2(&mut self) {
        self.engine.l2 = true;
    }

    /// A new adapter over a PostgreSQL database (for example `host=127.0.0.1 user=postgres`).
    pub fn postgres(url: &str) -> Result<Self, BpaError> {
        Ok(Adapter {
            engine: Engine::new(Box::new(PgStore::connect(url)?))?,
        })
    }

    /// A new adapter over a PostgreSQL database in the VAE scope `(tenant, vae)`; `reset`
    /// moves it to a fresh VAE of the tenant, so several adapters share one database
    /// (round trips between VAEs, EXIT-1-12).
    pub fn postgres_scope(url: &str, tenant: &str, vae: &str) -> Result<Self, BpaError> {
        let scope = ubos_store::kernel::Scope::new(tenant, vae);
        Ok(Adapter {
            engine: Engine::new(Box::new(PgStore::connect_scope(url, scope)?))?,
        })
    }

    /// An adapter over an existing VAE of a PostgreSQL database without resetting it: a
    /// second writer on the same VAE (concurrency tests, SCN-005 A1).
    pub fn postgres_existing(
        url: &str,
        tenant: &str,
        vae: &str,
        authority: &str,
    ) -> Result<Self, BpaError> {
        let scope = ubos_store::kernel::Scope {
            tenant: tenant.into(),
            vae: vae.into(),
            authority: authority.into(),
        };
        Ok(Adapter {
            engine: Engine::open_existing(Box::new(PgStore::connect_scope(url, scope)?))?,
        })
    }

    /// An adapter over an engine a host built (the host API, CTR-001).
    pub fn from_engine(engine: Engine) -> Self {
        Adapter { engine }
    }

    /// The engine, consuming the adapter (the host API builds a `Vae` over it).
    pub fn into_engine(self) -> Engine {
        self.engine
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
                self.engine.tick()?;
                Value::Bool(true)
            }
            "load_genesis" => self
                .engine
                .load_genesis(field(i, "classes")?, i.get("value_types"))?,
            "commit" => self.engine.commit(i)?,
            "system_commit" => self.engine.system_commit(i)?,
            "read" => self.engine.read(i)?,
            "read_ledger" => self.engine.read_ledger(i)?,
            "create_branch" | "merge" | "diff" | "export_state" | "merge_preview"
            | "change_set" | "tag" | "delete_tag" | "tag_info" | "release" | "revert"
            | "branch_info" | "set_parent_line" | "commit_info" | "tree_diff" | "process_info"
            | "lineage" | "read_range" | "history" | "derivation" | "impact" | "outbox" | "ack"
            | "outbox_claim" | "outbox_ack" | "due_effective" | "bulk" | "resolve" => {
                self.engine.other(op, i)?
            }
            "parse_uri" => ubos_types::addr::parse_uri(field(i, "uri")?)?,
            "parse_did" => ubos_types::addr::parse_did(field(i, "did")?)?,
            "key_did" => Value::String(ubos_types::addr::key_did(field(i, "public_key")?)?),
            "aead_decrypt" => Value::String(ubos_crypto::aead_decrypt(
                text(i, "alg")?,
                text(i, "key")?,
                text(i, "nonce")?,
                text(i, "ciphertext")?,
                opt_text(i, "aad").unwrap_or_default(),
            )?),
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
            "validate_value" => crate::expressions::validate(i)?,
            "check_expr" => ubos_expr::check_expr(i)?,
            "parse_sheet" => ubos_rules::ops::parse_sheet(i)?,
            "check_table" => ubos_rules::tables::check_table(field(i, "table")?)?,
            "decide_table" => self.engine.decide_table(i)?,
            "simulate" => self.engine.simulate(i)?,
            "rule_impact" => self.engine.rule_impact(i)?,
            "run_rule_tests" => self.engine.run_rule_tests(i)?,
            "parse_query" => ubos_query::parse_query(field(i, "text")?)?,
            "cascade" => self.engine.cascade(i)?,
            "validate" => self.engine.validate(i)?,
            "decide" => self.engine.decide(i)?,
            "explain_cascade" => self.engine.explain_cascade(i)?,
            "actions" => self.engine.actions(i)?,
            "authorize" => self.engine.authorize_op(i)?,
            "decision_log" => Value::Array(self.engine.drain_decision_log()),
            "policy_schema" => self.engine.policy_schema()?,
            "changes" => self.engine.changes(i)?,
            "transitions" => self.engine.transitions(i)?,
            "audit" => self.engine.audit(i)?,
            "view" => self.engine.view(i)?,
            "currency" => self.engine.currency(i)?,
            "fx_rate" => self.engine.fx_rate(i)?,
            "read_projection" => self.engine.read_projection(i)?,
            "projection_lag" => self.engine.projection_lag(i)?,
            "rebuild_projection" => self.engine.rebuild_projection(i)?,
            "close_period" => self.engine.close_period(i)?,
            "reopen_period" => self.engine.reopen_period(i)?,
            "match" => self.engine.match_op(i)?,
            "unmatch" => self.engine.unmatch(i)?,
            "ingest_statement" => self.engine.ingest_statement(i)?,
            "restate" => self.engine.restate(i)?,
            "pending_change" => self.engine.pending_change(i)?,
            "approve" => self.engine.approve(i)?,
            "principal" => self.engine.principal_entry(i)?,
            "delegate" => self.engine.delegate(i)?,
            "revoke_delegation" => self.engine.revoke_delegation(i)?,
            "delegations" => self.engine.delegations(i)?,
            "task" => self.engine.task(i)?,
            "inbox" => self.engine.inbox(i)?,
            "sla_report" => self.engine.sla_report(i)?,
            "security_log" => self.engine.security_log(i)?,
            "verify_security_log" => self.engine.verify_security_log()?,
            "ubtp" => self.engine.ubtp(i)?,
            "declare_index" => {
                let r = self
                    .engine
                    .declare_index(text(i, "class")?, text(i, "field")?)?;
                self.engine.security_append(
                    "admin.declare_index",
                    json!({"class": text(i, "class")?, "field": text(i, "field")?}),
                )?;
                r
            }
            "check_sheet" => ubos_rules::ops::check_sheet(i)?,
            "specificity" => ubos_rules::ops::specificity(i)?,
            "eval_expr" => ubos_expr::eval_expr(i, None)?,
            "query" => self.engine.query(i)?,
            "execute" => self.engine.execute(i)?,
            "journal" => self.engine.journal(text(i, "process_id")?)?,
            "metering" => self.engine.metering_record(text(i, "process_id")?)?,
            "metering_total" => self.engine.metering_total()?,
            "instructions" => ubos_runtime::isa::instructions_op(),
            "profile_limits" => ubos_runtime::isa::profile_limits_op(i)?,
            "replay" => self.engine.replay_process(
                text(i, "process_id")?,
                i.get("tamper").filter(|v| !v.is_null()),
                i.get("drop").and_then(Value::as_u64),
            )?,
            "verify_asset" => self.engine.verify_asset(
                s_or(i, "branch", "main"),
                text(i, "object_id")?,
                i.get("verifier").and_then(Value::as_str),
            )?,
            "bpu_descriptor" => self
                .engine
                .bpu_descriptor(s_or(i, "branch", "main"), text(i, "class")?)?,
            "bpu_state" => {
                self.engine
                    .bpu_state(s_or(i, "branch", "main"), text(i, "object_id")?, i)?
            }
            "verify_bpu" => self
                .engine
                .verify_bpu(s_or(i, "branch", "main"), text(i, "class")?)?,
            "node_did" => self.engine.node_did()?,
            "did_document" => self.engine.did_document(field(i, "did")?)?,
            "rotate_node_key" => {
                let r = self.engine.rotate_node_key(field(i, "seed")?)?;
                self.engine
                    .security_append("key.rotate", json!({"did": r.get("did")}))?;
                r
            }
            "commit_record" => self.engine.commit_record(i)?,
            "prove" => self.engine.prove(i)?,
            "consistency_proof" => self.engine.consistency_proof(i)?,
            "tenant_root" => self.engine.tenant_root()?,
            "export_archive" => {
                let r = self.engine.export_archive()?;
                self.engine.security_append("export.archive", json!({}))?;
                r
            }
            "import_archive" => {
                let r = self.engine.import_archive(field(i, "archive")?)?;
                self.engine
                    .security_append("admin.import_archive", json!({}))?;
                r
            }
            "sign" => ubos_crypto::sign(
                &ubos_crypto::unhex32(field(i, "seed")?, "seed")?,
                field(i, "content_id")?
                    .as_str()
                    .ok_or_else(|| invalid("the signed content is a content ID"))?,
                text(i, "signer")?,
                text(i, "key_id")?,
                s_or(i, "role", "signer"),
            )?,
            "public_key" => Value::String(ubos_types::addr::b64u(&ubos_crypto::ed25519_public(
                &ubos_crypto::unhex32(field(i, "seed")?, "seed")?,
            ))),
            "verify_signature" => Value::Bool(ubos_crypto::verify(
                field(i, "signature")?,
                field(i, "content_id")?
                    .as_str()
                    .ok_or_else(|| invalid("the signed content is a content ID"))?,
                field(i, "did")?,
                opt_text(i, "at"),
            )?),
            "inclusion_proof" => ubos_proof::inclusion_proof(
                &ubos_proof::entries(field(i, "entries")?)?,
                text(i, "key")?,
            )?,
            "verify_inclusion" => Value::Bool(ubos_proof::verify_inclusion(
                field(i, "proof")?,
                opt_text(i, "root"),
                &opt_list(i, "dids"),
            )?),
            "verify_consistency" => Value::Bool(ubos_proof::verify_consistency(
                field(i, "commits")?,
                opt_text(i, "from"),
                opt_text(i, "to"),
                &opt_list(i, "dids"),
            )?),
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
                for k in [
                    "max_body_bytes",
                    "max_derivation_inputs",
                    "max_violations",
                    "max_scan",
                ] {
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
                if let Some(n) = i.get("max_violations").and_then(Value::as_u64) {
                    self.engine.max_violations = n;
                }
                if let Some(n) = i.get("max_scan").and_then(Value::as_u64) {
                    self.engine.max_scan = Some(n);
                }
                let mut limits = Map::new();
                for k in [
                    "max_body_bytes",
                    "max_derivation_inputs",
                    "max_violations",
                    "max_scan",
                ] {
                    if let Some(v) = i.get(k).filter(|v| !v.is_null()) {
                        limits.insert(k.into(), v.clone());
                    }
                }
                self.engine
                    .security_append("admin.set_limits", Value::Object(limits))?;
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
