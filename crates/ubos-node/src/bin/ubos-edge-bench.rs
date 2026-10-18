//! `ubos-edge-bench` — SUITE-NOD-BENCH on the workstation (VER-BENCH-2000, EXIT-1-17; PH-1
//! step 19c, DEC-085): the PH-1 performance targets measured through the edge of a Server on
//! PostgreSQL, over UBTP on WebSocket and HTTP.
//!
//! ```text
//! ubos-edge-bench --database URL [--objects N] [--rounds R] [--seconds S] [--quick] [--report <file>]
//! ```
//!
//! One Server in this process serves three fresh tenants of the node's registry, each with
//! the VAE `acme` and the classes below:
//!
//! 1. **Indexed queries** (MET-PERF-016, NR-PERF-008): the dataset of `ubos-bench query`
//!    (`Account` with the indexed `country` and `balance` and the unique key `number`; 13d,
//!    DEC-074) loaded on `feature/bench`, and the branch `feature/policy` with a policy that
//!    denies one country in ten. The first page (`limit 100`) of `eq`, `page2`, `range` and
//!    `key`, `eq_filter` (a mostly permitted page) and `eq_denied` (a page the policy denies
//!    entirely), each in the kernel (a second engine on the same store), over WebSocket and
//!    over HTTP; the difference is the cost of the edge.
//! 2. **WL-MIXED-OLTP** (MET-PERF-017, NR-PERF-011): 70 % reads, 20 % commits of 1–10
//!    objects, 5 % queries, 5 % ledger appends from WebSocket sessions. The capacity is the
//!    throughput of the sessions unpaced; the measurement runs at 70 % of it, with commit to
//!    handler start sampled on a subscription (the handler's commit time minus the ping's,
//!    kernel times) and the subscription latency of a UBTP `target` subscription (from the
//!    commit request to the emit at another session; NR-PERF-015, PH-2, recorded).
//! 3. **Fairness** (FR-TEN-024): tenant B's latency under a paced probe while tenant A is idle,
//!    at ten times B's load, flooding unpaced, and flooding with a per-principal request limit.
//!
//! `--quick` runs every part on a small dataset for a few seconds (CI). The report is signed
//! like the other node suites.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use ubos_dvm::crypto;
use ubos_dvm::types::{BpaError, canonical};
use ubos_node::server::{NodeVaes, Preset, Server};

const PW: &str = "correct horse battery";
const BENCH: &str = "feature/bench";
const POLICY: &str = "feature/policy";
const OLTP: &str = "feature/oltp";
const ITEMS_PER_WORKER: usize = 40;

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn uuid() -> String {
    ubos_node::uuid().unwrap_or_default()
}

/// A deterministic pseudo-random stream (xorshift), as `ubos-bench query`.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if v.is_empty() {
        return f64::NAN;
    }
    let i = ((p / 100.0) * (v.len() as f64 - 1.0)).round() as usize;
    (v[i.min(v.len() - 1)] * 100.0).round() / 100.0
}

/// Latencies by operation kind, operations, errors and seconds of a group of workers.
type MixResult = (BTreeMap<&'static str, Vec<f64>>, u64, u64, f64);

/// Latency statistics in ms.
fn stats(mut v: Vec<f64>) -> Value {
    let n = v.len();
    let mean = if n > 0 {
        v.iter().sum::<f64>() / n as f64
    } else {
        f64::NAN
    };
    json!({"n": n, "p50": pct(&mut v, 50.0), "p95": pct(&mut v, 95.0), "p99": pct(&mut v, 99.0),
        "max": pct(&mut v, 100.0), "mean": (mean * 100.0).round() / 100.0})
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

// ---- the model ----

fn field(name: &str, ty: &str, required: bool) -> Value {
    json!({"name": name, "label": name, "description": name, "type": ty, "required": required})
}

fn port(name: &str, params: &[&str], emits: &[&str]) -> Value {
    json!({"name": name, "description": name, "kind": "command", "impl": format!("bench.item#{name}"),
        "permission": name, "params": params.iter().map(|x| field(x, "text", false)).collect::<Vec<_>>(),
        "emits": emits})
}

/// `Account` of the query dataset, `Item` of the mixed workload with its ports, and the
/// ledger `Movement`.
fn classes() -> Value {
    json!([
        {"name": "Account", "label": "Account", "description": "Account", "namespace": "bench.edge", "parent": "DataObject",
         "kind": "definition", "schema_version": 1, "keys": [["number"]],
         "fields": [field("number", "text", true), field("name", "text", true), field("country", "text", true),
                    field("balance", "decimal", true), field("region", "text", false), field("note", "text", false)]},
        {"name": "Item", "label": "Item", "description": "Item", "namespace": "bench.edge", "parent": "DataObject",
         "kind": "definition", "schema_version": 1,
         "events": [{"name": "Pinged", "description": "an item was pinged"}],
         "fields": [field("name", "text", true), field("owner", "text", true), field("qty", "integer", false),
                    field("tag", "text", false)],
         "ports": [port("ping", &["n"], &["Pinged"]), port("mark", &["tag"], &[])]},
        {"name": "Movement", "label": "Movement", "description": "Movement", "namespace": "bench.edge", "parent": "DataObject",
         "kind": "ledger", "schema_version": 1, "ledger": {"key": ["account"], "amounts": ["amount"], "double_entry": false},
         "fields": [field("account", "text", true), field("amount", "money", true), field("memo", "text", false)]}
    ])
}

/// The logic of the ports, committed on the work branch.
fn logic() -> Value {
    json!({"op": "create", "class": "Logic", "object_id": uuid(), "slug": "bench-item", "body": {
    "abi": "1.0", "capabilities": ["commit", "read", "emit"], "exports": ["ping", "mark"],
    "name": "bench.item", "profile": "standard", "state": "committed", "tier": "host",
    "source": {"functions": {
        "ping": {"params": ["n"], "body": [{"do": "read.get", "args": {"target": "self_id"}},
            {"do": "commit.update", "args": {"target": "self_id", "set": {"tag": "params.n"}}},
            {"do": "emit.event", "args": {"type": "\"Pinged\"", "subject": "self_id", "payload": {"n": "params.n"}}},
            {"return": "\"pinged\""}]},
        "mark": {"params": ["tag"], "body": [{"do": "read.get", "args": {"target": "self_id"}},
            {"do": "commit.update", "args": {"target": "self_id", "set": {"tag": "params.tag"}}},
            {"return": "params.tag"}]}
    }}}})
}

fn account(k: u64, r: u64) -> Value {
    json!({"op": "create", "class": "Account", "object_id": format!("01890a5d-ac96-7000-8000-{k:012x}"), "slug": format!("a-{k}"),
        "body": {"number": format!("A-{k:09}"), "name": format!("Account {k}"),
            "country": format!("C{:02}", r % 100), "balance": format!("{}.{:02}", (r >> 8) % 10_000, (r >> 24) % 100),
            "region": format!("R{}", (r >> 32) % 7), "note": "bench"}})
}

/// The items of the mixed workload: `workers` owners with `ITEMS_PER_WORKER` items each,
/// plus the probe item of the subscription latency; slug → object id.
fn items(workers: usize) -> (Vec<Value>, BTreeMap<String, String>) {
    let mut changes = Vec::new();
    let mut ids = BTreeMap::new();
    let mut slugs: Vec<(String, String)> = (0..workers)
        .flat_map(|w| (0..ITEMS_PER_WORKER).map(move |k| (format!("it-{w}-{k}"), format!("w{w}"))))
        .collect();
    slugs.push(("probe".into(), "probe".into()));
    slugs.extend((0..400).map(|k| (format!("ping-{k}"), "ping".to_string())));
    for (slug, owner) in slugs {
        let id = uuid();
        changes.push(
            json!({"op": "create", "class": "Item", "object_id": id, "slug": slug,
            "body": {"name": slug, "owner": owner, "qty": 0}}),
        );
        ids.insert(slug, id);
    }
    (changes, ids)
}

// ---- clients ----

/// A UBTP session over WebSocket.
struct Ws {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    n: u64,
    /// Emits received while waiting for results: (time, frame).
    emits: Vec<(Instant, Value)>,
}

impl Ws {
    async fn open(addr: &str, tenant: &str, token: &str) -> Result<Ws, BpaError> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut req = format!("ws://{addr}/ubtp/v1?tenant={tenant}")
            .into_client_request()
            .map_err(err)?;
        req.headers_mut()
            .insert("Sec-WebSocket-Protocol", "ubtp.v1".parse().map_err(err)?);
        let (ws, _) = tokio_tungstenite::connect_async(req).await.map_err(err)?;
        let mut s = Ws {
            ws,
            n: 0,
            emits: Vec::new(),
        };
        s.request(
            "handshake",
            json!({"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": token}}),
        )
        .await?;
        Ok(s)
    }

    /// Send one request and wait for its result (`Err` for an error frame); emits that
    /// arrive meanwhile are kept.
    async fn request(&mut self, ty: &str, body: Value) -> Result<Value, BpaError> {
        self.request_within(ty, body, 120.0).await
    }

    async fn request_within(
        &mut self,
        ty: &str,
        body: Value,
        secs: f64,
    ) -> Result<Value, BpaError> {
        use tokio_tungstenite::tungstenite::Message;
        self.n += 1;
        let id = format!("r{}", self.n);
        let m = json!({"v": "1.0", "id": id, "type": ty, "body": body});
        self.ws
            .send(Message::Text(m.to_string().into()))
            .await
            .map_err(err)?;
        loop {
            let f = match tokio::time::timeout(Duration::from_secs_f64(secs), self.ws.next()).await
            {
                Ok(Some(Ok(f))) => f,
                _ => return Err(err(format!("{ty}: no answer"))),
            };
            let Ok(t) = f.to_text() else { continue };
            let v: Value = serde_json::from_str(t).unwrap_or(Value::Null);
            if v["type"] == "emit" {
                self.emits.push((Instant::now(), v));
                continue;
            }
            if v["reply_to"] != json!(id) {
                continue;
            }
            if v["type"] == "error" {
                return Err(BpaError::new("INT.PROTOCOL_ERROR", v["body"].to_string()));
            }
            return Ok(v["body"].clone());
        }
    }

    /// Wait for the next emit (up to `secs`).
    async fn emit(&mut self, secs: f64) -> Option<(Instant, Value)> {
        if !self.emits.is_empty() {
            return Some(self.emits.remove(0));
        }
        let until = Instant::now() + Duration::from_secs_f64(secs);
        while Instant::now() < until {
            let left = until.saturating_duration_since(Instant::now());
            let f = match tokio::time::timeout(left, self.ws.next()).await {
                Ok(Some(Ok(f))) => f,
                _ => return None,
            };
            let Ok(t) = f.to_text() else { continue };
            let v: Value = serde_json::from_str(t).unwrap_or(Value::Null);
            if v["type"] == "emit" {
                return Some((Instant::now(), v));
            }
        }
        None
    }
}

/// A UBTP session over HTTP on one kept-alive connection (`Ubtp-Session`).
struct Http {
    r: BufReader<std::net::TcpStream>,
    w: std::net::TcpStream,
    session: Option<String>,
    tenant: String,
    n: u64,
}

impl Http {
    fn connect(addr: &str, tenant: &str) -> Result<Http, BpaError> {
        let s = std::net::TcpStream::connect(addr).map_err(err)?;
        s.set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(err)?;
        s.set_nodelay(true).map_err(err)?;
        Ok(Http {
            r: BufReader::new(s.try_clone().map_err(err)?),
            w: s,
            session: None,
            tenant: tenant.to_string(),
            n: 0,
        })
    }

    /// One request on the kept-alive connection: status, headers, body.
    fn exchange(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(&str, String)],
        body: &str,
    ) -> Result<(u16, BTreeMap<String, String>, String), BpaError> {
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: keep-alive\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (k, v) in headers {
            head += &format!("{k}: {v}\r\n");
        }
        head += "\r\n";
        self.w.write_all(head.as_bytes()).map_err(err)?;
        self.w.write_all(body.as_bytes()).map_err(err)?;
        let mut line = String::new();
        self.r.read_line(&mut line).map_err(err)?;
        let status = line
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let mut hs = BTreeMap::new();
        loop {
            line.clear();
            self.r.read_line(&mut line).map_err(err)?;
            let l = line.trim_end();
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                hs.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let mut out = Vec::new();
        if hs.get("transfer-encoding").is_some_and(|t| t == "chunked") {
            loop {
                line.clear();
                self.r.read_line(&mut line).map_err(err)?;
                let n = usize::from_str_radix(line.trim(), 16).unwrap_or(0);
                let mut chunk = vec![0u8; n + 2];
                self.r.read_exact(&mut chunk).map_err(err)?;
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&chunk[..n]);
            }
        } else {
            let n: usize = hs
                .get("content-length")
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            out = vec![0u8; n];
            self.r.read_exact(&mut out).map_err(err)?;
        }
        Ok((status, hs, String::from_utf8_lossy(&out).to_string()))
    }

    fn request(&mut self, ty: &str, body: Value) -> Result<Value, BpaError> {
        self.n += 1;
        let id = format!("h{}", self.n);
        let m = json!({"v": "1.0", "id": id, "type": ty, "body": body});
        let mut headers = vec![("Ubtp-Tenant", self.tenant.clone())];
        if let Some(s) = &self.session {
            headers.push(("Ubtp-Session", s.clone()));
        }
        let (_, hs, text) = self.exchange("POST", "/ubtp/v1", &headers, &m.to_string())?;
        if let Some(s) = hs.get("ubtp-session") {
            self.session = Some(s.clone());
        }
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let f = v["responses"]
            .as_array()
            .and_then(|a| a.iter().find(|f| f["reply_to"] == json!(id)))
            .cloned()
            .ok_or_else(|| err(format!("{ty}: {text}")))?;
        if f["type"] == "error" {
            return Err(BpaError::new("INT.PROTOCOL_ERROR", f["body"].to_string()));
        }
        Ok(f["body"].clone())
    }

    fn rest(
        &mut self,
        method: &str,
        path: &str,
        token: &str,
        body: &Value,
    ) -> Result<(u16, Value), BpaError> {
        let b = if body.is_null() {
            String::new()
        } else {
            body.to_string()
        };
        let (st, _, text) = self.exchange(
            method,
            path,
            &[
                ("Authorization", format!("Bearer {token}")),
                ("Ubtp-Tenant", self.tenant.clone()),
            ],
            &b,
        )?;
        Ok((st, serde_json::from_str(&text).unwrap_or(Value::Null)))
    }
}

// ---- the environment ----

struct Tenant {
    slug: String,
    token: String,
    did: String,
    ids: BTreeMap<String, String>,
}

struct Env {
    rt: tokio::runtime::Runtime,
    addr: String,
    node: Arc<ubos_node::Node>,
    edge: Tenant,
    edge_t: ubos_node::Tenant,
    fa: Tenant,
    fb: Tenant,
    /// The subscription whose handler marks a pinged item.
    on_ping: String,
    load_s: f64,
}

/// A fresh tenant with the bench classes; on `with_dataset`, the query dataset of
/// `objects` accounts, its indexes and the policy branch.
fn seed(
    node: &ubos_node::Node,
    slug: &str,
    workers: usize,
    objects: u64,
) -> Result<(ubos_node::Tenant, BTreeMap<String, String>, f64), BpaError> {
    let t = node.create_tenant(slug)?;
    node.create_vae(&t, "acme")?;
    let mut v = node.open_vae(&t, "acme")?;
    v.call("load_genesis", &json!({"classes": classes()}))?;
    v.call("declare_index", &json!({"class": "Item", "field": "owner"}))?;
    v.call("create_branch", &json!({"name": OLTP, "from": "main"}))?;
    let (changes, ids) = items(workers);
    v.call("commit", &json!({"branch": OLTP, "changes": changes}))?;
    let mut load_s = 0.0;
    if objects > 0 {
        v.call(
            "declare_index",
            &json!({"class": "Account", "field": "country"}),
        )?;
        v.call(
            "declare_index",
            &json!({"class": "Account", "field": "balance"}),
        )?;
        v.call("create_branch", &json!({"name": BENCH, "from": "main"}))?;
        let mut rng = Rng(20_261_015 | 1);
        let t0 = Instant::now();
        let mut i = 0u64;
        while i < objects {
            let n = 5_000.min(objects - i);
            let changes: Vec<Value> = (i..i + n).map(|k| account(k, rng.next())).collect();
            v.call("commit", &json!({"branch": BENCH, "changes": changes}))?;
            i += n;
            if i.is_multiple_of(50_000) {
                eprintln!(
                    "  loaded {i} accounts ({:.0} s)",
                    t0.elapsed().as_secs_f64()
                );
            }
        }
        load_s = t0.elapsed().as_secs_f64();
        // the logic and one item on the dataset's branch: a port call there runs against
        // the branch of `objects` objects
        v.call(
            "commit",
            &json!({"branch": BENCH, "changes": [logic(),
            {"op": "create", "class": "Item", "object_id": uuid(), "slug": "big-probe",
             "body": {"name": "big-probe", "owner": "probe", "qty": 0}}]}),
        )?;
        v.call("create_branch", &json!({"name": POLICY, "from": BENCH}))?;
        v.call("commit", &json!({"branch": POLICY, "changes": [{"op": "create", "class": "Policy",
            "object_id": uuid(), "slug": "readable", "body": {"name": "readable", "description": "bench",
            "source": "permit(principal, action, resource) when { !(resource is UBOS::Model::Account) || (resource has country && !(resource.country like \"C*0\")) };"}}]}))?;
    }
    Ok((t, ids, load_s))
}

fn setup(url: &str, objects: u64, workers: usize) -> Result<Env, BpaError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let node = Arc::new(ubos_node::Node::connect(url)?);
    let run = &uuid().replace('-', "")[..10];
    let (es, fas, fbs) = (format!("edge{run}"), format!("fa{run}"), format!("fb{run}"));
    eprintln!("seeding {es} with {objects} accounts");
    let (edge_t, edge_ids, load_s) = seed(&node, &es, workers, objects)?;
    let (_, fa_ids, _) = seed(&node, &fas, workers, 0)?;
    let (_, fb_ids, _) = seed(&node, &fbs, workers, 0)?;
    let server = Server::new(
        Box::new(NodeVaes(node.clone())),
        Preset::Server,
        &es,
        "acme",
    );
    let mut dids = Vec::new();
    for t in [&es, &fas, &fbs] {
        dids.push(
            server
                .identity(t)?
                .bootstrap_admin("root", PW)?
                .unwrap_or_default(),
        );
    }
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .map_err(err)?;
    let addr = listener.local_addr().map_err(err)?.to_string();
    rt.spawn(server.serve_with(listener));
    let sign_in = |t: &str| -> Result<String, BpaError> {
        let mut h = Http::connect(&addr, t)?;
        let (st, _, text) = h.exchange(
            "POST",
            "/api/v1/identity/sign-in",
            &[("Ubtp-Tenant", t.to_string())],
            &json!({"username": "root", "password": PW}).to_string(),
        )?;
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if st != 200 {
            return Err(err(format!("sign-in {t}: {text}")));
        }
        Ok(v["token"].as_str().unwrap_or_default().to_string())
    };
    let tenant =
        |slug: &str, did: &str, ids: BTreeMap<String, String>| -> Result<Tenant, BpaError> {
            Ok(Tenant {
                slug: slug.to_string(),
                token: sign_in(slug)?,
                did: did.to_string(),
                ids,
            })
        };
    let edge = tenant(&es, &dids[0], edge_ids)?;
    let fa = tenant(&fas, &dids[1], fa_ids)?;
    let fb = tenant(&fbs, &dids[2], fb_ids)?;
    // the logic, a service principal and the subscription of `Pinged` (as SUITE-NOD-EVENTS)
    let mut h = Http::connect(&addr, &es)?;
    h.request(
        "handshake",
        json!({"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": edge.token}}),
    )?;
    h.request(
        "mutate",
        json!({"vae": "acme", "branch": OLTP, "commit": {"changes": [logic()]}}),
    )?;
    let (st, svc) = h.rest(
        "POST",
        "/api/v1/identity/principals",
        &edge.token,
        &json!({"kind": "service", "name": "bench-worker"}),
    )?;
    if st != 200 {
        return Err(err(format!("service principal: {svc}")));
    }
    let on_ping = uuid();
    h.request("mutate", json!({"vae": "acme", "branch": OLTP, "commit": {"changes": [
        {"op": "create", "class": "Subscription", "object_id": on_ping, "slug": "sub-on-ping",
         "body": {"name": "on-ping", "event_types": ["Pinged"], "branches": ["feature/*"],
                  "handler": {"kind": "port", "port": "mark", "target": "$event.object_id", "params": {"tag": "$event.payload.n"}},
                  "run_as": svc["did"], "policy": {"max_attempts": 5, "backoff_ms": 100}}}]}}))?;
    Ok(Env {
        rt,
        addr,
        node,
        edge,
        edge_t,
        fa,
        fb,
        on_ping,
        load_s,
    })
}

// ---- 1. indexed queries ----

/// The queries of one round: (name, branch, text).
fn round_queries(r: u64, objects: u64) -> Vec<(&'static str, &'static str, String)> {
    let seed = 20_261_015u64;
    let c = format!("C{:02}", (seed + r * 7) % 100);
    // a permitted country (not ending in 0) and a denied one
    let cp = format!("C{:02}", ((seed + r * 7) % 100) / 10 * 10 + 1 + r % 9);
    let cd = format!("C{:02}", (r % 10) * 10);
    let lo = ((seed + r * 13) % 99) * 100;
    let k = (seed + r * 101) % objects.max(1);
    let eq = |c: &str| {
        format!("from Account where this.country == \"{c}\" order by this.country limit 100")
    };
    vec![
        ("eq", BENCH, eq(&c)),
        (
            "range",
            BENCH,
            format!(
                "from Account where this.balance >= {lo}.00 and this.balance < {}.00 order by this.balance limit 100",
                lo + 100
            ),
        ),
        (
            "key",
            BENCH,
            format!("from Account where this.number == \"A-{k:09}\" limit 100"),
        ),
        ("eq_filter", POLICY, eq(&cp)),
        ("eq_denied", POLICY, eq(&cd)),
    ]
}

fn rows(v: &Value) -> usize {
    v["items"]
        .as_array()
        .or_else(|| v["objects"].as_array())
        .map_or(0, Vec::len)
}

fn bench_query(e: &Env, objects: u64, rounds: u64) -> Result<Value, BpaError> {
    let mut lat: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut nrows: BTreeMap<String, usize> = BTreeMap::new();
    let mut errors: BTreeMap<String, String> = BTreeMap::new();
    let mut note = |k: String,
                    t: f64,
                    r: Result<Value, BpaError>,
                    lat: &mut BTreeMap<String, Vec<f64>>|
     -> Option<Value> {
        match r {
            Ok(v) => {
                lat.entry(k.clone()).or_default().push(t);
                *nrows.entry(k).or_default() += rows(&v);
                Some(v)
            }
            Err(x) => {
                errors.insert(k, x.to_string());
                None
            }
        }
    };
    // the kernel: a second engine on the same store, with the administrator as principal
    let mut v = e.node.open_vae(&e.edge_t, "acme")?;
    let mut ws =
        e.rt.block_on(Ws::open(&e.addr, &e.edge.slug, &e.edge.token))?;
    let mut http = Http::connect(&e.addr, &e.edge.slug)?;
    http.request(
        "handshake",
        json!({"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": e.edge.token}}),
    )?;
    let denied_rounds = rounds.min(10);
    for r in 0..rounds {
        for (name, branch, text) in round_queries(r, objects) {
            if name == "eq_denied" && r >= denied_rounds {
                continue;
            }
            // kernel
            let t = Instant::now();
            let k = v.call("query", &json!({"branch": branch, "text": text, "profile": "standard", "principal": e.edge.did}));
            let k = note(format!("kernel/{name}"), ms(t), k, &mut lat);
            if name == "eq"
                && let Some(cur) = k.as_ref().and_then(|x| x["cursor"].as_str())
            {
                let t = Instant::now();
                let p2 = v.call("query", &json!({"branch": branch, "text": text, "profile": "standard", "principal": e.edge.did, "cursor": cur}));
                note("kernel/page2".into(), ms(t), p2, &mut lat);
            }
            // UBTP over WebSocket and HTTP
            let body = json!({"vae": "acme", "branch": branch, "query": {"text": text, "profile": "standard"}});
            let t = Instant::now();
            let w = e.rt.block_on(ws.request("query", body.clone()));
            let w = note(format!("ws/{name}"), ms(t), w, &mut lat);
            let t = Instant::now();
            let h = http.request("query", body.clone());
            let h = note(format!("http/{name}"), ms(t), h, &mut lat);
            if name == "eq" {
                for (b, first) in [("ws", w), ("http", h)] {
                    let Some(cur) = first
                        .as_ref()
                        .and_then(|x| x["cursor"].as_str())
                        .map(str::to_string)
                    else {
                        continue;
                    };
                    let mut p2 = body.clone();
                    p2["query"]["cursor"] = json!(cur);
                    let t = Instant::now();
                    let r2 = if b == "ws" {
                        e.rt.block_on(ws.request("query", p2))
                    } else {
                        http.request("query", p2)
                    };
                    note(format!("{b}/page2"), ms(t), r2, &mut lat);
                }
            }
        }
    }
    let mut out = serde_json::Map::new();
    for (k, l) in lat {
        let mut s = stats(l);
        s["rows"] = json!(nrows.get(&k).copied().unwrap_or(0));
        out.insert(k, s);
    }
    // the cost of the edge: binding p95 minus kernel p95, per query
    let mut edge = serde_json::Map::new();
    for name in ["eq", "page2", "range", "key", "eq_filter", "eq_denied"] {
        let k = out
            .get(&format!("kernel/{name}"))
            .and_then(|s| s["p95"].as_f64());
        for b in ["ws", "http"] {
            if let (Some(k), Some(x)) = (
                k,
                out.get(&format!("{b}/{name}"))
                    .and_then(|s| s["p95"].as_f64()),
            ) {
                edge.insert(
                    format!("{b}/{name}"),
                    json!(((x - k) * 100.0).round() / 100.0),
                );
            }
        }
    }
    let ordinary = ["eq", "page2", "range", "key", "eq_filter"];
    let met = ordinary.iter().all(|n| {
        ["ws", "http"].iter().all(|b| {
            out.get(&format!("{b}/{n}"))
                .and_then(|s| s["p95"].as_f64())
                .is_some_and(|p| p <= 100.0)
        })
    });
    Ok(
        json!({"objects": objects, "rounds": rounds, "denied_rounds": denied_rounds, "latency_ms": out,
        "edge_p95_ms": edge, "errors": errors, "target": "first 100 rows p95 <= 100 ms (NR-PERF-008)",
        "met_ordinary_pages": met}),
    )
}

// ---- 2. WL-MIXED-OLTP ----

#[derive(Clone)]
struct Mix {
    addr: String,
    tenant: String,
    token: String,
    ids: Arc<BTreeMap<String, String>>,
}

/// One worker of the mixed workload on its own items: closed loop (`per_s` 0) or paced;
/// the latencies per operation kind.
async fn mix_worker(
    m: Mix,
    w: usize,
    per_s: f64,
    gate: Gate,
    reads_only: bool,
) -> Result<(BTreeMap<&'static str, Vec<f64>>, u64), BpaError> {
    let Gate { ready, go, stop } = gate;
    let mut s = Ws::open(&m.addr, &m.tenant, &m.token).await?;
    // the versions of the worker's items: an update names its base (STD-TXN-020)
    let mut base: BTreeMap<String, String> = BTreeMap::new();
    let r = s
        .request("query", json!({"vae": "acme", "branch": OLTP, "query": {"text":
            format!("from Item where this.owner == \"w{w}\" order by this.owner limit 100"), "profile": "standard"}}))
        .await?;
    for it in r["items"].as_array().into_iter().flatten() {
        base.insert(
            it["object_id"].as_str().unwrap_or_default().to_string(),
            version_of(it),
        );
    }
    ready.fetch_add(1, Ordering::SeqCst);
    while !go.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut rng = Rng(0x9e37_79b9 ^ ((w as u64 + 1) * 0x1234_5678));
    let mut lat: BTreeMap<&'static str, Vec<f64>> = BTreeMap::new();
    let (mut errors, mut retries) = (0, 0u64);
    let t0 = Instant::now();
    let mut k = 0u64;
    while !stop.load(Ordering::Relaxed) {
        k += 1;
        let x = rng.next() % 100;
        let item = |r: u64| format!("it-{w}-{}", r % ITEMS_PER_WORKER as u64);
        let (kind, ty, body) = if x < 70 || (reads_only && x < 85) {
            (
                "read",
                "query",
                json!({"vae": "acme", "branch": OLTP, "read": format!("ubos://acme/Item/{}", item(rng.next()))}),
            )
        } else if x < 90 || reads_only {
            // 1–10 distinct items of the worker
            let (n, start) = (1 + rng.next() % 10, rng.next());
            let changes: Vec<Value> = (0..n)
                .map(|j| {
                    let slug = item(start + j);
                    let oid = &m.ids[&slug];
                    json!({"op": "update", "object_id": oid, "base": base[oid], "body": {"name": slug, "owner": format!("w{w}"), "qty": k as i64, "tag": format!("t{k}")}})
                })
                .collect();
            (
                "commit",
                "mutate",
                json!({"vae": "acme", "branch": OLTP, "commit": {"changes": changes}}),
            )
        } else if x < 95 {
            (
                "query",
                "query",
                json!({"vae": "acme", "branch": OLTP, "query": {"text": format!("from Item where this.owner == \"w{w}\" order by this.owner limit 20"), "profile": "standard"}}),
            )
        } else {
            (
                "append",
                "mutate",
                json!({"vae": "acme", "branch": OLTP, "commit": {"changes": [{"op": "append", "class": "Movement", "entry_id": uuid(),
                "ledger_key": {"account": format!("acct-{w}")}, "body": {"account": format!("acct-{w}"), "amount": {"amount": "1.00", "currency": "USD"}, "memo": "bench"},
                "valid_at": "2026-10-18"}]}}),
            )
        };
        let t = Instant::now();
        // a commit that loses the race for the branch head is retried with fresh bases, as
        // a client does (DSN-DVM-513); its latency includes the retries
        let mut r = s.request(ty, body.clone()).await;
        let mut tries = 0;
        while kind == "commit"
            && tries < 5
            && r.as_ref()
                .is_err_and(|e| e.message.contains("TXN.HEAD_MOVED"))
        {
            tries += 1;
            retries += 1;
            let mut b = body.clone();
            for c in b["commit"]["changes"].as_array_mut().into_iter().flatten() {
                let oid = c["object_id"].as_str().unwrap_or_default().to_string();
                let slug = m
                    .ids
                    .iter()
                    .find(|(_, v)| **v == oid)
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default();
                if let Ok(x) = s.request("query", json!({"vae": "acme", "branch": OLTP, "read": format!("ubos://acme/Item/{slug}")})).await {
                    c["base"] = json!(version_of(&x));
                    base.insert(oid, version_of(&x));
                }
            }
            r = s.request(ty, b).await;
        }
        match r {
            Ok(r) => {
                lat.entry(kind).or_default().push(ms(t));
                for (oid, vh) in r["versions"].as_object().into_iter().flatten() {
                    if let (Some(b), Some(vh)) = (base.get_mut(oid), vh.as_str()) {
                        *b = vh.to_string();
                    }
                }
            }
            Err(e) => {
                errors += 1;
                if errors <= 3 {
                    eprintln!("  worker {w} {kind}: {e}");
                }
            }
        }
        s.emits.clear();
        if per_s > 0.0 {
            let due = Duration::from_secs_f64(k as f64 / per_s);
            if let Some(d) = due.checked_sub(t0.elapsed()) {
                tokio::time::sleep(d).await;
            }
        }
    }
    lat.entry("retries")
        .or_default()
        .extend((0..retries).map(|_| 0.0));
    Ok((lat, errors))
}

/// The start gate of a group of workers: each opens its session and reads its bases, then
/// waits for `go`; the clock starts when all are ready.
#[derive(Clone)]
struct Gate {
    ready: Arc<std::sync::atomic::AtomicUsize>,
    go: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Gate {
    fn new() -> Gate {
        Gate {
            ready: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            go: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
    fn open_when(&self, n: usize) {
        let t = Instant::now();
        while self.ready.load(Ordering::SeqCst) < n && t.elapsed() < Duration::from_secs(600) {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.go.store(true, Ordering::SeqCst);
    }
}

/// Run `workers` mixed-workload sessions for `secs`; (latencies by kind, operations, errors).
fn run_mix(
    e: &Env,
    m: &Mix,
    workers: usize,
    per_s: f64,
    secs: f64,
    reads_only: bool,
) -> Result<MixResult, BpaError> {
    let gate = Gate::new();
    let handles: Vec<_> = (0..workers)
        .map(|w| {
            e.rt.spawn(mix_worker(m.clone(), w, per_s, gate.clone(), reads_only))
        })
        .collect();
    gate.open_when(workers);
    let t0 = Instant::now();
    std::thread::sleep(Duration::from_secs_f64(secs));
    gate.stop.store(true, Ordering::Relaxed);
    let elapsed = t0.elapsed().as_secs_f64();
    let mut lat: BTreeMap<&'static str, Vec<f64>> = BTreeMap::new();
    let (mut ops, mut errors) = (0u64, 0u64);
    for h in handles {
        let (l, x) = e.rt.block_on(h).map_err(err)??;
        errors += x;
        for (k, v) in l {
            if k != "retries" {
                ops += v.len() as u64;
            }
            lat.entry(k).or_default().extend(v);
        }
    }
    Ok((lat, ops, errors, elapsed))
}

/// Commit to handler start on the `Pinged` subscription, one ping at a time, while `stop`
/// is not set: (latencies in ms by kernel commit times, deliveries missing after 10 s).
fn handler_samples(
    e: &Env,
    stop: Arc<AtomicBool>,
    max: usize,
) -> Result<(Vec<f64>, usize), BpaError> {
    let mut ws =
        e.rt.block_on(Ws::open(&e.addr, &e.edge.slug, &e.edge.token))?;
    let mut http = Http::connect(&e.addr, &e.edge.slug)?;
    let mut pairs = Vec::new();
    let mut missing = 0;
    let mut i = 0;
    while !stop.load(Ordering::Relaxed) && i < max {
        let slug = format!("ping-{}", i % 400);
        let r = e.rt.block_on(ws.request("mutate", json!({"vae": "acme", "branch": OLTP,
            "port": {"target": format!("ubos://acme/Item/{slug}"), "name": "ping", "params": {"n": format!("{i:05}")}}})))?;
        ws.emits.clear();
        i += 1;
        let pid = r["process_id"].as_str().unwrap_or_default().to_string();
        let effect = format!("{pid}/1");
        let job = ubos_node::platform::delivery_job_id(&e.on_ping, &effect)?;
        let t0 = Instant::now();
        let done = loop {
            let (_, j) = http.rest(
                "GET",
                &format!("/api/v1/admin/jobs/{job}"),
                &e.edge.token,
                &Value::Null,
            )?;
            if j["state"] == "done" {
                break true;
            }
            if t0.elapsed() > Duration::from_secs(10) {
                break false;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        if done {
            pairs.push((
                pid,
                ubos_node::platform::delivery_process_id(&e.on_ping, &effect)?,
            ));
        } else {
            missing += 1;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let mut v = e.node.open_vae(&e.edge_t, "acme")?;
    let mut tx = |pid: &str| -> Result<chrono::DateTime<chrono::Utc>, BpaError> {
        let info = v.call("process_info", &json!({"process_id": pid}))?;
        let cseq = info["commits"][0]["cseq"]
            .as_u64()
            .ok_or_else(|| err(format!("no commit of {pid}")))?;
        let c = v.call("commit_info", &json!({"branch": OLTP, "time": cseq}))?;
        ubos_node::schedule::parse(c["tx_time"].as_str().unwrap_or_default())
    };
    let mut lat = Vec::new();
    for (ping, handler) in pairs {
        let d = tx(&handler)? - tx(&ping)?;
        lat.push(d.num_microseconds().unwrap_or(0) as f64 / 1000.0);
    }
    Ok((lat, missing))
}

/// The subscription latency: a session subscribed to the probe item, another committing to
/// it every 250 ms; (from the commit request, from the commit result) to the emit, in ms.
fn subscription_samples(
    e: &Env,
    stop: Arc<AtomicBool>,
) -> Result<(Vec<f64>, Vec<f64>, usize), BpaError> {
    let (addr, slug, token, probe) = (
        e.addr.clone(),
        e.edge.slug.clone(),
        e.edge.token.clone(),
        e.edge.ids["probe"].clone(),
    );
    e.rt.block_on(async move {
        let mut sub = Ws::open(&addr, &slug, &token).await?;
        let s = sub
            .request("subscribe", json!({"vae": "acme", "branch": OLTP, "target": "ubos://acme/Item/probe"}))
            .await?;
        let sid = s["subscription_id"].clone();
        let mut w = Ws::open(&addr, &slug, &token).await?;
        let (mut from_req, mut from_res, mut lost) = (Vec::new(), Vec::new(), 0);
        let r = w
            .request("query", json!({"vae": "acme", "branch": OLTP, "read": "ubos://acme/Item/probe"}))
            .await?;
        let mut vh = version_of(&r);
        let mut k = 0;
        while !stop.load(Ordering::Relaxed) {
            k += 1;
            let t0 = Instant::now();
            let r = w
                .request("mutate", json!({"vae": "acme", "branch": OLTP, "commit": {"changes": [
                    {"op": "update", "object_id": probe, "base": vh, "body": {"name": "probe", "owner": "probe", "qty": k}}]}}))
                .await?;
            vh = r["versions"][&probe].as_str().unwrap_or_default().to_string();
            let t1 = Instant::now();
            let cseq = r["result"]["cseq"].as_u64().unwrap_or(0);
            let mut got = None;
            while let Some((at, f)) = sub.emit(5.0).await {
                let cur = f["body"]["cursor"].as_str().unwrap_or_default().to_string();
                let _ = sub.request("ack", json!({"subscription_id": sid, "cursor": cur})).await;
                sub.emits.retain(|(_, x)| x["type"] == "emit");
                let c = ubos_proto::parse_cursor(&json!(cur)).map(|x| x.1).unwrap_or(0);
                if c >= cseq {
                    got = Some(at);
                    break;
                }
            }
            match got {
                Some(at) => {
                    from_req.push(at.duration_since(t0).as_secs_f64() * 1000.0);
                    from_res.push(at.saturating_duration_since(t1).as_secs_f64() * 1000.0);
                }
                None => lost += 1,
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Ok((from_req, from_res, lost))
    })
}

/// The version hash of a read result.
fn version_of(r: &Value) -> String {
    r.get("items")
        .and_then(|i| i.get(0))
        .unwrap_or(r)
        .get("version_hash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn by_kind(lat: BTreeMap<&'static str, Vec<f64>>) -> Value {
    let mut out = serde_json::Map::new();
    let mut all = Vec::new();
    for (k, v) in lat {
        if k == "retries" {
            out.insert("commit_retries".into(), json!(v.len()));
            continue;
        }
        all.extend(v.iter().copied());
        out.insert(k.to_string(), stats(v));
    }
    out.insert("all".into(), stats(all));
    Value::Object(out)
}

fn bench_mix(e: &Env, workers: usize, secs: f64, samples: usize) -> Result<Value, BpaError> {
    let m = Mix {
        addr: e.addr.clone(),
        tenant: e.edge.slug.clone(),
        token: e.edge.token.clone(),
        ids: Arc::new(e.edge.ids.clone()),
    };
    // idle commit to handler start
    let idle_stop = Arc::new(AtomicBool::new(false));
    let (idle, idle_missing) = handler_samples(e, idle_stop, samples / 4)?;
    // capacity: the sessions unpaced
    let cap_secs = (secs / 3.0).max(3.0);
    let (_, cap_ops, cap_err, cap_t) = run_mix(e, &m, workers, 0.0, cap_secs, false)?;
    let capacity = cap_ops as f64 / cap_t;
    let rate = capacity * 0.7;
    eprintln!("  capacity {capacity:.1} ops/s; measuring at {rate:.1} ops/s for {secs} s");
    // 70 % of capacity with the samplers alongside
    let stop = Arc::new(AtomicBool::new(false));
    let (s1, s2) = (stop.clone(), stop.clone());
    let (hs, ss, mix) = std::thread::scope(|sc| {
        let h = sc.spawn(|| handler_samples(e, s1, samples));
        let s = sc.spawn(|| subscription_samples(e, s2));
        let mix = run_mix(e, &m, workers, rate / workers as f64, secs, false);
        stop.store(true, Ordering::Relaxed);
        (h.join(), s.join(), mix)
    });
    let (lat, ops, errors, t) = mix?;
    let (handler, missing) = hs.map_err(|_| err("handler sampler panicked"))??;
    let (from_req, from_res, lost) = ss.map_err(|_| err("subscription sampler panicked"))??;
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    let mut h = stats(handler);
    h["missing"] = json!(missing);
    let mut idle = stats(idle);
    idle["missing"] = json!(idle_missing);
    let met = missing == 0
        && h["p95"].as_f64().is_some_and(|p| p <= 500.0)
        && h["p99"].as_f64().is_some_and(|p| p <= 2000.0);
    Ok(json!({
        "workers": workers, "mix": "70% reads, 20% commits of 1-10 objects, 5% queries, 5% ledger appends",
        "capacity_ops_per_s": r1(capacity), "capacity_errors": cap_err,
        "target_ops_per_s": r1(rate), "achieved_ops_per_s": r1(ops as f64 / t), "seconds": r1(t), "errors": errors,
        "latency_ms": by_kind(lat),
        "commit_to_handler_ms": {"idle": idle, "loaded": h, "target": "p95 <= 500 ms, p99 <= 2 s (NR-PERF-011)", "met": met,
            "measure": "commit time of the ping to commit time of its handler (kernel tx_time)"},
        "subscription_ms": {"from_request": stats(from_req), "from_result": stats(from_res), "lost": lost,
            "measure": "a target subscription on another session: commit request (or its result) to the emit; NR-PERF-015 (PH-2) recorded"}
    }))
}

// ---- 3. fairness ----

/// B's probe: one session at `per_s` mixed operations (reads and commits) for `secs`, while
/// tenant A runs `a_workers` sessions at `a_per_s` each (0 unpaced; no sessions when 0).
fn fairness_phase(
    e: &Env,
    a: &Mix,
    b: &Mix,
    a_workers: usize,
    a_per_s: f64,
    secs: f64,
) -> Result<Value, BpaError> {
    let gate = Gate::new();
    let handles: Vec<_> = (0..a_workers)
        .map(|w| {
            e.rt.spawn(mix_worker(a.clone(), w, a_per_s, gate.clone(), true))
        })
        .collect();
    gate.open_when(a_workers);
    std::thread::sleep(Duration::from_millis(1500));
    let (lat, ops, errors, t) = run_mix(e, b, 1, 10.0, secs, true)?;
    gate.stop.store(true, Ordering::Relaxed);
    let (mut a_ops, mut a_err) = (0u64, 0u64);
    for h in handles {
        if let Ok(Ok((l, x))) = e.rt.block_on(h) {
            a_ops += l.values().map(|v| v.len() as u64).sum::<u64>();
            a_err += x;
        }
    }
    Ok(
        json!({"a_sessions": a_workers, "a_paced_ops_per_s": a_per_s, "a_ops": a_ops, "a_errors": a_err,
        "b_ops_per_s": ((ops as f64 / t) * 10.0).round() / 10.0, "b_errors": errors, "b_latency_ms": by_kind(lat)}),
    )
}

fn bench_fairness(e: &Env, secs: f64, flood: usize) -> Result<Value, BpaError> {
    let mix = |t: &Tenant| Mix {
        addr: e.addr.clone(),
        tenant: t.slug.clone(),
        token: t.token.clone(),
        ids: Arc::new(t.ids.clone()),
    };
    let (a, b) = (mix(&e.fa), mix(&e.fb));
    let base = fairness_phase(e, &a, &b, 0, 0.0, secs)?;
    let tenfold = fairness_phase(e, &a, &b, 10, 10.0, secs)?;
    let flooded = fairness_phase(e, &a, &b, flood, 0.0, secs)?;
    // a per-principal request limit (DSN-NOD-604) through the operator API
    let mut h = Http::connect(&e.addr, &e.edge.slug)?;
    let (st, r) = h.rest(
        "POST",
        "/api/v1/admin/config",
        &e.edge.token,
        &json!({"document": "version = 1\n[limits]\nrequests_per_second = 20\nburst = 20\n"}),
    )?;
    let limited = if st == 200 {
        let x = fairness_phase(e, &a, &b, flood, 0.0, secs)?;
        let _ = h.rest(
            "POST",
            "/api/v1/admin/config",
            &e.edge.token,
            &json!({"document": "version = 1\n[limits]\nrequests_per_second = 0\n"}),
        );
        x
    } else {
        json!({"skipped": format!("config {st}: {r}")})
    };
    let p95 = |x: &Value| x["b_latency_ms"]["all"]["p95"].as_f64();
    let incr = |x: &Value| match (p95(&base), p95(x)) {
        (Some(b0), Some(b1)) if b0 > 0.0 => json!(((b1 / b0 - 1.0) * 1000.0).round() / 10.0),
        _ => Value::Null,
    };
    Ok(
        json!({"probe": "tenant B: one session, 10 operations per second, 85% reads and 15% commits",
        "baseline": base, "a_tenfold": tenfold, "a_flood": flooded, "a_flood_limited": limited,
        "b_p95_increase_percent": {"a_tenfold": incr(&tenfold), "a_flood": incr(&flooded), "a_flood_limited": incr(&limited)},
        "reference": "NR-PERF-022 (PH-4, ENV-REF-CELL): small-tenant p95 increase <= 30%"}),
    )
}

// ---- 4. port calls and the size of the branch ----

/// A command port on a small branch and on the dataset's branch: a process runs against a
/// view of its branch, so its latency grows with the branch. After the calls on the dataset's
/// branch, the time until a read on the small branch is answered again (the VAE's work
/// behind them).
fn bench_ports(e: &Env, calls: u64) -> Result<Value, BpaError> {
    let mut ws =
        e.rt.block_on(Ws::open(&e.addr, &e.edge.slug, &e.edge.token))?;
    let mut out = serde_json::Map::new();
    for (name, branch, slug) in [
        ("small_branch", OLTP, "it-0-0"),
        ("dataset_branch", BENCH, "big-probe"),
    ] {
        let mut lat = Vec::new();
        let mut errors = Vec::new();
        for r in 0..calls {
            let t = Instant::now();
            let x = e.rt.block_on(ws.request_within(
                "mutate",
                json!({"vae": "acme", "branch": branch,
                    "port": {"target": format!("ubos://acme/Item/{slug}"), "name": "mark", "params": {"tag": format!("p{r}")}}}),
                1800.0,
            ));
            ws.emits.clear();
            match x {
                Ok(_) => lat.push(ms(t)),
                Err(x) => errors.push(x.to_string()),
            }
        }
        let mut st = stats(lat);
        st["errors"] = json!(errors);
        out.insert(name.into(), st);
    }
    let t = Instant::now();
    let r = e.rt.block_on(ws.request_within(
        "query",
        json!({"vae": "acme", "branch": OLTP, "read": "ubos://acme/Item/it-0-1"}),
        1800.0,
    ));
    out.insert(
        "next_read_ms".into(),
        json!({"ms": (ms(t) * 10.0).round() / 10.0, "ok": r.is_ok()}),
    );
    Ok(Value::Object(out))
}

// ---- the run ----

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
    let quick = args.iter().any(|a| a == "--quick");
    let num = |k: &str, d: u64| arg(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let Some(url) = arg("--database") else {
        eprintln!(
            "usage: ubos-edge-bench --database URL [--objects N] [--rounds R] [--seconds S] [--quick] [--report <file>]"
        );
        std::process::exit(2);
    };
    let objects = num("--objects", if quick { 2_000 } else { 300_000 });
    let rounds = num("--rounds", if quick { 5 } else { 50 });
    let secs = num("--seconds", if quick { 4 } else { 60 }) as f64;
    let workers = num("--workers", 8) as usize;
    let t0 = Instant::now();
    let path = arg("--report");
    let save = |report: &Value| {
        if let Some(p) = &path {
            if let Some(d) = std::path::Path::new(p).parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(
                p,
                serde_json::to_string_pretty(&signed(report.clone())).unwrap_or_default() + "\n",
            );
        }
    };
    let run = || -> Result<Value, BpaError> {
        let e = setup(&url, objects, workers)?;
        let mut report = json!({"suite": "SUITE-NOD-BENCH", "item": "VER-BENCH-2000", "quick": quick,
            "environment": {"host": "workstation", "cpus": std::thread::available_parallelism().map_or(0, |n| n.get()),
                "database": "PostgreSQL", "build": if cfg!(debug_assertions) { "debug" } else { "release" }},
            "params": {"objects": objects, "rounds": rounds, "seconds": secs, "workers": workers},
            "load_seconds": (e.load_s * 10.0).round() / 10.0});
        eprintln!("queries ({rounds} rounds)");
        report["query"] = bench_query(&e, objects, rounds)?;
        save(&report);
        eprintln!("mixed workload");
        report["mixed"] = bench_mix(&e, workers, secs, if quick { 8 } else { 200 })?;
        save(&report);
        eprintln!("fairness");
        report["fairness"] = bench_fairness(&e, (secs / 3.0).max(3.0), if quick { 4 } else { 16 })?;
        save(&report);
        eprintln!("ports");
        report["ports"] = bench_ports(&e, if quick { 2 } else { 5 })?;
        report["met"] = json!(
            report["query"]["met_ordinary_pages"] == json!(true)
                && report["mixed"]["commit_to_handler_ms"]["met"] == json!(true)
        );
        report["seconds"] = json!(t0.elapsed().as_secs());
        std::mem::forget(e);
        Ok(report)
    };
    let report = match run() {
        Ok(r) => signed(r),
        Err(e) => {
            eprintln!("ubos-edge-bench: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_default()
    );
    save(&report);
}
