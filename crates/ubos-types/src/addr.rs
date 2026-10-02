//! Addressing (STD-ADDR-001, -002, -010; PH-1 step 4d-2, DEC-054): object URIs with
//! selectors and version suffixes, and the `did:ubos` method.

use crate::error::BpaError;
use crate::names;
use crate::values::validate_value;
use serde_json::{Map, Value, json};

const SELECTORS: &[&str] = &["branch", "commit", "tag", "time", "asof", "key", "view"];
const EXCLUSIVE: &[&str] = &["commit", "tag", "time"];
const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
/// Multicodec prefix of an Ed25519 public key.
const ED25519_PUB: [u8; 2] = [0xed, 0x01];

fn bad(msg: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", msg)
}

fn conflicting(msg: &str) -> BpaError {
    BpaError::new("ADDR.CONFLICTING_SELECTORS", msg)
}

/// STD-ADDR-001: the parts of an object URI; STD-ADDR-002: selector precedence.
pub fn parse_uri(uri: &Value) -> Result<Value, BpaError> {
    let Some(text) = uri.as_str() else {
        return Err(bad("not a ubos URI"));
    };
    let rest = text
        .strip_prefix("ubos://")
        .ok_or_else(|| bad("not a ubos URI"))?;
    // authority/class/slug[?selectors][@vN]
    let (path, version) = match rest.rsplit_once("@v") {
        Some((p, v)) if !p.contains('@') => (p, Some(v)),
        _ => (rest, None),
    };
    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path, None),
    };
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || p.contains(['?', '@'])) {
        return Err(bad("not a ubos URI"));
    }
    let (authority, class, slug) = (parts[0], parts[1], parts[2]);
    let auth: Vec<&str> = authority.split('.').collect();
    if auth.len() > 2 || !auth.iter().all(|a| names::matches("slug", a)) {
        return Err(bad("the authority is tenant[.vae]"));
    }
    if !names::matches("class-name", class) {
        return Err(bad("the class is a class name"));
    }
    if !names::matches("slug", slug) {
        return Err(bad("the object is named by its slug"));
    }
    let mut sel = Map::new();
    if let Some(q) = query {
        for part in q.split('&') {
            let Some((k, v)) = part.split_once('=') else {
                return Err(bad("a selector is key=value"));
            };
            if !SELECTORS.contains(&k) {
                return Err(bad(&format!("unknown selector {k}")));
            }
            if sel.contains_key(k) {
                return Err(conflicting(&format!("selector {k} given twice")));
            }
            match k {
                "branch" => names::check_name("branch-name", v)?,
                "commit" => {
                    validate_value(
                        "text",
                        Some(&json!({"pattern": "sha256:[0-9a-f]{64}"})),
                        &json!(v),
                    )?;
                }
                "tag" | "key" | "view" => {
                    if !names::matches("slug", v) {
                        return Err(bad(&format!("{k} is a slug")));
                    }
                }
                "time" => {
                    validate_value("instant", None, &json!(v))?;
                }
                _ => {
                    if validate_value("date", None, &json!(v)).is_err() {
                        validate_value("instant", None, &json!(v))?;
                    }
                }
            }
            sel.insert(k.to_string(), json!(v));
        }
    }
    if EXCLUSIVE.iter().filter(|k| sel.contains_key(**k)).count() > 1 {
        return Err(conflicting("at most one of commit, tag and time"));
    }
    let mut out = json!({"tenant": auth[0], "class": class, "slug": slug, "selectors": sel,
        "canonical": format!("ubos://{authority}/{class}/{slug}")});
    if auth.len() == 2 {
        out["vae"] = json!(auth[1]);
    }
    if let Some(v) = version {
        let n: u64 = v.parse().map_err(|_| bad("versions are numbered from 1"))?;
        if n < 1 || v != n.to_string() {
            return Err(bad("versions are numbered from 1"));
        }
        out["version"] = json!(n);
    }
    Ok(out)
}

// ---- did:ubos (STD-ADDR-010) ----

/// Base58btc encoding (leading zero bytes become `1`).
pub fn b58encode(b: &[u8]) -> String {
    let zeros = b.iter().take_while(|x| **x == 0).count();
    let mut digits: Vec<u8> = Vec::new();
    for &byte in &b[zeros..] {
        let mut carry = u32::from(byte);
        for d in digits.iter_mut() {
            carry += u32::from(*d) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut out = String::with_capacity(zeros + digits.len());
    out.extend(std::iter::repeat_n('1', zeros));
    out.extend(digits.iter().rev().map(|d| B58[*d as usize] as char));
    out
}

/// Base58btc decoding.
pub fn b58decode(s: &str) -> Result<Vec<u8>, BpaError> {
    let ones = s.bytes().take_while(|c| *c == b'1').count();
    let mut bytes: Vec<u8> = Vec::new();
    for c in s.bytes() {
        let Some(mut carry) = B58.iter().position(|x| *x == c).map(|p| p as u32) else {
            return Err(bad("not base58btc"));
        };
        for b in bytes.iter_mut() {
            carry += u32::from(*b) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut out = vec![0u8; ones];
    out.extend(bytes.iter().rev());
    Ok(out)
}

const B64URL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding.
pub fn b64u(b: &[u8]) -> String {
    let mut out = String::new();
    for chunk in b.chunks(3) {
        let n = chunk.len();
        let v = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64URL[(v >> 18) as usize & 63] as char);
        out.push(B64URL[(v >> 12) as usize & 63] as char);
        if n > 1 {
            out.push(B64URL[(v >> 6) as usize & 63] as char);
        }
        if n > 2 {
            out.push(B64URL[v as usize & 63] as char);
        }
    }
    out
}

/// Decode `n` bytes given in base64url without padding.
pub fn unb64u(v: &Value, n: usize, what: &str) -> Result<Vec<u8>, BpaError> {
    let err = || bad(&format!("{what} is {n} bytes in base64url without padding"));
    let Some(s) = v.as_str() else {
        return Err(err());
    };
    if s.len() != (4 * n).div_ceil(3) || !s.bytes().all(|c| B64URL.contains(&c)) {
        return Err(err());
    }
    let mut out = Vec::with_capacity(n);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in s.bytes() {
        let d = B64URL.iter().position(|x| *x == c).unwrap_or_default() as u32;
        acc = (acc << 6) | d;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    if out.len() != n {
        return Err(err());
    }
    Ok(out)
}

/// The key-form DID of an Ed25519 public key given in base64url.
pub fn key_did(public_key: &Value) -> Result<String, BpaError> {
    let k = unb64u(public_key, 32, "public_key")?;
    let mut raw = ED25519_PUB.to_vec();
    raw.extend(k);
    Ok(format!("did:ubos:k:z{}", b58encode(&raw)))
}

/// Parse a `did:ubos` identifier into its form and parts.
pub fn parse_did(did: &Value) -> Result<Value, BpaError> {
    let Some(rest) = did.as_str().and_then(|d| d.strip_prefix("did:ubos:")) else {
        return Err(bad("not a did:ubos identifier"));
    };
    if let Some(mb) = rest.strip_prefix("k:") {
        let Some(body) = mb.strip_prefix('z').filter(|b| !b.is_empty()) else {
            return Err(bad("the key form is multibase base58btc (z…)"));
        };
        let raw = b58decode(body)?;
        if raw.len() != 34 || raw[..2] != ED25519_PUB {
            return Err(bad("the key form holds an Ed25519 public key"));
        }
        return Ok(json!({"form": "key", "public_key": b64u(&raw[2..])}));
    }
    let Some((authority, uid)) = rest.rsplit_once(':') else {
        return Err(bad("the registered form is did:ubos:<tenant[.vae]>:<uuid>"));
    };
    let parts: Vec<&str> = authority.split('.').collect();
    if parts.len() > 2
        || !parts.iter().all(|p| names::matches("slug", p))
        || !names::matches("uuid", uid)
    {
        return Err(bad("the registered form is did:ubos:<tenant[.vae]>:<uuid>"));
    }
    let mut out = json!({"form": "registered", "tenant": parts[0], "uuid": uid});
    if parts.len() == 2 {
        out["vae"] = json!(parts[1]);
    }
    Ok(out)
}
