//! `ubos-server` — the single-node Server over UBTP (PH-1 step 14b, DEC-075; configuration
//! and bootstrap of step 16a, DEC-077).
//!
//! ```text
//! ubos-server [serve] --config ubos.toml [--genesis FILE]
//! ubos-server bootstrap --config ubos.toml [--genesis FILE] [--new-invitation]
//! ubos-server check-config ubos.toml
//! ubos-server config-schema
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
//! the default VAE when it is created.

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

fn classes(opt: &dyn Fn(&str) -> Option<String>) -> Option<Value> {
    opt("--genesis").map(|p| {
        let t = std::fs::read_to_string(&p).unwrap_or_else(|e| fail(format!("{p}: {e}")));
        serde_json::from_str(&t).unwrap_or_else(|e| fail(format!("{p}: {e}")))
    })
}

/// Create the default tenant and VAE when missing (with the genesis classes); returns
/// whether they were created.
fn ensure_tenant(
    node: &ubos_node::Node,
    tenant: &str,
    authority: &str,
    classes: Option<&Value>,
) -> Result<bool, BpaError> {
    let t = match node.tenant(tenant)? {
        Some(t) => t,
        None => node.create_tenant(tenant)?,
    };
    if node.vae(&t, authority)?.is_some() {
        return Ok(false);
    }
    node.create_vae(&t, authority)?;
    if let Some(c) = classes {
        node.open_vae(&t, authority)?
            .call("load_genesis", &json!({"classes": c}))?;
    }
    Ok(true)
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
    match ensure_tenant(&node, tenant, authority, classes.as_ref()) {
        Ok(created) => report.push(json!({"check": "tenant and VAE", "ok": true,
            "detail": format!("{tenant}/{authority} {}", if created { "created" } else { "exist" })})),
        Err(e) => {
            print(&report);
            eprintln!("bootstrap stopped: {e}");
            return 2;
        }
    }
    let user = cfg.text("bootstrap.admin_username").unwrap_or("admin");
    let invited = (|| -> Result<Value, BpaError> {
        let t = node
            .tenant(tenant)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "tenant"))?;
        let id = node.identity(&t)?;
        let admins: Vec<Value> = id
            .principals()?
            .into_iter()
            .filter(|p| p["role"] == "admin" && p["kind"] == "human")
            .collect();
        let active = admins.iter().any(|p| p["state"] == "active");
        if active || (!admins.is_empty() && !new_invitation) {
            return Ok(json!({"check": "administrator", "ok": true,
                "detail": if active { "an administrator exists" } else { "an invitation is pending (--new-invitation issues another)" }}));
        }
        let inv = id.invite(user, "admin", 24 * 3600)?;
        let listen = cfg.text("server.listen").unwrap_or("127.0.0.1:8080");
        Ok(json!({"check": "administrator", "ok": true,
            "detail": format!("{user} invited until {}", inv["expires_at"].as_str().unwrap_or("")),
            "link": format!("http://{listen}/api/v1/identity/invitations/{}", inv["token"].as_str().unwrap_or(""))}))
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
            let handle =
                VaeHandle::spawn(move || memory_vae_with(classes.as_ref(), Some(ev), Some(jb)))
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
            match ensure_tenant(&node, &tenant, &authority, classes.as_ref()) {
                Ok(true) => eprintln!("tenant {tenant} with VAE {authority} created"),
                Ok(false) => {}
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
