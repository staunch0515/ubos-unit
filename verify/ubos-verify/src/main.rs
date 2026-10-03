//! `ubos-verify` command line: verify a proof, a signature or an archive and print the
//! report (DSN-DVM-938). Exit status 0 when every check passed, 1 otherwise, 2 on usage.
//!
//! ```text
//! ubos-verify inclusion <proof.json> [--root <hash>] [--dids <dids.json>]
//! ubos-verify consistency <commits.json> [--from <commit id>] [--to <commit id>] [--dids <dids.json>]
//! ubos-verify signature <signature.json> --content-id <id> --did <did.json> [--at <instant>]
//! ubos-verify archive <archive.json>
//! ```
//!
//! A file argument of `-` reads standard input. `--quiet` prints the result line only.

use serde_json::Value;
use std::io::Read;

const USAGE: &str = "usage:
  ubos-verify inclusion <proof.json> [--root <hash>] [--dids <dids.json>]
  ubos-verify consistency <commits.json> [--from <commit id>] [--to <commit id>] [--dids <dids.json>]
  ubos-verify signature <signature.json> --content-id <id> --did <did.json> [--at <instant>]
  ubos-verify archive <archive.json>
options: --quiet (result line only); a file of - is standard input";

fn read_json(path: &str) -> Result<Value, String> {
    let text = if path == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        s
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?
    };
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

fn run() -> Result<ubos_verify::Report, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let (Some(cmd), Some(file)) = (args.first(), args.get(1)) else {
        return Err(USAGE.into());
    };
    let dids: Vec<Value> = match arg("--dids") {
        Some(p) => {
            let v = read_json(&p)?;
            match v {
                Value::Array(a) => a,
                other => vec![other],
            }
        }
        None => Vec::new(),
    };
    let input = read_json(file)?;
    Ok(match cmd.as_str() {
        "inclusion" => ubos_verify::inclusion(&input, arg("--root").as_deref(), &dids),
        "consistency" => ubos_verify::consistency(
            &input,
            arg("--from").as_deref(),
            arg("--to").as_deref(),
            &dids,
        ),
        "signature" => {
            let cid = arg("--content-id").ok_or(USAGE)?;
            let did = read_json(&arg("--did").ok_or(USAGE)?)?;
            ubos_verify::signature(&input, &cid, &did, arg("--at").as_deref())
        }
        "archive" => ubos_verify::archive(&input),
        _ => return Err(USAGE.into()),
    })
}

fn main() {
    let quiet = std::env::args().any(|a| a == "--quiet");
    match run() {
        Ok(report) => {
            if !quiet {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report.to_json()).unwrap_or_default()
                );
            }
            let failed: Vec<&str> = report
                .checks
                .iter()
                .filter(|c| !c.pass)
                .map(|c| c.name)
                .collect();
            if report.ok() {
                println!(
                    "ubos-verify: {} verified ({} checks)",
                    report.subject,
                    report.checks.len()
                );
                std::process::exit(0);
            }
            println!(
                "ubos-verify: {} NOT verified (failed: {})",
                report.subject,
                failed.join(", ")
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
