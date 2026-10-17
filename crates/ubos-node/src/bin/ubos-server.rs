//! `ubos-server` — the single-node Server over UBTP (PH-1 step 14b, DEC-075; configuration
//! and bootstrap of step 16a, DEC-077).
//!
//! ```text
//! ubos-server [serve] --config ubos.toml [--genesis FILE]
//! ubos-server bootstrap --config ubos.toml [--genesis FILE] [--new-invitation]
//! ubos-server check-config ubos.toml
//! ubos-server config-schema
//! ubos-server genesis-verify | genesis-sign [DIR]
//! ubos-server diagnostics --config ubos.toml [--out FILE]
//! ubos-server backup (base | point NAME | list | prune | wal-push PATH FILE | wal-fetch FILE PATH) --config ubos.toml
//! ubos-server restore --config ubos.toml --into DIR (--point NAME | --time T | --latest) [--port N] [--hba-from FILE]
//! ubos-server verify --config ubos.toml [--database URL] [--point NAME | --expected FILE] [--last N]
//! ubos-server upgrade --config ubos.toml --binary NEW [--service UNIT]
//! ubos-server migrate --config ubos.toml | version
//!
//! ubos-server [--listen 127.0.0.1:8080] [--database URL | --memory [--genesis FILE]]
//!             [--tenant SLUG] [--vae AUTHORITY] [--preset dev|server]
//!             [--admin USERNAME:PASSWORD]                        (development, no file)
//! ```
//!
//! With `--config` the node reads `ubos.toml` (DSN-NOD-603): an invalid file stops the
//! start with every error named; SIGHUP reads it again and applies its reloadable keys
//! (DSN-NOD-604), keeping the running configuration when the file is invalid. Without a
//! file, the flags make the configuration.
//!
//! `bootstrap` (DSN-NOD-602) checks the prerequisites (PostgreSQL version, extensions,
//! clock, root key), applies the migrations, creates the default tenant and VAE and prints
//! a one-time administrator invitation link; it can run again and completes what is
//! missing. `--genesis FILE` declares the classes of a JSON list in the memory VAE, or in
//! the default VAE when it is created; a `{classes, branches}` object also creates the
//! branches the VAE lacks.
//!
//! `diagnostics` (DSN-NOD-615, step 16d) writes the offline part of a diagnostic bundle
//! (versions, host, configuration, prerequisites, database counts) for a node that does
//! not start; the running node serves the full bundle at `POST /api/v1/admin/diagnostics`.
//!
//! `backup`, `restore`, `verify` and `upgrade` (DSN-NOD-620, -623, -633; step 16e) are
//! described in `ubos_node::backup`: sealed base backups and WAL archive under
//! `backup.dir`, named restore points with recorded heads, a restore prepared into a new
//! data directory, its verification, and a one-command upgrade with backup and report.

use serde_json::{Value, json};
use std::sync::Arc;
use ubos_dvm::types::BpaError;
use ubos_node::config::Config;
use ubos_node::server::{NodeVaes, Preset, Server, VaeHandle, memory_vae_with};
use ubos_node::services::{MemEventLog, MemJobQueue};

fn fail(e: impl std::fmt::Display) -> ! {
    eprintln!("{e}");
    std::process::exit(1)
}

/// The errors of an invalid configuration, one per line.
fn report_invalid(e: &BpaError) {
    match e.details.as_ref().and_then(|d| d["errors"].as_array()) {
        Some(list) => {
            for x in list {
                eprintln!(
                    "  {}: {}",
                    x["key"].as_str().unwrap_or("(document)"),
                    x["message"].as_str().unwrap_or("")
                );
            }
        }
        None => eprintln!("  {e}"),
    }
}

/// The configuration from `--config`, or from the flags of a development start.
fn load(
    opt: &dyn Fn(&str) -> Option<String>,
    flag: &dyn Fn(&str) -> bool,
) -> (Config, Option<std::path::PathBuf>) {
    if let Some(p) = opt("--config") {
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| fail(format!("{p}: {e}")));
        match Config::parse(&text) {
            Ok(c) => return (c, Some(p.into())),
            Err(e) => {
                eprintln!("{p}: the configuration is invalid:");
                report_invalid(&e);
                std::process::exit(2)
            }
        }
    }
    let q = |s: String| serde_json::to_string(&s).unwrap_or_default();
    let mut t = String::from("version = 1\n[server]\n");
    t += &format!(
        "listen = {}\n",
        q(opt("--listen").unwrap_or_else(|| "127.0.0.1:8080".into()))
    );
    t += &format!(
        "preset = {}\n",
        q(opt("--preset").unwrap_or_else(|| "dev".into()))
    );
    t += &format!(
        "tenant = {}\n",
        q(opt("--tenant").unwrap_or_else(|| "acme".into()))
    );
    t += &format!(
        "vae = {}\n",
        q(opt("--vae").unwrap_or_else(|| "acme".into()))
    );
    if let (Some(db), false) = (opt("--database"), flag("--memory")) {
        t += &format!("[database]\nurl = {}\n", q(db));
        if std::env::var("UBOS_ROOT_KEK").is_ok() {
            t += "[keys]\nroot_kek = \"env:UBOS_ROOT_KEK\"\n";
        } else if let Ok(f) = std::env::var("UBOS_ROOT_KEK_FILE") {
            t += &format!("[keys]\nroot_kek = {}\n", q(format!("file:{f}")));
        }
    }
    match Config::parse(&t) {
        Ok(c) => (c, None),
        Err(e) => {
            eprintln!("the options make an invalid configuration:");
            report_invalid(&e);
            std::process::exit(2)
        }
    }
}

/// The database URL and root key of a configuration (references resolved).
fn database(cfg: &Config) -> Option<(String, [u8; 32])> {
    let url = cfg.text("database.url")?;
    let url =
        ubos_node::config::resolve(url).unwrap_or_else(|e| fail(format!("database.url: {e}")));
    let root = match cfg.text("keys.root_kek") {
        Some(r) => {
            let hex = ubos_node::config::resolve(r)
                .unwrap_or_else(|e| fail(format!("keys.root_kek: {e}")));
            ubos_dvm::crypto::unhex32(&json!(hex), "keys.root_kek")
                .unwrap_or_else(|_| fail("keys.root_kek: 64 hex digits"))
        }
        None => ubos_node::root_kek().unwrap_or_else(|e| fail(e)),
    };
    Some((url, root))
}

/// The `--genesis` file: a list of class declarations, or `{classes, value_types?,
/// branches}` where the branches are created from `main` when missing (suites and `forge
/// dev` run uncertified logic on them).
fn classes(opt: &dyn Fn(&str) -> Option<String>) -> Option<Value> {
    opt("--genesis").map(|p| {
        let t = std::fs::read_to_string(&p).unwrap_or_else(|e| fail(format!("{p}: {e}")));
        let v: Value = serde_json::from_str(&t).unwrap_or_else(|e| fail(format!("{p}: {e}")));
        if v.is_array() {
            json!({"classes": v, "branches": []})
        } else {
            v
        }
    })
}

/// Create the genesis branches a VAE lacks.
fn genesis_branches(
    call: &mut dyn FnMut(&str, &Value) -> Result<Value, BpaError>,
    genesis: &Value,
) -> Result<(), BpaError> {
    for b in genesis["branches"].as_array().into_iter().flatten() {
        let name = b.as_str().unwrap_or_default();
        if call("branch_info", &json!({"branch": name})).is_err() {
            call("create_branch", &json!({"name": name, "from": "main"}))?;
        }
    }
    Ok(())
}

/// Provision the default tenant and VAE (DSN-NOD-402: resumable steps, genesis Buks,
/// projection, creation event), then declare the `--genesis` classes in a VAE that lacks
/// them; returns the provisioning report.
fn ensure_tenant(
    node: &ubos_node::Node,
    tenant: &str,
    authority: &str,
    classes: Option<&Value>,
    admin: Option<&str>,
) -> Result<Value, BpaError> {
    let report = node.provision(tenant, authority, admin)?;
    if let Some(c) = classes {
        let t = node
            .tenant(tenant)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "tenant"))?;
        let mut vae = node.open_vae(&t, authority)?;
        let fresh: Vec<Value> = c["classes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| {
                vae.engine_mut()
                    .registry
                    .decl(d["name"].as_str().unwrap_or_default())
                    .is_err()
            })
            .cloned()
            .collect();
        if !fresh.is_empty() {
            let mut g = json!({"classes": fresh});
            if let Some(vt) = c.get("value_types").filter(|v| !v.is_null()) {
                g["value_types"] = vt.clone();
            }
            vae.call("load_genesis", &g)?;
        }
        genesis_branches(&mut |op, input| vae.call(op, input), c)?;
    }
    Ok(report)
}

/// `bootstrap` (DSN-NOD-602): checks, migrations, tenant, VAE, administrator invitation.
fn bootstrap(cfg: &Config, classes: Option<Value>, new_invitation: bool) -> i32 {
    let Some((url, root)) = database(cfg) else {
        eprintln!("bootstrap needs database.url");
        return 2;
    };
    let mut report = Vec::new();
    let mut ok = true;
    for c in ubos_node::Node::prerequisites(&url, ubos_node::EXTENSIONS) {
        ok &= c["ok"] == json!(true);
        report.push(c);
    }
    let dev_root = root == *b"ubos-development-root-kek-000000";
    let server = cfg.text("server.preset") == Some("server");
    let key_ok = !(dev_root && server);
    report.push(json!({"check": "root key", "ok": key_ok,
        "detail": if dev_root { "the development root key" } else { "a root key from keys.root_kek" },
        "fix": if key_ok { Value::Null } else { json!("set keys.root_kek to a reference to 64 hex digits (env: or file:)") }}));
    ok &= key_ok;
    let print = |r: &[Value]| {
        for c in r {
            let mark = if c["ok"] == json!(true) {
                "ok  "
            } else {
                "FAIL"
            };
            println!(
                "{mark} {}: {}",
                c["check"].as_str().unwrap_or("?"),
                c["detail"].as_str().unwrap_or("")
            );
            if let Some(f) = c["fix"].as_str() {
                println!("     fix: {f}");
            }
        }
    };
    if !ok {
        print(&report);
        eprintln!("bootstrap stopped: a prerequisite is missing");
        return 2;
    }
    let node = match ubos_node::Node::connect_with_root(&url, root) {
        Ok(n) => n,
        Err(e) => {
            print(&report);
            eprintln!("bootstrap stopped: {e}");
            return 2;
        }
    };
    report.push(json!({"check": "migrations", "ok": true, "detail": "applied"}));
    let (tenant, authority) = (
        cfg.text("server.tenant").unwrap_or("acme"),
        cfg.text("server.vae").unwrap_or("acme"),
    );
    let user = cfg.text("bootstrap.admin_username").unwrap_or("admin");
    let prov = match ensure_tenant(&node, tenant, authority, classes.as_ref(), Some(user)) {
        Ok(p) => p,
        Err(e) => {
            print(&report);
            eprintln!("bootstrap stopped: {e}");
            return 2;
        }
    };
    for st in prov["steps"].as_array().into_iter().flatten() {
        report.push(
            json!({"check": format!("tenant {}", st["step"].as_str().unwrap_or("?")), "ok": true,
            "detail": format!("{tenant}/{authority}: {}", st["status"].as_str().unwrap_or(""))}),
        );
    }
    let listen = cfg.text("server.listen").unwrap_or("127.0.0.1:8080");
    let link = |inv: &Value| {
        format!(
            "http://{listen}/api/v1/identity/invitations/{}",
            inv["token"].as_str().unwrap_or("")
        )
    };
    let invited = (|| -> Result<Value, BpaError> {
        let inv = &prov["invitation"];
        if inv["token"].is_string() {
            return Ok(json!({"check": "administrator", "ok": true,
                "detail": format!("{user} invited until {}", inv["expires_at"].as_str().unwrap_or("")), "link": link(inv)}));
        }
        let t = node
            .tenant(tenant)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "tenant"))?;
        let id = node.identity(&t)?;
        let active = id
            .principals()?
            .iter()
            .any(|p| p["role"] == "admin" && p["kind"] == "human" && p["state"] == "active");
        if active || !new_invitation {
            return Ok(json!({"check": "administrator", "ok": true,
                "detail": if active { "an administrator exists" } else { "an invitation is pending (--new-invitation issues another)" }}));
        }
        let inv = id.invite(user, "admin", 24 * 3600)?;
        Ok(json!({"check": "administrator", "ok": true,
            "detail": format!("{user} invited until {}", inv["expires_at"].as_str().unwrap_or("")), "link": link(&inv)}))
    })();
    match invited {
        Ok(v) => report.push(v),
        Err(e) => {
            print(&report);
            eprintln!("bootstrap stopped: {e}");
            return 2;
        }
    }
    print(&report);
    if let Some(link) = report.last().and_then(|r| r["link"].as_str()) {
        println!("\nThe administrator sets a password once, within 24 hours:");
        println!(
            "  curl -X POST {link} -H 'content-type: application/json' -d '{{\"password\": \"…\"}}'"
        );
    }
    println!("bootstrap complete");
    0
}

fn print(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

/// `backup base | point NAME | wal-push PATH FILE | wal-fetch FILE PATH | list | prune`
/// (DSN-NOD-620, -623; step 16e).
fn backup_cmd(
    args: &[String],
    opt: &dyn Fn(&str) -> Option<String>,
    flag: &dyn Fn(&str) -> bool,
) -> i32 {
    use ubos_node::backup;
    let (cfg, _) = load(opt, flag);
    let target = backup::Target::required(&cfg).unwrap_or_else(|e| fail(e));
    let arg = |i: usize| {
        args.get(i)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| fail("backup: a missing argument"))
    };
    let owner = || {
        let (url, _) = database(&cfg).unwrap_or_else(|| fail("database.url is not configured"));
        let db = ubos_node::db::Db::owner(&url).unwrap_or_else(|e| fail(e));
        (url, db)
    };
    let r = match args.get(1).map(String::as_str) {
        Some("base") => {
            let (url, db) = owner();
            backup::base_backup(&db, &url, &target)
        }
        Some("point") => backup::restore_point(&owner().1, &target, &arg(2)),
        Some("wal-push") => {
            // PostgreSQL's archive_command: a non-zero exit keeps the segment for a retry
            return match backup::wal_push(&target, std::path::Path::new(&arg(2)), &arg(3)) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("ubos-server backup wal-push: {e}");
                    1
                }
            };
        }
        Some("wal-fetch") => {
            return match backup::wal_fetch(&target, &arg(2), std::path::Path::new(&arg(3))) {
                Ok(()) => 0,
                Err(_) => 1,
            };
        }
        Some("list") => (|| {
            let bases = backup::bases(&target)?;
            let points: Vec<Value> = backup::points(&target)?
                .into_iter()
                .map(|p| json!({"name": p["name"], "at": p["at"], "wal_file": p["wal_file"]}))
                .collect();
            let chain = match bases.first().and_then(|b| b["start_wal"].as_str()) {
                Some(s) => backup::chain_end(&target, s)?,
                None => None,
            };
            Ok(json!({"base_backups": bases, "points": points, "wal_chain_end": chain}))
        })(),
        Some("prune") => backup::prune(&target),
        _ => fail(
            "backup base | point NAME | wal-push PATH FILE | wal-fetch FILE PATH | list | prune",
        ),
    };
    match r {
        Ok(v) => {
            print(&v);
            0
        }
        Err(e) => {
            eprintln!("ubos-server backup: {e}");
            if let Some(d) = &e.details {
                print(d);
            }
            1
        }
    }
}

/// `restore --into DIR (--point NAME | --time T | --latest) [--port N] [--hba-from FILE]`
/// (DSN-NOD-620).
fn restore_cmd(opt: &dyn Fn(&str) -> Option<String>, flag: &dyn Fn(&str) -> bool) -> i32 {
    use ubos_node::backup::{self, Stop};
    let (cfg, _) = load(opt, flag);
    let target = backup::Target::required(&cfg).unwrap_or_else(|e| fail(e));
    let into = opt("--into").unwrap_or_else(|| fail("restore --into DIR"));
    let stop = match (opt("--point"), opt("--time"), flag("--latest")) {
        (Some(p), None, false) => Stop::Point(p),
        (None, Some(t), false) => Stop::Time(t),
        (None, None, true) => Stop::Latest,
        _ => fail("restore: one of --point NAME, --time T or --latest"),
    };
    let port = opt("--port").and_then(|p| p.parse().ok()).unwrap_or(5433);
    let hba = opt("--hba-from");
    match backup::prepare_restore(
        &target,
        &stop,
        std::path::Path::new(&into),
        port,
        hba.as_deref().map(std::path::Path::new),
    ) {
        Ok(v) => {
            print(&v);
            eprintln!(
                "start the restored server with: {}\nthen verify it with: ubos-server verify --config FILE --database \"host=... port={port} ...\"{}",
                v["start"].as_str().unwrap_or_default(),
                match &stop {
                    Stop::Point(p) => format!(" --point {p}"),
                    _ => String::new(),
                }
            );
            0
        }
        Err(e) => {
            eprintln!("ubos-server restore: {e}");
            if let Some(d) = &e.details {
                print(d);
            }
            1
        }
    }
}

/// `verify [--database URL] [--point NAME | --expected FILE] [--last N] [--report FILE]`:
/// the heads against a restore point's, the DVM roots and the indexes of the last commits
/// of every VAE (DSN-NOD-620 acceptance 1, DSN-NOD-633).
fn verify_cmd(opt: &dyn Fn(&str) -> Option<String>, flag: &dyn Fn(&str) -> bool) -> i32 {
    use ubos_node::backup;
    let (cfg, _) = load(opt, flag);
    let (url, root) = database(&cfg).unwrap_or_else(|| fail("verify needs database.url"));
    let url = opt("--database").unwrap_or(url);
    let target = backup::Target::of(&cfg).unwrap_or_else(|e| fail(e));
    let expected = match (opt("--point"), opt("--expected")) {
        (Some(p), _) => {
            let t = target
                .as_ref()
                .unwrap_or_else(|| fail("--point needs backup.dir and backup.key"));
            let pts = backup::points(t).unwrap_or_else(|e| fail(e));
            Some(
                pts.into_iter()
                    .find(|x| x["name"] == json!(p))
                    .unwrap_or_else(|| fail(format!("no restore point {p}")))["heads"]
                    .clone(),
            )
        }
        (None, Some(f)) => Some(
            serde_json::from_str(&std::fs::read_to_string(&f).unwrap_or_else(|e| fail(e)))
                .unwrap_or_else(|e| fail(e)),
        ),
        _ => None,
    };
    let last = opt("--last").and_then(|n| n.parse().ok()).unwrap_or(1000);
    let mut report =
        backup::verify(&url, root, expected.as_ref(), last).unwrap_or_else(|e| fail(e));
    report["point"] = json!(opt("--point"));
    let path = match (opt("--report"), &target) {
        (Some(p), _) => {
            std::fs::write(&p, serde_json::to_vec_pretty(&report).unwrap_or_default())
                .unwrap_or_else(|e| fail(e));
            Some(p)
        }
        (None, Some(t)) => backup::write_report(t, "verify", &report)
            .ok()
            .map(|p| p.display().to_string()),
        _ => None,
    };
    print(
        &json!({"ok": report["ok"], "heads_equal": report["heads_equal"], "vaes": report["checks"].as_array().map_or(0, Vec::len),
        "mismatches": report["mismatches"], "failed": report["checks"].as_array().map(|a| a.iter().filter(|c| c["ok"] != json!(true)).cloned().collect::<Vec<_>>()),
        "report": path}),
    );
    if report["ok"] == json!(true) { 0 } else { 1 }
}

/// `upgrade --binary NEW [--service UNIT] [--last N]` (DSN-NOD-633, NR-OPER-002): one
/// command — check the new binary and the configuration, back up (a base backup and a
/// restore point), stop the service, migrate with the new binary, install it (the old one
/// kept as `.previous`), start the service and verify with the new binary against the
/// point; a failed step after the installation puts the old binary back. The report is
/// written under `backup.dir/reports/`.
fn upgrade_cmd(opt: &dyn Fn(&str) -> Option<String>, flag: &dyn Fn(&str) -> bool) -> i32 {
    use std::process::Command;
    use ubos_node::backup;
    let (cfg, file) = load(opt, flag);
    let file = file.unwrap_or_else(|| fail("upgrade --config FILE"));
    let new = opt("--binary").unwrap_or_else(|| fail("upgrade --binary NEW"));
    let service = opt("--service");
    let last = opt("--last").unwrap_or_else(|| "1000".into());
    let target = backup::Target::required(&cfg)
        .unwrap_or_else(|e| fail(format!("an upgrade backs up first: {e}")));
    let (url, _) = database(&cfg).unwrap_or_else(|| fail("upgrade needs database.url"));
    let exe = std::env::current_exe().unwrap_or_else(|e| fail(e));
    let started = std::time::Instant::now();
    let mut steps: Vec<Value> = Vec::new();
    let run = |c: &mut Command| -> Result<String, String> {
        match c.output() {
            Ok(o) if o.status.success() => {
                Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
            }
            Ok(o) => Err(format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout).trim(),
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => Err(e.to_string()),
        }
    };
    let listen = cfg
        .text("server.listen")
        .unwrap_or("127.0.0.1:8080")
        .to_string();
    let live = || {
        listen
            .parse::<std::net::SocketAddr>()
            .ok()
            .and_then(|a| {
                std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(300)).ok()
            })
            .is_some()
    };
    let finish = |steps: Vec<Value>, ok: bool| -> i32 {
        let report = json!({"ok": ok, "from": env!("CARGO_PKG_VERSION"), "binary": new, "service": service,
            "seconds": started.elapsed().as_secs_f64(), "steps": steps});
        let path = backup::write_report(&target, "upgrade", &report)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        print(&json!({"ok": ok, "report": path, "steps": report["steps"]}));
        if ok { 0 } else { 1 }
    };
    macro_rules! step {
        ($name:expr, $r:expr) => {
            match $r {
                Ok(v) => steps.push(json!({"step": $name, "ok": true, "detail": v})),
                Err(e) => {
                    steps.push(json!({"step": $name, "ok": false, "detail": e}));
                    return finish(steps, false);
                }
            }
        };
    }
    // 1. the new binary and the configuration
    step!("new version", run(Command::new(&new).arg("version")));
    step!(
        "configuration",
        run(Command::new(&new).arg("check-config").arg(&file))
    );
    if service.is_none() && live() {
        step!(
            "node stopped",
            Err::<String, _>(format!(
                "a node listens on {listen}: stop it first, or pass --service UNIT"
            ))
        );
    }
    // 2. the automatic backup and the point to verify against
    let db = ubos_node::db::Db::owner(&url).unwrap_or_else(|e| fail(e));
    step!(
        "base backup",
        backup::base_backup(&db, &url, &target).map_err(|e| e.to_string())
    );
    let point = format!(
        "pre-upgrade-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
    );
    drop(db);
    if let Some(u) = &service {
        step!(
            "service stopped",
            run(Command::new("systemctl").args(["stop", u]))
        );
    }
    let db = ubos_node::db::Db::owner(&url).unwrap_or_else(|e| fail(e));
    step!(
        "restore point",
        backup::restore_point(&db, &target, &point).map_err(|e| e.to_string())
    );
    let point = steps
        .last()
        .and_then(|s| s["detail"]["name"].as_str())
        .unwrap_or(&point)
        .to_string();
    drop(db);
    // 3. migrate with the new binary, then install it
    step!(
        "migrations",
        run(Command::new(&new).args(["migrate", "--config"]).arg(&file))
    );
    let previous = exe.with_extension("previous");
    let same = std::fs::canonicalize(&new).ok() == std::fs::canonicalize(&exe).ok();
    let install = || -> Result<String, String> {
        if same {
            return Ok("the running binary is the new one".into());
        }
        std::fs::copy(&exe, &previous).map_err(|e| e.to_string())?;
        let staged = exe.with_extension("new");
        std::fs::copy(&new, &staged).map_err(|e| e.to_string())?;
        std::fs::rename(&staged, &exe).map_err(|e| e.to_string())?;
        Ok(format!(
            "{} (previous kept as {})",
            exe.display(),
            previous.display()
        ))
    };
    step!("install", install());
    let rollback = |steps: &mut Vec<Value>| {
        if !same && std::fs::copy(&previous, &exe).is_ok() {
            steps.push(json!({"step": "rollback", "ok": true,
                "detail": format!("the previous binary is back; the database is restorable to the point {point}")}));
        }
    };
    // 4. start and verify with the new binary
    if let Some(u) = &service {
        if let Err(e) = run(Command::new("systemctl").args(["start", u])) {
            steps.push(json!({"step": "service started", "ok": false, "detail": e}));
            rollback(&mut steps);
            return finish(steps, false);
        }
        let t0 = std::time::Instant::now();
        while !live() && t0.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        steps.push(json!({"step": "service started", "ok": live(), "detail": listen}));
    }
    let v = run(Command::new(&exe)
        .args(["verify", "--config"])
        .arg(&file)
        .args(["--point", &point, "--last", &last]));
    let ok = v.is_ok();
    let detail = match &v {
        Ok(t) | Err(t) => serde_json::from_str::<Value>(t).unwrap_or(json!(t)),
    };
    steps.push(json!({"step": "verification", "ok": ok, "detail": detail}));
    if !ok {
        rollback(&mut steps);
    }
    finish(steps, ok)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let flag = |k: &str| args.iter().any(|a| a == k);
    match args.first().map(String::as_str) {
        Some("genesis-verify") => {
            // the embedded genesis Buks verify against the trusted publisher key (CI)
            let mut bad = false;
            for b in ubos_node::genesis::embedded().unwrap_or_else(|e| fail(e)) {
                match b.verify() {
                    Ok(()) => println!("ok   {} {} {}", b.name(), b.version(), b.hash),
                    Err(e) => {
                        bad = true;
                        println!("FAIL {} {}: {e}", b.name(), b.version());
                    }
                }
            }
            std::process::exit(if bad { 1 } else { 0 });
        }
        Some("genesis-sign") => {
            // sign the embedded genesis manifests (development publisher key in PH-1)
            let seed = ubos_node::genesis::dev_publisher_seed();
            let dir = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| "crates/ubos-node/genesis".into());
            for n in ["system", "ontology"] {
                let m = std::fs::read_to_string(format!("{dir}/{n}.json"))
                    .unwrap_or_else(|e| fail(format!("{dir}/{n}.json: {e}")));
                let sig = ubos_node::genesis::sign(&m, &seed).unwrap_or_else(|e| fail(e));
                std::fs::write(format!("{dir}/{n}.sig"), sig).unwrap_or_else(|e| fail(e));
                println!("signed {dir}/{n}.json");
            }
            return;
        }
        Some("config-schema") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&ubos_node::config::schema()).unwrap_or_default()
            );
            return;
        }
        Some("check-config") => {
            let p = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| fail("check-config FILE"));
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| fail(format!("{p}: {e}")));
            match Config::parse(&text) {
                Ok(c) => println!("{p}: valid ({})", c.hash()),
                Err(e) => {
                    eprintln!("{p}: the configuration is invalid:");
                    report_invalid(&e);
                    std::process::exit(2)
                }
            }
            return;
        }
        Some("diagnostics") => {
            // the offline part of a diagnostic bundle, for a node that does not start
            // (DSN-NOD-615); the running node serves the full bundle at
            // POST /api/v1/admin/diagnostics
            let started = std::time::Instant::now();
            let (cfg, file) = load(&opt, &flag);
            let mut m = serde_json::Map::new();
            m.insert("versions".into(), ubos_node::diagnostics::versions());
            m.insert("host".into(), ubos_node::diagnostics::host());
            m.insert(
                "configuration".into(),
                json!({"file": file.as_ref().map(|f| f.display().to_string()),
                "hash": cfg.hash(), "values": cfg.values()}),
            );
            if let Some(url) = cfg.text("database.url") {
                match ubos_node::config::resolve(url) {
                    Ok(url) => {
                        m.insert(
                            "prerequisites".into(),
                            json!(ubos_node::Node::prerequisites(&url, ubos_node::EXTENSIONS)),
                        );
                        m.insert(
                            "database".into(),
                            match ubos_node::db::Db::owner(&url) {
                                Ok(db) => ubos_node::diagnostics::database(&db),
                                Err(e) => json!({"error": e.to_string()}),
                            },
                        );
                    }
                    Err(e) => {
                        m.insert("database".into(), json!({"error": e.to_string()}));
                    }
                }
            }
            let bundle = ubos_node::diagnostics::finish(
                m,
                &ubos_node::config::secret_values(&cfg),
                "offline",
                started,
            );
            let text = serde_json::to_string_pretty(&bundle).unwrap_or_default() + "\n";
            match opt("--out") {
                Some(p) => {
                    std::fs::write(&p, text).unwrap_or_else(|e| fail(format!("{p}: {e}")));
                    eprintln!(
                        "diagnostic bundle written to {p} ({} ms, {} redactions)",
                        bundle["manifest"]["duration_ms"], bundle["manifest"]["redactions"]
                    );
                }
                None => print!("{text}"),
            }
            return;
        }
        Some("version") => {
            println!("ubos-server {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("migrate") => {
            let (cfg, _) = load(&opt, &flag);
            let (url, _) = database(&cfg).unwrap_or_else(|| fail("migrate needs database.url"));
            ubos_dvm::store::PgBackend::migrate(&url).unwrap_or_else(|e| fail(e));
            println!("migrations applied");
            return;
        }
        Some("backup") => std::process::exit(backup_cmd(&args, &opt, &flag)),
        Some("restore") => std::process::exit(restore_cmd(&opt, &flag)),
        Some("verify") => std::process::exit(verify_cmd(&opt, &flag)),
        Some("upgrade") => std::process::exit(upgrade_cmd(&opt, &flag)),
        Some("bootstrap") => {
            let (cfg, _) = load(&opt, &flag);
            std::process::exit(bootstrap(&cfg, classes(&opt), flag("--new-invitation")));
        }
        _ => {}
    }
    let (cfg, file) = load(&opt, &flag);
    let listen: std::net::SocketAddr = cfg
        .text("server.listen")
        .unwrap_or("127.0.0.1:8080")
        .parse()
        .unwrap_or_else(|e| fail(format!("server.listen: {e}")));
    let preset = match cfg.text("server.preset") {
        Some("server") => Preset::Server,
        _ => Preset::Dev,
    };
    let tenant = cfg.text("server.tenant").unwrap_or("acme").to_string();
    let authority = cfg.text("server.vae").unwrap_or("acme").to_string();
    let classes = classes(&opt);
    let server = match database(&cfg) {
        None => {
            let events = Arc::new(MemEventLog::default());
            let jobs = Arc::new(MemJobQueue::default());
            let (ev, jb) = (events.clone(), jobs.clone());
            let handle = VaeHandle::spawn(move || {
                let mut v = memory_vae_with(None, Some(ev), Some(jb))?;
                ubos_node::genesis::install_all(&mut v)?;
                if let Some(c) = classes {
                    let mut g = json!({"classes": c["classes"]});
                    if let Some(vt) = c.get("value_types").filter(|v| !v.is_null()) {
                        g["value_types"] = vt.clone();
                    }
                    v.call("load_genesis", &g)?;
                    genesis_branches(&mut |op, input| v.call(op, input), &c)?;
                }
                Ok(v)
            })
            .unwrap_or_else(|e| fail(e));
            let identity = ubos_node::identity::Identity::memory(&tenant, &authority)
                .unwrap_or_else(|e| fail(e));
            Server::single_with(
                handle,
                Arc::new(identity),
                Some(events),
                Some(jobs),
                preset,
                &tenant,
                &authority,
            )
        }
        Some((url, root)) => {
            let node = ubos_node::Node::connect_with_root(&url, root).unwrap_or_else(|e| fail(e));
            match ensure_tenant(&node, &tenant, &authority, classes.as_ref(), None) {
                Ok(r) => {
                    let created = r["steps"].as_array().is_some_and(|a| {
                        a.iter()
                            .any(|x| x["step"] == "register" && x["status"] == "done")
                    });
                    if created {
                        eprintln!("tenant {tenant} with VAE {authority} created");
                    }
                }
                Err(e) => fail(e),
            }
            Server::new(
                Box::new(NodeVaes(Arc::new(node))),
                preset,
                &tenant,
                &authority,
            )
        }
    };
    let who = format!(
        "start:{}",
        std::env::var("USER").unwrap_or_else(|_| "operator".into())
    );
    if let Err(e) = server.configure(cfg, file.clone(), &who) {
        fail(e);
    }
    // the first administrator of the default tenant in development (`--admin` or
    // `UBOS_ADMIN`), when the username is free; a server is bootstrapped instead
    if let Some(a) = opt("--admin").or_else(|| std::env::var("UBOS_ADMIN").ok()) {
        let Some((user, password)) = a.split_once(':') else {
            fail("--admin takes USERNAME:PASSWORD")
        };
        match server
            .identity(&tenant)
            .and_then(|id| id.bootstrap_admin(user, password))
        {
            Ok(Some(did)) => eprintln!("administrator {user} created as {did}"),
            Ok(None) => eprintln!("administrator {user} exists"),
            Err(e) => fail(e),
        }
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    // SIGHUP reads the configuration file again (DSN-NOD-604)
    if file.is_some() {
        let s2 = server.clone();
        rt.spawn(async move {
            let Ok(mut hup) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) else {
                return;
            };
            while hup.recv().await.is_some() {
                let who = format!("sighup:{}", std::env::var("USER").unwrap_or_else(|_| "operator".into()));
                let s3 = s2.clone();
                let r = tokio::task::spawn_blocking(move || s3.reload(&who)).await;
                match r {
                    Ok(Ok(v)) => eprintln!("ubos-server: configuration {} applied; restart required for {}", v["hash"], v["restart_required"]),
                    Ok(Err(e)) => {
                        eprintln!("ubos-server: the configuration file is invalid; the running configuration stays:");
                        report_invalid(&e);
                    }
                    Err(e) => eprintln!("ubos-server: reload: {e}"),
                }
            }
        });
    }
    eprintln!("ubos-server listening on {listen} ({preset:?} preset)");
    if let Err(e) = rt.block_on(server.serve(listen)) {
        fail(e);
    }
}
