//! Node identity of the adapter (STD-PROOF-010, STD-ADDR-010; PH-1 step 6a, DEC-059).
//!
//! The node record `{seed, n, keys}` is kept under the settings key `node`; until a key is
//! rotated it is the convention of STD-CONF-005 (convention 10): DID
//! `did:ubos:acme:01900000-0000-7000-8000-000000000001` with the seed `01` × 32 as key `#k1`.
//! The record lives here rather than in the DVM facade so that the version layer can sign
//! every commit record at flush time (STD-PROOF-011). The key service of the node host
//! (PH-1 step 7) replaces the seed in the record.

use crate::Store;
use serde_json::{Value, json};
use ubos_crypto::types::BpaError;

/// The adapter's node DID (STD-CONF-005, convention 10).
pub const NODE_DID: &str = "did:ubos:acme:01900000-0000-7000-8000-000000000001";
/// The adapter's initial node seed.
pub const NODE_SEED: [u8; 32] = [1u8; 32];
/// Settings key of the node record.
pub const META_KEY: &str = "node";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The node record `{seed, n, keys}`: the stored one, or the initial convention.
pub fn record(store: &mut dyn Store) -> Result<Value, BpaError> {
    if let Some(n) = store.meta(META_KEY)? {
        return Ok(n);
    }
    Ok(
        json!({"seed": hex(&NODE_SEED), "n": 1, "keys": [{"key_id": format!("{NODE_DID}#k1"),
        "public_key": ubos_crypto::types::addr::b64u(&ubos_crypto::ed25519_public(&NODE_SEED))}]}),
    )
}

/// A DAT-Signature of a content ID by the node's current key.
pub fn sign(store: &mut dyn Store, cid: &str, role: &str) -> Result<Value, BpaError> {
    let n = record(store)?;
    let seed = ubos_crypto::unhex32(&n["seed"], "seed")?;
    let key_id = n["keys"]
        .as_array()
        .and_then(|k| k.last())
        .and_then(|k| k.get("key_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    ubos_crypto::sign(&seed, cid, NODE_DID, &key_id, role)
}

/// The node's DID document `{id, keys}` with its key history (STD-ADDR-010).
pub fn did(store: &mut dyn Store) -> Result<Value, BpaError> {
    let n = record(store)?;
    Ok(json!({"id": NODE_DID, "keys": n["keys"]}))
}

/// Rotate the node key (STD-PROOF-010): the current key ends at `at` (exclusive), the key
/// of `seed` starts there as `#k<n+1>`. Returns the new DID document.
pub fn rotate(store: &mut dyn Store, seed: &Value, at: &str) -> Result<Value, BpaError> {
    let bytes = ubos_crypto::unhex32(seed, "seed")?;
    let mut n = record(store)?;
    let count = n["n"].as_u64().unwrap_or(1) + 1;
    if let Some(last) = n["keys"].as_array_mut().and_then(|k| k.last_mut()) {
        last["valid_to"] = json!(at);
    }
    if let Some(k) = n["keys"].as_array_mut() {
        k.push(json!({"key_id": format!("{NODE_DID}#k{count}"),
            "public_key": ubos_crypto::types::addr::b64u(&ubos_crypto::ed25519_public(&bytes)),
            "valid_from": at}));
    }
    n["n"] = json!(count);
    n["seed"] = json!(hex(&bytes));
    store.put_meta(META_KEY, &n)?;
    did(store)
}
