//! `ubos-crypto` — engine layer (UBS-ARC-07 §2): AEAD encryption and key-service traits.
//!
//! PH-1 step 3b (DEC-048) provides what the store needs for the physical form of stored
//! bytes (DSN-DVM-002): XChaCha20-Poly1305 (STD-CONF-011) under tenant data keys, and a
//! local key provider for development and CI until the key service of the node host (PH-1
//! step 7, DSN-NOD key hierarchy) replaces it. PH-1 step 5c (DEC-057) adds Ed25519
//! signatures over content IDs (STD-PROOF-011, DAT-Signature) for the node key of the
//! verification gate; signature verification and key histories follow with step 6.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde_json::json;
use sha2::{Digest, Sha256};
use ubos_types::BpaError;

/// Length of an XChaCha20-Poly1305 nonce.
pub const NONCE_LEN: usize = 24;

/// A data key with its identifier (generation) within a tenant.
#[derive(Clone)]
pub struct DataKey {
    /// Key identifier, stored with every ciphertext for rotation (FR-IAM-095).
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
