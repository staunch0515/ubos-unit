//! Canonical JSON, NFC and content identifiers (STD-FND-001, -002, -003, -010, -011).

use crate::error::{BpaError, invalid};
use serde_json::Value;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Largest integer allowed as a JSON number in canonical data.
pub const MAX_SAFE: i64 = (1 << 53) - 1;

/// Registered domain tags (STD-FND-011, STD-PROOF-001).
pub const DOMAINS: &[&str] = &[
    "ubos.chunk.v1",
    "ubos.version.v1",
    "ubos.entry.v1",
    "ubos.commit.v1",
    "ubos.tree.leaf.v1",
    "ubos.tree.node.v1",
    "ubos.tree.boundary.v1",
    "ubos.seal.v1",
    "ubos.pack.v1",
    "ubos.field-commit.v1",
];

/// Normalise text to NFC (STD-FND-003).
pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in nfc(text).chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn utf16_key(text: &str) -> Vec<u16> {
    nfc(text).encode_utf16().collect()
}

fn write(out: &mut String, value: &Value) -> Result<(), BpaError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let i = if let Some(i) = n.as_i64() {
                i
            } else if n.as_u64().is_some() {
                return Err(invalid("integer beyond 2^53-1 must be a decimal string"));
            } else {
                return Err(invalid("number with a fraction or exponent"));
            };
            if i.abs() > MAX_SAFE {
                return Err(invalid("integer beyond 2^53-1 must be a decimal string"));
            }
            out.push_str(&i.to_string());
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(out, v)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|k| utf16_key(k));
            let normed: std::collections::BTreeSet<String> = keys.iter().map(|k| nfc(k)).collect();
            if normed.len() != keys.len() {
                return Err(invalid("duplicate member after NFC"));
            }
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, k);
                out.push(':');
                write(out, &map[*k])?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// RFC 8785 serialisation of canonical data; every string is normalised to NFC first.
pub fn canonical(value: &Value) -> Result<String, BpaError> {
    let mut out = String::new();
    write(&mut out, value)?;
    Ok(out)
}

/// `sha256:<hex>` of `domain 0x00 data` (STD-FND-010, -011).
pub fn digest(domain: &str, data: &[u8]) -> Result<String, BpaError> {
    if !DOMAINS.contains(&domain) {
        return Err(invalid("unregistered domain tag"));
    }
    let mut h = Sha256::new();
    h.update(domain.as_bytes());
    h.update([0u8]);
    h.update(data);
    Ok(format!("sha256:{}", hex(&h.finalize())))
}

/// Content identifier of a canonical value under a domain tag.
pub fn content_id(domain: &str, value: &Value) -> Result<String, BpaError> {
    digest(domain, canonical(value)?.as_bytes())
}

/// Lowercase hexadecimal encoding.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
