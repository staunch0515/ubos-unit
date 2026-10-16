//! `ubos-proto-suite` — SUITE-NOD-PROTO (VER-CONF-7000; PH-1 step 14d, DEC-075): the
//! released Protocol vectors of BPA 1.0 replayed over the three bindings of the Server —
//! in-process, HTTP (`POST /ubtp/v1`) and WebSocket (`GET /ubtp/v1`, `ubtp.v1`) — with
//! identical results required across bindings; the version-skew cases of a client that
//! offers another protocol version, ABI range, encoding or envelope version, or a newer
//! client's unknown members; and the equivalence of the REST facade with the `query` and
//! `mutate` messages (STD-PROTO-021).
//!
//! ```text
//! ubos-proto-suite [--vectors standard/vectors/protocol] [--report <file>] [--verbose]
//! ```
//!
//! Every vector runs on a fresh engine state (`reset`) over a memory VAE with a fixed clock
//! (the vectors' `set_now`) and seeded entropy; the steps that are not `ubtp` messages
//! (`commit`, `read`, `query`, …) run in-process on the same VAE, the `ubtp` steps go over
//! the binding under test. Over a network binding the Server names the session, so its
//! identifier is mapped back to the vector's session key before the frames are compared;
//! over WebSocket each vector session is one connection and a step reads as many frames as
//! the in-process binding answered. Everything else must match exactly.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ubos_dvm::host::{FixedClock, NoConnectors, NoModels, Recorder, SeededEntropy, Services, Vae};
use ubos_dvm::types::{BpaError, canonical};
use ubos_dvm::vectors::{canonical_eq, pointer, resolve, strip, subset};
use ubos_dvm::{STATEFUL_OPERATIONS, crypto};
use ubos_node::server::{Preset, Server, VaeHandle};

/// The bindings under test (DSN-NOD-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binding {
    InProcess,
    Http,
    WebSocket,
}

impl Binding {
    fn name(self) -> &'static str {
        match self {
            Binding::InProcess => "in_process",
            Binding::Http => "http",
            Binding::WebSocket => "websocket",
        }
    }
}

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

/// A suite VAE: the memory store, a fixed clock and seeded entropy, nothing recorded.
fn suite_vae(clock: Arc<FixedClock>) -> Result<Vae, BpaError> {
    let engine = ubos_dvm::Engine::new(Box::new(ubos_dvm::store::MemStore::default()))?;
    let rec = Arc::new(Recorder::default());
    Ok(Vae::open(
        engine,
        Services {
            clock,
            entropy: Arc::new(SeededEntropy::new([7u8; 32])),
            connectors: Arc::new(NoConnectors),
            models: Arc::new(NoModels),
            outbox: rec.clone(),
            jobs: rec.clone(),
            telemetry: rec,
        },
    ))
}

// ---- transports ----

/// One HTTP/1.1 request to the Server; returns the status, the `Ubtp-Session` header and
/// the body.
fn http(
    addr: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(u16, Option<String>, Vec<u8>), BpaError> {
    let mut s = TcpStream::connect(addr).map_err(err)?;
    s.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(err)?;
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).map_err(err)?;
    s.write_all(body).map_err(err)?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(err)?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| err("malformed HTTP response"))?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| err("malformed status line"))?;
    let mut session = None;
    let mut length: Option<usize> = None;
    let mut chunked = false;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if k == "ubtp-session" {
            session = Some(v.to_string());
        } else if k == "content-length" {
            length = v.parse().ok();
        } else if k == "transfer-encoding" && v.contains("chunked") {
            chunked = true;
        }
    }
    let mut body = raw[split + 4..].to_vec();
    if chunked {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let Some(n) = body[i..].windows(2).position(|w| w == b"\r\n") else {
                break;
            };
            let size = usize::from_str_radix(String::from_utf8_lossy(&body[i..i + n]).trim(), 16)
                .unwrap_or(0);
            i += n + 2;
            if size == 0 {
                break;
            }
            out.extend_from_slice(&body[i..i + size]);
            i += size + 2;
        }
        body = out;
    } else if let Some(n) = length {
        body.truncate(n);
    }
    Ok((status, session, body))
}

/// A WebSocket connection of one vector session.
struct WsConn {
    rt: Arc<tokio::runtime::Runtime>,
    stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl WsConn {
    fn open(rt: Arc<tokio::runtime::Runtime>, addr: &str) -> Result<WsConn, BpaError> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut req = format!("ws://{addr}/ubtp/v1")
            .into_client_request()
            .map_err(err)?;
        req.headers_mut()
            .insert("Sec-WebSocket-Protocol", "ubtp.v1".parse().map_err(err)?);
        let (stream, resp) = rt
            .block_on(tokio_tungstenite::connect_async(req))
            .map_err(err)?;
        let proto = resp
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if proto != "ubtp.v1" {
            return Err(err(format!("subprotocol {proto:?} negotiated")));
        }
        Ok(WsConn { rt, stream })
    }

    fn send(&mut self, m: &Value) -> Result<(), BpaError> {
        use tokio_tungstenite::tungstenite::Message;
        self.rt
            .block_on(self.stream.send(Message::Text(m.to_string().into())))
            .map_err(err)
    }

    /// Read `n` frames (10 s for each).
    fn read(&mut self, n: usize) -> Result<Vec<Value>, BpaError> {
        use tokio_tungstenite::tungstenite::Message;
        let mut out = Vec::new();
        while out.len() < n {
            let next = self.rt.block_on(async {
                tokio::time::timeout(Duration::from_secs(10), self.stream.next()).await
            });
            match next {
                Ok(Some(Ok(Message::Text(t)))) => {
                    out.push(serde_json::from_str(&t).map_err(err)?);
                }
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
                    return Err(err(format!(
                        "the connection closed after {} of {n} frames",
                        out.len()
                    )));
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(e))) => return Err(err(e)),
                Err(_) => {
                    return Err(err(format!("timeout after {} of {n} frames", out.len())));
                }
            }
        }
        Ok(out)
    }
}

/// The binding driver of one vector: the in-process handle, or the Server with the
/// vector's session keys mapped to the Server's sessions.
struct Driver {
    binding: Binding,
    handle: VaeHandle,
    addr: String,
    rt: Arc<tokio::runtime::Runtime>,
    /// vector session key → Server session id (HTTP).
    http_sessions: HashMap<String, String>,
    /// vector session key → connection (WebSocket).
    ws: HashMap<String, WsConn>,
}

impl Driver {
    fn reset(&mut self) -> Result<(), BpaError> {
        self.http_sessions.clear();
        self.ws.clear();
        self.handle.call_blocking("reset", json!({}))?;
        Ok(())
    }

    /// Map the Server's session identifier back to the vector's key in a frame.
    fn normalise(frames: Vec<Value>, server: &str, key: &str) -> Vec<Value> {
        if server.is_empty() || server == key {
            return frames;
        }
        frames
            .into_iter()
            .map(|f| {
                let text = f
                    .to_string()
                    .replace(&format!("\"{server}\""), &format!("\"{key}\""));
                serde_json::from_str(&text).unwrap_or(f)
            })
            .collect()
    }

    /// The credentials of a vector's handshake as a network client presents them: the
    /// in-process credentials (or none: the binding's default principal) become the `dev`
    /// kind with the same members, which the `dev` preset turns back into exactly those
    /// in-process credentials; anything else is sent as it is.
    fn network_credentials(m: &Value) -> Value {
        let mut m = m.clone();
        if m["type"] != "handshake" || !m["body"].is_object() {
            return m;
        }
        let c = &m["body"]["credentials"];
        let mapped = match c {
            Value::Null => Some(json!({"kind": "dev"})),
            Value::Object(o) if o.get("kind") == Some(&json!("in_process")) => {
                let mut o = o.clone();
                o.insert("kind".into(), json!("dev"));
                Some(Value::Object(o))
            }
            _ => None,
        };
        if let Some(c) = mapped {
            m["body"]["credentials"] = c;
        }
        m
    }

    /// Run one `ubtp` step: `expect_frames` is what the in-process binding answered
    /// (WebSocket reads that many frames).
    fn ubtp(&mut self, input: &Value, expect_frames: Option<usize>) -> Result<Value, BpaError> {
        let key = input["session"].as_str().unwrap_or("s1").to_string();
        let mut messages = input["messages"].as_array().cloned().unwrap_or_default();
        if self.binding != Binding::InProcess {
            messages = messages.iter().map(Self::network_credentials).collect();
        }
        match self.binding {
            Binding::InProcess => self.handle.call_blocking("ubtp", input.clone()),
            Binding::Http => {
                let server = self.http_sessions.get(&key).cloned();
                let mut headers: Vec<(&str, &str)> = Vec::new();
                let sid;
                if let Some(s) = &server {
                    sid = s.clone();
                    headers.push(("Ubtp-Session", sid.as_str()));
                }
                let (status, session, body) = http(
                    &self.addr,
                    "POST",
                    "/ubtp/v1",
                    &headers,
                    Value::Array(messages).to_string().as_bytes(),
                )?;
                if status != 200 {
                    return Err(err(format!(
                        "HTTP {status}: {}",
                        String::from_utf8_lossy(&body)
                    )));
                }
                let v: Value = serde_json::from_slice(&body).map_err(err)?;
                let server = match (server, session) {
                    (Some(s), _) => s,
                    (None, Some(s)) => {
                        self.http_sessions.insert(key.clone(), s.clone());
                        s
                    }
                    (None, None) => String::new(),
                };
                let frames = v["responses"].as_array().cloned().unwrap_or_default();
                Ok(json!({"responses": Self::normalise(frames, &server, &key)}))
            }
            Binding::WebSocket => {
                if !self.ws.contains_key(&key) {
                    let c = WsConn::open(self.rt.clone(), &self.addr)?;
                    self.ws.insert(key.clone(), c);
                }
                let conn = self.ws.get_mut(&key).expect("connection");
                for m in &messages {
                    conn.send(m)?;
                }
                let n = expect_frames.unwrap_or(messages.len());
                let frames = conn.read(n)?;
                // the Server's session id is the handshake result's `session_id`
                let server = frames
                    .iter()
                    .find(|f| f["type"] == "result" && f["body"].get("session_id").is_some())
                    .and_then(|f| f["body"]["session_id"].as_str())
                    .unwrap_or_default()
                    .to_string();
                Ok(json!({"responses": Self::normalise(frames, &server, &key)}))
            }
        }
    }
}

// ---- the vector loop ----

/// Vectors whose subject is the in-process binding itself, with what a network binding
/// must answer instead (the Server, not the binding, verifies network credentials).
const BINDING_SPECIFIC: &[(&str, &str)] = &[(
    "VER-CONF-5427",
    "a bearer token is verified by the Server: an invalid one is refused with IAM.DENIED before the binding",
)];

/// The network check of a binding-specific vector: every `ubtp` step's first frame is an
/// `IAM.DENIED` error answering the handshake.
fn network_check(d: &mut Driver, vector: &Value) -> Result<(), String> {
    for (n, step) in vector["steps"].as_array().into_iter().flatten().enumerate() {
        if step["op"] != "ubtp" {
            continue;
        }
        let r = d.ubtp(&step["input"], Some(1)).map_err(|e| e.to_string())?;
        let f = &r["responses"][0];
        let want = json!({"type": "error", "reply_to": "h", "body": {"code": "IAM.DENIED"}});
        if !subset(&want, f) {
            return Err(format!("s{}: got {f}", n + 1));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct VectorRun {
    id: String,
    outcome: &'static str,
    detail: String,
    /// The responses of each `ubtp` step, by step label.
    ubtp: BTreeMap<String, Value>,
}

fn label_of(step: &Value, phase: &str, n: usize) -> String {
    step["label"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}{}", &phase[..1], n + 1))
}

/// Run one vector over a driver; `reference` is the in-process run (frame counts).
fn run_vector(
    d: &mut Driver,
    clock: &FixedClock,
    vector: &Value,
    reference: Option<&VectorRun>,
) -> VectorRun {
    let id = vector["id"].as_str().unwrap_or("?").to_string();
    let mut run = VectorRun {
        id: id.clone(),
        outcome: "pass",
        detail: String::new(),
        ubtp: BTreeMap::new(),
    };
    let fail = |run: &mut VectorRun, d: String| {
        run.outcome = "fail";
        run.detail = d;
    };
    if let Err(e) = d.reset() {
        fail(&mut run, format!("reset: {e}"));
        return run;
    }
    let mut results: HashMap<String, Value> = HashMap::new();
    for phase in ["setup", "steps"] {
        let steps = vector[phase].as_array().cloned().unwrap_or_default();
        for (n, step) in steps.iter().enumerate() {
            let label = label_of(step, phase, n);
            let op = step["op"].as_str().unwrap_or_default();
            let input = match resolve(step.get("input").unwrap_or(&json!({})), &results) {
                Ok(i) => i,
                Err(e) => {
                    fail(&mut run, format!("{label}: {e}"));
                    return run;
                }
            };
            if op == "set_now"
                && let Some(t) = input["now"].as_str()
            {
                clock.set(t);
            }
            let out = if op == "ubtp" {
                let expect_frames = reference
                    .and_then(|r| r.ubtp.get(&label))
                    .and_then(|v| v["responses"].as_array())
                    .map(Vec::len);
                let r = d.ubtp(&input, expect_frames);
                if let Ok(v) = &r {
                    run.ubtp.insert(label.clone(), v.clone());
                }
                r
            } else {
                d.handle.call_blocking(op, input)
            };
            if let Ok(v) = &out {
                results.insert(label.clone(), v.clone());
            }
            let Some(exp) = step.get("expect") else {
                if let Err(e) = out {
                    fail(&mut run, format!("{label}: unexpected error {e}"));
                    return run;
                }
                continue;
            };
            let stateful = STATEFUL_OPERATIONS.contains(&op);
            let problem = if let Some(code) = exp.get("error_code").and_then(Value::as_str) {
                match &out {
                    Err(e) if e.code == code => match exp.get("details") {
                        Some(dd) if !e.details.as_ref().is_some_and(|x| subset(dd, x)) => {
                            Some(format!("error details differ: {:?}", e.details))
                        }
                        _ => None,
                    },
                    Err(e) => Some(format!("expected {code}, got {e}")),
                    Ok(v) => Some(format!("expected {code}, got result {v}")),
                }
            } else {
                match &out {
                    Err(e) => Some(format!("unexpected error {e}")),
                    Ok(v) => {
                        if let Some(r) = exp.get("same_as").and_then(Value::as_str) {
                            match pointer(&results, r) {
                                Some(x) if x == v => None,
                                _ => Some(format!("not the same as {r}")),
                            }
                        } else if exp["match"] == "subset" {
                            (!subset(&exp["result"], v))
                                .then(|| format!("subset mismatch: got {v}"))
                        } else {
                            let actual = if stateful { strip(v) } else { v.clone() };
                            (!canonical_eq(&exp["result"], &actual))
                                .then(|| format!("mismatch: got {actual}"))
                        }
                    }
                }
            };
            if let Some(p) = problem {
                fail(&mut run, format!("{label}: {p}"));
                return run;
            }
        }
    }
    run
}

// ---- version skew (VER-CONF-7000 procedure 2) ----

/// The handshake of a client of the released version, with one member changed.
fn skew_cases() -> Vec<(String, Value, Value)> {
    let hs = |body: Value| json!({"v": "1.0", "id": "h", "type": "handshake", "body": body});
    let full = json!({"vae": "acme", "versions": ["1.0"], "abi": {"min": "1.0", "max": "1.0"}, "encodings": ["json"],
        "features": [], "credentials": {"kind": "in_process"}});
    let with = |k: &str, v: Value| {
        let mut b = full.clone();
        b[k] = v;
        hs(b)
    };
    let unsupported = |supported: Value| {
        json!({"type": "error", "reply_to": "h",
        "body": {"code": "INT.VERSION_UNSUPPORTED", "details": {"supported": supported}}})
    };
    let result = json!({"type": "result", "reply_to": "h", "body": {"version": "1.0", "abi": "1.0", "encoding": "json"}});
    let mut older_envelope = with("versions", json!(["0.9"]));
    older_envelope["v"] = json!("0.9");
    let mut unknown_type = hs(full.clone());
    unknown_type["type"] = json!("frobnicate");
    vec![
        (
            "client N-1: protocol version 0.9 only".into(),
            with("versions", json!(["0.9"])),
            unsupported(json!({"versions": ["1.0"], "abi": {"min": "1.0", "max": "1.0"}})),
        ),
        (
            "client offering 0.9 and 1.0".into(),
            with("versions", json!(["0.9", "1.0"])),
            result.clone(),
        ),
        (
            "client N-1: ABI 0.9 only".into(),
            with("abi", json!({"min": "0.9", "max": "0.9"})),
            unsupported(json!({"versions": ["1.0"], "abi": {"min": "1.0", "max": "1.0"}})),
        ),
        (
            "client N+1: ABI range 0.9 to 2.0".into(),
            with("abi", json!({"min": "0.9", "max": "2.0"})),
            result.clone(),
        ),
        (
            "client with an encoding the server lacks".into(),
            with("encodings", json!(["cbor"])),
            unsupported(json!({"encodings": ["json"]})),
        ),
        (
            "client N-1: envelope version 0.9".into(),
            older_envelope,
            json!({"type": "error", "reply_to": "h", "body": {"code": "INT.VERSION_UNSUPPORTED"}}),
        ),
        (
            "client N+1: unknown message type".into(),
            unknown_type,
            json!({"type": "error", "reply_to": "h", "body": {"code": "INT.PROTOCOL_ERROR"}}),
        ),
        (
            "client N+1: unknown handshake members are ignored".into(),
            with("x_future", json!({"compression": "zstd"})),
            result,
        ),
    ]
}

fn run_skew(d: &mut Driver) -> Vec<Value> {
    let mut out = Vec::new();
    for (name, message, expect) in skew_cases() {
        let r = d
            .reset()
            .and_then(|_| d.ubtp(&json!({"session": "s1", "messages": [message]}), Some(1)));
        let (outcome, detail, frames) = match r {
            Ok(v) => {
                let frames = v["responses"].clone();
                let first = frames.get(0).cloned().unwrap_or(Value::Null);
                if subset(&expect, &first) {
                    ("pass", String::new(), frames)
                } else {
                    ("fail", format!("got {first}"), frames)
                }
            }
            Err(e) => ("fail", e.to_string(), Value::Null),
        };
        out.push(json!({"case": name, "outcome": outcome, "detail": detail, "frames": frames}));
    }
    out
}

// ---- REST equivalence (STD-PROTO-021) ----

/// The facade's answers against the `query` and `mutate` messages over the same VAE.
fn run_rest(d: &mut Driver, genesis: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut case = |name: &str, r: Result<bool, BpaError>, detail: String| {
        let (o, dt) = match r {
            Ok(true) => ("pass", String::new()),
            Ok(false) => ("fail", detail),
            Err(e) => ("fail", e.to_string()),
        };
        out.push(json!({"case": name, "outcome": o, "detail": dt}));
    };
    let setup = (|| -> Result<(), BpaError> {
        d.reset()?;
        d.handle
            .call_blocking("load_genesis", json!({"classes": genesis}))?;
        let hs = json!({"v": "1.0", "id": "h", "type": "handshake", "body": {"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "in_process"}}});
        let commit = json!({"v": "1.0", "id": "c", "type": "mutate", "body": {"vae": "acme", "branch": "main", "commit": {"changes": [
            {"op": "create", "class": "Customer", "object_id": "01890a5d-ac96-7001-8000-000000000001", "slug": "acme", "body": {"name": "Acme", "country": "US", "rating": 3}},
            {"op": "create", "class": "Customer", "object_id": "01890a5d-ac96-7001-8000-000000000002", "slug": "beta", "body": {"name": "Beta", "country": "DE", "rating": 4}}]}}});
        let r = d.ubtp(
            &json!({"session": "rest", "messages": [hs, commit]}),
            Some(2),
        )?;
        if r["responses"][1]["type"] != "result" {
            return Err(err(format!("setup commit: {}", r["responses"][1])));
        }
        Ok(())
    })();
    if let Err(e) = setup {
        case("setup", Err(e), String::new());
        return out;
    }
    let addr = d.addr.clone();
    let get = |path: &str| -> Result<(u16, Value), BpaError> {
        let (st, _, body) = http(&addr, "GET", path, &[], b"")?;
        Ok((st, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    };
    let via_ubtp =
        |d: &mut Driver, id: &str, type_: &str, body: Value| -> Result<Value, BpaError> {
            let m = json!({"v": "1.0", "id": id, "type": type_, "body": body});
            let r = d.ubtp(&json!({"session": "rest", "messages": [m]}), Some(1))?;
            Ok(r["responses"][0].clone())
        };
    // read
    let r = (|| {
        let (st, facade) = get("/api/v1/acme/Customer/acme")?;
        let f = via_ubtp(
            d,
            "r1",
            "query",
            json!({"vae": "acme", "read": "ubos://acme/Customer/acme"}),
        )?;
        Ok((
            st == 200 && facade == f["body"],
            format!("facade {facade} vs ubtp {f}"),
        ))
    })();
    match r {
        Ok((ok, dt)) => case(
            "read: GET /api/v1/{vae}/{Class}/{slug} equals the query message's read",
            Ok(ok),
            dt,
        ),
        Err(e) => case("read", Err(e), String::new()),
    }
    // query
    let r = (|| {
        let (st, facade) = get("/api/v1/acme/Customer?q=this.rating%20%3E%3D%204")?;
        let f = via_ubtp(
            d,
            "q1",
            "query",
            json!({"vae": "acme", "query": "from Customer where this.rating >= 4"}),
        )?;
        Ok((
            st == 200 && facade == f["body"],
            format!("facade {facade} vs ubtp {f}"),
        ))
    })();
    match r {
        Ok((ok, dt)) => case(
            "query: GET /api/v1/{vae}/{Class}?q= equals the query message's textual query",
            Ok(ok),
            dt,
        ),
        Err(e) => case("query", Err(e), String::new()),
    }
    // not found: RFC 9457 problem with the code's status (STD-ERR-002)
    let r = (|| {
        let (st, facade) = get("/api/v1/acme/Customer/nobody")?;
        let f = via_ubtp(
            d,
            "r2",
            "query",
            json!({"vae": "acme", "read": "ubos://acme/Customer/nobody"}),
        )?;
        let same = facade["code"] == f["body"]["code"] && facade["status"] == json!(404);
        Ok((
            st == 404 && same && facade["type"] == json!("urn:ubos:error:IAM.NOT_FOUND"),
            format!("facade {facade} vs ubtp {f}"),
        ))
    })();
    match r {
        Ok((ok, dt)) => case(
            "error: a problem document carries the message's error code and the status of STD-ERR-002",
            Ok(ok),
            dt,
        ),
        Err(e) => case("error", Err(e), String::new()),
    }
    // mutate through a port: the same error as the message (no port implemented here)
    let r = (|| {
        let (st, _, body) = http(
            &d.addr,
            "POST",
            "/api/v1/acme/Customer/acme/touch",
            &[("Idempotency-Key", "k1")],
            b"{}",
        )?;
        let facade: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let f = via_ubtp(
            d,
            "m1",
            "mutate",
            json!({"vae": "acme", "branch": "main", "idempotency_key": "k1", "port": {"target": "ubos://acme/Customer/acme", "name": "touch", "params": {}}}),
        )?;
        Ok((
            st == 501 && facade["code"] == f["body"]["code"] && f["type"] == "error",
            format!("{st} facade {facade} vs ubtp {f}"),
        ))
    })();
    match r {
        Ok((ok, dt)) => case(
            "mutate: POST /api/v1/{vae}/{Class}/{slug}/{port} equals the mutate message's port call",
            Ok(ok),
            dt,
        ),
        Err(e) => case("mutate", Err(e), String::new()),
    }
    out
}

// ---- the suite ----

struct Suite {
    runs: BTreeMap<&'static str, Vec<VectorRun>>,
    skew: BTreeMap<&'static str, Vec<Value>>,
    rest: Vec<Value>,
    identical: Vec<Value>,
}

fn load_vectors(dir: &std::path::Path) -> Result<Vec<Value>, BpaError> {
    let superseded: Vec<String> = std::fs::read_to_string(dir.join("../superseded.txt"))
        .map(|t| t.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(err)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for p in paths {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&p).map_err(err)?).map_err(err)?;
        if v["profile"] == "Protocol"
            && !superseded
                .iter()
                .any(|s| Some(s.as_str()) == v["id"].as_str())
        {
            out.push(v);
        }
    }
    Ok(out)
}

fn run(dir: &std::path::Path, verbose: bool) -> Result<Suite, BpaError> {
    let vectors = load_vectors(dir)?;
    if vectors.is_empty() {
        return Err(err(format!("no Protocol vectors in {}", dir.display())));
    }
    let genesis = vectors
        .iter()
        .flat_map(|v| v["setup"].as_array().cloned().unwrap_or_default())
        .find(|s| s["op"] == "load_genesis")
        .map(|s| s["input"]["classes"].clone())
        .unwrap_or(json!([]));
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(err)?,
    );
    let mut suite = Suite {
        runs: BTreeMap::new(),
        skew: BTreeMap::new(),
        rest: Vec::new(),
        identical: Vec::new(),
    };
    for binding in [Binding::InProcess, Binding::Http, Binding::WebSocket] {
        let clock = Arc::new(FixedClock::new("2026-01-01T00:00:00.000000Z"));
        let c2 = clock.clone();
        let handle = VaeHandle::spawn(move || suite_vae(c2))?;
        let identity = Arc::new(ubos_node::identity::Identity::memory("acme", "acme")?);
        let server = Server::single(handle.clone(), identity, Preset::Dev, "acme", "acme");
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .map_err(err)?;
        let addr = listener.local_addr().map_err(err)?.to_string();
        rt.spawn(server.serve_with(listener));
        let mut d = Driver {
            binding,
            handle,
            addr,
            rt: rt.clone(),
            http_sessions: HashMap::new(),
            ws: HashMap::new(),
        };
        let reference = suite.runs.get("in_process").cloned();
        let mut runs = Vec::new();
        for v in &vectors {
            let refr = reference
                .as_ref()
                .and_then(|r| r.iter().find(|x| x.id == v["id"]));
            let specific = BINDING_SPECIFIC.iter().find(|(id, _)| v["id"] == *id);
            let r = match specific {
                Some((id, why)) if binding != Binding::InProcess => {
                    let check = d
                        .reset()
                        .map_err(|e| e.to_string())
                        .and_then(|_| network_check(&mut d, v));
                    VectorRun {
                        id: (*id).to_string(),
                        outcome: if check.is_ok() { "pass" } else { "fail" },
                        detail: match check {
                            Ok(()) => format!("binding-specific: {why}"),
                            Err(e) => format!("binding-specific: {why}; {e}"),
                        },
                        ubtp: BTreeMap::new(),
                    }
                }
                _ => run_vector(&mut d, &clock, v, refr),
            };
            if verbose && r.outcome != "pass" {
                eprintln!("FAIL [{}] {} {}", binding.name(), r.id, r.detail);
            }
            runs.push(r);
        }
        suite.runs.insert(binding.name(), runs);
        suite.skew.insert(binding.name(), run_skew(&mut d));
        if binding == Binding::Http {
            suite.rest = run_rest(&mut d, &genesis);
        }
    }
    // identical results across bindings: every ubtp step's frames equal the in-process ones
    let reference = &suite.runs["in_process"];
    for r in reference {
        if let Some((_, why)) = BINDING_SPECIFIC.iter().find(|(id, _)| r.id == *id) {
            suite
                .identical
                .push(json!({"vector": r.id, "identical": "binding_specific", "detail": why}));
            continue;
        }
        let mut same = true;
        let mut detail = String::new();
        for b in ["http", "websocket"] {
            let other = suite.runs[b].iter().find(|x| x.id == r.id);
            match other {
                Some(o) if o.ubtp == r.ubtp && o.outcome == r.outcome => {}
                Some(o) => {
                    same = false;
                    let step = r
                        .ubtp
                        .iter()
                        .find(|(k, v)| o.ubtp.get(*k) != Some(v))
                        .map(|(k, _)| k.clone())
                        .unwrap_or_default();
                    detail.push_str(&format!("{b}: differs at {step}; "));
                }
                None => {
                    same = false;
                    detail.push_str(&format!("{b}: not run; "));
                }
            }
        }
        if verbose && !same {
            eprintln!("DIFF {} {}", r.id, detail);
        }
        suite
            .identical
            .push(json!({"vector": r.id, "identical": same, "detail": detail}));
    }
    Ok(suite)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let dir = arg("--vectors").unwrap_or_else(|| "standard/vectors/protocol".into());
    let verbose = args.iter().any(|a| a == "--verbose");
    let start = Instant::now();
    let suite = match run(std::path::Path::new(&dir), verbose) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let mut report = json!({
        "suite": "SUITE-NOD-PROTO", "item": "VER-CONF-7000", "step": "PH-1 14d", "decision": "DEC-075",
        "implementation": {"crate": "ubos-node", "version": env!("CARGO_PKG_VERSION")},
        "environment": {"backend": "memory", "os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "vectors": suite.runs["in_process"].len(),
        "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    let mut all_ok = true;
    let mut summary = Vec::new();
    for (b, runs) in &suite.runs {
        let passed = runs.iter().filter(|r| r.outcome == "pass").count();
        all_ok &= passed == runs.len();
        summary.push(format!("{b} {passed}/{}", runs.len()));
        report["bindings"][b] = json!({"passed": passed, "cases": runs.len(),
            "results": runs.iter().map(|r| {
                let mut v = json!({"vector": r.id, "outcome": r.outcome});
                if !r.detail.is_empty() { v["detail"] = json!(r.detail); }
                v
            }).collect::<Vec<_>>()});
        let skew = &suite.skew[b];
        let sp = skew.iter().filter(|c| c["outcome"] == "pass").count();
        all_ok &= sp == skew.len();
        report["version_skew"][b] = json!({"passed": sp, "cases": skew.len(), "results": skew});
    }
    let ident = suite
        .identical
        .iter()
        .filter(|x| x["identical"] == true)
        .count();
    let specific = suite
        .identical
        .iter()
        .filter(|x| x["identical"] == "binding_specific")
        .count();
    all_ok &= ident + specific == suite.identical.len();
    report["identical"] = json!({"vectors": ident, "binding_specific": specific, "of": suite.identical.len(), "results": suite.identical});
    let rp = suite.rest.iter().filter(|c| c["outcome"] == "pass").count();
    all_ok &= rp == suite.rest.len();
    report["rest_equivalence"] =
        json!({"passed": rp, "cases": suite.rest.len(), "results": suite.rest});
    let seed: [u8; 32] = crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report).unwrap_or_default();
    let sig = crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] =
        json!({"alg": "ed25519", "key": hex(&crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    if let Some(p) = arg("--report") {
        if let Some(dir) = std::path::Path::new(&p).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(
            &p,
            serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
        ) {
            eprintln!("cannot write {p}: {e}");
        }
    }
    println!(
        "protocol suite: vectors {}; identical across bindings {ident} of {} ({specific} binding-specific); version skew {}; REST equivalence {rp} of {} ({} s)",
        summary.join(", "),
        suite.identical.len(),
        suite
            .skew
            .iter()
            .map(|(b, s)| format!(
                "{b} {}/{}",
                s.iter().filter(|c| c["outcome"] == "pass").count(),
                s.len()
            ))
            .collect::<Vec<_>>()
            .join(", "),
        suite.rest.len(),
        report["seconds"]
    );
    if !all_ok {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    /// The Protocol vectors pass over the three bindings with identical results, the
    /// version-skew cases pass and the REST facade is equivalent (SUITE-NOD-PROTO,
    /// VER-CONF-7000; the verification of EXIT-1-15).
    #[test]
    fn protocol_suite_passes_over_every_binding() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../standard/vectors/protocol");
        let s = super::run(&dir, true).expect("suite runs");
        for (b, runs) in &s.runs {
            let failed: Vec<String> = runs
                .iter()
                .filter(|r| r.outcome != "pass")
                .map(|r| format!("{} {}", r.id, r.detail))
                .collect();
            assert!(failed.is_empty(), "{b}: {failed:?}");
            let skew: Vec<String> = s.skew[b]
                .iter()
                .filter(|c| c["outcome"] != "pass")
                .map(|c| c.to_string())
                .collect();
            assert!(skew.is_empty(), "{b} version skew: {skew:?}");
        }
        let diff: Vec<String> = s
            .identical
            .iter()
            .filter(|x| x["identical"] == false)
            .map(|x| x.to_string())
            .collect();
        assert!(diff.is_empty(), "not identical: {diff:?}");
        let rest: Vec<String> = s
            .rest
            .iter()
            .filter(|c| c["outcome"] != "pass")
            .map(|c| c.to_string())
            .collect();
        assert!(rest.is_empty(), "REST: {rest:?}");
    }
}
