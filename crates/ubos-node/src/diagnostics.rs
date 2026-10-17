//! Diagnostic bundles (PH-1 step 16d, DEC-077; DSN-NOD-615; FR-OPS-053; NR-OPER-005).
//!
//! A bundle is one JSON document `{manifest, versions, host, configuration, health,
//! telemetry, spans, logs, runtime, database, security_log}` built from operational
//! sources only: the configuration (references, never secrets: DSN-NOD-605), versions,
//! the host, the self-checks, the telemetry the allow-list already admitted (DSN-NOD-611),
//! counts and sizes of the database, and the kinds of the security-log entries without
//! their records. No object, event or job content is read, so no business value can enter
//! by construction (FR-OPS-053).
//!
//! As a second line, [`finish`] looks for every secret the configuration refers to (the
//! resolved references and a connection string's password) in the finished text; an
//! occurrence is replaced by `[redacted]` and counted in `manifest.redactions`.
//!
//! The running node serves a bundle at `POST /api/v1/admin/diagnostics`; `ubos-server
//! diagnostics --config ubos.toml` writes the offline part (versions, host,
//! configuration, database, prerequisites) for a node that does not start.

use serde_json::{Map, Value, json};
use ubos_dvm::types::content_id;

/// The format of a bundle.
pub const FORMAT: &str = "ubos.diagnostics.v1";

/// The versions of the node: the server, the protocol, the embedded genesis Buks, the build.
pub fn versions() -> Value {
    let genesis: Vec<Value> = crate::genesis::embedded()
        .unwrap_or_default()
        .iter()
        .map(|b| json!({"name": b.name(), "version": b.version(), "content_hash": b.hash}))
        .collect();
    json!({"ubos_server": env!("CARGO_PKG_VERSION"), "ubtp": "1.0", "config_schema": 1,
        "genesis": genesis,
        "build": {"profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH}})
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The `kB` value of a `/proc` status line, in MB.
fn proc_mb(text: &str, key: &str) -> Option<i64> {
    let line = text.lines().find(|l| l.starts_with(key))?;
    let kb: i64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

/// The host and the process: kernel, cores, memory, resident size, threads, open files.
pub fn host() -> Value {
    let status = read("/proc/self/status");
    let threads = status
        .lines()
        .find(|l| l.starts_with("Threads:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse::<i64>().ok());
    let fds = std::fs::read_dir("/proc/self/fd").map(|d| d.count()).ok();
    json!({"kernel": read("/proc/sys/kernel/osrelease").trim(),
        "cores": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        "memory_mb": proc_mb(&read("/proc/meminfo"), "MemTotal:"),
        "memory_available_mb": proc_mb(&read("/proc/meminfo"), "MemAvailable:"),
        "process": {"pid": std::process::id(), "rss_mb": proc_mb(&status, "VmRSS:"),
            "threads": threads, "open_files": fds}})
}

/// The kinds of security-log entries and their counts (`security_log` of an identity),
/// without the entries.
pub fn log_summary(log: &Value) -> Value {
    let mut kinds: std::collections::BTreeMap<String, u64> = Default::default();
    for e in log["entries"].as_array().into_iter().flatten() {
        *kinds
            .entry(e["kind"].as_str().unwrap_or("?").to_string())
            .or_default() += 1;
    }
    json!({"head_seq": log["head"]["seq"], "recent_entries": log["entries"].as_array().map_or(0, Vec::len),
        "kinds": kinds})
}

/// Finish a bundle: look for the secrets in its text and redact them, then add the
/// manifest `{format, mode, generated_at, duration_ms, sections, redactions,
/// content_hash}`.
pub fn finish(
    sections: Map<String, Value>,
    secrets: &[String],
    mode: &str,
    started: std::time::Instant,
) -> Value {
    let names: Vec<String> = sections.keys().cloned().collect();
    let mut text = serde_json::to_string(&Value::Object(sections)).unwrap_or_default();
    let mut redactions = 0;
    for s in secrets {
        // the secret as it appears inside a JSON string
        let enc = serde_json::to_string(s).unwrap_or_default();
        let enc = enc.trim_matches('"');
        if enc.is_empty() {
            continue;
        }
        let n = text.matches(enc).count();
        if n > 0 {
            redactions += n;
            text = text.replace(enc, "[redacted]");
        }
    }
    let mut body: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
    let hash = content_id("ubos.chunk.v1", &body).unwrap_or_default();
    let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
    let manifest = json!({"format": FORMAT, "mode": mode,
        "generated_at": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "duration_ms": started.elapsed().as_millis() as u64, "sections": names,
        "redactions": redactions, "content_hash": hash});
    if let Some(m) = body.as_object_mut() {
        let mut out = Map::new();
        out.insert("manifest".into(), manifest);
        out.extend(std::mem::take(m));
        return Value::Object(out);
    }
    json!({"manifest": manifest})
}

/// The database without its content: the server version, size, connections, applied
/// migrations, per-table row estimates and sizes, and the tenants by state.
pub fn database(db: &crate::db::Db) -> Value {
    let one = |sql: &str| {
        db.query(sql, &[])
            .map(|r| r.into_iter().next().unwrap_or(Value::Null))
            .unwrap_or_else(|e| json!({"error": e.to_string()}))
    };
    let all = |sql: &str| {
        db.query(sql, &[])
            .map(Value::from)
            .unwrap_or_else(|e| json!({"error": e.to_string()}))
    };
    json!({
        "server": one("SELECT current_setting('server_version') AS version, pg_database_size(current_database())::bigint AS size_bytes, \
            (SELECT count(*) FROM pg_stat_activity WHERE datname = current_database())::bigint AS connections, \
            current_setting('max_connections')::int AS max_connections"),
        "migrations": one("SELECT max(version)::bigint AS latest, count(*)::bigint AS applied, bool_and(success) AS all_succeeded FROM _sqlx_migrations"),
        "tables": all("SELECT relname::text AS table, n_live_tup::bigint AS rows_estimate, pg_total_relation_size(relid)::bigint AS bytes \
            FROM pg_stat_user_tables ORDER BY relname"),
        "tenants": all("SELECT state, count(*)::bigint AS tenants FROM tenant GROUP BY state ORDER BY state"),
    })
}
