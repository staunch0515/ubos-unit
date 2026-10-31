//! Buk upgrades, shipped migrations and rollback (PH-2 step 7e-1, DEC-131; FR-PKG-041, -042,
//! -044; FR-TEN-032; DSN-NOD-407, DSN-DVM-123, DSN-FRG-307).
//!
//! An upgrade installs a later version of an installed Buk through the same plan, loader
//! branch and change set as an installation (`packages`), with these differences:
//!
//! * **Classes.** A changed class goes one schema version up by `change_class`:
//!   - an additive change needs nothing;
//!   - a lens-required change needs a `Lens` the new version seeds (from the current
//!     schema version);
//!   - a breaking change needs a migration, a one-way lens under `migrations/`.
//!
//!   Additive and lensed changes are applied when the upgrade is staged: declarations are
//!   shared by the branches of a VAE, and existing data reads through the lens. A breaking
//!   change is applied when the upgrade completes, after the merge. Its instances on the
//!   target and on the target's overlays are then rewritten by a bulk process (one commit per
//!   batch, resumable, DSN-DVM-560), and the seed changes of that class follow.
//! * **Assets.** Changed assets are updated on the loader branch and verified there.
//! * **Seed data.** New objects are created. An object whose current body is still the
//!   previous version's seed body is updated. An object changed locally is kept and
//!   reported.
//! * **Overlays.** The branches whose parent line is the target receive the upgrade through
//!   inheritance. The plan lists, per overlay, the overridden objects that the upgrade
//!   changes; completion reports each overlay's upstream changes (STD-VER-082), which the
//!   overlay resolves by acknowledgement or three-way merge.
//! * **Rollback.** `buk_rollback` reverts the upgrade's merge (STD-VER-071) and restores the
//!   previous registry record. Class changes and rewritten instances stay: data written
//!   under the new schema reads through the lenses (FR-PKG-044).

use crate::buk::Archive;
use crate::packages::{
    MARKS, REGISTRY, archive_key, asset_object_id, items, problem, s, stable_id,
};
use crate::stateful::Engine;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ubos_types::{BpaError, canonical};
use ubos_version::state::Repo;

/// The seed changes of an upgrade: the lenses, those staged on the loader branch, and those
/// of classes changed by migration.
type SeedSplit = (Vec<Value>, Vec<Value>, Vec<Value>);

/// Objects rewritten per commit of a migration (DSN-DVM-560).
pub const MIGRATION_BATCH: u64 = 1_000;

fn lens_of(seed: &[Value], name: &str, from: i64) -> Option<Value> {
    seed.iter()
        .filter(|ch| s(ch, "class") == Some("Lens"))
        .find(|ch| {
            let c = s(&ch["body"], "class").unwrap_or_default();
            (c == name || c.ends_with(&format!("/Class/{name}")))
                && ch["body"]["from"].as_i64() == Some(from)
        })
        .cloned()
}

fn migration_of(migrations: &[Value], name: &str, from: i64) -> Option<Value> {
    migrations
        .iter()
        .find(|m| {
            let c = s(m, "class").unwrap_or_default();
            (c == name || c.ends_with(&format!("/Class/{name}")))
                && m["from"].as_i64() == Some(from)
        })
        .cloned()
}

impl Engine {
    /// The plan entry of a changed class (`Ok`), or the problem that blocks it (`Err`).
    pub(crate) fn class_change(
        &mut self,
        target: &str,
        name: &str,
        old: &Value,
        new: &Value,
        seed: &[Value],
        migrations: &[Value],
    ) -> Result<Result<Value, Value>, BpaError> {
        let cur = self.registry.class(name)?.schema_version;
        if new["schema_version"].as_i64() != Some(cur + 1) {
            return Ok(Err(problem(
                "PKG.MANIFEST_INVALID",
                format!(
                    "class {name} changes from schema version {cur}; an upgrade takes it to {} (one step)",
                    cur + 1
                ),
            )));
        }
        let lens = lens_of(seed, name, cur);
        let migration = migration_of(migrations, name, cur);
        let stored = self.stored_bodies(target, name)?;
        let with = lens
            .as_ref()
            .map(|l| l["body"].clone())
            .or_else(|| migration.clone());
        let res = crate::lenses::classify_change(
            &json!({"from": old, "to": new, "stored": stored, "lens": with}),
        )?;
        let class = res["classification"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let total = res["totality"]["total"] == json!(true);
        let via = match class.as_str() {
            "lens_required" | "breaking" if migration.is_some() && total && lens.is_none() => {
                "migration"
            }
            "lens_required" if lens.is_some() && total => "lens",
            "lens_required" => {
                return Ok(Err(problem(
                    "MODEL.LENS_REQUIRED",
                    format!("class {name} needs a lens from schema version {cur}"),
                )));
            }
            "breaking" => {
                return Ok(Err(problem(
                    "MODEL.MIGRATION_REQUIRED",
                    format!(
                        "class {name} changes incompatibly and needs a migration from schema version {cur}"
                    ),
                )));
            }
            _ => "none",
        };
        Ok(Ok(
            json!({"class": name, "from": cur, "to": cur + 1, "classification": class, "via": via,
            "lens": lens.and_then(|l| l.get("object_id").cloned()),
            "migration": if via == "migration" { migration.unwrap_or(Value::Null) } else { Value::Null },
            "declaration": new}),
        ))
    }

    /// The seed of an upgrade against the target: object IDs to create, to update (unchanged
    /// since the previous version's seed), kept (changed locally) and unchanged.
    pub(crate) fn seed_upgrade(
        &mut self,
        target: &str,
        seed: &[Value],
        previous: Option<&Archive>,
    ) -> Result<Value, BpaError> {
        let prev: Vec<Value> = previous
            .map(|p| items(p, "seed", "changes").0)
            .unwrap_or_default();
        let (mut create, mut update, mut kept, mut same, mut skipped) =
            (Vec::new(), Vec::new(), Vec::new(), 0, 0);
        for ch in seed {
            let Some(oid) = s(ch, "object_id") else {
                skipped += 1;
                continue;
            };
            let Ok(cur) = self.read(&json!({"branch": target, "object_id": oid})) else {
                create.push(json!(oid));
                continue;
            };
            if canonical(&cur["body"])? == canonical(&ch["body"])? {
                same += 1;
                continue;
            }
            let was = prev
                .iter()
                .find(|p| s(p, "object_id") == Some(oid))
                .map(|p| p["body"].clone());
            match was {
                Some(b) if canonical(&b)? == canonical(&cur["body"])? => update.push(json!(oid)),
                _ => kept.push(json!(oid)),
            }
        }
        Ok(
            json!({"create": create, "update": update, "kept": kept, "unchanged": same,
                  "skipped": skipped}),
        )
    }

    /// The overlays of a target (the branches whose parent line it is), each with the
    /// objects it overrides that the upgrade changes.
    pub(crate) fn overlay_report(
        &mut self,
        target: &str,
        touched: &BTreeSet<String>,
    ) -> Result<Value, BpaError> {
        let mut out = Vec::new();
        for b in self.store.branch_names()? {
            let pl = Repo::new(self.store.as_mut(), &self.registry)
                .branch_meta(&b)?
                .parent_line;
            let Some(pl) = pl.filter(|p| p.branch == target) else {
                continue;
            };
            let overridden: Vec<String> = self
                .store
                .meta(&format!("overrides:{b}"))?
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default()
                .keys()
                .filter(|k| touched.contains(*k))
                .cloned()
                .collect();
            out.push(
                json!({"branch": b, "mode": pl.mode, "pin": pl.pin, "overridden": overridden}),
            );
        }
        Ok(Value::Array(out))
    }

    /// The seed changes of an upgrade, split into those staged on the loader branch and
    /// those of classes that change by migration (committed after the migration).
    fn seed_changes(
        &mut self,
        target: &str,
        a: &Archive,
        plan: &Value,
        deferred: &BTreeSet<String>,
    ) -> Result<SeedSplit, BpaError> {
        let (seed, _) = items(a, "seed", "changes");
        let in_list = |k: &str, oid: &str| {
            plan["seed"]["plan"][k]
                .as_array()
                .is_some_and(|l| l.contains(&json!(oid)))
        };
        let (mut lenses, mut staged, mut later) = (Vec::new(), Vec::new(), Vec::new());
        for ch in seed {
            let oid = s(&ch, "object_id").unwrap_or_default().to_string();
            let change = if in_list("create", &oid) {
                ch.clone()
            } else if in_list("update", &oid) {
                let (o, _, _, _) = self.object_view(target, &json!({}), &oid)?;
                json!({"op": "update", "object_id": oid, "base": o.head, "body": ch["body"]})
            } else {
                continue;
            };
            let class = match s(&ch, "class") {
                Some(c) => c.to_string(),
                None => self.object_view(target, &json!({}), &oid)?.0.class,
            };
            if class == "Lens" {
                lenses.push(change);
            } else if deferred.contains(&class) {
                later.push(change);
            } else {
                staged.push(change);
            }
        }
        Ok((lenses, staged, later))
    }

    /// Stage an upgrade (`buk_install` of a later version): the loader branch, the new
    /// classes, the lenses and the additive and lensed class changes, the changed assets
    /// (verified), the seed, and the change set into the target, submitted.
    pub(crate) fn stage_upgrade(&mut self, a: &Archive, plan: Value) -> Result<Value, BpaError> {
        let id = a.id();
        let target = plan["target"].as_str().unwrap_or_default().to_string();
        let root = a.root()?;
        let label = format!("{id} {}", a.version());
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
        self.other("create_branch", &json!({"name": loader, "from": target}))?;
        let process = json!({"kind": "import", "message": format!("upgrade to {label}")});
        // new classes and value types
        let (classes, value_types) = items(a, "model", "classes");
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
        let changes: Vec<Value> = plan["classes"]["change"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let deferred: BTreeSet<String> = changes
            .iter()
            .filter(|c| c["via"] == "migration")
            .filter_map(|c| s(c, "class").map(str::to_string))
            .collect();
        let (lenses, mut staged, later) = self.seed_changes(&target, a, &plan, &deferred)?;
        // the lenses first, then the additive and lensed class changes
        if !lenses.is_empty() {
            self.commit(&json!({"branch": loader, "changes": lenses, "process": process}))?;
        }
        for c in changes.iter().filter(|c| c["via"] != "migration") {
            let mut input = json!({"class": c["declaration"], "branch": loader});
            if c["via"] == "lens" {
                input["lens"] = c["lens"].clone();
            }
            self.change_class(&input)?;
        }
        // the assets and the seed on the loader branch
        let (assets, _) = items(a, "logic", "assets");
        let listed =
            |k: &str, n: &Value| plan["assets"][k].as_array().is_some_and(|l| l.contains(n));
        let mut asset_changes = Vec::new();
        for x in &assets {
            let name = s(x, "name").unwrap_or_default();
            let oid = asset_object_id(name);
            if listed("add", &x["name"]) {
                asset_changes.push(json!({"op": "create", "class": "Logic", "object_id": oid,
                    "slug": name.replace('.', "-"), "body": x}));
            } else if listed("change", &x["name"]) {
                let (o, _, _, _) = self.object_view(&loader, &json!({}), &oid)?;
                let mut body = x.clone();
                body["state"] = json!("committed");
                body["verification"] = Value::Null;
                asset_changes
                    .push(json!({"op": "update", "object_id": oid, "base": o.head, "body": body}));
            }
        }
        asset_changes.append(&mut staged);
        if !asset_changes.is_empty() {
            self.commit(&json!({"branch": loader, "changes": asset_changes, "process": process}))?;
        }
        let mut verified = Vec::new();
        for k in ["add", "change"] {
            for name in plan["assets"][k].as_array().cloned().unwrap_or_default() {
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
        }
        // the change set into the target
        let cs = stable_id(
            &json!({"buk_upgrade": root, "target": target, "loader": loader}),
            '8',
        )?;
        self.change_set(&json!({"action": "open", "id": cs, "source": loader, "target": target,
            "title": format!("Upgrade to {label}"),
            "description": format!("Buk {label}, root {root}, from {}", plan["from"]["version"].as_str().unwrap_or_default())}))?;
        let submitted = self.change_set(&json!({"action": "submit", "id": cs}))?;
        // the registry: the upgrade is pending on the installed record
        let m = &a.manifest;
        let sig = &a.signatures[0];
        let mut reg = self.registry_records()?;
        let mut rec = reg.get(&id).cloned().unwrap_or_default();
        rec["pending"] = json!({"version": a.version(), "root": root, "publisher": sig["signer"],
            "signature": sig["sig"], "namespace": m["namespace"], "name": m["name"],
            "dependencies": m.get("dependencies").cloned().unwrap_or(json!({})),
            "capabilities": m.get("capabilities").cloned().unwrap_or(json!([])),
            "loader": loader, "change_set": cs,
            "migrations": changes.iter().filter(|c| c["via"] == "migration").cloned().collect::<Vec<_>>(),
            "later": later});
        reg.insert(id, rec);
        self.store.put_meta(REGISTRY, &Value::Object(reg))?;
        self.store.put_meta(&archive_key(&root), &a.to_json()?)?;
        Ok(
            json!({"plan": plan, "change_set": cs, "state": submitted["state"], "loader": loader,
                  "verified_assets": verified}),
        )
    }

    /// Complete an upgrade: approve and merge its change set, apply the class changes by
    /// migration and rewrite their instances on the target and its overlays, commit the
    /// seed of those classes, and record the new version with the previous one kept for a
    /// rollback; the overlays' upstream changes are reported.
    pub(crate) fn complete_upgrade(
        &mut self,
        id: &str,
        rec: Value,
        i: &Value,
    ) -> Result<Value, BpaError> {
        let p = rec["pending"].clone();
        let cs = p["change_set"].as_str().unwrap_or_default().to_string();
        let target = rec["target"].as_str().unwrap_or("main").to_string();
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
        // class changes by migration, after the merge (DSN-DVM-123); a retry skips what is done
        let mut overlays: Vec<String> = Vec::new();
        for b in self.store.branch_names()? {
            let pl = Repo::new(self.store.as_mut(), &self.registry)
                .branch_meta(&b)?
                .parent_line;
            if pl.is_some_and(|x| x.branch == target) {
                overlays.push(b);
            }
        }
        let mut migrated = Vec::new();
        for c in p["migrations"].as_array().cloned().unwrap_or_default() {
            let name = s(&c, "class").unwrap_or_default().to_string();
            if self.registry.class(&name)?.schema_version < c["to"].as_i64().unwrap_or(0) {
                self.change_class(&json!({"class": c["declaration"], "branch": target,
                    "migration": c["migration"]}))?;
            }
            for b in std::iter::once(target.clone()).chain(overlays.iter().cloned()) {
                let r = self.migrate_instances(&b, &name, &cs)?;
                migrated.push(json!({"class": name, "branch": b, "objects": r["objects"], "state": r["state"]}));
            }
        }
        // the seed of those classes, on the heads the migration left
        let mut later = p["later"].as_array().cloned().unwrap_or_default();
        for ch in later.iter_mut().filter(|c| c["op"] == "update") {
            let oid = s(ch, "object_id").unwrap_or_default().to_string();
            ch["base"] = json!(self.object_view(&target, &json!({}), &oid)?.0.head);
        }
        if !later.is_empty() {
            self.commit(&json!({"branch": target, "changes": later,
                "process": {"kind": "import", "message": format!("upgrade to {id} {}: seed of migrated classes",
                    p["version"].as_str().unwrap_or_default())}}))?;
        }
        // the record: the new version, with the previous one for a rollback
        let mut previous = rec.clone();
        previous.as_object_mut().map(|m| {
            m.remove("pending");
            m.remove("previous")
        });
        let mut next = rec.clone();
        for k in [
            "version",
            "root",
            "publisher",
            "signature",
            "namespace",
            "name",
            "dependencies",
            "capabilities",
        ] {
            next[k] = p[k].clone();
        }
        next.as_object_mut().map(|m| m.remove("pending"));
        next["state"] = json!("installed");
        next["previous"] = previous;
        next["upgrade"] = json!({"change_set": cs, "loader": p["loader"], "merged_commit": merged,
                                 "migrated": migrated});
        self.record_buk(id, &target, next.clone(), "update")?;
        let mut upstream = Vec::new();
        for b in &overlays {
            let u = self.upstream_changes(&json!({ "branch": b }))?;
            upstream.push(json!({"branch": b, "changes": u["changes"]}));
        }
        Ok(
            json!({"buk": id, "state": "installed", "version": next["version"], "change_set": cs,
                  "migrated": migrated, "overlays": upstream}),
        )
    }

    /// Write the `Buk` object of a registry record on the target, with the registry in the
    /// same commit.
    pub(crate) fn record_buk(
        &mut self,
        id: &str,
        target: &str,
        rec: Value,
        op: &str,
    ) -> Result<(), BpaError> {
        let mut reg = self.registry_records()?;
        reg.insert(id.to_string(), rec.clone());
        MARKS.with(|m| {
            m.borrow_mut()
                .push((target.to_string(), REGISTRY.to_string(), Value::Object(reg)))
        });
        let body = json!({"name": id, "version": rec["version"], "content_hash": rec["root"],
                          "publisher": rec["publisher"], "signature": rec["signature"]});
        let change = if op == "create" {
            json!({"op": "create", "class": "Buk", "object_id": rec["object_id"],
                   "slug": format!("buk-{}", id.replace('/', "-")), "body": body})
        } else {
            json!({"op": "update", "object_id": rec["object_id"], "body": body})
        };
        let r = self.system_commit(&json!({"branch": target, "changes": [change],
            "process": {"kind": "import", "message": format!("Buk {id} {}", rec["version"].as_str().unwrap_or_default())}}));
        MARKS.with(|m| m.borrow_mut().clear());
        r.map(|_| ())
    }

    /// Rewrite the instances of a class on a branch to the current schema version by a
    /// bulk process (DSN-DVM-123, -560): the branch's own objects whose version is older,
    /// in commits of at most `MIGRATION_BATCH` objects; a failed or interrupted process is
    /// resumed by calling again.
    pub fn migrate_instances(
        &mut self,
        branch: &str,
        class: &str,
        reason: &str,
    ) -> Result<Value, BpaError> {
        let cur = self.registry.class(class)?.schema_version;
        let pid = stable_id(
            &json!({"migration": class, "to": cur, "branch": branch, "reason": reason}),
            '8',
        )?;
        if let Some(rec) = self.store.process_rec(&pid)?
            && let Some(b) = rec.get("bulk_state").filter(|b| b.is_object())
        {
            if b["state"] == "committed" {
                return Ok(json!({"state": "committed", "objects": 0, "process_id": pid}));
            }
            let r = self.bulk(&json!({"process_id": pid, "branch": branch, "resume": true,
                "changes": rec["migration_changes"], "max_changes": MIGRATION_BATCH}))?;
            return Ok(json!({"state": r["state"], "process_id": pid}));
        }
        let own: Vec<(String, String)> = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let parent = repo.branch_meta(branch)?.parent_line.is_some();
            if parent {
                let (v, _) = repo.view(branch, &json!({}))?;
                v.state
                    .objects
                    .iter()
                    .filter(|(oid, o)| {
                        o.class == class && v.provenance.get(*oid).is_some_and(|(b, _)| b == branch)
                    })
                    .map(|(oid, o)| (oid.clone(), o.head.clone()))
                    .collect()
            } else {
                let cseq = repo.ordinal(branch, None)?;
                repo.objects_in(branch, cseq, Some(class))?
                    .into_iter()
                    .filter(|(_, o)| o.class == class)
                    .map(|(oid, o)| (oid, o.head))
                    .collect()
            }
        };
        let mut changes = Vec::new();
        for (oid, head) in own {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let v = repo.version(&head)?;
            if v["tombstone"] == json!(true)
                || v["schema_version"].as_i64().is_none_or(|x| x >= cur)
            {
                continue;
            }
            let body = crate::lenses::current_body(&mut repo, &v)?;
            changes.push(json!({"op": "update", "object_id": oid, "base": head, "body": Value::Object(body)}));
        }
        let n = changes.len();
        if n == 0 {
            return Ok(json!({"state": "committed", "objects": 0}));
        }
        let r = self.bulk(&json!({"process_id": pid, "branch": branch, "changes": changes,
            "max_changes": MIGRATION_BATCH,
            "process": {"kind": "job", "message": format!("migration of {class} to schema version {cur} ({reason})")}}))?;
        if let Some(mut rec) = self.store.process_rec(&pid)? {
            rec["migration_changes"] = Value::Array(changes);
            self.store.put_process_rec(&pid, &rec)?;
        }
        Ok(json!({"state": r["state"], "objects": n, "process_id": pid}))
    }

    /// Adapter operation `buk_rollback` (FR-PKG-044): revert the merge of the last upgrade
    /// of a Buk (STD-VER-071) and restore the previous registry record. Class changes and
    /// rewritten instances stay; data written under the new schema reads through the lenses.
    pub fn buk_rollback(&mut self, i: &Value) -> Result<Value, BpaError> {
        let id = s(i, "buk")
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "buk names the Buk ID"))?
            .to_string();
        let rec = self
            .registry_records()?
            .get(&id)
            .cloned()
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("{id} is not installed")))?;
        if rec["state"] != "installed" || !rec["previous"].is_object() || rec["pending"].is_object()
        {
            return Err(BpaError::new(
                "FLOW.TRANSITION_NOT_ALLOWED",
                format!("{id} has no completed upgrade to roll back"),
            ));
        }
        // the installed Buks that depend on this one accept the previous version
        let prev_v = rec["previous"]["version"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        for (other, o) in self.registry_records()? {
            if let Some(range) = o["dependencies"].get(&id).and_then(Value::as_str) {
                let ok = crate::buk::Version::parse(&prev_v)
                    .ok()
                    .zip(crate::buk::Range::parse(range).ok())
                    .is_some_and(|(v, r)| r.matches(&v));
                if !ok {
                    return Err(BpaError::with_details(
                        "PKG.CONFIGURATION_INCOMPLETE",
                        json!({"reason": "a dependent requires the installed version", "dependent": other, "range": range}),
                    ));
                }
            }
        }
        let target = rec["target"].as_str().unwrap_or("main").to_string();
        let cs = rec["upgrade"]["change_set"].clone();
        let mut input = json!({"change_set": cs});
        if let Some(r) = i.get("resolutions").filter(|r| !r.is_null()) {
            input["resolutions"] = r.clone();
        }
        // the change set object's own state changed after its merge; that conflict keeps the
        // current state, and any other conflict goes back to the administrator
        let reverted = match self.revert(&input) {
            Err(e) if e.code == "VER.MERGE_CONFLICT" => {
                let conflicts: Vec<Value> = e
                    .details
                    .as_ref()
                    .and_then(|d| d["conflicts"].as_array().cloned())
                    .unwrap_or_default();
                let own: Vec<Value> = conflicts
                    .iter()
                    .filter(|c| c["object_id"] == cs)
                    .map(|c| {
                        json!({"object_id": c["object_id"], "path": c["path"], "choice": "target",
                                    "resolver": "ubos:buk_rollback"})
                    })
                    .collect();
                if own.is_empty()
                    || own.len() < conflicts.len()
                        && i.get("resolutions").is_none_or(Value::is_null)
                {
                    return Err(e);
                }
                let mut all = input["resolutions"].as_array().cloned().unwrap_or_default();
                all.extend(own);
                input["resolutions"] = Value::Array(all);
                self.revert(&input)?
            }
            r => r?,
        };
        let mut prev = rec["previous"].clone();
        prev["rolled_back"] =
            json!({"version": rec["version"], "root": rec["root"], "change_set": cs});
        self.record_buk(&id, &target, prev.clone(), "update")?;
        Ok(
            json!({"buk": id, "state": "installed", "version": prev["version"], "reverted": reverted,
                  "kept": {"class_changes": rec["upgrade"]["migrated"], "note": "class changes and migrated instances stay; they read through the lenses"}}),
        )
    }
}
