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

/// Declare the platform classes in a VAE that lacks them (an existing class of the same
/// name in another namespace is left alone and reported).
pub fn ensure(vae: &mut Vae) -> Result<(), BpaError> {
    match vae.call("class_at", &json!({"class": "Subscription"})) {
        Ok(c) if c["namespace"] == json!(NAMESPACE) => Ok(()),
        Ok(_) => Err(BpaError::new(
            "MODEL.DECLARATION_INCOMPLETE",
            "a class Subscription of another namespace exists; event subscriptions are off in this VAE",
        )),
        Err(_) => vae
            .call("load_genesis", &json!({"classes": [subscription_class()]}))
            .map(|_| ()),
    }
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

/// Enqueue the deliveries of released effects (before they are acknowledged): one job per
/// subscription and event. Returns the number enqueued.
pub fn dispatch(vae: &mut Vae, effects: &[Value]) -> Result<usize, BpaError> {
    let mut by_branch: std::collections::BTreeMap<String, Vec<Value>> =
        std::collections::BTreeMap::new();
    for e in effects {
        let b = e["branch"].as_str().unwrap_or("main").to_string();
        by_branch.entry(b).or_default().push(e.clone());
    }
    let mut n = 0;
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
                let job = json!({"kind": "delivery", "max_attempts": max,
                    "dedupe_key": format!("{sub}:{}", e["effect_id"].as_str().unwrap_or_default()),
                    "payload": {"subscription": sub, "name": b["name"], "run_as": b["run_as"], "handler": b["handler"],
                        "policy": policy, "event": event_view(e)}});
                vae.services().jobs.enqueue(&job)?;
                n += 1;
            }
        }
    }
    Ok(n)
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
    let mut process = json!({"process_id": pid, "kind": "action", "initiator": did, "principal_kind": kind,
        "channel": "event", "trigger": trigger});
    if let Some(pp) = parent {
        process["parent_process"] = json!(pp);
    }
    let mut ctx = json!({"principal": did, "principal_kind": kind, "channel": "event"});
    if let Some(r) = principal["role"].as_str() {
        process["role"] = json!(r);
        ctx["role"] = json!(r);
    }
    let h = &p["handler"];
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
