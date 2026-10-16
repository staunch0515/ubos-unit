//! `ubos-auth-suite` — SUITE-NOD-AUTH (PH-1 step 14, DEC-075): the local identity of the
//! node against the acceptance criteria of DSN-NOD-101, -105, -106, -108, -109, -110, -111,
//! -150 and -153, on the memory backend or on PostgreSQL, and the Server's enforcement of
//! revocation and scopes over its bindings.
//!
//! ```text
//! ubos-auth-suite [--database URL] [--report <file>]
//! ```
//!
//! The identity cases run on a fixed clock, so timeouts and lockouts are checked at the
//! stated durations without waiting. With `--database` each run creates fresh tenants in
//! the node's registry (migration 0008); the Server cases run over a memory VAE either way.

use futures_util::StreamExt;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ubos_dvm::types::{BpaError, canonical};
use ubos_dvm::{crypto, types::addr::b64u};
use ubos_node::identity::{self, Backend, Identity, MemoryBackend};
use ubos_node::server::{Preset, Server, VaeHandle, memory_vae};

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn check(cond: bool, what: &str) -> Result<(), BpaError> {
    if cond {
        Ok(())
    } else {
        Err(err(format!("check failed: {what}")))
    }
}

fn reason(r: &Result<impl std::fmt::Debug, BpaError>) -> String {
    match r {
        Err(e) => e
            .details
            .as_ref()
            .and_then(|d| d["reason"].as_str())
            .unwrap_or(e.code)
            .to_string(),
        Ok(_) => "ok".into(),
    }
}

const T0: i64 = 1_790_000_000;
const PW: &str = "correct horse battery";

/// A memory backend shared with the suite (to delete a log row).
struct SharedMem(Arc<MemoryBackend>);

impl Backend for SharedMem {
    fn get(&self, k: &str, key: &str) -> Result<Option<Value>, BpaError> {
        self.0.get(k, key)
    }
    fn put(&self, k: &str, key: &str, b: &Value) -> Result<(), BpaError> {
        self.0.put(k, key, b)
    }
    fn remove(&self, k: &str, key: &str) -> Result<(), BpaError> {
        self.0.remove(k, key)
    }
    fn list(&self, k: &str) -> Result<Vec<Value>, BpaError> {
        self.0.list(k)
    }
    fn log_head(&self) -> Result<Option<Value>, BpaError> {
        self.0.log_head()
    }
    fn log_append(&self, seq: u64, e: &Value) -> Result<(), BpaError> {
        self.0.log_append(seq, e)
    }
    fn log_range(&self, f: u64, t: u64, l: u64) -> Result<Vec<Value>, BpaError> {
        self.0.log_range(f, t, l)
    }
}

/// Where the identities live.
enum Place {
    Memory,
    Node(Arc<ubos_node::Node>),
}

/// A fresh identity and a way to delete one of its log rows.
struct Fresh {
    id: Identity,
    delete_log: Box<dyn Fn(u64) -> Result<(), BpaError>>,
}

impl Place {
    fn fresh(&self, prefix: &str) -> Result<Fresh, BpaError> {
        let suffix: [u8; 4] = crypto::random_bytes()?;
        let slug = format!(
            "{prefix}{}",
            suffix
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let f = match self {
            Place::Memory => {
                let mem = Arc::new(MemoryBackend::default());
                let master: [u8; 32] = crypto::random_bytes()?;
                let m2 = mem.clone();
                Fresh {
                    id: Identity::new(Box::new(SharedMem(mem)), &slug, &slug, master),
                    delete_log: Box::new(move |seq| {
                        m2.delete_log_entry(seq);
                        Ok(())
                    }),
                }
            }
            Place::Node(node) => {
                let t = node.create_tenant(&slug)?;
                let db = node.tenant_db(&t)?;
                let tid = t.id.clone();
                Fresh {
                    id: node.identity(&t)?,
                    delete_log: Box::new(move |seq| {
                        db.execute(
                            "DELETE FROM node_security_log WHERE tenant_id = $1::uuid AND seq = $2",
                            &[
                                ubos_node::db::Param::Text(&tid),
                                ubos_node::db::Param::Int(seq as i64),
                            ],
                        )
                        .map(|_| ())
                    }),
                }
            }
        };
        f.id.set_now(Some(T0));
        Ok(f)
    }
}

/// A human with a password.
fn human(id: &Identity, user: &str) -> Result<String, BpaError> {
    let p = id.create_principal("human", user, None, None)?;
    let did = p["did"].as_str().unwrap_or_default().to_string();
    id.set_password(user, &did, PW)?;
    Ok(did)
}

fn log_kinds(id: &Identity) -> Result<Vec<String>, BpaError> {
    Ok(id.security_log(None, None, Some(10_000))?["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["kind"].as_str().map(str::to_string))
        .collect())
}

// ---- identity cases ----

fn credentials_argon2id_totp(p: &Place) -> Result<Value, BpaError> {
    let f = p.fresh("auth")?;
    let id = &f.id;
    human(id, "alice")?;
    let rec = id
        .backend_record("credential", "alice")?
        .unwrap_or_default();
    let phc = rec["hash"].as_str().unwrap_or_default();
    check(
        phc.starts_with("$argon2id$v=19$m=65536,t=3,"),
        "Argon2id with 64 MiB and 3 iterations (DSN-NOD-101)",
    )?;
    check(!rec.to_string().contains(PW), "the password is not stored")?;
    let s = id.sign_in("alice", PW, None, "10.0.0.1", None)?;
    check(s.level == "password", "password level")?;
    let enrol = id.enrol_totp("alice")?;
    let secret = base32_decode(enrol["secret"].as_str().unwrap_or_default());
    check(
        reason(&id.confirm_totp("alice", "000000")) == "totp_invalid",
        "a wrong code does not confirm",
    )?;
    id.confirm_totp("alice", &identity::totp(&secret, T0))?;
    check(
        reason(&id.sign_in("alice", PW, None, "10.0.0.1", None)) == "totp_required",
        "TOTP required once enrolled",
    )?;
    let s = id.sign_in(
        "alice",
        PW,
        Some(&identity::totp(&secret, T0 + 30)),
        "10.0.0.1",
        None,
    )?;
    check(
        s.level == "mfa",
        "a code of the next step is accepted (±1 step) at the mfa level",
    )?;
    check(
        !id.backend_record("credential", "alice")?
            .unwrap_or_default()
            .to_string()
            .contains(&enrol["secret"].as_str().unwrap_or_default().to_string()),
        "the TOTP secret is stored sealed",
    )?;
    Ok(json!({"phc_prefix": &phc[..30.min(phc.len())]}))
}

fn base32_decode(s: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let (mut acc, mut bits, mut out) = (0u32, 0u32, Vec::new());
    for c in s.bytes() {
        let Some(d) = A.iter().position(|x| *x == c) else {
            continue;
        };
        acc = (acc << 5) | d as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    out
}

fn brute_force(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-101 acceptance 1: 20 failed logins in a minute, then the 21st is delayed or
    // locked and a security event is logged; per account across sources and per source
    // across accounts
    let f = p.fresh("auth")?;
    let id = &f.id;
    human(id, "bob")?;
    let mut reasons = Vec::new();
    for n in 0..20 {
        id.set_now(Some(T0 + 2 * n));
        // a different source each time: the account counter is what locks
        let r = id.sign_in("bob", "wrong", None, &format!("10.1.0.{n}"), None);
        reasons.push(reason(&r));
    }
    id.set_now(Some(T0 + 50));
    let r21 = id.sign_in("bob", PW, None, "10.9.9.9", None);
    check(
        matches!(reason(&r21).as_str(), "locked" | "throttled"),
        "the 21st attempt, with the right password, is refused",
    )?;
    check(
        reasons.iter().any(|r| r == "throttled"),
        "progressive delay before the lock",
    )?;
    let kinds = log_kinds(id)?;
    check(
        kinds.iter().any(|k| k == "auth.locked"),
        "the lock is a security event",
    )?;
    check(
        kinds.iter().any(|k| k == "auth.locked_attempt"),
        "the refused attempt is a security event",
    )?;
    id.set_now(Some(T0 + 50 + identity::LOCK_SECS + 1));
    check(
        id.sign_in("bob", PW, None, "10.9.9.9", None).is_ok(),
        "the lock lapses after LOCK_SECS",
    )?;
    // per source: failures on several accounts from one address throttle that address
    let g = p.fresh("auth")?;
    human(&g.id, "carol")?;
    for n in 0..6 {
        g.id.set_now(Some(T0 + n));
        let _ =
            g.id.sign_in(&format!("nobody{n}"), "wrong", None, "10.2.0.1", None);
    }
    g.id.set_now(Some(T0 + 6));
    check(
        reason(&g.id.sign_in("carol", PW, None, "10.2.0.1", None)) == "throttled",
        "the source is throttled across accounts",
    )?;
    check(
        g.id.sign_in("carol", PW, None, "10.2.0.2", None).is_ok(),
        "another source is not",
    )?;
    Ok(json!({"attempts": reasons, "attempt_21": reason(&r21)}))
}

fn sessions_and_timeouts(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-105 acceptance 1: idle timeout of 15 minutes, idle for 16, re-authentication
    let f = p.fresh("auth")?;
    let id = &f.id;
    let did = human(id, "dora")?;
    id.set_session_policy(
        "human",
        &json!({"idle_secs": 900, "absolute_secs": 3600, "max_sessions": 2}),
    )?;
    let s = id.sign_in("dora", PW, None, "10.0.0.1", Some("laptop"))?;
    let rec = id
        .backend_record_by("session", |r| r["session_id"] == json!(s.session_id))?
        .unwrap_or_default();
    check(
        !rec.to_string().contains(&s.token),
        "the token is stored hashed",
    )?;
    check(
        rec["device"] == "laptop" && rec["level"] == "password",
        "device binding and level",
    )?;
    id.set_now(Some(T0 + 14 * 60));
    check(
        id.validate(&s.token, "10.0.0.1").is_ok(),
        "valid after 14 idle minutes",
    )?;
    id.set_now(Some(T0 + 14 * 60 + 16 * 60));
    let r = id.validate(&s.token, "10.0.0.1");
    check(
        reason(&r) == "session_expired"
            && r.as_ref()
                .err()
                .and_then(|e| e.details.clone())
                .is_some_and(|d| d["cause"] == "idle_timeout"),
        "idle 16 minutes: re-authentication",
    )?;
    // absolute timeout even when active
    id.set_now(Some(T0 + 4000));
    let s2 = id.sign_in("dora", PW, None, "10.0.0.1", None)?;
    for k in 1..=7 {
        id.set_now(Some(T0 + 4000 + k * 500));
        check(
            id.validate(&s2.token, "10.0.0.1").is_ok(),
            "active within the absolute bound",
        )?;
    }
    id.set_now(Some(T0 + 4000 + 3600));
    let r = id.validate(&s2.token, "10.0.0.1");
    check(
        reason(&r) == "session_expired"
            && r.as_ref()
                .err()
                .and_then(|e| e.details.clone())
                .is_some_and(|d| d["cause"] == "absolute_timeout"),
        "absolute timeout of an active session",
    )?;
    // concurrent limit: the oldest live session makes room
    id.set_now(Some(T0 + 9000));
    let a = id.sign_in("dora", PW, None, "10.0.0.1", None)?;
    id.set_now(Some(T0 + 9001));
    let _b = id.sign_in("dora", PW, None, "10.0.0.1", None)?;
    id.set_now(Some(T0 + 9002));
    let _c = id.sign_in("dora", PW, None, "10.0.0.1", None)?;
    check(
        reason(&id.validate(&a.token, "10.0.0.1")) == "session_revoked",
        "the third session evicts the first",
    )?;
    // a token of another tenant
    let other = p.fresh("auth")?;
    check(
        reason(&other.id.validate(&a.token, "10.0.0.1")) == "invalid_token",
        "a token is valid in its tenant only",
    )?;
    let live = id
        .sessions(Some(&did))?
        .iter()
        .filter(|x| x["revoked"] == false)
        .count();
    Ok(json!({"live_sessions": live}))
}

fn revocation(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-106 (single node): a revoked session fails its next validation
    let f = p.fresh("auth")?;
    let id = &f.id;
    let did = human(id, "erin")?;
    let s = id.sign_in("erin", PW, None, "10.0.0.1", None)?;
    check(id.validate(&s.token, "10.0.0.1").is_ok(), "valid before")?;
    id.revoke_session(&s.session_id, "incident")?;
    check(
        reason(&id.validate(&s.token, "10.0.0.1")) == "session_revoked",
        "revoked at once",
    )?;
    let s2 = id.sign_in("erin", PW, None, "10.0.0.1", None)?;
    let k = id.issue_api_key(&did, "k", vec!["read".into()], vec![], None)?;
    id.set_principal_state(&did, "suspended")?;
    check(
        reason(&id.validate(&s2.token, "10.0.0.1")) == "session_revoked",
        "suspension revokes sessions",
    )?;
    check(
        reason(&id.validate(k["key"].as_str().unwrap_or_default(), "10.0.0.1"))
            == "api_key_revoked",
        "and API keys",
    )?;
    check(
        id.sign_out(&s2.token)?,
        "sign-out of a revoked session is idempotent",
    )?;
    Ok(json!({}))
}

fn principal_keys(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-108 acceptance 1: after a rotation, old signatures verify against the
    // historical key
    let f = p.fresh("auth")?;
    let id = &f.id;
    let svc = id.create_principal("service", "importer", None, None)?;
    let did = svc["did"].as_str().unwrap_or_default().to_string();
    check(
        did.starts_with(&format!("did:ubos:{}:", id.authority())),
        "registered-form DID (STD-ADDR-010)",
    )?;
    let sig = id.sign(&did, b"commit 1")?;
    id.set_now(Some(T0 + 100));
    let h = id.rotate_key(&did)?;
    check(
        h["keys"].as_array().map_or(0, Vec::len) == 2
            && h["keys"][0]["valid_to"] == json!(T0 + 100),
        "rotation closes the old key's validity",
    )?;
    let key = sig["key"].as_str().unwrap_or_default();
    let v = id.verify(&did, key, b"commit 1", &sig["signature"], Some(T0 + 10))?;
    check(
        v["ok"] == true,
        "the old signature verifies against the historical key at its time",
    )?;
    let v = id.verify(&did, key, b"commit 1", &sig["signature"], Some(T0 + 200))?;
    check(
        v["ok"] == false && v["signature_ok"] == true,
        "but the key was not valid after the rotation",
    )?;
    let v = id.verify(&did, key, b"commit 2", &sig["signature"], None)?;
    check(v["signature_ok"] == false, "a forged message fails")?;
    let doc = id.did_document(&did)?;
    check(
        doc["verificationMethod"].as_array().map_or(0, Vec::len) == 2,
        "the DID document keeps the history",
    )?;
    // a client-held key: the node cannot sign with it
    let public = b64u(&crypto::ed25519_public(&[9u8; 32]));
    id.register_key(&did, &json!(public))?;
    check(
        id.sign(&did, b"x").err().map(|e| e.code) == Some("IAM.KEY_UNAVAILABLE"),
        "a client-held key is not used by the node",
    )?;
    Ok(json!({"keys": doc["verificationMethod"].as_array().map_or(0, Vec::len) + 1}))
}

fn principal_lifecycle(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-109 acceptance 1: a deprovisioned user's commits verify; they cannot sign in
    let f = p.fresh("auth")?;
    let id = &f.id;
    let inv = id.create_principal("human", "fred", None, None)?;
    let did = inv["did"].as_str().unwrap_or_default().to_string();
    check(inv["state"] == "invited", "a human starts invited")?;
    id.set_password("fred", &did, PW)?;
    check(
        id.principal(&did)?["state"] == "active",
        "the first credential activates",
    )?;
    let sig = id.sign(&did, b"approved")?;
    id.set_principal_state(&did, "suspended")?;
    check(
        reason(&id.sign_in("fred", PW, None, "10.0.0.1", None)) == "principal_not_active",
        "suspended cannot sign in",
    )?;
    id.set_principal_state(&did, "active")?;
    check(
        id.sign_in("fred", PW, None, "10.0.0.1", None).is_ok(),
        "reactivated can",
    )?;
    id.set_now(Some(T0 + 60));
    id.set_principal_state(&did, "deprovisioned")?;
    check(
        reason(&id.sign_in("fred", PW, None, "10.0.0.1", None)) == "principal_not_active",
        "deprovisioned cannot sign in",
    )?;
    let v = id.verify(
        &did,
        sig["key"].as_str().unwrap_or_default(),
        b"approved",
        &sig["signature"],
        Some(T0 + 1),
    )?;
    check(v["ok"] == true, "their signatures verify")?;
    check(
        id.set_principal_state(&did, "active").is_err(),
        "deprovisioning is final",
    )?;
    check(
        id.sign(&did, b"later").is_err(),
        "and their keys are closed",
    )?;
    Ok(json!({}))
}

fn service_principals_per_tenant(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-110 acceptance 1: a job of tenant A reading tenant B is denied
    let a = p.fresh("auth")?;
    let b = p.fresh("auth")?;
    let svc =
        a.id.create_principal("service", "jobs", None, Some(vec!["read".into()]))?;
    let did = svc["did"].as_str().unwrap_or_default().to_string();
    check(
        svc["state"] == "active",
        "a service principal is active at once",
    )?;
    let k =
        a.id.issue_api_key(&did, "jobs", vec!["read".into()], vec![], None)?;
    let key = k["key"].as_str().unwrap_or_default();
    check(
        a.id.validate(key, "10.0.0.1").is_ok(),
        "valid in its tenant",
    )?;
    check(
        reason(&b.id.validate(key, "10.0.0.1")) == "invalid_token",
        "denied in another tenant",
    )?;
    check(
        b.id.principal(&did).is_err(),
        "the principal is unknown there",
    )?;
    Ok(json!({}))
}

fn security_log_chain(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-111 acceptance 1: a deleted log row is detected as a gap
    let f = p.fresh("auth")?;
    let id = &f.id;
    let did = human(id, "gina")?;
    let s = id.sign_in("gina", PW, None, "10.0.0.1", None)?;
    let _ = id.sign_in("gina", "wrong", None, "10.0.0.1", None);
    id.rotate_key(&did)?;
    id.mirror("acme/acme", &[json!({"decision": "deny", "action": "read"})], &[json!({"seq": 1, "hash": "sha256:00", "kind": "authz.deny", "at": "2026-01-01T00:00:00Z", "record": {}})])?;
    let v = id.verify_log()?;
    check(v["ok"] == true, "the chain verifies")?;
    let kinds = log_kinds(id)?;
    for k in [
        "principal.created",
        "credential.password_set",
        "auth.signed_in",
        "auth.failed",
        "session.opened",
        "key.created",
        "key.rotated",
        "dvm.decision",
        "dvm.security",
    ] {
        check(kinds.iter().any(|x| x == k), &format!("{k} is logged"))?;
    }
    let text = id.security_log(None, None, Some(10_000))?.to_string();
    check(
        !text.contains(PW) && !text.contains(&s.token),
        "no secret reaches the log",
    )?;
    (f.delete_log)(3)?;
    let v = id.verify_log()?;
    check(
        v["ok"] == false && v["gaps"] == json!([3]),
        "a deleted row is a gap",
    )?;
    Ok(json!({"entries": v["entries"], "gaps": v["gaps"]}))
}

fn api_keys(p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-150 acceptance 1: a key used from an unlisted IP is rejected and logged
    let f = p.fresh("auth")?;
    let id = &f.id;
    let svc = id.create_principal("service", "sync", None, None)?;
    let did = svc["did"].as_str().unwrap_or_default().to_string();
    let k = id.issue_api_key(
        &did,
        "sync",
        vec!["write".into()],
        vec!["10.0.0.0/8".into(), "192.168.1.7".into()],
        Some(3600),
    )?;
    let key = k["key"].as_str().unwrap_or_default();
    let secret = key.rsplit('.').next().unwrap_or_default();
    check(secret.len() == 43, "256-bit secret in base64url")?;
    check(
        !id.api_keys(None)?
            .iter()
            .any(|x| x.to_string().contains(secret)),
        "shown once: not in listings",
    )?;
    check(
        id.validate(key, "10.20.30.40").is_ok(),
        "an address in a range",
    )?;
    check(id.validate(key, "192.168.1.7").is_ok(), "a listed address")?;
    check(
        reason(&id.validate(key, "172.16.0.1")) == "source_not_allowed",
        "an unlisted address is rejected",
    )?;
    check(
        log_kinds(id)?.iter().any(|x| x == "api_key.rejected"),
        "and logged",
    )?;
    id.set_now(Some(T0 + 3601));
    check(
        reason(&id.validate(key, "10.0.0.1")) == "api_key_expired",
        "expiry",
    )?;
    check(
        identity::check_scope("delete").is_err(),
        "unknown scopes are refused",
    )?;
    Ok(json!({}))
}

fn scope_intersection(_p: &Place) -> Result<Value, BpaError> {
    // DSN-NOD-153: scopes intersected with the principal's permissions
    let g = |s: &[&str], p: Option<&[&str]>| {
        let s: Vec<String> = s.iter().map(|x| x.to_string()).collect();
        let p: Option<Vec<String>> = p.map(|p| p.iter().map(|x| x.to_string()).collect());
        identity::grants_of(Some(&s), p.as_deref()).unwrap_or_default()
    };
    let ro = g(&["read"], None);
    check(
        ro.contains(&"read".to_string()) && !ro.iter().any(|x| x == "update" || x == "commit.raw"),
        "read scope: no write grants",
    )?;
    let both = g(&["write"], Some(&["read", "update:Customer"]));
    check(
        both.contains(&"read".to_string())
            && both.contains(&"update:Customer".to_string())
            && !both.contains(&"update".to_string())
            && !both.contains(&"create".to_string()),
        "never more than the principal",
    )?;
    let class = g(&["write:Invoice"], Some(&["update"]));
    check(
        class == vec!["update:Invoice".to_string()],
        "a class scope narrows a broad grant",
    )?;
    check(
        !identity::scope_allows_write(Some(&["read".to_string()])),
        "a read scope allows no mutate",
    )?;
    check(
        identity::scope_allows_vae(Some(&["vae:acme".to_string()]), "acme")
            && !identity::scope_allows_vae(Some(&["vae:acme".to_string()]), "beta"),
        "VAE scopes",
    )?;
    Ok(json!({"read": ro, "write_on_update_customer": both}))
}

// ---- Server cases (memory VAE) ----

fn http_json(
    addr: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &Value,
) -> Result<(u16, Value), BpaError> {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(addr).map_err(err)?;
    s.set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(err)?;
    let b = body.to_string();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\n\r\n{b}", b.len()).map_err(err)?;
    let mut raw = String::new();
    s.read_to_string(&mut raw).map_err(err)?;
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = raw.split_once("\r\n\r\n").map(|x| x.1).unwrap_or_default();
    let body = if raw
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        body.split("\r\n").skip(1).step_by(2).collect::<String>()
    } else {
        body.to_string()
    };
    Ok((status, serde_json::from_str(&body).unwrap_or(Value::Null)))
}

/// A Server case: its name, its result and its duration in milliseconds.
type ServerCase = (String, Result<Value, BpaError>, u128);

fn server_cases() -> Result<Vec<ServerCase>, BpaError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let classes = json!([{"name": "Customer", "label": "Customer", "description": "Customer", "namespace": "acme.suite",
        "parent": "DataObject", "kind": "definition", "schema_version": 1,
        "fields": [{"name": "name", "label": "name", "description": "name", "type": "text", "required": true}]}]);
    let handle = VaeHandle::spawn(move || memory_vae(Some(&classes)))?;
    let id = Arc::new(Identity::memory("acme", "acme")?);
    let admin = id.bootstrap_admin("root", PW)?.unwrap_or_default();
    let server = Server::single(handle, id.clone(), Preset::Server, "acme", "acme");
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .map_err(err)?;
    let addr = listener.local_addr().map_err(err)?.to_string();
    rt.spawn(server.serve_with(listener));
    let hs = |cred: Value| json!({"v": "1.0", "id": "h", "type": "handshake", "body": {"vae": "acme", "versions": ["1.0"], "credentials": cred}});
    let mut out = Vec::new();
    let mut case = |name: &str, f: &mut dyn FnMut() -> Result<Value, BpaError>| {
        let t = Instant::now();
        let r = f();
        out.push((name.to_string(), r, t.elapsed().as_millis()));
    };
    // the server preset refuses dev and in-process credentials
    case("server_preset_credentials", &mut || {
        let (_, r) = http_json(&addr, "POST", "/ubtp/v1", None, &hs(json!({"kind": "dev"})))?;
        check(
            r["responses"][0]["body"]["code"] == "IAM.DENIED",
            "dev credentials refused in the server preset",
        )?;
        let (_, r) = http_json(
            &addr,
            "POST",
            "/ubtp/v1",
            None,
            &hs(json!({"kind": "in_process", "principal": admin})),
        )?;
        check(
            r["responses"][0]["body"]["code"] == "IAM.DENIED",
            "in-process credentials refused over the network",
        )?;
        let (st, _) = http_json(&addr, "GET", "/api/v1/acme/Customer/x", None, &Value::Null)?;
        check(st == 403, "the facade needs a credential")?;
        Ok(json!({}))
    });
    // DSN-NOD-153 acceptance 1 over the Server: a read-only scope cannot mutate though the
    // principal could
    case("read_only_scope_denies_mutate", &mut || {
        let (_, s) = http_json(
            &addr,
            "POST",
            "/api/v1/identity/sign-in",
            None,
            &json!({"username": "root", "password": PW}),
        )?;
        let tok = s["token"].as_str().unwrap_or_default().to_string();
        let (_, k) = http_json(
            &addr,
            "POST",
            &format!("/api/v1/identity/principals/{admin}/api-keys"),
            Some(&tok),
            &json!({"name": "ro", "scopes": ["read"]}),
        )?;
        let ro = k["key"].as_str().unwrap_or_default().to_string();
        let commit = json!({"v": "1.0", "id": "c", "type": "mutate", "body": {"vae": "acme", "branch": "main", "commit": {"changes": [
            {"op": "create", "class": "Customer", "object_id": "01890a5d-ac96-7001-8000-000000000001", "slug": "acme", "body": {"name": "Acme"}}]}}});
        let (_, r) = http_json(
            &addr,
            "POST",
            "/ubtp/v1",
            None,
            &json!([hs(json!({"kind": "bearer", "token": tok})), commit.clone()]),
        )?;
        check(
            r["responses"][1]["type"] == "result",
            "the admin's own session commits",
        )?;
        let (_, r) = http_json(
            &addr,
            "POST",
            "/ubtp/v1",
            None,
            &json!([hs(json!({"kind": "api_key", "key": ro})), commit]),
        )?;
        check(
            r["responses"][0]["type"] == "result",
            "the read-only key opens a session",
        )?;
        check(
            r["responses"][1]["body"]["code"] == "IAM.DENIED"
                && r["responses"][1]["body"]["details"]["reason"] == "scope_read_only",
            "and cannot mutate",
        )?;
        let (st, _) = http_json(
            &addr,
            "GET",
            "/api/v1/acme/Customer/acme",
            Some(&ro),
            &Value::Null,
        )?;
        check(st == 200, "but can read through the facade")?;
        Ok(json!({}))
    });
    // DSN-NOD-106 acceptance 1 (single node): revocation closes the open connections
    case("revocation_closes_connections", &mut || {
        let user = id.create_principal("human", "hana", None, None)?;
        let did = user["did"].as_str().unwrap_or_default().to_string();
        id.set_password("hana", &did, PW)?;
        let (_, s) = http_json(
            &addr,
            "POST",
            "/api/v1/identity/sign-in",
            None,
            &json!({"username": "hana", "password": PW}),
        )?;
        let tok = s["token"].as_str().unwrap_or_default().to_string();
        let mut conns = Vec::new();
        for _ in 0..3 {
            use futures_util::SinkExt;
            use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
            let mut req = format!("ws://{addr}/ubtp/v1")
                .into_client_request()
                .map_err(err)?;
            req.headers_mut()
                .insert("Sec-WebSocket-Protocol", "ubtp.v1".parse().map_err(err)?);
            let (mut ws, _) = rt
                .block_on(tokio_tungstenite::connect_async(req))
                .map_err(err)?;
            rt.block_on(
                ws.send(Message::Text(
                    hs(json!({"kind": "bearer", "token": tok}))
                        .to_string()
                        .into(),
                )),
            )
            .map_err(err)?;
            let f = rt
                .block_on(ws.next())
                .ok_or_else(|| err("closed"))?
                .map_err(err)?;
            check(
                f.to_text().unwrap_or_default().contains("\"result\""),
                "bearer handshake",
            )?;
            conns.push(ws);
        }
        let (_, a) = http_json(
            &addr,
            "POST",
            "/api/v1/identity/sign-in",
            None,
            &json!({"username": "root", "password": PW}),
        )?;
        let admin_tok = a["token"].as_str().unwrap_or_default().to_string();
        let started = Instant::now();
        let (st, _) = http_json(
            &addr,
            "DELETE",
            &format!(
                "/api/v1/identity/sessions/{}",
                s["session_id"].as_str().unwrap_or_default()
            ),
            Some(&admin_tok),
            &Value::Null,
        )?;
        check(st == 200, "revoke")?;
        let mut closed = 0;
        for ws in conns.iter_mut() {
            let next = rt
                .block_on(async { tokio::time::timeout(Duration::from_secs(5), ws.next()).await });
            if matches!(
                next,
                Ok(None)
                    | Ok(Some(Err(_)))
                    | Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))))
            ) {
                closed += 1;
            }
        }
        let ms = started.elapsed().as_millis();
        check(closed == 3, "all three connections closed")?;
        let (st, _) = http_json(
            &addr,
            "GET",
            "/api/v1/identity/me",
            Some(&tok),
            &Value::Null,
        )?;
        check(st == 403, "the token is refused")?;
        Ok(json!({"connections": 3, "closed_within_ms": ms, "bound_ms": 60_000}))
    });
    Ok(out)
}

// ---- runner ----

type Case = fn(&Place) -> Result<Value, BpaError>;

const CASES: &[(&str, &str, Case)] = &[
    (
        "credentials_argon2id_totp",
        "DSN-NOD-101",
        credentials_argon2id_totp,
    ),
    ("brute_force_delay_and_lock", "DSN-NOD-101", brute_force),
    (
        "sessions_and_timeouts",
        "DSN-NOD-105",
        sessions_and_timeouts,
    ),
    ("revocation", "DSN-NOD-106", revocation),
    ("principal_keys_and_history", "DSN-NOD-108", principal_keys),
    ("principal_lifecycle", "DSN-NOD-109", principal_lifecycle),
    (
        "service_principals_per_tenant",
        "DSN-NOD-110",
        service_principals_per_tenant,
    ),
    ("security_log_chain", "DSN-NOD-111", security_log_chain),
    ("api_keys", "DSN-NOD-150", api_keys),
    ("scope_intersection", "DSN-NOD-153", scope_intersection),
];

fn run(database: Option<&str>) -> Result<Vec<Value>, BpaError> {
    let place = match database {
        Some(url) => Place::Node(Arc::new(ubos_node::Node::connect(url)?)),
        None => Place::Memory,
    };
    let mut results = Vec::new();
    for (name, clause, f) in CASES {
        let t = Instant::now();
        let r = f(&place);
        results.push(result(name, clause, r, t.elapsed().as_millis()));
    }
    let clauses = ["DSN-NOD-004", "DSN-NOD-153", "DSN-NOD-106"];
    for (i, (name, r, ms)) in server_cases()?.into_iter().enumerate() {
        results.push(result(&format!("server.{name}"), clauses[i.min(2)], r, ms));
    }
    Ok(results)
}

fn result(name: &str, clause: &str, r: Result<Value, BpaError>, ms: u128) -> Value {
    match r {
        Ok(v) => json!({"case": name, "clause": clause, "result": "pass", "ms": ms, "evidence": v}),
        Err(e) => {
            json!({"case": name, "clause": clause, "result": "fail", "ms": ms, "detail": e.to_string()})
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let db = arg("--database");
    let start = Instant::now();
    let results = match run(db.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let passed = results.iter().filter(|r| r["result"] == "pass").count();
    for r in &results {
        println!(
            "{:4} {} ({} ms){}",
            if r["result"] == "pass" { "ok" } else { "FAIL" },
            r["case"].as_str().unwrap_or("?"),
            r["ms"],
            r["detail"]
                .as_str()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default()
        );
    }
    let mut report = json!({
        "suite": "SUITE-NOD-AUTH", "step": "PH-1 14d", "decision": "DEC-075",
        "implementation": {"crate": "ubos-node", "version": env!("CARGO_PKG_VERSION")},
        "environment": {"backend": if db.is_some() { "postgresql" } else { "memory" }, "os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "cases": results.len(), "passed": passed, "results": results,
        "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    let seed: [u8; 32] = crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report).unwrap_or_default();
    let sig = crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] =
        json!({"alg": "ed25519", "key": hex(&crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    if let Some(p) = arg("--report") {
        if let Some(d) = std::path::Path::new(&p).parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(
            &p,
            serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
        );
    }
    println!(
        "auth suite: {passed} of {} cases passed ({} s)",
        report["cases"], report["seconds"]
    );
    if passed != report["cases"].as_u64().unwrap_or(0) as usize {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    /// Every case of SUITE-NOD-AUTH passes in memory, and on the PostgreSQL named by
    /// `UBOS_TEST_DATABASE` when it is set.
    #[test]
    fn auth_suite_passes() {
        let mut dbs = vec![None];
        if let Ok(url) = std::env::var("UBOS_TEST_DATABASE") {
            dbs.push(Some(url));
        }
        for db in dbs {
            let r = super::run(db.as_deref()).expect("suite runs");
            let failed: Vec<String> = r
                .iter()
                .filter(|x| x["result"] != "pass")
                .map(|x| x.to_string())
                .collect();
            assert!(failed.is_empty(), "{db:?}: {failed:?}");
        }
    }
}
