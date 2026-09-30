//! Branch state, commits and ledgers.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use ubos_model::Registry;
use ubos_model::types::decimal::Decimal;
use ubos_model::types::values::minor_unit;
use ubos_model::types::{BpaError, canonical, content_id, names};
use ubos_store::{Flush, Store};

/// A valid-time segment `[from, to)` of a timeline; `to = None` is an open end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// Start (date or instant).
    pub from: String,
    /// End, exclusive; `None` for an open end.
    pub to: Option<String>,
    /// Version hash valid in the segment.
    pub version: String,
}

/// `a < b` where `None` is +infinity.
pub fn lt(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(x), Some(y)) => x < y,
    }
}

/// The segment of a timeline that contains `asof`.
pub fn at<'a>(tl: &'a [Segment], asof: &str) -> Option<&'a Segment> {
    tl.iter()
        .find(|s| s.from.as_str() <= asof && lt(Some(asof), s.to.as_deref()))
}

/// State of a Definition, RawDocument or System object on a branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectState {
    /// Class name.
    pub class: String,
    /// Kind of the class.
    pub kind: String,
    /// Current slug.
    pub slug: String,
    /// Last version written on the branch.
    pub head: String,
    /// Valid-time segments, sorted and non-overlapping.
    pub timeline: Vec<Segment>,
}

/// A ledger entry on a branch (STD-KIND-020).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Entry identifier.
    pub entry_id: String,
    /// Ledger class.
    pub class: String,
    /// Ledger key values.
    pub ledger_key: Value,
    /// Business effect date or instant.
    pub valid_at: String,
    /// Field values.
    pub body: Value,
    /// Corrected entries.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub corrects: Option<Vec<String>>,
    /// `reversal` or `adjustment`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub correction_type: Option<String>,
    /// Double-entry group.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub posting_group: Option<String>,
    /// Branch sequence within the ledger key.
    pub seq: u64,
    /// Commit that made the entry visible on this branch.
    pub cseq: u64,
    /// Entry hash (sequence excluded).
    pub entry_hash: String,
    /// `{branch, seq}` where the entry was first appended, after merges.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_seq: Option<Value>,
}

/// The state of a branch at a commit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Objects by identifier.
    pub objects: BTreeMap<String, ObjectState>,
    /// Ledgers in insertion order: `(class, canonical ledger key)` and its entries.
    pub ledger: Vec<(String, String, Vec<Entry>)>,
}

impl State {
    /// Entries of a ledger key, created on demand.
    pub fn queue(&mut self, class: &str, key: &str) -> &mut Vec<Entry> {
        let i = match self
            .ledger
            .iter()
            .position(|(c, k, _)| c == class && k == key)
        {
            Some(i) => i,
            None => {
                self.ledger
                    .push((class.to_string(), key.to_string(), Vec::new()));
                self.ledger.len() - 1
            }
        };
        &mut self.ledger[i].2
    }

    /// The entries of a ledger key.
    pub fn entries(&self, class: &str, key: &str) -> &[Entry] {
        self.ledger
            .iter()
            .find(|(c, k, _)| c == class && k == key)
            .map(|x| x.2.as_slice())
            .unwrap_or(&[])
    }

    /// An entry by identifier.
    pub fn find_entry(&self, entry_id: &str) -> Option<&Entry> {
        self.ledger
            .iter()
            .flat_map(|x| x.2.iter())
            .find(|e| e.entry_id == entry_id)
    }
}

/// A commit as stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRecord {
    /// Parent commits; the first is the previous head of the branch.
    pub parents: Vec<u64>,
    /// Commit sequence on the branch line.
    pub cseq: u64,
    /// Branch state after the commit.
    pub state: State,
}

fn storage(e: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

/// Access to the stored branches, commits and versions.
pub struct Repo<'a> {
    /// The store.
    pub store: &'a mut dyn Store,
    /// The class registry.
    pub reg: &'a Registry,
    cache: HashMap<u64, CommitRecord>,
}

impl<'a> Repo<'a> {
    /// A repository view over a store and a class registry.
    pub fn new(store: &'a mut dyn Store, reg: &'a Registry) -> Self {
        Repo {
            store,
            reg,
            cache: HashMap::new(),
        }
    }

    /// A commit record.
    pub fn commit(&mut self, id: u64) -> Result<CommitRecord, BpaError> {
        if let Some(c) = self.cache.get(&id) {
            return Ok(c.clone());
        }
        let c: CommitRecord = serde_json::from_value(self.store.commit(id)?).map_err(storage)?;
        self.cache.insert(id, c.clone());
        Ok(c)
    }

    /// Initialise `main` with the empty genesis commit (cseq 0).
    pub fn init(&mut self) -> Result<(), BpaError> {
        let id = self.store.next_commit_id()?;
        let rec = serde_json::to_value(CommitRecord {
            parents: vec![],
            cseq: 0,
            state: State::default(),
        })
        .map_err(storage)?;
        self.store.create_branch("main", 0)?;
        let f = Flush {
            versions: vec![],
            commit_id: id,
            commit: rec,
            branch: "main".into(),
            expected_head: 0,
        };
        if !self.store.flush(&f)? {
            return Err(storage("cannot initialise main"));
        }
        Ok(())
    }

    /// The head commit of a branch.
    pub fn head(&mut self, branch: &str) -> Result<u64, BpaError> {
        self.store
            .head(branch)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown branch"))
    }

    /// The commit of a branch with the given cseq (default: head).
    pub fn commit_at(&mut self, branch: &str, time: Option<u64>) -> Result<u64, BpaError> {
        let mut id = self.head(branch)?;
        let Some(t) = time else { return Ok(id) };
        loop {
            let c = self.commit(id)?;
            if c.cseq <= t {
                return if c.cseq == t {
                    Ok(id)
                } else {
                    Err(BpaError::new("IAM.NOT_FOUND", "no commit with this cseq"))
                };
            }
            id = c.parents[0];
        }
    }

    /// The state of a branch at a cseq.
    pub fn state_at(&mut self, branch: &str, time: Option<u64>) -> Result<State, BpaError> {
        let id = self.commit_at(branch, time)?;
        Ok(self.commit(id)?.state)
    }

    /// A version record by hash.
    pub fn version(&mut self, hash: &str) -> Result<Value, BpaError> {
        self.store.version(hash)
    }

    /// Create a branch at a commit of another branch (STD-VER-010, -013).
    pub fn create_branch(
        &mut self,
        name: &str,
        source: &str,
        time: Option<u64>,
    ) -> Result<Value, BpaError> {
        names::check_name("branch-name", name)?;
        let id = self.commit_at(source, time)?;
        if !self.store.create_branch(name, id)? {
            return Err(BpaError::new("TXN.ALREADY_EXISTS", "branch exists"));
        }
        Ok(json!({"head": self.commit(id)?.cseq}))
    }

    /// Begin a commit on a branch.
    pub fn begin(&mut self, branch: &str) -> Result<Tx, BpaError> {
        let head = self.head(branch)?;
        let c = self.commit(head)?;
        Ok(Tx {
            branch: branch.to_string(),
            head,
            cseq: c.cseq + 1,
            state: c.state,
            versions: Vec::new(),
            appended: Vec::new(),
        })
    }

    /// Write a commit with the given parents and state; `TXN.HEAD_MOVED` if the head moved.
    pub fn write(&mut self, tx: Tx, parents: Vec<u64>) -> Result<u64, BpaError> {
        let id = self.store.next_commit_id()?;
        let rec = CommitRecord {
            parents,
            cseq: tx.cseq,
            state: tx.state,
        };
        let f = Flush {
            versions: tx.versions,
            commit_id: id,
            commit: serde_json::to_value(&rec).map_err(storage)?,
            branch: tx.branch,
            expected_head: tx.head,
        };
        if !self.store.flush(&f)? {
            return Err(BpaError::new("TXN.HEAD_MOVED", "the branch head moved"));
        }
        self.cache.insert(id, rec);
        Ok(id)
    }

    /// Create a version record and remember it for the flush (STD-OBJ-002).
    #[allow(clippy::too_many_arguments)]
    pub fn new_version(
        &self,
        tx: &mut Tx,
        oid: &str,
        class: &str,
        slug: &str,
        body: &Map<String, Value>,
        from: &str,
        to: Option<&str>,
        predecessor: Option<&str>,
        tombstone: bool,
    ) -> Result<String, BpaError> {
        let c = self.reg.class(class)?;
        let valid = json!({"from": from, "to": to});
        let mut env = json!({
            "object_id": oid, "class": class, "class_version": c.class_version, "schema_version": c.schema_version,
            "kind": c.kind, "slug": slug, "valid": valid, "body_ref": content_id("ubos.chunk.v1", &Value::Object(body.clone()))?,
        });
        if let Some(p) = predecessor {
            env["predecessor"] = json!(p);
        }
        if tombstone {
            env["tombstone"] = json!(true);
        }
        let vh = content_id("ubos.version.v1", &env)?;
        let rec = json!({"object_id": oid, "class": class, "slug": slug, "body": body, "valid": valid, "tombstone": tombstone});
        tx.versions.push((vh.clone(), rec));
        Ok(vh)
    }

    /// A version record, looking first at the versions of an open transaction.
    pub fn version_in(&mut self, tx: &Tx, hash: &str) -> Result<Value, BpaError> {
        match tx.versions.iter().find(|(h, _)| h == hash) {
            Some((_, v)) => Ok(v.clone()),
            None => self.version(hash),
        }
    }

    /// Entry hash: the entry without `seq`, `source_seq` and `cseq`, with class version and
    /// body reference (STD-KIND-020).
    pub fn entry_hash(&self, e: &Entry) -> Result<String, BpaError> {
        let mut rec = json!({"entry_id": e.entry_id, "class": e.class, "ledger_key": e.ledger_key, "valid_at": e.valid_at});
        if let Some(c) = &e.corrects {
            rec["corrects"] = json!(c);
        }
        if let Some(c) = &e.correction_type {
            rec["correction_type"] = json!(c);
        }
        if let Some(g) = &e.posting_group {
            rec["posting_group"] = json!(g);
        }
        rec["class_version"] = json!(self.reg.class(&e.class)?.class_version);
        rec["body_ref"] = json!(content_id("ubos.chunk.v1", &e.body)?);
        content_id("ubos.entry.v1", &rec)
    }

    fn check_period(&mut self, tx: &Tx, class: &str, today: &str) -> Result<(), BpaError> {
        let Some(scope) = self
            .reg
            .class(class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("period_scope"))
            .and_then(Value::as_str)
        else {
            return Ok(());
        };
        let objs: Vec<ObjectState> = tx
            .state
            .objects
            .values()
            .filter(|o| o.class == scope)
            .cloned()
            .collect();
        for o in objs {
            let hash = at(&o.timeline, today)
                .map(|s| s.version.clone())
                .unwrap_or(o.head.clone());
            let body = self.version_in(tx, &hash)?["body"].clone();
            let (start, end) = (
                body["start"].as_str().unwrap_or_default(),
                body["end"].as_str().unwrap_or_default(),
            );
            if body["state"] == "closed" && start <= today && today < end {
                return Err(BpaError::new(
                    "LEDG.PERIOD_CLOSED",
                    "posting date in a closed period",
                ));
            }
        }
        Ok(())
    }

    fn amounts(&self, class: &str) -> Result<Vec<String>, BpaError> {
        Ok(self
            .reg
            .class(class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("amounts"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Append, reverse or adjust (STD-KIND-020…025); returns the new entry's id and seq.
    pub fn append(
        &mut self,
        tx: &mut Tx,
        ch: &Value,
        op: &str,
        today: &str,
        default_valid: &dyn Fn(&str) -> String,
    ) -> Result<(String, u64), BpaError> {
        let s = |k: &str| ch.get(k).and_then(Value::as_str).map(str::to_string);
        let mut entry = if op == "append" {
            let class = s("class").unwrap_or_default();
            let c = self.reg.class(&class)?.clone();
            if c.kind.as_deref() != Some("ledger") {
                return Err(BpaError::new(
                    "LOGIC.FORBIDDEN_TARGET",
                    "not a ledger class",
                ));
            }
            let spec = c.ledger.clone().unwrap_or_default();
            let key_fields: Vec<String> = spec
                .get("key")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            let lk = ch
                .get("ledger_key")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut have: Vec<&String> = lk.keys().collect();
            have.sort();
            let mut want: Vec<&String> = key_fields.iter().collect();
            want.sort();
            if have != want {
                return Err(BpaError::new("MODEL.INVALID_VALUE", "ledger key fields"));
            }
            let mut key = Map::new();
            for k in &key_fields {
                let f = &c.fields[k];
                key.insert(
                    k.clone(),
                    ubos_model::types::values::validate_value(
                        f["type"].as_str().unwrap_or_default(),
                        f.get("constraints"),
                        &lk[k],
                    )?,
                );
            }
            let body = self
                .reg
                .check_body(&class, ch.get("body").unwrap_or(&json!({})), false)?;
            Entry {
                entry_id: s("entry_id").unwrap_or_default(),
                class: class.clone(),
                ledger_key: Value::Object(key),
                valid_at: s("valid_at").unwrap_or_else(|| default_valid(&class)),
                body: Value::Object(body),
                corrects: None,
                correction_type: None,
                posting_group: s("posting_group"),
                seq: 0,
                cseq: 0,
                entry_hash: String::new(),
                source_seq: None,
            }
        } else {
            let target = tx
                .state
                .find_entry(&s("entry_id").unwrap_or_default())
                .cloned()
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown entry"))?;
            let key = canonical(&target.ledger_key)?;
            let q = tx.state.entries(&target.class, &key);
            let body = if op == "reverse" {
                if target.correction_type.as_deref() == Some("reversal") {
                    return Err(BpaError::new(
                        "LEDG.REVERSAL_NOT_REVERSIBLE",
                        "reversal of a reversal",
                    ));
                }
                if q.iter().any(|e| {
                    e.correction_type.as_deref() == Some("reversal")
                        && e.corrects
                            .as_ref()
                            .is_some_and(|c| c.contains(&target.entry_id))
                }) {
                    return Err(BpaError::new("LEDG.ALREADY_REVERSED", "already reversed"));
                }
                let mut b = target.body.as_object().cloned().unwrap_or_default();
                for a in self.amounts(&target.class)? {
                    if let Some(v) = b.get(&a).cloned() {
                        b.insert(a, negate(&v)?);
                    }
                }
                Value::Object(b)
            } else {
                Value::Object(self.reg.check_body(
                    &target.class,
                    ch.get("body").unwrap_or(&json!({})),
                    false,
                )?)
            };
            Entry {
                entry_id: s("new_entry_id").unwrap_or_default(),
                class: target.class.clone(),
                ledger_key: target.ledger_key.clone(),
                valid_at: s("valid_at").unwrap_or(target.valid_at.clone()),
                body,
                corrects: Some(vec![target.entry_id.clone()]),
                correction_type: Some(
                    if op == "reverse" {
                        "reversal"
                    } else {
                        "adjustment"
                    }
                    .into(),
                ),
                posting_group: if op == "reverse" {
                    target
                        .posting_group
                        .as_ref()
                        .map(|g| format!("{g}/reversal"))
                } else {
                    None
                },
                seq: 0,
                cseq: 0,
                entry_hash: String::new(),
                source_seq: None,
            }
        };
        if !names::matches("uuid", &entry.entry_id) {
            return Err(BpaError::new("MODEL.INVALID_VALUE", "entry_id is a UUID"));
        }
        if tx.state.find_entry(&entry.entry_id).is_some()
            || tx.state.objects.contains_key(&entry.entry_id)
        {
            return Err(BpaError::new("TXN.ALREADY_EXISTS", "entry exists"));
        }
        self.check_period(tx, &entry.class.clone(), today)?;
        let key = canonical(&entry.ledger_key)?;
        entry.cseq = tx.cseq;
        entry.entry_hash = self.entry_hash(&entry)?;
        let q = tx.state.queue(&entry.class, &key);
        entry.seq = q.len() as u64 + 1;
        let out = (entry.entry_id.clone(), entry.seq);
        q.push(entry.clone());
        tx.appended.push(entry);
        Ok(out)
    }

    /// Double entry: each posting group of a double-entry class balances per currency (STD-KIND-024).
    pub fn check_double_entry(&self, tx: &Tx) -> Result<(), BpaError> {
        let mut groups: BTreeMap<(String, String), BTreeMap<String, Decimal>> = BTreeMap::new();
        for e in &tx.appended {
            let double = self
                .reg
                .class(&e.class)?
                .ledger
                .as_ref()
                .and_then(|l| l.get("double_entry"))
                == Some(&Value::Bool(true));
            if !double {
                continue;
            }
            let g = groups
                .entry((
                    e.class.clone(),
                    e.posting_group.clone().unwrap_or(e.entry_id.clone()),
                ))
                .or_default();
            for a in self.amounts(&e.class)? {
                let v = &e.body[&a];
                let (cur, amt) = match v {
                    Value::Object(o) => (
                        o["currency"].as_str().unwrap_or_default().to_string(),
                        o["amount"].as_str().unwrap_or("0"),
                    ),
                    other => (String::new(), other.as_str().unwrap_or("0")),
                };
                let d = Decimal::parse(amt)?;
                let sum = g.remove(&cur).map_or(d.clone(), |x| x.add(&d));
                g.insert(cur, sum);
            }
        }
        if groups.values().any(|t| t.values().any(|x| !x.is_zero())) {
            return Err(BpaError::new(
                "LEDG.UNBALANCED",
                "posting group does not balance",
            ));
        }
        Ok(())
    }

    /// State-tree entries of a state (STD-PROOF-001).
    pub fn state_entries(&self, st: &State) -> Result<Vec<(String, String)>, BpaError> {
        let mut out: Vec<(String, String)> = st
            .objects
            .iter()
            .filter(|(_, o)| o.kind != "system")
            .map(|(id, o)| (format!("o/{id}"), o.head.clone()))
            .collect();
        for (class, _, q) in &st.ledger {
            let ch = content_id("ubos.chunk.v1", &json!(class))?[7..].to_string();
            let kh = match q.first() {
                Some(e) => content_id("ubos.chunk.v1", &e.ledger_key)?[7..].to_string(),
                None => String::new(),
            };
            for e in q {
                out.push((format!("l/{ch}/{kh}/{:020}", e.seq), e.entry_hash.clone()));
            }
        }
        out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        Ok(out)
    }
}

fn negate(v: &Value) -> Result<Value, BpaError> {
    Ok(match v {
        Value::Object(o) => {
            let cur = o["currency"].as_str().unwrap_or_default();
            let d = Decimal::parse(o["amount"].as_str().unwrap_or("0"))?.neg();
            json!({"amount": d.format(minor_unit(cur)), "currency": cur})
        }
        Value::String(s) => json!(Decimal::parse(s)?.neg().format(None)),
        other => other.clone(),
    })
}

/// An open commit on a branch.
#[derive(Debug, Clone)]
pub struct Tx {
    /// Branch name.
    pub branch: String,
    /// Head the commit is built on.
    pub head: u64,
    /// cseq the commit will get.
    pub cseq: u64,
    /// State being changed.
    pub state: State,
    /// New version records.
    pub versions: Vec<(String, Value)>,
    /// Entries appended in this commit.
    pub appended: Vec<Entry>,
}
