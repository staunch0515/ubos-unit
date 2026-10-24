//! Periods (BPA 1.0-rc8, PH-2 step 3d, DEC-101): the transitions of period objects
//! (STD-KIND-025), closing with a close artefact and a signed tag (STD-KIND-026,
//! DSN-DVM-443), and reopening with a reason (DSN-DVM-444).

use crate::stateful::{Engine, SYSTEM_MARK};
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use ubos_types::decimal::Decimal;
use ubos_types::values::minor_unit;
use ubos_types::{BpaError, canonical, names};
use ubos_version::state::{Repo, Tx};

thread_local! {
    /// The transition the running period operation makes (`closed` is entered by
    /// `close_period` and left by `reopen_period` only).
    static OPERATION: Cell<Option<(&'static str, &'static str)>> = const { Cell::new(None) };
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// `{period class: [ledger classes it scopes]}`.
fn period_classes(reg: &ubos_model::Registry) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in reg.classes() {
        if let Some(p) = c
            .ledger
            .as_ref()
            .and_then(|l| l.get("period_scope"))
            .and_then(Value::as_str)
        {
            out.entry(p.to_string()).or_default().push(c.name.clone());
        }
    }
    out
}

/// STD-KIND-025 (1.0-rc8): an update of a period object moves its state between `open`
/// and `closing` only; `closed` is entered and left by the period operations.
pub(crate) fn check_transitions(
    repo: &mut Repo,
    tx: &mut Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    let periods = period_classes(repo.reg);
    if periods.is_empty() {
        return Ok(());
    }
    for (oid, vh) in written {
        let Some(old) = tx.loaded.get(oid).cloned().flatten() else {
            continue; // created by the commit: any state
        };
        if !periods.contains_key(&old.class) {
            continue;
        }
        let new = repo.version_in(tx, vh)?;
        if new["tombstone"] == json!(true) {
            continue;
        }
        let before = repo.version_in(tx, &old.head)?;
        let (a, b) = (
            s(&before["body"], "state").unwrap_or_default().to_string(),
            s(&new["body"], "state").unwrap_or_default().to_string(),
        );
        if a == b {
            continue;
        }
        let allowed = matches!(
            (a.as_str(), b.as_str()),
            ("open", "closing") | ("closing", "open")
        ) || OPERATION.with(|o| o.get()) == Some((leak(&a), leak(&b)));
        if !allowed {
            return Err(BpaError::with_details(
                "MODEL.CONSTRAINT_VIOLATED",
                json!({"period_transition": [a, b]}),
            ));
        }
    }
    Ok(())
}

/// The static name of a state, for comparing with the running operation.
fn leak(state: &str) -> &'static str {
    match state {
        "open" => "open",
        "closing" => "closing",
        "closed" => "closed",
        _ => "",
    }
}

/// Run `f` as the period operation making the transition `from → to`.
pub(crate) fn as_operation<T>(
    from: &'static str,
    to: &'static str,
    f: impl FnOnce() -> Result<T, BpaError>,
) -> Result<T, BpaError> {
    OPERATION.with(|o| o.set(Some((from, to))));
    let r = f();
    OPERATION.with(|o| o.set(None));
    r
}

/// The balance of one ledger key: its key, its number of entries and the sums per amount
/// field and currency.
type Balance = (Value, u64, BTreeMap<String, BTreeMap<String, Decimal>>);

fn add(acc: &mut BTreeMap<String, Decimal>, v: &Value) -> Result<(), BpaError> {
    let (k, amt) = match v {
        Value::Object(o) => (
            s(v, "currency").unwrap_or_default().to_string(),
            Decimal::parse(o.get("amount").and_then(Value::as_str).unwrap_or("0"))?,
        ),
        Value::String(t) => (String::new(), Decimal::parse(t)?),
        Value::Number(n) => (String::new(), Decimal::parse(&n.to_string())?),
        _ => return Ok(()),
    };
    let x = match acc.get(&k) {
        Some(x) => x.add(&amt),
        None => amt,
    };
    acc.insert(k, x);
    Ok(())
}

fn total(acc: &BTreeMap<String, Decimal>) -> Value {
    if acc.keys().all(String::is_empty) {
        return json!(acc[""].format(None));
    }
    Value::Array(
        acc.iter()
            .map(|(c, v)| json!({"amount": v.format(minor_unit(c)), "currency": c}))
            .collect(),
    )
}

impl Engine {
    /// The period object, its slug and its head body; `IAM.NOT_FOUND` or
    /// `MODEL.INVALID_VALUE` when it is not a live object of a period class.
    fn period(
        &mut self,
        branch: &str,
        oid: &str,
    ) -> Result<(String, String, Value, Vec<String>), BpaError> {
        let periods = period_classes(&self.registry);
        let (o, ..) = self.object_view(branch, &json!({}), oid)?;
        let Some(scoped) = periods.get(&o.class) else {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "not an object of a period class",
            ));
        };
        let v = Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?;
        if v["tombstone"] == json!(true) {
            return Err(BpaError::new("IAM.NOT_FOUND", "the period is ended"));
        }
        Ok((
            o.slug.clone(),
            o.head.clone(),
            v["body"].clone(),
            scoped.clone(),
        ))
    }

    /// STD-KIND-026: the balances per ledger key of the scoped classes over the entries
    /// whose posting date (the date of their commit's transaction time) is before `end`.
    fn close_balances(
        &mut self,
        branch: &str,
        head: u64,
        classes: &[String],
        end: &str,
    ) -> Result<Vec<Value>, BpaError> {
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let entries = r.entries_in(branch, head, None, None)?;
        let mut dates: BTreeMap<u64, String> = BTreeMap::new();
        let mut groups: BTreeMap<(String, String), Balance> = BTreeMap::new();
        for e in entries.iter().filter(|e| classes.contains(&e.class)) {
            let date = match dates.get(&e.cseq) {
                Some(d) => d.clone(),
                None => {
                    let id = r.commit_at(branch, Some(e.cseq))?;
                    let d = r.meta(id)?.tx_time.unwrap_or_default();
                    let d = d.get(..10).unwrap_or("9999-12-31").to_string();
                    dates.insert(e.cseq, d.clone());
                    d
                }
            };
            if date.as_str() >= end {
                continue;
            }
            let amounts: Vec<String> = self
                .registry
                .class(&e.class)?
                .ledger
                .as_ref()
                .and_then(|l| l.get("amounts"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            let g = groups
                .entry((e.class.clone(), canonical(&e.ledger_key)?))
                .or_insert_with(|| (e.ledger_key.clone(), 0, BTreeMap::new()));
            g.1 += 1;
            for a in amounts {
                if let Some(v) = e.body.get(&a) {
                    add(g.2.entry(a.clone()).or_default(), v)?;
                }
            }
        }
        Ok(groups
            .into_iter()
            .map(|((class, _), (key, n, sums))| {
                let amounts: Map<String, Value> = sums
                    .iter()
                    .filter(|(_, acc)| !acc.is_empty())
                    .map(|(a, acc)| (a.clone(), total(acc)))
                    .collect();
                json!({"class": class, "ledger_key": key, "entries": n, "amounts": amounts})
            })
            .collect())
    }

    /// Adapter operation `close_period` (STD-KIND-026): close a period in `closing` in one
    /// commit that writes the close artefact, then sign a tag on that commit.
    pub fn close_period(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let pid = s(i, "period").unwrap_or_default().to_string();
        let (slug, head_v, body, classes) = self.period(&branch, &pid)?;
        let state = s(&body, "state").unwrap_or_default().to_string();
        if state != "closing" {
            return Err(BpaError::with_details(
                "MODEL.CONSTRAINT_VIOLATED",
                json!({"period_transition": [state, "closed"]}),
            ));
        }
        let Some(aid) = s(i, "artefact_id").map(str::to_string) else {
            return Err(BpaError::new("MODEL.DECLARATION_INCOMPLETE", "artefact_id"));
        };
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let cseq = r.ordinal(&branch, None)?;
        let mut n = 1;
        for o in r.objects_in(&branch, cseq, Some("CloseArtefact"))?.values() {
            let v = r.version(&o.head)?;
            if v["tombstone"] != json!(true) && s(&v["body"], "period") == Some(pid.as_str()) {
                n += 1;
            }
        }
        let head = r.commit_at(&branch, None)?;
        let rec = r.commit(head)?;
        drop(r);
        let tag = format!("close-{slug}-{n}");
        names::check_name("slug", &tag)?;
        if self.store.tag_rec(&tag)?.is_some() {
            return Err(BpaError::new(
                "VER.TAG_IMMUTABLE",
                "tag names are never reused",
            ));
        }
        let end = s(&body, "end").unwrap_or_default().to_string();
        let balances = self.close_balances(&branch, cseq, &classes, &end)?;
        let art = json!({"period": pid, "period_slug": slug, "sequence": n, "start": body["start"],
                         "end": body["end"], "tag": tag,
                         "as_of": {"cseq": rec.cseq, "commit_id": rec.commit_id, "root": rec.root},
                         "balances": balances});
        let mut input = json!({"branch": branch, "changes": [
            {"op": "update", "object_id": pid, "base": head_v, "body": {"state": "closed"}},
            {"op": "create", "object_id": aid, "class": "CloseArtefact",
             "slug": format!("{slug}-close-{n}"), "body": art, SYSTEM_MARK: true}]});
        if let Some(p) = i.get("process") {
            input["process"] = p.clone();
        }
        let res = as_operation("closing", "closed", || {
            self.system_write = true;
            let r = self.commit(&input);
            self.system_write = false;
            r
        })?;
        let mut t = json!({"name": tag, "branch": branch});
        if let Some(sg) = i.get("signer") {
            t["signer"] = sg.clone();
        }
        self.tag(&t)?;
        Ok(
            json!({"cseq": res["cseq"], "commit_id": res["commit_id"], "artefact_id": aid, "tag": tag,
                  "sequence": n, "balances": balances}),
        )
    }

    /// Adapter operation `reopen_period` (STD-KIND-025): a closed period is reopened with a
    /// reason, recorded with the commit's process; its close artefacts stay.
    pub fn reopen_period(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let Some(reason) = s(i, "reason").filter(|r| !r.is_empty()).map(str::to_string) else {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "a reopening states its reason",
            ));
        };
        let pid = s(i, "period").unwrap_or_default().to_string();
        let (_, head_v, body, _) = self.period(&branch, &pid)?;
        let state = s(&body, "state").unwrap_or_default().to_string();
        if state != "closed" {
            return Err(BpaError::with_details(
                "MODEL.CONSTRAINT_VIOLATED",
                json!({"period_transition": [state, "open"]}),
            ));
        }
        let mut process = i.get("process").cloned().unwrap_or_else(|| json!({}));
        process["reason"] = json!(reason);
        let change =
            json!({"op": "update", "object_id": pid, "base": head_v, "body": {"state": "open"}});
        if let Some(pending) = self.governed_reopen(&branch, &pid, &reason, i, &process, &change)? {
            return Ok(pending);
        }
        let input = json!({"branch": branch, "process": process, "changes": [change]});
        let res = as_operation("closed", "open", || self.commit(&input))?;
        Ok(
            json!({"cseq": res["cseq"], "commit_id": res["commit_id"], "state": "open", "reason": reason}),
        )
    }

    /// STD-KIND-025 (1.0-rc10, DSN-DVM-444): with a command port `reopen` on the period's
    /// class, its decisions govern the reopening; `require` with `defer` stores a pending
    /// change (STD-TXN-060). None when the reopening goes ahead.
    fn governed_reopen(
        &mut self,
        branch: &str,
        pid: &str,
        reason: &str,
        i: &Value,
        process: &Value,
        change: &Value,
    ) -> Result<Option<Value>, BpaError> {
        let (o, ..) = self.object_view(branch, &json!({}), pid)?;
        let governed = self
            .registry
            .class(&o.class)?
            .ports
            .get("reopen")
            .is_some_and(|p| s(&p.decl, "kind") != Some("query"));
        if !governed {
            return Ok(None);
        }
        let mut q = json!({"branch": branch, "object_id": pid, "port": "reopen",
                           "params": {"reason": reason}});
        if let Some(c) = i.get("ctx") {
            q["ctx"] = c.clone();
        }
        let r = self.decide(&q)?;
        match s(&r, "outcome") {
            Some("deny") => return Err(BpaError::with_details("RULE.DECISION_DENIED", r)),
            Some("require") => {}
            _ => return Ok(None),
        }
        if i.get("defer").and_then(Value::as_bool) != Some(true) {
            return Err(BpaError::with_details("RULE.DECISION_REQUIRES", r));
        }
        let (Some(id), Some(initiator)) = (s(process, "process_id"), s(process, "initiator"))
        else {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "a deferred reopening names its process and initiator",
            ));
        };
        let pending = self.defer_changes(
            branch,
            id,
            initiator,
            std::slice::from_ref(change),
            &[(pid.to_string(), "reopen".to_string(), r)],
            json!({"process": {"reason": reason}, "period_op": true}),
        )?;
        Ok(Some(
            json!({"state": "closed", "reason": reason, "pending": pending}),
        ))
    }
}
