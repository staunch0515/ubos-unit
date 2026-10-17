//! The operations layer (DSN-SDK-002, -003, -005, -006; IF-SDK-002, -003, -005): reads and
//! queries with temporal and branch selectors, snapshot-consistent paging, mutations with
//! automatic idempotency keys and the retry policy, and subscriptions that resume.

use crate::decimal::reject_floats;
use crate::error::{Error, Result};
use crate::session::{Client, SubState, Update};
use crate::util::{TraceContext, uuid7};
use futures_util::Stream;
use serde_json::{Map, Value, json};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// The coordinates of a read (DSN-SDK-006, STD-ADDR-002): `branch`, `time` (transaction
/// time: the snapshot at that instant), `commit` (at most one of `time` and `commit`) and
/// `asof` (valid time).
#[derive(Debug, Clone, Default)]
pub struct Selectors {
    /// Branch (default `main`).
    pub branch: Option<String>,
    /// Valid time: a date or a canonical instant (`2026-10-17T08:00:00.000000Z`).
    pub asof: Option<String>,
    /// Transaction time: a canonical instant.
    pub time: Option<String>,
    /// A commit.
    pub commit: Option<String>,
}

impl Selectors {
    /// On a branch.
    pub fn branch(b: &str) -> Selectors {
        Selectors {
            branch: Some(b.to_string()),
            ..Selectors::default()
        }
    }

    pub(crate) fn apply(&self, body: &mut Value) {
        for (k, v) in [
            ("branch", &self.branch),
            ("asof", &self.asof),
            ("time", &self.time),
            ("commit", &self.commit),
        ] {
            if let Some(v) = v {
                body[k] = json!(v);
            }
        }
    }
}

/// A read item: the object version with its evidence (DSN-SDK-006).
#[derive(Debug, Clone)]
pub struct Item(pub Value);

impl Item {
    /// The object identifier.
    pub fn object_id(&self) -> &str {
        self.0["object_id"].as_str().unwrap_or_default()
    }
    /// The class.
    pub fn class(&self) -> &str {
        self.0["class"].as_str().unwrap_or_default()
    }
    /// The slug.
    pub fn slug(&self) -> &str {
        self.0["slug"].as_str().unwrap_or_default()
    }
    /// The version identifier (`version_hash`).
    pub fn version(&self) -> &str {
        self.0["version_hash"].as_str().unwrap_or_default()
    }
    /// The commit the version was read at, when the node names it.
    pub fn commit(&self) -> Option<&str> {
        self.0["commit"].as_str()
    }
    /// The body.
    pub fn body(&self) -> &Value {
        &self.0["body"]
    }
    /// The evaluation evidence: the coordinates the read was evaluated at.
    pub fn evaluated_at(&self) -> &Value {
        &self.0["evaluated_at"]
    }
}

/// Options of a call (IF-SDK-003).
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    /// Branch.
    pub branch: Option<String>,
    /// The idempotency key; generated (UUIDv7) when absent and reused on retries
    /// (DSN-SDK-002).
    pub idempotency_key: Option<String>,
    /// The deadline of the call including its retries.
    pub deadline: Option<Duration>,
    /// The head `cseq` the caller saw: a moved head is `TXN.HEAD_MOVED`, not retried.
    pub base: Option<u64>,
    /// A message for the process.
    pub message: Option<String>,
    /// The reason (audit).
    pub reason: Option<String>,
    /// The caller's trace context; the SDK sends a child span of it (DSN-SDK-016).
    pub trace: Option<TraceContext>,
}

/// The outcome of a mutation: `{process_id, commits, versions, warnings, result}` or a
/// pending change.
#[derive(Debug, Clone)]
pub struct MutateResult(pub Value);

impl MutateResult {
    /// The process.
    pub fn process_id(&self) -> &str {
        self.0["process_id"].as_str().unwrap_or_default()
    }
    /// The port's result.
    pub fn result(&self) -> &Value {
        &self.0["result"]
    }
    /// The commits made.
    pub fn commits(&self) -> &Value {
        &self.0["commits"]
    }
    /// The version hashes of the objects the commits wrote.
    pub fn versions(&self) -> &Value {
        &self.0["versions"]
    }
    /// A pending change (`require` decision), when there is one.
    pub fn pending(&self) -> Option<&Value> {
        self.0.get("pending")
    }
}

/// What a subscription follows (STD-PROTO-012).
#[derive(Debug, Clone)]
pub enum Target {
    /// One object (`ubos://…`).
    Object(String),
    /// The objects of a query.
    Query(Value),
    /// Released events, optionally of some types.
    Events(Vec<String>),
}

impl Client {
    fn deadline(&self, d: Option<Duration>) -> Instant {
        Instant::now() + d.unwrap_or(self.inner.cfg.retry.deadline)
    }

    /// A query request with retries of transport failures and retryable errors.
    pub(crate) async fn ask(&self, body: Value) -> Result<Value> {
        let deadline = self.deadline(None);
        self.inner
            .cfg
            .retry
            .run(deadline, true, |_| {
                self.request_until("query", body.clone(), None, deadline)
            })
            .await
    }

    pub(crate) fn body(&self, extra: Value) -> Value {
        let mut b = json!({"vae": self.inner.cfg.vae});
        if let (Some(o), Value::Object(e)) = (b.as_object_mut(), extra) {
            o.extend(e);
        }
        b
    }

    /// Read one object (`ubos://vae/Class/slug` or with selectors in the URI).
    pub async fn read(&self, uri: &str, sel: &Selectors) -> Result<Item> {
        let mut b = self.body(json!({"read": uri}));
        sel.apply(&mut b);
        let r = self.ask(b).await?;
        let mut item = r["items"][0].clone();
        if item.is_null() {
            return Err(Error::new(
                "IAM.NOT_FOUND",
                "no such object",
                json!({"uri": uri}),
            ));
        }
        item["evaluated_at"] = r["evaluated_at"].clone();
        Ok(Item(item))
    }

    /// The first page of a query (text or `DAT-Query`) with up to `limit` items; follow
    /// [`Page::next`] for the rest. The pages share the snapshot of the first one
    /// (DSN-SDK-005).
    pub async fn query_page(&self, query: Value, sel: &Selectors, limit: u32) -> Result<Page> {
        let q = match query {
            Value::String(t) => json!({"text": t, "limit": limit}),
            Value::Object(mut o) => {
                o.entry("limit").or_insert(json!(limit));
                Value::Object(o)
            }
            other => {
                return Err(Error::new(
                    "MODEL.INVALID_VALUE",
                    "a query is its text or a DAT-Query object",
                    other,
                ));
            }
        };
        let mut b = self.body(json!({"query": q}));
        sel.apply(&mut b);
        let r = self.ask(b.clone()).await?;
        Ok(Page::new(self.clone(), b, r))
    }

    /// Every item of a query as an async stream following the cursors (DSN-SDK-005).
    pub fn query(
        &self,
        query: Value,
        sel: Selectors,
        page_size: u32,
    ) -> impl Stream<Item = Result<Item>> + '_ {
        enum St {
            Start(Value, Selectors),
            Page(Page, std::vec::IntoIter<Item>),
            Done,
        }
        futures_util::stream::unfold(St::Start(query, sel), move |st| async move {
            let mut st = st;
            loop {
                st = match st {
                    St::Done => return None,
                    St::Start(q, sel) => match self.query_page(q, &sel, page_size).await {
                        Ok(p) => {
                            let it = p.items.clone().into_iter();
                            St::Page(p, it)
                        }
                        Err(e) => return Some((Err(e), St::Done)),
                    },
                    St::Page(p, mut it) => match it.next() {
                        Some(x) => return Some((Ok(x), St::Page(p, it))),
                        None => match p.next().await {
                            Ok(Some(n)) => {
                                let it = n.items.clone().into_iter();
                                St::Page(n, it)
                            }
                            Ok(None) => return None,
                            Err(e) => return Some((Err(e), St::Done)),
                        },
                    },
                };
            }
        })
    }

    /// `history {target, from?, to?}`: the commits of an object.
    pub async fn history(&self, target: &str, sel: &Selectors) -> Result<Value> {
        let mut b = self.body(json!({"history": {"target": target}}));
        sel.apply(&mut b);
        self.ask(b).await
    }

    /// `timeline {target}`: the valid-time segments of an object.
    pub async fn timeline(&self, target: &str, sel: &Selectors) -> Result<Value> {
        let mut b = self.body(json!({"timeline": {"target": target}}));
        sel.apply(&mut b);
        self.ask(b).await
    }

    /// `diff {from, to}`: the differences between two coordinates, each a branch and a
    /// commit sequence (`cseq`, as a mutation's result gives it).
    pub async fn diff(&self, from: (&str, u64), to: (&str, u64)) -> Result<Value> {
        self.ask(
            self.body(json!({"diff": {"from": {"branch": from.0, "time": from.1},
            "to": {"branch": to.0, "time": to.1}}})),
        )
        .await
    }

    /// `explain {target, kind, property | port}`.
    pub async fn explain(&self, explain: Value, sel: &Selectors) -> Result<Value> {
        let mut b = self.body(json!({"explain": explain}));
        sel.apply(&mut b);
        self.ask(b).await
    }

    /// A mutation (`port` or `commit` body) with an idempotency key reused on every retry;
    /// `TXN.HEAD_MOVED` is retried only without a `base` (re-executable).
    pub async fn mutate(&self, payload: Value, o: &CallOptions) -> Result<MutateResult> {
        reject_floats(&payload)?;
        let key = o.idempotency_key.clone().unwrap_or_else(uuid7);
        let mut b = self.body(payload);
        b["idempotency_key"] = json!(key);
        for (k, v) in [
            ("branch", &o.branch),
            ("message", &o.message),
            ("reason", &o.reason),
        ] {
            if let Some(v) = v {
                b[k] = json!(v);
            }
        }
        if let Some(base) = o.base {
            b["base"] = json!(base);
        }
        let deadline = self.deadline(o.deadline);
        let trace = o.trace.clone();
        self.inner
            .cfg
            .retry
            .run(deadline, o.base.is_none(), |_| {
                self.request_until("mutate", b.clone(), trace.as_ref(), deadline)
            })
            .await
            .map(MutateResult)
    }

    /// Invoke a port of an object (IF-SDK-003).
    pub async fn invoke(
        &self,
        target: &str,
        port: &str,
        params: Value,
        o: &CallOptions,
    ) -> Result<MutateResult> {
        self.mutate(
            json!({"port": {"target": target, "name": port, "params": params}}),
            o,
        )
        .await
    }

    /// Commit raw changes (principals with `commit.raw`: import, Forge).
    pub async fn commit(&self, changes: Value, o: &CallOptions) -> Result<MutateResult> {
        self.mutate(json!({"commit": {"changes": changes}}), o)
            .await
    }

    /// Subscribe (IF-SDK-005); the subscription resumes after reconnections from its last
    /// acknowledged cursor.
    pub async fn subscribe(
        &self,
        target: Target,
        sel: &Selectors,
        from_cursor: Option<&str>,
    ) -> Result<Subscription> {
        let mut b = match target {
            Target::Object(u) => json!({"target": u}),
            Target::Query(q) => json!({"query": q}),
            Target::Events(types) => {
                let mut m = Map::new();
                if !types.is_empty() {
                    m.insert("types".into(), json!(types));
                }
                json!({"events": m})
            }
        };
        b["vae"] = json!(self.inner.cfg.vae);
        if let Some(br) = &sel.branch {
            b["branch"] = json!(br);
        }
        let mut first = b.clone();
        if let Some(c) = from_cursor {
            first["from_cursor"] = json!(c);
        }
        let r = self.subscribe_raw(first).await?;
        let (tx, rx) = mpsc::unbounded_channel();
        let local = self
            .inner
            .generation
            .load(std::sync::atomic::Ordering::SeqCst)
            .wrapping_mul(1_000_003)
            .wrapping_add(crate::util::jitter(u64::MAX >> 1));
        self.inner.subs.lock().expect("subs").insert(
            local,
            SubState {
                body: b,
                cursor: r["cursor"].as_str().map(str::to_string),
                server_id: r["subscription_id"].as_str().map(str::to_string),
                tx,
            },
        );
        Ok(Subscription {
            client: self.clone(),
            local,
            rx,
        })
    }
}

/// One page of a query result.
#[derive(Debug, Clone)]
pub struct Page {
    client: Option<Client>,
    body: Value,
    /// The items.
    pub items: Vec<Item>,
    /// The cursor of the next page, if any.
    pub cursor: Option<Value>,
    /// The coordinates the query was evaluated at (the snapshot).
    pub evaluated_at: Value,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("url", &self.inner.cfg.url)
            .field("vae", &self.inner.cfg.vae)
            .finish()
    }
}

impl Page {
    fn new(client: Client, body: Value, r: Value) -> Page {
        let ev = r["evaluated_at"].clone();
        let items = r["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut i| {
                i["evaluated_at"] = ev.clone();
                Item(i)
            })
            .collect();
        Page {
            client: Some(client),
            body,
            items,
            cursor: r.get("cursor").filter(|c| !c.is_null()).cloned(),
            evaluated_at: ev,
        }
    }

    /// The next page, or `None` after the last.
    pub async fn next(&self) -> Result<Option<Page>> {
        let (Some(c), Some(client)) = (&self.cursor, &self.client) else {
            return Ok(None);
        };
        let mut b = self.body.clone();
        b["query"]["cursor"] = c.clone();
        let r = client.ask(b.clone()).await?;
        Ok(Some(Page::new(client.clone(), b, r)))
    }
}

/// A subscription: its updates in order (IF-SDK-005).
pub struct Subscription {
    client: Client,
    local: u64,
    rx: mpsc::UnboundedReceiver<Update>,
}

impl Subscription {
    /// The next update, or `None` when the subscription is closed.
    pub async fn next(&mut self) -> Option<Update> {
        self.rx.recv().await
    }

    /// The next update within `timeout`.
    pub async fn next_within(&mut self, timeout: Duration) -> Option<Update> {
        tokio::time::timeout(timeout, self.rx.recv())
            .await
            .ok()
            .flatten()
    }

    /// The cursor of the last update acknowledged (the resume point).
    pub fn cursor(&self) -> Option<String> {
        self.client
            .inner
            .subs
            .lock()
            .expect("subs")
            .get(&self.local)
            .and_then(|s| s.cursor.clone())
    }

    /// End the subscription (`unsubscribe`).
    pub async fn close(self) -> Result<()> {
        let server = self
            .client
            .inner
            .subs
            .lock()
            .expect("subs")
            .remove(&self.local)
            .and_then(|s| s.server_id);
        if let Some(id) = server {
            self.client
                .request("unsubscribe", json!({"subscription_id": id}), None)
                .await?;
        }
        Ok(())
    }
}
