//! Verification records of logic assets (DSN-FRG-103, STD-CTX-023): the asset hash, ABI,
//! passed checks, test results, coverage and inferred capabilities, signed with the
//! verifier key (a developer or CI key given as a seed, or an ephemeral one).

use serde_json::{Value, json};
use ubos_dvm::crypto;
use ubos_dvm::execution::code_identity;
use ubos_dvm::types::addr::{b64u, key_did};
use ubos_dvm::verification::{ANALYZER, analyse};
use ubos_dvm::{BpaError, content_id};

/// The verifier key: seed, DID (key form) and key identifier.
pub struct VerifierKey {
    seed: [u8; 32],
    /// The key-form DID of the key.
    pub did: String,
    key_id: String,
}

impl VerifierKey {
    /// A key from a seed.
    pub fn new(seed: [u8; 32]) -> Result<VerifierKey, BpaError> {
        let public = b64u(&crypto::ed25519_public(&seed));
        let did = key_did(&json!(public))?;
        let mb = did.trim_start_matches("did:ubos:k:").to_string();
        Ok(VerifierKey {
            seed,
            key_id: format!("{did}#{mb}"),
            did,
        })
    }

    /// The DID document of the key (STD-ADDR-010).
    pub fn document(&self) -> Result<Value, BpaError> {
        ubos_dvm::types::addr::key_document(&json!(self.did))
    }
}

/// Capabilities an asset's instructions imply (the families it calls).
pub fn inferred_capabilities(body: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if let Some(d) = o.get("do").and_then(Value::as_str)
                    && let Some((fam, _)) = d.split_once('.')
                    && fam != "sys"
                {
                    out.insert(fam.to_string());
                }
                o.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(&body["source"], &mut out);
    out.into_iter().collect()
}

/// Build and sign the verification record of an asset: `tests` is `(passed, failed)` of
/// the unit and property tests, `coverage` the fraction of its exported functions invoked
/// (recorded in permille, so that the record stays canonical JSON).
pub fn record(
    key: &VerifierKey,
    body: &Value,
    tests: (u64, u64),
    coverage: f64,
    at: &str,
) -> Result<Value, BpaError> {
    let findings = analyse(body)?;
    let rule = |x: &Value| {
        x.get("rule")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let part = |pred: &dyn Fn(&str) -> bool| -> Vec<Value> {
        findings
            .iter()
            .filter(|f| pred(&rule(f)))
            .cloned()
            .collect()
    };
    let statics = part(&|r| !matches!(r, "undeclared_instruction" | "type_error"));
    let types = part(&|r| r == "type_error");
    let caps = part(&|r| r == "undeclared_instruction");
    let check = |name: &str, f: Vec<Value>| json!({"name": name, "result": if f.is_empty() { "pass" } else { "fail" }, "findings": f});
    let checks = vec![
        check("static_analysis", statics),
        check("type_check", types),
        check("capabilities", caps),
        json!({"name": "tests", "result": if tests.1 == 0 { "pass" } else { "fail" },
            "findings": if tests.1 == 0 { json!([]) } else { json!([{"rule": "test_failed", "count": tests.1}]) },
            "passed": tests.0, "failed": tests.1}),
    ];
    let mut rec = json!({
        "asset": code_identity(body)?, "abi": body.get("abi").cloned().unwrap_or(json!("1.0")),
        "name": body.get("name"), "checks": checks, "coverage_permille": (coverage * 1000.0).round() as u64,
        "inferred_capabilities": inferred_capabilities(body),
        "declared_capabilities": body.get("capabilities").cloned().unwrap_or(json!([])),
        "analyzer_version": ANALYZER, "verified_by": key.did, "at": at,
    });
    let rid = content_id("ubos.chunk.v1", &rec)?;
    rec["signature"] = crypto::sign(&key.seed, &rid, &key.did, &key.key_id, "signer")?;
    rec["record_id"] = json!(rid);
    Ok(rec)
}

/// Verify a record's signature and that it binds `body` (the acceptance of DSN-FRG-103:
/// one changed byte of the asset or the record fails).
pub fn verify(record: &Value, body: &Value, did: &Value) -> Result<bool, BpaError> {
    let mut unsigned = record.as_object().cloned().unwrap_or_default();
    unsigned.remove("signature");
    unsigned.remove("record_id");
    let rid = content_id("ubos.chunk.v1", &Value::Object(unsigned))?;
    if record.get("record_id") != Some(&json!(rid)) {
        return Err(BpaError::new(
            "PROOF.VERIFICATION_FAILED",
            "the record identifier differs",
        ));
    }
    crypto::verify(&record["signature"], &rid, did, None)?;
    if record.get("asset") != Some(&json!(code_identity(body)?)) {
        return Err(BpaError::new(
            "PROOF.VERIFICATION_FAILED",
            "the record binds another asset",
        ));
    }
    Ok(true)
}
