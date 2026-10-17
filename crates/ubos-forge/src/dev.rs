//! `forge dev` (DSN-FRG-501, NR-PERF-016; PH-1 step 17e, DEC-078): a local development node
//! with the project installed, watching its sources.
//!
//! The node is `ubos-server` in the `dev` preset, started as a child process on a local
//! PostgreSQL database (`--database`), or in memory, and it serves UBTP to local clients.
//! The project is installed over UBTP with the Rust SDK (this crate may depend on
//! `ubos-dvm` and `ubos-sdk` only, AR-031): its seed and logic on the development branch
//! `feature/dev`. Then the sources are watched:
//!
//! - every change is checked first (the check pipeline of DSN-FRG-002); with errors nothing
//!   is installed and the findings are printed;
//! - a changed logic asset is installed again on `feature/dev` at once, so the running VAE
//!   uses it from the next call (the loop of NR-PERF-016);
//! - a changed model, seed or manifest rebuilds the development VAE, since classes are
//!   declared at genesis in PH-1 (class evolution comes with PH-2): the node restarts on a
//!   fresh tenant named after the model and the project is installed again, and the
//!   generated types are written again;
//! - with `--test`, the project's tests run after each accepted change.
//!
//! Each step prints one line (`--json`: one JSON object per line) with its timings.

use crate::check::{Backend, asset_object_id, check};
use crate::project::Project;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use ubos_sdk::{Auth, CallOptions, Client, Config, Selectors};

/// The development branch the project is installed on.
pub const BRANCH: &str = "feature/dev";

/// The options of `forge dev`.
#[derive(Debug, Clone)]
pub struct Options {
    /// The project.
    pub dir: PathBuf,
    /// The `ubos-server` binary.
    pub server: PathBuf,
    /// A PostgreSQL connection string; in memory when `None`.
    pub database: Option<String>,
    /// Where the node listens.
    pub listen: String,
    /// Run the tests after each accepted change.
    pub test: bool,
    /// Write the generated types after each model change.
    pub types: bool,
    /// Install once and stop (no watching).
    pub once: bool,
    /// JSON lines instead of text.
    pub json: bool,
    /// The interval of the source scan.
    pub poll: Duration,
}

/// The default `ubos-server`: `UBOS_SERVER`, else next to this executable, else on `PATH`.
pub fn default_server() -> PathBuf {
    if let Some(p) = std::env::var_os("UBOS_SERVER") {
        return PathBuf::from(p);
    }
    if let Ok(me) = std::env::current_exe()
        && let Some(dir) = me.parent()
        && dir.join("ubos-server").exists()
    {
        return dir.join("ubos-server");
    }
    PathBuf::from("ubos-server")
}

fn say(o: &Options, event: Value, text: String) {
    if o.json {
        println!("{event}");
    } else {
        println!("forge dev: {text}");
    }
    let _ = std::io::stdout().flush();
}

/// The files of the project that `forge dev` watches, with their modification time and
/// size (generated output and hidden folders are left out).
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (SystemTime, u64)> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (SystemTime, u64)>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || ["generated", "target", "node_modules"].contains(&name.as_str())
            {
                continue;
            }
            match e.metadata() {
                Ok(m) if m.is_dir() => walk(&p, out),
                Ok(m) => {
                    out.insert(p, (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
                }
                Err(_) => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

/// What a rebuild depends on: the model, the value types, the seed and the manifest.
fn genesis_key(p: &Project) -> String {
    let v = json!({"classes": p.declarations(), "value_types": p.value_type_decls(),
        "seed": p.seed.iter().map(|i| &i.value).collect::<Vec<_>>(),
        "manifest": {"name": p.manifest.name, "vae": p.manifest.vae}});
    ubos_dvm::content_id("ubos.chunk.v1", &v).unwrap_or_default()
}

/// The content hash of every logic asset by name.
fn asset_hashes(p: &Project) -> BTreeMap<String, String> {
    p.logic
        .iter()
        .map(|i| {
            let name = i.value["name"].as_str().unwrap_or("asset").to_string();
            (
                name,
                ubos_dvm::content_id("ubos.chunk.v1", &i.value).unwrap_or_default(),
            )
        })
        .collect()
}

struct Node {
    child: Child,
    url: String,
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `GET /readyz` answers 200.
fn ready(listen: &str) -> bool {
    let Ok(mut s) = std::net::TcpStream::connect(listen) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if s.write_all(format!("GET /readyz HTTP/1.0\r\nhost: {listen}\r\n\r\n").as_bytes())
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 16];
    s.read(&mut buf).is_ok_and(|n| {
        buf[..n].starts_with(b"HTTP/1.1 200") || buf[..n].starts_with(b"HTTP/1.0 200")
    })
}

/// Start the node for a project on a tenant named after its genesis key.
fn start(o: &Options, p: &Project, key: &str, work: &Path) -> Result<Node, String> {
    let tenant = format!("dev-{}", &key.trim_start_matches("sha256:")[..12]);
    let genesis = work.join("genesis.json");
    let mut g = json!({"classes": p.declarations(), "branches": [BRANCH]});
    let vts = p.value_type_decls();
    if !vts.is_empty() {
        g["value_types"] = json!(vts);
    }
    std::fs::write(&genesis, g.to_string()).map_err(|e| e.to_string())?;
    let mut cfg = format!(
        "version = 1\n[server]\nlisten = \"{}\"\npreset = \"dev\"\ntenant = \"{tenant}\"\nvae = \"{}\"\n",
        o.listen, p.manifest.vae
    );
    if let Some(db) = &o.database {
        cfg.push_str(&format!(
            "[database]\nurl = \"{}\"\n",
            db.replace('"', "\\\"")
        ));
    }
    let cfg_path = work.join("ubos.toml");
    std::fs::write(&cfg_path, cfg).map_err(|e| e.to_string())?;
    let log = std::fs::File::create(work.join("server.log")).map_err(|e| e.to_string())?;
    if o.database.is_some() {
        // migrations, the tenant and its VAE with the genesis (idempotent)
        let out = Command::new(&o.server)
            .arg("bootstrap")
            .arg("--config")
            .arg(&cfg_path)
            .arg("--genesis")
            .arg(&genesis)
            .output()
            .map_err(|e| format!("{}: {e}", o.server.display()))?;
        if !out.status.success() {
            return Err(format!(
                "bootstrap failed:\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    let child = Command::new(&o.server)
        .arg("--config")
        .arg(&cfg_path)
        .arg("--genesis")
        .arg(&genesis)
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("{}: {e}", o.server.display()))?;
    let mut node = Node {
        child,
        url: format!("http://{}", o.listen),
    };
    let until = Instant::now() + Duration::from_secs(60);
    while !ready(&o.listen) {
        if Instant::now() > until || node.child.try_wait().ok().flatten().is_some() {
            let log = std::fs::read_to_string(work.join("server.log")).unwrap_or_default();
            return Err(format!("the node did not start:\n{log}"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = &mut node;
    Ok(node)
}

/// Install the seed (once per VAE) and the logic assets whose content differs from
/// `installed`; returns the names of the assets installed.
async fn install(
    c: &Client,
    p: &Project,
    installed: &mut BTreeMap<String, String>,
    seed: bool,
) -> Result<Vec<String>, String> {
    let o = CallOptions {
        branch: Some(BRANCH.into()),
        ..CallOptions::default()
    };
    if seed {
        let mut files: Vec<&str> = Vec::new();
        for i in &p.seed {
            if !files.contains(&i.file.as_str()) {
                files.push(&i.file);
            }
        }
        for f in files {
            let changes: Vec<&Value> = p
                .seed
                .iter()
                .filter(|i| i.file == f)
                .map(|i| &i.value)
                .collect();
            c.commit(json!(changes), &o)
                .await
                .map_err(|e| format!("{f}: {e}"))?;
        }
    }
    let vae = c.config().vae.clone();
    let mut done = Vec::new();
    for (item, (name, hash)) in p.logic.iter().zip(asset_hashes(p)) {
        if installed.get(&name) == Some(&hash) {
            continue;
        }
        let slug = name.replace('.', "-");
        let uri = format!("ubos://{vae}/Logic/{slug}");
        let change = match c.read(&uri, &Selectors::branch(BRANCH)).await {
            Ok(cur) => {
                json!({"op": "update", "object_id": cur.object_id(), "base": cur.version(), "body": item.value})
            }
            Err(_) => {
                json!({"op": "create", "class": "Logic", "object_id": asset_object_id(&name), "slug": slug, "body": item.value})
            }
        };
        c.commit(json!([change]), &o)
            .await
            .map_err(|e| format!("{}: asset {name}: {e}", item.file))?;
        installed.insert(name.clone(), hash);
        done.push(name);
    }
    Ok(done)
}

/// Check a project; the findings that are errors, printed.
fn checked(o: &Options, p: &Project) -> (bool, u128) {
    let t = Instant::now();
    let findings = check(p);
    let errors: Vec<_> = findings.iter().filter(|f| f.severity == "error").collect();
    let ms = t.elapsed().as_millis();
    if !errors.is_empty() {
        for f in &errors {
            say(
                o,
                json!({"event": "finding", "finding": f.to_json()}),
                format!(
                    "{}{}: error [{}] {}",
                    f.file,
                    f.line.map(|l| format!(":{l}")).unwrap_or_default(),
                    f.rule,
                    f.message
                ),
            );
        }
        say(
            o,
            json!({"event": "rejected", "errors": errors.len(), "check_ms": ms}),
            format!("{} errors; nothing installed", errors.len()),
        );
    }
    (errors.is_empty(), ms)
}

fn tests(o: &Options, p: &Project) -> Value {
    if !o.test {
        return Value::Null;
    }
    let t = Instant::now();
    match crate::test::run(p, &Backend::default()) {
        Ok(r) => {
            let failed: Vec<&str> = r
                .results
                .iter()
                .filter(|x| !x.pass)
                .map(|x| x.name.as_str())
                .collect();
            json!({"tests": r.results.len(), "failed": failed, "test_ms": t.elapsed().as_millis() as u64})
        }
        Err(e) => json!({"error": e.to_string(), "test_ms": t.elapsed().as_millis() as u64}),
    }
}

fn write_types(o: &Options, p: &Project) {
    if !o.types {
        return;
    }
    if let Ok((ts, rs, _)) = crate::types::generate(p) {
        let out = p.root.join("generated");
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::write(out.join("types.ts"), ts);
        let _ = std::fs::write(out.join("types.rs"), rs);
    }
}

/// Run `forge dev` until interrupted (or once).
pub fn run(o: Options) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run_async(o))
}

async fn connect(node: &Node, vae: &str) -> Result<Client, String> {
    let cfg = Config::new(&node.url, vae, Auth::Dev);
    Client::connect(cfg).await.map_err(|e| e.to_string())
}

async fn run_async(o: Options) -> Result<(), String> {
    let work = std::env::temp_dir().join(format!("forge-dev-{}", std::process::id()));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let mut seen = snapshot(&o.dir);
    // a project with errors waits for a change before the node starts
    let mut project = Project::load(&o.dir)?;
    loop {
        let (ok, _) = checked(&o, &project);
        if ok {
            break;
        }
        loop {
            tokio::time::sleep(o.poll).await;
            let now = snapshot(&o.dir);
            if now != seen {
                seen = now;
                break;
            }
        }
        project = Project::load(&o.dir)?;
    }
    let t0 = Instant::now();
    let mut key = genesis_key(&project);
    let mut node = start(&o, &project, &key, &work)?;
    let mut client = connect(&node, &project.manifest.vae).await?;
    let mut installed = BTreeMap::new();
    let assets = install(&client, &project, &mut installed, true).await?;
    write_types(&o, &project);
    let t = tests(&o, &project);
    say(
        &o,
        json!({"event": "ready", "url": node.url, "vae": project.manifest.vae, "branch": BRANCH,
            "assets": assets, "ms": t0.elapsed().as_millis() as u64, "tests": t}),
        format!(
            "ready in {} ms: {} (VAE {}, branch {BRANCH}), {} assets installed{}",
            t0.elapsed().as_millis(),
            node.url,
            project.manifest.vae,
            assets.len(),
            if t.is_null() {
                String::new()
            } else {
                format!("; tests {t}")
            }
        ),
    );
    if o.once {
        return Ok(());
    }
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| e.to_string())?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = term.recv() => break,
            _ = tokio::time::sleep(o.poll) => {}
        }
        let now = snapshot(&o.dir);
        if now == seen {
            continue;
        }
        let changed: Vec<String> = now
            .iter()
            .filter(|(p, m)| seen.get(*p) != Some(m))
            .map(|(p, _)| p.strip_prefix(&o.dir).unwrap_or(p).display().to_string())
            .chain(
                seen.keys()
                    .filter(|p| !now.contains_key(*p))
                    .map(|p| p.strip_prefix(&o.dir).unwrap_or(p).display().to_string()),
            )
            .collect();
        seen = now;
        let t0 = Instant::now();
        let next = match Project::load(&o.dir) {
            Ok(p) => p,
            Err(e) => {
                say(
                    &o,
                    json!({"event": "rejected", "error": e}),
                    format!("the project does not load: {e}"),
                );
                continue;
            }
        };
        let (ok, check_ms) = checked(&o, &next);
        if !ok {
            continue;
        }
        let k = genesis_key(&next);
        let ti = Instant::now();
        let (kind, assets) = if k != key {
            // classes are declared at genesis in PH-1: a fresh development VAE
            drop(node);
            node = start(&o, &next, &k, &work)?;
            client = connect(&node, &next.manifest.vae).await?;
            installed.clear();
            let a = install(&client, &next, &mut installed, true).await;
            write_types(&o, &next);
            key = k;
            ("rebuilt", a)
        } else {
            (
                "reloaded",
                install(&client, &next, &mut installed, false).await,
            )
        };
        let install_ms = ti.elapsed().as_millis();
        project = next;
        match assets {
            Ok(a) => {
                let t = tests(&o, &project);
                say(
                    &o,
                    json!({"event": kind, "changed": changed, "assets": a, "check_ms": check_ms,
                        "install_ms": install_ms, "ms": t0.elapsed().as_millis() as u64, "tests": t}),
                    format!(
                        "{kind} in {} ms (check {check_ms} ms, install {install_ms} ms): {} changed, assets [{}]{}",
                        t0.elapsed().as_millis(),
                        changed.join(", "),
                        a.join(", "),
                        if t.is_null() {
                            String::new()
                        } else {
                            format!("; tests {t}")
                        }
                    ),
                );
            }
            Err(e) => say(
                &o,
                json!({"event": "install_failed", "error": e}),
                format!("install failed: {e}"),
            ),
        }
    }
    drop(node);
    let _ = std::fs::remove_dir_all(&work);
    say(&o, json!({"event": "stopped"}), "stopped".into());
    Ok(())
}
