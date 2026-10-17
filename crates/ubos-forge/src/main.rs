//! `forge` command line: `init`, `check`, `types`, `test` (PH-1 step 7b, DEC-060), `dev`
//! (step 17e, DEC-078) and `conform` (PH-0). Every command takes `--json`; exit status 0 on success, 1 on findings
//! or failures, 2 on usage (DSN-FRG-001).

use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use ubos_dvm::{Adapter, canonical};
use ubos_forge::runner::{Outcome, run_vector};

/// Live bytes are counted for the memory limit of the L2 host (DSN-DVM-612, W-2).
#[global_allocator]
static ALLOC: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

const USAGE: &str = "usage:
  forge init <dir> [--name <name>] [--json]
  forge check [<project dir>] [--json]
  forge types [<project dir>] [--out <dir>] [--sdk <module>] [--json]
  forge test [<project dir>] [--report <json file>] [--junit <xml file>] [--records <dir>] \
[--key <file with 64 hex chars>] [--backend memory|postgres] [--database <connection string>] [--json]
  forge dev [<project dir>] [--database <connection string>] [--listen <host:port>] \
[--server <ubos-server>] [--cors <origin>]... [--test] [--types] [--once] [--json]
  forge conform --vectors <dir> [--tag <tag>] [--profiles <p1,p2>] [--report <file>] \
[--key <file with 64 hex chars>] [--standard <version>] [--build <hash>] \
[--backend memory|postgres] [--database <connection string>] [--l2] [--verbose]";

fn files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            files(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    Ok(())
}

fn seed(path: Option<&str>) -> Result<[u8; 32], String> {
    let text = match path {
        Some(p) => std::fs::read_to_string(p).map_err(|e| e.to_string())?,
        None => {
            // an ephemeral runner key; its public key is part of the report
            let mut s = [0u8; 32];
            let mut f = std::fs::File::open("/dev/urandom").map_err(|e| e.to_string())?;
            std::io::Read::read_exact(&mut f, &mut s).map_err(|e| e.to_string())?;
            return Ok(s);
        }
    };
    let t = text.trim();
    if t.len() != 64 {
        return Err("key file must hold 64 hex characters".into());
    }
    let mut s = [0u8; 32];
    for (i, b) in s.iter_mut().enumerate() {
        *b = u8::from_str_radix(&t[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(s)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn conform(args: &[String]) -> Result<bool, String> {
    let mut opts: BTreeMap<&str, String> = BTreeMap::new();
    let mut verbose = false;
    let mut l2 = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--verbose" => verbose = true,
            "--l2" => l2 = true,
            "--vectors" | "--tag" | "--profiles" | "--report" | "--key" | "--standard"
            | "--build" | "--backend" | "--database" => {
                opts.insert(a.as_str(), it.next().ok_or(USAGE)?.clone());
            }
            _ => return Err(USAGE.into()),
        }
    }
    let dir = opts.get("--vectors").ok_or(USAGE)?;
    let profiles: Vec<String> = opts
        .get("--profiles")
        .map(|p| p.split(',').map(str::to_string).collect())
        .unwrap_or_else(|| {
            ["Core", "Bitemporal", "Ledger", "Proof"]
                .map(String::from)
                .to_vec()
        });
    let mut adapter = match opts
        .get("--backend")
        .map(String::as_str)
        .unwrap_or("memory")
    {
        "memory" => Adapter::new(),
        "postgres" => Adapter::postgres(
            opts.get("--database")
                .map(String::as_str)
                .unwrap_or("host=127.0.0.1 user=postgres"),
        ),
        _ => return Err(USAGE.into()),
    }
    .map_err(|e| e.to_string())?;
    ubos_dvm::memory::install_probe(|| ALLOC.allocated());
    if l2 {
        adapter.enable_l2();
    }
    let mut paths = Vec::new();
    files(Path::new(dir), &mut paths).map_err(|e| e.to_string())?;
    paths.sort();
    let superseded = ubos_forge::runner::superseded(Path::new(dir));
    let mut results = Vec::new();
    let mut summary: BTreeMap<String, BTreeMap<&str, u64>> = BTreeMap::new();
    let mut failed = 0;
    for p in &paths {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let profile = v["profile"].as_str().unwrap_or_default().to_string();
        let outcome = if superseded.contains(v["id"].as_str().unwrap_or_default()) {
            Outcome::NotApplicable("superseded by a later release".into())
        } else if profiles.contains(&profile) {
            run_vector(&mut adapter, &v, opts.get("--tag").map(String::as_str))
        } else {
            Outcome::NotApplicable("profile not claimed".into())
        };
        let (name, detail) = match &outcome {
            Outcome::Pass => ("pass", String::new()),
            Outcome::Fail(d) => {
                failed += 1;
                ("fail", d.clone())
            }
            Outcome::NotApplicable(d) => ("not_applicable", d.clone()),
        };
        if verbose && name == "fail" {
            eprintln!("FAIL {} {}", v["id"].as_str().unwrap_or("?"), detail);
        }
        *summary.entry(profile).or_default().entry(name).or_default() += 1;
        let mut r = json!({"vector": v["id"], "outcome": name});
        if !detail.is_empty() {
            r["detail"] = Value::String(detail);
        }
        results.push(r);
    }
    let key = SigningKey::from_bytes(&seed(opts.get("--key").map(String::as_str))?);
    let mut report = json!({
        "standard_version": opts.get("--standard").cloned().unwrap_or_else(|| "0.1.0".into()),
        "implementation": {"name": "ubos-dvm prototype", "vendor": "UBOS", "version": env!("CARGO_PKG_VERSION"),
                           "build_hash": opts.get("--build").cloned().unwrap_or_default(), "backends": [adapter.backend()]},
        "profiles_claimed": profiles,
        "results": results,
        "summary": summary,
        "environment": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
    });
    let body = canonical(&report).map_err(|e| e.to_string())?;
    let sig = key.sign(body.as_bytes());
    report["signature"] = json!({"alg": "ed25519", "key": hex(key.verifying_key().as_bytes()), "sig": hex(&sig.to_bytes())});
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = opts.get("--report") {
        std::fs::write(path, text + "\n").map_err(|e| e.to_string())?;
    }
    let mut line = Map::new();
    for (p, counts) in &summary {
        line.insert(p.clone(), json!(counts));
    }
    println!(
        "conformance: {} vectors, {}",
        paths.len(),
        Value::Object(line)
    );
    println!("failures: {failed}");
    Ok(failed == 0)
}

/// Options of a project command: the positional project directory and `--k v` pairs.
fn options(args: &[String]) -> (PathBuf, BTreeMap<String, String>, bool) {
    let mut dir = PathBuf::from(".");
    let mut opts = BTreeMap::new();
    let mut json_out = false;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--json" {
            json_out = true;
        } else if let Some(k) = a.strip_prefix("--") {
            if let Some(v) = args.get(i + 1) {
                opts.insert(k.to_string(), v.clone());
                i += 1;
            }
        } else {
            dir = PathBuf::from(a);
        }
        i += 1;
    }
    (dir, opts, json_out)
}

fn load(dir: &Path) -> Result<ubos_forge::project::Project, String> {
    ubos_forge::project::Project::load(dir)
}

fn emit(json_out: bool, v: &Value, text: &str) {
    if json_out {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
    } else {
        println!("{text}");
    }
}

fn init(args: &[String]) -> Result<bool, String> {
    let (dir, opts, json_out) = options(args);
    let name = opts
        .get("name")
        .cloned()
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "project".into());
    let written = ubos_forge::project::Project::init(&dir, &name)?;
    emit(
        json_out,
        &json!({"command": "init", "project": name, "dir": dir.display().to_string(), "written": written}),
        &format!(
            "forge init: {} files written to {}",
            written.len(),
            dir.display()
        ),
    );
    Ok(true)
}

fn check(args: &[String]) -> Result<bool, String> {
    let (dir, _, json_out) = options(args);
    let project = load(&dir)?;
    let findings = ubos_forge::check::check(&project);
    let errors = findings.iter().filter(|f| f.severity == "error").count();
    let list: Vec<Value> = findings.iter().map(|f| f.to_json()).collect();
    let mut text = String::new();
    for f in &findings {
        text.push_str(&format!(
            "{}{}: {} [{}] {}\n",
            f.file,
            f.line.map(|l| format!(":{l}")).unwrap_or_default(),
            f.severity,
            f.rule,
            f.message
        ));
    }
    text.push_str(&format!(
        "forge check: {} classes, {} assets, {} seed changes, {} tests; {} findings, {errors} errors",
        project.classes.len(), project.logic.len(), project.seed.len(), project.tests.len(), findings.len()
    ));
    emit(
        json_out,
        &json!({"command": "check", "project": project.manifest.name, "findings": list, "errors": errors,
            "counts": {"classes": project.classes.len(), "assets": project.logic.len(), "seed": project.seed.len(), "tests": project.tests.len()}}),
        &text,
    );
    Ok(errors == 0)
}

fn types(args: &[String]) -> Result<bool, String> {
    let (dir, opts, json_out) = options(args);
    let project = load(&dir)?;
    let sdk = opts.get("sdk").map(String::as_str).unwrap_or("@ubos/sdk");
    let (ts, rs, header) =
        ubos_forge::types::generate_with(&project, sdk).map_err(|e| e.to_string())?;
    let out = opts
        .get("out")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("generated"));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("types.ts"), ts).map_err(|e| e.to_string())?;
    std::fs::write(out.join("types.rs"), rs).map_err(|e| e.to_string())?;
    let model = ubos_forge::types::model(&project).map_err(|e| e.to_string())?;
    let model_text = serde_json::to_string_pretty(&model).unwrap_or_default() + "\n";
    std::fs::write(out.join("model.json"), model_text).map_err(|e| e.to_string())?;
    emit(
        json_out,
        &json!({"command": "types", "header": header, "files": [out.join("types.ts").display().to_string(),
            out.join("types.rs").display().to_string(), out.join("model.json").display().to_string()]}),
        &format!(
            "forge types: types.ts, types.rs and model.json written to {} (model {})",
            out.display(),
            header["model_hash"]
        ),
    );
    Ok(true)
}

/// `forge dev` (DSN-FRG-501): flags without values (`--test`, `--types`, `--once`,
/// `--json`) and options with one.
fn dev(args: &[String]) -> Result<bool, String> {
    let mut o = ubos_forge::dev::Options {
        dir: PathBuf::from("."),
        server: ubos_forge::dev::default_server(),
        database: std::env::var("UBOS_DEV_DATABASE").ok(),
        listen: "127.0.0.1:7070".into(),
        cors: vec![
            "http://127.0.0.1:7080".into(),
            "http://localhost:7080".into(),
        ],
        test: false,
        types: false,
        once: false,
        json: false,
        poll: std::time::Duration::from_millis(150),
    };
    let mut i = 0;
    while i < args.len() {
        let value = || {
            args.get(i + 1)
                .cloned()
                .ok_or(format!("{} needs a value", args[i]))
        };
        match args[i].as_str() {
            "--test" => o.test = true,
            "--types" => o.types = true,
            "--once" => o.once = true,
            "--json" => o.json = true,
            "--database" => {
                o.database = Some(value()?);
                i += 1;
            }
            "--memory" => o.database = None,
            "--listen" => {
                o.listen = value()?;
                i += 1;
            }
            "--server" => {
                o.server = PathBuf::from(value()?);
                i += 1;
            }
            "--cors" => {
                o.cors.push(value()?);
                i += 1;
            }
            a if a.starts_with("--") => return Err(format!("unknown option {a}\n{USAGE}")),
            a => o.dir = PathBuf::from(a),
        }
        i += 1;
    }
    ubos_forge::dev::run(o).map(|()| true)
}

fn test(args: &[String]) -> Result<bool, String> {
    let (dir, opts, json_out) = options(args);
    let project = load(&dir)?;
    let backend = ubos_forge::check::Backend {
        database: match opts.get("backend").map(String::as_str) {
            Some("postgres") => Some(
                opts.get("database")
                    .cloned()
                    .ok_or("--database is required with --backend postgres")?,
            ),
            Some("memory") | None => None,
            Some(other) => return Err(format!("unknown backend {other}")),
        },
    };
    let report = ubos_forge::test::run(&project, &backend).map_err(|e| e.to_string())?;
    let mut out = report.to_json(&project);
    // verification records of the assets (DSN-FRG-103), signed with the verifier key
    let key = ubos_forge::records::VerifierKey::new(seed(opts.get("key").map(String::as_str))?)
        .map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    for item in &project.logic {
        let name = item
            .value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let tests_of = |pass: bool| {
            report
                .results
                .iter()
                .filter(|r| r.pass == pass && matches!(r.kind.as_str(), "unit" | "property"))
                .count() as u64
        };
        let coverage = out["coverage"][&name]["fraction"].as_f64().unwrap_or(0.0);
        let rec = ubos_forge::records::record(
            &key,
            &item.value,
            (tests_of(true), tests_of(false)),
            coverage,
            ubos_forge::check::CLOCK,
        )
        .map_err(|e| e.to_string())?;
        if let Some(d) = opts.get("records") {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            std::fs::write(
                Path::new(d).join(format!("{name}.json")),
                serde_json::to_string_pretty(&rec).unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
        }
        records.push(json!({"asset": name, "record_id": rec["record_id"], "result": if rec["checks"].as_array().is_some_and(|c| c.iter().all(|x| x["result"] == json!("pass"))) { "pass" } else { "fail" }}));
    }
    out["verifier"] = json!(key.did);
    out["records"] = json!(records);
    if let Some(p) = opts.get("report") {
        std::fs::write(p, serde_json::to_string_pretty(&out).unwrap_or_default())
            .map_err(|e| e.to_string())?;
    }
    if let Some(p) = opts.get("junit") {
        std::fs::write(p, report.to_junit(&project)).map_err(|e| e.to_string())?;
    }
    let mut text = String::new();
    for r in &report.results {
        text.push_str(&format!(
            "{} {} ({}, {} ms){}\n",
            if r.pass { "ok  " } else { "FAIL" },
            r.name,
            r.kind,
            r.ms,
            r.failure
                .as_ref()
                .map(|f| format!(": {f}"))
                .unwrap_or_default()
        ));
    }
    for f in &report.findings {
        text.push_str(&format!(
            "{}: {} [{}] {}\n",
            f.file, f.severity, f.rule, f.message
        ));
    }
    text.push_str(&format!(
        "forge test: {} of {} tests passed on {}; roots genesis {} seed {}",
        out["passed"],
        out["tests"],
        if backend.database.is_some() {
            "postgres"
        } else {
            "memory"
        },
        report.roots["genesis"],
        report.roots["seed"]
    ));
    out["backend"] = json!(if backend.database.is_some() {
        "postgres"
    } else {
        "memory"
    });
    let ok = out["failed"] == json!(0) && report.findings.is_empty();
    emit(json_out, &out, &text);
    Ok(ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let project_cmd = |r: Result<bool, String>| match r {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    };
    match args.first().map(String::as_str) {
        Some("init") => project_cmd(init(&args[1..])),
        Some("check") => project_cmd(check(&args[1..])),
        Some("types") => project_cmd(types(&args[1..])),
        Some("test") => project_cmd(test(&args[1..])),
        Some("dev") => project_cmd(dev(&args[1..])),
        Some("conform") => match conform(&args[1..]) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(2)
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
