//! Buks in Forge (PH-2 step 7c, DEC-129): `forge new`, `pack`, `sign` and `verify`, and the
//! dependency resolution into `forge.lock` (DSN-FRG-301…305; FR-PKG-011, -012, -021…023,
//! -061…063; FR-DEV-011). The archive itself is `ubos_dvm::buk`.

use crate::project::{FOLDERS, Project};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use ubos_dvm::BpaError;
use ubos_dvm::buk::{ABI_MAJOR, Archive, Range, Version, abi_major, split_id};

/// Folders packed besides the source folders: shipped migrations (part 7e).
pub const EXTRA_FOLDERS: &[&str] = &["migrations"];

/// The lock file name.
pub const LOCK: &str = "forge.lock";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

/// The files of a project as packed: the parsed content of every YAML and JSON source, and
/// every other file as UTF-8 text (binary files are refused).
pub fn project_files(root: &Path) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    for f in FOLDERS.iter().chain(EXTRA_FOLDERS) {
        let mut paths = Vec::new();
        walk(&root.join(f), &mut paths);
        for p in paths {
            let r = rel(root, &p);
            let bytes = std::fs::read(&p).map_err(|e| format!("{r}: {e}"))?;
            let text = String::from_utf8(bytes)
                .map_err(|_| format!("{r}: binary files are not packed in ubos.buk.v1"))?;
            let v = if [".yaml", ".yml", ".json"].iter().any(|x| r.ends_with(x)) {
                crate::project::parse_source(&r, &text)
                    .map_err(|(l, m)| format!("{r}:{}: {m}", l.unwrap_or(0)))?
            } else {
                json!(text)
            };
            out.insert(r, v);
        }
    }
    Ok(out)
}

/// The capabilities the assets of a project use (instruction families).
pub fn used_capabilities(project: &Project) -> BTreeSet<String> {
    project
        .logic
        .iter()
        .flat_map(|i| crate::records::inferred_capabilities(&i.value))
        .collect()
}

fn declared_capabilities(project: &Project) -> Vec<String> {
    match &project.manifest.capabilities {
        Some(c) => c.clone(),
        None => project
            .logic
            .iter()
            .flat_map(|i| {
                i.value["capabilities"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

/// The manifest of a project's Buk with the locked dependencies.
pub fn manifest(project: &Project, lock: &Lock) -> Value {
    let m = &project.manifest;
    let mut out = json!({
        "name": m.name, "namespace": m.namespace, "version": m.version, "abi": m.abi,
        "dependencies": m.dependencies, "capabilities": declared_capabilities(project),
        "contents": {"classes": project.classes.len(), "value_types": project.value_types.len(),
                     "assets": project.logic.len(), "seed": project.seed.len(), "tests": project.tests.len()},
        "gates": {"script_coverage": format!("{:.2}", m.script_coverage)},
        "dev": {"vae": m.vae, "branch": m.branch},
    });
    if !m.description.is_empty() {
        out["description"] = json!(m.description);
    }
    if !m.licence.is_empty() {
        out["licence"] = json!(m.licence);
    }
    if !lock.entries.is_empty() {
        out["locked"] = lock.to_json();
    }
    out
}

// ---- dependency resolution (DSN-FRG-303) ----

/// One resolved dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct Locked {
    /// Exact version.
    pub version: String,
    /// Root hash of the archive.
    pub root: String,
    /// Its own dependencies (IDs).
    pub requires: Vec<String>,
}

/// The lock: Buk ID → resolved version.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lock {
    /// Entries in ID order.
    pub entries: BTreeMap<String, Locked>,
}

impl Lock {
    /// The lock as JSON (manifest member `locked`).
    pub fn to_json(&self) -> Value {
        Value::Object(
            self.entries
                .iter()
                .map(|(id, l)| {
                    (
                        id.clone(),
                        json!({"version": l.version, "root": l.root, "requires": l.requires}),
                    )
                })
                .collect(),
        )
    }

    /// The text of `forge.lock` (TOML, deterministic).
    pub fn to_text(&self) -> String {
        let mut t =
            String::from("# forge.lock: generated by `forge pack`; do not edit\nversion = 1\n");
        for (id, l) in &self.entries {
            let req: Vec<String> = l.requires.iter().map(|r| format!("\"{r}\"")).collect();
            t.push_str(&format!(
                "\n[[buk]]\nid = \"{id}\"\nversion = \"{}\"\nroot = \"{}\"\nrequires = [{}]\n",
                l.version,
                l.root,
                req.join(", ")
            ));
        }
        t
    }

    /// Read `forge.lock`; an absent file is an empty lock.
    pub fn read(root: &Path) -> Result<Lock, String> {
        let p = root.join(LOCK);
        let Ok(text) = std::fs::read_to_string(&p) else {
            return Ok(Lock::default());
        };
        let t: toml::Value = toml::from_str(&text).map_err(|e| format!("{LOCK}: {e}"))?;
        let mut entries = BTreeMap::new();
        for b in t
            .get("buk")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let g = |k: &str| b.get(k).and_then(toml::Value::as_str).unwrap_or_default();
            entries.insert(
                g("id").to_string(),
                Locked {
                    version: g("version").to_string(),
                    root: g("root").to_string(),
                    requires: b
                        .get("requires")
                        .and_then(toml::Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            );
        }
        Ok(Lock { entries })
    }
}

/// A local repository of Buk archives: ID → archives by version, newest first.
#[derive(Debug, Default)]
pub struct Repository {
    by_id: BTreeMap<String, Vec<(Version, Archive)>>,
}

impl Repository {
    /// The `*.buk` files of a directory (none when it does not exist).
    pub fn open(dir: &Path) -> Result<Repository, String> {
        let mut paths = Vec::new();
        walk(dir, &mut paths);
        paths.sort();
        let mut by_id: BTreeMap<String, Vec<(Version, Archive)>> = BTreeMap::new();
        for p in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "buk"))
        {
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let a = Archive::decode(&bytes).map_err(|e| format!("{}: {e}", p.display()))?;
            let v = Version::parse(a.version()).map_err(err)?;
            let list = by_id.entry(a.id()).or_default();
            if let Some((_, b)) = list.iter().find(|(x, _)| x == &v) {
                if b.root().map_err(err)? != a.root().map_err(err)? {
                    return Err(format!(
                        "{}: {} {v} is in the repository twice with different content",
                        p.display(),
                        a.id()
                    ));
                }
                continue;
            }
            list.push((v, a));
        }
        for list in by_id.values_mut() {
            list.sort_by(|a, b| b.0.cmp(&a.0));
        }
        Ok(Repository { by_id })
    }

    /// The archive of an exact version.
    pub fn get(&self, id: &str, version: &str) -> Option<&Archive> {
        let v = Version::parse(version).ok()?;
        self.by_id
            .get(id)?
            .iter()
            .find(|(x, _)| x == &v)
            .map(|(_, a)| a)
    }
}

/// A requirement with the path that led to it.
#[derive(Debug, Clone)]
struct Req {
    id: String,
    range: Range,
    path: Vec<String>,
}

impl Req {
    fn explain(&self) -> String {
        let mut p = self.path.clone();
        p.push(format!("{} {}", self.id, self.range));
        p.join(" → ")
    }
}

/// Resolve the dependencies of a manifest against a repository: the highest versions that
/// satisfy every range, preferring the versions of `prefer` (the previous lock), by a
/// deterministic backtracking search. A failure names every path to the Buk that cannot be
/// satisfied (FR-PKG-023).
pub fn resolve(
    name: &str,
    deps: &BTreeMap<String, String>,
    repo: &Repository,
    prefer: &Lock,
) -> Result<Lock, String> {
    let mut pending = Vec::new();
    for (id, r) in deps {
        split_id(id).map_err(err)?;
        pending.push(Req {
            id: id.clone(),
            range: Range::parse(r).map_err(err)?,
            path: vec![name.to_string()],
        });
    }
    let mut steps = 0u64;
    let mut chosen: BTreeMap<String, (Version, Vec<Req>)> = BTreeMap::new();
    match search(pending, &mut chosen, repo, prefer, &mut steps) {
        Ok(()) => {
            let mut lock = Lock::default();
            for (id, (v, _)) in &chosen {
                let a = repo
                    .get(id, &v.to_string())
                    .expect("chosen from the repository");
                let requires = a
                    .manifest
                    .get("dependencies")
                    .and_then(Value::as_object)
                    .map(|d| d.keys().cloned().collect())
                    .unwrap_or_default();
                lock.entries.insert(
                    id.clone(),
                    Locked {
                        version: v.to_string(),
                        root: a.root().map_err(err)?,
                        requires,
                    },
                );
            }
            Ok(lock)
        }
        Err(c) => Err(c),
    }
}

const MAX_STEPS: u64 = 100_000;

fn search(
    mut pending: Vec<Req>,
    chosen: &mut BTreeMap<String, (Version, Vec<Req>)>,
    repo: &Repository,
    prefer: &Lock,
    steps: &mut u64,
) -> Result<(), String> {
    *steps += 1;
    if *steps > MAX_STEPS {
        return Err("dependency resolution gave up after 100000 steps".into());
    }
    let Some(req) = (!pending.is_empty()).then(|| pending.remove(0)) else {
        return Ok(());
    };
    if let Some((v, reqs)) = chosen.get_mut(&req.id) {
        if req.range.matches(v) {
            reqs.push(req);
            return search(pending, chosen, repo, prefer, steps);
        }
        let mut all = reqs.clone();
        all.push(req.clone());
        return Err(conflict(&req.id, &all));
    }
    let Some(cands) = repo.by_id.get(&req.id) else {
        return Err(format!(
            "{} is not in the repository (required by {})",
            req.id,
            req.explain()
        ));
    };
    let mut order: Vec<&(Version, Archive)> =
        cands.iter().filter(|(v, _)| req.range.matches(v)).collect();
    if let Some(l) = prefer.entries.get(&req.id)
        && let Ok(pv) = Version::parse(&l.version)
        && let Some(i) = order.iter().position(|(v, _)| v == &pv)
    {
        let x = order.remove(i);
        order.insert(0, x);
    }
    if order.is_empty() {
        let avail: Vec<String> = cands.iter().map(|(v, _)| v.to_string()).collect();
        return Err(format!(
            "no version of {} satisfies {} (available: {})",
            req.id,
            req.explain(),
            avail.join(", ")
        ));
    }
    let mut last = String::new();
    for (v, a) in order {
        if abi_major(a.manifest["abi"].as_str().unwrap_or_default()).ok() != Some(ABI_MAJOR) {
            last = format!("{} {v} needs another ABI", req.id);
            continue;
        }
        let mut next = pending.clone();
        let mut path = req.path.clone();
        path.push(format!("{} {v}", req.id));
        for (d, r) in a
            .manifest
            .get("dependencies")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
        {
            next.push(Req {
                id: d,
                range: Range::parse(r.as_str().unwrap_or_default()).map_err(err)?,
                path: path.clone(),
            });
        }
        let snapshot = chosen.clone();
        chosen.insert(req.id.clone(), (v.clone(), vec![req.clone()]));
        match search(next, chosen, repo, prefer, steps) {
            Ok(()) => return Ok(()),
            Err(e) => {
                *chosen = snapshot;
                last = e;
            }
        }
    }
    Err(last)
}

fn conflict(id: &str, reqs: &[Req]) -> String {
    let mut t = format!("no version of {id} satisfies all of:");
    for r in reqs {
        t.push_str(&format!("\n  {}", r.explain()));
    }
    t
}

// ---- pack (DSN-FRG-301, -302) ----

/// The outcome of `pack`.
#[derive(Debug)]
pub enum Packed {
    /// The archive and the lock.
    Archive(Box<Archive>, Lock),
    /// The errors of `forge check` that stop packing.
    Findings(Vec<Value>),
}

/// Pack a project: `forge check` must pass (it loads the seed against the model, so seed
/// data that the model refuses stops packing), the dependencies are resolved against
/// `repo_dir` into `forge.lock`, and the archive is built.
pub fn pack(root: &Path, repo_dir: &Path) -> Result<Packed, String> {
    let project = Project::load(root)?;
    let errors: Vec<Value> = crate::check::check(&project)
        .iter()
        .filter(|f| f.severity == "error")
        .map(|f| f.to_json())
        .collect();
    if !errors.is_empty() {
        return Ok(Packed::Findings(errors));
    }
    let previous = Lock::read(root)?;
    let lock = if project.manifest.dependencies.is_empty() {
        Lock::default()
    } else {
        let repo = Repository::open(repo_dir)?;
        resolve(
            &format!("{}/{}", project.manifest.namespace, project.manifest.name),
            &project.manifest.dependencies,
            &repo,
            &previous,
        )?
    };
    let archive = Archive::new(manifest(&project, &lock), project_files(root)?).map_err(err)?;
    if lock != previous {
        std::fs::write(root.join(LOCK), lock.to_text()).map_err(err)?;
    }
    Ok(Packed::Archive(Box::new(archive), lock))
}

/// The file name of an archive: `<namespace>.<name>-<version>.buk`.
pub fn file_name(a: &Archive) -> String {
    format!(
        "{}.{}-{}.buk",
        a.manifest["namespace"].as_str().unwrap_or_default(),
        a.manifest["name"].as_str().unwrap_or_default(),
        a.version()
    )
}

// ---- unpack ----

/// Write the archive's files and a `forge.toml` built from its manifest into `dir`, so
/// that the Forge pipeline can run on them. YAML sources are written as JSON (a subset of
/// YAML) under their own paths.
pub fn unpack(a: &Archive, dir: &Path) -> Result<(), String> {
    let m = &a.manifest;
    let q = |v: &Value| serde_json::to_string(v.as_str().unwrap_or_default()).unwrap_or_default();
    let mut toml = format!(
        "[project]\nname = {}\nversion = {}\nvae = {}\nabi = {}\nbranch = {}\nnamespace = {}\n",
        q(&m["name"]),
        q(&m["version"]),
        q(&m["dev"]["vae"]),
        q(&m["abi"]),
        q(&m["dev"]["branch"]),
        q(&m["namespace"])
    );
    let caps: Vec<String> = m["capabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| serde_json::to_string(c).unwrap_or_default())
        .collect();
    toml.push_str(&format!("capabilities = [{}]\n", caps.join(", ")));
    toml.push_str(&format!(
        "\n[gates]\nscript_coverage = {}\n",
        m["gates"]["script_coverage"].as_str().unwrap_or("0.80")
    ));
    if let Some(d) = m["dependencies"].as_object().filter(|d| !d.is_empty()) {
        toml.push_str("\n[dependencies]\n");
        for (k, v) in d {
            toml.push_str(&format!("{} = {}\n", q(&json!(k)), q(v)));
        }
    }
    std::fs::create_dir_all(dir).map_err(err)?;
    std::fs::write(dir.join("forge.toml"), toml).map_err(err)?;
    for (path, h) in &a.tree {
        let content = &a.chunks[h];
        let p = dir.join(path);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(err)?;
        }
        let structured = [".yaml", ".yml", ".json"].iter().any(|x| path.ends_with(x));
        let text = if structured {
            serde_json::to_string_pretty(content).map_err(err)?
        } else {
            content.as_str().unwrap_or_default().to_string()
        };
        std::fs::write(&p, text).map_err(err)?;
    }
    Ok(())
}

/// A fresh scratch directory.
pub fn scratch(tag: &str) -> Result<PathBuf, String> {
    let mut b = [0u8; 8];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut b);
    }
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let d = std::env::temp_dir().join(format!("forge-{tag}-{hex}"));
    std::fs::create_dir_all(&d).map_err(err)?;
    Ok(d)
}

// ---- verify (DSN-FRG-305) ----

/// Options of `forge verify`.
#[derive(Debug, Default)]
pub struct VerifyOptions {
    /// DIDs of trusted publishers; any valid signer when empty.
    pub trust: Vec<String>,
    /// Repository of dependency archives.
    pub repo: Option<PathBuf>,
    /// The previous version of the Buk, for breaking-change checks.
    pub previous: Option<Archive>,
}

fn check_entry(name: &str, findings: Vec<Value>) -> Value {
    json!({"name": name, "result": if findings.is_empty() { "pass" } else { "fail" }, "findings": findings})
}

fn finding(rule: &str, message: impl Into<String>) -> Value {
    json!({"rule": rule, "message": message.into()})
}

/// Verify a Buk: signatures, manifest, namespace ownership, ABI, dependencies, the full
/// checks, the tests, the coverage gate, the capabilities requested against those used,
/// and the shipped migrations of changed classes. The record carries no time, so the same
/// inputs give the same record (FR-PKG-063).
pub fn verify(a: &Archive, o: &VerifyOptions) -> Result<Value, String> {
    let mut checks = Vec::new();
    // signatures
    let mut f = Vec::new();
    let signers = match a.verify_signatures() {
        Ok(s) => s,
        Err(e) => {
            f.push(finding(e.code, e.to_string()));
            Vec::new()
        }
    };
    if a.signatures.is_empty() {
        f.push(finding("PKG.SIGNATURE_INVALID", "the Buk is not signed"));
    }
    if !o.trust.is_empty() && f.is_empty() && !signers.iter().any(|s| o.trust.contains(s)) {
        f.push(finding(
            "PKG.SIGNATURE_INVALID",
            "no signature of a trusted publisher",
        ));
    }
    checks.push(check_entry("signatures", f));
    // manifest and ABI
    let mut f = Vec::new();
    if let Err(e) = ubos_dvm::buk::check_manifest(&a.manifest) {
        f.push(finding(e.code, e.to_string()));
    }
    checks.push(check_entry("manifest", f));
    let mut f = Vec::new();
    let abi = a.manifest["abi"].as_str().unwrap_or_default();
    if abi_major(abi).ok() != Some(ABI_MAJOR) {
        f.push(finding(
            "PKG.MANIFEST_INVALID",
            format!("ABI {abi} is not ABI {ABI_MAJOR}.x"),
        ));
    }
    // unpack and load
    let dir = scratch("verify")?;
    let res = (|| -> Result<Value, String> {
        unpack(a, &dir)?;
        let project = Project::load(&dir)?;
        for item in &project.logic {
            let x = item.value["abi"].as_str().unwrap_or("1.0");
            if abi_major(x).ok() != Some(ABI_MAJOR) {
                f.push(finding(
                    "PKG.MANIFEST_INVALID",
                    format!("{}: asset ABI {x}", item.file),
                ));
            }
        }
        checks.push(check_entry("abi", std::mem::take(&mut f)));
        // namespace ownership
        let ns = a.manifest["namespace"].as_str().unwrap_or_default();
        let owned = |n: &str| n == ns || n.starts_with(&format!("{ns}."));
        let mut f = Vec::new();
        for item in &project.classes {
            let cns = item.value["namespace"].as_str().unwrap_or_default();
            if !owned(cns) {
                f.push(finding(
                    "PKG.NAMESPACE_VIOLATION",
                    format!(
                        "{}: class {} is in namespace {cns:?}, outside {ns}",
                        item.file,
                        item.value["name"].as_str().unwrap_or("?")
                    ),
                ));
            }
        }
        for item in &project.logic {
            let n = item.value["name"].as_str().unwrap_or_default();
            if !owned(n) {
                f.push(finding(
                    "PKG.NAMESPACE_VIOLATION",
                    format!("{}: asset {n} is outside {ns}", item.file),
                ));
            }
        }
        checks.push(check_entry("namespace", f));
        // dependencies
        let mut f = Vec::new();
        let deps: BTreeMap<String, String> = a.manifest["dependencies"]
            .as_object()
            .map(|d| {
                d.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !deps.is_empty() {
            let locked = a.manifest["locked"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for (id, r) in &deps {
                let ok = locked.get(id).is_some_and(|l| {
                    Version::parse(l["version"].as_str().unwrap_or_default())
                        .ok()
                        .zip(Range::parse(r).ok())
                        .is_some_and(|(v, r)| r.matches(&v))
                });
                if !ok {
                    f.push(finding(
                        "PKG.MANIFEST_INVALID",
                        format!("{id} {r} is not locked to a matching version"),
                    ));
                }
            }
            match &o.repo {
                None => f.push(finding(
                    "dependencies_unchecked",
                    "no repository given: the locked archives were not checked",
                )),
                Some(d) => {
                    let repo = Repository::open(d)?;
                    for (id, l) in &locked {
                        let v = l["version"].as_str().unwrap_or_default();
                        match repo.get(id, v) {
                            None => f.push(finding(
                                "PKG.MANIFEST_INVALID",
                                format!("{id} {v} is not in the repository"),
                            )),
                            Some(x) if json!(x.root().map_err(err)?) != l["root"] => {
                                f.push(finding(
                                    "PKG.MANIFEST_INVALID",
                                    format!("{id} {v}: the root differs from the locked one"),
                                ))
                            }
                            Some(_) => {}
                        }
                    }
                }
            }
        }
        checks.push(check_entry("dependencies", f));
        // full checks
        let found = crate::check::check(&project);
        let f: Vec<Value> = found
            .iter()
            .filter(|x| x.severity == "error")
            .map(|x| x.to_json())
            .collect();
        checks.push(check_entry("checks", f));
        // tests and coverage
        let report = crate::test::run(&project, &crate::check::Backend::default())
            .map_err(|e| e.to_string())?;
        let rep = report.to_json(&project);
        let f: Vec<Value> = rep["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["result"] != "pass")
            .map(|r| finding("test_failed", r["name"].as_str().unwrap_or("?")))
            .collect();
        let mut tests = check_entry("tests", f);
        tests["passed"] = rep["passed"].clone();
        tests["failed"] = rep["failed"].clone();
        checks.push(tests);
        let gate = project.manifest.script_coverage;
        let mut f = Vec::new();
        let mut cov = Map::new();
        for (name, c) in rep["coverage"].as_object().cloned().unwrap_or_default() {
            let frac = c["fraction"].as_f64().unwrap_or(0.0);
            cov.insert(name.clone(), json!((frac * 1000.0).round() as u64));
            if frac + 1e-9 < gate {
                f.push(finding(
                    "coverage_gate",
                    format!(
                        "{name}: {:.0}% of its functions tested, gate {:.0}%",
                        frac * 100.0,
                        gate * 100.0
                    ),
                ));
            }
        }
        let mut c = check_entry("coverage", f);
        c["permille"] = Value::Object(cov);
        checks.push(c);
        // capabilities requested against those used
        let requested: BTreeSet<String> = a.manifest["capabilities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        let used = used_capabilities(&project);
        let f: Vec<Value> = used
            .difference(&requested)
            .map(|c| {
                finding(
                    "capability_missing",
                    format!("{c} is used but not requested"),
                )
            })
            .collect();
        let mut c = check_entry("capabilities", f);
        c["used"] = json!(used);
        c["requested"] = json!(requested);
        checks.push(c);
        // migrations for changed classes
        checks.push(check_entry(
            "migrations",
            migrations(&project, o.previous.as_ref())?,
        ));
        Ok(Value::Null)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    res?;
    let passed = checks.iter().all(|c| c["result"] == "pass");
    Ok(json!({
        "buk": a.id(), "version": a.version(), "root": a.root().map_err(err)?,
        "signers": signers, "checks": checks, "result": if passed { "pass" } else { "fail" },
        "verifier": format!("forge {}", env!("CARGO_PKG_VERSION")),
    }))
}

/// Breaking and lens-required class changes against the previous version need a shipped
/// lens (a seeded `Lens` of the class from its previous schema version) or a migration
/// (a file under `migrations/` naming the class), DSN-FRG-307; their execution comes with
/// part 7e.
fn migrations(project: &Project, previous: Option<&Archive>) -> Result<Vec<Value>, String> {
    let Some(prev) = previous else {
        return Ok(Vec::new());
    };
    let dir = scratch("previous")?;
    let old = (|| {
        unpack(prev, &dir)?;
        Project::load(&dir)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let old = old?;
    let lenses: Vec<(String, i64)> = project
        .seed
        .iter()
        .flat_map(|i| {
            let ch = &i.value;
            (ch["class"] == "Lens").then(|| {
                (
                    ch["body"]["class"].as_str().unwrap_or_default().to_string(),
                    ch["body"]["from"].as_i64().unwrap_or(-1),
                )
            })
        })
        .collect();
    let migrated: BTreeSet<String> = project_files(&project.root)?
        .into_iter()
        .filter(|(p, _)| p.starts_with("migrations/"))
        .filter_map(|(_, v)| v["class"].as_str().map(str::to_string))
        .collect();
    let mut f = Vec::new();
    for item in &project.classes {
        let name = item.value["name"].as_str().unwrap_or_default();
        let Some(o) = old.classes.iter().find(|x| x.value["name"] == json!(name)) else {
            continue;
        };
        if o.value == item.value {
            continue;
        }
        let res = ubos_dvm::lenses::classify_change(&json!({"from": o.value, "to": item.value}))
            .map_err(|e| e.to_string())?;
        let class = res["classification"].as_str().unwrap_or_default();
        let from = o.value["schema_version"].as_i64().unwrap_or(1);
        let lensed = lenses
            .iter()
            .any(|(c, f)| (c == name || c.ends_with(&format!("/Class/{name}"))) && *f == from);
        match class {
            "lens_required" if !lensed && !migrated.contains(name) => f.push(finding(
                "MODEL.LENS_REQUIRED",
                format!(
                    "{}: class {name} changes from schema version {from} without a lens",
                    item.file
                ),
            )),
            "breaking" if !migrated.contains(name) => f.push(finding(
                "MODEL.MIGRATION_REQUIRED",
                format!(
                    "{}: class {name} changes incompatibly without a migration",
                    item.file
                ),
            )),
            _ => {}
        }
    }
    Ok(f)
}

/// Sign a verification record with the verifier key (FR-PKG-061): the record's content ID
/// is signed and stored as `record_id`.
pub fn sign_record(record: &mut Value, seed: &[u8; 32]) -> Result<(), BpaError> {
    let key = crate::records::VerifierKey::new(*seed)?;
    record["verified_by"] = json!(key.did);
    let rid = ubos_dvm::content_id("ubos.chunk.v1", record)?;
    let kid = format!("{}#{}", key.did, key.did.trim_start_matches("did:ubos:k:"));
    record["signature"] = ubos_dvm::crypto::sign(seed, &rid, &key.did, &kid, "signer")?;
    record["record_id"] = json!(rid);
    Ok(())
}

// ---- new (FR-DEV-011) ----

/// Scaffold a new Buk project in an empty or absent directory: the `forge init` skeleton
/// with the Buk keys of the manifest (namespace, description, licence, capabilities,
/// dependencies and the coverage gate).
pub fn new_project(dir: &Path, name: &str, namespace: &str) -> Result<Vec<String>, String> {
    if dir.exists()
        && std::fs::read_dir(dir)
            .map(|mut d| d.next().is_some())
            .unwrap_or(true)
    {
        return Err(format!("{} exists and is not empty", dir.display()));
    }
    ubos_dvm::buk::check_manifest(
        &json!({"name": name, "namespace": namespace, "version": "0.1.0", "abi": "1.0"}),
    )
    .map_err(err)?;
    let written = Project::init(dir, name)?;
    if namespace != "acme" {
        for f in &written {
            let p = dir.join(f);
            let t = std::fs::read_to_string(&p).map_err(err)?;
            let t = t
                .replace(
                    "namespace: acme.app",
                    &format!("namespace: {namespace}.app"),
                )
                .replace(
                    "acme.customer.describe",
                    &format!("{namespace}.customer.describe"),
                );
            std::fs::write(&p, t).map_err(err)?;
        }
    }
    let p = dir.join("forge.toml");
    let mut t = std::fs::read_to_string(&p).map_err(err)?;
    t.push_str(&format!(
        "namespace = \"{namespace}\"\ndescription = \"\"\nlicence = \"\"\ncapabilities = [\"read\"]\n\n[gates]\nscript_coverage = 0.80\n\n[dependencies]\n# \"<namespace>/<name>\" = \"^1.0.0\"\n"
    ));
    std::fs::write(&p, t).map_err(err)?;
    Ok(written)
}
