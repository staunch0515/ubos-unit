//! Time, ledger and transaction operations of the engine (PH-1 step 4c, DEC-052):
//! derivation records and impact sets (STD-TXN-030, STD-TIME-031), idempotent requests
//! (STD-TXN-040), process effects and the outbox with became-effective events (STD-TXN-050,
//! STD-TIME-050), range queries and object history (STD-TIME-014, -040) and bulk processes
//! (STD-TXN-070). Records that are not part of a commit (derivation records, the outbox,
//! the schedule of future segment starts, idempotency records, bulk process state) live
//! under keys of the store's metadata in the transition layout (UBS-SYS-DVM-04); step 4d
//! gives them tables.

use crate::stateful::Engine;
use crate::versioning::{Process, process, record};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_model::Registry;
use ubos_store::Store;
use ubos_types::values::validate_value;
use ubos_types::{BpaError, canonical, content_id};
use ubos_version::{Repo, Segment, State, Tx, at, lt};

/// Re-evaluation modes of a derivation (STD-TXN-030).
const MODES: &[&str] = &["as_originally", "as_effective", "current"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

fn not_found(m: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", m)
}

fn text_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// A date or an instant, validated.
pub fn instant_or_date(v: &Value) -> Result<String, BpaError> {
    match validate_value("date", None, v) {
        Ok(x) => Ok(text_of(&x)),
        Err(_) => Ok(text_of(&validate_value("instant", None, v)?)),
    }
}

fn meta_list(store: &mut dyn Store, key: &str) -> Result<Vec<Value>, BpaError> {
    Ok(store
        .meta(key)?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default())
}

/// The segment of a read result (`version_hash`, `body`, `slug`, `valid`, provenance).
pub fn segment(
    repo: &mut Repo<'_>,
    from: &str,
    to: Option<&str>,
    version: &str,
) -> Result<Value, BpaError> {
    let v = repo.version(version)?;
    let mut out = json!({"version_hash": version, "body": v["body"], "slug": v["slug"], "valid": {"from": from, "to": to}});
    if let Some(p) = s(&v, "provenance_ref") {
        out["provenance_ref"] = json!(p);
    }
    Ok(out)
}

// ---- derivation records (STD-TXN-030) ----

/// Class of a derivation input and, for a ledger entry, its commit ordinal: a version of
/// the named object, or a ledger entry with that hash.
fn input_meta(
    repo: &mut Repo<'_>,
    tx: &Tx,
    item: &Value,
) -> Result<(String, Option<u64>), BpaError> {
    let vh = s(item, "version").unwrap_or_default();
    if let Ok(v) = repo.version_in(tx, vh) {
        if v["object_id"] != item["object"] {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                "version of another object",
            ));
        }
        return Ok((text_of(&v["class"]), None));
    }
    if let Some(e) = repo.find_entry(tx, s(item, "object").unwrap_or_default())?
        && e.entry_hash == vh
    {
        return Ok((e.class, Some(e.cseq)));
    }
    Err(BpaError::new(
        "MODEL.REFERENCE_INVALID",
        "unknown input version",
    ))
}

/// The commit ordinal on a branch at which a version of an object first appeared in its
/// timeline (the version record itself is content-addressed and carries no ordinal);
/// `fallback` when it is not in the branch's history, for example a version of the open
/// transaction. Walks the first-parent history; step 4d answers this from the temporal
/// index.
pub fn version_cseq(
    repo: &mut Repo<'_>,
    branch: &str,
    oid: &str,
    version: &str,
    fallback: u64,
) -> Result<u64, BpaError> {
    let head = repo.head(branch)?;
    let mut cseqs: Vec<u64> = repo
        .first_parents(head)?
        .iter()
        .map(|c| repo.commit(*c).map(|x| x.cseq))
        .collect::<Result<_, _>>()?;
    cseqs.sort();
    for cseq in cseqs {
        if repo
            .object_at(branch, Some(cseq), oid)?
            .is_some_and(|o| o.timeline.iter().any(|x| x.version == version))
        {
            return Ok(cseq);
        }
    }
    Ok(fallback)
}

/// The derivation record of a change, validated and stored under `derivation:<id>`;
/// returns its identifier and the logic it names (STD-TXN-030).
pub fn derivation_of(
    repo: &mut Repo<'_>,
    tx: &Tx,
    ch: &Value,
    process_id: &str,
    max_inputs: u64,
) -> Result<Option<(String, Vec<String>)>, BpaError> {
    let Some(d) = ch.get("derivation").filter(|d| !d.is_null()) else {
        return Ok(None);
    };
    let logic: Vec<String> = match d.get("logic").and_then(Value::as_array) {
        Some(l) if !l.is_empty() && l.iter().all(Value::is_string) => {
            l.iter().map(text_of).collect()
        }
        _ => {
            return Err(invalid(
                "a derivation names the logic versions that produced it",
            ));
        }
    };
    let mode = s(d, "mode").unwrap_or("as_effective").to_string();
    if !MODES.contains(&mode.as_str()) {
        return Err(invalid("mode is as_originally, as_effective or current"));
    }
    let mut items: BTreeMap<String, (Value, String, Option<u64>)> = BTreeMap::new();
    for x in d
        .get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !x.is_object()
            || ["object", "version", "asof"]
                .iter()
                .any(|k| x.get(k).is_none())
        {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "an input is {object, version, asof}",
            ));
        }
        let item = json!({"object": x["object"], "version": x["version"], "asof": instant_or_date(&x["asof"])?});
        let (class, cseq) = input_meta(repo, tx, &item)?;
        items.insert(canonical(&item)?, (item, class, cseq));
    }
    let mut list: Vec<(Value, String, Option<u64>)> = items.into_values().collect();
    list.sort_by(|a, b| {
        let k = |i: &Value| {
            (
                text_of(&i["object"]),
                text_of(&i["asof"]),
                text_of(&i["version"]),
            )
        };
        k(&a.0).cmp(&k(&b.0))
    });
    let inputs: Vec<Value> = list.iter().map(|x| x.0.clone()).collect();
    let mut rec = json!({"process_id": process_id, "logic": logic, "mode": mode});
    if list.len() as u64 > max_inputs {
        let mut per: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
        for (item, class, cseq) in &list {
            let cseq = match cseq {
                Some(c) => *c,
                None => version_cseq(
                    repo,
                    &tx.branch,
                    s(item, "object").unwrap_or_default(),
                    s(item, "version").unwrap_or_default(),
                    tx.cseq,
                )?,
            };
            let e = per.entry(class.clone()).or_insert((cseq, cseq, 0));
            e.0 = e.0.min(cseq);
            e.1 = e.1.max(cseq);
            e.2 += 1;
        }
        rec["inputs_summary"] = json!(
            per.iter()
                .map(|(c, (lo, hi, n))| json!({"class": c, "version_range": [lo, hi], "count": n}))
                .collect::<Vec<_>>()
        );
        rec["inputs_commitment"] = json!(content_id("ubos.chunk.v1", &json!(inputs))?);
    } else if !inputs.is_empty() {
        rec["inputs"] = json!(inputs);
    }
    let rid = content_id("ubos.chunk.v1", &rec)?;
    repo.store.put_meta(
        &format!("derivation:{rid}"),
        &json!({"record": rec, "inputs": inputs}),
    )?;
    Ok(Some((rid, logic)))
}

// ---- outbox (STD-TXN-050) and became-effective events (STD-TIME-050) ----

/// Release the effects of a committed process to the outbox under stable identifiers
/// `<process_id>/<n>` (STD-TXN-050).
pub fn effects(
    store: &mut dyn Store,
    p: &Process,
    branch: &str,
    cseq: u64,
) -> Result<(), BpaError> {
    let list =
        p.p.get("effects")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
    if list.is_empty() {
        return Ok(());
    }
    let ckey = format!("effects:{}", p.id);
    let mut n = store
        .meta(&ckey)?
        .and_then(|v| v.as_u64())
        .unwrap_or_default();
    let mut outbox = meta_list(store, "outbox")?;
    for e in &list {
        n += 1;
        let mut item = json!({"effect_id": format!("{}/{n}", p.id), "type": e["type"], "branch": branch, "cseq": cseq});
        for k in ["object_id", "payload"] {
            if let Some(v) = e.get(k)
                && !v.is_null()
            {
                item[k] = v.clone();
            }
        }
        outbox.push(item);
    }
    store.put_meta(&ckey, &json!(n))?;
    store.put_meta("outbox", &Value::Array(outbox))
}

fn emits(reg: &Registry, class: &str) -> bool {
    reg.decl(class)
        .ok()
        .and_then(|d| d.get("emit_became_effective"))
        .is_some_and(|v| !v.is_null() && *v != json!(false))
}

/// True when a segment start has been reached by the clock (date or instant granularity).
fn due(reg: &Registry, now: &str, from: &str, class: &str) -> bool {
    let instant = reg
        .class(class)
        .map(|c| c.valid_time == "instant")
        .unwrap_or(false);
    from <= if instant { now } else { &now[..10] }
}

/// Future segment starts of the written objects of classes that emit `BecameEffective`
/// (STD-TIME-050), to be added to the schedule after the commit is written.
pub fn pending_starts(
    reg: &Registry,
    now: &str,
    branch: &str,
    st: &State,
    versions: &Value,
) -> Vec<Value> {
    let mut out = Vec::new();
    for oid in versions.as_object().map(|m| m.keys()).into_iter().flatten() {
        let Some(o) = st.objects.get(oid) else {
            continue;
        };
        if !emits(reg, &o.class) {
            continue;
        }
        for seg in &o.timeline {
            if !due(reg, now, &seg.from, &o.class) {
                out.push(json!({"key": [branch, oid, seg.from, seg.version], "class": o.class}));
            }
        }
    }
    out
}

/// Add pending starts to the schedule (deduplicated).
pub fn schedule(store: &mut dyn Store, pending: &[Value]) -> Result<(), BpaError> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut sched = meta_list(store, "schedule")?;
    for p in pending {
        if !sched.iter().any(|x| x["key"] == p["key"]) {
            sched.push(p.clone());
        }
    }
    store.put_meta("schedule", &Value::Array(sched))
}

impl Engine {
    /// Emit the `BecameEffective` events whose segment start the clock has reached and
    /// whose segment still starts there with the same version (STD-TIME-050).
    pub fn tick(&mut self) -> Result<(), BpaError> {
        let sched = meta_list(self.store.as_mut(), "schedule")?;
        if sched.is_empty() {
            return Ok(());
        }
        let mut keep = Vec::new();
        let mut outbox = meta_list(self.store.as_mut(), "outbox")?;
        for item in sched {
            let key = &item["key"];
            let (branch, oid, from, vh) = (
                text_of(&key[0]),
                text_of(&key[1]),
                text_of(&key[2]),
                text_of(&key[3]),
            );
            let class = text_of(&item["class"]);
            if !due(&self.registry, &self.now, &from, &class) {
                keep.push(item);
                continue;
            }
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let Ok((view, _)) = repo.view(&branch, &json!({})) else {
                continue;
            };
            if view
                .state
                .objects
                .get(&oid)
                .is_some_and(|o| o.timeline.iter().any(|x| x.from == from && x.version == vh))
            {
                outbox.push(
                    json!({"effect_id": format!("became-effective/{branch}/{oid}/{from}"),
                    "type": "BecameEffective", "branch": branch, "object_id": oid,
                    "payload": {"valid_from": from}}),
                );
            }
        }
        self.store.put_meta("schedule", &Value::Array(keep))?;
        self.store.put_meta("outbox", &Value::Array(outbox))
    }

    /// `outbox`: the released effects not yet acknowledged, in release order (STD-TXN-050).
    pub fn outbox(&mut self) -> Result<Value, BpaError> {
        self.tick()?;
        let acked: BTreeSet<String> = meta_list(self.store.as_mut(), "acked")?
            .iter()
            .map(text_of)
            .collect();
        let effects: Vec<Value> = meta_list(self.store.as_mut(), "outbox")?
            .into_iter()
            .filter(|e| !acked.contains(s(e, "effect_id").unwrap_or_default()))
            .collect();
        Ok(json!({"effects": effects}))
    }

    /// `ack`: acknowledge released effects.
    pub fn ack(&mut self, ids: &Value) -> Result<Value, BpaError> {
        let known: BTreeSet<String> = meta_list(self.store.as_mut(), "outbox")?
            .iter()
            .map(|e| text_of(&e["effect_id"]))
            .collect();
        let ids: Vec<String> = ids
            .as_array()
            .ok_or_else(|| invalid("effect_ids is a list"))?
            .iter()
            .map(text_of)
            .collect();
        if let Some(x) = ids.iter().find(|x| !known.contains(*x)) {
            return Err(not_found(&format!("unknown effect {x}")));
        }
        let mut acked: BTreeSet<String> = meta_list(self.store.as_mut(), "acked")?
            .iter()
            .map(text_of)
            .collect();
        acked.extend(ids);
        self.store.put_meta("acked", &json!(acked))?;
        Ok(Value::Bool(true))
    }

    // ---- idempotent requests (STD-TXN-040) ----

    /// The idempotency record key and request hash of a commit, when its process names an
    /// idempotency key: `(tenant, initiator, key)` → hash of `{branch, changes}`.
    pub fn idempotency(p: &Process, input: &Value) -> Result<Option<(String, String)>, BpaError> {
        let Some(k) = p.p.get("idempotency_key").filter(|k| !k.is_null()) else {
            return Ok(None);
        };
        let key = format!(
            "idem:{}:{}:{}",
            ubos_version::TENANT,
            text_of(&p.p["initiator"]),
            canonical(k)?
        );
        let h = content_id(
            "ubos.chunk.v1",
            &json!({"branch": input.get("branch").cloned().unwrap_or(Value::Null),
                "changes": input.get("changes").cloned().unwrap_or(json!([]))}),
        )?;
        Ok(Some((key, h)))
    }

    /// The stored result of a repeated request, or `TXN.IDEMPOTENCY_MISMATCH` when the
    /// payload differs.
    pub fn replay(&mut self, key: &str, hash: &str) -> Result<Option<Value>, BpaError> {
        match self.store.meta(key)? {
            None => Ok(None),
            Some(known) => {
                if s(&known, "hash") != Some(hash) {
                    return Err(BpaError::new(
                        "TXN.IDEMPOTENCY_MISMATCH",
                        "another payload under the same key",
                    ));
                }
                Ok(Some(known["result"].clone()))
            }
        }
    }

    /// Remember the result of a request under its idempotency key.
    pub fn remember(&mut self, key: &str, hash: &str, result: &Value) -> Result<(), BpaError> {
        self.store.put_meta(
            key,
            &json!({"hash": hash, "result": result, "at": self.now}),
        )
    }

    // ---- range queries, history, derivation records (STD-TIME-014, -040, STD-TXN-030) ----

    /// `read_range` (STD-TIME-014): the segments of an object clipped to `[from, to)`.
    pub fn read_range(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let oid = s(input, "object_id").unwrap_or_default().to_string();
        let (o, _, ev, _) = self.object_view(&branch, input, &oid)?;
        let a = instant_or_date(input.get("from").unwrap_or(&Value::Null))?;
        let b = match input.get("to").filter(|v| !v.is_null()) {
            Some(v) => Some(instant_or_date(v)?),
            None => None,
        };
        if b.is_some() && !lt(Some(&a), b.as_deref()) {
            return Err(invalid("empty range"));
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut out = Vec::new();
        for seg in &o.timeline {
            let starts_before_end = b.as_deref().is_none_or(|b| lt(Some(&seg.from), Some(b)));
            if starts_before_end && lt(Some(&a), seg.to.as_deref()) {
                let from = if a.as_str() < seg.from.as_str() {
                    seg.from.as_str()
                } else {
                    a.as_str()
                };
                let to = match (seg.to.as_deref(), b.as_deref()) {
                    (None, x) | (x, None) => x,
                    (Some(x), Some(y)) => Some(x.min(y)),
                };
                out.push(segment(&mut repo, from, to, &seg.version)?);
            }
        }
        Ok(json!({"segments": out, "evaluated_at": ev}))
    }

    /// `history` (STD-TIME-040): the operations on an object per commit of the branch's
    /// first-parent history, filtered by transaction time `[from, to)`.
    pub fn history(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let oid = s(input, "object_id").unwrap_or_default();
        let mut bounds = [None, None];
        for (i, k) in ["from", "to"].iter().enumerate() {
            if let Some(v) = input.get(k).filter(|v| !v.is_null()) {
                bounds[i] = Some(text_of(&validate_value("instant", None, v)?));
            }
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.head(branch)?;
        let mut out = Vec::new();
        for cid in repo.first_parents(head)? {
            let c = repo.commit(cid)?;
            let Some(t) = c.tx_time.clone() else {
                continue;
            };
            if bounds[0].as_deref().is_some_and(|f| t.as_str() < f)
                || bounds[1].as_deref().is_some_and(|u| t.as_str() >= u)
            {
                continue;
            }
            if let Some(ops) = c.ops.get(oid) {
                out.push(
                    json!({"cseq": c.cseq, "commit_id": c.commit_id, "tx_time": t, "ops": ops}),
                );
            }
        }
        out.sort_by_key(|e| e["cseq"].as_u64().unwrap_or_default());
        let head_id = repo.commit(head)?.commit_id;
        Ok(json!({"commits": out, "evaluated_at": {branch: head_id}}))
    }

    /// `derivation` (STD-TXN-030): the derivation record of the head version of an object
    /// or of a ledger entry.
    pub fn derivation(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let (view, _) = repo.view(branch, input)?;
        let rid = if let Some(oid) = s(input, "object_id") {
            let o = view
                .state
                .objects
                .get(oid)
                .ok_or_else(|| not_found("unknown object"))?;
            s(&repo.version(&o.head)?, "provenance_ref").map(str::to_string)
        } else {
            view.state
                .find_entry(s(input, "entry_id").unwrap_or_default())
                .ok_or_else(|| not_found("unknown entry"))?
                .provenance_ref
                .clone()
        };
        let Some(rid) = rid else {
            return Err(not_found("written by direct input: no derivation record"));
        };
        let rec = self
            .store
            .meta(&format!("derivation:{rid}"))?
            .ok_or_else(|| not_found("unknown derivation record"))?;
        let mut out = rec["record"].as_object().cloned().unwrap_or_default();
        out.insert("provenance_ref".into(), json!(rid));
        if let Some(inputs) = out.get("inputs").and_then(Value::as_array).cloned() {
            // version hashes are named version_hash in adapter results (STD-CONF-005, rule 6)
            out.insert(
                "inputs".into(),
                json!(
                    inputs
                        .iter()
                        .map(|x| json!({"object": x["object"], "asof": x["asof"], "version_hash": x["version"]}))
                        .collect::<Vec<_>>()
                ),
            );
        }
        Ok(Value::Object(out))
    }

    // ---- impact set (STD-TIME-031) ----

    /// `impact`: the derived versions and entries whose inputs a retroactive commit
    /// superseded, closed over derivation records.
    pub fn impact(&mut self, branch: &str, time: Option<u64>) -> Result<Value, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let cid = repo.commit_at(branch, time)?;
        let c = repo.commit(cid)?;
        let Some(&parent) = c.parents.first() else {
            return Err(invalid("the first commit changes nothing"));
        };
        let before = repo.commit_state(parent)?.objects;
        let after = repo.commit_state(cid)?.objects;
        type Seed = (String, String, Option<String>, Vec<Segment>, Vec<Segment>);
        let seeds: Vec<Seed> = c
            .extra
            .get("retroactive")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| {
                let oid = text_of(&r["object_id"]);
                let tl = |m: &BTreeMap<String, ubos_version::ObjectState>| {
                    m.get(&oid).map(|o| o.timeline.clone()).unwrap_or_default()
                };
                (
                    oid.clone(),
                    text_of(&r["from"]),
                    s(r, "to").map(str::to_string),
                    tl(&before),
                    tl(&after),
                )
            })
            .collect();
        // Derived outputs of the branch history in commit order.
        let head = repo.head(branch)?;
        let mut anc: BTreeSet<u64> = BTreeSet::new();
        let mut todo = vec![head];
        while let Some(x) = todo.pop() {
            if anc.insert(x) {
                todo.extend(repo.commit(x)?.parents.iter().copied());
            }
        }
        let mut commits: Vec<(u64, Vec<Value>)> = Vec::new();
        for x in anc {
            let cx = repo.commit(x)?;
            let derived = cx
                .extra
                .get("derived")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !derived.is_empty() {
                commits.push((cx.cseq, derived));
            }
        }
        commits.sort_by_key(|x| x.0);
        let outputs: Vec<(String, String, String, String)> = commits
            .iter()
            .flat_map(|(_, d)| d.iter())
            .map(|d| {
                (
                    text_of(&d[0]),
                    text_of(&d[1]),
                    text_of(&d[2]),
                    text_of(&d[3]),
                )
            })
            .collect();
        let mut inputs_of: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (_, _, _, rid) in &outputs {
            if !inputs_of.contains_key(rid) {
                let ins = repo
                    .store
                    .meta(&format!("derivation:{rid}"))?
                    .and_then(|r| r["inputs"].as_array().cloned())
                    .unwrap_or_default();
                inputs_of.insert(rid.clone(), ins);
            }
        }
        let seeded = |i: &Value| {
            let (obj, asof, ver) = (
                text_of(&i["object"]),
                text_of(&i["asof"]),
                text_of(&i["version"]),
            );
            seeds.iter().any(|(oid, a, b, tb, ta)| {
                if obj != *oid || lt(Some(&asof), Some(a)) || !lt(Some(&asof), b.as_deref()) {
                    return false;
                }
                let old = at(tb, &asof);
                let new = at(ta, &asof);
                old.is_some_and(|o| o.version == ver) && new.is_none_or(|n| n.version != ver)
            })
        };
        let mut hit_versions: BTreeSet<String> = BTreeSet::new();
        let mut hit_entries: BTreeSet<String> = BTreeSet::new();
        loop {
            let mut changed = false;
            for (kind, oid, h, rid) in &outputs {
                if (kind == "object" && hit_versions.contains(h))
                    || (kind == "entry" && hit_entries.contains(oid))
                {
                    continue;
                }
                let hit = inputs_of.get(rid).into_iter().flatten().any(|i| {
                    seeded(i)
                        || hit_versions.contains(s(i, "version").unwrap_or_default())
                        || hit_entries.contains(s(i, "object").unwrap_or_default())
                });
                if hit {
                    if kind == "object" {
                        hit_versions.insert(h.clone());
                    } else {
                        hit_entries.insert(oid.clone());
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut impacted: Vec<(String, u64, String)> = Vec::new();
        for h in &hit_versions {
            let oid = text_of(&repo.version(h)?["object_id"]);
            let cseq = version_cseq(&mut repo, branch, &oid, h, u64::MAX)?;
            impacted.push((oid, cseq, h.clone()));
        }
        impacted.sort();
        Ok(json!({
            "impacted": impacted.iter().map(|(o, _, h)| json!({"object_id": o, "version_hash": h})).collect::<Vec<_>>(),
            "entries": hit_entries,
        }))
    }

    // ---- bulk processes (STD-TXN-070) ----

    /// `bulk`: a job that commits its changes in chunks of at most `max_changes`, each a
    /// child process; `independent` commits on the branch, `all_or_nothing` on a staging
    /// branch merged once on completion; a failed job is resumed after its last committed
    /// child.
    pub fn bulk(&mut self, i: &Value) -> Result<Value, BpaError> {
        let pid = s(i, "process_id").unwrap_or_default().to_string();
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let changes = i
            .get("changes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let size = match i.get("max_changes") {
            None | Some(Value::Null) => 100_000,
            Some(v) => match v.as_u64() {
                Some(n) if n >= 1 && !v.is_boolean() => n as usize,
                _ => return Err(invalid("max_changes is a positive integer")),
            },
        };
        let mode = s(i, "mode").unwrap_or("independent").to_string();
        if mode != "independent" && mode != "all_or_nothing" {
            return Err(invalid("mode is independent or all_or_nothing"));
        }
        let key = format!("bulk:{pid}");
        let mut b = match self.store.meta(&key)? {
            Some(b) if i.get("resume") == Some(&json!(true)) => {
                if s(&b, "state") != Some("failed") {
                    return Err(BpaError::new(
                        "FLOW.TRANSITION_NOT_ALLOWED",
                        "only a failed bulk process is resumed",
                    ));
                }
                b
            }
            None if i.get("resume") == Some(&json!(true)) => {
                return Err(not_found("unknown bulk process"));
            }
            Some(_) => {
                return Err(BpaError::new("TXN.ALREADY_EXISTS", "bulk process exists"));
            }
            None => {
                let mut next = self.next_process;
                let p = process(
                    self.store.as_mut(),
                    Some(&json!({"process_id": s(i, "process_id"), "kind": "job"})),
                    "job",
                    &mut next,
                )?;
                self.next_process = next;
                let now = self.now.clone();
                Repo::new(self.store.as_mut(), &self.registry).check_active(&branch, &now)?;
                let mut b = json!({"branch": branch, "mode": mode, "staging": null, "children": [], "state": "running", "attempts": 0});
                if mode == "all_or_nothing" {
                    let staging = s(i, "staging").unwrap_or_default().to_string();
                    self.other("create_branch", &json!({"name": staging, "from": branch}))?;
                    b["staging"] = json!(staging);
                }
                self.store.put_meta(&key, &b)?;
                record(self.store.as_mut(), &p, "running", None)?;
                b
            }
        };
        let chunks: Vec<&[Value]> = changes.chunks(size).collect();
        let target = s(&b, "staging")
            .unwrap_or_else(|| s(&b, "branch").unwrap_or("main"))
            .to_string();
        let done = b["children"].as_array().map(Vec::len).unwrap_or_default();
        for (n, chunk) in chunks.iter().enumerate().skip(done) {
            b["attempts"] = json!(b["attempts"].as_u64().unwrap_or_default() + 1);
            self.store.put_meta(&key, &b)?;
            let input = json!({"branch": target, "changes": chunk,
                "process": {"kind": "job", "trigger": {"type": "job", "ref": pid}, "parent_process": pid, "bulk": pid}});
            match self.commit(&input) {
                Err(e) => {
                    b["state"] = json!("failed");
                    self.store.put_meta(&key, &b)?;
                    self.set_process_state(&pid, "failed")?;
                    return Ok(
                        json!({"state": "failed", "error": e.code, "children": b["children"]}),
                    );
                }
                Ok(r) => {
                    b["children"].as_array_mut().expect("list").push(
                        json!({"n": n + 1, "branch": target, "cseq": r["cseq"], "changes": chunk.len()}),
                    );
                    self.store.put_meta(&key, &b)?;
                }
            }
        }
        let mut out = json!({"state": "committed", "children": b["children"]});
        if let Some(staging) = s(&b, "staging").map(str::to_string) {
            let branch = text_of(&b["branch"]);
            let r = self.merge(&json!({"source": staging, "target": branch}))?;
            out["merge"] = json!({"cseq": r["cseq"]});
        }
        b["state"] = json!("committed");
        self.store.put_meta(&key, &b)?;
        self.set_process_state(&pid, "committed")?;
        Ok(out)
    }

    fn set_process_state(&mut self, pid: &str, state: &str) -> Result<(), BpaError> {
        let key = format!("process:{pid}");
        if let Some(mut rec) = self.store.meta(&key)? {
            rec["state"] = json!(state);
            self.store.put_meta(&key, &rec)?;
        }
        Ok(())
    }

    /// Number of child processes a bulk process started (attempts, STD-TXN-070).
    pub fn bulk_children(&mut self, pid: &str) -> Result<u64, BpaError> {
        Ok(self
            .store
            .meta(&format!("bulk:{pid}"))?
            .and_then(|b| b["attempts"].as_u64())
            .unwrap_or_default())
    }

    // ---- consistent coordinates across references (STD-TIME-013) ----

    /// Targets of a reference field at the coordinates of the source segment.
    pub fn follow(
        &mut self,
        st: &State,
        o: &ubos_version::ObjectState,
        body: &Value,
        field: &str,
        asof: &str,
    ) -> Result<Vec<Value>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let exts = crate::relations::extensions_of(&mut repo, st)?;
        let reg_ext;
        let reg: &Registry = if exts.is_empty() {
            &self.registry
        } else {
            reg_ext = self.registry.with_extensions(&exts)?;
            &reg_ext
        };
        let c = reg.class(&o.class)?;
        let Some(f) = c.fields.get(field) else {
            return Err(invalid(&format!("unknown field {field}")));
        };
        let is_list = s(f, "type") == Some("list")
            && f.get("constraints")
                .and_then(|c| c.get("item_type"))
                .and_then(|t| s(t, "type"))
                == Some("reference");
        let uris: Vec<String> = match body.get(field) {
            Some(Value::String(u)) if s(f, "type") == Some("reference") => vec![u.clone()],
            Some(Value::Array(items)) if is_list => items.iter().map(text_of).collect(),
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for uri in uris {
            if let Some(tid) = crate::relations::resolve(st, &uri)
                && let Some(t) = st.objects.get(&tid)
                && let Some(seg) = at(&t.timeline, asof)
            {
                let mut x = segment(&mut repo, &seg.from, seg.to.as_deref(), &seg.version)?;
                x["object_id"] = json!(tid);
                out.push(x);
            }
        }
        Ok(out)
    }
}

/// Entry of the `derived` member of a commit record: `[kind, id, hash, record]`.
pub fn derived_entry(kind: &str, id: &str, hash: &str, rid: &str) -> Value {
    json!([kind, id, hash, rid])
}
