//! `ubos-crypto` — engine layer (UBS-ARC-07 §2): AEAD encryption and key-service traits.
//!
//! PH-1 step 3b (DEC-048) provides what the store needs for the physical form of stored
//! bytes (DSN-DVM-002): XChaCha20-Poly1305 (STD-CONF-011) under tenant data keys, and a
//! local key provider for development and CI until the key service of the node host (PH-1
//! step 7, DSN-NOD key hierarchy) replaces it. PH-1 step 5c (DEC-057) adds Ed25519
//! signatures over content IDs (STD-PROOF-011, DAT-Signature) for the node key of the
//! verification gate; PH-1 step 6a (DEC-059) adds signature verification against a DID
//! document with a key history (STD-PROOF-011, STD-ADDR-010).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde_json::json;
use sha2::{Digest, Sha256};
use ubos_types::BpaError;

/// The types crate, re-exported for crates whose allowed dependencies name only this one
/// (`ubos-proof`, AR-031).
pub use ubos_types as types;

/// Length of an XChaCha20-Poly1305 nonce.
pub const NONCE_LEN: usize = 24;

/// A data key with its identifier (generation) within a tenant.
#[derive(Clone)]
pub struct DataKey {
    /// Key identifier, stored with every ciphertext for rotation (FR-IAM-033).
    pub id: u32,
    /// Key bytes.
    pub key: [u8; 32],
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DataKey({})", self.id)
    }
}

/// Source of tenant data keys (key hierarchy: root KEK → tenant KEK → tenant data keys).
pub trait KeyService: Send + Sync {
    /// The current data key of a tenant, used for new writes.
    fn current(&self, tenant: &str) -> Result<DataKey, BpaError>;
    /// A data key of a tenant by identifier, used for reads.
    fn by_id(&self, tenant: &str, id: u32) -> Result<DataKey, BpaError>;
}

/// Development key provider: data keys derived from one local master secret. Not a key
/// service for production (DEC-047, DEC-048).
pub struct LocalKeys {
    master: [u8; 32],
    current: u32,
}

impl LocalKeys {
    /// Keys derived from a master secret; `current` is the generation used for writes.
    pub fn new(master: [u8; 32], current: u32) -> LocalKeys {
        LocalKeys { master, current }
    }

    /// Keys from `UBOS_LOCAL_MASTER_KEY` (64 hex digits), or a fixed development secret.
    pub fn from_env() -> Result<LocalKeys, BpaError> {
        let mut master = *b"ubos-local-development-master-00";
        if let Ok(hex) = std::env::var("UBOS_LOCAL_MASTER_KEY") {
            let bad = || {
                BpaError::new(
                    "IAM.KEY_UNAVAILABLE",
                    "UBOS_LOCAL_MASTER_KEY is not 64 hex digits",
                )
            };
            if hex.len() != 64 {
                return Err(bad());
            }
            for (i, b) in master.iter_mut().enumerate() {
                *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
            }
        }
        Ok(LocalKeys::new(master, 1))
    }

    fn derive(&self, tenant: &str, id: u32) -> DataKey {
        let mut h = Sha256::new();
        h.update(b"ubos.local-data-key.v1\0");
        h.update(self.master);
        h.update(tenant.as_bytes());
        h.update(id.to_be_bytes());
        DataKey {
            id,
            key: h.finalize().into(),
        }
    }
}

impl KeyService for LocalKeys {
    fn current(&self, tenant: &str) -> Result<DataKey, BpaError> {
        Ok(self.derive(tenant, self.current))
    }
    fn by_id(&self, tenant: &str, id: u32) -> Result<DataKey, BpaError> {
        if id == 0 || id > self.current {
            return Err(BpaError::new(
                "IAM.KEY_UNAVAILABLE",
                format!("unknown data key {id}"),
            ));
        }
        Ok(self.derive(tenant, id))
    }
}

/// Random bytes from the operating system.
pub fn random_bytes<const N: usize>() -> Result<[u8; N], BpaError> {
    let mut out = [0u8; N];
    getrandom::getrandom(&mut out)
        .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?;
    Ok(out)
}

/// Encrypt with XChaCha20-Poly1305: ciphertext with the tag appended (STD-CONF-011).
pub fn seal(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    plain: &[u8],
) -> Result<Vec<u8>, BpaError> {
    XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(nonce), Payload { msg: plain, aad })
        .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "encryption failed"))
}

/// Decrypt and authenticate; a wrong key, nonce, associated data or a modified ciphertext
/// fails with `NOD.BLOB_UNAVAILABLE` and `{"integrity": true}` (DSN-DVM-002).
pub fn open(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    sealed: &[u8],
) -> Result<Vec<u8>, BpaError> {
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad })
        .map_err(|_| integrity("authentication failed"))
}

/// `NOD.BLOB_UNAVAILABLE` with the integrity flag (DSN-DVM-002).
pub fn integrity(reason: &str) -> BpaError {
    let mut e = BpaError::with_details("NOD.BLOB_UNAVAILABLE", json!({"integrity": true}));
    e.message = reason.to_string();
    e
}

/// HMAC-SHA-256 (RFC 2104) of `data` under `key`, for blind indexes (STD-OBJ-012).
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// Bytes of a lowercase hexadecimal text, optionally of a fixed length (STD-CONF-011).
fn unhex(v: &str, n: Option<usize>, what: &str) -> Result<Vec<u8>, BpaError> {
    let ok = v.len().is_multiple_of(2)
        && v.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && n.is_none_or(|n| v.len() == 2 * n);
    if !ok {
        let len = n.map(|n| format!("{n} bytes of ")).unwrap_or_default();
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            format!("{what} is {len}lowercase hexadecimal"),
        ));
    }
    Ok((0..v.len() / 2)
        .map(|i| u8::from_str_radix(&v[2 * i..2 * i + 2], 16).unwrap_or_default())
        .collect())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `aead_decrypt` (STD-CONF-011): the plaintext of an AES-256-GCM or XChaCha20-Poly1305
/// ciphertext, all values in lowercase hexadecimal; `PROOF.VERIFICATION_FAILED` when the
/// authentication tag does not verify.
pub fn aead_decrypt(
    alg: &str,
    key: &str,
    nonce: &str,
    ciphertext: &str,
    aad: &str,
) -> Result<String, BpaError> {
    let failed = || {
        BpaError::new(
            "PROOF.VERIFICATION_FAILED",
            "the authentication tag does not verify",
        )
    };
    let out = match alg {
        "aes-256-gcm" => {
            use aes_gcm::{Aes256Gcm, Nonce};
            let k = unhex(key, Some(32), "key")?;
            let n = unhex(nonce, Some(12), "nonce")?;
            let ct = unhex(ciphertext, None, "ciphertext")?;
            let a = unhex(aad, None, "aad")?;
            Aes256Gcm::new_from_slice(&k)
                .map_err(|_| failed())?
                .decrypt(Nonce::from_slice(&n), Payload { msg: &ct, aad: &a })
        }
        "xchacha20-poly1305" => {
            let k = unhex(key, Some(32), "key")?;
            let n = unhex(nonce, Some(NONCE_LEN), "nonce")?;
            let ct = unhex(ciphertext, None, "ciphertext")?;
            let a = unhex(aad, None, "aad")?;
            XChaCha20Poly1305::new_from_slice(&k)
                .map_err(|_| failed())?
                .decrypt(XNonce::from_slice(&n), Payload { msg: &ct, aad: &a })
        }
        _ => {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "unregistered AEAD algorithm",
            ));
        }
    };
    out.map(|p| hex(&p)).map_err(|_| failed())
}

/// Signature roles of DAT-Signature.
pub const ROLES: &[&str] = &["author", "node", "approver", "signer", "notary", "tagger"];

/// The Ed25519 public key of a 32-byte seed.
pub fn ed25519_public(seed: &[u8; 32]) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(seed)
        .verifying_key()
        .to_bytes()
}

/// An Ed25519 signature over `msg` with the key of `seed`.
pub fn ed25519_sign(seed: &[u8; 32], msg: &[u8]) -> [u8; 64] {
    use ed25519_dalek::Signer;
    ed25519_dalek::SigningKey::from_bytes(seed)
        .sign(msg)
        .to_bytes()
}

/// A DAT-Signature of a content ID: the key of `seed` signs the UTF-8 bytes of the content
/// ID (STD-PROOF-011).
pub fn sign(
    seed: &[u8; 32],
    content_id: &str,
    signer: &str,
    key_id: &str,
    role: &str,
) -> Result<serde_json::Value, BpaError> {
    if !ROLES.contains(&role) {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "unknown signature role",
        ));
    }
    Ok(
        json!({"role": role, "signer": signer, "key_id": key_id, "alg": "ed25519",
        "sig": ubos_types::addr::b64u(&ed25519_sign(seed, content_id.as_bytes()))}),
    )
}

/// 32 bytes from lowercase hexadecimal (`what` names the value in the error).
pub fn unhex32(v: &serde_json::Value, what: &str) -> Result<[u8; 32], BpaError> {
    let bad = || {
        BpaError::new(
            "MODEL.INVALID_VALUE",
            format!("{what} is 32 bytes of lowercase hexadecimal"),
        )
    };
    let s = v.as_str().ok_or_else(bad)?;
    if s.len() != 64
        || !s
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(bad());
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
    }
    Ok(out)
}

/// Registered signature algorithms (STD-CONF-011).
pub const SIGNATURE_ALGORITHMS: &[&str] = &["ed25519"];

/// Whether `sig` is a valid Ed25519 signature of `msg` under `public`.
pub fn ed25519_verify(public: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    use ed25519_dalek::Verifier;
    match ed25519_dalek::VerifyingKey::from_bytes(public) {
        Ok(k) => k
            .verify(msg, &ed25519_dalek::Signature::from_bytes(sig))
            .is_ok(),
        Err(_) => false,
    }
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

fn signature_invalid(m: &str) -> BpaError {
    BpaError::new("PROOF.SIGNATURE_INVALID", m)
}

/// The shape of a DAT-Signature (`MODEL.INVALID_VALUE` otherwise); returns the signature
/// bytes.
pub fn check_signature(s: &serde_json::Value) -> Result<[u8; 64], BpaError> {
    let o = s
        .as_object()
        .filter(|o| {
            ["role", "signer", "key_id", "alg", "sig"]
                .iter()
                .all(|k| o.contains_key(*k))
        })
        .ok_or_else(|| invalid("a signature is DAT-Signature"))?;
    if !o["role"].as_str().is_some_and(|r| ROLES.contains(&r)) {
        return Err(invalid("unknown signature role"));
    }
    if !o["alg"]
        .as_str()
        .is_some_and(|a| SIGNATURE_ALGORITHMS.contains(&a))
    {
        return Err(invalid("unregistered signature algorithm"));
    }
    let bytes = ubos_types::addr::unb64u(&o["sig"], 64, "sig")?;
    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// STD-PROOF-011: the key named by `key_id` verifies `signature` over the content ID and was
/// valid for the signer at `at` (half-open `[valid_from, valid_to)`) according to the key
/// history of the DID document `did` (`{id, keys}`, STD-ADDR-010). `at` is omitted when the
/// time is not known. A failing check is `PROOF.SIGNATURE_INVALID`; a malformed input is
/// `MODEL.INVALID_VALUE`.
pub fn verify(
    signature: &serde_json::Value,
    content_id: &str,
    did: &serde_json::Value,
    at: Option<&str>,
) -> Result<bool, BpaError> {
    let sig = check_signature(signature)?;
    let keys = did
        .as_object()
        .and_then(|d| d.get("keys"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("a DID document is {id, keys}"))?;
    if signature["signer"] != did["id"] {
        return Err(signature_invalid(
            "the signer is not the subject of the DID document",
        ));
    }
    let k = keys
        .iter()
        .find(|k| k.get("key_id") == signature.get("key_id"))
        .ok_or_else(|| signature_invalid("unknown key"))?;
    if let Some(at) = at {
        let from = k.get("valid_from").and_then(serde_json::Value::as_str);
        let to = k.get("valid_to").and_then(serde_json::Value::as_str);
        if from.is_some_and(|f| at < f) || to.is_some_and(|t| at >= t) {
            return Err(signature_invalid(
                "the key was not valid at the signed time",
            ));
        }
    }
    let pk = ubos_types::addr::unb64u(
        k.get("public_key").unwrap_or(&serde_json::Value::Null),
        32,
        "public_key",
    )?;
    let mut public = [0u8; 32];
    public.copy_from_slice(&pk);
    if !ed25519_verify(&public, content_id.as_bytes(), &sig) {
        return Err(signature_invalid("the signature does not verify"));
    }
    Ok(true)
}

/// HKDF-SHA-256 (RFC 5869), one 32-byte output block: `HKDF-Expand(HKDF-Extract(salt, ikm),
/// info, 32)`. Used by the key hierarchy of the node host (DSN-NOD-131): tenant KEK → data
/// keys and node signing keys.
pub fn hkdf_sha256(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let prk = hmac_sha256(salt, ikm);
    let mut msg = info.to_vec();
    msg.push(1);
    hmac_sha256(&prk, &msg)
}
