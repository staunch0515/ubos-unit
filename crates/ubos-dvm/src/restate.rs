//! Restatement (PH-2 step 3f, DEC-103): the re-evaluation modes of logic assets
//! (STD-TIME-041, BPA 1.0-rc8, DSN-DVM-331), the re-run of the processes behind an impact
//! set on a correction branch (STD-TIME-042, DSN-DVM-322) and the compensation proposals of
//! their ledger postings (STD-TIME-043, BPA 1.0-rc9, DSN-DVM-323).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use ubos_types::decimal::Decimal;
use ubos_types::values::minor_unit;
use ubos_types::{BpaError, canonical};
use ubos_version::state::{ObjectState, Repo, at};

/// The logic version a re-run uses: `(mode, recomputed date, versions of the original run)`.
type Mode = (String, String, Vec<String>);

thread_local! {
    static MODE: RefCell<Option<Mode>> = const { RefCell::new(None) };
    /// The ledger appends of the running re-run (its restated postings).
    static CAPTURE: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// STD-TIME-041: the version of a logic asset a process runs — the head, or under a
/// restatement the version of its mode: the version whose timeline covers the recomputed
/// date (`as_effective`), the version the original process ran (`as_originally`) or the
/// head (`current`).
pub(crate) fn logic_version(o: &ObjectState) -> String {
    MODE.with(|m| match &*m.borrow() {
        None => o.head.clone(),
        Some((mode, _, _)) if mode == "current" => o.head.clone(),
        Some((mode, _, original)) if mode == "as_originally" => original
            .iter()
            .find(|vh| **vh == o.head || o.timeline.iter().any(|x| x.version == **vh))
            .cloned()
            .unwrap_or_else(|| o.head.clone()),
        Some((_, date, _)) => at(&o.timeline, date)
            .map(|x| x.version.clone())
            .unwrap_or_else(|| o.head.clone()),
    })
}

/// STD-TIME-043: during a re-run, a ledger append is captured as a restated posting
/// instead of being committed; `false` outside a re-run.
pub(crate) fn capture(posting: Value) -> bool {
    CAPTURE.with(|c| match c.borrow_mut().as_mut() {
        Some(list) => {
            list.push(posting);
            true
        }
        None => false,
    })
}

/// `(currency, amount)` of a decimal or money value.
fn amount(v: &Value) -> Result<(String, Decimal), BpaError> {
    match v {
        Value::Object(o) => Ok((
            s(v, "currency").unwrap_or_default().to_string(),
            Decimal::parse(o.get("amount").and_then(Value::as_str).unwrap_or("0"))?,
        )),
        Value::Number(n) => Ok((String::new(), Decimal::parse(&n.to_string())?)),
        Value::String(t) => Ok((String::new(), Decimal::parse(t)?)),
        _ => Ok((String::new(), Decimal::parse("0")?)),
    }
}

/// The sums of one ledger class, key and currency.
#[derive(Default)]
struct Sums {
    key: Value,
    old: BTreeMap<String, Decimal>,
    new: BTreeMap<String, Decimal>,
    corrects: Vec<String>,
    valid: Vec<String>,
}

impl Engine {
    fn amount_fields(&self, class: &str) -> Vec<String> {
        self.registry
            .class(class)
            .ok()
            .and_then(|c| c.ledger.clone())
            .and_then(|l| l.get("amounts").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    }

    /// Adapter operation `restate` (STD-TIME-042, with the modes of STD-TIME-041 and the
    /// proposals of STD-TIME-043).
    pub fn restate(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let name = s(i, "name").unwrap_or_default().to_string();
        let mode = s(i, "mode").unwrap_or("as_effective").to_string();
        if !matches!(mode.as_str(), "as_originally" | "as_effective" | "current") {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "mode is as_originally, as_effective or current",
            ));
        }
        if name.split('/').next() != Some("correction") {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "a restatement runs on a correction branch",
            ));
        }
        let policy = match i.get("proposals") {
            None | Some(Value::Null) => None,
            Some(p) => {
                let pol = s(p, "date_policy").unwrap_or("open_period").to_string();
                if !matches!(pol.as_str(), "open_period" | "original") {
                    return Err(BpaError::new(
                        "MODEL.INVALID_VALUE",
                        "date_policy is open_period or original",
                    ));
                }
                Some(pol)
            }
        };
        let imp = self.impact(&branch, i.get("time").and_then(Value::as_u64))?;
        // the derivation record of every derived output of the branch
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.head(&branch)?;
        let head_cseq = repo.meta(head)?.cseq;
        let mut rid_of: BTreeMap<String, String> = BTreeMap::new();
        let mut todo = vec![head];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(x) = todo.pop() {
            if !seen.insert(x) {
                continue;
            }
            let c = repo.commit(x)?;
            todo.extend(c.parents.iter().copied());
            for d in c
                .extra
                .get("derived")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let (kind, oid, h, rid) = (
                    d[0].as_str().unwrap_or_default(),
                    d[1].as_str().unwrap_or_default(),
                    d[2].as_str().unwrap_or_default(),
                    d[3].as_str().unwrap_or_default(),
                );
                let k = if kind == "object" { h } else { oid };
                rid_of.insert(k.to_string(), rid.to_string());
            }
        }
        let mut order: Vec<String> = Vec::new();
        let keys: Vec<String> = imp["impacted"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| s(v, "version_hash").map(str::to_string))
            .chain(
                imp["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_string)),
            )
            .collect();
        for k in keys {
            if let Some(rid) = rid_of.get(&k)
                && let Some(rec) = repo.store.derivation_rec(rid)?
                && let Some(pid) = s(&rec["record"], "process_id")
                && !order.iter().any(|p| p == pid)
            {
                order.push(pid.to_string());
            }
        }
        let before = repo.objects_in(&branch, head_cseq, None)?;
        drop(repo);
        let mut rerun = Vec::new();
        let mut manual = Vec::new();
        let mut journals = BTreeMap::new();
        for pid in &order {
            match self.journal_full(pid) {
                Ok(j) if j.get("invocation").is_some_and(|x| !x.is_null()) => {
                    journals.insert(pid.clone(), j);
                    rerun.push(pid.clone());
                }
                _ => manual.push(pid.clone()),
            }
        }
        self.other("create_branch", &json!({"name": name, "from": branch}))?;
        let mut processes = Vec::new();
        let mut postings: Vec<Value> = Vec::new();
        for pid in &rerun {
            let j = &journals[pid];
            let mut inv = j["invocation"].clone();
            inv["branch"] = json!(name);
            let d = s(&inv["ctx"], "asof")
                .or_else(|| s(&j["context"], "asof"))
                .or_else(|| s(&j["context"], "now"))
                .unwrap_or_default();
            let date = d.get(..10).unwrap_or(d).to_string();
            let original: Vec<String> = j["logic"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            MODE.with(|m| *m.borrow_mut() = Some((mode.clone(), date, original)));
            CAPTURE.with(|c| *c.borrow_mut() = Some(Vec::new()));
            let out = ubos_runtime::program::with_derivation_mode(&mode, || self.execute(&inv));
            MODE.with(|m| *m.borrow_mut() = None);
            let got = CAPTURE.with(|c| c.borrow_mut().take()).unwrap_or_default();
            let out = out?;
            for mut p in got {
                p["process_id"] = json!(pid);
                postings.push(p);
            }
            processes.push(
                json!({"process_id": pid, "cseq": out.get("cseq").cloned().unwrap_or(Value::Null)}),
            );
        }
        // the old and new bodies of the objects the restatement changed
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let top = repo.ordinal(&name, None)?;
        let after = repo.objects_in(&name, top, None)?;
        let mut changes = Vec::new();
        for (oid, o) in &after {
            let old = before.get(oid);
            if old.is_some_and(|x| x.head == o.head) {
                continue;
            }
            let ob = match old {
                Some(x) => repo.version(&x.head)?["body"].clone(),
                None => Value::Null,
            };
            let nb = repo.version(&o.head)?["body"].clone();
            let mut c = json!({"object_id": oid, "class": o.class, "old": ob, "new": nb});
            if let Some(e) = i.get("materiality").filter(|e| !e.is_null())
                && !ob.is_null()
            {
                let r = ubos_expr::eval_expr(
                    &json!({"expr": e, "classes": self.registry.declarations(), "class": o.class,
                            "this": nb, "old": ob}),
                    None,
                )?;
                c["material"] = json!(r["value"] == json!(true));
            }
            changes.push(c);
        }
        drop(repo);
        let mut out = json!({"branch": name, "mode": mode, "processes": processes, "not_rerun": manual,
                             "changes": changes});
        if let Some(pol) = policy {
            out["proposals"] = self.proposals(&branch, head_cseq, &rerun, &postings, &pol)?;
            out["postings"] = Value::Array(postings);
        }
        Ok(out)
    }

    /// STD-TIME-043: per ledger class, key and currency, the restated postings minus the
    /// entries the original processes appended.
    fn proposals(
        &mut self,
        branch: &str,
        cseq: u64,
        pids: &[String],
        postings: &[Value],
        policy: &str,
    ) -> Result<Value, BpaError> {
        let mut ids = std::collections::BTreeSet::new();
        for pid in pids {
            for c in self
                .store
                .process_rec(pid)?
                .and_then(|r| r["commits"].as_array().cloned())
                .unwrap_or_default()
            {
                if c[0].as_str() == Some(branch)
                    && let Some(n) = c[1].as_u64()
                {
                    let mut r = Repo::new(self.store.as_mut(), &self.registry);
                    let id = r.commit_at(branch, Some(n))?;
                    let rec = r.commit(id)?;
                    ids.extend(rec.ops.keys().cloned());
                }
            }
        }
        let mut groups: BTreeMap<(String, String, String), Sums> = BTreeMap::new();
        let acc = |groups: &mut BTreeMap<(String, String, String), Sums>,
                   this: &Engine,
                   class: &str,
                   key: &Value,
                   body: &Value,
                   valid_at: &str,
                   old: Option<&str>|
         -> Result<(), BpaError> {
            for a in this.amount_fields(class) {
                let Some(v) = body.get(&a).filter(|v| !v.is_null()) else {
                    continue;
                };
                let (cur, amt) = amount(v)?;
                let g = groups
                    .entry((class.to_string(), canonical(key)?, cur))
                    .or_insert_with(|| Sums {
                        key: key.clone(),
                        ..Sums::default()
                    });
                let side = if old.is_some() {
                    &mut g.old
                } else {
                    &mut g.new
                };
                let x = match side.get(&a) {
                    Some(x) => x.add(&amt),
                    None => amt,
                };
                side.insert(a.clone(), x);
                if let Some(eid) = old {
                    if !g.corrects.iter().any(|c| c == eid) {
                        g.corrects.push(eid.to_string());
                    }
                    g.valid.push(valid_at.to_string());
                }
            }
            Ok(())
        };
        let entries =
            Repo::new(self.store.as_mut(), &self.registry).entries_in(branch, cseq, None, None)?;
        for e in entries.iter().filter(|e| ids.contains(&e.entry_id)) {
            acc(
                &mut groups,
                self,
                &e.class,
                &e.ledger_key,
                &e.body,
                &e.valid_at,
                Some(&e.entry_id),
            )?;
        }
        for p in postings {
            acc(
                &mut groups,
                self,
                s(p, "class").unwrap_or_default(),
                &p["ledger_key"],
                &p["body"],
                s(p, "valid_at").unwrap_or_default(),
                None,
            )?;
        }
        let today = self.now[..10].to_string();
        let mut out = Vec::new();
        for ((class, _, cur), g) in groups {
            let mut fields: Vec<&String> = g.old.keys().chain(g.new.keys()).collect();
            fields.sort();
            fields.dedup();
            let zero = Decimal::parse("0")?;
            let mut amounts = Map::new();
            let mut nonzero = false;
            for f in fields {
                let d = g
                    .new
                    .get(f)
                    .cloned()
                    .unwrap_or_else(|| zero.clone())
                    .sub(g.old.get(f).unwrap_or(&zero));
                nonzero |= !d.is_zero();
                let v = if cur.is_empty() {
                    json!(d.format(None))
                } else {
                    json!({"amount": d.format(minor_unit(&cur)), "currency": cur})
                };
                amounts.insert(f.clone(), v);
            }
            if !nonzero {
                continue;
            }
            let valid = if policy == "open_period" {
                today.clone()
            } else {
                let from_postings: Vec<String> = postings
                    .iter()
                    .filter(|p| s(p, "class") == Some(class.as_str()))
                    .filter_map(|p| s(p, "valid_at").map(str::to_string))
                    .collect();
                let v = if g.valid.is_empty() {
                    &from_postings
                } else {
                    &g.valid
                };
                v.iter().max().cloned().unwrap_or_default()
            };
            out.push(
                json!({"class": class, "ledger_key": g.key, "valid_at": valid, "amounts": amounts,
                            "corrects": g.corrects}),
            );
        }
        Ok(Value::Array(out))
    }
}
