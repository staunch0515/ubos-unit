//! `forge simulate` (DSN-FRG-205, PH-2 step 5f, DEC-117): a simulation of rule or logic
//! changes on a Server (STD-LGS-044, -074). The candidates are on a branch of the VAE; the
//! Server re-runs the historical port calls of the target in the window on sandbox branches
//! with them, releases no effects (FR-DEV-042) and stores the run; the command reports the
//! calls whose outcome changed (FR-DEV-041, DSN-DVM-761).

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::time::Duration;

/// What to simulate, and where.
pub struct Options {
    /// The Server, `host:port`.
    pub server: String,
    /// A bearer token of an administrator of the tenant.
    pub token: String,
    /// The VAE (default: the Server's default authority).
    pub vae: Option<String>,
    /// The branch holding the candidates.
    pub source: String,
    /// The branch whose history is re-run (default `main`).
    pub target: String,
    /// The first date of the window `[from, to)`.
    pub from: Option<String>,
    /// The end of the window, excluded.
    pub to: Option<String>,
    /// Only the calls of this port.
    pub port: Option<String>,
    /// The budget of processes.
    pub budget: u64,
    /// Take the candidate logic too (STD-LGS-074).
    pub logic: bool,
    /// The change set the run is evidence for.
    pub change_set: Option<String>,
}

/// A random UUID (version 4) for the run.
fn run_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| e.to_string())?;
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

/// One HTTP/1.1 request with a JSON body: the status and the JSON reply.
fn post(addr: &str, path: &str, body: &Value, token: &str) -> Result<(u16, Value), String> {
    let mut s = std::net::TcpStream::connect(addr).map_err(|e| format!("{addr}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(3600)))
        .map_err(|e| e.to_string())?;
    let payload = body.to_string();
    let req = format!(
        "POST {path} HTTP/1.1\r\nhost: {addr}\r\nconnection: close\r\ncontent-type: application/json\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\n\r\n{payload}",
        payload.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf);
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut r = rest;
        while let Some((size, tail)) = r.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 || tail.len() < n {
                break;
            }
            out.push_str(&tail[..n]);
            r = tail[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        rest.to_string()
    };
    Ok((status, serde_json::from_str(&body).unwrap_or(Value::Null)))
}

/// Run a simulation on the Server: the stored run (`SimulationRun` with `run_id`).
pub fn run(o: &Options) -> Result<Value, String> {
    let mut input = json!({"source": o.source, "target": o.target, "budget": {"processes": o.budget},
                           "run_id": run_id()?});
    if o.from.is_some() || o.to.is_some() {
        input["from"] = json!(o.from);
        input["to"] = json!(o.to);
    }
    if let Some(p) = &o.port {
        input["select"] = json!({"port": p});
    }
    if o.logic {
        input["include"] = json!(["logic"]);
    }
    if let Some(cs) = &o.change_set {
        input["change_set"] = json!(cs);
    }
    let path = match &o.vae {
        Some(v) => format!("/api/v1/admin/simulate?vae={v}"),
        None => "/api/v1/admin/simulate".to_string(),
    };
    let (status, reply) = post(&o.server, &path, &input, &o.token)?;
    if status != 200 {
        let code = reply
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("NOD.UNAVAILABLE");
        let detail = reply
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Err(format!("simulate: {status} {code} {detail}"));
    }
    Ok(reply)
}

/// The text report: the coverage, then one line per call whose outcome changed, with the
/// sandbox branch that holds its re-run (DSN-DVM-760).
pub fn text(run: &Value) -> String {
    let c = &run["coverage"];
    let mut out = format!(
        "simulation {}: {} of {} calls re-run{}, {} changed\n",
        run["run_id"].as_str().unwrap_or_default(),
        c["processes"],
        c["selected"],
        if c["complete"] == json!(true) {
            ""
        } else {
            " (budget reached)"
        },
        run["changed"]
    );
    let rid = run["run_id"].as_str().unwrap_or_default();
    let short = rid.get(..8).unwrap_or(rid);
    for (n, r) in run["results"].as_array().into_iter().flatten().enumerate() {
        if r["changed"] == json!(true) {
            let o = |v: &Value| {
                v["error"]
                    .as_str()
                    .map(|e| format!("failed {e}"))
                    .unwrap_or_else(|| v["outcome"].as_str().unwrap_or_default().to_string())
            };
            out.push_str(&format!(
                "  {} {}: {} -> {} (sandbox/sim-{short}-{})\n",
                r["date"].as_str().unwrap_or_default(),
                r["process_id"].as_str().unwrap_or_default(),
                o(&r["actual"]),
                o(&r["simulated"]),
                n + 1
            ));
        }
    }
    out
}
