//! `ubos-fault` — durability under real crashes on PostgreSQL (SUITE-DVM-FAULT,
//! VER-FAULT-0001, EXIT-1-06; PH-1 step 8b, DEC-061).
//!
//! ```text
//! ubos-fault --database <url> [--events 10000] [--workers 4] [--power-loss]
//!            [--pg-stop <command>] [--pg-start <command>] [--verify-every 50]
//!            [--dir <ack-log dir>] [--report <file>] [--verbose]
//! ubos-fault worker --database <url> --tenant <t> --vae <v> --authority <a> --ack <file> --seed <n>
//! ```
//!
//! The driver builds a VAE (40 objects, a branch) and starts worker processes that run the
//! mixed workload (WL-MIXED-OLTP: updates against the versions they read, appends, reads)
//! and append every acknowledged commit to their own log, `fsync`ed before the next step.
//! Then it injects events: SIGKILL of every worker process at a random moment (the node
//! crashes), and, with
//! `--power-loss`, an immediate stop of the PostgreSQL cluster (`pg_ctlcluster … stop -m
//! immediate`: every server process is killed without a checkpoint, the server's equivalent
//! of power loss) followed by `pg_ctl start`, both commands configurable; the two kinds
//! alternate. After every event the recovery check opens the VAE afresh and compares the
//! acknowledgement logs with the store on the quiet store (the workers are dead until the
//! check ends): every acknowledged commit is at its `cseq` with its identity and its
//! versions, `cseq` is dense on every branch, and a commit the store holds without an
//! acknowledgement is one whose acknowledgement the crash lost (counted, never partial). The
//! root verifier runs on the commits added since the last
//! check; every `--verify-every` events and at the end, the index and sequence verifiers
//! rebuild the whole branch (DSN-DVM-092, DSN-DVM-402). The report is signed by an ephemeral
//! runner key.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use ubos_dvm::verify::verify_roots;
use ubos_dvm::{Adapter, BpaError};
use ubos_store::PgBackend;
use ubos_store::kernel::{Scope, StoreRead};
use ubos_store::verify::{verify_indexes, verify_sequences};
use ubos_types::canonical;

const OBJECTS: u64 = 40;
const BRANCHES: [&str; 2] = ["main", "feature/x"];
const KEYS: [&str; 3] = ["item-0", "item-1", "item-2"];

fn err(m: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.to_string())
}

fn uid(n: u64) -> String {
    format!("01890a5d-ac96-7{:03x}-8000-{:012x}", n % 4096, n)
}

fn classes() -> Value {
    let f = |n: &str| json!({"name": n, "type": "text", "label": n, "description": n});
    json!([
        {"name": "Item", "namespace": "fault", "kind": "definition", "parent": "DataObject", "label": "Item",
         "description": "Item", "schema_version": 1, "fields": [f("a"), f("b"), f("c")]},
        {"name": "StockMove", "namespace": "fault", "kind": "ledger", "parent": "DataObject", "label": "StockMove",
         "description": "StockMove", "schema_version": 1,
         "fields": [{"name": "item", "type": "text", "label": "item", "description": "item", "required": true},
                    {"name": "quantity", "type": "decimal", "label": "quantity", "description": "quantity", "required": true}],
         "ledger": {"key": ["item"], "amounts": ["quantity"], "double_entry": false}}
    ])
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn trace(verbose: bool, what: &str) {
    if verbose {
        eprintln!("    {what}");
    }
}

fn opt(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

// ---- worker ----

/// The worker: the mixed workload with an acknowledgement log; exits 3 on a storage error
/// (the driver restarts it).
fn worker(args: &[String]) -> i32 {
    let get = |k: &str| opt(args, k).unwrap_or_default();
    let (url, tenant, vae, authority, ack) = (
        get("--database"),
        get("--tenant"),
        get("--vae"),
        get("--authority"),
        get("--ack"),
    );
    let seed: u64 = get("--seed").parse().unwrap_or(1);
    let mut rng = Rng(seed ^ 0x0BAD_5EED_0BAD_5EED);
    let mut adapter = match Adapter::postgres_existing(&url, &tenant, &vae, &authority) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("worker {seed}: cannot open the VAE: {e}");
            return 3;
        }
    };
    let mut log = match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&ack)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("worker {seed}: ack log: {e}");
            return 2;
        }
    };
    let mut next_entry = seed * 1_000_000 + 10_000;
    loop {
        let branch = if rng.chance(80) {
            BRANCHES[0]
        } else {
            BRANCHES[1]
        };
        // WL-MIXED-OLTP: 20% reads, 80% commits (updates with appends)
        if rng.chance(20) {
            let o = uid(rng.below(OBJECTS));
            if let Err(e) = adapter.execute("read", &json!({"branch": branch, "object_id": o}))
                && e.code == "NOD.STORAGE_UNAVAILABLE"
            {
                return 3;
            }
            continue;
        }
        let mut attempts = 0;
        loop {
            attempts += 1;
            let mut changes = Vec::new();
            let mut entries = Vec::new();
            for _ in 0..1 + rng.below(2) {
                let o = uid(rng.below(OBJECTS));
                let r = match adapter.execute("read", &json!({"branch": branch, "object_id": o})) {
                    Ok(r) => r,
                    Err(e) if e.code == "NOD.STORAGE_UNAVAILABLE" => return 3,
                    Err(_) => continue,
                };
                let field = ["a", "b", "c"][rng.below(3) as usize];
                changes.push(
                    json!({"op": "update", "object_id": o, "base": r["version_hash"],
                                    "body": {field: format!("w{seed}-{}", rng.next() % 1000)}}),
                );
            }
            if rng.chance(50) {
                next_entry += 1;
                let key = KEYS[rng.below(3) as usize];
                changes.push(json!({"op": "append", "class": "StockMove", "entry_id": uid(next_entry), "ledger_key": {"item": key},
                                    "body": {"item": key, "quantity": (rng.below(20) as i64 - 5).to_string()}, "valid_at": "2026-03-15"}));
                entries.push(json!({"key": key, "id": uid(next_entry)}));
            }
            if changes.is_empty() {
                break;
            }
            match adapter.execute("commit", &json!({"branch": branch, "changes": changes})) {
                Ok(r) => {
                    let line = json!({"b": branch, "c": r["cseq"], "id": r["commit_id"], "v": r["versions"], "e": entries});
                    if writeln!(log, "{line}")
                        .and_then(|_| log.sync_data())
                        .is_err()
                    {
                        return 2;
                    }
                    break;
                }
                Err(e) if e.code == "TXN.HEAD_MOVED" => {
                    if attempts >= 5 {
                        break;
                    }
                }
                Err(e) if e.code == "NOD.STORAGE_UNAVAILABLE" => return 3,
                Err(_) => break,
            }
        }
    }
}

// ---- driver ----

struct AckLine {
    b: String,
    c: u64,
    id: String,
    v: BTreeMap<String, String>,
    e: Vec<Value>,
}

impl AckLine {
    /// A complete acknowledgement line; a line the crash cut short parses as none.
    fn parse(line: &str) -> Option<AckLine> {
        let v: Value = serde_json::from_str(line).ok()?;
        Some(AckLine {
            b: v["b"].as_str()?.to_string(),
            c: v["c"].as_u64()?,
            id: v["id"].as_str()?.to_string(),
            v: v["v"]
                .as_object()?
                .iter()
                .map(|(k, x)| (k.clone(), x.as_str().unwrap_or_default().to_string()))
                .collect(),
            e: v["e"].as_array().cloned().unwrap_or_default(),
        })
    }
}

/// An acknowledged commit: identity, versions written, entries appended.
type Acked = (String, BTreeMap<String, String>, Vec<Value>);

struct Driver {
    url: String,
    scope: Scope,
    dir: std::path::PathBuf,
    workers: Vec<Option<Child>>,
    seeds: Vec<u64>,
    rng: Rng,
    rt: tokio::runtime::Runtime,
    backend: PgBackend,
    /// Bytes consumed per ack log.
    consumed: Vec<u64>,
    /// Acknowledged commits not yet verified: (branch, cseq) → (commit id, versions, entries).
    acked: BTreeMap<(String, u64), Acked>,
    /// Acknowledged commits read from the logs in all.
    acked_total: u64,
    /// Highest cseq verified per branch.
    verified: BTreeMap<String, u64>,
    /// Commits present without acknowledgement (their acknowledgement was lost in a crash).
    unacked: u64,
    findings: Vec<Value>,
    events: Vec<Value>,
    verbose: bool,
}

impl Driver {
    fn spawn(&mut self, i: usize) -> Result<(), BpaError> {
        let exe = std::env::current_exe().map_err(err)?;
        self.seeds[i] += 100;
        let child = Command::new(exe)
            .args([
                "worker",
                "--database",
                &self.url,
                "--tenant",
                &self.scope.tenant,
                "--vae",
                &self.scope.vae,
                "--authority",
                &self.scope.authority,
                "--ack",
                &self.dir.join(format!("ack-{i}.log")).to_string_lossy(),
                "--seed",
                &self.seeds[i].to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(err)?;
        self.workers[i] = Some(child);
        Ok(())
    }

    fn kill(&mut self, i: usize) {
        if let Some(mut c) = self.workers[i].take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Respawn workers that died (a storage error after a power loss) or were killed.
    fn respawn_dead(&mut self) -> Result<u64, BpaError> {
        let mut n = 0;
        for i in 0..self.workers.len() {
            let dead = match &mut self.workers[i] {
                None => true,
                Some(c) => c.try_wait().map_err(err)?.is_some(),
            };
            if dead {
                self.workers[i] = None;
                self.spawn(i)?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn power_loss(&self, stop: &str, start: &str) -> Result<f64, BpaError> {
        let t = Instant::now();
        let shell = |cmd: &str| {
            Command::new("sh")
                .args(["-c", cmd])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(err)
        };
        if !shell(stop)?.success() {
            return Err(err(format!("the stop command failed: {stop}")));
        }
        if !shell(start)?.success() {
            return Err(err(format!("the start command failed: {start}")));
        }
        for _ in 0..600 {
            let ok = Command::new("pg_isready")
                .args(["-h", "127.0.0.1", "-q"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok(t.elapsed().as_secs_f64());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(err("PostgreSQL did not come back within 30 s"))
    }

    /// Read the lines the workers appended since the last check (by byte offset; a line a
    /// crash cut short is skipped, the next worker ends it with a newline).
    fn collect_acks(&mut self) -> Result<u64, BpaError> {
        use std::io::{Read, Seek, SeekFrom};
        let mut n = 0;
        for i in 0..self.workers.len() {
            let path = self.dir.join(format!("ack-{i}.log"));
            let Ok(mut f) = std::fs::File::open(&path) else {
                continue;
            };
            f.seek(SeekFrom::Start(self.consumed[i])).map_err(err)?;
            let mut text = String::new();
            f.read_to_string(&mut text).map_err(err)?;
            let complete = text.rfind('\n').map(|p| p + 1).unwrap_or(0);
            for line in text[..complete].lines() {
                let Some(a) = AckLine::parse(line) else {
                    continue;
                };
                self.acked.insert((a.b, a.c), (a.id, a.v, a.e));
                n += 1;
                self.acked_total += 1;
            }
            self.consumed[i] += complete as u64;
        }
        Ok(n)
    }

    /// The recovery check after an event.
    fn check(&mut self, event: usize, sequences: bool) -> Result<Value, BpaError> {
        let t = Instant::now();
        trace(self.verbose, "check: open the VAE");
        let mut adapter = Adapter::postgres_existing(
            &self.url,
            &self.scope.tenant,
            &self.scope.vae,
            &self.scope.authority,
        )?;
        let mut heads: BTreeMap<&str, ubos_store::BranchRef> = BTreeMap::new();
        for b in BRANCHES {
            let head = adapter
                .engine_mut()
                .store
                .branch(b)?
                .ok_or_else(|| err(format!("branch {b} missing")))?;
            heads.insert(b, head);
        }
        trace(self.verbose, "check: collect acknowledgements");
        let new_acks = self.collect_acks()?;
        let mut checked = 0u64;
        let verbose = self.verbose;
        let mut finding = |what: &str, detail: String| {
            if verbose {
                eprintln!("  finding {what}: {detail}");
            }
            self.findings
                .push(json!({"event": event, "check": what, "detail": detail}));
        };
        for b in BRANCHES {
            let head = heads[b].clone();
            let first = self.verified.get(b).copied().unwrap_or(match &head.parent {
                Some(p) => p.1,
                None => 0,
            }) + 1;
            for c in first..=head.head_cseq {
                checked += 1;
                let id = adapter.engine_mut().store.commit_at(b, c)?;
                let Some(id) = id else {
                    finding(
                        "dense_cseq",
                        format!("{b}: no commit at cseq {c} (head {})", head.head_cseq),
                    );
                    continue;
                };
                let rec = adapter.engine_mut().store.commit(id)?;
                match self.acked.get(&(b.to_string(), c)) {
                    Some((cid, versions, _)) => {
                        if rec["commit_id"].as_str() != Some(cid.as_str()) {
                            finding(
                                "identity",
                                format!(
                                    "{b}@{c}: acknowledged {cid}, store holds {}",
                                    rec["commit_id"]
                                ),
                            );
                        }
                        for (o, v) in versions {
                            if adapter.engine_mut().store.version(v).is_err() {
                                finding(
                                    "version",
                                    format!("{b}@{c}: acknowledged version {v} of {o} missing"),
                                );
                            }
                        }
                    }
                    None => self.unacked += 1,
                }
            }
            let bid = self
                .rt
                .block_on(self.backend.branch(b))?
                .map(|r| r.branch_id)
                .ok_or_else(|| err(format!("no kernel branch {b}")))?;
            trace(
                verbose,
                &format!(
                    "check: {b} commits {first}..={} checked; kernel branch",
                    head.head_cseq
                ),
            );
            if head.head_cseq >= first {
                trace(verbose, "check: verify_roots");
                // roots are per commit: the commits added since the last check
                let r = verify_roots(adapter.engine_mut(), b, first, head.head_cseq)?;
                for f in r.findings {
                    finding(
                        "verify_roots",
                        format!("{b}: {} {} {}", f.check, f.key, f.detail),
                    );
                }
            }
            if sequences {
                trace(verbose, "check: verify_indexes and verify_sequences");
                // the index verifier rebuilds a branch's rows from all its commits
                let r = self
                    .rt
                    .block_on(verify_indexes(&self.backend, bid, 0, u64::MAX >> 1))?;
                for f in r.findings {
                    finding(
                        "verify_indexes",
                        format!("{b}: {} {} {}", f.check, f.key, f.detail),
                    );
                }
                let r = self.rt.block_on(verify_sequences(&self.backend, bid))?;
                for f in r.findings {
                    finding(
                        "verify_sequences",
                        format!("{b}: {} {} {}", f.check, f.key, f.detail),
                    );
                }
            }
            self.verified.insert(b.to_string(), head.head_cseq);
        }
        // acknowledgements of verified commits are no longer needed, except their entries
        // until the next ledger check
        let verified = self.verified.clone();
        if sequences {
            self.acked
                .retain(|(b, c), _| verified.get(b).is_none_or(|v| c > v));
        } else {
            self.acked
                .retain(|(b, c), (_, _, e)| verified.get(b).is_none_or(|v| c > v) || !e.is_empty());
        }
        // every acknowledged entry is in its ledger
        let acked_entries: Vec<(String, Value)> = self
            .acked
            .values()
            .flat_map(|(_, _, e)| e.iter().cloned())
            .map(|e| {
                (
                    e["key"].as_str().unwrap_or_default().to_string(),
                    e["id"].clone(),
                )
            })
            .collect();
        if sequences && !acked_entries.is_empty() {
            for b in BRANCHES {
                for key in KEYS {
                    let r = adapter.execute(
                        "read_ledger",
                        &json!({"branch": b, "class": "StockMove", "ledger_key": {"item": key}}),
                    )?;
                    let ids: Vec<Value> = r["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|e| e["entry_id"].clone())
                        .collect();
                    let seqs: Vec<u64> = r["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e["seq"].as_u64())
                        .collect();
                    if seqs.iter().enumerate().any(|(i, s)| *s != i as u64 + 1) {
                        finding("ledger_gap", format!("{b} {key}: sequences {seqs:?}"));
                    }
                    if b == "main" {
                        for (k, id) in &acked_entries {
                            if k == key && !ids.contains(id) {
                                // entries appended on the branch live there; on main every main append must be present
                                let on_branch = self.acked.iter().any(|((ab, _), (_, _, e))| {
                                    ab != "main" && e.iter().any(|x| &x["id"] == id)
                                });
                                if !on_branch {
                                    finding(
                                        "lost_entry",
                                        format!("{key}: acknowledged entry {id} missing"),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(json!({"new_acks": new_acks, "commits_checked": checked, "ms": t.elapsed().as_millis()}))
    }
}

fn run(args: &[String]) -> Result<bool, BpaError> {
    let url = opt(args, "--database").ok_or_else(|| err("usage: ubos-fault --database <url> [--events N] [--workers W] [--power-loss] [--cluster \"16 main\"] [--report FILE]"))?;
    let events: usize = opt(args, "--events")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let n_workers: usize = opt(args, "--workers")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let power_loss = args.iter().any(|a| a == "--power-loss");
    let pg_stop =
        opt(args, "--pg-stop").unwrap_or_else(|| "pg_ctlcluster 16 main stop -m immediate".into());
    let pg_start = opt(args, "--pg-start").unwrap_or_else(|| {
        "su postgres -c \"/usr/lib/postgresql/16/bin/pg_ctl -D /var/lib/postgresql/16/main -o '-c config_file=/etc/postgresql/16/main/postgresql.conf' -w -l /var/log/postgresql/postgresql-16-main.log start\"".into()
    });
    let verify_every: usize = opt(args, "--verify-every")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let verbose = args.iter().any(|a| a == "--verbose");
    let dir = std::path::PathBuf::from(
        opt(args, "--dir").unwrap_or_else(|| format!("/tmp/ubos-fault-{}", std::process::id())),
    );
    std::fs::create_dir_all(&dir).map_err(err)?;
    let start = Instant::now();
    // the VAE and its dataset
    let mut adapter = Adapter::postgres(&url)?;
    let scope = adapter
        .engine_mut()
        .store
        .scope()
        .ok_or_else(|| err("the adapter's store has no scope"))?;
    adapter.execute("set_now", &json!({"now": "2026-03-15T10:00:00.000000Z"}))?;
    adapter.execute("load_genesis", &json!({"classes": classes()}))?;
    let creates: Vec<Value> = (0..OBJECTS)
        .map(|o| {
            json!({"op": "create", "object_id": uid(o), "class": "Item", "slug": format!("i{o}"),
                        "body": {"a": "0", "b": "0", "c": "0"}, "valid": {"from": "2026-01-01"}})
        })
        .collect();
    adapter.execute("commit", &json!({"branch": "main", "changes": creates}))?;
    adapter.execute(
        "create_branch",
        &json!({"name": "feature/x", "from": "main"}),
    )?;
    drop(adapter);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let backend = rt.block_on(PgBackend::connect(&url, scope.clone()))?;
    let server: String = rt
        .block_on(
            sqlx::query_scalar::<_, String>("SELECT current_setting('server_version')")
                .fetch_one(backend.pool()),
        )
        .map_err(err)?;
    let mut d = Driver {
        url: url.clone(),
        scope,
        dir: dir.clone(),
        workers: (0..n_workers).map(|_| None).collect(),
        seeds: (0..n_workers as u64).collect(),
        rng: Rng(0x5EED_F0A1_7000_0001),
        rt,
        backend,
        consumed: vec![0; n_workers],
        acked: BTreeMap::new(),
        acked_total: 0,
        verified: BTreeMap::new(),
        unacked: 0,
        findings: Vec::new(),
        events: Vec::new(),
        verbose,
    };
    for i in 0..n_workers {
        d.spawn(i)?;
    }
    let mut kills = 0u64;
    let mut losses = 0u64;
    let mut restart_secs = 0.0;
    for e in 1..=events {
        std::thread::sleep(Duration::from_millis(30 + d.rng.below(170)));
        let kind = if power_loss && e % 2 == 0 {
            "power_loss"
        } else {
            "sigkill"
        };
        let mut detail = json!({});
        trace(d.verbose, &format!("event {e}: {kind}"));
        if kind == "power_loss" {
            let secs = d.power_loss(&pg_stop, &pg_start)?;
            trace(d.verbose, "power loss: server back");
            restart_secs += secs;
            losses += 1;
            // the workers lost their connections: they exit on the next storage error
            for i in 0..n_workers {
                d.kill(i);
            }
            detail["restart_ms"] = json!((secs * 1000.0).round() as u64);
        } else {
            // the node crashes: every worker process is killed at this instant
            for i in 0..n_workers {
                d.kill(i);
            }
            kills += 1;
            detail["killed"] = json!(n_workers as u64);
        }
        let full = e % verify_every == 0 || e == events;
        let before = d.findings.len();
        let check = d.check(e, full)?;
        trace(d.verbose, "respawn");
        let respawned = d.respawn_dead()?;
        if d.verbose || d.findings.len() > before {
            eprintln!(
                "event {e} {kind}: {} new acks, {} commits checked, {} findings",
                check["new_acks"],
                check["commits_checked"],
                d.findings.len() - before
            );
        }
        if e % 100 == 0 || e == events {
            eprintln!(
                "{e}/{events}: {} acknowledged commits, {} without acknowledgement, {} findings, {:.0} s",
                d.acked_total,
                d.unacked,
                d.findings.len(),
                start.elapsed().as_secs_f64()
            );
        }
        if e % 1000 == 0 || e == events || !d.findings.is_empty() && d.findings.len() - before > 0 {
            d.events.push(json!({"event": e, "kind": kind, "detail": detail, "respawned": respawned, "check": check,
                                 "acknowledged": d.acked_total, "unacknowledged": d.unacked, "findings": d.findings.len()}));
        }
        if d.findings.len() >= 20 {
            break;
        }
    }
    for i in 0..n_workers {
        d.kill(i);
    }
    let _ = d.collect_acks();
    let final_check = d.check(events + 1, true)?;
    let passed = d.findings.is_empty();
    let mut report = json!({
        "suite": "SUITE-DVM-FAULT", "item": "VER-FAULT-0001", "criterion": "EXIT-1-06", "decision": "DEC-061",
        "backend": "postgresql", "server_version": server, "scope": {"tenant": d.scope.tenant, "vae": d.scope.vae},
        "workload": "WL-MIXED-OLTP (80% commits of 1–2 updates against the versions read, half with a ledger append; 20% reads)",
        "workers": n_workers, "events": {"total": events, "sigkill": kills, "power_loss": losses,
                                         "postgres_restart_ms": (restart_secs * 1000.0).round() as u64},
        "acknowledged_commits": d.acked_total, "commits_without_acknowledgement": d.unacked,
        "lost_acknowledged_commits": d.findings.iter().filter(|f| f["check"] == "lost_commit" || f["check"] == "identity" || f["check"] == "dense_cseq").count(),
        "partial_commits": d.findings.iter().filter(|f| f["check"] == "version" || f["check"] == "lost_entry").count(),
        "verifier_findings": d.findings.iter().filter(|f| f["check"].as_str().is_some_and(|c| c.starts_with("verify_") || c == "ledger_gap")).count(),
        "findings": d.findings, "samples": d.events, "final_check": final_check,
        "passed": passed, "ms": start.elapsed().as_millis() as u64,
        "environment": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
    });
    let seed: [u8; 32] = ubos_crypto::random_bytes::<32>().unwrap_or([7u8; 32]);
    let body = canonical(&report)?;
    let sig = ubos_crypto::ed25519_sign(&seed, body.as_bytes());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    report["signature"] = json!({"alg": "ed25519", "key": hex(&ubos_crypto::ed25519_public(&seed)), "sig": hex(&sig)});
    let text = serde_json::to_string_pretty(&report).map_err(err)?;
    match opt(args, "--report") {
        Some(p) => {
            if let Some(parent) = std::path::Path::new(&p).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(p, text + "\n").map_err(err)?;
        }
        None => println!("{text}"),
    }
    eprintln!(
        "fault suite: {} events ({kills} node crashes by SIGKILL, {losses} power losses), {} acknowledged commits, {} without acknowledgement, {} findings: {}",
        events,
        d.acked_total,
        d.unacked,
        report["findings"].as_array().map(Vec::len).unwrap_or(0),
        if passed { "pass" } else { "FAIL" }
    );
    Ok(passed)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("worker") {
        std::process::exit(worker(&args[1..]));
    }
    match run(&args) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("ubos-fault: {e}");
            std::process::exit(2);
        }
    }
}
