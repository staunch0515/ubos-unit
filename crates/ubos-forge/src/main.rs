//! `forge` command line (PH-0: `forge conform`).

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

const USAGE: &str = "usage: forge conform --vectors <dir> [--tag <tag>] [--profiles <p1,p2>] \
[--report <file>] [--key <file with 64 hex chars>] [--standard <version>] [--build <hash>] \
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
    let mut results = Vec::new();
    let mut summary: BTreeMap<String, BTreeMap<&str, u64>> = BTreeMap::new();
    let mut failed = 0;
    for p in &paths {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let profile = v["profile"].as_str().unwrap_or_default().to_string();
        let outcome = if profiles.contains(&profile) {
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
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
