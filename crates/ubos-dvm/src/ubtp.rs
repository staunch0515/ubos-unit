//! The in-process UBTP binding (STD-PROTO-001, -002, -010…-013, -020; PH-1 step 14a,
//! DEC-075): the adapter operation `ubtp {session, messages}` feeds a sequence of
//! `DAT-UbtpEnvelope` messages to a session and returns the frames the binding produced in
//! order (`result`, `error`, `emit`, `pong`). Sessions persist across calls, so a caller
//! may commit through other operations between two calls and receive the emits on the
//! next one. The message types and codecs are `ubos-proto`; this module gives them their
//! semantics over the engine, and the Server hands its transport frames to the same code
//! (FR-INT-015). Since BPA 1.0-rc4 (PH-1 step 18b, DEC-079) `mutate` carries the
//! versioning requests of STD-PROTO-014 and `query` the introspection requests of
//! STD-PROTO-015; since BPA 1.0-rc5 (step 18d, DEC-082) the error of a `mutate` whose
//! process was recorded carries its `process_id`.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_proto::{InProcess, Limits, Request};
use ubos_types::{BpaError, canonical, content_id};
use ubos_version::Repo;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// An error with a text detail, as the reference binding reports its own (`details`).
fn err(code: &'static str, detail: &str) -> BpaError {
    let mut e = BpaError::with_details(code, json!(detail));
    e.message = detail.to_string();
    e
}

fn protocol(detail: &str) -> BpaError {
    err("INT.PROTOCOL_ERROR", detail)
}

fn invalid(detail: &str) -> BpaError {
    err("MODEL.INVALID_VALUE", detail)
}

/// A subscription of a session (STD-PROTO-012).
#[derive(Debug, Clone)]
struct Subscription {
    /// `target`, `query` or `events`.
    kind: String,
    spec: Value,
    branch: String,
    /// The last commit delivered or queued.
    cseq: u64,
    /// The effects released on the branch without a commit, delivered or queued (events
    /// subscriptions, 1.0-rc6).
    n: u64,
    /// The object of a `target` subscription.
    object_id: Option<String>,
    /// Emits delivered and not yet acknowledged: `((cseq, n), body)`.
    unacked: Vec<((u64, u64), Value)>,
    /// Emits computed and not yet delivered (the window is full).
    queued: Vec<((u64, u64), Value)>,
}

/// A UBTP session of the in-process binding.
#[derive(Debug, Clone, Default)]
pub struct Session {
    key: String,
    open: bool,
    principal: String,
    kind: String,
    role: Option<String>,
    grants: Option<Vec<String>>,
    /// The authentication level the host established (the `auth_level` of the decision
    /// context, DSN-NOD-105), if any.
    auth_level: Option<String>,
    window: u64,
    ids: BTreeSet<String>,
    next_id: u64,
    next_sub: u64,
    subs: BTreeMap<u64, Subscription>,
    /// Idempotency keys of `mutate`: key → (payload hash, result body).
    idem: BTreeMap<String, (String, Value)>,
}

impl Session {
    fn new(key: &str) -> Session {
        Session {
            key: key.to_string(),
            window: Limits::default().window,
            ..Default::default()
        }
    }

    fn sid(&mut self) -> String {
        self.next_id += 1;
        format!("s{}", self.next_id)
    }

    fn frame(
        &mut self,
        type_: &str,
        body: Value,
        reply_to: Option<&str>,
        trace: Option<&str>,
    ) -> Value {
        let id = self.sid();
        ubos_proto::frame(&id, type_, body, reply_to, trace)
    }

    fn error(&mut self, e: &BpaError, reply_to: Option<&str>, trace: Option<&str>) -> Value {
        let id = self.sid();
        ubos_proto::error_frame(&id, e, reply_to, trace)
    }

    /// The process a session's mutate runs as.
    fn process(&self, extra: Map<String, Value>) -> Value {
        let mut p =
            json!({"initiator": self.principal, "channel": "api", "principal_kind": self.kind});
        if let Some(r) = &self.role {
            p["role"] = json!(r);
        }
        if let Some(a) = &self.auth_level {
            p["auth_level"] = json!(a);
        }
        for (k, v) in extra {
            p[k] = v;
        }
        p
    }

    /// The decision context of a session's requests.
    fn ctx(&self) -> Value {
        let mut c =
            json!({"principal": self.principal, "principal_kind": self.kind, "channel": "api"});
        if let Some(r) = &self.role {
            c["role"] = json!(r);
        }
        if let Some(a) = &self.auth_level {
            c["auth_level"] = json!(a);
        }
        c
    }
}

/// The versioning requests of `mutate` (STD-PROTO-014, since 1.0-rc4).
const VERSIONING: [&str; 5] = ["create_branch", "change_set", "merge", "revert", "bulk"];

/// The introspection requests of `query` (STD-PROTO-015, since 1.0-rc4).
const INTROSPECTION: [&str; 7] = [
    "class",
    "dispatch",
    "commit_info",
    "merge_preview",
    "ledger",
    "process",
    "replay",
];

/// The permission a versioning request needs, if any (STD-PROTO-014).
fn permission(kind: &str, v: &Map<String, Value>) -> Option<&'static str> {
    match kind {
        "create_branch" => Some("branch.create"),
        "merge" => Some("branch.merge"),
        "revert" => Some("revert"),
        "bulk" => Some("commit.raw"),
        _ => match v.get("action").and_then(Value::as_str) {
            Some("approve" | "reject") => Some("changeset.approve"),
            Some("merge") => Some("branch.merge"),
            _ => None,
        },
    }
}

/// The members of `v` named in `keys` that are present and not null.
fn pick(v: &Map<String, Value>, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for k in keys {
        if let Some(x) = v.get(*k).filter(|x| !x.is_null()) {
            out.insert((*k).to_string(), x.clone());
        }
    }
    Value::Object(out)
}

/// The kinds of a request body, exactly one of which must be present.
fn one_of<'a>(
    body: &Map<String, Value>,
    kinds: &[&'a str],
    what: &str,
) -> Result<&'a str, BpaError> {
    let found: Vec<&str> = kinds
        .iter()
        .copied()
        .filter(|k| body.contains_key(*k))
        .collect();
    if found.len() != 1 {
        return Err(invalid(what));
    }
    Ok(found[0])
}

impl Engine {
    /// `ubtp {session, messages}` (STD-CONF-005, STD-PROTO-020): the frames produced.
    pub fn ubtp(&mut self, input: &Value) -> Result<Value, BpaError> {
        let Some(messages) = input.get("messages").and_then(Value::as_array).cloned() else {
            return Err(BpaError::new("MODEL.INVALID_VALUE", "messages is a list"));
        };
        let key = s(input, "session").unwrap_or("s1").to_string();
        let mut session = self
            .ubtp_sessions
            .remove(&key)
            .unwrap_or_else(|| Session::new(&key));
        let mut out = Vec::new();
        let r = (|| -> Result<(), BpaError> {
            if session.open {
                out.extend(self.catch_up(&mut session)?);
            }
            for m in &messages {
                let frames = self.message(&mut session, m)?;
                out.extend(frames);
            }
            Ok(())
        })();
        self.ubtp_sessions.insert(key, session);
        r?;
        Ok(json!({"responses": out}))
    }

    /// One message: the envelope checks, the handling, the frames.
    fn message(&mut self, session: &mut Session, m: &Value) -> Result<Vec<Value>, BpaError> {
        let req = match self.envelope(session, m) {
            Ok(r) => r,
            Err(e) => {
                let rid = ubos_proto::id_of(m);
                return Ok(vec![session.error(&e, rid.as_deref(), None)]);
            }
        };
        let rid = req.id.clone();
        let trace = req.trace.clone();
        let t = req.type_.as_str();
        if t == "ping" {
            return Ok(vec![session.frame(
                "pong",
                json!({}),
                Some(&rid),
                trace.as_deref(),
            )]);
        }
        self.ubtp_pid = None;
        let body = match t {
            "handshake" => self.handshake(session, &req.body),
            "cancel" => Ok(json!({"cancelled": false})),
            "mutate" => self.mutate(session, &req.body),
            "query" => self.query_message(session, &req.body),
            "subscribe" => self.subscribe(session, &req.body),
            "unsubscribe" => Self::unsubscribe(session, &req.body),
            "ack" => Self::ack_emits(session, &req.body),
            _ => Err(err("LOGIC.NOT_AVAILABLE", "sync comes with PH-3")),
        };
        let body = match body {
            Ok(b) => b,
            Err(e) => {
                let mut f = session.error(&e, Some(&rid), trace.as_deref());
                // STD-PROTO-010 (1.0-rc5): a mutate whose process was recorded names it
                if t == "mutate"
                    && let Some(pid) = self.ubtp_pid.take()
                    && self.store.process_rec(&pid)?.is_some()
                {
                    f["body"]["process_id"] = json!(pid);
                }
                return Ok(vec![f]);
            }
        };
        let mut frames = vec![session.frame("result", body, Some(&rid), trace.as_deref())];
        if t == "mutate" || t == "ack" {
            frames.extend(self.catch_up(session)?);
        }
        Ok(frames)
    }

    /// The envelope checks of STD-PROTO-001 plus the session's: a duplicate id, a request
    /// before the handshake, a second handshake.
    fn envelope(&mut self, session: &mut Session, m: &Value) -> Result<Request, BpaError> {
        let req = ubos_proto::request(m)?;
        if !session.ids.insert(req.id.clone()) {
            return Err(protocol(&format!("duplicate id {}", req.id)));
        }
        if req.type_ != "handshake" && !session.open {
            return Err(protocol("the first message is the handshake"));
        }
        if req.type_ == "handshake" && session.open {
            return Err(protocol("the session is already open"));
        }
        Ok(req)
    }

    // ---- handshake (STD-PROTO-002) ----

    fn handshake(
        &mut self,
        session: &mut Session,
        body: &Map<String, Value>,
    ) -> Result<Value, BpaError> {
        let InProcess {
            principal,
            kind,
            role,
            grants,
            auth_level,
        } = ubos_proto::negotiate(body)?;
        session.principal = principal.unwrap_or_else(|| self.principal.clone());
        session.kind = kind;
        session.role = role;
        session.grants = grants;
        session.auth_level = auth_level;
        session.open = true;
        let limits = Limits {
            window: session.window,
            ..Limits::default()
        };
        Ok(ubos_proto::handshake_result(
            &session.key,
            &session.principal,
            &session.kind,
            &self.store.authority(),
            &limits,
        ))
    }

    /// The next process identifier of the binding: the reference scheme over the engine's
    /// process counter, which the implicit processes of other operations advance too.
    fn next_pid(&mut self) -> String {
        self.next_process += 1;
        let pid = format!("00000000-0000-4000-8000-{:012x}", self.next_process);
        self.ubtp_pid = Some(pid.clone());
        pid
    }

    fn head_cseq(&mut self, branch: &str) -> Result<u64, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.head(branch)?;
        Ok(repo.meta(head)?.cseq)
    }

    // ---- mutate (STD-PROTO-010) ----

    fn mutate(&mut self, session: &mut Session, b: &Map<String, Value>) -> Result<Value, BpaError> {
        let branch = b
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let versioning = VERSIONING.iter().copied().find(|k| b.contains_key(*k));
        if versioning.is_some() {
            let mut all = vec!["port", "commit"];
            all.extend(VERSIONING);
            one_of(
                b,
                &all,
                "mutate carries exactly one of port, commit, create_branch, change_set, merge, revert, bulk",
            )?;
        } else if b.contains_key("port") == b.contains_key("commit") {
            return Err(invalid("mutate carries port or commit"));
        }
        let key = b
            .get("idempotency_key")
            .and_then(Value::as_str)
            .map(str::to_string);
        let payload: Map<String, Value> = b
            .iter()
            .filter(|(k, _)| k.as_str() != "idempotency_key")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let hash = content_id("ubos.chunk.v1", &Value::Object(payload))?;
        if let Some(k) = &key
            && let Some((h, body)) = session.idem.get(k)
        {
            if *h != hash {
                return Err(BpaError::new("TXN.IDEMPOTENCY_MISMATCH", ""));
            }
            return Ok(body.clone());
        }
        // STD-TXN-040: a key is scoped by the principal, not by the session. A key the
        // kernel already recorded (in an earlier session: a client retrying after a lost
        // reply) returns the first result before the base is checked, since the first
        // attempt moved the head; new requests carry the key into their process, so the
        // record is written with the commit.
        let validate_only = b.get("validate_only") == Some(&json!(true));
        let mut replayed = None;
        if let Some(k) = key
            .as_ref()
            .filter(|_| !validate_only && versioning.is_none())
        {
            let request = match (b.get("commit"), b.get("port").unwrap_or(&Value::Null)) {
                (Some(c), _) => {
                    json!({"branch": branch, "changes": c.get("changes").cloned().unwrap_or(json!([]))})
                }
                (None, p) => json!({"branch": branch, "target": p["target"], "port": p["name"],
                    "params": p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({}))}),
            };
            let kernel_hash = content_id("ubos.chunk.v1", &request)?;
            replayed = self.replay(&session.principal, &canonical(&json!(k))?, &kernel_hash)?;
        }
        if let Some(r) = replayed {
            let body = self.replayed_response(&branch, b.contains_key("commit"), &r)?;
            if let Some(k) = key {
                session.idem.insert(k, (hash, body.clone()));
            }
            return Ok(body);
        }
        if let Some(base) = b.get("base").filter(|v| !v.is_null()) {
            let head = self.head_cseq(&branch)?;
            if *base != json!(head) {
                return Err(BpaError::with_details(
                    "TXN.HEAD_MOVED",
                    json!({"head": head}),
                ));
            }
        }
        let mut extra = Map::new();
        for k in ["message", "reason"] {
            if let Some(v) = b.get(k).and_then(Value::as_str).filter(|t| !t.is_empty()) {
                extra.insert(k.into(), json!(v));
            }
        }
        if let Some(k) = key
            .as_ref()
            .filter(|_| !validate_only && versioning.is_none())
        {
            extra.insert("idempotency_key".into(), json!(k));
        }
        let body = if let Some(kind) = versioning {
            self.versioning(session, b, kind, &branch, extra)?
        } else if let Some(c) = b.get("commit") {
            let changes = match c
                .as_object()
                .and_then(|o| o.get("changes"))
                .and_then(Value::as_array)
            {
                Some(ch) => ch.clone(),
                None => return Err(invalid("commit is {changes}")),
            };
            if let Some(g) = &session.grants
                && !g.iter().any(|x| x == "commit.raw")
            {
                return Err(err(
                    "IAM.DENIED",
                    "raw commits need the commit.raw permission",
                ));
            }
            if b.get("validate_only") == Some(&json!(true)) {
                let r = self.validate(&json!({"branch": branch, "changes": changes, "process": session.process(extra)}))?;
                json!({"validated": true, "violations": r["violations"], "warnings": r["warnings"]})
            } else {
                let pid = self.next_pid();
                extra.insert("process_id".into(), json!(pid));
                let r = self.commit(&json!({"branch": branch, "changes": changes, "process": session.process(extra)}))?;
                json!({"process_id": pid, "commits": [r["commit_id"]], "versions": r["versions"],
                    "warnings": r.get("warnings").cloned().unwrap_or_else(|| json!([])),
                    "result": {"cseq": r["cseq"], "entries": r["entries"], "retroactive": r["retroactive"]}})
            }
        } else {
            let p = &b["port"];
            let (Some(target), Some(name)) = (s(p, "target"), s(p, "name")) else {
                return Err(invalid("port is {target, name, params}"));
            };
            if b.get("validate_only") == Some(&json!(true)) {
                return Err(err(
                    "LOGIC.NOT_AVAILABLE",
                    "the preview of a port call comes with the simulation of PH-2",
                ));
            }
            let pid = self.next_pid();
            extra.insert("process_id".into(), json!(pid));
            let mut i = json!({"branch": branch, "target": target, "port": name,
                "params": p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({})),
                "process": session.process(extra), "ctx": session.ctx()});
            if let Some(g) = &session.grants {
                i["grants"] = json!(g);
            }
            match self.execute(&i) {
                Ok(r) => {
                    let (mut commits, mut versions) = (Vec::new(), Map::new());
                    if let Some(cseq) = r.get("cseq").and_then(Value::as_u64).filter(|c| *c > 0) {
                        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                        let cid = repo.commit_at(&branch, Some(cseq))?;
                        let c = repo.commit(cid)?;
                        commits.push(json!(c.commit_id));
                        let ids: Vec<String> = c.ops.keys().cloned().collect();
                        for (oid, o) in repo.objects_in_ids(&branch, cseq, &ids)? {
                            versions.insert(oid, json!(o.head));
                        }
                    }
                    json!({"process_id": pid, "commits": commits, "versions": versions, "warnings": [],
                        "result": r.get("result").cloned().unwrap_or(Value::Null)})
                }
                Err(e) if e.code == "RULE.DECISION_REQUIRES" => {
                    let steps = e
                        .details
                        .as_ref()
                        .and_then(|d| d.get("steps"))
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    json!({"process_id": pid, "pending": {"pending_change_id": pid, "steps": steps},
                        "commits": [], "versions": {}, "warnings": [], "result": null})
                }
                Err(e) => return Err(e),
            }
        };
        if let Some(k) = key {
            session.idem.insert(k, (hash, body.clone()));
        }
        Ok(body)
    }

    // ---- versioning requests (STD-PROTO-014, since 1.0-rc4) ----

    /// The head of every branch.
    fn branch_heads(&mut self) -> Result<BTreeMap<String, u64>, BpaError> {
        let mut out = BTreeMap::new();
        for name in self.store.branch_names()? {
            if let Some(b) = self.store.branch(&name)? {
                out.insert(name, b.head);
            }
        }
        Ok(out)
    }

    /// A versioning request of `mutate`: the operation it names, with the commits it made
    /// and the versions they wrote.
    fn versioning(
        &mut self,
        session: &Session,
        b: &Map<String, Value>,
        kind: &str,
        branch: &str,
        extra: Map<String, Value>,
    ) -> Result<Value, BpaError> {
        let Some(v) = b.get(kind).and_then(Value::as_object).cloned() else {
            return Err(invalid(&format!("{kind} is a map")));
        };
        if b.get("validate_only") == Some(&json!(true)) {
            return Err(invalid("validate_only applies to port and commit"));
        }
        if let (Some(g), Some(p)) = (&session.grants, permission(kind, &v))
            && !g.iter().any(|x| x == p)
        {
            return Err(err(
                "IAM.DENIED",
                &format!("{kind} needs the {p} permission"),
            ));
        }
        let before = self.branch_heads()?;
        let mut pid = None;
        let process = |pid: &mut Option<String>, this: &mut Self| {
            let id = this.next_pid();
            let mut x = extra.clone();
            x.insert("process_id".into(), json!(id));
            *pid = Some(id);
            session.process(x)
        };
        let r = match kind {
            "create_branch" => {
                let name = v.get("name").cloned().unwrap_or(Value::Null);
                let mut i = pick(&v, &["name", "time"]);
                i["from"] = v.get("from").cloned().unwrap_or_else(|| json!(branch));
                let r = self.other("create_branch", &i)?;
                json!({"branch": name, "head": r["head"]})
            }
            "change_set" => {
                let mut i = pick(
                    &v,
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
                let action = s(&i, "action").unwrap_or_default().to_string();
                if action == "open" && i.get("target").is_none() {
                    i["target"] = json!(branch);
                }
                if action == "approve" || action == "reject" {
                    i["approver"] = json!(session.principal);
                }
                self.change_set(&i)?
            }
            "merge" => {
                let mut i = pick(&v, &["source", "resolutions"]);
                i["target"] = json!(branch);
                i["process"] = process(&mut pid, self);
                self.merge(&i)?
            }
            "revert" => {
                let mut i = pick(
                    &v,
                    &[
                        "process_id",
                        "change_set",
                        "time",
                        "commit",
                        "reversal_ids",
                        "resolutions",
                        "dry_run",
                    ],
                );
                i["branch"] = json!(branch);
                if v.get("dry_run") != Some(&json!(true)) {
                    i["process"] = process(&mut pid, self);
                }
                self.revert(&i)?
            }
            _ => {
                let mut i = pick(&v, &["changes", "max_changes", "mode", "staging"]);
                let id = match v.get("resume").filter(|x| !x.is_null()) {
                    Some(r) => {
                        i["resume"] = json!(true);
                        let id = r.as_str().unwrap_or_default().to_string();
                        self.ubtp_pid = Some(id.clone());
                        id
                    }
                    None => self.next_pid(),
                };
                i["process_id"] = json!(id);
                i["branch"] = json!(branch);
                i["process"] = session.process(extra.clone());
                pid = Some(id);
                self.bulk(&i)?
            }
        };
        // the commits the request made: on each branch whose head moved, back along the
        // first parents to the head before or to the commit the branch was created from
        let after = self.branch_heads()?;
        let mut made = BTreeSet::new();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        for (name, head) in &after {
            let stop = before.get(name).copied();
            let mut id = *head;
            while Some(id) != stop {
                let c = repo.commit(id)?;
                if c.branch != *name || !made.insert(id) {
                    break;
                }
                match c.parents.first() {
                    Some(p) => id = *p,
                    None => break,
                }
            }
        }
        let (mut commits, mut versions) = (Vec::new(), Map::new());
        for id in made {
            let c = repo.commit(id)?;
            commits.push(json!(c.commit_id));
            let ids: Vec<String> = c.ops.keys().cloned().collect();
            for (oid, o) in repo.objects_in_ids(&c.branch, c.cseq, &ids)? {
                versions.insert(oid, json!(o.head));
            }
        }
        let mut body =
            json!({"commits": commits, "versions": versions, "warnings": [], "result": r});
        if let Some(p) = pid {
            body["process_id"] = json!(p);
        }
        Ok(body)
    }

    // ---- introspection requests (STD-PROTO-015, since 1.0-rc4) ----

    fn introspect(&mut self, b: &Map<String, Value>, kind: &str) -> Result<Value, BpaError> {
        let (branch, sel) = Self::coords(b);
        let Some(v) = b.get(kind).filter(|v| v.is_object()).cloned() else {
            return Err(invalid(&format!("{kind} is a map")));
        };
        if kind == "process" || kind == "replay" {
            let pid = s(&v, "process_id").unwrap_or_default().to_string();
            let item = if kind == "replay" {
                self.replay_process(&pid, None, None)?
            } else {
                let mut item = self.process_info(&pid)?;
                if v.get("journal") == Some(&json!(true)) {
                    item["journal"] = self.journal(&pid)?;
                }
                if v.get("metering") == Some(&json!(true)) {
                    item["metering"] = self.metering_record(&pid)?;
                }
                item
            };
            return Ok(json!({"items": [item], "evaluated_at": {}}));
        }
        let mut i = json!({"branch": branch});
        for (k, x) in &sel {
            if k != "asof" {
                i[k] = x.clone();
            }
        }
        let (view, _) = Repo::new(self.store.as_mut(), &self.registry).view(&branch, &i)?;
        let items = match kind {
            "class" => {
                let mut c = i.clone();
                c["class"] = v.get("name").cloned().unwrap_or(Value::Null);
                vec![self.other("class_at", &c)?]
            }
            "dispatch" => {
                let exts = {
                    let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                    crate::relations::extensions_of(&mut repo, &view.state)?
                };
                let reg = self.registry.with_extensions(&exts)?;
                vec![reg.dispatch(
                    s(&v, "class").unwrap_or_default(),
                    s(&v, "port").unwrap_or_default(),
                    s(&v, "after"),
                )?]
            }
            "commit_info" => {
                let mut item = self.commit_info(&i)?;
                item["root"] = self.commit_record(&i)?["root"].clone();
                vec![item]
            }
            "merge_preview" => {
                vec![self.merge_preview(s(&v, "source").unwrap_or_default(), &branch)?]
            }
            _ => {
                let mut l = i.clone();
                l["class"] = v.get("class").cloned().unwrap_or(Value::Null);
                l["ledger_key"] = v.get("ledger_key").cloned().unwrap_or(Value::Null);
                if let Some(a) = sel.get("asof") {
                    l["asof"] = a.clone();
                }
                self.read_ledger(&l)?["entries"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            }
        };
        Ok(json!({"items": items, "evaluated_at": view.evaluated_at}))
    }

    /// The response to a mutation the kernel replayed from its idempotency record: the
    /// commit, its versions and its process as the first attempt answered them.
    fn replayed_response(&mut self, branch: &str, raw: bool, r: &Value) -> Result<Value, BpaError> {
        let (mut commits, mut versions, mut pid) = (Vec::new(), Map::new(), Value::Null);
        if let Some(cseq) = r.get("cseq").and_then(Value::as_u64).filter(|c| *c > 0) {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let cid = repo.commit_at(branch, Some(cseq))?;
            let c = repo.commit(cid)?;
            commits.push(json!(c.commit_id));
            pid = c.extra.get("process_id").cloned().unwrap_or(Value::Null);
            let ids: Vec<String> = c.ops.keys().cloned().collect();
            for (oid, o) in repo.objects_in_ids(branch, cseq, &ids)? {
                versions.insert(oid, json!(o.head));
            }
        }
        Ok(if raw {
            json!({"process_id": pid, "commits": commits, "versions": r.get("versions").cloned().unwrap_or(json!(versions)),
                "warnings": r.get("warnings").cloned().unwrap_or_else(|| json!([])),
                "result": {"cseq": r["cseq"], "entries": r["entries"], "retroactive": r["retroactive"]}})
        } else {
            json!({"process_id": pid, "commits": commits, "versions": versions, "warnings": [],
                "result": r.get("result").cloned().unwrap_or(Value::Null)})
        })
    }

    // ---- query (STD-PROTO-011) ----

    /// The branch and the selectors of a query or subscribe body.
    fn coords(b: &Map<String, Value>) -> (String, Map<String, Value>) {
        let branch = b
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let mut sel = Map::new();
        for k in ["time", "commit", "asof"] {
            if let Some(v) = b.get(k).filter(|v| !v.is_null()) {
                sel.insert(k.into(), v.clone());
            }
        }
        (branch, sel)
    }

    /// A read item of a query result: the read at the selectors with `object_id` and
    /// `class`.
    fn item(
        &mut self,
        branch: &str,
        oid: &str,
        sel: &Map<String, Value>,
    ) -> Result<Value, BpaError> {
        let mut i = json!({"branch": branch, "object_id": oid});
        for (k, v) in sel {
            i[k] = v.clone();
        }
        let mut r = self.read(&i)?;
        let class = {
            let (o, _, _, _) = self.object_view(branch, &i, oid)?;
            o.class
        };
        r["object_id"] = json!(oid);
        r["class"] = json!(class);
        Ok(r)
    }

    /// The object a target names: a URI resolved in the state at the selectors, or an
    /// object identifier.
    fn object_of(
        &mut self,
        branch: &str,
        target: &Value,
        sel: &Map<String, Value>,
    ) -> Result<String, BpaError> {
        let Some(t) = target.as_str() else {
            return Err(invalid("a target is a URI or an object identifier"));
        };
        if !t.starts_with("ubos://") {
            return Ok(t.to_string());
        }
        let mut i = json!({"branch": branch});
        for (k, v) in sel {
            if k != "asof" {
                i[k] = v.clone();
            }
        }
        let (view, _) = Repo::new(self.store.as_mut(), &self.registry).view(branch, &i)?;
        crate::relations::resolve(&view.state, t, &self.store.authority())
            .ok_or_else(|| err("IAM.NOT_FOUND", "unknown object"))
    }

    fn query_message(
        &mut self,
        session: &mut Session,
        b: &Map<String, Value>,
    ) -> Result<Value, BpaError> {
        let kinds = [
            "read", "query", "view", "explain", "history", "timeline", "diff",
        ];
        if INTROSPECTION.iter().any(|k| b.contains_key(*k)) {
            let mut all = kinds.to_vec();
            all.extend(INTROSPECTION);
            let k = one_of(b, &all, "query carries exactly one request kind")?;
            return self.introspect(b, k);
        }
        let k = one_of(
            b,
            &kinds,
            "query carries exactly one of read, query, view, explain, history, timeline, diff",
        )?;
        let (branch, sel) = Self::coords(b);
        let without_asof = |sel: &Map<String, Value>| -> Map<String, Value> {
            sel.iter()
                .filter(|(k, _)| k.as_str() != "asof")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        match k {
            "read" => {
                let mut uri = s(&Value::Object(b.clone()), "read")
                    .unwrap_or_default()
                    .to_string();
                // the message's coordinates complete the selectors the URI lacks; a commit
                // implies its branch (STD-ADDR-002), so the default branch is not added to it
                let mut add = Vec::new();
                if b.contains_key("branch") || !sel.contains_key("commit") {
                    add.push(("branch".to_string(), json!(branch)));
                }
                for n in ["asof", "time", "commit"] {
                    if let Some(v) = sel.get(n) {
                        add.push((n.to_string(), v.clone()));
                    }
                }
                for (name, value) in add {
                    let text = match &value {
                        Value::String(t) => t.clone(),
                        other => other.to_string(),
                    };
                    if !uri.contains(&format!("?{name}=")) && !uri.contains(&format!("&{name}=")) {
                        uri.push(if uri.contains('?') { '&' } else { '?' });
                        uri.push_str(&format!("{name}={text}"));
                    }
                }
                let mut item = self.resolve(&json!(uri))?;
                let class = {
                    let vh = s(&item, "version_hash").unwrap_or_default().to_string();
                    Repo::new(self.store.as_mut(), &self.registry).version(&vh)?["class"].clone()
                };
                item["class"] = class;
                let ev = item
                    .as_object_mut()
                    .and_then(|m| m.remove("evaluated_at"))
                    .unwrap_or(json!({}));
                Ok(json!({"items": [item], "evaluated_at": ev}))
            }
            "query" => {
                let mut q = match &b["query"] {
                    Value::Object(q) => q.clone(),
                    other => {
                        let mut m = Map::new();
                        m.insert("text".into(), other.clone());
                        m
                    }
                };
                q.insert("branch".into(), json!(branch));
                for (k, v) in &sel {
                    q.insert(k.clone(), v.clone());
                }
                let r = self.query(&Value::Object(q))?;
                let mut isel = without_asof(&sel);
                if let Some(a) = sel.get("asof") {
                    isel.insert("asof".into(), a.clone());
                }
                isel.insert("commit".into(), r["evaluated_at"][&branch].clone());
                let mut items = Vec::new();
                for oid in r["objects"].as_array().into_iter().flatten() {
                    let oid = oid.as_str().unwrap_or_default();
                    items.push(self.item(&branch, oid, &isel)?);
                }
                let mut out = json!({"items": items, "evaluated_at": r["evaluated_at"]});
                if let Some(c) = r.get("cursor").filter(|v| !v.is_null()) {
                    out["cursor"] = c.clone();
                }
                Ok(out)
            }
            "view" => Err(err(
                "LOGIC.NOT_AVAILABLE",
                "view payloads come with the views of PH-1 step 13",
            )),
            "explain" => {
                let e = &b["explain"];
                let oid = self.object_of(&branch, &e["target"], &sel)?;
                let mut ctx = json!({"principal_kind": session.kind, "channel": "api"});
                if let Some(r) = &session.role {
                    ctx["role"] = json!(r);
                }
                if let Some(a) = &session.auth_level {
                    ctx["auth_level"] = json!(a);
                }
                let mut i = json!({"branch": branch, "object_id": oid, "ctx": ctx});
                for (k, v) in &sel {
                    i[k] = v.clone();
                }
                let kind = s(e, "kind").unwrap_or("cascade");
                let mut r = match kind {
                    "cascade" => {
                        i["property"] = e["property"].clone();
                        self.explain_cascade(&i)?
                    }
                    "decision" => {
                        i["port"] = e["port"].clone();
                        if let Some(p) = e.get("params").filter(|v| !v.is_null()) {
                            i["params"] = p.clone();
                        }
                        let mut r = self.decide(&i)?;
                        let (_, _, ev, _) = self.object_view(&branch, &i, &oid)?;
                        r["evaluated_at"] = json!(ev);
                        r
                    }
                    _ => return Err(invalid("explain kind is cascade or decision")),
                };
                let ev = r
                    .as_object_mut()
                    .and_then(|m| m.remove("evaluated_at"))
                    .unwrap_or(json!({}));
                Ok(json!({"items": [r], "evaluated_at": ev}))
            }
            "history" => {
                let h = &b["history"];
                let oid = self.object_of(&branch, &h["target"], &sel)?;
                let mut i = json!({"branch": branch, "object_id": oid});
                for k in ["from", "to"] {
                    if let Some(v) = h.get(k).filter(|v| !v.is_null()) {
                        i[k] = v.clone();
                    }
                }
                let r = self.history(&i)?;
                Ok(json!({"items": r["commits"], "evaluated_at": r["evaluated_at"]}))
            }
            "timeline" => {
                let t = &b["timeline"];
                let oid = self.object_of(&branch, &t["target"], &sel)?;
                let mut i = json!({"branch": branch, "object_id": oid,
                    "from": t.get("from").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!("0001-01-01"))});
                if let Some(v) = t.get("to").filter(|v| !v.is_null()) {
                    i["to"] = v.clone();
                }
                for (k, v) in &sel {
                    i[k] = v.clone();
                }
                let r = self.read_range(&i)?;
                Ok(json!({"items": r["segments"], "evaluated_at": r["evaluated_at"]}))
            }
            _ => {
                let d = &b["diff"];
                let items =
                    Repo::new(self.store.as_mut(), &self.registry).diff(&d["from"], &d["to"])?;
                Ok(json!({"items": items, "evaluated_at": {}}))
            }
        }
    }

    // ---- subscriptions (STD-PROTO-012, -013) ----

    fn subscribe(
        &mut self,
        session: &mut Session,
        b: &Map<String, Value>,
    ) -> Result<Value, BpaError> {
        let kind = one_of(
            b,
            &["target", "query", "events"],
            "subscribe carries exactly one of target, query, events",
        )?;
        let branch = b
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let head = self.head_cseq(&branch)?;
        let retention = Limits::default().retention;
        let events = kind == "events";
        let released = if events {
            self.timer_effects(&branch)?.len() as u64
        } else {
            0
        };
        let (cseq, n) = match b.get("from_cursor").filter(|v| !v.is_null()) {
            Some(c) => {
                let (cb, cseq, n) = ubos_proto::parse_cursor(c)?;
                if cb != branch {
                    return Err(invalid("the cursor names another branch"));
                }
                if cseq > head || (events && n > released) {
                    return Err(invalid("the cursor is ahead of the branch"));
                }
                if head - cseq > retention {
                    return Err(BpaError::with_details(
                        "INT.CURSOR_EXPIRED",
                        json!({"retention": retention, "head": ubos_proto::cursor(&branch, head, 0)}),
                    ));
                }
                (cseq, if events { n } else { 0 })
            }
            None => (head, released),
        };
        let spec = b[kind].clone();
        let mut sub = Subscription {
            kind: kind.to_string(),
            spec: spec.clone(),
            branch: branch.clone(),
            cseq,
            n,
            object_id: None,
            unacked: Vec::new(),
            queued: Vec::new(),
        };
        match kind {
            "target" => {
                let oid = match spec.as_str() {
                    Some(t) => {
                        let (view, _) = Repo::new(self.store.as_mut(), &self.registry)
                            .view(&branch, &json!({}))?;
                        crate::relations::resolve(&view.state, t, &self.store.authority())
                    }
                    None => None,
                };
                sub.object_id = Some(oid.ok_or_else(|| err("IAM.NOT_FOUND", "unknown object"))?);
            }
            "query" => {
                let Some(q) = spec
                    .as_object()
                    .filter(|q| q.get("from").is_some_and(Value::is_string))
                else {
                    return Err(err(
                        "QRY.INVALID_QUERY",
                        "a subscription query is a DAT-Query",
                    ));
                };
                let mut q = q.clone();
                q.insert("branch".into(), json!(branch));
                q.insert("limit".into(), json!(1));
                self.query(&Value::Object(q))?;
            }
            _ => {}
        }
        session.next_sub += 1;
        let n = session.next_sub;
        session.subs.insert(n, sub);
        Ok(
            json!({"subscription_id": format!("sub-{n}"), "cursor": ubos_proto::cursor(&branch, cseq, n)}),
        )
    }

    fn sub_number(session: &Session, b: &Map<String, Value>) -> Result<u64, BpaError> {
        let sid = b
            .get("subscription_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        sid.strip_prefix("sub-")
            .and_then(|n| n.parse::<u64>().ok())
            .filter(|n| session.subs.contains_key(n))
            .ok_or_else(|| err("IAM.NOT_FOUND", "unknown subscription"))
    }

    fn unsubscribe(session: &mut Session, b: &Map<String, Value>) -> Result<Value, BpaError> {
        let n = Self::sub_number(session, b)?;
        session.subs.remove(&n);
        Ok(json!({"subscription_id": format!("sub-{n}")}))
    }

    fn ack_emits(session: &mut Session, b: &Map<String, Value>) -> Result<Value, BpaError> {
        let n = Self::sub_number(session, b)?;
        let cursor = b.get("cursor").cloned().unwrap_or(Value::Null);
        let (_, cseq, k) = ubos_proto::parse_cursor(&cursor)?;
        if let Some(sub) = session.subs.get_mut(&n) {
            sub.unacked.retain(|(pos, _)| *pos > (cseq, k));
        }
        Ok(json!({"subscription_id": format!("sub-{n}"), "cursor": cursor}))
    }

    /// The effects released on a branch without a commit, in outbox order (STD-TIME-050);
    /// the store keeps them under commit sequence 0.
    fn timer_effects(&mut self, branch: &str) -> Result<Vec<Value>, BpaError> {
        self.store.outbox_released(branch, 0)
    }

    /// An event of an emit body.
    fn event(e: &Value) -> Value {
        json!({"event_id": e["effect_id"], "type": e["type"], "object_id": e.get("object_id").cloned().unwrap_or(Value::Null),
            "payload": e.get("payload").cloned().unwrap_or_else(|| json!({}))})
    }

    /// The changes (or events) of a commit for a subscription; `None` when it has none.
    fn changes_of(
        &mut self,
        sub: &Subscription,
        cseq: u64,
    ) -> Result<Option<Vec<Value>>, BpaError> {
        let branch = sub.branch.clone();
        if sub.kind == "events" {
            let types: Vec<String> = sub
                .spec
                .get("types")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            // released effects, acknowledged or not: a host acknowledges the outbox
            // after every operation
            let evs: Vec<Value> = self
                .store
                .outbox_released(&branch, cseq)?
                .into_iter()
                .filter(|e| {
                    types.is_empty() || s(e, "type").is_some_and(|t| types.iter().any(|x| x == t))
                })
                .map(|e| Self::event(&e))
                .collect();
            return Ok(if evs.is_empty() { None } else { Some(evs) });
        }
        let (commit_id, ops) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let cid = repo.commit_at(&branch, Some(cseq))?;
            let c = repo.commit(cid)?;
            (c.commit_id, c.ops)
        };
        let mut changed: Vec<String> = ops.keys().cloned().collect();
        let objects = Repo::new(self.store.as_mut(), &self.registry)
            .objects_in_ids(&branch, cseq, &changed)?;
        if sub.kind == "target" {
            changed.retain(|o| Some(o) == sub.object_id.as_ref());
        } else {
            let mut q = sub.spec.as_object().cloned().unwrap_or_default();
            q.insert("branch".into(), json!(branch));
            q.insert("commit".into(), json!(commit_id));
            q.insert("limit".into(), json!(1000));
            let matching: BTreeSet<String> = self.query(&Value::Object(q))?["objects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            let cls = s(&sub.spec, "from").unwrap_or_default().to_string();
            changed.retain(|o| {
                matching.contains(o)
                    || objects.get(o).is_some_and(|ob| {
                        self.registry
                            .class(&ob.class)
                            .map(|c| c.linearization.contains(&cls))
                            .unwrap_or(false)
                    })
            });
        }
        let mut items = Vec::new();
        for oid in changed {
            let Some(o) = objects.get(&oid) else {
                continue;
            };
            let kind = ops[&oid]
                .last()
                .and_then(|op| s(op, "kind"))
                .unwrap_or("update");
            let op = match kind {
                "create" => "added",
                "end" => "ended",
                _ => "changed",
            };
            items.push(json!({"object_id": oid, "class": o.class, "op": op, "version_hash": o.head, "cseq": cseq}));
        }
        Ok(if items.is_empty() { None } else { Some(items) })
    }

    /// Deliver the emits of the commits since each subscription's cursor, within the
    /// window of unacknowledged emits.
    fn catch_up(&mut self, session: &mut Session) -> Result<Vec<Value>, BpaError> {
        let mut out = Vec::new();
        let numbers: Vec<u64> = session.subs.keys().copied().collect();
        for n in numbers {
            let mut sub = session.subs[&n].clone();
            let head = self.head_cseq(&sub.branch)?;
            while sub.cseq < head {
                sub.cseq += 1;
                let Some(items) = self.changes_of(&sub, sub.cseq)? else {
                    continue;
                };
                let mut body = json!({"subscription_id": format!("sub-{n}"), "cursor": ubos_proto::cursor(&sub.branch, sub.cseq, sub.n)});
                body[if sub.kind == "events" {
                    "events"
                } else {
                    "changes"
                }] = json!(items);
                sub.queued.push(((sub.cseq, sub.n), body));
            }
            if sub.kind == "events" {
                // effects released without a commit follow the commits, one frame each (1.0-rc6)
                let types: Vec<String> = sub
                    .spec
                    .get("types")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                let timers = self.timer_effects(&sub.branch)?;
                for e in timers.iter().skip(sub.n as usize) {
                    sub.n += 1;
                    if !types.is_empty()
                        && !s(e, "type").is_some_and(|t| types.iter().any(|x| x == t))
                    {
                        continue;
                    }
                    let body = json!({"subscription_id": format!("sub-{n}"),
                        "cursor": ubos_proto::cursor(&sub.branch, sub.cseq, sub.n), "events": [Self::event(e)]});
                    sub.queued.push(((sub.cseq, sub.n), body));
                }
            }
            while !sub.queued.is_empty() && (sub.unacked.len() as u64) < session.window {
                let e = sub.queued.remove(0);
                out.push(ubos_proto::frame(
                    &session.sid(),
                    "emit",
                    e.1.clone(),
                    None,
                    None,
                ));
                sub.unacked.push(e);
            }
            session.subs.insert(n, sub);
        }
        Ok(out)
    }
}
