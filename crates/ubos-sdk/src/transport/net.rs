//! Connections: an endpoint URL, a TCP or TLS stream, and one HTTP/1.1 request on it.

use crate::error::{Error, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

/// A connected byte stream (plain or TLS).
pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

/// Where a node listens: `http[s]://host[:port][/prefix]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// TLS (`https`, `wss`).
    pub tls: bool,
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Path prefix without a trailing slash (empty for the root).
    pub prefix: String,
}

impl Endpoint {
    /// Parse a node URL (`http`, `https`, `ws` or `wss`).
    pub fn parse(url: &str) -> Result<Endpoint> {
        let bad = || {
            Error::new(
                "MODEL.INVALID_VALUE",
                "a node URL is http[s]://host[:port][/prefix]",
                serde_json::json!({"url": url}),
            )
        };
        let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
        let tls = match scheme {
            "http" | "ws" => false,
            "https" | "wss" => true,
            _ => return Err(bad()),
        };
        let (auth, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let (host, port) = match auth.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => {
                (h.to_string(), p.parse::<u16>().map_err(|_| bad())?)
            }
            _ => (auth.to_string(), if tls { 443 } else { 80 }),
        };
        if host.is_empty() {
            return Err(bad());
        }
        Ok(Endpoint {
            tls,
            host: host.trim_matches(['[', ']']).to_string(),
            port,
            prefix: path.trim_end_matches('/').to_string(),
        })
    }

    /// `host[:port]` for the `Host` header.
    pub fn authority(&self) -> String {
        let default = if self.tls { 443 } else { 80 };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

/// Open a stream to the endpoint (TLS with the Mozilla roots when `tls`).
pub async fn connect(ep: &Endpoint) -> Result<Box<dyn Io>> {
    let tcp = TcpStream::connect((ep.host.as_str(), ep.port))
        .await
        .map_err(Error::transport)?;
    let _ = tcp.set_nodelay(true);
    if !ep.tls {
        return Ok(Box::new(tcp));
    }
    tls(ep, tcp).await
}

#[cfg(feature = "tls")]
async fn tls(ep: &Endpoint, tcp: TcpStream) -> Result<Box<dyn Io>> {
    use std::sync::{Arc, OnceLock};
    use tokio_rustls::rustls::{self, ClientConfig, RootCertStore, pki_types::ServerName};
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let config = CONFIG
        .get_or_init(|| {
            let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            Arc::new(
                ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()
                    .unwrap_or_else(|_| ClientConfig::builder_with_protocol_versions(&[]))
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone();
    let name = ServerName::try_from(ep.host.clone()).map_err(Error::transport)?;
    let s = tokio_rustls::TlsConnector::from(config)
        .connect(name, tcp)
        .await
        .map_err(Error::transport)?;
    Ok(Box::new(s))
}

#[cfg(not(feature = "tls"))]
async fn tls(_ep: &Endpoint, _tcp: TcpStream) -> Result<Box<dyn Io>> {
    Err(Error::transport(
        "https and wss need the SDK's `tls` feature",
    ))
}

/// One HTTP/1.1 request on a new connection; the response with its streaming body.
pub async fn request(ep: &Endpoint, req: Request<Full<Bytes>>) -> Result<Response<Incoming>> {
    let io = hyper_util::rt::TokioIo::new(connect(ep).await?);
    let (mut send, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(Error::transport)?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    send.send_request(req).await.map_err(Error::transport)
}

/// A request builder for `path` (under the endpoint's prefix) with the `Host` header.
pub fn builder(ep: &Endpoint, method: &str, path: &str) -> hyper::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(format!("{}{path}", ep.prefix))
        .header("host", ep.authority())
        .header(
            "user-agent",
            concat!("ubos-sdk-rust/", env!("CARGO_PKG_VERSION")),
        )
}

/// The whole body of a response.
pub async fn body(resp: Response<Incoming>) -> Result<Bytes> {
    resp.into_body()
        .collect()
        .await
        .map(|b| b.to_bytes())
        .map_err(Error::transport)
}
