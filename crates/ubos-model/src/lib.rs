//! `ubos-model` — engine layer (UBS-ARC-07 §2).
//!
//! Classes, traits, value types, constraints, extensions, reflection. Allowed dependencies:
//! `ubos-types` (AR-031).
//!
//! PH-1 step 4a (DEC-050): the full class registry of STD-CLS-001…030 — declaration checks,
//! the genesis model (STD-CLS-010), C3 linearization with traits (STD-CLS-011), kinds
//! (STD-CLS-014), narrowing of inherited fields (STD-CLS-013), port overrides and dispatch
//! (STD-CLS-020, -021), type selectors (STD-CLS-022), class extensions (STD-CLS-030) and
//! relationship declarations (STD-CLS-040). Declarations are kept as the JSON given to the
//! adapter, so that effective definitions compare exactly with the reference oracle.

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub use ubos_types as types;
use ubos_types::{BpaError, canonical, content_id, names, values};

const CLASS_REQUIRED: &[&str] = &[
    "name",
    "namespace",
    "kind",
    "parent",
    "label",
    "description",
    "schema_version",
    "fields",
];
const TRAIT_REQUIRED: &[&str] = &[
    "name",
    "namespace",
    "applies_to",
    "label",
    "description",
    "schema_version",
    "fields",
];
const FIELD_REQUIRED: &[&str] = &["name", "type", "label", "description"];
const PORT_REQUIRED: &[&str] = &["name", "kind", "permission", "description"];
/// Members of `DAT-PortDecl` (STD-CLS-003; the optional ones since 1.0-rc2, DEC-067).
const PORT_MEMBERS: &[&str] = &[
    "name",
    "kind",
    "params",
    "result",
    "errors",
    "states",
    "transition",
    "guards",
    "impl",
    "permission",
    "idempotent",
    "ui",
    "description",
    "emits",
];
/// Overlay policies of a member (STD-CLS-031).
const OVERLAY: &[&str] = &["overridable", "extend_only", "locked"];
/// Severities of a validation (STD-LGS-030).
const SEVERITIES: &[&str] = &["error", "warning", "info"];
const KINDS: &[&str] = &["definition", "ledger", "system", "raw_document", "inherit"];
const CONCRETE: &[&str] = &["definition", "ledger", "raw_document"];
const MIN_KEYS: &[&str] = &["min", "min_length", "min_items"];
const MAX_KEYS: &[&str] = &["max", "max_length", "max_items"];
/// Members a refinement may restate (STD-CLS-013); every other member must be unchanged.
const REFINABLE: &[&str] = &[
    "type",
    "required",
    "constraints",
    "default",
    "classification",
    "label",
    "description",
];
const NARROWABLE_CONSTRAINTS: &[&str] = &[
    "min",
    "min_length",
    "min_items",
    "max",
    "max_length",
    "max_items",
    "pattern",
    "allowed_values",
    "scale",
];
/// Bodies up to 16 MiB (STD-OBJ-009).
pub const MAX_BODY: usize = 16 * 1024 * 1024;
/// Scalar bases a custom value type may have (STD-FND-026).
const NOT_A_BASE: &[&str] = &["composite", "list", "map", "document", "blob", "json"];

fn err(code: &'static str, m: impl Into<String>) -> BpaError {
    BpaError::new(code, m)
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn field(name: &str, type_: &str, required: bool) -> Value {
    let mut f =
        json!({"name": name, "type": type_, "label": name.replace('_', " "), "description": name});
    if required {
        f["required"] = json!(true);
    }
    f
}

fn enum_field(name: &str, allowed: &[&str]) -> Value {
    let mut f = field(name, "enum", true);
    f["constraints"] = json!({"allowed_values": allowed});
    f
}

/// Genesis declarations (STD-CLS-010; `ClassExtension` STD-CLS-030; `ChangeSet` and
/// `Release` STD-VER-020, -061; `Logic` STD-CTX-022). Only what the registry needs is
/// declared: name, kind, parent, abstractness and fields.
fn genesis() -> BTreeMap<String, Value> {
    let g = |name: &str,
             kind: Option<&str>,
             parent: Option<&str>,
             abs: bool,
             fields: Vec<Value>| {
        json!({"name": name, "kind": kind, "parent": parent, "abstract": abs, "fields": fields})
    };
    let ext_json = |n: &str| field(n, "json", false);
    let list = vec![
        g("Object", None, None, true, vec![]),
        g("DataObject", None, Some("Object"), true, vec![]),
        g(
            "LogicAsset",
            Some("definition"),
            Some("Object"),
            true,
            vec![],
        ),
        g(
            "ViewAsset",
            Some("definition"),
            Some("Object"),
            true,
            vec![],
        ),
        g(
            "ConfigAsset",
            Some("definition"),
            Some("Object"),
            true,
            vec![],
        ),
        g("SystemObject", Some("system"), Some("Object"), true, vec![]),
        g(
            "ClassExtension",
            Some("definition"),
            Some("ConfigAsset"),
            false,
            vec![
                field("target", "text", true),
                field("namespace", "text", true),
                field("order", "integer", true),
                ext_json("fields"),
                ext_json("refinements"),
                ext_json("ports"),
                ext_json("views"),
                // since 1.0-rc2 (STD-LGS-030, DEC-067): validations added by the extension
                ext_json("validations"),
            ],
        ),
        g(
            "ChangeSet",
            Some("definition"),
            Some("Object"),
            false,
            vec![
                field("source", "text", true),
                field("target", "text", true),
                field("title", "text", true),
                field("description", "text", false),
                enum_field(
                    "state",
                    &[
                        "open",
                        "submitted",
                        "approved",
                        "rejected",
                        "merged",
                        "abandoned",
                    ],
                ),
                field("preview", "text", false),
                ext_json("approvals"),
                ext_json("simulations"),
                ext_json("selection"),
                field("merged_commit", "text", false),
            ],
        ),
        g(
            "Logic",
            Some("definition"),
            Some("LogicAsset"),
            false,
            vec![
                field("name", "text", true),
                enum_field("tier", &["l2", "l3", "host"]),
                field("abi", "text", true),
                field("source", "json", true),
                field("source_ref", "text", false),
                field("exports", "json", true),
                ext_json("imports"),
                field("capabilities", "json", true),
                enum_field(
                    "profile",
                    &["strict", "standard", "batch", "simulation", "agent"],
                ),
                ext_json("tests"),
                enum_field(
                    "state",
                    &["draft", "committed", "verified", "deprecated", "retired"],
                ),
                field("verification", "text", false),
                ext_json("publisher_signature"),
            ],
        ),
        // STD-LGS-002, -010 (BPA 1.0-rc2, DEC-066): sheets and Buk property declarations are
        // Definition objects below ConfigAsset; `ast` is derived from `source` at commit.
        g(
            "GovernanceSheet",
            Some("definition"),
            Some("ConfigAsset"),
            false,
            vec![
                field("name", "text", true),
                enum_field(
                    "layer",
                    &[
                        "platform", "industry", "vendor", "tenant", "vae", "unit", "object",
                    ],
                ),
                field("priority", "integer", false),
                ext_json("scope"),
                field("source", "text", true),
                ext_json("ast"),
                field("cascading", "boolean", false),
                ext_json("tests"),
                field("description", "text", true),
            ],
        ),
        g(
            "PropertyDecl",
            Some("definition"),
            Some("ConfigAsset"),
            false,
            vec![
                field("name", "text", true),
                field("schema", "json", true),
                enum_field(
                    "enforcement",
                    &["commit", "action", "read", "view", "retention", "notify"],
                ),
                enum_field(
                    "combine",
                    &["override", "max", "min", "union", "all_true", "any_true"],
                ),
                field("owner", "text", true),
                field("handler", "text", false),
                field("description", "text", true),
            ],
        ),
        // STD-CLS-004 (BPA 1.0-rc3, DEC-071): the lifecycle of a class is a Definition object
        // below ConfigAsset that its declaration names by URI.
        g(
            "Lifecycle",
            Some("definition"),
            Some("ConfigAsset"),
            false,
            vec![
                field("name", "text", true),
                field("states", "json", true),
                field("initial", "text", true),
                ext_json("final"),
                field("transitions", "json", true),
                ext_json("effects"),
                ext_json("field_rules"),
                field("description", "text", true),
            ],
        ),
        // DSN-DVM-803 (PH-1 step 13b, DEC-073): policies are Definition objects below
        // ConfigAsset; `source` is one Cedar policy, validated against the entity model at
        // commit (kernel; a standard clause follows with BPA 1.0.0).
        g(
            "Policy",
            Some("definition"),
            Some("ConfigAsset"),
            false,
            vec![
                field("name", "text", true),
                field("source", "text", true),
                field("enabled", "boolean", false),
                field("description", "text", true),
            ],
        ),
        // STD-FND-028, -029, STD-KIND-029 (BPA 1.0-rc9, DEC-098): currencies, FX rates and
        // accounts are Definition objects.
        g(
            "Currency",
            Some("definition"),
            Some("Object"),
            false,
            vec![
                field("code", "text", true),
                field("numeric", "text", false),
                field("name", "text", true),
                field("minor_units", "integer", true),
            ],
        ),
        g(
            "FxRate",
            Some("definition"),
            Some("Object"),
            false,
            vec![
                field("source", "text", true),
                field("base", "text", true),
                field("quote", "text", true),
                field("rate_type", "text", true),
                field("rate", "decimal", true),
                field("fixed_at", "instant", false),
            ],
        ),
        g(
            "LedgerAccount",
            Some("definition"),
            Some("Object"),
            false,
            vec![
                field("number", "text", true),
                field("name", "text", true),
                enum_field(
                    "type",
                    &["asset", "liability", "equity", "income", "expense"],
                ),
                field("currency", "text", false),
                {
                    let mut f = field("parent", "reference", false);
                    f["constraints"] = json!({"target_class": "LedgerAccount"});
                    f
                },
            ],
        ),
        // STD-KIND-026 (BPA 1.0-rc8, DEC-090): close artefacts are System objects written by
        // close_period.
        g(
            "CloseArtefact",
            Some("system"),
            Some("SystemObject"),
            false,
            vec![
                field("period", "text", true),
                field("period_slug", "text", true),
                field("sequence", "integer", true),
                field("start", "date", true),
                field("end", "date", true),
                field("tag", "text", true),
                field("as_of", "json", true),
                field("balances", "json", true),
            ],
        ),
        // STD-TIME-021 (BPA 1.0-rc9, DEC-098): projections are Definition objects below
        // ConfigAsset.
        g(
            "Projection",
            Some("definition"),
            Some("ConfigAsset"),
            false,
            vec![
                field("name", "text", true),
                field("source", "text", true),
                ext_json("group_by"),
                field("aggregates", "json", true),
                {
                    let mut f = field("mode", "enum", false);
                    f["constraints"] = json!({"allowed_values": ["sync", "async"]});
                    f
                },
                field("description", "text", false),
            ],
        ),
        g(
            "Release",
            Some("definition"),
            Some("Object"),
            false,
            vec![
                field("version", "text", true),
                field("tag", "text", true),
                field("notes", "text", false),
                ext_json("change_sets"),
                ext_json("approvals"),
            ],
        ),
    ];
    list.into_iter()
        .map(|d| (s(&d, "name").unwrap_or_default().to_string(), d))
        .collect()
}

/// True for a genesis class name.
pub fn is_genesis(name: &str) -> bool {
    matches!(
        name,
        "Object"
            | "DataObject"
            | "LogicAsset"
            | "ViewAsset"
            | "ConfigAsset"
            | "SystemObject"
            | "ClassExtension"
            | "ChangeSet"
            | "Logic"
            | "GovernanceSheet"
            | "PropertyDecl"
            | "Lifecycle"
            | "Policy"
            | "Release"
            | "Currency"
            | "FxRate"
            | "LedgerAccount"
            | "Projection"
            | "CloseArtefact"
    )
}

// ---- declaration checks (STD-CLS-001, -002, -040) ----

fn require(d: &Value, keys: &[&str], what: &str) -> Result<(), BpaError> {
    for k in keys {
        if d.get(*k).is_none() {
            return Err(err(
                "MODEL.DECLARATION_INCOMPLETE",
                format!("{what} member {k}"),
            ));
        }
    }
    Ok(())
}

fn is_reference_list(f: &Value) -> bool {
    s(f, "type") == Some("list")
        && f.get("constraints")
            .and_then(|c| c.get("item_type"))
            .and_then(|i| s(i, "type"))
            == Some("reference")
}

fn check_relationship(f: &Value, rel: &Value) -> Result<(), BpaError> {
    if !rel.is_object() || rel.get("target").is_none() || rel.get("cardinality").is_none() {
        return Err(err(
            "MODEL.DECLARATION_INCOMPLETE",
            "relationship needs target and cardinality",
        ));
    }
    let shape = match s(rel, "cardinality") {
        Some("one_to_one" | "many_to_one") => "reference",
        Some("one_to_many" | "many_to_many") => "list",
        _ => return Err(err("MODEL.INVALID_VALUE", "cardinality")),
    };
    let fits = (shape == "reference" && s(f, "type") == Some("reference"))
        || (shape == "list" && is_reference_list(f));
    if !fits {
        return Err(err(
            "MODEL.INVALID_VALUE",
            "cardinality does not fit the field type",
        ));
    }
    if !matches!(
        s(rel, "ownership").unwrap_or("association"),
        "association" | "composition"
    ) {
        return Err(err("MODEL.INVALID_VALUE", "ownership"));
    }
    if !matches!(
        s(rel, "on_end").unwrap_or("restrict"),
        "restrict" | "cascade" | "detach"
    ) {
        return Err(err("MODEL.INVALID_VALUE", "on_end"));
    }
    if let Some(inv) = rel.get("inverse")
        && !names::matches("member-name", inv.as_str().unwrap_or_default())
    {
        return Err(err("MODEL.INVALID_NAME", inv.to_string()));
    }
    Ok(())
}

/// Check a field declaration; `custom` are the names of declared custom value types.
pub fn check_field(f: &Value, custom: &BTreeSet<String>) -> Result<(), BpaError> {
    require(f, FIELD_REQUIRED, "field")?;
    let name = s(f, "name").unwrap_or_default();
    if !names::matches("member-name", name) {
        return Err(err("MODEL.INVALID_NAME", name));
    }
    let t = s(f, "type").unwrap_or_default();
    if !values::TYPES.contains(&t) && !custom.contains(t) {
        return Err(err("MODEL.UNKNOWN_TYPE", t));
    }
    if let Some(c) = f.get("classification")
        && !values::LEVELS.contains(&c.as_str().unwrap_or_default())
    {
        return Err(err("MODEL.INVALID_VALUE", "classification"));
    }
    if let Some(l) = f.get("lookup")
        && !matches!(l.as_str(), Some("none" | "equality"))
    {
        return Err(err("MODEL.INVALID_VALUE", "lookup"));
    }
    if let Some(rel) = f.get("relationship")
        && !rel.is_null()
    {
        check_relationship(f, rel)?;
    }
    Ok(())
}

fn is_error_code(e: &Value) -> bool {
    e.as_str().is_some_and(|t| {
        t.split_once('.').is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_uppercase())
                && b.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')
        })
    })
}

fn non_empty_text(v: &Value) -> bool {
    v.as_str().is_some_and(|t| !t.trim().is_empty())
}

/// STD-CLS-003: a `DAT-PortDecl` (its optional members since 1.0-rc2, DEC-067).
fn check_port(p: &Value, custom: &BTreeSet<String>) -> Result<(), BpaError> {
    require(p, PORT_REQUIRED, "port")?;
    let o = p
        .as_object()
        .ok_or_else(|| err("MODEL.INVALID_VALUE", "a port is a map"))?;
    if let Some(k) = o.keys().find(|k| !PORT_MEMBERS.contains(&k.as_str())) {
        return Err(err(
            "MODEL.INVALID_VALUE",
            format!("unknown port member {k}"),
        ));
    }
    let name = s(p, "name").unwrap_or_default();
    if !names::matches("member-name", name) {
        return Err(err("MODEL.INVALID_NAME", name));
    }
    let kind = s(p, "kind").unwrap_or_default();
    if !matches!(kind, "query" | "command") {
        return Err(err("MODEL.INVALID_VALUE", "port kind"));
    }
    if let Some(params) = p.get("params") {
        let Some(list) = params.as_array() else {
            return Err(err("MODEL.INVALID_VALUE", "params is a list"));
        };
        for q in list {
            check_field(q, custom)?;
        }
    }
    if let Some(r) = p.get("result")
        && r.as_str().is_none_or(|t| t.is_empty())
    {
        return Err(err("MODEL.INVALID_VALUE", "result is a type name"));
    }
    if members(p, "errors").any(|e| !is_error_code(e)) {
        return Err(err("MODEL.INVALID_VALUE", "an error is a registered code"));
    }
    for st in members(p, "states") {
        if !st
            .as_str()
            .is_some_and(|t| names::matches("member-name", t))
        {
            return Err(err("MODEL.INVALID_NAME", format!("state {st}")));
        }
    }
    if let Some(t) = p.get("transition") {
        if !t.as_str().is_some_and(|t| names::matches("member-name", t)) {
            return Err(err("MODEL.INVALID_NAME", "transition"));
        }
        if kind != "command" {
            return Err(err(
                "MODEL.INVALID_VALUE",
                "only a command port triggers a transition",
            ));
        }
    }
    if members(p, "guards").any(|g| g.as_str().is_none_or(|t| t.is_empty())) {
        return Err(err("MODEL.INVALID_VALUE", "a guard is a reference"));
    }
    if let Some(i) = p.get("impl")
        && !non_empty_text(i)
    {
        return Err(err("MODEL.INVALID_VALUE", "impl references a logic asset"));
    }
    if p["permission"].as_str().is_none_or(|t| t.is_empty()) {
        return Err(err(
            "MODEL.INVALID_VALUE",
            "permission is a policy action name",
        ));
    }
    if let Some(i) = p.get("idempotent") {
        if !i.is_boolean() {
            return Err(err("MODEL.INVALID_VALUE", "idempotent is boolean"));
        }
        if kind == "query" && i == &Value::Bool(false) {
            return Err(err(
                "MODEL.INVALID_VALUE",
                "a query port is always idempotent",
            ));
        }
    }
    if let Some(u) = p.get("ui")
        && !u.is_object()
    {
        return Err(err("MODEL.INVALID_VALUE", "ui is a map"));
    }
    if p["description"].as_str().is_none_or(|t| t.is_empty()) {
        return Err(err("MODEL.INVALID_VALUE", "description"));
    }
    Ok(())
}

/// STD-LGS-030: the `validations` of a class, an extension or a refinement (1.0-rc2,
/// DEC-067); the rule text is parsed by the engine layer.
pub fn check_validations(items: &Value) -> Result<(), BpaError> {
    let Some(list) = items.as_array() else {
        return Err(err("MODEL.INVALID_VALUE", "validations is a list"));
    };
    for v in list {
        let ok = v.is_object()
            && v["name"].is_string()
            && v["rule"].is_string()
            && SEVERITIES.contains(&s(v, "severity").unwrap_or_default());
        if !ok {
            return Err(err(
                "MODEL.DECLARATION_INCOMPLETE",
                "a validation has name, rule and severity",
            ));
        }
    }
    Ok(())
}

/// STD-CLS-031: `overlay_policy` maps members to `overridable`, `extend_only` or `locked`.
fn check_overlay_policy(d: &Value) -> Result<(), BpaError> {
    let Some(pol) = d.get("overlay_policy").filter(|v| !v.is_null()) else {
        return Ok(());
    };
    let ok = pol.as_object().is_some_and(|m| {
        m.values()
            .all(|v| OVERLAY.contains(&v.as_str().unwrap_or_default()))
    });
    if !ok {
        return Err(err(
            "MODEL.INVALID_VALUE",
            "overlay_policy maps members to overridable, extend_only or locked",
        ));
    }
    Ok(())
}

fn members<'a>(d: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    d.get(key).and_then(Value::as_array).into_iter().flatten()
}

/// Check a class or trait declaration (STD-CLS-001, -002).
pub fn check_decl(d: &Value, custom: &BTreeSet<String>) -> Result<(), BpaError> {
    let is_trait = d.get("trait").is_some_and(|t| t.as_bool() == Some(true));
    require(
        d,
        if is_trait {
            TRAIT_REQUIRED
        } else {
            CLASS_REQUIRED
        },
        "class",
    )?;
    if !names::matches("class-name", s(d, "name").unwrap_or_default()) {
        return Err(err("MODEL.INVALID_NAME", s(d, "name").unwrap_or_default()));
    }
    if !names::matches("namespace", s(d, "namespace").unwrap_or_default()) {
        return Err(err(
            "MODEL.INVALID_NAME",
            s(d, "namespace").unwrap_or_default(),
        ));
    }
    if !is_trait && !KINDS.contains(&s(d, "kind").unwrap_or_default()) {
        return Err(err("MODEL.INVALID_VALUE", "kind"));
    }
    let mut seen = BTreeSet::new();
    for f in members(d, "fields") {
        check_field(f, custom)?;
        if !seen.insert(s(f, "name").unwrap_or_default().to_string()) {
            return Err(err("MODEL.DECLARATION_INCOMPLETE", "duplicate field"));
        }
    }
    let mut seen = BTreeSet::new();
    for p in members(d, "ports") {
        check_port(p, custom)?;
        if !seen.insert(s(p, "name").unwrap_or_default().to_string()) {
            return Err(err("MODEL.DECLARATION_INCOMPLETE", "duplicate port"));
        }
    }
    if let Some(v) = d.get("validations") {
        check_validations(v)?;
    }
    // STD-CLS-004 (1.0-rc3): the lifecycle is a URI; a class with a lifecycle has no field state
    if let Some(l) = d.get("lifecycle") {
        if !non_empty_text(l) {
            return Err(err(
                "MODEL.INVALID_VALUE",
                "lifecycle is the URI of a Lifecycle object",
            ));
        }
        if members(d, "fields").any(|f| s(f, "name") == Some("state")) {
            return Err(err(
                "MODEL.INVALID_VALUE",
                "a class with a lifecycle has no field state",
            ));
        }
    }
    check_overlay_policy(d)
}

/// Check an extension as given in adapter inputs or as the body of a `ClassExtension`
/// object (STD-CLS-030).
pub fn check_extension(e: &Value, custom: &BTreeSet<String>) -> Result<(), BpaError> {
    require(e, &["target", "namespace", "order"], "extension")?;
    if !names::matches("namespace", s(e, "namespace").unwrap_or_default()) {
        return Err(err(
            "MODEL.INVALID_NAME",
            s(e, "namespace").unwrap_or_default(),
        ));
    }
    for f in members(e, "fields") {
        check_field(f, custom)?;
    }
    for p in members(e, "ports") {
        check_port(p, custom)?;
    }
    if let Some(v) = e.get("validations") {
        check_validations(v)?;
    }
    for r in members(e, "refinements") {
        if let Some(v) = r.get("validations") {
            check_validations(v)?;
        }
    }
    Ok(())
}

/// Check a custom value type declaration (DAT-ValueTypeDecl, STD-FND-026) without its
/// expressions, which the L1 evaluator checks.
pub fn check_value_type(d: &Value, known: &BTreeSet<String>) -> Result<(), BpaError> {
    for k in ["name", "base", "description"] {
        if !d.is_object() || d.get(k).is_none() {
            return Err(err(
                "MODEL.DECLARATION_INCOMPLETE",
                format!("a value type declares {k}"),
            ));
        }
    }
    let name = s(d, "name").unwrap_or_default();
    if !names::matches("class-name", name) || known.contains(name) {
        return Err(err(
            "MODEL.INVALID_NAME",
            "a value type has a PascalCase name of its own",
        ));
    }
    let base = s(d, "base").unwrap_or_default();
    if !values::TYPES.contains(&base) || NOT_A_BASE.contains(&base) {
        return Err(err(
            "MODEL.UNKNOWN_TYPE",
            "the base of a value type is a scalar type of STD-FND-020",
        ));
    }
    Ok(())
}

// ---- narrowing (STD-CLS-013) ----

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(t) => t.parse().ok(),
        _ => None,
    }
}

fn less(a: &Value, b: &Value) -> bool {
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x < y,
        _ => false,
    }
}

/// Apply a refinement `new` to an inherited field `old` (STD-CLS-013).
pub fn refine(old: &Value, new: &Value) -> Result<Value, BpaError> {
    let nv = |m: &str| err("MODEL.NARROWING_VIOLATION", m.to_string());
    if new.get("type").is_some() && new.get("type") != old.get("type") {
        return Err(nv("type"));
    }
    if old.get("required") == Some(&json!(true)) && new.get("required") == Some(&json!(false)) {
        return Err(nv("required"));
    }
    let empty = Map::new();
    let n = new.as_object().unwrap_or(&empty);
    for (k, v) in n {
        if !REFINABLE.contains(&k.as_str()) && Some(v) != old.get(k) {
            return Err(nv(k));
        }
    }
    let oc = old
        .get("constraints")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let nc = new
        .get("constraints")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (k, v) in &nc {
        if !NARROWABLE_CONSTRAINTS.contains(&k.as_str()) && Some(v) != oc.get(k) {
            return Err(nv(k));
        }
    }
    for k in MIN_KEYS {
        if let (Some(o), Some(nw)) = (oc.get(*k), nc.get(*k))
            && less(nw, o)
        {
            return Err(nv(k));
        }
    }
    for k in MAX_KEYS {
        if let (Some(o), Some(nw)) = (oc.get(*k), nc.get(*k))
            && less(o, nw)
        {
            return Err(nv(k));
        }
    }
    if let (Some(o), Some(nw)) = (oc.get("pattern"), nc.get("pattern"))
        && o != nw
    {
        return Err(nv("pattern"));
    }
    if let (Some(o), Some(nw)) = (oc.get("allowed_values"), nc.get("allowed_values")) {
        let os: Vec<&Value> = o.as_array().map(|a| a.iter().collect()).unwrap_or_default();
        let ok = nw
            .as_array()
            .map(|a| a.iter().all(|x| os.contains(&x)))
            .unwrap_or(false);
        if !ok {
            return Err(nv("allowed_values"));
        }
    }
    if let (Some(o), Some(nw)) = (oc.get("scale"), nc.get("scale"))
        && o != nw
    {
        return Err(nv("scale"));
    }
    if let (Some(o), Some(nw)) = (s(old, "classification"), s(new, "classification"))
        && values::LEVELS.iter().position(|l| *l == nw)
            < values::LEVELS.iter().position(|l| *l == o)
    {
        return Err(nv("classification"));
    }
    let mut merged = old.as_object().cloned().unwrap_or_default();
    for (k, v) in n {
        if k != "constraints" {
            merged.insert(k.clone(), v.clone());
        }
    }
    if !oc.is_empty() || !nc.is_empty() {
        let mut c = oc;
        c.extend(nc);
        merged.insert("constraints".into(), Value::Object(c));
    }
    Ok(Value::Object(merged))
}

/// STD-CLS-021: an override `special` of a port `general`.
fn check_override(general: &Value, special: &Value) -> Result<(), BpaError> {
    let psi = |m: &str| err("MODEL.PORT_SIGNATURE_INCOMPATIBLE", m.to_string());
    let params = |p: &Value| -> BTreeMap<String, Value> {
        members(p, "params")
            .map(|q| (s(q, "name").unwrap_or_default().to_string(), q.clone()))
            .collect()
    };
    let (gp, sp) = (params(general), params(special));
    if gp.keys().collect::<Vec<_>>() != sp.keys().collect::<Vec<_>>() {
        return Err(psi("parameter names"));
    }
    for (n, g) in &gp {
        // the inherited parameter must narrow the override's
        if refine(&sp[n], g).is_err() {
            return Err(psi(&format!("parameter {n}")));
        }
    }
    if special.get("result") != general.get("result") {
        return Err(psi("result type"));
    }
    let ge: Vec<&Value> = members(general, "errors").collect();
    if !members(special, "errors").all(|e| ge.contains(&e)) {
        return Err(psi("error codes"));
    }
    if special.get("kind") != general.get("kind") {
        return Err(psi("port kind"));
    }
    Ok(())
}

// ---- the registry ----

/// A port in an effective definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    /// The declaration (with its namespaced name for extension ports).
    pub decl: Value,
    /// Class that declares it.
    pub declared_by: String,
    /// Class whose implementation is in force, if any.
    pub impl_by: Option<String>,
}

/// The effective definition of a class (STD-CLS-001…030).
#[derive(Debug, Clone)]
pub struct Class {
    /// Class name.
    pub name: String,
    /// Resolved kind (`definition`, `ledger`, `system`, `raw_document`); `None` for the
    /// abstract roots.
    pub kind: Option<String>,
    /// No direct instances.
    pub is_abstract: bool,
    /// `date` or `instant`.
    pub valid_time: String,
    /// Effective fields by name (own, inherited, refined and extension fields).
    pub fields: BTreeMap<String, Value>,
    /// Effective ports by name.
    pub ports: BTreeMap<String, Port>,
    /// `direct` or `change_set` (STD-VER-012).
    pub write_policy: String,
    /// Ledger declaration `{key, amounts, double_entry, period_scope}`.
    pub ledger: Option<Map<String, Value>>,
    /// `schema_version` of the declaration.
    pub schema_version: i64,
    /// Content identifier of the declaration as loaded.
    pub class_version: String,
    /// C3 linearization, the class first (STD-CLS-011).
    pub linearization: Vec<String>,
    /// Validations of the declarations, refinements and extensions along the
    /// linearization (STD-LGS-030; since 1.0-rc2).
    pub validations: Vec<Value>,
    /// Overlay policy of the members (STD-CLS-031; since 1.0-rc2).
    pub overlay_policy: Map<String, Value>,
    /// URI of the lifecycle in force: the nearest one named along the linearization
    /// (STD-CLS-004; since 1.0-rc3).
    pub lifecycle: Option<String>,
}

impl Class {
    /// The definition as the adapter reports it (`effective_class`, `class_at`).
    pub fn to_json(&self) -> Value {
        let mut out = json!({
            "linearization": self.linearization,
            "kind": self.kind,
            "write_policy": self.write_policy,
            "fields": self.fields,
        });
        if !self.validations.is_empty() {
            out["validations"] = Value::Array(self.validations.clone());
        }
        if !self.overlay_policy.is_empty() {
            out["overlay_policy"] = Value::Object(self.overlay_policy.clone());
        }
        if let Some(l) = &self.lifecycle {
            out["lifecycle"] = json!(l);
        }
        if !self.ports.is_empty() {
            let ports: Map<String, Value> = self
                .ports
                .iter()
                .map(|(k, p)| (k.clone(), p.decl.clone()))
                .collect();
            out["ports"] = Value::Object(ports);
        }
        out
    }

    /// Fields that hold references, with the URI values of a body: `(field, uri)`.
    pub fn refs(&self, body: &Value) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (name, f) in &self.fields {
            let Some(v) = body.get(name) else { continue };
            if v.is_null() {
                continue;
            }
            if s(f, "type") == Some("reference") {
                if let Some(u) = v.as_str() {
                    out.push((name.clone(), u.to_string()));
                }
            } else if is_reference_list(f) {
                for u in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
                    out.push((name.clone(), u.to_string()));
                }
            }
        }
        out
    }

    /// The relationship declared on a field, if any.
    pub fn relationship(&self, field: &str) -> Option<&Value> {
        self.fields
            .get(field)
            .and_then(|f| f.get("relationship"))
            .filter(|r| r.is_object())
    }
}

/// The evaluator of custom value types (STD-FND-026), installed by the engine layer:
/// `ubos-model` cannot depend on the expression language (UBS-ARC-07 §2).
pub trait CustomTypes: Send + Sync {
    /// Type-check the `normalize` and `validate` members of a declaration.
    fn check(&self, decl: &Value) -> Result<(), BpaError>;
    /// Normalise, then validate, a value of the type; returns the canonical value.
    fn value(
        &self,
        decl: &Value,
        value: &Value,
        constraints: Option<&Value>,
    ) -> Result<Value, BpaError>;
}

#[derive(Clone)]
struct Hook(Option<std::sync::Arc<dyn CustomTypes>>);

impl std::fmt::Debug for Hook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "CustomTypes(installed)"
        } else {
            "CustomTypes(none)"
        })
    }
}

/// A registry of class declarations with their effective definitions.
#[derive(Debug, Clone)]
pub struct Registry {
    decls: BTreeMap<String, Value>,
    classes: BTreeMap<String, Class>,
    value_types: BTreeMap<String, Value>,
    custom: Hook,
    /// Largest canonical body in bytes (STD-OBJ-009, adapter `set_limits`).
    pub max_body: usize,
    /// Field indexes declared to the kernel by class (DSN-DVM-902, FR-ANL-014; PH-1 step
    /// 13a): an index of a class also serves its subclasses. The fields of the unique
    /// `keys` of a declaration are indexed without a declaration.
    indexes: BTreeMap<String, BTreeSet<String>>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry::new()
    }
}

fn c3_merge(mut seqs: Vec<Vec<String>>) -> Result<Vec<String>, BpaError> {
    let mut out = Vec::new();
    seqs.retain(|q| !q.is_empty());
    while !seqs.is_empty() {
        let head = seqs
            .iter()
            .map(|q| &q[0])
            .find(|h| !seqs.iter().any(|t| t[1..].contains(h)))
            .cloned()
            .ok_or_else(|| err("MODEL.LINEARIZATION_INCONSISTENT", ""))?;
        out.push(head.clone());
        for q in &mut seqs {
            q.retain(|x| *x != head);
        }
        seqs.retain(|q| !q.is_empty());
    }
    Ok(out)
}

fn depth(decls: &BTreeMap<String, Value>, name: &str) -> usize {
    if decls[name]
        .get("trait")
        .is_some_and(|t| t.as_bool() == Some(true))
    {
        return 1;
    }
    let mut n = 0;
    let mut cur = name;
    while let Some(p) = s(&decls[cur], "parent") {
        cur = p;
        n += 1;
        if n > 1000 {
            break;
        }
    }
    n
}

impl Registry {
    /// The genesis registry.
    pub fn new() -> Self {
        let mut r = Registry {
            decls: genesis(),
            classes: BTreeMap::new(),
            value_types: BTreeMap::new(),
            custom: Hook(None),
            max_body: MAX_BODY,
            indexes: BTreeMap::new(),
        };
        r.recompute(&[]).expect("genesis model");
        r
    }

    /// Declare a field index (DSN-DVM-902): the field is an effective field of the class
    /// with an indexable type (`MODEL.UNKNOWN_FIELD`, `MODEL.INVALID_VALUE`).
    pub fn declare_index(&mut self, class: &str, field: &str) -> Result<(), BpaError> {
        let c = self.class(class)?;
        let Some(f) = c.fields.get(field) else {
            return Err(err("MODEL.UNKNOWN_FIELD", format!("{class}.{field}")));
        };
        if !ubos_types::values::indexable(s(f, "type").unwrap_or_default()) {
            return Err(err(
                "MODEL.INVALID_VALUE",
                format!("the type of {class}.{field} has no index"),
            ));
        }
        self.indexes
            .entry(class.to_string())
            .or_default()
            .insert(field.to_string());
        Ok(())
    }

    /// The declared indexes, as `{class: [field]}` (persisted by the engine).
    pub fn declared_indexes(&self) -> Value {
        Value::Object(
            self.indexes
                .iter()
                .map(|(c, fs)| (c.clone(), json!(fs)))
                .collect(),
        )
    }

    /// Restore declared indexes (unknown classes and fields are skipped).
    pub fn restore_indexes(&mut self, v: &Value) {
        for (c, fs) in v.as_object().into_iter().flatten() {
            for f in fs
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let _ = self.declare_index(c, f);
            }
        }
    }

    /// The indexed fields of a class with their types: the indexes declared on the class
    /// or an ancestor and the fields of the unique `keys` along the linearization.
    pub fn indexed_fields(&self, class: &str) -> Vec<(String, String)> {
        let Ok(c) = self.class(class) else {
            return Vec::new();
        };
        let mut names: BTreeSet<String> = BTreeSet::new();
        for anc in &c.linearization {
            if let Some(fs) = self.indexes.get(anc) {
                names.extend(fs.iter().cloned());
            }
            if let Some(d) = self.decls.get(anc) {
                for key in members(d, "keys") {
                    names.extend(
                        key.as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_string),
                    );
                }
            }
        }
        names
            .into_iter()
            .filter_map(|n| {
                let t = s(c.fields.get(&n)?, "type")?;
                ubos_types::values::indexable(t).then(|| (n, t.to_string()))
            })
            .collect()
    }

    /// The unique keys of a class (STD-CLS-001 `keys`, DSN-DVM-111): `(declaring class,
    /// fields)` along the linearization. A key declared on a class is unique among the
    /// objects of that class and its subclasses.
    pub fn unique_keys(&self, class: &str) -> Vec<(String, Vec<String>)> {
        let Ok(c) = self.class(class) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for anc in &c.linearization {
            if let Some(d) = self.decls.get(anc) {
                for key in members(d, "keys") {
                    let fields: Vec<String> = key
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                    if !fields.is_empty() {
                        out.push((anc.clone(), fields));
                    }
                }
            }
        }
        out
    }

    /// Whether a field of a class is indexed by a declaration (on the class or an
    /// ancestor), not only as a key field.
    pub fn is_declared_index(&self, class: &str, field: &str) -> bool {
        self.class(class).is_ok_and(|c| {
            c.linearization
                .iter()
                .any(|a| self.indexes.get(a).is_some_and(|fs| fs.contains(field)))
        })
    }

    /// Whether a field of a class is indexed.
    pub fn is_indexed(&self, class: &str, field: &str) -> bool {
        self.indexed_fields(class).iter().any(|(f, _)| f == field)
    }

    /// The registry with an evaluator of custom value types installed.
    pub fn with_custom(mut self, hook: std::sync::Arc<dyn CustomTypes>) -> Registry {
        self.custom = Hook(Some(hook));
        self
    }

    /// Add or replace declarations (and custom value types) and recompute every
    /// effective class.
    pub fn load(
        &self,
        classes: &[Value],
        value_types: Option<&[Value]>,
    ) -> Result<Registry, BpaError> {
        let mut out = self.clone();
        if let Some(vts) = value_types {
            let mut known = BTreeSet::new();
            for d in vts {
                check_value_type(d, &known)?;
                if let Some(h) = &out.custom.0 {
                    h.check(d)?;
                }
                known.insert(s(d, "name").unwrap_or_default().to_string());
            }
            out.value_types = vts
                .iter()
                .map(|d| (s(d, "name").unwrap_or_default().to_string(), d.clone()))
                .collect();
        }
        let custom: BTreeSet<String> = out.value_types.keys().cloned().collect();
        for d in classes {
            let name = s(d, "name").unwrap_or_default().to_string();
            if is_genesis(&name) {
                return Err(err(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "genesis class redeclared",
                ));
            }
            check_decl(d, &custom)?;
            out.decls.insert(name, d.clone());
        }
        out.recompute(&[])?;
        Ok(out)
    }

    /// The registry with the given extensions applied to every class (bodies of
    /// `ClassExtension` objects, STD-CLS-030).
    pub fn with_extensions(&self, extensions: &[Value]) -> Result<Registry, BpaError> {
        let mut out = self.clone();
        out.recompute(extensions)?;
        Ok(out)
    }

    fn recompute(&mut self, extensions: &[Value]) -> Result<(), BpaError> {
        let custom: BTreeSet<String> = self.value_types.keys().cloned().collect();
        for e in extensions {
            check_extension(e, &custom)?;
        }
        let names: Vec<String> = self.decls.keys().cloned().collect();
        let mut classes = BTreeMap::new();
        for n in names {
            classes.insert(n.clone(), self.effective(&n, extensions)?);
        }
        self.classes = classes;
        Ok(())
    }

    /// Names of the custom value types.
    pub fn value_types(&self) -> &BTreeMap<String, Value> {
        &self.value_types
    }

    /// Declarations as loaded (genesis excluded), for persistence.
    pub fn declarations(&self) -> Vec<Value> {
        self.decls
            .iter()
            .filter(|(n, _)| !is_genesis(n))
            .map(|(_, d)| d.clone())
            .collect()
    }

    /// A declaration by name, or `MODEL.REFERENCE_INVALID`.
    pub fn decl(&self, name: &str) -> Result<&Value, BpaError> {
        self.decls
            .get(name)
            .ok_or_else(|| err("MODEL.REFERENCE_INVALID", format!("unknown class {name}")))
    }

    /// C3 linearization (STD-CLS-011): `L(C) = C + merge(L(T1), …, L(Tn), L(P), [T1, …, Tn, P])`.
    pub fn linearize(&self, name: &str) -> Result<Vec<String>, BpaError> {
        self.lin(name, &mut Vec::new())
    }

    fn lin(&self, name: &str, stack: &mut Vec<String>) -> Result<Vec<String>, BpaError> {
        if stack.iter().any(|x| x == name) {
            return Err(err("MODEL.INHERITANCE_CYCLE", name));
        }
        let d = self.decl(name)?;
        let mut parents: Vec<String> = members(d, "traits")
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if let Some(p) = s(d, "parent") {
            parents.push(p.to_string());
        }
        stack.push(name.to_string());
        let mut seqs = Vec::new();
        for p in &parents {
            seqs.push(self.lin(p, stack)?);
        }
        stack.pop();
        seqs.push(parents);
        let mut out = vec![name.to_string()];
        out.extend(c3_merge(seqs)?);
        Ok(out)
    }

    fn resolve_kind(&self, name: &str, depth_: usize) -> Result<Option<String>, BpaError> {
        if depth_ > 1000 {
            return Err(err("MODEL.INHERITANCE_CYCLE", name));
        }
        let d = self.decl(name)?;
        if d.get("trait").is_some_and(|t| t.as_bool() == Some(true)) {
            return Ok(None);
        }
        let Some(parent) = s(d, "parent") else {
            return Ok(None);
        };
        let pk = self.resolve_kind(parent, depth_ + 1)?;
        let k = s(d, "kind");
        if parent == "DataObject" && !is_genesis(name) {
            return match k {
                Some(k) if CONCRETE.contains(&k) => Ok(Some(k.to_string())),
                _ => Err(err(
                    "MODEL.KIND_MISMATCH",
                    "children of DataObject declare a concrete kind",
                )),
            };
        }
        if is_genesis(name) {
            return Ok(k.map(str::to_string).or(pk));
        }
        match k {
            Some("inherit") => Ok(pk),
            Some(k) if Some(k) == pk.as_deref() => Ok(pk),
            _ => Err(err("MODEL.KIND_MISMATCH", name)),
        }
    }

    fn effective(&self, name: &str, extensions: &[Value]) -> Result<Class, BpaError> {
        let lin = self.linearize(name)?;
        let kind = self.resolve_kind(name, 0)?;
        for c in &lin {
            let d = &self.decls[c];
            if d.get("trait").is_some_and(|t| t.as_bool() == Some(true))
                && let Some(k) = &kind
                && !members(d, "applies_to").any(|a| a.as_str() == Some(k))
            {
                return Err(err(
                    "MODEL.KIND_MISMATCH",
                    "trait does not apply to the kind",
                ));
            }
        }
        let mut fields: BTreeMap<String, Value> = BTreeMap::new();
        let mut ports: BTreeMap<String, Port> = BTreeMap::new();
        let mut policy: Option<String> = None;
        let mut validations: Vec<Value> = Vec::new();
        let mut overlay: Map<String, Value> = Map::new();
        for c in lin.iter().rev() {
            let d = &self.decls[c];
            validations.extend(members(d, "validations").cloned());
            if let Some(o) = d.get("overlay_policy").and_then(Value::as_object) {
                overlay.extend(o.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            if let Some(wp) = s(d, "write_policy") {
                if policy.as_deref() == Some("change_set") && wp == "direct" {
                    return Err(err("MODEL.NARROWING_VIOLATION", "write_policy"));
                }
                policy = Some(wp.to_string());
            }
            for f in members(d, "fields") {
                let n = s(f, "name").unwrap_or_default().to_string();
                let v = match fields.get(&n) {
                    Some(old) => refine(old, f)?,
                    None => f.clone(),
                };
                fields.insert(n, v);
            }
            for p in members(d, "ports") {
                let n = s(p, "name").unwrap_or_default().to_string();
                let inherited_impl = ports.get(&n).and_then(|q| q.impl_by.clone());
                if let Some(old) = ports.get(&n) {
                    check_override(&old.decl, p)?;
                }
                let impl_by = if p.get("impl").is_some() {
                    Some(c.clone())
                } else {
                    inherited_impl
                };
                ports.insert(
                    n,
                    Port {
                        decl: p.clone(),
                        declared_by: c.clone(),
                        impl_by,
                    },
                );
            }
        }
        // Extensions of the classes in L(C): most general target first, then order, then
        // namespace (STD-CLS-030).
        let pos: BTreeMap<&str, usize> = lin
            .iter()
            .rev()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();
        let mut exts: Vec<&Value> = extensions
            .iter()
            .filter(|e| s(e, "target").is_some_and(|t| pos.contains_key(t)))
            .collect();
        exts.sort_by(|a, b| {
            let ka = (
                pos[s(a, "target").unwrap_or_default()],
                a.get("order").and_then(Value::as_i64).unwrap_or_default(),
                s(a, "namespace").unwrap_or_default(),
            );
            let kb = (
                pos[s(b, "target").unwrap_or_default()],
                b.get("order").and_then(Value::as_i64).unwrap_or_default(),
                s(b, "namespace").unwrap_or_default(),
            );
            ka.cmp(&kb)
        });
        for e in exts {
            let ns = s(e, "namespace").unwrap_or_default();
            let target = s(e, "target").unwrap_or_default().to_string();
            for f in members(e, "fields") {
                let key = format!("{ns}.{}", s(f, "name").unwrap_or_default());
                if fields.contains_key(&key) {
                    return Err(err(
                        "MODEL.DECLARATION_INCOMPLETE",
                        format!("duplicate member {key}"),
                    ));
                }
                let mut nf = f.clone();
                nf["name"] = json!(key);
                fields.insert(key, nf);
            }
            for r in members(e, "refinements") {
                let n = s(r, "name").unwrap_or_default().to_string();
                let Some(old) = fields.get(&n) else {
                    return Err(err(
                        "MODEL.NARROWING_VIOLATION",
                        "refinement of an unknown member",
                    ));
                };
                // STD-CLS-031: the overlay policy of the member (1.0-rc2, DEC-067)
                let pol = overlay
                    .get(&n)
                    .and_then(Value::as_str)
                    .unwrap_or("overridable");
                if pol == "locked" {
                    return Err(err("TEN.OVERRIDE_LOCKED", n));
                }
                let additive = r.as_object().is_some_and(|m| {
                    m.keys()
                        .all(|k| matches!(k.as_str(), "name" | "constraints" | "validations"))
                });
                if pol == "extend_only" && !additive {
                    return Err(err(
                        "TEN.OVERRIDE_LOCKED",
                        format!("{n} accepts additive refinements only"),
                    ));
                }
                let mut plain = r.clone();
                if let Some(m) = plain.as_object_mut() {
                    m.remove("validations");
                }
                let v = refine(old, &plain)?;
                fields.insert(n, v);
                validations.extend(members(r, "validations").cloned());
            }
            validations.extend(members(e, "validations").cloned());
            for v in members(e, "views") {
                let locked = |k: &str| overlay.get(k).and_then(Value::as_str) == Some("locked");
                let mode = v.get("mode").and_then(Value::as_str).unwrap_or_default();
                if locked("views") || (v.is_object() && locked(&format!("views.{mode}"))) {
                    return Err(err("TEN.OVERRIDE_LOCKED", "views"));
                }
            }
            for p in members(e, "ports") {
                let key = format!("{ns}.{}", s(p, "name").unwrap_or_default());
                if ports.contains_key(&key) {
                    return Err(err(
                        "MODEL.DECLARATION_INCOMPLETE",
                        format!("duplicate member {key}"),
                    ));
                }
                let mut np = p.clone();
                np["name"] = json!(key);
                let impl_by = p.get("impl").is_some().then(|| target.clone());
                ports.insert(
                    key,
                    Port {
                        decl: np,
                        declared_by: target.clone(),
                        impl_by,
                    },
                );
            }
        }
        let policy = policy.unwrap_or_else(|| {
            if lin.iter().any(|c| c == "ConfigAsset") {
                "change_set".into()
            } else {
                "direct".into()
            }
        });
        let d = &self.decls[name];
        // STD-CLS-004 (1.0-rc3): the lifecycle in force is the nearest one named along the
        // linearization; a class with a lifecycle has no field state
        let lifecycle = lin
            .iter()
            .find_map(|c| s(&self.decls[c], "lifecycle").map(str::to_string));
        if lifecycle.is_some() && fields.contains_key("state") {
            return Err(err(
                "MODEL.INVALID_VALUE",
                "a class with a lifecycle has no field state",
            ));
        }
        Ok(Class {
            name: name.to_string(),
            kind,
            is_abstract: d.get("abstract") == Some(&Value::Bool(true)),
            valid_time: s(d, "valid_time").unwrap_or("date").to_string(),
            fields,
            ports,
            write_policy: policy,
            ledger: d.get("ledger").and_then(Value::as_object).cloned(),
            schema_version: d.get("schema_version").and_then(Value::as_i64).unwrap_or(1),
            class_version: content_id("ubos.chunk.v1", d)?,
            linearization: lin,
            validations,
            overlay_policy: overlay,
            lifecycle,
        })
    }

    /// The effective class, or `MODEL.REFERENCE_INVALID`.
    pub fn class(&self, name: &str) -> Result<&Class, BpaError> {
        self.classes
            .get(name)
            .ok_or_else(|| err("MODEL.REFERENCE_INVALID", format!("unknown class {name}")))
    }

    /// All effective classes.
    pub fn classes(&self) -> impl Iterator<Item = &Class> {
        self.classes.values()
    }

    /// Names of the classes whose linearization contains `name`.
    pub fn subclasses(&self, name: &str) -> Vec<String> {
        self.classes
            .values()
            .filter(|c| c.linearization.iter().any(|l| l == name))
            .map(|c| c.name.clone())
            .collect()
    }

    /// STD-CLS-020: the class whose implementation of `port` runs for an object of `name`;
    /// with `after`, the search continues after that class (`run.super`).
    pub fn dispatch(&self, name: &str, port: &str, after: Option<&str>) -> Result<Value, BpaError> {
        let c = self.class(name)?;
        let lin = &c.linearization;
        let start = match after {
            Some(a) => {
                lin.iter().position(|x| x == a).ok_or_else(|| {
                    err(
                        "MODEL.REFERENCE_INVALID",
                        format!("class {a} is not in the linearization"),
                    )
                })? + 1
            }
            None => 0,
        };
        let not_impl = || err("MODEL.PORT_NOT_IMPLEMENTED", port);
        if port.contains('.') {
            return match c.ports.get(port) {
                Some(p)
                    if p.decl.get("impl").is_some() && lin[start..].contains(&p.declared_by) =>
                {
                    Ok(json!({"class": p.declared_by, "impl": p.decl["impl"]}))
                }
                _ => Err(not_impl()),
            };
        }
        for cls in &lin[start..] {
            for p in members(&self.decls[cls], "ports") {
                if s(p, "name") == Some(port) && p.get("impl").is_some() {
                    return Ok(json!({"class": cls, "impl": p["impl"]}));
                }
            }
        }
        Err(not_impl())
    }

    /// STD-CLS-022: whether a type selector matches an object of class `name`, and the
    /// selector's specificity (its depth in the hierarchy).
    pub fn type_selector(&self, name: &str, selector: &str, only: bool) -> Result<Value, BpaError> {
        self.decl(selector)?;
        let lin = self.linearize(name)?;
        let matches = if only {
            selector == name
        } else {
            lin.iter().any(|c| c == selector)
        };
        Ok(json!({"matches": matches, "specificity": depth(&self.decls, selector)}))
    }

    /// Validate an object body against a class; `partial` skips the required-field and
    /// size checks. Members undeclared on a RawDocument class are kept as they are
    /// (STD-KIND-060).
    pub fn check_body(
        &self,
        class: &str,
        body: &Value,
        partial: bool,
    ) -> Result<Map<String, Value>, BpaError> {
        let c = self.class(class)?;
        let b = body
            .as_object()
            .ok_or_else(|| err("MODEL.INVALID_VALUE", "body is an object"))?;
        let mut out = Map::new();
        for (k, v) in b {
            match c.fields.get(k) {
                Some(f) => {
                    let t = s(f, "type").unwrap_or_default();
                    let value = match self.value_types.get(t) {
                        Some(d) => match &self.custom.0 {
                            Some(h) if !v.is_null() => h.value(d, v, f.get("constraints"))?,
                            Some(_) => Value::Null,
                            None => {
                                return Err(err(
                                    "ADAPTER.UNSUPPORTED",
                                    "custom value types need an evaluator (STD-FND-026)",
                                ));
                            }
                        },
                        None => values::validate_value(t, f.get("constraints"), v)?,
                    };
                    out.insert(k.clone(), value);
                }
                None if c.kind.as_deref() == Some("raw_document") => {
                    canonical(v)?;
                    out.insert(k.clone(), v.clone());
                }
                None => {
                    return Err(err("MODEL.INVALID_VALUE", format!("undeclared field {k}")));
                }
            }
        }
        if !partial {
            for (name, f) in &c.fields {
                if f.get("required") == Some(&Value::Bool(true))
                    && out.get(name).is_none_or(Value::is_null)
                {
                    return Err(err(
                        "MODEL.CONSTRAINT_VIOLATED",
                        format!("required field {name}"),
                    ));
                }
            }
            if canonical(&Value::Object(out.clone()))?.len() > self.max_body {
                return Err(err(
                    "MODEL.OBJECT_TOO_LARGE",
                    "body larger than the configured limit",
                ));
            }
        }
        Ok(out)
    }
}

/// A registry over the declarations given to a pure adapter operation (`linearize`,
/// `effective_class`, `dispatch`, `type_selector`): declarations are checked, and the
/// extensions apply to every class.
pub fn registry_of(classes: &[Value], extensions: &[Value]) -> Result<Registry, BpaError> {
    let mut r = Registry::new();
    for d in classes {
        let name = s(d, "name").unwrap_or_default().to_string();
        if is_genesis(&name) {
            return Err(err(
                "MODEL.DECLARATION_INCOMPLETE",
                "genesis class redeclared",
            ));
        }
        r.decls.insert(name, d.clone());
    }
    let custom = BTreeSet::new();
    for d in classes {
        check_decl(d, &custom)?;
    }
    r.recompute(extensions)?;
    Ok(r)
}

/// `linearize` without declaration checks (the error registry vectors use bare
/// declarations).
pub fn linearize_raw(classes: &[Value], name: &str) -> Result<Vec<String>, BpaError> {
    let mut r = Registry::new();
    for d in classes {
        r.decls
            .insert(s(d, "name").unwrap_or_default().to_string(), d.clone());
    }
    r.linearize(name)
}
