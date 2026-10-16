//! Node configuration (PH-1 step 16a, DEC-077; DSN-NOD-603…-605; FR-OPS-091…-093;
//! NR-OPER-003).
//!
//! The configuration of a node is a versioned TOML document (`ubos.toml`) checked against
//! [`SCHEMA`]: every key has a type, a range and whether it is reloadable. A document
//! with an unknown key, a wrong type, a value out of range or an inline secret is rejected
//! as a whole, with every error named, and the running configuration stays (FR-OPS-091).
//! Secrets are references only: `env:NAME`, `file:/path` or `kms:<provider>/<key>`
//! (DSN-NOD-605). An applied version is recorded with its hash, author and diff
//! (FR-OPS-093); reloadable keys take effect at once, the others at the next start
//! (DSN-NOD-604).
//!
//! ```toml
//! version = 1
//! [server]
//! listen = "0.0.0.0:8080"
//! preset = "server"
//! [database]
//! url = "env:UBOS_DATABASE_URL"
//! [limits]
//! requests_per_second = 50
//! ```

use serde_json::{Map, Value, json};
use ubos_dvm::types::{BpaError, content_id};

/// The type of a configuration key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Text, at most 1,024 characters.
    Text,
    /// One of the listed values.
    Choice(&'static [&'static str]),
    /// An integer in `[min, max]`.
    Int(i64, i64),
    /// A number in `[min, max]`.
    Float(f64, f64),
    /// A socket address `host:port`.
    Address,
    /// An `http://` or `https://` URL.
    Url,
    /// An absolute path.
    Path,
    /// A secret: a reference only.
    Secret,
    /// A PostgreSQL connection string: a reference, or inline without a password.
    Database,
    /// A tenant slug or a VAE authority (`[a-z0-9-]`, 1…63).
    Name,
}

/// A key of the schema.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// `section.key`.
    pub path: &'static str,
    /// Its type and range.
    pub kind: Kind,
    /// The default (JSON text), or `None` when the key is optional without one.
    pub default: Option<&'static str>,
    /// Whether it takes effect without a restart.
    pub reloadable: bool,
    /// What it sets.
    pub description: &'static str,
}

const fn key(
    path: &'static str,
    kind: Kind,
    default: Option<&'static str>,
    reloadable: bool,
    description: &'static str,
) -> Key {
    Key {
        path,
        kind,
        default,
        reloadable,
        description,
    }
}

/// The log levels.
pub const LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];

/// The schema of `ubos.toml` (schema version 1).
pub const SCHEMA: &[Key] = &[
    key(
        "server.listen",
        Kind::Address,
        Some("\"127.0.0.1:8080\""),
        false,
        "the address the Server listens on",
    ),
    key(
        "server.preset",
        Kind::Choice(&["dev", "server"]),
        Some("\"server\""),
        false,
        "the edition preset (DSN-NOD-004)",
    ),
    key(
        "server.tenant",
        Kind::Name,
        Some("\"acme\""),
        false,
        "the default tenant",
    ),
    key(
        "server.vae",
        Kind::Name,
        Some("\"acme\""),
        false,
        "the default VAE authority",
    ),
    key(
        "database.url",
        Kind::Database,
        None,
        false,
        "the PostgreSQL connection string (memory store when absent)",
    ),
    key(
        "keys.root_kek",
        Kind::Secret,
        None,
        false,
        "the root key-encryption key (64 hex digits), as a reference",
    ),
    key(
        "log.level",
        Kind::Choice(LEVELS),
        Some("\"info\""),
        true,
        "the log level",
    ),
    key(
        "telemetry.export",
        Kind::Choice(&["none", "stderr", "otlp"]),
        Some("\"none\""),
        true,
        "where telemetry goes",
    ),
    key(
        "telemetry.otlp_endpoint",
        Kind::Url,
        None,
        true,
        "the OTLP/HTTP endpoint",
    ),
    key(
        "telemetry.sample_ratio",
        Kind::Float(0.0, 1.0),
        Some("1.0"),
        true,
        "the share of traces kept",
    ),
    key(
        "limits.requests_per_second",
        Kind::Int(0, 100_000),
        Some("0"),
        true,
        "requests per principal per second (0: no limit)",
    ),
    key(
        "limits.burst",
        Kind::Int(1, 100_000),
        Some("20"),
        true,
        "the burst of the request limit",
    ),
    key(
        "backup.dir",
        Kind::Path,
        None,
        true,
        "the backup directory (a separate failure domain)",
    ),
    key(
        "backup.key",
        Kind::Secret,
        None,
        true,
        "the backup key (64 hex digits), as a reference",
    ),
    key(
        "backup.retention_days",
        Kind::Int(1, 365),
        Some("35"),
        true,
        "the point-in-time window in days",
    ),
    key(
        "bootstrap.admin_username",
        Kind::Text,
        Some("\"admin\""),
        false,
        "the first administrator's username",
    ),
];

/// The schema version of this build.
pub const SCHEMA_VERSION: i64 = 1;

fn invalid(errors: Vec<Value>) -> BpaError {
    let first = errors
        .first()
        .and_then(|e| e["message"].as_str())
        .unwrap_or("invalid configuration")
        .to_string();
    BpaError::with_details(
        "MODEL.INVALID_VALUE",
        json!({"reason": "configuration_invalid", "errors": errors, "message": first}),
    )
}

/// Whether a text is a secret reference (`env:NAME`, `file:/path`, `kms:<provider>/<key>`).
pub fn is_reference(v: &str) -> bool {
    if let Some(n) = v.strip_prefix("env:") {
        return !n.is_empty()
            && n.chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
            && n.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    }
    if let Some(p) = v.strip_prefix("file:") {
        return p.starts_with('/') && p.len() > 1 && !p.contains(char::is_whitespace);
    }
    if let Some(k) = v.strip_prefix("kms:") {
        return k.split_once('/').is_some_and(|(prov, rest)| {
            !prov.is_empty()
                && prov
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && !rest.is_empty()
                && !rest.contains(char::is_whitespace)
        });
    }
    false
}

/// Whether an inline connection string carries a password.
fn has_password(url: &str) -> bool {
    let l = url.to_ascii_lowercase();
    if l.contains("password=") {
        return true;
    }
    // postgres://user:password@host
    if let Some(rest) = l.split_once("://").map(|x| x.1) {
        let auth = rest.split(['@', '/']).next().unwrap_or_default();
        return rest.contains('@') && auth.contains(':');
    }
    false
}

fn check_value(k: &Key, v: &Value) -> Result<(), String> {
    let text = || v.as_str().ok_or_else(|| "a text value".to_string());
    match k.kind {
        Kind::Text => {
            let t = text()?;
            if t.is_empty() || t.chars().count() > 1024 {
                return Err("1 to 1,024 characters".into());
            }
        }
        Kind::Choice(list) => {
            let t = text()?;
            if !list.contains(&t) {
                return Err(format!("one of {}", list.join(", ")));
            }
        }
        Kind::Int(lo, hi) => {
            let n = v.as_i64().ok_or("an integer")?;
            if n < lo || n > hi {
                return Err(format!("an integer from {lo} to {hi}"));
            }
        }
        Kind::Float(lo, hi) => {
            let n = v.as_f64().ok_or("a number")?;
            if !(lo..=hi).contains(&n) {
                return Err(format!("a number from {lo} to {hi}"));
            }
        }
        Kind::Address => {
            let t = text()?;
            if t.parse::<std::net::SocketAddr>().is_err() {
                return Err("an address host:port (an IP address and a port)".into());
            }
        }
        Kind::Url => {
            let t = text()?;
            let ok = (t.starts_with("http://") || t.starts_with("https://"))
                && t.len() > 8
                && !t.contains(char::is_whitespace);
            if !ok {
                return Err("an http:// or https:// URL".into());
            }
        }
        Kind::Path => {
            let t = text()?;
            if !t.starts_with('/') || t.contains('\0') || t.len() > 4096 {
                return Err("an absolute path".into());
            }
        }
        Kind::Secret => {
            let t = text()?;
            if !is_reference(t) {
                return Err(
                    "an inline secret: write a reference (env:NAME, file:/path or kms:provider/key)"
                        .into(),
                );
            }
        }
        Kind::Database => {
            let t = text()?;
            if is_reference(t) {
                return Ok(());
            }
            if ["env:", "file:", "kms:"].iter().any(|p| t.starts_with(p)) {
                return Err(
                    "a well-formed reference (env:NAME, file:/path or kms:provider/key)".into(),
                );
            }
            if has_password(t) {
                return Err("an inline database password: write a reference (env:NAME, file:/path or kms:provider/key)".into());
            }
            if t.is_empty() || t.len() > 4096 {
                return Err("a connection string".into());
            }
        }
        Kind::Name => {
            let t = text()?;
            let ok = !t.is_empty()
                && t.len() <= 63
                && t.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && !t.starts_with('-');
            if !ok {
                return Err("a name of 1 to 63 characters a-z, 0-9 and -".into());
            }
        }
    }
    Ok(())
}

/// A validated configuration: every key of [`SCHEMA`] with its value (defaults filled in)
/// and the hash of the document as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    values: Map<String, Value>,
    hash: String,
    text: String,
}

fn toml_to_json(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Float(f) => json!(f),
        toml::Value::Boolean(b) => json!(b),
        toml::Value::Datetime(d) => json!(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            Value::Object(t.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect())
        }
    }
}

impl Config {
    /// Parse and validate a document; every error is named (`MODEL.INVALID_VALUE` with
    /// `details.errors`).
    pub fn parse(text: &str) -> Result<Config, BpaError> {
        let doc: toml::Value = text.parse().map_err(|e: toml::de::Error| {
            invalid(vec![
                json!({"key": null, "message": format!("not TOML: {}", e.message())}),
            ])
        })?;
        let doc = toml_to_json(doc);
        let mut errors = Vec::new();
        let mut values = Map::new();
        let Some(top) = doc.as_object() else {
            return Err(invalid(vec![json!({"key": null, "message": "a table"})]));
        };
        match top.get("version") {
            Some(v) if v.as_i64() == Some(SCHEMA_VERSION) => {}
            Some(v) => errors.push(json!({"key": "version", "message": format!("schema version {SCHEMA_VERSION} (found {v})")})),
            None => errors.push(json!({"key": "version", "message": format!("version = {SCHEMA_VERSION} is required")})),
        }
        for (section, body) in top {
            if section == "version" {
                continue;
            }
            let Some(tbl) = body.as_object() else {
                errors.push(json!({"key": section, "message": "a section ([name])"}));
                continue;
            };
            for (k, v) in tbl {
                let path = format!("{section}.{k}");
                match SCHEMA.iter().find(|x| x.path == path) {
                    None => errors.push(json!({"key": path, "message": "an unknown key"})),
                    Some(def) => match check_value(def, v) {
                        Ok(()) => {
                            values.insert(path, v.clone());
                        }
                        Err(m) => {
                            // "is an inline secret …" or "expects <type>"
                            let m = if m.starts_with("an inline") {
                                format!("is {m}")
                            } else {
                                format!("expects {m}")
                            };
                            errors.push(json!({"key": path, "message": m}))
                        }
                    },
                }
            }
        }
        // keys that need one another
        let get = |k: &str| values.get(k).and_then(Value::as_str);
        if get("telemetry.export") == Some("otlp")
            && !values.contains_key("telemetry.otlp_endpoint")
        {
            errors.push(json!({"key": "telemetry.otlp_endpoint", "message": "required when telemetry.export is otlp"}));
        }
        if values.contains_key("backup.dir") != values.contains_key("backup.key") {
            errors.push(
                json!({"key": "backup.key", "message": "backup.dir and backup.key go together"}),
            );
        }
        if get("server.preset") != Some("dev")
            && values.contains_key("database.url")
            && !values.contains_key("keys.root_kek")
        {
            errors.push(json!({"key": "keys.root_kek", "message": "required with a database in the server preset"}));
        }
        if !errors.is_empty() {
            return Err(invalid(errors));
        }
        for k in SCHEMA {
            if let (false, Some(d)) = (values.contains_key(k.path), k.default) {
                values.insert(
                    k.path.to_string(),
                    serde_json::from_str(d).unwrap_or(Value::Null),
                );
            }
        }
        let hash = content_id("ubos.chunk.v1", &json!({"config": text}))?;
        Ok(Config {
            values,
            hash,
            text: text.to_string(),
        })
    }

    /// The defaults (no file).
    pub fn defaults() -> Config {
        Config::parse("version = 1\n[server]\npreset = \"dev\"\n").unwrap_or_else(|_| Config {
            values: Map::new(),
            hash: String::new(),
            text: String::new(),
        })
    }

    /// A value.
    pub fn get(&self, path: &str) -> Option<&Value> {
        self.values.get(path).filter(|v| !v.is_null())
    }
    /// A text value.
    pub fn text(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(Value::as_str)
    }
    /// An integer value.
    pub fn int(&self, path: &str) -> i64 {
        self.get(path).and_then(Value::as_i64).unwrap_or(0)
    }
    /// A number value.
    pub fn float(&self, path: &str) -> f64 {
        self.get(path).and_then(Value::as_f64).unwrap_or(0.0)
    }
    /// The content hash of the document.
    pub fn hash(&self) -> &str {
        &self.hash
    }
    /// The document as written.
    pub fn document(&self) -> &str {
        &self.text
    }
    /// Every key with its value (references, never secrets).
    pub fn values(&self) -> Value {
        Value::Object(self.values.clone())
    }

    /// The keys that differ from `old`: `[{key, from, to, reloadable}]`.
    pub fn diff(&self, old: &Config) -> Vec<Value> {
        SCHEMA
            .iter()
            .filter(|k| self.values.get(k.path) != old.values.get(k.path))
            .map(|k| {
                json!({"key": k.path, "from": old.values.get(k.path).cloned().unwrap_or(Value::Null),
                    "to": self.values.get(k.path).cloned().unwrap_or(Value::Null), "reloadable": k.reloadable})
            })
            .collect()
    }

    /// The running configuration after a reload to `new`: the reloadable keys from `new`,
    /// the others kept (they take effect at the next start).
    pub fn reloaded(&self, new: &Config) -> Config {
        let mut values = self.values.clone();
        for k in SCHEMA.iter().filter(|k| k.reloadable) {
            match new.values.get(k.path) {
                Some(v) => values.insert(k.path.to_string(), v.clone()),
                None => values.remove(k.path),
            };
        }
        Config {
            values,
            hash: new.hash.clone(),
            text: new.text.clone(),
        }
    }
}

/// Resolve a secret reference: `env:NAME` from the environment, `file:/path` from the
/// file (trimmed). `kms:` references need the KMS integration (PH-2).
pub fn resolve(reference: &str) -> Result<String, BpaError> {
    if let Some(n) = reference.strip_prefix("env:") {
        return std::env::var(n).map_err(|_| {
            BpaError::new(
                "IAM.KEY_UNAVAILABLE",
                format!("the environment variable {n} is not set"),
            )
        });
    }
    if let Some(p) = reference.strip_prefix("file:") {
        return std::fs::read_to_string(p)
            .map(|t| t.trim().to_string())
            .map_err(|e| BpaError::new("IAM.KEY_UNAVAILABLE", format!("{p}: {e}")));
    }
    if reference.starts_with("kms:") {
        return Err(BpaError::new(
            "IAM.KEY_UNAVAILABLE",
            "kms: references need the KMS integration (PH-2); use env: or file:",
        ));
    }
    Ok(reference.to_string())
}

/// A secret-free description of the schema (operators and the diagnostic bundle).
pub fn schema() -> Value {
    Value::Array(
        SCHEMA
            .iter()
            .map(|k| {
                json!({"key": k.path, "type": format!("{:?}", k.kind), "default": k.default.and_then(|d| serde_json::from_str::<Value>(d).ok()),
                    "reloadable": k.reloadable, "description": k.description})
            })
            .collect(),
    )
}
