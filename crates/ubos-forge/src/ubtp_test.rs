//! `forge test --backend ubtp` (PH-1 step 18d-2, DEC-082): the scenario tests of a project
//! run over UBTP against a `dev`-preset Server on PostgreSQL, through the Rust SDK, over
//! WebSocket or HTTP (EXIT-1-19).
//!
//! The runner starts `ubos-server` with the project's connectors as `dev.connectors` and an
//! administrator, then builds every test its own fixture on a fresh tenant: the node clock
//! is set to the fixture clock, the tenant is bootstrapped with the project's genesis at
//! that clock (`dev.test_clock`), the seed is committed on `main` and the logic on
//! `feature/forge` by raw commits, and the clock moves to the test's start. Each step is
//! mapped to a UBTP message:
//!
//! | Step | UBTP |
//! |---|---|
//! | `read` | `query read` of the object's URI (the runner knows the slugs of the seed and of the objects the test creates); `tx_time` is the `time` selector, a commit sequence becomes the `commit` selector, `provenance` is the last commit of the object's `history` |
//! | `commit` (also `concurrent`) | `mutate commit`, with the `message` and `reason` of the step's process |
//! | `query`, `history`, `diff` | `query` of the same kind |
//! | `create_branch`, `change_set`, `merge`, `revert`, `bulk` | the versioning requests of `mutate` (STD-PROTO-014) |
//! | `commit_info`, `export_state`, `merge_preview`, `read_ledger`, `process_info`, `metering`, `journal`, `replay` | the introspection requests of `query` (STD-PROTO-015) |
//! | `effective_class`, `dispatch` | `query class`, `query dispatch` of the installed model; an expected error is a `bootstrap` whose genesis the node rejects |
//! | `execute` of a port | `mutate port`; the result carries the process's metering; a pending change (a `require` decision) is `RULE.DECISION_REQUIRES` with its steps, as in process |
//! | `explain_cascade` | `query explain` of kind `cascade` (STD-PROTO-011) |
//! | `released_effects` | the emits of events subscriptions on `main` and `feature/forge` |
//! | `advance` | `POST /api/v1/admin/clock {advance}` |
//!
//! Process identifiers a test names are mapped to the ones the Server assigns (from the
//! result, or from the error of a failed `mutate`, STD-PROTO-010 since BPA 1.0-rc5) and
//! mapped back in the results. A step that uses a replay test hook (`drop`, `tamper`) is
//! outside the Protocol (STD-PROTO-015): its test is reported as verified in process only.

use crate::check::{CLOCK, asset_object_id};
use crate::project::Project;
use crate::runner::resolve;
use crate::test::{Report, TestResult, expectation};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use ubos_dvm::BpaError;
use ubos_sdk::{Auth, Binding, Client, Config, Selectors, Subscription, Target, Update};

/// Options of a run.
pub struct Options {
    /// The `ubos-server` binary.
    pub server: PathBuf,
    /// The PostgreSQL connection string (the tenants of the run are created there).
    pub database: String,
    /// The binding: WebSocket or HTTP.
    pub binding: Binding,
}

/// The outcome of a run: the report and the tests verified in process only.
pub struct Outcome {
    /// The report, as `forge test` writes it.
    pub report: Report,
    /// Tests whose steps use a test hook outside the Protocol: `(test, reason)`.
    pub in_process_only: Vec<(String, String)>,
}

const ADMIN: &str = "admin";
const PASSWORD: &str = "scenario runner password 18d";

/// A code as the kernel's errors hold it: the registered one, or kept for the run.
fn code(c: &str) -> &'static str {
    ubos_sdk::registry::REGISTRY
        .iter()
        .find(|r| r.1 == c)
        .map(|r| r.1)
        .unwrap_or_else(|| Box::leak(c.to_string().into_boxed_str()))
}

fn err(c: &str, message: impl Into<String>) -> BpaError {
    BpaError::new(code(c), message.into())
}

fn sdk_error(e: ubos_sdk::Error) -> BpaError {
    let mut b = BpaError::new(code(&e.code_text), e.message.clone());
    if !e.details.is_null() {
        b.details = Some(*e.details.clone());
    }
    if let Some(p) = e.process_id {
        let mut d = b.details.take().unwrap_or_else(|| json!({}));
        if d.is_object() {
            d["process_id"] = json!(p);
        }
        b.details = Some(d);
    }
    b
}

/// A blocking HTTP/1.1 request with a JSON body; the status and the JSON reply.
fn http(
    addr: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
    token: Option<&str>,
) -> Result<(u16, Value), BpaError> {
    let io = |e: std::io::Error| err("SDK.TRANSPORT", e.to_string());
    let mut s = std::net::TcpStream::connect(addr).map_err(io)?;
    s.set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(io)?;
    let payload = body.map(Value::to_string).unwrap_or_default();
    let auth = token
        .map(|t| format!("authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nhost: {addr}\r\nconnection: close\r\ncontent-type: application/json\r\n{auth}content-length: {}\r\n\r\n{payload}",
        payload.len()
    );
    s.write_all(req.as_bytes()).map_err(io)?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).map_err(io)?;
    let text = String::from_utf8_lossy(&buf);
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        // de-chunk
        let mut out = String::new();
        let mut r = rest;
        while let Some((size, tail)) = r.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 || tail.len() < n {
                break;
            }
            out.push_str(&tail[..n]);
            r = tail[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        rest.to_string()
    };
    Ok((status, serde_json::from_str(&body).unwrap_or(Value::Null)))
}

/// The running Server of a run.
struct Node {
    child: Child,
    addr: String,
    token: String,
    work: PathBuf,
    database: String,
    vae: String,
    server: PathBuf,
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> Result<u16, BpaError> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| err("SDK.TRANSPORT", e.to_string()))
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

impl Node {
    fn config(&self, tenant: &str, clock: Option<&str>, connectors: Option<&str>) -> String {
        let mut c = format!(
            "version = 1\n[server]\nlisten = {}\npreset = \"dev\"\ntenant = {}\nvae = {}\n[database]\nurl = {}\n",
            quote(&self.addr),
            quote(tenant),
            quote(&self.vae),
            quote(&self.database)
        );
        if clock.is_some() || connectors.is_some() {
            c.push_str("[dev]\n");
            if let Some(t) = clock {
                c.push_str(&format!("test_clock = {}\n", quote(t)));
            }
            if let Some(j) = connectors {
                c.push_str(&format!("connectors = {}\n", quote(j)));
            }
        }
        c
    }

    fn start(
        o: &Options,
        project: &Project,
        tenant: &str,
        genesis: &Value,
        connectors: Option<&Value>,
    ) -> Result<Node, BpaError> {
        let io = |e: std::io::Error| err("SDK.TRANSPORT", e.to_string());
        let work = std::env::temp_dir().join(format!("forge-ubtp-{tenant}"));
        std::fs::create_dir_all(&work).map_err(io)?;
        let mut node = Node {
            child: Command::new("true").spawn().map_err(io)?,
            addr: format!("127.0.0.1:{}", free_port()?),
            token: String::new(),
            work,
            database: o.database.clone(),
            vae: project.manifest.vae.clone(),
            server: o.server.clone(),
        };
        let cfg = node.work.join("node.toml");
        let g = node.work.join("genesis.json");
        let conns = connectors.map(Value::to_string);
        std::fs::write(&cfg, node.config(tenant, Some(CLOCK), conns.as_deref())).map_err(io)?;
        std::fs::write(&g, genesis.to_string()).map_err(io)?;
        let log = std::fs::File::create(node.work.join("server.log"))
            .map_err(|e| err("SDK.TRANSPORT", e.to_string()))?;
        node.child = Command::new(&o.server)
            .arg("--config")
            .arg(&cfg)
            .arg("--genesis")
            .arg(&g)
            .arg("--admin")
            .arg(format!("{ADMIN}:{PASSWORD}"))
            .stdout(Stdio::from(
                log.try_clone()
                    .map_err(|e| err("SDK.TRANSPORT", e.to_string()))?,
            ))
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| err("SDK.TRANSPORT", format!("{}: {e}", o.server.display())))?;
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            if let Ok((200, _)) = http(&node.addr, "GET", "/readyz", None, None) {
                break;
            }
            if Instant::now() > until || node.child.try_wait().ok().flatten().is_some() {
                let log = std::fs::read_to_string(node.work.join("server.log")).unwrap_or_default();
                return Err(err(
                    "SDK.TRANSPORT",
                    format!("the node did not start:\n{log}"),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let (_, signed) = http(
            &node.addr,
            "POST",
            "/api/v1/identity/sign-in",
            Some(&json!({"username": ADMIN, "password": PASSWORD})),
            None,
        )?;
        node.token = signed["token"]
            .as_str()
            .ok_or_else(|| err("IAM.DENIED", format!("sign-in: {signed}")))?
            .to_string();
        Ok(node)
    }

    fn clock(&self, body: Value) -> Result<Value, BpaError> {
        let (status, v) = http(
            &self.addr,
            "POST",
            "/api/v1/admin/clock",
            Some(&body),
            Some(&self.token),
        )?;
        if status != 200 {
            return Err(err("NOD.CLOCK", format!("clock {body}: {status} {v}")));
        }
        Ok(v)
    }

    /// Bootstrap a tenant with a genesis at a clock; the output on failure.
    fn bootstrap(&self, tenant: &str, genesis: &Value, clock: &str) -> Result<(), String> {
        let cfg = self.work.join(format!("{tenant}.toml"));
        let g = self.work.join(format!("{tenant}.genesis.json"));
        std::fs::write(&cfg, self.config(tenant, Some(clock), None)).map_err(|e| e.to_string())?;
        std::fs::write(&g, genesis.to_string()).map_err(|e| e.to_string())?;
        let out = Command::new(&self.server)
            .arg("bootstrap")
            .arg("--config")
            .arg(&cfg)
            .arg("--genesis")
            .arg(&g)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ))
        }
    }
}

/// The state of one test over UBTP.
struct Run<'a> {
    c: Client,
    node: &'a Node,
    project: &'a Project,
    vae: String,
    /// object id → (class, slug)
    ids: HashMap<String, (String, String)>,
    /// the test's process id → the Server's, and back
    pids: HashMap<String, String>,
    back: HashMap<String, String>,
    /// change set id → its target branch
    change_sets: HashMap<String, String>,
    subs: Vec<Subscription>,
    effects: Vec<Value>,
    genesis_value_types: Option<Value>,
    tenant_seq: u32,
    /// Commits the Server's fixture has on `main` beyond the in-process fixture's (the
    /// genesis Buks): commit sequences are reported relative to the fixture.
    offset: u64,
}

fn s<'v>(v: &'v Value, k: &str) -> Option<&'v str> {
    v.get(k).and_then(Value::as_str)
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut m = Map::new();
    for k in keys {
        if let Some(x) = v.get(*k).filter(|x| !x.is_null()) {
            m.insert((*k).to_string(), x.clone());
        }
    }
    Value::Object(m)
}

impl Run<'_> {
    fn body(&self, extra: Value) -> Value {
        let mut b = json!({"vae": self.vae});
        if let (Some(o), Value::Object(e)) = (b.as_object_mut(), extra) {
            o.extend(e);
        }
        b
    }

    async fn query(&self, extra: Value) -> Result<Value, BpaError> {
        self.c
            .request("query", self.body(extra), None)
            .await
            .map_err(sdk_error)
    }

    async fn one(&self, extra: Value) -> Result<Value, BpaError> {
        Ok(self.query(extra).await?["items"][0].clone())
    }

    async fn mutate(&mut self, extra: Value, test_pid: Option<&str>) -> Result<Value, BpaError> {
        let r = self.c.request("mutate", self.body(extra), None).await;
        let pid = match &r {
            Ok(b) => s(b, "process_id").map(str::to_string),
            Err(e) => e.process_id.clone(),
        };
        if let (Some(t), Some(p)) = (test_pid, pid) {
            self.pids.insert(t.to_string(), p.clone());
            self.back.insert(p, t.to_string());
        }
        r.map_err(sdk_error)
    }

    fn pid(&self, test_pid: &str) -> String {
        self.pids
            .get(test_pid)
            .cloned()
            .unwrap_or_else(|| test_pid.to_string())
    }

    /// Server process ids replaced by the test's, and commit sequences made relative to
    /// the fixture, in a result.
    fn unmap(&self, v: Value) -> Value {
        match v {
            Value::String(t) => Value::String(self.back.get(&t).cloned().unwrap_or(t)),
            Value::Array(a) => Value::Array(a.into_iter().map(|x| self.unmap(x)).collect()),
            Value::Object(m) => Value::Object(
                m.into_iter()
                    .map(|(k, x)| {
                        let x = match (k.as_str(), x.as_u64()) {
                            ("cseq" | "head", Some(n)) if n >= self.offset => {
                                json!(n - self.offset)
                            }
                            _ => self.unmap(x),
                        };
                        (k, x)
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    /// A commit sequence of the test, on the Server.
    fn seq(&self, v: &Value) -> Value {
        match v.as_u64() {
            Some(n) => json!(n + self.offset),
            None => v.clone(),
        }
    }

    fn learn(&mut self, changes: &Value) {
        for ch in changes.as_array().into_iter().flatten() {
            if s(ch, "op") == Some("create")
                && let (Some(id), Some(class), Some(slug)) =
                    (s(ch, "object_id"), s(ch, "class"), s(ch, "slug"))
            {
                self.ids
                    .insert(id.to_string(), (class.to_string(), slug.to_string()));
            }
        }
    }

    fn uri(&self, oid: &str) -> Result<String, BpaError> {
        let (class, slug) = self.ids.get(oid).ok_or_else(|| {
            err(
                "IAM.NOT_FOUND",
                format!("the runner knows no slug of {oid}"),
            )
        })?;
        Ok(format!("ubos://{}/{class}/{slug}", self.vae))
    }

    /// The effects released so far: the emits of the events subscriptions, drained until
    /// they are quiet.
    async fn released(&mut self) -> Value {
        let until = Instant::now() + Duration::from_secs(4);
        loop {
            let mut got = false;
            for sub in &mut self.subs {
                while let Some(Update::Emit(b)) = sub.next_within(Duration::from_millis(300)).await
                {
                    got = true;
                    for e in b["events"].as_array().into_iter().flatten() {
                        self.effects
                            .push(pick(e, &["type", "object_id", "payload", "event_id"]));
                    }
                }
            }
            if !got || Instant::now() > until {
                break;
            }
        }
        json!({"effects": self.effects})
    }

    async fn step(
        &mut self,
        op: &str,
        i: &Value,
        concurrent: Option<u64>,
    ) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let test_pid = i
            .get("process")
            .and_then(|p| s(p, "process_id"))
            .or_else(|| s(i, "process_id"))
            .map(str::to_string);
        match op {
            "read" => {
                let oid = s(i, "object_id").unwrap_or_default().to_string();
                let mut b = json!({"read": self.uri(&oid)?, "branch": branch});
                if let Some(a) = i.get("asof").filter(|x| !x.is_null()) {
                    b["asof"] = a.clone();
                }
                if let Some(t) = i.get("tx_time").filter(|x| !x.is_null()) {
                    b["time"] = t.clone();
                }
                if let Some(n) = i.get("time").and_then(Value::as_u64) {
                    // a commit sequence: the commit it names
                    let c = self
                        .one(json!({"commit_info": {}, "branch": branch, "time": n + self.offset}))
                        .await?;
                    b["commit"] = c["commit_id"].clone();
                    b.as_object_mut().map(|m| m.remove("branch"));
                }
                if let Some(c) = i.get("commit").filter(|x| !x.is_null()) {
                    b["commit"] = c.clone();
                }
                let mut item = self.one(b).await?;
                if i.get("provenance") == Some(&json!(true)) {
                    let mut h = json!({"history": {"target": oid}, "branch": branch});
                    if let Some(t) = i.get("tx_time").filter(|x| !x.is_null()) {
                        h["history"]["to"] = t.clone();
                    }
                    let commits = self.query(h).await?;
                    let last = commits["items"]
                        .as_array()
                        .and_then(|a| a.last())
                        .cloned()
                        .unwrap_or(Value::Null);
                    item["provenance"] = json!({"branch": branch, "cseq": last["cseq"], "commit_id": last["commit_id"]});
                }
                Ok(item)
            }
            "commit" => {
                let changes = i.get("changes").cloned().unwrap_or(json!([]));
                self.learn(&changes);
                let mut body = json!({"branch": branch, "commit": {"changes": changes}});
                // the message and reason of the test's process go with the mutation
                // (STD-PROTO-010; `audit_level: strict` needs both, SCN-009)
                for k in ["message", "reason"] {
                    if let Some(v) = i
                        .get("process")
                        .and_then(|p| p.get(k))
                        .filter(|v| !v.is_null())
                    {
                        body[k] = v.clone();
                    }
                }
                if let Some(n) = concurrent {
                    let mut tasks = Vec::new();
                    for _ in 0..n {
                        let (c, b) = (self.c.clone(), self.body(body.clone()));
                        tasks.push(tokio::spawn(
                            async move { c.request("mutate", b, None).await },
                        ));
                    }
                    let (mut ok, mut errors) = (0u64, BTreeMap::<String, u64>::new());
                    for t in tasks {
                        match t.await {
                            Ok(Ok(_)) => ok += 1,
                            Ok(Err(e)) => *errors.entry(e.code_text.clone()).or_default() += 1,
                            Err(e) => *errors.entry(e.to_string()).or_default() += 1,
                        }
                    }
                    return Ok(json!({"ok": ok, "errors": errors, "parallel": true}));
                }
                let r = self.mutate(body, test_pid.as_deref()).await?;
                Ok(
                    json!({"cseq": r["result"]["cseq"], "entries": r["result"]["entries"],
                    "retroactive": r["result"]["retroactive"], "versions": r["versions"],
                    "commit_id": r["commits"][0], "process_id": r["process_id"]}),
                )
            }
            "query" => {
                let mut q = i.clone();
                q.as_object_mut().map(|m| m.remove("branch"));
                q.as_object_mut().map(|m| m.remove("principal"));
                let r = self.query(json!({"query": q, "branch": branch})).await?;
                let objects: Vec<Value> = r["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| x["object_id"].clone())
                    .collect();
                Ok(
                    json!({"objects": objects, "cursor": r["cursor"], "evaluated_at": r["evaluated_at"]}),
                )
            }
            "history" => {
                let mut h = pick(i, &["from", "to"]);
                h["target"] = i["object_id"].clone();
                let r = self.query(json!({"history": h, "branch": branch})).await?;
                Ok(json!({"commits": r["items"], "evaluated_at": r["evaluated_at"]}))
            }
            "diff" => {
                let mut d = pick(i, &["from", "to"]);
                for k in ["from", "to"] {
                    if let Some(t) = d[k].get("time").cloned() {
                        d[k]["time"] = self.seq(&t);
                    }
                }
                Ok(self.query(json!({"diff": d})).await?["items"].clone())
            }
            "create_branch" => {
                let from = s(i, "from").unwrap_or("main").to_string();
                Ok(self.mutate(json!({"branch": from, "create_branch": pick(i, &["name", "from", "time"])}), None).await?["result"].clone())
            }
            "commit_info" | "export_state" => {
                let mut b = json!({"commit_info": {}, "branch": branch});
                if let Some(x) = i.get("time").filter(|x| !x.is_null()) {
                    b["time"] = self.seq(x);
                }
                if let Some(x) = i.get("commit").filter(|x| !x.is_null()) {
                    b["commit"] = x.clone();
                }
                self.one(b).await
            }
            "effective_class" | "dispatch" => {
                let q = if op == "dispatch" {
                    json!({"dispatch": pick(i, &["class", "port", "after"]), "branch": "main"})
                } else {
                    json!({"class": {"name": i["class"]}, "branch": "main"})
                };
                self.one(q).await
            }
            "merge_preview" => {
                let target = s(i, "target").unwrap_or("main").to_string();
                self.one(json!({"merge_preview": {"source": i["source"]}, "branch": target}))
                    .await
            }
            "change_set" => {
                let id = s(i, "id").unwrap_or_default().to_string();
                if let Some(t) = s(i, "target") {
                    self.change_sets.insert(id.clone(), t.to_string());
                }
                let target = self
                    .change_sets
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "main".into());
                let cs = pick(
                    i,
                    &[
                        "action",
                        "id",
                        "source",
                        "target",
                        "title",
                        "description",
                        "resolutions",
                    ],
                );
                Ok(self
                    .mutate(json!({"branch": target, "change_set": cs}), None)
                    .await?["result"]
                    .clone())
            }
            "merge" => {
                let target = s(i, "target").unwrap_or("main").to_string();
                let r = self
                    .mutate(
                        json!({"branch": target, "merge": pick(i, &["source", "resolutions"])}),
                        test_pid.as_deref(),
                    )
                    .await?;
                Ok(r["result"].clone())
            }
            "revert" => {
                let mut rv = pick(
                    i,
                    &[
                        "change_set",
                        "time",
                        "commit",
                        "reversal_ids",
                        "dry_run",
                        "resolutions",
                    ],
                );
                if let Some(p) = s(i, "process_id") {
                    rv["process_id"] = json!(self.pid(p));
                }
                if let Some(x) = rv.get("time").cloned() {
                    rv["time"] = self.seq(&x);
                }
                Ok(self
                    .mutate(json!({"branch": branch, "revert": rv}), None)
                    .await?["result"]
                    .clone())
            }
            "bulk" => {
                let changes = i.get("changes").cloned().unwrap_or(json!([]));
                self.learn(&changes);
                let r = self
                    .mutate(json!({"branch": branch, "bulk": pick(i, &["changes", "max_changes", "mode", "staging"])}), test_pid.as_deref())
                    .await?;
                Ok(r["result"].clone())
            }
            "read_ledger" => {
                let mut b = json!({"ledger": pick(i, &["class", "ledger_key"]), "branch": branch});
                if let Some(a) = i.get("asof").filter(|x| !x.is_null()) {
                    b["asof"] = a.clone();
                }
                Ok(json!({"entries": self.query(b).await?["items"]}))
            }
            "execute" => {
                if i.get("asset").is_some() {
                    return Err(err(
                        "LOGIC.NOT_AVAILABLE",
                        "a logic function runs through a port over UBTP",
                    ));
                }
                let port = json!({"target": i["target"], "name": i["port"], "params": i.get("params").cloned().unwrap_or(json!({}))});
                let r = self
                    .mutate(json!({"branch": branch, "port": port}), test_pid.as_deref())
                    .await?;
                let pid = s(&r, "process_id").unwrap_or_default().to_string();
                // a `require` decision is a pending change over UBTP and an error in process
                // (STD-PROTO-010, STD-LGS-031)
                if let Some(p) = r.get("pending").filter(|p| !p.is_null()) {
                    return Err(BpaError::with_details(
                        "RULE.DECISION_REQUIRES",
                        json!({"steps": p["steps"], "process_id": pid}),
                    ));
                }
                let rec = self
                    .one(json!({"process": {"process_id": pid, "metering": true}}))
                    .await?;
                Ok(
                    json!({"result": r["result"], "process_id": pid, "metering": rec["metering"], "commits": r["commits"]}),
                )
            }
            "process_info" | "metering" | "journal" => {
                let pid = self.pid(s(i, "process_id").unwrap_or_default());
                let rec = self.one(json!({"process": {"process_id": pid, "journal": op == "journal", "metering": op == "metering"}})).await?;
                Ok(match op {
                    "metering" => {
                        let mut m = rec["metering"].clone();
                        if m.is_object() {
                            m["process_id"] = json!(pid);
                        }
                        m
                    }
                    "journal" => rec["journal"].clone(),
                    _ => rec,
                })
            }
            "replay" => {
                let pid = self.pid(s(i, "process_id").unwrap_or_default());
                self.one(json!({"replay": {"process_id": pid}})).await
            }
            "released_effects" => Ok(self.released().await),
            "explain_cascade" => {
                let oid = s(i, "object_id").unwrap_or_default().to_string();
                let b = json!({"explain": {"target": self.uri(&oid)?, "kind": "cascade",
                    "property": i["property"]}, "branch": branch});
                self.one(b).await
            }
            other => Err(err(
                "LOGIC.NOT_AVAILABLE",
                format!("{other} has no UBTP mapping"),
            )),
        }
    }

    /// An expected error of `effective_class` or `dispatch` over inline declarations: the
    /// node refuses a genesis with them.
    fn rejected_genesis(&mut self, i: &Value, code: &str) -> Result<Value, BpaError> {
        self.tenant_seq += 1;
        let tenant = format!("rej{}-{}", std::process::id(), self.tenant_seq);
        let mut g = json!({"classes": i.get("classes").cloned().unwrap_or(json!([]))});
        if let Some(v) = &self.genesis_value_types {
            g["value_types"] = v.clone();
        }
        match self.node.bootstrap(&tenant, &g, CLOCK) {
            Ok(()) => Ok(json!({"bootstrapped": tenant})),
            Err(out) if out.contains(code) => Err(err(code, "the node refused the genesis")),
            Err(out) => Err(err(
                "NOD.BOOTSTRAP_FAILED",
                out.lines().rev().take(5).collect::<Vec<_>>().join(" | "),
            )),
        }
    }
}

/// Run the scenario tests of a project over UBTP.
pub fn run(project: &Project, o: &Options) -> Result<Outcome, BpaError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| err("SDK.TRANSPORT", e.to_string()))?;
    let mut genesis = json!({"classes": project.declarations()});
    let vts = project.value_type_decls();
    if !vts.is_empty() {
        genesis["value_types"] = json!(vts);
    }
    let mut results = Vec::new();
    let mut in_process_only = Vec::new();
    for (n, item) in project.tests.iter().enumerate() {
        let t = &item.value;
        let name = s(t, "name").unwrap_or("unnamed").to_string();
        let kind = s(t, "kind").unwrap_or("scenario").to_string();
        let started = Instant::now();
        let mut steps = 0;
        let hook =
            t["steps"].as_array().into_iter().flatten().find(|st| {
                st["input"].get("drop").is_some() || st["input"].get("tamper").is_some()
            });
        let failure: Option<Value> = if kind != "scenario" {
            Some(json!({"actual": "only scenario tests run over UBTP"}))
        } else if let Some(h) = hook {
            in_process_only.push((
                name.clone(),
                format!(
                    "step {} uses the replay test hook {}, outside the Protocol (STD-PROTO-015)",
                    h["op"], h["input"]
                ),
            ));
            None
        } else {
            let r = rt.block_on(run_test(o, project, &genesis, t, n, &mut steps));
            match r {
                Ok(f) => f,
                Err(e) => {
                    Some(json!({"step": steps, "actual": {"error": e.code, "message": e.message}}))
                }
            }
        };
        results.push(TestResult {
            name,
            kind,
            file: item.file.clone(),
            pass: failure.is_none(),
            failure,
            ms: started.elapsed().as_millis(),
            steps,
        });
    }
    Ok(Outcome {
        report: Report {
            results,
            invoked: BTreeMap::new(),
            roots: json!({}),
            findings: Vec::new(),
        },
        in_process_only,
    })
}

async fn run_test(
    o: &Options,
    project: &Project,
    genesis: &Value,
    t: &Value,
    n: usize,
    steps: &mut usize,
) -> Result<Option<Value>, BpaError> {
    // the fixture at the fixture clock on a fresh tenant of a Server started for the test
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let tenant = format!("t{stamp:x}-{n}");
    let node = Node::start(o, project, &tenant, genesis, t.get("connectors"))?;
    let node = &node;
    let mut cfg =
        Config::new(&format!("http://{}", node.addr), &node.vae, Auth::Dev).binding(o.binding);
    cfg.tenant = Some(tenant.clone());
    let c = Client::connect(cfg).await.map_err(sdk_error)?;
    let mut run = Run {
        c,
        node,
        project,
        vae: node.vae.clone(),
        ids: HashMap::new(),
        pids: HashMap::new(),
        back: HashMap::new(),
        change_sets: HashMap::new(),
        subs: Vec::new(),
        effects: Vec::new(),
        genesis_value_types: genesis.get("value_types").cloned(),
        tenant_seq: 0,
        offset: 0,
    };
    install(&mut run).await?;
    // the in-process fixture has the genesis at 0 and one commit per seed file on `main`
    let head = run
        .one(json!({"commit_info": {}, "branch": "main"}))
        .await?["cseq"]
        .as_u64()
        .unwrap_or(0);
    let mut seed_files: Vec<&str> = project.seed.iter().map(|i| i.file.as_str()).collect();
    seed_files.dedup();
    run.offset = head.saturating_sub(seed_files.len() as u64);
    for b in ["main", "feature/forge"] {
        let sub = run
            .c
            .subscribe(Target::Events(Vec::new()), &Selectors::branch(b), None)
            .await
            .map_err(sdk_error)?;
        run.subs.push(sub);
    }
    let start = s(t, "clock").unwrap_or(CLOCK);
    node.clock(json!({"set": start}))?;
    let mut labels: HashMap<String, Value> = HashMap::new();
    for step in t["steps"].as_array().into_iter().flatten() {
        *steps += 1;
        if let Some(by) = s(step, "advance") {
            node.clock(json!({"advance": by}))?;
            // the leader loop ticks every second: due timers fire and are released
            tokio::time::sleep(Duration::from_millis(2500)).await;
            continue;
        }
        let op = s(step, "op").unwrap_or("read");
        let input = resolve(step.get("input").unwrap_or(&json!({})), &labels)
            .map_err(|e| err("MODEL.INVALID_VALUE", e))?;
        let expected_error = step["expect"].get("error_code").and_then(Value::as_str);
        let out = match (op, expected_error) {
            ("effective_class" | "dispatch", Some(code)) if input.get("classes").is_some() => {
                run.rejected_genesis(&input, code)
            }
            _ => {
                let conc = step.get("concurrent").and_then(Value::as_u64);
                run.step(op, &input, conc).await
            }
        };
        let out = out.map(|v| run.unmap(v));
        if let Ok(v) = &out
            && let Some(l) = s(step, "label")
        {
            labels.insert(l.to_string(), v.clone());
        }
        if let Some(d) = expectation(step.get("expect"), &out, &labels, *steps) {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

/// The seed on `main` and the logic on `feature/forge` by raw commits, as the in-process
/// fixture has them (DSN-FRG-203).
async fn install(run: &mut Run<'_>) -> Result<(), BpaError> {
    let project = run.project;
    let mut files: Vec<&str> = Vec::new();
    for i in &project.seed {
        if !files.contains(&i.file.as_str()) {
            files.push(&i.file);
        }
    }
    for f in files {
        let changes: Vec<Value> = project
            .seed
            .iter()
            .filter(|i| i.file == f)
            .map(|i| i.value.clone())
            .collect();
        let changes = json!(changes);
        run.learn(&changes);
        run.mutate(
            json!({"branch": "main", "commit": {"changes": changes}}),
            None,
        )
        .await
        .map_err(|e| err(e.code, format!("seed {f}: {}", e.message)))?;
    }
    if !project.logic.is_empty() {
        run.mutate(
            json!({"branch": "main", "create_branch": {"name": "feature/forge", "from": "main"}}),
            None,
        )
        .await?;
        for item in &project.logic {
            let name = s(&item.value, "name").unwrap_or("asset").to_string();
            let ch = json!([{"op": "create", "class": "Logic", "object_id": asset_object_id(&name),
                "slug": name.replace('.', "-"), "body": item.value}]);
            run.mutate(
                json!({"branch": "feature/forge", "commit": {"changes": ch}}),
                None,
            )
            .await
            .map_err(|e| err(e.code, format!("asset {name}: {}", e.message)))?;
        }
    }
    Ok(())
}

/// The path of the report's directory, created.
pub fn ensure_dir(p: &Path) -> Result<(), BpaError> {
    std::fs::create_dir_all(p).map_err(|e| err("SDK.TRANSPORT", e.to_string()))
}
