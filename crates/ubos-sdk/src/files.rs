//! Files (DSN-SDK-011, IF-SDK-006): resumable tus uploads with progress and hash
//! verification, and ranged downloads through the node's signed links (DSN-NOD-320, -327).

use crate::error::{Error, Result};
use crate::session::Client;
use crate::transport::net;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// The value of a `blob` field (STD-FND-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    /// The file identifier.
    pub content_id: String,
    /// Size in bytes.
    pub size: u64,
    /// Media type.
    pub media_type: Option<String>,
    /// File name.
    pub name: Option<String>,
}

/// Options of an upload.
#[derive(Default)]
pub struct UploadOptions<'a> {
    /// File name.
    pub name: Option<String>,
    /// Media type.
    pub media_type: Option<String>,
    /// Bytes per request (default 4 MiB).
    pub chunk_size: Option<usize>,
    /// Called with `(bytes sent, total)`.
    pub on_progress: Option<&'a mut dyn FnMut(u64, u64)>,
    /// Resume this upload (`Location`) instead of creating one.
    pub resume: Option<String>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Client {
    async fn files_http(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, String)],
        body: Bytes,
    ) -> Result<(u16, hyper::HeaderMap, Bytes)> {
        let ep = self.inner.link.endpoint();
        let mut b = net::builder(ep, method, path).header("tus-resumable", "1.0.0");
        if let Some(t) = self.bearer() {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        if let Some(t) = &self.inner.cfg.tenant {
            b = b.header("ubtp-tenant", t);
        }
        for (k, v) in headers {
            b = b.header(*k, v);
        }
        let req = b.body(Full::new(body)).map_err(Error::transport)?;
        let resp = net::request(ep, req).await?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let bytes = net::body(resp).await?;
        Ok((status, headers, bytes))
    }

    fn problem(status: u16, body: &Bytes) -> Error {
        let v: Value = serde_json::from_slice(body)
            .unwrap_or(json!({"code": "INT.PROTOCOL_ERROR", "detail": format!("HTTP {status}")}));
        Error::from_body(&v)
    }

    /// The offset an upload resumes from (`HEAD`).
    pub async fn upload_offset(&self, location: &str) -> Result<u64> {
        let (st, h, b) = self.files_http("HEAD", location, &[], Bytes::new()).await?;
        if st != 200 {
            return Err(Client::problem(st, &b));
        }
        h.get("upload-offset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| Error::transport("no Upload-Offset"))
    }

    /// Upload bytes as a file: create the upload (or resume `opts.resume`), send the bytes
    /// in chunks, resume from the node's offset after a transport failure, and check the
    /// node's SHA-256 of the result against the bytes sent.
    pub async fn upload(
        &self,
        data: &[u8],
        mut opts: UploadOptions<'_>,
    ) -> Result<(BlobRef, String)> {
        let vae = self.inner.cfg.vae.clone();
        let total = data.len() as u64;
        let location = match opts.resume.take() {
            Some(l) => l,
            None => {
                let mut meta = Vec::new();
                if let Some(n) = &opts.name {
                    meta.push(format!("filename {}", crate::util::base64(n.as_bytes())));
                }
                if let Some(t) = &opts.media_type {
                    meta.push(format!("filetype {}", crate::util::base64(t.as_bytes())));
                }
                let (st, h, b) = self
                    .files_http(
                        "POST",
                        &format!("/files/v1/{vae}"),
                        &[
                            ("upload-length", total.to_string()),
                            ("upload-metadata", meta.join(",")),
                        ],
                        Bytes::new(),
                    )
                    .await?;
                if st != 201 {
                    return Err(Client::problem(st, &b));
                }
                h.get("location")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
                    .ok_or_else(|| Error::transport("no Location"))?
            }
        };
        let chunk = opts.chunk_size.unwrap_or(4 << 20).max(1);
        let deadline = Instant::now()
            + self
                .inner
                .cfg
                .retry
                .deadline
                .max(std::time::Duration::from_secs(300));
        let mut offset = if total == 0 {
            0
        } else {
            self.upload_offset(&location).await?
        };
        let mut attempt = 0;
        while offset < total {
            let end = (offset as usize + chunk).min(data.len());
            let r = self
                .files_http(
                    "PATCH",
                    &location,
                    &[
                        ("upload-offset", offset.to_string()),
                        ("content-type", "application/offset+octet-stream".into()),
                    ],
                    Bytes::copy_from_slice(&data[offset as usize..end]),
                )
                .await;
            match r {
                Ok((204, h, _)) => {
                    offset = h
                        .get("upload-offset")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(end as u64);
                    attempt = 0;
                    if let Some(p) = opts.on_progress.as_mut() {
                        p(offset, total);
                    }
                }
                Ok((409, _, _)) => offset = self.upload_offset(&location).await?,
                Ok((st, _, b)) => return Err(Client::problem(st, &b)),
                Err(e) => {
                    attempt += 1;
                    let wait = self.inner.cfg.retry.backoff(attempt);
                    if Instant::now() + wait >= deadline
                        || attempt >= self.inner.cfg.retry.max_attempts
                    {
                        return Err(e);
                    }
                    tokio::time::sleep(wait).await;
                    offset = self.upload_offset(&location).await?;
                }
            }
        }
        let (st, _, b) = self.files_http("GET", &location, &[], Bytes::new()).await?;
        if st != 200 {
            return Err(Client::problem(st, &b));
        }
        let v: Value = serde_json::from_slice(&b).map_err(Error::transport)?;
        let reference: BlobRef = serde_json::from_value(v["reference"].clone())
            .map_err(|_| Error::transport("the upload did not complete"))?;
        let local = hex(&Sha256::digest(data));
        if v["sha256"].as_str().is_some_and(|s| s != local) {
            return Err(Error::new(
                "NOD.BLOB_UNAVAILABLE",
                "the node's SHA-256 of the file differs from the bytes sent",
                json!({"integrity": true}),
            ));
        }
        Ok((reference, location))
    }

    /// A signed download link for the file of an object's `blob` field:
    /// `{url, expires_in, file, size, sha256, classification}`.
    pub async fn file_link(&self, object: &str, field: &str) -> Result<Value> {
        let vae = self.inner.cfg.vae.clone();
        let body = json!({"object": object, "field": field}).to_string();
        let (st, _, b) = self
            .files_http(
                "POST",
                &format!("/files/v1/{vae}/links"),
                &[("content-type", "application/json".into())],
                Bytes::from(body),
            )
            .await?;
        if st != 200 {
            return Err(Client::problem(st, &b));
        }
        serde_json::from_slice(&b).map_err(Error::transport)
    }

    /// Download the file of an object's `blob` field, whole (`range = None`, verified
    /// against its SHA-256) or a byte range `[start, end)`; `on_progress(bytes, total)`.
    pub async fn download(
        &self,
        object: &str,
        field: &str,
        range: Option<(u64, u64)>,
        mut on_progress: Option<&mut dyn FnMut(u64, u64)>,
    ) -> Result<Vec<u8>> {
        let link = self.file_link(object, field).await?;
        let url = link["url"].as_str().unwrap_or_default().to_string();
        let ep = self.inner.link.endpoint();
        let mut b = net::builder(ep, "GET", &url);
        if let Some((s, e)) = range {
            b = b.header("range", format!("bytes={s}-{}", e.saturating_sub(1)));
        }
        let req = b.body(Full::new(Bytes::new())).map_err(Error::transport)?;
        let resp = net::request(ep, req).await?;
        let status = resp.status().as_u16();
        if status != 200 && status != 206 {
            let body = net::body(resp).await?;
            return Err(Client::problem(status, &body));
        }
        let sha = resp
            .headers()
            .get("ubos-content-sha256")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let total: u64 = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = resp.into_body();
        let mut out = Vec::with_capacity(total as usize);
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(Error::transport)?;
            if let Ok(d) = frame.into_data() {
                out.extend_from_slice(&d);
                if let Some(p) = on_progress.as_mut() {
                    p(out.len() as u64, total);
                }
            }
        }
        if range.is_none()
            && let Some(s) = sha
            && hex(&Sha256::digest(&out)) != s
        {
            return Err(Error::new(
                "NOD.BLOB_UNAVAILABLE",
                "the downloaded bytes do not match the file's SHA-256",
                json!({"integrity": true}),
            ));
        }
        Ok(out)
    }
}
