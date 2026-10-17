//! A tour of the SDK against a running node: connection, typed errors, mutations with
//! idempotency keys, exact decimals, paged queries, subscriptions that survive a restart
//! of the node, and files.
//!
//! ```text
//! cargo run -p ubos-sdk --example tour -- URL VAE USER PASSWORD (ws|http) [--restart]
//! ```
//!
//! The VAE needs the classes `Customer` (`name`, `email`, `credit_limit` money) and
//! `Document` (`title`, `passport` blob). With `--restart` the tour prints `RESTART` and
//! waits for a line on standard input while the node is restarted, then checks that the
//! client reconnected and the subscription resumed.

use futures_util::StreamExt;
use serde_json::json;
use std::time::Duration;
use ubos_sdk::{
    Auth, Binding, CallOptions, Client, Code, Config, Selectors, Target, TraceContext, Update,
    UploadOptions,
};

struct Tally(u32, u32);

impl Tally {
    fn check(&mut self, name: &str, ok: bool, detail: impl std::fmt::Debug) {
        if ok {
            self.0 += 1;
            println!("ok   {name}");
        } else {
            self.1 += 1;
            println!("FAIL {name}: {detail:?}");
        }
    }
}

/// An update of `name`: raw updates name the version they replace (STD-TXN-020).
async fn rename(
    c: &Client,
    vae: &str,
    slug: &str,
    oid: &str,
    name: &str,
    o: &CallOptions,
) -> ubos_sdk::Result<ubos_sdk::MutateResult> {
    let item = c
        .read(
            &format!("ubos://{vae}/Customer/{slug}"),
            &Selectors::default(),
        )
        .await?;
    c.commit(
        json!([{"op": "update", "object_id": oid, "base": item.version(), "set": {"name": name}}]),
        o,
    )
    .await
}

fn oid(n: u64) -> String {
    format!("01890a5d-ac96-7001-8000-{:012x}", 0x5d_0000 + n)
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 5 {
        eprintln!("tour URL VAE USER PASSWORD (ws|http) [--restart]");
        std::process::exit(2);
    }
    let binding = if a[4] == "http" {
        Binding::Http
    } else {
        Binding::WebSocket
    };
    let restart = a.iter().any(|x| x == "--restart");
    let mut t = Tally(0, 0);
    let (vae, tag) = (a[1].clone(), &a[4]);
    let auth = Auth::Password {
        username: a[2].clone(),
        password: a[3].clone(),
    };
    // a wrong password: a typed, non-retryable error
    let bad = Client::connect(
        Config::new(
            &a[0],
            &vae,
            Auth::Password {
                username: a[2].clone(),
                password: "wrong".into(),
            },
        )
        .binding(binding),
    )
    .await;
    t.check(
        "a wrong password is a typed IAM error",
        bad.as_ref()
            .err()
            .is_some_and(|e| e.code_text.starts_with("IAM.") && !e.retryable()),
        bad.err(),
    );
    let c = match Client::connect(Config::new(&a[0], &vae, auth).binding(binding)).await {
        Ok(c) => c,
        Err(e) => {
            println!("FAIL connect: {e}");
            std::process::exit(1);
        }
    };
    t.check(
        "connected, protocol 1.0 negotiated",
        c.version() == "1.0",
        c.session(),
    );
    let base = std::process::id() as u64 * 1000 + if tag == "http" { 500 } else { 0 };
    // mutations: raw commit, exact decimals, floats refused
    let changes: Vec<_> = (0..25)
        .map(|i| json!({"op": "create", "class": "Customer", "object_id": oid(base + i), "slug": format!("t{}", base + i),
            "body": {"name": format!("tour {i}"), "email": format!("c{i}@example.com"), "credit_limit": ubos_sdk::Money::new("1250.50", "USD").unwrap()}}))
        .collect();
    let r = c.commit(json!(changes), &CallOptions::default()).await;
    t.check("25 objects committed", r.is_ok(), r.as_ref().err());
    let f = c
        .commit(json!([{"op": "create", "class": "Customer", "object_id": oid(base + 99), "slug": "float", "body": {"name": "x", "credit_limit": {"amount": 12.5, "currency": "USD"}}}]), &CallOptions::default())
        .await;
    t.check(
        "a float refused at the API boundary (LOGIC.FLOAT_TO_DECIMAL)",
        f.as_ref()
            .err()
            .is_some_and(|e| e.is(Code::LogicFloatToDecimal)),
        f.err(),
    );
    // idempotency: the same key returns the first result
    let key = ubos_sdk::util::uuid7();
    let o = CallOptions {
        idempotency_key: Some(key.clone()),
        trace: Some(TraceContext::new()),
        ..CallOptions::default()
    };
    let item0 = c
        .read(
            &format!("ubos://{vae}/Customer/t{base}"),
            &Selectors::default(),
        )
        .await;
    let v0 = item0
        .as_ref()
        .map(|i| i.version().to_string())
        .unwrap_or_default();
    let ch = json!([{"op": "update", "object_id": oid(base), "base": v0, "set": {"name": "tour 0 renamed"}}]);
    let r1 = c.commit(ch.clone(), &o).await;
    let r2 = c.commit(ch, &o).await;
    t.check("the same idempotency key returns the first result", matches!((&r1, &r2), (Ok(x), Ok(y)) if x.process_id() == y.process_id() && !x.process_id().is_empty()), (r1.as_ref().err(), r2.as_ref().err()));
    // reads with selectors and evidence
    let item = c
        .read(
            &format!("ubos://{vae}/Customer/t{base}"),
            &Selectors::branch("main"),
        )
        .await;
    t.check(
        "a read with its version and evidence",
        item.as_ref().is_ok_and(|i| {
            i.version().starts_with("sha256:")
                && !i.evaluated_at().is_null()
                && i.body()["credit_limit"]["amount"] == "1250.50"
        }),
        item.as_ref().map(|i| i.0.clone()),
    );
    let missing = c
        .read(
            &format!("ubos://{vae}/Customer/no-such-{base}"),
            &Selectors::default(),
        )
        .await;
    t.check(
        "a missing object is a typed error, not retried",
        missing
            .as_ref()
            .err()
            .is_some_and(|e| e.code.is_some() && !e.retryable()),
        missing.err(),
    );
    // paging through snapshot cursors
    let q = json!(format!("from Customer where this.name != \"\" "));
    let items: Vec<_> = c.query(q.clone(), Selectors::default(), 10).collect().await;
    let ok = items.iter().all(|i| i.is_ok());
    let n = items.len();
    let snap: Vec<_> = items
        .iter()
        .filter_map(|i| i.as_ref().ok())
        .map(|i| i.evaluated_at().clone())
        .collect();
    t.check("a query streamed in pages of 10", ok && n >= 25, (ok, n));
    t.check(
        "every page of the same snapshot",
        snap.windows(2).all(|w| w[0] == w[1]),
        snap.first(),
    );
    // a subscription
    let target = format!("ubos://{vae}/Customer/t{base}");
    let mut sub = match c
        .subscribe(Target::Object(target.clone()), &Selectors::default(), None)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            println!("FAIL subscribe: {e}");
            std::process::exit(1);
        }
    };
    let r = rename(
        &c,
        &vae,
        &format!("t{base}"),
        &oid(base),
        "tour 0 again",
        &CallOptions::default(),
    )
    .await;
    t.check("an update with its base", r.is_ok(), r.err());
    let u = sub.next_within(Duration::from_secs(10)).await;
    t.check("the change emitted to the subscription", matches!(&u, Some(Update::Emit(b)) if b["changes"].as_array().is_some_and(|a| !a.is_empty())), &u);
    // files
    let data: Vec<u8> = (0..2_000_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let mut seen = 0;
    let mut progress = |n: u64, _total: u64| seen = n;
    let up = c
        .upload(
            &data,
            UploadOptions {
                name: Some("scan.bin".into()),
                media_type: Some("application/octet-stream".into()),
                chunk_size: Some(512 * 1024),
                on_progress: Some(&mut progress),
                resume: None,
            },
        )
        .await;
    t.check(
        "an upload in chunks, verified by SHA-256",
        up.as_ref().is_ok_and(|(r, _)| r.size == data.len() as u64) && seen == data.len() as u64,
        up.as_ref().err(),
    );
    if let Ok((reference, _)) = &up {
        let slug = format!("doc{base}");
        let r = c
            .commit(json!([{"op": "create", "class": "Document", "object_id": oid(base + 50), "slug": slug, "body": {"title": "tour", "passport": reference}}]), &CallOptions::default())
            .await;
        t.check("the file referred to by a blob field", r.is_ok(), r.err());
        let doc = format!("ubos://{vae}/Document/{slug}");
        let whole = c.download(&doc, "passport", None, None).await;
        t.check(
            "the whole file downloaded and verified",
            whole.as_ref().is_ok_and(|b| *b == data),
            whole.as_ref().err(),
        );
        let part = c.download(&doc, "passport", Some((1000, 5000)), None).await;
        t.check(
            "a range downloaded",
            part.as_ref().is_ok_and(|b| b[..] == data[1000..5000]),
            part.as_ref().err(),
        );
    }
    if restart {
        println!("RESTART");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        // the next request reconnects, signs in again and resubscribes
        let r = rename(
            &c,
            &vae,
            &format!("t{base}"),
            &oid(base),
            "after restart",
            &CallOptions::default(),
        )
        .await;
        t.check(
            "the client reconnected after the node restarted",
            r.is_ok(),
            r.as_ref().err(),
        );
        let u = sub.next_within(Duration::from_secs(15)).await;
        t.check(
            "the subscription resumed: the change after the restart arrived",
            matches!(&u, Some(Update::Emit(_))),
            &u,
        );
    }
    let cursor = sub.cursor();
    t.check("the subscription's cursor", cursor.is_some(), &cursor);
    let closed = sub.close().await;
    t.check("unsubscribed", closed.is_ok(), closed.err());
    println!("\n{tag}: {} ok, {} failed", t.0, t.1);
    std::process::exit(if t.1 == 0 { 0 } else { 1 });
}
