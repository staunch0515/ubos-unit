//! The single-node Server over UBTP (PH-1 step 14b, DEC-075; DSN-NOD-001, -003, -006,
//! -008, -010…-018; STD-PROTO-020, -021).
//!
//! The Server gives the kernel's in-process binding (`ubos-dvm::ubtp`, the adapter
//! operation `ubtp`) its network transports: every message that arrives over WebSocket,
//! HTTP or the REST facade is decoded, authenticated, routed to its tenant and VAE, handed
//! to that one binding and answered with the frames it produced, so that the semantics are
//! the binding's whatever the transport (FR-INT-015). Each VAE runs on one actor thread
//! ([`VaeHandle`]: the kernel is single-threaded and its store is not `Send`), the Tokio
//! runtime serves the connections (DSN-NOD-001), and after a mutate the Server fans the
//! emits of the commit out to the other sessions of the VAE and polls once a second as the
//! fallback (DSN-NOD-014).
//!
//! Bindings (STD-PROTO-020): WebSocket at `GET /ubtp/v1` with the subprotocol `ubtp.v1`,
//! one message per frame, one session per connection; HTTP at `POST /ubtp/v1` taking a
//! message or a list of messages with the session in the `Ubtp-Session` header (the
//! handshake opens the session and returns its id), and `GET /ubtp/v1/stream?session=` as
//! server-sent events carrying the `emit` frames; the REST facade of STD-PROTO-021 under
//! `/api/v1`. Frames are limited to 16 MB (DSN-NOD-018) and each session's push queue to
//! 1,000 frames (DSN-NOD-016): a queue that overflows closes the stream, and the client
//! resumes its subscriptions from their cursors (DSN-NOD-015).
//!
//! Credentials: a session's handshake over the network carries `{kind: "dev", principal?,
//! principal_kind?, role?, grants?}` in the `dev` preset, which the Server turns into the
//! binding's in-process credentials; the bearer tokens, sessions and API keys of local
//! identity come with step 14c. In-process credentials arriving over the network are
//! refused.

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ubos_dvm::host::Vae;
use ubos_dvm::types::BpaError;

/// Largest frame (DSN-NOD-018).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
/// Bounded push queue per session (DSN-NOD-016).
pub const QUEUE: usize = 1_000;
/// The WebSocket subprotocol (STD-PROTO-020).
pub const SUBPROTOCOL: &str = "ubtp.v1";

/// The HTTP status of each registered error code (STD-ERR-002, the `HTTP` column).
const STATUS: &[(&str, u16)] = &[
    ("MODEL.DECLARATION_INCOMPLETE", 422),
    ("MODEL.CONSTRAINT_VIOLATED", 422),
    ("MODEL.NARROWING_VIOLATION", 422),
    ("MODEL.LINEARIZATION_INCONSISTENT", 422),
    ("MODEL.INHERITANCE_CYCLE", 422),
    ("MODEL.KIND_MISMATCH", 422),
    ("MODEL.KIND_IMMUTABLE", 409),
    ("MODEL.ABSTRACT_INSTANTIATION", 422),
    ("MODEL.KEY_CONFLICT", 409),
    ("MODEL.REFERENCE_INVALID", 422),
    ("MODEL.REFERENCE_RESTRICTED", 409),
    ("MODEL.CARDINALITY_VIOLATED", 422),
    ("MODEL.PORT_SIGNATURE_INCOMPATIBLE", 422),
    ("MODEL.PORT_NOT_IMPLEMENTED", 501),
    ("MODEL.LENS_REQUIRED", 422),
    ("MODEL.MIGRATION_REQUIRED", 422),
    ("MODEL.CURRENCY_MISMATCH", 422),
    ("MODEL.OBJECT_TOO_LARGE", 413),
    ("MODEL.INVALID_VALUE", 422),
    ("MODEL.UNKNOWN_TYPE", 422),
    ("MODEL.INVALID_NAME", 422),
    ("VER.CHANGE_SET_REQUIRED", 409),
    ("VER.MERGE_CONFLICT", 409),
    ("VER.INVALID_BRANCH_NAME", 422),
    ("VER.TAG_IMMUTABLE", 409),
    ("VER.INHERITANCE_CYCLE", 422),
    ("VER.SELECTION_NOT_CLOSED", 422),
    ("VER.MERGE_NOT_ALLOWED", 409),
    ("VER.BRANCH_ARCHIVED", 409),
    ("TIME.OVERLAPPING_VALIDITY", 409),
    ("TIME.RESERVED_FIELD", 422),
    ("LEDG.IMMUTABLE_ENTRY", 409),
    ("LEDG.ALREADY_REVERSED", 409),
    ("LEDG.REVERSAL_NOT_REVERSIBLE", 409),
    ("LEDG.UNBALANCED", 422),
    ("LEDG.PERIOD_CLOSED", 409),
    ("LEDG.CORRECTION_TARGET_MISSING", 409),
    ("TXN.HEAD_MOVED", 409),
    ("TXN.READ_SET_CHANGED", 409),
    ("TXN.ALREADY_EXISTS", 409),
    ("TXN.IDEMPOTENCY_MISMATCH", 422),
    ("LOGIC.LIMIT_EXCEEDED", 429),
    ("LOGIC.CANCELLED", 499),
    ("LOGIC.FORBIDDEN_CAPABILITY", 403),
    ("LOGIC.FORBIDDEN_TARGET", 403),
    ("LOGIC.CONTEXT_IMMUTABLE", 500),
    ("LOGIC.PARAMETER_FORBIDDEN", 400),
    ("LOGIC.TYPE_ERROR", 422),
    ("LOGIC.UNKNOWN_FIELD", 422),
    ("LOGIC.DIVISION_BY_ZERO", 422),
    ("LOGIC.FLOAT_TO_DECIMAL", 422),
    ("LOGIC.ABI_UNSUPPORTED", 501),
    ("LOGIC.UNVERIFIED_ASSET", 403),
    ("LOGIC.ASSET_QUARANTINED", 503),
    ("LOGIC.SCRIPT_TOO_COMPLEX", 422),
    ("LOGIC.REPLAY_INPUT_MISSING", 500),
    ("LOGIC.CONNECTOR_UNAVAILABLE", 503),
    ("LOGIC.CONNECTOR_ERROR", 502),
    ("LOGIC.CLASSIFICATION_EGRESS_DENIED", 403),
    ("LOGIC.EGRESS_NOT_ALLOWED", 403),
    ("LOGIC.NOT_AVAILABLE", 501),
    ("RULE.PARSE_ERROR", 422),
    ("RULE.UNKNOWN_PROPERTY", 422),
    ("RULE.AMBIGUOUS_CASCADE", 422),
    ("RULE.LOCKED_OVERRIDE", 409),
    ("RULE.DECLARATION_UNSATISFIED", 422),
    ("RULE.INVARIANT_VIOLATED", 422),
    ("RULE.VALIDATION_FAILED", 422),
    ("RULE.DECISION_DENIED", 403),
    ("RULE.DECISION_REQUIRES", 202),
    ("RULE.NO_MATCH", 422),
    ("RULE.MULTIPLE_MATCH", 422),
    ("RULE.UNTYPED_ACCESS", 422),
    ("RULE.TESTS_FAILED", 409),
    ("FLOW.TRANSITION_NOT_ALLOWED", 409),
    ("FLOW.GUARD_FAILED", 409),
    ("FLOW.STATE_DIRECT_WRITE", 409),
    ("FLOW.PORT_UNAVAILABLE", 409),
    ("FLOW.EFFECT_FAILED", 422),
    ("FLOW.FIELD_RULE_VIOLATED", 422),
    ("EVT.UNKNOWN_EVENT_TYPE", 422),
    ("QRY.SCAN_LIMIT", 422),
    ("QRY.INVALID_QUERY", 400),
    ("QRY.NOT_FOUND_AT_TIME", 404),
    ("QRY.CLASSIFICATION_NOT_EMBEDDABLE", 422),
    ("IAM.DENIED", 403),
    ("IAM.NOT_FOUND", 404),
    ("IAM.DELEGATION_EXPIRED", 401),
    ("IAM.KEY_UNAVAILABLE", 503),
    ("TEN.SUSPENDED", 423),
    ("TEN.OVERRIDE_LOCKED", 409),
    ("PKG.SIGNATURE_INVALID", 422),
    ("PKG.MANIFEST_INVALID", 422),
    ("PKG.NAMESPACE_VIOLATION", 409),
    ("PKG.CONFIGURATION_INCOMPLETE", 409),
    ("PROOF.SIGNATURE_INVALID", 422),
    ("PROOF.VALUE_ERASED", 410),
    ("PROOF.VERIFICATION_FAILED", 422),
    ("SYNC.INTEGRITY_FAILURE", 422),
    ("SYNC.UPSTREAM_UNAVAILABLE", 502),
    ("AI.CLASSIFICATION_BLOCKED", 403),
    ("AI.BUDGET_EXHAUSTED", 429),
    ("AI.OUTPUT_INVALID", 502),
    ("AI.PROVIDER_UNAVAILABLE", 503),
    ("AUD.LEGAL_HOLD_ACTIVE", 423),
    ("BILL.LICENCE_INACTIVE", 402),
    ("BILL.NOT_ENTITLED", 402),
    ("OPS.QUOTA_EXCEEDED", 429),
    ("INT.PROTOCOL_ERROR", 400),
    ("INT.VERSION_UNSUPPORTED", 426),
    ("INT.CURSOR_EXPIRED", 410),
    ("ADDR.CONFLICTING_SELECTORS", 400),
    ("NOD.STORAGE_UNAVAILABLE", 503),
    ("NOD.BLOB_UNAVAILABLE", 503),
];

/// The HTTP status of an error code: the registered status, else 500.
pub fn status_of(code: &str) -> u16 {
    STATUS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, s)| *s)
        .unwrap_or(500)
}

/// An RFC 9457 problem with the `code` and `explanation` extensions (STD-PROTO-021).
pub fn problem(e: &BpaError) -> Value {
    let status = status_of(e.code);
    let explanation = match &e.details {
        Some(d) => d.clone(),
        None => json!(e.message),
    };
    json!({"type": format!("urn:ubos:error:{}", e.code), "title": e.code, "status": status,
        "detail": e.message, "code": e.code, "explanation": explanation})
}

// ---- the VAE actor ----

struct Call {
    op: String,
    input: Value,
    reply: tokio::sync::oneshot::Sender<Result<Value, BpaError>>,
}

/// A VAE on its own thread: the kernel's operations run there in order (the kernel is
/// single-threaded), callers send operations and await their results.
#[derive(Clone)]
pub struct VaeHandle {
    tx: std::sync::mpsc::Sender<Call>,
}

impl VaeHandle {
    /// Start the actor; `open` runs on the actor's thread and opens the VAE.
    pub fn spawn<F>(open: F) -> Result<VaeHandle, BpaError>
    where
        F: FnOnce() -> Result<Vae, BpaError> + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel::<Call>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), BpaError>>();
        std::thread::Builder::new()
            .name("ubos-vae".into())
            .spawn(move || {
                let mut vae = match open() {
                    Ok(v) => {
                        let _ = ready_tx.send(Ok(()));
                        v
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(call) = rx.recv() {
                    let r = vae.call(&call.op, &call.input);
                    let _ = call.reply.send(r);
                }
            })
            .map_err(|e| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(VaeHandle { tx }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                "the VAE thread ended before it opened",
            )),
        }
    }

    /// Run one operation on the VAE and await its result.
    pub async fn call(&self, op: &str, input: Value) -> Result<Value, BpaError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Call {
                op: op.to_string(),
                input,
                reply,
            })
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?;
        rx.await
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?
    }

    /// Run one operation from a thread outside the runtime (suites and setup).
    pub fn call_blocking(&self, op: &str, input: Value) -> Result<Value, BpaError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Call {
                op: op.to_string(),
                input,
                reply,
            })
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?;
        rx.blocking_recv()
            .map_err(|_| BpaError::new("NOD.STORAGE_UNAVAILABLE", "the VAE thread ended"))?
    }
}

// ---- the server ----

/// The edition preset (DSN-NOD-004): `dev` accepts `dev` credentials; `server` accepts
/// only the credentials of local identity (step 14c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Development: `dev` credentials, the facade opens sessions for the default principal.
    Dev,
    /// Server: the credentials of local identity only.
    Server,
}

/// What the Server serves: the VAEs it can open, by `(tenant, authority)`.
pub trait VaeSource: Send + Sync {
    /// Open the VAE of a tenant and authority as an actor; `None` when there is none.
    fn open(&self, tenant: &str, authority: &str) -> Result<Option<VaeHandle>, BpaError>;
}

/// One VAE already open (suites, the memory preset): every tenant and authority maps to it.
pub struct SingleVae(pub VaeHandle);

impl VaeSource for SingleVae {
    fn open(&self, _tenant: &str, _authority: &str) -> Result<Option<VaeHandle>, BpaError> {
        Ok(Some(self.0.clone()))
    }
}

/// The VAEs of the node's tenancy registry (DSN-NOD-008).
pub struct NodeVaes(pub Arc<crate::Node>);

impl VaeSource for NodeVaes {
    fn open(&self, tenant: &str, authority: &str) -> Result<Option<VaeHandle>, BpaError> {
        let Some(t) = self.0.tenant(tenant)? else {
            return Ok(None);
        };
        if self.0.vae(&t, authority)?.is_none() {
            return Ok(None);
        }
        let node = self.0.clone();
        let authority = authority.to_string();
        VaeHandle::spawn(move || node.open_vae(&t, &authority)).map(Some)
    }
}

struct SessionEntry {
    vae: String,
    handle: VaeHandle,
    push: Option<tokio::sync::mpsc::Sender<Value>>,
    /// Frames the Server produced itself (errors before the binding), numbered apart.
    own: u64,
}

/// The Server's state.
pub struct Server {
    source: Box<dyn VaeSource>,
    preset: Preset,
    /// Default tenant and authority of requests that name none.
    default_tenant: String,
    default_authority: String,
    vaes: Mutex<HashMap<String, VaeHandle>>,
    sessions: Mutex<HashMap<String, SessionEntry>>,
    next_session: Mutex<u64>,
}

type Shared = Arc<Server>;

impl Server {
    /// A Server over a VAE source.
    pub fn new(
        source: Box<dyn VaeSource>,
        preset: Preset,
        tenant: &str,
        authority: &str,
    ) -> Shared {
        Arc::new(Server {
            source,
            preset,
            default_tenant: tenant.to_string(),
            default_authority: authority.to_string(),
            vaes: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            next_session: Mutex::new(0),
        })
    }

    /// The router of the bindings and the facade.
    pub fn router(self: &Shared) -> Router {
        Router::new()
            .route("/ubtp/v1", get(ws_upgrade).post(http_messages))
            .route("/ubtp/v1/stream", get(sse_stream))
            .route("/api/v1/{vae}/{class}", get(rest_query))
            .route("/api/v1/{vae}/{class}/{slug}", get(rest_read))
            .route("/api/v1/{vae}/{class}/{slug}/{port}", post(rest_mutate))
            .layer(DefaultBodyLimit::max(MAX_FRAME))
            .with_state(self.clone())
    }

    /// Serve on `listen` until the task is cancelled.
    pub async fn serve(self: Shared, listen: std::net::SocketAddr) -> std::io::Result<()> {
        let listener = tokio::net::TcpListener::bind(listen).await?;
        let poll = self.clone();
        tokio::spawn(async move { poll.poll_loop().await });
        axum::serve(listener, self.router()).await
    }

    /// The VAE of a tenant and authority (opened once; DSN-NOD-008).
    fn vae(&self, tenant: &str, authority: &str) -> Result<VaeHandle, BpaError> {
        let key = format!("{tenant}/{authority}");
        if let Some(h) = self.vaes.lock().expect("vaes").get(&key) {
            return Ok(h.clone());
        }
        let h = self
            .source
            .open(tenant, authority)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant or VAE"))?;
        self.vaes.lock().expect("vaes").insert(key, h.clone());
        Ok(h)
    }

    /// Open a session on the VAE a handshake names.
    fn open_session(
        &self,
        tenant: Option<&str>,
        authority: Option<&str>,
    ) -> Result<(String, VaeHandle), BpaError> {
        let tenant = tenant.unwrap_or(&self.default_tenant);
        let authority = authority.unwrap_or(&self.default_authority);
        let handle = self.vae(tenant, authority)?;
        let key = {
            let mut n = self.next_session.lock().expect("sessions");
            *n += 1;
            format!("sess-{}", *n)
        };
        self.sessions.lock().expect("sessions").insert(
            key.clone(),
            SessionEntry {
                vae: format!("{tenant}/{authority}"),
                handle: handle.clone(),
                push: None,
                own: 0,
            },
        );
        Ok((key, handle))
    }

    fn session(&self, key: &str) -> Option<(String, VaeHandle)> {
        self.sessions
            .lock()
            .expect("sessions")
            .get(key)
            .map(|s| (s.vae.clone(), s.handle.clone()))
    }

    fn close_session(&self, key: &str) {
        self.sessions.lock().expect("sessions").remove(key);
    }

    fn attach_push(&self, key: &str, tx: tokio::sync::mpsc::Sender<Value>) -> bool {
        match self.sessions.lock().expect("sessions").get_mut(key) {
            Some(s) => {
                s.push = Some(tx);
                true
            }
            None => false,
        }
    }

    /// A frame the Server produces itself, numbered apart from the binding's.
    fn own_error(&self, key: &str, e: &BpaError, reply_to: Option<&str>) -> Value {
        let n = match self.sessions.lock().expect("sessions").get_mut(key) {
            Some(s) => {
                s.own += 1;
                s.own
            }
            None => 0,
        };
        ubos_proto::error_frame(&format!("n{n}"), e, reply_to, None)
    }

    /// The request pipeline of one message (DSN-NOD-006): decode (done by the transport),
    /// authenticate (the handshake's credentials), construct the context (the session's
    /// VAE), call the binding, encode the frames. `session` is `None` for a message that
    /// opens a session: the handshake.
    async fn handle(
        self: &Shared,
        session: Option<String>,
        tenant: Option<String>,
        message: Value,
    ) -> (String, Vec<Value>) {
        let mut message = message;
        let rid = ubos_proto::id_of(&message);
        let is_handshake = message.get("type") == Some(&json!("handshake"));
        let (key, handle) = match session {
            Some(k) => match self.session(&k) {
                Some((_, h)) => (k, h),
                None => {
                    let e = BpaError::new("IAM.NOT_FOUND", "unknown session");
                    return (
                        k.clone(),
                        vec![ubos_proto::error_frame("n0", &e, rid.as_deref(), None)],
                    );
                }
            },
            None => {
                let authority = message
                    .get("body")
                    .and_then(|b| b.get("vae"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                match self.open_session(tenant.as_deref(), authority.as_deref()) {
                    Ok(x) => x,
                    Err(e) => {
                        return (
                            "".into(),
                            vec![ubos_proto::error_frame("n0", &e, rid.as_deref(), None)],
                        );
                    }
                }
            }
        };
        if is_handshake && let Err(e) = self.authenticate(&mut message) {
            return (key.clone(), vec![self.own_error(&key, &e, rid.as_deref())]);
        }
        let out = handle
            .call("ubtp", json!({"session": key, "messages": [message]}))
            .await;
        let frames = match out {
            Ok(v) => v["responses"].as_array().cloned().unwrap_or_default(),
            Err(e) => vec![self.own_error(&key, &e, rid.as_deref())],
        };
        if message.get("type") == Some(&json!("mutate")) {
            let me = self.clone();
            let k = key.clone();
            tokio::spawn(async move { me.fan_out(Some(&k)).await });
        }
        (key, frames)
    }

    /// The credentials a handshake over the network may carry (DEC-075 item 1, 14b): the
    /// `dev` kind of the `dev` preset becomes the binding's in-process credentials;
    /// in-process credentials from the network are refused; the kinds of local identity
    /// come with step 14c.
    fn authenticate(&self, message: &mut Value) -> Result<(), BpaError> {
        let Some(body) = message.get_mut("body").and_then(Value::as_object_mut) else {
            return Ok(());
        };
        let kind = body
            .get("credentials")
            .and_then(|c| c.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match (kind.as_str(), self.preset) {
            ("dev", Preset::Dev) => {
                if let Some(c) = body.get_mut("credentials").and_then(Value::as_object_mut) {
                    c.insert("kind".into(), json!("in_process"));
                }
                Ok(())
            }
            ("in_process", _) => Err(BpaError::new(
                "IAM.DENIED",
                "in-process credentials are not accepted over the network",
            )),
            (k, _) => Err(BpaError::new(
                "IAM.DENIED",
                format!("credentials of kind {k:?} are not accepted by this server"),
            )),
        }
    }

    /// Push the emits of new commits to the other sessions of the VAE a session committed
    /// on (DSN-NOD-014): each session with a live stream gets the binding's catch-up.
    async fn fan_out(self: &Shared, from: Option<&str>) {
        let targets: Vec<(String, VaeHandle, tokio::sync::mpsc::Sender<Value>)> = {
            let sessions = self.sessions.lock().expect("sessions");
            let vae = from.and_then(|k| sessions.get(k).map(|s| s.vae.clone()));
            sessions
                .iter()
                .filter(|(k, s)| {
                    Some(k.as_str()) != from && vae.as_ref().is_none_or(|v| *v == s.vae)
                })
                .filter_map(|(k, s)| s.push.clone().map(|p| (k.clone(), s.handle.clone(), p)))
                .collect()
        };
        for (key, handle, push) in targets {
            let r = handle
                .call("ubtp", json!({"session": key, "messages": []}))
                .await;
            if let Ok(v) = r {
                for f in v["responses"].as_array().into_iter().flatten() {
                    if push.try_send(f.clone()).is_err() {
                        // the queue is full (DSN-NOD-016): the stream closes, the client
                        // resumes from its cursors
                        if let Some(entry) = self.sessions.lock().expect("sessions").get_mut(&key) {
                            entry.push = None;
                        }
                        break;
                    }
                }
            }
        }
    }

    /// The poll fallback of DSN-NOD-014: once a second, every streaming session catches up.
    async fn poll_loop(self: Shared) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tick.tick().await;
            self.fan_out(None).await;
        }
    }
}

// ---- WebSocket (STD-PROTO-020) ----

async fn ws_upgrade(State(s): State<Shared>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let tenant = header(&headers, "ubtp-tenant");
    ws.protocols([SUBPROTOCOL])
        .max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| ws_session(s, tenant, socket))
}

async fn ws_session(s: Shared, tenant: Option<String>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (push_tx, mut push_rx) = tokio::sync::mpsc::channel::<Value>(QUEUE);
    let mut session: Option<String> = None;
    loop {
        tokio::select! {
            m = stream.next() => {
                let Some(Ok(m)) = m else { break };
                let text = match m {
                    Message::Text(t) => t.to_string(),
                    Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let message: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => {
                        let e = BpaError::new("INT.PROTOCOL_ERROR", "a frame is a JSON envelope");
                        let f = ubos_proto::error_frame("n0", &e, None, None);
                        if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
                        continue;
                    }
                };
                let (key, frames) = s.handle(session.clone(), tenant.clone(), message).await;
                if session.is_none() && !key.is_empty() {
                    session = Some(key.clone());
                    s.attach_push(&key, push_tx.clone());
                }
                for f in frames {
                    if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
                }
            }
            f = push_rx.recv() => {
                let Some(f) = f else { break };
                if sink.send(Message::Text(f.to_string().into())).await.is_err() { break; }
            }
        }
    }
    if let Some(k) = session {
        s.close_session(&k);
    }
}

// ---- HTTP (STD-PROTO-020) ----

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

async fn http_messages(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let tenant = header(&headers, "ubtp-tenant");
    let mut session = header(&headers, "ubtp-session");
    let messages: Vec<Value> = match serde_json::from_slice::<Value>(&body) {
        Ok(Value::Array(a)) => a,
        Ok(v) => vec![v],
        Err(_) => {
            let e = BpaError::new(
                "INT.PROTOCOL_ERROR",
                "the body is a message or a list of messages",
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"responses": [ubos_proto::error_frame("n0", &e, None, None)]})),
            )
                .into_response();
        }
    };
    let mut out = Vec::new();
    // an empty list on an open session polls the binding for pending emits
    if messages.is_empty()
        && let Some(k) = &session
        && let Some((_, h)) = s.session(k)
        && let Ok(v) = h.call("ubtp", json!({"session": k, "messages": []})).await
    {
        out.extend(v["responses"].as_array().cloned().unwrap_or_default());
    }
    for m in messages {
        let (key, frames) = s.handle(session.clone(), tenant.clone(), m).await;
        if session.is_none() && !key.is_empty() {
            session = Some(key);
        }
        out.extend(frames);
    }
    let mut resp = Json(json!({"responses": out})).into_response();
    if let Some(k) = session
        && let Ok(v) = k.parse()
    {
        resp.headers_mut().insert("ubtp-session", v);
    }
    resp
}

#[derive(serde::Deserialize)]
struct StreamQuery {
    session: String,
}

async fn sse_stream(State(s): State<Shared>, Query(q): Query<StreamQuery>) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Value>(QUEUE);
    if !s.attach_push(&q.session, tx) {
        let e = BpaError::new("IAM.NOT_FOUND", "unknown session");
        return (StatusCode::NOT_FOUND, Json(problem(&e))).into_response();
    }
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|f| {
            let ev: Result<Event, std::convert::Infallible> =
                Ok(Event::default().event("emit").data(f.to_string()));
            (ev, rx)
        })
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ---- REST facade (STD-PROTO-021) ----

/// The session of a facade request: the `Ubtp-Session` header, or in the `dev` preset a
/// session the facade opens for the default principal.
async fn facade_session(s: &Shared, headers: &HeaderMap, vae: &str) -> Result<String, BpaError> {
    if let Some(k) = header(headers, "ubtp-session") {
        return match s.session(&k) {
            Some(_) => Ok(k),
            None => Err(BpaError::new("IAM.NOT_FOUND", "unknown session")),
        };
    }
    if s.preset != Preset::Dev {
        return Err(BpaError::new(
            "IAM.DENIED",
            "the facade needs a session (Ubtp-Session)",
        ));
    }
    let tenant = header(headers, "ubtp-tenant");
    let hs = json!({"v": ubos_proto::VERSION, "id": "facade", "type": "handshake",
        "body": {"vae": vae, "versions": [ubos_proto::VERSION], "credentials": {"kind": "dev"}}});
    let (key, frames) = s.handle(None, tenant, hs).await;
    match frames.first() {
        Some(f) if f["type"] == json!("result") => Ok(key),
        Some(f) => Err(BpaError::with_details(
            f["body"]["code"]
                .as_str()
                .unwrap_or("IAM.DENIED")
                .to_string()
                .leak(),
            f["body"]["details"].clone(),
        )),
        None => Err(BpaError::new("IAM.DENIED", "no session")),
    }
}

fn facade_response(frames: Vec<Value>) -> Response {
    let Some(f) = frames.into_iter().next() else {
        let e = BpaError::new("NOD.STORAGE_UNAVAILABLE", "no response");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(problem(&e))).into_response();
    };
    if f["type"] == json!("error") {
        let code: &'static str = f["body"]["code"]
            .as_str()
            .unwrap_or("NOD.STORAGE_UNAVAILABLE")
            .to_string()
            .leak();
        let mut e = BpaError::with_details(code, f["body"]["details"].clone());
        e.message = f["body"]["details"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let status =
            StatusCode::from_u16(status_of(code)).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem(&e)),
        )
            .into_response();
    }
    Json(f["body"].clone()).into_response()
}

async fn facade(
    s: &Shared,
    headers: &HeaderMap,
    vae: &str,
    id: &str,
    type_: &str,
    body: Value,
) -> Response {
    let key = match facade_session(s, headers, vae).await {
        Ok(k) => k,
        Err(e) => {
            let status = StatusCode::from_u16(status_of(e.code))
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            return (
                status,
                [(header::CONTENT_TYPE, "application/problem+json")],
                Json(problem(&e)),
            )
                .into_response();
        }
    };
    let n = {
        let mut sessions = s.sessions.lock().expect("sessions");
        match sessions.get_mut(&key) {
            Some(e) => {
                e.own += 1;
                e.own
            }
            None => 0,
        }
    };
    let message =
        json!({"v": ubos_proto::VERSION, "id": format!("{id}-{n}"), "type": type_, "body": body});
    let (_, frames) = s.handle(Some(key), None, message).await;
    facade_response(frames)
}

#[derive(serde::Deserialize)]
struct RestQuery {
    q: Option<String>,
    branch: Option<String>,
}

async fn rest_read(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((vae, class, slug)): Path<(String, String, String)>,
    Query(q): Query<RestQuery>,
) -> Response {
    let mut body = json!({"vae": vae, "read": format!("ubos://{vae}/{class}/{slug}")});
    if let Some(b) = q.branch {
        body["branch"] = json!(b);
    }
    facade(&s, &headers, &vae, "read", "query", body).await
}

async fn rest_query(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((vae, class)): Path<(String, String)>,
    Query(q): Query<RestQuery>,
) -> Response {
    let text = match q.q {
        Some(t) if t.trim_start().starts_with("from ") => t,
        Some(t) => format!("from {class} where {t}"),
        None => format!("from {class}"),
    };
    let mut body = json!({"vae": vae, "query": text});
    if let Some(b) = q.branch {
        body["branch"] = json!(b);
    }
    facade(&s, &headers, &vae, "query", "query", body).await
}

async fn rest_mutate(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((vae, class, slug, port)): Path<(String, String, String, String)>,
    Query(q): Query<RestQuery>,
    body: Bytes,
) -> Response {
    let params: Value = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => {
                let e = BpaError::new(
                    "MODEL.INVALID_VALUE",
                    "the body is the JSON map of the port's parameters",
                );
                return (
                    StatusCode::BAD_REQUEST,
                    [(header::CONTENT_TYPE, "application/problem+json")],
                    Json(problem(&e)),
                )
                    .into_response();
            }
        }
    };
    let mut b = json!({"vae": vae, "port": {"target": format!("ubos://{vae}/{class}/{slug}"), "name": port, "params": params}});
    if let Some(br) = q.branch {
        b["branch"] = json!(br);
    }
    if let Some(k) = header(&headers, "idempotency-key") {
        b["idempotency_key"] = json!(k);
    }
    facade(&s, &headers, &vae, "mutate", "mutate", b).await
}

/// A memory VAE for the `memory` preset and the suites: an engine over the memory store
/// with the system clock and OS entropy, the genesis classes loaded when given.
pub fn memory_vae(classes: Option<&Value>) -> Result<Vae, BpaError> {
    use ubos_dvm::host::{NoConnectors, NoModels, OsEntropy, Recorder, Services, SystemClock};
    let mut engine = ubos_dvm::Engine::new(Box::new(ubos_dvm::store::MemStore::default()))?;
    if let Some(c) = classes {
        engine.load_genesis(c, None)?;
    }
    let rec = Arc::new(Recorder::default());
    let services = Services {
        clock: Arc::new(SystemClock),
        entropy: Arc::new(OsEntropy),
        connectors: Arc::new(NoConnectors),
        models: Arc::new(NoModels),
        outbox: rec.clone(),
        jobs: rec.clone(),
        telemetry: rec,
    };
    Ok(Vae::open(engine, services))
}

/// Unused-import guard for `Map` in builds without the facade helpers.
#[allow(dead_code)]
fn _map_type(_: &Map<String, Value>) {}
