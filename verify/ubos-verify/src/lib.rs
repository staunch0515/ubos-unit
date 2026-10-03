//! `ubos-verify` — the independent verifier (DEC-028, component `ubos-verify`; PH-1 step 6c,
//! DEC-059). Licence Apache-2.0.
//!
//! A verifier needs no kernel: it depends on `ubos-proof` (and through it on `ubos-tree`
//! and `ubos-crypto`) only (AR-031, `docs/tools/check_deps.py`). It checks what BPA 1.0
//! lets an auditor hold in hand — an inclusion proof (STD-PROOF-020), a consistency proof
//! (STD-PROOF-021), a signature over a content ID (STD-PROOF-011) and a portable archive
//! (STD-PROOF-070) — and writes a structured report (DSN-DVM-938): every check with its
//! result and, for a failure, the registered error code and the reason in plain words.
//!
//! The state roots of an archive are recomputed by an importing kernel, not here: a
//! verifier without a store checks everything that is content-addressed or signed.

use serde_json::{Value, json};
use ubos_crypto::types::BpaError;

/// One check of a report.
#[derive(Debug, Clone)]
pub struct Check {
    /// Name of the check.
    pub name: &'static str,
    /// Result of the check: `pass` or `fail`.
    pub pass: bool,
    /// Registered error code of a failure.
    pub code: Option<String>,
    /// Reason of a failure, or a short note on a pass.
    pub detail: Option<String>,
}

/// A verification report (DSN-DVM-938).
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// What was verified: `inclusion`, `consistency`, `signature` or `archive`.
    pub subject: &'static str,
    /// The checks in the order they ran; a report stops at the first failure of a chain.
    pub checks: Vec<Check>,
}

impl Report {
    fn new(subject: &'static str) -> Report {
        Report {
            subject,
            checks: Vec::new(),
        }
    }

    fn record<T>(&mut self, name: &'static str, r: Result<T, BpaError>) -> Option<T> {
        match r {
            Ok(v) => {
                self.checks.push(Check {
                    name,
                    pass: true,
                    code: None,
                    detail: None,
                });
                Some(v)
            }
            Err(e) => {
                self.checks.push(Check {
                    name,
                    pass: false,
                    code: Some(e.code.to_string()),
                    detail: Some(e.message.clone()),
                });
                None
            }
        }
    }

    fn note(&mut self, detail: String) {
        if let Some(c) = self.checks.last_mut() {
            c.detail = Some(detail);
        }
    }

    /// Whether every check passed.
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.pass)
    }

    /// The report as JSON: `{subject, result, checks: [{check, result, code?, detail?}]}`.
    pub fn to_json(&self) -> Value {
        let checks: Vec<Value> = self
            .checks
            .iter()
            .map(|c| {
                let mut v =
                    json!({"check": c.name, "result": if c.pass { "pass" } else { "fail" }});
                if let Some(code) = &c.code {
                    v["code"] = json!(code);
                }
                if let Some(d) = &c.detail {
                    v["detail"] = json!(d);
                }
                v
            })
            .collect();
        json!({"subject": self.subject, "result": if self.ok() { "pass" } else { "fail" }, "checks": checks})
    }
}

/// Verify an inclusion proof against a root, the signed commit record it carries (checked
/// with `dids`), or both (STD-PROOF-020).
pub fn inclusion(proof: &Value, root: Option<&str>, dids: &[Value]) -> Report {
    let mut r = Report::new("inclusion");
    if r.record(
        "inclusion_proof",
        ubos_proof::verify_inclusion(proof, root, dids),
    )
    .is_some()
    {
        let key = proof.get("key").and_then(Value::as_str).unwrap_or_default();
        r.note(format!("key {key} is in the tree"));
    }
    r
}

/// Verify a consistency proof: signed commit records from the later commit back to the
/// earlier one (STD-PROOF-021).
pub fn consistency(
    commits: &Value,
    from: Option<&str>,
    to: Option<&str>,
    dids: &[Value],
) -> Report {
    let mut r = Report::new("consistency");
    if r.record(
        "consistency_proof",
        ubos_proof::verify_consistency(commits, from, to, dids),
    )
    .is_some()
    {
        let n = commits.as_array().map(Vec::len).unwrap_or_default();
        r.note(format!("{n} commit records chained by first parents"));
    }
    r
}

/// Verify a DAT-Signature over a content ID against a DID document, at `at` when given
/// (STD-PROOF-011).
pub fn signature(sig: &Value, content_id: &str, did: &Value, at: Option<&str>) -> Report {
    let mut r = Report::new("signature");
    if r.record("signature", ubos_crypto::verify(sig, content_id, did, at))
        .is_some()
    {
        let key = sig
            .get("key_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        r.note(format!("verified with key {key}"));
    }
    r
}

/// Verify a portable archive (STD-PROOF-070) check by check: format and manifest signature,
/// chunks, version envelopes, entry records, commit records with signatures and parents,
/// branches and head roots, counts.
pub fn archive(a: &Value) -> Report {
    let mut r = Report::new("archive");
    let Some(dids) = r.record("manifest_signature", ubos_proof::archive_manifest(a)) else {
        return r;
    };
    r.note(format!(
        "format {} of VAE {}, {} DID document(s)",
        ubos_proof::ARCHIVE_FORMAT,
        a["manifest"]["vae"].as_str().unwrap_or_default(),
        dids.len()
    ));
    type Step<'a> = Box<dyn Fn() -> Result<String, BpaError> + 'a>;
    let steps: [(&'static str, Step<'_>); 6] = [
        (
            "chunks",
            Box::new(|| {
                ubos_proof::archive_chunks(a).map(|n| format!("{n} chunks match their content IDs"))
            }),
        ),
        (
            "versions",
            Box::new(|| {
                ubos_proof::archive_versions(a)
                    .map(|n| format!("{n} version envelopes match their hashes"))
            }),
        ),
        (
            "entries",
            Box::new(|| {
                ubos_proof::archive_entries(a)
                    .map(|n| format!("{n} entry records match their hashes"))
            }),
        ),
        (
            "commits",
            Box::new(|| {
                ubos_proof::archive_commits(a, &dids)
                    .map(|n| format!("{n} commit records: IDs, signatures and parents verified"))
            }),
        ),
        (
            "refs",
            Box::new(|| {
                ubos_proof::archive_refs(a).map(|n| {
                    format!(
                        "{n} branches name commits of the archive with the manifest's head roots"
                    )
                })
            }),
        ),
        (
            "counts",
            Box::new(|| ubos_proof::archive_counts(a).map(|c| format!("counts {c}"))),
        ),
    ];
    for (name, step) in steps {
        match r.record(name, step()) {
            Some(d) => r.note(d),
            None => return r,
        }
    }
    r
}
