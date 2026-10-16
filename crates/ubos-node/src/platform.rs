//! The event platform of a VAE (PH-1 step 15b, DEC-076; DSN-NOD-203, -204, -209;
//! FR-EVT-021…-024, -051, -052).
//!
//! * The platform class `Subscription` (namespace `ubos.platform`, outside BPA 1.0), which
//!   the node declares in every VAE it opens: a Definition object with a name, the event
//!   types it listens to (`["*"]` for all), an optional selector (a JSON pattern that the
//!   event `{type, object_id, branch, payload}` must contain), the branch scope (`["main"]`
//!   by default; `prefix/*` patterns), the handler (`{kind: "port", port, target?, params?}`
//!   or `{kind: "logic", asset, function, args?}`; PH-1), the service principal it runs as,
//!   the retry policy (FR-EVT-051) and an owner.
//! * The listener registry: the subscriptions an event on a branch reaches are the enabled
//!   `Subscription` objects of that branch's state whose scope includes the branch, so a
//!   subscription committed on a draft does not fire for `main` events (FR-EVT-023), and one
//!   scoped to `main` does not fire on a work branch (DSN-NOD-203 acceptance 1).
//! * Dispatch: the releaser enqueues one delivery job per subscription and event (dedupe key
//!   `<subscription>:<event>`) before it acknowledges the outbox, so a crash between them
//!   repeats the enqueue, never loses it.
//! * Delivery: the handler runs as one process of the subscription's service principal with
//!   the process identifier derived from `(subscription, event)` (DSN-NOD-204); a delivery
//!   whose process already committed is done without running again, so a handler that
//!   crashed after its commit and before the acknowledgement commits once.
//!
//! Parameters of a handler may refer to the event: a string `$event.<path>` is replaced by
//! that member of the event (`$event.object_id`, `$event.payload.amount`, …).

use serde_json::{Map, Value, json};
use ubos_dvm::host::Vae;
use ubos_dvm::types::{BpaError, content_id};
use ubos_dvm::vectors::subset;

/// The namespace of the platform classes.
pub const NAMESPACE: &str = "ubos.platform";

fn field(name: &str, ty: &str, required: bool) -> Value {
    json!({"name": name, "label": name.replace('_', " "), "description": name.replace('_', " "), "type": ty, "required": required})
}

/// The declaration of `Subscription`.
pub fn subscription_class() -> Value {
    json!({"name": "Subscription", "label": "Subscription", "namespace": NAMESPACE,
    "description": "An event subscription of the platform: event types, selector, branch scope, handler, service principal, retry policy and owner (FR-EVT-021)",
    "parent": "DataObject", "kind": "definition", "schema_version": 1,
    "fields": [
        field("name", "text", true),
        field("event_types", "json", true),
        field("selector", "json", false),
        field("branches", "json", false),
        field("handler", "json", true),
        field("run_as", "text", true),
        field("policy", "json", false),
        field("owner", "text", false),
        field("enabled", "boolean", false),
    ]})
}

/// The declaration of `Job` (FR-EVT-031): a System-kind object per job, written by the
/// platform only (`system_commit`); the lease and its fencing token live in the `job`
/// table (DEC-076).
pub fn job_class() -> Value {
    json!({"name": "Job", "label": "Job", "namespace": NAMESPACE,
    "description": "An asynchronous job of the platform: type, queue, payload, state, attempts, last error, priority (FR-EVT-031)",
    "parent": "SystemObject", "kind": "system", "schema_version": 1,
    "fields": [
        field("job_type", "text", true),
        field("queue", "text", false),
        field("payload", "json", false),
        field("state", "text", true),
        field("attempts", "integer", false),
        field("max_attempts", "integer", false),
        field("last_error", "text", false),
        field("priority", "integer", false),
        field("dedupe_key", "text", false),
        field("result", "json", false),
        field("poison", "boolean", false),
    ]})
}

/// The declaration of `Schedule` (FR-EVT-041, DSN-NOD-224): a Definition object with a
/// rule (`{cron}` or `{calendar, at, nth?}`), a time zone, an active window
/// (`{from?, to?}`), a misfire policy (`skip`, `fire_once` or `fire_all`; `fire_once` when
/// absent), a target (`{kind: "port", target, port, params?}` or `{kind: "logic", asset,
/// function, args?}`, on `main` unless it names a `branch`; workflow targets in PH-2,
/// DEC-076), the service principal it runs
/// as, and whether it is enabled and paused.
pub fn schedule_class() -> Value {
    json!({"name": "Schedule", "label": "Schedule", "namespace": NAMESPACE,
    "description": "A schedule of the platform: rule, time zone, active window, misfire policy, target and service principal (FR-EVT-041)",
    "parent": "DataObject", "kind": "definition", "schema_version": 1,
    "fields": [
        field("name", "text", true),
        field("rule", "json", true),
        field("time_zone", "text", false),
        field("window", "json", false),
        field("misfire", "text", false),
        field("target", "json", true),
        field("run_as", "text", true),
        field("policy", "json", false),
        field("owner", "text", false),
        field("enabled", "boolean", false),
        field("paused", "boolean", false),
    ]})
}

/// Declare the platform classes in a VAE that lacks them (an existing class of the same
/// name in another namespace is left alone and reported).
pub fn ensure(vae: &mut Vae) -> Result<(), BpaError> {
    let mut missing = Vec::new();
    for (name, decl) in [
        ("Subscription", subscription_class()),
        ("Job", job_class()),
        ("Schedule", schedule_class()),
    ] {
        match vae.engine_mut().registry.decl(name) {
            Ok(c) if c["namespace"] == json!(NAMESPACE) => {}
            Ok(_) => {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    format!(
                        "a class {name} of another namespace exists; the event platform is off in this VAE"
                    ),
                ));
            }
            Err(_) => missing.push(decl),
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    vae.call("load_genesis", &json!({"classes": missing}))
        .map(|_| ())
}

/// Whether a branch is in a scope (`name` or `prefix/*`; `["main"]` when absent).
pub fn in_scope(scope: &Value, branch: &str) -> bool {
    let list: Vec<&str> = match scope.as_array() {
        Some(a) => a.iter().filter_map(Value::as_str).collect(),
        None => vec!["main"],
    };
    list.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => branch.starts_with(prefix),
        None => *p == branch,
    })
}

/// The event as a handler and a selector see it.
pub fn event_view(e: &Value) -> Value {
    json!({"event_id": e["effect_id"], "type": e["type"], "branch": e["branch"], "cseq": e.get("cseq").cloned().unwrap_or(Value::Null),
        "object_id": e.get("object_id").cloned().unwrap_or(Value::Null), "payload": e.get("payload").cloned().unwrap_or_else(|| json!({}))})
}

/// The subscriptions of a branch's state: `{object_id, body}` each.
pub fn subscriptions(vae: &mut Vae, branch: &str) -> Result<Vec<Value>, BpaError> {
    let ids = match vae.call(
        "list_objects",
        &json!({"class": "Subscription", "branch": branch}),
    ) {
        Ok(v) => v["objects"].as_array().cloned().unwrap_or_default(),
        Err(e)
            if e.code == "MODEL.UNKNOWN_TYPE"
                || e.code == "VER.INVALID_BRANCH_NAME"
                || e.code == "IAM.NOT_FOUND" =>
        {
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for id in ids {
        let oid = id.as_str().unwrap_or_default();
        let r = vae.call("read", &json!({"branch": branch, "object_id": oid}))?;
        out.push(json!({"object_id": oid, "slug": r["slug"], "body": r["body"]}));
    }
    Ok(out)
}

/// Whether a subscription listens to an event on `branch` (FR-EVT-021, -023).
pub fn listens(sub: &Value, event: &Value, branch: &str) -> bool {
    let b = &sub["body"];
    if b["enabled"] == json!(false) || !in_scope(&b["branches"], branch) {
        return false;
    }
    let ty = event["type"].as_str().unwrap_or_default();
    let typed = b["event_types"]
        .as_array()
        .is_some_and(|a| a.iter().any(|t| t == "*" || t == ty));
    typed
        && b.get("selector")
            .filter(|x| !x.is_null())
            .is_none_or(|sel| subset(sel, &event_view(event)))
}

/// The listener registry's answers (FR-EVT-022): the subscriptions of a branch that listen
/// to `type_` (all when `None`), or whose handler or name contains `handler`.
pub fn listeners(
    vae: &mut Vae,
    branch: &str,
    type_: Option<&str>,
    handler: Option<&str>,
) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for s in subscriptions(vae, branch)? {
        let b = &s["body"];
        if let Some(t) = type_ {
            let ev = json!({"effect_id": "", "type": t, "branch": branch});
            let sel_free = {
                let mut x = s.clone();
                x["body"]["selector"] = Value::Null;
                x
            };
            if !listens(&sel_free, &ev, branch) {
                continue;
            }
        }
        if let Some(h) = handler {
            let text = format!(
                "{} {}",
                b["name"].as_str().unwrap_or_default(),
                b["handler"]
            );
            if !text.contains(h) {
                continue;
            }
        }
        out.push(json!({"subscription": s["object_id"], "name": b["name"], "event_types": b["event_types"],
            "branches": b.get("branches").cloned().filter(|x| !x.is_null()).unwrap_or_else(|| json!(["main"])),
            "selector": b.get("selector").cloned().unwrap_or(Value::Null), "handler": b["handler"],
            "run_as": b["run_as"], "owner": b.get("owner").cloned().unwrap_or(Value::Null),
            "enabled": b["enabled"] != json!(false)}));
    }
    Ok(out)
}

/// A UUID derived from a key (job and process identifiers).
fn derived_uuid(domain: &str, key: &str) -> Result<String, BpaError> {
    let h = content_id("ubos.chunk.v1", &json!({domain: key}))?;
    let hex: String = h.trim_start_matches("sha256:").chars().take(32).collect();
    Ok(format!(
        "{}-{}-8{}-8{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    ))
}

/// The job objects' state of a table state.
pub fn object_state(table_state: &str) -> &'static str {
    match table_state {
        "done" => "succeeded",
        "dead" => "dead",
        "cancelled" => "cancelled",
        "running" => "running",
        _ => "pending",
    }
}

/// Create the `Job` objects of job specifications (`{job_id, kind, queue?, priority?,
/// payload, max_attempts?, dedupe_key?, delay_ms?}`) in one platform commit, skipping those
/// that exist, then enqueue their rows (FR-EVT-031). Returns how many were new.
pub fn create_jobs(vae: &mut Vae, specs: &[Value]) -> Result<usize, BpaError> {
    let mut changes = Vec::new();
    let mut new = Vec::new();
    for sp in specs {
        let id = sp["job_id"].as_str().unwrap_or_default().to_string();
        match vae.call("read", &json!({"branch": "main", "object_id": id})) {
            Ok(_) => continue,
            Err(e) if e.code == "IAM.NOT_FOUND" => {}
            Err(e) => return Err(e),
        }
        changes.push(json!({"op": "create", "class": "Job", "object_id": id, "slug": format!("job-{id}"),
            "body": {"job_type": sp["kind"], "queue": sp.get("queue").cloned().unwrap_or_else(|| json!("default")),
                "payload": sp["payload"], "state": "pending", "attempts": 0,
                "max_attempts": sp.get("max_attempts").cloned().unwrap_or_else(|| json!(3)),
                "priority": sp.get("priority").cloned().unwrap_or_else(|| json!(0)),
                "dedupe_key": sp.get("dedupe_key").cloned().unwrap_or(Value::Null)}}));
        new.push(sp.clone());
    }
    if !changes.is_empty() {
        vae.call(
            "system_commit",
            &json!({"branch": "main", "changes": changes, "process": {"kind": "job", "message": "jobs enqueued"}}),
        )?;
    }
    // rows of every spec: a crash between the commit and the rows is repaired here on the
    // next release round, or by the sweeper
    for sp in specs {
        vae.services().jobs.enqueue(sp)?;
    }
    Ok(new.len())
}

/// Commit a job object's new state (`set` merged into its body).
pub fn job_state(vae: &mut Vae, job_id: &str, set: &Value) -> Result<(), BpaError> {
    vae.call(
        "system_commit",
        &json!({"branch": "main", "changes": [{"op": "update", "object_id": job_id, "body": set}],
            "process": {"kind": "job", "message": "job state"}}),
    )
    .map(|_| ())
}

/// The `Job` objects still pending that have no row among `known`, as specifications for
/// the sweeper (FR-EVT-032): one listing of the objects, and a read only of those without a
/// row (normally none), so that a sweep does not grow with the history of jobs.
pub fn open_jobs(
    vae: &mut Vae,
    known: &std::collections::HashSet<String>,
) -> Result<Vec<Value>, BpaError> {
    let ids = match vae.call("list_objects", &json!({"class": "Job", "branch": "main"})) {
        Ok(v) => v["objects"].as_array().cloned().unwrap_or_default(),
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for id in ids {
        let oid = id.as_str().unwrap_or_default();
        if known.contains(oid) {
            continue;
        }
        let b = vae.call("read", &json!({"branch": "main", "object_id": oid}))?["body"].clone();
        if matches!(b["state"].as_str(), Some("pending" | "running")) {
            out.push(json!({"job_id": oid, "kind": b["job_type"], "queue": b["queue"], "priority": b["priority"],
                "payload": b["payload"], "max_attempts": b["max_attempts"], "attempts": b["attempts"],
                "dedupe_key": b["dedupe_key"]}));
        }
    }
    Ok(out)
}

/// The job specification of a `Job` effect of the kernel (`run.async`): a port job on the
/// effect's object, run as the parent process's initiator after `not_before`.
pub fn effect_job(e: &Value, now: &str) -> Option<Value> {
    if e["type"] != "Job" {
        return None;
    }
    let p = &e["payload"];
    let id = p["job_id"].as_str()?;
    let delay = match (
        p["not_before"].as_str(),
        ubos_dvm::types::time::parse_instant(now),
    ) {
        (Some(nb), Ok(n)) => ubos_dvm::types::time::parse_instant(nb)
            .map(|t| (t - n).num_milliseconds().max(0))
            .unwrap_or(0),
        _ => 0,
    };
    Some(
        json!({"job_id": id, "kind": "port", "queue": "default", "max_attempts": 5, "delay_ms": delay,
        "dedupe_key": format!("effect:{}", e["effect_id"].as_str().unwrap_or(id)),
        "payload": {"branch": e["branch"], "target": e["object_id"], "port": p["port"], "params": p["params"],
            "parent_process": p["parent_process"]}}),
    )
}

/// The jobs of released effects (before they are acknowledged): one delivery per
/// subscription and event, and a port job per `Job` effect. Returns the specifications.
pub fn dispatch_specs(vae: &mut Vae, effects: &[Value], now: &str) -> Result<Vec<Value>, BpaError> {
    let mut specs: Vec<Value> = effects.iter().filter_map(|e| effect_job(e, now)).collect();
    specs.extend(delivery_specs(vae, effects)?);
    Ok(specs)
}

/// Create and enqueue the jobs of released effects; returns how many jobs.
pub fn dispatch(vae: &mut Vae, effects: &[Value]) -> Result<usize, BpaError> {
    let now = ubos_dvm::types::time::format_instant(&chrono::DateTime::<chrono::Utc>::from(
        std::time::SystemTime::now(),
    ));
    let specs = dispatch_specs(vae, effects, &now)?;
    if specs.is_empty() {
        return Ok(0);
    }
    create_jobs(vae, &specs)?;
    Ok(specs.len())
}

/// The delivery jobs of released effects: one per subscription and event.
fn delivery_specs(vae: &mut Vae, effects: &[Value]) -> Result<Vec<Value>, BpaError> {
    let mut by_branch: std::collections::BTreeMap<String, Vec<Value>> =
        std::collections::BTreeMap::new();
    for e in effects {
        let b = e["branch"].as_str().unwrap_or("main").to_string();
        by_branch.entry(b).or_default().push(e.clone());
    }
    let mut out = Vec::new();
    for (branch, evs) in by_branch {
        let subs = subscriptions(vae, &branch)?;
        if subs.is_empty() {
            continue;
        }
        for e in &evs {
            for s in subs.iter().filter(|s| listens(s, e, &branch)) {
                let b = &s["body"];
                let policy = b
                    .get("policy")
                    .cloned()
                    .filter(|x| x.is_object())
                    .unwrap_or_else(|| json!({}));
                let max = crate::services::RetryPolicy::from_json(&policy).max_attempts;
                let sub = s["object_id"].as_str().unwrap_or_default();
                let effect = e["effect_id"].as_str().unwrap_or_default();
                let dedupe = format!("{sub}:{effect}");
                out.push(json!({"job_id": delivery_job_id(sub, effect)?, "kind": "delivery", "queue": "deliveries",
                    "max_attempts": max, "dedupe_key": dedupe,
                    "payload": {"subscription": sub, "name": b["name"], "run_as": b["run_as"], "handler": b["handler"],
                        "policy": policy, "event": event_view(e)}}));
            }
        }
    }
    Ok(out)
}

/// The job identifier of a delivery: a UUID derived from `(subscription, effect)`.
pub fn delivery_job_id(subscription: &str, effect_id: &str) -> Result<String, BpaError> {
    derived_uuid("delivery", &format!("{subscription}:{effect_id}"))
}

/// The process identifier of a delivery: a UUID derived from `(subscription, event)`.
pub fn delivery_process_id(subscription: &str, event_id: &str) -> Result<String, BpaError> {
    let h = content_id(
        "ubos.chunk.v1",
        &json!({"delivery": [subscription, event_id]}),
    )?;
    let hex: String = h.trim_start_matches("sha256:").chars().take(32).collect();
    Ok(format!(
        "{}-{}-8{}-8{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    ))
}

/// Replace `$event.<path>` strings by members of the event.
pub fn template(v: &Value, event: &Value) -> Value {
    match v {
        Value::String(s) if s.starts_with("$event.") => {
            let path = s.trim_start_matches("$event.").replace('.', "/");
            event
                .pointer(&format!("/{path}"))
                .cloned()
                .unwrap_or(Value::Null)
        }
        Value::String(s) if s == "$event" => event.clone(),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), template(x, event)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| template(x, event)).collect()),
        other => other.clone(),
    }
}

/// Run one delivery (DSN-NOD-204): `job` is the claimed delivery job, `principal` the
/// service principal it runs as (`{did, kind, role?, grants?}`). Returns `{status, process_id,
/// result?}`; `status` is `done`, or `already_done` when the process of this delivery
/// committed before (a redelivery after a crash).
pub fn deliver(vae: &mut Vae, job: &Value, principal: &Value) -> Result<Value, BpaError> {
    let p = &job["payload"];
    let sub = p["subscription"].as_str().unwrap_or_default();
    let event = &p["event"];
    let event_id = event["event_id"].as_str().unwrap_or_default();
    let pid = delivery_process_id(sub, event_id)?;
    if let Ok(info) = vae.call("process_info", &json!({"process_id": pid}))
        && info["state"] == "committed"
    {
        return Ok(json!({"status": "already_done", "process_id": pid}));
    }
    let branch = event["branch"].as_str().unwrap_or("main");
    let did = principal["did"].as_str().unwrap_or_default();
    let kind = principal["kind"].as_str().unwrap_or("service");
    // the event's process is the parent when the kernel recorded it
    let parent = event_id
        .split('/')
        .next()
        .filter(|x| x.len() == 36)
        .filter(|x| vae.call("process_info", &json!({"process_id": x})).is_ok())
        .map(str::to_string);
    let trigger = json!({"type": if parent.is_some() { "event" } else { "external" }, "source": "subscription",
        "subscription": sub, "event_id": event_id, "event_type": event["type"]});
    // the job's idempotency key commits the handler's work once, whichever holder of the
    // lease gets there first (DEC-076)
    let mut process = json!({"process_id": pid, "kind": "action", "initiator": did, "principal_kind": kind,
        "channel": "event", "trigger": trigger, "idempotency_key": format!("job:{}", job["job_id"].as_str().unwrap_or(pid.as_str()))});
    if let Some(pp) = parent {
        process["parent_process"] = json!(pp);
    }
    let mut ctx = json!({"principal": did, "principal_kind": kind, "channel": "event"});
    if let Some(r) = principal["role"].as_str() {
        process["role"] = json!(r);
        ctx["role"] = json!(r);
    }
    run_handler(
        vae,
        &p["handler"],
        event,
        branch,
        process,
        ctx,
        principal,
        &pid,
    )
}

/// Run a handler or a schedule's target (`{kind: "port" | "logic", …}`) as the process
/// `process`; parameters may refer to `event` (`$event.<path>`).
#[allow(clippy::too_many_arguments)]
fn run_handler(
    vae: &mut Vae,
    h: &Value,
    event: &Value,
    branch: &str,
    process: Value,
    ctx: Value,
    principal: &Value,
    pid: &str,
) -> Result<Value, BpaError> {
    let mut input = match h["kind"].as_str() {
        Some("port") => {
            let port = h["port"].as_str().ok_or_else(|| {
                BpaError::new("MODEL.INVALID_VALUE", "a port handler names its port")
            })?;
            let target = match h.get("target").filter(|t| !t.is_null()) {
                Some(t) => template(t, event),
                None => event["object_id"].clone(),
            };
            json!({"branch": branch, "target": target, "port": port,
                "params": template(h.get("params").filter(|x| !x.is_null()).unwrap_or(&json!({})), event)})
        }
        Some("logic") => {
            let (Some(asset), Some(function)) = (h["asset"].as_str(), h["function"].as_str())
            else {
                return Err(BpaError::new(
                    "MODEL.INVALID_VALUE",
                    "a logic handler names its asset and function",
                ));
            };
            json!({"branch": branch, "asset": asset, "function": function,
                "args": template(h.get("args").filter(|x| !x.is_null()).unwrap_or(&json!({})), event)})
        }
        other => {
            return Err(BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                format!(
                    "handlers of kind {other:?} come in PH-2 (DEC-076); PH-1 runs port and logic handlers"
                ),
            ));
        }
    };
    input["process"] = process;
    input["ctx"] = ctx;
    if let Some(g) = principal.get("grants").filter(|g| g.is_array()) {
        input["grants"] = g.clone();
    }
    let r = vae.call("execute", &input)?;
    Ok(
        json!({"status": "done", "process_id": pid, "result": r.get("result").cloned().unwrap_or(Value::Null)}),
    )
}

/// The principal of a process recorded by the kernel (its initiator), if any.
pub fn initiator_of(vae: &mut Vae, process_id: &str) -> Option<String> {
    vae.call("process_info", &json!({"process_id": process_id}))
        .ok()
        .and_then(|i| i["initiator"].as_str().map(str::to_string))
}

/// Run a port job (a `Job` effect of `run.async`, or an operator's): the port on its target
/// as one process with the job's identifier, the parent as trigger when the kernel recorded
/// it, and the job's idempotency key; `already_done` when that process committed before.
pub fn run_port(vae: &mut Vae, job: &Value, principal: &Value) -> Result<Value, BpaError> {
    let p = &job["payload"];
    let job_id = job["job_id"].as_str().unwrap_or_default();
    if let Ok(info) = vae.call("process_info", &json!({"process_id": job_id}))
        && info["state"] == "committed"
    {
        return Ok(json!({"status": "already_done", "process_id": job_id}));
    }
    let port = p["port"]
        .as_str()
        .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "a port job names its port"))?;
    let parent = p["parent_process"]
        .as_str()
        .filter(|x| vae.call("process_info", &json!({"process_id": x})).is_ok())
        .map(str::to_string);
    let did = principal["did"].as_str().unwrap_or_default();
    let kind = principal["kind"].as_str().unwrap_or("service");
    let mut process = json!({"process_id": job_id, "kind": "job", "initiator": did, "principal_kind": kind,
        "channel": "job", "idempotency_key": format!("job:{job_id}"),
        "trigger": {"type": if parent.is_some() { "job" } else { "external" }, "source": "job", "job_id": job_id}});
    if let Some(pp) = parent {
        process["parent_process"] = json!(pp);
    }
    let mut ctx = json!({"principal": did, "principal_kind": kind, "channel": "job"});
    if let Some(r) = principal["role"].as_str() {
        process["role"] = json!(r);
        ctx["role"] = json!(r);
    }
    let mut input = json!({"branch": p["branch"].as_str().unwrap_or("main"), "target": p["target"], "port": port,
        "params": p.get("params").filter(|x| x.is_object()).cloned().unwrap_or_else(|| json!({})),
        "process": process, "ctx": ctx});
    if let Some(g) = principal.get("grants").filter(|g| g.is_array()) {
        input["grants"] = g.clone();
    }
    let r = vae.call("execute", &input)?;
    Ok(
        json!({"status": "done", "process_id": job_id, "result": r.get("result").cloned().unwrap_or(Value::Null)}),
    )
}

/// One step of a bulk job (DSN-NOD-230, DSN-DVM-560): at most `max_chunks` chunks of the
/// kernel's bulk process named by the job (its identifier unless the payload names one),
/// resumed when it exists; returns the bulk result (`suspended`, `committed` or `failed`).
pub fn run_bulk(vae: &mut Vae, job: &Value, max_chunks: u64) -> Result<Value, BpaError> {
    let mut input = job["payload"].clone();
    if !input.is_object() {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "a bulk job's payload is the bulk input",
        ));
    }
    let pid = input["process_id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| job["job_id"].as_str().unwrap_or_default().to_string());
    let known = vae
        .call("process_info", &json!({"process_id": pid}))
        .ok()
        .map(|i| i["state"].clone());
    if known == Some(json!("committed")) {
        return Ok(json!({"state": "committed", "process_id": pid}));
    }
    input["process_id"] = json!(pid);
    input["max_chunks"] = json!(max_chunks);
    input["resume"] = json!(known.is_some());
    vae.call("bulk", &input)
}

// ---- schedules (step 15d) ----

/// The `Schedule` objects of `main` (`{schedule_id, body}`) and main's head, or `None` when
/// main's head is `known` (nothing changed since the caller's last read; FR-EVT-042).
pub fn schedules(vae: &mut Vae, known: Option<&str>) -> Result<Value, BpaError> {
    let head = vae.call("branch_info", &json!({"branch": "main"}))?["head"].clone();
    if known.is_some_and(|k| head == json!(k)) {
        return Ok(json!({"head": head, "unchanged": true}));
    }
    let ids = match vae.call(
        "list_objects",
        &json!({"class": "Schedule", "branch": "main"}),
    ) {
        Ok(v) => v["objects"].as_array().cloned().unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut items = Vec::new();
    for id in ids {
        let oid = id.as_str().unwrap_or_default();
        let b = vae.call("read", &json!({"branch": "main", "object_id": oid}))?["body"].clone();
        items.push(json!({"schedule_id": oid, "body": b}));
    }
    Ok(json!({"head": head, "items": items}))
}

/// The job identifier of a schedule's slot: one job per `(schedule, slot)` (DSN-NOD-224).
pub fn schedule_job_id(schedule: &str, slot: &str) -> Result<String, BpaError> {
    derived_uuid("schedule", &format!("{schedule}@{slot}"))
}

/// The job specification of a slot that fires.
pub fn schedule_job(
    schedule: &str,
    body: &Value,
    slot: &str,
    caught_up: bool,
) -> Result<Value, BpaError> {
    let id = schedule_job_id(schedule, slot)?;
    let policy = body.get("policy").cloned().unwrap_or(Value::Null);
    let max = crate::services::RetryPolicy::from_json(&policy).max_attempts;
    Ok(
        json!({"job_id": id, "kind": "schedule", "queue": "schedules", "max_attempts": max,
        "dedupe_key": format!("schedule:{schedule}@{slot}"),
        "payload": {"schedule": schedule, "slot": slot, "caught_up": caught_up, "target": body["target"],
            "run_as": body["run_as"], "policy": policy}}),
    )
}

/// Run a scheduled firing: the schedule's target as one process with the job's identifier
/// (so a slot starts one process, FR-EVT-043), triggered by `{type: "schedule", schedule,
/// slot}`, as the schedule's service principal; `already_done` when it committed before.
pub fn run_schedule(vae: &mut Vae, job: &Value, principal: &Value) -> Result<Value, BpaError> {
    let p = &job["payload"];
    let job_id = job["job_id"].as_str().unwrap_or_default().to_string();
    if let Ok(info) = vae.call("process_info", &json!({"process_id": job_id}))
        && info["state"] == "committed"
    {
        return Ok(json!({"status": "already_done", "process_id": job_id}));
    }
    let did = principal["did"].as_str().unwrap_or_default();
    let kind = principal["kind"].as_str().unwrap_or("service");
    let slot = json!({"schedule": p["schedule"], "slot": p["slot"], "caught_up": p["caught_up"]});
    let mut process = json!({"process_id": job_id, "kind": "action", "initiator": did, "principal_kind": kind,
        "channel": "schedule", "idempotency_key": format!("job:{job_id}"),
        "trigger": {"type": "schedule", "source": "schedule", "schedule": p["schedule"], "slot": p["slot"], "job_id": job_id}});
    let mut ctx = json!({"principal": did, "principal_kind": kind, "channel": "schedule"});
    if let Some(r) = principal["role"].as_str() {
        process["role"] = json!(r);
        ctx["role"] = json!(r);
    }
    // a target's parameters may refer to the slot as `$event.slot`; it runs on `main`
    // unless it names its branch
    let branch = p["target"]["branch"].as_str().unwrap_or("main").to_string();
    run_handler(
        vae,
        &p["target"],
        &slot,
        &branch,
        process,
        ctx,
        principal,
        &job_id,
    )
}

/// Commit a schedule's `paused` flag on `main` as an operator (FR-EVT-044).
pub fn set_paused(
    vae: &mut Vae,
    schedule: &str,
    paused: bool,
    by: &str,
) -> Result<Value, BpaError> {
    let known = vae.call(
        "list_objects",
        &json!({"class": "Schedule", "branch": "main"}),
    )?;
    if !known["objects"]
        .as_array()
        .is_some_and(|a| a.contains(&json!(schedule)))
    {
        return Err(BpaError::new("IAM.NOT_FOUND", "no such schedule"));
    }
    let cur = vae.call("read", &json!({"branch": "main", "object_id": schedule}))?;
    let message = if paused {
        "schedule paused"
    } else {
        "schedule resumed"
    };
    vae.call(
        "commit",
        &json!({"branch": "main", "changes": [{"op": "update", "object_id": schedule, "base": cur["version_hash"], "body": {"paused": paused}}],
            "process": {"kind": "action", "initiator": by, "channel": "admin", "message": message},
            "ctx": {"principal": by, "channel": "admin"}}),
    )
}
