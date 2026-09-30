//! `ubos-model` — engine layer (UBS-ARC-07 §2).
//!
//! Classes, traits, value types, constraints, extensions, lenses, reflection, schema export.
//! Allowed dependencies: `ubos-types` (AR-031).
//!
//! PH-0 prototype scope (DEC-033): a minimal class registry with what the BPA 0.1 vectors
//! tagged `prototype` need — declaration checks (STD-CLS-001, -002), the genesis model and
//! kinds along the parent line (STD-CLS-010, -014), inherited fields, abstract classes,
//! valid-time granularity and ledger declarations. Traits, C3 linearization and narrowing
//! (STD-CLS-011…013) follow in PH-1.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
pub use ubos_types as types;
use ubos_types::{BpaError, content_id, names, values};

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
const FIELD_REQUIRED: &[&str] = &["name", "type", "label", "description"];
const KINDS: &[&str] = &["definition", "ledger", "system", "raw_document", "inherit"];
const CONCRETE: &[&str] = &["definition", "ledger", "raw_document"];

/// Genesis classes with parent and kind (STD-CLS-010).
const GENESIS: &[(&str, Option<&str>, Option<&str>)] = &[
    ("Object", None, None),
    ("DataObject", Some("Object"), None),
    ("LogicAsset", Some("Object"), Some("definition")),
    ("ViewAsset", Some("Object"), Some("definition")),
    ("ConfigAsset", Some("Object"), Some("definition")),
    ("SystemObject", Some("Object"), Some("system")),
];

/// The effective definition of a class.
#[derive(Debug, Clone)]
pub struct Class {
    /// Class name.
    pub name: String,
    /// Resolved kind (`definition`, `ledger`, `system`, `raw_document`).
    pub kind: Option<String>,
    /// No direct instances.
    pub is_abstract: bool,
    /// `date` or `instant`.
    pub valid_time: String,
    /// Effective fields by name (own and inherited).
    pub fields: BTreeMap<String, Value>,
    /// Ledger declaration `{key, amounts, double_entry, period_scope}`.
    pub ledger: Option<Map<String, Value>>,
    /// `schema_version` of the declaration.
    pub schema_version: i64,
    /// Content identifier of the declaration as loaded.
    pub class_version: String,
    /// Parent line from the class up to `Object`.
    pub lineage: Vec<String>,
}

/// A registry of class declarations.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    decls: BTreeMap<String, Value>,
    classes: BTreeMap<String, Class>,
}

fn check_decl(d: &Value) -> Result<(), BpaError> {
    if d.get("trait").is_some() || d.get("traits").is_some() {
        return Err(BpaError::new(
            "ADAPTER.UNSUPPORTED",
            "traits are not in the PH-0 prototype",
        ));
    }
    for k in CLASS_REQUIRED {
        if d.get(*k).is_none() {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                format!("class member {k}"),
            ));
        }
    }
    names::check_name("class-name", d["name"].as_str().unwrap_or_default())?;
    names::check_name("namespace", d["namespace"].as_str().unwrap_or_default())?;
    if !KINDS.contains(&d["kind"].as_str().unwrap_or_default()) {
        return Err(BpaError::new("MODEL.INVALID_VALUE", "kind"));
    }
    let fields = d["fields"]
        .as_array()
        .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "fields"))?;
    let mut seen = std::collections::BTreeSet::new();
    for f in fields {
        for k in FIELD_REQUIRED {
            if f.get(*k).is_none() {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    format!("field member {k}"),
                ));
            }
        }
        names::check_name("member-name", f["name"].as_str().unwrap_or_default())?;
        if !values::TYPES.contains(&f["type"].as_str().unwrap_or_default()) {
            return Err(BpaError::new("MODEL.UNKNOWN_TYPE", "field type"));
        }
        if !seen.insert(f["name"].as_str().unwrap_or_default().to_string()) {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "duplicate field",
            ));
        }
    }
    Ok(())
}

impl Registry {
    /// An empty registry (genesis only).
    pub fn new() -> Self {
        Registry::default()
    }

    /// Add or replace declarations and recompute every effective class.
    pub fn load(&self, classes: &[Value]) -> Result<Registry, BpaError> {
        let mut decls = self.decls.clone();
        for d in classes {
            let name = d
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if GENESIS.iter().any(|g| g.0 == name) {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "genesis class redeclared",
                ));
            }
            check_decl(d)?;
            decls.insert(name, d.clone());
        }
        let mut out = Registry {
            decls,
            classes: BTreeMap::new(),
        };
        let names: Vec<String> = out.decls.keys().cloned().collect();
        for n in names {
            let c = out.effective(&n)?;
            out.classes.insert(n, c);
        }
        Ok(out)
    }

    fn parent_of(&self, name: &str) -> Result<Option<String>, BpaError> {
        if let Some(g) = GENESIS.iter().find(|g| g.0 == name) {
            return Ok(g.1.map(str::to_string));
        }
        let d = self.decls.get(name).ok_or_else(|| {
            BpaError::new("MODEL.REFERENCE_INVALID", format!("unknown class {name}"))
        })?;
        Ok(d["parent"].as_str().map(str::to_string))
    }

    fn kind_of(&self, name: &str, depth: usize) -> Result<Option<String>, BpaError> {
        if depth > 64 {
            return Err(BpaError::new("MODEL.INHERITANCE_CYCLE", name.to_string()));
        }
        if let Some(g) = GENESIS.iter().find(|g| g.0 == name) {
            return Ok(g.2.map(str::to_string));
        }
        let parent = self.parent_of(name)?.unwrap_or_default();
        let declared = self.decls[name]["kind"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if parent == "DataObject" {
            return if CONCRETE.contains(&declared.as_str()) {
                Ok(Some(declared))
            } else {
                Err(BpaError::new(
                    "MODEL.KIND_MISMATCH",
                    "children of DataObject declare a concrete kind",
                ))
            };
        }
        let pk = self.kind_of(&parent, depth + 1)?;
        if declared == "inherit" || Some(&declared) == pk.as_ref() {
            Ok(pk)
        } else {
            Err(BpaError::new("MODEL.KIND_MISMATCH", name.to_string()))
        }
    }

    fn effective(&self, name: &str) -> Result<Class, BpaError> {
        let mut lineage = vec![name.to_string()];
        let mut cur = name.to_string();
        while let Some(p) = self.parent_of(&cur)? {
            if lineage.contains(&p) {
                return Err(BpaError::new("MODEL.INHERITANCE_CYCLE", name.to_string()));
            }
            lineage.push(p.clone());
            cur = p;
        }
        let kind = self.kind_of(name, 0)?;
        let mut fields = BTreeMap::new();
        for c in lineage.iter().rev() {
            if let Some(d) = self.decls.get(c) {
                for f in d["fields"].as_array().into_iter().flatten() {
                    fields.insert(
                        f["name"].as_str().unwrap_or_default().to_string(),
                        f.clone(),
                    );
                }
            }
        }
        let d = self.decls.get(name).cloned().unwrap_or(Value::Null);
        Ok(Class {
            name: name.to_string(),
            kind,
            is_abstract: d.get("abstract") == Some(&Value::Bool(true)),
            valid_time: d
                .get("valid_time")
                .and_then(Value::as_str)
                .unwrap_or("date")
                .to_string(),
            fields,
            ledger: d.get("ledger").and_then(Value::as_object).cloned(),
            schema_version: d.get("schema_version").and_then(Value::as_i64).unwrap_or(0),
            class_version: content_id("ubos.chunk.v1", &d)?,
            lineage,
        })
    }

    /// The effective class, or `MODEL.REFERENCE_INVALID`.
    pub fn class(&self, name: &str) -> Result<&Class, BpaError> {
        self.classes.get(name).ok_or_else(|| {
            BpaError::new("MODEL.REFERENCE_INVALID", format!("unknown class {name}"))
        })
    }

    /// Names of the classes whose parent line contains `name`.
    pub fn subclasses(&self, name: &str) -> Vec<String> {
        self.classes
            .values()
            .filter(|c| c.lineage.iter().any(|l| l == name))
            .map(|c| c.name.clone())
            .collect()
    }

    /// Declarations as loaded, for persistence.
    pub fn declarations(&self) -> Vec<Value> {
        self.decls.values().cloned().collect()
    }

    /// Validate an object body against a class; `partial` skips the required-field check.
    /// Members undeclared on a RawDocument class are kept as they are (STD-KIND-060).
    pub fn check_body(
        &self,
        class: &str,
        body: &Value,
        partial: bool,
    ) -> Result<Map<String, Value>, BpaError> {
        let c = self.class(class)?;
        let b = body
            .as_object()
            .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "body is an object"))?;
        let mut out = Map::new();
        for (k, v) in b {
            match c.fields.get(k) {
                Some(f) => {
                    out.insert(
                        k.clone(),
                        values::validate_value(
                            f["type"].as_str().unwrap_or_default(),
                            f.get("constraints"),
                            v,
                        )?,
                    );
                }
                None if c.kind.as_deref() == Some("raw_document") => {
                    ubos_types::canonical(v)?;
                    out.insert(k.clone(), v.clone());
                }
                None => {
                    return Err(BpaError::new(
                        "MODEL.INVALID_VALUE",
                        format!("undeclared field {k}"),
                    ));
                }
            }
        }
        if !partial {
            for (name, f) in &c.fields {
                if f.get("required") == Some(&Value::Bool(true))
                    && out.get(name).is_none_or(Value::is_null)
                {
                    return Err(BpaError::new(
                        "MODEL.CONSTRAINT_VIOLATED",
                        format!("required field {name}"),
                    ));
                }
            }
        }
        Ok(out)
    }
}
