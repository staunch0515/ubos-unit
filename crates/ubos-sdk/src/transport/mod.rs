//! The transport layer (DSN-SDK-001, STD-PROTO-020): the UBTP bindings of the Server.
//!
//! * [`Binding::Http`]: `POST /ubtp/v1` carries a list of messages and returns their
//!   frames; the `Ubtp-Session` header keeps the session; `emit` frames arrive on the
//!   server-sent events of `GET /ubtp/v1/stream?session=`.
//! * [`Binding::WebSocket`]: `/ubtp/v1` with the subprotocol `ubtp.v1`, one message per
//!   frame; replies are matched by `reply_to`, `emit` frames are routed to the session.
//!
//! The layer knows envelopes only; a lost connection is a transport error the session
//! layer reconnects from.

pub mod net;

use crate::error::{Error, Result};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use net::Endpoint;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// A UBTP binding of the Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    /// HTTP request–response with server-sent events for `emit`.
    Http,
    /// WebSocket, subprotocol `ubtp.v1`.
    WebSocket,
}

type Pending = Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<Value>>>>;
type WsSink =
    futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<Box<dyn net::Io>>, Message>;

/// The connection of one session over one binding.
pub struct Link {
    binding: Binding,
    ep: Endpoint,
    tenant: Option<String>,
    http_session: std::sync::Mutex<Option<String>>,
    ws: Mutex<Option<WsSink>>,
    pending: Pending,
    emits: mpsc::UnboundedSender<Value>,
    reader: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    lost: Arc<AtomicBool>,
}

impl Link {
    /// A link to an endpoint; `emit` frames go to `emits`.
    pub fn new(
        binding: Binding,
        ep: Endpoint,
        tenant: Option<String>,
        emits: mpsc::UnboundedSender<Value>,
    ) -> Link {
        Link {
            binding,
            ep,
            tenant,
            http_session: std::sync::Mutex::new(None),
            ws: Mutex::new(None),
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            emits,
            reader: std::sync::Mutex::new(None),
            lost: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the stream ended by itself (the session must be opened again).
    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    /// The endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.ep
    }

    /// The binding.
    pub fn binding(&self) -> Binding {
        self.binding
    }

    /// Drop the connection and the session (the next exchange opens anew).
    pub async fn reset(&self) {
        *self.http_session.lock().expect("session") = None;
        if let Some(h) = self.reader.lock().expect("reader").take() {
            h.abort();
        }
        self.lost.store(false, Ordering::SeqCst);
        if let Some(mut s) = self.ws.lock().await.take() {
            let _ = s.close().await;
        }
        for (_, tx) in self.pending.lock().expect("pending").drain() {
            drop(tx);
        }
    }

    /// Send messages and return the frames that reply to them, in order.
    pub async fn exchange(&self, messages: Vec<Value>, timeout: Duration) -> Result<Vec<Value>> {
        match tokio::time::timeout(timeout, async {
            match self.binding {
                Binding::Http => self.http(messages).await,
                Binding::WebSocket => self.websocket(messages).await,
            }
        })
        .await
        {
            Ok(r) => r,
            Err(_) => Err(Error::transport("the request timed out")),
        }
    }

    async fn http(&self, messages: Vec<Value>) -> Result<Vec<Value>> {
        let session = self.http_session.lock().expect("session").clone();
        let mut b =
            net::builder(&self.ep, "POST", "/ubtp/v1").header("content-type", "application/json");
        if let Some(s) = &session {
            b = b.header("ubtp-session", s);
        }
        if let Some(t) = &self.tenant {
            b = b.header("ubtp-tenant", t);
        }
        let body = serde_json::to_vec(&Value::Array(messages)).unwrap_or_default();
        let req = b
            .body(Full::new(Bytes::from(body)))
            .map_err(Error::transport)?;
        let resp = net::request(&self.ep, req).await?;
        let new_session = resp
            .headers()
            .get("ubtp-session")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let status = resp.status().as_u16();
        let bytes = net::body(resp).await?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| {
            Error::transport(format!("the node answered {status} without UBTP frames"))
        })?;
        if let Some(s) = new_session {
            let started = session.as_deref() != Some(s.as_str());
            *self.http_session.lock().expect("session") = Some(s.clone());
            if started {
                self.start_sse(s);
            }
        }
        let mut out = Vec::new();
        for f in v["responses"].as_array().cloned().unwrap_or_default() {
            if f["type"] == "emit" {
                let _ = self.emits.send(f);
            } else {
                out.push(f);
            }
        }
        Ok(out)
    }

    /// Read the session's server-sent events into the emit channel.
    fn start_sse(&self, session: String) {
        let (ep, emits, lost) = (self.ep.clone(), self.emits.clone(), self.lost.clone());
        let h = tokio::spawn(async move {
            Link::read_sse(&ep, &session, &emits).await;
            // the stream ended by itself (an abort on reset never gets here)
            lost.store(true, Ordering::SeqCst);
        });
        if let Some(old) = self.reader.lock().expect("reader").replace(h) {
            old.abort();
        }
    }

    async fn read_sse(ep: &Endpoint, session: &str, emits: &mpsc::UnboundedSender<Value>) {
        {
            let path = format!("/ubtp/v1/stream?session={session}");
            let Ok(req) = net::builder(ep, "GET", &path)
                .header("accept", "text/event-stream")
                .body(Full::new(Bytes::new()))
            else {
                return;
            };
            let Ok(resp) = net::request(ep, req).await else {
                return;
            };
            let mut body = resp.into_body();
            let mut buf = String::new();
            while let Some(Ok(frame)) = body.frame().await {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                buf.push_str(&String::from_utf8_lossy(&data));
                while let Some(i) = buf.find("\n\n") {
                    let event: String = buf.drain(..i + 2).collect();
                    let data: String = event
                        .lines()
                        .filter_map(|l| l.strip_prefix("data:"))
                        .map(str::trim_start)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        let _ = emits.send(v);
                    }
                }
            }
        }
    }

    async fn ws_connect(&self) -> Result<WsSink> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let scheme = if self.ep.tls { "wss" } else { "ws" };
        let url = format!(
            "{scheme}://{}{}/ubtp/v1",
            self.ep.authority(),
            self.ep.prefix
        );
        let mut req = url.into_client_request().map_err(Error::transport)?;
        req.headers_mut().insert(
            "sec-websocket-protocol",
            hyper::http::HeaderValue::from_static("ubtp.v1"),
        );
        if let Some(t) = &self.tenant
            && let Ok(v) = hyper::http::HeaderValue::from_str(t)
        {
            req.headers_mut().insert("ubtp-tenant", v);
        }
        let io = net::connect(&self.ep).await?;
        let (ws, _) = tokio_tungstenite::client_async(req, io)
            .await
            .map_err(Error::transport)?;
        let (sink, mut stream) = ws.split();
        let (pending, emits) = (self.pending.clone(), self.emits.clone());
        let lost = self.lost.clone();
        let h = tokio::spawn(async move {
            while let Some(Ok(m)) = stream.next().await {
                let text = match m {
                    Message::Text(t) => t.to_string(),
                    Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let Ok(f) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let to = f["reply_to"].as_str().map(str::to_string);
                match to.and_then(|id| pending.lock().expect("pending").remove(&id)) {
                    Some(tx) => {
                        let _ = tx.send(f);
                    }
                    None if f["type"] == "emit" => {
                        let _ = emits.send(f);
                    }
                    None => {}
                }
            }
            // the connection ended: every waiting request fails with a transport error
            pending.lock().expect("pending").clear();
            lost.store(true, Ordering::SeqCst);
        });
        if let Some(old) = self.reader.lock().expect("reader").replace(h) {
            old.abort();
        }
        Ok(sink)
    }

    async fn websocket(&self, messages: Vec<Value>) -> Result<Vec<Value>> {
        let mut guard = self.ws.lock().await;
        if guard.is_none() {
            *guard = Some(self.ws_connect().await?);
        }
        let mut waits = Vec::new();
        for m in &messages {
            let id = m["id"].as_str().unwrap_or_default().to_string();
            let (tx, rx) = oneshot::channel();
            self.pending.lock().expect("pending").insert(id, tx);
            waits.push(rx);
        }
        let sink = guard.as_mut().expect("connected");
        for m in &messages {
            if let Err(e) = sink.send(Message::text(m.to_string())).await {
                *guard = None;
                return Err(Error::transport(e));
            }
        }
        drop(guard);
        let mut out = Vec::new();
        for rx in waits {
            match rx.await {
                Ok(f) => out.push(f),
                Err(_) => {
                    *self.ws.lock().await = None;
                    return Err(Error::transport("the connection closed"));
                }
            }
        }
        Ok(out)
    }
}
