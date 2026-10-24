//! Matching and statements (PH-2 step 3e, DEC-102): deterministic matching of two ledger
//! sides (STD-KIND-027, BPA 1.0-rc8), immutable match records and unmatching (STD-KIND-028,
//! DSN-DVM-445) and the idempotent ingestion of statement lines (STD-KIND-030, BPA 1.0-rc9,
//! DSN-DVM-446).

use crate::stateful::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::decimal::Decimal;
use ubos_types::{BpaError, canonical, content_id};
use ubos_version::state::{Entry, Repo};

/// The genesis ledger class of match records.
pub const MATCH_CLASS: &str = "MatchRecord";
const KINDS: &[&str] = &["one_to_one", "one_to_many", "many_to_one"];

thread_local! {
    /// Set while `match` or `unmatch` commits: match records are appended by them only.
    static MATCHING: Cell<bool> = const { Cell::new(false) };
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn incomplete(m: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", m.to_string())
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m.to_string())
}

/// A UUID (version 8) derived from content: the first 32 hexadecimal digits of the SHA-256
/// of the canonical JSON of `parts`, with the version and variant bits set (STD-KIND-028,
/// STD-KIND-030).
pub fn derived_uuid(parts: &Value) -> Result<String, BpaError> {
    let h = hex(&Sha256::digest(canonical(parts)?.as_bytes()));
    let v = 8 | (u8::from_str_radix(&h[16..17], 16).unwrap_or_default() & 3);
    Ok(format!(
        "{}-{}-8{}-{:x}{}-{}",
        &h[..8],
        &h[8..12],
        &h[13..16],
        v,
        &h[17..20],
        &h[20..32]
    ))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// STD-KIND-028: match records are appended by `match` and `unmatch` only.
pub(crate) fn check_appended(appended: &[Entry]) -> Result<(), BpaError> {
    if !MATCHING.with(Cell::get) && appended.iter().any(|e| e.class == MATCH_CLASS) {
        return Err(BpaError::new("LOGIC.FORBIDDEN_TARGET", MATCH_CLASS));
    }
    Ok(())
}

/// STD-KIND-030: the entry id of a statement line, from its source, account, statement and
/// line fields named by the class's `ledger.statement`.
pub fn statement_id(
    spec: &serde_json::Map<String, Value>,
    body: &Value,
) -> Result<String, BpaError> {
    let vals: Vec<Value> = ["source", "account", "statement", "line"]
        .iter()
        .map(|k| {
            spec.get(*k)
                .and_then(Value::as_str)
                .and_then(|f| body.get(f))
                .cloned()
                .unwrap_or(Value::Null)
        })
        .collect();
    derived_uuid(&json!(["ubos.statement.v1", vals]))
}

/// `(currency, amount)` of a decimal or money value.
fn amount(v: &Value) -> Result<(String, Decimal), BpaError> {
    match v {
        Value::Object(o) => Ok((
            s(v, "currency").unwrap_or_default().to_string(),
            Decimal::parse(o.get("amount").and_then(Value::as_str).unwrap_or("0"))?,
        )),
        Value::String(t) => Ok((String::new(), Decimal::parse(t)?)),
        Value::Number(n) => Ok((String::new(), Decimal::parse(&n.to_string())?)),
        _ => Ok((String::new(), Decimal::parse("0")?)),
    }
}

fn days(a: &str, b: &str) -> i64 {
    let d = |x: &str| chrono::NaiveDate::parse_from_str(x.get(..10).unwrap_or(x), "%Y-%m-%d").ok();
    match (d(a), d(b)) {
        (Some(x), Some(y)) => (x - y).num_days().abs(),
        _ => i64::MAX,
    }
}

fn check_rule(r: &Value) -> Result<(), BpaError> {
    if s(r, "id").is_none() || !s(r, "kind").is_some_and(|k| KINDS.contains(&k)) {
        return Err(incomplete("a matching rule has an id and a kind"));
    }
    let amt = r.get("amount").and_then(Value::as_array);
    if !amt.is_some_and(|a| a.len() == 2 && a.iter().all(Value::is_string)) {
        return Err(incomplete(
            "a matching rule names the amount field of each side",
        ));
    }
    for p in r
        .get("equal")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !p
            .as_array()
            .is_some_and(|a| a.len() == 2 && a.iter().all(Value::is_string))
        {
            return Err(invalid("equal is a list of [left field, right field]"));
        }
    }
    if Decimal::parse(s(r, "tolerance").unwrap_or("0"))? < Decimal::parse("0")? {
        return Err(invalid("a tolerance is not negative"));
    }
    match r.get("window_days") {
        None => {}
        Some(w) if w.as_u64().is_some() => {}
        Some(_) => return Err(invalid("window_days is a non-negative integer")),
    }
    Ok(())
}

/// One rule as the matching applies it.
struct Rule<'a> {
    r: &'a Value,
    reference: String,
}

impl Rule<'_> {
    fn eq(&self, l: &Entry, x: &Entry) -> bool {
        self.r
            .get("equal")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .all(|p| {
                let (a, b) = (
                    p[0].as_str().unwrap_or_default(),
                    p[1].as_str().unwrap_or_default(),
                );
                l.body.get(a) == x.body.get(b)
            })
    }

    fn near(&self, l: &Entry, x: &Entry) -> bool {
        days(&l.valid_at, &x.valid_at) <= self.r["window_days"].as_i64().unwrap_or(0)
    }

    fn within(&self, ls: &[&Entry], rs: &[&Entry]) -> Result<bool, BpaError> {
        let (a, b) = (
            self.r["amount"][0].as_str().unwrap_or_default(),
            self.r["amount"][1].as_str().unwrap_or_default(),
        );
        let sum = |es: &[&Entry], f: &str| -> Result<BTreeMap<String, Decimal>, BpaError> {
            let mut out: BTreeMap<String, Decimal> = BTreeMap::new();
            for e in es {
                let (c, v) = amount(e.body.get(f).unwrap_or(&Value::Null))?;
                let x = match out.get(&c) {
                    Some(x) => x.add(&v),
                    None => v,
                };
                out.insert(c, x);
            }
            Ok(out)
        };
        let tl = sum(ls, a)?;
        let mut tr = sum(rs, b)?;
        if self.r.get("negate") == Some(&Value::Bool(true)) {
            tr = tr.into_iter().map(|(c, v)| (c, v.neg())).collect();
        }
        if tl.keys().collect::<Vec<_>>() != tr.keys().collect::<Vec<_>>() {
            return Ok(false);
        }
        let tol = Decimal::parse(s(self.r, "tolerance").unwrap_or("0"))?;
        Ok(tl.iter().all(|(c, x)| {
            let d = x.sub(&tr[c]);
            let d = if d < Decimal::parse("0").expect("zero") {
                d.neg()
            } else {
                d
            };
            d <= tol
        }))
    }
}

/// STD-KIND-027: apply the rules in priority order to the candidates of each side.
fn run_rules(
    rules: &[Value],
    left: &[Entry],
    right: &[Entry],
    used_l: &mut BTreeSet<String>,
    used_r: &mut BTreeSet<String>,
) -> Result<Vec<Value>, BpaError> {
    let mut matches = Vec::new();
    for r in rules {
        let rule = Rule {
            r,
            reference: content_id("ubos.chunk.v1", r)?,
        };
        let id = s(r, "id").unwrap_or_default();
        if s(r, "kind") == Some("one_to_one") {
            for l in left {
                if used_l.contains(&l.entry_id) {
                    continue;
                }
                for x in right {
                    if used_r.contains(&x.entry_id) || !rule.eq(l, x) || !rule.near(l, x) {
                        continue;
                    }
                    if rule.within(&[l], &[x])? {
                        used_l.insert(l.entry_id.clone());
                        used_r.insert(x.entry_id.clone());
                        matches.push(json!({"rule": id, "rule_ref": rule.reference,
                                            "left": [l.entry_id], "right": [x.entry_id]}));
                        break;
                    }
                }
            }
            continue;
        }
        let one_to_many = s(r, "kind") == Some("one_to_many");
        let (one, many) = if one_to_many {
            (left, right)
        } else {
            (right, left)
        };
        for x in one {
            let used_one = if one_to_many { &*used_l } else { &*used_r };
            if used_one.contains(&x.entry_id) {
                continue;
            }
            let used_many = if one_to_many { &*used_r } else { &*used_l };
            let group: Vec<&Entry> = many
                .iter()
                .filter(|y| !used_many.contains(&y.entry_id))
                .filter(|y| {
                    if one_to_many {
                        rule.eq(x, y) && rule.near(x, y)
                    } else {
                        rule.eq(y, x) && rule.near(y, x)
                    }
                })
                .collect();
            let ok = !group.is_empty()
                && if one_to_many {
                    rule.within(&[x], &group)?
                } else {
                    rule.within(&group, &[x])?
                };
            if !ok {
                continue;
            }
            let mut ids: Vec<String> = group.iter().map(|y| y.entry_id.clone()).collect();
            ids.sort();
            if one_to_many {
                used_l.insert(x.entry_id.clone());
                used_r.extend(ids.iter().cloned());
                matches.push(json!({"rule": id, "rule_ref": rule.reference, "left": [x.entry_id], "right": ids}));
            } else {
                used_r.insert(x.entry_id.clone());
                used_l.extend(ids.iter().cloned());
                matches.push(json!({"rule": id, "rule_ref": rule.reference, "left": ids, "right": [x.entry_id]}));
            }
        }
    }
    Ok(matches)
}

impl Engine {
    fn head_entries(&mut self, branch: &str) -> Result<Vec<Entry>, BpaError> {
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let head = r.ordinal(branch, None)?;
        r.entries_in(branch, head, None, None)
    }

    /// The entries of a side, ordered by `valid_at` then `entry_id` (STD-KIND-027).
    fn side(&mut self, entries: &[Entry], side: Option<&Value>) -> Result<Vec<Entry>, BpaError> {
        let Some(class) = side.and_then(|x| s(x, "class")) else {
            return Err(incomplete("a side names a ledger class"));
        };
        if self.registry.class(class)?.kind.as_deref() != Some("ledger") {
            return Err(invalid("a side is a ledger class"));
        }
        let key = match side.and_then(|x| x.get("ledger_key")) {
            Some(k) if !k.is_null() => Some(canonical(k)?),
            _ => None,
        };
        let mut out = Vec::new();
        for e in entries.iter().filter(|e| e.class == class) {
            if key
                .as_ref()
                .is_none_or(|k| canonical(&e.ledger_key).ok().as_ref() == Some(k))
            {
                out.push(e.clone());
            }
        }
        out.sort_by(|a, b| (&a.valid_at, &a.entry_id).cmp(&(&b.valid_at, &b.entry_id)));
        Ok(out)
    }

    /// `(active records, all records)` of a match set (STD-KIND-028).
    fn match_records(entries: &[Entry], set: &str) -> (Vec<Entry>, Vec<Entry>) {
        let q: Vec<Entry> = entries
            .iter()
            .filter(|e| e.class == MATCH_CLASS && s(&e.ledger_key, "match_set") == Some(set))
            .cloned()
            .collect();
        let undone: BTreeSet<&str> = q.iter().filter_map(|e| s(&e.body, "unmatches")).collect();
        let active = q
            .iter()
            .filter(|e| s(&e.body, "unmatches").is_none() && !undone.contains(e.entry_id.as_str()))
            .cloned()
            .collect();
        (active, q)
    }

    fn actor(&self, i: &Value) -> String {
        i.get("process")
            .and_then(|p| s(p, "initiator"))
            .map(str::to_string)
            .unwrap_or_else(|| self.principal.clone())
    }

    fn append_records(
        &mut self,
        branch: &str,
        i: &Value,
        changes: Vec<Value>,
    ) -> Result<Value, BpaError> {
        let mut input = json!({"branch": branch, "changes": changes});
        if let Some(p) = i.get("process") {
            input["process"] = p.clone();
        }
        MATCHING.with(|m| m.set(true));
        let r = self.commit(&input);
        MATCHING.with(|m| m.set(false));
        r
    }

    /// Adapter operation `match` (STD-KIND-027, -028).
    pub fn match_op(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let Some(set) = s(i, "match_set")
            .filter(|x| !x.is_empty())
            .map(str::to_string)
        else {
            return Err(incomplete("match_set"));
        };
        let rules = i
            .get("rules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if rules.is_empty() {
            return Err(incomplete("rules"));
        }
        for r in &rules {
            check_rule(r)?;
        }
        let ids: BTreeSet<&str> = rules.iter().filter_map(|r| s(r, "id")).collect();
        if ids.len() != rules.len() {
            return Err(invalid("rule ids are unique"));
        }
        let entries = self.head_entries(&branch)?;
        let left = self.side(&entries, i.get("left"))?;
        let right = self.side(&entries, i.get("right"))?;
        let (active, _) = Self::match_records(&entries, &set);
        let mut used_l: BTreeSet<String> = BTreeSet::new();
        let mut used_r: BTreeSet<String> = BTreeSet::new();
        for e in &active {
            for x in e.body["left"].as_array().into_iter().flatten() {
                used_l.insert(x.as_str().unwrap_or_default().to_string());
            }
            for x in e.body["right"].as_array().into_iter().flatten() {
                used_r.insert(x.as_str().unwrap_or_default().to_string());
            }
        }
        let mut matches = run_rules(&rules, &left, &right, &mut used_l, &mut used_r)?;
        let breaks = json!({
            "left": left.iter().filter(|e| !used_l.contains(&e.entry_id)).map(|e| e.entry_id.clone()).collect::<Vec<_>>(),
            "right": right.iter().filter(|e| !used_r.contains(&e.entry_id)).map(|e| e.entry_id.clone()).collect::<Vec<_>>(),
        });
        let mut out = json!({"breaks": breaks});
        if i.get("commit") == Some(&Value::Bool(true)) && !matches.is_empty() {
            let actor = self.actor(i);
            let mut changes = Vec::new();
            for m in matches.iter_mut() {
                let body = json!({"match_set": set, "left": m["left"], "right": m["right"], "rule": m["rule"],
                                  "rule_ref": m["rule_ref"], "actor": actor});
                let eid = derived_uuid(&json!(["ubos.match.v1", body]))?;
                m["entry_id"] = json!(eid);
                changes.push(
                    json!({"op": "append", "class": MATCH_CLASS, "entry_id": eid,
                                    "ledger_key": {"match_set": set}, "body": body}),
                );
            }
            let r = self.append_records(&branch, i, changes)?;
            out["cseq"] = r["cseq"].clone();
        }
        out["matches"] = Value::Array(matches);
        Ok(out)
    }

    /// Adapter operation `unmatch` (STD-KIND-028): a record that supersedes an active one.
    pub fn unmatch(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let set = s(i, "match_set").unwrap_or_default().to_string();
        let entries = self.head_entries(&branch)?;
        let (active, all) = Self::match_records(&entries, &set);
        let Some(target) = all
            .iter()
            .find(|e| Some(e.entry_id.as_str()) == s(i, "entry_id"))
        else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown match record"));
        };
        if !active.iter().any(|e| e.entry_id == target.entry_id) {
            return Err(BpaError::new(
                "MODEL.CONSTRAINT_VIOLATED",
                "the record is an unmatch or is already undone",
            ));
        }
        let mut body = json!({"match_set": set, "left": target.body["left"], "right": target.body["right"],
                              "rule": target.body.get("rule").cloned().unwrap_or(Value::Null),
                              "rule_ref": target.body.get("rule_ref").cloned().unwrap_or(Value::Null),
                              "actor": self.actor(i), "unmatches": target.entry_id});
        if let Some(r) = i.get("reason").filter(|r| !r.is_null()) {
            body["reason"] = r.clone();
        }
        let eid = derived_uuid(&json!(["ubos.match.v1", body]))?;
        let r = self.append_records(
            &branch,
            i,
            vec![
                json!({"op": "append", "class": MATCH_CLASS, "entry_id": eid,
                        "ledger_key": {"match_set": set}, "body": body}),
            ],
        )?;
        Ok(json!({"cseq": r["cseq"], "entry_id": eid, "unmatches": target.entry_id}))
    }

    /// Adapter operation `ingest_statement` (STD-KIND-030): append the new lines in one
    /// commit; a repeated line is a duplicate and a changed line fails the ingestion.
    pub fn ingest_statement(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let class = s(i, "class").unwrap_or_default().to_string();
        let Some(spec) = self
            .registry
            .class(&class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("statement"))
            .and_then(Value::as_object)
            .cloned()
        else {
            return Err(invalid("the class declares no statement lines"));
        };
        let known: BTreeMap<String, Value> = self
            .head_entries(&branch)?
            .into_iter()
            .filter(|e| e.class == class)
            .map(|e| (e.entry_id, e.body))
            .collect();
        let mut seen: BTreeMap<String, Value> = BTreeMap::new();
        let (mut appended, mut dups, mut changes) = (Vec::new(), Vec::new(), Vec::new());
        for line in i
            .get("lines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let body = Value::Object(self.registry.check_body(&class, &line["body"], false)?);
            let eid = statement_id(&spec, &body)?;
            if let Some(prior) = known.get(&eid).or_else(|| seen.get(&eid)) {
                if content_id("ubos.chunk.v1", prior)? != content_id("ubos.chunk.v1", &body)? {
                    return Err(BpaError::with_details(
                        "TXN.IDEMPOTENCY_MISMATCH",
                        json!({"entry_id": eid}),
                    ));
                }
                dups.push(eid);
                continue;
            }
            seen.insert(eid.clone(), body);
            let mut ch = json!({"op": "append", "class": class, "entry_id": eid,
                                "ledger_key": line["ledger_key"], "body": line["body"]});
            if let Some(v) = line.get("valid_at").filter(|v| !v.is_null()) {
                ch["valid_at"] = v.clone();
            }
            changes.push(ch);
            appended.push(eid);
        }
        let mut out = json!({"appended": appended, "duplicates": dups});
        if !changes.is_empty() {
            let mut input = json!({"branch": branch, "changes": changes});
            if let Some(p) = i.get("process") {
                input["process"] = p.clone();
            }
            let r = self.commit(&input)?;
            out["cseq"] = r["cseq"].clone();
            out["commit_id"] = r["commit_id"].clone();
        }
        Ok(out)
    }
}
