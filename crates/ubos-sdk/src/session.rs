//! The session layer (DSN-SDK-001, -004, -010, -014, -016): the handshake with version
//! negotiation, the credentials, request–reply over the binding, and reconnection with
//! re-authentication that resumes every subscription from its last acknowledged cursor.

use crate::error::{Error, Result};
use crate::retry::RetryPolicy;
use crate::transport::{Binding, Link, net};
use crate::util::TraceContext;
use bytes::Bytes;
use http_body_util::Full;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

/// The protocol versions this SDK speaks, newest first (DSN-SDK-014).
pub const VERSIONS: &[&str] = &["1.0"];

/// How the SDK authenticates (DSN-SDK-010; PH-1: DEC-078 item 3).
#[derive(Clone)]
pub enum Auth {
    /// A local account: signed in at connection and again when the session ends.
    Password {
        /// User name.
        username: String,
        /// Password.
        password: String,
    },
    /// An API key.
    ApiKey(String),
    /// A bearer token obtained elsewhere.
    Token(String),
    /// No credentials (a node in the `dev` preset).
    Dev,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // credentials never reach logs
        f.write_str(match self {
            Auth::Password { .. } => "Auth::Password(..)",
            Auth::ApiKey(_) => "Auth::ApiKey(..)",
            Auth::Token(_) => "Auth::Token(..)",
            Auth::Dev => "Auth::Dev",
        })
    }
}

/// The configuration of a client (IF-SDK-001).
#[derive(Debug, Clone)]
pub struct Config {
    /// The node URL (`http[s]://host[:port]`).
    pub url: String,
    /// The VAE authority.
    pub vae: String,
    /// The credentials.
    pub auth: Auth,
    /// The binding.
    pub binding: Binding,
    /// The tenant (`Ubtp-Tenant`) when the node serves several.
    pub tenant: Option<String>,
    /// The retry policy.
    pub retry: RetryPolicy,
    /// The timeout of one request.
    pub request_timeout: Duration,
}

impl Config {
    /// A configuration with the defaults: WebSocket, the default retry policy, 30 s.
    pub fn new(url: &str, vae: &str, auth: Auth) -> Config {
        Config {
            url: url.to_string(),
            vae: vae.to_string(),
            auth,
            binding: Binding::WebSocket,
            tenant: None,
            retry: RetryPolicy::default(),
            request_timeout: Duration::from_secs(30),
        }
    }

    /// The same configuration over another binding.
    pub fn binding(mut self, b: Binding) -> Config {
        self.binding = b;
        self
    }
}

/// An update a subscription delivers (IF-SDK-005).
#[derive(Debug, Clone)]
pub enum Update {
    /// The changes or events of one commit with its cursor.
    Emit(Value),
    /// The resume point expired (`INT.CURSOR_EXPIRED`): the application refetches and the
    /// subscription continues from the head.
    Reset(Error),
}

pub(crate) struct SubState {
    pub body: Value,
    pub cursor: Option<String>,
    pub server_id: Option<String>,
    pub tx: mpsc::UnboundedSender<Update>,
}

pub(crate) struct Inner {
    pub cfg: Config,
    pub link: Link,
    next: AtomicU64,
    connected: AtomicBool,
    reconnecting: tokio::sync::Mutex<()>,
    pub token: std::sync::Mutex<Option<String>>,
    pub handshake: std::sync::Mutex<Value>,
    pub subs: std::sync::Mutex<HashMap<u64, SubState>>,
    pub generation: AtomicU64,
}

/// A connected client (IF-SDK-001). Cheap to clone; clones share the session.
#[derive(Clone)]
pub struct Client {
    pub(crate) inner: Arc<Inner>,
}

impl Client {
    /// Connect: authenticate and hand-shake (`IAM.DENIED`, `INT.VERSION_UNSUPPORTED`).
    pub async fn connect(cfg: Config) -> Result<Client> {
        let ep = net::Endpoint::parse(&cfg.url)?;
        let (tx, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            link: Link::new(cfg.binding, ep, cfg.tenant.clone(), tx),
            cfg,
            next: AtomicU64::new(1),
            connected: AtomicBool::new(false),
            reconnecting: tokio::sync::Mutex::new(()),
            token: std::sync::Mutex::new(None),
            handshake: std::sync::Mutex::new(Value::Null),
            subs: std::sync::Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
        });
        let c = Client { inner };
        c.spawn_router(rx);
        c.establish().await?;
        c.spawn_watchdog();
        Ok(c)
    }

    /// Reopen the session when its stream ended by itself while subscriptions are open, so
    /// that they resume from their cursors without waiting for a request (DSN-SDK-004).
    fn spawn_watchdog(&self) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let Some(inner) = weak.upgrade() else { break };
                let subscribed = inner
                    .subs
                    .lock()
                    .expect("subs")
                    .values()
                    .any(|s| s.server_id.is_some());
                if subscribed && inner.link.lost() {
                    let c = Client { inner };
                    let seen = c.inner.generation.load(Ordering::SeqCst);
                    c.inner.connected.store(false, Ordering::SeqCst);
                    let deadline = std::time::Instant::now() + c.inner.cfg.retry.deadline;
                    let _ = c.reconnect(seen, deadline).await;
                }
            }
        });
    }

    /// The handshake result: `{version, abi, session_id, principal, limits, features}`.
    pub fn session(&self) -> Value {
        self.inner.handshake.lock().expect("handshake").clone()
    }

    /// The protocol version negotiated.
    pub fn version(&self) -> String {
        self.session()["version"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    /// The configuration.
    pub fn config(&self) -> &Config {
        &self.inner.cfg
    }

    /// The bearer credential for the HTTP APIs (files): the session token or API key.
    pub(crate) fn bearer(&self) -> Option<String> {
        match &self.inner.cfg.auth {
            Auth::ApiKey(k) => Some(k.clone()),
            Auth::Token(t) => Some(t.clone()),
            Auth::Password { .. } => self.inner.token.lock().expect("token").clone(),
            Auth::Dev => None,
        }
    }

    fn id(&self) -> String {
        format!("c{}", self.inner.next.fetch_add(1, Ordering::Relaxed))
    }

    /// Sign in with a password (`POST /api/v1/identity/sign-in`).
    async fn sign_in(&self, username: &str, password: &str) -> Result<String> {
        let ep = self.inner.link.endpoint();
        let body = json!({"username": username, "password": password}).to_string();
        let req = net::builder(ep, "POST", "/api/v1/identity/sign-in")
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body)))
            .map_err(Error::transport)?;
        let resp = net::request(ep, req).await?;
        let ok = resp.status().is_success();
        let v: Value = serde_json::from_slice(&net::body(resp).await?).unwrap_or(Value::Null);
        if !ok {
            return Err(Error::from_body(&v));
        }
        v["token"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Error::new("IAM.DENIED", "no token", v))
    }

    async fn credentials(&self) -> Result<Value> {
        Ok(match &self.inner.cfg.auth {
            Auth::Password { username, password } => {
                // a reconnection reuses the session token while the node accepts it
                let held = self.inner.token.lock().expect("token").clone();
                let t = match held {
                    Some(t) => t,
                    None => self.sign_in(username, password).await?,
                };
                *self.inner.token.lock().expect("token") = Some(t.clone());
                json!({"kind": "bearer", "token": t})
            }
            Auth::ApiKey(k) => json!({"kind": "api_key", "key": k}),
            Auth::Token(t) => json!({"kind": "bearer", "token": t}),
            Auth::Dev => json!({"kind": "dev"}),
        })
    }

    /// Open the session: credentials, handshake, then resubscribe. A held session token
    /// the node refuses (expired, revoked) is dropped and the client signs in again.
    async fn establish(&self) -> Result<()> {
        let held = matches!(self.inner.cfg.auth, Auth::Password { .. })
            && self.inner.token.lock().expect("token").is_some();
        match self.establish_once().await {
            Err(e) if held && !e.is_transport() && e.code_text.starts_with("IAM.") => {
                *self.inner.token.lock().expect("token") = None;
                self.establish_once().await
            }
            r => r,
        }
    }

    async fn establish_once(&self) -> Result<()> {
        let inner = &self.inner;
        inner.link.reset().await;
        let credentials = self.credentials().await?;
        let hs = json!({"v": VERSIONS[0], "id": self.id(), "type": "handshake",
            "body": {"vae": inner.cfg.vae, "versions": VERSIONS, "abi": {"min": "1.0", "max": "1.0"},
                "encodings": ["json"], "features": [], "credentials": credentials}});
        let frames = inner
            .link
            .exchange(vec![hs], inner.cfg.request_timeout)
            .await?;
        let f = frames
            .into_iter()
            .next()
            .ok_or_else(|| Error::transport("no handshake reply"))?;
        if f["type"] != "result" {
            return Err(Error::from_body(&f["body"]));
        }
        let version = f["body"]["version"].as_str().unwrap_or_default();
        if !VERSIONS.contains(&version) {
            return Err(Error::new(
                "INT.VERSION_UNSUPPORTED",
                format!("the node chose {version}"),
                json!({"supported": VERSIONS}),
            ));
        }
        *inner.handshake.lock().expect("handshake") = f["body"].clone();
        inner.connected.store(true, Ordering::SeqCst);
        inner.generation.fetch_add(1, Ordering::SeqCst);
        self.resubscribe().await;
        Ok(())
    }

    /// Reconnect with back-off within `deadline` (DSN-SDK-004); concurrent callers wait
    /// for one reconnection.
    pub(crate) async fn reconnect(&self, seen: u64, deadline: std::time::Instant) -> Result<()> {
        let _g = self.inner.reconnecting.lock().await;
        if self.inner.generation.load(Ordering::SeqCst) != seen
            && self.inner.connected.load(Ordering::SeqCst)
        {
            return Ok(());
        }
        self.inner.connected.store(false, Ordering::SeqCst);
        let policy = self.inner.cfg.retry.clone();
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.establish().await {
                Ok(()) => return Ok(()),
                Err(e) if !e.is_transport() => return Err(e),
                Err(e) => {
                    let wait = policy.backoff(attempt);
                    if std::time::Instant::now() + wait >= deadline {
                        return Err(e);
                    }
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }

    /// One request and its reply body (`result`), or its error. A lost session (the node
    /// restarted, the session expired) reconnects once and repeats the request.
    pub async fn request(
        &self,
        type_: &str,
        body: Value,
        trace: Option<&TraceContext>,
    ) -> Result<Value> {
        let deadline = std::time::Instant::now() + self.inner.cfg.retry.deadline;
        self.request_until(type_, body, trace, deadline).await
    }

    pub(crate) async fn request_until(
        &self,
        type_: &str,
        body: Value,
        trace: Option<&TraceContext>,
        deadline: std::time::Instant,
    ) -> Result<Value> {
        for round in 0..2 {
            let generation = self.inner.generation.load(Ordering::SeqCst);
            if self.inner.link.lost() {
                self.inner.connected.store(false, Ordering::SeqCst);
            }
            if !self.inner.connected.load(Ordering::SeqCst) {
                self.reconnect(generation, deadline).await?;
            }
            let mut m = json!({"v": VERSIONS[0], "id": self.id(), "type": type_, "body": body});
            if let Some(t) = trace {
                m["trace"] = json!(t.child().traceparent());
            }
            let r = self
                .inner
                .link
                .exchange(vec![m], self.inner.cfg.request_timeout)
                .await;
            let frames = match r {
                Ok(f) => f,
                Err(e) => {
                    self.inner.connected.store(false, Ordering::SeqCst);
                    return Err(e);
                }
            };
            let f = frames.into_iter().next().unwrap_or(Value::Null);
            if f["type"] == "result" {
                return Ok(f["body"].clone());
            }
            let e = Error::from_body(&f["body"]);
            let says = |w: &str| {
                e.message.contains(w) || e.details.as_str().is_some_and(|d| d.contains(w))
            };
            let lost = (e.code_text == "INT.PROTOCOL_ERROR" && says("handshake"))
                || (e.code_text == "IAM.NOT_FOUND" && says("session"))
                || (e.code_text == "IAM.DENIED" && e.reason() == Some("session_expired"));
            if lost && round == 0 {
                self.inner.connected.store(false, Ordering::SeqCst);
                continue;
            }
            return Err(e);
        }
        Err(Error::transport("the session could not be restored"))
    }

    /// Route `emit` frames to their subscriptions and acknowledge them (STD-PROTO-013).
    fn spawn_router(&self, mut rx: mpsc::UnboundedReceiver<Value>) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            while let Some(f) = rx.recv().await {
                let Some(inner) = weak.upgrade() else { break };
                let b = &f["body"];
                let (Some(sid), Some(cursor)) =
                    (b["subscription_id"].as_str(), b["cursor"].as_str())
                else {
                    continue;
                };
                let delivered = {
                    let mut subs = inner.subs.lock().expect("subs");
                    match subs
                        .values_mut()
                        .find(|s| s.server_id.as_deref() == Some(sid))
                    {
                        Some(s) => {
                            s.cursor = Some(cursor.to_string());
                            s.tx.send(Update::Emit(b.clone())).is_ok()
                        }
                        None => false,
                    }
                };
                if delivered {
                    let c = Client { inner };
                    let _ = c
                        .request(
                            "ack",
                            json!({"subscription_id": sid, "cursor": cursor}),
                            None,
                        )
                        .await;
                }
            }
        });
    }

    /// Subscribe again after a reconnection, each from its last acknowledged cursor; an
    /// expired cursor sends [`Update::Reset`] and subscribes from the head.
    async fn resubscribe(&self) {
        let pending: Vec<(u64, Value, Option<String>)> = self
            .inner
            .subs
            .lock()
            .expect("subs")
            .iter()
            .filter(|(_, s)| s.server_id.is_some())
            .map(|(k, s)| (*k, s.body.clone(), s.cursor.clone()))
            .collect();
        for (k, body, cursor) in pending {
            let mut b = body.clone();
            if let Some(c) = &cursor {
                b["from_cursor"] = json!(c);
            }
            let r = match self.subscribe_raw(b).await {
                Err(e) if e.code_text == "INT.CURSOR_EXPIRED" => {
                    if let Some(s) = self.inner.subs.lock().expect("subs").get(&k) {
                        let _ = s.tx.send(Update::Reset(e));
                    }
                    self.subscribe_raw(body).await
                }
                r => r,
            };
            if let Ok(v) = r
                && let Some(s) = self.inner.subs.lock().expect("subs").get_mut(&k)
            {
                s.server_id = v["subscription_id"].as_str().map(str::to_string);
                if let Some(c) = v["cursor"].as_str() {
                    s.cursor = Some(c.to_string());
                }
            }
        }
    }

    pub(crate) async fn subscribe_raw(&self, body: Value) -> Result<Value> {
        let mut m = json!({"v": VERSIONS[0], "id": self.id(), "type": "subscribe", "body": body});
        if m["body"]["vae"].is_null() {
            m["body"]["vae"] = json!(self.inner.cfg.vae);
        }
        let frames = self
            .inner
            .link
            .exchange(vec![m], self.inner.cfg.request_timeout)
            .await?;
        let f = frames.into_iter().next().unwrap_or(Value::Null);
        if f["type"] == "result" {
            Ok(f["body"].clone())
        } else {
            Err(Error::from_body(&f["body"]))
        }
    }
}
