//! `forge check` (DSN-FRG-101): parse → model checks → sheets → decision tables → script
//! type checks → static analysis → lenses and views → seed data, every stage reported in one
//! run with file, line, rule and severity. The checks are the engine's own (DSN-FRG-004):
//! the class registry of `ubos-model` and the analyser of the verification gate.

use crate::project::{Finding, Project};
use serde_json::{Value, json};
use ubos_dvm::Adapter;
use ubos_dvm::host::{Services, Vae};
use ubos_dvm::verification::analyse;

/// The deterministic clock of checks and tests.
pub const CLOCK: &str = "2026-01-01T00:00:00.000000Z";

fn finding(
    file: &str,
    rule: &str,
    severity: &'static str,
    message: String,
    stage: &'static str,
) -> Finding {
    Finding {
        file: file.to_string(),
        line: None,
        rule: rule.to_string(),
        severity,
        message,
        stage,
    }
}

/// Run the pipeline; returns every finding.
pub fn check(project: &Project) -> Vec<Finding> {
    let mut out = project.findings.clone();
    // model checks (linearization, narrowing, kinds): the registry loads every declaration
    let base = ubos_dvm::expressions::registry();
    let decls = project.declarations();
    let vts = project.value_type_decls();
    let vts_opt = if vts.is_empty() {
        None
    } else {
        Some(vts.as_slice())
    };
    let registry = match base.load(&decls, vts_opt) {
        Ok(r) => Some(r),
        Err(e) => {
            // one declaration at a time names the file; the declarations that load alone
            // still serve the later stages, so that every stage reports in one run
            let mut good = Vec::new();
            let mut named = false;
            for item in &project.classes {
                match base.load(std::slice::from_ref(&item.value), vts_opt) {
                    Ok(_) => good.push(item.value.clone()),
                    Err(e1) => {
                        named = true;
                        out.push(finding(
                            &item.file,
                            e1.code,
                            "error",
                            e1.message.clone(),
                            "model",
                        ));
                    }
                }
            }
            if !named {
                out.push(finding(
                    "model/",
                    e.code,
                    "error",
                    e.message.clone(),
                    "model",
                ));
            }
            base.load(&good, vts_opt).ok()
        }
    };
    // sheets, decision tables, lenses, views: gate 1b
    for (file, what) in &project.later {
        out.push(finding(
            file,
            "later_gate",
            "warning",
            format!("{what} are compiled from gate 1b; the file is not checked"),
            "sheets",
        ));
    }
    // script type checks and static analysis: the analyser of the verification gate
    for item in &project.logic {
        let body = &item.value;
        let name = body.get("name").and_then(Value::as_str).unwrap_or("?");
        if body.get("abi").and_then(Value::as_str) != Some(project.manifest.abi.as_str()) {
            out.push(finding(
                &item.file,
                "abi",
                "error",
                format!("asset {name}: abi is {}", project.manifest.abi),
                "scripts",
            ));
        }
        match analyse(body) {
            Ok(findings) => {
                for f in findings {
                    let rule = f
                        .get("rule")
                        .and_then(Value::as_str)
                        .unwrap_or("finding")
                        .to_string();
                    let stage = if rule == "type_error" {
                        "scripts"
                    } else {
                        "static_analysis"
                    };
                    let detail: Vec<String> = f
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(k, _)| *k != "rule")
                        .map(|(k, v)| format!("{k} {v}"))
                        .collect();
                    out.push(finding(
                        &item.file,
                        &rule,
                        "error",
                        format!("asset {name}: {rule} {}", detail.join(", ")),
                        stage,
                    ));
                }
            }
            Err(e) => out.push(finding(
                &item.file,
                e.code,
                "error",
                format!("asset {name}: {}", e.message),
                "static_analysis",
            )),
        }
    }
    // port implementations named by classes (`<asset>#<function>`) must exist in logic/
    for item in &project.classes {
        for port in item
            .value
            .get("ports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(imp) = port.get("impl").and_then(Value::as_str) else {
                continue;
            };
            let (asset, function) = imp.split_once('#').unwrap_or((imp, ""));
            let found = project
                .logic
                .iter()
                .find(|l| l.value.get("name").and_then(Value::as_str) == Some(asset));
            let exported = found.is_some_and(|l| {
                function.is_empty()
                    || l.value["exports"]
                        .as_array()
                        .is_some_and(|e| e.iter().any(|x| x.as_str() == Some(function)))
            });
            if found.is_none() || !exported {
                out.push(finding(
                    &item.file,
                    "port_impl_missing",
                    "error",
                    format!(
                        "port {} names {imp}, which no asset of logic/ exports",
                        port.get("name").and_then(Value::as_str).unwrap_or("?")
                    ),
                    "scripts",
                ));
            }
        }
    }
    // seed data: loaded into an in-memory VAE with the deterministic services (the
    // declarations that load, when some do not)
    if let Some(reg) = &registry {
        match seed_vae_with(project, &reg.declarations(), &Backend::default()) {
            Ok((_, findings, _)) => out.extend(findings),
            Err(e) => out.push(finding("seed/", e.code, "error", e.message, "seed")),
        }
    }
    out
}

/// The store a fixture is built on: in memory, or a fresh VAE of a PostgreSQL database.
#[derive(Debug, Clone, Default)]
pub struct Backend {
    /// `postgres` connection string; in memory when `None`.
    pub database: Option<String>,
}

/// The fixture of a project: the VAE, the findings of the seed load and the recorder of the
/// deterministic services (released effects, jobs, telemetry).
pub type Fixture = (Vae, Vec<Finding>, std::sync::Arc<ubos_dvm::host::Recorder>);

/// An in-memory VAE with the project's model and seed loaded under the deterministic
/// services; the findings of the seed load name the seed file of a failing change.
pub fn seed_vae(project: &Project) -> Result<Fixture, ubos_dvm::BpaError> {
    seed_vae_with(project, &project.declarations(), &Backend::default())
}

/// `seed_vae` over the given declarations and backend. The L2 host is on, so that `l2`
/// assets of the project run (STD-CTX-020).
pub fn seed_vae_with(
    project: &Project,
    decls: &[Value],
    backend: &Backend,
) -> Result<Fixture, ubos_dvm::BpaError> {
    let (services, recorder) = Services::deterministic(CLOCK, [1u8; 32]);
    let adapter = match &backend.database {
        Some(url) => Adapter::postgres(url)?,
        None => Adapter::new()?,
    };
    let mut engine = adapter.into_engine();
    engine.l2 = true;
    let mut vae = Vae::open(engine, services);
    let vts = project.value_type_decls();
    let mut genesis = json!({"classes": decls});
    if !vts.is_empty() {
        genesis["value_types"] = json!(vts);
    }
    vae.call("load_genesis", &genesis)?;
    let mut findings = Vec::new();
    let ctx = ubos_dvm::host::Context {
        principal: "did:ubos:acme:01900000-0000-7000-8000-000000000002".into(),
        on_behalf_of: None,
    };
    // the seed is committed file by file, so that a failing change names its file
    let mut files: Vec<String> = Vec::new();
    for item in &project.seed {
        if !files.contains(&item.file) {
            files.push(item.file.clone());
        }
    }
    for file in files {
        let changes: Vec<Value> = project
            .seed
            .iter()
            .filter(|i| i.file == file)
            .map(|i| i.value.clone())
            .collect();
        if let Err(e) = vae.commit(&ctx, json!({"branch": "main", "changes": changes})) {
            findings.push(finding(&file, e.code, "error", e.message, "seed"));
        }
    }
    // logic assets are committed on a feature branch (write policies of `main`)
    if !project.logic.is_empty() {
        vae.call(
            "create_branch",
            &json!({"name": "feature/forge", "from": "main"}),
        )?;
        for item in &project.logic {
            let name = item
                .value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("asset")
                .to_string();
            let oid = asset_object_id(&name);
            let slug = name.replace('.', "-");
            if let Err(e) = vae.commit(
                &ctx,
                json!({"branch": "feature/forge", "changes": [{"op": "create", "class": "Logic",
                "object_id": oid, "slug": slug, "body": item.value}]}),
            ) {
                findings.push(finding(
                    &item.file,
                    e.code,
                    "error",
                    format!("asset {name}: {}", e.message),
                    "scripts",
                ));
            }
        }
    }
    Ok((vae, findings, recorder))
}

/// A stable object identifier for a logic asset of a project (from its name).
pub fn asset_object_id(name: &str) -> String {
    let h = ubos_dvm::content_id("ubos.chunk.v1", &json!(name)).unwrap_or_default();
    let x = &h[7..];
    format!(
        "{}-{}-7{}-8{}-{}",
        &x[0..8],
        &x[8..12],
        &x[13..16],
        &x[17..20],
        &x[20..32]
    )
}
