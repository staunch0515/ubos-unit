//! Projections (BPA 1.0-rc9, PH-2 step 3c, DEC-100): the fold of the entries of a ledger
//! class by group (STD-TIME-021), the watermark of `async` projections, rebuilds, and
//! `read.projection` with the roll-up over the chart of accounts (STD-ISA-037).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use ubos_types::decimal::Decimal;
use ubos_types::values::minor_unit;
use ubos_types::{BpaError, canonical};
use ubos_version::state::{Entry, ObjectState, Repo, at};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// `a <= b`, comparing a date and an instant at the shorter of the two (STD-TIME-020).
fn prefix_le(a: &str, b: &str) -> bool {
    let n = a.len().min(b.len());
    a[..n] <= b[..n]
}

/// The accumulator of one aggregate.
#[derive(Debug, Clone, Default)]
pub struct Acc {
    fn_: String,
    n: u64,
    /// Per currency (`""` for numbers): the sum, minimum or maximum.
    cur: BTreeMap<String, Decimal>,
    scale: u32,
    int: bool,
    money: bool,
    last: Option<Value>,
    /// `(valid_at, cseq, seq)` of the entry `last` comes from.
    last_key: Option<(String, u64, u64)>,
}

impl Acc {
    fn new(fn_: &str) -> Acc {
        Acc {
            fn_: fn_.to_string(),
            ..Acc::default()
        }
    }

    fn add(&mut self, v: Option<&Value>, at: (String, u64, u64)) -> Result<(), BpaError> {
        self.n += 1;
        let Some(v) = v.filter(|v| !v.is_null()) else {
            return Ok(());
        };
        match self.fn_.as_str() {
            "count" => return Ok(()),
            "last" => {
                if self.last_key.as_ref().is_none_or(|k| *k <= at) {
                    self.last = Some(v.clone());
                    self.last_key = Some(at);
                }
                return Ok(());
            }
            _ => {}
        }
        let (k, amt) = match v {
            Value::Object(o) => {
                self.money = true;
                (
                    s(v, "currency").unwrap_or_default().to_string(),
                    Decimal::parse(o.get("amount").and_then(Value::as_str).unwrap_or("0"))?,
                )
            }
            Value::Number(n) => {
                self.int = true;
                (String::new(), Decimal::parse(&n.to_string())?)
            }
            Value::String(t) => {
                self.scale = self
                    .scale
                    .max(t.split_once('.').map_or(0, |x| x.1.len() as u32));
                (String::new(), Decimal::parse(t)?)
            }
            _ => return Ok(()),
        };
        let entry = self.cur.get(&k).cloned();
        let next = match (self.fn_.as_str(), entry) {
            ("sum", Some(x)) => x.add(&amt),
            ("min", Some(x)) if x <= amt => x,
            ("max", Some(x)) if x >= amt => x,
            _ => amt,
        };
        self.cur.insert(k, next);
        Ok(())
    }

    /// Combine with the accumulator of other entries (rows and the fold of the rest).
    fn combine(&mut self, o: &Acc) {
        self.n += o.n;
        self.scale = self.scale.max(o.scale);
        self.int |= o.int;
        self.money |= o.money;
        if let Some(k) = &o.last_key
            && self.last_key.as_ref().is_none_or(|m| m <= k)
        {
            self.last = o.last.clone();
            self.last_key = Some(k.clone());
        }
        for (k, v) in &o.cur {
            let x = match (self.fn_.as_str(), self.cur.get(k)) {
                ("sum", Some(x)) => x.add(v),
                ("min", Some(x)) if *x <= *v => x.clone(),
                ("max", Some(x)) if *x >= *v => x.clone(),
                _ => v.clone(),
            };
            self.cur.insert(k.clone(), x);
        }
    }

    /// The stored form of a row's accumulator (DSN-DVM-420).
    fn to_json(&self) -> Value {
        let cur: Map<String, Value> = self
            .cur
            .iter()
            .map(|(k, v)| (k.clone(), json!(v.format(None))))
            .collect();
        json!({"fn": self.fn_, "n": self.n, "cur": cur, "scale": self.scale, "int": self.int,
               "money": self.money, "last": self.last,
               "last_key": self.last_key.as_ref().map(|(a, c, q)| json!([a, c, q]))})
    }

    fn from_json(v: &Value) -> Result<Acc, BpaError> {
        let mut cur = BTreeMap::new();
        for (k, x) in v["cur"].as_object().into_iter().flatten() {
            cur.insert(k.clone(), Decimal::parse(x.as_str().unwrap_or("0"))?);
        }
        let last_key = v["last_key"].as_array().map(|a| {
            (
                a[0].as_str().unwrap_or_default().to_string(),
                a[1].as_u64().unwrap_or_default(),
                a[2].as_u64().unwrap_or_default(),
            )
        });
        Ok(Acc {
            fn_: s(v, "fn").unwrap_or_default().to_string(),
            n: v["n"].as_u64().unwrap_or_default(),
            cur,
            scale: v["scale"].as_u64().unwrap_or_default() as u32,
            int: v["int"] == json!(true),
            money: v["money"] == json!(true),
            last: v.get("last").filter(|x| !x.is_null()).cloned(),
            last_key,
        })
    }

    /// Add another accumulator of `count` or `sum` (roll-up).
    fn merge(&mut self, o: &Acc) {
        self.n += o.n;
        self.scale = self.scale.max(o.scale);
        self.int |= o.int;
        self.money |= o.money;
        for (k, v) in &o.cur {
            let x = match self.cur.get(k) {
                Some(x) => x.add(v),
                None => v.clone(),
            };
            self.cur.insert(k.clone(), x);
        }
    }

    fn value(&self) -> Value {
        match self.fn_.as_str() {
            "count" => return json!(self.n),
            "last" => return self.last.clone().unwrap_or(Value::Null),
            _ => {}
        }
        if self.cur.is_empty() {
            return Value::Null;
        }
        if self.money {
            return Value::Array(
                self.cur
                    .iter()
                    .map(|(c, v)| json!({"amount": v.format(minor_unit(c)), "currency": c}))
                    .collect(),
            );
        }
        let v = &self.cur[""];
        if self.int {
            return v
                .format(Some(0))
                .parse::<i64>()
                .map(|i| json!(i))
                .unwrap_or_else(|_| json!(v.format(None)));
        }
        json!(v.format(Some(self.scale)))
    }
}

/// One group: its key, its number of entries and its accumulators.
pub type Group = (Value, u64, Vec<Acc>);

/// The fold of entries by the groups and aggregates of a declaration (STD-TIME-021).
pub fn fold(decl: &Value, entries: &[Entry]) -> Result<BTreeMap<String, Group>, BpaError> {
    let mut groups = BTreeMap::new();
    fold_into(&mut groups, decl, entries)?;
    Ok(groups)
}

fn group_key(group_by: &[String], e: &Entry) -> Value {
    let mut key = Map::new();
    for f in group_by {
        let v = e
            .ledger_key
            .get(f)
            .or_else(|| e.body.get(f))
            .cloned()
            .unwrap_or(Value::Null);
        key.insert(f.clone(), v);
    }
    Value::Object(key)
}

fn group_by_of(decl: &Value) -> Vec<String> {
    decl["group_by"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect()
}

/// The rows of entries (DSN-DVM-420): one per commit, group and valid time.
pub fn rows_of(
    decl: &Value,
    projection: &str,
    version: &str,
    entries: &[Entry],
) -> Result<Vec<ubos_store::ProjectionRow>, BpaError> {
    let group_by = group_by_of(decl);
    let mut parts: BTreeMap<(u64, String, String), Vec<Entry>> = BTreeMap::new();
    for e in entries {
        let k = canonical(&group_key(&group_by, e))?;
        parts
            .entry((e.cseq, k, e.valid_at.clone()))
            .or_default()
            .push(e.clone());
    }
    let mut out = Vec::new();
    for ((cseq, k, valid_at), es) in parts {
        let g = fold(decl, &es)?;
        let (key, n, accs) = g.into_values().next().expect("one group");
        out.push(ubos_store::ProjectionRow {
            projection: projection.to_string(),
            version: version.to_string(),
            cseq,
            group_key: k,
            valid_at,
            delta: json!({"key": key, "entries": n, "accs": accs.iter().map(Acc::to_json).collect::<Vec<_>>()}),
        });
    }
    Ok(out)
}

/// Add a stored row to the groups.
fn add_row(
    groups: &mut BTreeMap<String, Group>,
    r: &ubos_store::ProjectionRow,
) -> Result<(), BpaError> {
    let accs: Vec<Acc> = r.delta["accs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(Acc::from_json)
        .collect::<Result<_, _>>()?;
    let n = r.delta["entries"].as_u64().unwrap_or_default();
    match groups.get_mut(&r.group_key) {
        Some(g) => {
            g.1 += n;
            for (a, b) in g.2.iter_mut().zip(&accs) {
                a.combine(b);
            }
        }
        None => {
            groups.insert(r.group_key.clone(), (r.delta["key"].clone(), n, accs));
        }
    }
    Ok(())
}

fn fold_into(
    groups: &mut BTreeMap<String, Group>,
    decl: &Value,
    entries: &[Entry],
) -> Result<(), BpaError> {
    let group_by = group_by_of(decl);
    let aggs = decl["aggregates"].as_array().cloned().unwrap_or_default();
    for e in entries {
        let key = group_key(&group_by, e);
        let g = groups.entry(canonical(&key)?).or_insert_with(|| {
            (
                key.clone(),
                0,
                aggs.iter()
                    .map(|a| Acc::new(s(a, "fn").unwrap_or_default()))
                    .collect(),
            )
        });
        g.1 += 1;
        for (acc, a) in g.2.iter_mut().zip(&aggs) {
            let at = (e.valid_at.clone(), e.cseq, e.seq);
            acc.add(s(a, "field").and_then(|f| e.body.get(f)), at)?;
        }
    }
    Ok(())
}

/// The mark of a projection on a branch: rows exist for the branch's own entries up to it.
pub fn mark_key(branch: &str, projection: &str, version: &str) -> String {
    format!("projection_mark:{branch}:{projection}:{version}")
}

/// Maintain the projections of a commit in its flush (DSN-DVM-420): for each live
/// projection whose source class the commit appends to, or that the commit writes, add the
/// rows of the branch's entries after its mark and of the appended entries, and move the
/// mark to the commit. `async` projections are maintained after the commit
/// (`maintain_projections`, PH-2 step 6f-2, DEC-125); the flush records their branch.
pub(crate) fn maintain(
    repo: &mut Repo,
    tx: &mut ubos_version::state::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    let wrote = written.iter().any(|(oid, _)| {
        tx.state
            .objects
            .get(oid)
            .is_some_and(|o| o.class == "Projection")
    });
    if tx.appended.is_empty() && !wrote {
        return Ok(());
    }
    repo.load_class(tx, "Projection")?;
    let live: Vec<(String, String)> = tx
        .state
        .objects
        .iter()
        .filter(|(_, o)| o.class == "Projection")
        .map(|(id, o)| (id.clone(), o.head.clone()))
        .collect();
    let mut async_seen = false;
    for (oid, vh) in live {
        let v = repo.version_in(tx, &vh)?;
        if v["tombstone"] == json!(true) {
            continue;
        }
        let decl = v["body"].clone();
        if s(&decl, "mode") == Some("async") {
            async_seen = true;
            continue;
        }
        let source = s(&decl, "source").unwrap_or_default().to_string();
        let mine = written.iter().any(|(o, _)| *o == oid);
        let appended: Vec<Entry> = tx
            .appended
            .iter()
            .filter(|e| e.class == source)
            .cloned()
            .collect();
        if appended.is_empty() && !mine {
            continue;
        }
        let key = mark_key(&tx.branch, &oid, &vh);
        let through = repo
            .store
            .meta(&key)?
            .and_then(|m| m["through"].as_u64())
            .unwrap_or(0);
        let mut entries: Vec<Entry> = repo
            .store
            .entries_between(&tx.branch, &source, through, tx.base)?
            .into_iter()
            .map(|e| {
                serde_json::from_value(e)
                    .map_err(|x| BpaError::new("NOD.STORAGE_UNAVAILABLE", x.to_string()))
            })
            .collect::<Result<_, _>>()?;
        entries.extend(appended);
        tx.projections.extend(rows_of(&decl, &oid, &vh, &entries)?);
        tx.marks.push((key, json!({"through": tx.cseq})));
    }
    // the branches whose `async` projections the maintenance looks at
    if async_seen {
        let mut branches: Vec<Value> = repo
            .store
            .meta(ASYNC_BRANCHES)?
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if !branches.contains(&json!(tx.branch)) {
            branches.push(json!(tx.branch));
            tx.marks
                .push((ASYNC_BRANCHES.to_string(), Value::Array(branches)));
        }
    }
    Ok(())
}

/// The branches with `async` projections (`kernel_meta`).
const ASYNC_BRANCHES: &str = "projection_async_branches";

/// The entries of a branch's own commits in `(from, to]`.
fn own_entries(
    repo: &mut Repo,
    branch: &str,
    source: &str,
    from: u64,
    to: u64,
) -> Result<Vec<Entry>, BpaError> {
    repo.store
        .entries_between(branch, source, from, to)?
        .into_iter()
        .map(|e| {
            serde_json::from_value(e)
                .map_err(|x| BpaError::new("NOD.STORAGE_UNAVAILABLE", x.to_string()))
        })
        .collect()
}

/// The body of an object at a date, when live then.
fn body_at(repo: &mut Repo, o: &ObjectState, date: &str) -> Result<Option<Value>, BpaError> {
    let Some(seg) = at(&o.timeline, date) else {
        return Ok(None);
    };
    let v = repo.version(&seg.version)?;
    Ok((v["tombstone"] != json!(true)).then(|| v["body"].clone()))
}

impl Engine {
    /// The declaration of the live projection named `name` at `(branch, cseq)`.
    pub fn projection_body(
        &mut self,
        branch: &str,
        cseq: u64,
        name: &str,
    ) -> Result<Value, BpaError> {
        Ok(self.projection_decl(branch, cseq, name)?.2)
    }

    /// `(object id, version hash, declaration)` of the live projection named `name` at
    /// `(branch, cseq)`.
    fn projection_decl(
        &mut self,
        branch: &str,
        cseq: u64,
        name: &str,
    ) -> Result<(String, String, Value), BpaError> {
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        for (oid, o) in r.objects_in(branch, cseq, Some("Projection"))? {
            let v = r.version(&o.head)?;
            if v["tombstone"] != json!(true) && s(&v["body"], "name") == Some(name) {
                return Ok((oid, o.head.clone(), v["body"].clone()));
            }
        }
        Err(BpaError::new("IAM.NOT_FOUND", "unknown projection"))
    }

    /// The groups of a projection over the entries of a branch chain up to `wm`: the rows
    /// up to each branch's mark and the fold of the entries after it (DSN-DVM-420).
    fn groups_at(
        &mut self,
        branch: &str,
        wm: u64,
        (oid, vh, decl): (&str, &str, &Value),
        asof: Option<&str>,
    ) -> Result<BTreeMap<String, Group>, BpaError> {
        let source = s(decl, "source").unwrap_or_default().to_string();
        let chain = Repo::new(self.store.as_mut(), &self.registry).chain_of(branch, wm)?;
        let mut groups = BTreeMap::new();
        for (b, cb) in chain {
            let mark = self
                .store
                .meta(&mark_key(&b, oid, vh))?
                .and_then(|m| m["through"].as_u64());
            let covered = mark.map_or(0, |t| t.min(cb));
            if covered > 0 {
                for r in self.store.projection_rows(&b, oid, vh, covered)? {
                    if asof.is_none_or(|a| prefix_le(&r.valid_at, a)) {
                        add_row(&mut groups, &r)?;
                    }
                }
            }
            let rest: Vec<Entry> = self
                .store
                .entries_between(&b, &source, covered, cb)?
                .into_iter()
                .map(|e| {
                    serde_json::from_value::<Entry>(e)
                        .map_err(|x| BpaError::new("NOD.STORAGE_UNAVAILABLE", x.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|e| asof.is_none_or(|a| prefix_le(&e.valid_at, a)))
                .collect();
            fold_into(&mut groups, decl, &rest)?;
        }
        Ok(groups)
    }

    fn lag_key(branch: &str, name: &str) -> String {
        format!("projection_lag:{branch}:{name}")
    }

    /// STD-TIME-021: a projection at a snapshot (`time`, default the head) with the
    /// watermark, the `asof`, `group`, `rollup` and `at_least` of STD-ISA-037.
    pub fn projection_at(&mut self, branch: &str, i: &Value) -> Result<Value, BpaError> {
        let name = s(i, "projection").unwrap_or_default().to_string();
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let snap = r.ordinal(branch, i.get("time").and_then(Value::as_u64))?;
        drop(r);
        let (oid, vh, decl) = self.projection_decl(branch, snap, &name)?;
        let mut wm = snap;
        if s(&decl, "mode") == Some("async")
            && let Some(l) = self
                .store
                .meta(&Self::lag_key(branch, &name))?
                .and_then(|v| v.as_u64())
        {
            wm = wm.min(l);
        }
        // STD-ISA-037 (PH-2 step 6f-2): a read that may wait lets the projection's
        // maintenance run before it judges the watermark
        if let Some(n) = i.get("at_least").and_then(Value::as_u64)
            && wm < n
            && i.get("timeout_ms").and_then(Value::as_u64).unwrap_or(0) > 0
        {
            self.maintain_projection(branch, (&oid, &vh, &decl), None)?;
        }
        if let Some(n) = i.get("at_least").and_then(Value::as_u64)
            && wm < n
        {
            return Err(BpaError::with_details(
                "QRY.WATERMARK_TIMEOUT",
                json!({"watermark": wm, "at_least": n}),
            ));
        }
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let wid = r.commit_at(branch, Some(wm))?;
        let commit_id = r.meta(wid)?.commit_id;
        drop(r);
        let source = s(&decl, "source").unwrap_or_default().to_string();
        let asof = s(i, "asof");
        let mut groups = self.groups_at(branch, wm, (&oid, &vh, &decl), asof)?;
        if i.get("rollup") == Some(&Value::Bool(true)) {
            let date = asof
                .map(str::to_string)
                .unwrap_or_else(|| self.now[..10].to_string());
            groups = self.rollup(branch, snap, &decl, &source, groups, &date)?;
        }
        let group_by: Vec<&str> = decl["group_by"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let want = i
            .get("group")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if want.keys().any(|k| !group_by.contains(&k.as_str())) {
            return Err(BpaError::new(
                "QRY.INVALID_QUERY",
                "group names fields of group_by",
            ));
        }
        let aggs = decl["aggregates"].as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for (key, n, accs) in groups.values() {
            if want.iter().any(|(k, v)| key.get(k) != Some(v)) {
                continue;
            }
            let mut values = Map::new();
            for (a, acc) in aggs.iter().zip(accs) {
                values.insert(s(a, "as").unwrap_or_default().to_string(), acc.value());
            }
            out.push(json!({"key": key, "entries": n, "values": values}));
        }
        Ok(
            json!({"projection": name, "groups": out, "watermark": {"cseq": wm, "commit_id": commit_id}}),
        )
    }

    /// STD-ISA-037: add the leaf groups into every account above them, at `date`.
    fn rollup(
        &mut self,
        branch: &str,
        cseq: u64,
        decl: &Value,
        source: &str,
        groups: BTreeMap<String, Group>,
        date: &str,
    ) -> Result<BTreeMap<String, Group>, BpaError> {
        let field = self
            .registry
            .class(source)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("account"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let grouped = decl["group_by"]
            .as_array()
            .is_some_and(|g| field.as_deref().is_some_and(|f| g.iter().any(|x| x == f)));
        let sums = decl["aggregates"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|a| matches!(s(a, "fn"), Some("count" | "sum")));
        let (Some(field), true, true) = (field, grouped, sums) else {
            return Err(BpaError::new(
                "QRY.INVALID_QUERY",
                "rollup needs the account field and count or sum aggregates",
            ));
        };
        let authority = self.store.authority();
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let accounts = r.objects_in(branch, cseq, Some("LedgerAccount"))?;
        let mut parent: BTreeMap<String, Option<String>> = BTreeMap::new();
        for o in accounts.values() {
            let uri = crate::relations::uri_of(&authority, &o.class, &o.slug);
            let p = body_at(&mut r, o, date)?.and_then(|b| s(&b, "parent").map(str::to_string));
            parent.insert(uri, p);
        }
        let mut out = groups.clone();
        for (key, n, accs) in groups.values() {
            let mut uri = s(key, &field).unwrap_or_default().to_string();
            let mut seen = Vec::new();
            while let Some(Some(p)) = parent.get(&uri) {
                if seen.contains(p) {
                    break;
                }
                seen.push(p.clone());
                let mut pkey = key.clone();
                pkey[&field] = json!(p);
                let g = out.entry(canonical(&pkey)?).or_insert_with(|| {
                    (
                        pkey.clone(),
                        0,
                        accs.iter().map(|a| Acc::new(&a.fn_)).collect(),
                    )
                });
                g.1 += n;
                for (acc, src) in g.2.iter_mut().zip(accs) {
                    acc.merge(src);
                }
                uri = p.clone();
            }
        }
        Ok(out)
    }

    /// Adapter operation `read_projection` (STD-TIME-021, STD-ISA-037).
    pub fn read_projection(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        self.projection_at(&branch, i)
    }

    /// The commit up to which the maintenance of an `async` projection may go: the head, or
    /// the commit `projection_lag` stopped it at.
    fn maintenance_target(&mut self, branch: &str, name: &str) -> Result<u64, BpaError> {
        let head = Repo::new(self.store.as_mut(), &self.registry).ordinal(branch, None)?;
        Ok(
            match self
                .store
                .meta(&Self::lag_key(branch, name))?
                .and_then(|v| v.as_u64())
            {
                Some(l) => l.min(head),
                None => head,
            },
        )
    }

    /// Maintain an `async` projection after its commits (PH-2 step 6f-2, DEC-125): add the
    /// rows of the branch's own entries from its mark towards the target, at most about
    /// `limit` entries (whole commits) per call, in one storage transaction. A new or changed
    /// projection starts at mark 0, so this is also its backfill (DSN-NOD-207). Returns the
    /// mark afterwards.
    fn maintain_projection(
        &mut self,
        branch: &str,
        (oid, vh, decl): (&str, &str, &Value),
        limit: Option<usize>,
    ) -> Result<u64, BpaError> {
        let name = s(decl, "name").unwrap_or_default().to_string();
        let key = mark_key(branch, oid, vh);
        let through = self
            .store
            .meta(&key)?
            .and_then(|m| m["through"].as_u64())
            .unwrap_or(0);
        let target = self.maintenance_target(branch, &name)?;
        if through >= target {
            return Ok(through);
        }
        let source = s(decl, "source").unwrap_or_default().to_string();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut entries = own_entries(&mut repo, branch, &source, through, target)?;
        entries.sort_by_key(|e| (e.cseq, e.seq));
        let mut upto = target;
        if let Some(l) = limit.filter(|l| *l > 0 && entries.len() > *l) {
            upto = entries[l - 1].cseq;
            entries.retain(|e| e.cseq <= upto);
        }
        let rows = rows_of(decl, oid, vh, &entries)?;
        self.store
            .append_projection(branch, oid, vh, &rows, (&key, &json!({"through": upto})))?;
        Ok(upto)
    }

    /// The live `async` projections of a branch at its head: `(object id, version, declaration)`.
    fn async_projections(
        &mut self,
        branch: &str,
    ) -> Result<Vec<(String, String, Value)>, BpaError> {
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let head = r.ordinal(branch, None)?;
        let mut out = Vec::new();
        for (oid, o) in r.objects_in(branch, head, Some("Projection"))? {
            let v = r.version(&o.head)?;
            if v["tombstone"] != json!(true) && s(&v["body"], "mode") == Some("async") {
                out.push((oid, o.head.clone(), v["body"].clone()));
            }
        }
        Ok(out)
    }

    /// Host operation `maintain_projections {branch?, limit?}` (PH-2 step 6f-2, DEC-125): one
    /// round of the maintenance of the `async` projections of a branch, or of every branch
    /// that has them; `{projections}` with each one's `{branch, projection, through, target}`.
    pub fn maintain_projections(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branches: Vec<String> = match s(i, "branch") {
            Some(b) => vec![b.to_string()],
            None => self
                .store
                .meta(ASYNC_BRANCHES)?
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|b| b.as_str().map(str::to_string))
                .collect(),
        };
        let limit = i
            .get("limit")
            .and_then(Value::as_u64)
            .map(|l| usize::try_from(l).unwrap_or(usize::MAX));
        let mut out = Vec::new();
        for b in branches {
            let Ok(list) = self.async_projections(&b) else {
                continue;
            };
            for (oid, vh, decl) in list {
                let through = self.maintain_projection(&b, (&oid, &vh, &decl), limit)?;
                let name = s(&decl, "name").unwrap_or_default().to_string();
                let target = self.maintenance_target(&b, &name)?;
                out.push(
                    json!({"branch": b, "projection": name, "through": through, "target": target}),
                );
            }
        }
        Ok(json!({ "projections": out }))
    }

    /// Host operation `projection_status {branch, projection}` (FR-EVT-072): the progress of
    /// a projection's maintenance or backfill, `{projection, mode, version, through, target,
    /// entries, maintained, state}` with `state` `live` or `backfilling`.
    pub fn projection_status(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let name = s(i, "projection").unwrap_or_default().to_string();
        let head = Repo::new(self.store.as_mut(), &self.registry).ordinal(&branch, None)?;
        let (oid, vh, decl) = self.projection_decl(&branch, head, &name)?;
        let through = self
            .store
            .meta(&mark_key(&branch, &oid, &vh))?
            .and_then(|m| m["through"].as_u64())
            .unwrap_or(0);
        let target = self.maintenance_target(&branch, &name)?;
        let source = s(&decl, "source").unwrap_or_default().to_string();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let total = own_entries(&mut repo, &branch, &source, 0, target)?.len();
        let done = own_entries(&mut repo, &branch, &source, 0, through.min(target))?.len();
        let mode = s(&decl, "mode").unwrap_or("sync");
        Ok(
            json!({"projection": name, "mode": mode, "version": vh, "through": through,
            "target": target, "entries": total, "maintained": done,
            "state": if mode != "async" || through >= target { "live" } else { "backfilling" }}),
        )
    }

    /// Adapter operation `projection_lag`: stop or resume the maintenance of an `async`
    /// projection after a commit (STD-TIME-021).
    pub fn projection_lag(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let name = s(i, "projection").unwrap_or_default().to_string();
        let head = Repo::new(self.store.as_mut(), &self.registry).ordinal(&branch, None)?;
        let (_, _, decl) = self.projection_decl(&branch, head, &name)?;
        if s(&decl, "mode") != Some("async") {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "only an async projection lags",
            ));
        }
        let cseq = i.get("cseq").cloned().unwrap_or(Value::Null);
        self.store.put_meta(&Self::lag_key(&branch, &name), &cseq)?;
        Ok(json!({"projection": name, "cseq": cseq}))
    }

    /// Adapter operation `rebuild_projection`: rebuild from the committed entries; an
    /// `async` projection is then maintained up to the head (STD-TIME-021).
    pub fn rebuild_projection(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let name = s(i, "projection").unwrap_or_default().to_string();
        let head = Repo::new(self.store.as_mut(), &self.registry).ordinal(&branch, None)?;
        let (oid, vh, decl) = self.projection_decl(&branch, head, &name)?;
        // the rows of the branch's own entries replace the old ones in one storage
        // transaction, with the mark at the head (FR-ANL-021, -023)
        let source = s(&decl, "source").unwrap_or_default().to_string();
        let own: Vec<Entry> = self
            .store
            .entries_between(&branch, &source, 0, head)?
            .into_iter()
            .map(|e| {
                serde_json::from_value::<Entry>(e)
                    .map_err(|x| BpaError::new("NOD.STORAGE_UNAVAILABLE", x.to_string()))
            })
            .collect::<Result<_, _>>()?;
        let rows = rows_of(&decl, &oid, &vh, &own)?;
        self.store.replace_projection(
            &branch,
            &oid,
            &vh,
            &rows,
            (&mark_key(&branch, &oid, &vh), &json!({"through": head})),
        )?;
        let key = Self::lag_key(&branch, &name);
        if self.store.meta(&key)?.is_some_and(|v| !v.is_null()) {
            self.store.put_meta(&key, &json!(head))?;
        }
        let r = self.projection_at(&branch, &json!({"projection": name}))?;
        Ok(
            json!({"projection": name, "groups": r["groups"].as_array().map_or(0, Vec::len), "watermark": r["watermark"]}),
        )
    }
}
