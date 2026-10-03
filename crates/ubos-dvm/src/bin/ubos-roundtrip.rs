//! `ubos-roundtrip` — the portable export and import round trip of EXIT-1-12
//! (VER-CONF-8403, STD-PROOF-070; PH-1 step 6c, DEC-059).
//!
//! For each of `--vaes` VAEs the tool builds a dataset on one VAE scope of a PostgreSQL
//! database (objects of two definition classes over several months with updates, a ledger,
//! a feature branch merged back, a release branch, tags), exports it, verifies the archive
//! with the independent `ubos-verify` when `--verifier` names the binary, imports it into a
//! second, empty VAE scope of the same database, exports that one again and imports it into
//! a third. After each import it compares, branch by branch, the head roots and state
//! entries, the tenant root, every tag, every ledger and every object read on every branch,
//! and the second export against the first, every side reading at the same adapter clock.
//! The report (`--report`) is signed by an ephemeral runner key like the other suite
//! reports.
//!
//! ```text
//! ubos-roundtrip --database "host=127.0.0.1 user=postgres dbname=bpa" [--vaes 3]
//!     [--objects 200] [--entries 300] [--months 3] [--verifier ./target/release/ubos-verify]
//!     [--report verify/reports/exit-1-12/roundtrip.json] [--archives <dir>]
//! ```

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::time::Instant;
use ubos_dvm::Adapter;
use ubos_types::{BpaError, canonical};

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn classes() -> Value {
    json!([
        {"name": "Customer", "label": "Customer", "description": "Customer", "namespace": "acme.finance",
         "parent": "DataObject", "kind": "definition", "schema_version": 1, "fields": [
            {"name": "name", "label": "name", "description": "name", "type": "text", "required": true},
            {"name": "email", "label": "email", "description": "email", "type": "text"},
            {"name": "credit_limit", "label": "credit limit", "description": "credit limit", "type": "money"},
            {"name": "country", "label": "country", "description": "country", "type": "text", "default": "US"}]},
        {"name": "Item", "label": "Item", "description": "Item", "namespace": "acme.finance",
         "parent": "DataObject", "kind": "definition", "schema_version": 1, "fields": [
            {"name": "sku", "label": "sku", "description": "sku", "type": "text", "required": true},
            {"name": "price", "label": "price", "description": "price", "type": "money", "required": true}]},
        {"name": "StockMove", "label": "StockMove", "description": "StockMove", "namespace": "acme.finance",
         "parent": "DataObject", "kind": "ledger", "schema_version": 1,
         "ledger": {"key": ["item"], "amounts": ["quantity"], "double_entry": false}, "fields": [
            {"name": "item", "label": "item", "description": "item", "type": "text", "required": true},
            {"name": "quantity", "label": "quantity", "description": "quantity", "type": "decimal", "required": true}]}
    ])
}

/// SplitMix64 finaliser.
fn mix(a: u64, b: u64, c: u64) -> u64 {
    let mut z = a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ c.wrapping_mul(0xD1B5_4A32_D192_ED03);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn uuid(v: u64, i: u64) -> String {
    format!(
        "0189{v:04x}-{:04x}-7{:03x}-8000-{i:012x}",
        (i >> 16) & 0xffff,
        i & 0xfff
    )
}

fn now_of(month: u64, step: u64) -> String {
    format!(
        "2026-{:02}-{:02}T10:{:02}:00.000000Z",
        1 + month,
        1 + step % 27,
        step % 60
    )
}

struct Spec {
    vaes: u64,
    objects: u64,
    entries: u64,
    months: u64,
}

struct Side {
    adapter: Adapter,
}

impl Side {
    fn exec(&mut self, op: &str, input: Value) -> Result<Value, BpaError> {
        self.adapter.execute(op, &input).map_err(|e| {
            err(format!(
                "{op} {}: {e}",
                canonical(&input)
                    .unwrap_or_default()
                    .chars()
                    .take(120)
                    .collect::<String>()
            ))
        })
    }

    fn scope(&mut self) -> Value {
        match self.adapter.engine_mut().store.scope() {
            Some(s) => json!({"tenant": s.tenant, "vae": s.vae}),
            None => Value::Null,
        }
    }
}

/// Build the dataset of VAE `v` on a fresh scope.
fn build(side: &mut Side, spec: &Spec, v: u64) -> Result<Value, BpaError> {
    side.exec("reset", json!({}))?;
    side.exec("set_now", json!({"now": now_of(0, 0)}))?;
    side.exec("load_genesis", json!({"classes": classes()}))?;
    let mut heads: BTreeMap<String, String> = BTreeMap::new();
    let items: u64 = (spec.objects / 4).max(1);
    let batch = 50;
    let mut step = 1;
    // initial load
    for start in (0..spec.objects).step_by(batch) {
        let mut changes = Vec::new();
        for i in start..(start + batch as u64).min(spec.objects) {
            let oid = uuid(v, i);
            let change = if i < items {
                json!({"op": "create", "class": "Item", "object_id": oid, "slug": format!("item-{i}"),
                    "body": {"sku": format!("SKU-{i:05}"), "price": {"amount": format!("{}.{:02}", 1 + mix(v, i, 1) % 500, mix(v, i, 2) % 100), "currency": "USD"}}})
            } else {
                json!({"op": "create", "class": "Customer", "object_id": oid, "slug": format!("c-{i}"),
                    "body": {"name": format!("Customer {i}"), "email": format!("c{i}@example.test"),
                        "credit_limit": {"amount": format!("{}.00", 100 * (1 + mix(v, i, 3) % 90)), "currency": "USD"}}})
            };
            changes.push(change);
        }
        side.exec("set_now", json!({"now": now_of(0, step)}))?;
        step += 1;
        let r = side.exec("commit", json!({"branch": "main", "changes": changes}))?;
        for (oid, vh) in r["versions"].as_object().into_iter().flatten() {
            heads.insert(oid.clone(), vh.as_str().unwrap_or_default().to_string());
        }
    }
    side.exec(
        "create_branch",
        json!({"name": "release/v1", "from": "main"}),
    )?;
    // months: ledger entries, updates, a feature branch merged back, a tag
    let mut entry_no = 0u64;
    for m in 0..spec.months {
        let per_month = spec.entries / spec.months.max(1);
        for start in (0..per_month).step_by(batch) {
            let mut changes = Vec::new();
            for _ in start..(start + batch as u64).min(per_month) {
                let item = mix(v, entry_no, 7) % items;
                changes.push(json!({"op": "append", "class": "StockMove", "entry_id": uuid(v + 0x100, entry_no),
                    "ledger_key": {"item": format!("SKU-{item:05}")},
                    "body": {"item": format!("SKU-{item:05}"), "quantity": format!("{}", 1 + mix(v, entry_no, 8) % 40)},
                    "valid_at": format!("2026-{:02}-{:02}", 1 + m, 1 + mix(v, entry_no, 9) % 27)}));
                entry_no += 1;
            }
            side.exec("set_now", json!({"now": now_of(m, step)}))?;
            step += 1;
            side.exec("commit", json!({"branch": "main", "changes": changes}))?;
        }
        // 10% of the customers change
        let mut changes = Vec::new();
        for i in items..spec.objects {
            if mix(v, i, 10 + m).is_multiple_of(10) {
                let oid = uuid(v, i);
                changes.push(
                    json!({"op": "update", "object_id": oid, "base": heads[&oid],
                    "body": {"email": format!("c{i}.m{m}@example.test")},
                    "valid": {"from": format!("2026-{:02}-15", 1 + m)}}),
                );
            }
        }
        if !changes.is_empty() {
            side.exec("set_now", json!({"now": now_of(m, step)}))?;
            step += 1;
            let r = side.exec("commit", json!({"branch": "main", "changes": changes}))?;
            for (oid, vh) in r["versions"].as_object().into_iter().flatten() {
                heads.insert(oid.clone(), vh.as_str().unwrap_or_default().to_string());
            }
        }
        // a feature branch with two new customers, merged back
        let feature = format!("feature/m{m}");
        side.exec("create_branch", json!({"name": feature, "from": "main"}))?;
        let mut changes = Vec::new();
        for k in 0..2u64 {
            let i = spec.objects + m * 2 + k;
            changes.push(json!({"op": "create", "class": "Customer", "object_id": uuid(v, i), "slug": format!("c-{i}"),
                "body": {"name": format!("Customer {i}"), "country": "DE"}}));
        }
        side.exec("set_now", json!({"now": now_of(m, step)}))?;
        step += 1;
        side.exec("commit", json!({"branch": feature, "changes": changes}))?;
        side.exec("set_now", json!({"now": now_of(m, step)}))?;
        step += 1;
        side.exec("merge", json!({"source": feature, "target": "main"}))?;
        side.exec(
            "tag",
            json!({"name": format!("m{m}-close"), "branch": "main"}),
        )?;
    }
    // the clock the comparisons read at: every side is set to it before its snapshot
    let now = now_of(spec.months.saturating_sub(1), step);
    side.exec("set_now", json!({"now": now}))?;
    Ok(
        json!({"objects": spec.objects + 2 * spec.months, "entries": entry_no,
        "months": spec.months, "now": now}),
    )
}

/// Everything compared after an import: per branch the exported state and info, the tenant
/// root, the tags, every ledger and every object on every branch.
fn snapshot(side: &mut Side, archive: &Value) -> Result<Value, BpaError> {
    let mut out = Map::new();
    let mut branches = Map::new();
    let mut objects = Map::new();
    for (name, _) in archive["refs"].as_object().into_iter().flatten() {
        let state = side.exec("export_state", json!({"branch": name}))?;
        let info = side.exec("branch_info", json!({"branch": name}))?;
        branches.insert(name.clone(), json!({"state": state, "info": info}));
        let mut reads = Map::new();
        for class in ["Customer", "Item"] {
            let list = side.exec("list_objects", json!({"branch": name, "class": class}))?;
            for o in list["objects"].as_array().into_iter().flatten() {
                let oid = o.as_str().unwrap_or_default();
                reads.insert(
                    oid.to_string(),
                    side.exec("read", json!({"branch": name, "object_id": oid}))?,
                );
            }
        }
        objects.insert(name.clone(), Value::Object(reads));
    }
    let mut ledgers = Map::new();
    let mut keys: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for (_, c) in archive["commits"].as_object().into_iter().flatten() {
        for g in c["ledger"].as_array().into_iter().flatten() {
            let class = g["class"].as_str().unwrap_or_default().to_string();
            keys.insert(
                format!("{class} {}", canonical(&g["ledger_key"])?),
                (class, g["ledger_key"].clone()),
            );
        }
    }
    for (name, (class, key)) in &keys {
        let r = side.exec(
            "read_ledger",
            json!({"branch": "main", "class": class, "ledger_key": key}),
        )?;
        ledgers.insert(name.clone(), r);
    }
    let mut tags = Map::new();
    for (name, _) in archive["tags"].as_object().into_iter().flatten() {
        tags.insert(name.clone(), side.exec("tag_info", json!({"name": name}))?);
    }
    out.insert("branches".into(), Value::Object(branches));
    out.insert("objects".into(), Value::Object(objects));
    out.insert("ledgers".into(), Value::Object(ledgers));
    out.insert("tags".into(), Value::Object(tags));
    out.insert("tenant_root".into(), side.exec("tenant_root", json!({}))?);
    Ok(Value::Object(out))
}

/// Compare two snapshots member by member; the names of the differing members.
fn compare(a: &Value, b: &Value) -> (Value, Vec<String>) {
    let mut diffs = Vec::new();
    let mut counts = Map::new();
    for k in ["branches", "objects", "ledgers", "tags"] {
        let (ma, mb) = (a[k].as_object(), b[k].as_object());
        let n = ma
            .map(|m| {
                if k == "objects" {
                    m.values()
                        .map(|v| v.as_object().map(|o| o.len()).unwrap_or(0))
                        .sum()
                } else {
                    m.len()
                }
            })
            .unwrap_or(0);
        counts.insert(k.into(), json!(n));
        if ma != mb {
            // name the first differing members, so that a failure says where to look
            let keys: Vec<String> = ma
                .into_iter()
                .flatten()
                .filter(|(name, v)| mb.and_then(|m| m.get(*name)) != Some(v))
                .map(|(name, v)| {
                    let other = mb.and_then(|m| m.get(name)).unwrap_or(&Value::Null);
                    let inner: Vec<String> = v
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(f, x)| other.get(*f) != Some(x))
                        .map(|(f, _)| f.clone())
                        .take(5)
                        .collect();
                    format!("{name} ({})", inner.join(", "))
                })
                .take(5)
                .collect();
            diffs.push(format!("{k}: {}", keys.join("; ")));
        }
    }
    if a["tenant_root"] != b["tenant_root"] {
        diffs.push("tenant_root".into());
    }
    (Value::Object(counts), diffs)
}

fn verifier(bin: &str, path: &str) -> Value {
    match std::process::Command::new(bin)
        .args(["archive", path, "--quiet"])
        .output()
    {
        Ok(o) => {
            json!({"binary": bin, "exit": o.status.code(), "output": String::from_utf8_lossy(&o.stdout).trim()})
        }
        Err(e) => json!({"binary": bin, "error": e.to_string()}),
    }
}

fn run(
    url: &str,
    spec: &Spec,
    verifier_bin: Option<&str>,
    archives: Option<&str>,
) -> Result<Value, BpaError> {
    let mut vaes = Vec::new();
    let mut all_ok = true;
    for v in 1..=spec.vaes {
        let t0 = Instant::now();
        let mut a = Side {
            adapter: Adapter::postgres(url)?,
        };
        let dataset = build(&mut a, spec, v)?;
        let scope_a = a.scope();
        let built_ms = t0.elapsed().as_millis();
        let t1 = Instant::now();
        let exported = a.exec("export_archive", json!({}))?;
        let export_ms = t1.elapsed().as_millis();
        let archive = exported["archive"].clone();
        let bytes = canonical(&archive)?.len();
        let snap_a = snapshot(&mut a, &archive)?;
        let mut verified = Value::Null;
        if let Some(bin) = verifier_bin {
            let dir = archives.map(str::to_string).unwrap_or_else(|| {
                std::env::temp_dir()
                    .join("ubos-roundtrip")
                    .to_string_lossy()
                    .into_owned()
            });
            std::fs::create_dir_all(&dir).map_err(err)?;
            let path = format!("{dir}/vae-{v}.json");
            std::fs::write(&path, serde_json::to_vec(&archive).map_err(err)?).map_err(err)?;
            verified = verifier(bin, &path);
            if verified["exit"] != json!(0) {
                all_ok = false;
                eprintln!("vae {v}: verifier failed: {verified}");
            }
        }
        // leg 1: import into a second, empty VAE scope
        let mut b = Side {
            adapter: Adapter::postgres(url)?,
        };
        b.exec("reset", json!({}))?;
        b.exec("set_now", json!({"now": dataset["now"].clone()}))?;
        let t2 = Instant::now();
        let imported = b.exec("import_archive", json!({"archive": archive}))?;
        let import_ms = t2.elapsed().as_millis();
        let scope_b = b.scope();
        let snap_b = snapshot(&mut b, &archive)?;
        let (counts, diffs_b) = compare(&snap_a, &snap_b);
        // leg 2: export the second VAE and import into a third
        let exported2 = b.exec("export_archive", json!({}))?;
        let archive2 = exported2["archive"].clone();
        let identical = archive2 == archive;
        let mut c = Side {
            adapter: Adapter::postgres(url)?,
        };
        c.exec("reset", json!({}))?;
        c.exec("set_now", json!({"now": dataset["now"].clone()}))?;
        c.exec("import_archive", json!({"archive": archive2}))?;
        let scope_c = c.scope();
        let snap_c = snapshot(&mut c, &archive)?;
        let (_, diffs_c) = compare(&snap_a, &snap_c);
        let ok = diffs_b.is_empty() && diffs_c.is_empty() && identical;
        all_ok &= ok;
        println!(
            "vae {v}: {} commits, {} versions, {} entries, {} KB; import {} ms; roots {}; second export {}",
            exported["counts"]["commits"],
            exported["counts"]["version_records"],
            exported["counts"]["entries"],
            bytes / 1024,
            import_ms,
            if diffs_b.is_empty() && diffs_c.is_empty() {
                "identical"
            } else {
                "DIFFER"
            },
            if identical { "identical" } else { "DIFFERS" }
        );
        vaes.push(json!({
            "vae": v, "dataset": dataset, "scopes": {"source": scope_a, "imported": scope_b, "reimported": scope_c},
            "counts": exported["counts"], "archive_bytes": bytes, "imported": imported,
            "milliseconds": {"build": built_ms, "export": export_ms, "import": import_ms},
            "verifier": verified,
            "compared": counts, "differences_after_import": diffs_b, "differences_after_reimport": diffs_c,
            "second_export_identical": identical, "result": if ok { "pass" } else { "fail" },
        }));
    }
    Ok(json!({"vaes": vaes, "result": if all_ok { "pass" } else { "fail" }}))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let num = |k: &str, d: u64| arg(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let Some(url) = arg("--database") else {
        eprintln!(
            "usage: ubos-roundtrip --database <url> [--vaes N] [--objects N] [--entries N] [--months N] [--verifier <ubos-verify>] [--report <file>] [--archives <dir>]"
        );
        std::process::exit(2);
    };
    let spec = Spec {
        vaes: num("--vaes", 3),
        objects: num("--objects", 200),
        entries: num("--entries", 300),
        months: num("--months", 3),
    };
    let start = Instant::now();
    let out = match run(
        &url,
        &spec,
        arg("--verifier").as_deref(),
        arg("--archives").as_deref(),
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("roundtrip failed: {e}");
            std::process::exit(1);
        }
    };
    let mut report = json!({
        "criterion": "EXIT-1-12", "item": "VER-CONF-8403", "clause": "STD-PROOF-070",
        "implementation": {"name": "ubos-roundtrip", "crate": "ubos-dvm", "version": env!("CARGO_PKG_VERSION")},
        "environment": {"backend": "postgresql", "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)},
        "specification": {"vaes": spec.vaes, "objects": spec.objects, "entries": spec.entries, "months": spec.months},
        "vaes": out["vaes"], "result": out["result"],
        "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    let seed: [u8; 32] = ubos_crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report).unwrap_or_default();
    let sig = ubos_crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] = json!({"alg": "ed25519", "key": hex(&ubos_crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    if let Some(p) = arg("--report") {
        if let Some(dir) = std::path::Path::new(&p).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(
            &p,
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        ) {
            eprintln!("cannot write {p}: {e}");
        }
    }
    println!(
        "roundtrip: {} ({} VAEs, {} s)",
        out["result"].as_str().unwrap_or("?"),
        spec.vaes,
        report["seconds"]
    );
    if out["result"] != json!("pass") {
        std::process::exit(1);
    }
}
