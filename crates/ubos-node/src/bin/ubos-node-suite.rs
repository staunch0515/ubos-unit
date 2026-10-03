//! `ubos-node-suite` — the host-service suite of the in-process node host (PH-1 step 7a,
//! DEC-060): every CTR-002 service on PostgreSQL, the key hierarchy, the node signer behind
//! signed commit records, reopening a VAE, tenant isolation of blobs and secrets, journaled
//! connector calls replayed without the host, jobs with leases and poison, the event log
//! and telemetry redaction. The report (`--report`) is signed by an ephemeral runner key.
//!
//! ```text
//! ubos-node-suite --database "host=127.0.0.1 user=postgres dbname=bpa" [--report <file>]
//! ```

use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;
use ubos_dvm::crypto;
use ubos_dvm::host::Vae;
use ubos_dvm::types::{BpaError, canonical};
use ubos_node::keys::Hierarchy;
use ubos_node::services::{EventLog, JobQueue, PgBlobs};
use ubos_node::{Node, Tenant};

#[global_allocator]
static ALLOC: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

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

fn code(r: &Result<Value, BpaError>) -> &str {
    r.as_ref().err().map(|e| e.code).unwrap_or("ok")
}

fn classes() -> Value {
    json!([
        {"name": "Account", "label": "Account", "description": "Account", "namespace": "acme.suite",
         "parent": "DataObject", "kind": "definition", "schema_version": 1, "fields": [
            {"name": "name", "label": "name", "description": "name", "type": "text", "required": true},
            {"name": "note", "label": "note", "description": "note", "type": "text"}]}
    ])
}

fn logic(name: &str, body: Value, capabilities: Value) -> Value {
    json!({"abi": "1.0", "capabilities": capabilities, "exports": ["main"], "name": name,
        "profile": "standard", "state": "committed", "tier": "host",
        "source": {"functions": {"main": {"params": [], "body": body}}}})
}

struct Suite {
    node: Node,
    url: String,
    results: Vec<Value>,
}

impl Suite {
    fn case(&mut self, name: &str, f: impl FnOnce(&mut Suite) -> Result<Value, BpaError>) {
        let t = Instant::now();
        let out = f(self);
        let ms = t.elapsed().as_millis();
        let (ok, detail) = match &out {
            Ok(v) => (true, v.clone()),
            Err(e) => (false, json!({"error": e.code, "message": e.message})),
        };
        println!(
            "{} {name} ({ms} ms){}",
            if ok { "ok  " } else { "FAIL" },
            if ok {
                String::new()
            } else {
                format!(": {detail}")
            }
        );
        self.results.push(json!({"case": name, "result": if ok { "pass" } else { "fail" }, "detail": detail, "ms": ms}));
    }

    fn tenant(&self, prefix: &str) -> Result<Tenant, BpaError> {
        let slug = format!("{prefix}-{}", &ubos_node::uuid()?[..8]);
        self.node.create_tenant(&slug)
    }

    fn open(&self, t: &Tenant, authority: &str) -> Result<Vae, BpaError> {
        let mut vae = self.node.open_vae(t, authority)?;
        if vae.engine_mut().registry.declarations().is_empty() {
            vae.call("load_genesis", &json!({"classes": classes()}))?;
        }
        Ok(vae)
    }
}

/// Run every case of the suite on the node at `url`.
fn run(url: &str) -> Result<Suite, BpaError> {
    let node = Node::connect(url)?;
    let mut s = Suite {
        node,
        url: url.to_string(),
        results: Vec::new(),
    };
    let ctx = Node::context("did:ubos:acme:01900000-0000-7000-8000-000000000002");

    s.case("tenant_registry", |s| {
        let a = s.tenant("alpha")?;
        let b = s.tenant("beta")?;
        check(a.id != b.id, "distinct tenant ids")?;
        check(
            s.node.tenant(&a.slug)?.map(|t| t.id) == Some(a.id.clone()),
            "tenant found by slug",
        )?;
        let dup = s.node.create_tenant(&a.slug);
        check(
            dup.as_ref().err().map(|e| e.code) == Some("TXN.ALREADY_EXISTS"),
            "duplicate slug rejected",
        )?;
        let v1 = s.node.create_vae(&a, "alpha")?;
        check(
            s.node.vae(&a, "alpha")? == Some(v1.clone()),
            "VAE found by authority",
        )?;
        check(
            s.node.create_vae(&a, "alpha").is_err(),
            "duplicate authority rejected",
        )?;
        check(s.node.tenants()?.len() >= 2, "tenants listed")?;
        Ok(json!({"tenants": [a.slug, b.slug], "vae": v1}))
    });

    s.case("key_hierarchy", |s| {
        let a = s.tenant("keys")?;
        let b = s.tenant("keys")?;
        let owner = Arc::new(ubos_node::db::Db::owner(&s.url)?);
        let ha = Hierarchy::open(owner.clone(), ubos_node::root_kek()?, &a.id)?;
        let hb = Hierarchy::open(owner.clone(), ubos_node::root_kek()?, &b.id)?;
        check(
            ha.derive("ubos.data-key.v1", "1") != hb.derive("ubos.data-key.v1", "1"),
            "tenants derive different keys",
        )?;
        let wrong = Hierarchy::open(owner.clone(), [9u8; 32], &a.id);
        check(
            wrong.as_ref().err().map(|e| e.code) == Some("IAM.KEY_UNAVAILABLE"),
            "wrong root KEK rejected",
        )?;
        use ubos_dvm::crypto::KeyService;
        let k1 = ha.current("")?;
        let next = ha.rotate_data_key()?;
        let k2 = ha.current("")?;
        check(
            next == 2 && k2.id == 2 && k1.key != k2.key,
            "data key rotated",
        )?;
        check(ha.by_id("", 1)?.key == k1.key, "old generation readable")?;
        check(ha.by_id("", 3).is_err(), "future generation unknown")?;
        Ok(json!({"generations": next}))
    });

    s.case("open_vae_signed_commits", |s| {
        let t = s.tenant("sign")?;
        let mut vae = s.open(&t, "signco")?;
        check(ubos_node::has_signer(vae.engine_mut().store.as_ref()), "node signer installed")?;
        let r = vae.commit(&ctx, json!({"branch": "main", "changes": [{"op": "create", "class": "Account",
            "object_id": "01890a5d-ac96-7001-8000-000000000001", "slug": "a-1", "body": {"name": "Acme"}}]}))?;
        check(r["cseq"] == json!(1), "first commit cseq 1")?;
        let rec = vae.call("commit_record", &json!({"branch": "main"}))?;
        let did = vae.call("node_did", &json!({}))?;
        check(did["id"].as_str().is_some_and(|d| d.starts_with("did:ubos:signco:")), "node DID is the VAE's registered DID")?;
        check(rec["vae"] == json!("signco"), "commit record names the authority")?;
        let sig = &rec["signatures"][0];
        crypto::verify(sig, rec["commit_id"].as_str().unwrap_or_default(), &did, rec["tx_time"].as_str())?;
        check(sig["signer"] == did["id"], "signed by the node key")?;
        let genesis = vae.call("commit_record", &json!({"branch": "main", "time": 0}))?;
        crypto::verify(&genesis["signatures"][0], genesis["commit_id"].as_str().unwrap_or_default(), &did, None)?;
        Ok(json!({"did": did["id"], "commit_id": rec["commit_id"]}))
    });

    s.case("reopen_vae", |s| {
        let t = s.tenant("reopen")?;
        let mut vae = s.open(&t, "reopenco")?;
        vae.commit(&ctx, json!({"branch": "main", "changes": [{"op": "create", "class": "Account",
            "object_id": "01890a5d-ac96-7001-8000-000000000001", "slug": "a-1", "body": {"name": "Acme"}}]}))?;
        let p1 = vae.begin_process(&ctx, "action")?;
        drop(vae);
        let mut again = s.node.open_vae(&t, "reopenco")?;
        check(!again.engine_mut().registry.declarations().is_empty(), "declarations restored from the store")?;
        let read = again.read(&ctx, json!({"branch": "main", "object_id": "01890a5d-ac96-7001-8000-000000000001"}))?;
        check(read["body"]["name"] == json!("Acme"), "object readable after reopen")?;
        let r = again.commit(&ctx, json!({"branch": "main", "changes": [{"op": "create", "class": "Account",
            "object_id": "01890a5d-ac96-7002-8000-000000000002", "slug": "a-2", "body": {"name": "Beta"}}]}))?;
        check(r["cseq"] == json!(2), "commits continue")?;
        let p2 = again.begin_process(&ctx, "action")?;
        check(p1["process_id"] != p2["process_id"], "process identifiers continue")?;
        Ok(json!({"cseq": r["cseq"]}))
    });

    s.case("node_key_rotation", |s| {
        let t = s.tenant("rotate")?;
        let mut vae = s.open(&t, "rotco")?;
        vae.commit(&ctx, json!({"branch": "main", "changes": [{"op": "create", "class": "Account",
            "object_id": "01890a5d-ac96-7001-8000-000000000001", "slug": "a-1", "body": {"name": "Acme"}}]}))?;
        let r1 = vae.call("commit_record", &json!({"branch": "main"}))?;
        let doc = vae.call("rotate_node_key", &json!({"seed": "00".repeat(32)}))?;
        check(doc["keys"].as_array().map(Vec::len) == Some(2), "two keys after rotation")?;
        check(doc["keys"][0]["valid_to"] == doc["keys"][1]["valid_from"], "history is contiguous")?;
        vae.commit(&ctx, json!({"branch": "main", "changes": [{"op": "create", "class": "Account",
            "object_id": "01890a5d-ac96-7002-8000-000000000002", "slug": "a-2", "body": {"name": "Beta"}}]}))?;
        let r2 = vae.call("commit_record", &json!({"branch": "main"}))?;
        check(r2["signatures"][0]["key_id"].as_str().is_some_and(|k| k.ends_with("#k2")), "new commits use #k2")?;
        crypto::verify(&r1["signatures"][0], r1["commit_id"].as_str().unwrap_or_default(), &doc, r1["tx_time"].as_str())?;
        crypto::verify(&r2["signatures"][0], r2["commit_id"].as_str().unwrap_or_default(), &doc, r2["tx_time"].as_str())?;
        let late = crypto::verify(&r1["signatures"][0], r1["commit_id"].as_str().unwrap_or_default(), &doc, r2["tx_time"].as_str());
        check(late.as_ref().err().map(|e| e.code) == Some("PROOF.SIGNATURE_INVALID"), "old key invalid after rotation")?;
        let reopened = s.node.open_vae(&t, "rotco")?.call("node_did", &json!({}))?;
        check(reopened == doc, "key history persists")?;
        Ok(json!({"keys": doc["keys"].as_array().map(Vec::len)}))
    });

    s.case("blob_service_isolated", |s| {
        let t = s.tenant("blob")?;
        let other = s.tenant("blob")?;
        let mut vae = s.open(&t, "blobco")?;
        let big: String = (0..3)
            .map(|_| crypto::random_bytes::<8192>().unwrap_or([0u8; 8192]).iter().map(|b| format!("{b:02x}")).collect::<String>())
            .collect();
        // a record above the inline size (8 KiB stored) is placed in the blob service
        let record = json!({"kind": "suite", "payload": big});
        let id = vae.engine_mut().store.put_record(&record)?;
        let blobs = PgBlobs::new(s.node.tenant_db(&t)?, &t.id);
        check(blobs.count()? >= 1, "a large record went to the blob service")?;
        check(vae.engine_mut().store.record(&id)? == Some(record.clone()), "large record read back through the blob service")?;
        vae.commit(&ctx, json!({"branch": "main", "changes": [{"op": "create", "class": "Account",
            "object_id": "01890a5d-ac96-7001-8000-000000000001", "slug": "a-1", "body": {"name": "Acme", "note": big}}]}))?;
        let read = vae.read(&ctx, json!({"branch": "main", "object_id": "01890a5d-ac96-7001-8000-000000000001"}))?;
        check(read["body"]["note"].as_str() == Some(big.as_str()), "large body read back")?;
        let foreign = PgBlobs::new(s.node.tenant_db(&other)?, &other.id);
        check(foreign.count()? == 0, "another tenant sees no blobs")?;
        Ok(json!({"blobs": blobs.count()?}))
    });

    s.case("secrets", |s| {
        let t = s.tenant("secret")?;
        let other = s.tenant("secret")?;
        use ubos_dvm::host::Secrets;
        let store = ubos_node::keys::SecretStore::new(s.node.tenant_db(&t)?, s.node.keys(&t)?);
        store.put("api-key", b"s3cret")?;
        check(store.resolve("api-key")? == b"s3cret", "secret round trip")?;
        let missing = store.resolve("nope");
        check(
            missing.as_ref().err().map(|e| e.code) == Some("IAM.NOT_FOUND"),
            "unknown handle",
        )?;
        let hierarchy = s.node.keys(&t)?;
        hierarchy.rotate_data_key()?;
        let store2 = ubos_node::keys::SecretStore::new(s.node.tenant_db(&t)?, hierarchy);
        check(
            store2.resolve("api-key")? == b"s3cret",
            "readable after data key rotation",
        )?;
        store2.put("token", b"t0ken")?;
        check(
            store2.resolve("token")? == b"t0ken",
            "new secret under the new key",
        )?;
        let foreign =
            ubos_node::keys::SecretStore::new(s.node.tenant_db(&other)?, s.node.keys(&other)?);
        check(
            foreign.resolve("api-key").is_err(),
            "another tenant cannot resolve it",
        )?;
        check(store2.delete("token")?, "secret deleted")?;
        Ok(json!({"handles": 1}))
    });

    s.case("connector_journaled_and_replayed", |s| {
        let t = s.tenant("conn")?;
        s.node.services().connectors.register(
            "echo",
            Arc::new(|op, req, secrets| {
                let n = secrets.resolve("api-key").map(|b| b.len()).unwrap_or(0);
                Ok(json!({"op": op, "echo": req, "secret_len": n}))
            }),
        );
        let mut vae = s.open(&t, "connco")?;
        ubos_node::keys::SecretStore::new(s.node.tenant_db(&t)?, s.node.keys(&t)?).put("api-key", b"12345")?;
        vae.call("create_branch", &json!({"name": "feature/app", "from": "main"}))?;
        let body = json!([{"do": "call.connector", "as": "r", "args": {"connector": "\"echo\"", "operation": "\"ping\"", "request": {"n": 1}, "when": "\"now\""}}, {"return": "r"}]);
        vae.commit(&ctx, json!({"branch": "feature/app", "changes": [{"op": "create", "class": "Logic",
            "object_id": "01890a5d-ac96-705a-8000-00000000005a", "slug": "acme-echo", "body": logic("acme.echo", body, json!(["call"]))}]}))?;
        let pid = "01890a5d-ac96-71f5-8000-0000000001f5";
        let r = vae.invoke_port(&ctx, json!({"asset": "acme.echo", "function": "main", "args": [], "branch": "feature/app", "process": {"process_id": pid}}))?;
        check(r["result"]["secret_len"] == json!(5), &format!("connector saw the secret, logic only its length; got {r}"))?;
        check(r["result"]["echo"]["n"] == json!(1), "request echoed")?;
        let journal = vae.call("journal", &json!({"process_id": pid}))?;
        let recs = journal["records"].as_array().cloned().unwrap_or_default();
        check(recs.iter().any(|x| x["instruction"] == json!("call.connector")), "connector call journaled")?;
        let calls_before = s.node.services().connectors.calls.lock().map(|g| g.len()).unwrap_or(0);
        let replay = vae.replay(pid)?;
        check(replay["outcome"] == json!("identical"), "replay identical")?;
        let calls_after = s.node.services().connectors.calls.lock().map(|g| g.len()).unwrap_or(0);
        check(calls_before == calls_after, "replay did not call the connector")?;
        let unknown = vae.invoke_port(&ctx, json!({"asset": "acme.echo", "function": "main", "args": [], "branch": "feature/app"}));
        check(unknown.is_ok(), "second live call runs")?;
        let body2 = json!([{"do": "call.connector", "args": {"connector": "\"bank\"", "operation": "\"pay\"", "request": {}, "when": "\"now\""}}]);
        vae.commit(&ctx, json!({"branch": "feature/app", "changes": [{"op": "create", "class": "Logic",
            "object_id": "01890a5d-ac96-705b-8000-00000000005b", "slug": "acme-bank", "body": logic("acme.bank", body2, json!(["call"]))}]}))?;
        let bank = vae.invoke_port(&ctx, json!({"asset": "acme.bank", "function": "main", "args": [], "branch": "feature/app"}));
        check(code(&bank) == "LOGIC.CONNECTOR_UNAVAILABLE", "unregistered connector unavailable")?;
        let body3 = json!([{"do": "ask.model", "args": {"route": "\"default\"", "template": "\"ubos://connco/Logic/acme-ask\"", "vars": {}, "output_schema": {"type": "text"}}}]);
        let r3 = vae.commit(&ctx, json!({"branch": "feature/app", "changes": [{"op": "create", "class": "Logic",
            "object_id": "01890a5d-ac96-705c-8000-00000000005c", "slug": "acme-ask", "body": logic("acme.ask", body3, json!(["ask"]))}]}));
        let model = match r3 {
            Ok(_) => code(&vae.invoke_port(&ctx, json!({"asset": "acme.ask", "function": "main", "args": [], "branch": "feature/app"}))).to_string(),
            Err(e) => e.code.to_string(),
        };
        Ok(json!({"journal_records": recs.len(), "model": model}))
    });

    s.case("event_log_release", |s| {
        let t = s.tenant("events")?;
        let mut vae = s.open(&t, "eventco")?;
        let vae_id = s.node.vae(&t, "eventco")?.unwrap_or_default();
        let effect = json!({"effect_id": "suite/1", "type": "Pinged", "branch": "main", "cseq": 1, "payload": {"n": 1}});
        vae.engine_mut().store.outbox_push(std::slice::from_ref(&effect))?;
        let released = vae.release_outbox()?;
        check(released == 1, "one effect released")?;
        let log = EventLog::new(s.node.tenant_db(&t)?, &t.id, &vae_id);
        let events = log.events(0, 10)?;
        check(events.len() == 1 && events[0]["type"] == json!("Pinged"), "event in the log")?;
        check(vae.engine_mut().store.outbox_pending()?.is_empty(), "outbox acknowledged")?;
        check(vae.release_outbox()? == 0, "nothing released twice")?;
        Ok(json!({"events": events.len()}))
    });

    s.case("jobs_lease_retry_poison", |s| {
        let t = s.tenant("jobs")?;
        let vae_id = s.node.create_vae(&t, "jobco")?;
        use ubos_dvm::host::Jobs;
        let q = JobQueue::new(s.node.tenant_db(&t)?, &t.id, &vae_id);
        let id = q.enqueue(&json!({"kind": "reindex", "payload": {"x": 1}, "max_attempts": 2}))?;
        let c1 = q.claim("w1", 60)?.ok_or_else(|| err("claim 1"))?;
        check(
            c1["job_id"] == json!(id) && c1["attempts"] == json!(1),
            "claimed once",
        )?;
        check(q.claim("w2", 60)?.is_none(), "leased job not claimed twice")?;
        check(q.fail(&id, "boom")? == "queued", "first failure requeues")?;
        let c2 = q.claim("w2", 60)?.ok_or_else(|| err("claim 2"))?;
        check(c2["attempts"] == json!(2), "second attempt")?;
        check(
            q.fail(&id, "boom")? == "poison",
            "exhausted attempts are poison",
        )?;
        let id2 = q.enqueue(&json!({"kind": "report", "payload": {}}))?;
        let c3 = q.claim("w1", 60)?.ok_or_else(|| err("claim 3"))?;
        check(c3["job_id"] == json!(id2), "next job claimed")?;
        q.complete(&id2, &json!({"rows": 3}))?;
        check(
            q.job(&id2)?.map(|j| j["state"] == json!("done")) == Some(true),
            "job done",
        )?;
        Ok(json!({"poison": id, "done": id2}))
    });

    s.case("telemetry_redaction", |s| {
        let tel = s.node.services().telemetry.clone();
        let before = tel.events.lock().map(|g| g.len()).unwrap_or(0);
        tel.emit_now(&json!({"name": "suite.event", "op": "commit", "duration_us": 12, "body": {"name": "Acme"}, "customer": "Acme Ltd"}));
        let last = tel.events.lock().ok().and_then(|g| g.back().cloned()).unwrap_or(Value::Null);
        check(tel.events.lock().map(|g| g.len()).unwrap_or(0) == before + 1, "event kept")?;
        check(last.get("body").is_none() && last.get("customer").is_none(), "business data dropped")?;
        check(last["op"] == json!("commit"), "operational members kept")?;
        check(tel.events.lock().map(|g| g.iter().any(|e| e["name"] == json!("dvm.operation"))).unwrap_or(false), "operations emit telemetry")?;
        Ok(last)
    });

    s.case("entropy_from_host", |s| {
        let t = s.tenant("seed")?;
        let mut vae = s.open(&t, "seedco")?;
        vae.call("create_branch", &json!({"name": "feature/app", "from": "main"}))?;
        let body = json!([{"do": "sys.uuid", "as": "id"}, {"return": "id"}]);
        vae.commit(&ctx, json!({"branch": "feature/app", "changes": [{"op": "create", "class": "Logic",
            "object_id": "01890a5d-ac96-705a-8000-00000000005a", "slug": "acme-id", "body": logic("acme.id", body, json!([]))}]}))?;
        let a = vae.invoke_port(&ctx, json!({"asset": "acme.id", "function": "main", "args": [], "branch": "feature/app"}))?;
        let b = vae.invoke_port(&ctx, json!({"asset": "acme.id", "function": "main", "args": [], "branch": "feature/app"}))?;
        check(a["result"] != b["result"], "host entropy differs per process")?;
        let pid = a["process_id"].as_str().unwrap_or_default().to_string();
        let replay = vae.replay(&pid)?;
        check(replay["outcome"] == json!("identical"), "seed recorded, replay identical")?;
        Ok(json!({"a": a["result"], "b": b["result"]}))
    });
    Ok(s)
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
        eprintln!("usage: ubos-node-suite --database <url> [--report <file>]");
        std::process::exit(2);
    };
    let start = Instant::now();
    let s = match run(&url) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot connect: {e}");
            std::process::exit(1);
        }
    };
    let passed = s
        .results
        .iter()
        .filter(|r| r["result"] == json!("pass"))
        .count();
    let mut report = json!({
        "suite": "SUITE-NOD-HOST", "step": "PH-1 7a", "decision": "DEC-060",
        "implementation": {"crate": "ubos-node", "version": env!("CARGO_PKG_VERSION")},
        "environment": {"backend": "postgresql", "os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "cases": s.results.len(), "passed": passed, "results": s.results,
        "seconds": (start.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    let seed: [u8; 32] = crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report).unwrap_or_default();
    let sig = crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] =
        json!({"alg": "ed25519", "key": hex(&crypto::ed25519_public(&seed)), "sig": hex(&sig)});
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
        "node suite: {passed} of {} cases passed ({} s)",
        s.results.len(),
        report["seconds"]
    );
    if passed != s.results.len() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    /// Every case of the host suite passes on the PostgreSQL named by `UBOS_TEST_DATABASE`
    /// (SUITE-NOD-HOST; a test body of the mutation score, VER-INSP-0001); skipped otherwise.
    #[test]
    fn host_suite_passes_on_postgres() {
        let Ok(url) = std::env::var("UBOS_TEST_DATABASE") else {
            eprintln!("UBOS_TEST_DATABASE not set: skipped");
            return;
        };
        let s = super::run(&url).expect("node connects");
        let failed: Vec<String> = s
            .results
            .iter()
            .filter(|r| r["result"] != serde_json::json!("pass"))
            .map(|r| r.to_string())
            .collect();
        assert!(s.results.len() >= 12, "{} cases", s.results.len());
        assert!(failed.is_empty(), "failing cases:\n{}", failed.join("\n"));
    }
}
