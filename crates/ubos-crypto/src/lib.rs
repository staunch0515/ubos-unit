//! `ubos-crypto` — engine layer (UBS-ARC-07 §2): AEAD encryption and key-service traits.
//!
//! PH-1 step 3b (DEC-048) provides what the store needs for the physical form of stored
//! bytes (DSN-DVM-002): XChaCha20-Poly1305 (STD-CONF-011) under tenant data keys, and a
//! local key provider for development and CI until the key service of the node host (PH-1
//! step 7, DSN-NOD key hierarchy) replaces it. Signing moves here from the prototype crates
//! with PH-1 step 6.

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
