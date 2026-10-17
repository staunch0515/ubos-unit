//! Traces, metrics and logs of the node over OTLP (PH-1 step 16c, DEC-077; DSN-NOD-610,
//! -611; FR-OPS-041, -042; NR-OBS-001).
//!
//! * Traces: a span per UBTP request at the edge (its parent from the message's W3C
//!   `trace`, or the `traceparent` header), a span per DVM operation with the kernel's
//!   phases (`dvm.rules`, `dvm.flush`, `store.flush`) as children, and a span per job
//!   linked to the trace of the request whose process started it. Trace and process IDs
//!   are attributes.
//! * Metrics: per span name, the count and the summed duration; node gauges.
//! * Logs: the node's operational events (already redacted, step 7a) and self-check
//!   alerts.
//!
//! Span attributes pass an allow-list ([`ATTRIBUTES`], DSN-NOD-611): no field value,
//! object name or document content reaches telemetry. Export (`telemetry.export`): `none`
//! (kept in a ring for the operator API), `stderr` (JSON lines) or `otlp` (OTLP/HTTP with
//! JSON encoding to `telemetry.otlp_endpoint`, `/v1/traces`, `/v1/metrics`, `/v1/logs`,
//! every second; `http://` endpoints — a local collector — in PH-1). The queue is bounded:
//! spans beyond it are dropped and counted.

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// The attribute keys a span may carry (DSN-NOD-611).
pub const ATTRIBUTES: &[&str] = &[
    "ubos.message_type",
    "ubos.op",
    "ubos.process_id",
    "ubos.trace_id",
    "ubos.tenant",
    "ubos.vae",
    "ubos.job_id",
    "ubos.job_kind",
    "ubos.error_code",
    "ubos.session",
    "ubos.node",
    "http.method",
    "http.route",
    "http.status_code",
];

/// Spans kept for the export queue, and in the ring for the operator API.
pub const QUEUE: usize = 10_000;
/// Spans kept in the ring.
pub const RING: usize = 2_000;

/// A finished span.
#[derive(Debug, Clone)]
pub struct Span {
    /// 32 hex digits.
    pub trace_id: String,
    /// 16 hex digits.
    pub span_id: String,
    /// The parent span, if any.
    pub parent: Option<String>,
    /// `ubtp.<type>`, `dvm.<op>`, `dvm.flush`, `job.<kind>`, …
    pub name: String,
    /// Unix nanoseconds.
    pub start_ns: u64,
    /// Unix nanoseconds.
    pub end_ns: u64,
    /// Allow-listed attributes.
    pub attributes: Map<String, Value>,
    /// The error code when the span failed.
    pub error: Option<String>,
}

impl Span {
    /// The span as JSON (the stderr export and the operator API).
    pub fn to_json(&self) -> Value {
        json!({"trace_id": self.trace_id, "span_id": self.span_id, "parent": self.parent, "name": self.name,
            "start_ns": self.start_ns, "end_ns": self.end_ns, "duration_us": (self.end_ns.saturating_sub(self.start_ns)) / 1000,
            "attributes": self.attributes, "error": self.error})
    }
}

/// A trace context: the trace, the parent span and whether it is sampled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// 32 hex digits.
    pub trace_id: String,
    /// 16 hex digits.
    pub span_id: String,
    /// Sampled.
    pub sampled: bool,
}

impl Context {
    /// Parse a W3C `traceparent` (`00-<trace>-<span>-<flags>`).
    pub fn parse(tp: &str) -> Option<Context> {
        let p: Vec<&str> = tp.trim().split('-').collect();
        let hex = |s: &str, n: usize| s.len() == n && s.chars().all(|c| c.is_ascii_hexdigit());
        if p.len() != 4 || !hex(p[0], 2) || !hex(p[1], 32) || !hex(p[2], 16) || !hex(p[3], 2) {
            return None;
        }
        if p[1].chars().all(|c| c == '0') || p[2].chars().all(|c| c == '0') {
            return None;
        }
        let flags = u8::from_str_radix(p[3], 16).ok()?;
        Some(Context {
            trace_id: p[1].to_ascii_lowercase(),
            span_id: p[2].to_ascii_lowercase(),
            sampled: flags & 1 == 1,
        })
    }
    /// The `traceparent` of this context.
    pub fn traceparent(&self) -> String {
        format!(
            "00-{}-{}-{}",
            self.trace_id,
            self.span_id,
            if self.sampled { "01" } else { "00" }
        )
    }
}

fn random_hex(n: usize) -> String {
    let b: [u8; 16] = ubos_dvm::crypto::random_bytes().unwrap_or([7u8; 16]);
    b.iter().map(|x| format!("{x:02x}")).collect::<String>()[..n].to_string()
}

/// A new span identifier.
pub fn span_id() -> String {
    random_hex(16)
}

/// Unix nanoseconds now.
pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default()
}

/// Keep only the allow-listed attributes, as scalars (DSN-NOD-611).
pub fn scrub(attrs: &Map<String, Value>) -> Map<String, Value> {
    attrs
        .iter()
        .filter(|(k, v)| ATTRIBUTES.contains(&k.as_str()) && !v.is_object() && !v.is_array())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Where telemetry goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    /// `none`, `stderr` or `otlp`.
    pub mode: String,
    /// The OTLP/HTTP endpoint.
    pub endpoint: Option<String>,
    /// The share of new traces kept.
    pub ratio: f64,
}

/// The node's tracer.
pub struct Tracer {
    export: Mutex<Export>,
    queue: Mutex<VecDeque<Span>>,
    ring: Mutex<VecDeque<Span>>,
    logs: Mutex<VecDeque<Value>>,
    metrics: Mutex<BTreeMap<String, (u64, u64, u64)>>,
    /// Spans dropped because the queue was full.
    pub dropped: AtomicU64,
    /// Export failures.
    pub export_errors: AtomicU64,
    /// The last export error.
    pub last_error: Mutex<Option<String>>,
    node: Mutex<String>,
}

impl Tracer {
    /// A tracer of a node.
    pub fn new(node: &str) -> Tracer {
        Tracer {
            export: Mutex::new(Export {
                mode: "none".into(),
                endpoint: None,
                ratio: 1.0,
            }),
            queue: Mutex::new(VecDeque::new()),
            ring: Mutex::new(VecDeque::new()),
            logs: Mutex::new(VecDeque::new()),
            metrics: Mutex::new(BTreeMap::new()),
            dropped: AtomicU64::new(0),
            export_errors: AtomicU64::new(0),
            last_error: Mutex::new(None),
            node: Mutex::new(node.to_string()),
        }
    }

    /// The node identifier the spans carry.
    pub fn set_node(&self, node: &str) {
        *self.node.lock().expect("node") = node.to_string();
    }

    /// Apply the telemetry keys of the configuration.
    pub fn configure(&self, e: Export) {
        *self.export.lock().expect("export") = e;
    }

    /// The export settings.
    pub fn export(&self) -> Export {
        self.export.lock().expect("export").clone()
    }

    /// The context of a new span: a child of `parent`, or a new trace sampled by the
    /// configured ratio.
    pub fn child(&self, parent: Option<&Context>) -> Context {
        match parent {
            Some(p) => Context {
                trace_id: p.trace_id.clone(),
                span_id: span_id(),
                sampled: p.sampled,
            },
            None => {
                let trace_id = random_hex(32);
                let ratio = self.export().ratio;
                let bucket =
                    u64::from_str_radix(&trace_id[..8], 16).unwrap_or(0) as f64 / u32::MAX as f64;
                Context {
                    trace_id,
                    span_id: span_id(),
                    sampled: bucket < ratio,
                }
            }
        }
    }

    /// Record a finished span (its attributes scrubbed); unsampled spans count in the
    /// metrics only.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        ctx: &Context,
        parent: Option<&str>,
        name: &str,
        start_ns: u64,
        end_ns: u64,
        attributes: &Map<String, Value>,
        error: Option<&str>,
    ) {
        {
            let mut m = self.metrics.lock().expect("metrics");
            let e = m.entry(name.to_string()).or_insert((0, 0, 0));
            e.0 += 1;
            e.1 += end_ns.saturating_sub(start_ns) / 1000;
            if error.is_some() {
                e.2 += 1;
            }
        }
        if !ctx.sampled {
            return;
        }
        let mut attrs = scrub(attributes);
        attrs.insert("ubos.trace_id".into(), json!(ctx.trace_id));
        attrs.insert("ubos.node".into(), json!(*self.node.lock().expect("node")));
        let s = Span {
            trace_id: ctx.trace_id.clone(),
            span_id: ctx.span_id.clone(),
            parent: parent.map(str::to_string),
            name: name.to_string(),
            start_ns,
            end_ns,
            attributes: attrs,
            error: error.map(str::to_string),
        };
        {
            let mut r = self.ring.lock().expect("ring");
            if r.len() >= RING {
                r.pop_front();
            }
            r.push_back(s.clone());
        }
        if self.export().mode != "none" {
            let mut q = self.queue.lock().expect("queue");
            if q.len() >= QUEUE {
                q.pop_front();
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            q.push_back(s);
        }
    }

    /// Record a log record (already redacted).
    pub fn log(&self, severity: &str, body: Value) {
        if self.export().mode == "none" {
            return;
        }
        let mut l = self.logs.lock().expect("logs");
        if l.len() >= QUEUE {
            l.pop_front();
        }
        l.push_back(json!({"severity": severity, "time_ns": now_ns(), "body": body}));
    }

    /// The recent spans (the operator API), of one trace or all.
    pub fn recent(&self, trace: Option<&str>, limit: usize) -> Vec<Value> {
        self.ring
            .lock()
            .expect("ring")
            .iter()
            .rev()
            .filter(|s| trace.is_none_or(|t| s.trace_id == t))
            .take(limit)
            .map(Span::to_json)
            .collect()
    }

    /// The metrics: per span name `{count, total_us, errors}`.
    pub fn metrics(&self) -> Value {
        Value::Object(
            self.metrics
                .lock()
                .expect("metrics")
                .iter()
                .map(|(k, (c, t, e))| (k.clone(), json!({"count": c, "total_us": t, "errors": e})))
                .collect(),
        )
    }

    /// Export what is queued (once a second): spans, logs and, every `with_metrics` call,
    /// the metrics with the node gauges.
    pub fn flush(&self, gauges: &Value, with_metrics: bool) {
        let e = self.export();
        let spans: Vec<Span> = self.queue.lock().expect("queue").drain(..).collect();
        let logs: Vec<Value> = self.logs.lock().expect("logs").drain(..).collect();
        match e.mode.as_str() {
            "stderr" => {
                for s in &spans {
                    eprintln!("ubos-trace {}", s.to_json());
                }
                for l in &logs {
                    eprintln!("ubos-log {l}");
                }
            }
            "otlp" => {
                let Some(ep) = e.endpoint.clone() else {
                    return;
                };
                let node = self.node.lock().expect("node").clone();
                let res = json!({"attributes": [kv("service.name", &json!("ubos-node")), kv("service.instance.id", &json!(node))]});
                if !spans.is_empty() {
                    let body = json!({"resourceSpans": [{"resource": res, "scopeSpans": [{"scope": {"name": "ubos-node"},
                        "spans": spans.iter().map(otlp_span).collect::<Vec<_>>()}]}]});
                    self.post(&ep, "/v1/traces", &body);
                }
                if !logs.is_empty() {
                    let body = json!({"resourceLogs": [{"resource": res, "scopeLogs": [{"scope": {"name": "ubos-node"},
                        "logRecords": logs.iter().map(|l| json!({"timeUnixNano": l["time_ns"].to_string(), "severityText": l["severity"],
                            "body": {"stringValue": l["body"].to_string()}})).collect::<Vec<_>>()}]}]});
                    self.post(&ep, "/v1/logs", &body);
                }
                if with_metrics {
                    let now = now_ns().to_string();
                    let mut points = Vec::new();
                    for (name, v) in self.metrics().as_object().into_iter().flatten() {
                        for (k, unit) in [("count", "1"), ("total_us", "us"), ("errors", "1")] {
                            points.push(json!({"name": format!("ubos.span.{k}"), "unit": unit, "sum": {"aggregationTemporality": 2, "isMonotonic": true,
                                "dataPoints": [{"asInt": v[k].as_u64().unwrap_or(0).to_string(), "timeUnixNano": now, "attributes": [kv("span.name", &json!(name))]}]}}));
                        }
                    }
                    for (k, v) in gauges.as_object().into_iter().flatten() {
                        points.push(json!({"name": format!("ubos.{k}"), "unit": "1", "gauge": {"dataPoints": [{"asInt": v.as_i64().unwrap_or(0).to_string(), "timeUnixNano": now}]}}));
                    }
                    let body = json!({"resourceMetrics": [{"resource": res, "scopeMetrics": [{"scope": {"name": "ubos-node"}, "metrics": points}]}]});
                    self.post(&ep, "/v1/metrics", &body);
                }
            }
            _ => {}
        }
    }

    fn post(&self, endpoint: &str, path: &str, body: &Value) {
        if let Err(e) = http_post(endpoint, path, body) {
            self.export_errors.fetch_add(1, Ordering::Relaxed);
            *self.last_error.lock().expect("last error") = Some(e);
        }
    }
}

fn kv(k: &str, v: &Value) -> Value {
    let any = match v {
        Value::String(s) => json!({"stringValue": s}),
        Value::Bool(b) => json!({"boolValue": b}),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({"intValue": n.to_string()}),
        Value::Number(n) => json!({"doubleValue": n.as_f64()}),
        other => json!({"stringValue": other.to_string()}),
    };
    json!({"key": k, "value": any})
}

fn otlp_span(s: &Span) -> Value {
    let mut o = json!({"traceId": s.trace_id, "spanId": s.span_id, "name": s.name, "kind": 1,
        "startTimeUnixNano": s.start_ns.to_string(), "endTimeUnixNano": s.end_ns.to_string(),
        "attributes": s.attributes.iter().map(|(k, v)| kv(k, v)).collect::<Vec<_>>(),
        "status": if s.error.is_some() { json!({"code": 2, "message": s.error}) } else { json!({"code": 1}) }});
    if let Some(p) = &s.parent {
        o["parentSpanId"] = json!(p);
    }
    o
}

/// POST JSON to an `http://host:port` endpoint (HTTP/1.1, a 5 s timeout).
fn http_post(endpoint: &str, path: &str, body: &Value) -> Result<(), String> {
    use std::io::{Read, Write};
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or("PH-1 exports to http:// endpoints (a local collector); https needs a TLS client")?;
    let host = rest.split('/').next().unwrap_or(rest);
    let base = rest[host.len()..].trim_end_matches('/');
    let addr = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    let sock = std::net::ToSocketAddrs::to_socket_addrs(&addr)
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("no address")?;
    let mut s = std::net::TcpStream::connect_timeout(&sock, std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let b = body.to_string();
    write!(
        s,
        "POST {base}{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
        b.len()
    )
    .map_err(|e| e.to_string())?;
    let mut head = [0u8; 32];
    let n = s.read(&mut head).map_err(|e| e.to_string())?;
    let line = String::from_utf8_lossy(&head[..n]);
    let code = line.split_whitespace().nth(1).unwrap_or("0");
    if code.starts_with('2') {
        Ok(())
    } else {
        Err(format!("the collector answered {code}"))
    }
}
