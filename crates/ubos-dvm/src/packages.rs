//! Buk installation in the kernel (PH-2 step 7d, DEC-130; DSN-FRG-306, DSN-DVM-134,
//! DSN-NOD-422; FR-PKG-031…033, -053).
//!
//! * `buk_plan`: the archive is checked (integrity, signatures, trusted publishers) and the
//!   install plan computed against a target branch: classes, value types, assets, seed
//!   data with the subscriptions, schedules and connectors in it, sheets, views,
//!   components, required configuration, the capabilities requested, the dependencies and
//!   the problems that block the installation. Nothing is written.
//! * `buk_install`: the new classes are declared (declarations are shared by the branches of
//!   a VAE), the assets and the seed are committed on the loader branch
//!   `loader/<namespace>/<name>-<version>` (dots as dashes), the assets are verified there by
//!   the node (`verify_asset`), and a change set from the loader branch into the
//!   target is opened and submitted, so that its preview validates the merge. The
//!   registry records the Buk as `pending`.
//! * `buk_complete`: the change set is approved and merged, and the Buk is recorded as a
//!   System-kind `Buk` object on the target together with its registry record, in one
//!   commit.
//! * `buks`: the installation registry.
//!
//! The registry record (namespace, dependencies, change set, loader branch, state) is kept
//! in the kernel metadata under `buks`, written with the commit that records the `Buk`
//! object; the `Buk` class of the `system` genesis Buk holds name, version, root,
//! publisher and signature, and gains the other members with the genesis upgrades of
//! part 7e.

use crate::buk::{ABI_MAJOR, Archive, Range, Version, abi_major};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::{BpaError, canonical, content_id};

/// Kernel metadata key of the installation registry.
pub const REGISTRY: &str = "buks";

thread_local! {
    /// Metadata written with the next commit's flush: `(branch, key, value)`.
    pub(crate) static MARKS: std::cell::RefCell<Vec<(String, String, Value)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Add the pending metadata of this thread to a commit on its branch.
pub(crate) fn marks(tx: &mut ubos_version::state::Tx) {
    MARKS.with(|m| {
        let mut m = m.borrow_mut();
        let (mine, rest): (Vec<_>, Vec<_>) = m.drain(..).partition(|(b, _, _)| b == &tx.branch);
        *m = rest;
        tx.marks.extend(mine.into_iter().map(|(_, k, v)| (k, v)));
    });
}

pub(crate) fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(m: impl Into<String>) -> BpaError {
    BpaError::new("PKG.MANIFEST_INVALID", m.into())
}

/// A stable object identifier from a content ID (version 7 or 8 layout of a UUID).
pub(crate) fn stable_id(v: &Value, version: char) -> Result<String, BpaError> {
    let h = content_id("ubos.chunk.v1", v)?;
    let x = &h[7..];
    Ok(format!(
        "{}-{}-{version}{}-8{}-{}",
        &x[0..8],
        &x[8..12],
        &x[13..16],
        &x[17..20],
        &x[20..32]
    ))
}

/// The object identifier of a logic asset (the one `forge test` uses).
pub fn asset_object_id(name: &str) -> String {
    stable_id(&json!(name), '7').unwrap_or_default()
}

/// The archive of an operation input: the archive document under `archive`.
fn archive_of(i: &Value) -> Result<Archive, BpaError> {
    let doc = i
        .get("archive")
        .filter(|a| a.is_object())
        .ok_or_else(|| invalid("the input carries the archive document"))?;
    Archive::decode(canonical(doc)?.as_bytes())
}

/// Items of a folder of the archive, as Forge loads them: a file holds one item, a list,
/// or a map with the list under `key` (and `value_types` in `model/`).
pub(crate) fn items(a: &Archive, folder: &str, key: &str) -> (Vec<Value>, Vec<Value>) {
    let mut out = Vec::new();
    let mut extra = Vec::new();
    for (path, h) in &a.tree {
        let structured = [".yaml", ".yml", ".json"].iter().any(|x| path.ends_with(x));
        if !path.starts_with(&format!("{folder}/")) || !structured {
            continue;
        }
        match &a.chunks[h] {
            Value::Array(l) => out.extend(l.iter().cloned()),
            Value::Object(o) if o.contains_key(key) => {
                out.extend(o[key].as_array().cloned().unwrap_or_default());
                extra.extend(
                    o.get("value_types")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            other => out.push(other.clone()),
        }
    }
    (out, extra)
}

fn files_in(a: &Archive, folder: &str) -> usize {
    a.tree
        .keys()
        .filter(|p| p.starts_with(&format!("{folder}/")) && !p.ends_with("README.md"))
        .count()
}

fn names(v: &[Value]) -> Vec<String> {
    v.iter()
        .filter_map(|x| s(x, "name").map(str::to_string))
        .collect()
}

/// Kernel metadata key of an installed archive, kept for the upgrades that follow.
pub(crate) fn archive_key(root: &str) -> String {
    format!("buk_archive:{root}")
}

/// Whether an asset as stored (with the state and verification the node recorded)
/// carries the archive's body.
fn same_asset(stored: &Value, shipped: &Value) -> Result<bool, BpaError> {
    let strip = |v: &Value| {
        let mut m = v.as_object().cloned().unwrap_or_default();
        m.remove("state");
        m.remove("verification");
        Value::Object(m)
    };
    Ok(canonical(&strip(stored))? == canonical(&strip(shipped))?)
}

pub(crate) fn problem(code: &str, message: impl Into<String>) -> Value {
    json!({"code": code, "message": message.into()})
}

impl Engine {
    /// The installation registry: Buk ID → record.
    pub(crate) fn registry_records(&mut self) -> Result<Map<String, Value>, BpaError> {
        Ok(self
            .store
            .meta(REGISTRY)?
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default())
    }

    /// Adapter operation `buks` (DSN-DVM-134): the installation registry.
    pub fn buks(&mut self, _: &Value) -> Result<Value, BpaError> {
        let items: Vec<Value> = self.registry_records()?.into_values().collect();
        Ok(json!({ "items": items }))
    }

    /// The Buk IDs whose installed or pending records depend on `id`.
    pub fn buk_dependents(&mut self, id: &str) -> Result<Vec<String>, BpaError> {
        Ok(self
            .registry_records()?
            .into_iter()
            .filter(|(_, r)| r["dependencies"].get(id).is_some())
            .map(|(k, _)| k)
            .collect())
    }

    /// Adapter operation `buk_plan` (DSN-FRG-306, FR-PKG-033): the install plan of an
    /// archive against a target branch; nothing is written.
    pub fn buk_plan(&mut self, i: &Value) -> Result<Value, BpaError> {
        let a = archive_of(i)?;
        self.plan_of(&a, i)
    }

    fn plan_of(&mut self, a: &Archive, i: &Value) -> Result<Value, BpaError> {
        let target = s(i, "branch").unwrap_or("main").to_string();
        let m = &a.manifest;
        let id = a.id();
        let root = a.root()?;
        let ns = s(m, "namespace").unwrap_or_default().to_string();
        let owned = |n: &str| n == ns || n.starts_with(&format!("{ns}."));
        let mut problems = Vec::new();
        // signatures and trusted publishers (DSN-NOD-422)
        let signers = a.verify_signatures()?;
        if signers.is_empty() {
            return Err(BpaError::new(
                "PKG.SIGNATURE_INVALID",
                "the Buk is not signed",
            ));
        }
        let trust: Vec<String> = i["trust"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
        if !trust.is_empty() && !signers.iter().any(|x| trust.contains(x)) {
            return Err(BpaError::with_details(
                "PKG.SIGNATURE_INVALID",
                json!({"reason": "no signature of a trusted publisher", "signers": signers}),
            ));
        }
        if abi_major(s(m, "abi").unwrap_or_default()).ok() != Some(ABI_MAJOR) {
            problems.push(problem(
                "PKG.MANIFEST_INVALID",
                format!("ABI {} is not ABI {ABI_MAJOR}.x", m["abi"]),
            ));
        }
        self.other("branch_info", &json!({"branch": target}))?;
        // the registry: the same content, another version, or a namespace held by another Buk
        let reg = self.registry_records()?;
        let mut status = "new";
        let mut from: Option<Value> = None;
        if let Some(r) = reg.get(&id) {
            if r["root"] == json!(root) {
                status = if r["state"] == "installed" {
                    "present"
                } else {
                    "pending"
                };
            } else if r["pending"]["root"] == json!(root) {
                status = "pending";
            } else if r["state"] != "installed" || r["pending"].is_object() {
                problems.push(problem(
                    "PKG.MANIFEST_INVALID",
                    format!("another installation of {id} is pending"),
                ));
            } else {
                // an upgrade (DEC-131): a later version than the installed one
                status = "upgrade";
                from = Some(json!({"version": r["version"], "root": r["root"]}));
                let newer = Version::parse(a.version())
                    .ok()
                    .zip(Version::parse(r["version"].as_str().unwrap_or_default()).ok())
                    .is_some_and(|(n, o)| n > o);
                if !newer {
                    problems.push(problem(
                        "PKG.MANIFEST_INVALID",
                        format!(
                            "{id} {} is installed; a downgrade is a rollback of the upgrade",
                            r["version"].as_str().unwrap_or_default()
                        ),
                    ));
                }
                // the installed Buks that depend on this one accept the new version
                for (other, o) in &reg {
                    if let Some(range) = o["dependencies"].get(&id).and_then(Value::as_str) {
                        let ok = Version::parse(a.version())
                            .ok()
                            .zip(Range::parse(range).ok())
                            .is_some_and(|(v, r)| r.matches(&v));
                        if !ok {
                            problems.push(problem(
                                "PKG.CONFIGURATION_INCOMPLETE",
                                format!("{other} requires {id} {range}"),
                            ));
                        }
                    }
                }
            }
        }
        let upgrading = status == "upgrade";
        let previous = match &from {
            Some(f) => self
                .store
                .meta(&archive_key(f["root"].as_str().unwrap_or_default()))?,
            None => None,
        }
        .and_then(|d| Archive::decode(canonical(&d).ok()?.as_bytes()).ok());
        for (other, r) in &reg {
            let ons = r["namespace"].as_str().unwrap_or_default();
            // a namespace is shared only by Buks of the same publisher
            let overlaps = ons == ns || owned(ons) || ns.starts_with(&format!("{ons}."));
            if other != &id && overlaps && !signers.iter().any(|x| r["publisher"] == json!(x)) {
                problems.push(problem(
                    "PKG.NAMESPACE_VIOLATION",
                    format!(
                        "namespace {ns} overlaps {ons} of the Buk {other} of another publisher"
                    ),
                ));
            }
        }
        // classes and value types
        let (classes, value_types) = items(a, "model", "classes");
        let (mut add, mut unchanged, mut changed) = (Vec::new(), Vec::new(), Vec::new());
        let (seed_all, _) = items(a, "seed", "changes");
        let (migrations, _) = items(a, "migrations", "migrations");
        for c in &classes {
            let name = s(c, "name").unwrap_or_default().to_string();
            let cns = s(c, "namespace").unwrap_or_default();
            if !owned(cns) {
                problems.push(problem(
                    "PKG.NAMESPACE_VIOLATION",
                    format!("class {name} is in namespace {cns:?}, outside {ns}"),
                ));
            }
            match self.registry.decl(&name) {
                Err(_) => add.push(name),
                Ok(d) if d["namespace"] != json!(cns) => problems.push(problem(
                    "PKG.NAMESPACE_VIOLATION",
                    format!("class {name} exists in namespace {}", d["namespace"]),
                )),
                Ok(d) if canonical(d)? == canonical(c)? => unchanged.push(name),
                Ok(_) if !upgrading => problems.push(problem(
                    "PKG.MANIFEST_INVALID",
                    format!("class {name} exists with another declaration"),
                )),
                Ok(d) => {
                    let d = d.clone();
                    match self.class_change(&target, &name, &d, c, &seed_all, &migrations)? {
                        Ok(ch) => changed.push(ch),
                        Err(p) => problems.push(p),
                    }
                }
            }
        }
        // logic assets
        let (assets, _) = items(a, "logic", "assets");
        let (mut a_add, mut a_same, mut a_change) = (Vec::new(), Vec::new(), Vec::new());
        for x in &assets {
            let name = s(x, "name").unwrap_or_default().to_string();
            if !owned(&name) {
                problems.push(problem(
                    "PKG.NAMESPACE_VIOLATION",
                    format!("asset {name} is outside {ns}"),
                ));
            }
            match self.read(&json!({"branch": target, "object_id": asset_object_id(&name)})) {
                Err(_) => a_add.push(name),
                Ok(r) if same_asset(&r["body"], x)? => a_same.push(name),
                Ok(_) if upgrading => a_change.push(name),
                Ok(_) => problems.push(problem(
                    "PKG.MANIFEST_INVALID",
                    format!("asset {name} exists with another body"),
                )),
            }
        }
        // seed data
        let (seed, _) = items(a, "seed", "changes");
        let mut by_class: BTreeMap<String, u64> = BTreeMap::new();
        for ch in &seed {
            *by_class
                .entry(s(ch, "class").unwrap_or("?").to_string())
                .or_default() += 1;
        }
        let count = |k: &str| by_class.get(k).copied().unwrap_or(0);
        let seed_plan = if upgrading {
            self.seed_upgrade(&target, &seed, previous.as_ref())?
        } else {
            json!({"create": seed.len()})
        };
        let removed: Vec<String> = previous
            .as_ref()
            .map(|p| {
                let now: BTreeSet<String> = names(&assets).into_iter().collect();
                names(&items(p, "logic", "assets").0)
                    .into_iter()
                    .filter(|n| !now.contains(n))
                    .collect()
            })
            .unwrap_or_default();
        let overlays = if upgrading {
            let mut touched: BTreeSet<String> =
                a_change.iter().map(|n| asset_object_id(n)).collect();
            touched.extend(
                seed_plan["update"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x.as_str().map(str::to_string)),
            );
            self.overlay_report(&target, &touched)?
        } else {
            json!([])
        };
        // dependencies (installed, with a matching version)
        let mut deps = Vec::new();
        for (d, r) in m["dependencies"].as_object().into_iter().flatten() {
            let r = r.as_str().unwrap_or_default();
            let installed = reg
                .get(d)
                .filter(|x| x["state"] == "installed")
                .and_then(|x| x["version"].as_str())
                .map(str::to_string);
            let ok = installed.as_ref().is_some_and(|v| {
                Version::parse(v)
                    .ok()
                    .zip(Range::parse(r).ok())
                    .is_some_and(|(v, r)| r.matches(&v))
            });
            if !ok {
                problems.push(problem(
                    "PKG.CONFIGURATION_INCOMPLETE",
                    match &installed {
                        Some(v) => format!("dependency {d} {r}: {v} is installed"),
                        None => format!("dependency {d} {r} is not installed"),
                    },
                ));
            }
            deps.push(json!({"id": d, "range": r, "installed": installed, "satisfied": ok}));
        }
        // required configuration (manifest member `configuration`: keys to be set)
        let configuration = m.get("configuration").cloned().unwrap_or(json!([]));
        let plan = json!({
            "buk": id, "version": a.version(), "root": root, "namespace": ns, "signers": signers,
            "target": target, "status": status,
            "loader": format!("loader/{}/{}-{}", ns.replace('.', "-"), s(m, "name").unwrap_or_default().replace('_', "-"),
                a.version().replace('.', "-").to_lowercase()),
            "from": from,
            "classes": {"add": add, "unchanged": unchanged, "change": changed},
            "value_types": {"add": names(&value_types)},
            "assets": {"add": a_add, "unchanged": a_same, "change": a_change, "kept": removed},
            "seed": {"changes": seed.len(), "by_class": by_class, "plan": seed_plan},
            "overlays": overlays,
            "subscriptions": count("Subscription"), "schedules": count("Schedule"),
            "connectors": by_class.iter().filter(|(k, _)| k.contains("Connector")).map(|(_, n)| n).sum::<u64>(),
            "sheets": files_in(a, "sheets"), "views": files_in(a, "views"), "components": files_in(a, "components"),
            "configuration": configuration, "permissions": m.get("capabilities").cloned().unwrap_or(json!([])),
            "dependencies": deps, "problems": problems,
        });
        Ok(plan)
    }

    /// Adapter operation `buk_install` (FR-PKG-031): declare the new classes, stage the
    /// assets and the seed on the loader branch, and open and submit the change set into
    /// the target. A Buk whose installation is pending returns its change set again.
    pub fn buk_install(&mut self, i: &Value) -> Result<Value, BpaError> {
        let a = archive_of(i)?;
        let plan = self.plan_of(&a, i)?;
        if let Some(p) = plan["problems"].as_array().and_then(|p| p.first()) {
            let code: &'static str = match s(p, "code") {
                Some("PKG.NAMESPACE_VIOLATION") => "PKG.NAMESPACE_VIOLATION",
                Some("PKG.CONFIGURATION_INCOMPLETE") => "PKG.CONFIGURATION_INCOMPLETE",
                Some("MODEL.LENS_REQUIRED") => "MODEL.LENS_REQUIRED",
                Some("MODEL.MIGRATION_REQUIRED") => "MODEL.MIGRATION_REQUIRED",
                _ => "PKG.MANIFEST_INVALID",
            };
            return Err(BpaError::with_details(code, plan));
        }
        let id = a.id();
        if plan["status"] == "upgrade" {
            return self.stage_upgrade(&a, plan);
        }
        if plan["status"] != "new" {
            let rec = self
                .registry_records()?
                .get(&id)
                .cloned()
                .unwrap_or_default();
            return Ok(
                json!({"plan": plan, "change_set": rec["change_set"], "state": rec["state"]}),
            );
        }
        let target = plan["target"].as_str().unwrap_or_default().to_string();
        // a loader branch of its own per attempt: one left by a failed attempt is kept
        let base = plan["loader"].as_str().unwrap_or_default().to_string();
        let mut loader = base.clone();
        let mut n = 1;
        while self
            .other("branch_info", &json!({ "branch": loader }))
            .is_ok()
        {
            n += 1;
            loader = format!("{base}-{n}");
        }
        let root = a.root()?;
        let label = format!("{id} {}", a.version());
        // 1. the loader branch, then the new classes and value types (shared by the
        // branches of the VAE; a retry finds them unchanged)
        self.other("create_branch", &json!({"name": loader, "from": target}))?;
        let (classes, value_types) = items(&a, "model", "classes");
        let added: BTreeSet<String> = plan["classes"]["add"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
        let new: Vec<Value> = classes
            .into_iter()
            .filter(|c| s(c, "name").is_some_and(|n| added.contains(n)))
            .collect();
        let vts: Vec<Value> = value_types
            .into_iter()
            .filter(|v| s(v, "name").is_some_and(|n| self.registry.value_types().get(n).is_none()))
            .collect();
        if !new.is_empty() || !vts.is_empty() {
            self.load_genesis(&json!(new), (!vts.is_empty()).then(|| json!(vts)).as_ref())?;
        }
        // 2. the assets and the seed on the loader branch
        let process = json!({"kind": "import", "message": format!("install {label}")});
        let (assets, _) = items(&a, "logic", "assets");
        let mut changes: Vec<Value> = assets
            .iter()
            .filter(|x| {
                plan["assets"]["add"]
                    .as_array()
                    .is_some_and(|l| l.contains(&x["name"]))
            })
            .map(|x| {
                let name = s(x, "name").unwrap_or_default();
                json!({"op": "create", "class": "Logic", "object_id": asset_object_id(name),
                       "slug": name.replace('.', "-"), "body": x})
            })
            .collect();
        changes.extend(items(&a, "seed", "changes").0);
        if !changes.is_empty() {
            self.commit(&json!({"branch": loader, "changes": changes, "process": process}))?;
        }
        // the assets are verified on the loader branch by the node (STD-CTX-023), so that
        // they arrive verified and run on protected branches
        let mut verified = Vec::new();
        for name in plan["assets"]["add"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let name = name.as_str().unwrap_or_default();
            let r = self.verify_asset(&loader, &asset_object_id(name), None)?;
            if r["verified"] != json!(true) {
                return Err(BpaError::with_details(
                    "LOGIC.UNVERIFIED_ASSET",
                    json!({"asset": name, "loader": loader, "record": r["record"]}),
                ));
            }
            verified.push(json!({"asset": name, "record_id": r["record_id"]}));
        }
        // 3. the change set into the target, submitted for its preview
        let cs = stable_id(
            &json!({"buk_install": root, "target": target, "loader": loader}),
            '8',
        )?;
        self.change_set(
            &json!({"action": "open", "id": cs, "source": loader, "target": target,
            "title": format!("Install {label}"),
            "description": format!("Buk {label}, root {root}")}),
        )?;
        let submitted = self.change_set(&json!({"action": "submit", "id": cs}))?;
        // 4. the registry record, pending
        let m = &a.manifest;
        let sig = &a.signatures[0];
        let rec = json!({"id": id, "name": m["name"], "namespace": m["namespace"], "version": a.version(),
            "root": root, "publisher": sig["signer"], "signature": sig["sig"],
            "dependencies": m.get("dependencies").cloned().unwrap_or(json!({})),
            "capabilities": m.get("capabilities").cloned().unwrap_or(json!([])),
            "target": target, "loader": loader, "change_set": cs, "state": "pending",
            "object_id": stable_id(&json!({"buk_record": id}), '8')?});
        let mut reg = self.registry_records()?;
        reg.insert(id, rec);
        self.store.put_meta(REGISTRY, &Value::Object(reg))?;
        self.store.put_meta(&archive_key(&root), &a.to_json()?)?;
        Ok(
            json!({"plan": plan, "change_set": cs, "state": submitted["state"], "loader": loader,
                  "verified_assets": verified}),
        )
    }

    /// Adapter operation `buk_complete` (FR-PKG-031): approve and merge the install change
    /// set, and record the Buk on the target with its registry record in one commit.
    pub fn buk_complete(&mut self, i: &Value) -> Result<Value, BpaError> {
        let id = s(i, "buk")
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "buk names the Buk ID"))?
            .to_string();
        let mut rec = self.registry_records()?.get(&id).cloned().ok_or_else(|| {
            BpaError::new("IAM.NOT_FOUND", format!("{id} is not being installed"))
        })?;
        if rec["pending"].is_object() {
            return self.complete_upgrade(&id, rec, i);
        }
        if rec["state"] == "installed" {
            return Ok(json!({"buk": id, "state": "installed", "change_set": rec["change_set"]}));
        }
        let cs = rec["change_set"].as_str().unwrap_or_default().to_string();
        let mut state = self.change_set(&json!({"action": "get", "id": cs}))?["state"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if state == "submitted" {
            let approver = i.get("approver").filter(|a| !a.is_null()).ok_or_else(|| {
                BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "an approval names its approver",
                )
            })?;
            state = self
                .change_set(&json!({"action": "approve", "id": cs, "approver": approver}))?["state"]
                .as_str()
                .unwrap_or_default()
                .to_string();
        }
        let mut merged = Value::Null;
        if state == "approved" {
            let r = self.change_set(&json!({"action": "merge", "id": cs}))?;
            merged = r.get("merged_commit").cloned().unwrap_or(Value::Null);
            state = r["state"].as_str().unwrap_or_default().to_string();
        }
        if state != "merged" {
            return Err(BpaError::with_details(
                "FLOW.TRANSITION_NOT_ALLOWED",
                json!({"change_set": cs, "state": state}),
            ));
        }
        let target = rec["target"].as_str().unwrap_or("main").to_string();
        rec["state"] = json!("installed");
        rec["merged_commit"] = merged;
        let mut reg = self.registry_records()?;
        reg.insert(id.clone(), rec.clone());
        MARKS.with(|m| {
            m.borrow_mut()
                .push((target.clone(), REGISTRY.to_string(), Value::Object(reg)))
        });
        let r = self.system_commit(&json!({"branch": target, "changes": [{"op": "create", "class": "Buk",
            "object_id": rec["object_id"], "slug": format!("buk-{}", id.replace('/', "-")),
            "body": {"name": id, "version": rec["version"], "content_hash": rec["root"],
                     "publisher": rec["publisher"], "signature": rec["signature"]}}],
            "process": {"kind": "import", "message": format!("installed Buk {id} {}", rec["version"].as_str().unwrap_or_default())}}));
        MARKS.with(|m| m.borrow_mut().clear());
        r?;
        Ok(
            json!({"buk": id, "state": "installed", "change_set": cs, "object_id": rec["object_id"]}),
        )
    }
}
