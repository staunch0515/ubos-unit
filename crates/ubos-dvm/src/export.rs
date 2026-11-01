//! Model export (PH-2 step 7f, DEC-133; DSN-DVM-132; FR-MODEL-103, -104): an OpenAPI 3.1
//! description of a model scope's REST operations and ports, and a model graph.
//!
//! * **Scope.** The classes whose namespace is the given one or lies below it, or every
//!   class outside the kernel's genesis model when no scope is given.
//! * **OpenAPI.** One schema per class body (fields as JSON Schema, inherited ones
//!   included), one per stored object, and the paths of the Server's REST binding
//!   (`/api/v1/<authority>`): `GET /<Class>` (a criteria query in `q`), `GET /<Class>/{slug}`
//!   and `POST /<Class>/{slug}/<port>` for every port of a concrete class, with bearer
//!   authentication and problem details (RFC 9457) for errors.
//! * **Graph** (`ubos.model-graph.v1`). Nodes for classes and traits, with kind, namespace,
//!   abstractness, schema version and members. Edges `extends` (parent), `has_trait`,
//!   `references` (a reference field and its target class) and `implemented_by` (a port and
//!   the logic asset that implements it).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use ubos_model::Registry;
use ubos_types::{BpaError, content_id};

/// The format tag of the model graph.
pub const GRAPH_FORMAT: &str = "ubos.model-graph.v1";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The classes of a scope, in name order.
fn scope_classes(reg: &Registry, scope: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = reg
        .classes()
        .filter(|c| {
            let ns = reg
                .decl(&c.name)
                .ok()
                .and_then(|d| s(d, "namespace"))
                .unwrap_or_default();
            match scope {
                Some(sc) => ns == sc || ns.starts_with(&format!("{sc}.")),
                None => !ubos_model::is_genesis(&c.name),
            }
        })
        .map(|c| c.name.clone())
        .collect();
    out.sort();
    out
}

/// The JSON Schema of a field or parameter declaration (STD-FND-020 value types).
pub fn field_schema(f: &Value) -> Value {
    let t = s(f, "type").unwrap_or("json");
    let mut out = match t {
        "text" => json!({"type": "string"}),
        "integer" => {
            json!({"type": ["integer", "string"], "description": "an integer; beyond 2^53 a decimal string"})
        }
        "decimal" => json!({"type": "string", "pattern": "^-?[0-9]+(\\.[0-9]+)?$"}),
        "money" => json!({"type": "object", "required": ["amount", "currency"],
            "properties": {"amount": {"type": "string", "pattern": "^-?[0-9]+(\\.[0-9]+)?$"},
                           "currency": {"type": "string", "pattern": "^[A-Z]{3}$"}}}),
        "quantity" => json!({"type": "object", "required": ["value", "unit"],
            "properties": {"value": {"type": "string"}, "unit": {"type": "string"}}}),
        "boolean" => json!({"type": "boolean"}),
        "date" => json!({"type": "string", "format": "date"}),
        "time" => {
            json!({"type": "string", "pattern": "^[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]{1,6})?$"})
        }
        "instant" => json!({"type": "string", "format": "date-time"}),
        "period" => json!({"type": "object", "required": ["from"],
            "properties": {"from": {"type": "string"}, "to": {"type": ["string", "null"]}}}),
        "duration" => json!({"type": "string", "format": "duration"}),
        "enum" => {
            json!({"type": "string", "enum": f.get("allowed_values").cloned().unwrap_or(json!([]))})
        }
        "reference" => {
            let mut r = json!({"type": "string", "format": "uri"});
            if let Some(tc) = s(f, "target_class") {
                r["description"] = json!(format!("a reference to a {tc}"));
            }
            r
        }
        "composite" => {
            let mut props = Map::new();
            let mut req = Vec::new();
            for m in f
                .get("members")
                .or_else(|| f.get("fields"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(n) = s(m, "name") {
                    props.insert(n.to_string(), field_schema(m));
                    if m["required"] == json!(true) {
                        req.push(json!(n));
                    }
                }
            }
            json!({"type": "object", "properties": props, "required": req})
        }
        "list" => {
            let item = f
                .get("item_type")
                .map(|it| match it {
                    Value::String(t) => field_schema(&json!({ "type": t })),
                    other => field_schema(other),
                })
                .unwrap_or(json!({}));
            json!({"type": "array", "items": item})
        }
        "map" => {
            let v = f
                .get("value_type")
                .map(|it| match it {
                    Value::String(t) => field_schema(&json!({ "type": t })),
                    other => field_schema(other),
                })
                .unwrap_or(json!({}));
            json!({"type": "object", "additionalProperties": v})
        }
        "document" => {
            json!({"type": "object", "description": "a rich-text document (STD-FND-025)"})
        }
        "blob" => json!({"type": "object", "required": ["content_id", "size", "media_type"],
            "properties": {"content_id": {"type": "string"}, "size": {"type": "integer"},
                           "media_type": {"type": "string"}, "name": {"type": "string"}}}),
        _ => json!({}),
    };
    for (k, j) in [
        ("min_length", "minLength"),
        ("max_length", "maxLength"),
        ("pattern", "pattern"),
        ("min_items", "minItems"),
        ("max_items", "maxItems"),
    ] {
        if let Some(v) = f.get(k).filter(|v| !v.is_null())
            && out.is_object()
        {
            out[j] = v.clone();
        }
    }
    if let Some(d) = s(f, "description") {
        out["description"] = json!(d);
    }
    out
}

/// The schema of a class body: its effective fields.
fn body_schema(reg: &Registry, name: &str) -> Result<Value, BpaError> {
    let c = reg.class(name)?;
    let mut props = Map::new();
    let mut req = Vec::new();
    for (n, f) in &c.fields {
        props.insert(n.clone(), field_schema(f));
        if f["required"] == json!(true) {
            req.push(json!(n));
        }
    }
    let d = reg.decl(name)?;
    let mut out = json!({"type": "object", "properties": props, "required": req,
        "description": s(d, "description").unwrap_or(name)});
    if let Some(ns) = s(d, "namespace") {
        out["x-ubos-namespace"] = json!(ns);
    }
    out["x-ubos-schema-version"] = json!(c.schema_version);
    Ok(out)
}

/// The parameters of a port as a JSON Schema object.
fn params_schema(port: &Value) -> Value {
    let mut props = Map::new();
    let mut req = Vec::new();
    match port.get("params") {
        Some(Value::Array(a)) => {
            for p in a {
                if let Some(n) = s(p, "name") {
                    props.insert(n.to_string(), field_schema(p));
                    if p["required"] == json!(true) {
                        req.push(json!(n));
                    }
                }
            }
        }
        Some(Value::Object(m)) => {
            for (n, p) in m {
                props.insert(n.clone(), field_schema(p));
            }
        }
        _ => {}
    }
    json!({"type": "object", "properties": props, "required": req})
}

/// The OpenAPI 3.1 description of a scope (DSN-DVM-132).
pub fn openapi(reg: &Registry, authority: &str, scope: Option<&str>) -> Result<Value, BpaError> {
    let names = scope_classes(reg, scope);
    let mut schemas = Map::new();
    schemas.insert("Problem".into(), json!({"type": "object", "description": "problem details (RFC 9457)",
        "properties": {"type": {"type": "string"}, "title": {"type": "string"}, "status": {"type": "integer"},
                       "detail": {"type": "string"}, "code": {"type": "string"}}, "required": ["type", "title", "status"]}));
    schemas.insert("PortResult".into(), json!({"type": "object", "description": "the outcome of a port call",
        "properties": {"process_id": {"type": "string"}, "result": {}, "commits": {"type": "array", "items": {"type": "string"}},
                       "versions": {"type": "object"}, "warnings": {"type": "array"}}, "required": ["process_id"]}));
    let mut paths = Map::new();
    let problem = json!({"description": "an error", "content": {"application/problem+json": {"schema": {"$ref": "#/components/schemas/Problem"}}}});
    let branch = json!({"name": "branch", "in": "query", "required": false, "schema": {"type": "string"},
        "description": "the branch (main when absent)"});
    for name in &names {
        let c = reg.class(name)?;
        let d = reg.decl(name)?.clone();
        schemas.insert(format!("{name}Body"), body_schema(reg, name)?);
        let is_trait = d["trait"] == json!(true);
        if c.is_abstract || is_trait {
            continue;
        }
        schemas.insert(format!("{name}Object"), json!({"type": "object",
            "description": format!("a stored version of a {name}"),
            "properties": {"object_id": {"type": "string", "format": "uuid"}, "class": {"type": "string", "const": name},
                           "slug": {"type": "string"}, "version_hash": {"type": "string"}, "valid": {"type": "object"},
                           "body": {"$ref": format!("#/components/schemas/{name}Body")}},
            "required": ["object_id", "class", "body"]}));
        let items = json!({"200": {"description": format!("{name} objects"), "content": {"application/json": {"schema": {
            "type": "object", "properties": {"items": {"type": "array", "items": {"$ref": format!("#/components/schemas/{name}Object")}}},
            "required": ["items"]}}}}, "default": problem});
        paths.insert(format!("/{name}"), json!({"get": {"operationId": format!("query{name}"),
            "summary": format!("Query {name} objects"), "tags": [name],
            "parameters": [{"name": "q", "in": "query", "required": false, "schema": {"type": "string"},
                            "description": "a criteria query: a condition, or a full query starting with from (STD-QRY)"}, branch],
            "responses": items}}));
        paths.insert(format!("/{name}/{{slug}}"), json!({"get": {"operationId": format!("read{name}"),
            "summary": format!("Read a {name}"), "tags": [name],
            "parameters": [{"name": "slug", "in": "path", "required": true, "schema": {"type": "string"}}, branch],
            "responses": items}}));
        for (pname, p) in &c.ports {
            let mut op = json!({"operationId": format!("{}{}", pname, name), "summary": s(&p.decl, "description").unwrap_or(pname),
                "tags": [name],
                "parameters": [{"name": "slug", "in": "path", "required": true, "schema": {"type": "string"}}, branch],
                "requestBody": {"required": false, "content": {"application/json": {"schema": params_schema(&p.decl)}}},
                "responses": {"200": {"description": "the port's result", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/PortResult"}}}},
                              "default": problem}});
            if let Some(k) = s(&p.decl, "kind") {
                op["x-ubos-port-kind"] = json!(k);
            }
            if let Some(i) = &p.impl_by {
                op["x-ubos-impl"] = json!(i);
            }
            paths.insert(format!("/{name}/{{slug}}/{pname}"), json!({ "post": op }));
        }
    }
    let version = content_id("ubos.chunk.v1", &json!({"openapi": schemas}))?;
    Ok(json!({
        "openapi": "3.1.0",
        "info": {"title": format!("{authority} model{}", scope.map(|x| format!(" ({x})")).unwrap_or_default()),
                 "version": version.trim_start_matches("sha256:").get(..12).unwrap_or_default(),
                 "description": "The REST operations and ports of the model scope, exported by the DVM (DSN-DVM-132)."},
        "servers": [{"url": format!("/api/v1/{authority}")}],
        "security": [{"bearer": []}],
        "paths": paths,
        "components": {"schemas": schemas,
                       "securitySchemes": {"bearer": {"type": "http", "scheme": "bearer"}}},
    }))
}

/// The model graph of a scope (`ubos.model-graph.v1`, DSN-DVM-132).
pub fn model_graph(reg: &Registry, scope: Option<&str>) -> Result<Value, BpaError> {
    let names = scope_classes(reg, scope);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for name in &names {
        let c = reg.class(name)?;
        let d = reg.decl(name)?;
        let own: Vec<&str> = d["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| s(f, "name"))
            .collect();
        nodes.push(json!({"id": name, "kind": if d["trait"] == json!(true) { json!("trait") } else { json!(c.kind) },
            "namespace": d["namespace"], "abstract": c.is_abstract, "schema_version": c.schema_version,
            "description": d["description"], "fields": own, "ports": c.ports.keys().collect::<Vec<_>>(),
            "linearization": c.linearization}));
        if let Some(p) = s(d, "parent") {
            edges.push(json!({"from": name, "to": p, "type": "extends"}));
        }
        for t in d["traits"].as_array().into_iter().flatten() {
            edges.push(json!({"from": name, "to": t, "type": "has_trait"}));
        }
        for f in d["fields"].as_array().into_iter().flatten() {
            if s(f, "type") == Some("reference")
                && let Some(tc) = s(f, "target_class")
            {
                edges.push(
                    json!({"from": name, "to": tc, "type": "references", "field": f["name"]}),
                );
            }
        }
        for (pname, p) in &c.ports {
            if p.declared_by == *name
                && let Some(i) = &p.impl_by
            {
                edges.push(json!({"from": name, "to": i, "type": "implemented_by", "port": pname}));
            }
        }
    }
    Ok(json!({"format": GRAPH_FORMAT, "scope": scope, "nodes": nodes, "edges": edges}))
}

impl Engine {
    /// Adapter operation `export_openapi` (DSN-DVM-132): `{scope?}`.
    pub fn export_openapi(&mut self, i: &Value) -> Result<Value, BpaError> {
        let authority = self.store.authority();
        openapi(&self.registry, &authority, s(i, "scope"))
    }

    /// Adapter operation `export_model_graph` (DSN-DVM-132): `{scope?}`.
    pub fn export_model_graph(&mut self, i: &Value) -> Result<Value, BpaError> {
        model_graph(&self.registry, s(i, "scope"))
    }
}
