//! Physical form of stored bytes and chunk placement (DSN-DVM-002, DSN-DVM-003, DEC-048).
//!
//! Content IDs are computed over logical bytes; the store keeps them compressed with zstd
//! and then encrypted with XChaCha20-Poly1305 under the tenant's current data key. The
//! stored form is
//!
//! ```text
//! 0x01 | key id (u32, big-endian) | nonce (24 bytes) | ciphertext with tag
//! ```
//!
//! with associated data `"ubos.physical.v1" 0x00 tenant 0x00 content-id`, so that bytes
//! moved to another tenant or another content ID fail authentication. Reading reverses the
//! steps; any failure is `NOD.BLOB_UNAVAILABLE` with `{"integrity": true}`. Chunks are
//! additionally checked against their content ID after decompression.

use crate::kernel::{ChunkRow, Hash, Placement, hash_text};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use ubos_crypto::{KeyService, NONCE_LEN, integrity, open, random_bytes, seal};
use ubos_types::{BpaError, digest};

/// Format byte of the stored form.
const FORMAT: u8 = 1;
/// Largest physical size of an inline chunk (DSN-DVM-003).
pub const INLINE_MAX: usize = 8 * 1024;
/// zstd level of stored bytes.
const LEVEL: i32 = 3;

fn aad(tenant: &str, id: &Hash) -> Vec<u8> {
    let mut a = b"ubos.physical.v1\0".to_vec();
    a.extend_from_slice(tenant.as_bytes());
    a.push(0);
    a.extend_from_slice(id);
    a
}

fn storage(e: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

/// Compression and encryption of stored bytes for one tenant.
#[derive(Clone)]
pub struct Physical {
    keys: Arc<dyn KeyService>,
    tenant: String,
}

impl Physical {
    /// The physical form of a tenant under a key service.
    pub fn new(keys: Arc<dyn KeyService>, tenant: &str) -> Physical {
        Physical {
            keys,
            tenant: tenant.to_string(),
        }
    }

    /// Stored form of logical bytes identified by `id`.
    pub fn seal(&self, id: &Hash, logical: &[u8]) -> Result<Vec<u8>, BpaError> {
        let packed = zstd::bulk::compress(logical, LEVEL).map_err(storage)?;
        let key = self.keys.current(&self.tenant)?;
        let nonce: [u8; NONCE_LEN] = random_bytes()?;
        let sealed = seal(&key.key, &nonce, &aad(&self.tenant, id), &packed)?;
        let mut out = Vec::with_capacity(1 + 4 + NONCE_LEN + sealed.len());
        out.push(FORMAT);
        out.extend_from_slice(&key.id.to_be_bytes());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// Logical bytes of a stored form identified by `id`.
    pub fn open(&self, id: &Hash, stored: &[u8]) -> Result<Vec<u8>, BpaError> {
        if stored.len() < 1 + 4 + NONCE_LEN || stored[0] != FORMAT {
            return Err(integrity("unknown physical form"));
        }
        let key_id = u32::from_be_bytes([stored[1], stored[2], stored[3], stored[4]]);
        let key = self.keys.by_id(&self.tenant, key_id)?;
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&stored[5..5 + NONCE_LEN]);
        let packed = open(
            &key.key,
            &nonce,
            &aad(&self.tenant, id),
            &stored[5 + NONCE_LEN..],
        )?;
        zstd::stream::decode_all(packed.as_slice()).map_err(|_| integrity("decompression failed"))
    }
}

/// Storage of large chunks outside the database (the host blob service, DSN-DVM-003).
pub trait BlobStore: Send + Sync {
    /// Store bytes under a path (idempotent).
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), BpaError>;
    /// Read the bytes under a path.
    fn get(&self, path: &str) -> Result<Vec<u8>, BpaError>;
}

/// A blob store in a local directory, until the host blob service of step 7 (DEC-047).
pub struct LocalBlobs {
    root: PathBuf,
}

impl LocalBlobs {
    /// Blobs under a directory, created when missing.
    pub fn new(root: impl Into<PathBuf>) -> Result<LocalBlobs, BpaError> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(storage)?;
        Ok(LocalBlobs { root })
    }
}

impl BlobStore for LocalBlobs {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), BpaError> {
        let p = self.root.join(path);
        if p.exists() {
            return Ok(());
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(storage)?;
        }
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, bytes).map_err(storage)?;
        std::fs::rename(&tmp, &p).map_err(storage)
    }
    fn get(&self, path: &str) -> Result<Vec<u8>, BpaError> {
        std::fs::read(self.root.join(path)).map_err(|_| {
            let mut e = BpaError::new("NOD.BLOB_UNAVAILABLE", format!("blob {path} missing"));
            e.details = Some(serde_json::json!({"integrity": false}));
            e
        })
    }
}

/// Object-store path of a chunk: `t/<tenant-hash>/c/<content-id>` (DSN-DVM-003).
pub fn chunk_path(tenant: &str, id: &Hash) -> String {
    let th: [u8; 32] = Sha256::digest(tenant.as_bytes()).into();
    let t = hash_text(&th);
    let c = hash_text(id);
    format!("t/{}/c/{}", &t[7..], &c[7..])
}

/// Content ID of a chunk's logical bytes (STD-OBJ-008, domain `ubos.chunk.v1`).
pub fn chunk_id(logical: &[u8]) -> Result<Hash, BpaError> {
    crate::kernel::hash_of(&digest("ubos.chunk.v1", logical)?)
}

/// Chunks with their placement: small stored forms inline, large ones in the blob store,
/// written before the flush transaction so that it holds descriptors only.
pub struct Chunks {
    physical: Physical,
    blobs: Arc<dyn BlobStore>,
}

impl Chunks {
    /// Chunks of a tenant.
    pub fn new(physical: Physical, blobs: Arc<dyn BlobStore>) -> Chunks {
        Chunks { physical, blobs }
    }

    /// Prepare a chunk: compute its ID, seal it, and place it (large chunks are written to
    /// the blob store now). The returned row goes into the flush (`put_chunks`).
    pub fn prepare(&self, logical: &[u8]) -> Result<ChunkRow, BpaError> {
        let id = chunk_id(logical)?;
        let stored = self.physical.seal(&id, logical)?;
        let size_logical = u32::try_from(logical.len()).map_err(storage)?;
        if stored.len() <= INLINE_MAX {
            return Ok(ChunkRow {
                content_id: id,
                size_logical,
                placement: Placement::Inline,
                body: Some(stored),
            });
        }
        self.blobs
            .put(&chunk_path(&self.physical.tenant, &id), &stored)?;
        Ok(ChunkRow {
            content_id: id,
            size_logical,
            placement: Placement::ObjectStore,
            body: None,
        })
    }

    /// Logical bytes of a chunk row, verified against its content ID (DSN-DVM-002).
    pub fn read(&self, row: &ChunkRow) -> Result<Vec<u8>, BpaError> {
        let stored = match (&row.placement, &row.body) {
            (Placement::Inline, Some(b)) => b.clone(),
            (Placement::Inline, None) => return Err(integrity("inline chunk without body")),
            (Placement::ObjectStore, _) => self
                .blobs
                .get(&chunk_path(&self.physical.tenant, &row.content_id))?,
        };
        let logical = self.physical.open(&row.content_id, &stored)?;
        if chunk_id(&logical)? != row.content_id {
            return Err(integrity("content ID mismatch"));
        }
        Ok(logical)
    }
}
