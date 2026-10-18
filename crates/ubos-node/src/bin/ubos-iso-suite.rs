//! `ubos-iso-suite` — SUITE-NOD-ISO (VER-SEC-2000; PH-1 step 19a, DEC-085): the tenant
//! isolation suite of DSN-NOD-406 (FR-TEN-023, NR-SEC-003).
//!
//! ```text
//! ubos-iso-suite --database URL [--report <file>]
//! ```
//!
//! One Server on PostgreSQL serves three tenants: the operator's (the node's default tenant)
//! and two ordinary tenants A and B, each with a VAE `acme`, the same genesis classes and
//! objects with the same identifiers and slugs; B's object bodies, its principals and its
//! files carry a canary. A's administrator, with its session token and with an API key, then
//! tries to reach B through every interface of the PH-1 Server: UBTP over HTTP and WebSocket
//! (the tenant named by header, query and body), the REST facade, the identity API, the
//! operator API, the file service, forged tokens and guessed or stolen session keys. A case
//! passes when the attempt is refused or answers with A's own data, and no response carries
//! B's canary or identifiers. The report is signed like the other node suites.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ubos_dvm::crypto;
use ubos_dvm::types::{BpaError, canonical};
use ubos_node::server::{NodeVaes, Preset, Server};

const PW: &str = "correct horse battery";
const OBJECTS: usize = 20;

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn rand_hex(n: usize) -> String {
    hex(&crypto::random_bytes::<16>().unwrap_or([9u8; 16]))[..n].to_string()
}

/// One HTTP/1.1 request with headers and a raw body: `(status, headers, body)`.
fn http(
    addr: &str,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> Result<(u16, String, String), BpaError> {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(addr).map_err(err)?;
    s.set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(err)?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes()).map_err(err)?;
    s.write_all(body).map_err(err)?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(err)?;
    let raw = String::from_utf8_lossy(&raw).to_string();
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut rest = rest;
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 || tail.len() < n {
                break;
            }
            out.push_str(&tail[..n]);
            rest = tail[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        rest.to_string()
    };
    Ok((status, head.to_string(), body))
}

fn json_headers(token: Option<&str>, tenant: Option<&str>) -> Vec<(&'static str, String)> {
    let mut h = vec![("Content-Type", "application/json".to_string())];
    if let Some(t) = token {
        h.push(("Authorization", format!("Bearer {t}")));
    }
    if let Some(t) = tenant {
        h.push(("Ubtp-Tenant", t.to_string()));
    }
    h
}

fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case(name)
            .then(|| v.trim().to_string())
    })
}

/// A tenant of the suite: its slug, administrator and credentials.
struct Tenant {
    slug: String,
    canary: String,
    token: String,
    api_key: String,
    did: String,
}

fn person(n: usize, canary: &str) -> Value {
    json!({"op": "create", "class": "Person", "object_id": format!("01890a5d-ac96-7a00-8000-{n:012x}"),
        "slug": format!("p{n}"), "body": {"name": format!("{canary} person {n}"), "email": format!("p{n}@{canary}.example")}})
}

fn handshake(credentials: Value) -> Value {
    json!({"v": "1.0", "id": "h", "type": "handshake", "body": {"vae": "acme", "versions": ["1.0"], "credentials": credentials}})
}

fn ubtp(
    addr: &str,
    tenant_header: Option<&str>,
    messages: Value,
) -> Result<(u16, String, String), BpaError> {
    http(
        addr,
        "POST",
        "/ubtp/v1",
        &json_headers(None, tenant_header),
        messages.to_string().as_bytes(),
    )
}

/// The cases and their outcomes.
struct Run {
    b: Vec<String>,
    results: Vec<Value>,
}

impl Run {
    /// Record one attempt: it passes when `refused_or_own` holds and no part of the response
    /// carries a marker of tenant B.
    fn case(&mut self, group: &str, name: &str, outcome: Result<(bool, String), BpaError>) {
        let (ok, detail) = match outcome {
            Ok((own, text)) => {
                let leak = self.b.iter().find(|m| text.contains(m.as_str()));
                match leak {
                    Some(m) => (false, format!("tenant B's marker {m} in the response")),
                    None if !own => (
                        false,
                        format!("not refused: {}", &text[..text.len().min(300)]),
                    ),
                    None => (true, String::new()),
                }
            }
            // a transport failure is not a cross-tenant access, but it is not evidence either
            Err(e) => (false, format!("transport: {e}")),
        };
        self.results.push(
            json!({"group": group, "case": name, "result": if ok { "pass" } else { "fail" },
            "detail": if ok { Value::Null } else { json!(detail) }}),
        );
    }
}

fn setup(url: &str) -> Result<(tokio::runtime::Runtime, String, Tenant, Tenant, String), BpaError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let node = Arc::new(ubos_node::Node::connect(url)?);
    let run = rand_hex(8);
    let (op, a, b) = (
        format!("isoop{run}"),
        format!("isoa{run}"),
        format!("isob{run}"),
    );
    for t in [&op, &a, &b] {
        node.provision(t, "acme", None)?;
    }
    let server = Server::new(
        Box::new(NodeVaes(node.clone())),
        Preset::Server,
        &op,
        "acme",
    );
    let mut users = Vec::new();
    for (t, user) in [(&op, "operator"), (&a, "alice"), (&b, "bob")] {
        let did = server
            .identity(t)?
            .bootstrap_admin(user, PW)?
            .unwrap_or_default();
        users.push(did);
    }
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .map_err(err)?;
    let addr = listener.local_addr().map_err(err)?.to_string();
    rt.spawn(server.serve_with(listener));
    let sign_in = |tenant: &str, user: &str| -> Result<String, BpaError> {
        let (st, _, body) = http(
            &addr,
            "POST",
            "/api/v1/identity/sign-in",
            &json_headers(None, Some(tenant)),
            json!({"username": user, "password": PW})
                .to_string()
                .as_bytes(),
        )?;
        let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if st != 200 {
            return Err(err(format!("sign-in {tenant}: {body}")));
        }
        Ok(v["token"].as_str().unwrap_or_default().to_string())
    };
    let op_token = sign_in(&op, "operator")?;
    let mut tenants = Vec::new();
    for (i, (t, user)) in [(&a, "alice"), (&b, "bob")].into_iter().enumerate() {
        let token = sign_in(t, user)?;
        let did = users[i + 1].clone();
        let (st, _, body) = http(
            &addr,
            "POST",
            &format!("/api/v1/identity/principals/{did}/api-keys"),
            &json_headers(Some(&token), None),
            json!({"name": "suite", "scopes": ["read", "write"]})
                .to_string()
                .as_bytes(),
        )?;
        let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if st != 200 {
            return Err(err(format!("api key {t}: {body}")));
        }
        let canary = format!("CANARY{}{}", t.to_uppercase(), rand_hex(6));
        let changes: Vec<Value> = (1..=OBJECTS).map(|n| person(n, &canary)).collect();
        let msgs = json!([handshake(json!({"kind": "bearer", "token": token})),
            {"v": "1.0", "id": "m", "type": "mutate", "body": {"vae": "acme", "branch": "main", "commit": {"changes": changes}}}]);
        let (_, _, body) = ubtp(&addr, None, msgs)?;
        if !body.contains("\"result\"") || body.contains("\"error\"") {
            return Err(err(format!("seed {t}: {}", &body[..body.len().min(500)])));
        }
        tenants.push(Tenant {
            slug: t.to_string(),
            canary,
            token,
            api_key: v["key"].as_str().unwrap_or_default().to_string(),
            did,
        });
    }
    let b = tenants.pop().expect("B");
    let a = tenants.pop().expect("A");
    Ok((rt, addr, a, b, op_token))
}

/// An answer of the attacker's session: refused (an error frame or status), or a read of
/// A's own object (A's canary, never B's).
fn own_or_refused(text: &str, a: &Tenant) -> bool {
    text.contains("\"error\"")
        || text.contains("IAM.")
        || text.contains(&a.canary)
        || text.contains("QRY.NOT_FOUND")
        // a session of A's own (its principal is A's), answering from A's VAE
        || text.contains(&format!("did:ubos:{}:", a.slug))
}

fn run(url: &str) -> Result<Value, BpaError> {
    let started = Instant::now();
    let (rt, addr, a, b, op_token) = setup(url)?;
    let mut r = Run {
        b: vec![b.canary.clone(), b.did.clone(), "bob".into()],
        results: Vec::new(),
    };
    // B's session keys, to try them from A: a stolen key and the sequential guesses of
    // the earlier counter
    let (_, head, _) = ubtp(
        &addr,
        None,
        json!([handshake(json!({"kind": "bearer", "token": b.token}))]),
    )?;
    let b_session = header_value(&head, "ubtp-session").unwrap_or_default();
    let creds = [
        ("bearer", json!({"kind": "bearer", "token": a.token})),
        ("api_key", json!({"kind": "api_key", "key": a.api_key})),
    ];
    // 1. UBTP over HTTP: A's credentials naming B by header (or not at all), every object,
    // read, history and timeline
    for (cname, cred) in &creds {
        for (sel, header) in [("header", Some(b.slug.as_str())), ("none", None)] {
            for n in 1..=OBJECTS {
                for (kind, body) in [
                    ("read", json!({"read": format!("ubos://acme/Person/p{n}")})),
                    (
                        "history",
                        json!({"history": {"target": format!("ubos://acme/Person/p{n}")}}),
                    ),
                    (
                        "timeline",
                        json!({"timeline": {"target": format!("ubos://acme/Person/p{n}")}}),
                    ),
                ] {
                    let mut q = body;
                    q["vae"] = json!("acme");
                    let msgs = json!([handshake(cred.clone()), {"v": "1.0", "id": "q", "type": "query", "body": q}]);
                    let out =
                        ubtp(&addr, header, msgs).map(|(_, _, t)| (own_or_refused(&t, &a), t));
                    r.case("ubtp-http", &format!("{cname} {sel} {kind} p{n}"), out);
                }
            }
            let msgs = json!([handshake(cred.clone()), {"v": "1.0", "id": "q", "type": "query",
                "body": {"vae": "acme", "query": {"from": "Person"}}}]);
            let out = ubtp(&addr, header, msgs).map(|(_, _, t)| (own_or_refused(&t, &a), t));
            r.case("ubtp-http", &format!("{cname} {sel} query Person"), out);
        }
        // B's tenant named in the handshake body as the VAE
        let msgs = json!([{"v": "1.0", "id": "h", "type": "handshake", "body": {"vae": b.slug, "versions": ["1.0"], "credentials": cred}}]);
        let out = ubtp(&addr, Some(&b.slug), msgs).map(|(_, _, t)| (own_or_refused(&t, &a), t));
        r.case(
            "ubtp-http",
            &format!("{cname} handshake names B as the VAE"),
            out,
        );
    }
    // 2. UBTP over WebSocket: the tenant query parameter and the header
    for (cname, cred) in &creds {
        for (sel, with_header) in [("query", false), ("header", true)] {
            for n in 1..=OBJECTS {
                let out = rt.block_on(ws_read(&addr, &b.slug, with_header, cred.clone(), n));
                let out = out.map(|t| (own_or_refused(&t, &a), t));
                r.case("ubtp-ws", &format!("{cname} {sel} read p{n}"), out);
            }
        }
    }
    // 3. sessions: B's stolen key presented with A's identity, and guessed keys
    for (route, method, path) in [
        ("ubtp", "POST", "/ubtp/v1".to_string()),
        ("facade read", "GET", "/api/v1/acme/Person/p1".to_string()),
        ("facade query", "GET", "/api/v1/acme/Person".to_string()),
    ] {
        for (who, token, tenant) in [
            ("A bearer", Some(a.token.as_str()), None),
            ("A tenant header", None, Some(a.slug.as_str())),
            (
                "A bearer and tenant",
                Some(a.token.as_str()),
                Some(a.slug.as_str()),
            ),
        ] {
            let mut h = json_headers(token, tenant);
            h.push(("Ubtp-Session", b_session.clone()));
            let body = if method == "POST" {
                json!([{"v": "1.0", "id": "q", "type": "query", "body": {"vae": "acme", "read": "ubos://acme/Person/p1"}}]).to_string()
            } else {
                String::new()
            };
            let out = http(&addr, method, &path, &h, body.as_bytes())
                .map(|(st, _, t)| (st >= 400 || own_or_refused(&t, &a), t));
            r.case(
                "session",
                &format!("B's session key on {route} with {who}"),
                out,
            );
        }
    }
    for n in 1..=60 {
        let mut h = json_headers(None, None);
        h.push(("Ubtp-Session", format!("sess-{n}")));
        let body = json!([{"v": "1.0", "id": "q", "type": "query", "body": {"vae": "acme", "read": "ubos://acme/Person/p1"}}]).to_string();
        let out = http(&addr, "POST", "/ubtp/v1", &h, body.as_bytes())
            .map(|(st, _, t)| (st >= 400 || t.contains("unknown session"), t));
        r.case("session", &format!("guessed session key sess-{n}"), out);
    }
    for n in 1..=10 {
        let out = http(
            &addr,
            "GET",
            &format!("/ubtp/v1/stream?session=sess-{n}"),
            &[],
            b"",
        )
        .map(|(st, _, t)| (st == 404, t));
        r.case("session", &format!("guessed stream session sess-{n}"), out);
    }
    // 4. the REST facade: A's credentials naming B
    for (cname, token) in [("bearer", &a.token), ("api_key", &a.api_key)] {
        for n in 1..=OBJECTS {
            let out = http(
                &addr,
                "GET",
                &format!("/api/v1/acme/Person/p{n}"),
                &json_headers(Some(token), Some(&b.slug)),
                b"",
            )
            .map(|(st, _, t)| (st >= 400 || own_or_refused(&t, &a), t));
            r.case("facade", &format!("{cname} read p{n} naming B"), out);
        }
        let out = http(
            &addr,
            "GET",
            "/api/v1/acme/Person",
            &json_headers(Some(token), Some(&b.slug)),
            b"",
        )
        .map(|(st, _, t)| (st >= 400 || own_or_refused(&t, &a), t));
        r.case("facade", &format!("{cname} query naming B"), out);
        let out = http(
            &addr,
            "POST",
            "/api/v1/acme/Person/p1/archive",
            &json_headers(Some(token), Some(&b.slug)),
            b"{}",
        )
        .map(|(st, _, t)| (st >= 400, t));
        r.case("facade", &format!("{cname} port call naming B"), out);
    }
    // 5. forged tokens: B's prefix on A's secret
    for (kind, token) in [("session token", &a.token), ("API key", &a.api_key)] {
        let forged = format!("{}{}", b.slug, &token[token.find('.').unwrap_or(0)..]);
        for n in 1..=OBJECTS {
            let cred = if kind == "API key" {
                json!({"kind": "api_key", "key": forged})
            } else {
                json!({"kind": "bearer", "token": forged})
            };
            let msgs = json!([handshake(cred), {"v": "1.0", "id": "q", "type": "query", "body": {"vae": "acme", "read": format!("ubos://acme/Person/p{n}")}}]);
            let out = ubtp(&addr, None, msgs)
                .map(|(_, _, t)| (t.contains("IAM.DENIED") && !t.contains(&a.canary), t));
            r.case("forged", &format!("{kind} with B's prefix, read p{n}"), out);
        }
        let out = http(
            &addr,
            "GET",
            "/api/v1/identity/principals",
            &json_headers(Some(&forged), None),
            b"",
        )
        .map(|(st, _, t)| (st >= 400, t));
        r.case(
            "forged",
            &format!("{kind} with B's prefix on the identity API"),
            out,
        );
        let out = http(
            &addr,
            "GET",
            "/api/v1/admin/events",
            &json_headers(Some(&forged), None),
            b"",
        )
        .map(|(st, _, t)| (st >= 400, t));
        r.case(
            "forged",
            &format!("{kind} with B's prefix on the operator API"),
            out,
        );
    }
    // 6. the identity API: A's administrator naming B
    let ident: Vec<(&str, String)> = vec![
        ("GET", "/api/v1/identity/principals".into()),
        ("GET", format!("/api/v1/identity/principals/{}", b.did)),
        ("GET", format!("/api/v1/identity/principals/{}/keys", b.did)),
        (
            "GET",
            format!("/api/v1/identity/principals/{}/api-keys", b.did),
        ),
        (
            "POST",
            format!("/api/v1/identity/principals/{}/api-keys", b.did),
        ),
        (
            "POST",
            format!("/api/v1/identity/principals/{}/state", b.did),
        ),
        (
            "POST",
            format!("/api/v1/identity/principals/{}/password", b.did),
        ),
        (
            "POST",
            format!("/api/v1/identity/principals/{}/sign", b.did),
        ),
        ("GET", "/api/v1/identity/security-log".into()),
        ("GET", "/api/v1/identity/security-log/verify".into()),
        ("GET", "/api/v1/identity/security-log/export".into()),
        ("GET", "/api/v1/identity/me".into()),
        ("POST", "/api/v1/identity/principals".into()),
    ];
    for (cname, token) in [("bearer", &a.token), ("api_key", &a.api_key)] {
        for (method, path) in &ident {
            let body = json!({"name": "x", "scopes": ["read"], "state": "disabled", "password": "x".repeat(20),
                "kind": "human", "username": "mallory", "role": "admin", "payload": "aGk"}).to_string();
            // a refusal may echo the DID the caller asked for; that is not B's data
            let out = http(
                &addr,
                method,
                path,
                &json_headers(Some(token), Some(&b.slug)),
                body.as_bytes(),
            )
            .map(|(st, _, t)| {
                let refused = st >= 400 || !t.contains(&b.slug);
                let t = if st >= 400 {
                    t.replace(&b.did, "<requested DID>")
                } else {
                    t
                };
                (refused, t)
            });
            r.case(
                "identity",
                &format!("{cname} {method} {path} naming B"),
                out,
            );
        }
    }
    // 7. the operator API: A's administrator on B's VAE, B's tenant and the node-wide routes
    let admin: Vec<(&str, String, bool)> = vec![
        (
            "GET",
            format!("/api/v1/admin/events?vae=acme&slug={}", b.slug),
            false,
        ),
        ("GET", format!("/api/v1/admin/jobs?slug={}", b.slug), false),
        ("GET", "/api/v1/admin/schedules".into(), false),
        ("GET", "/api/v1/admin/leaders".into(), false),
        ("GET", "/api/v1/admin/listeners".into(), false),
        ("GET", format!("/api/v1/admin/tenant?slug={}", b.slug), true),
        (
            "POST",
            format!("/api/v1/admin/tenant?slug={}", b.slug),
            true,
        ),
        ("POST", "/api/v1/admin/diagnostics".into(), true),
        ("GET", "/api/v1/admin/health".into(), true),
        ("POST", "/api/v1/admin/health/run".into(), true),
        ("GET", "/api/v1/admin/telemetry/spans".into(), true),
        ("GET", "/api/v1/admin/telemetry/metrics".into(), true),
        ("GET", "/api/v1/admin/config".into(), true),
        ("POST", "/api/v1/admin/config".into(), true),
    ];
    for (method, path, must_refuse) in &admin {
        let out = http(
            &addr,
            method,
            path,
            &json_headers(Some(&a.token), Some(&b.slug)),
            b"{}",
        )
        .map(|(st, _, t)| {
            (
                (!must_refuse || st == 403) && (st >= 400 || !t.contains(&b.slug)),
                t,
            )
        });
        r.case("operator", &format!("{method} {path}"), out);
    }
    // 8. the file service: B's upload and objects reached with A's credentials
    let (st, head, _) = http(
        &addr,
        "POST",
        "/files/v1/acme",
        &[
            ("Authorization", format!("Bearer {}", b.token)),
            ("Upload-Length", "11".into()),
            ("Upload-Metadata", "filename aXNvLnR4dA==".into()),
            ("Tus-Resumable", "1.0.0".into()),
        ],
        b"",
    )?;
    let upload = header_value(&head, "location").unwrap_or_default();
    if st == 201 && !upload.is_empty() {
        let _ = http(
            &addr,
            "PATCH",
            &upload,
            &[
                ("Authorization", format!("Bearer {}", b.token)),
                ("Upload-Offset", "0".into()),
                ("Content-Type", "application/offset+octet-stream".into()),
                ("Tus-Resumable", "1.0.0".into()),
            ],
            b"hello world",
        )?;
    }
    for (cname, token) in [("bearer", &a.token), ("api_key", &a.api_key)] {
        for method in ["GET", "HEAD", "PATCH", "DELETE"] {
            let out = http(
                &addr,
                method,
                &upload,
                &[
                    ("Authorization", format!("Bearer {token}")),
                    ("Upload-Offset", "0".into()),
                    ("Content-Type", "application/offset+octet-stream".into()),
                    ("Tus-Resumable", "1.0.0".into()),
                    ("Ubtp-Tenant", b.slug.clone()),
                ],
                b"x",
            )
            .map(|(st, _, t)| (st == 404 || st == 403, format!("{st} {t}")));
            r.case("files", &format!("{cname} {method} B's upload"), out);
        }
        let out = http(
            &addr,
            "POST",
            "/files/v1/acme/links",
            &json_headers(Some(token), Some(&b.slug)),
            json!({"object": "ubos://acme/Person/p1", "field": "photo"})
                .to_string()
                .as_bytes(),
        )
        .map(|(st, _, t)| (st >= 400, t));
        r.case(
            "files",
            &format!("{cname} download link for B's object"),
            out,
        );
    }
    // 9. the operator's isolation self-check: no tenant pool without a tenant
    let out = http(
        &addr,
        "POST",
        "/api/v1/admin/health/run",
        &json_headers(Some(&op_token), None),
        b"{}",
    )
    .map(|(st, _, t)| {
        let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
        let ok = st == 200
            && v.to_string().contains("\"isolation\"")
            && v["checks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["check"] == "isolation")
                .all(|c| c["status"] == "ok");
        // the operator's view holds no tenant data: no marker test applies to it
        (ok, String::new())
    });
    r.case(
        "store",
        "isolation self-check: no tenant pool without a tenant",
        out,
    );
    let cases = r.results.len();
    let passed = r.results.iter().filter(|x| x["result"] == "pass").count();
    let report = json!({
        "suite": "SUITE-NOD-ISO", "items": ["VER-SEC-2000"], "step": "PH-1 step 19a", "decision": "DEC-085",
        "implementation": {"crate": "ubos-node", "version": env!("CARGO_PKG_VERSION")},
        "environment": {"backend": "postgresql", "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)},
        "tenants": {"operator": "default tenant", "a": a.slug, "b": b.slug},
        "cases": cases, "passed": passed, "results": r.results,
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    std::mem::forget(rt);
    Ok(report)
}

async fn ws_read(
    addr: &str,
    b: &str,
    with_header: bool,
    cred: Value,
    n: usize,
) -> Result<String, BpaError> {
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
    let url = if with_header {
        format!("ws://{addr}/ubtp/v1")
    } else {
        format!("ws://{addr}/ubtp/v1?tenant={b}")
    };
    let mut req = url.into_client_request().map_err(err)?;
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", "ubtp.v1".parse().map_err(err)?);
    if with_header {
        req.headers_mut()
            .insert("Ubtp-Tenant", b.parse().map_err(err)?);
    }
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.map_err(err)?;
    let mut out = String::new();
    for m in [
        handshake(cred),
        json!({"v": "1.0", "id": "q", "type": "query", "body": {"vae": "acme", "read": format!("ubos://acme/Person/p{n}")}}),
    ] {
        ws.send(Message::Text(m.to_string().into()))
            .await
            .map_err(err)?;
        match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
            Ok(Some(Ok(f))) => out.push_str(f.to_text().unwrap_or_default()),
            _ => break,
        }
    }
    Ok(out)
}

fn signed(mut report: Value) -> Value {
    let seed: [u8; 32] = crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report).unwrap_or_default();
    let sig = crypto::ed25519_sign(&seed, body.as_bytes());
    report["signature"] =
        json!({"alg": "ed25519", "key": hex(&crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    report
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(url) = arg("--database") else {
        eprintln!("usage: ubos-iso-suite --database URL [--report <file>]");
        std::process::exit(2);
    };
    let report = match run(&url) {
        Ok(r) => signed(r),
        Err(e) => {
            eprintln!("ubos-iso-suite: {e}");
            std::process::exit(1);
        }
    };
    for x in report["results"].as_array().into_iter().flatten() {
        if x["result"] != "pass" {
            println!(
                "FAIL {} {}: {}",
                x["group"].as_str().unwrap_or_default(),
                x["case"].as_str().unwrap_or_default(),
                x["detail"].as_str().unwrap_or_default()
            );
        }
    }
    let mut groups: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for x in report["results"].as_array().into_iter().flatten() {
        let g = groups
            .entry(x["group"].as_str().unwrap_or_default().to_string())
            .or_default();
        g.0 += 1;
        g.1 += usize::from(x["result"] == "pass");
    }
    for (g, (n, p)) in &groups {
        println!("  {g}: {p} of {n}");
    }
    println!(
        "SUITE-NOD-ISO: {} of {} cases pass, 0 cross-tenant accesses required ({} s)",
        report["passed"], report["cases"], report["seconds"]
    );
    if let Some(p) = arg("--report") {
        if let Some(d) = std::path::Path::new(&p).parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(
            &p,
            serde_json::to_string_pretty(&report).unwrap_or_default() + "\n",
        );
    }
    let ok = report["passed"] == report["cases"];
    std::process::exit(if ok { 0 } else { 1 });
}
