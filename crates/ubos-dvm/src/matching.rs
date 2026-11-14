//! Matching, match records and idempotent ingestion in the kernel (PH-2 step 3d, DEC-098):
//! the adapter operations `match` and `unmatch` of BPA 1.0-rc8 (STD-KIND-027, -028;
//! DSN-DVM-445) and the kernel operation `ingest` (DSN-DVM-446).
//!
//! The matching itself is the pure library `ubos_ledger::matching`; this module reads the
//! two sides and the match records of a match set from a branch and appends the records.
//! Match records are ledger entries of the genesis class `MatchRecord`: they are immutable,
//! an unmatch appends a record that supersedes the one it names, and no ordinary commit
//! can append them (`commit_on` rejects the class without the platform's mark).

use crate::stateful::{Engine, SYSTEM_MARK};
use serde_json::{Map, Value, json};
use ubos_ledger::matching::{self, Candidate, RECORD_CLASS, RECORD_KEY, Record};
use ubos_types::{BpaError, canonical};
use ubos_version::{Entry, Repo};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn incomplete(message: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", message)
}

impl Engine {
    /// The entries of a ledger class on a branch at its head, of one ledger key or of all.
    pub(crate) fn ledger_entries(
        &mut self,
        branch: &str,
        class: &str,
        key: Option<&str>,
    ) -> Result<Vec<Entry>, BpaError> {
        if self.needs_view(branch, &json!({}))? {
            let (v, _) = Repo::new(self.store.as_mut(), &self.registry).view(branch, &json!({}))?;
            return Ok(v
                .state
                .ledger
                .iter()
                .filter(|(c, k, _)| c == class && key.is_none_or(|x| k == x))
                .flat_map(|x| x.2.iter().cloned())
                .collect());
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.ordinal(branch, None)?;
        let entries = repo.entries_in(branch, head, key.map(|k| (class, k)), None)?;
        Ok(entries.into_iter().filter(|e| e.class == class).collect())
    }

    /// The candidates of a side `{class, ledger_key?}`: a ledger class and optionally one
    /// of its ledger keys (STD-KIND-027).
    fn side(&mut self, branch: &str, side: Option<&Value>) -> Result<Vec<Candidate>, BpaError> {
        let class = side
            .filter(|v| v.is_object())
            .and_then(|v| s(v, "class"))
            .ok_or_else(|| incomplete("a side names a ledger class"))?;
        if self.registry.class(class)?.kind.as_deref() != Some("ledger") {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "a side is a ledger class",
            ));
        }
        let key = match side.and_then(|v| v.get("ledger_key")) {
            None | Some(Value::Null) => None,
            Some(k) => Some(canonical(k)?),
        };
        Ok(self
            .ledger_entries(branch, class, key.as_deref())?
            .into_iter()
            .map(|e| Candidate {
                id: e.entry_id,
                valid_at: e.valid_at,
                body: e.body,
            })
            .collect())
    }

    /// The match records of a match set on a branch, in append order.
    fn match_records(&mut self, branch: &str, match_set: &Value) -> Result<Vec<Record>, BpaError> {
        let key = canonical(&json!({ RECORD_KEY: match_set }))?;
        Ok(self
            .ledger_entries(branch, RECORD_CLASS, Some(&key))?
            .into_iter()
            .map(|e| Record {
                id: e.entry_id,
                body: e.body,
            })
            .collect())
    }

    /// The actor of a match record: the initiator of the process.
    fn match_actor(&self, input: &Value) -> String {
        input
            .get("process")
            .and_then(|p| s(p, "initiator"))
            .filter(|a| !a.is_empty())
            .or_else(|| s(input, "principal"))
            .unwrap_or(&self.principal)
            .to_string()
    }

    /// Commit record appends as the platform, under the process of the operation.
    fn commit_records(
        &mut self,
        input: &Value,
        branch: &str,
        records: Vec<(String, Value)>,
    ) -> Result<Value, BpaError> {
        let changes: Vec<Value> = records
            .into_iter()
            .map(|(id, body)| {
                let key = json!({ RECORD_KEY: body[RECORD_KEY] });
                let mut ch = json!({"op": "append", "class": RECORD_CLASS, "entry_id": id,
                                    "ledger_key": key, "body": body});
                ch[SYSTEM_MARK] = json!(true);
                ch
            })
            .collect();
        let mut commit = json!({"branch": branch, "changes": changes});
        for k in ["process", "principal", "on_behalf_of"] {
            if let Some(v) = input.get(k).filter(|v| !v.is_null()) {
                commit[k] = v.clone();
            }
        }
        self.system_write = true;
        let r = self.commit(&commit);
        self.system_write = false;
        r
    }

    /// `match` (STD-KIND-027): run matching rules in priority order over the unmatched
    /// entries of two sides; with `commit`, append one match record per match
    /// (STD-KIND-028). Returns `{matches, breaks, cseq?}`.
    pub fn match_entries(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let match_set = s(input, "match_set")
            .filter(|m| !m.is_empty())
            .ok_or_else(|| incomplete("match_set"))?
            .to_string();
        let rules = matching::parse_rules(input.get("rules"))?;
        self.branch_meta(&branch)?;
        let left = self.side(&branch, input.get("left"))?;
        let right = self.side(&branch, input.get("right"))?;
        let records = self.match_records(&branch, &json!(match_set))?;
        let (used_left, used_right) = matching::used(&records);
        let outcome = matching::run(&rules, &left, &right, &used_left, &used_right)?;
        let mut matches: Vec<Value> = outcome
            .matches
            .iter()
            .map(|m| json!({"rule": m.rule, "rule_ref": m.rule_ref, "left": m.left, "right": m.right}))
            .collect();
        let mut out = Map::new();
        if input.get("commit").is_some_and(truthy) && !matches.is_empty() {
            let actor = self.match_actor(input);
            let mut appends = Vec::new();
            for (m, shown) in outcome.matches.iter().zip(matches.iter_mut()) {
                let body = matching::record_body(&match_set, m, &actor);
                let id = matching::record_id(&body)?;
                shown["entry_id"] = json!(id);
                appends.push((id, body));
            }
            let r = self.commit_records(input, &branch, appends)?;
            out.insert("cseq".into(), r["cseq"].clone());
        }
        out.insert("matches".into(), Value::Array(matches));
        out.insert(
            "breaks".into(),
            json!({"left": outcome.breaks_left, "right": outcome.breaks_right}),
        );
        Ok(Value::Object(out))
    }

    /// `unmatch` (STD-KIND-028): undo a match by appending a record that supersedes it; the
    /// undone record stays, and its entries become candidates again. Returns
    /// `{cseq, entry_id, unmatches}`.
    pub fn unmatch(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        self.branch_meta(&branch)?;
        let match_set = input.get("match_set").cloned().unwrap_or(Value::Null);
        let records = self.match_records(&branch, &match_set)?;
        let target = s(input, "entry_id").unwrap_or_default().to_string();
        let actor = self.match_actor(input);
        let body = matching::undo_body(&records, &match_set, &target, &actor, input.get("reason"))?;
        let id = matching::record_id(&body)?;
        let r = self.commit_records(input, &branch, vec![(id.clone(), body)])?;
        Ok(json!({"cseq": r["cseq"], "entry_id": id, "unmatches": target}))
    }

    /// `ingest` (DSN-DVM-446): append the lines of a delivered statement to a ledger class,
    /// each line once however often the statement is delivered.
    ///
    /// Input: `{branch?, class, ledger_key, source, statement, lines: [{line, body,
    /// valid_at?}], process?}`. Every line is one commit whose idempotency key is derived
    /// from `(source, ledger key, statement, line number, content hash)` (STD-TXN-040); the
    /// entry identifier is derived from the same key, so a line delivered again after its
    /// idempotency record expired, or by another initiator, is still recognised. Returns
    /// `{lines: [{line, entry_id, duplicate, cseq?}], ingested, duplicates}`. A line that
    /// fails stops the delivery with its error; the lines before it stay, and delivering
    /// the statement again continues after them.
    pub fn ingest(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let text = |k: &str| {
            s(input, k)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| incomplete(k))
        };
        let (class, source, statement) = (text("class")?, text("source")?, text("statement")?);
        let ledger_key = input
            .get("ledger_key")
            .filter(|k| k.is_object())
            .ok_or_else(|| incomplete("ledger_key"))?;
        let lines = input
            .get("lines")
            .and_then(Value::as_array)
            .ok_or_else(|| incomplete("lines"))?;
        let process = match input.get("process") {
            None | Some(Value::Null) => json!({}),
            Some(p) if p.is_object() => p.clone(),
            Some(_) => {
                return Err(BpaError::new("MODEL.INVALID_VALUE", "process is an object"));
            }
        };
        self.branch_meta(&branch)?;
        let (mut out, mut ingested, mut duplicates) = (Vec::new(), 0u64, 0u64);
        for l in lines {
            let number = l
                .get("line")
                .and_then(Value::as_u64)
                .ok_or_else(|| incomplete("a line has its number"))?;
            let body = l
                .get("body")
                .filter(|b| b.is_object())
                .ok_or_else(|| incomplete("a line has a body"))?;
            let valid_at = l.get("valid_at").cloned().unwrap_or(Value::Null);
            let content = json!({"class": class, "body": body, "valid_at": valid_at});
            let key = matching::ingestion_key(source, ledger_key, statement, number, &content)?;
            let id = matching::ingestion_id(&key)?;
            let mut change = json!({"op": "append", "class": class, "entry_id": id,
                                    "ledger_key": ledger_key, "body": body});
            if !valid_at.is_null() {
                change["valid_at"] = valid_at;
            }
            let mut p = process.clone();
            p["idempotency_key"] = json!(key);
            let mut commit = json!({"branch": branch, "changes": [change], "process": p});
            for k in ["principal", "on_behalf_of"] {
                if let Some(v) = input.get(k).filter(|v| !v.is_null()) {
                    commit[k] = v.clone();
                }
            }
            let head = Repo::new(self.store.as_mut(), &self.registry).ordinal(&branch, None)?;
            let mut row = json!({"line": number, "entry_id": id});
            let duplicate = match self.commit(&commit) {
                // a known key returns the stored result of the first delivery
                Ok(r) => {
                    row["cseq"] = r["cseq"].clone();
                    r["cseq"].as_u64().is_none_or(|c| c <= head)
                }
                // the derived entry exists although no idempotency record answers for it
                Err(e) if e.code == "TXN.ALREADY_EXISTS" => true,
                Err(e) => return Err(e),
            };
            row["duplicate"] = json!(duplicate);
            if duplicate {
                duplicates += 1;
            } else {
                ingested += 1;
            }
            out.push(row);
        }
        Ok(json!({"lines": out, "ingested": ingested, "duplicates": duplicates}))
    }
}

/// The truth of an optional input member, as the reference model reads it.
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
    use crate::Adapter;
    use serde_json::{Value, json};

    const SET: &str = "set-1";

    fn class(name: &str) -> Value {
        let field = |n: &str, t: &str, required: bool| json!({"name": n, "type": t, "label": n, "description": n, "required": required});
        json!({"name": name, "namespace": "acme.test", "parent": "DataObject", "kind": "ledger",
               "label": name, "description": name, "schema_version": 1,
               "fields": [field("holder", "text", true), field("reference", "text", false),
                          field("amount", "money", true)],
               "ledger": {"key": ["holder"], "amounts": ["amount"], "double_entry": false}})
    }

    fn adapter() -> Adapter {
        let mut a = Adapter::new().expect("adapter");
        a.execute(
            "load_genesis",
            &json!({"classes": [class("OwnLine"), class("FeedLine")]}),
        )
        .expect("genesis");
        a.execute("set_now", &json!({"now": "2026-03-15T10:00:00.000000Z"}))
            .expect("clock");
        a
    }

    fn id(n: u32) -> String {
        format!("01890a5d-ac96-7{n:03x}-8000-{n:012x}")
    }

    fn line(class: &str, n: u32, reference: &str, amount: &str) -> Value {
        json!({"op": "append", "class": class, "entry_id": id(n), "ledger_key": {"holder": "h-1"},
               "valid_at": "2026-03-10",
               "body": {"holder": "h-1", "reference": reference,
                        "amount": {"amount": amount, "currency": "USD"}}})
    }

    fn run(a: &mut Adapter, commit: bool) -> Value {
        a.execute(
            "match",
            &json!({"match_set": SET, "commit": commit,
                    "left": {"class": "OwnLine", "ledger_key": {"holder": "h-1"}},
                    "right": {"class": "FeedLine"},
                    "rules": [{"id": "exact", "kind": "one_to_one",
                               "equal": [["reference", "reference"]], "amount": ["amount", "amount"]}]}),
        )
        .expect("match")
    }

    fn records(a: &mut Adapter) -> Vec<Value> {
        a.execute(
            "read_ledger",
            &json!({"class": "MatchRecord", "ledger_key": {"match_set": SET}}),
        )
        .expect("records")["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn seeded() -> Adapter {
        let mut a = adapter();
        a.execute(
            "commit",
            &json!({"changes": [line("OwnLine", 10, "R-1", "100.00"), line("FeedLine", 20, "R-1", "100.00"),
                                line("OwnLine", 11, "R-2", "50.00"), line("FeedLine", 21, "R-3", "50.00")]}),
        )
        .expect("lines");
        a
    }

    #[test]
    fn a_run_is_repeatable_and_a_committed_run_writes_immutable_records() {
        let mut a = seeded();
        let first = run(&mut a, false);
        assert_eq!(
            first,
            run(&mut a, false),
            "matched twice, the results are identical"
        );
        assert_eq!(first["matches"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            first["breaks"],
            json!({"left": [id(11)], "right": [id(21)]})
        );
        assert!(first.get("cseq").is_none() && records(&mut a).is_empty());

        let done = run(&mut a, true);
        assert_eq!(done["cseq"], 2);
        let rec = records(&mut a);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0]["entry_id"], done["matches"][0]["entry_id"]);
        assert_eq!(rec[0]["body"]["left"], json!([id(10)]));
        assert_eq!(rec[0]["body"]["rule_ref"], done["matches"][0]["rule_ref"]);
        // matched entries are no candidates of a later run and are not reported as breaks
        let later = run(&mut a, true);
        assert_eq!(later["matches"], json!([]));
        assert_eq!(later["breaks"], first["breaks"]);
        assert!(later.get("cseq").is_none());
        // a record is appended by the matching operations only and cannot be corrected
        let body = rec[0]["body"].clone();
        let forged = a
            .execute(
                "commit",
                &json!({"changes": [{"op": "append", "class": "MatchRecord", "entry_id": id(30),
                                     "ledger_key": {"match_set": SET}, "body": body}]}),
            )
            .expect_err("forbidden");
        assert_eq!(forged.code, "LOGIC.FORBIDDEN_TARGET");
        let marked = a
            .execute(
                "commit",
                &json!({"changes": [{"op": "append", "class": "MatchRecord", "entry_id": id(30),
                                     "ledger_key": {"match_set": SET}, "body": rec[0]["body"],
                                     "__platform": true}]}),
            )
            .expect_err("the platform's mark is not accepted from outside");
        assert_eq!(marked.code, "MODEL.INVALID_VALUE");
        for op in ["reverse", "adjust"] {
            let corrected = a
                .execute(
                    "commit",
                    &json!({"changes": [{"op": op, "entry_id": rec[0]["entry_id"],
                                         "new_entry_id": id(31), "body": rec[0]["body"]}]}),
                )
                .expect_err("a record is not corrected by a commit");
            assert_eq!(corrected.code, "LOGIC.FORBIDDEN_TARGET");
        }
        assert_eq!(records(&mut a).len(), 1);
    }

    #[test]
    fn an_unmatch_appends_a_superseding_record() {
        let mut a = seeded();
        let done = run(&mut a, true);
        let target = done["matches"][0]["entry_id"].clone();
        let undo = a
            .execute(
                "unmatch",
                &json!({"match_set": SET, "entry_id": target, "reason": "wrong line"}),
            )
            .expect("unmatch");
        assert_eq!(undo["unmatches"], target);
        let rec = records(&mut a);
        assert_eq!(rec.len(), 2, "both records stay");
        assert_eq!(rec[0]["entry_id"], target);
        assert_eq!(rec[1]["entry_id"], undo["entry_id"]);
        assert_eq!(rec[1]["body"]["unmatches"], target);
        assert_eq!(rec[1]["body"]["reason"], "wrong line");
        assert_eq!(rec[1]["body"]["left"], rec[0]["body"]["left"]);
        // the entries are candidates again
        let again = run(&mut a, false);
        assert_eq!(again["matches"].as_array().map(Vec::len), Some(1));
        let code = |a: &mut Adapter, entry: &Value| {
            a.execute("unmatch", &json!({"match_set": SET, "entry_id": entry}))
                .expect_err("rejected")
                .code
        };
        assert_eq!(code(&mut a, &target), "MODEL.CONSTRAINT_VIOLATED");
        assert_eq!(code(&mut a, &undo["entry_id"]), "MODEL.CONSTRAINT_VIOLATED");
        assert_eq!(code(&mut a, &json!(id(99))), "IAM.NOT_FOUND");
    }

    #[test]
    fn malformed_inputs_are_rejected_before_anything_is_read() {
        let mut a = seeded();
        let base = json!({"match_set": SET, "left": {"class": "OwnLine"}, "right": {"class": "FeedLine"},
                          "rules": [{"id": "r", "kind": "one_to_one", "amount": ["amount", "amount"]}]});
        let code = |a: &mut Adapter, k: &str, v: Value| {
            let mut i = base.clone();
            i[k] = v;
            a.execute("match", &i).expect_err("rejected").code
        };
        assert_eq!(
            code(&mut a, "match_set", json!("")),
            "MODEL.DECLARATION_INCOMPLETE"
        );
        assert_eq!(
            code(&mut a, "rules", json!([])),
            "MODEL.DECLARATION_INCOMPLETE"
        );
        assert_eq!(
            code(&mut a, "left", json!("OwnLine")),
            "MODEL.DECLARATION_INCOMPLETE"
        );
        assert_eq!(
            code(&mut a, "right", json!({"class": "Logic"})),
            "MODEL.INVALID_VALUE"
        );
    }

    fn deliver(a: &mut Adapter, lines: Value) -> Value {
        a.execute(
            "ingest",
            &json!({"class": "FeedLine", "ledger_key": {"holder": "h-1"}, "source": "feed-a",
                    "statement": "2026-03", "lines": lines}),
        )
        .expect("ingest")
    }

    fn feed(n: u64, amount: &str) -> Value {
        json!({"line": n, "valid_at": "2026-03-10",
               "body": {"holder": "h-1", "reference": format!("R-{n}"),
                        "amount": {"amount": amount, "currency": "USD"}}})
    }

    fn held(a: &mut Adapter) -> usize {
        a.execute(
            "read_ledger",
            &json!({"class": "FeedLine", "ledger_key": {"holder": "h-1"}}),
        )
        .expect("ledger")["entries"]
            .as_array()
            .map_or(0, Vec::len)
    }

    /// DSN-DVM-446, acceptance 1: the same statement ingested three times — each line
    /// exists once.
    #[test]
    fn a_statement_delivered_three_times_holds_each_line_once() {
        let mut a = adapter();
        let statement = json!([feed(1, "100.00"), feed(2, "50.00"), feed(3, "7.25")]);
        let first = deliver(&mut a, statement.clone());
        assert_eq!(
            (first["ingested"].clone(), first["duplicates"].clone()),
            (json!(3), json!(0))
        );
        for _ in 0..2 {
            let again = deliver(&mut a, statement.clone());
            assert_eq!(
                (again["ingested"].clone(), again["duplicates"].clone()),
                (json!(0), json!(3))
            );
            for (x, y) in first["lines"]
                .as_array()
                .into_iter()
                .flatten()
                .zip(again["lines"].as_array().into_iter().flatten())
            {
                assert_eq!(x["entry_id"], y["entry_id"]);
                assert_eq!(
                    x["cseq"], y["cseq"],
                    "the stored result of the first delivery"
                );
                assert_eq!(y["duplicate"], true);
            }
        }
        assert_eq!(held(&mut a), 3);
        // a delivery that adds a line appends that line only
        let more = deliver(&mut a, json!([feed(1, "100.00"), feed(4, "1.00")]));
        assert_eq!(
            (more["ingested"].clone(), more["duplicates"].clone()),
            (json!(1), json!(1))
        );
        assert_eq!(held(&mut a), 4);
    }

    #[test]
    fn the_derived_entry_guards_a_line_without_an_idempotency_record() {
        let mut a = adapter();
        deliver(&mut a, json!([feed(1, "100.00")]));
        // another initiator has no idempotency record of the line (STD-TXN-040 scopes the
        // key by initiator): the derived entry identifier still recognises it
        let other = a
            .execute(
                "ingest",
                &json!({"class": "FeedLine", "ledger_key": {"holder": "h-1"}, "source": "feed-a",
                        "statement": "2026-03", "lines": [feed(1, "100.00")],
                        "process": {"initiator": "did:ubos:acme:01900000-0000-7000-8000-000000000009"}}),
            )
            .expect("ingest");
        assert_eq!(other["duplicates"], 1);
        assert_eq!(held(&mut a), 1);
        // every part of the key distinguishes: another statement, another content
        let mut next = json!({"class": "FeedLine", "ledger_key": {"holder": "h-1"}, "source": "feed-a",
                              "statement": "2026-04", "lines": [feed(1, "100.00")]});
        assert_eq!(a.execute("ingest", &next).expect("ingest")["ingested"], 1);
        next["lines"] = json!([feed(1, "100.01")]);
        assert_eq!(a.execute("ingest", &next).expect("ingest")["ingested"], 1);
        assert_eq!(held(&mut a), 3);
        // ingested lines are ordinary entries: they match
        a.execute(
            "commit",
            &json!({"changes": [line("OwnLine", 10, "R-1", "100.00")]}),
        )
        .expect("own line");
        let m = run(&mut a, false);
        assert_eq!(m["matches"].as_array().map(Vec::len), Some(1));
        assert_eq!(m["breaks"]["right"].as_array().map(Vec::len), Some(2));
    }
}
