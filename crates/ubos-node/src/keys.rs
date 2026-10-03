//! The key hierarchy of a tenant (DSN-NOD-131): root KEK → tenant KEK (stored wrapped in the
//! registry) → data keys and node signing keys derived with HKDF, never stored; the node
//! signer (DSN-NOD-134) and the secret store (DSN-NOD-130).

use crate::db::{Db, Param};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use ubos_dvm::crypto::{self, DataKey, KeyService};
use ubos_dvm::host::Secrets;
use ubos_dvm::store::node::NodeSigner;
use ubos_dvm::types::BpaError;

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn unavailable(m: &str) -> BpaError {
    BpaError::new("IAM.KEY_UNAVAILABLE", m)
}

/// The unwrapped key hierarchy of one tenant.
pub struct Hierarchy {
    db: Arc<Db>,
    tenant: String,
    kek: [u8; 32],
    current: Mutex<u32>,
}

impl std::fmt::Debug for Hierarchy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hierarchy({})", self.tenant)
    }
}

impl Hierarchy {
    /// Unwrap the tenant KEK with the root KEK (`IAM.KEY_UNAVAILABLE` when the root key
    /// does not open it).
    pub fn open(db: Arc<Db>, root: [u8; 32], tenant: &str) -> Result<Hierarchy, BpaError> {
        let rows = db.query(
            "SELECT kek_wrapped, kek_nonce, data_key FROM tenant WHERE tenant_id = $1::uuid",
            &[Param::Text(tenant)],
        )?;
        let row = rows
            .first()
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("unknown tenant {tenant}")))?;
        let wrapped = unhex(row["kek_wrapped"].as_str().unwrap_or_default());
        let nonce = unhex(row["kek_nonce"].as_str().unwrap_or_default());
        let nonce: [u8; crypto::NONCE_LEN] = nonce
            .try_into()
            .map_err(|_| unavailable("the tenant KEK nonce is malformed"))?;
        let kek = crypto::open(&root, &nonce, tenant.as_bytes(), &wrapped)
            .map_err(|_| unavailable("the root KEK does not open the tenant KEK"))?;
        let kek: [u8; 32] = kek
            .try_into()
            .map_err(|_| unavailable("the tenant KEK is not 32 bytes"))?;
        Ok(Hierarchy {
            db,
            tenant: tenant.to_string(),
            kek,
            current: Mutex::new(row["data_key"].as_u64().unwrap_or(1) as u32),
        })
    }

    /// Derive a 32-byte key of the tenant for a purpose and index.
    pub fn derive(&self, purpose: &str, index: &str) -> [u8; 32] {
        crypto::hkdf_sha256(
            &self.kek,
            self.tenant.as_bytes(),
            format!("{purpose}\0{index}").as_bytes(),
        )
    }

    /// Rotate the data key (FR-IAM-095): new writes use the next generation; old
    /// generations stay readable.
    pub fn rotate_data_key(&self) -> Result<u32, BpaError> {
        let next = *self.current.lock().map_err(|_| err("lock"))? + 1;
        self.db.execute(
            "UPDATE tenant SET data_key = $2 WHERE tenant_id = $1::uuid",
            &[Param::Text(&self.tenant), Param::Int(i64::from(next))],
        )?;
        *self.current.lock().map_err(|_| err("lock"))? = next;
        Ok(next)
    }

    /// The current data key generation.
    pub fn current_data_key(&self) -> u32 {
        self.current.lock().map(|g| *g).unwrap_or(1)
    }
}

impl KeyService for Hierarchy {
    fn current(&self, _tenant: &str) -> Result<DataKey, BpaError> {
        let id = self.current_data_key();
        Ok(DataKey {
            id,
            key: self.derive("ubos.data-key.v1", &id.to_string()),
        })
    }
    fn by_id(&self, _tenant: &str, id: u32) -> Result<DataKey, BpaError> {
        if id == 0 || id > self.current_data_key() {
            return Err(unavailable(&format!("unknown data key {id}")));
        }
        Ok(DataKey {
            id,
            key: self.derive("ubos.data-key.v1", &id.to_string()),
        })
    }
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or_default())
        .collect()
}

/// The node signing key of a VAE (DSN-NOD-134, STD-PROOF-010): seed `n` is derived from
/// the tenant KEK; the public key history lives in `node_key`. The DID is the registered
/// form `did:ubos:<authority>:<vae id>`.
pub struct Signer {
    db: Arc<Db>,
    keys: Arc<Hierarchy>,
    tenant: String,
    vae: String,
    did: String,
}

impl Signer {
    /// The signer of a VAE; the first key is registered when none exists.
    pub fn new(
        db: Arc<Db>,
        keys: Arc<Hierarchy>,
        tenant: &str,
        vae: &str,
        authority: &str,
    ) -> Signer {
        Signer {
            db,
            keys,
            tenant: tenant.to_string(),
            vae: vae.to_string(),
            did: format!("did:ubos:{authority}:{vae}"),
        }
    }

    fn seed(&self, n: u32) -> [u8; 32] {
        self.keys
            .derive("ubos.node-signing.v1", &format!("{}\0{n}", self.vae))
    }

    fn key_id(&self, n: u32) -> String {
        format!("{}#k{n}", self.did)
    }

    /// The key rows, registering `#k1` when the VAE has none.
    fn rows(&self) -> Result<Vec<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT n, public_key, valid_from, valid_to FROM node_key WHERE tenant_id = $1::uuid AND vae_id = $2::uuid ORDER BY n",
            &[Param::Text(&self.tenant), Param::Text(&self.vae)],
        )?;
        if !rows.is_empty() {
            return Ok(rows);
        }
        let pk = ubos_dvm::types::addr::b64u(&crypto::ed25519_public(&self.seed(1)));
        self.db.execute(
            "INSERT INTO node_key (tenant_id, vae_id, n, public_key) VALUES ($1::uuid, $2::uuid, 1, $3) ON CONFLICT DO NOTHING",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(&pk)],
        )?;
        Ok(vec![
            json!({"n": 1, "public_key": pk, "valid_from": null, "valid_to": null}),
        ])
    }
}

impl NodeSigner for Signer {
    fn did(&self) -> Result<Value, BpaError> {
        let keys: Vec<Value> = self
            .rows()?
            .iter()
            .map(|r| {
                let n = r["n"].as_u64().unwrap_or(1) as u32;
                let mut k = json!({"key_id": self.key_id(n), "public_key": r["public_key"]});
                for f in ["valid_from", "valid_to"] {
                    if !r[f].is_null() {
                        k[f] = r[f].clone();
                    }
                }
                k
            })
            .collect();
        Ok(json!({"id": self.did, "keys": keys}))
    }

    fn sign(&self, cid: &str, role: &str) -> Result<Value, BpaError> {
        let rows = self.rows()?;
        let n = rows.last().and_then(|r| r["n"].as_u64()).unwrap_or(1) as u32;
        crypto::sign(&self.seed(n), cid, &self.did, &self.key_id(n), role)
    }

    fn rotate(&self, at: &str) -> Result<Value, BpaError> {
        let rows = self.rows()?;
        let n = rows.last().and_then(|r| r["n"].as_u64()).unwrap_or(1) as u32;
        let next = n + 1;
        let pk = ubos_dvm::types::addr::b64u(&crypto::ed25519_public(&self.seed(next)));
        self.db.execute(
            "UPDATE node_key SET valid_to = $4 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND n = $3",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Int(i64::from(n)), Param::Text(at)],
        )?;
        self.db.execute(
            "INSERT INTO node_key (tenant_id, vae_id, n, public_key, valid_from) VALUES ($1::uuid, $2::uuid, $3, $4, $5)",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Int(i64::from(next)), Param::Text(&pk), Param::Text(at)],
        )?;
        self.did()
    }
}

/// Secrets of a tenant (DSN-NOD-130): sealed under the current data key with the handle
/// as associated data; resolved only inside connector execution.
pub struct SecretStore {
    db: Arc<Db>,
    keys: Arc<Hierarchy>,
}

impl SecretStore {
    /// The secret store of a tenant.
    pub fn new(db: Arc<Db>, keys: Arc<Hierarchy>) -> SecretStore {
        SecretStore { db, keys }
    }

    /// Store (or replace) a secret under a handle.
    pub fn put(&self, handle: &str, secret: &[u8]) -> Result<(), BpaError> {
        let k = self.keys.current("")?;
        let nonce: [u8; crypto::NONCE_LEN] = crypto::random_bytes()?;
        let sealed = crypto::seal(&k.key, &nonce, handle.as_bytes(), secret)?;
        let tenant = self.keys.tenant.clone();
        self.db.execute(
            "INSERT INTO secret (tenant_id, handle, key_id, nonce, sealed) VALUES ($1::uuid, $2, $3, $4, $5) ON CONFLICT (tenant_id, handle) DO UPDATE SET key_id = EXCLUDED.key_id, nonce = EXCLUDED.nonce, sealed = EXCLUDED.sealed",
            &[Param::Text(&tenant), Param::Text(handle), Param::Int(i64::from(k.id)), Param::Bytes(&nonce), Param::Bytes(&sealed)],
        )?;
        Ok(())
    }

    /// Remove a secret.
    pub fn delete(&self, handle: &str) -> Result<bool, BpaError> {
        let tenant = self.keys.tenant.clone();
        Ok(self.db.execute(
            "DELETE FROM secret WHERE tenant_id = $1::uuid AND handle = $2",
            &[Param::Text(&tenant), Param::Text(handle)],
        )? > 0)
    }
}

impl Secrets for SecretStore {
    fn resolve(&self, handle: &str) -> Result<Vec<u8>, BpaError> {
        let tenant = self.keys.tenant.clone();
        let rows = self.db.query(
            "SELECT key_id, nonce, sealed FROM secret WHERE tenant_id = $1::uuid AND handle = $2",
            &[Param::Text(&tenant), Param::Text(handle)],
        )?;
        let row = rows
            .first()
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("no secret {handle}")))?;
        let k = self
            .keys
            .by_id("", row["key_id"].as_u64().unwrap_or(1) as u32)?;
        let nonce: [u8; crypto::NONCE_LEN] = unhex(row["nonce"].as_str().unwrap_or_default())
            .try_into()
            .map_err(|_| unavailable("secret nonce malformed"))?;
        crypto::open(
            &k.key,
            &nonce,
            handle.as_bytes(),
            &unhex(row["sealed"].as_str().unwrap_or_default()),
        )
    }
}
