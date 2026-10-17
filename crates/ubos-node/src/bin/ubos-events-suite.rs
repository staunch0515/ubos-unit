//! `ubos-events-suite` — SUITE-NOD-EVENTS and SUITE-NOD-JOBS (PH-1 step 15e, DEC-076), and
//! the workstation runs of MET-PERF-017, NR-PERF-012 and NR-DUR-006 (VER-BENCH-2000, NOD
//! part of EXIT-1-17).
//!
//! ```text
//! ubos-events-suite --suite events|jobs [--database URL] [--report <file>]
//! ubos-events-suite --bench [--database URL] [--samples N] [--load F] [--timers N] [--effects N] [--report <file>]
//! ubos-events-suite --serve --database URL --tenant T --listen ADDR   (a node for the fault cases)
//! ```
//!
//! The cases drive a Server in this process (a memory VAE, or a fresh tenant of the node's
//! PostgreSQL registry) through its bindings: UBTP over HTTP for commits and ports, the REST
//! facade for reads and `/api/v1/admin` for the event log, jobs and schedules. The store
//! cases call the stores directly, and the schedule cases evaluate on a simulated clock. The
//! fault cases need PostgreSQL: they run nodes as child processes (this binary with
//! `--serve`), kill them with SIGKILL and restart them.

use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ubos_dvm::crypto;
use ubos_dvm::types::{BpaError, canonical};
use ubos_node::leases::{Leases, MemLeases};
use ubos_node::schedule::{self, MemScheduleStore, PgScheduleStore, ScheduleStore};
use ubos_node::server::{NodeVaes, Preset, Server, VaeHandle, memory_vae_with};
use ubos_node::services::{JobStore, MemEventLog, MemJobQueue, RetryPolicy};

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn check(cond: bool, what: &str) -> Result<(), BpaError> {
    if cond {
        Ok(())
    } else {
        Err(err(format!("check failed: {what}")))
    }
}

fn check_v(cond: bool, what: &str, v: &Value) -> Result<(), BpaError> {
    if cond {
        Ok(())
    } else {
        let s = v.to_string();
        Err(err(format!(
            "check failed: {what}: {}",
            &s[..s.len().min(600)]
        )))
    }
}

const PW: &str = "correct horse battery";
const BR: &str = "feature/ev";

// ---- the VAE of the suites ----

fn field(name: &str, ty: &str, required: bool) -> Value {
    json!({"name": name, "label": name, "description": name, "type": ty, "required": required})
}

fn port(name: &str, params: &[&str], emits: &[&str]) -> Value {
    let mut p = json!({"name": name, "description": name, "kind": "command", "impl": format!("suite.acct#{name}"),
        "permission": name, "params": params.iter().map(|x| field(x, "text", false)).collect::<Vec<_>>()});
    if !emits.is_empty() {
        p["emits"] = json!(emits);
    }
    p
}

/// The genesis classes: `Account` with its ports and events, `Rate` (instant valid time,
/// emitting `BecameEffective`).
fn classes() -> Value {
    json!([
        {"name": "Account", "label": "Account", "description": "Account", "namespace": "suite.events", "parent": "DataObject",
         "kind": "definition", "schema_version": 1,
         "events": [{"name": "AccountClosed", "description": "an account was closed"}, {"name": "Pinged", "description": "an account was pinged"}],
         "fields": [field("name", "text", true), field("status", "text", false), field("tag", "text", false)],
         "ports": [port("close", &[], &["AccountClosed"]), port("ping", &["n"], &["Pinged"]), port("mark", &["tag"], &[]),
                   port("later", &[], &[])]},
        {"name": "Rate", "label": "Rate", "description": "Rate", "namespace": "suite.events", "parent": "DataObject",
         "kind": "definition", "schema_version": 1, "valid_time": "instant", "emit_became_effective": true,
         "fields": [field("value", "decimal", true)]}
    ])
}

/// The logic of the ports (committed on the work branch: protected branches run verified
/// logic only).
fn logic(oid: &str) -> Value {
    json!({"op": "create", "class": "Logic", "object_id": oid, "slug": "suite-acct", "body": {
    "abi": "1.0", "capabilities": ["commit", "read", "run", "emit"], "exports": ["close", "ping", "mark", "later"],
    "name": "suite.acct", "profile": "standard", "state": "committed", "tier": "host",
    "source": {"functions": {
        "close": {"params": [], "body": [{"do": "read.get", "args": {"target": "self_id"}},
            {"do": "commit.update", "args": {"target": "self_id", "set": {"status": "\"closed\""}}},
            {"do": "emit.event", "args": {"type": "\"AccountClosed\"", "subject": "self_id", "payload": {}}},
            {"return": "\"closed\""}]},
        "ping": {"params": ["n"], "body": [{"do": "read.get", "args": {"target": "self_id"}},
            {"do": "commit.update", "args": {"target": "self_id", "set": {"status": "params.n"}}},
            {"do": "emit.event", "args": {"type": "\"Pinged\"", "subject": "self_id", "payload": {"n": "params.n"}}},
            {"return": "\"pinged\""}]},
        "mark": {"params": ["tag"], "body": [{"do": "read.get", "args": {"target": "self_id"}},
            {"do": "commit.update", "args": {"target": "self_id", "set": {"tag": "params.tag"}}},
            {"return": "params.tag"}]},
        "later": {"params": [], "body": [{"do": "run.async", "args": {"target": "self_id", "port": "\"close\"", "params": {}}},
            {"return": "\"scheduled\""}]}
    }}}})
}

fn uuid() -> String {
    ubos_node::uuid().unwrap_or_default()
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
}

fn iso(t: chrono::DateTime<chrono::Utc>) -> String {
    ubos_dvm::types::time::format_instant(&t)
}

// ---- HTTP ----

fn http(
    addr: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &Value,
) -> Result<(u16, Value), BpaError> {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(addr).map_err(err)?;
    s.set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(err)?;
    let b = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\n\r\n{b}", b.len()).map_err(err)?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(err)?;
    let raw = String::from_utf8_lossy(&raw).to_string();
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut rest = body;
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
        body.to_string()
    };
    Ok((status, serde_json::from_str(&body).unwrap_or(Value::Null)))
}

/// A client of one node: its address and an administrator's token.
#[derive(Clone)]
struct Client {
    addr: String,
    token: String,
}

impl Client {
    fn sign_in(addr: &str) -> Result<Client, BpaError> {
        let (st, r) = http(
            addr,
            "POST",
            "/api/v1/identity/sign-in",
            None,
            &json!({"username": "root", "password": PW}),
        )?;
        check_v(st == 200, "sign-in", &r)?;
        Ok(Client {
            addr: addr.to_string(),
            token: r["token"].as_str().unwrap_or_default().to_string(),
        })
    }
    fn get(&self, path: &str) -> Result<Value, BpaError> {
        Ok(http(&self.addr, "GET", path, Some(&self.token), &Value::Null)?.1)
    }
    fn call(&self, method: &str, path: &str, body: &Value) -> Result<(u16, Value), BpaError> {
        http(&self.addr, method, path, Some(&self.token), body)
    }
    /// One UBTP mutate (with its handshake); the mutate's response.
    fn mutate(&self, body: Value) -> Result<Value, BpaError> {
        self.mutate_traced(body, None)
    }
    /// A mutate carrying a W3C trace context in its envelope.
    fn mutate_traced(&self, body: Value, trace: Option<&str>) -> Result<Value, BpaError> {
        let mut m = json!({"v": "1.0", "id": "m", "type": "mutate", "body": body});
        if let Some(t) = trace {
            m["trace"] = json!(t);
        }
        let msgs = json!([{"v": "1.0", "id": "h", "type": "handshake", "body": {"vae": "acme", "versions": ["1.0"],
                "credentials": {"kind": "bearer", "token": self.token}}}, m]);
        let (_, r) = http(&self.addr, "POST", "/ubtp/v1", None, &msgs)?;
        Ok(r["responses"][1].clone())
    }
    fn commit(&self, branch: &str, changes: Value) -> Result<Value, BpaError> {
        let r =
            self.mutate(json!({"vae": "acme", "branch": branch, "commit": {"changes": changes}}))?;
        check_v(r["type"] == "result", "commit", &r)?;
        Ok(r["body"].clone())
    }
    fn port(&self, slug: &str, name: &str, params: Value) -> Result<Value, BpaError> {
        let r = self.mutate(json!({"vae": "acme", "branch": BR, "port": {"target": format!("ubos://acme/Account/{slug}"), "name": name, "params": params}}))?;
        check_v(r["type"] == "result", &format!("port {name}"), &r)?;
        Ok(r["body"].clone())
    }
    fn account(&self, slug: &str) -> Result<Option<Value>, BpaError> {
        let r = self.get(&format!("/api/v1/acme/Account/{slug}?branch={BR}"))?;
        Ok(r["items"].get(0).map(|x| x["body"].clone()))
    }
    fn accounts(&self, slugs: &[String]) -> Result<(), BpaError> {
        let changes: Vec<Value> = slugs
            .iter()
            .map(|s| json!({"op": "create", "class": "Account", "object_id": uuid(), "slug": s, "body": {"name": s, "status": "open"}}))
            .collect();
        self.commit(BR, json!(changes)).map(|_| ())
    }
    fn service(&self, name: &str) -> Result<String, BpaError> {
        let (st, r) = self.call(
            "POST",
            "/api/v1/identity/principals",
            &json!({"kind": "service", "name": name}),
        )?;
        check_v(st == 200, "service principal", &r)?;
        Ok(r["did"].as_str().unwrap_or_default().to_string())
    }
    #[allow(clippy::too_many_arguments)]
    fn subscribe(
        &self,
        name: &str,
        types: &[&str],
        branches: &[&str],
        handler: Value,
        run_as: &str,
        policy: Value,
    ) -> Result<String, BpaError> {
        let id = uuid();
        self.commit(BR, json!([{"op": "create", "class": "Subscription", "object_id": id, "slug": format!("sub-{name}"),
            "body": {"name": name, "event_types": types, "branches": branches, "handler": handler, "run_as": run_as, "policy": policy}}]))?;
        Ok(id)
    }
    /// The events of the log, paged.
    fn events(&self, query: &str) -> Result<Vec<Value>, BpaError> {
        let mut out = Vec::new();
        let mut after = 0;
        loop {
            let r = self.get(&format!(
                "/api/v1/admin/events?limit=1000&after={after}{}{query}",
                if query.is_empty() { "" } else { "&" }
            ))?;
            let items = r["items"].as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                break;
            }
            after = items.last().and_then(|x| x["seq"].as_i64()).unwrap_or(0);
            let n = items.len();
            out.extend(items);
            if n < 1000 {
                break;
            }
        }
        Ok(out)
    }
    fn jobs(&self, query: &str) -> Result<Vec<Value>, BpaError> {
        Ok(
            self.get(&format!("/api/v1/admin/jobs?limit=10000&{query}"))?["items"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        )
    }
    fn log_kinds(&self) -> Result<Vec<String>, BpaError> {
        let r = self.get("/api/v1/identity/security-log?limit=10000")?;
        Ok(r["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["kind"].as_str().map(str::to_string))
            .collect())
    }
}

fn wait<T>(
    secs: f64,
    mut f: impl FnMut() -> Result<Option<T>, BpaError>,
) -> Result<Option<T>, BpaError> {
    let t = Instant::now();
    loop {
        if let Some(x) = f()? {
            return Ok(Some(x));
        }
        if t.elapsed().as_secs_f64() > secs {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---- the environment ----

/// Where the VAE lives.
enum Backend {
    /// One memory VAE (its handle, job queue and event log are the Server's).
    Memory {
        handle: VaeHandle,
        jobs: Arc<MemJobQueue>,
    },
    /// A fresh tenant of the node's registry.
    Pg {
        url: String,
        node: Arc<ubos_node::Node>,
        tenant: ubos_node::Tenant,
    },
}

/// A Server in this process with its client, the suite's service principal and the
/// subscription of `Pinged` events.
struct Env {
    _rt: tokio::runtime::Runtime,
    backend: Backend,
    c: Client,
    svc: String,
    /// The subscription of `Pinged` events.
    on_ping: String,
    n: std::cell::Cell<u64>,
}

impl Env {
    fn slug(&self, p: &str) -> String {
        self.n.set(self.n.get() + 1);
        format!("{p}-{}", self.n.get())
    }
    fn pg(&self) -> Option<(&str, &Arc<ubos_node::Node>, &ubos_node::Tenant)> {
        match &self.backend {
            Backend::Pg { url, node, tenant } => Some((url, node, tenant)),
            Backend::Memory { .. } => None,
        }
    }
    /// A host operation on the VAE, outside the Server's actor on PostgreSQL (a second
    /// engine over the same store, as another node would be).
    fn host(&self, op: &str, input: Value) -> Result<Value, BpaError> {
        match &self.backend {
            Backend::Memory { handle, .. } => handle.call_blocking(op, input),
            Backend::Pg { node, tenant, .. } => {
                let mut v = node.open_vae(tenant, "acme")?;
                match op {
                    "host.deliver" => {
                        ubos_node::platform::deliver(&mut v, &input["job"], &input["principal"])
                    }
                    "host.jobs_open" => ubos_node::platform::open_jobs(&mut v, &Default::default())
                        .map(|j| json!({"items": j})),
                    _ => v.call(op, &input),
                }
            }
        }
    }
}

fn seed_pg(node: &ubos_node::Node, slug: &str) -> Result<ubos_node::Tenant, BpaError> {
    let t = node.create_tenant(slug)?;
    node.create_vae(&t, "acme")?;
    let mut v = node.open_vae(&t, "acme")?;
    v.call("load_genesis", &json!({"classes": classes()}))?;
    v.call("create_branch", &json!({"name": BR, "from": "main"}))?;
    Ok(t)
}

/// What every node of the suites holds: the logic, a service principal and the
/// subscription that marks a pinged account with the ping's number.
fn seed_http(c: &Client) -> Result<(String, String), BpaError> {
    c.commit(BR, json!([logic(&uuid())]))?;
    let svc = c.service("suite-worker")?;
    let sub = c.subscribe(
        "on-ping",
        &["Pinged"],
        &["feature/*"],
        json!({"kind": "port", "port": "mark", "target": "$event.object_id", "params": {"tag": "$event.payload.n"}}),
        &svc,
        json!({"max_attempts": 5, "backoff_ms": 100}),
    )?;
    Ok((svc, sub))
}

fn env(database: Option<&str>) -> Result<Env, BpaError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let (server, backend) = match database {
        None => {
            let (events, jobs) = (
                Arc::new(MemEventLog::default()),
                Arc::new(MemJobQueue::default()),
            );
            let (ev, jb) = (events.clone(), jobs.clone());
            let handle = VaeHandle::spawn(move || {
                let mut v = memory_vae_with(Some(&classes()), Some(ev), Some(jb))?;
                v.call("create_branch", &json!({"name": BR, "from": "main"}))?;
                Ok(v)
            })?;
            let id = Arc::new(ubos_node::identity::Identity::memory("acme", "acme")?);
            id.bootstrap_admin("root", PW)?;
            let s = Server::single_with(
                handle.clone(),
                id,
                Some(events),
                Some(jobs.clone()),
                Preset::Server,
                "acme",
                "acme",
            );
            (s, Backend::Memory { handle, jobs })
        }
        Some(url) => {
            let node = Arc::new(ubos_node::Node::connect(url)?);
            let slug = format!("evs{}", &uuid().replace('-', "")[..12]);
            let t = seed_pg(&node, &slug)?;
            let s = Server::new(
                Box::new(NodeVaes(node.clone())),
                Preset::Server,
                &slug,
                "acme",
            );
            s.identity(&slug)?.bootstrap_admin("root", PW)?;
            (
                s,
                Backend::Pg {
                    url: url.to_string(),
                    node,
                    tenant: t,
                },
            )
        }
    };
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .map_err(err)?;
    let addr = listener.local_addr().map_err(err)?.to_string();
    rt.spawn(server.serve_with(listener));
    let c = Client::sign_in(&addr)?;
    let (svc, on_ping) = seed_http(&c)?;
    Ok(Env {
        _rt: rt,
        backend,
        c,
        svc,
        on_ping,
        n: std::cell::Cell::new(0),
    })
}

// ---- child nodes (fault cases) ----

struct Child {
    proc: std::process::Child,
    addr: String,
}

fn free_port() -> Result<String, BpaError> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(err)?;
    Ok(l.local_addr().map_err(err)?.to_string())
}

fn spawn_node(url: &str, tenant: &str, addr: &str) -> Result<Child, BpaError> {
    let exe = std::env::current_exe().map_err(err)?;
    let proc = std::process::Command::new(exe)
        .args([
            "--serve",
            "--database",
            url,
            "--tenant",
            tenant,
            "--listen",
            addr,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(err)?;
    let ok = wait(60.0, || {
        Ok(std::net::TcpStream::connect(addr).ok().map(|_| ()))
    })?;
    check(ok.is_some(), "the node listens")?;
    Ok(Child {
        proc,
        addr: addr.to_string(),
    })
}

impl Child {
    fn kill(&mut self) {
        let _ = self.proc.kill();
        let _ = self.proc.wait();
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        self.kill();
    }
}

fn serve(url: &str, tenant: &str, listen: &str) -> Result<(), BpaError> {
    let node = Arc::new(ubos_node::Node::connect(url)?);
    let s = Server::new(Box::new(NodeVaes(node)), Preset::Server, tenant, "acme");
    s.identity(tenant)?.bootstrap_admin("root", PW)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let listen: std::net::SocketAddr = listen.parse().map_err(err)?;
    rt.block_on(s.serve(listen)).map_err(err)
}

// ---- SUITE-NOD-EVENTS ----

/// Pinged events of an object, in log order: their numbers.
fn pings(c: &Client, events: &[Value], oid: &str) -> Vec<String> {
    let _ = c;
    events
        .iter()
        .filter(|e| e["type"] == "Pinged" && e["object_id"] == json!(oid))
        .map(|e| e["payload"]["n"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn object_id(c: &Client, slug: &str) -> Result<String, BpaError> {
    let r = c.get(&format!("/api/v1/acme/Account/{slug}?branch={BR}"))?;
    Ok(r["items"][0]["object_id"]
        .as_str()
        .or_else(|| r["items"][0]["id"].as_str())
        .unwrap_or_default()
        .to_string())
}

/// DSN-NOD-201, FR-EVT-014: effects reach the log once each, in commit order per object.
fn release_order(e: &Env) -> Result<Value, BpaError> {
    let slugs: Vec<String> = (0..3).map(|_| e.slug("ord")).collect();
    e.c.accounts(&slugs)?;
    let ids: Vec<String> = slugs
        .iter()
        .map(|s| object_id(&e.c, s))
        .collect::<Result<_, _>>()?;
    for round in 0..10 {
        for s in &slugs {
            e.c.port(s, "ping", json!({"n": format!("{round:02}")}))?;
        }
    }
    let all = wait(15.0, || {
        let ev = e.c.events("type=Pinged")?;
        Ok(ids
            .iter()
            .all(|o| pings(&e.c, &ev, o).len() >= 10)
            .then_some(ev))
    })?
    .ok_or_else(|| err("the 30 effects did not reach the log"))?;
    let expected: Vec<String> = (0..10).map(|r| format!("{r:02}")).collect();
    for o in &ids {
        check_v(
            pings(&e.c, &all, o) == expected,
            "each object's events once each, in commit order",
            &json!(pings(&e.c, &all, o)),
        )?;
    }
    let mine: Vec<&Value> = all
        .iter()
        .filter(|x| ids.contains(&x["object_id"].as_str().unwrap_or_default().to_string()))
        .collect();
    let distinct: std::collections::BTreeSet<&str> = mine
        .iter()
        .filter_map(|x| x["event_id"].as_str().or_else(|| x["effect_id"].as_str()))
        .collect();
    check(distinct.len() == 30, "30 distinct effect identifiers")?;
    let seqs: Vec<i64> = mine.iter().filter_map(|x| x["seq"].as_i64()).collect();
    check(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "log sequence increasing",
    )?;
    Ok(json!({"objects": 3, "effects": 30}))
}

/// DSN-NOD-201: outbox claims are leases; a claimed effect is not handed to another holder
/// until its lease lapses, and an acknowledged one never again.
fn outbox_claims(_e: &Env) -> Result<Value, BpaError> {
    let mut v = memory_vae_with(Some(&classes()), None, None)?;
    v.set_server_mode(true, false);
    v.call("create_branch", &json!({"name": BR, "from": "main"}))?;
    let ids: Vec<String> = (0..4).map(|_| uuid()).collect();
    let mut changes = vec![logic(&uuid())];
    for (i, id) in ids.iter().enumerate() {
        changes.push(json!({"op": "create", "class": "Account", "object_id": id, "slug": format!("c-{i}"), "body": {"name": "c", "status": "open"}}));
    }
    v.call("commit", &json!({"branch": BR, "changes": changes}))?;
    for (i, _) in ids.iter().enumerate() {
        for n in 0..2 {
            v.call(
                "execute",
                &json!({"branch": BR, "target": format!("ubos://acme/Account/c-{i}"), "port": "ping", "params": {"n": format!("{n}")},
                    "process": {"process_id": uuid()}}),
            )?;
        }
    }
    let a = v.call(
        "outbox_claim",
        &json!({"holder": "a", "n": 4, "lease_ms": 300}),
    )?["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let b = v.call(
        "outbox_claim",
        &json!({"holder": "b", "n": 100, "lease_ms": 300}),
    )?["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let key = |r: &Value| r["effect_id"].as_str().unwrap_or_default().to_string();
    let ak: Vec<String> = a.iter().map(key).collect();
    check_v(a.len() == 4, "holder a claims 4", &json!(a))?;
    check_v(
        b.iter().all(|r| !ak.contains(&key(r))),
        "holder b gets none of a's",
        &json!(b),
    )?;
    // per-object order: b gets no later effect of an object whose earlier effect a holds
    let a_obj: Vec<&Value> = a.iter().map(|r| &r["object_id"]).collect();
    check_v(
        b.iter().all(|r| !a_obj.contains(&&r["object_id"])),
        "per-object order across holders",
        &json!({"a": a, "b": b}),
    )?;
    let first_ack: Vec<Value> = a.iter().take(2).map(|r| r["effect_id"].clone()).collect();
    v.call("outbox_ack", &json!({"ids": first_ack}))?;
    std::thread::sleep(Duration::from_millis(400));
    let c = v.call(
        "outbox_claim",
        &json!({"holder": "c", "n": 100, "lease_ms": 300}),
    )?["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let ck: Vec<String> = c.iter().map(key).collect();
    check_v(
        ak[2..].iter().all(|k| ck.contains(k)),
        "a's unacknowledged effects are claimable after the lease",
        &json!(c),
    )?;
    check_v(
        ak[..2].iter().all(|k| !ck.contains(k)),
        "acknowledged effects are not handed out again",
        &json!(c),
    )?;
    Ok(json!({"claimed_a": a.len(), "claimed_b": b.len(), "after_lease": c.len()}))
}

/// DSN-NOD-203, FR-EVT-021…-023: a subscription reaches the branches of its scope only, and
/// the listener registry answers who listens.
fn subscription_scope(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("scope");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let h = json!({"kind": "port", "port": "mark", "target": "$event.object_id", "params": {"tag": "closed-seen"}});
    e.c.subscribe(
        "closed-main",
        &["AccountClosed"],
        &["main"],
        h.clone(),
        &e.svc,
        Value::Null,
    )?;
    e.c.subscribe(
        "closed-feature",
        &["AccountClosed"],
        &["feature/*"],
        h,
        &e.svc,
        Value::Null,
    )?;
    let l = e.c.get(&format!(
        "/api/v1/admin/listeners?type=AccountClosed&branch={BR}"
    ))?;
    let names: Vec<&str> = l["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x["name"].as_str())
        .collect();
    check_v(
        names.contains(&"closed-feature") && !names.contains(&"closed-main"),
        "listener registry by scope",
        &l,
    )?;
    e.c.port(&slug, "close", json!({}))?;
    let seen = wait(10.0, || {
        Ok(e.c
            .account(&slug)?
            .filter(|b| b["tag"] == "closed-seen")
            .map(|_| ()))
    })?;
    check(seen.is_some(), "the feature-scoped handler ran")?;
    let oid = object_id(&e.c, &slug)?;
    let mine: Vec<Value> =
        e.c.jobs("kind=delivery")?
            .into_iter()
            .filter(|j| j["payload"]["event"]["object_id"] == json!(oid))
            .collect();
    let subs: Vec<&str> = mine
        .iter()
        .filter_map(|j| j["payload"]["name"].as_str())
        .collect();
    check_v(
        subs == ["closed-feature"],
        "one delivery, for the subscription in scope",
        &json!(mine),
    )?;
    Ok(json!({"listeners": names, "deliveries": subs}))
}

/// DSN-NOD-204, FR-EVT-024: a handler runs as one process of its service principal, and a
/// redelivery of a committed delivery does not run again.
fn handler_process_once(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("once");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let oid = object_id(&e.c, &slug)?;
    e.c.port(&slug, "ping", json!({"n": "7"}))?;
    let job = wait(10.0, || {
        Ok(e.c
            .jobs("kind=delivery&state=done")?
            .into_iter()
            .find(|j| j["payload"]["event"]["object_id"] == json!(oid)))
    })?
    .ok_or_else(|| err("no delivery"))?;
    let pid = job["result"]["process_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let info = e.host("process_info", json!({"process_id": pid}))?;
    check_v(
        info["state"] == "committed" && info["initiator"] == json!(e.svc),
        "a committed process of the service principal",
        &info,
    )?;
    let again = e.host(
        "host.deliver",
        json!({"job": job, "principal": {"did": e.svc, "kind": "service"}}),
    )?;
    check_v(
        again["status"] == "already_done",
        "a redelivery does not run again",
        &again,
    )?;
    check(
        e.c.account(&slug)?.is_some_and(|b| b["tag"] == "7"),
        "the handler's effect once",
    )?;
    Ok(
        json!({"process_id": pid, "redelivery": again["status"], "trigger": info.get("trigger").cloned().unwrap_or(Value::Null)}),
    )
}

/// DSN-NOD-209, FR-EVT-051, -052: a failing delivery is retried by its policy, then dead
/// with its last error; operators re-drive, annotate and cancel dead letters, audited, and
/// cannot edit payloads.
fn dead_letters(e: &Env) -> Result<Value, BpaError> {
    let target = e.slug("missing");
    let src = e.slug("dl");
    e.c.accounts(std::slice::from_ref(&src))?;
    e.c.subscribe(
        &format!("dl-{target}"),
        &["Pinged"],
        &["feature/*"],
        json!({"kind": "port", "port": "mark", "target": format!("ubos://acme/Account/{target}"), "params": {"tag": "fixed"}}),
        &e.svc,
        json!({"max_attempts": 2, "backoff_ms": 100, "jitter": false}),
    )?;
    let src_id = object_id(&e.c, &src)?;
    e.c.port(&src, "ping", json!({"n": "1"}))?;
    let name = format!("dl-{target}");
    let dead = wait(15.0, || {
        Ok(e.c.jobs("kind=delivery&state=dead")?.into_iter().find(|j| {
            j["payload"]["name"] == json!(name)
                && j["payload"]["event"]["object_id"] == json!(src_id)
        }))
    })?
    .ok_or_else(|| err("no dead letter"))?;
    check_v(
        dead["attempts"] == 2 && dead["error"].as_str().is_some_and(|x| !x.is_empty()),
        "dead after 2 attempts with its last error",
        &dead,
    )?;
    let jid = dead["job_id"].as_str().unwrap_or_default().to_string();
    // the fix, then the re-drive
    e.c.accounts(std::slice::from_ref(&target))?;
    let (st, r) = e.c.call(
        "POST",
        &format!("/api/v1/admin/jobs/{jid}/retry"),
        &Value::Null,
    )?;
    check_v(st == 200 && r["ok"] == true, "re-drive", &r)?;
    let done = wait(10.0, || {
        Ok(e.c
            .get(&format!("/api/v1/admin/jobs/{jid}"))
            .ok()
            .filter(|j| j["state"] == "done"))
    })?;
    check(done.is_some(), "the re-driven delivery is done")?;
    check(
        e.c.account(&target)?.is_some_and(|b| b["tag"] == "fixed"),
        "its effect applied",
    )?;
    let (st, _) = e.call_annotate(&jid)?;
    check(st == 200, "annotate")?;
    let (st, _) = e.c.call(
        "PUT",
        &format!("/api/v1/admin/jobs/{jid}"),
        &json!({"payload": {}}),
    )?;
    check(st == 404 || st == 405, "payloads are not editable")?;
    // a second dead letter, cancelled
    let target2 = e.slug("missing");
    let name2 = format!("dl-{target2}");
    e.c.subscribe(
        &name2,
        &["Pinged"],
        &["feature/*"],
        json!({"kind": "port", "port": "mark", "target": format!("ubos://acme/Account/{target2}"), "params": {"tag": "x"}}),
        &e.svc,
        json!({"max_attempts": 1}),
    )?;
    e.c.port(&src, "ping", json!({"n": "2"}))?;
    let dead2 = wait(15.0, || {
        Ok(e.c
            .jobs("kind=delivery&state=dead")?
            .into_iter()
            .find(|j| j["payload"]["name"] == json!(name2)))
    })?
    .ok_or_else(|| err("no second dead letter"))?;
    let jid2 = dead2["job_id"].as_str().unwrap_or_default().to_string();
    let (_, r) = e.c.call(
        "POST",
        &format!("/api/v1/admin/jobs/{jid2}/cancel"),
        &Value::Null,
    )?;
    check_v(r["ok"] == true, "cancel a dead letter", &r)?;
    let j2 = e.c.get(&format!("/api/v1/admin/jobs/{jid2}"))?;
    check_v(j2["state"] == "cancelled", "cancelled", &j2)?;
    let kinds = e.c.log_kinds()?;
    check_v(
        kinds.iter().any(|k| k == "delivery.dead")
            && kinds.iter().any(|k| k == "admin.job_retry")
            && kinds.iter().any(|k| k == "admin.job_cancel")
            && kinds.iter().any(|k| k == "admin.job_annotate"),
        "dead letters and operator actions audited",
        &json!(kinds),
    )?;
    Ok(json!({"job_id": jid, "attempts": 2, "redriven": true}))
}

impl Env {
    fn call_annotate(&self, jid: &str) -> Result<(u16, Value), BpaError> {
        self.c.call(
            "POST",
            &format!("/api/v1/admin/jobs/{jid}/annotate"),
            &json!({"text": "fixed the target account"}),
        )
    }
}

/// DSN-NOD-208, FR-TIME-052: a future-dated version fires `BecameEffective` once, after its
/// valid-from instant.
fn became_effective(e: &Env) -> Result<Value, BpaError> {
    let due = now() + chrono::Duration::milliseconds(2000);
    let oid = uuid();
    let slug = e.slug("rate");
    e.c.commit("main", json!([{"op": "create", "class": "Rate", "object_id": oid, "slug": slug, "body": {"value": "1.5"}, "valid": {"from": iso(due)}}]))?;
    let ev = wait(10.0, || {
        let ev = e.c.events(&format!("object={oid}"))?;
        Ok((!ev.is_empty()).then_some(ev))
    })?
    .ok_or_else(|| err("no BecameEffective"))?;
    let at = ev[0]["released_at"].as_str().unwrap_or_default();
    let lag = schedule::parse(at)
        .map(|t| (t - due).num_milliseconds())
        .unwrap_or(i64::MAX);
    check_v(
        ev[0]["type"] == "BecameEffective" && (0..=5000).contains(&lag),
        "fired after valid-from, within 5 s",
        &json!(ev),
    )?;
    std::thread::sleep(Duration::from_millis(1500));
    check(e.c.events(&format!("object={oid}"))?.len() == 1, "once")?;
    Ok(json!({"lag_ms": lag}))
}

/// DSN-NOD-202, FR-EVT-015: the event log answers by type, object and time, and keeps each
/// type by its retention.
fn event_log_query_retention(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("ret");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let oid = object_id(&e.c, &slug)?;
    e.c.port(&slug, "ping", json!({"n": "1"}))?;
    e.c.port(&slug, "close", json!({}))?;
    let got = wait(10.0, || {
        let ev = e.c.events(&format!("object={oid}"))?;
        Ok((ev.len() >= 2).then_some(ev))
    })?
    .ok_or_else(|| err("events missing"))?;
    let types: Vec<&str> = got.iter().filter_map(|x| x["type"].as_str()).collect();
    check_v(
        types == ["Pinged", "AccountClosed"],
        "by object, in order",
        &json!(got),
    )?;
    let by_type = e.c.events("type=AccountClosed")?;
    check(
        !by_type.is_empty() && by_type.iter().all(|x| x["type"] == "AccountClosed"),
        "by type",
    )?;
    let later = iso(now() + chrono::Duration::hours(1));
    check(
        e.c.events(&format!("from={later}"))?.is_empty(),
        "by time: none released after now",
    )?;
    let (st, _) = e.c.call(
        "PUT",
        "/api/v1/admin/events/retention",
        &json!({"type": "AccountClosed", "days": 30}),
    )?;
    check(st == 200, "retention set")?;
    let (_, r29) = e.c.call(
        "POST",
        "/api/v1/admin/events/sweep",
        &json!({"now": iso(now() + chrono::Duration::days(29))}),
    )?;
    check_v(r29["removed"] == 0, "kept before 30 days", &r29)?;
    let (_, r31) = e.c.call(
        "POST",
        "/api/v1/admin/events/sweep",
        &json!({"now": iso(now() + chrono::Duration::days(31))}),
    )?;
    check_v(
        r31["removed"].as_u64().unwrap_or(0) >= 1,
        "removed after 30 days",
        &r31,
    )?;
    let left = e.c.events(&format!("object={oid}"))?;
    let types: Vec<&str> = left.iter().filter_map(|x| x["type"].as_str()).collect();
    check_v(types == ["Pinged"], "other types kept", &json!(left))?;
    let (st, _) = http(&e.c.addr, "GET", "/api/v1/admin/events", None, &Value::Null)?;
    check(st == 403, "the log needs an administrator")?;
    Ok(json!({"removed": r31["removed"]}))
}

/// DSN-NOD-228: leadership is a lease with a fencing token; another holder takes it only
/// after it lapses, with a larger token, and the former holder cannot renew.
fn leader_leases(e: &Env) -> Result<Value, BpaError> {
    let l = e.c.get("/api/v1/admin/leaders")?;
    check_v(
        l["items"].as_array().is_some_and(|a| {
            a.iter()
                .any(|x| x["leader"] == true && x["token"].as_i64().unwrap_or(0) >= 1)
        }),
        "this node leads its VAE",
        &l,
    )?;
    let leases: Arc<dyn Leases> = match e.pg() {
        Some((_, node, _)) => node.leases(),
        None => Arc::new(MemLeases::default()),
    };
    let name = format!("suite:{}", uuid());
    let t1 = leases
        .acquire(&name, "a", 300)?
        .ok_or_else(|| err("a acquires"))?;
    check(
        leases.acquire(&name, "b", 300)?.is_none(),
        "b waits while a's lease lives",
    )?;
    check(
        leases.acquire(&name, "a", 300)? == Some(t1),
        "a renews with the same token",
    )?;
    std::thread::sleep(Duration::from_millis(400));
    let t2 = leases
        .acquire(&name, "b", 300)?
        .ok_or_else(|| err("b takes over"))?;
    check(t2 > t1, "a larger token")?;
    check(leases.acquire(&name, "a", 300)?.is_none(), "a cannot renew")?;
    check(leases.token(&name)? == Some(t2), "the token fences")?;
    Ok(json!({"tokens": [t1, t2]}))
}

/// NR-DUR-006, FR-TXN-052 (PostgreSQL): a node killed while it releases and delivers
/// leaves nothing lost or doubled; after restart every effect is in the log once and every
/// delivery done once.
fn crash_recovery(e: &Env) -> Result<Value, BpaError> {
    let Some((url, node, _)) = e.pg() else {
        return Ok(json!({"skipped": "needs PostgreSQL"}));
    };
    let slug = format!("evf{}", &uuid().replace('-', "")[..12]);
    seed_pg(node, &slug)?;
    let addr = free_port()?;
    let mut n1 = spawn_node(url, &slug, &addr)?;
    let c = Client::sign_in(&addr)?;
    seed_http(&c)?;
    let accts: Vec<String> = (0..3).map(|i| format!("cr-{i}")).collect();
    c.accounts(&accts)?;
    let ids: Vec<String> = accts
        .iter()
        .map(|s| object_id(&c, s))
        .collect::<Result<_, _>>()?;
    let total = 60;
    for k in 0..total {
        c.port(&accts[k % 3], "ping", json!({"n": format!("{:03}", k)}))?;
    }
    n1.kill();
    let killed = Instant::now();
    std::thread::sleep(Duration::from_millis(500));
    let n2 = spawn_node(url, &slug, &addr)?;
    let restarted = Instant::now();
    let c = Client::sign_in(&n2.addr)?;
    let done = wait(300.0, || {
        let ev = c.events("type=Pinged")?;
        let jobs = c.jobs("kind=delivery&state=done")?;
        Ok((ev.len() >= total && jobs.len() >= total).then_some((ev, jobs)))
    })?;
    let (ev, jobs) = done.ok_or_else(|| err("not all effects delivered within 5 minutes"))?;
    let secs = restarted.elapsed().as_secs_f64();
    check_v(
        ev.len() == total,
        "each effect in the log once",
        &json!(ev.len()),
    )?;
    check_v(
        jobs.len() == total,
        "each delivery done once",
        &json!(jobs.len()),
    )?;
    for (i, o) in ids.iter().enumerate() {
        let want: Vec<String> = (0..total)
            .filter(|k| k % 3 == i)
            .map(|k| format!("{k:03}"))
            .collect();
        check_v(
            pings(&c, &ev, o) == want,
            "per-object order kept across the crash",
            &json!(pings(&c, &ev, o)),
        )?;
    }
    let last = c.account(&accts[2])?.unwrap_or_default();
    check_v(
        last["tag"] == json!(format!("{:03}", total - 1)),
        "the last delivery applied",
        &last,
    )?;
    drop(n2);
    Ok(
        json!({"effects": total, "down_ms": (restarted - killed).as_millis(), "delivered_s_after_restart": (secs * 10.0).round() / 10.0}),
    )
}

/// DSN-NOD-228 (PostgreSQL): with two nodes, killing the leader hands the VAE to the other
/// within 20 s with a larger token, and the new leader fires became-effective events.
fn failover(e: &Env) -> Result<Value, BpaError> {
    let Some((url, node, _)) = e.pg() else {
        return Ok(json!({"skipped": "needs PostgreSQL"}));
    };
    let slug = format!("evl{}", &uuid().replace('-', "")[..12]);
    seed_pg(node, &slug)?;
    let (a1, a2) = (free_port()?, free_port()?);
    let n1 = spawn_node(url, &slug, &a1)?;
    let c1 = Client::sign_in(&a1)?;
    seed_http(&c1)?;
    let n2 = spawn_node(url, &slug, &a2)?;
    let c2 = Client::sign_in(&a2)?;
    // the second node opens the VAE with a session (a commit through it)
    c2.accounts(&["fo-1".to_string()])?;
    let leaders = |c: &Client| -> Result<Vec<Value>, BpaError> {
        Ok(c.get("/api/v1/admin/leaders")?["items"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    };
    let pair = wait(20.0, || {
        let (l1, l2) = (leaders(&c1)?, leaders(&c2)?);
        let h1 = l1.iter().any(|x| x["leader"] == true);
        let h2 = l2.iter().any(|x| x["leader"] == true);
        Ok((h1 != h2 && !l2.is_empty()).then_some((h1, l1, l2)))
    })?
    .ok_or_else(|| err("not exactly one leader"))?;
    let (first_is_1, l1, l2) = pair;
    let t1 = if first_is_1 { &l1 } else { &l2 }[0]["token"]
        .as_i64()
        .unwrap_or(0);
    // kill the leader; the other node is to take over
    let (mut nodes, ca) = (vec![n1, n2], if first_is_1 { c2 } else { c1 });
    nodes[if first_is_1 { 0 } else { 1 }].kill();
    let killed = Instant::now();
    let took = wait(40.0, || {
        Ok(leaders(&ca)?
            .into_iter()
            .find(|x| x["leader"] == true)
            .map(|x| {
                (
                    killed.elapsed().as_secs_f64(),
                    x["token"].as_i64().unwrap_or(0),
                )
            }))
    })?
    .ok_or_else(|| err("no takeover"))?;
    check_v(
        took.0 <= 20.0 && took.1 > t1,
        "takeover within 20 s with a larger token",
        &json!([took.0, t1, took.1]),
    )?;
    let due = now() + chrono::Duration::milliseconds(1500);
    let oid = uuid();
    ca.commit("main", json!([{"op": "create", "class": "Rate", "object_id": oid, "slug": "fo-rate", "body": {"value": "2"}, "valid": {"from": iso(due)}}]))?;
    let fired = wait(10.0, || {
        Ok((!ca.events(&format!("object={oid}"))?.is_empty()).then_some(()))
    })?;
    check(
        fired.is_some(),
        "the new leader fires became-effective events",
    )?;
    drop(nodes);
    Ok(json!({"takeover_s": (took.0 * 10.0).round() / 10.0, "tokens": [t1, took.1]}))
}

/// NR-OBS-001, FR-OPS-041, DSN-NOD-610, -611: a traced port call yields one trace linking
/// the request, the DVM operation, its flush and storage phases and the job of the
/// subscription it triggered, with the process ID as an attribute; no business value
/// reaches a span.
fn traces(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("trace");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let hex = uuid().replace('-', "");
    let (trace_id, client_span) = (hex.clone(), hex[..16].to_string());
    let tp = format!("00-{trace_id}-{client_span}-01");
    let canary = format!("canary{}", &uuid().replace('-', "")[..12]);
    let r = e.c.mutate_traced(
        json!({"vae": "acme", "branch": BR, "port": {"target": format!("ubos://acme/Account/{slug}"), "name": "ping", "params": {"n": canary}}}),
        Some(&tp),
    )?;
    check_v(r["type"] == "result", "the traced ping", &r)?;
    let spans = wait(15.0, || {
        let s = e.c.get(&format!(
            "/api/v1/admin/telemetry/spans?trace={trace_id}&limit=500"
        ))?["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(s.iter().any(|x| x["name"] == "job.delivery").then_some(s))
    })?
    .ok_or_else(|| err("no job span in the trace"))?;
    let by = |n: &str| {
        spans
            .iter()
            .find(|x| x["name"] == n)
            .cloned()
            .unwrap_or(Value::Null)
    };
    let (edge, dvm, flush, store, job) = (
        by("ubtp.mutate"),
        by("dvm.ubtp"),
        by("dvm.flush"),
        by("store.flush"),
        by("job.delivery"),
    );
    let names: Vec<&str> = spans.iter().filter_map(|x| x["name"].as_str()).collect();
    check_v(
        !edge.is_null() && !dvm.is_null() && !flush.is_null() && !store.is_null() && !job.is_null(),
        "edge, DVM, flush, storage and job spans in one trace",
        &json!(names),
    )?;
    check_v(
        edge["parent"] == json!(client_span),
        "the edge span continues the client's trace",
        &edge,
    )?;
    check_v(
        dvm["parent"] == edge["span_id"]
            && flush["parent"] == dvm["span_id"]
            && store["parent"] == dvm["span_id"],
        "the DVM span under the edge, the phases under the DVM span",
        &json!([edge["span_id"], dvm, flush, store]),
    )?;
    check_v(
        edge["parent"].is_string()
            && edge["attributes"]["ubos.process_id"].is_string()
            && job["parent"] == edge["span_id"],
        "the job joins the request's trace, with the process ID",
        &json!([edge, job]),
    )?;
    let spans_all: Value = e.c.get("/api/v1/admin/telemetry/spans?limit=2000")?;
    let metrics: Value = e.c.get("/api/v1/admin/telemetry/metrics")?;
    let all = format!("{spans_all}{metrics}");
    check(
        !all.contains(&canary),
        "no business value in telemetry (DSN-NOD-611)",
    )?;
    Ok(json!({"trace_id": trace_id, "spans": names}))
}

// ---- SUITE-NOD-JOBS ----

/// DSN-NOD-220, FR-EVT-031: a `run.async` job is a System-kind `Job` object on `main` whose
/// state follows the run, and ordinary commits cannot write it.
fn jobs_are_objects(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("later");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let r = e.c.port(&slug, "later", json!({}))?;
    check_v(r["result"] == "scheduled", "the port schedules a job", &r)?;
    let closed = wait(10.0, || {
        Ok(e.c
            .account(&slug)?
            .filter(|b| b["status"] == "closed")
            .map(|_| ()))
    })?;
    check(closed.is_some(), "the job ran")?;
    let oid = object_id(&e.c, &slug)?;
    let job =
        e.c.jobs("kind=port")?
            .into_iter()
            .find(|j| {
                j["payload"]["target"] == json!(oid)
                    || j["payload"]["target"]
                        .as_str()
                        .is_some_and(|t| t.ends_with(&slug))
            })
            .ok_or_else(|| err("no port job"))?;
    let jid = job["job_id"].as_str().unwrap_or_default().to_string();
    let obj = wait(5.0, || {
        let r = e.c.get(&format!("/api/v1/acme/Job/job-{jid}"))?;
        Ok(r["items"]
            .get(0)
            .filter(|x| x["body"]["state"] == "succeeded")
            .cloned())
    })?
    .ok_or_else(|| err("the Job object is not succeeded"))?;
    let r = e.c.mutate(json!({"vae": "acme", "branch": "main", "commit": {"changes": [
        {"op": "update", "object_id": jid, "base": obj["version_hash"], "body": {"state": "pending"}}]}}))?;
    check_v(
        r["type"] == "error",
        "an ordinary commit cannot write a Job",
        &r,
    )?;
    Ok(json!({"job_id": jid, "object_state": obj["body"]["state"], "refused": r["body"]["code"]}))
}

/// The job store of the VAE for the store cases (a kind the Server's worker does not run).
fn store(e: &Env) -> Result<Arc<dyn JobStore>, BpaError> {
    Ok(match &e.backend {
        Backend::Memory { .. } => Arc::new(MemJobQueue::default()),
        Backend::Pg { node, tenant, .. } => Arc::new(
            node.jobs(tenant, "acme")?
                .ok_or_else(|| err("no job store"))?,
        ),
    })
}

fn spec(kind: &str, queue: &str, priority: i64, max: i64) -> Value {
    json!({"job_id": uuid(), "kind": kind, "queue": queue, "priority": priority, "max_attempts": max, "payload": {"n": 1}})
}

/// DSN-NOD-221, FR-EVT-033: a lease's token fences results and heartbeats; an expired
/// lease is claimed again with a larger token.
fn lease_fencing(e: &Env) -> Result<Value, BpaError> {
    let s = store(e)?;
    let kind = format!("suite-{}", &uuid()[..8]);
    let id = s.enqueue(&spec(&kind, "default", 0, 3))?;
    let j1 = s
        .claim(&[&kind], "w1", 300)?
        .ok_or_else(|| err("w1 claims"))?;
    let t1 = j1["lease_token"].as_i64().unwrap_or(0);
    check(s.heartbeat(&id, t1, 300)?, "w1 heartbeats")?;
    check(
        s.claim(&[&kind], "w2", 300)?.is_none(),
        "w2 waits while the lease lives",
    )?;
    std::thread::sleep(Duration::from_millis(450));
    let j2 = s
        .claim(&[&kind], "w2", 300)?
        .ok_or_else(|| err("w2 claims after expiry"))?;
    let t2 = j2["lease_token"].as_i64().unwrap_or(0);
    check(t2 > t1, "a larger token")?;
    check(!s.heartbeat(&id, t1, 300)?, "w1's heartbeat is fenced")?;
    check(
        !s.complete(&id, Some(t1), &json!({"by": "w1"}))?,
        "w1's result is fenced",
    )?;
    check(
        s.complete(&id, Some(t2), &json!({"by": "w2"}))?,
        "w2 completes",
    )?;
    let j = s.job(&id)?.unwrap_or_default();
    check_v(
        j["state"] == "done" && j["result"]["by"] == "w2",
        "w2's result kept",
        &j,
    )?;
    Ok(json!({"tokens": [t1, t2]}))
}

/// DSN-NOD-223, FR-EVT-051, -053: retries back off exponentially; the same error three
/// times is poison and dead early, other errors retry until the policy is spent.
fn retries_and_poison(e: &Env) -> Result<Value, BpaError> {
    let p = RetryPolicy::from_json(&json!({"jitter": false}));
    let d: Vec<i64> = (1..=4).map(|a| p.delay_ms(a)).collect();
    check_v(
        p.max_attempts == 5 && d == [1000, 2000, 4000, 8000],
        "default policy: 5 attempts from 1 s, doubling",
        &json!(d),
    )?;
    let s = store(e)?;
    let kind = format!("suite-{}", &uuid()[..8]);
    let poison = s.enqueue(&spec(&kind, "default", 0, 5))?;
    let mut last = None;
    for _ in 0..3 {
        let j = s.claim(&[&kind], "w", 5000)?.ok_or_else(|| err("claim"))?;
        last = s.fail(
            &poison,
            j["lease_token"].as_i64(),
            "MODEL.INVALID_VALUE: bad row 17",
            0,
            false,
        )?;
    }
    let f = last.ok_or_else(|| err("fail fenced"))?;
    check_v(
        f.state == "dead" && f.poison,
        "same error 3 times: dead, poison",
        &json!({"state": f.state, "poison": f.poison}),
    )?;
    let other = s.enqueue(&spec(&kind, "default", 0, 4))?;
    let mut states = Vec::new();
    for code in [
        "NET.TIMEOUT",
        "NET.RESET",
        "NOD.STORAGE_UNAVAILABLE",
        "NET.REFUSED",
    ] {
        let j = s.claim(&[&kind], "w", 5000)?.ok_or_else(|| err("claim"))?;
        let f = s
            .fail(&other, j["lease_token"].as_i64(), code, 0, false)?
            .ok_or_else(|| err("fenced"))?;
        states.push((f.state, f.poison));
    }
    check_v(
        states.last() == Some(&("dead".to_string(), false))
            && states[..3].iter().all(|x| x.0 == "queued"),
        "different errors: retried until the policy is spent",
        &json!(format!("{states:?}")),
    )?;
    Ok(json!({"delays_ms": d, "poison_after": 3}))
}

/// DSN-NOD-229, FR-EVT-034: claims take the highest priority first, skip paused queues and
/// respect queue and tenant limits.
fn priorities_and_limits(e: &Env) -> Result<Value, BpaError> {
    let s = store(e)?;
    let kind = format!("suite-{}", &uuid()[..8]);
    let q = format!("q-{}", &uuid()[..8]);
    let low = s.enqueue(&spec(&kind, &q, 0, 3))?;
    let high = s.enqueue(&spec(&kind, &q, 5, 3))?;
    s.set_queue(&q, Some(true), None)?;
    check(
        s.claim(&[&kind], "w", 5000)?.is_none(),
        "a paused queue gives nothing",
    )?;
    s.set_queue(&q, Some(false), Some(Some(1)))?;
    let a = s.claim(&[&kind], "w", 5000)?.ok_or_else(|| err("claim"))?;
    check_v(a["job_id"] == json!(high), "highest priority first", &a)?;
    check(
        s.claim(&[&kind], "w", 5000)?.is_none(),
        "queue concurrency 1",
    )?;
    s.complete(&high, a["lease_token"].as_i64(), &json!({}))?;
    check(s.reprioritise(&low, 9)?, "re-prioritise")?;
    s.set_queue(&q, None, Some(None))?;
    // the tenant limit: one running job holds it
    let third = s.enqueue(&spec(&kind, &q, 0, 3))?;
    s.set_tenant_limit(Some(1))?;
    let b = s.claim(&[&kind], "w", 5000)?.ok_or_else(|| err("claim"))?;
    check_v(
        b["job_id"] == json!(low) && b["priority"] == 9,
        "then the re-prioritised one",
        &b,
    )?;
    let blocked = s.claim(&[&kind], "w", 5000)?.is_none();
    s.set_tenant_limit(None)?;
    check(blocked, "the tenant limit holds")?;
    let c = s
        .claim(&[&kind], "w", 5000)?
        .ok_or_else(|| err("claim after the limit"))?;
    check_v(
        c["job_id"] == json!(third),
        "the next after the limit is lifted",
        &c,
    )?;
    s.complete(&low, b["lease_token"].as_i64(), &json!({}))?;
    s.complete(&third, c["lease_token"].as_i64(), &json!({}))?;
    let qs = s.queues()?;
    check_v(
        qs.iter().any(|x| x["queue"] == json!(q)),
        "queues listed",
        &json!(qs),
    )?;
    Ok(json!({"queue": q}))
}

/// FR-EVT-032, DSN-NOD-220: the runtime rows are a hint; a lost row is rebuilt from the
/// pending `Job` object, and an existing one is left alone.
fn sweeper_restores(e: &Env) -> Result<Value, BpaError> {
    // a pending Job object: an operator's port job in a paused queue
    let q = format!("hold-{}", &uuid()[..8]);
    let slug = e.slug("sw");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let (st, _) = e.c.call(
        "POST",
        &format!("/api/v1/admin/queues/{q}/pause"),
        &Value::Null,
    )?;
    check(st == 200, "pause")?;
    let (st, r) = e.c.call("POST", "/api/v1/admin/jobs", &json!({"kind": "port", "queue": q,
        "payload": {"branch": BR, "target": format!("ubos://acme/Account/{slug}"), "port": "mark", "params": {"tag": "swept"}}}))?;
    check_v(st == 200, "operator job", &r)?;
    let jid = r["job_id"].as_str().unwrap_or_default().to_string();
    let open = e.host("host.jobs_open", json!({}))?;
    let pending = open["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|j| j["job_id"] == json!(jid))
        .cloned()
        .ok_or_else(|| err("the Job object is pending"))?;
    let restored = match &e.backend {
        Backend::Memory { jobs, .. } => {
            let s = jobs.clone();
            let present = !s.restore(&pending)?;
            let fresh = MemJobQueue::default();
            let rebuilt = fresh.restore(&pending)?;
            check(present && rebuilt, "kept when present, rebuilt when lost")?;
            check_v(
                fresh
                    .job(&jid)?
                    .is_some_and(|j| j["state"] == "queued" && j["queue"] == json!(q)),
                "the rebuilt row",
                &fresh.job(&jid)?.unwrap_or_default(),
            )?;
            true
        }
        Backend::Pg { url, node, tenant } => {
            let s = node.jobs(tenant, "acme")?.ok_or_else(|| err("no jobs"))?;
            check(!s.restore(&pending)?, "kept when present")?;
            let db = ubos_node::db::Db::owner(url)?;
            db.execute(
                "DELETE FROM job WHERE job_id = $1::uuid",
                &[ubos_node::db::Param::Text(&jid)],
            )?;
            check(s.job(&jid)?.is_none(), "the row is lost")?;
            check(s.restore(&pending)?, "rebuilt from the object")?;
            s.job(&jid)?.is_some_and(|j| j["state"] == "queued")
        }
    };
    check(restored, "restored")?;
    let (st, _) = e.c.call(
        "POST",
        &format!("/api/v1/admin/queues/{q}/resume"),
        &Value::Null,
    )?;
    check(st == 200, "resume")?;
    let ran = wait(10.0, || {
        Ok(e.c
            .account(&slug)?
            .filter(|b| b["tag"] == "swept")
            .map(|_| ()))
    })?;
    check(
        ran.is_some(),
        "the restored job runs once the queue resumes",
    )?;
    Ok(json!({"job_id": jid}))
}

/// DSN-NOD-230: a bulk process runs as a job batch by batch, and a cancellation stops it
/// between batches.
fn bulk_cancel(e: &Env) -> Result<Value, BpaError> {
    let tag = e.slug("bk");
    let changes: Vec<Value> = (0..200)
        .map(|n| json!({"op": "create", "class": "Account", "object_id": uuid(), "slug": format!("{tag}-{n}"), "body": {"name": "b", "status": "open"}}))
        .collect();
    let (st, r) = e.c.call(
        "POST",
        "/api/v1/admin/jobs",
        &json!({"kind": "bulk", "payload": {"branch": BR, "changes": changes, "max_changes": 1}}),
    )?;
    check_v(st == 200, "bulk job", &r)?;
    let jid = r["job_id"].as_str().unwrap_or_default().to_string();
    let running = wait(15.0, || {
        Ok(e.c.get(&format!("/api/v1/admin/jobs/{jid}"))?["state"]
            .as_str()
            .filter(|s| *s == "running")
            .map(|_| ()))
    })?;
    check(running.is_some(), "running")?;
    std::thread::sleep(Duration::from_millis(300));
    let (_, r) = e.c.call(
        "POST",
        &format!("/api/v1/admin/jobs/{jid}/cancel"),
        &Value::Null,
    )?;
    check_v(r["ok"] == true, "cancel", &r)?;
    let c = wait(20.0, || {
        Ok(e.c.get(&format!("/api/v1/admin/jobs/{jid}"))?["state"]
            .as_str()
            .filter(|s| *s == "cancelled")
            .map(|_| ()))
    })?;
    check(c.is_some(), "cancelled")?;
    let mut made = 0;
    for n in 0..200 {
        if e.c.account(&format!("{tag}-{n}"))?.is_some() {
            made += 1;
        }
    }
    check_v(
        (1..200).contains(&made),
        "stopped between batches",
        &json!(made),
    )?;
    Ok(json!({"batches_before_cancel": made, "of": 200}))
}

/// DSN-NOD-221 (with STD-TXN-040): two holders of the same job run its process; the
/// idempotency key commits it once.
fn idempotent_execution(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("idem");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let key = format!("job:{}", uuid());
    let input = json!({"branch": BR, "target": format!("ubos://acme/Account/{slug}"), "port": "mark", "params": {"tag": "once"},
        "process": {"process_id": uuid(), "idempotency_key": key}});
    let a = e.host("execute", input.clone())?;
    let mut again = input.clone();
    again["process"]["process_id"] = json!(uuid());
    let b = e.host("execute", again)?;
    let head = e.host("branch_info", json!({"branch": BR}))?;
    check_v(
        a["cseq"].is_u64() && a["cseq"] == b["cseq"] && head["cseq"] == a["cseq"],
        "the second run replays the first commit and commits nothing",
        &json!([a["cseq"], b["cseq"], head["cseq"]]),
    )?;
    Ok(json!({"cseq": a["cseq"], "replayed": true}))
}

/// FR-EVT-041: cron and business-calendar rules in a time zone.
fn schedule_rules(_e: &Env) -> Result<Value, BpaError> {
    let t = |s: &str| schedule::parse(s);
    let r = schedule::Rule::from_body(
        &json!({"rule": {"calendar": {"weekend": ["sat", "sun"], "holidays": ["2026-11-26"]}, "at": "18:00"},
        "time_zone": "America/New_York"}),
    )?;
    let (s, _) = r.slots(t("2026-10-30T00:00:00Z")?, t("2026-11-04T00:00:00Z")?);
    let got: Vec<String> = s.iter().map(|x| iso(*x)).collect();
    check_v(
        got == [
            "2026-10-30T22:00:00.000000Z",
            "2026-11-02T23:00:00.000000Z",
            "2026-11-03T23:00:00.000000Z",
        ],
        "18:00 New York on business days across the DST change",
        &json!(got),
    )?;
    let (s, _) = r.slots(t("2026-11-25T00:00:00Z")?, t("2026-11-28T00:00:00Z")?);
    check(s.len() == 2, "a holiday has no slot")?;
    let nth =
        schedule::Rule::from_body(&json!({"rule": {"calendar": {}, "at": "09:00", "nth": 3}}))?;
    check(
        nth.next_after(t("2026-11-01T00:00:00Z")?) == Some(t("2026-11-04T09:00:00Z")?),
        "the 3rd business day",
    )?;
    let c = schedule::Rule::from_body(&json!({"rule": {"cron": "*/15 9-17 * * 1-5"}}))?;
    let (s, _) = c.slots(t("2026-10-16T16:50:00Z")?, t("2026-10-19T09:20:00Z")?);
    check(
        s.len() == 6 && s[4] == t("2026-10-19T09:00:00Z")?,
        "cron steps, ranges and weekdays",
    )?;
    let gap = schedule::Rule::from_body(
        &json!({"rule": {"cron": "30 2 * * *"}, "time_zone": "America/New_York"}),
    )?;
    check(
        gap.slots(t("2026-03-07T00:00:00Z")?, t("2026-03-10T00:00:00Z")?)
            .0
            .len()
            == 2,
        "no slot in the spring-forward gap",
    )?;
    let amb = schedule::Rule::from_body(
        &json!({"rule": {"cron": "30 1 * * *"}, "time_zone": "America/New_York"}),
    )?;
    check(
        amb.slots(t("2026-10-31T12:00:00Z")?, t("2026-11-01T12:00:00Z")?)
            .0
            == vec![t("2026-11-01T05:30:00Z")?],
        "one slot in the fall-back overlap, at the earlier instant",
    )?;
    check(
        schedule::Rule::from_body(&json!({"rule": {"cron": "61 * * * *"}})).is_err(),
        "a bad rule is refused",
    )?;
    Ok(json!({"dst": got}))
}

fn sched_store(e: &Env) -> Result<Arc<dyn ScheduleStore>, BpaError> {
    Ok(match &e.backend {
        Backend::Memory { .. } => Arc::new(MemScheduleStore::default()),
        Backend::Pg { url, tenant, .. } => {
            let db = Arc::new(ubos_node::db::Db::tenant(url, &tenant.id)?);
            let id = vae_id(e)?;
            Arc::new(PgScheduleStore::new(db, &tenant.id, &id))
        }
    })
}

fn vae_id(e: &Env) -> Result<String, BpaError> {
    let (url, _, tenant) = e.pg().ok_or_else(|| err("no PostgreSQL"))?;
    let db = ubos_node::db::Db::owner(url)?;
    let r = db.query(
        "SELECT vae_id::text AS v FROM vae WHERE tenant_id = $1::uuid AND authority = 'acme'",
        &[ubos_node::db::Param::Text(&tenant.id)],
    )?;
    Ok(r.first()
        .and_then(|x| x["v"].as_str())
        .unwrap_or_default()
        .to_string())
}

fn sched_body(rule: &str, misfire: &str) -> Value {
    json!({"name": "s", "rule": {"cron": rule}, "misfire": misfire, "run_as": "did:svc",
        "target": {"kind": "port", "branch": BR, "port": "mark", "target": "ubos://acme/Account/x", "params": {"tag": "$event.slot"}}})
}

/// DSN-NOD-224, FR-EVT-043: two leaders evaluating at once (a lease handover) create one
/// job per slot. Each slot is recorded once; an evaluator that finds a slot still `firing`
/// (left by a crash, or another leader between its record and its enqueue) enqueues it
/// again with the same job identifier, derived from the slot, so it remains one job.
fn schedule_exactly_once(e: &Env) -> Result<Value, BpaError> {
    let st = sched_store(e)?;
    let sid = uuid();
    let b = sched_body("* * * * *", "fire_once");
    let base = schedule::parse("2026-10-16T12:00:30.000000Z")?;
    st.set_cursor(&sid, base)?;
    let mut specs = Vec::new();
    for round in 1..=20 {
        let at = base + chrono::Duration::minutes(round);
        let hs: Vec<_> = (0..2)
            .map(|_| {
                let (st, sid, b) = (st.clone(), sid.clone(), b.clone());
                std::thread::spawn(move || -> Result<Vec<Value>, BpaError> {
                    let ev = schedule::evaluate(st.as_ref(), &sid, &b, at)?;
                    schedule::mark_fired(st.as_ref(), &sid, &ev.firing)?;
                    Ok(ev.specs)
                })
            })
            .collect();
        for h in hs {
            specs.extend(h.join().map_err(|_| err("thread"))??);
        }
    }
    // every enqueue carries its slot's job identifier
    for sp in &specs {
        let slot = sp["payload"]["slot"].as_str().unwrap_or_default();
        check_v(
            sp["job_id"] == json!(ubos_node::platform::schedule_job_id(&sid, slot)?),
            "the job identifier is the slot's",
            sp,
        )?;
    }
    let jobs: std::collections::BTreeSet<String> = specs
        .iter()
        .map(|x| x["job_id"].as_str().unwrap_or_default().to_string())
        .collect();
    check_v(
        jobs.len() == 20,
        "20 slots, 20 jobs",
        &json!({"jobs": jobs.len(), "enqueues": specs.len()}),
    )?;
    let fired = st.slots(&sid, Some("fired"), 100)?;
    check(
        fired.len() == 20 && st.slots(&sid, None, 100)?.len() == 20,
        "20 slots recorded, each fired",
    )?;
    // every enqueue, repeated ones included, into the job store: one job per slot
    let q = store(e)?;
    let kind = format!("suite-{}", &uuid()[..8]);
    for sp in &specs {
        let mut sp = sp.clone();
        sp["kind"] = json!(kind);
        q.enqueue(&sp)?;
    }
    let rows = q.list(&ubos_node::services::JobFilter {
        kind: Some(kind.clone()),
        limit: 1000,
        ..Default::default()
    })?;
    check_v(
        rows.len() == 20,
        "20 jobs in the job store",
        &json!(rows.len()),
    )?;
    Ok(json!({"evaluators": 2, "slots": 20, "jobs": jobs.len(), "enqueues": specs.len()}))
}

/// DSN-NOD-226, FR-EVT-041: after a 3-hour outage over an hourly schedule, `fire_all` runs
/// three jobs in slot order, `fire_once` the latest and `skip` none.
fn schedule_missed_slots(e: &Env) -> Result<Value, BpaError> {
    let st = sched_store(e)?;
    let down = schedule::parse("2026-10-16T10:30:00.000000Z")?;
    let up = schedule::parse("2026-10-16T13:30:00.000000Z")?;
    let mut out = json!({});
    for (policy, want) in [("fire_all", 3), ("fire_once", 1), ("skip", 0)] {
        let sid = uuid();
        let b = sched_body("0 * * * *", policy);
        schedule::evaluate(st.as_ref(), &sid, &b, down)?;
        let ev = schedule::evaluate(st.as_ref(), &sid, &b, up)?;
        let slots: Vec<String> = ev
            .specs
            .iter()
            .map(|s| {
                s["payload"]["slot"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        check_v(
            slots.len() == want,
            &format!("{policy}: {want} jobs"),
            &json!(slots),
        )?;
        let mut sorted = slots.clone();
        sorted.sort();
        check(sorted == slots, "in slot order")?;
        if policy == "fire_once" {
            check(slots == ["2026-10-16T13:00:00.000000Z"], "the latest slot")?;
        }
        schedule::mark_fired(st.as_ref(), &sid, &ev.firing)?;
        let recorded = st.slots(&sid, None, 100)?;
        check_v(
            recorded.len() == 3,
            "every missed slot recorded",
            &json!(recorded),
        )?;
        out[policy] = json!(slots);
    }
    Ok(out)
}

/// DSN-NOD-225, FR-EVT-044: a pause over 3 slots records them; on resume, `fire_once` runs
/// one catch-up job and skips the others, `fire_all` runs all three.
fn schedule_pause_resume(e: &Env) -> Result<Value, BpaError> {
    let st = sched_store(e)?;
    let mut out = json!({});
    for (policy, want) in [("fire_once", 1), ("fire_all", 3)] {
        let sid = uuid();
        let mut b = sched_body("0 * * * *", policy);
        schedule::evaluate(
            st.as_ref(),
            &sid,
            &b,
            schedule::parse("2026-10-16T10:30:00.000000Z")?,
        )?;
        b["paused"] = json!(true);
        let ev = schedule::evaluate(
            st.as_ref(),
            &sid,
            &b,
            schedule::parse("2026-10-16T13:30:00.000000Z")?,
        )?;
        check(ev.specs.is_empty(), "nothing fires while paused")?;
        check(
            st.slots(&sid, Some("paused"), 10)?.len() == 3,
            "3 slots recorded paused",
        )?;
        b["paused"] = json!(false);
        let ev = schedule::evaluate(
            st.as_ref(),
            &sid,
            &b,
            schedule::parse("2026-10-16T13:30:10.000000Z")?,
        )?;
        schedule::mark_fired(st.as_ref(), &sid, &ev.firing)?;
        check_v(
            ev.specs.len() == want && ev.specs.iter().all(|s| s["payload"]["caught_up"] == true),
            &format!("{policy}: {want} catch-up jobs"),
            &json!(ev.specs),
        )?;
        let again = schedule::evaluate(
            st.as_ref(),
            &sid,
            &b,
            schedule::parse("2026-10-16T13:30:20.000000Z")?,
        )?;
        check(again.specs.is_empty(), "no second catch-up")?;
        let statuses: Vec<String> = st
            .slots(&sid, None, 10)?
            .iter()
            .map(|x| x["status"].as_str().unwrap_or_default().to_string())
            .collect();
        out[policy] = json!(statuses);
    }
    check_v(
        out["fire_once"] == json!(["skipped", "skipped", "fired"]),
        "fire_once records the others skipped",
        &out,
    )?;
    Ok(out)
}

/// FR-EVT-042: a committed change of a schedule takes effect at the next evaluation; the
/// Server shows the new next slots at once, without a restart.
fn schedule_hot_change(e: &Env) -> Result<Value, BpaError> {
    let st = sched_store(e)?;
    let sid = uuid();
    let daily = json!({"rule": {"cron": "0 6 * * *"}, "target": {"kind": "port", "port": "mark", "target": "x"}, "run_as": "did:svc"});
    let t0 = schedule::parse("2026-10-16T07:00:00.000000Z")?;
    schedule::evaluate(st.as_ref(), &sid, &daily, t0)?;
    let mut hourly = daily.clone();
    hourly["rule"] = json!({"cron": "0 * * * *"});
    let ev = schedule::evaluate(
        st.as_ref(),
        &sid,
        &hourly,
        t0 + chrono::Duration::minutes(60),
    )?;
    check_v(
        ev.specs.len() == 1,
        "daily changed to hourly: the next firing within one hour",
        &json!(ev.specs),
    )?;
    // through the Server
    let id = uuid();
    let body = json!({"name": "hot", "rule": {"cron": "0 6 * * *"}, "time_zone": "UTC", "run_as": e.svc,
        "target": {"kind": "port", "branch": BR, "port": "mark", "target": "ubos://acme/Account/none", "params": {}}});
    e.c.commit("main", json!([{"op": "create", "class": "Schedule", "object_id": id, "slug": format!("hot-{}", &id[..8]), "body": body}]))?;
    let next = |c: &Client| -> Result<Value, BpaError> {
        Ok(c.get("/api/v1/admin/schedules")?["items"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|x| x["schedule_id"] == json!(id))
            .map(|x| x["next"].clone())
            .unwrap_or(Value::Null))
    };
    let n1 = next(&e.c)?;
    check_v(
        n1[0]
            .as_str()
            .is_some_and(|s| s.ends_with("T06:00:00.000000Z")),
        "listed with its daily slot",
        &n1,
    )?;
    let cur =
        e.c.get(&format!("/api/v1/acme/Schedule/hot-{}", &id[..8]))?;
    e.c.commit("main", json!([{"op": "update", "object_id": id, "base": cur["items"][0]["version_hash"], "body": {"rule": {"cron": "0 * * * *"}}}]))?;
    let n2 = next(&e.c)?;
    let within_hour = n2[0]
        .as_str()
        .and_then(|s| schedule::parse(s).ok())
        .is_some_and(|t| t - now() <= chrono::Duration::hours(1));
    check_v(within_hour, "the committed hourly rule shows at once", &n2)?;
    // disabled at the end: a disabled schedule leaves no schedule work behind
    let cur =
        e.c.get(&format!("/api/v1/acme/Schedule/hot-{}", &id[..8]))?;
    e.c.commit("main", json!([{"op": "update", "object_id": id, "base": cur["items"][0]["version_hash"], "body": {"enabled": false}}]))?;
    Ok(json!({"before": n1, "after": n2}))
}

/// DSN-NOD-224, FR-EVT-044 through the Server: an every-minute schedule fires its next slot
/// once, as its service principal, and an operator pauses it.
fn schedule_through_server(e: &Env) -> Result<Value, BpaError> {
    let slug = e.slug("tick");
    e.c.accounts(std::slice::from_ref(&slug))?;
    let id = uuid();
    e.c.commit("main", json!([{"op": "create", "class": "Schedule", "object_id": id, "slug": format!("tick-{}", &id[..8]),
        "body": {"name": "tick", "rule": {"cron": "* * * * *"}, "run_as": e.svc,
            "target": {"kind": "port", "branch": BR, "port": "mark", "target": format!("ubos://acme/Account/{slug}"), "params": {"tag": "$event.slot"}}}}]))?;
    let job = wait(75.0, || {
        Ok(e.c
            .jobs("kind=schedule&state=done")?
            .into_iter()
            .find(|j| j["payload"]["schedule"] == json!(id)))
    })?
    .ok_or_else(|| err("no firing within 75 s"))?;
    let slot = job["payload"]["slot"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let lag = schedule::parse(&slot).ok().and_then(|s| {
        e.c.get(&format!(
            "/api/v1/admin/jobs/{}",
            job["job_id"].as_str().unwrap_or_default()
        ))
        .ok()
        .map(|_| (now() - s).num_milliseconds())
    });
    check(
        e.c.account(&slug)?.is_some_and(|b| b["tag"] == json!(slot)),
        "its target ran with the slot",
    )?;
    let slots = e.c.get(&format!("/api/v1/admin/schedules/{id}/slots"))?;
    check_v(
        slots["items"].as_array().is_some_and(|a| {
            a.len() == 1 && a[0]["status"] == "fired" && a[0]["job_id"] == job["job_id"]
        }),
        "the slot recorded fired with its job",
        &slots,
    )?;
    let info = e.host("process_info", json!({"process_id": job["job_id"]}))?;
    check_v(
        info["initiator"] == json!(e.svc),
        "run as the service principal",
        &info,
    )?;
    let (st, r) = e.c.call(
        "POST",
        &format!("/api/v1/admin/schedules/{id}/pause"),
        &Value::Null,
    )?;
    check_v(st == 200, "pause", &r)?;
    let l = e.c.get("/api/v1/admin/schedules")?;
    check(
        l["items"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|x| x["schedule_id"] == json!(id) && x["paused"] == true),
        "listed paused",
    )?;
    check(
        e.c.log_kinds()?.iter().any(|k| k == "admin.schedule_pause"),
        "the pause audited",
    )?;
    let cur =
        e.c.get(&format!("/api/v1/acme/Schedule/tick-{}", &id[..8]))?;
    e.c.commit("main", json!([{"op": "update", "object_id": id, "base": cur["items"][0]["version_hash"], "body": {"enabled": false}}]))?;
    // the leader forgets the disabled schedule at its next evaluation
    std::thread::sleep(Duration::from_millis(2500));
    Ok(json!({"slot": slot, "observed_ms_after_slot": lag}))
}

// ---- the benchmarks ----

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if v.is_empty() {
        return f64::NAN;
    }
    let i = ((p / 100.0) * (v.len() as f64 - 1.0)).round() as usize;
    (v[i.min(v.len() - 1)] * 10.0).round() / 10.0
}

fn stats(mut v: Vec<f64>) -> Value {
    let n = v.len();
    let mean = if n > 0 {
        v.iter().sum::<f64>() / n as f64
    } else {
        f64::NAN
    };
    json!({"n": n, "p50": pct(&mut v, 50.0), "p95": pct(&mut v, 95.0), "p99": pct(&mut v, 99.0), "max": pct(&mut v, 100.0),
        "mean": (mean * 10.0).round() / 10.0})
}

/// Latencies of `n` pings, one at a time: from the commit of the ping to the commit of
/// its handler, by the kernel's commit times (the handler's commit bounds its start from
/// above), in ms; and how many deliveries were not done within 10 s. The suite waits on the
/// delivery job (the job store), so the measurement adds no load to the VAE.
fn ping_latencies(e: &Env, prefix: &str, n: usize) -> Result<(Vec<f64>, usize), BpaError> {
    let slugs: Vec<String> = (0..n).map(|i| format!("{prefix}-{i}")).collect();
    for chunk in slugs.chunks(100) {
        e.c.accounts(chunk)?;
    }
    let mut pairs = Vec::new();
    let mut missing = 0;
    for (i, s) in slugs.iter().enumerate() {
        let r = e.c.port(s, "ping", json!({"n": format!("{i:05}")}))?;
        let pid = r["process_id"].as_str().unwrap_or_default().to_string();
        let effect = format!("{pid}/1");
        let job = ubos_node::platform::delivery_job_id(&e.on_ping, &effect)?;
        let t0 = Instant::now();
        let done = loop {
            let j = e.c.get(&format!("/api/v1/admin/jobs/{job}"))?;
            if j["state"] == "done" {
                break true;
            }
            if t0.elapsed() > Duration::from_secs(10) {
                break false;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        if done {
            let handler = ubos_node::platform::delivery_process_id(&e.on_ping, &effect)?;
            pairs.push((pid, handler));
        } else {
            missing += 1;
        }
    }
    // the commit times, read once the sampling is over
    let tx = |cseq: u64| -> Result<chrono::DateTime<chrono::Utc>, BpaError> {
        let c = e.host("commit_info", json!({"branch": BR, "time": cseq}))?;
        schedule::parse(c["tx_time"].as_str().unwrap_or_default())
    };
    let cseq = |pid: &str| -> Result<u64, BpaError> {
        let info = e.host("process_info", json!({"process_id": pid}))?;
        info["commits"][0]["cseq"]
            .as_u64()
            .ok_or_else(|| err(format!("no commit of {pid}")))
    };
    let mut lat = Vec::new();
    for (ping, handler) in pairs {
        let ms = (tx(cseq(&handler)?)? - tx(cseq(&ping)?)?)
            .num_microseconds()
            .unwrap_or(0) as f64
            / 1000.0;
        lat.push(ms);
    }
    Ok((lat, missing))
}

/// Background load: `workers` clients calling a command port on their own accounts, each
/// paced to `per_s` calls per second (`0` for as fast as the VAE answers).
struct Load {
    stop: Arc<std::sync::atomic::AtomicBool>,
    ops: Arc<std::sync::atomic::AtomicU64>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl Load {
    fn start(e: &Env, accounts: &[String], per_s: f64) -> Load {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ops = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let workers = accounts
            .iter()
            .map(|s| {
                let (c, stop, ops, s) = (e.c.clone(), stop.clone(), ops.clone(), s.clone());
                std::thread::spawn(move || {
                    let mut k = 0u64;
                    let t0 = Instant::now();
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        k += 1;
                        if c.port(&s, "mark", json!({"tag": format!("l{k}")})).is_ok() {
                            ops.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        if per_s > 0.0 {
                            let due = Duration::from_secs_f64(k as f64 / per_s);
                            if let Some(d) = due.checked_sub(t0.elapsed()) {
                                std::thread::sleep(d);
                            }
                        }
                    }
                })
            })
            .collect();
        Load { stop, ops, workers }
    }
    fn ops(&self) -> u64 {
        self.ops.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn stop(self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for w in self.workers {
            let _ = w.join();
        }
    }
}

/// MET-PERF-017 (NR-PERF-011): commit to handler start, idle and under a background load
/// of command-port calls at `fraction` of the VAE's measured capacity (WL-MIXED-OLTP's
/// convention of 70 % of the maximum sustainable throughput), in ms.
fn bench_handler(e: &Env, samples: usize, fraction: f64) -> Result<Value, BpaError> {
    let (idle, idle_missing) = ping_latencies(e, "idle", samples / 3)?;
    let load: Vec<String> = (0..4).map(|i| format!("load-{i}")).collect();
    e.c.accounts(&load)?;
    // the capacity: four clients as fast as the VAE answers, for 5 s
    let l = Load::start(e, &load, 0.0);
    std::thread::sleep(Duration::from_millis(1000));
    let (o0, t0) = (l.ops(), Instant::now());
    std::thread::sleep(Duration::from_secs(5));
    let capacity = (l.ops() - o0) as f64 / t0.elapsed().as_secs_f64();
    l.stop();
    let rate = capacity * fraction;
    let l = Load::start(e, &load, rate / load.len() as f64);
    std::thread::sleep(Duration::from_millis(1000));
    let (o0, t0) = (l.ops(), Instant::now());
    let (lat, missing) = ping_latencies(e, "lat", samples)?;
    let achieved = (l.ops() - o0) as f64 / t0.elapsed().as_secs_f64();
    l.stop();
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    let mut loaded = stats(lat);
    loaded["missing"] = json!(missing);
    let mut idle = stats(idle);
    idle["missing"] = json!(idle_missing);
    let met = missing == 0 && loaded["p95"].as_f64().is_some_and(|p| p <= 500.0);
    Ok(
        json!({"idle": idle, "loaded": loaded, "capacity_port_calls_per_s": r1(capacity), "load_fraction": fraction,
        "load_target_port_calls_per_s": r1(rate), "load_port_calls_per_s": r1(achieved),
        "measure": "commit time of the ping to commit time of its handler (kernel tx_time)",
        "target_p95_ms": 500, "met": met}),
    )
}

/// NR-PERF-012: 1,000 future-dated versions; the lag of each `BecameEffective` event's
/// release after its valid-from instant, in ms.
fn bench_timers(e: &Env, n: usize) -> Result<Value, BpaError> {
    let start = now() + chrono::Duration::seconds(15);
    let span_ms = 45_000i64;
    let mut due = std::collections::HashMap::new();
    let mut changes = Vec::new();
    for i in 0..n {
        let at = start + chrono::Duration::milliseconds(span_ms * i as i64 / n as i64);
        let oid = uuid();
        due.insert(oid.clone(), at);
        changes.push(json!({"op": "create", "class": "Rate", "object_id": oid, "slug": format!("t-{}-{i}", &oid[..8]), "body": {"value": "1"}, "valid": {"from": iso(at)}}));
        if changes.len() == 100 {
            e.c.commit("main", json!(std::mem::take(&mut changes)))?;
        }
    }
    if !changes.is_empty() {
        e.c.commit("main", json!(changes))?;
    }
    check(now() < start, "all committed before the first valid-from")?;
    let deadline = Instant::now() + Duration::from_secs(15 + 45 + 30);
    let mut lags = std::collections::HashMap::new();
    while Instant::now() < deadline && lags.len() < n {
        for ev in e.c.events("type=BecameEffective")? {
            let oid = ev["object_id"].as_str().unwrap_or_default();
            if let (Some(d), Some(at)) = (due.get(oid), ev["released_at"].as_str())
                && let Ok(t) = schedule::parse(at)
            {
                lags.entry(oid.to_string())
                    .or_insert((t - *d).num_milliseconds() as f64);
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    let early = lags.values().filter(|x| **x < 0.0).count();
    let within = lags.values().filter(|x| **x <= 5000.0).count();
    let mut s = stats(lags.values().copied().collect());
    s["versions"] = json!(n);
    s["fired"] = json!(lags.len());
    s["before_valid_from"] = json!(early);
    s["within_5s"] = json!(within);
    s["target"] = json!("p99 <= 5000 ms");
    s["met"] =
        json!(lags.len() == n && early == 0 && s["p99"].as_f64().is_some_and(|p| p <= 5000.0));
    Ok(s)
}

/// NR-DUR-006 (PostgreSQL): effects committed while no node releases them (a crash between
/// commit and release) are all released after a node starts; the seconds to the last one.
/// It runs before any Server of this process exists (a Server serves every tenant of the
/// registry and would release them first).
fn bench_recovery(database: Option<&str>, n: usize) -> Result<Value, BpaError> {
    let Some(url) = database else {
        return Ok(json!({"skipped": "needs PostgreSQL"}));
    };
    let node = &Arc::new(ubos_node::Node::connect(url)?);
    let slug = format!("evr{}", &uuid().replace('-', "")[..12]);
    let t = seed_pg(node, &slug)?;
    // commits without a releaser: the VAE in server mode, nothing claims its outbox
    {
        let mut v = node.open_vae(&t, "acme")?;
        v.set_server_mode(true, false);
        let mut changes = vec![logic(&uuid())];
        for i in 0..10 {
            changes.push(json!({"op": "create", "class": "Account", "object_id": uuid(), "slug": format!("r-{i}"), "body": {"name": "r", "status": "open"}}));
        }
        v.call("commit", &json!({"branch": BR, "changes": changes}))?;
        for k in 0..n {
            v.call(
                "execute",
                &json!({"branch": BR, "target": format!("ubos://acme/Account/r-{}", k % 10), "port": "ping", "params": {"n": format!("{k:05}")},
                    "process": {"process_id": uuid()}}),
            )?;
        }
        let pending = v.call("outbox", &json!({}))?;
        let k = pending["effects"].as_array().map_or(0, Vec::len);
        check_v(
            k >= n,
            "the effects wait in the outbox",
            &json!({"pending": k, "keys": pending.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())}),
        )?;
    }
    let addr = free_port()?;
    let started = Instant::now();
    let child = spawn_node(url, &slug, &addr)?;
    let c = Client::sign_in(&child.addr)?;
    let all = wait(300.0, || {
        let ev = c.events("type=Pinged")?;
        Ok((ev.len() >= n).then_some(ev.len()))
    })?;
    let secs = started.elapsed().as_secs_f64();
    let got = all.unwrap_or(0);
    let ev = c.events("type=Pinged")?;
    let distinct: std::collections::BTreeSet<String> = ev
        .iter()
        .filter_map(|x| {
            x["event_id"]
                .as_str()
                .or_else(|| x["effect_id"].as_str())
                .map(str::to_string)
        })
        .collect();
    drop(child);
    Ok(
        json!({"effects": n, "released": got, "distinct": distinct.len(), "seconds_after_start": (secs * 10.0).round() / 10.0,
        "target": "100% within 300 s", "met": got == n && distinct.len() == n && secs <= 300.0}),
    )
}

// ---- running ----

type Case = (
    &'static str,
    &'static str,
    fn(&Env) -> Result<Value, BpaError>,
);

const EVENTS: &[Case] = &[
    ("release_order", "DSN-NOD-201", release_order),
    ("outbox_claims", "DSN-NOD-201", outbox_claims),
    ("subscription_scope", "DSN-NOD-203", subscription_scope),
    ("handler_process_once", "DSN-NOD-204", handler_process_once),
    ("dead_letters", "DSN-NOD-209", dead_letters),
    ("became_effective", "DSN-NOD-208", became_effective),
    (
        "event_log_query_retention",
        "DSN-NOD-202",
        event_log_query_retention,
    ),
    ("leader_leases", "DSN-NOD-228", leader_leases),
    ("traces", "NR-OBS-001", traces),
    ("crash_recovery", "NR-DUR-006", crash_recovery),
    ("failover", "DSN-NOD-228", failover),
];

const JOBS: &[Case] = &[
    ("jobs_are_objects", "DSN-NOD-220", jobs_are_objects),
    ("lease_fencing", "DSN-NOD-221", lease_fencing),
    ("retries_and_poison", "DSN-NOD-223", retries_and_poison),
    (
        "priorities_and_limits",
        "DSN-NOD-229",
        priorities_and_limits,
    ),
    ("sweeper_restores", "FR-EVT-032", sweeper_restores),
    ("bulk_cancel", "DSN-NOD-230", bulk_cancel),
    ("idempotent_execution", "DSN-NOD-221", idempotent_execution),
    ("schedule_rules", "FR-EVT-041", schedule_rules),
    (
        "schedule_exactly_once",
        "DSN-NOD-224",
        schedule_exactly_once,
    ),
    (
        "schedule_missed_slots",
        "DSN-NOD-226",
        schedule_missed_slots,
    ),
    (
        "schedule_pause_resume",
        "DSN-NOD-225",
        schedule_pause_resume,
    ),
    ("schedule_hot_change", "FR-EVT-042", schedule_hot_change),
    (
        "schedule_through_server",
        "DSN-NOD-224",
        schedule_through_server,
    ),
];

fn result(name: &str, clause: &str, r: Result<Value, BpaError>, ms: u128) -> Value {
    match r {
        Ok(v) if v.get("skipped").is_some() => {
            json!({"case": name, "clause": clause, "result": "skip", "ms": ms, "detail": v["skipped"]})
        }
        Ok(v) => json!({"case": name, "clause": clause, "result": "pass", "ms": ms, "evidence": v}),
        Err(e) => {
            json!({"case": name, "clause": clause, "result": "fail", "ms": ms, "detail": e.to_string()})
        }
    }
}

fn run(suite: &str, database: Option<&str>) -> Result<Vec<Value>, BpaError> {
    let cases = match suite {
        "events" => EVENTS,
        "jobs" => JOBS,
        other => return Err(err(format!("unknown suite {other}"))),
    };
    let e = env(database)?;
    let mut out = Vec::new();
    for (name, clause, f) in cases {
        let t = Instant::now();
        let r = f(&e);
        let v = result(name, clause, r, t.elapsed().as_millis());
        eprintln!(
            "{:4} {name} ({} ms){}",
            match v["result"].as_str() {
                Some("pass") => "ok",
                Some("skip") => "skip",
                _ => "FAIL",
            },
            v["ms"],
            v["detail"]
                .as_str()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default()
        );
        out.push(v);
    }
    // the Server's stores own runtimes that must not be dropped inside its own; the process
    // ends here anyway
    std::mem::forget(e);
    Ok(out)
}

fn signed(mut report: Value) -> Value {
    let seed: [u8; 32] = crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report).unwrap_or_default();
    let sig = crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] =
        json!({"alg": "ed25519", "key": hex(&crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    report
}

fn write(path: Option<String>, report: &Value) {
    if let Some(p) = path {
        if let Some(d) = std::path::Path::new(&p).parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(
            &p,
            serde_json::to_string_pretty(report).unwrap_or_default() + "\n",
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let flag = |k: &str| args.iter().any(|a| a == k);
    let db = arg("--database");
    if flag("--serve") {
        if let Err(e) = serve(
            &db.unwrap_or_default(),
            &arg("--tenant").unwrap_or_default(),
            &arg("--listen").unwrap_or_default(),
        ) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let environment = json!({"backend": if db.is_some() { "postgresql" } else { "memory" }, "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH, "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)});
    let start = Instant::now();
    if flag("--bench") {
        let num = |k: &str, d: usize| arg(k).and_then(|x| x.parse().ok()).unwrap_or(d);
        let r = (|| -> Result<Value, BpaError> {
            eprintln!("NR-DUR-006 …");
            let d = bench_recovery(db.as_deref(), num("--effects", 1000))?;
            eprintln!("  {d}");
            let e = env(db.as_deref())?;
            eprintln!("MET-PERF-017 …");
            let fraction = arg("--load").and_then(|x| x.parse().ok()).unwrap_or(0.7);
            let h = bench_handler(&e, num("--samples", 300), fraction)?;
            eprintln!("  {h}");
            eprintln!("NR-PERF-012 …");
            let t = bench_timers(&e, num("--timers", 1000))?;
            eprintln!("  {t}");
            std::mem::forget(e);
            Ok(json!({"MET-PERF-017": h, "NR-PERF-012": t, "NR-DUR-006": d}))
        })();
        let metrics = match r {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };
        let report = signed(
            json!({"benchmark": "VER-BENCH-2000 (NOD part: MET-PERF-017, NR-PERF-012, NR-DUR-006)", "step": "PH-1 15e",
            "decision": "DEC-076", "implementation": {"crate": "ubos-node", "version": env!("CARGO_PKG_VERSION")},
            "environment": environment, "metrics": metrics, "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0}),
        );
        write(arg("--report"), &report);
        println!(
            "{}",
            serde_json::to_string_pretty(&report["metrics"]).unwrap_or_default()
        );
        return;
    }
    let suite = arg("--suite").unwrap_or_else(|| "events".into());
    let results = match run(&suite, db.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let passed = results.iter().filter(|r| r["result"] == "pass").count();
    let skipped = results.iter().filter(|r| r["result"] == "skip").count();
    let name = if suite == "jobs" {
        "SUITE-NOD-JOBS"
    } else {
        "SUITE-NOD-EVENTS"
    };
    let report = signed(json!({
        "suite": name, "step": "PH-1 15e", "decision": "DEC-076",
        "implementation": {"crate": "ubos-node", "version": env!("CARGO_PKG_VERSION")},
        "environment": environment, "cases": results.len(), "passed": passed, "skipped": skipped, "results": results,
        "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    }));
    write(arg("--report"), &report);
    println!(
        "{name}: {passed} of {} cases passed, {skipped} skipped ({} s)",
        report["cases"], report["seconds"]
    );
    if passed + skipped != report["cases"].as_u64().unwrap_or(0) as usize {
        std::process::exit(1);
    }
}
