//! Encrypted fields under subject keys and their erasure (BPA 1.0-rc8; PH-2 step 8c,
//! DEC-138; STD-OBJ-010, -011; DSN-DVM-830, -831, -834).
//!
//! * **Stored form.** A class that declares `subject_key` stores its fields of effective
//!   classification `personal` or `sensitive_personal` as `DAT-EncryptedField` items:
//!   AES-256-GCM under the subject's key, a random nonce and a random 16-byte salt inside
//!   the ciphertext, and the field commitment
//!   `H(ubos.field-commit.v1 || salt || canonical(value))`. The sealer runs when a version is
//!   created, before it is hashed, so hashes, trees and archives cover the stored form.
//! * **Opened form.** Reads, queries, merges, rules and logic see the plaintext: the opener
//!   runs when a version record is read. A field whose subject key was destroyed reads as
//!   the marker `{erased, commitment}`; a key the key service cannot provide fails with
//!   `IAM.KEY_UNAVAILABLE`.
//! * **Subjects.** `subject_key` is `self` (the object) or an unprotected text or reference
//!   field: the subject ID is the text, or the object ID the reference names.
//! * **Key service.** Subject keys are kernel metadata (`subject_key/<subject_id>`), outside
//!   the business data and archives: created on the first protected write, rotated by
//!   `subject_key`, destroyed by `erase_subject`. `key_availability` simulates a key the
//!   service cannot provide for now.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use ubos_store::Store;
use ubos_types::{BpaError, canonical};
use ubos_version::state::{Repo, Tx};

/// The protected classifications.
pub const PROTECTED: &[&str] = &["personal", "sensitive_personal"];
/// The AEAD of encrypted fields.
const ALG: &str = "aes-256-gcm";
/// Prefix of the kernel metadata keys of subject keys.
const KEY_PREFIX: &str = "subject_key/";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// Whether a value is a stored `DAT-EncryptedField`.
pub(crate) fn is_stored(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|o| o.contains_key("enc") && o.contains_key("ct") && o.contains_key("key"))
}

/// Whether a value is an erased marker (STD-OBJ-011).
pub(crate) fn is_marker(v: &Value) -> bool {
    v.as_object().is_some_and(|o| {
        o.get("erased") == Some(&json!(true)) && o.len() == 2 && o.contains_key("commitment")
    })
}

/// The marker of an erased field.
fn marker(e: &Value) -> Value {
    json!({"erased": true, "commitment": e["commitment"]})
}

/// `H(ubos.field-commit.v1 || salt || canonical(value))` (STD-OBJ-010).
pub fn commitment(salt: &[u8], value: &Value) -> Result<String, BpaError> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"ubos.field-commit.v1");
    h.update(salt);
    h.update(canonical(value)?.as_bytes());
    Ok(format!("sha256:{}", ubos_types::canon::hex(&h.finalize())))
}

/// Pure operation `verify_disclosure {commitment, salt, value}`.
pub fn verify_disclosure(i: &Value) -> Result<Value, BpaError> {
    let salt = s(i, "salt")
        .and_then(b64u_decode)
        .filter(|b| b.len() == 16)
        .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "salt is 16 bytes in base64url"))?;
    let value = i.get("value").cloned().unwrap_or(Value::Null);
    Ok(json!(
        commitment(&salt, &value)? == s(i, "commitment").unwrap_or_default()
    ))
}

fn b64u_decode(t: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32)
    };
    let bytes = t.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (k, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * k);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

fn unavailable(subject: &str) -> BpaError {
    BpaError::with_details("IAM.KEY_UNAVAILABLE", json!({"subject_id": subject}))
}

fn erased(details: Value) -> BpaError {
    BpaError::with_details("PROOF.VALUE_ERASED", details)
}

/// The key record of a subject in the key service.
fn key_record(store: &mut dyn Store, subject: &str) -> Result<Option<Value>, BpaError> {
    Ok(store
        .meta(&format!("{KEY_PREFIX}{subject}"))?
        .filter(|v| !v.is_null()))
}

fn put_key_record(store: &mut dyn Store, subject: &str, rec: &Value) -> Result<(), BpaError> {
    store.put_meta(&format!("{KEY_PREFIX}{subject}"), rec)
}

fn new_key() -> Result<String, BpaError> {
    Ok(ubos_types::canon::hex(&ubos_crypto::random_bytes::<32>()?))
}

fn key_bytes(hex: &str) -> Option<[u8; 32]> {
    let mut k = [0u8; 32];
    for (j, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(2 * j..2 * j + 2)?, 16).ok()?;
    }
    Some(k)
}

/// The current key of a subject for a write, created on the first one:
/// `PROOF.VALUE_ERASED` once destroyed, `IAM.KEY_UNAVAILABLE` when unavailable.
fn key_for_write(store: &mut dyn Store, subject: &str) -> Result<(u64, [u8; 32]), BpaError> {
    let rec = match key_record(store, subject)? {
        Some(r) => r,
        None => {
            let r = json!({"versions": {"1": new_key()?}, "current": 1, "destroyed": false, "unavailable": false});
            put_key_record(store, subject, &r)?;
            r
        }
    };
    if rec["destroyed"] == json!(true) {
        return Err(erased(json!({"subject_id": subject})));
    }
    if rec["unavailable"] == json!(true) {
        return Err(unavailable(subject));
    }
    let n = rec["current"].as_u64().unwrap_or(1);
    let k = s(&rec["versions"], &n.to_string())
        .and_then(key_bytes)
        .ok_or_else(|| unavailable(subject))?;
    Ok((n, k))
}

/// The associated data of a field: its object and name.
fn aad(oid: &str, field: &str) -> Vec<u8> {
    format!("{oid}\u{0}{field}").into_bytes()
}

/// `{salt, value}` of a stored field under its key.
fn open_field(
    store: &mut dyn Store,
    oid: &str,
    field: &str,
    e: &Value,
) -> Result<Option<Value>, BpaError> {
    let subject = s(&e["key"], "subject_id").unwrap_or_default().to_string();
    let Some(rec) = key_record(store, &subject)? else {
        return Err(unavailable(&subject));
    };
    if rec["destroyed"] == json!(true) {
        return Ok(None);
    }
    if rec["unavailable"] == json!(true) {
        return Err(unavailable(&subject));
    }
    let n = e["key"]["key_version"].as_u64().unwrap_or_default();
    let key = s(&rec["versions"], &n.to_string())
        .and_then(key_bytes)
        .ok_or_else(|| unavailable(&subject))?;
    let nonce: [u8; 12] = s(e, "nonce")
        .and_then(b64u_decode)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| ubos_crypto::integrity("nonce"))?;
    let ct = s(e, "ct")
        .and_then(b64u_decode)
        .ok_or_else(|| ubos_crypto::integrity("ciphertext"))?;
    let pt = ubos_crypto::gcm_open(&key, &nonce, &aad(oid, field), &ct)?;
    serde_json::from_slice(&pt)
        .map(Some)
        .map_err(|_| ubos_crypto::integrity("plaintext"))
}

/// `(subject_key, protected fields)` of a class, `None` without `subject_key`.
fn subject_decl(reg: &ubos_model::Registry, class: &str) -> Option<(String, Vec<String>)> {
    let c = reg.class(class).ok()?;
    let decl = |key: &str| {
        c.linearization
            .iter()
            .find_map(|n| reg.decl(n).ok().and_then(|d| s(d, key)).map(str::to_string))
    };
    let sk = decl("subject_key")?;
    let default = decl("classification_default").unwrap_or_else(|| "internal".into());
    let fields = c
        .fields
        .iter()
        .filter(|(_, d)| PROTECTED.contains(&s(d, "classification").unwrap_or(&default)))
        .map(|(f, _)| f.clone())
        .collect();
    Some((sk, fields))
}

/// The subject of an object: itself, the text of the subject key field, or the object a
/// reference names.
fn subject_of(
    repo: &mut Repo<'_>,
    tx: &Tx,
    oid: &str,
    sk: &str,
    body: &Map<String, Value>,
) -> Result<String, BpaError> {
    if sk == "self" {
        return Ok(oid.to_string());
    }
    let Some(v) = body
        .get(sk)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    else {
        return Err(BpaError::with_details(
            "MODEL.CONSTRAINT_VIOLATED",
            json!({"field": sk, "message": "the subject key field has no value"}),
        ));
    };
    if v.starts_with("ubos://")
        && let Ok(p) = ubos_types::addr::parse_uri(&json!(v))
        && let (Some(class), Some(slug)) = (s(&p, "class"), s(&p, "slug"))
        && let Some((id, _)) = repo.slug_in(tx, class, slug)?
    {
        return Ok(id);
    }
    Ok(v.to_string())
}

/// The sealer (STD-OBJ-010): the protected fields of a body in their stored form. An
/// erased marker keeps the field as its predecessor stored it.
pub(crate) fn seal(
    repo: &mut Repo<'_>,
    tx: &Tx,
    oid: &str,
    class: &str,
    body: &mut Map<String, Value>,
    predecessor: Option<&str>,
) -> Result<(), BpaError> {
    let Some((sk, fields)) = subject_decl(repo.reg, class) else {
        return Ok(());
    };
    let mut prior: Option<Map<String, Value>> = None;
    for f in fields {
        let Some(v) = body
            .get(&f)
            .filter(|v| !v.is_null() && !is_stored(v))
            .cloned()
        else {
            continue;
        };
        if is_marker(&v) {
            if prior.is_none() {
                let rec = match predecessor {
                    Some(p) => match tx.version_ix.get(p) {
                        Some(i) => tx.versions[*i].1.clone(),
                        None => repo.stored_version(p)?,
                    },
                    None => json!({}),
                };
                prior = Some(rec["body"].as_object().cloned().unwrap_or_default());
            }
            let kept = prior
                .as_ref()
                .and_then(|b| b.get(&f))
                .filter(|e| is_stored(e) && e.get("commitment") == v.get("commitment"))
                .cloned()
                .ok_or_else(|| erased(json!({"field": f})))?;
            body.insert(f, kept);
            continue;
        }
        let subject = subject_of(repo, tx, oid, &sk, body)?;
        let (n, key) = key_for_write(&mut *repo.store, &subject)?;
        let salt = ubos_crypto::random_bytes::<16>()?;
        let nonce = ubos_crypto::random_bytes::<12>()?;
        let pt = canonical(&json!({"salt": ubos_types::addr::b64u(&salt), "value": v}))?;
        let ct = ubos_crypto::gcm_seal(&key, &nonce, &aad(oid, &f), pt.as_bytes())?;
        let e = json!({
            "enc": ALG,
            "key": {"subject_id": subject, "key_version": n},
            "nonce": ubos_types::addr::b64u(&nonce),
            "ct": ubos_types::addr::b64u(&ct),
            "commitment": commitment(&salt, &v)?,
        });
        body.insert(f, e);
    }
    Ok(())
}

/// The opener (STD-OBJ-010, -011): the plaintext of the encrypted fields of a version
/// record, or erased markers.
pub(crate) fn open(store: &mut dyn Store, rec: &mut Value) -> Result<(), BpaError> {
    if !rec["body"]
        .as_object()
        .is_some_and(|b| b.values().any(is_stored))
    {
        return Ok(());
    }
    let oid = s(rec, "object_id").unwrap_or_default().to_string();
    let Some(body) = rec.get_mut("body").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for (f, e) in body.iter_mut() {
        if !is_stored(e) {
            continue;
        }
        *e = match open_field(store, &oid, f, e)? {
            Some(o) => o["value"].clone(),
            None => marker(e),
        };
    }
    Ok(())
}

/// Install the sealer and opener once per process.
pub(crate) fn install() {
    ubos_version::state::set_field_codec(seal, open);
}

impl Engine {
    /// The version record of an object at the read's coordinates, as stored.
    fn stored_record(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let oid = s(i, "object_id").unwrap_or_default().to_string();
        let (o, _, _, _) = self.object_view(&branch, i, &oid)?;
        let a = s(i, "asof")
            .map(str::to_string)
            .unwrap_or_else(|| self.default_valid_of(&o.class));
        let seg = ubos_version::at(&o.timeline, &a)
            .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
        Repo::new(self.store.as_mut(), &self.registry).stored_version(&seg.version)
    }

    /// Adapter operation `stored_form`: the body as stored and hashed (STD-OBJ-010).
    pub fn stored_form(&mut self, i: &Value) -> Result<Value, BpaError> {
        let v = self.stored_record(i)?;
        Ok(json!({"body": v["body"]}))
    }

    /// Adapter operation `disclose {branch, object_id, field}`: the value of an encrypted
    /// field with the salt of its commitment (STD-OBJ-010).
    pub fn disclose(&mut self, i: &Value) -> Result<Value, BpaError> {
        let v = self.stored_record(i)?;
        let f = s(i, "field").unwrap_or_default();
        let e = v["body"]
            .get(f)
            .filter(|e| is_stored(e))
            .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "the field is not encrypted"))?;
        let oid = s(&v, "object_id").unwrap_or_default().to_string();
        match open_field(self.store.as_mut(), &oid, f, e)? {
            None => Err(erased(json!({"field": f}))),
            Some(o) => Ok(json!({"field": f, "value": o["value"], "salt": o["salt"],
                                 "commitment": e["commitment"]})),
        }
    }

    /// Adapter operation `subject_key {subject_id, rotate}`: create or rotate a subject
    /// key; later writes use the new key version (STD-OBJ-010).
    pub fn subject_key(&mut self, i: &Value) -> Result<Value, BpaError> {
        let subject = s(i, "subject_id")
            .filter(|x| !x.is_empty())
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "subject_id"))?
            .to_string();
        let (mut n, _) = key_for_write(self.store.as_mut(), &subject)?;
        if i.get("rotate") == Some(&json!(true)) {
            let mut rec = key_record(self.store.as_mut(), &subject)?.unwrap_or_default();
            n += 1;
            rec["versions"][n.to_string()] = json!(new_key()?);
            rec["current"] = json!(n);
            put_key_record(self.store.as_mut(), &subject, &rec)?;
        }
        Ok(json!({"subject_id": subject, "key_version": n}))
    }

    /// Adapter operation `key_availability {subject_id, available}`: simulate a key the key
    /// service cannot provide for now (DSN-DVM-834).
    pub fn key_availability(&mut self, i: &Value) -> Result<Value, BpaError> {
        let subject = s(i, "subject_id").unwrap_or_default().to_string();
        let mut rec = key_record(self.store.as_mut(), &subject)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown subject key"))?;
        let available = i.get("available") != Some(&json!(false));
        rec["unavailable"] = json!(!available);
        put_key_record(self.store.as_mut(), &subject, &rec)?;
        Ok(json!({"subject_id": subject, "available": available}))
    }

    /// Adapter operation `erase_subject {subject_id, reason}`: destroy a subject key
    /// (STD-OBJ-011). The fields of every version encrypted under it read as erased markers;
    /// the versions, hashes and proofs are unchanged.
    pub fn erase_subject(&mut self, i: &Value) -> Result<Value, BpaError> {
        let subject = s(i, "subject_id").unwrap_or_default().to_string();
        let mut rec = key_record(self.store.as_mut(), &subject)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown subject key"))?;
        let reason = s(i, "reason").filter(|r| !r.is_empty()).ok_or_else(|| {
            BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "an erasure states its reason",
            )
        })?;
        let (versions, fields) = self.subject_versions(&subject)?;
        rec["destroyed"] = json!(true);
        rec["versions"] = json!({});
        rec["erasure"] = json!({"reason": reason, "at": self.now});
        put_key_record(self.store.as_mut(), &subject, &rec)?;
        Ok(
            json!({"subject_id": subject, "erased": true, "erased_versions": versions,
                  "erased_fields": fields}),
        )
    }

    /// The versions of every branch with fields encrypted under a subject's key, and the
    /// number of those fields: the versions of each object's timeline and their
    /// predecessors.
    fn subject_versions(&mut self, subject: &str) -> Result<(u64, u64), BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let (mut versions, mut fields) = (0u64, 0u64);
        for b in repo.store.branch_names()? {
            let Some(head) = repo.store.branch(&b)? else {
                continue;
            };
            let cseq = head.head_cseq;
            for (_, o) in repo.objects_in(&b, cseq, None)? {
                let mut todo: Vec<String> = std::iter::once(o.head.clone())
                    .chain(o.timeline.iter().map(|g| g.version.clone()))
                    .collect();
                while let Some(vh) = todo.pop() {
                    if !seen.insert(vh.clone()) {
                        continue;
                    }
                    let v = repo.stored_version(&vh)?;
                    let n = v["body"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(_, e)| {
                            is_stored(e) && s(&e["key"], "subject_id") == Some(subject)
                        })
                        .count() as u64;
                    if n > 0 {
                        versions += 1;
                        fields += n;
                    }
                    if let Some(p) = s(&v, "predecessor") {
                        todo.push(p.to_string());
                    }
                }
            }
        }
        Ok((versions, fields))
    }
}
