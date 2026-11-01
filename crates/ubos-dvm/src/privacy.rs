//! Personal-data read logging and the subject access export (PH-2 step 8d, DEC-139;
//! DSN-DVM-823, -837; FR-AUD-051, NR-PRIV-005, CR-GDPR-002, CR-CCPA-001).
//!
//! * **Read log.** A read or query made `as` a viewer whose output, after masking, shows
//!   personal or sensitive personal fields of objects whose subject is not the viewer
//!   appends one `pii.read` record to the security log per operation: the principal, the
//!   operation, the branch and commit, and the objects with the fields shown (names, never
//!   values). The viewer is the subject when its principal is the subject ID or the
//!   principal field of the subject object.
//! * **Subject export.** `subject_export {subject_id, branch?}` gives every object of the
//!   branch linked to the subject through its class's `subject_key`, with all its versions,
//!   and the entries of ledger classes linked the same way, in JSON with a manifest. Fields
//!   encrypted under another subject's key are left out. The export is recorded in the
//!   security log (`subject.export`); key rotations and erasures are recorded too.

use crate::protection::{is_marker, is_stored, protected_fields, subject_decl};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::{BpaError, content_id};
use ubos_version::Repo;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The format of a subject export.
pub const EXPORT_FORMAT: &str = "ubos.subject-export.v1";

impl Engine {
    /// The subject of a stored version: the subject of its first encrypted field.
    fn version_subject(&mut self, vh: &str) -> Result<Option<String>, BpaError> {
        let v = Repo::new(self.store.as_mut(), &self.registry).stored_version(vh)?;
        Ok(v["body"]
            .as_object()
            .into_iter()
            .flatten()
            .find(|(_, e)| is_stored(e))
            .and_then(|(_, e)| s(&e["key"], "subject_id").map(str::to_string)))
    }

    /// Whether a viewer is the subject: its principal is the subject ID, or the principal
    /// field of the subject object at the snapshot.
    fn viewer_is_subject(
        &mut self,
        branch: &str,
        sel: &Value,
        subject: &str,
        principal: &str,
    ) -> Result<bool, BpaError> {
        if subject == principal {
            return Ok(true);
        }
        if !ubos_types::names::matches("uuid", subject) {
            return Ok(false);
        }
        let mut i = sel.clone();
        i["branch"] = json!(branch);
        i["object_id"] = json!(subject);
        let Ok((r, class)) = self.read_classed(&i) else {
            return Ok(false);
        };
        Ok(self
            .principal_field(&class)
            .and_then(|f| {
                r["body"]
                    .get(&f)
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .is_some_and(|p| p == principal))
    }

    /// The personal fields an output shows for an object to a viewer who is not its
    /// subject, empty otherwise.
    pub(crate) fn personal_shown(
        &mut self,
        branch: &str,
        sel: &Value,
        class: &str,
        version: &str,
        shown: &Map<String, Value>,
        principal: &str,
    ) -> Result<Vec<String>, BpaError> {
        let fields: Vec<String> = protected_fields(&self.registry, class)
            .into_iter()
            .filter(|f| shown.get(f).is_some_and(|v| !v.is_null() && !is_marker(v)))
            .collect();
        if fields.is_empty() {
            return Ok(fields);
        }
        if let Some(subject) = self.version_subject(version)?
            && self.viewer_is_subject(branch, sel, &subject, principal)?
        {
            return Ok(Vec::new());
        }
        Ok(fields)
    }

    /// Append one `pii.read` record for an operation (DSN-DVM-823): nothing when no
    /// personal field was shown.
    pub(crate) fn log_personal_reads(
        &mut self,
        operation: &str,
        branch: &str,
        evaluated_at: &Value,
        principal: &str,
        shown: BTreeMap<String, Vec<String>>,
    ) -> Result<(), BpaError> {
        if shown.is_empty() {
            return Ok(());
        }
        let objects: Vec<&String> = shown.keys().collect();
        let rec = json!({"principal": principal, "operation": operation, "branch": branch,
            "evaluated_at": evaluated_at, "objects": objects, "fields": shown});
        self.security_append("pii.read", rec)?;
        Ok(())
    }

    /// Whether an object of a class is linked to a subject through its `subject_key`.
    fn linked(
        &mut self,
        branch: &str,
        cseq: u64,
        oid: &str,
        sk: &str,
        body: &Value,
        subject: &str,
    ) -> Result<bool, BpaError> {
        if sk == "self" {
            return Ok(oid == subject);
        }
        let Some(v) = body.get(sk).and_then(Value::as_str) else {
            return Ok(false);
        };
        if v == subject {
            return Ok(true);
        }
        if v.starts_with("ubos://")
            && let Ok(p) = ubos_types::addr::parse_uri(&json!(v))
            && let (Some(class), Some(slug)) = (s(&p, "class"), s(&p, "slug"))
        {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            return Ok(repo
                .object_by_slug(branch, cseq, class, slug)?
                .is_some_and(|(id, _)| id == subject));
        }
        Ok(false)
    }

    /// A body for the export: fields encrypted under another subject's key left out.
    fn export_body(stored: &Value, opened: &Value, subject: &str) -> Value {
        let mut body = opened["body"].as_object().cloned().unwrap_or_default();
        for (f, e) in stored["body"].as_object().into_iter().flatten() {
            if is_stored(e) && s(&e["key"], "subject_id") != Some(subject) {
                body.remove(f);
            }
        }
        Value::Object(body)
    }

    /// Adapter operation `subject_export {subject_id, branch?}` (DSN-DVM-837).
    pub fn subject_export(&mut self, i: &Value) -> Result<Value, BpaError> {
        let subject = s(i, "subject_id")
            .filter(|x| !x.is_empty())
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "subject_id"))?
            .to_string();
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let (states, cseq) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let head = repo
                .store
                .branch(&branch)?
                .ok_or_else(|| BpaError::new("VER.BRANCH_NOT_FOUND", branch.clone()))?;
            (
                repo.objects_in(&branch, head.head_cseq, None)?,
                head.head_cseq,
            )
        };
        let mut objects = Vec::new();
        let mut versions_n = 0u64;
        for (oid, o) in states {
            let Some((sk, _)) = subject_decl(&self.registry, &o.class) else {
                continue;
            };
            let head = Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?;
            if !self.linked(&branch, cseq, &oid, &sk, &head["body"], &subject)? {
                continue;
            }
            // the versions of the object: its timeline and their predecessors
            let mut todo: Vec<String> = std::iter::once(o.head.clone())
                .chain(o.timeline.iter().map(|g| g.version.clone()))
                .collect();
            let mut seen = BTreeSet::new();
            let mut versions = Vec::new();
            while let Some(vh) = todo.pop() {
                if !seen.insert(vh.clone()) {
                    continue;
                }
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                let stored = repo.stored_version(&vh)?;
                let opened = repo.version(&vh)?;
                versions.push(json!({"version_hash": vh, "valid": opened["valid"],
                    "tombstone": opened["tombstone"] == json!(true),
                    "body": Engine::export_body(&stored, &opened, &subject)}));
                if let Some(p) = s(&stored, "predecessor") {
                    todo.push(p.to_string());
                }
            }
            versions.sort_by(|a, b| {
                (s(&a["valid"], "from"), s(a, "version_hash"))
                    .cmp(&(s(&b["valid"], "from"), s(b, "version_hash")))
            });
            versions_n += versions.len() as u64;
            objects.push(json!({"object_id": oid, "class": o.class, "slug": o.slug,
                "versions": versions}));
        }
        let mut entries = Vec::new();
        for e in self.store.entries(&branch, cseq, None, None)? {
            let class = s(&e, "class").unwrap_or_default().to_string();
            let Some((sk, _)) = subject_decl(&self.registry, &class) else {
                continue;
            };
            let oid = s(&e, "entry_id").unwrap_or_default().to_string();
            if self.linked(&branch, cseq, &oid, &sk, &e["body"], &subject)? {
                entries.push(
                    json!({"entry_id": oid, "class": class, "valid_at": e["valid_at"],
                    "body": e["body"]}),
                );
            }
        }
        let content = json!({"objects": objects, "entries": entries});
        let manifest = json!({
            "format": EXPORT_FORMAT,
            "subject_id": subject,
            "branch": branch,
            "cseq": cseq,
            "generated_at": self.now,
            "counts": {"objects": objects.len(), "versions": versions_n, "entries": entries.len()},
            "content": content_id("ubos.chunk.v1", &content)?,
        });
        self.security_append(
            "subject.export",
            json!({"subject_id": subject, "branch": branch, "cseq": cseq,
                   "counts": manifest["counts"], "content": manifest["content"]}),
        )?;
        Ok(json!({"manifest": manifest, "objects": content["objects"],
                  "entries": content["entries"]}))
    }
}
