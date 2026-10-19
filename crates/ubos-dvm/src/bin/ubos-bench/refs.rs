//! `ubos-bench refs`: commits on a branch of a model with relationships (PH-2 step 0b-1b).
//!
//! The dataset is `Customer`, `SalesOrder` with a `many_to_one` reference to its customer
//! (`on_end` restrict) and `Contact` with a `one_to_one` reference to a customer, created
//! in commits of `--per-commit` objects: a third of `--objects` are customers, the rest
//! orders; one customer in a hundred has a contact; the last `2 * (--rounds + --full-rounds)`
//! customers have no orders. The suite measures the commit latency of:
//!
//! - `create_order`: an order of a random customer (reference integrity);
//! - `update_order`: a change of an order's amount;
//! - `create_contact`: a contact of a customer without one (one-to-one cardinality);
//! - `end_free`: ending a customer without orders (the `on_end` rule finds no source);
//! - `end_referenced`: ending a customer with orders, rejected `MODEL.REFERENCE_RESTRICTED`.
//!
//! Each runs `--rounds` times on the reference rows of the field index (`indexed`) and
//! `--full-rounds` times with the checks over the whole branch state (`full`, the path
//! before step 0b-1b, forced through `index_since`).

use std::collections::BTreeMap;
use std::time::Instant;

use serde_json::{Value, json};
use ubos_dvm::Engine;
use ubos_store::{MemStore, PgStore};
use ubos_types::BpaError;

use crate::run::summary;

const BRANCH: &str = "feature/refs";

fn opt<T: std::str::FromStr>(args: &[String], k: &str, d: T) -> T {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn text(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn uuid(i: u64) -> String {
    format!("01890a5d-ac96-7000-8000-{i:012x}")
}

fn classes() -> Value {
    let f = |n: &str, t: &str, required: bool| json!({"name": n, "type": t, "label": n, "description": n, "required": required});
    let r = |n: &str, card: &str| {
        json!({"name": n, "type": "reference", "label": n, "description": n, "required": true,
        "constraints": {"target_class": "Customer"},
        "relationship": {"cardinality": card, "target": "Customer", "inverse": format!("{n}_of")}})
    };
    let class = |name: &str, fields: Value| {
        json!({"name": name, "namespace": "bench.refs", "kind": "definition",
        "label": name, "description": name, "parent": "DataObject", "schema_version": 1, "fields": fields})
    };
    json!([
        class("Customer", json!([f("name", "text", true)])),
        class(
            "SalesOrder",
            json!([r("customer", "many_to_one"), f("amount", "decimal", true)])
        ),
        class(
            "Contact",
            json!([r("account", "one_to_one"), f("email", "text", true)])
        ),
    ])
}

fn customer_uri(c: u64) -> String {
    format!("ubos://acme/Customer/c-{c}")
}

/// A deterministic pseudo-random stream (xorshift).
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

/// `ubos-bench refs`.
pub fn refs(args: &[String], url: &str) -> Result<Value, BpaError> {
    let objects: u64 = opt(args, "--objects", 30_000);
    let per_commit: u64 = opt(args, "--per-commit", 1_000);
    let rounds: u64 = opt(args, "--rounds", 50);
    let full_rounds: u64 = opt(args, "--full-rounds", 5);
    let backend = text(args, "--backend").unwrap_or_else(|| "memory".into());
    let mut e = match backend.as_str() {
        "postgres" => Engine::new(Box::new(PgStore::connect(url)?.with_fixed_scope()))?,
        _ => Engine::new(Box::new(MemStore::default()))?,
    };
    e.load_genesis(&classes(), None)?;
    e.other("create_branch", &json!({"name": BRANCH, "from": "main"}))?;
    // the last `2 * spare` customers have no orders: half get a contact, half are ended
    let spare = rounds + full_rounds;
    let customers = (objects / 3).max(spare * 4);
    let orders = objects.saturating_sub(customers);
    let busy = customers - 2 * spare;
    let t0 = Instant::now();
    let mut next = 0u64;
    let mut batch: Vec<Value> = Vec::new();
    let flush = |e: &mut Engine, batch: &mut Vec<Value>, force: bool| -> Result<(), BpaError> {
        if batch.len() as u64 >= per_commit || (force && !batch.is_empty()) {
            e.commit(&json!({"branch": BRANCH, "changes": std::mem::take(batch)}))?;
        }
        Ok(())
    };
    for c in 0..customers {
        batch.push(json!({"op": "create", "class": "Customer", "object_id": uuid(next), "slug": format!("c-{c}"),
            "body": {"name": format!("Customer {c}")}}));
        next += 1;
        flush(&mut e, &mut batch, false)?;
    }
    flush(&mut e, &mut batch, true)?;
    let mut contacts = 0u64;
    for c in (0..busy).step_by(100) {
        batch.push(json!({"op": "create", "class": "Contact", "object_id": uuid(next), "slug": format!("k-{c}"),
            "body": {"account": customer_uri(c), "email": format!("c{c}@example.com")}}));
        next += 1;
        contacts += 1;
        flush(&mut e, &mut batch, false)?;
    }
    flush(&mut e, &mut batch, true)?;
    let mut order_ids = Vec::new();
    for o in 0..orders {
        let id = uuid(next);
        batch.push(
            json!({"op": "create", "class": "SalesOrder", "object_id": id, "slug": format!("o-{o}"),
            "body": {"customer": customer_uri(o % busy), "amount": "10.00"}}),
        );
        order_ids.push(id);
        next += 1;
        flush(&mut e, &mut batch, false)?;
        if (o + 1) % 50_000 == 0 {
            eprintln!(
                "  loaded {} orders ({:.0} s)",
                o + 1,
                t0.elapsed().as_secs_f64()
            );
        }
    }
    flush(&mut e, &mut batch, true)?;
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let total = customers + contacts + orders;
    eprintln!("loaded {total} objects in {:.0} s", load_ms / 1000.0);

    let mut rng = Rng(20_261_019);
    let mut free = busy;
    let mut report = serde_json::Map::new();
    for (mode, n) in [("indexed", rounds), ("full", full_rounds)] {
        if n == 0 {
            continue;
        }
        if mode == "full" {
            // the checks over the whole branch state, as before step 0b-1b
            e.index_since.insert(
                "@ref.rows".into(),
                BTreeMap::from([(BRANCH.into(), u64::MAX)]),
            );
        }
        let mut lat: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
        let mut errors: BTreeMap<&str, String> = BTreeMap::new();
        for _ in 0..n {
            let r = rng.next();
            let c = r % busy;
            let ops: Vec<(&str, Value, Option<&str>)> = vec![
                (
                    "create_order",
                    json!({"op": "create", "class": "SalesOrder", "object_id": uuid(next), "slug": format!("n-{next}"),
                    "body": {"customer": customer_uri(c), "amount": "5.00"}}),
                    None,
                ),
                (
                    "update_order",
                    json!({"op": "update", "object_id": order_ids[(r >> 20) as usize % order_ids.len()],
                    "body": {"customer": customer_uri(((r >> 20) % order_ids.len() as u64) % busy), "amount": format!("{}.00", r % 1000)}}),
                    None,
                ),
                (
                    "create_contact",
                    json!({"op": "create", "class": "Contact", "object_id": uuid(next + 1), "slug": format!("k-new-{next}"),
                    "body": {"account": customer_uri(free), "email": "new@example.com"}}),
                    None,
                ),
                (
                    "end_free",
                    json!({"op": "end", "object_id": uuid(free + spare)}),
                    None,
                ),
                (
                    "end_referenced",
                    json!({"op": "end", "object_id": uuid(c)}),
                    Some("MODEL.REFERENCE_RESTRICTED"),
                ),
            ];
            next += 2;
            free += 1;
            for (name, mut ch, expect) in ops {
                if matches!(ch["op"].as_str(), Some("update" | "end")) {
                    let oid = ch["object_id"].as_str().unwrap_or_default().to_string();
                    let o = e.read(&json!({"branch": BRANCH, "object_id": oid}))?;
                    ch["base"] = o["version_hash"].clone();
                }
                let t = Instant::now();
                let res = e.commit(&json!({"branch": BRANCH, "changes": [ch]}));
                let us = t.elapsed().as_micros() as u64;
                match (res, expect) {
                    (Ok(_), None) => lat.entry(name).or_default().push(us),
                    (Err(x), Some(code)) if x.code == code => lat.entry(name).or_default().push(us),
                    (Ok(_), Some(code)) => {
                        errors.insert(name, format!("expected {code}"));
                    }
                    (Err(x), _) => {
                        errors.insert(name, format!("{}: {}", x.code, x.message));
                    }
                }
            }
        }
        let mut m = serde_json::Map::new();
        for (k, v) in lat {
            m.insert(k.into(), summary(v));
        }
        if !errors.is_empty() {
            m.insert("errors".into(), json!(errors));
        }
        eprintln!("{mode}: {}", Value::Object(m.clone()));
        report.insert(mode.into(), Value::Object(m));
    }
    Ok(json!({
        "benchmark": "refs", "backend": backend, "objects": total, "customers": customers,
        "orders": orders, "contacts": contacts, "per_commit": per_commit, "rounds": rounds,
        "full_rounds": full_rounds, "load_ms": load_ms, "latency_ms": report,
    }))
}
