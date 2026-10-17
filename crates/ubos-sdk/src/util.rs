//! Identifiers and trace context: UUIDv7 idempotency keys (DSN-SDK-002) and W3C trace
//! context (DSN-SDK-016).

use crate::error::{Error, Result};

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    // the operating system's generator; on failure the bytes stay zero and the keys
    // still differ by their time part
    let _ = getrandom::getrandom(&mut b);
    b
}

/// A UUIDv7 (RFC 9562): 48 bits of Unix milliseconds, then random bits.
pub fn uuid7() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let mut b: [u8; 16] = random();
    b[..6].copy_from_slice(&ms.to_be_bytes()[2..]);
    b[6] = (b[6] & 0x0f) | 0x70;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// A W3C trace context `00-<trace>-<span>-<flags>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// 32 hexadecimal digits.
    pub trace_id: String,
    /// 16 hexadecimal digits.
    pub span_id: String,
    /// Sampled.
    pub sampled: bool,
}

impl TraceContext {
    /// A new trace (sampled).
    pub fn new() -> TraceContext {
        let t: [u8; 16] = random();
        let s: [u8; 8] = random();
        TraceContext {
            trace_id: t.iter().map(|x| format!("{x:02x}")).collect(),
            span_id: s.iter().map(|x| format!("{x:02x}")).collect(),
            sampled: true,
        }
    }

    /// Parse a `traceparent` header value.
    pub fn parse(tp: &str) -> Result<TraceContext> {
        let p: Vec<&str> = tp.split('-').collect();
        let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
        if p.len() != 4 || p[0] != "00" || !hex(p[1], 32) || !hex(p[2], 16) || !hex(p[3], 2) {
            return Err(Error::new(
                "MODEL.INVALID_VALUE",
                "a traceparent is 00-<32 hex>-<16 hex>-<2 hex>",
                serde_json::Value::Null,
            ));
        }
        Ok(TraceContext {
            trace_id: p[1].to_string(),
            span_id: p[2].to_string(),
            sampled: p[3] == "01",
        })
    }

    /// A child span of the same trace (the SDK's request inside the caller's span).
    pub fn child(&self) -> TraceContext {
        let s: [u8; 8] = random();
        TraceContext {
            trace_id: self.trace_id.clone(),
            span_id: s.iter().map(|x| format!("{x:02x}")).collect(),
            sampled: self.sampled,
        }
    }

    /// The `traceparent` value.
    pub fn traceparent(&self) -> String {
        format!(
            "00-{}-{}-{}",
            self.trace_id,
            self.span_id,
            if self.sampled { "01" } else { "00" }
        )
    }
}

impl Default for TraceContext {
    fn default() -> TraceContext {
        TraceContext::new()
    }
}

/// Uniformly random in `[0, n)` (back-off jitter).
pub fn jitter(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    u64::from_be_bytes(random::<8>()) % n
}

/// Standard base64 (tus metadata).
pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
