//! The Forge project model (DSN-FRG-002, PH-1 step 7b, DEC-060): a directory with
//! `forge.toml` and the source folders `model/`, `sheets/`, `logic/`, `components/`,
//! `views/`, `seed/`, `tests/` and `docs/`. Sources are YAML (or JSON) files, so that they
//! diff and merge in Git; every loaded item remembers its file for findings.

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// The source folders of a project.
pub const FOLDERS: &[&str] = &[
    "model",
    "sheets",
    "logic",
    "components",
    "views",
    "seed",
    "tests",
    "docs",
];

/// A finding of a Forge command (DSN-FRG-101): file, line, rule, severity and message.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Source file, relative to the project root.
    pub file: String,
    /// Line (1-based) when known.
    pub line: Option<u64>,
    /// Rule or error code.
    pub rule: String,
    /// `error`, `warning` or `info`.
    pub severity: &'static str,
    /// What is wrong, in plain words.
    pub message: String,
    /// Pipeline stage.
    pub stage: &'static str,
}

impl Finding {
    /// The finding as JSON.
    pub fn to_json(&self) -> Value {
        json!({"file": self.file, "line": self.line, "rule": self.rule, "severity": self.severity,
            "message": self.message, "stage": self.stage})
    }
}

/// An item loaded from a source file.
#[derive(Debug, Clone)]
pub struct Item {
    /// File relative to the project root.
    pub file: String,
    /// The value.
    pub value: Value,
}

/// The manifest `forge.toml`.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Project name.
    pub name: String,
    /// Project version.
    pub version: String,
    /// Authority of the VAE the project installs into (STD-ADDR-001).
    pub vae: String,
    /// ABI version of the logic assets.
    pub abi: String,
    /// Branch the generated types are versioned for.
    pub branch: String,
    /// Namespace of the Buk (DSN-FRG-301); the VAE authority by default.
    pub namespace: String,
    /// One-line description.
    pub description: String,
    /// Licence terms (an SPDX expression or a reference).
    pub licence: String,
    /// Dependencies: Buk ID `<namespace>/<name>` → version range (DSN-FRG-303).
    pub dependencies: std::collections::BTreeMap<String, String>,
    /// Capabilities the Buk requests; the union of its assets' capabilities when absent.
    pub capabilities: Option<Vec<String>>,
    /// Coverage gate of the logic assets (`gates.script_coverage`, DSN-FRG-104).
    pub script_coverage: f64,
}

/// A loaded project.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project root.
    pub root: PathBuf,
    /// The manifest.
    pub manifest: Manifest,
    /// Class declarations (`model/`).
    pub classes: Vec<Item>,
    /// Custom value types (`model/`, member `value_types`).
    pub value_types: Vec<Item>,
    /// Logic assets (`logic/`): bodies of `Logic` objects.
    pub logic: Vec<Item>,
    /// Seed changes (`seed/`): commit changes in file order.
    pub seed: Vec<Item>,
    /// Tests (`tests/`).
    pub tests: Vec<Item>,
    /// Files of folders whose contents gate 1b compiles (`sheets/`, `components/`, `views/`).
    pub later: Vec<(String, &'static str)>,
    /// Findings of the load (parse errors, unknown members).
    pub findings: Vec<Finding>,
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Parse a YAML or JSON source text into JSON.
pub fn parse_source(path: &str, text: &str) -> Result<Value, (Option<u64>, String)> {
    if path.ends_with(".json") {
        return serde_json::from_str(text).map_err(|e| (Some(e.line() as u64), e.to_string()));
    }
    serde_yaml_ng::from_str::<Value>(text).map_err(|e| {
        let line = e.location().map(|l| l.line() as u64);
        (line, e.to_string())
    })
}

fn source_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(source_files(&p));
            } else if p
                .extension()
                .is_some_and(|x| x == "yaml" || x == "yml" || x == "json")
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Items of a folder: each file holds one item, a list of items, or a map with the
/// member `key` holding the list (and `value_types` next to it in `model/`).
fn load_folder(
    root: &Path,
    folder: &str,
    key: &str,
    findings: &mut Vec<Finding>,
) -> (Vec<Item>, Vec<Item>) {
    let mut items = Vec::new();
    let mut extra = Vec::new();
    for p in source_files(&root.join(folder)) {
        let file = rel(root, &p);
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) => {
                findings.push(Finding {
                    file,
                    line: None,
                    rule: "io".into(),
                    severity: "error",
                    message: e.to_string(),
                    stage: "parse",
                });
                continue;
            }
        };
        let v = match parse_source(&file, &text) {
            Ok(v) => v,
            Err((line, msg)) => {
                findings.push(Finding {
                    file,
                    line,
                    rule: "parse".into(),
                    severity: "error",
                    message: msg,
                    stage: "parse",
                });
                continue;
            }
        };
        let push = |list: &mut Vec<Item>, v: Value| {
            list.push(Item {
                file: file.clone(),
                value: v,
            })
        };
        match v {
            Value::Array(a) => a.into_iter().for_each(|x| push(&mut items, x)),
            Value::Object(ref o) if o.contains_key(key) => {
                for x in o[key].as_array().cloned().unwrap_or_default() {
                    push(&mut items, x);
                }
                if let Some(vts) = o.get("value_types").and_then(Value::as_array) {
                    for x in vts.iter().cloned() {
                        push(&mut extra, x);
                    }
                }
            }
            other => push(&mut items, other),
        }
    }
    (items, extra)
}

impl Project {
    /// Load the project at `root` (`MODEL.DECLARATION_INCOMPLETE` findings for a missing or
    /// malformed manifest).
    pub fn load(root: &Path) -> Result<Project, String> {
        let path = root.join("forge.toml");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let t: toml::Value = toml::from_str(&text).map_err(|e| format!("forge.toml: {e}"))?;
        let p = t.get("project").ok_or("forge.toml: [project] is missing")?;
        let get = |k: &str, d: &str| {
            p.get(k)
                .and_then(toml::Value::as_str)
                .unwrap_or(d)
                .to_string()
        };
        let name = get("name", "");
        if name.is_empty() {
            return Err("forge.toml: project.name is missing".into());
        }
        let manifest = Manifest {
            name,
            version: get("version", "0.1.0"),
            vae: get("vae", "acme"),
            abi: get("abi", "1.0"),
            branch: get("branch", "main"),
            namespace: get("namespace", &get("vae", "acme")),
            description: get("description", ""),
            licence: get("licence", ""),
            dependencies: t
                .get("dependencies")
                .and_then(toml::Value::as_table)
                .map(|d| {
                    d.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            capabilities: p
                .get("capabilities")
                .and_then(toml::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                }),
            script_coverage: t
                .get("gates")
                .and_then(|g| g.get("script_coverage"))
                .and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
                .unwrap_or(0.80),
        };
        let mut findings = Vec::new();
        let (classes, value_types) = load_folder(root, "model", "classes", &mut findings);
        let (logic, _) = load_folder(root, "logic", "assets", &mut findings);
        let (seed, _) = load_folder(root, "seed", "changes", &mut findings);
        let (tests, _) = load_folder(root, "tests", "tests", &mut findings);
        let mut later = Vec::new();
        for (folder, what) in [
            ("sheets", "governance sheets"),
            ("components", "L3 components"),
            ("views", "views"),
        ] {
            for p in source_files(&root.join(folder)) {
                later.push((rel(root, &p), what));
            }
        }
        Ok(Project {
            root: root.to_path_buf(),
            manifest,
            classes,
            value_types,
            logic,
            seed,
            tests,
            later,
            findings,
        })
    }

    /// The class declarations as the engine loads them.
    pub fn declarations(&self) -> Vec<Value> {
        self.classes.iter().map(|i| i.value.clone()).collect()
    }

    /// The value type declarations.
    pub fn value_type_decls(&self) -> Vec<Value> {
        self.value_types.iter().map(|i| i.value.clone()).collect()
    }

    /// The file an item with `name` came from, if any.
    pub fn file_of(items: &[Item], name: &str) -> Option<String> {
        items
            .iter()
            .find(|i| i.value.get("name").and_then(Value::as_str) == Some(name))
            .map(|i| i.file.clone())
    }

    /// Content ID of the model: the sorted declarations and value types (FR-DEV-032).
    pub fn model_hash(&self) -> Result<String, ubos_dvm::BpaError> {
        let mut decls = self.declarations();
        decls.sort_by_key(|d| {
            d.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        let mut vts = self.value_type_decls();
        vts.sort_by_key(|d| {
            d.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        ubos_dvm::content_id(
            "ubos.chunk.v1",
            &json!({"classes": decls, "value_types": vts}),
        )
    }

    /// Write the skeleton of a new project (`forge init`): the manifest, the folders and
    /// one example per folder that `forge check` and `forge test` accept.
    pub fn init(root: &Path, name: &str) -> Result<Vec<String>, String> {
        if root.join("forge.toml").exists() {
            return Err(format!("{} already holds a project", root.display()));
        }
        let mut written = Vec::new();
        let mut write = |rel: &str, text: &str| -> Result<(), String> {
            let p = root.join(rel);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
            written.push(rel.to_string());
            Ok(())
        };
        write(
            "forge.toml",
            &format!(
                "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nvae = \"acme\"\nabi = \"1.0\"\nbranch = \"main\"\n"
            ),
        )?;
        write(
            "model/customer.yaml",
            "classes:\n  - name: Customer\n    label: Customer\n    description: A customer\n    namespace: acme.app\n    parent: DataObject\n    kind: definition\n    schema_version: 1\n    fields:\n      - {name: name, label: name, description: name, type: text, required: true}\n      - {name: email, label: email, description: email, type: text}\n      - {name: credit_limit, label: credit limit, description: credit limit, type: money}\n    ports:\n      - {name: describe, description: describe, kind: command, params: [], permission: describe, impl: acme.customer.describe#describe}\n",
        )?;
        write(
            "logic/customer-describe.yaml",
            "assets:\n  - name: acme.customer.describe\n    abi: \"1.0\"\n    tier: host\n    profile: standard\n    state: committed\n    capabilities: [read]\n    exports: [describe, greet, double]\n    source:\n      functions:\n        describe:\n          params: []\n          body:\n            - {do: read.get, as: me, args: {target: self_id}}\n            - {return: \"me.fields.name\"}\n        greet:\n          params: [who]\n          body:\n            - {return: \"concat(\\\"Hello \\\", params.who)\"}\n        double:\n          params: [n]\n          body:\n            - {return: \"params.n * 2\"}\n",
        )?;
        write(
            "seed/customers.yaml",
            "changes:\n  - {op: create, class: Customer, object_id: 01890a5d-ac96-7001-8000-000000000001, slug: c-1, body: {name: Acme, email: hello@acme.example}}\n",
        )?;
        write(
            "tests/customers.yaml",
            "tests:\n  - name: a seeded customer reads back\n    kind: scenario\n    steps:\n      - op: read\n        input: {branch: main, object_id: 01890a5d-ac96-7001-8000-000000000001}\n        expect: {match: subset, result: {body: {name: Acme}}}\n      - advance: 30d\n      - op: read\n        input: {branch: main, object_id: 01890a5d-ac96-7001-8000-000000000001}\n        expect: {match: subset, result: {body: {name: Acme}}}\n  - name: describe returns the name\n    kind: port\n    target: 01890a5d-ac96-7001-8000-000000000001\n    port: describe\n    params: {}\n    expect: {result: Acme}\n  - name: greet builds a greeting\n    kind: unit\n    asset: acme.customer.describe\n    function: greet\n    args: [Ada]\n    expect: {result: Hello Ada}\n  - name: double stays in range\n    kind: property\n    asset: acme.customer.describe\n    function: double\n    cases: 20\n    params: [{type: integer, min: 0, max: 100}]\n    result_type: integer\n    invariant: \"params.result >= 0 and params.result <= 200\"\n",
        )?;
        for f in ["sheets", "components", "views", "docs"] {
            write(
                &format!("{f}/README.md"),
                &format!("# {f}\n\nSources of this folder are compiled from gate 1b.\n"),
            )?;
        }
        Ok(written)
    }
}

/// A JSON object, or an empty one.
pub fn object(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}
