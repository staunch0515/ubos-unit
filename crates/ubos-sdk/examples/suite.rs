//! The Rust runner of SUITE-SDK-PROTO and SUITE-SDK-NET (PH-1 step 17d, DEC-078): every
//! case of `verify/sdk/cases.json` against a node behind the fault proxy of
//! `verify/sdk/sdk_suite.py`, which starts it:
//!
//! ```text
//! suite --url PROXY --direct NODE --control CONTROL --vae V --user U --password P
//!       --api-key K --token T --binding ws|http --tag TAG [--cases P01,N03]
//! ```
//!
//! One JSON line per case: `{"case", "ok", "checks": [{"name", "ok", "detail"}], "ms"}`.
//! The typed API is the one `forge types` generated for `verify/sdk/fixture`.

#[rustfmt::skip]
#[path = "../../../verify/sdk/fixture/generated/types.rs"]
mod types;

use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use types::{Counterparty, CounterpartyAny};
use ubos_sdk::registry::REGISTRY;
use ubos_sdk::typed::Cmp;
use ubos_sdk::{
    Auth, Binding, CallOptions, ChangeSetAction, Client, Code, Config, Decimal, Error, Money,
    RetryPolicy, Selectors, Target, TraceContext, Update, UploadOptions,
};

struct Args(HashMap<String, String>);

impl Args {
    fn get(&self, k: &str) -> String {
        self.0.get(k).cloned().unwrap_or_default()
    }
}

#[derive(Default)]
struct Case {
    checks: Vec<Value>,
}

impl Case {
    fn check(&mut self, name: &str, ok: bool, detail: impl std::fmt::Debug) {
        let d = if ok {
            String::new()
        } else {
            format!("{detail:?}")
        };
        self.checks.push(
            json!({"name": name, "ok": ok, "detail": d.chars().take(600).collect::<String>()}),
        );
    }
    fn ok(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|c| c["ok"] == true)
    }
}

struct Env {
    a: Args,
    c: Client,
    direct: Client,
    tag: String,
    vae: String,
    next: std::sync::atomic::AtomicU64,
}

impl Env {
    fn binding(&self) -> Binding {
        if self.a.get("binding") == "http" {
            Binding::Http
        } else {
            Binding::WebSocket
        }
    }
    fn cfg(&self, url: &str, auth: Auth) -> Config {
        Config::new(url, &self.vae, auth).binding(self.binding())
    }
    fn password(&self) -> Auth {
        Auth::Password {
            username: self.a.get("user"),
            password: self.a.get("password"),
        }
    }
    /// A fresh object identifier and slug of this run.
    fn oid(&self) -> (String, String) {
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let id = ubos_sdk::util::uuid7();
        (id, format!("{}-{n}", self.tag))
    }
    fn uri(&self, class: &str, slug: &str) -> String {
        format!("ubos://{}/{class}/{slug}", self.vae)
    }
    /// A control request to the fault proxy: `/fault?mode=…`, `/restart`, `/stats`.
    async fn control(&self, path: &str) -> Value {
        let url = self.a.get("control");
        let host = url.trim_start_matches("http://").trim_end_matches('/');
        let Ok(mut s) = tokio::net::TcpStream::connect(host).await else {
            return Value::Null;
        };
        let req = format!("POST {path} HTTP/1.0\r\nhost: {host}\r\ncontent-length: 0\r\n\r\n");
        if s.write_all(req.as_bytes()).await.is_err() {
            return Value::Null;
        }
        let mut buf = Vec::new();
        let _ = s.read_to_end(&mut buf).await;
        let text = String::from_utf8_lossy(&buf);
        text.split("\r\n\r\n")
            .nth(1)
            .and_then(|b| serde_json::from_str(b).ok())
            .unwrap_or(Value::Null)
    }
    async fn customer(
        &self,
        c: &Client,
        name: &str,
        extra: Value,
    ) -> ubos_sdk::Result<(String, String)> {
        let (id, slug) = self.oid();
        let mut body = json!({"name": name, "country": self.tag});
        if let (Some(b), Value::Object(e)) = (body.as_object_mut(), extra) {
            b.extend(e);
        }
        c.commit(json!([{"op": "create", "class": "Customer", "object_id": id, "slug": slug, "body": body}]), &CallOptions::default())
            .await?;
        Ok((id, slug))
    }
    async fn rename(
        &self,
        c: &Client,
        id: &str,
        slug: &str,
        name: &str,
        o: &CallOptions,
    ) -> ubos_sdk::Result<ubos_sdk::MutateResult> {
        let item = c
            .read(&self.uri("Customer", slug), &Selectors::default())
            .await?;
        c.commit(json!([{"op": "update", "object_id": id, "base": item.version(), "body": {"name": name}}]), o)
            .await
    }
    async fn account(&self) -> ubos_sdk::Result<(String, String)> {
        let (id, slug) = self.oid();
        let o = CallOptions {
            branch: Some("feature/sdk".into()),
            ..CallOptions::default()
        };
        self.c
            .commit(json!([{"op": "create", "class": "Account", "object_id": id, "slug": slug, "body": {"name": "acct", "balance": "0"}}]), &o)
            .await?;
        Ok((id, slug))
    }
}

fn history_len(v: &Value) -> usize {
    v["items"].as_array().map(Vec::len).unwrap_or(0)
}

/// A canonical instant: RFC 3339 UTC with six fraction digits.
fn rfc3339(t: std::time::SystemTime) -> String {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let (secs, us) = (d.as_secs() as i64, d.subsec_micros());
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil from days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{us:06}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn data(n: usize, seed: u32) -> Vec<u8> {
    (0..n as u32)
        .map(|i| ((i ^ seed).wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect()
}

async fn run(e: &Env, id: &str) -> Case {
    let mut k = Case::default();
    let c = &e.c;
    let none = CallOptions::default();
    match id {
        "P01" => {
            let s = c.session();
            k.check("protocol 1.0 negotiated", c.version() == "1.0", &s);
            k.check(
                "the session names its principal",
                !s["principal"].is_null(),
                &s,
            );
        }
        "P02" => {
            for (name, auth) in [
                ("password", e.password()),
                ("api key", Auth::ApiKey(e.a.get("api-key"))),
                ("bearer token", Auth::Token(e.a.get("token"))),
            ] {
                let r = Client::connect(e.cfg(&e.a.get("url"), auth)).await;
                let read = match &r {
                    Ok(x) => x
                        .query_page(json!("from Customer"), &Selectors::default(), 1)
                        .await
                        .map(|_| ()),
                    Err(err) => Err(err.clone()),
                };
                k.check(
                    &format!("{name}: connected and read"),
                    read.is_ok(),
                    read.err(),
                );
            }
            let t0 = Instant::now();
            let bad = Client::connect(e.cfg(
                &e.a.get("url"),
                Auth::Password {
                    username: e.a.get("user"),
                    password: "wrong".into(),
                },
            ))
            .await;
            k.check(
                "a wrong password: a typed IAM error, not retryable, not retried",
                bad.as_ref().err().is_some_and(|x| {
                    x.code.is_some() && x.code_text.starts_with("IAM.") && !x.retryable()
                }) && t0.elapsed() < Duration::from_secs(5),
                bad.err(),
            );
        }
        "P03" => {
            let changes: Vec<Value> = (0..3)
                .map(|i| {
                    let (oid, slug) = e.oid();
                    json!({"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": format!("p03 {i}"), "country": e.tag}})
                })
                .collect();
            let r = c.commit(json!(changes), &none).await;
            k.check(
                "the process, one commit and three versions",
                r.as_ref().is_ok_and(|m| {
                    !m.process_id().is_empty()
                        && m.commits().as_array().is_some_and(|a| a.len() == 1)
                        && m.0["versions"].as_object().is_some_and(|v| v.len() == 3)
                }),
                r.as_ref().map(|m| m.0.clone()),
            );
        }
        "P04" => {
            let (oid, slug) = e.oid();
            let created = c.commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "p04 first", "country": e.tag}}]), &none).await;
            let first_commit = created
                .as_ref()
                .ok()
                .and_then(|m| m.commits()[0].as_str().map(str::to_string));
            let uri = e.uri("Customer", &slug);
            let between = rfc3339(std::time::SystemTime::now());
            tokio::time::sleep(Duration::from_millis(1100)).await;
            let upd = e.rename(c, &oid, &slug, "p04 second", &none).await;
            let commit = upd
                .as_ref()
                .ok()
                .and_then(|m| m.commits()[0].as_str().map(str::to_string));
            k.check("update", upd.is_ok(), upd.as_ref().err());
            let main = c.read(&uri, &Selectors::branch("main")).await;
            k.check(
                "branch main: the latest version with its version and evidence",
                main.as_ref().is_ok_and(|i| {
                    i.body()["name"] == "p04 second"
                        && i.version().starts_with("sha256:")
                        && !i.evaluated_at().is_null()
                }),
                main.as_ref().map(|i| i.0.clone()),
            );
            let at_time = c
                .read(
                    &uri,
                    &Selectors {
                        time: Some(between.clone()),
                        ..Selectors::default()
                    },
                )
                .await;
            k.check(
                "time: the snapshot before the update",
                at_time
                    .as_ref()
                    .is_ok_and(|i| i.body()["name"] == "p04 first"),
                at_time.as_ref().map(|i| i.0.clone()),
            );
            let at = c
                .read(
                    &uri,
                    &Selectors {
                        commit: commit.clone(),
                        ..Selectors::default()
                    },
                )
                .await;
            k.check(
                "commit: the version of that commit",
                at.as_ref().is_ok_and(|i| i.body()["name"] == "p04 second"),
                (&commit, at.as_ref().map(|i| i.0.clone())),
            );
            if let Some(fc) = first_commit.clone() {
                let old = c
                    .read(
                        &uri,
                        &Selectors {
                            commit: Some(fc),
                            ..Selectors::default()
                        },
                    )
                    .await;
                k.check(
                    "commit: the first version at its commit",
                    old.as_ref().is_ok_and(|i| i.body()["name"] == "p04 first"),
                    old.as_ref().map(|i| i.0.clone()),
                );
            }
            let today = between[..10].to_string();
            let both = c
                .read(
                    &uri,
                    &Selectors {
                        asof: Some(today.clone()),
                        time: Some(between.clone()),
                        ..Selectors::default()
                    },
                )
                .await;
            k.check(
                "time and asof: the evidence names the snapshot's commit and the valid segment",
                both.as_ref().is_ok_and(|i| {
                    i.evaluated_at().as_object().is_some_and(|m| {
                        m.values().any(|v| {
                            Some(v.as_str().unwrap_or_default()) == first_commit.as_deref()
                        })
                    }) && i.0["valid"]["from"]
                        .as_str()
                        .is_some_and(|f| f <= today.as_str())
                }),
                both.as_ref().map(|i| i.0.clone()),
            );
            let missing = c
                .read(
                    &e.uri("Customer", &format!("{}-missing", e.tag)),
                    &Selectors::default(),
                )
                .await;
            k.check(
                "a missing object: IAM.NOT_FOUND, typed",
                missing
                    .as_ref()
                    .err()
                    .is_some_and(|x| x.is(Code::IamNotFound)),
                missing.err(),
            );
        }
        "P05" => {
            let q = format!(
                "from Customer where this.country == \"{}-p05\" order by this.name",
                e.tag
            );
            let changes: Vec<Value> = (0..25)
                .map(|i| { let (oid, slug) = e.oid(); json!({"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": format!("p05 {i:02}"), "country": format!("{}-p05", e.tag)}}) })
                .collect();
            let r = c.commit(json!(changes), &none).await;
            k.check("25 objects", r.is_ok(), r.err());
            let p1 = c.query_page(json!(q), &Selectors::default(), 10).await;
            let Ok(p1) = p1 else {
                k.check("first page", false, p1.err());
                return k;
            };
            let (oid, slug) = e.oid();
            let extra = c.commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "p05 extra", "country": format!("{}-p05", e.tag)}}]), &none).await;
            k.check("a commit between pages", extra.is_ok(), extra.err());
            let mut items = p1.items.clone();
            let mut snaps = vec![p1.evaluated_at.clone()];
            let mut page = p1;
            let mut pages = 1;
            loop {
                match page.next().await {
                    Ok(Some(p)) => {
                        items.extend(p.items.clone());
                        snaps.push(p.evaluated_at.clone());
                        pages += 1;
                        page = p;
                    }
                    Ok(None) => break,
                    Err(err) => {
                        k.check("next page", false, err);
                        break;
                    }
                }
            }
            k.check(
                "three pages of 10, 10 and 5 items",
                pages == 3 && items.len() == 25,
                (pages, items.len()),
            );
            k.check(
                "every page of the first page's snapshot",
                snaps.windows(2).all(|w| w[0] == w[1]),
                &snaps,
            );
            let names: Vec<String> = items
                .iter()
                .map(|i| i.body()["name"].as_str().unwrap_or_default().to_string())
                .collect();
            let mut sorted = names.clone();
            sorted.sort();
            k.check(
                "ordered, without duplicates",
                names == sorted && {
                    let mut d = names.clone();
                    d.dedup();
                    d.len() == names.len()
                },
                &names,
            );
            let fresh: Vec<_> = c.query(json!(q), Selectors::default(), 10).collect().await;
            k.check(
                "a new query sees the commit made between pages",
                fresh.len() == 26 && fresh.iter().all(|x| x.is_ok()),
                fresh.len(),
            );
        }
        "P06" => {
            let (oid, slug) = e.oid();
            let created = c.commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "p06 a", "country": e.tag}}]), &none).await;
            let r1 = created.ok().and_then(|m| m.result()["cseq"].as_u64());
            let uri = e.uri("Customer", &slug);
            let upd = e.rename(c, &oid, &slug, "p06 b", &none).await;
            let r2 = upd.as_ref().ok().and_then(|m| m.result()["cseq"].as_u64());
            let h = c.history(&uri, &Selectors::default()).await;
            k.check(
                "history: the two commits of the object",
                h.as_ref().is_ok_and(|v| history_len(v) == 2),
                h.clone().map_err(|x| x.to_string()),
            );
            let t = c.timeline(&uri, &Selectors::default()).await;
            k.check(
                "timeline: the valid-time segments",
                t.as_ref()
                    .is_ok_and(|v| v["items"].as_array().is_some_and(|a| !a.is_empty())),
                t.clone().map_err(|x| x.to_string()),
            );
            if let (Some(a), Some(b)) = (r1, r2) {
                let d = c.diff(("main", a), ("main", b)).await;
                k.check(
                    "diff: the change between the two commits names the object",
                    d.as_ref().is_ok_and(|v| v.to_string().contains(&oid)),
                    d.clone().map_err(|x| x.to_string()),
                );
            } else {
                k.check("diff: commits known", false, "no commit identifiers");
            }
            if let Ok((_, aslug)) = e.account().await {
                let x = c.explain(json!({"target": e.uri("Account", &aslug), "kind": "decision", "port": "close"}), &Selectors::branch("feature/sdk")).await;
                k.check("explain: the decision of a port", x.is_ok(), x.err());
            }
        }
        "P07" => {
            let Ok((_oid, slug)) = e.account().await else {
                k.check("setup", false, "");
                return k;
            };
            let target = e.uri("Account", &slug);
            let o = CallOptions {
                branch: Some("feature/sdk".into()),
                ..CallOptions::default()
            };
            let r = c.invoke(&target, "mark", json!({"tag": "p07"}), &o).await;
            k.check(
                "the port's result",
                r.as_ref()
                    .is_ok_and(|m| m.result() == "p07" && !m.process_id().is_empty()),
                r.as_ref().map(|m| m.0.clone()),
            );
            let item = c.read(&target, &Selectors::branch("feature/sdk")).await;
            k.check(
                "its effect on the branch",
                item.as_ref().is_ok_and(|i| i.body()["tag"] == "p07"),
                item.as_ref().map(|i| i.0.clone()),
            );
        }
        "P08" => {
            let Ok((oid, slug)) = e.customer(c, "p08", json!({})).await else {
                k.check("setup", false, "");
                return k;
            };
            let uri = e.uri("Customer", &slug);
            let key = ubos_sdk::util::uuid7();
            let o = CallOptions {
                idempotency_key: Some(key.clone()),
                trace: Some(TraceContext::new()),
                ..CallOptions::default()
            };
            let v = c
                .read(&uri, &Selectors::default())
                .await
                .map(|i| i.version().to_string())
                .unwrap_or_default();
            let ch = json!([{"op": "update", "object_id": oid, "base": v, "body": {"name": "p08 renamed"}}]);
            let r1 = c.commit(ch.clone(), &o).await;
            let r2 = c.commit(ch.clone(), &o).await;
            // in another session of the same principal (STD-TXN-040)
            let other = Client::connect(e.cfg(&e.a.get("url"), e.password())).await;
            let r4 = match &other {
                Ok(x) => x.commit(ch.clone(), &o).await,
                Err(x) => Err(x.clone()),
            };
            k.check("in another session too", matches!((&r1, &r4), (Ok(a), Ok(b)) if a.process_id() == b.process_id() && a.commits() == b.commits()), (r1.as_ref().map(|m| m.0.clone()), r4.as_ref().map(|m| m.0.clone()).map_err(|x| x.to_string())));
            let changed = json!([{"op": "update", "object_id": oid, "base": v, "body": {"name": "p08 other"}}]);
            let mm = c.commit(changed, &o).await;
            k.check(
                "another payload under the key: TXN.IDEMPOTENCY_MISMATCH",
                mm.as_ref()
                    .err()
                    .is_some_and(|x| x.is(Code::TxnIdempotencyMismatch)),
                mm.as_ref().err(),
            );
            k.check("the same key returns the first process", matches!((&r1, &r2), (Ok(a), Ok(b)) if a.process_id() == b.process_id() && !a.process_id().is_empty()), (r1.as_ref().err(), r2.as_ref().err()));
            let h = c
                .history(&uri, &Selectors::default())
                .await
                .map(|v| history_len(&v));
            k.check(
                "one commit for the three calls",
                h.as_ref().is_ok_and(|n| *n == 2),
                &h,
            );
            let r3 = e.rename(c, &oid, &slug, "p08 again", &none).await;
            k.check(
                "a new key runs again",
                matches!((&r1, &r3), (Ok(a), Ok(b)) if a.process_id() != b.process_id()),
                r3.as_ref().err(),
            );
        }
        "P09" => {
            let Ok((oid, slug)) = e.customer(c, "p09", json!({})).await else {
                k.check("setup", false, "");
                return k;
            };
            let uri = e.uri("Customer", &slug);
            let before = c
                .history(&uri, &Selectors::default())
                .await
                .map(|v| history_len(&v))
                .unwrap_or(0);
            let item = c.read(&uri, &Selectors::default()).await;
            let v = item
                .as_ref()
                .map(|i| i.version().to_string())
                .unwrap_or_default();
            let stats0 = e.control("/stats").await;
            e.control("/fault?mode=cut_reply").await;
            let r = c.commit(json!([{"op": "update", "object_id": oid, "base": v, "body": {"name": "p09 once"}}]), &none).await;
            let stats1 = e.control("/stats").await;
            k.check(
                "the reply was cut by the proxy",
                stats1["cuts"].as_u64() > stats0["cuts"].as_u64(),
                (&stats0, &stats1),
            );
            k.check(
                "the retried commit returns the first result",
                r.is_ok(),
                r.as_ref().err(),
            );
            let after = c
                .history(&uri, &Selectors::default())
                .await
                .map(|v| history_len(&v))
                .unwrap_or(0);
            k.check("committed once", after == before + 1, (before, after));
            let head = r
                .as_ref()
                .ok()
                .and_then(|m| m.result()["cseq"].as_u64())
                .unwrap_or(1);
            let t0 = Instant::now();
            let stale = c
                .commit(
                    json!([{"op": "update", "object_id": oid, "body": {"name": "p09 stale"}}]),
                    &CallOptions {
                        base: Some(head.saturating_sub(1)),
                        ..CallOptions::default()
                    },
                )
                .await;
            k.check(
                "a stale base: TXN.HEAD_MOVED, not retried",
                stale
                    .as_ref()
                    .err()
                    .is_some_and(|x| x.is(Code::TxnHeadMoved))
                    && t0.elapsed() < Duration::from_secs(3),
                (stale.as_ref().err(), t0.elapsed()),
            );
        }
        "P10" => {
            let p = RetryPolicy {
                max_attempts: 3,
                base: Duration::from_millis(1),
                cap: Duration::from_millis(1),
                deadline: Duration::from_secs(5),
            };
            let mut wrong = Vec::new();
            for (_, code, _, retryable, _, _) in REGISTRY {
                let attempts = std::sync::atomic::AtomicU32::new(0);
                let _: ubos_sdk::Result<()> = p
                    .run(Instant::now() + Duration::from_secs(5), false, |_| {
                        attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        async { Err(Error::new(code, "", Value::Null)) }
                    })
                    .await;
                let n = attempts.load(std::sync::atomic::Ordering::SeqCst);
                let expect = if *code == "TXN.HEAD_MOVED" {
                    1
                } else if *retryable {
                    3
                } else {
                    1
                };
                if n != expect {
                    wrong.push((code.to_string(), n));
                }
            }
            k.check(
                &format!(
                    "{} registered codes: retried exactly when retryable",
                    REGISTRY.len()
                ),
                wrong.is_empty(),
                &wrong,
            );
            let attempts = std::sync::atomic::AtomicU32::new(0);
            let _: ubos_sdk::Result<()> = p
                .run(Instant::now() + Duration::from_secs(5), true, |_| {
                    attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    async { Err(Error::new("TXN.HEAD_MOVED", "", Value::Null)) }
                })
                .await;
            k.check(
                "TXN.HEAD_MOVED retried when the call has no base",
                attempts.into_inner() == 3,
                "",
            );
            let attempts = std::sync::atomic::AtomicU32::new(0);
            let _: ubos_sdk::Result<()> = p
                .run(Instant::now() + Duration::from_secs(5), false, |_| {
                    attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    async { Err(Error::transport("reset")) }
                })
                .await;
            k.check("transport failures retried", attempts.into_inner() == 3, "");
        }
        "P11" => {
            let status = |code: &str| REGISTRY.iter().find(|r| r.1 == code).map(|r| r.5);
            let expect =
                |k: &mut Case, name: &str, r: &ubos_sdk::Result<Value>, code: Code, text: &str| {
                    k.check(
                        name,
                        r.as_ref().err().is_some_and(|x| {
                            x.is(code)
                                && x.area().is_some()
                                && x.status == status(text)
                                && !x.retryable()
                        }),
                        r.as_ref().err(),
                    );
                };
            let r = c
                .read(&e.uri("Customer", "no-such-object"), &Selectors::default())
                .await
                .map(|i| i.0);
            expect(
                &mut k,
                "a missing object: IAM.NOT_FOUND (404)",
                &r,
                Code::IamNotFound,
                "IAM.NOT_FOUND",
            );
            let r = c
                .query_page(json!("from Customer where"), &Selectors::default(), 1)
                .await
                .map(|_| Value::Null);
            expect(
                &mut k,
                "a malformed query: QRY.INVALID_QUERY (400)",
                &r,
                Code::QryInvalidQuery,
                "QRY.INVALID_QUERY",
            );
            let (oid, slug) = e.oid();
            let r = c.commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "p11", "rating": 9}}]), &none).await.map(|m| m.0);
            expect(
                &mut k,
                "a failed validation: RULE.VALIDATION_FAILED (422)",
                &r,
                Code::RuleValidationFailed,
                "RULE.VALIDATION_FAILED",
            );
            k.check(
                "its details name the validation",
                r.as_ref()
                    .err()
                    .is_some_and(|x| x.details.to_string().contains("rating_range")),
                r.as_ref().err(),
            );
            let (oid, slug) = e.oid();
            let r = c.commit(json!([{"op": "create", "class": "NoSuchClass", "object_id": oid, "slug": slug, "body": {}}]), &none).await.map(|m| m.0);
            k.check(
                "an unknown class: a registered error",
                r.as_ref()
                    .err()
                    .is_some_and(|x| x.code.is_some() && x.status.is_some()),
                r.as_ref().err(),
            );
            let r = c.commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "x", "credit_limit": {"amount": 1.5, "currency": "USD"}}}]), &none).await.map(|m| m.0);
            k.check(
                "a float: LOGIC.FLOAT_TO_DECIMAL at the boundary with its path",
                r.as_ref().err().is_some_and(|x| {
                    x.is(Code::LogicFloatToDecimal)
                        && x.details["path"] == "commit.changes.0.body.credit_limit.amount"
                }),
                r.as_ref().err(),
            );
        }
        "P12" => {
            let Ok((oid, slug)) = e.customer(c, "p12", json!({})).await else {
                k.check("setup", false, "");
                return k;
            };
            let uri = e.uri("Customer", &slug);
            let sub = c
                .subscribe(Target::Object(uri.clone()), &Selectors::default(), None)
                .await;
            let qsub = c.subscribe(Target::Query(json!({"from": "Customer", "where": format!("this.country == \"{}-p12q\"", e.tag)})), &Selectors::default(), None).await;
            let esub = c
                .subscribe(
                    Target::Events(vec!["AccountClosed".into()]),
                    &Selectors::branch("feature/sdk"),
                    None,
                )
                .await;
            let (Ok(mut sub), Ok(mut qsub), Ok(mut esub)) = (sub, qsub, esub) else {
                k.check("subscribed", false, "");
                return k;
            };
            let _ = e.rename(c, &oid, &slug, "p12 renamed", &none).await;
            let u = sub.next_within(Duration::from_secs(10)).await;
            k.check(
                "object: the change emitted",
                matches!(&u, Some(Update::Emit(b)) if b.to_string().contains(&oid)),
                &u,
            );
            let qid = e
                .customer(c, "p12 q", json!({"country": format!("{}-p12q", e.tag)}))
                .await
                .map(|x| x.0)
                .unwrap_or_default();
            // a query subscription may also emit other changes of the class; the new object comes
            let mut got = false;
            let until = Instant::now() + Duration::from_secs(10);
            while !got && Instant::now() < until {
                match qsub.next_within(Duration::from_secs(10)).await {
                    Some(Update::Emit(b)) => got = !qid.is_empty() && b.to_string().contains(&qid),
                    _ => break,
                }
            }
            k.check("query: a new matching object emitted", got, &qid);
            if let Ok((_, aslug)) = e.account().await {
                let _ = c
                    .invoke(
                        &e.uri("Account", &aslug),
                        "close",
                        json!({}),
                        &CallOptions {
                            branch: Some("feature/sdk".into()),
                            ..CallOptions::default()
                        },
                    )
                    .await;
                let mut got = false;
                let until = Instant::now() + Duration::from_secs(10);
                while Instant::now() < until && !got {
                    match esub.next_within(Duration::from_secs(10)).await {
                        Some(Update::Emit(b)) => got = b.to_string().contains("AccountClosed"),
                        _ => break,
                    }
                }
                k.check("events: the released AccountClosed emitted", got, "");
            }
            k.check(
                "the cursor of the last update",
                sub.cursor().is_some(),
                sub.cursor(),
            );
            for (name, s) in [("object", sub), ("query", qsub), ("events", esub)] {
                let r = s.close().await;
                k.check(&format!("{name}: unsubscribed"), r.is_ok(), r.err());
            }
        }
        "P13" => {
            let d = data(1_200_000, 13);
            let mut location = String::new();
            let mut on_location = |l: &str| location = l.to_string();
            let s0 = e.control("/stats").await;
            e.control("/fault?mode=cut_after&bytes=500000").await;
            let mut once = e.cfg(&e.a.get("url"), e.password());
            once.retry = RetryPolicy::none();
            let first = match Client::connect(once).await {
                Ok(c1) => c1
                    .upload(
                        &d,
                        UploadOptions {
                            name: Some("p13.bin".into()),
                            chunk_size: Some(256 * 1024),
                            on_location: Some(&mut on_location),
                            ..UploadOptions::default()
                        },
                    )
                    .await
                    .map(|_| ()),
                Err(x) => Err(x),
            };
            e.control("/fault?mode=none").await;
            let s1 = e.control("/stats").await;
            k.check(
                "the first attempt interrupted by a dropped connection",
                first.is_err() && s1["cuts"].as_u64() > s0["cuts"].as_u64(),
                (&first, &s1),
            );
            let offset = c.upload_offset(&location).await;
            k.check(
                "the node kept a part",
                offset.as_ref().is_ok_and(|o| *o > 0 && *o < d.len() as u64),
                (&location, offset.as_ref().err(), offset.as_ref().ok()),
            );
            let mut seen = 0;
            let mut progress = |n: u64, _: u64| seen = n;
            let up = c
                .upload(
                    &d,
                    UploadOptions {
                        resume: Some(location.clone()),
                        chunk_size: Some(256 * 1024),
                        on_progress: Some(&mut progress),
                        ..UploadOptions::default()
                    },
                )
                .await;
            k.check(
                "resumed from its location, completed and verified by SHA-256",
                up.as_ref().is_ok_and(|(r, _)| r.size == d.len() as u64) && seen == d.len() as u64,
                up.as_ref().err(),
            );
            if let Ok((reference, _)) = up {
                let (oid, slug) = e.oid();
                let r = c.commit(json!([{"op": "create", "class": "Document", "object_id": oid, "slug": slug, "body": {"title": "p13", "passport": reference}}]), &none).await;
                k.check("referred to by a blob field", r.is_ok(), r.err());
                let doc = e.uri("Document", &slug);
                let whole = c.download(&doc, "passport", None, None).await;
                k.check(
                    "the whole file downloaded and verified",
                    whole.as_ref().is_ok_and(|b| *b == d),
                    whole.as_ref().err(),
                );
                let part = c
                    .download(&doc, "passport", Some((100_000, 300_000)), None)
                    .await;
                k.check(
                    "a range",
                    part.as_ref().is_ok_and(|b| b[..] == d[100_000..300_000]),
                    part.as_ref().err(),
                );
            }
        }
        "P14" => {
            let big = "12345678901234567890.1234567891";
            let (oid, slug) = e.oid();
            let r = c.commit(json!([{"op": "create", "class": "Account", "object_id": oid, "slug": slug, "body": {"name": "p14", "balance": Decimal::new(big).unwrap()}}]), &none).await;
            k.check("a 30-digit decimal committed", r.is_ok(), r.as_ref().err());
            let back = c
                .read(&e.uri("Account", &slug), &Selectors::default())
                .await;
            k.check(
                "read back exactly",
                back.as_ref().is_ok_and(|i| i.body()["balance"] == big),
                back.as_ref().map(|i| i.body()["balance"].clone()),
            );
            let m = Money::new("1250.50", "USD");
            k.check(
                "money in canonical form",
                m.as_ref().is_ok_and(|m| {
                    serde_json::to_value(m).ok()
                        == Some(json!({"amount": "1250.5", "currency": "USD"}))
                }),
                &m,
            );
            k.check(
                "negative zero is zero",
                Decimal::new("-0.000").map(|d| d.to_string()).ok() == Some("0".into()),
                "",
            );
            k.check("an exponent is refused", Decimal::new("1e3").is_err(), "");
            let f = ubos_sdk::decimal::reject_floats(&json!({"a": [1, {"b": 0.1}]}));
            k.check(
                "a float refused with its path",
                f.as_ref()
                    .err()
                    .is_some_and(|x| x.details["path"] == "a.1.b"),
                f.err(),
            );
        }
        "P15" => {
            let Ok((_, cslug)) = e
                .customer(
                    c,
                    "p15 customer",
                    json!({"credit_limit": Money::new("10", "USD").unwrap()}),
                )
                .await
            else {
                k.check("setup", false, "");
                return k;
            };
            let (sid, sslug) = e.oid();
            let r = c.commit(json!([{"op": "create", "class": "Supplier", "object_id": sid, "slug": sslug, "body": {"name": "p15 supplier", "country": e.tag, "iban": "DE00"}}]), &none).await;
            k.check("a supplier", r.is_ok(), r.err());
            let got = types::Customer::get(c, &cslug, &Selectors::default()).await;
            k.check("a typed read: the generated struct with its money", matches!(&got, Ok(types::CustomerAny::Customer(t)) if t.body.credit_limit.as_ref().is_some_and(|m| Decimal::new(&m.amount).ok() == Decimal::new("10").ok())), &got);
            let q = Counterparty::query()
                .filter("country", Cmp::Eq, e.tag.as_str())
                .order_by("name", false);
            k.check("the builder compiles to DAT-Query",
                q.to_query() == json!({"from": "Counterparty", "where": format!("this.country == \"{}\"", e.tag), "order_by": [{"path": "this.name", "dir": "asc"}]}), q.to_query());
            let all = q
                .all(c, Selectors::default(), CounterpartyAny::from_item)
                .await;
            let text: Vec<_> = c
                .query(
                    json!(format!(
                        "from Counterparty where this.country == \"{}\" order by this.name",
                        e.tag
                    )),
                    Selectors::default(),
                    100,
                )
                .collect()
                .await;
            let ids = |v: Vec<String>| v;
            let typed_ids = all
                .as_ref()
                .map(|v| ids(v.iter().map(|x| x.item().object_id().to_string()).collect()))
                .unwrap_or_default();
            let text_ids: Vec<String> = text
                .iter()
                .filter_map(|x| x.as_ref().ok())
                .map(|i| i.object_id().to_string())
                .collect();
            k.check(
                "the same results as the text query",
                !typed_ids.is_empty() && typed_ids == text_ids,
                (&typed_ids, &text_ids),
            );
            let classes: Vec<&str> = all
                .as_ref()
                .map(|v| {
                    v.iter()
                        .map(|x| match x {
                            CounterpartyAny::Customer(_) => "Customer",
                            CounterpartyAny::Supplier(_) => "Supplier",
                            CounterpartyAny::Counterparty(_) => "Counterparty",
                        })
                        .collect()
                })
                .unwrap_or_default();
            k.check(
                "polymorphic results as the subclass variants",
                classes.contains(&"Customer") && classes.contains(&"Supplier"),
                &classes,
            );
            let iban = all.as_ref().ok().and_then(|v| {
                v.iter().find_map(|x| match x {
                    CounterpartyAny::Supplier(t) => t.body.iban.clone(),
                    _ => None,
                })
            });
            k.check(
                "a variant's own fields",
                iban.as_deref() == Some("DE00"),
                &iban,
            );
            if let Ok((_, aslug)) = e.account().await {
                let o = CallOptions {
                    branch: Some("feature/sdk".into()),
                    ..CallOptions::default()
                };
                let r =
                    types::account::rename(c, &e.uri("Account", &aslug), "p15 renamed".into(), &o)
                        .await;
                k.check(
                    "a typed port call",
                    r.as_ref().is_ok_and(|m| m.result() == "p15 renamed"),
                    r.as_ref().err(),
                );
            }
        }
        // ---- versioning and introspection (BPA 1.0-rc4, step 18c) ----
        "P16" => {
            let (a, b) = (
                format!("feature/{}-p16a", e.tag),
                format!("feature/{}-p16b", e.tag),
            );
            let on = |br: &str| CallOptions {
                branch: Some(br.to_string()),
                ..CallOptions::default()
            };
            let r = c.create_branch(&a, "main", &none).await;
            k.check(
                "a branch from main",
                r.as_ref().is_ok_and(|m| m.result()["branch"] == json!(a)),
                r.as_ref().map(|m| m.0.clone()),
            );
            let Ok((oid, slug)) = e.customer(c, "p16", json!({})).await else {
                k.check("setup", false, "");
                return k;
            };
            let a2 = format!("{a}x");
            let _ = c.create_branch(&a2, "main", &none).await;
            let r = c.create_branch(&b, &a2, &none).await;
            k.check("a branch from a branch", r.is_ok(), r.err());
            let item = c
                .read(&e.uri("Customer", &slug), &Selectors::branch(&b))
                .await;
            let base = item
                .as_ref()
                .map(|i| i.version().to_string())
                .unwrap_or_default();
            let w = c
                .commit(json!([{"op": "update", "object_id": oid, "base": base, "body": {"name": "p16 on b"}}]), &on(&b))
                .await;
            k.check("a commit on the source branch", w.is_ok(), w.as_ref().err());
            let p = c.merge_preview(&b, &Selectors::branch(&a2)).await;
            k.check(
                "the merge preview lists the change and no conflict",
                p.as_ref().is_ok_and(|v| {
                    v["changes"].as_array().is_some_and(|x| !x.is_empty())
                        && v["conflicts"].as_array().is_some_and(Vec::is_empty)
                }),
                &p,
            );
            let m = c.merge(&b, None, &on(&a2)).await;
            k.check(
                "the merge: one process, its commit and the version written",
                m.as_ref().is_ok_and(|m| {
                    !m.process_id().is_empty()
                        && m.commits().as_array().is_some_and(|x| !x.is_empty())
                        && m.versions().get(&oid).is_some()
                }),
                m.as_ref().map(|m| m.0.clone()),
            );
            let after = c
                .read(&e.uri("Customer", &slug), &Selectors::branch(&a2))
                .await;
            k.check(
                "the target reads the merged version",
                after.as_ref().is_ok_and(|i| i.body()["name"] == "p16 on b"),
                after.as_ref().map(|i| i.0.clone()),
            );
            // both branches change the same field: the merge is a typed conflict
            let base = after
                .as_ref()
                .map(|i| i.version().to_string())
                .unwrap_or_default();
            let _ = c
                .commit(json!([{"op": "update", "object_id": oid, "base": base, "body": {"name": "p16 on a"}}]), &on(&a2))
                .await;
            let item = c
                .read(&e.uri("Customer", &slug), &Selectors::branch(&b))
                .await;
            let base = item
                .as_ref()
                .map(|i| i.version().to_string())
                .unwrap_or_default();
            let _ = c
                .commit(json!([{"op": "update", "object_id": oid, "base": base, "body": {"name": "p16 on b again"}}]), &on(&b))
                .await;
            let x = c.merge(&b, None, &on(&a2)).await;
            k.check(
                "a conflicting merge is VER.MERGE_CONFLICT, not retried",
                x.as_ref()
                    .err()
                    .is_some_and(|x| x.code_text == "VER.MERGE_CONFLICT" && !x.retryable()),
                x.as_ref().map(|m| m.0.clone()),
            );
        }
        "P17" => {
            let (t, s) = (
                format!("feature/{}-p17t", e.tag),
                format!("feature/{}-p17s", e.tag),
            );
            let on = CallOptions {
                branch: Some(t.clone()),
                ..CallOptions::default()
            };
            let _ = c.create_branch(&t, "main", &none).await;
            let _ = c.create_branch(&s, &t, &none).await;
            let (oid, slug) = e.oid();
            let w = c
                .commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "p17", "country": e.tag}}]),
                    &CallOptions { branch: Some(s.clone()), ..CallOptions::default() })
                .await;
            k.check("a commit on the source", w.is_ok(), w.err());
            let id = ubos_sdk::util::uuid7();
            let state = |r: &ubos_sdk::Result<ubos_sdk::MutateResult>| {
                r.as_ref()
                    .map(|m| m.result()["state"].clone())
                    .unwrap_or(Value::Null)
            };
            let r = c.open_change_set(&id, &s, "p17", &on).await;
            k.check("open", state(&r) == "open", r.as_ref().map(|m| m.0.clone()));
            let r = c.change_set(&id, ChangeSetAction::Submit, None, &on).await;
            k.check(
                "submit: the preview is recorded",
                state(&r) == "submitted"
                    && r.as_ref().is_ok_and(|m| m.result()["preview"].is_string()),
                r.as_ref().map(|m| m.0.clone()),
            );
            let r = c.change_set(&id, ChangeSetAction::Approve, None, &on).await;
            let me = c.session()["principal"]["did"].clone();
            k.check(
                "approve: the approver is the session's principal",
                state(&r) == "approved"
                    && r.as_ref()
                        .is_ok_and(|m| m.result()["approvals"][0]["approver"] == me),
                (r.as_ref().map(|m| m.0.clone()), &me),
            );
            let r = c.change_set(&id, ChangeSetAction::Merge, None, &on).await;
            k.check(
                "merge: merged with its commit",
                state(&r) == "merged"
                    && r.as_ref()
                        .is_ok_and(|m| m.result()["merged_commit"].is_string()),
                r.as_ref().map(|m| m.0.clone()),
            );
            let got = c
                .read(&e.uri("Customer", &slug), &Selectors::branch(&t))
                .await;
            k.check("the target reads the change", got.is_ok(), got.err());
            let r = c.change_set(&id, ChangeSetAction::Submit, None, &on).await;
            k.check(
                "an action out of state is FLOW.TRANSITION_NOT_ALLOWED",
                r.as_ref()
                    .err()
                    .is_some_and(|x| x.code_text == "FLOW.TRANSITION_NOT_ALLOWED"),
                r.as_ref().map(|m| m.0.clone()),
            );
        }
        "P18" => {
            let br = format!("feature/{}-p18", e.tag);
            let on = CallOptions {
                branch: Some(br.clone()),
                ..CallOptions::default()
            };
            let _ = c.create_branch(&br, "main", &none).await;
            let (oid, slug) = e.oid();
            let w = c
                .commit(json!([{"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": "p18", "country": e.tag}}]), &on)
                .await;
            let pid = w
                .as_ref()
                .map(|m| m.process_id().to_string())
                .unwrap_or_default();
            let dry = c
                .revert(json!({"process_id": pid, "dry_run": true}), &on)
                .await;
            k.check(
                "a dry run names the commit and commits nothing",
                dry.as_ref().is_ok_and(|m| {
                    m.result()["reverted"]
                        .as_array()
                        .is_some_and(|x| x.len() == 1)
                        && m.commits().as_array().is_some_and(Vec::is_empty)
                }),
                dry.as_ref().map(|m| m.0.clone()),
            );
            let r = c.revert(json!({"process_id": pid}), &on).await;
            k.check(
                "revert: one process and one commit",
                r.as_ref().is_ok_and(|m| {
                    !m.process_id().is_empty()
                        && m.commits().as_array().is_some_and(|x| x.len() == 1)
                }),
                r.as_ref().map(|m| m.0.clone()),
            );
            let gone = c
                .read(&e.uri("Customer", &slug), &Selectors::branch(&br))
                .await;
            k.check(
                "the reverted object no longer reads at the head",
                gone.as_ref().err().is_some_and(|x| {
                    x.code_text == "QRY.NOT_FOUND_AT_TIME" || x.code_text == "IAM.NOT_FOUND"
                }),
                gone.as_ref().map(|i| i.0.clone()),
            );
            let changes: Vec<Value> = (0..3)
                .map(|i| {
                    let (oid, slug) = e.oid();
                    json!({"op": "create", "class": "Customer", "object_id": oid, "slug": slug, "body": {"name": format!("p18 bulk {i}"), "country": e.tag}})
                })
                .collect();
            let b = c
                .bulk(json!({"changes": changes, "max_changes": 1}), &on)
                .await;
            k.check(
                "a bulk process commits its chunks as children",
                b.as_ref().is_ok_and(|m| {
                    m.result()["state"] == "committed"
                        && m.result()["children"]
                            .as_array()
                            .is_some_and(|x| x.len() == 3)
                        && m.commits().as_array().is_some_and(|x| x.len() == 3)
                }),
                b.as_ref().map(|m| m.0.clone()),
            );
            let bid = b
                .as_ref()
                .map(|m| m.process_id().to_string())
                .unwrap_or_default();
            let rec = c.process(&bid, false, false).await;
            k.check(
                "the bulk process record counts its children",
                rec.as_ref().is_ok_and(|v| v["children"] == 3),
                &rec,
            );
        }
        "P19" => {
            let sdk = CallOptions {
                branch: Some("feature/sdk".into()),
                ..CallOptions::default()
            };
            let sel = Selectors::branch("feature/sdk");
            let cls = c.effective_class("Customer", &sel).await;
            k.check(
                "the effective class with its linearization",
                cls.as_ref()
                    .is_ok_and(|v| v["linearization"][0] == "Customer"),
                &cls,
            );
            let d = c.dispatch("Account", "rename", &sel).await;
            k.check(
                "dispatch names the implementing class",
                d.as_ref()
                    .is_ok_and(|v| v["class"] == "Account" && v["impl"].is_string()),
                &d,
            );
            let ci = c.commit_info(&sel).await;
            k.check(
                "the head commit with its state root",
                ci.as_ref().is_ok_and(|v| {
                    v["branch"] == "feature/sdk"
                        && v["root"].is_string()
                        && v["commit_id"].is_string()
                }),
                &ci,
            );
            let l = c.ledger("Account", json!({"account": "none"}), &sel).await;
            k.check(
                "a ledger key without entries is empty",
                l.as_ref().is_ok_and(Vec::is_empty),
                &l,
            );
            let Ok((_, aslug)) = e.account().await else {
                k.check("setup", false, "");
                return k;
            };
            let r = c
                .invoke(
                    &e.uri("Account", &aslug),
                    "rename",
                    json!({"name": "p19"}),
                    &sdk,
                )
                .await;
            let pid = r
                .as_ref()
                .map(|m| m.process_id().to_string())
                .unwrap_or_default();
            let rec = c.process(&pid, true, true).await;
            k.check(
                "the process record with its journal and metering",
                rec.as_ref().is_ok_and(|v| {
                    v["state"] == "committed"
                        && v["journal"]["context"].is_object()
                        && v["metering"]["units"].is_u64()
                }),
                &rec,
            );
            let rp = c.replay(&pid).await;
            k.check(
                "the replay is identical",
                rp.as_ref().is_ok_and(|v| v["outcome"] == "identical"),
                &rp,
            );
            // BPA 1.0-rc5: a failed port call names its process
            let bad = c
                .invoke(&e.uri("Account", &aslug), "rename", json!({}), &sdk)
                .await;
            let failed_pid = bad.as_ref().err().and_then(|x| x.process_id.clone());
            let failed = match &failed_pid {
                Some(p) => c.process(p, false, true).await.ok(),
                None => None,
            };
            k.check(
                "a failed port call names its process, whose record is failed",
                failed
                    .as_ref()
                    .is_some_and(|v| v["state"] == "failed" && v["metering"].is_object()),
                (&bad.as_ref().err(), &failed),
            );
            let none_pid = c.process(&ubos_sdk::util::uuid7(), false, false).await;
            k.check(
                "an unknown process is IAM.NOT_FOUND",
                none_pid
                    .as_ref()
                    .err()
                    .is_some_and(|x| x.code_text == "IAM.NOT_FOUND"),
                &none_pid,
            );
        }
        // ---- SUITE-SDK-NET ----
        "N01" => {
            let r0 = c
                .query_page(json!("from Customer"), &Selectors::default(), 1)
                .await;
            k.check("before", r0.is_ok(), r0.err());
            let s = e.control("/fault?mode=drop").await;
            k.check("the proxy dropped the connections", !s.is_null(), &s);
            let r1 = c
                .query_page(json!("from Customer"), &Selectors::default(), 1)
                .await;
            k.check("the next request reconnects", r1.is_ok(), r1.err());
        }
        "N02" => {
            let s0 = e.control("/stats").await;
            e.control("/fault?mode=cut_reply").await;
            let r = c
                .query_page(json!("from Customer"), &Selectors::default(), 5)
                .await;
            let s1 = e.control("/stats").await;
            k.check(
                "the reply was cut",
                s1["cuts"].as_u64() > s0["cuts"].as_u64(),
                (&s0, &s1),
            );
            k.check("the read retried and answered", r.is_ok(), r.err());
        }
        "N03" => {
            e.control("/fault?mode=latency&ms=300").await;
            let t0 = Instant::now();
            let r = c
                .query_page(json!("from Customer"), &Selectors::default(), 1)
                .await;
            let read_ms = t0.elapsed().as_millis();
            let w = e.customer(c, "n03", json!({})).await;
            e.control("/fault?mode=none").await;
            k.check("a read completes", r.is_ok(), r.err());
            k.check("a commit completes", w.is_ok(), w.err());
            k.check("the latency was applied", read_ms >= 550, read_ms);
        }
        "N04" => {
            e.control("/fault?mode=partition&ms=3000").await;
            let t0 = Instant::now();
            let r = c
                .query_page(json!("from Customer"), &Selectors::default(), 1)
                .await;
            let ms = t0.elapsed().as_millis();
            k.check(
                "the request completed after the partition healed",
                r.is_ok() && ms >= 2500,
                (r.err(), ms),
            );
        }
        "N05" => {
            let mut cfg = e.cfg(&e.a.get("url"), e.password());
            cfg.request_timeout = Duration::from_millis(1500);
            cfg.retry.deadline = Duration::from_secs(4);
            let Ok(short) = Client::connect(cfg).await else {
                k.check("setup", false, "");
                return k;
            };
            e.control("/fault?mode=partition&ms=9000").await;
            let t0 = Instant::now();
            let r = short
                .query_page(json!("from Customer"), &Selectors::default(), 1)
                .await;
            let ms = t0.elapsed().as_millis();
            e.control("/fault?mode=none").await;
            k.check(
                "a transport error, retryable",
                r.as_ref()
                    .err()
                    .is_some_and(|x| x.is_transport() && x.retryable()),
                r.as_ref().err(),
            );
            k.check("within the deadline and one request timeout", ms < 6000, ms);
            let again = c
                .query_page(json!("from Customer"), &Selectors::default(), 1)
                .await;
            k.check(
                "served again after the partition",
                again.is_ok(),
                again.err(),
            );
        }
        "N06" => {
            let Ok((oid, slug)) = e.customer(c, "n06 0", json!({})).await else {
                k.check("setup", false, "");
                return k;
            };
            let Ok(mut sub) = c
                .subscribe(
                    Target::Object(e.uri("Customer", &slug)),
                    &Selectors::default(),
                    None,
                )
                .await
            else {
                k.check("subscribed", false, "");
                return k;
            };
            let mut want = std::collections::BTreeSet::new();
            for i in 1..=9 {
                match e
                    .rename(&e.direct, &oid, &slug, &format!("n06 {i}"), &none)
                    .await
                {
                    Ok(m) => {
                        if let Some(q) = m.result()["cseq"].as_u64() {
                            want.insert(q);
                        }
                    }
                    Err(x) => k.check(&format!("update {i}"), false, x),
                }
                if i % 3 == 0 {
                    // the subscriber makes no request: the SDK notices the dropped stream,
                    // reconnects and resubscribes from its cursor by itself
                    e.control("/fault?mode=drop").await;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            let mut seen = std::collections::BTreeSet::new();
            let until = Instant::now() + Duration::from_secs(20);
            while !want.is_subset(&seen) && Instant::now() < until {
                match sub.next_within(Duration::from_secs(5)).await {
                    Some(Update::Emit(b)) => {
                        for ch in b["changes"].as_array().into_iter().flatten() {
                            if ch["object_id"] == oid.as_str()
                                && let Some(q) = ch["cseq"].as_u64()
                            {
                                seen.insert(q);
                            }
                        }
                    }
                    Some(Update::Reset(x)) => {
                        k.check("no reset", false, x);
                        break;
                    }
                    None => {}
                }
            }
            k.check("all 9 updates arrived across 3 dropped connections, without a request of the subscriber", want.len() == 9 && want.is_subset(&seen), (&want, &seen));
            let _ = sub.close().await;
        }
        "N07" => {
            let Ok((oid, slug)) = e.customer(c, "n07", json!({})).await else {
                k.check("setup", false, "");
                return k;
            };
            let Ok(mut sub) = c
                .subscribe(
                    Target::Object(e.uri("Customer", &slug)),
                    &Selectors::default(),
                    None,
                )
                .await
            else {
                k.check("subscribed", false, "");
                return k;
            };
            let s = e.control("/restart").await;
            k.check("the node restarted", s["restarted"] == true, &s);
            let Ok(direct) = Client::connect(e.cfg(&e.a.get("direct"), e.password())).await else {
                k.check("direct client", false, "");
                return k;
            };
            let r = e.rename(c, &oid, &slug, "n07 after", &none).await;
            k.check(
                "the client reconnected and signed in again",
                r.is_ok(),
                r.as_ref().err(),
            );
            let other = e
                .rename(&direct, &oid, &slug, "n07 other", &none)
                .await
                .ok()
                .and_then(|m| m.result()["cseq"].as_u64());
            let mut got = false;
            let until = Instant::now() + Duration::from_secs(15);
            while !got && Instant::now() < until {
                match sub.next_within(Duration::from_secs(15)).await {
                    Some(Update::Emit(b)) => {
                        got = b["changes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|ch| ch["cseq"].as_u64() == other && other.is_some())
                    }
                    _ => break,
                }
            }
            k.check(
                "the subscription resumed: another client's change arrived",
                got,
                other,
            );
            let _ = sub.close().await;
        }
        "N08" => {
            let d = data(1_500_000, 8);
            let s0 = e.control("/stats").await;
            e.control("/fault?mode=cut_after&bytes=600000").await;
            let up = c
                .upload(
                    &d,
                    UploadOptions {
                        chunk_size: Some(256 * 1024),
                        ..UploadOptions::default()
                    },
                )
                .await;
            e.control("/fault?mode=none").await;
            let s1 = e.control("/stats").await;
            k.check(
                "a connection dropped during the upload",
                s1["cuts"].as_u64() > s0["cuts"].as_u64(),
                (&s0, &s1),
            );
            k.check(
                "the upload resumed and completed, verified by SHA-256",
                up.as_ref().is_ok_and(|(r, _)| r.size == d.len() as u64),
                up.as_ref().err(),
            );
        }
        other => k.check("known case", false, other),
    }
    k
}

#[tokio::main]
async fn main() {
    let mut m = HashMap::new();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    for w in argv.chunks(2) {
        if let [k, v] = w {
            m.insert(k.trim_start_matches("--").to_string(), v.clone());
        }
    }
    let a = Args(m);
    let vae = a.get("vae");
    let binding = if a.get("binding") == "http" {
        Binding::Http
    } else {
        Binding::WebSocket
    };
    let auth = Auth::Password {
        username: a.get("user"),
        password: a.get("password"),
    };
    let c = Client::connect(Config::new(&a.get("url"), &vae, auth.clone()).binding(binding)).await;
    let direct = Client::connect(Config::new(&a.get("direct"), &vae, auth).binding(binding)).await;
    let (Ok(c), Ok(direct)) = (c, direct) else {
        println!(
            "{}",
            json!({"case": "setup", "ok": false, "checks": [{"name": "connect", "ok": false, "detail": "no connection"}]})
        );
        std::process::exit(1);
    };
    let tag = a.get("tag");
    let e = Env {
        a,
        c,
        direct,
        tag,
        vae,
        next: std::sync::atomic::AtomicU64::new(0),
    };
    let cases: Vec<String> = match e.a.0.get("cases") {
        Some(l) => l.split(',').map(str::to_string).collect(),
        None => [
            (1..=19).map(|i| format!("P{i:02}")).collect::<Vec<_>>(),
            (1..=8).map(|i| format!("N{i:02}")).collect(),
        ]
        .concat(),
    };
    let mut failed = 0;
    for id in cases {
        let t0 = Instant::now();
        let k = run(&e, &id).await;
        failed += usize::from(!k.ok());
        println!(
            "{}",
            json!({"case": id, "ok": k.ok(), "checks": k.checks, "ms": t0.elapsed().as_millis() as u64})
        );
    }
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
