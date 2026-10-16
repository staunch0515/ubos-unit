//! Local identity of the node (PH-1 step 14c, DEC-075; DSN-NOD-101, -105, -106, -108,
//! -109, -110, -111, -150, -153; FR-IAM-011, -012, -013, -032…-035, -101, -102, FR-INT-082).
//!
//! One [`Identity`] per tenant over a [`Backend`] (the tenant tables `identity_record` and
//! `node_security_log` on PostgreSQL, or memory for the `memory` preset and the suites):
//!
//! * **principals** with a registered-form DID `did:ubos:<authority>:<uuid>` (STD-ADDR-010),
//!   a kind (`human`, `service`, `agent`), a role, grants and the lifecycle states `invited`,
//!   `active`, `suspended`, `deprovisioned` (DSN-NOD-109); platform services act under
//!   tenant-scoped service principals (DSN-NOD-110);
//! * **credentials**: passwords as Argon2id hashes (64 MiB, 3 iterations), a TOTP second
//!   factor (RFC 6238, HMAC-SHA1, 30 s, ±1 step), progressive delay from the sixth
//!   consecutive failure and a lock at the twentieth, per account and per source
//!   (DSN-NOD-101; the WebAuthn factor is deferred to PH-2);
//! * **sessions**: server-side records addressed by the hash of an opaque token
//!   `<tenant>.<secret>`, with idle and absolute timeouts and a concurrent-session limit per
//!   principal kind, the device and source, and the authentication level (`password`,
//!   `mfa`, `api_key`; DSN-NOD-105); revocation is immediate on the node and re-checked on
//!   every validation (DSN-NOD-106);
//! * **API keys** `<tenant>.ak.<secret>`: 256-bit secrets shown once and stored hashed,
//!   bound to a principal, scopes, optional IP ranges and an expiry (DSN-NOD-150); scopes
//!   (`read`, `write`, `admin`, `read:<Class>`, `write:<Class>`, `vae:<authority>`) become
//!   the grants of the binding's in-process credentials, intersected with the principal's
//!   own grants so that a credential never exceeds its principal (DSN-NOD-153);
//! * **principal keys**: an Ed25519 key per principal at first need, custodial (the seed is
//!   derived from the tenant's key hierarchy and never stored) or client-held, with the key
//!   history of the DID document (DSN-NOD-108; FR-IAM-032…-034);
//! * the node's hash-chained **security log** of authentication, session, administrative
//!   and key events, which the Server also feeds with the kernel's decision and security
//!   logs (DSN-NOD-111).
//!
//! Secrets never reach the log: records carry identifiers, names, reasons and counts only.

use crate::db::{Db, Param};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha1::Sha1;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use ubos_dvm::crypto;
use ubos_dvm::types::addr::{b58encode, b64u, unb64u};
use ubos_dvm::types::{BpaError, content_id};

/// Argon2id memory (KiB) and iterations (DSN-NOD-101: ≥ 64 MB, ≥ 3).
pub const ARGON2_MEMORY_KIB: u32 = 65_536;
/// Argon2id iterations.
pub const ARGON2_ITERATIONS: u32 = 3;
/// Consecutive failures after which the next attempt is delayed.
pub const DELAY_AFTER: u64 = 5;
/// Consecutive failures at which the account or source is locked.
pub const LOCK_AFTER: u64 = 20;
/// Lock duration in seconds.
pub const LOCK_SECS: i64 = 900;
/// Longest progressive delay in seconds.
pub const MAX_DELAY_SECS: i64 = 300;
/// TOTP step in seconds.
pub const TOTP_STEP: i64 = 30;
/// Bytes of entropy in a session token or API key (256 bits, DSN-NOD-150).
pub const TOKEN_BYTES: usize = 32;
/// Principal kinds.
pub const KINDS: &[&str] = &["human", "service", "agent"];
/// Principal states (DSN-NOD-109).
pub const STATES: &[&str] = &["invited", "active", "suspended", "deprovisioned"];
/// Grants of a `read` scope (the actions of STD-ISA-005 that read).
const READ_ACTIONS: &[&str] = &[
    "read",
    "read_history",
    "explain",
    "reflect",
    "evaluate",
    "ask",
];
/// Grants of a `write` scope beyond `read`; a write scope also grants `commit.raw` (raw
/// commits, whose changes the per-class grants still bound).
const WRITE_ACTIONS: &[&str] = &[
    "create", "update", "append", "correct", "execute", "invoke", "emit", "end", "notify",
];
/// Grants of an `admin` scope beyond `write`.
const ADMIN_ACTIONS: &[&str] = &["permission"];

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn denied(reason: &str, details: Value) -> BpaError {
    let mut d = details;
    d["reason"] = json!(reason);
    let mut e = BpaError::with_details("IAM.DENIED", d);
    e.message = reason.replace('_', " ");
    e
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn i(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or_default()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or_default())
        .collect()
}

/// The canonical text of a unix time (STD-TIME-001).
pub fn iso(secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|t| ubos_dvm::types::time::format_instant(&t))
        .unwrap_or_default()
}

/// Base64url without padding, any length (`MODEL.INVALID_VALUE` when malformed).
pub fn unb64u_any(v: &Value, what: &str) -> Result<Vec<u8>, BpaError> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bad = || invalid(&format!("{what} is base64url without padding"));
    let text = v.as_str().ok_or_else(bad)?;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let d = A.iter().position(|x| *x == c).ok_or_else(bad)? as u32;
        acc = (acc << 6) | d;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

/// Base32 (RFC 4648, no padding) for TOTP enrolment URIs.
pub fn base32(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for x in b {
        acc = (acc << 8) | u32::from(*x);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(A[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(A[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// The TOTP code of a secret at a unix time (RFC 6238 over RFC 4226, 6 digits).
pub fn totp(secret: &[u8], at: i64) -> String {
    let counter = (at.div_euclid(TOTP_STEP)) as u64;
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let o = (h[19] & 0x0f) as usize;
    let code = (u32::from(h[o] & 0x7f) << 24)
        | (u32::from(h[o + 1]) << 16)
        | (u32::from(h[o + 2]) << 8)
        | u32::from(h[o + 3]);
    format!("{:06}", code % 1_000_000)
}

fn argon() -> Argon2<'static> {
    let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, 1, None).expect("argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// An Argon2id hash in PHC string form.
pub fn hash_password(password: &str) -> Result<String, BpaError> {
    let salt: [u8; 16] = crypto::random_bytes()?;
    let salt = SaltString::encode_b64(&salt).map_err(err)?;
    argon()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(err)
}

/// Whether a password matches a PHC hash.
pub fn verify_password(phc: &str, password: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| argon().verify_password(password.as_bytes(), &h).is_ok())
}

/// A hash to verify against when the account does not exist, so that the time an attempt
/// takes does not tell accounts apart.
fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hash_password("ubos-no-such-account").unwrap_or_default())
}

/// The hash of a token as stored (a keyed SHA-256 under a public constant; DSN-NOD-105).
pub fn token_hash(token: &str) -> String {
    hex(&crypto::hmac_sha256(
        b"ubos.identity.token.v1",
        token.as_bytes(),
    ))
}

/// Whether `ip` lies in `range` (an address or a CIDR prefix, v4 or v6).
pub fn ip_in(range: &str, ip: &str) -> bool {
    let (net, bits) = match range.split_once('/') {
        Some((n, b)) => (n, b.parse::<u32>().ok()),
        None => (range, None),
    };
    let (Ok(net), Ok(ip)) = (
        net.parse::<std::net::IpAddr>(),
        ip.parse::<std::net::IpAddr>(),
    ) else {
        return false;
    };
    let as_bits = |a: std::net::IpAddr| match a {
        std::net::IpAddr::V4(v) => (u128::from(u32::from(v)), 32u32),
        std::net::IpAddr::V6(v) => (u128::from(v), 128u32),
    };
    let (n, nw) = as_bits(net);
    let (p, pw) = as_bits(ip);
    if nw != pw {
        return false;
    }
    let prefix = bits.unwrap_or(nw).min(nw);
    if prefix == 0 {
        return true;
    }
    let mask: u128 = if nw == 128 {
        u128::MAX << (128 - prefix)
    } else {
        (u32::MAX << (32 - prefix)) as u128
    };
    n & mask == p & mask
}

// ---- scopes (DSN-NOD-153, FR-INT-082) ----

/// Check a scope: `read`, `write`, `admin`, `read:<Class>`, `write:<Class>`,
/// `vae:<authority>`.
pub fn check_scope(scope: &str) -> Result<(), BpaError> {
    let ok = match scope.split_once(':') {
        None => matches!(scope, "read" | "write" | "admin"),
        Some((k, v)) => {
            !v.is_empty()
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                && matches!(k, "read" | "write" | "vae")
        }
    };
    if ok {
        Ok(())
    } else {
        Err(invalid(&format!("unknown scope {scope:?}")))
    }
}

/// Whether scopes allow a VAE: no `vae:` scope leaves every VAE allowed.
pub fn scope_allows_vae(scopes: Option<&[String]>, authority: &str) -> bool {
    let Some(sc) = scopes else { return true };
    let vaes: Vec<&str> = sc.iter().filter_map(|x| x.strip_prefix("vae:")).collect();
    vaes.is_empty() || vaes.contains(&authority)
}

/// Whether scopes allow any mutation.
pub fn scope_allows_write(scopes: Option<&[String]>) -> bool {
    scopes.is_none_or(|sc| {
        sc.iter()
            .any(|x| x == "write" || x == "admin" || x.starts_with("write:"))
    })
}

/// The grants of scopes alone (`None`: unrestricted).
fn scope_grants(scopes: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |actions: &[&str], class: Option<&str>| {
        for a in actions {
            let g = match class {
                Some(c) => format!("{a}:{c}"),
                None => (*a).to_string(),
            };
            if !out.contains(&g) {
                out.push(g);
            }
        }
    };
    for sc in scopes {
        match sc.split_once(':') {
            None if sc == "read" => push(READ_ACTIONS, None),
            None if sc == "write" => {
                push(READ_ACTIONS, None);
                push(WRITE_ACTIONS, None);
                push(&["commit.raw"], None);
            }
            None if sc == "admin" => {
                push(READ_ACTIONS, None);
                push(WRITE_ACTIONS, None);
                push(&["commit.raw"], None);
                push(ADMIN_ACTIONS, None);
            }
            Some(("read", c)) => push(READ_ACTIONS, Some(c)),
            Some(("write", c)) => {
                push(READ_ACTIONS, Some(c));
                push(WRITE_ACTIONS, Some(c));
                push(&["commit.raw"], None);
            }
            _ => {}
        }
    }
    out
}

/// The grants of a credential in the DVM context: the scopes' grants intersected with the
/// principal's own (DSN-NOD-153). `None` is unrestricted on either side.
pub fn grants_of(scopes: Option<&[String]>, principal: Option<&[String]>) -> Option<Vec<String>> {
    let scoped = scopes.map(scope_grants);
    match (scoped, principal) {
        (None, None) => None,
        (None, Some(p)) => Some(p.to_vec()),
        (Some(sg), None) => Some(sg),
        (Some(sg), Some(p)) => {
            let mut out: Vec<String> = Vec::new();
            for g in &sg {
                let action = g.split(':').next().unwrap_or_default();
                for pg in p {
                    let covered = pg == g
                        || (pg == action && g.contains(':'))
                        || (g == action && pg.starts_with(&format!("{action}:")));
                    if covered {
                        let keep = if pg == action { g.clone() } else { pg.clone() };
                        if !out.contains(&keep) {
                            out.push(keep);
                        }
                    }
                }
            }
            Some(out)
        }
    }
}

// ---- backends ----

/// Where an identity keeps its records and its log.
pub trait Backend: Send + Sync {
    /// A record by kind and key.
    fn get(&self, kind: &str, key: &str) -> Result<Option<Value>, BpaError>;
    /// Write a record.
    fn put(&self, kind: &str, key: &str, body: &Value) -> Result<(), BpaError>;
    /// Remove a record.
    fn remove(&self, kind: &str, key: &str) -> Result<(), BpaError>;
    /// The records of a kind, by key.
    fn list(&self, kind: &str) -> Result<Vec<Value>, BpaError>;
    /// The last log entry.
    fn log_head(&self) -> Result<Option<Value>, BpaError>;
    /// Append a log entry at `seq`.
    fn log_append(&self, seq: u64, entry: &Value) -> Result<(), BpaError>;
    /// Log entries with `from <= seq <= to`, oldest first, at most `limit`.
    fn log_range(&self, from: u64, to: u64, limit: u64) -> Result<Vec<Value>, BpaError>;
}

/// The memory backend (the `memory` preset, suites).
#[derive(Default)]
pub struct MemoryBackend {
    records: Mutex<HashMap<String, BTreeMap<String, Value>>>,
    log: Mutex<BTreeMap<u64, Value>>,
}

impl MemoryBackend {
    /// Drop a log entry (suites: the gap detection of DSN-NOD-111).
    pub fn delete_log_entry(&self, seq: u64) {
        if let Ok(mut l) = self.log.lock() {
            l.remove(&seq);
        }
    }
}

impl Backend for MemoryBackend {
    fn get(&self, kind: &str, key: &str) -> Result<Option<Value>, BpaError> {
        let r = self.records.lock().map_err(|_| err("lock"))?;
        Ok(r.get(kind).and_then(|m| m.get(key)).cloned())
    }
    fn put(&self, kind: &str, key: &str, body: &Value) -> Result<(), BpaError> {
        let mut r = self.records.lock().map_err(|_| err("lock"))?;
        r.entry(kind.to_string())
            .or_default()
            .insert(key.to_string(), body.clone());
        Ok(())
    }
    fn remove(&self, kind: &str, key: &str) -> Result<(), BpaError> {
        let mut r = self.records.lock().map_err(|_| err("lock"))?;
        if let Some(m) = r.get_mut(kind) {
            m.remove(key);
        }
        Ok(())
    }
    fn list(&self, kind: &str) -> Result<Vec<Value>, BpaError> {
        let r = self.records.lock().map_err(|_| err("lock"))?;
        Ok(r.get(kind)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }
    fn log_head(&self) -> Result<Option<Value>, BpaError> {
        let l = self.log.lock().map_err(|_| err("lock"))?;
        Ok(l.values().next_back().cloned())
    }
    fn log_append(&self, seq: u64, entry: &Value) -> Result<(), BpaError> {
        let mut l = self.log.lock().map_err(|_| err("lock"))?;
        l.insert(seq, entry.clone());
        Ok(())
    }
    fn log_range(&self, from: u64, to: u64, limit: u64) -> Result<Vec<Value>, BpaError> {
        let l = self.log.lock().map_err(|_| err("lock"))?;
        Ok(l.range(from..=to)
            .take(limit as usize)
            .map(|(_, v)| v.clone())
            .collect())
    }
}

/// The PostgreSQL backend: the tenant tables `identity_record` and `node_security_log`
/// under row-level security (migration 0008).
pub struct PgBackend {
    db: Arc<Db>,
    tenant: String,
}

impl PgBackend {
    /// A backend over a tenant's scoped database.
    pub fn new(db: Arc<Db>, tenant_id: &str) -> PgBackend {
        PgBackend {
            db,
            tenant: tenant_id.to_string(),
        }
    }
}

impl Backend for PgBackend {
    fn get(&self, kind: &str, key: &str) -> Result<Option<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT body FROM identity_record WHERE tenant_id = $1::uuid AND kind = $2 AND key = $3",
            &[Param::Text(&self.tenant), Param::Text(kind), Param::Text(key)],
        )?;
        Ok(rows.into_iter().next().map(|r| r["body"].clone()))
    }
    fn put(&self, kind: &str, key: &str, body: &Value) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO identity_record (tenant_id, kind, key, body) VALUES ($1::uuid, $2, $3, $4) \
             ON CONFLICT (tenant_id, kind, key) DO UPDATE SET body = EXCLUDED.body, updated_at = now()",
            &[Param::Text(&self.tenant), Param::Text(kind), Param::Text(key), Param::Json(body)],
        )?;
        Ok(())
    }
    fn remove(&self, kind: &str, key: &str) -> Result<(), BpaError> {
        self.db.execute(
            "DELETE FROM identity_record WHERE tenant_id = $1::uuid AND kind = $2 AND key = $3",
            &[
                Param::Text(&self.tenant),
                Param::Text(kind),
                Param::Text(key),
            ],
        )?;
        Ok(())
    }
    fn list(&self, kind: &str) -> Result<Vec<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT body FROM identity_record WHERE tenant_id = $1::uuid AND kind = $2 ORDER BY key",
            &[Param::Text(&self.tenant), Param::Text(kind)],
        )?;
        Ok(rows.into_iter().map(|r| r["body"].clone()).collect())
    }
    fn log_head(&self) -> Result<Option<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT entry FROM node_security_log WHERE tenant_id = $1::uuid ORDER BY seq DESC LIMIT 1",
            &[Param::Text(&self.tenant)],
        )?;
        Ok(rows.into_iter().next().map(|r| r["entry"].clone()))
    }
    fn log_append(&self, seq: u64, entry: &Value) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO node_security_log (tenant_id, seq, entry) VALUES ($1::uuid, $2, $3)",
            &[
                Param::Text(&self.tenant),
                Param::Int(seq as i64),
                Param::Json(entry),
            ],
        )?;
        Ok(())
    }
    fn log_range(&self, from: u64, to: u64, limit: u64) -> Result<Vec<Value>, BpaError> {
        let rows = self.db.query(
            "SELECT entry FROM node_security_log WHERE tenant_id = $1::uuid AND seq >= $2 AND seq <= $3 ORDER BY seq LIMIT $4",
            &[
                Param::Text(&self.tenant),
                Param::Int(from as i64),
                Param::Int(to as i64),
                Param::Int(limit as i64),
            ],
        )?;
        Ok(rows.into_iter().map(|r| r["entry"].clone()).collect())
    }
}

// ---- the identity of a tenant ----

/// A session opened by a sign-in.
#[derive(Debug, Clone)]
pub struct SignedIn {
    /// The opaque token, shown once.
    pub token: String,
    /// Session identifier (revocation, listing).
    pub session_id: String,
    /// Principal DID.
    pub principal: String,
    /// Authentication level: `password`, `mfa` or `api_key`.
    pub level: String,
    /// Absolute expiry (unix seconds).
    pub expires_at: i64,
}

/// What a validated credential authenticates.
#[derive(Debug, Clone)]
pub struct Authenticated {
    /// The principal record.
    pub principal: Value,
    /// Principal DID.
    pub did: String,
    /// Principal kind.
    pub kind: String,
    /// The session or API key identifier.
    pub credential_id: String,
    /// `session` or `api_key`.
    pub credential: &'static str,
    /// Authentication level.
    pub level: String,
    /// The credential's scopes (`None`: a session without scopes).
    pub scopes: Option<Vec<String>>,
}

impl Authenticated {
    /// Whether the principal holds the `admin` role.
    pub fn is_admin(&self) -> bool {
        s(&self.principal, "role") == Some("admin")
            && self
                .scopes
                .as_deref()
                .is_none_or(|sc| sc.iter().any(|x| x == "admin"))
    }

    /// The in-process credentials of the binding (STD-PROTO-020) for this principal: the
    /// scopes' grants intersected with the principal's (DSN-NOD-153).
    pub fn in_process(&self) -> Value {
        let own: Option<Vec<String>> = self.principal.get("grants").and_then(|g| {
            g.as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
        });
        let grants = grants_of(self.scopes.as_deref(), own.as_deref());
        let mut c = json!({"kind": "in_process", "principal": self.did, "principal_kind": self.kind,
            "auth_level": self.level});
        if let Some(r) = s(&self.principal, "role") {
            c["role"] = json!(r);
        }
        if let Some(g) = grants {
            c["grants"] = json!(g);
        }
        c
    }
}

/// The identity of one tenant.
pub struct Identity {
    backend: Box<dyn Backend>,
    tenant: String,
    authority: String,
    master: [u8; 32],
    /// Serialises log appends, counters and failure accounting.
    lock: Mutex<()>,
    /// A fixed time (suites), else the system clock.
    fixed_now: Mutex<Option<i64>>,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Identity({})", self.tenant)
    }
}

impl Identity {
    /// An identity over a backend; `master` seals TOTP secrets and seeds custodial keys
    /// (derived from the tenant KEK on a node, random in memory).
    pub fn new(
        backend: Box<dyn Backend>,
        tenant: &str,
        authority: &str,
        master: [u8; 32],
    ) -> Identity {
        Identity {
            backend,
            tenant: tenant.to_string(),
            authority: authority.to_string(),
            master,
            lock: Mutex::new(()),
            fixed_now: Mutex::new(None),
        }
    }

    /// An identity in memory with a random master key.
    pub fn memory(tenant: &str, authority: &str) -> Result<Identity, BpaError> {
        let master: [u8; 32] = crypto::random_bytes()?;
        Ok(Identity::new(
            Box::new(MemoryBackend::default()),
            tenant,
            authority,
            master,
        ))
    }

    /// The tenant slug (the token prefix).
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The authority of registered DIDs.
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// Fix the clock (suites); `None` returns to the system clock.
    pub fn set_now(&self, at: Option<i64>) {
        if let Ok(mut g) = self.fixed_now.lock() {
            *g = at;
        }
    }

    /// The current unix time.
    pub fn now(&self) -> i64 {
        if let Ok(g) = self.fixed_now.lock()
            && let Some(t) = *g
        {
            return t;
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default()
    }

    fn derive(&self, purpose: &str, index: &str) -> [u8; 32] {
        crypto::hkdf_sha256(
            &self.master,
            self.tenant.as_bytes(),
            format!("{purpose}\0{index}").as_bytes(),
        )
    }

    // ---- security log (DSN-NOD-111) ----

    /// Append an entry to the hash-chained log; returns its sequence number.
    pub fn log(&self, kind: &str, record: Value) -> Result<u64, BpaError> {
        let _g = self.lock.lock().map_err(|_| err("lock"))?;
        let head = self.backend.log_head()?;
        let seq = head.as_ref().map_or(0, |h| h["seq"].as_u64().unwrap_or(0)) + 1;
        let prev = head
            .as_ref()
            .and_then(|h| s(h, "hash"))
            .unwrap_or_default()
            .to_string();
        let mut entry = json!({"seq": seq, "at": iso(self.now()), "kind": kind, "record": record, "prev": prev});
        let hash = content_id("ubos.chunk.v1", &entry)?;
        entry["hash"] = json!(hash);
        self.backend.log_append(seq, &entry)?;
        Ok(seq)
    }

    /// `security_log {from?, to?, limit?}`: entries oldest first and the head.
    pub fn security_log(
        &self,
        from: Option<u64>,
        to: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Value, BpaError> {
        let head = self.backend.log_head()?;
        let head_seq = head.as_ref().map_or(0, |h| h["seq"].as_u64().unwrap_or(0));
        let head_hash = head
            .as_ref()
            .and_then(|h| s(h, "hash"))
            .unwrap_or_default()
            .to_string();
        let from = from.unwrap_or(1).max(1);
        let to = to.unwrap_or(head_seq).min(head_seq);
        let limit = limit.unwrap_or(100).clamp(1, 10_000);
        let entries = if from <= to {
            self.backend.log_range(from, to, limit)?
        } else {
            Vec::new()
        };
        Ok(json!({"entries": entries, "head": {"seq": head_seq, "hash": head_hash}}))
    }

    /// Walk the chain from the first entry to the head: a missing entry is a gap, an entry
    /// whose hash or `prev` does not match is broken (DSN-NOD-111 acceptance 1).
    pub fn verify_log(&self) -> Result<Value, BpaError> {
        let head = self.backend.log_head()?;
        let head_seq = head.as_ref().map_or(0, |h| h["seq"].as_u64().unwrap_or(0));
        let head_hash = head
            .as_ref()
            .and_then(|h| s(h, "hash"))
            .unwrap_or_default()
            .to_string();
        let mut gaps = Vec::new();
        let mut broken = Vec::new();
        let mut prev = String::new();
        let mut entries: u64 = 0;
        let mut seq = 1;
        while seq <= head_seq {
            let chunk = self.backend.log_range(seq, head_seq, 1_000)?;
            let end = chunk
                .last()
                .and_then(|e| e["seq"].as_u64())
                .unwrap_or(head_seq);
            let mut by_seq: BTreeMap<u64, Value> = BTreeMap::new();
            for e in chunk {
                by_seq.insert(e["seq"].as_u64().unwrap_or(0), e);
            }
            for n in seq..=end {
                let Some(e) = by_seq.get(&n) else {
                    gaps.push(n);
                    prev.clear();
                    continue;
                };
                entries += 1;
                let mut body = e.clone();
                body.as_object_mut().map(|m| m.remove("hash"));
                let want = content_id("ubos.chunk.v1", &body)?;
                let ok = s(e, "hash") == Some(want.as_str())
                    && (n == 1
                        || gaps.last() == Some(&(n - 1))
                        || s(e, "prev") == Some(prev.as_str()));
                if !ok {
                    broken.push(n);
                }
                prev = s(e, "hash").unwrap_or_default().to_string();
            }
            seq = end + 1;
        }
        let ok = gaps.is_empty() && broken.is_empty() && (head_seq == 0 || prev == head_hash);
        Ok(
            json!({"ok": ok, "entries": entries, "head": {"seq": head_seq, "hash": head_hash},
            "gaps": gaps, "broken": broken}),
        )
    }

    /// Feed the kernel's decision records and security-log entries of a VAE into the node
    /// log (DSN-NOD-111): one node entry per kernel record, carrying the VAE, the kernel
    /// sequence and hash of a security entry, and the decision record as is (no field
    /// values beyond what the kernel logs).
    pub fn mirror(
        &self,
        vae: &str,
        decisions: &[Value],
        security: &[Value],
    ) -> Result<(), BpaError> {
        for d in decisions {
            self.log("dvm.decision", json!({"vae": vae, "decision": d}))?;
        }
        for e in security {
            self.log(
                "dvm.security",
                json!({"vae": vae, "seq": e["seq"], "hash": e["hash"],
                "kind": e["kind"], "at": e["at"], "record": e["record"]}),
            )?;
        }
        Ok(())
    }

    // ---- principals (DSN-NOD-109, -110) ----

    /// Create a principal: a registered-form DID, `invited` for a human (active once a
    /// credential is set), `active` for a service or agent.
    pub fn create_principal(
        &self,
        kind: &str,
        name: &str,
        role: Option<&str>,
        grants: Option<Vec<String>>,
    ) -> Result<Value, BpaError> {
        if !KINDS.contains(&kind) {
            return Err(invalid("principal kind is human, service or agent"));
        }
        if name.trim().is_empty() {
            return Err(invalid("a principal has a name"));
        }
        let did = format!("did:ubos:{}:{}", self.authority, crate::uuid()?);
        let state = if kind == "human" { "invited" } else { "active" };
        let now = self.now();
        let mut p = json!({"did": did, "kind": kind, "name": name, "state": state,
            "created_at": now, "updated_at": now});
        if let Some(r) = role {
            p["role"] = json!(r);
        }
        if let Some(g) = grants {
            p["grants"] = json!(g);
        }
        self.backend.put("principal", &did, &p)?;
        self.log(
            "principal.created",
            json!({"principal": did, "kind": kind, "name": name, "role": role}),
        )?;
        Ok(p)
    }

    /// A principal by DID (`IAM.NOT_FOUND` when unknown).
    pub fn principal(&self, did: &str) -> Result<Value, BpaError> {
        self.backend
            .get("principal", did)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("unknown principal {did}")))
    }

    /// Every principal.
    pub fn principals(&self) -> Result<Vec<Value>, BpaError> {
        self.backend.list("principal")
    }

    /// Change a principal's state (DSN-NOD-109): `invited` → `active`, `active` ⇄
    /// `suspended`, any → `deprovisioned` (final). Suspension and deprovisioning revoke the
    /// principal's sessions and API keys; deprovisioning also closes its keys, which stay in
    /// the history for verification.
    pub fn set_principal_state(&self, did: &str, state: &str) -> Result<Value, BpaError> {
        if !STATES.contains(&state) {
            return Err(invalid(
                "state is invited, active, suspended or deprovisioned",
            ));
        }
        let mut p = self.principal(did)?;
        let from = s(&p, "state").unwrap_or_default().to_string();
        let allowed = match (from.as_str(), state) {
            (f, t) if f == t => true,
            ("deprovisioned", _) => false,
            (_, "invited") => false,
            _ => true,
        };
        if !allowed {
            return Err(BpaError::new(
                "FLOW.TRANSITION_NOT_ALLOWED",
                format!("a principal does not go from {from} to {state}"),
            ));
        }
        p["state"] = json!(state);
        p["updated_at"] = json!(self.now());
        self.backend.put("principal", did, &p)?;
        self.log(
            "principal.state",
            json!({"principal": did, "from": from, "to": state}),
        )?;
        if state == "suspended" || state == "deprovisioned" {
            self.revoke_principal_sessions(did, state)?;
            for k in self.api_keys(Some(did))? {
                if let Some(id) = s(&k, "key_id") {
                    self.revoke_api_key(id)?;
                }
            }
        }
        if state == "deprovisioned" {
            self.close_keys(did)?;
        }
        Ok(p)
    }

    fn require_active(&self, p: &Value) -> Result<(), BpaError> {
        match s(p, "state") {
            Some("active") => Ok(()),
            other => Err(denied(
                "principal_not_active",
                json!({"state": other.unwrap_or("unknown")}),
            )),
        }
    }

    // ---- credentials (DSN-NOD-101) ----

    /// Set the password of a username for a principal (a human; an invited principal
    /// becomes active). The hash is Argon2id; the old password stops working.
    pub fn set_password(&self, username: &str, did: &str, password: &str) -> Result<(), BpaError> {
        let username = username.trim();
        if username.is_empty() || username.len() > 256 {
            return Err(invalid("a username is 1 to 256 characters"));
        }
        if password.chars().count() < 8 || password.len() > 1024 {
            return Err(invalid("a password is at least 8 characters"));
        }
        let mut p = self.principal(did)?;
        if s(&p, "state") == Some("deprovisioned") {
            return Err(denied(
                "principal_not_active",
                json!({"state": "deprovisioned"}),
            ));
        }
        if let Some(c) = self.backend.get("credential", username)?
            && s(&c, "principal") != Some(did)
        {
            return Err(BpaError::new(
                "TXN.ALREADY_EXISTS",
                "the username belongs to another principal",
            ));
        }
        let now = self.now();
        let mut c = self
            .backend
            .get("credential", username)?
            .unwrap_or_else(|| json!({"username": username, "principal": did, "created_at": now}));
        c["hash"] = json!(hash_password(password)?);
        c["failures"] = json!(0);
        c["updated_at"] = json!(now);
        self.backend.put("credential", username, &c)?;
        if s(&p, "state") == Some("invited") {
            p["state"] = json!("active");
            p["updated_at"] = json!(now);
            self.backend.put("principal", did, &p)?;
            self.log(
                "principal.state",
                json!({"principal": did, "from": "invited", "to": "active"}),
            )?;
        }
        self.log(
            "credential.password_set",
            json!({"principal": did, "username": username}),
        )?;
        Ok(())
    }

    /// The principal a username belongs to (`IAM.NOT_FOUND` when unknown).
    pub fn credential_principal(&self, username: &str) -> Result<String, BpaError> {
        self.backend
            .get("credential", username)?
            .and_then(|c| s(&c, "principal").map(str::to_string))
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown username"))
    }

    /// Start TOTP enrolment: a fresh secret sealed in the credential, returned once in
    /// base32 with the `otpauth` URI; it counts from `confirm_totp`.
    pub fn enrol_totp(&self, username: &str) -> Result<Value, BpaError> {
        let mut c = self
            .backend
            .get("credential", username)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown username"))?;
        let secret: [u8; 20] = crypto::random_bytes()?;
        let nonce: [u8; crypto::NONCE_LEN] = crypto::random_bytes()?;
        let key = self.derive("ubos.identity.totp.v1", username);
        let sealed = crypto::seal(&key, &nonce, username.as_bytes(), &secret)?;
        c["totp_sealed"] = json!(hex(&sealed));
        c["totp_nonce"] = json!(hex(&nonce));
        c["totp_enabled"] = json!(false);
        self.backend.put("credential", username, &c)?;
        self.log(
            "credential.totp_enrolled",
            json!({"principal": c["principal"], "username": username}),
        )?;
        let b32 = base32(&secret);
        Ok(
            json!({"secret": b32, "uri": format!("otpauth://totp/UBOS:{username}?secret={b32}&issuer=UBOS&digits=6&period={TOTP_STEP}")}),
        )
    }

    fn totp_secret(&self, username: &str, c: &Value) -> Option<Vec<u8>> {
        let sealed = unhex(s(c, "totp_sealed")?);
        let nonce: [u8; crypto::NONCE_LEN] = unhex(s(c, "totp_nonce")?).try_into().ok()?;
        let key = self.derive("ubos.identity.totp.v1", username);
        crypto::open(&key, &nonce, username.as_bytes(), &sealed).ok()
    }

    fn totp_ok(&self, secret: &[u8], code: &str) -> bool {
        let now = self.now();
        let code = code.trim();
        (-1..=1).any(|k| totp(secret, now + k * TOTP_STEP) == code)
    }

    /// Confirm TOTP enrolment with a current code; sign-ins then require the factor.
    pub fn confirm_totp(&self, username: &str, code: &str) -> Result<(), BpaError> {
        let mut c = self
            .backend
            .get("credential", username)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown username"))?;
        let secret = self
            .totp_secret(username, &c)
            .ok_or_else(|| denied("totp_not_enrolled", json!({})))?;
        if !self.totp_ok(&secret, code) {
            return Err(denied("totp_invalid", json!({})));
        }
        c["totp_enabled"] = json!(true);
        self.backend.put("credential", username, &c)?;
        self.log(
            "credential.totp_enabled",
            json!({"principal": c["principal"], "username": username}),
        )?;
        Ok(())
    }

    /// Whether the account or source may attempt now (`Err` with `retry_after` otherwise).
    fn check_throttle(&self, what: &str, key: &str, rec: &Value) -> Result<(), BpaError> {
        let now = self.now();
        let locked_until = i(rec, "locked_until");
        if locked_until > now {
            self.log(
                "auth.locked_attempt",
                json!({what: key, "retry_after": locked_until - now}),
            )?;
            return Err(denied("locked", json!({"retry_after": locked_until - now})));
        }
        let not_before = i(rec, "not_before");
        if not_before > now {
            self.log(
                "auth.throttled",
                json!({what: key, "retry_after": not_before - now}),
            )?;
            return Err(denied(
                "throttled",
                json!({"retry_after": not_before - now}),
            ));
        }
        Ok(())
    }

    /// Count a failure on an account or source: progressive delay from `DELAY_AFTER`
    /// consecutive failures, a lock at `LOCK_AFTER`; the count resets after a success or
    /// a quiet `LOCK_SECS`. Returns the lock event to log, if any.
    fn record_failure(&self, kind: &str, key: &str, rec: &mut Value) -> Option<(String, Value)> {
        let now = self.now();
        let last = i(rec, "last_failure_at");
        let failures = if now - last > LOCK_SECS {
            1
        } else {
            rec["failures"].as_u64().unwrap_or(0) + 1
        };
        rec["failures"] = json!(failures);
        rec["last_failure_at"] = json!(now);
        if failures >= LOCK_AFTER {
            rec["locked_until"] = json!(now + LOCK_SECS);
            rec["not_before"] = json!(0);
            return Some((
                "auth.locked".into(),
                json!({kind: key, "failures": failures, "until": iso(now + LOCK_SECS)}),
            ));
        } else if failures >= DELAY_AFTER {
            let delay = (1i64 << (failures - DELAY_AFTER).min(20)).min(MAX_DELAY_SECS);
            rec["not_before"] = json!(now + delay);
        }
        None
    }

    fn source_record(&self, source: &str) -> Result<Value, BpaError> {
        Ok(self
            .backend
            .get("source", source)?
            .unwrap_or_else(|| json!({"source": source, "failures": 0})))
    }

    /// Count a failed attempt on the source and, when the account exists, on the account
    /// (under the lock; the log entries follow outside it) and return the error to give.
    fn fail(
        &self,
        username: &str,
        cred: Option<&mut Value>,
        source: &str,
        reason: &str,
    ) -> Result<BpaError, BpaError> {
        let mut events = Vec::new();
        {
            let _g = self.lock.lock().map_err(|_| err("lock"))?;
            let mut src = self.source_record(source)?;
            events.extend(self.record_failure("source", source, &mut src));
            self.backend.put("source", source, &src)?;
            if let Some(c) = cred {
                events.extend(self.record_failure("account", username, c));
                self.backend.put("credential", username, c)?;
            }
        }
        for (kind, rec) in events {
            self.log(&kind, rec)?;
        }
        self.log(
            "auth.failed",
            json!({"username": username, "source": source, "reason": reason}),
        )?;
        Ok(denied("invalid_credentials", json!({})))
    }

    /// Sign in with a password and, once enrolled, a TOTP code; opens a session at the level
    /// `password` or `mfa`. Failures count per account and per source (DSN-NOD-101).
    pub fn sign_in(
        &self,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
        source: &str,
        device: Option<&str>,
    ) -> Result<SignedIn, BpaError> {
        let username = username.trim();
        self.check_throttle("source", source, &self.source_record(source)?)?;
        let Some(mut c) = self.backend.get("credential", username)? else {
            verify_password(dummy_hash(), password);
            return Err(self.fail(username, None, source, "unknown_account")?);
        };
        self.check_throttle("account", username, &c)?;
        let hash = s(&c, "hash").unwrap_or_default().to_string();
        if !verify_password(&hash, password) {
            return Err(self.fail(username, Some(&mut c), source, "password")?);
        }
        let did = s(&c, "principal").unwrap_or_default().to_string();
        let p = self.principal(&did)?;
        if let Err(e) = self.require_active(&p) {
            self.log(
                "auth.denied",
                json!({"username": username, "principal": did, "source": source,
                "reason": "principal_not_active", "state": p["state"]}),
            )?;
            return Err(e);
        }
        let level = if c["totp_enabled"].as_bool().unwrap_or(false) {
            let secret = self
                .totp_secret(username, &c)
                .ok_or_else(|| denied("totp_not_enrolled", json!({})))?;
            match totp_code {
                None => {
                    self.log(
                        "auth.totp_required",
                        json!({"username": username, "source": source}),
                    )?;
                    return Err(denied("totp_required", json!({})));
                }
                Some(code) if self.totp_ok(&secret, code) => "mfa",
                Some(_) => return Err(self.fail(username, Some(&mut c), source, "totp")?),
            }
        } else {
            "password"
        };
        {
            let _g = self.lock.lock().map_err(|_| err("lock"))?;
            c["failures"] = json!(0);
            c["not_before"] = json!(0);
            c["last_sign_in_at"] = json!(self.now());
            self.backend.put("credential", username, &c)?;
            self.backend.remove("source", source)?;
        }
        let signed = self.open_session(&p, level, source, device, None)?;
        self.log(
            "auth.signed_in",
            json!({"username": username, "principal": did, "source": source,
            "level": level, "session_id": signed.session_id}),
        )?;
        Ok(signed)
    }

    // ---- sessions (DSN-NOD-105, -106) ----

    /// The session policy of a principal kind: idle and absolute timeouts and the
    /// concurrent-session limit (FR-IAM-101).
    pub fn session_policy(&self, kind: &str) -> Result<Value, BpaError> {
        let pol = self
            .backend
            .get("policy", "session")?
            .unwrap_or_else(|| json!({}));
        let default = if kind == "human" {
            json!({"idle_secs": 1800, "absolute_secs": 43_200, "max_sessions": 10})
        } else {
            json!({"idle_secs": 3600, "absolute_secs": 86_400, "max_sessions": 100})
        };
        let mut out = default;
        if let Some(k) = pol.get(kind).and_then(Value::as_object) {
            for (key, v) in k {
                out[key] = v.clone();
            }
        }
        Ok(out)
    }

    /// Set the session policy of a principal kind (an administrative action).
    pub fn set_session_policy(&self, kind: &str, policy: &Value) -> Result<Value, BpaError> {
        if !KINDS.contains(&kind) {
            return Err(invalid("principal kind is human, service or agent"));
        }
        for k in ["idle_secs", "absolute_secs", "max_sessions"] {
            if let Some(v) = policy.get(k)
                && v.as_i64().is_none_or(|n| n <= 0)
            {
                return Err(invalid(&format!("{k} is a positive integer")));
            }
        }
        let mut pol = self
            .backend
            .get("policy", "session")?
            .unwrap_or_else(|| json!({}));
        let mut entry = pol.get(kind).cloned().unwrap_or_else(|| json!({}));
        for k in ["idle_secs", "absolute_secs", "max_sessions"] {
            if let Some(v) = policy.get(k) {
                entry[k] = v.clone();
            }
        }
        pol[kind] = entry;
        self.backend.put("policy", "session", &pol)?;
        self.log("policy.session", json!({"kind": kind, "policy": pol[kind]}))?;
        self.session_policy(kind)
    }

    /// Open a session for a principal (after a sign-in or for a trusted caller).
    pub fn open_session(
        &self,
        principal: &Value,
        level: &str,
        source: &str,
        device: Option<&str>,
        scopes: Option<Vec<String>>,
    ) -> Result<SignedIn, BpaError> {
        self.require_active(principal)?;
        let did = s(principal, "did").unwrap_or_default().to_string();
        let kind = s(principal, "kind").unwrap_or("human").to_string();
        let pol = self.session_policy(&kind)?;
        let now = self.now();
        let secret: [u8; TOKEN_BYTES] = crypto::random_bytes()?;
        let token = format!("{}.{}", self.tenant, b64u(&secret));
        let hash = token_hash(&token);
        let session_id = crate::uuid()?;
        let expires_at = now + i(&pol, "absolute_secs");
        let rec = json!({"session_id": session_id, "hash": hash, "principal": did, "principal_kind": kind,
            "level": level, "source": source, "device": device, "scopes": scopes,
            "created_at": now, "last_seen": now, "expires_at": expires_at,
            "idle_secs": pol["idle_secs"], "revoked": false});
        // the concurrent-session limit: the oldest live sessions make room
        let max = pol["max_sessions"].as_u64().unwrap_or(10) as usize;
        let mut live: Vec<Value> = self
            .sessions(Some(&did))?
            .into_iter()
            .filter(|x| !x["revoked"].as_bool().unwrap_or(false))
            .collect();
        live.sort_by_key(|x| i(x, "created_at"));
        while live.len() + 1 > max {
            let old = live.remove(0);
            if let Some(id) = s(&old, "session_id") {
                self.revoke_session(id, "evicted")?;
            }
        }
        self.backend.put("session", &hash, &rec)?;
        self.backend
            .put("session_id", &session_id, &json!({"hash": hash}))?;
        self.log(
            "session.opened",
            json!({"session_id": session_id, "principal": did, "level": level,
            "source": source, "device": device, "expires_at": iso(expires_at)}),
        )?;
        Ok(SignedIn {
            token,
            session_id,
            principal: did,
            level: level.to_string(),
            expires_at,
        })
    }

    /// The sessions of a principal, or every session (without hashes).
    pub fn sessions(&self, principal: Option<&str>) -> Result<Vec<Value>, BpaError> {
        Ok(self
            .backend
            .list("session")?
            .into_iter()
            .filter(|x| principal.is_none_or(|p| s(x, "principal") == Some(p)))
            .map(|mut x| {
                x.as_object_mut().map(|m| m.remove("hash"));
                x
            })
            .collect())
    }

    /// A session record by identifier.
    fn session_by_id(&self, session_id: &str) -> Result<Option<(String, Value)>, BpaError> {
        let Some(ix) = self.backend.get("session_id", session_id)? else {
            return Ok(None);
        };
        let hash = s(&ix, "hash").unwrap_or_default().to_string();
        Ok(self.backend.get("session", &hash)?.map(|r| (hash, r)))
    }

    /// The principal of a session identifier, if the session exists.
    pub fn session_principal(&self, session_id: &str) -> Result<Option<String>, BpaError> {
        Ok(self
            .session_by_id(session_id)?
            .and_then(|(_, r)| s(&r, "principal").map(str::to_string)))
    }

    /// Revoke a session (DSN-NOD-106): it fails validation from now on.
    pub fn revoke_session(&self, session_id: &str, reason: &str) -> Result<bool, BpaError> {
        let Some((hash, mut r)) = self.session_by_id(session_id)? else {
            return Ok(false);
        };
        if r["revoked"].as_bool().unwrap_or(false) {
            return Ok(true);
        }
        r["revoked"] = json!(true);
        r["revoked_at"] = json!(self.now());
        r["revoke_reason"] = json!(reason);
        self.backend.put("session", &hash, &r)?;
        self.log(
            "session.revoked",
            json!({"session_id": session_id, "principal": r["principal"], "reason": reason}),
        )?;
        Ok(true)
    }

    /// Revoke every session of a principal.
    pub fn revoke_principal_sessions(&self, did: &str, reason: &str) -> Result<u64, BpaError> {
        let mut n = 0;
        for x in self.sessions(Some(did))? {
            if !x["revoked"].as_bool().unwrap_or(false)
                && let Some(id) = s(&x, "session_id")
                && self.revoke_session(id, reason)?
            {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Sign out: revoke the session of a token.
    pub fn sign_out(&self, token: &str) -> Result<bool, BpaError> {
        let Some(r) = self.backend.get("session", &token_hash(token))? else {
            return Ok(false);
        };
        self.revoke_session(s(&r, "session_id").unwrap_or_default(), "sign_out")
    }

    /// Validate a session token or an API key presented from `source` (DSN-NOD-105,
    /// -106, -150): the token's tenant, the record, revocation, the timeouts or expiry, the
    /// source against the key's IP ranges and the principal's state, all re-checked on every
    /// call; the session's `last_seen` or the key's `last_used` is advanced.
    pub fn validate(&self, token: &str, source: &str) -> Result<Authenticated, BpaError> {
        let Some(rest) = token.strip_prefix(&format!("{}.", self.tenant)) else {
            self.log(
                "auth.rejected",
                json!({"source": source, "reason": "other_tenant"}),
            )?;
            return Err(denied("invalid_token", json!({})));
        };
        if rest.starts_with("ak.") {
            return self.validate_api_key(token, source);
        }
        let hash = token_hash(token);
        let Some(mut r) = self.backend.get("session", &hash)? else {
            return Err(denied("invalid_token", json!({})));
        };
        let session_id = s(&r, "session_id").unwrap_or_default().to_string();
        if r["revoked"].as_bool().unwrap_or(false) {
            return Err(denied("session_revoked", json!({"session_id": session_id})));
        }
        let now = self.now();
        let expired = if now >= i(&r, "expires_at") {
            Some("absolute_timeout")
        } else if now - i(&r, "last_seen") > i(&r, "idle_secs") {
            Some("idle_timeout")
        } else {
            None
        };
        if let Some(why) = expired {
            r["revoked"] = json!(true);
            r["revoked_at"] = json!(now);
            r["revoke_reason"] = json!(why);
            self.backend.put("session", &hash, &r)?;
            self.log(
                "session.expired",
                json!({"session_id": session_id, "principal": r["principal"], "reason": why}),
            )?;
            return Err(denied(
                "session_expired",
                json!({"session_id": session_id, "cause": why}),
            ));
        }
        let did = s(&r, "principal").unwrap_or_default().to_string();
        let p = self.principal(&did)?;
        self.require_active(&p)?;
        if now - i(&r, "last_seen") >= 30 {
            r["last_seen"] = json!(now);
            self.backend.put("session", &hash, &r)?;
        }
        let scopes = r["scopes"].as_array().map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });
        Ok(Authenticated {
            kind: s(&p, "kind").unwrap_or("human").to_string(),
            principal: p,
            did,
            credential_id: session_id,
            credential: "session",
            level: s(&r, "level").unwrap_or("password").to_string(),
            scopes,
        })
    }

    // ---- API keys (DSN-NOD-150, -153) ----

    /// Issue an API key for a principal: the key is returned once; the record keeps its
    /// hash, scopes, IP ranges and expiry (90 days unless `ttl_secs` is given).
    pub fn issue_api_key(
        &self,
        did: &str,
        name: &str,
        scopes: Vec<String>,
        ip_ranges: Vec<String>,
        ttl_secs: Option<i64>,
    ) -> Result<Value, BpaError> {
        let p = self.principal(did)?;
        self.require_active(&p)?;
        if scopes.is_empty() {
            return Err(invalid("an API key has at least one scope"));
        }
        for sc in &scopes {
            check_scope(sc)?;
        }
        for r in &ip_ranges {
            let net = r.split('/').next().unwrap_or_default();
            if net.parse::<std::net::IpAddr>().is_err() {
                return Err(invalid(&format!("{r:?} is not an address or CIDR range")));
            }
        }
        let ttl = ttl_secs.unwrap_or(90 * 86_400);
        if ttl <= 0 {
            return Err(invalid("ttl_secs is positive"));
        }
        let now = self.now();
        let secret: [u8; TOKEN_BYTES] = crypto::random_bytes()?;
        let key = format!("{}.ak.{}", self.tenant, b64u(&secret));
        let hash = token_hash(&key);
        let key_id = crate::uuid()?;
        let rec = json!({"key_id": key_id, "hash": hash, "principal": did, "name": name, "scopes": scopes,
            "ip_ranges": ip_ranges, "created_at": now, "expires_at": now + ttl, "revoked": false, "last_used": null});
        self.backend.put("api_key", &hash, &rec)?;
        self.backend
            .put("api_key_id", &key_id, &json!({"hash": hash}))?;
        self.log(
            "api_key.issued",
            json!({"key_id": key_id, "principal": did, "name": name, "scopes": scopes,
            "ip_ranges": ip_ranges, "expires_at": iso(now + ttl)}),
        )?;
        Ok(json!({"key": key, "key_id": key_id, "expires_at": now + ttl, "scopes": scopes}))
    }

    /// The API keys of a principal, or every key (without hashes).
    pub fn api_keys(&self, principal: Option<&str>) -> Result<Vec<Value>, BpaError> {
        Ok(self
            .backend
            .list("api_key")?
            .into_iter()
            .filter(|x| principal.is_none_or(|p| s(x, "principal") == Some(p)))
            .map(|mut x| {
                x.as_object_mut().map(|m| m.remove("hash"));
                x
            })
            .collect())
    }

    /// Revoke an API key by identifier.
    pub fn revoke_api_key(&self, key_id: &str) -> Result<bool, BpaError> {
        let Some(ix) = self.backend.get("api_key_id", key_id)? else {
            return Ok(false);
        };
        let hash = s(&ix, "hash").unwrap_or_default().to_string();
        let Some(mut r) = self.backend.get("api_key", &hash)? else {
            return Ok(false);
        };
        if !r["revoked"].as_bool().unwrap_or(false) {
            r["revoked"] = json!(true);
            r["revoked_at"] = json!(self.now());
            self.backend.put("api_key", &hash, &r)?;
            self.log(
                "api_key.revoked",
                json!({"key_id": key_id, "principal": r["principal"]}),
            )?;
        }
        Ok(true)
    }

    fn validate_api_key(&self, key: &str, source: &str) -> Result<Authenticated, BpaError> {
        let hash = token_hash(key);
        let Some(mut r) = self.backend.get("api_key", &hash)? else {
            self.log(
                "api_key.rejected",
                json!({"source": source, "reason": "unknown"}),
            )?;
            return Err(denied("invalid_token", json!({})));
        };
        let key_id = s(&r, "key_id").unwrap_or_default().to_string();
        let now = self.now();
        if r["revoked"].as_bool().unwrap_or(false) {
            return Err(denied("api_key_revoked", json!({"key_id": key_id})));
        }
        if now >= i(&r, "expires_at") {
            self.log(
                "api_key.rejected",
                json!({"key_id": key_id, "source": source, "reason": "expired"}),
            )?;
            return Err(denied("api_key_expired", json!({"key_id": key_id})));
        }
        let ranges: Vec<&str> = r["ip_ranges"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !ranges.is_empty() && !ranges.iter().any(|rg| ip_in(rg, source)) {
            self.log(
                "api_key.rejected",
                json!({"key_id": key_id, "source": source, "reason": "source_not_allowed"}),
            )?;
            return Err(denied("source_not_allowed", json!({"key_id": key_id})));
        }
        let did = s(&r, "principal").unwrap_or_default().to_string();
        let p = self.principal(&did)?;
        self.require_active(&p)?;
        if now - i(&r, "last_used") >= 60 {
            r["last_used"] = json!(now);
            self.backend.put("api_key", &hash, &r)?;
        }
        let scopes: Vec<String> = r["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        Ok(Authenticated {
            kind: s(&p, "kind").unwrap_or("service").to_string(),
            principal: p,
            did,
            credential_id: key_id,
            credential: "api_key",
            level: "api_key".into(),
            scopes: Some(scopes),
        })
    }

    // ---- principal keys (DSN-NOD-108) ----

    fn key_seed(&self, did: &str, n: u64) -> [u8; 32] {
        self.derive("ubos.principal-key.v1", &format!("{did}/{n}"))
    }

    fn key_entry(&self, did: &str, n: u64, public: &[u8; 32], custody: &str) -> Value {
        let mut raw = vec![0xed, 0x01];
        raw.extend_from_slice(public);
        json!({"id": format!("{did}#key-{n}"), "n": n, "public_key": b64u(public),
            "multibase": format!("z{}", b58encode(&raw)), "custody": custody,
            "valid_from": self.now(), "valid_to": null})
    }

    /// The key history of a principal, creating a custodial key at first need.
    pub fn ensure_key(&self, did: &str) -> Result<Value, BpaError> {
        self.principal(did)?;
        if let Some(h) = self.backend.get("principal_key", did)? {
            return Ok(h);
        }
        let public = crypto::ed25519_public(&self.key_seed(did, 1));
        let h = json!({"did": did, "keys": [self.key_entry(did, 1, &public, "custodial")]});
        self.backend.put("principal_key", did, &h)?;
        self.log(
            "key.created",
            json!({"principal": did, "key": format!("{did}#key-1"), "custody": "custodial"}),
        )?;
        Ok(h)
    }

    fn current_key(h: &Value) -> Option<&Value> {
        h["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .find(|k| k["valid_to"].is_null())
    }

    fn close_current(&self, h: &mut Value) {
        let now = self.now();
        if let Some(keys) = h["keys"].as_array_mut() {
            for k in keys.iter_mut().rev() {
                if k["valid_to"].is_null() {
                    k["valid_to"] = json!(now);
                    break;
                }
            }
        }
    }

    /// Rotate: the current key's validity ends now, a new custodial key begins (FR-IAM-033).
    pub fn rotate_key(&self, did: &str) -> Result<Value, BpaError> {
        let mut h = self.ensure_key(did)?;
        self.close_current(&mut h);
        let n = h["keys"].as_array().map_or(0, Vec::len) as u64 + 1;
        let public = crypto::ed25519_public(&self.key_seed(did, n));
        let entry = self.key_entry(did, n, &public, "custodial");
        if let Some(k) = h["keys"].as_array_mut() {
            k.push(entry);
        }
        self.backend.put("principal_key", did, &h)?;
        self.log(
            "key.rotated",
            json!({"principal": did, "key": format!("{did}#key-{n}")}),
        )?;
        Ok(h)
    }

    /// Register a client-held public key (a device or agent that brings its own): it
    /// becomes the current key; the node never holds its private part.
    pub fn register_key(&self, did: &str, public_key: &Value) -> Result<Value, BpaError> {
        let public: [u8; 32] = unb64u(public_key, 32, "public_key")?
            .try_into()
            .map_err(|_| invalid("public_key"))?;
        let mut h = self.ensure_key(did)?;
        self.close_current(&mut h);
        let n = h["keys"].as_array().map_or(0, Vec::len) as u64 + 1;
        let entry = self.key_entry(did, n, &public, "client");
        if let Some(k) = h["keys"].as_array_mut() {
            k.push(entry);
        }
        self.backend.put("principal_key", did, &h)?;
        self.log(
            "key.registered",
            json!({"principal": did, "key": format!("{did}#key-{n}"), "custody": "client"}),
        )?;
        Ok(h)
    }

    /// Close every key of a principal (deprovisioning); the history stays.
    fn close_keys(&self, did: &str) -> Result<(), BpaError> {
        if let Some(mut h) = self.backend.get("principal_key", did)? {
            self.close_current(&mut h);
            self.backend.put("principal_key", did, &h)?;
            self.log("key.closed", json!({"principal": did}))?;
        }
        Ok(())
    }

    /// The DID document of a registered-form principal (STD-ADDR-010): every verification
    /// method with its validity period, newest last, and the node's UBTP service.
    pub fn did_document(&self, did: &str) -> Result<Value, BpaError> {
        let h = self.ensure_key(did)?;
        let methods: Vec<Value> = h["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|k| {
                json!({"id": k["id"], "type": "Ed25519VerificationKey2020", "controller": did,
                    "publicKeyMultibase": k["multibase"], "custody": k["custody"],
                    "validFrom": iso(i(k, "valid_from")),
                    "validTo": k["valid_to"].as_i64().map(iso)})
            })
            .collect();
        let current: Vec<Value> = Self::current_key(&h)
            .map(|k| vec![k["id"].clone()])
            .unwrap_or_default();
        Ok(
            json!({"@context": ["https://www.w3.org/ns/did/v1"], "id": did,
            "verificationMethod": methods, "authentication": current, "assertionMethod": current,
            "service": [{"id": format!("{did}#ubtp"), "type": "UbtpService", "serviceEndpoint": "/ubtp/v1"}]}),
        )
    }

    /// Sign with the principal's current custodial key; `IAM.KEY_UNAVAILABLE` when the
    /// current key is client-held or closed.
    pub fn sign(&self, did: &str, message: &[u8]) -> Result<Value, BpaError> {
        let h = self.ensure_key(did)?;
        let Some(k) = Self::current_key(&h) else {
            return Err(BpaError::new(
                "IAM.KEY_UNAVAILABLE",
                "the principal's keys are closed",
            ));
        };
        if s(k, "custody") != Some("custodial") {
            return Err(BpaError::new(
                "IAM.KEY_UNAVAILABLE",
                "the current key is client-held",
            ));
        }
        let n = k["n"].as_u64().unwrap_or(1);
        let sig = crypto::ed25519_sign(&self.key_seed(did, n), message);
        self.log("key.signed", json!({"principal": did, "key": k["id"]}))?;
        Ok(json!({"key": k["id"], "algorithm": "ed25519", "signature": b64u(&sig)}))
    }

    /// Verify a signature against the named historical key: valid when the key existed
    /// and, if `at` is given, was valid then (DSN-NOD-108 acceptance 1).
    pub fn verify(
        &self,
        did: &str,
        key_id: &str,
        message: &[u8],
        signature: &Value,
        at: Option<i64>,
    ) -> Result<Value, BpaError> {
        let h = self
            .backend
            .get("principal_key", did)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "the principal has no keys"))?;
        let Some(k) = h["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|k| s(k, "id") == Some(key_id))
        else {
            return Err(BpaError::new(
                "IAM.NOT_FOUND",
                format!("unknown key {key_id}"),
            ));
        };
        let public: [u8; 32] = unb64u(&k["public_key"], 32, "public_key")?
            .try_into()
            .map_err(|_| invalid("public_key"))?;
        let sig: [u8; 64] = unb64u(signature, 64, "signature")?
            .try_into()
            .map_err(|_| invalid("signature"))?;
        let ok = crypto::ed25519_verify(&public, message, &sig);
        let in_period = at.is_none_or(|t| {
            t >= i(k, "valid_from") && k["valid_to"].as_i64().is_none_or(|end| t < end)
        });
        Ok(
            json!({"ok": ok && in_period, "signature_ok": ok, "key_valid_at": in_period, "key": key_id}),
        )
    }

    // ---- bootstrap ----

    /// Create the first administrator when the username is free: a human principal with
    /// the `admin` role and the password (the `--admin` option of `ubos-server`).
    pub fn bootstrap_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<String>, BpaError> {
        if self.backend.get("credential", username)?.is_some() {
            return Ok(None);
        }
        let p = self.create_principal("human", username, Some("admin"), None)?;
        let did = s(&p, "did").unwrap_or_default().to_string();
        self.set_password(username, &did, password)?;
        Ok(Some(did))
    }
}
