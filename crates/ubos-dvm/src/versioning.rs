//! Versioning operations of the engine (STD-VER-011…090; DEC-051): processes, write policy,
//! validation of merged states, merges with resolutions, merge previews, change sets, tags
//! and releases, revert, branch metadata, snapshot selection and commit records.

use crate::relations::Checks;
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_model::Registry;
use ubos_store::Store;
use ubos_types::{BpaError, canonical, content_id, names};
use ubos_version::{PROTECTED, Repo, State, Tx, base_id, unique};

/// Initiator when a process names none (DEC-041).
pub const PRINCIPAL: &str = "did:ubos:acme:01900000-0000-7000-8000-000000000002";
const PROCESS_KINDS: &[&str] = &[
    "action",
    "schedule",
    "job",
    "handler",
    "merge",
    "import",
    "agent_run",
    "sync",
    "kernel",
];
const TRIGGERS: &[&str] = &["user", "event", "schedule", "job", "merge", "external"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn not_found(m: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", m)
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

/// A normalised process of a commit, merge or revert (STD-TXN-001).
#[derive(Debug, Clone)]
pub struct Process {
    /// Process identifier.
    pub id: String,
    /// Named by the caller.
    pub explicit: bool,
    /// The process as given, with defaults.
    pub p: Value,
}

// ---- processes (STD-TXN-001, -031) ----

/// Normalise the process of an operation; a missing identifier is generated.
pub fn process(
    store: &mut dyn Store,
    input: Option<&Value>,
    kind: &str,
    next: &mut u64,
    principal: &str,
) -> Result<Process, BpaError> {
    let mut p = input.cloned().unwrap_or_else(|| json!({}));
    if !p.is_object() {
        return Err(invalid("process is an object"));
    }
    let explicit = p.get("process_id").is_some_and(|v| !v.is_null());
    if explicit
        && !p["process_id"]
            .as_str()
            .is_some_and(|x| names::matches("uuid", x))
    {
        return Err(invalid("process_id is a UUID"));
    }
    if p.get("kind").is_none() {
        p["kind"] = json!(kind);
    }
    if !PROCESS_KINDS.contains(&s(&p, "kind").unwrap_or_default()) {
        return Err(invalid("process kind"));
    }
    if p.get("trigger").is_none() {
        p["trigger"] = json!({"type": "user"});
    }
    let ttype = p["trigger"]
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !p["trigger"].is_object() || !TRIGGERS.contains(&ttype.as_str()) {
        return Err(invalid("trigger type"));
    }
    let parent = p.get("parent_process").filter(|v| !v.is_null()).cloned();
    if parent.is_none() && matches!(ttype.as_str(), "event" | "job") {
        return Err(BpaError::new(
            "MODEL.DECLARATION_INCOMPLETE",
            "a process triggered by an event or job names its parent",
        ));
    }
    if let Some(pp) = &parent
        && store
            .process_rec(pp.as_str().unwrap_or_default())?
            .is_none()
    {
        return Err(not_found("unknown parent process"));
    }
    if p.get("initiator").is_none() {
        p["initiator"] = json!(principal);
    }
    for e in p
        .get("effects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !e.is_object() || !e["type"].is_string() {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "an effect has a type",
            ));
        }
    }
    let id = if explicit {
        p["process_id"].as_str().unwrap_or_default().to_string()
    } else {
        *next += 1;
        format!("01900000-0000-7000-8000-{:012x}", 0x1000_0000u64 + *next)
    };
    Ok(Process { id, explicit, p })
}

/// Record the state of a process, a commit it made and the effects it released
/// (STD-TXN-031). Processes the caller did not name are not recorded: nothing can refer to
/// them.
pub fn record(
    store: &mut dyn Store,
    p: &Process,
    state: &str,
    commit: Option<(&str, u64)>,
    effects: u32,
) -> Result<(), BpaError> {
    if !p.explicit {
        return Ok(());
    }
    let mut rec = match store.process_rec(&p.id)? {
        Some(r) => r,
        None => {
            let mut r = json!({"process_id": p.id, "kind": p.p["kind"], "initiator": p.p["initiator"],
                "trigger": p.p["trigger"], "state": state, "commits": [], "effects": 0, "explicit": p.explicit});
            for k in ["parent_process", "bulk", "idempotency_key"] {
                if let Some(v) = p.p.get(k)
                    && !v.is_null()
                {
                    r[k] = v.clone();
                }
            }
            r
        }
    };
    rec["state"] = json!(state);
    if let Some((b, c)) = commit {
        rec["commits"]
            .as_array_mut()
            .expect("list")
            .push(json!([b, c]));
    }
    if effects > 0 {
        rec["effects"] = json!(rec["effects"].as_u64().unwrap_or_default() + u64::from(effects));
    }
    store.put_process_rec(&p.id, &rec)
}

fn all_processes(store: &mut dyn Store) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for id in store.process_ids()? {
        if let Some(r) = store.process_rec(&id)? {
            out.push(r);
        }
    }
    Ok(out)
}

fn descendants(
    store: &mut dyn Store,
    roots: &BTreeSet<String>,
) -> Result<BTreeSet<String>, BpaError> {
    let all = all_processes(store)?;
    let mut out = roots.clone();
    let mut todo: Vec<String> = roots.iter().cloned().collect();
    while let Some(x) = todo.pop() {
        for r in &all {
            let id = s(r, "process_id").unwrap_or_default().to_string();
            if s(r, "parent_process") == Some(x.as_str()) && out.insert(id.clone()) {
                todo.push(id);
            }
        }
    }
    Ok(out)
}

impl Engine {
    /// `process_info` (STD-TXN-031).
    pub fn process_info(&mut self, process_id: &str) -> Result<Value, BpaError> {
        let rec = self
            .store
            .process_rec(process_id)?
            .ok_or_else(|| not_found("unknown process"))?;
        let mut out = Map::new();
        for (k, v) in rec.as_object().into_iter().flatten() {
            if !matches!(k.as_str(), "commits" | "effects" | "explicit") {
                out.insert(k.clone(), v.clone());
            }
        }
        let commits: Vec<Value> = rec["commits"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| json!({"branch": c[0], "cseq": c[1]}))
            .collect();
        out.insert("commits".into(), Value::Array(commits));
        // Child processes of a bulk job are counted by its attempts (STD-TXN-070); the
        // children themselves are implicit processes and not recorded (DEC-051).
        let kids = self.bulk_children(process_id)?;
        if kids > 0 {
            out.insert("children".into(), json!(kids));
        }
        Ok(Value::Object(out))
    }

    /// `lineage` (STD-TXN-031).
    pub fn lineage(&mut self, process_id: &str, direction: &str) -> Result<Value, BpaError> {
        let rec = self
            .store
            .process_rec(process_id)?
            .ok_or_else(|| not_found("unknown process"))?;
        match direction {
            "ancestors" => {
                let mut out = Vec::new();
                let mut p = s(&rec, "parent_process").map(str::to_string);
                while let Some(x) = p {
                    let r = self.store.process_rec(&x)?;
                    out.push(json!(x));
                    p = r
                        .as_ref()
                        .and_then(|r| s(r, "parent_process"))
                        .map(str::to_string);
                }
                Ok(json!({"processes": out}))
            }
            "descendants" => {
                let roots: BTreeSet<String> = [process_id.to_string()].into();
                let mut d = descendants(self.store.as_mut(), &roots)?;
                d.remove(process_id);
                Ok(json!({"processes": d}))
            }
            _ => Err(invalid("direction is ancestors or descendants")),
        }
    }

    // ---- validation and policy ----

    /// STD-VER-012: on protected branches, change-set classes are written only by the
    /// merge of an approved change set.
    pub fn write_policy(
        &mut self,
        branch: &str,
        classes: &[String],
        via_change_set: bool,
    ) -> Result<(), BpaError> {
        if via_change_set {
            return Ok(());
        }
        let kind = Repo::new(self.store.as_mut(), &self.registry)
            .branch_meta(branch)?
            .kind;
        if !PROTECTED.contains(&kind.as_str()) {
            return Ok(());
        }
        for c in classes {
            if self.registry.class(c)?.write_policy == "change_set" {
                return Err(BpaError::new("VER.CHANGE_SET_REQUIRED", c.to_string()));
            }
        }
        Ok(())
    }

    /// STD-VER-055: constraints, references, cardinality, double entry and periods of a
    /// merged or reverted state, relative to the target state `t`. The transaction holds
    /// the merged state and the appended entries.
    pub fn validate_state(&mut self, tx: &mut Tx, t: &State) -> Result<(), BpaError> {
        let now = self.now.clone();
        let today = now[..10].to_string();
        let exts = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let st = tx.state.clone();
            let mut out = Vec::new();
            for o in st.objects.values() {
                if o.class == "ClassExtension" {
                    let v = repo.version_in(tx, &o.head)?;
                    if v["tombstone"] != json!(true) {
                        out.push(v["body"].clone());
                    }
                }
            }
            out
        };
        let reg = self.registry.with_extensions(&exts)?;
        let mut repo = Repo::new(self.store.as_mut(), &reg);
        let changed: Vec<String> = tx
            .state
            .objects
            .iter()
            .filter(|(id, o)| {
                t.objects
                    .get(*id)
                    .is_none_or(|x| x.head != o.head || x.timeline != o.timeline)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let mut written: Vec<(String, String)> = Vec::new();
        for oid in &changed {
            let o = tx.state.objects[oid].clone();
            let v = repo.version_in(tx, &o.head)?;
            if v["tombstone"] != json!(true) {
                reg.check_body(&o.class, &v["body"], false)?;
                written.push((oid.clone(), o.head.clone()));
            }
        }
        let ids: Vec<String> = tx.state.objects.keys().cloned().collect();
        for src in ids {
            let o = tx.state.objects[&src].clone();
            let v = repo.version_in(tx, &o.head)?;
            if v["tombstone"] == json!(true)
                || written.iter().any(|(a, b)| *a == src && *b == o.head)
            {
                continue;
            }
            let eff = reg.class(&o.class)?;
            let refs = eff.refs(&v["body"]);
            let authority = repo.store.authority();
            if refs.iter().any(|(_, u)| {
                crate::relations::resolve(&tx.state, u, &authority)
                    .is_some_and(|t| changed.contains(&t))
            }) {
                written.push((src, o.head.clone()));
            }
        }
        let regr = &reg;
        let default_valid = move |c: &str| match regr.class(c).map(|c| c.valid_time.as_str()) {
            Ok("instant") => now.clone(),
            _ => today.clone(),
        };
        let mut checks = Checks {
            repo: &mut repo,
            tx,
            default_valid: &default_valid,
        };
        checks.integrity(&written)?;
        checks.cardinality_check()?;
        checks.extensions_apply(&self.registry, &written)?;
        repo.check_double_entry(tx)?;
        let classes: BTreeSet<String> = tx.appended.iter().map(|e| e.class.clone()).collect();
        for c in classes {
            repo.check_period(tx, &c, &self.now[..10])?;
        }
        Ok(())
    }

    /// The rule binding of a commit (STD-TIME-040): class versions evaluated for its
    /// changes and the extensions applied to them.
    pub fn rule_binding(
        &mut self,
        tx: &Tx,
        versions: &Value,
        entries: &Value,
    ) -> Result<Value, BpaError> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for oid in versions.as_object().map(|m| m.keys()).into_iter().flatten() {
            if let Some(o) = tx.state.objects.get(oid) {
                names.insert(o.class.clone());
            }
        }
        for (_, _, q) in &tx.state.ledger {
            for e in q {
                if entries.get(&e.entry_id).is_some() {
                    names.insert(e.class.clone());
                }
            }
        }
        let mut lin: BTreeSet<String> = BTreeSet::new();
        for n in &names {
            lin.extend(self.registry.class(n)?.linearization.iter().cloned());
        }
        let mut items: Vec<Value> = Vec::new();
        for c in &lin {
            let decl = self.registry.decl(c)?;
            items.push(json!({"class": c, "class_version": content_id("ubos.chunk.v1", decl)?}));
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut exts: Vec<(String, String, Value)> = Vec::new();
        for (oid, o) in &tx.state.objects {
            if o.class == "ClassExtension" {
                let v = repo.version_in(tx, &o.head)?;
                if v["tombstone"] != json!(true) {
                    exts.push((oid.clone(), o.head.clone(), v["body"].clone()));
                }
            }
        }
        for (oid, vh, body) in exts {
            if s(&body, "target").is_some_and(|t| lin.contains(t)) {
                items.push(
                    json!({"class": "ClassExtension", "object_id": oid, "class_version": vh}),
                );
            }
        }
        Ok(json!({"classes": items}))
    }

    // ---- merge (STD-VER-050…058) ----

    /// `merge` with resolutions, process and write policy.
    pub fn merge(&mut self, input: &Value) -> Result<Value, BpaError> {
        let source = s(input, "source").unwrap_or_default().to_string();
        let target = s(input, "target").unwrap_or("main").to_string();
        self.merge_into(&source, &target, input, None)
    }

    fn merge_into(
        &mut self,
        source: &str,
        target: &str,
        input: &Value,
        change_set: Option<&str>,
    ) -> Result<Value, BpaError> {
        let now = self.now.clone();
        let resolutions = input
            .get("resolutions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut next = self.next_process;
        let p = process(
            self.store.as_mut(),
            input.get("process"),
            "merge",
            &mut next,
            &self.principal,
        )?;
        self.next_process = next;
        let plan = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            repo.head(source)?;
            repo.check_active(target, &now)?;
            repo.merge_allowed(source, target, change_set.is_some())?;
            repo.merge_prepare(source, target, &resolutions)?
        };
        let Some(mut plan) = plan else {
            let t = Repo::new(self.store.as_mut(), &self.registry).head(target)?;
            let cseq = Repo::new(self.store.as_mut(), &self.registry)
                .commit(t)?
                .cseq;
            return Ok(json!({"cseq": cseq, "merged": false}));
        };
        let t = plan.target_state.clone();
        let changed: Vec<String> = plan
            .tx
            .state
            .objects
            .iter()
            .filter(|(id, o)| {
                t.objects
                    .get(*id)
                    .is_none_or(|x| x.head != o.head || x.timeline != o.timeline)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let seen: BTreeSet<String> = t
            .ledger
            .iter()
            .flat_map(|x| x.2.iter())
            .map(|e| e.entry_id.clone())
            .collect();
        let entries: Vec<ubos_version::Entry> = plan
            .tx
            .state
            .ledger
            .iter()
            .flat_map(|x| x.2.iter())
            .filter(|e| !seen.contains(&e.entry_id))
            .cloned()
            .collect();
        plan.tx.appended = entries.clone();
        let versions: Map<String, Value> = changed
            .iter()
            .map(|id| (id.clone(), json!(plan.tx.state.objects[id].head)))
            .collect();
        let entry_ids: Map<String, Value> = entries
            .iter()
            .map(|e| (e.entry_id.clone(), json!({})))
            .collect();
        let classes: Vec<String> = changed
            .iter()
            .map(|id| plan.tx.state.objects[id].class.clone())
            .chain(entries.iter().map(|e| e.class.clone()))
            .collect();
        self.write_policy(target, &classes, change_set.is_some())?;
        let binding = self.rule_binding(
            &plan.tx,
            &Value::Object(versions.clone()),
            &Value::Object(entry_ids),
        )?;
        self.validate_state(&mut plan.tx, &t)?;
        let mut tx = plan.tx;
        tx.now = Some(now.clone());
        for oid in &changed {
            Repo::op(&mut tx, oid, "merge", None, None);
        }
        tx.extra.insert("rule_binding".into(), binding);
        if !plan.resolutions.is_empty() {
            tx.extra
                .insert("resolutions".into(), json!(plan.resolutions));
        }
        if let Some(cs) = change_set {
            tx.extra.insert("change_set".into(), json!(cs));
        }
        if p.explicit {
            tx.extra.insert("process_id".into(), json!(p.id));
        }
        tx.extra.insert(
            "appended".into(),
            json!(
                entries
                    .iter()
                    .map(|e| e.entry_id.clone())
                    .collect::<Vec<_>>()
            ),
        );
        let cseq = tx.cseq;
        let (commit_id, tx_time) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let id = repo.write(tx, plan.parents)?;
            let c = repo.commit(id)?;
            self.touch_branch(target)?;
            (c.commit_id, c.tx_time)
        };
        record(
            self.store.as_mut(),
            &p,
            "committed",
            Some((target, cseq)),
            0,
        )?;
        self.instances.extend(classes);
        let mut out = json!({"cseq": cseq, "merged": true, "appended": plan.appended, "commit_id": commit_id, "tx_time": tx_time});
        if !plan.resolutions.is_empty() {
            out["resolutions"] = json!(plan.resolutions);
        }
        Ok(out)
    }

    /// Record a commit's time as the branch's last activity (STD-VER-011).
    pub fn touch_branch(&mut self, branch: &str) -> Result<(), BpaError> {
        let mut m = self.branch_meta(branch)?;
        if m.kind != "draft" {
            return Ok(());
        }
        m.active_at = Some(self.now.clone());
        self.branch_cache.remove(branch);
        Repo::new(self.store.as_mut(), &self.registry).put_branch_meta(branch, &m)
    }

    /// `merge_preview` (STD-VER-057).
    pub fn merge_preview(&mut self, source: &str, target: &str) -> Result<Value, BpaError> {
        let (base, source_head, target_head, t_state, merged, conflicts, validation) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let (sh, th) = (repo.head(source)?, repo.head(target)?);
            let mut tx = repo.begin(target)?;
            let (best, t_state, st, conflicts) = repo.preview_states(source, target, &mut tx)?;
            let ids: Vec<String> = best
                .iter()
                .map(|c| repo.commit(*c).map(|x| x.commit_id))
                .collect::<Result<_, _>>()?;
            let base = base_id(&ids)?;
            tx.state = st.clone();
            // The class extensions in force are read from the transaction (validation).
            repo.load_class(&mut tx, "ClassExtension")?;
            (
                base,
                repo.commit(sh)?.commit_id,
                repo.commit(th)?.commit_id,
                t_state,
                (st, tx),
                conflicts,
                Vec::<Value>::new(),
            )
        };
        let (st, mut tx) = merged;
        let mut validation = validation;
        {
            let seen: BTreeSet<String> = t_state
                .ledger
                .iter()
                .flat_map(|x| x.2.iter())
                .map(|e| e.entry_id.clone())
                .collect();
            tx.appended = st
                .ledger
                .iter()
                .flat_map(|x| x.2.iter())
                .filter(|e| !seen.contains(&e.entry_id))
                .cloned()
                .collect();
            if st != t_state
                && let Err(e) = self.validate_state(&mut tx, &t_state)
            {
                validation.push(json!({"code": e.code}));
            }
        }
        let changes = Repo::new(self.store.as_mut(), &self.registry).diff_states_in(
            &t_state,
            &st,
            Some(&tx),
        )?;
        let mut counts: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        for ch in &changes {
            *counts
                .entry(s(ch, "class").unwrap_or_default().to_string())
                .or_default()
                .entry(s(ch, "op").unwrap_or_default().to_string())
                .or_default() += 1;
        }
        let summary = json!({"counts": counts});
        let preview = content_id(
            "ubos.chunk.v1",
            &json!({"changes": changes, "conflicts": conflicts, "validation": validation, "summary": summary}),
        )?;
        Ok(
            json!({"base": base, "source_head": source_head, "target_head": target_head, "changes": changes,
            "conflicts": conflicts, "validation": validation, "summary": summary, "preview": preview}),
        )
    }

    // ---- change sets (STD-VER-020, -021) ----

    fn cs_body(&mut self, oid: &str) -> Result<(String, String, Map<String, Value>), BpaError> {
        let cs = self
            .store
            .change_set_rec(oid)?
            .ok_or_else(|| not_found("unknown change set"))?;
        let target = s(&cs, "target").unwrap_or_default().to_string();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let st = repo.state_at(&target, None)?;
        let o = st
            .objects
            .get(oid)
            .ok_or_else(|| not_found("unknown change set"))?
            .clone();
        let body = repo.version(&o.head)?["body"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        Ok((target, o.head, body))
    }

    fn cs_write(&mut self, oid: &str, body: &Map<String, Value>) -> Result<(), BpaError> {
        let (target, head, _) = self.cs_body(oid)?;
        self.commit(&json!({"branch": target, "changes": [{"op": "update", "object_id": oid, "base": head, "body": body}]}))?;
        Ok(())
    }

    /// STD-VER-021: a changed preview invalidates every approval.
    fn cs_refresh(&mut self, oid: &str) -> Result<Map<String, Value>, BpaError> {
        let (_, _, mut body) = self.cs_body(oid)?;
        let state = s(&Value::Object(body.clone()), "state")
            .unwrap_or_default()
            .to_string();
        if !matches!(state.as_str(), "submitted" | "approved") {
            return Ok(body);
        }
        let source = body["source"].as_str().unwrap_or_default().to_string();
        let target = body["target"].as_str().unwrap_or_default().to_string();
        let preview = self.merge_preview(&source, &target)?["preview"].clone();
        if Some(&preview) != body.get("preview") {
            body.insert("state".into(), json!("submitted"));
            body.insert("preview".into(), preview);
            body.insert("approvals".into(), json!([]));
            self.cs_write(oid, &body)?;
        }
        Ok(body)
    }

    fn cs_result(body: &Map<String, Value>, extra: Option<Value>) -> Value {
        let mut out = json!({"state": body.get("state").cloned().unwrap_or(Value::Null)});
        for k in ["preview", "approvals", "merged_commit"] {
            if let Some(v) = body.get(k)
                && !v.is_null()
            {
                out[k] = v.clone();
            }
        }
        if let Some(e) = extra {
            for (k, v) in e.as_object().into_iter().flatten() {
                out[k] = v.clone();
            }
        }
        out
    }

    /// `change_set` (STD-VER-020, -021).
    pub fn change_set(&mut self, i: &Value) -> Result<Value, BpaError> {
        let action = s(i, "action").unwrap_or_default().to_string();
        let oid = s(i, "id").unwrap_or_default().to_string();
        if action == "open" {
            let source = s(i, "source").unwrap_or_default().to_string();
            let target = s(i, "target").unwrap_or_default().to_string();
            {
                let now = self.now.clone();
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                repo.head(&source)?;
                repo.check_active(&target, &now)?;
                if PROTECTED.contains(&repo.branch_meta(&source)?.kind.as_str()) {
                    return Err(BpaError::new(
                        "VER.MERGE_NOT_ALLOWED",
                        "the source of a change set is not protected",
                    ));
                }
                repo.merge_allowed(&source, &target, true)?;
            }
            if !names::matches("uuid", &oid) {
                return Err(invalid("a change set id is a UUID"));
            }
            let mut body = json!({"source": source, "target": target, "title": i.get("title").cloned().unwrap_or(Value::Null), "state": "open"});
            if let Some(d) = i.get("description")
                && !d.is_null()
            {
                body["description"] = d.clone();
            }
            self.commit(&json!({"branch": target, "changes": [{"op": "create", "object_id": oid, "class": "ChangeSet",
                "slug": format!("cs-{oid}"), "body": body}]}))?;
            self.store
                .put_change_set_rec(&oid, &json!({"target": target, "merge": null}))?;
            return Ok(Self::cs_result(body.as_object().expect("object"), None));
        }
        if action == "get" {
            let body = self.cs_refresh(&oid)?;
            return Ok(Self::cs_result(&body, None));
        }
        let mut body = self.cs_refresh(&oid)?;
        let state = body["state"].as_str().unwrap_or_default().to_string();
        let allowed: &[&str] = match action.as_str() {
            "submit" => &["open"],
            "approve" | "reject" => &["submitted"],
            "merge" => &["approved"],
            "abandon" => &["open", "submitted"],
            _ => return Err(invalid("unknown change set action")),
        };
        if !allowed.contains(&state.as_str()) {
            return Err(BpaError::new(
                "FLOW.TRANSITION_NOT_ALLOWED",
                format!("{action} from {state}"),
            ));
        }
        let source = body["source"].as_str().unwrap_or_default().to_string();
        let target = body["target"].as_str().unwrap_or_default().to_string();
        match action.as_str() {
            "submit" => {
                let p = self.merge_preview(&source, &target)?;
                if !p["conflicts"].as_array().is_some_and(Vec::is_empty) {
                    return Err(BpaError::with_details(
                        "VER.MERGE_CONFLICT",
                        json!({"conflicts": p["conflicts"]}),
                    ));
                }
                if let Some(v) = p["validation"].as_array().and_then(|a| a.first()) {
                    let code: &'static str = Box::leak(
                        s(v, "code")
                            .unwrap_or_default()
                            .to_string()
                            .into_boxed_str(),
                    );
                    return Err(BpaError::new(code, "the merge preview does not validate"));
                }
                body.insert("state".into(), json!("submitted"));
                body.insert("preview".into(), p["preview"].clone());
                body.insert("approvals".into(), json!([]));
            }
            "approve" | "reject" => {
                if i.get("approver").is_none_or(Value::is_null) {
                    return Err(BpaError::new(
                        "MODEL.DECLARATION_INCOMPLETE",
                        "an approval names its approver",
                    ));
                }
                if action == "reject" {
                    body.insert("state".into(), json!("rejected"));
                } else {
                    let mut approvals = body
                        .get("approvals")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    approvals.push(json!({"approver": i["approver"], "decision": "approve", "preview": body.get("preview"), "at": self.now}));
                    body.insert("approvals".into(), Value::Array(approvals));
                    body.insert("state".into(), json!("approved"));
                }
            }
            "abandon" => {
                body.insert("state".into(), json!("abandoned"));
            }
            _ => {
                let res = self.merge_into(&source, &target, i, Some(&oid))?;
                let head = Repo::new(self.store.as_mut(), &self.registry).head(&target)?;
                self.store
                    .put_change_set_rec(&oid, &json!({"target": target, "merge": head}))?;
                let merged_commit = match res.get("commit_id") {
                    Some(c) if !c.is_null() => c.clone(),
                    _ => json!(
                        Repo::new(self.store.as_mut(), &self.registry)
                            .commit(head)?
                            .commit_id
                    ),
                };
                body.insert("state".into(), json!("merged"));
                body.insert("merged_commit".into(), merged_commit);
                self.cs_write(&oid, &body)?;
                return Ok(Self::cs_result(&body, Some(json!({"cseq": res["cseq"]}))));
            }
        }
        self.cs_write(&oid, &body)?;
        Ok(Self::cs_result(&body, None))
    }

    // ---- tags and releases (STD-VER-060, -061) ----

    /// `tag` (STD-VER-060).
    pub fn tag(&mut self, i: &Value) -> Result<Value, BpaError> {
        let name = s(i, "name").unwrap_or_default().to_string();
        names::check_name("slug", &name)?;
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(&branch, i)?;
        if repo.tag(&name)?.is_some() {
            return Err(BpaError::new(
                "VER.TAG_IMMUTABLE",
                "tag names are never reused",
            ));
        }
        let c = repo.commit(snap.commit)?;
        let signer = s(i, "signer").unwrap_or(&self.principal).to_string();
        let signature = content_id(
            "ubos.chunk.v1",
            &json!({"name": name, "commit": c.commit_id, "signer": signer}),
        )?;
        self.store.put_tag_rec(
            &name,
            &json!({"commit": snap.commit, "commit_id": c.commit_id, "branch": branch, "signer": signer,
                    "deleted": false, "signature": signature}),
        )?;
        Ok(json!({"name": name, "branch": branch, "cseq": c.cseq, "commit_id": c.commit_id}))
    }

    /// `delete_tag` (STD-VER-060): a tombstone keeps the name.
    pub fn delete_tag(&mut self, name: &str) -> Result<Value, BpaError> {
        let mut t = self
            .store
            .tag_rec(name)?
            .filter(|t| t["deleted"] != json!(true))
            .ok_or_else(|| not_found("unknown tag"))?;
        t["deleted"] = json!(true);
        self.store.put_tag_rec(name, &t)?;
        Ok(Value::Bool(true))
    }

    /// `tag_info`.
    pub fn tag_info(&mut self, name: &str) -> Result<Value, BpaError> {
        let t = self
            .store
            .tag_rec(name)?
            .ok_or_else(|| not_found("unknown tag"))?;
        let c = Repo::new(self.store.as_mut(), &self.registry)
            .commit(t["commit"].as_u64().unwrap_or_default())?;
        Ok(
            json!({"name": name, "branch": t["branch"], "cseq": c.cseq, "commit_id": c.commit_id,
            "signer": t["signer"], "signature": t["signature"], "deleted": t["deleted"]}),
        )
    }

    /// `release` (STD-VER-061): a tag plus a `Release` object.
    pub fn release(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let version = s(i, "version").unwrap_or_default().to_string();
        if !is_semver(&version) {
            return Err(invalid("a release version is a semantic version"));
        }
        let tag = s(i, "tag").unwrap_or_default().to_string();
        {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let st = repo.state_at(&branch, None)?;
            for o in st.objects.values() {
                if o.class == "Release" {
                    let v = repo.version(&o.head)?;
                    if v["tombstone"] != json!(true) && v["body"]["version"] == json!(version) {
                        return Err(BpaError::new(
                            "MODEL.KEY_CONFLICT",
                            "the version is already released on this branch",
                        ));
                    }
                }
            }
            names::check_name("slug", &tag)?;
            if repo.tag(&tag)?.is_some() {
                return Err(BpaError::new(
                    "VER.TAG_IMMUTABLE",
                    "tag names are never reused",
                ));
            }
        }
        let mut sel = json!({});
        for k in ["time", "commit", "tx_time"] {
            if let Some(v) = i.get(k) {
                sel[k] = v.clone();
            }
        }
        let (tagged_commit, tagged_cseq, tagged_id) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let snap = repo.select(&branch, &sel)?;
            let c = repo.commit(snap.commit)?;
            (snap.commit, c.cseq, c.commit_id)
        };
        let mut body = json!({"version": version, "tag": tag});
        for k in ["notes", "change_sets"] {
            if let Some(v) = i.get(k)
                && !v.is_null()
            {
                body[k] = v.clone();
            }
        }
        let res = self.commit(
            &json!({"branch": branch, "changes": [{"op": "create", "object_id": i["object_id"],
            "class": "Release", "slug": tag, "body": body}]}),
        )?;
        let signer = s(i, "signer").unwrap_or(&self.principal).to_string();
        self.store.put_tag_rec(
            &tag,
            &json!({"commit": tagged_commit, "commit_id": tagged_id, "branch": branch, "signer": signer, "deleted": false,
                    "signature": content_id("ubos.chunk.v1", &json!({"name": tag, "commit": tagged_id}))?}),
        )?;
        Ok(
            json!({"tag": tag, "tagged": tagged_cseq, "cseq": res["cseq"], "versions": res["versions"]}),
        )
    }

    // ---- branches (STD-VER-011, -080) ----

    /// `branch_info`.
    pub fn branch_info(&mut self, branch: &str) -> Result<Value, BpaError> {
        let now = self.now.clone();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let m = repo.branch_meta(branch)?;
        let head = repo.head(branch)?;
        let hc = repo.commit(head)?;
        let from = repo.commit(m.created_from)?.commit_id;
        let mut out = json!({"name": branch, "kind": m.kind, "state": repo.branch_state(branch, &now)?,
            "created_from": from, "head": hc.commit_id, "cseq": hc.cseq});
        if let Some(exp) = repo.expires_at(branch)? {
            out["expires_at"] = json!(exp);
        }
        if let Some(pl) = m.parent_line {
            out["parent_line"] = json!({"branch": pl.branch, "mode": pl.mode});
            if let Some(pin) = pl.pin {
                out["parent_line"]["pin"] = json!(repo.commit(pin)?.cseq);
            }
        }
        Ok(out)
    }

    /// `set_parent_line` (STD-VER-080).
    pub fn set_parent_line(&mut self, branch: &str, pl: Option<&Value>) -> Result<Value, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut m = repo.branch_meta(branch)?;
        m.parent_line = match pl {
            Some(p) if !p.is_null() => Some(repo.parent_line(branch, p)?),
            _ => None,
        };
        repo.put_branch_meta(branch, &m)?;
        Ok(Value::Bool(true))
    }

    /// `commit_info` (STD-TIME-040).
    pub fn commit_info(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main");
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, i)?;
        repo.commit_info(snap.commit)
    }

    /// `tree_diff` (STD-VER-031).
    pub fn tree_diff(&mut self, a: &Value, b: &Value) -> Result<Value, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let (va, _) = repo.view(s(a, "branch").unwrap_or("main"), a)?;
        let (vb, _) = repo.view(s(b, "branch").unwrap_or("main"), b)?;
        Ok(json!({"keys": repo.tree_keys_diff(&va.state, &vb.state)?}))
    }

    // ---- revert (STD-VER-070, -071) ----

    #[allow(clippy::type_complexity)]
    fn revert_targets(&mut self, i: &Value) -> Result<(Vec<(String, u64)>, Vec<String>), BpaError> {
        if let Some(pid) = s(i, "process_id") {
            let rec = self
                .store
                .process_rec(pid)?
                .ok_or_else(|| not_found("unknown process"))?;
            // the process's own commits (STD-VER-071; dependents are reported, not
            // reverted), and for a bulk process the chunk commits of its children
            // (STD-TXN-033), which are its own work
            let mut commits: Vec<Value> = rec["commits"].as_array().cloned().unwrap_or_default();
            for child in rec["bulk_state"]["children"]
                .as_array()
                .into_iter()
                .flatten()
            {
                commits.push(json!([child["branch"], child["cseq"]]));
            }
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let mut targets = Vec::new();
            for c in &commits {
                let b = c[0].as_str().unwrap_or_default().to_string();
                let cid = repo.commit_at(&b, c[1].as_u64())?;
                if !targets.contains(&(b.clone(), cid)) {
                    targets.push((b, cid));
                }
            }
            return Ok((targets, vec![pid.to_string()]));
        }
        let targets = if let Some(cs) = s(i, "change_set") {
            let rec = self
                .store
                .change_set_rec(cs)?
                .ok_or_else(|| not_found("unknown change set"))?;
            let Some(m) = rec["merge"].as_u64() else {
                return Err(BpaError::new(
                    "FLOW.TRANSITION_NOT_ALLOWED",
                    "the change set is not merged",
                ));
            };
            vec![(s(&rec, "target").unwrap_or_default().to_string(), m)]
        } else {
            let branch = s(i, "branch").unwrap_or_default().to_string();
            let mut sel = json!({});
            for k in ["time", "commit", "tag"] {
                if let Some(v) = i.get(k)
                    && !v.is_null()
                {
                    sel[k] = v.clone();
                }
            }
            if sel.as_object().is_some_and(Map::is_empty) {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "revert names a commit",
                ));
            }
            let c = Repo::new(self.store.as_mut(), &self.registry)
                .select(&branch, &sel)?
                .commit;
            vec![(branch, c)]
        };
        let mut pids = Vec::new();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        for (_, c) in &targets {
            if let Some(p) = repo
                .commit(*c)?
                .extra
                .get("process_id")
                .and_then(Value::as_str)
            {
                pids.push(p.to_string());
            }
        }
        Ok((targets, pids))
    }

    /// `revert` (STD-VER-070, -071).
    pub fn revert(&mut self, i: &Value) -> Result<Value, BpaError> {
        let (targets, pids) = self.revert_targets(i)?;
        let roots: BTreeSet<String> = pids.iter().cloned().collect();
        let mut dependents = descendants(self.store.as_mut(), &roots)?;
        for p in &roots {
            dependents.remove(p);
        }
        let mut reverted: Vec<(String, u64)> = Vec::new();
        {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            for (b, c) in &targets {
                reverted.push((b.clone(), repo.commit(*c)?.cseq));
            }
        }
        reverted.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let reverted_json: Vec<Value> = reverted
            .iter()
            .map(|(b, c)| json!({"branch": b, "cseq": c}))
            .collect();
        if i.get("dry_run") == Some(&json!(true)) {
            return Ok(json!({"reverted": reverted_json, "dependents": dependents}));
        }
        let revert_resolutions: Vec<Value> = i
            .get("resolutions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut resolutions_used: Vec<Value> = Vec::new();
        let rev_ids = i
            .get("reversal_ids")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let now = self.now.clone();
        let today = now[..10].to_string();
        let mut next = self.next_process;
        let p = process(
            self.store.as_mut(),
            i.get("process"),
            "action",
            &mut next,
            &self.principal,
        )?;
        self.next_process = next;
        let branches: BTreeSet<String> = targets.iter().map(|t| t.0.clone()).collect();
        let mut plans: Vec<(Tx, Value, Vec<String>, Vec<String>)> = Vec::new();
        for branch in &branches {
            Repo::new(self.store.as_mut(), &self.registry).check_active(branch, &now)?;
            let reg = self.registry.clone();
            let mut repo = Repo::new(self.store.as_mut(), &reg);
            let mut tx = repo.begin(branch)?;
            repo.load_all(&mut tx)?;
            let t = tx.state.clone();
            let mut result =
                json!({"cseq": tx.cseq, "versions": {}, "entries": {}, "retroactive": []});
            let mut cids: Vec<(u64, u64)> = targets
                .iter()
                .filter(|(b, _)| b == branch)
                .map(|(_, c)| repo.commit(*c).map(|x| (x.cseq, *c)))
                .collect::<Result<_, _>>()?;
            cids.sort_by(|a, b| b.0.cmp(&a.0));
            let default_valid = {
                let (reg2, now2, today2) = (reg.clone(), now.clone(), today.clone());
                move |c: &str| match reg2.class(c).map(|c| c.valid_time.as_str()) {
                    Ok("instant") => now2.clone(),
                    _ => today2.clone(),
                }
            };
            let mut reverts = Vec::new();
            for (_, cid) in cids {
                let c = repo.commit(cid)?;
                reverts.push(c.commit_id.clone());
                let Some(parent) = c.parents.first().copied() else {
                    return Err(invalid("the first commit of a branch cannot be reverted"));
                };
                let cstate = repo.commit_state(cid)?;
                let pstate = repo.commit_state(parent)?;
                let current = tx.state.clone();
                let cseq = tx.cseq;
                // resolutions as in a merge (STD-VER-058): `source` keeps the reverted value,
                // `target` the later change (SCN-018 step 2)
                let mut res = Some(ubos_version::Resolver::new(&revert_resolutions)?);
                let (mut st, conflicts) = repo.merge_states(
                    &cstate,
                    &pstate,
                    &current,
                    false,
                    Some(cseq),
                    Some(branch),
                    false,
                    &mut res,
                    &mut tx,
                )?;
                if !conflicts.is_empty() {
                    return Err(BpaError::with_details(
                        "VER.MERGE_CONFLICT",
                        json!({"conflicts": unique(conflicts)}),
                    ));
                }
                if let Some(r) = res {
                    resolutions_used.extend(r.used.values().cloned());
                }
                // objects created by C are ended from the start of their timeline
                let missing: Vec<String> = current
                    .objects
                    .keys()
                    .filter(|k| !st.objects.contains_key(*k))
                    .cloned()
                    .collect();
                for oid in missing {
                    let o = current.objects[&oid].clone();
                    st.objects.insert(oid.clone(), o.clone());
                    let tombstone = repo.version_in(&tx, &o.head)?["tombstone"] == json!(true);
                    if let Some(first) = o.timeline.first().cloned()
                        && !tombstone
                    {
                        tx.state = st.clone();
                        let mut checks = Checks {
                            repo: &mut repo,
                            tx: &mut tx,
                            default_valid: &default_valid,
                        };
                        let mut ended = Vec::new();
                        checks.end_object_pub(&oid, &first.from, &mut result, &mut ended)?;
                        st = tx.state.clone();
                    }
                }
                tx.state = st;
                let before: BTreeSet<String> = pstate
                    .ledger
                    .iter()
                    .flat_map(|x| x.2.iter())
                    .map(|e| e.entry_id.clone())
                    .collect();
                let mut added: Vec<ubos_version::Entry> = cstate
                    .ledger
                    .iter()
                    .flat_map(|x| x.2.iter())
                    .filter(|e| !before.contains(&e.entry_id))
                    .cloned()
                    .collect();
                added.sort_by(|a, b| {
                    (a.cseq, a.seq, a.entry_id.clone()).cmp(&(b.cseq, b.seq, b.entry_id.clone()))
                });
                for e in added {
                    let Some(new_id) = rev_ids.get(&e.entry_id) else {
                        return Err(BpaError::new(
                            "MODEL.DECLARATION_INCOMPLETE",
                            format!("a reversal id is needed for entry {}", e.entry_id),
                        ));
                    };
                    let (id, seq) = repo.append(
                        &mut tx,
                        &json!({"entry_id": e.entry_id, "new_entry_id": new_id}),
                        "reverse",
                        &today,
                        &default_valid,
                        None,
                    )?;
                    result["entries"][id] = json!({"seq": seq});
                }
            }
            let changed: Vec<String> = tx
                .state
                .objects
                .iter()
                .filter(|(id, o)| {
                    t.objects
                        .get(*id)
                        .is_none_or(|x| x.head != o.head || x.timeline != o.timeline)
                })
                .map(|(id, _)| id.clone())
                .collect();
            for oid in &changed {
                result["versions"][oid] = json!(tx.state.objects[oid].head);
            }
            let classes: Vec<String> = changed
                .iter()
                .map(|id| tx.state.objects[id].class.clone())
                .collect();
            plans.push((tx, result, reverts, classes));
            let _ = t;
        }
        let mut out = Vec::new();
        let mut touched = Vec::new();
        for (mut tx, result, reverts, classes) in plans {
            let branch = tx.branch.clone();
            let t = Repo::new(self.store.as_mut(), &self.registry).state_at(&branch, None)?;
            let binding = self.rule_binding(&tx, &result["versions"], &result["entries"])?;
            self.validate_state(&mut tx, &t)?;
            tx.now = Some(now.clone());
            for oid in result["versions"]
                .as_object()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
            {
                Repo::op(&mut tx, &oid, "revert", None, None);
            }
            tx.extra.insert("rule_binding".into(), binding);
            tx.extra.insert("reverts".into(), json!(reverts));
            if p.explicit {
                tx.extra.insert("process_id".into(), json!(p.id));
            }
            tx.extra.insert(
                "appended".into(),
                json!(
                    result["entries"]
                        .as_object()
                        .map(|m| m.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default()
                ),
            );
            if !resolutions_used.is_empty() {
                tx.extra
                    .insert("resolutions".into(), json!(resolutions_used));
            }
            let cseq = tx.cseq;
            let head = tx.head;
            Repo::new(self.store.as_mut(), &self.registry).write(tx, vec![head])?;
            self.touch_branch(&branch)?;
            record(
                self.store.as_mut(),
                &p,
                "committed",
                Some((&branch, cseq)),
                0,
            )?;
            touched.extend(classes);
            out.push(json!({"branch": branch, "cseq": cseq, "versions": result["versions"], "entries": result["entries"]}));
        }
        self.instances.extend(touched);
        let mut out = json!({"reverted": reverted_json, "dependents": dependents, "commits": out});
        if !resolutions_used.is_empty() {
            out["resolutions"] = json!(resolutions_used);
        }
        Ok(out)
    }
}

/// Semantic Versioning 2.0.0 (STD-VER-061).
fn is_semver(v: &str) -> bool {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let r = R.get_or_init(|| {
        regex::Regex::new(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$").expect("grammar")
    });
    r.is_match(v)
}

/// Canonical key of a ledger key value.
pub fn ledger_key(v: &Value) -> Result<String, BpaError> {
    canonical(v)
}

/// A registry clone for closures that outlive a borrow of the engine.
pub fn registry_of(e: &Engine) -> Registry {
    e.registry.clone()
}
