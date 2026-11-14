//! Periods, close artefacts and restatement in the kernel (PH-2 step 3c, DEC-098): the
//! adapter operations `close_period`, `reopen_period` and `restate` of BPA 1.0-rc8
//! (STD-KIND-025, -026, STD-TIME-041, -042; DSN-DVM-322, -323, -442…-444).
//!
//! The rules themselves are the pure library `ubos_ledger::periods`; this module reads
//! period objects, entries and journals from a branch and makes the commits.
//!
//! - **Periods.** A period class is a class some ledger class names as its
//!   `period_scope`. An update moves a period between `open` and `closing` only
//!   (`commit_on` asks [`check_update`]); `closed` is entered by `close_period` and left
//!   by `reopen_period`, whose commits carry the platform's mark. The period rule itself —
//!   an append with a posting date in a closed period fails — is checked by
//!   `Repo::check_period` at every commit and merge.
//! - **Close artefacts.** `close_period` is one commit that closes the period and creates
//!   an object of the genesis System class `CloseArtefact` with the balances per ledger
//!   key at the commit the close extends, then a signed tag on the closing commit. The
//!   commit fails with `TXN.HEAD_MOVED` when the branch moved since the balances were
//!   read ([`check_artefact`]).
//! - **Governed reopen (DSN-DVM-444).** `reopen_period` is an ordinary commit of the
//!   period object: on a protected branch a period class with the write policy
//!   `change_set` is reopened through an approved change set only
//!   (`VER.CHANGE_SET_REQUIRED` otherwise). The reason is recorded with the commit.
//! - **Restatement.** `restate` re-runs the processes behind the impact set of a
//!   correcting commit on a correction branch; while a process is re-run, the engine's
//!   `logic_mode` selects the logic versions of the mode, and the mode is recorded in the
//!   derivation records. Postings the re-runs changed are reported as compensation
//!   proposals (DSN-DVM-323); nothing is appended for them.

use crate::stateful::{Engine, SYSTEM_MARK};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_ledger::periods::{
    self, ARTEFACT_CLASS, AsOf, CLOSED, Dating, Mode, OPEN, Period, Posted, Posting, Produced, Via,
};
use ubos_model::Registry;
use ubos_types::{BpaError, names};
use ubos_version::{Entry, ObjectState, Repo, Segment, at};

/// The member of a platform change that names the period operation making it.
pub(crate) const PERIOD_OP: &str = "__period";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn not_found(message: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", message)
}

/// The re-evaluation mode of the process a restatement re-runs (STD-TIME-041).
#[derive(Debug, Clone)]
pub(crate) struct LogicMode {
    /// The mode.
    pub(crate) mode: Mode,
    /// The valid date recomputed: the `asof` of the original process.
    pub(crate) date: String,
    /// The logic versions the original process ran.
    pub(crate) original: Vec<String>,
}

impl LogicMode {
    /// The version of a logic asset the re-run takes.
    pub(crate) fn version(&self, head: &str, timeline: &[Segment]) -> String {
        let covering = at(timeline, &self.date).map(|seg| seg.version.as_str());
        periods::logic_version(self.mode, head, covering, &self.original, |vh| {
            timeline.iter().any(|seg| seg.version == vh)
        })
    }
}

/// `{period class: [ledger classes it scopes]}` (STD-KIND-025).
fn period_scopes(reg: &Registry) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in reg.classes() {
        if let Some(scope) = c
            .ledger
            .as_ref()
            .and_then(|l| l.get("period_scope"))
            .and_then(Value::as_str)
        {
            out.entry(scope.to_string())
                .or_default()
                .push(c.name.clone());
        }
    }
    out
}

/// STD-KIND-025: an update of a period object moves its state between `open` and
/// `closing` only; a change of the platform that names its period operation makes the
/// transition of that operation.
pub(crate) fn check_update(
    reg: &Registry,
    class: &str,
    base: &Map<String, Value>,
    body: &Map<String, Value>,
    change: &Value,
) -> Result<(), BpaError> {
    let state =
        |b: &'_ Map<String, Value>| b.get("state").and_then(Value::as_str).map(str::to_string);
    let (old, new) = (state(base), state(body));
    if old == new || !period_scopes(reg).contains_key(class) {
        return Ok(());
    }
    let platform = change.get(SYSTEM_MARK) == Some(&json!(true));
    let via = match (platform, s(change, PERIOD_OP)) {
        (true, Some("close")) => Via::Close,
        (true, Some("reopen")) => Via::Reopen,
        _ => Via::Update,
    };
    periods::check_transition(old.as_deref(), new.as_deref(), via)
}

/// STD-KIND-026: a close artefact is created by the commit that extends the commit its
/// balances were read at; a branch that moved in between fails the close.
pub(crate) fn check_artefact(change: &Value, cseq: u64) -> Result<(), BpaError> {
    // a commit that is not the platform's is refused as a write to a System class
    if s(change, "class") != Some(ARTEFACT_CLASS) || change.get(SYSTEM_MARK) != Some(&json!(true)) {
        return Ok(());
    }
    let read_at = change["body"]["as_of"]["cseq"].as_u64();
    if read_at.and_then(|c| c.checked_add(1)) != Some(cseq) {
        return Err(BpaError::new(
            "TXN.HEAD_MOVED",
            "the branch moved since the balances of the close were read",
        ));
    }
    Ok(())
}

impl Engine {
    /// The objects of a branch at its head, of one class or of all.
    fn objects_of(
        &mut self,
        branch: &str,
        class: Option<&str>,
    ) -> Result<BTreeMap<String, ObjectState>, BpaError> {
        if self.needs_view(branch, &json!({}))? {
            let (v, _) = Repo::new(self.store.as_mut(), &self.registry).view(branch, &json!({}))?;
            return Ok(v
                .state
                .objects
                .into_iter()
                .filter(|(_, o)| class.is_none_or(|c| o.class == c))
                .collect());
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.ordinal(branch, None)?;
        repo.objects_in(branch, head, class)
    }

    /// A live period object of a branch: its identifier, state and body.
    fn period(
        &mut self,
        branch: &str,
        input: &Value,
    ) -> Result<(String, ObjectState, Value), BpaError> {
        let oid = s(input, "period").ok_or_else(|| not_found("unknown period"))?;
        let o = if self.needs_view(branch, &json!({}))? {
            self.objects_of(branch, None)?.remove(oid)
        } else {
            Repo::new(self.store.as_mut(), &self.registry).object_at(branch, None, oid)?
        };
        let o = o.ok_or_else(|| not_found("unknown period"))?;
        if !period_scopes(&self.registry).contains_key(&o.class) {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "not an object of a period class",
            ));
        }
        let v = self.store.version(&o.head)?;
        if v["tombstone"] == json!(true) {
            return Err(not_found("the period is ended"));
        }
        Ok((oid.to_string(), o, v["body"].clone()))
    }

    /// The bodies of the close artefacts of a period on a branch, in sequence order.
    fn artefacts_of(
        &mut self,
        branch: &str,
        period: &str,
    ) -> Result<Vec<(String, Value)>, BpaError> {
        let mut out = Vec::new();
        for (id, o) in self.objects_of(branch, Some(ARTEFACT_CLASS))? {
            let body = self.store.version(&o.head)?["body"].clone();
            if s(&body, "period") == Some(period) {
                out.push((id, body));
            }
        }
        out.sort_by_key(|(_, b)| b["sequence"].as_u64().unwrap_or_default());
        Ok(out)
    }

    /// The amount fields of ledger classes.
    fn amount_fields(&self, classes: &[String]) -> Result<BTreeMap<String, Vec<String>>, BpaError> {
        let mut out = BTreeMap::new();
        for c in classes {
            let fields = self
                .registry
                .class(c)?
                .ledger
                .as_ref()
                .and_then(|l| l.get("amounts"))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            out.insert(c.clone(), fields);
        }
        Ok(out)
    }

    /// The posting date of the entries appended at ordinal `cseq` of a branch: the date of
    /// the transaction time of that commit (STD-KIND-025).
    fn posting_date(
        &mut self,
        branch: &str,
        cseq: u64,
        cache: &mut BTreeMap<u64, Option<String>>,
    ) -> Result<Option<String>, BpaError> {
        if let Some(d) = cache.get(&cseq) {
            return Ok(d.clone());
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let id = repo.commit_at(branch, Some(cseq))?;
        let d = repo
            .commit(id)?
            .tx_time
            .map(|t| t.get(..10).unwrap_or(&t).to_string());
        cache.insert(cseq, d.clone());
        Ok(d)
    }

    /// Commit changes of a period operation as the platform, under the operation's process.
    fn commit_period(
        &mut self,
        input: &Value,
        branch: &str,
        mut changes: Vec<Value>,
        process: Option<Value>,
    ) -> Result<Value, BpaError> {
        for c in &mut changes {
            c[SYSTEM_MARK] = json!(true);
        }
        let mut commit = json!({"branch": branch, "changes": changes});
        if let Some(p) = process {
            commit["process"] = p;
        }
        for k in ["principal", "on_behalf_of"] {
            if let Some(v) = input.get(k).filter(|v| !v.is_null()) {
                commit[k] = v.clone();
            }
        }
        self.system_write = true;
        let r = self.commit(&commit);
        self.system_write = false;
        r
    }

    /// The commit identifier of a commit result, read from the branch when the result
    /// does not carry it.
    fn commit_id_of(&mut self, branch: &str, result: &Value) -> Result<Value, BpaError> {
        if let Some(id) = result.get("commit_id").filter(|v| v.is_string()) {
            return Ok(id.clone());
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let id = repo.commit_at(branch, result["cseq"].as_u64())?;
        Ok(json!(repo.meta(id)?.commit_id))
    }

    /// `close_period` (STD-KIND-026; DSN-DVM-442, -443): close a period in `closing`.
    ///
    /// Input: `{branch?, period, artefact_id, process?, signer?}`. One commit sets the
    /// period to `closed` and creates the close artefact — the period, the sequence of the
    /// close, the tag, the commit the close extends (`as_of`) and the balances per ledger
    /// key of the scoped classes over the entries posted before the period's end; a signed
    /// tag `close-<period slug>-<n>` then names the closing commit. Returns `{cseq,
    /// commit_id, artefact_id, tag, sequence, balances}`.
    pub fn close_period(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let now = self.now.clone();
        Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
        let (oid, o, body) = self.period(&branch, input)?;
        periods::check_close(s(&body, "state"))?;
        let aid = s(input, "artefact_id")
            .ok_or_else(|| BpaError::new("MODEL.DECLARATION_INCOMPLETE", "artefact_id"))?
            .to_string();
        let earlier = self.artefacts_of(&branch, &oid)?;
        let n = periods::next_sequence(earlier.iter().map(|(_, b)| b), &oid);
        let tag = periods::tag_name(&o.slug, n);
        names::check_name("slug", &tag)?;
        let as_of = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            if repo.tag(&tag)?.is_some() {
                return Err(BpaError::new(
                    "VER.TAG_IMMUTABLE",
                    "tag names are never reused",
                ));
            }
            let head = repo.head(&branch)?;
            let c = repo.commit(head)?;
            AsOf {
                cseq: c.cseq,
                commit_id: c.commit_id,
                root: c.root,
            }
        };
        let classes = period_scopes(&self.registry)
            .remove(&o.class)
            .unwrap_or_default();
        let amounts = self.amount_fields(&classes)?;
        let mut entries: Vec<(Entry, Option<String>)> = Vec::new();
        let mut dates = BTreeMap::new();
        for class in &classes {
            for e in self.ledger_entries(&branch, class, None)? {
                let d = self.posting_date(&branch, e.cseq, &mut dates)?;
                entries.push((e, d));
            }
        }
        let period = Period::of(&body);
        let posted: Vec<Posted<'_>> = entries
            .iter()
            .map(|(e, d)| Posted {
                class: &e.class,
                ledger_key: &e.ledger_key,
                posting_date: d.as_deref(),
                body: &e.body,
            })
            .collect();
        let balances = periods::balances(&posted, &amounts, &period.end)?;
        let artefact = periods::artefact_body(&oid, &o.slug, &period, n, &as_of, balances.clone());
        let changes = vec![
            json!({"op": "update", "object_id": oid, "base": o.head, "body": {"state": CLOSED},
                   PERIOD_OP: "close"}),
            json!({"op": "create", "object_id": aid, "class": ARTEFACT_CLASS,
                   "slug": periods::artefact_slug(&o.slug, n), "body": artefact}),
        ];
        let process = input.get("process").filter(|p| !p.is_null()).cloned();
        let res = self.commit_period(input, &branch, changes, process)?;
        let mut tag_input = json!({"name": tag, "branch": branch});
        if let Some(signer) = s(input, "signer").or_else(|| s(input, "principal")) {
            tag_input["signer"] = json!(signer);
        }
        self.tag(&tag_input)?;
        let commit_id = self.commit_id_of(&branch, &res)?;
        Ok(
            json!({"cseq": res["cseq"], "commit_id": commit_id, "artefact_id": aid, "tag": tag,
                  "sequence": n, "balances": balances}),
        )
    }

    /// `reopen_period` (STD-KIND-025; DSN-DVM-444): reopen a closed period with a reason.
    ///
    /// Input: `{branch?, period, reason, process?}`. One commit returns the period to
    /// `open`; the reason is recorded with the commit's process, and the close artefacts
    /// of the period stay. Returns `{cseq, commit_id, state, reason}` and, as `superseded`,
    /// the close artefact the reopening supersedes (`{artefact_id, tag, sequence}`).
    pub fn reopen_period(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let reason = periods::reopen_reason(input.get("reason"))?.to_string();
        let (oid, o, body) = self.period(&branch, input)?;
        periods::check_reopen(s(&body, "state"))?;
        let superseded = self.artefacts_of(&branch, &oid)?.pop();
        let mut process = match input.get("process") {
            Some(p) if p.is_object() => p.clone(),
            _ => json!({}),
        };
        process["reason"] = json!(reason);
        let changes = vec![
            json!({"op": "update", "object_id": oid, "base": o.head, "body": {"state": OPEN},
                         PERIOD_OP: "reopen"}),
        ];
        let res = self.commit_period(input, &branch, changes, Some(process))?;
        let commit_id = self.commit_id_of(&branch, &res)?;
        let mut out = json!({"cseq": res["cseq"], "commit_id": commit_id, "state": OPEN,
                             "reason": reason});
        if let Some((id, b)) = superseded {
            out["superseded"] =
                json!({"artefact_id": id, "tag": b["tag"], "sequence": b["sequence"]});
        }
        Ok(out)
    }

    /// The process that produced a result, from its derivation record.
    fn producer(&mut self, provenance_ref: Option<&str>) -> Result<Option<String>, BpaError> {
        let Some(rid) = provenance_ref else {
            return Ok(None);
        };
        Ok(self
            .store
            .derivation_rec(rid)?
            .and_then(|r| s(&r["record"], "process_id").map(str::to_string)))
    }

    /// The entries a set of commits of a branch appended, as postings.
    fn postings_of(&mut self, branch: &str, commits: &[u64]) -> Result<Vec<Posting>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.ordinal(branch, None)?;
        let mut out = Vec::new();
        for c in commits {
            let ids: Vec<String> = repo
                .commit(*c)?
                .extra
                .get("appended")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            for id in ids {
                for e in repo.entries_in(branch, head, None, Some(&id))? {
                    out.push(Posting {
                        entry_id: e.entry_id,
                        class: e.class,
                        ledger_key: e.ledger_key,
                        valid_at: e.valid_at,
                        body: e.body,
                    });
                }
            }
        }
        Ok(out)
    }

    /// `restate` (STD-TIME-041, -042; DSN-DVM-322, -323): restate the results a correcting
    /// commit made stale.
    ///
    /// Input: `{branch?, time, name, mode?, materiality?, compensation?}`: the commit at
    /// ordinal `time` of `branch` is the correcting commit, `name` the correction branch to
    /// create, `mode` the re-evaluation mode (`as_effective` by default), `materiality` an
    /// L1 boolean over `this` (the new body) and `old`, `compensation` the dating of the
    /// proposals (`original`, the default, or `current`).
    ///
    /// The processes that produced the impact set of the commit (STD-TIME-031) are re-run
    /// on the correction branch in the order of their first commits, each with its
    /// recorded invocation and the logic versions of the mode; a process without a
    /// recorded invocation is reported in `not_rerun`. The source branch is unchanged.
    /// Returns `{branch, mode, processes, not_rerun, changes, proposals}`: `processes` are
    /// `{process_id, cseq}` of the original processes and the commits of their re-runs,
    /// `changes` are `{object_id, class, old, new, material?}` of the objects the
    /// correction branch changed, and `proposals` the adjustments that compensate the
    /// postings the re-runs changed (`ubos_ledger::periods::proposals`).
    ///
    /// A re-run that fails stops the restatement with its error; the correction branch
    /// keeps the re-runs before it.
    pub fn restate(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let mode = Mode::parse(input.get("mode"))?;
        let name = periods::correction_branch(input.get("name"))?.to_string();
        let dating = Dating::parse(input.get("compensation"))?;
        let imp = self.impact(&branch, input.get("time").and_then(Value::as_u64))?;
        // the producing processes of the impacted versions and entries
        let mut results: Vec<Produced> = Vec::new();
        for v in imp["impacted"].as_array().into_iter().flatten() {
            let (oid, vh) = (
                s(v, "object_id").unwrap_or_default(),
                s(v, "version_hash").unwrap_or_default(),
            );
            let rid = s(&self.store.version(vh)?, "provenance_ref").map(str::to_string);
            if let Some(process) = self.producer(rid.as_deref())? {
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                let cseq =
                    crate::transactions::version_cseq(&mut repo, &branch, oid, vh, u64::MAX)?;
                results.push(Produced { process, cseq });
            }
        }
        let head = Repo::new(self.store.as_mut(), &self.registry).ordinal(&branch, None)?;
        let mut posted: Vec<Produced> = Vec::new();
        for id in imp["entries"].as_array().into_iter().flatten() {
            let found = Repo::new(self.store.as_mut(), &self.registry).entries_in(
                &branch,
                head,
                None,
                id.as_str(),
            )?;
            for e in found {
                if let Some(process) = self.producer(e.provenance_ref.as_deref())? {
                    posted.push(Produced {
                        process,
                        cseq: e.cseq,
                    });
                }
            }
        }
        // entries in the order of their commits
        posted.sort_by_key(|p| p.cseq);
        results.extend(posted);
        let mut journals: BTreeMap<String, Value> = BTreeMap::new();
        for r in &results {
            if !journals.contains_key(&r.process)
                && let Ok(j) = self.journal_full(&r.process)
                && j["invocation"].is_object()
            {
                journals.insert(r.process.clone(), j);
            }
        }
        let plan = periods::plan(&results, |p| journals.contains_key(p));
        let before = self.objects_of(&branch, None)?;
        self.other("create_branch", &json!({"name": name, "from": branch}))?;
        let mut processes = Vec::new();
        let (mut original, mut restated) = (Vec::new(), Vec::new());
        for pid in &plan.rerun {
            let j = &journals[pid];
            let mut inv = j["invocation"].clone();
            inv["branch"] = json!(name);
            let date = periods::recomputed_date(
                inv.get("ctx").and_then(|c| s(c, "asof")),
                s(&j["context"], "asof"),
                s(&j["context"], "now"),
            );
            self.logic_mode = Some(LogicMode {
                mode,
                date,
                original: j["logic"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            });
            let out = self.execute(&inv);
            self.logic_mode = None;
            let out = out?;
            let cseq = out.get("cseq").cloned().unwrap_or(Value::Null);
            // the postings of the original process and of its re-run (DSN-DVM-323)
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let mut commits = Vec::new();
            for id in j["outputs"]["commit_ids"].as_array().into_iter().flatten() {
                if let Ok(c) = repo.by_id(&branch, id.as_str().unwrap_or_default()) {
                    commits.push(c);
                }
            }
            let rerun_commit = match cseq.as_u64() {
                Some(c) => vec![repo.commit_at(&name, Some(c))?],
                None => Vec::new(),
            };
            original.extend(self.postings_of(&branch, &commits)?);
            restated.extend(self.postings_of(&name, &rerun_commit)?);
            processes.push(json!({"process_id": pid, "cseq": cseq}));
        }
        // old and new results of the objects the restatement wrote
        let after = self.objects_of(&name, None)?;
        let declarations = self.registry.declarations();
        let materiality = input.get("materiality").filter(|m| !m.is_null());
        let mut changes = Vec::new();
        for (oid, o) in &after {
            let old = before.get(oid);
            if old.is_some_and(|x| x.head == o.head) {
                continue;
            }
            let old_body = match old {
                Some(x) => Some(self.store.version(&x.head)?["body"].clone()),
                None => None,
            };
            let new_body = self.store.version(&o.head)?["body"].clone();
            let mut c = periods::change(oid, &o.class, old_body.as_ref(), &new_body);
            if let (Some(expr), Some(ob)) = (materiality, &old_body) {
                let r = ubos_expr::eval_expr(
                    &json!({"expr": expr, "classes": declarations, "class": o.class,
                            "this": new_body, "old": ob}),
                    None,
                )?;
                c["material"] = json!(truthy(&r["value"]));
            }
            changes.push(c);
        }
        let classes: BTreeSet<String> = original
            .iter()
            .chain(&restated)
            .map(|p| p.class.clone())
            .collect();
        let classes: Vec<String> = classes.into_iter().collect();
        let amounts = self.amount_fields(&classes)?;
        let today = self.now[..10].to_string();
        let proposals = periods::proposals(&original, &restated, &amounts, dating, &today)?;
        Ok(
            json!({"branch": name, "mode": mode.as_str(), "processes": processes,
                  "not_rerun": plan.not_rerun, "changes": changes, "proposals": proposals}),
        )
    }
}

/// The truth of a value, as the reference model reads it.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Adapter;
    use ubos_ledger::periods::CLOSING;

    fn field(name: &str, type_: &str, required: bool) -> Value {
        json!({"name": name, "type": type_, "label": name, "description": name, "required": required})
    }

    fn class(name: &str, kind: &str, fields: Vec<Value>) -> Value {
        json!({"name": name, "namespace": "acme.test", "parent": "DataObject", "kind": kind,
               "label": name, "description": name, "schema_version": 1, "fields": fields})
    }

    /// A period class, a ledger class it scopes and a ledger class it does not.
    fn classes(governed: bool) -> Vec<Value> {
        let mut span = class(
            "Span",
            "definition",
            vec![
                field("start", "date", true),
                field("end", "date", true),
                field("state", "text", true),
            ],
        );
        if governed {
            span["write_policy"] = json!("change_set");
        }
        let mut line = class(
            "Line",
            "ledger",
            vec![
                field("holder", "text", true),
                field("amount", "money", true),
            ],
        );
        line["ledger"] = json!({"key": ["holder"], "amounts": ["amount"], "double_entry": false,
                                "period_scope": "Span"});
        let mut free = class(
            "Move",
            "ledger",
            vec![
                field("item", "text", true),
                field("quantity", "decimal", true),
            ],
        );
        free["ledger"] = json!({"key": ["item"], "amounts": ["quantity"], "double_entry": false});
        let rate = class("Rate", "definition", vec![field("value", "decimal", true)]);
        let fee = class(
            "Fee",
            "definition",
            vec![
                field("rate", "decimal", true),
                field("method", "text", true),
            ],
        );
        vec![span, line, free, rate, fee]
    }

    fn adapter(governed: bool) -> Adapter {
        let mut a = Adapter::new().expect("adapter");
        a.execute("load_genesis", &json!({"classes": classes(governed)}))
            .expect("genesis");
        clock(&mut a, "2026-03-15T10:00:00.000000Z");
        a
    }

    fn clock(a: &mut Adapter, now: &str) {
        a.execute("set_now", &json!({"now": now})).expect("clock");
    }

    fn id(n: u32) -> String {
        format!("01890a5d-ac96-7{n:03x}-8000-{n:012x}")
    }

    const MARCH: u32 = 0x32;

    fn create_period(a: &mut Adapter, branch: &str, state: &str) {
        a.execute(
            "commit",
            &json!({"branch": branch, "changes": [{"op": "create", "class": "Span", "object_id": id(MARCH),
                    "slug": "2026-03", "body": {"start": "2026-03-01", "end": "2026-04-01", "state": state}}]}),
        )
        .expect("period");
    }

    fn head(a: &mut Adapter, branch: &str, oid: &str) -> Value {
        a.execute("read", &json!({"branch": branch, "object_id": oid}))
            .expect("read")
    }

    fn set_state(a: &mut Adapter, state: &str) -> Result<Value, BpaError> {
        let base = head(a, "main", &id(MARCH))["version_hash"].clone();
        a.execute(
            "commit",
            &json!({"changes": [{"op": "update", "object_id": id(MARCH), "base": base,
                                 "body": {"state": state}}]}),
        )
    }

    fn post(a: &mut Adapter, n: u32, holder: &str, amount: &str) -> Result<Value, BpaError> {
        a.execute(
            "commit",
            &json!({"changes": [{"op": "append", "class": "Line", "entry_id": id(n),
                    "ledger_key": {"holder": holder},
                    "body": {"holder": holder, "amount": {"amount": amount, "currency": "USD"}}}]}),
        )
    }

    fn close(a: &mut Adapter, artefact: u32) -> Result<Value, BpaError> {
        a.execute(
            "close_period",
            &json!({"period": id(MARCH), "artefact_id": id(artefact)}),
        )
    }

    fn code<T: std::fmt::Debug>(r: Result<T, BpaError>) -> &'static str {
        r.expect_err("an error").code
    }

    #[test]
    fn a_close_seals_the_balances_and_a_reopen_names_what_it_supersedes() {
        let mut a = adapter(false);
        create_period(&mut a, "main", OPEN);
        post(&mut a, 0x0a, "h-1", "100.00").expect("posting");
        post(&mut a, 0x0b, "h-1", "20.50").expect("posting");
        post(&mut a, 0x0c, "h-2", "-7.00").expect("posting");
        assert_eq!(code(close(&mut a, 0x3c)), "MODEL.CONSTRAINT_VIOLATED");
        set_state(&mut a, CLOSING).expect("soft close");
        // a soft close still takes postings (STD-KIND-025)
        post(&mut a, 0x0d, "h-2", "-3.00").expect("posting in closing");
        let closed = close(&mut a, 0x3c).expect("close");
        let balances = json!([
            {"class": "Line", "ledger_key": {"holder": "h-1"}, "entries": 2,
             "amounts": {"amount": [{"amount": "120.50", "currency": "USD"}]}},
            {"class": "Line", "ledger_key": {"holder": "h-2"}, "entries": 2,
             "amounts": {"amount": [{"amount": "-10.00", "currency": "USD"}]}},
        ]);
        assert_eq!(closed["balances"], balances);
        assert_eq!(closed["tag"], "close-2026-03-1");
        assert_eq!(closed["sequence"], 1);
        // DSN-DVM-443: the artefact names the commit the close extends and its state root
        let artefact = head(&mut a, "main", &id(0x3c));
        assert_eq!(artefact["slug"], "2026-03-close-1");
        assert_eq!(artefact["body"]["balances"], balances);
        let before = closed["cseq"].as_u64().expect("cseq") - 1;
        assert_eq!(artefact["body"]["as_of"]["cseq"], before);
        assert!(artefact["body"]["as_of"]["commit_id"].is_string());
        assert!(artefact["body"]["as_of"]["root"].is_string());
        let tag = a
            .execute("tag_info", &json!({"name": "close-2026-03-1"}))
            .expect("tag");
        assert_eq!(tag["cseq"], closed["cseq"]);
        assert_eq!(tag["commit_id"], closed["commit_id"]);
        assert!(tag["signature"].is_string());
        // closed: no posting, no direct way out
        assert_eq!(
            code(post(&mut a, 0x0e, "h-1", "1.00")),
            "LEDG.PERIOD_CLOSED"
        );
        assert_eq!(code(set_state(&mut a, OPEN)), "MODEL.CONSTRAINT_VIOLATED");
        assert_eq!(
            code(set_state(&mut a, CLOSING)),
            "MODEL.CONSTRAINT_VIOLATED"
        );
        assert_eq!(code(close(&mut a, 0x3d)), "MODEL.CONSTRAINT_VIOLATED");
        // DSN-DVM-444: the reopening records its reason and names the artefact it supersedes
        let reopened = a
            .execute(
                "reopen_period",
                &json!({"period": id(MARCH), "reason": "late invoice"}),
            )
            .expect("reopen");
        assert_eq!(reopened["state"], OPEN);
        assert_eq!(reopened["reason"], "late invoice");
        assert_eq!(
            reopened["superseded"],
            json!({"artefact_id": id(0x3c), "tag": "close-2026-03-1", "sequence": 1})
        );
        post(&mut a, 0x0e, "h-1", "1.00").expect("posting after the reopening");
        set_state(&mut a, CLOSING).expect("soft close");
        let again = close(&mut a, 0x3d).expect("second close");
        assert_eq!(again["sequence"], 2);
        assert_eq!(again["tag"], "close-2026-03-2");
        assert_eq!(
            again["balances"][0]["amounts"]["amount"],
            json!([{"amount": "121.50", "currency": "USD"}])
        );
        // the first artefact keeps the figures as closed
        assert_eq!(
            head(&mut a, "main", &id(0x3c))["body"]["balances"],
            balances
        );
    }

    #[test]
    fn the_balances_count_what_was_posted_before_the_end() {
        let mut a = adapter(false);
        create_period(&mut a, "main", OPEN);
        post(&mut a, 0x0a, "h-1", "100.00").expect("posting in March");
        // posted in April: outside the March close, whatever its valid time
        clock(&mut a, "2026-04-02T09:00:00.000000Z");
        a.execute(
            "commit",
            &json!({"changes": [{"op": "append", "class": "Line", "entry_id": id(0x0b),
                    "ledger_key": {"holder": "h-1"}, "valid_at": "2026-03-20",
                    "body": {"holder": "h-1", "amount": {"amount": "5.00", "currency": "USD"}}},
                    {"op": "append", "class": "Move", "entry_id": id(0x0c), "ledger_key": {"item": "bolt"},
                     "body": {"item": "bolt", "quantity": "3"}}]}),
        )
        .expect("posting in April");
        set_state(&mut a, CLOSING).expect("soft close");
        let closed = close(&mut a, 0x3c).expect("close");
        // only the scoped class, only the entries posted before the end
        assert_eq!(
            closed["balances"],
            json!([{"class": "Line", "ledger_key": {"holder": "h-1"}, "entries": 1,
                    "amounts": {"amount": [{"amount": "100.00", "currency": "USD"}]}}])
        );
    }

    #[test]
    fn only_the_period_operations_make_their_transitions() {
        let mut a = adapter(false);
        create_period(&mut a, "main", CLOSING);
        let base = head(&mut a, "main", &id(MARCH))["version_hash"].clone();
        // a commit cannot pose as a period operation
        for extra in [
            json!({SYSTEM_MARK: true, PERIOD_OP: "close"}),
            json!({PERIOD_OP: "close"}),
        ] {
            let mut change = json!({"op": "update", "object_id": id(MARCH), "base": base,
                                    "body": {"state": CLOSED}});
            for (k, v) in extra.as_object().expect("object") {
                change[k] = v.clone();
            }
            let r = a.execute("commit", &json!({"changes": [change]}));
            assert!(
                matches!(code(r), "MODEL.INVALID_VALUE" | "MODEL.CONSTRAINT_VIOLATED"),
                "{extra}"
            );
        }
        assert_eq!(head(&mut a, "main", &id(MARCH))["body"]["state"], CLOSING);
        // an update that keeps the state changes other fields freely
        a.execute(
            "commit",
            &json!({"changes": [{"op": "update", "object_id": id(MARCH), "base": base,
                                 "body": {"end": "2026-04-02"}}]}),
        )
        .expect("update of another field");
        // the operations name a live object of a period class
        let r = a.execute(
            "close_period",
            &json!({"period": id(0xfff), "artefact_id": id(0x3c)}),
        );
        assert_eq!(code(r), "IAM.NOT_FOUND");
        a.execute(
            "commit",
            &json!({"changes": [{"op": "create", "class": "Rate", "object_id": id(1), "slug": "r-1",
                                 "body": {"value": "1"}}]}),
        )
        .expect("rate");
        let r = a.execute("reopen_period", &json!({"period": id(1), "reason": "x"}));
        assert_eq!(code(r), "MODEL.INVALID_VALUE");
        let r = a.execute("close_period", &json!({"period": id(MARCH)}));
        assert_eq!(code(r), "MODEL.DECLARATION_INCOMPLETE");
        // artefacts are the platform's: no commit creates one
        let r = a.execute(
            "commit",
            &json!({"changes": [{"op": "create", "class": ARTEFACT_CLASS, "object_id": id(0x3c),
                                 "slug": "fake", "body": {}}]}),
        );
        assert_eq!(code(r), "LOGIC.FORBIDDEN_TARGET");
    }

    #[test]
    fn a_close_extends_the_commit_its_balances_were_read_at() {
        let artefact = |cseq: u64, platform: bool| {
            let mut c =
                json!({"op": "create", "class": ARTEFACT_CLASS, "body": {"as_of": {"cseq": cseq}}});
            if platform {
                c[SYSTEM_MARK] = json!(true);
            }
            c
        };
        assert!(check_artefact(&artefact(4, true), 5).is_ok());
        // the branch moved between the read and the commit
        assert_eq!(
            code(check_artefact(&artefact(3, true), 5)),
            "TXN.HEAD_MOVED"
        );
        assert_eq!(
            code(check_artefact(&artefact(5, true), 5)),
            "TXN.HEAD_MOVED"
        );
        // other creates are not concerned
        assert!(check_artefact(&artefact(3, false), 5).is_ok());
        assert!(check_artefact(&json!({"op": "create", "class": "Span"}), 5).is_ok());
    }

    #[test]
    fn a_governed_period_is_reopened_through_an_approved_change_set() {
        // DSN-DVM-444: the period class has the write policy `change_set`
        let mut a = adapter(true);
        let merge = |a: &mut Adapter, n: u32, source: &str| {
            for (action, extra) in [
                (
                    "open",
                    json!({"source": source, "target": "main", "title": source}),
                ),
                ("submit", json!({})),
                (
                    "approve",
                    json!({"approver": "did:ubos:acme:01900000-0000-7000-8000-000000000009"}),
                ),
                ("merge", json!({})),
            ] {
                let mut i = json!({"action": action, "id": id(n)});
                for (k, v) in extra.as_object().expect("object") {
                    i[k] = v.clone();
                }
                a.execute("change_set", &i).expect(action);
            }
        };
        a.execute(
            "create_branch",
            &json!({"name": "feature/periods", "from": "main"}),
        )
        .expect("branch");
        create_period(&mut a, "feature/periods", CLOSED);
        merge(&mut a, 0x70, "feature/periods");
        assert_eq!(head(&mut a, "main", &id(MARCH))["body"]["state"], CLOSED);
        assert_eq!(
            code(post(&mut a, 0x0a, "h-1", "1.00")),
            "LEDG.PERIOD_CLOSED"
        );
        // acceptance 1: a reopen without approval fails
        let direct = a.execute(
            "reopen_period",
            &json!({"period": id(MARCH), "reason": "late invoice"}),
        );
        assert_eq!(code(direct), "VER.CHANGE_SET_REQUIRED");
        assert_eq!(head(&mut a, "main", &id(MARCH))["body"]["state"], CLOSED);
        // the reopening on a working branch reaches main through the change set
        a.execute(
            "create_branch",
            &json!({"name": "feature/reopen", "from": "main"}),
        )
        .expect("branch");
        a.execute(
            "reopen_period",
            &json!({"branch": "feature/reopen", "period": id(MARCH), "reason": "late invoice"}),
        )
        .expect("reopen on the working branch");
        assert_eq!(head(&mut a, "main", &id(MARCH))["body"]["state"], CLOSED);
        merge(&mut a, 0x71, "feature/reopen");
        assert_eq!(head(&mut a, "main", &id(MARCH))["body"]["state"], OPEN);
        post(&mut a, 0x0a, "h-1", "1.00").expect("posting after the governed reopening");
    }

    // ---- restatement ----

    const LOGIC: u32 = 0x50;

    /// `compute(d, r, f)` sets a fee from a rate; `post(d, r)` appends a movement of the
    /// rate's value.
    fn source(version: u32) -> Value {
        json!({"functions": {
            "compute": {"params": ["d", "r", "f"], "body": [
                {"do": "read.get", "as": "r", "args": {"target": "params.r"}},
                {"do": "read.get", "args": {"target": "params.f"}},
                {"do": "commit.update", "args": {"target": "params.f",
                    "set": {"method": format!("\"v{version}\""), "rate": "r.fields.value"},
                    "timeline": {"from": "params.d", "op": "\"supersede_from\""}}},
                {"return": "1"}]},
            "post": {"params": ["d", "r"], "body": [
                {"do": "read.get", "as": "r", "args": {"target": "params.r"}},
                {"do": "commit.append", "as": "e", "args": {"class": "\"Move\"",
                    "ledger_key": {"item": "\"fees\""},
                    "fields": {"item": "\"fees\"", "quantity": "r.fields.value"},
                    "valid_at": "params.d"}},
                {"return": "1"}]}}})
    }

    /// A rate, two fees whose identifiers run against the order they are computed in, and
    /// the verified logic.
    fn restatable() -> Adapter {
        let mut a = adapter(false);
        clock(&mut a, "2026-01-05T09:00:00.000000Z");
        let valid = json!({"from": "2026-01-01"});
        a.execute(
            "commit",
            &json!({"changes": [
                {"op": "create", "class": "Rate", "object_id": id(1), "slug": "r-1",
                 "body": {"value": "1000"}, "valid": valid},
                {"op": "create", "class": "Fee", "object_id": id(0x22), "slug": "f-1",
                 "body": {"method": "none", "rate": "0"}, "valid": valid},
                {"op": "create", "class": "Fee", "object_id": id(0x21), "slug": "f-2",
                 "body": {"method": "none", "rate": "0"}, "valid": valid},
                {"op": "create", "class": "Logic", "object_id": id(LOGIC), "slug": "acme-fee",
                 "body": {"abi": "1.0", "capabilities": ["read", "commit"], "exports": ["compute", "post"],
                          "name": "acme.fee", "profile": "standard", "source": source(1),
                          "state": "committed", "tier": "host"}}]}),
        )
        .expect("data");
        verify(&mut a);
        a
    }

    fn verify(a: &mut Adapter) {
        a.execute("verify_asset", &json!({"object_id": id(LOGIC)}))
            .expect("verified");
    }

    fn run(a: &mut Adapter, pid: u32, function: &str, mut args: Vec<&str>) -> Value {
        args.insert(0, "2026-02-15");
        a.execute(
            "execute",
            &json!({"asset": "acme.fee", "function": function, "args": args,
                    "ctx": {"asof": "2026-02-15"}, "process": {"process_id": id(pid)}}),
        )
        .expect("process")
    }

    /// Correct the rate retroactively; returns the ordinal of the correcting commit.
    fn correct(a: &mut Adapter, value: &str) -> Value {
        let base = head(a, "main", &id(1))["version_hash"].clone();
        a.execute(
            "commit",
            &json!({"changes": [{"op": "update", "object_id": id(1), "base": base, "body": {"value": value},
                                 "valid": {"from": "2026-01-01", "kind": "supersede_from"}}]}),
        )
        .expect("correction")["cseq"]
            .clone()
    }

    fn moves(a: &mut Adapter, branch: &str) -> Vec<Value> {
        a.execute(
            "read_ledger",
            &json!({"branch": branch, "class": "Move", "ledger_key": {"item": "fees"}}),
        )
        .expect("ledger")["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn processes_are_rerun_in_the_order_of_their_first_commits() {
        let mut a = restatable();
        clock(&mut a, "2026-02-16T09:00:00.000000Z");
        // the later process writes the object with the smaller identifier
        run(
            &mut a,
            0x1f5,
            "compute",
            vec!["ubos://acme/Rate/r-1", "ubos://acme/Fee/f-1"],
        );
        run(
            &mut a,
            0x1f6,
            "compute",
            vec!["ubos://acme/Rate/r-1", "ubos://acme/Fee/f-2"],
        );
        clock(&mut a, "2026-03-05T09:00:00.000000Z");
        let fix = correct(&mut a, "1120");
        let head_before = head(&mut a, "main", &id(0x22))["version_hash"].clone();
        let r = a
            .execute(
                "restate",
                &json!({"name": "correction/rate", "time": fix,
                        "materiality": "this.rate - old.rate > 100"}),
            )
            .expect("restate");
        let pids: Vec<&str> = r["processes"]
            .as_array()
            .expect("processes")
            .iter()
            .filter_map(|p| p["process_id"].as_str())
            .collect();
        assert_eq!(pids, [id(0x1f5), id(0x1f6)]);
        assert_eq!(r["not_rerun"], json!([]));
        assert_eq!(r["mode"], "as_effective");
        // changes in object order, each with its materiality
        assert_eq!(
            r["changes"],
            json!([
                {"object_id": id(0x21), "class": "Fee", "old": {"method": "v1", "rate": "1000"},
                 "new": {"method": "v1", "rate": "1120"}, "material": true},
                {"object_id": id(0x22), "class": "Fee", "old": {"method": "v1", "rate": "1000"},
                 "new": {"method": "v1", "rate": "1120"}, "material": true},
            ])
        );
        assert_eq!(r["proposals"], json!([]));
        // the source branch is unchanged; the restated result keeps its valid time
        assert_eq!(head(&mut a, "main", &id(0x22))["version_hash"], head_before);
        let restated = a
            .execute(
                "read",
                &json!({"branch": "correction/rate", "object_id": id(0x22), "asof": "2026-02-20"}),
            )
            .expect("read");
        assert_eq!(restated["valid"]["from"], "2026-02-15");
        assert_eq!(restated["body"]["rate"], "1120");
    }

    #[test]
    fn each_mode_runs_its_logic_version_and_records_itself() {
        for (mode, method) in [
            ("as_originally", "v1"),
            ("as_effective", "v1"),
            ("current", "v2"),
        ] {
            let mut a = restatable();
            clock(&mut a, "2026-02-16T09:00:00.000000Z");
            run(
                &mut a,
                0x1f5,
                "compute",
                vec!["ubos://acme/Rate/r-1", "ubos://acme/Fee/f-1"],
            );
            // a second version of the logic, effective from March
            clock(&mut a, "2026-03-01T09:00:00.000000Z");
            let base = head(&mut a, "main", &id(LOGIC))["version_hash"].clone();
            a.execute(
                "commit",
                &json!({"changes": [{"op": "update", "object_id": id(LOGIC), "base": base,
                                     "body": {"source": source(2)}}]}),
            )
            .expect("new logic");
            verify(&mut a);
            clock(&mut a, "2026-03-05T09:00:00.000000Z");
            let fix = correct(&mut a, "1120");
            let r = a
                .execute(
                    "restate",
                    &json!({"name": "correction/rate", "time": fix, "mode": mode}),
                )
                .expect("restate");
            assert_eq!(
                r["changes"][0]["new"],
                json!({"method": method, "rate": "1120"}),
                "{mode}"
            );
            // STD-TIME-041: the mode is recorded in the derivation record of the result
            let d = a
                .execute(
                    "derivation",
                    &json!({"branch": "correction/rate", "object_id": id(0x22)}),
                )
                .expect("derivation");
            assert_eq!(d["mode"], mode);
            // a process run outside a restatement records the default again
            run(
                &mut a,
                0x1f7,
                "compute",
                vec!["ubos://acme/Rate/r-1", "ubos://acme/Fee/f-2"],
            );
            let d = a
                .execute("derivation", &json!({"object_id": id(0x21)}))
                .expect("derivation");
            assert_eq!(d["mode"], "as_effective");
        }
    }

    #[test]
    fn restated_postings_propose_their_compensation() {
        // DSN-DVM-323, acceptance 1: a restated fee that changes by +120
        let mut a = restatable();
        clock(&mut a, "2026-02-16T09:00:00.000000Z");
        run(&mut a, 0x1f5, "post", vec!["ubos://acme/Rate/r-1"]);
        let original = moves(&mut a, "main");
        assert_eq!(original.len(), 1);
        clock(&mut a, "2026-03-05T09:00:00.000000Z");
        let fix = correct(&mut a, "1120");
        let r = a
            .execute("restate", &json!({"name": "correction/rate", "time": fix}))
            .expect("restate");
        let restated = moves(&mut a, "correction/rate");
        assert_eq!(
            restated.len(),
            2,
            "the re-run posts on the correction branch"
        );
        assert_eq!(restated[1]["body"]["quantity"], "1120");
        assert_eq!(
            moves(&mut a, "main"),
            original,
            "the source ledger is unchanged"
        );
        assert_eq!(
            r["proposals"],
            json!([{"class": "Move", "ledger_key": {"item": "fees"}, "valid_at": "2026-02-15",
                    "amounts": {"quantity": "120"},
                    "original": [original[0]["entry_id"]], "restated": [restated[1]["entry_id"]]}])
        );
        // dated today on request; nothing is appended for a proposal
        let today = a
            .execute(
                "restate",
                &json!({"name": "correction/rate-2", "time": fix, "compensation": "current"}),
            )
            .expect("restate");
        assert_eq!(today["proposals"][0]["valid_at"], "2026-03-05");
        assert_eq!(today["proposals"][0]["amounts"], json!({"quantity": "120"}));
        assert_eq!(moves(&mut a, "correction/rate-2").len(), 2);
        let bad = a.execute(
            "restate",
            &json!({"name": "correction/rate-3", "time": fix, "compensation": "later"}),
        );
        assert_eq!(code(bad), "MODEL.INVALID_VALUE");
    }

    #[test]
    fn a_result_written_by_direct_input_is_not_rerun() {
        let mut a = restatable();
        clock(&mut a, "2026-02-16T09:00:00.000000Z");
        let rate = head(&mut a, "main", &id(1))["version_hash"].clone();
        let fee = head(&mut a, "main", &id(0x22))["version_hash"].clone();
        a.execute(
            "commit",
            &json!({"process": {"process_id": id(0x1f8)},
                    "changes": [{"op": "update", "object_id": id(0x22), "base": fee, "body": {"method": "manual"},
                                 "derivation": {"logic": ["logic:manual@1"],
                                                "inputs": [{"object": id(1), "asof": "2026-02-15", "version": rate}]}}]}),
        )
        .expect("direct input");
        clock(&mut a, "2026-03-05T09:00:00.000000Z");
        let fix = correct(&mut a, "1120");
        // a restatement names a correction branch and a known mode
        for bad in [
            json!({"name": "feature/rate", "time": fix}),
            json!({"name": "correction/rate", "time": fix, "mode": "latest"}),
        ] {
            assert_eq!(code(a.execute("restate", &bad)), "MODEL.INVALID_VALUE");
        }
        let r = a
            .execute("restate", &json!({"name": "correction/rate", "time": fix}))
            .expect("restate");
        assert_eq!(r["processes"], json!([]));
        assert_eq!(r["not_rerun"], json!([id(0x1f8)]));
        assert_eq!(r["changes"], json!([]));
        // the correction branch exists and is of its kind
        let info = a
            .execute("branch_info", &json!({"branch": "correction/rate"}))
            .expect("branch");
        assert_eq!(info["kind"], periods::CORRECTION_KIND);
    }
}
