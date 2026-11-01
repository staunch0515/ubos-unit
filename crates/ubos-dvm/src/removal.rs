//! Buk removal and deprecation (PH-2 step 7e-2, DEC-132; FR-PKG-051, -052, -053;
//! DSN-DVM-134).
//!
//! * **Uninstall** (`buk_uninstall`): refused while installed Buks depend on the Buk, naming
//!   them (FR-PKG-053), and for the genesis Buks. The plan lists the Buk's behaviour that
//!   goes and its data that stays:
//!   - its logic assets are retired (ports implemented by them no longer run);
//!   - the objects its seed created of classes that are not its own (subscriptions,
//!     schedules, sheets, views and other configuration) are ended;
//!   - its classes and their data stay, read-only (`retention: read_only`, FR-PKG-051):
//!     readable and exportable, written only by the platform until retention allows their
//!     disposal.
//!
//!   The retirements and ends are staged on a loader branch and submitted as a change set;
//!   `buk_complete` approves and merges it, marks the classes read-only and records the Buk
//!   as `uninstalled` in one commit.
//! * **Reinstall.** Installing an uninstalled Buk again goes through the upgrade path. Its
//!   retired assets are restored and verified, and its classes are writable again. Objects
//!   that were ended stay ended and are reported.
//! * **Deprecation** (FR-PKG-052): a publisher deprecates versions of a Buk in the manifest
//!   of a version (`deprecation`), or later by a notice signed with the publisher's key
//!   (`buk_deprecate`). Plans and the registry show it, and the registry marks a Buk past
//!   its end of support.

use crate::buk::{Archive, Range, Version};
use crate::packages::{
    MARKS, READ_ONLY_KEY, archive_key, asset_object_id, items, problem, s, stable_id,
};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::{BpaError, canonical, content_id};
use ubos_version::state::Repo;

/// The namespace of the genesis Buks, which are not removed.
const GENESIS: &str = "ubos.platform";

/// The shape of a deprecation notice: `message`, and optionally `end_of_support` (a date)
/// and `replacement` (a Buk ID).
pub fn check_deprecation(d: &Value) -> Result<(), BpaError> {
    let bad = |m: &str| BpaError::new("PKG.MANIFEST_INVALID", m.to_string());
    if !d.is_object() || s(d, "message").is_none_or(str::is_empty) {
        return Err(bad("a deprecation states its message"));
    }
    if let Some(e) = d.get("end_of_support").filter(|e| !e.is_null()) {
        let ok = e
            .as_str()
            .is_some_and(|x| ubos_types::time::parse_date(x).is_ok());
        if !ok {
            return Err(bad("end_of_support is a date YYYY-MM-DD"));
        }
    }
    if let Some(r) = d.get("replacement").filter(|r| !r.is_null()) {
        crate::buk::split_id(r.as_str().unwrap_or_default())?;
    }
    Ok(())
}

impl Engine {
    /// The read-only classes of uninstalled Buks (class → Buk ID), read once.
    pub(crate) fn read_only_classes(&mut self) -> Result<BTreeMap<String, String>, BpaError> {
        if let Some(r) = &self.read_only {
            return Ok(r.clone());
        }
        let m: BTreeMap<String, String> = self
            .store
            .meta(READ_ONLY_KEY)?
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, v.as_str().unwrap_or_default().to_string()))
            .collect();
        self.read_only = Some(m.clone());
        Ok(m)
    }

    /// The uninstall plan of an installed Buk, with its archive.
    fn uninstall_plan(&mut self, id: &str) -> Result<(Value, Option<Archive>), BpaError> {
        let reg = self.registry_records()?;
        let rec = reg
            .get(id)
            .cloned()
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("{id} is not installed")))?;
        let mut problems = Vec::new();
        if rec["state"] != "installed" || rec["pending"].is_object() {
            problems.push(problem(
                "FLOW.TRANSITION_NOT_ALLOWED",
                format!("{id} is not installed, or a change of it is pending"),
            ));
        }
        if rec["namespace"] == json!(GENESIS) {
            problems.push(problem(
                "MODEL.CONSTRAINT_VIOLATED",
                "the genesis Buks are not removed",
            ));
        }
        // dependency protection (FR-PKG-053)
        let dependents: Vec<String> = reg
            .iter()
            .filter(|(k, r)| {
                *k != id && r["state"] != "uninstalled" && r["dependencies"].get(id).is_some()
            })
            .map(|(k, _)| k.clone())
            .collect();
        if !dependents.is_empty() {
            problems.push(problem(
                "MODEL.CONSTRAINT_VIOLATED",
                format!("installed Buks depend on {id}: {}", dependents.join(", ")),
            ));
        }
        let target = rec["target"].as_str().unwrap_or("main").to_string();
        let archive = self
            .store
            .meta(&archive_key(rec["root"].as_str().unwrap_or_default()))?
            .and_then(|d| Archive::decode(canonical(&d).ok()?.as_bytes()).ok());
        let Some(a) = archive else {
            problems.push(problem(
                "PKG.MANIFEST_INVALID",
                format!("no archive of {id} is recorded; reinstall the version to remove it"),
            ));
            return Ok((
                json!({"buk": id, "dependents": dependents, "problems": problems}),
                None,
            ));
        };
        // classes stay, read-only, with their data
        let own: Vec<String> = items(&a, "model", "classes")
            .0
            .iter()
            .filter_map(|c| s(c, "name").map(str::to_string))
            .collect();
        let mut data = Map::new();
        {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let cseq = repo.ordinal(&target, None)?;
            for c in &own {
                let n = repo
                    .objects_in(&target, cseq, Some(c))?
                    .values()
                    .filter(|o| &o.class == c)
                    .count();
                data.insert(c.clone(), json!(n));
            }
        }
        // logic assets are retired unless another installed Buk's class implements a port by them
        let mut used_elsewhere: BTreeSet<String> = BTreeSet::new();
        for (k, r) in &reg {
            if k == id || r["state"] != "installed" {
                continue;
            }
            if let Some(o) = self
                .store
                .meta(&archive_key(r["root"].as_str().unwrap_or_default()))?
                .and_then(|d| Archive::decode(canonical(&d).ok()?.as_bytes()).ok())
            {
                for c in items(&o, "model", "classes").0 {
                    for p in c["ports"].as_array().into_iter().flatten() {
                        if let Some(imp) = s(p, "impl") {
                            used_elsewhere.insert(imp.split('#').next().unwrap_or(imp).to_string());
                        }
                    }
                }
            }
        }
        let (mut retire, mut kept) = (Vec::new(), Vec::new());
        for x in items(&a, "logic", "assets").0 {
            let name = s(&x, "name").unwrap_or_default().to_string();
            match self.read(&json!({"branch": target, "object_id": asset_object_id(&name)})) {
                Ok(r) if r["body"]["state"] != "retired" => {
                    if used_elsewhere.contains(&name) {
                        kept.push(name);
                    } else {
                        retire.push(name);
                    }
                }
                _ => {}
            }
        }
        // configuration objects the seed created of classes that are not the Buk's own
        let mut end = Vec::new();
        let mut by_class: BTreeMap<String, u64> = BTreeMap::new();
        for ch in items(&a, "seed", "changes").0 {
            let (Some(oid), Some(class)) = (s(&ch, "object_id"), s(&ch, "class")) else {
                continue;
            };
            if own.iter().any(|c| c == class) || class == "Lens" {
                continue;
            }
            if self
                .read(&json!({"branch": target, "object_id": oid}))
                .is_ok()
            {
                end.push(json!(oid));
                *by_class.entry(class.to_string()).or_default() += 1;
            }
        }
        let plan = json!({"buk": id, "version": rec["version"], "target": target,
            "loader": format!("loader/{}/{}-{}-uninstall", rec["namespace"].as_str().unwrap_or_default().replace('.', "-"),
                rec["name"].as_str().unwrap_or_default().replace('_', "-"), rec["version"].as_str().unwrap_or_default().replace('.', "-")),
            "retention": "read_only",
            "classes": {"read_only": own, "objects": data},
            "assets": {"retire": retire, "kept_used_by_other_buks": kept},
            "configuration": {"end": end, "by_class": by_class},
            "views": a.tree.keys().filter(|p| p.starts_with("views/")).count(),
            "sheets": a.tree.keys().filter(|p| p.starts_with("sheets/")).count(),
            "dependents": dependents, "problems": problems});
        Ok((plan, Some(a)))
    }

    /// Adapter operation `buk_uninstall` (FR-PKG-051, -053): the plan with `plan: true`;
    /// otherwise the retirements and ends are staged on a loader branch and submitted as a
    /// change set, which `buk_complete` approves and merges.
    pub fn buk_uninstall(&mut self, i: &Value) -> Result<Value, BpaError> {
        let id = s(i, "buk")
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "buk names the Buk ID"))?
            .to_string();
        if let Some(r) = s(i, "retention")
            && r != "read_only"
        {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "retention is read_only: data is disposed of by the retention rules, not by an uninstall",
            ));
        }
        let (plan, _) = self.uninstall_plan(&id)?;
        if i.get("plan") == Some(&json!(true)) {
            return Ok(plan);
        }
        if let Some(p) = plan["problems"].as_array().and_then(|p| p.first()) {
            let code: &'static str = match s(p, "code") {
                Some("FLOW.TRANSITION_NOT_ALLOWED") => "FLOW.TRANSITION_NOT_ALLOWED",
                Some("PKG.MANIFEST_INVALID") => "PKG.MANIFEST_INVALID",
                _ => "MODEL.CONSTRAINT_VIOLATED",
            };
            return Err(BpaError::with_details(code, plan));
        }
        let target = plan["target"].as_str().unwrap_or("main").to_string();
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
        let mut changes = Vec::new();
        for name in plan["assets"]["retire"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let oid = asset_object_id(name.as_str().unwrap_or_default());
            let (o, _, _, _) = self.object_view(&loader, &json!({}), &oid)?;
            changes.push(json!({"op": "update", "object_id": oid, "base": o.head, "body": {"state": "retired"}}));
        }
        for oid in plan["configuration"]["end"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let oid = oid.as_str().unwrap_or_default().to_string();
            let (o, _, _, _) = self.object_view(&loader, &json!({}), &oid)?;
            changes.push(json!({"op": "end", "object_id": oid, "base": o.head}));
        }
        let label = format!("{id} {}", plan["version"].as_str().unwrap_or_default());
        if !changes.is_empty() {
            self.commit(&json!({"branch": loader, "changes": changes,
                "process": {"kind": "import", "message": format!("uninstall {label}")}}))?;
        }
        let cs = stable_id(
            &json!({"buk_uninstall": id, "target": target, "loader": loader}),
            '8',
        )?;
        self.change_set(
            &json!({"action": "open", "id": cs, "source": loader, "target": target,
            "title": format!("Uninstall {label}"),
            "description": format!("Buk {label}: behaviour removed, data kept read-only")}),
        )?;
        let submitted = self.change_set(&json!({"action": "submit", "id": cs}))?;
        let mut reg = self.registry_records()?;
        if let Some(r) = reg.get_mut(&id) {
            r["pending"] = json!({"kind": "uninstall", "change_set": cs, "loader": loader,
                "classes": plan["classes"]["read_only"], "retention": "read_only"});
        }
        self.store
            .put_meta(crate::packages::REGISTRY, &Value::Object(reg))?;
        Ok(json!({"plan": plan, "change_set": cs, "state": submitted["state"], "loader": loader}))
    }

    /// Complete an uninstall: approve and merge its change set, then mark the Buk's classes
    /// read-only and record it as `uninstalled`, in one commit.
    pub(crate) fn complete_uninstall(
        &mut self,
        id: &str,
        rec: Value,
        i: &Value,
    ) -> Result<Value, BpaError> {
        let p = rec["pending"].clone();
        let cs = p["change_set"].as_str().unwrap_or_default().to_string();
        let merged = self.approve_and_merge(&cs, i)?;
        let target = rec["target"].as_str().unwrap_or("main").to_string();
        let mut ro = self.read_only_classes()?;
        for c in p["classes"].as_array().into_iter().flatten() {
            ro.insert(c.as_str().unwrap_or_default().to_string(), id.to_string());
        }
        let mut next = rec.clone();
        if let Some(m) = next.as_object_mut() {
            m.remove("pending");
            m.remove("previous");
        }
        next["state"] = json!("uninstalled");
        next["change_set"] = json!(cs);
        next["uninstalled"] = json!({"change_set": cs, "merged_commit": merged, "retention": p["retention"],
                                     "read_only": p["classes"]});
        MARKS.with(|m| {
            m.borrow_mut()
                .push((target.clone(), READ_ONLY_KEY.to_string(), json!(ro)))
        });
        self.record_buk(id, &target, next, "update")?;
        self.read_only = Some(ro);
        Ok(json!({"buk": id, "state": "uninstalled", "change_set": cs, "read_only": p["classes"]}))
    }

    /// Approve (when submitted) and merge a change set; the merge commit.
    pub(crate) fn approve_and_merge(&mut self, cs: &str, i: &Value) -> Result<Value, BpaError> {
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
        Ok(merged)
    }

    /// Lift the read-only mark of a reinstalled Buk's classes, with the commit that records
    /// it.
    pub(crate) fn lift_read_only(&mut self, id: &str, target: &str) -> Result<(), BpaError> {
        let mut ro = self.read_only_classes()?;
        let before = ro.len();
        ro.retain(|_, b| b != id);
        if ro.len() != before {
            MARKS.with(|m| {
                m.borrow_mut()
                    .push((target.to_string(), READ_ONLY_KEY.to_string(), json!(ro)))
            });
            self.read_only = Some(ro);
        }
        Ok(())
    }

    /// Adapter operation `buk_deprecate` (FR-PKG-052): record a deprecation notice of an
    /// installed Buk. The notice `{buk, versions, message, end_of_support?, replacement?}`
    /// is signed by the Buk's publisher (a DAT-Signature over its content ID); it applies
    /// when the installed version is in `versions`.
    pub fn buk_deprecate(&mut self, i: &Value) -> Result<Value, BpaError> {
        let notice = i
            .get("notice")
            .filter(|n| n.is_object())
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "a notice"))?
            .clone();
        check_deprecation(&notice)?;
        let id = s(&notice, "buk").unwrap_or_default().to_string();
        let versions = Range::parse(s(&notice, "versions").unwrap_or_default())?;
        let reg = self.registry_records()?;
        let rec = reg
            .get(&id)
            .cloned()
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("{id} is not installed")))?;
        let sig = &i["signature"];
        let doc = crate::types::addr::key_document(&sig["signer"]).map_err(|_| {
            BpaError::new("PKG.SIGNATURE_INVALID", "the signer is not a key-form DID")
        })?;
        crate::crypto::verify(sig, &content_id("ubos.chunk.v1", &notice)?, &doc, None)
            .map_err(|e| BpaError::new("PKG.SIGNATURE_INVALID", e.message))?;
        if sig["signer"] != rec["publisher"] {
            return Err(BpaError::new(
                "PKG.SIGNATURE_INVALID",
                "the notice is not signed by the Buk's publisher",
            ));
        }
        let applies = Version::parse(rec["version"].as_str().unwrap_or_default())
            .ok()
            .is_some_and(|v| versions.matches(&v));
        if !applies {
            return Ok(json!({"buk": id, "applies": false, "version": rec["version"]}));
        }
        let mut d = notice.clone();
        if let Some(m) = d.as_object_mut() {
            m.remove("buk");
        }
        d["signer"] = sig["signer"].clone();
        let mut next = rec.clone();
        next["deprecation"] = d;
        let target = rec["target"].as_str().unwrap_or("main").to_string();
        self.record_buk(&id, &target, next.clone(), "update")?;
        Ok(json!({"buk": id, "applies": true, "deprecation": next["deprecation"]}))
    }

    /// The support state of a registry record: `supported`, `deprecated`, or
    /// `end_of_support` when the deprecation's date has passed.
    pub(crate) fn support_of(&self, rec: &Value) -> &'static str {
        let d = &rec["deprecation"];
        if !d.is_object() {
            return "supported";
        }
        match s(d, "end_of_support") {
            Some(e) if self.now.get(..10).is_some_and(|today| today >= e) => "end_of_support",
            _ => "deprecated",
        }
    }
}
