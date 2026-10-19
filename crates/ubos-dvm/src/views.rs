//! View payloads (PH-1 step 13c, DEC-073; DSN-DVM-920…922; STD-CLS-023; FR-UX-011…015,
//! FR-UX-031).
//!
//! `view` resolves the view of an object (or of a class, for the form of a new object) in a
//! mode and returns a payload with the data, the field states and the available actions,
//! computed server-side for the principal: a field is hidden by the lifecycle's field rules
//! or a sheet's `field_visibility`, read-only when the principal may not update the object,
//! when the state's rules or a sheet say so, or when the field is computed; required by the
//! declaration, the state's rules or a sheet's `field_required`; masked by a sheet's `mask`
//! (`hide`, `partial`, `hash`; FR-IAM-061 in the view). The view is the one declared for
//! the mode by the first class of the linearization that declares one, refined by a context
//! variant (role, device, locale), else a view generated from the effective definition
//! (FR-UX-012). With `staged` edits the payload carries the violations the flush would
//! report and the field states of the edited body, without a flush (DSN-DVM-921). For a
//! snapshot, principal and view the payload is deterministic (`payload_hash`; DSN-DVM-922).

use crate::flow::{Lifecycle, Lifecycles, Permit, state_of};
use crate::relations::{extensions_of, resolve};
use crate::sheets::cascade_state;
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use ubos_authz::Resource;
use ubos_model::Registry;
use ubos_types::{BpaError, content_id};
use ubos_version::{Repo, State};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn list(v: Option<&Value>) -> BTreeSet<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The widget a generated view uses for a field type (a hint for clients; FR-UX-012).
fn widget(t: &str) -> &'static str {
    match t {
        "text" => "text",
        "integer" | "decimal" => "number",
        "money" => "money",
        "boolean" => "checkbox",
        "enum" => "select",
        "date" => "date",
        "instant" => "datetime",
        "reference" => "reference",
        "list" => "list",
        _ => "json",
    }
}

/// A masked value: `partial` keeps the last four characters of a text, `hash` replaces the
/// value by its content ID; `hide` is handled by the caller (the value is left out).
fn masked(mode: &str, v: &Value) -> Result<Value, BpaError> {
    Ok(match mode {
        "partial" => match v.as_str() {
            Some(t) => {
                let n = t.chars().count();
                let keep = n.min(4);
                let stars: String = "*".repeat(n - keep);
                let tail: String = t.chars().skip(n - keep).collect();
                json!(format!("{stars}{tail}"))
            }
            None => json!("***"),
        },
        "hash" => json!(content_id("ubos.chunk.v1", v)?),
        _ => v.clone(),
    })
}

/// The declared view of a class for a mode (STD-CLS-023): the first class of the
/// linearization whose `views` names the mode.
fn declared_view(reg: &Registry, lin: &[String], mode: &str) -> Option<(String, String)> {
    for c in lin {
        let Ok(d) = reg.decl(c) else {
            continue;
        };
        for v in d
            .get("views")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (m, uri) = match v {
                Value::Object(_) => (s(v, "mode"), s(v, "view").or_else(|| s(v, "uri"))),
                _ => (None, None),
            };
            if m == Some(mode)
                && let Some(u) = uri
            {
                return Some((c.clone(), u.to_string()));
            }
        }
    }
    None
}

/// The context variant of a view body that matches the request (role, device class,
/// locale): the first variant whose every stated key matches.
fn variant<'a>(body: &'a Value, input: &Value) -> Option<&'a Value> {
    body.get("variants")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|v| {
            ["role", "device", "locale"].iter().all(|k| match s(v, k) {
                Some(want) => s(input, k) == Some(want),
                None => true,
            })
        })
}

/// What the snapshot resolves for a view: the registry with the extensions in force, the
/// lifecycle, the state, the view (declared or generated) and the effective sheet values.
type Resolved = (
    Registry,
    Option<Lifecycle>,
    Option<String>,
    Value,
    ubos_rules::Effective,
);

impl Engine {
    /// The violations of a validation preview: those it reports, or the error the flush
    /// would fail with (a constraint, a field rule, a declaration) as one violation.
    fn staged_violations(&mut self, input: &Value) -> Result<Value, BpaError> {
        match self.validate(input) {
            Ok(r) => Ok(r["violations"].clone()),
            Err(e)
                if e.code.starts_with("MODEL.")
                    || e.code.starts_with("FLOW.")
                    || e.code.starts_with("RULE.") =>
            {
                let mut v = json!({"code": e.code, "severity": "error", "message": e.message});
                if let Some(d) = e.details {
                    v["details"] = d;
                }
                Ok(json!([v]))
            }
            Err(e) => Err(e),
        }
    }

    /// `view {branch, object_id | class, mode?, staged?, principal?, role?, device?,
    /// locale?, ...coordinates}`: the view payload (DSN-DVM-920).
    pub fn view(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let mode = s(input, "mode").unwrap_or("detail").to_string();
        let oid = s(input, "object_id").map(str::to_string);
        // the snapshot: the object's view, or the branch view for a class form
        let (object, st, ev): (
            Option<(ubos_version::ObjectState, Value)>,
            State,
            BTreeMap<String, String>,
        ) = match &oid {
            Some(oid) => {
                let (o, _, ev, st) = self.object_view(&branch, input, oid)?;
                let st = match st {
                    Some(st) => st,
                    None => {
                        let time = input.get("time").and_then(Value::as_u64);
                        Repo::new(self.store.as_mut(), &self.registry).state_at(&branch, time)?
                    }
                };
                let v = Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?;
                if v["tombstone"] == json!(true) {
                    return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
                }
                (Some((o, v)), st, ev)
            }
            None => {
                let (v, _) = Repo::new(self.store.as_mut(), &self.registry).view(&branch, input)?;
                (None, v.state, v.evaluated_at)
            }
        };
        let class = match &object {
            Some((o, _)) => o.class.clone(),
            None => s(input, "class")
                .ok_or_else(|| {
                    BpaError::new(
                        "MODEL.DECLARATION_INCOMPLETE",
                        "view names an object or a class",
                    )
                })?
                .to_string(),
        };
        self.registry.decl(&class)?;
        // authorization (DSN-DVM-804): the object must be readable; what the principal may
        // do decides the field states and the actions
        let principal = Engine::principal_of(input);
        let set = match &principal {
            Some(_) => self.policy_set_in(&st)?,
            None => None,
        };
        let ctx = self.access_ctx(input, &branch)?;
        let kind = self.branch_meta(&branch)?.kind;
        let resource = match &object {
            Some((o, v)) => {
                self.object_resource(&branch, oid.as_deref().unwrap_or_default(), o, v)?
            }
            None => Resource::Class {
                name: class.clone(),
            },
        };
        let mut may_write = true;
        let mut perms: BTreeMap<String, bool> = BTreeMap::new();
        if let (Some(p), Some(set)) = (&principal, &set) {
            if object.is_some() {
                self.enforce(set, &branch, p, "read", &resource, &ctx)?;
            }
            let write = if object.is_some() { "update" } else { "create" };
            may_write = self
                .decide_access(set, &branch, p, write, &resource, &ctx, None)?
                .allow;
            let branch_res = Resource::Branch {
                name: branch.clone(),
                kind: kind.clone(),
            };
            if may_write {
                may_write = self
                    .decide_access(set, &branch, p, write, &branch_res, &ctx, None)?
                    .allow;
            }
            let ports: Vec<String> = self
                .registry
                .class(&class)?
                .ports
                .values()
                .filter_map(|p| s(&p.decl, "permission").map(str::to_string))
                .filter(|p| p != "none")
                .collect();
            for perm in ports {
                let ok = self
                    .decide_access(set, &branch, p, &perm, &resource, &ctx, None)?
                    .allow;
                perms.insert(perm, ok);
            }
        }
        // the body, with the staged edits (DSN-DVM-921)
        let head_body: Map<String, Value> = object
            .as_ref()
            .and_then(|(_, v)| v["body"].as_object().cloned())
            .unwrap_or_default();
        let staged = input.get("staged").and_then(Value::as_object).cloned();
        let mut body = head_body.clone();
        if let Some(st_) = &staged {
            for (k, v) in st_ {
                if v.is_null() {
                    body.remove(k);
                } else {
                    body.insert(k.clone(), v.clone());
                }
            }
        }
        let body_v = Value::Object(body.clone());
        let asof = self.default_valid_of(&class);
        // the effective definition at the snapshot, the lifecycle, the declared view and
        // the sheets
        let authority = self.store.authority();
        let (reg, lc, state, view, eff): Resolved = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let exts = extensions_of(&mut repo, &st)?;
            let reg: Cow<Registry> = if exts.is_empty() {
                Cow::Borrowed(&self.registry)
            } else {
                Cow::Owned(self.registry.with_extensions(&exts)?)
            };
            let reg = reg.into_owned();
            let lc = Lifecycles::of(&repo).lifecycle(&mut repo, None, &st, &reg, &class)?;
            let state = match &object {
                Some((_, v)) => state_of(v),
                None => lc
                    .as_ref()
                    .and_then(|(_, _, b)| s(b, "initial").map(str::to_string)),
            };
            let lin = reg.class(&class)?.linearization.clone();
            let view = match declared_view(&reg, &lin, &mode) {
                Some((declared_by, uri)) => {
                    let void = resolve(&st, &uri, &authority).ok_or_else(|| {
                        BpaError::new("MODEL.REFERENCE_INVALID", format!("view {uri} not found"))
                    })?;
                    let vo = &st.objects[&void];
                    let vv = repo.version(&vo.head)?;
                    let layout = variant(&vv["body"], input)
                        .and_then(|x| x.get("layout"))
                        .or_else(|| vv["body"].get("layout"))
                        .cloned()
                        .unwrap_or_else(|| vv["body"].clone());
                    json!({"source": "declared", "declared_by": declared_by, "uri": uri,
                        "object_id": void, "version_hash": vo.head, "layout": layout})
                }
                None => json!({"source": "generated"}),
            };
            let (eff, _, _) = cascade_state(
                &mut repo,
                None,
                &st,
                &reg,
                &mut self.rules,
                &class,
                &body_v,
                &asof,
                &ctx,
                state.as_deref(),
            )?;
            (reg, lc, state, view, eff)
        };
        let value = |p: &str| eff.get(p).map(|(v, _)| v.clone()).unwrap_or(Value::Null);
        // a map-typed property cascades per key (`field_visibility.note`; STD-LGS-001)
        let keyed = |p: &str| -> Map<String, Value> {
            let mut out = value(p).as_object().cloned().unwrap_or_default();
            for (path, (v, _)) in &eff {
                if let Some(k) = path.strip_prefix(p)
                    && let Some(k) = k.strip_prefix('.')
                {
                    out.insert(k.to_string(), v.clone());
                }
            }
            out
        };
        let visibility = keyed("field_visibility");
        let mask = keyed("mask");
        let sheet_required = list(Some(&value("field_required")));
        let sheet_read_only = value("read_only") == json!(true);
        let rules = match (&lc, &state) {
            (Some((_, _, lcb)), Some(st_)) => ubos_flow::field_rules(lcb, st_).cloned(),
            _ => None,
        };
        let lc_hidden = list(rules.as_ref().and_then(|r| r.get("hidden")));
        let lc_read_only = list(rules.as_ref().and_then(|r| r.get("read_only")));
        let lc_required = list(rules.as_ref().and_then(|r| r.get("required")));
        let eff_class = reg.class(&class)?.clone();
        let class_default = reg
            .decl(&class)
            .ok()
            .and_then(|d| s(d, "classification_default"))
            .unwrap_or("internal")
            .to_string();
        // the fields and the data (FR-UX-013)
        let mut fields = Vec::new();
        let mut data = Map::new();
        let mut shown: Vec<String> = Vec::new();
        for (name, decl) in &eff_class.fields {
            let vis = visibility.get(name).and_then(Value::as_str);
            if vis == Some("hidden") || lc_hidden.contains(name) {
                continue;
            }
            let t = s(decl, "type").unwrap_or("json");
            let computed = decl.get("computed").is_some_and(|c| !c.is_null());
            let read_only = !may_write
                || sheet_read_only
                || computed
                || vis == Some("read_only")
                || lc_read_only.contains(name);
            let required = decl.get("required") == Some(&json!(true))
                || sheet_required.contains(name)
                || lc_required.contains(name);
            let mut f = json!({"name": name, "type": t, "label": decl.get("label").cloned().unwrap_or_else(|| json!(name)),
                "required": required, "read_only": read_only, "widget": widget(t),
                "classification": decl.get("classification").cloned().unwrap_or_else(|| json!(class_default))});
            for k in ["description", "constraints", "default", "relationship"] {
                if let Some(v) = decl.get(k).filter(|v| !v.is_null()) {
                    f[k] = v.clone();
                }
            }
            let m = mask.get(name).and_then(Value::as_str);
            if let Some(m) = m {
                f["masked"] = json!(m);
            }
            if let Some(v) = body.get(name)
                && m != Some("hide")
            {
                data.insert(name.clone(), masked(m.unwrap_or(""), v)?);
            }
            shown.push(name.clone());
            fields.push(f);
        }
        let view = match view {
            Value::Object(mut m) if m.get("source") == Some(&json!("generated")) => {
                let mut layout = json!({"sections": [{"name": "fields", "fields": shown}]});
                if let Some((lo, _, _)) = &lc {
                    layout["lifecycle"] = json!(lo);
                }
                m.insert("layout".into(), layout);
                Value::Object(m)
            }
            v => v,
        };
        // the actions of the object (DSN-DVM-732)
        let actions = match &oid {
            Some(oid) if staged.is_none() => {
                let mut permit = |perm: &str, _: &str| perms.get(perm).copied().unwrap_or(true);
                let p: Permit<'_> = &mut permit;
                let a = self.actions_in(&st, &branch, oid, &ctx, Some(p), None)?;
                a["actions"].clone()
            }
            _ => json!([]),
        };
        // the violations of the staged edits, as the flush would report them (DSN-DVM-921):
        // the validation violations, or the model, flow or rule error the flush would fail
        // with, as one violation naming its code
        let violations = match (&staged, &object) {
            (Some(_), Some((o, _))) => {
                let process =
                    json!({"channel": ctx["channel"], "principal_kind": ctx["principal_kind"]});
                Some(
                    self.staged_violations(&json!({"branch": branch, "changes": [{"op": "update",
                    "object_id": oid, "base": o.head, "body": body}], "process": process}))?,
                )
            }
            (Some(_), None) => {
                let new_id =
                    s(input, "object_id").unwrap_or("01900000-0000-7000-8000-00000000ffff");
                let process =
                    json!({"channel": ctx["channel"], "principal_kind": ctx["principal_kind"]});
                let slug = s(input, "slug").unwrap_or("staged");
                Some(self.staged_violations(&json!({"branch": branch, "changes": [{"op": "create",
                    "object_id": new_id, "class": class, "slug": slug, "body": body}], "process": process}))?)
            }
            _ => None,
        };
        // read logging of `audit_level: strict` (STD-LGS-011; no field values)
        if value("audit_level") == json!("strict")
            && let Some(p) = &principal
        {
            self.security_append(
                "read.view",
                json!({"principal": p.id, "class": class, "object_id": oid, "branch": branch, "mode": mode}),
            )?;
        }
        let mut payload = json!({
            "class": class, "mode": mode, "view": view, "fields": fields, "data": data,
            "actions": actions,
            "permissions": {"read": true, (if object.is_some() { "update" } else { "create" }): may_write},
            "evaluated_at": ev,
        });
        if let Some(oid) = &oid {
            payload["object_id"] = json!(oid);
            if let Some((o, _)) = &object {
                payload["version_hash"] = json!(o.head);
            }
        }
        if let Some(st_) = &state {
            payload["state"] = json!(st_);
        }
        if let Some(v) = violations {
            payload["violations"] = v;
        }
        // DSN-DVM-922: the hash of the payload for the evidence of what a user saw
        payload["payload_hash"] = json!(content_id("ubos.chunk.v1", &payload)?);
        Ok(payload)
    }
}
