//! `ubos-proof` — engine layer (UBS-ARC-07 §2).
//!
//! Evidence formats and verification primitives shared with the independent verifier
//! `ubos-verify` (PH-1 step 6, DEC-059). Allowed dependencies: `ubos-tree`, `ubos-crypto`
//! (AR-031); nothing here reads a store, so a verifier needs no kernel.
//!
//! * inclusion proofs over the state tree (STD-PROOF-020): [`inclusion_proof`] builds one
//!   from the entries of a tree, [`verify_inclusion`] checks one against a root or a signed
//!   commit record;
//! * signed commit records (STD-PROOF-011): [`commit_check`] recomputes the commit ID and
//!   verifies every signature against the DID documents given;
//! * consistency proofs (STD-PROOF-021): [`verify_consistency`] checks a first-parent chain
//!   of signed commit records.
//!
//! The portable archive (STD-PROOF-070) follows in step 6b.

use serde_json::{Value, json};
use ubos_crypto::types::{BpaError, content_id};
use ubos_tree::{is_boundary, node_hash, tree_root};

/// `PROOF.VERIFICATION_FAILED`.
pub fn failed(m: &str) -> BpaError {
    BpaError::new("PROOF.VERIFICATION_FAILED", m)
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

/// `[key, hash]` pairs from a JSON list (`MODEL.INVALID_VALUE` otherwise).
pub fn entries(v: &Value) -> Result<Vec<(String, String)>, BpaError> {
    v.as_array()
        .ok_or_else(|| invalid("entries is a list"))?
        .iter()
        .map(|e| match e.as_array().map(Vec::as_slice) {
            Some([Value::String(k), Value::String(h)]) => Ok((k.clone(), h.clone())),
            _ => Err(invalid("entry is [key, hash]")),
        })
        .collect()
}

fn entries_json(ents: &[(String, String)]) -> Value {
    Value::Array(ents.iter().map(|(k, h)| json!([k, h])).collect())
}

/// `{key, value, nodes}`: the canonical nodes on the path from the leaf holding `key` to the
/// root of the prolly tree over `entries` (STD-PROOF-001, -020). `IAM.NOT_FOUND` when the
/// key is not in the tree.
pub fn inclusion_proof(entries: &[(String, String)], key: &str) -> Result<Value, BpaError> {
    let mut items: Vec<(String, String)> = entries.to_vec();
    items.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    tree_root(&items)?; // rejects duplicate keys
    let value = items
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, h)| h.clone())
        .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "key not in the tree"))?;
    let mut level: u32 = 0;
    let mut path = Vec::new();
    let mut target = key.to_string();
    loop {
        let mut nodes: Vec<Vec<(String, String)>> = Vec::new();
        let mut current = Vec::new();
        for (k, h) in &items {
            current.push((k.clone(), h.clone()));
            if is_boundary(level, k) {
                nodes.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            nodes.push(current);
        }
        let mine = nodes
            .iter()
            .find(|n| n.iter().any(|(k, _)| *k == target))
            .cloned()
            .ok_or_else(|| failed("the tree lost the key"))?;
        path.push(json!({"level": level, "entries": entries_json(&mine)}));
        if nodes.len() == 1 {
            return Ok(json!({"key": key, "value": value, "nodes": path}));
        }
        target = mine[0].0.clone();
        items = nodes
            .iter()
            .map(|n| Ok((n[0].0.clone(), node_hash(level, n)?)))
            .collect::<Result<_, BpaError>>()?;
        level += 1;
    }
}

/// The commit ID of a signed commit record, after verifying every signature against the DID
/// documents in `dids` (STD-PROOF-011). The ID is the content ID of the record without its
/// `signatures`; `tx_time` is the time the keys are checked at.
pub fn commit_check(commit: &Value, dids: &[Value]) -> Result<String, BpaError> {
    let sigs = commit
        .as_object()
        .and_then(|c| c.get("signatures"))
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| failed("a commit record carries signatures"))?;
    let mut rec = commit.as_object().cloned().unwrap_or_default();
    rec.remove("signatures");
    let cid = content_id("ubos.commit.v1", &Value::Object(rec))?;
    let at = commit.get("tx_time").and_then(Value::as_str);
    for s in sigs {
        let signer = s.get("signer");
        let doc = dids
            .iter()
            .find(|d| signer.is_some() && d.get("id") == signer)
            .ok_or_else(|| {
                BpaError::new("PROOF.SIGNATURE_INVALID", "no DID document for the signer")
            })?;
        ubos_crypto::verify(s, &cid, doc, at)?;
    }
    Ok(cid)
}

/// Verify an inclusion proof (STD-PROOF-020) against `root`, or against the signed commit
/// record the proof carries (`proof.commit`, checked with `dids`), or both.
pub fn verify_inclusion(
    proof: &Value,
    root: Option<&str>,
    dids: &[Value],
) -> Result<bool, BpaError> {
    let nodes = proof
        .as_object()
        .and_then(|p| p.get("nodes"))
        .and_then(Value::as_array)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| invalid("an inclusion proof is DAT-InclusionProof"))?;
    let leaf = json!([proof.get("key"), proof.get("value")]);
    let holds = nodes[0]
        .get("entries")
        .and_then(Value::as_array)
        .is_some_and(|e| e.contains(&leaf));
    if !holds || nodes[0].get("level") != Some(&json!(0)) {
        return Err(failed("the leaf does not hold the key and value"));
    }
    let mut h = String::new();
    for (i, n) in nodes.iter().enumerate() {
        let ents = entries(n.get("entries").unwrap_or(&Value::Null))
            .map_err(|_| failed("a node is not canonical"))?;
        let sorted = ents
            .windows(2)
            .all(|w| w[0].0.as_bytes() < w[1].0.as_bytes());
        if ents.is_empty() || !sorted {
            return Err(failed("a node is not canonical"));
        }
        let level = n.get("level").and_then(Value::as_u64);
        if i > 0 {
            let prev = &nodes[i - 1];
            let first = prev["entries"][0][0].as_str().unwrap_or_default();
            let linked = ents.iter().any(|(k, v)| k == first && *v == h);
            let next_level = prev.get("level").and_then(Value::as_u64).map(|l| l + 1) == level;
            if !linked || !next_level {
                return Err(failed("the hash chain is broken"));
            }
        }
        let level = level.ok_or_else(|| failed("a node is not canonical"))?;
        h = node_hash(
            u32::try_from(level).map_err(|_| failed("a node is not canonical"))?,
            &ents,
        )?;
    }
    if let Some(commit) = proof.get("commit") {
        commit_check(commit, dids)?;
        if commit.get("root") != Some(&json!(h)) {
            return Err(failed("the root is not the root of the commit"));
        }
    } else if root.is_none() {
        return Err(invalid("a root or a signed commit is needed"));
    }
    if root.is_some_and(|r| r != h) {
        return Err(failed("the proof does not lead to the root"));
    }
    Ok(true)
}

/// Verify a consistency proof (STD-PROOF-021): `commits` are signed records from the later
/// commit back to the earlier one along first parents; `from` and `to` pin the ends.
pub fn verify_consistency(
    commits: &Value,
    from: Option<&str>,
    to: Option<&str>,
    dids: &[Value],
) -> Result<bool, BpaError> {
    let commits = commits
        .as_array()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| invalid("a consistency proof is a list of commit records"))?;
    let ids = commits
        .iter()
        .map(|c| commit_check(c, dids))
        .collect::<Result<Vec<_>, _>>()?;
    for i in 0..commits.len() - 1 {
        let first_parent = commits[i]
            .get("parents")
            .and_then(Value::as_array)
            .and_then(|p| p.first())
            .and_then(Value::as_str);
        if first_parent != Some(ids[i + 1].as_str()) {
            return Err(failed("a first-parent link is broken"));
        }
        let a = commits[i].get("tx_time").and_then(Value::as_str);
        let b = commits[i + 1].get("tx_time").and_then(Value::as_str);
        match (a, b) {
            (None, _) => return Err(failed("transaction times do not increase")),
            (Some(a), Some(b)) if b >= a => {
                return Err(failed("transaction times do not increase"));
            }
            _ => {}
        }
    }
    if to.is_some_and(|t| ids[0] != t) {
        return Err(failed("the proof does not start at the later commit"));
    }
    if from.is_some_and(|f| ids[ids.len() - 1] != f) {
        return Err(failed("the proof does not end at the earlier commit"));
    }
    Ok(true)
}

// ---- portable archive (STD-PROOF-070) ----

/// Format name of the portable archive.
pub const ARCHIVE_FORMAT: &str = "ubos.archive.v1";

fn obj<'a>(v: &'a Value, k: &str) -> &'a serde_json::Map<String, Value> {
    static EMPTY: std::sync::OnceLock<serde_json::Map<String, Value>> = std::sync::OnceLock::new();
    v.get(k)
        .and_then(Value::as_object)
        .unwrap_or_else(|| EMPTY.get_or_init(serde_json::Map::new))
}

/// Verify a portable archive (STD-PROOF-070) without a store: format, manifest signature by
/// a node whose DID document the archive carries, every content ID (chunks, version
/// envelopes, entry records, commit records with their signatures), the parents and the
/// branch heads named, and the counts of the manifest. The state roots are checked by the
/// importer, which rebuilds the states. Returns the DID documents by subject.
pub fn verify_archive(archive: &Value) -> Result<Vec<Value>, BpaError> {
    let m = archive
        .get("manifest")
        .filter(|m| m.is_object())
        .ok_or_else(|| invalid(&format!("not a {ARCHIVE_FORMAT} archive")))?;
    if m.get("format") != Some(&json!(ARCHIVE_FORMAT)) {
        return Err(invalid(&format!("not a {ARCHIVE_FORMAT} archive")));
    }
    let dids: Vec<Value> = archive
        .get("dids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let sig = m.get("signature").filter(|s| s.is_object());
    let doc = sig.and_then(|s| dids.iter().find(|d| d.get("id") == s.get("signer")));
    let (Some(sig), Some(doc)) = (sig, doc) else {
        return Err(BpaError::new(
            "PROOF.SIGNATURE_INVALID",
            "the manifest is not signed by a known node",
        ));
    };
    let mut unsigned = m.as_object().cloned().unwrap_or_default();
    unsigned.remove("signature");
    ubos_crypto::verify(
        sig,
        &content_id("ubos.chunk.v1", &Value::Object(unsigned))?,
        doc,
        None,
    )?;
    let chunks = obj(archive, "chunks");
    for (cid, v) in chunks {
        if content_id("ubos.chunk.v1", v)? != *cid {
            return Err(failed(&format!(
                "chunk {cid} does not match its content ID"
            )));
        }
    }
    let versions = obj(archive, "versions");
    for (vh, env) in versions {
        let body = env.get("body_ref").and_then(Value::as_str);
        if content_id("ubos.version.v1", env)? != *vh
            || !body.is_some_and(|b| chunks.contains_key(b))
        {
            return Err(failed(&format!("version {vh} does not match its envelope")));
        }
    }
    let entries = obj(archive, "entries");
    for (eh, rec) in entries {
        let body = rec.get("body_ref").and_then(Value::as_str);
        if content_id("ubos.entry.v1", rec)? != *eh || !body.is_some_and(|b| chunks.contains_key(b))
        {
            return Err(failed(&format!("entry {eh} does not match its record")));
        }
    }
    let commits = obj(archive, "commits");
    for (cid, c) in commits {
        let rec = c.get("record").unwrap_or(&Value::Null);
        if commit_check(rec, &dids)? != *cid {
            return Err(failed(&format!("commit {cid} does not match its record")));
        }
        let missing = rec
            .get("parents")
            .and_then(Value::as_array)
            .is_some_and(|ps| {
                ps.iter()
                    .any(|p| !p.as_str().is_some_and(|p| commits.contains_key(p)))
            });
        if missing {
            return Err(failed(&format!("a parent of commit {cid} is missing")));
        }
    }
    let refs = obj(archive, "refs");
    for r in refs.values() {
        let known = |k: &str| {
            r.get(k)
                .and_then(Value::as_str)
                .is_some_and(|c| commits.contains_key(c))
        };
        if !known("head") || !known("created_from") {
            return Err(failed("a branch names a missing commit"));
        }
    }
    let counts = json!({"chunks": chunks.len(), "versions": versions.len(), "entries": entries.len(),
        "commits": commits.len(), "refs": refs.len(), "tags": obj(archive, "tags").len()});
    if m.get("counts") != Some(&counts) {
        return Err(failed("the counts differ from the manifest"));
    }
    Ok(dids)
}
