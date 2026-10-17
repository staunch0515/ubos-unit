//! Files: resumable uploads, content-addressed chunks and download links (PH-1 step 17a,
//! DEC-078; DSN-NOD-320, -321, -327; FR-FILE-011, -012; STD-OBJ-008).
//!
//! **Uploads.** A tus-compatible upload declares its length, then receives the bytes in
//! order (`append` at the offset received so far; a wrong offset is refused, so a client
//! resumes from `received`). The bytes are cut into content-defined chunks as they arrive
//! (FastCDC 2020, 16/64/256 KiB, STD-OBJ-008); each chunk is sealed with the tenant's data
//! key (the physical form of DSN-DVM-002) and written to the tenant's blob store before the
//! upload's state advances; the bytes after the last cut stay in the upload (`carry`, at
//! most one chunk). A chunk's path carries the tenant: identical chunks are stored once
//! within a tenant and never shared between tenants (DSN-NOD-321).
//!
//! **Files.** The last byte completes the upload: the manifest `{chunks, total_size,
//! media_type}` of STD-OBJ-008 is recorded, and its content identifier is the file
//! identifier a `blob` field holds (`{"content_id": "sha256:…", "size", "media_type",
//! "name"}`, STD-FND-020), so the file is durable before the commit that references it
//! (FR-FILE-011).
//!
//! **Links.** A download link is a signed token `payload.mac` naming the tenant, the file,
//! the principal and the object version it was issued for, valid for
//! `files.download_link_seconds` (DSN-NOD-327); the MAC key is derived from the tenant's
//! key hierarchy.

use crate::db::{Db, Param};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ubos_dvm::crypto::{self, KeyService, LocalKeys};
use ubos_dvm::store::physical::{BlobStore, Physical, chunk_id, chunk_path};
use ubos_dvm::types::{BpaError, content_id};

/// Minimum, average and maximum chunk sizes (STD-OBJ-008, `DAT-ChunkingParams`).
pub const MIN_CHUNK: usize = 16 * 1024;
/// The target size.
pub const AVG_CHUNK: usize = 64 * 1024;
/// The maximum size.
pub const MAX_CHUNK: usize = 256 * 1024;
/// How long an unfinished upload is kept.
pub const UPLOAD_TTL_SECS: i64 = 24 * 3600;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex32(t: &str) -> Option<[u8; 32]> {
    let t = t.strip_prefix("sha256:").unwrap_or(t);
    if t.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&t[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

fn not_found(what: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", format!("unknown {what}"))
}

/// An upload in progress or completed.
#[derive(Debug, Clone)]
pub struct Upload {
    /// Identifier (UUID).
    pub id: String,
    /// The principal who created it; only they continue it.
    pub principal: String,
    /// Declared length in bytes.
    pub length: u64,
    /// Bytes received.
    pub received: u64,
    /// Media type from the metadata.
    pub media_type: Option<String>,
    /// File name from the metadata.
    pub name: Option<String>,
    /// Bytes after the last chunk boundary.
    pub carry: Vec<u8>,
    /// Chunks cut so far: `(content id, size)`.
    pub chunks: Vec<(String, u64)>,
    /// The file, once complete.
    pub file_id: Option<String>,
    /// Expiry (Unix seconds).
    pub expires_at: i64,
}

impl Upload {
    /// The state a client sees: `{upload_id, length, offset, file, name, media_type}`.
    pub fn json(&self) -> Value {
        json!({"upload_id": self.id, "length": self.length, "offset": self.received,
            "file": self.file_id, "name": self.name, "media_type": self.media_type})
    }
}

/// A completed file.
#[derive(Debug, Clone)]
pub struct FileRec {
    /// The content identifier of the manifest.
    pub id: String,
    /// The manifest `{chunks, total_size, media_type}` (STD-OBJ-008).
    pub manifest: Value,
    /// The size of each chunk.
    pub sizes: Vec<u64>,
    /// Total size.
    pub size: u64,
    /// SHA-256 of the content (hexadecimal), for clients to verify.
    pub sha256: String,
    /// Media type.
    pub media_type: Option<String>,
}

impl FileRec {
    /// The chunk identifiers in order.
    pub fn chunks(&self) -> Vec<String> {
        self.manifest["chunks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The value of a `blob` field referring to the file (STD-FND-020).
    pub fn reference(&self, name: Option<&str>) -> Value {
        json!({"content_id": self.id, "size": self.size, "media_type": self.media_type, "name": name})
    }
}

/// Where uploads and file records live.
pub trait FileRows: Send + Sync {
    /// Record a new upload.
    fn insert(&self, u: &Upload) -> Result<(), BpaError>;
    /// An upload.
    fn upload(&self, id: &str) -> Result<Option<Upload>, BpaError>;
    /// Save an upload whose `received` is still `from`; `false` when another append won.
    fn advance(&self, u: &Upload, from: u64) -> Result<bool, BpaError>;
    /// Remove an upload.
    fn remove(&self, id: &str) -> Result<(), BpaError>;
    /// Record a file (idempotent: the same content is the same file).
    fn put_file(&self, f: &FileRec) -> Result<(), BpaError>;
    /// A file.
    fn file(&self, id: &str) -> Result<Option<FileRec>, BpaError>;
    /// Remove the unfinished uploads expired at `now`; how many.
    fn expire(&self, now: i64) -> Result<u64, BpaError>;
}

/// The rows in PostgreSQL (the tenant's pool, row-level security).
pub struct PgRows {
    db: Arc<Db>,
    tenant: String,
}

impl PgRows {
    /// The rows of a tenant.
    pub fn new(db: Arc<Db>, tenant: &str) -> PgRows {
        PgRows {
            db,
            tenant: tenant.to_string(),
        }
    }
}

fn upload_of(r: &Value) -> Upload {
    let carry = r["carry"]
        .as_str()
        .map(|h| {
            (0..h.len() / 2)
                .filter_map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).ok())
                .collect()
        })
        .unwrap_or_default();
    Upload {
        id: r["upload_id"].as_str().unwrap_or_default().to_string(),
        principal: r["principal"].as_str().unwrap_or_default().to_string(),
        length: r["length"].as_u64().unwrap_or(0),
        received: r["received"].as_u64().unwrap_or(0),
        media_type: r["media_type"].as_str().map(str::to_string),
        name: r["name"].as_str().map(str::to_string),
        carry,
        chunks: r["chunks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|c| {
                        (
                            c[0].as_str().unwrap_or_default().to_string(),
                            c[1].as_u64().unwrap_or(0),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        file_id: r["file_id"].as_str().map(str::to_string),
        expires_at: r["expires_at"].as_i64().unwrap_or(0),
    }
}

fn chunks_json(u: &Upload) -> Value {
    Value::Array(u.chunks.iter().map(|(c, n)| json!([c, n])).collect())
}

impl FileRows for PgRows {
    fn insert(&self, u: &Upload) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO upload (tenant_id, upload_id, principal, length, media_type, name, expires_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, to_timestamp($7))",
            &[
                Param::Text(&self.tenant),
                Param::Text(&u.id),
                Param::Text(&u.principal),
                Param::Int(u.length as i64),
                Param::OptText(u.media_type.as_deref()),
                Param::OptText(u.name.as_deref()),
                Param::Int(u.expires_at),
            ],
        )?;
        Ok(())
    }
    fn upload(&self, id: &str) -> Result<Option<Upload>, BpaError> {
        if uuid_ok(id).is_none() {
            return Ok(None);
        }
        let rows = self.db.query(
            "SELECT upload_id::text AS upload_id, principal, length, received, media_type, name, \
             encode(carry, 'hex') AS carry, chunks, file_id, extract(epoch FROM expires_at)::bigint AS expires_at \
             FROM upload WHERE tenant_id = $1::uuid AND upload_id = $2::uuid",
            &[Param::Text(&self.tenant), Param::Text(id)],
        )?;
        Ok(rows.first().map(upload_of))
    }
    fn advance(&self, u: &Upload, from: u64) -> Result<bool, BpaError> {
        let n = self.db.execute(
            "UPDATE upload SET received = $3, carry = $4, chunks = $5, file_id = $6 \
             WHERE tenant_id = $1::uuid AND upload_id = $2::uuid AND received = $7",
            &[
                Param::Text(&self.tenant),
                Param::Text(&u.id),
                Param::Int(u.received as i64),
                Param::Bytes(&u.carry),
                Param::Json(&chunks_json(u)),
                Param::OptText(u.file_id.as_deref()),
                Param::Int(from as i64),
            ],
        )?;
        Ok(n == 1)
    }
    fn remove(&self, id: &str) -> Result<(), BpaError> {
        if uuid_ok(id).is_some() {
            self.db.execute(
                "DELETE FROM upload WHERE tenant_id = $1::uuid AND upload_id = $2::uuid",
                &[Param::Text(&self.tenant), Param::Text(id)],
            )?;
        }
        Ok(())
    }
    fn put_file(&self, f: &FileRec) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO file (tenant_id, file_id, manifest, sizes, size, sha256, media_type) \
             VALUES ($1::uuid, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING",
            &[
                Param::Text(&self.tenant),
                Param::Text(&f.id),
                Param::Json(&f.manifest),
                Param::Json(&json!(f.sizes)),
                Param::Int(f.size as i64),
                Param::Text(&f.sha256),
                Param::OptText(f.media_type.as_deref()),
            ],
        )?;
        Ok(())
    }
    fn file(&self, id: &str) -> Result<Option<FileRec>, BpaError> {
        let rows = self.db.query(
            "SELECT file_id, manifest, sizes, size, sha256, media_type FROM file WHERE tenant_id = $1::uuid AND file_id = $2",
            &[Param::Text(&self.tenant), Param::Text(id)],
        )?;
        Ok(rows.first().map(|r| FileRec {
            id: r["file_id"].as_str().unwrap_or_default().to_string(),
            manifest: r["manifest"].clone(),
            sizes: r["sizes"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default(),
            size: r["size"].as_u64().unwrap_or(0),
            sha256: r["sha256"].as_str().unwrap_or_default().to_string(),
            media_type: r["media_type"].as_str().map(str::to_string),
        }))
    }
    fn expire(&self, now: i64) -> Result<u64, BpaError> {
        self.db.execute(
            "DELETE FROM upload WHERE tenant_id = $1::uuid AND file_id IS NULL AND expires_at < to_timestamp($2)",
            &[Param::Text(&self.tenant), Param::Int(now)],
        )
    }
}

fn uuid_ok(id: &str) -> Option<()> {
    let ok = id.len() == 36
        && id
            .chars()
            .enumerate()
            .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) && c == '-' || c.is_ascii_hexdigit());
    ok.then_some(())
}

/// The rows in memory (the memory preset and the suites).
#[derive(Default)]
pub struct MemRows {
    uploads: Mutex<HashMap<String, Upload>>,
    files: Mutex<HashMap<String, FileRec>>,
}

impl FileRows for MemRows {
    fn insert(&self, u: &Upload) -> Result<(), BpaError> {
        self.uploads
            .lock()
            .expect("uploads")
            .insert(u.id.clone(), u.clone());
        Ok(())
    }
    fn upload(&self, id: &str) -> Result<Option<Upload>, BpaError> {
        Ok(self.uploads.lock().expect("uploads").get(id).cloned())
    }
    fn advance(&self, u: &Upload, from: u64) -> Result<bool, BpaError> {
        let mut m = self.uploads.lock().expect("uploads");
        match m.get(&u.id) {
            Some(cur) if cur.received == from => {
                m.insert(u.id.clone(), u.clone());
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn remove(&self, id: &str) -> Result<(), BpaError> {
        self.uploads.lock().expect("uploads").remove(id);
        Ok(())
    }
    fn put_file(&self, f: &FileRec) -> Result<(), BpaError> {
        self.files
            .lock()
            .expect("files")
            .entry(f.id.clone())
            .or_insert_with(|| f.clone());
        Ok(())
    }
    fn file(&self, id: &str) -> Result<Option<FileRec>, BpaError> {
        Ok(self.files.lock().expect("files").get(id).cloned())
    }
    fn expire(&self, now: i64) -> Result<u64, BpaError> {
        let mut m = self.uploads.lock().expect("uploads");
        let before = m.len();
        m.retain(|_, u| u.file_id.is_some() || u.expires_at >= now);
        Ok((before - m.len()) as u64)
    }
}

/// Blobs in memory.
#[derive(Default)]
pub struct MemBlobs(Mutex<HashMap<String, Vec<u8>>>);

impl BlobStore for MemBlobs {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), BpaError> {
        self.0
            .lock()
            .expect("blobs")
            .entry(path.to_string())
            .or_insert_with(|| bytes.to_vec());
        Ok(())
    }
    fn get(&self, path: &str) -> Result<Vec<u8>, BpaError> {
        self.0
            .lock()
            .expect("blobs")
            .get(path)
            .cloned()
            .ok_or_else(|| BpaError::new("NOD.BLOB_UNAVAILABLE", format!("blob {path} missing")))
    }
}

/// The file service of one tenant.
pub struct Files {
    rows: Arc<dyn FileRows>,
    physical: Physical,
    blobs: Arc<dyn BlobStore>,
    tenant: String,
    link_key: [u8; 32],
}

impl Files {
    /// The service over rows, a key service, a blob store and the link key.
    pub fn new(
        rows: Arc<dyn FileRows>,
        keys: Arc<dyn KeyService>,
        blobs: Arc<dyn BlobStore>,
        tenant: &str,
        link_key: [u8; 32],
    ) -> Files {
        Files {
            rows,
            physical: Physical::new(keys, tenant),
            blobs,
            tenant: tenant.to_string(),
            link_key,
        }
    }

    /// A service in memory with fresh keys (the memory preset).
    pub fn memory(tenant: &str) -> Result<Files, BpaError> {
        let master: [u8; 32] = crypto::random_bytes()?;
        Ok(Files::new(
            Arc::new(MemRows::default()),
            Arc::new(LocalKeys::new(master, 1)),
            Arc::new(MemBlobs::default()),
            tenant,
            crypto::random_bytes()?,
        ))
    }

    /// Start an upload of `length` bytes (at most `max` bytes, FR-FILE-012).
    pub fn create(
        &self,
        principal: &str,
        length: u64,
        media_type: Option<&str>,
        name: Option<&str>,
        max: u64,
    ) -> Result<Upload, BpaError> {
        let _ = self.rows.expire(now());
        if length > max {
            return Err(BpaError::with_details(
                "MODEL.OBJECT_TOO_LARGE",
                json!({"reason": "file_too_large", "length": length, "max": max}),
            ));
        }
        let u = Upload {
            id: crate::uuid()?,
            principal: principal.to_string(),
            length,
            received: 0,
            media_type: media_type.map(str::to_string),
            name: name.map(str::to_string),
            carry: Vec::new(),
            chunks: Vec::new(),
            file_id: None,
            expires_at: now() + UPLOAD_TTL_SECS,
        };
        self.rows.insert(&u)?;
        if length == 0 {
            // an empty file completes at once
            return self.append(&u.id, principal, 0, &[]);
        }
        Ok(u)
    }

    /// An upload of a principal.
    pub fn upload(&self, id: &str, principal: &str) -> Result<Upload, BpaError> {
        match self.rows.upload(id)? {
            Some(u) if u.principal == principal => Ok(u),
            _ => Err(not_found("upload")),
        }
    }

    /// Abandon an upload (the chunks already stored stay: they may be shared).
    pub fn terminate(&self, id: &str, principal: &str) -> Result<(), BpaError> {
        self.upload(id, principal)?;
        self.rows.remove(id)
    }

    /// Append bytes at `offset`, which must be the bytes received so far
    /// (`TXN.HEAD_MOVED` with the expected offset otherwise); the last byte completes the
    /// file.
    pub fn append(
        &self,
        id: &str,
        principal: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<Upload, BpaError> {
        let mut u = self.upload(id, principal)?;
        if u.file_id.is_some() && !data.is_empty() {
            return Err(BpaError::with_details(
                "TXN.HEAD_MOVED",
                json!({"reason": "upload_complete", "offset": u.received}),
            ));
        }
        if offset != u.received {
            return Err(BpaError::with_details(
                "TXN.HEAD_MOVED",
                json!({"reason": "offset_mismatch", "offset": u.received}),
            ));
        }
        if u.received + data.len() as u64 > u.length {
            return Err(BpaError::with_details(
                "MODEL.OBJECT_TOO_LARGE",
                json!({"reason": "beyond_length", "length": u.length}),
            ));
        }
        let from = u.received;
        let mut buf = std::mem::take(&mut u.carry);
        buf.extend_from_slice(data);
        let last = u.received + data.len() as u64 == u.length;
        let cuts: Vec<(usize, usize)> =
            fastcdc::v2020::FastCDC::new(&buf, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK)
                .map(|c| (c.offset, c.length))
                .collect();
        let mut done = 0;
        for (i, (off, len)) in cuts.iter().enumerate() {
            // the last piece is a chunk only when the content ends there or the cut is
            // forced at the maximum size; otherwise later bytes may move its boundary
            let tail = i + 1 == cuts.len();
            if tail && !last && *len < MAX_CHUNK {
                break;
            }
            let id = self.store(&buf[*off..off + len])?;
            u.chunks.push((id, *len as u64));
            done = off + len;
        }
        u.carry = buf[done..].to_vec();
        u.received += data.len() as u64;
        if last && u.file_id.is_none() {
            let f = self.complete(&u)?;
            u.file_id = Some(f.id);
        }
        if !self.rows.advance(&u, from)? {
            return Err(BpaError::with_details(
                "TXN.HEAD_MOVED",
                json!({"reason": "concurrent_append", "offset": self.upload(id, principal)?.received}),
            ));
        }
        Ok(u)
    }

    /// Record the file of a complete upload.
    fn complete(&self, u: &Upload) -> Result<FileRec, BpaError> {
        let ids: Vec<String> = u.chunks.iter().map(|(c, _)| c.clone()).collect();
        let manifest = json!({"chunks": ids, "total_size": u.length, "media_type": u.media_type});
        let id = content_id("ubos.chunk.v1", &json!({"file": manifest}))?;
        let mut h = Sha256::new();
        for c in &ids {
            h.update(self.chunk(c)?);
        }
        let f = FileRec {
            id,
            manifest,
            sizes: u.chunks.iter().map(|(_, n)| *n).collect(),
            size: u.length,
            sha256: hex(&h.finalize()),
            media_type: u.media_type.clone(),
        };
        self.rows.put_file(&f)?;
        Ok(f)
    }

    /// Seal and store one chunk; its content identifier.
    fn store(&self, logical: &[u8]) -> Result<String, BpaError> {
        let id = chunk_id(logical)?;
        let stored = self.physical.seal(&id, logical)?;
        self.blobs.put(&chunk_path(&self.tenant, &id), &stored)?;
        Ok(format!("sha256:{}", hex(&id)))
    }

    /// The logical bytes of a chunk, verified against its identifier.
    pub fn chunk(&self, id: &str) -> Result<Vec<u8>, BpaError> {
        let h = unhex32(id).ok_or_else(|| not_found("chunk"))?;
        let stored = self.blobs.get(&chunk_path(&self.tenant, &h))?;
        let logical = self.physical.open(&h, &stored)?;
        if chunk_id(&logical)? != h {
            return Err(crypto::integrity("content ID mismatch"));
        }
        Ok(logical)
    }

    /// A file.
    pub fn file(&self, id: &str) -> Result<FileRec, BpaError> {
        self.rows.file(id)?.ok_or_else(|| not_found("file"))
    }

    /// The bytes `[start, end)` of a file, read chunk by chunk through `sink`.
    pub fn read_range(
        &self,
        f: &FileRec,
        start: u64,
        end: u64,
        mut sink: impl FnMut(Vec<u8>) -> bool,
    ) -> Result<(), BpaError> {
        let mut pos = 0u64;
        for (c, n) in f.chunks().iter().zip(&f.sizes) {
            let (a, b) = (pos, pos + n);
            pos = b;
            if b <= start || a >= end {
                continue;
            }
            let bytes = self.chunk(c)?;
            let lo = start.saturating_sub(a) as usize;
            let hi = (end.min(b) - a) as usize;
            if !sink(bytes[lo..hi].to_vec()) {
                break;
            }
        }
        Ok(())
    }

    /// A signed download link token for claims `{t, f, p, o, v, c, e}`.
    pub fn sign_link(&self, claims: &Value) -> String {
        let payload = b64url(&serde_json::to_vec(claims).unwrap_or_default());
        let mac = crypto::hmac_sha256(&self.link_key, payload.as_bytes());
        format!("{payload}.{}", b64url(&mac))
    }

    /// The claims of a link token, when its MAC verifies and it has not expired.
    pub fn verify_link(&self, token: &str) -> Result<Value, BpaError> {
        let denied = |m: &str| BpaError::with_details("IAM.DENIED", json!({"reason": m}));
        let (payload, mac) = token
            .split_once('.')
            .ok_or_else(|| denied("malformed link"))?;
        let want = crypto::hmac_sha256(&self.link_key, payload.as_bytes());
        let got = b64url_decode(mac).ok_or_else(|| denied("malformed link"))?;
        if got.len() != 32 || !constant_eq(&got, &want) {
            return Err(denied("the link signature does not verify"));
        }
        let claims: Value = b64url_decode(payload)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or_else(|| denied("malformed link"))?;
        if claims["e"].as_i64().unwrap_or(0) < now() {
            return Err(denied("the link has expired"));
        }
        Ok(claims)
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// The tenant named by a link token (before its MAC is checked with the tenant's key).
pub fn link_tenant(token: &str) -> Option<String> {
    let payload = token.split_once('.')?.0;
    let claims: Value = serde_json::from_slice(&b64url_decode(payload)?).ok()?;
    claims["t"].as_str().map(str::to_string)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding.
pub fn b64url(b: &[u8]) -> String {
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// Decode base64url without padding.
pub fn b64url_decode(t: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for ch in t.bytes() {
        let v = B64.iter().position(|&x| x == ch)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Some(out)
}

/// The current Unix time in seconds.
pub fn unix_now() -> i64 {
    now()
}
