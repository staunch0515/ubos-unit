//! `ubos-proto` — the UBTP message schemas and codecs (UBS-ARC-07 §2; STD-PROTO-001,
//! -002, -010…-013; PH-1 step 14a, DEC-075).
//!
//! The crate holds what every binding shares: the envelope (`DAT-UbtpEnvelope`) and its
//! structural checks, the message and frame types, the handshake negotiation of the
//! in-process credentials, the frame builders and the subscription cursor of the
//! reference binding. The semantics of the messages (what a `mutate` or a `query` does)
//! live in the kernel's in-process binding (`ubos-dvm`); the Server (`ubos-node`) decodes
//! transport frames into these messages and hands them to the same binding, so that the
//! semantics are binding-independent (FR-INT-015).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use ubos_types::BpaError;

/// The protocol version of BPA 1.0 (STD-PROTO-001).
pub const VERSION: &str = "1.0";
/// The ABI version the binding serves (STD-PROTO-002).
pub const ABI: &str = "1.0";
/// The message types of `DAT-UbtpEnvelope`.
pub const TYPES: &[&str] = &[
    "handshake",
    "mutate",
    "query",
    "subscribe",
    "unsubscribe",
    "emit",
    "sync",
    "ack",
    "error",
    "result",
    "ping",
    "pong",
    "cancel",
];
/// The types a client sends; the others are server frames.
pub const REQUESTS: &[&str] = &[
    "handshake",
    "mutate",
    "query",
    "subscribe",
    "unsubscribe",
    "ack",
    "ping",
    "cancel",
    "sync",
];
/// The members of an envelope.
pub const MEMBERS: &[&str] = &["v", "id", "reply_to", "type", "body", "trace"];

/// The reference limits of a session (STD-PROTO-002): the largest frame, the window of
/// unacknowledged emits per subscription, the retention of subscription cursors in
/// commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// Largest frame in bytes.
    pub frame_bytes: u64,
    /// Unacknowledged `emit` frames per subscription.
    pub window: u64,
    /// Commits a subscription cursor stays valid for.
    pub retention: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            frame_bytes: 16 * 1024 * 1024,
            window: 8,
            retention: 100,
        }
    }
}

fn protocol(m: impl Into<String>) -> BpaError {
    let m = m.into();
    let mut e = BpaError::with_details("INT.PROTOCOL_ERROR", json!(m));
    e.message = m;
    e
}

fn unsupported(details: Value) -> BpaError {
    BpaError::with_details("INT.VERSION_UNSUPPORTED", details)
}

/// A decoded request envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Client-unique message id.
    pub id: String,
    /// Message type (one of `REQUESTS`).
    pub type_: String,
    /// Type-specific body.
    pub body: Map<String, Value>,
    /// W3C traceparent, carried into the reply.
    pub trace: Option<String>,
}

/// The structural checks of a client message (STD-PROTO-001): a map with the envelope
/// members, the protocol version, a text id, a known client type, no `reply_to`, a map
/// body and a text trace. The session checks (the handshake first, duplicate ids) are the
/// binding's.
pub fn request(m: &Value) -> Result<Request, BpaError> {
    let Some(o) = m.as_object() else {
        return Err(protocol("a message is a DAT-UbtpEnvelope"));
    };
    for k in ["v", "id", "type", "body"] {
        if !o.contains_key(k) {
            return Err(protocol(format!("envelope member {k}")));
        }
    }
    for k in o.keys() {
        if !MEMBERS.contains(&k.as_str()) {
            return Err(protocol(format!("unknown envelope member {k}")));
        }
    }
    if o["v"] != json!(VERSION) {
        return Err(unsupported(json!({"supported": {"versions": [VERSION]}})));
    }
    let id = match o["id"].as_str() {
        Some(i) if !i.is_empty() => i.to_string(),
        _ => return Err(protocol("id is a client-unique text")),
    };
    let t = o["type"].as_str().unwrap_or_default();
    if !TYPES.contains(&t) {
        return Err(protocol(format!("unknown message type {t}")));
    }
    if !REQUESTS.contains(&t) {
        return Err(protocol(format!("{t} is a server message")));
    }
    if o.contains_key("reply_to") {
        return Err(protocol("a request carries no reply_to"));
    }
    let Some(body) = o["body"].as_object() else {
        return Err(protocol("body is a map"));
    };
    let trace = match o.get("trace") {
        None => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(_) => return Err(protocol("trace is text")),
    };
    Ok(Request {
        id,
        type_: t.to_string(),
        body: body.clone(),
        trace,
    })
}

/// The id of a message as far as it can be read (for the error frame of a malformed one).
pub fn id_of(m: &Value) -> Option<String> {
    m.get("id").and_then(Value::as_str).map(str::to_string)
}

/// A server frame (`result`, `error`, `emit`, `pong`).
pub fn frame(
    id: &str,
    type_: &str,
    body: Value,
    reply_to: Option<&str>,
    trace: Option<&str>,
) -> Value {
    let mut f = json!({"v": VERSION, "id": id, "type": type_, "body": body});
    if let Some(r) = reply_to {
        f["reply_to"] = json!(r);
    }
    if let Some(t) = trace {
        f["trace"] = json!(t);
    }
    f
}

/// The `error` frame of an error (STD-ERR-001 object: `code`, `message`, `details`).
pub fn error_frame(id: &str, e: &BpaError, reply_to: Option<&str>, trace: Option<&str>) -> Value {
    let mut body = json!({"code": e.code, "message": e.code});
    match &e.details {
        Some(d) => body["details"] = d.clone(),
        None if !e.message.is_empty() => body["details"] = json!(e.message),
        None => {}
    }
    frame(id, "error", body, reply_to, trace)
}

/// The credentials of the in-process binding (STD-PROTO-020): a trusted caller's
/// principal, kind, role and grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InProcess {
    /// Principal DID; the binding's default principal when absent.
    pub principal: Option<String>,
    /// Principal kind (`human` by default).
    pub kind: String,
    /// Role, if any.
    pub role: Option<String>,
    /// `None` leaves the caller unrestricted; a list restricts it to those permissions.
    pub grants: Option<Vec<String>>,
    /// Authentication level the host established (`password`, `mfa`, `api_key`; the
    /// `auth_level` of the context policies see), if any.
    pub auth_level: Option<String>,
}

/// Negotiate a handshake body (STD-PROTO-002): the protocol version, the ABI range and the
/// encodings must include what the binding serves; the credentials must be in-process
/// ones. Returns the credentials; the result body is `handshake_result`.
pub fn negotiate(body: &Map<String, Value>) -> Result<InProcess, BpaError> {
    let supported = json!({"supported": {"versions": [VERSION], "abi": {"min": ABI, "max": ABI}}});
    let ok = body
        .get("versions")
        .and_then(Value::as_array)
        .is_some_and(|v| v.iter().any(|x| x == VERSION));
    if !ok {
        return Err(unsupported(supported));
    }
    if let Some(abi) = body.get("abi").filter(|v| !v.is_null()) {
        let Some(a) = abi.as_object() else {
            return Err(unsupported(supported));
        };
        let txt = |k: &str| {
            a.get(k)
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| ABI.to_string())
        };
        if !(txt("min").as_str() <= ABI && ABI <= txt("max").as_str()) {
            return Err(unsupported(supported));
        }
    }
    if let Some(enc) = body.get("encodings").filter(|v| !v.is_null())
        && !enc
            .as_array()
            .is_some_and(|e| e.iter().any(|x| x == "json"))
    {
        return Err(unsupported(json!({"supported": {"encodings": ["json"]}})));
    }
    let cred = body.get("credentials").filter(|v| !v.is_null());
    let cred = match cred {
        None => Map::new(),
        Some(Value::Object(c)) if c.get("kind") == Some(&json!("in_process")) => c.clone(),
        Some(_) => {
            let mut e = BpaError::with_details(
                "IAM.DENIED",
                json!("the in-process binding takes in_process credentials"),
            );
            e.message = "the in-process binding takes in_process credentials".into();
            return Err(e);
        }
    };
    let text = |k: &str| cred.get(k).and_then(Value::as_str).map(str::to_string);
    Ok(InProcess {
        principal: text("principal"),
        kind: text("principal_kind").unwrap_or_else(|| "human".into()),
        role: text("role"),
        auth_level: text("auth_level"),
        grants: cred.get("grants").filter(|v| !v.is_null()).map(|g| {
            g.as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        }),
    })
}

/// The result body of a handshake.
pub fn handshake_result(
    session: &str,
    principal: &str,
    kind: &str,
    authority: &str,
    limits: &Limits,
) -> Value {
    json!({"version": VERSION, "abi": ABI, "session_id": session, "encoding": "json",
        "principal": {"did": principal, "kind": kind, "authority": authority},
        "limits": limits, "features": []})
}

/// The subscription cursor of the reference binding: `<branch>:<cseq>` (STD-PROTO-012).
pub fn cursor(branch: &str, cseq: u64) -> String {
    format!("{branch}:{cseq}")
}

/// Parse a cursor; `MODEL.INVALID_VALUE` when it is not one.
pub fn parse_cursor(c: &Value) -> Result<(String, u64), BpaError> {
    let bad = || {
        let mut e =
            BpaError::with_details("MODEL.INVALID_VALUE", json!("a cursor is <branch>:<cseq>"));
        e.message = "a cursor is <branch>:<cseq>".into();
        e
    };
    let Some(t) = c.as_str() else {
        return Err(bad());
    };
    let Some((b, n)) = t.rsplit_once(':') else {
        return Err(bad());
    };
    if n.is_empty() || !n.bytes().all(|x| x.is_ascii_digit()) {
        return Err(bad());
    }
    Ok((b.to_string(), n.parse().map_err(|_| bad())?))
}
