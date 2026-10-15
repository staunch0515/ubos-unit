//! Policy authorization of PH-1 step 13b (DEC-073; DSN-DVM-801…807, 820): the Cedar
//! policy engine (`cedar-policy`, DEC-021, DEC-065) as the only component that decides
//! access, the entity model generated from the business model (DSN-DVM-802), the compiled
//! policy set of the `Policy` objects of a branch (DSN-DVM-803) and the decision of a
//! request. The engine is pure: the policy objects, the versions of the resources, the
//! enforcement points and the decision log are the kernel's (`ubos-dvm`).
//!
//! Entity model (namespace `UBOS`): `User` (the principal, in its `Role`s), `VAE`, `Branch`
//! (kind), `Class` (name) and one entity type per business class under `UBOS::Model`,
//! whose attributes are the policy-visible fields (classification `public` or `internal`,
//! scalar types) with `class`, `slug`, `branch` and the lifecycle `state`. Actions are the
//! authorization actions of the instruction catalogue (STD-ISA-005), the port
//! permissions declared in the model, and `query`, `merge`, `approve` and `export`. The
//! context carries `principal_kind`, `channel`, `vae`, `branch_kind` and, when given,
//! `role` and `auth_level` (the decision context of STD-LGS-031).

use cedar_policy::{
    Authorizer, Context, Decision as CedarDecision, Entities, EntityUid, Policy, PolicyId,
    PolicySet, Request, Schema, ValidationMode, Validator,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use ubos_model::Registry;
use ubos_model::types::{BpaError, content_id};

/// The authorization actions of the instruction catalogue and the host API.
pub const ACTIONS: &[&str] = &[
    "create",
    "update",
    "end",
    "append",
    "correct",
    "read",
    "read_history",
    "reflect",
    "explain",
    "execute",
    "evaluate",
    "emit",
    "notify",
    "invoke",
    "ask",
    "permission",
    "query",
    "merge",
    "approve",
    "export",
];

/// Classification levels whose fields policies may read (STD-FND-027; DSN-DVM-802).
const VISIBLE: &[&str] = &["public", "internal"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(m: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

/// The Cedar type of a field type, or `None` for types policies do not see.
fn cedar_type(t: &str) -> Option<&'static str> {
    Some(match t {
        "integer" => "Long",
        "boolean" => "Bool",
        "text" | "enum" | "reference" | "date" | "instant" | "time" | "decimal" => "String",
        "money" => "{ amount: String, currency: String }",
        _ => return None,
    })
}

/// The entity model of a registry: the Cedar schema, its hash and the policy-visible
/// fields of every class (DSN-DVM-802).
#[derive(Debug, Clone)]
pub struct Model {
    /// The compiled schema.
    pub schema: Schema,
    /// Content identifier of the schema text.
    pub hash: String,
    /// The schema in the Cedar schema language (for policy authors).
    pub text: String,
    /// Policy-visible fields by class: `(field, field type)`.
    pub visible: BTreeMap<String, Vec<(String, String)>>,
    /// Every action of the schema.
    pub actions: BTreeSet<String>,
}

/// The policy-visible fields of a class: scalar fields whose effective classification is
/// `public` or `internal`.
fn visible_fields(reg: &Registry, class: &str) -> Vec<(String, String)> {
    let Ok(c) = reg.class(class) else {
        return Vec::new();
    };
    let default = reg
        .decl(class)
        .ok()
        .and_then(|d| s(d, "classification_default"))
        .unwrap_or("internal")
        .to_string();
    let mut out = Vec::new();
    for (name, f) in &c.fields {
        let level = s(f, "classification").unwrap_or(&default);
        if !VISIBLE.contains(&level) {
            continue;
        }
        let Some(t) = s(f, "type") else {
            continue;
        };
        if cedar_type(t).is_none() {
            continue;
        }
        // member names of extensions carry a namespace dot; Cedar attributes are plain
        if name.contains('.') {
            continue;
        }
        out.push((name.clone(), t.to_string()));
    }
    out
}

/// Generate the entity model of a registry (DSN-DVM-802).
pub fn model_of(reg: &Registry) -> Result<Model, BpaError> {
    let mut actions: BTreeSet<String> = ACTIONS.iter().map(|a| (*a).to_string()).collect();
    let mut classes: Vec<String> = Vec::new();
    let mut visible: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for c in reg.classes() {
        if c.is_abstract {
            continue;
        }
        classes.push(c.name.clone());
        visible.insert(c.name.clone(), visible_fields(reg, &c.name));
        for p in c.ports.values() {
            if let Some(perm) = s(&p.decl, "permission").filter(|p| *p != "none") {
                actions.insert(perm.to_string());
            }
        }
    }
    let mut text = String::new();
    text.push_str("namespace UBOS {\n");
    text.push_str("  entity Role;\n");
    text.push_str("  entity User in [Role] { kind: String, auth_level?: String };\n");
    text.push_str("  entity VAE;\n");
    text.push_str("  entity Branch in [VAE] { name: String, kind: String };\n");
    text.push_str("  entity Class in [VAE] { name: String };\n");
    text.push_str(
        "  type Ctx = { principal_kind: String, channel: String, vae: String, branch_kind: String, role?: String, auth_level?: String };\n",
    );
    let mut resources: Vec<String> = vec!["VAE".into(), "Branch".into(), "Class".into()];
    resources.extend(classes.iter().map(|c| format!("UBOS::Model::{c}")));
    let acts: Vec<String> = actions.iter().map(|a| format!("\"{a}\"")).collect();
    text.push_str(&format!(
        "  action {} appliesTo {{ principal: [User], resource: [{}], context: Ctx }};\n",
        acts.join(", "),
        resources.join(", ")
    ));
    text.push_str("}\n");
    text.push_str("namespace UBOS::Model {\n");
    for c in &classes {
        let mut attrs = vec![
            "class: String".to_string(),
            "slug: String".to_string(),
            "branch: String".to_string(),
            "state?: String".to_string(),
        ];
        for (f, t) in &visible[c] {
            if matches!(f.as_str(), "class" | "slug" | "branch" | "state") {
                continue;
            }
            attrs.push(format!("{f}?: {}", cedar_type(t).expect("visible type")));
        }
        text.push_str(&format!(
            "  entity {c} in [UBOS::Class, UBOS::Branch] {{ {} }};\n",
            attrs.join(", ")
        ));
    }
    text.push_str("}\n");
    let (schema, _warnings) =
        Schema::from_cedarschema_str(&text).map_err(|e| invalid(format!("entity schema: {e}")))?;
    let hash = content_id("ubos.chunk.v1", &json!(text))?;
    Ok(Model {
        schema,
        hash,
        text,
        visible,
        actions,
    })
}

/// A compiled policy set (DSN-DVM-803): the policies of the `Policy` objects of a branch,
/// validated against the entity schema.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The policies.
    pub set: PolicySet,
    /// Content identifier of the sources (with the schema hash).
    pub hash: String,
    /// Policy identifiers (the names of the policy objects).
    pub ids: Vec<String>,
}

/// Parse one policy source under its name and validate it against the model
/// (`MODEL.INVALID_VALUE` with the reason).
pub fn check_policy(model: &Model, name: &str, source: &str) -> Result<Policy, BpaError> {
    let p = Policy::parse(Some(PolicyId::new(name)), source)
        .map_err(|e| invalid(format!("policy {name}: {e}")))?;
    let mut set = PolicySet::new();
    set.add(p.clone())
        .map_err(|e| invalid(format!("policy {name}: {e}")))?;
    let v = Validator::new(model.schema.clone());
    let r = v.validate(&set, ValidationMode::default());
    if !r.validation_passed() {
        let msgs: Vec<String> = r.validation_errors().map(|e| e.to_string()).collect();
        let mut e = BpaError::with_details(
            "MODEL.INVALID_VALUE",
            json!({"policy": name, "errors": msgs}),
        );
        e.message = format!("policy {name}: {}", msgs.join("; "));
        return Err(e);
    }
    Ok(p)
}

/// Compile the policies `(name, source)` of a branch into one validated set.
pub fn compile(model: &Model, policies: &[(String, String)]) -> Result<Compiled, BpaError> {
    let mut set = PolicySet::new();
    let mut ids = Vec::new();
    for (name, source) in policies {
        let p = check_policy(model, name, source)?;
        set.add(p)
            .map_err(|e| invalid(format!("policy {name}: {e}")))?;
        ids.push(name.clone());
    }
    let hash = content_id(
        "ubos.chunk.v1",
        &json!({"schema": model.hash, "policies": policies}),
    )?;
    Ok(Compiled { set, hash, ids })
}

/// The principal of a request: the DID, its kind, the roles it is in and the
/// authentication level of the session.
#[derive(Debug, Clone, Default)]
pub struct Principal {
    /// DID.
    pub id: String,
    /// `human`, `service`, `agent` (STD-CTX-001 `principal_kind`).
    pub kind: String,
    /// Roles (the `role` of the decision context and the roles the host asserts).
    pub roles: Vec<String>,
    /// Authentication level, if known.
    pub auth_level: Option<String>,
}

/// The resource of a request.
#[derive(Debug, Clone)]
pub enum Resource {
    /// The VAE itself.
    Vae,
    /// A branch.
    Branch {
        /// Branch name.
        name: String,
        /// Branch kind.
        kind: String,
    },
    /// A class (creates and appends, reflection).
    Class {
        /// Class name.
        name: String,
    },
    /// An object with its policy-visible fields.
    Object {
        /// Object identifier.
        oid: String,
        /// Class name.
        class: String,
        /// Linearization of the class (the `Class` entities the object is in).
        lineage: Vec<String>,
        /// Branch the object is read on.
        branch: String,
        /// Branch kind.
        branch_kind: String,
        /// Slug.
        slug: String,
        /// Lifecycle state, if any.
        state: Option<String>,
        /// The body (only the policy-visible fields are passed to the engine).
        body: Map<String, Value>,
    },
}

/// A decision with its diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Allowed.
    pub allow: bool,
    /// The policies that determined it.
    pub policies: Vec<String>,
    /// Evaluation errors (a policy that errored counts as not satisfied).
    pub errors: Vec<String>,
}

fn uid(type_: &str, id: &str) -> Result<EntityUid, BpaError> {
    let text = format!("{type_}::{}", serde_json::to_string(id).unwrap_or_default());
    EntityUid::from_str(&text).map_err(|e| invalid(format!("entity {text}: {e}")))
}

/// The attribute value of a field for the entity (visible fields only).
fn attr_of(t: &str, v: &Value) -> Option<Value> {
    if v.is_null() {
        return None;
    }
    Some(match t {
        "integer" => match v {
            Value::Number(n) => json!(n.as_i64()?),
            Value::String(x) => json!(x.parse::<i64>().ok()?),
            _ => return None,
        },
        "boolean" => json!(v.as_bool()?),
        "money" => json!({"amount": s(v, "amount")?, "currency": s(v, "currency")?}),
        _ => json!(v.as_str()?),
    })
}

/// Decide a request (DSN-DVM-801): default deny; `policies` names the determining
/// policies.
pub fn authorize(
    model: &Model,
    compiled: &Compiled,
    principal: &Principal,
    action: &str,
    resource: &Resource,
    vae: &str,
    context: &Map<String, Value>,
) -> Result<Decision, BpaError> {
    if !model.actions.contains(action) {
        // an action the model does not know is never permitted
        return Ok(Decision {
            allow: false,
            policies: Vec::new(),
            errors: vec![format!("unknown action {action}")],
        });
    }
    let mut entities: Vec<Value> = Vec::new();
    let mut user = json!({"uid": {"type": "UBOS::User", "id": principal.id},
        "attrs": {"kind": principal.kind}, "parents": []});
    if let Some(a) = &principal.auth_level {
        user["attrs"]["auth_level"] = json!(a);
    }
    for r in &principal.roles {
        user["parents"]
            .as_array_mut()
            .expect("list")
            .push(json!({"type": "UBOS::Role", "id": r}));
        entities.push(json!({"uid": {"type": "UBOS::Role", "id": r}, "attrs": {}, "parents": []}));
    }
    entities.push(user);
    entities.push(json!({"uid": {"type": "UBOS::VAE", "id": vae}, "attrs": {}, "parents": []}));
    let branch_entity = |name: &str, kind: &str| {
        json!({"uid": {"type": "UBOS::Branch", "id": name}, "attrs": {"name": name, "kind": kind},
            "parents": [{"type": "UBOS::VAE", "id": vae}]})
    };
    let class_entity = |name: &str| {
        json!({"uid": {"type": "UBOS::Class", "id": name}, "attrs": {"name": name},
            "parents": [{"type": "UBOS::VAE", "id": vae}]})
    };
    let resource_uid = match resource {
        Resource::Vae => uid("UBOS::VAE", vae)?,
        Resource::Branch { name, kind } => {
            entities.push(branch_entity(name, kind));
            uid("UBOS::Branch", name)?
        }
        Resource::Class { name } => {
            entities.push(class_entity(name));
            uid("UBOS::Class", name)?
        }
        Resource::Object {
            oid,
            class,
            lineage,
            branch,
            branch_kind,
            slug,
            state,
            body,
        } => {
            let mut parents = vec![json!({"type": "UBOS::Branch", "id": branch})];
            entities.push(branch_entity(branch, branch_kind));
            for c in lineage {
                entities.push(class_entity(c));
                parents.push(json!({"type": "UBOS::Class", "id": c}));
            }
            let mut attrs = json!({"class": class, "slug": slug, "branch": branch});
            if let Some(st) = state {
                attrs["state"] = json!(st);
            }
            for (f, t) in model.visible.get(class).map(Vec::as_slice).unwrap_or(&[]) {
                if matches!(f.as_str(), "class" | "slug" | "branch" | "state") {
                    continue;
                }
                if let Some(v) = body.get(f).and_then(|v| attr_of(t, v)) {
                    attrs[f] = v;
                }
            }
            let type_ = format!("UBOS::Model::{class}");
            entities.push(
                json!({"uid": {"type": type_, "id": oid}, "attrs": attrs, "parents": parents}),
            );
            uid(&type_, oid)?
        }
    };
    let entities = Entities::from_json_value(Value::Array(entities), Some(&model.schema))
        .map_err(|e| invalid(format!("entities: {e}")))?;
    let action_uid = uid("UBOS::Action", action)?;
    let mut ctx = Map::new();
    for k in [
        "principal_kind",
        "channel",
        "vae",
        "branch_kind",
        "role",
        "auth_level",
    ] {
        if let Some(v) = context.get(k).filter(|v| v.is_string()) {
            ctx.insert(k.to_string(), v.clone());
        }
    }
    ctx.entry("principal_kind".to_string())
        .or_insert_with(|| json!(principal.kind));
    ctx.entry("vae".to_string()).or_insert_with(|| json!(vae));
    ctx.entry("channel".to_string())
        .or_insert_with(|| json!("api"));
    ctx.entry("branch_kind".to_string())
        .or_insert_with(|| json!("main"));
    let context = Context::from_json_value(Value::Object(ctx), Some((&model.schema, &action_uid)))
        .map_err(|e| invalid(format!("context: {e}")))?;
    let request = Request::new(
        uid("UBOS::User", &principal.id)?,
        action_uid,
        resource_uid,
        context,
        Some(&model.schema),
    )
    .map_err(|e| invalid(format!("request: {e}")))?;
    let response = Authorizer::new().is_authorized(&request, &compiled.set, &entities);
    Ok(Decision {
        allow: response.decision() == CedarDecision::Allow,
        policies: response
            .diagnostics()
            .reason()
            .map(|p| p.to_string())
            .collect(),
        errors: response
            .diagnostics()
            .errors()
            .map(|e| e.to_string())
            .collect(),
    })
}
