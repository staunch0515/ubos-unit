//! Portable archive of a VAE (STD-PROOF-070; PH-1 step 6b, DEC-059): export of every
//! commit with its state, versions, entries, branches and tags, and the load of a verified
//! archive into an empty store. The pure checks of an archive live in `ubos-proof`; the
//! importer here recomputes every state root from the loaded states (the check a verifier
//! without a store cannot make) and writes the commits through the same flush as `write`.

use crate::state::{CommitRecord, Entry, ObjectState, Repo, Segment, State, StoreNodes, storage};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use ubos_model::types::{BpaError, canonical, content_id};
use ubos_store::{BranchRef, EntryRow, Flush, ObjectRow};

/// Format name of the portable archive.
pub const ARCHIVE_FORMAT: &str = "ubos.archive.v1";

fn failed(m: String) -> BpaError {
    BpaError::new("PROOF.VERIFICATION_FAILED", m)
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn chunk_of(v: &Value) -> Result<String, BpaError> {
    content_id("ubos.chunk.v1", v)
}

/// The envelope of a stored version record (STD-OBJ-002): the record with `body_ref` for
/// its body, `tombstone` only when set.
fn envelope(rec: &Value) -> Result<Value, BpaError> {
    let mut env = json!({
        "object_id": rec["object_id"], "class": rec["class"], "class_version": rec["class_version"],
        "schema_version": rec["schema_version"], "kind": rec["kind"], "slug": rec["slug"],
        "valid": rec["valid"], "body_ref": chunk_of(&rec["body"])?,
    });
    if let Some(p) = rec.get("predecessor").filter(|p| !p.is_null()) {
        env["predecessor"] = p.clone();
    }
    if rec.get("tombstone") == Some(&json!(true)) {
        env["tombstone"] = json!(true);
    }
    if let Some(p) = rec.get("provenance_ref").filter(|p| !p.is_null()) {
        env["provenance_ref"] = p.clone();
    }
    Ok(env)
}

/// The stored version record of an archived envelope and its body.
fn stored_version(env: &Value, body: &Value) -> Value {
    let mut rec = env.clone();
    if let Some(o) = rec.as_object_mut() {
        o.remove("body_ref");
        o.insert("body".into(), body.clone());
        if !o.contains_key("tombstone") {
            o.insert("tombstone".into(), json!(false));
        }
    }
    rec
}

fn segments_json(tl: &[Segment]) -> Value {
    Value::Array(
        tl.iter()
            .map(|s| json!([s.from, s.to, s.version]))
            .collect(),
    )
}

fn object_json(o: &ObjectState) -> Value {
    json!({"class": o.class, "kind": o.kind, "slug": o.slug, "head": o.head,
        "timeline": segments_json(&o.timeline)})
}

fn object_state(v: &Value) -> Result<ObjectState, BpaError> {
    let bad =
        || failed("an object of the archive is not {class, kind, slug, head, timeline}".into());
    let tl = v
        .get("timeline")
        .and_then(Value::as_array)
        .ok_or_else(bad)?
        .iter()
        .map(|s| match s.as_array().map(Vec::as_slice) {
            Some([Value::String(f), to, Value::String(ver)]) => Ok(Segment {
                from: f.clone(),
                to: to.as_str().map(str::to_string),
                version: ver.clone(),
            }),
            _ => Err(bad()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ObjectState {
        class: text(v, "class").to_string(),
        kind: text(v, "kind").to_string(),
        slug: text(v, "slug").to_string(),
        head: text(v, "head").to_string(),
        timeline: tl,
    })
}

impl Repo<'_> {
    /// Every commit of the store: the parents of every branch head, transitively, as
    /// `(engine id, record)` by engine id.
    pub fn all_commits(&mut self) -> Result<BTreeMap<u64, CommitRecord>, BpaError> {
        let mut out = BTreeMap::new();
        let mut todo: Vec<u64> = Vec::new();
        for name in self.store.branch_names()? {
            todo.push(self.head(&name)?);
        }
        while let Some(id) = todo.pop() {
            if out.contains_key(&id) {
                continue;
            }
            let c = self.commit(id)?;
            todo.extend(c.parents.iter().copied());
            out.insert(id, c);
        }
        Ok(out)
    }

    /// `export_archive` (STD-PROOF-070): the archive and the counts of its records. The
    /// manifest is signed by the node (role `signer`) over its content ID.
    pub fn export_archive(&mut self, declarations: &[Value]) -> Result<Value, BpaError> {
        let mut chunks: BTreeMap<String, Value> = BTreeMap::new();
        let mut chunk = |v: &Value| -> Result<String, BpaError> {
            let cid = chunk_of(v)?;
            chunks.insert(cid.clone(), v.clone());
            Ok(cid)
        };
        let mut classes: Vec<String> = declarations
            .iter()
            .map(&mut chunk)
            .collect::<Result<_, _>>()?;
        classes.sort();
        let all = self.all_commits()?;
        let vae = self.store.authority();
        let mut versions: BTreeMap<String, Value> = BTreeMap::new();
        let mut entries: BTreeMap<String, Value> = BTreeMap::new();
        let mut commits: Map<String, Value> = Map::new();
        let mut records: HashMap<u64, Value> = HashMap::new();
        for id in all.keys() {
            records.insert(*id, self.commit_record(*id)?);
        }
        for (id, c) in &all {
            let st = self.commit_state(*id)?;
            let mut objects = Map::new();
            for (oid, o) in &st.objects {
                for vh in std::iter::once(&o.head).chain(o.timeline.iter().map(|s| &s.version)) {
                    if !versions.contains_key(vh) {
                        let rec = self.store.version(vh)?;
                        chunk(&rec["body"])?;
                        versions.insert(vh.clone(), envelope(&rec)?);
                    }
                }
                objects.insert(oid.clone(), object_json(o));
            }
            let mut ledger: Vec<(String, String, Value)> = Vec::new();
            for (class, key, q) in &st.ledger {
                if q.is_empty() {
                    continue;
                }
                let mut items = Vec::new();
                for e in q {
                    chunk(&e.body)?;
                    entries.insert(e.entry_hash.clone(), self.entry_record(e)?);
                    let mut item =
                        json!({"entry_hash": e.entry_hash, "seq": e.seq, "cseq": e.cseq});
                    if let Some(s) = &e.source_seq {
                        item["source_seq"] = s.clone();
                    }
                    items.push(item);
                }
                ledger.push((
                    class.clone(),
                    key.clone(),
                    json!({"class": class, "ledger_key": q[0].ledger_key, "entries": items}),
                ));
            }
            ledger.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
            let mut record = records[id].clone();
            record.as_object_mut().map(|r| r.remove("commit_id"));
            commits.insert(
                c.commit_id.clone(),
                json!({"record": record, "objects": objects,
                    "ledger": ledger.into_iter().map(|l| l.2).collect::<Vec<_>>(),
                    "hidden": c.hidden, "ops": c.ops}),
            );
        }
        let cid_of = |id: u64| {
            all.get(&id)
                .map(|c| c.commit_id.clone())
                .unwrap_or_default()
        };
        let mut refs = Map::new();
        let mut heads = Map::new();
        for name in self.store.branch_names()? {
            let head = self.head(&name)?;
            let m = self.branch_meta(&name)?;
            let mut r = json!({"head": cid_of(head), "kind": m.kind, "created_at": m.created_at,
                "active_at": m.active_at, "created_from": cid_of(m.created_from)});
            if let Some(pl) = &m.parent_line {
                let mut p = json!({"branch": pl.branch, "mode": pl.mode});
                if let Some(pin) = pl.pin {
                    p["pin"] = json!(cid_of(pin));
                }
                r["parent_line"] = p;
            }
            refs.insert(name.clone(), r);
            heads.insert(
                name,
                json!({"commit": cid_of(head), "root": all[&head].root}),
            );
        }
        let mut tags = Map::new();
        for name in self.store.tag_names()? {
            if let Some(t) = self.store.tag_rec(&name)? {
                tags.insert(
                    name,
                    json!({"branch": t["branch"], "commit": cid_of(t["commit"].as_u64().unwrap_or_default()),
                        "deleted": t["deleted"] == json!(true), "signature": t["signature"], "signer": t["signer"]}),
                );
            }
        }
        let counts = json!({"chunks": chunks.len(), "versions": versions.len(), "entries": entries.len(),
            "commits": commits.len(), "refs": refs.len(), "tags": tags.len()});
        let mut manifest = json!({"format": ARCHIVE_FORMAT, "vae": vae, "counts": counts, "heads": heads,
            "classes": classes});
        manifest["signature"] =
            ubos_store::node::sign(self.store, &chunk_of(&manifest)?, "signer")?;
        let archive = json!({"manifest": manifest, "chunks": chunks, "versions": versions, "entries": entries,
            "commits": commits, "refs": refs, "tags": tags, "dids": [ubos_store::node::did(self.store)?]});
        // the chunk count depends on how an implementation stores content, so it is left out
        let summary = json!({"version_records": versions.len(), "entries": entries.len(),
            "commits": commits.len(), "refs": refs.len(), "tags": tags.len()});
        Ok(json!({"counts": summary, "archive": archive}))
    }

    /// Load a verified archive into an empty store (STD-PROOF-070): commits in parent order
    /// through the flush of `write`, with the delta of each state against its first parent
    /// as the write set, the tree nodes from the parent's root, every state root and head
    /// root checked; then the branch metadata and the tags. Returns `{branches: {name:
    /// head cseq}}`.
    pub fn import_archive(&mut self, a: &Value) -> Result<Value, BpaError> {
        let chunks = a
            .get("chunks")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let versions = a
            .get("versions")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let entry_recs = a
            .get("entries")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let commits = a
            .get("commits")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let refs = a
            .get("refs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let manifest = a.get("manifest").cloned().unwrap_or(Value::Null);
        // states of every archived commit
        let mut states: HashMap<String, State> = HashMap::new();
        for (cid, c) in &commits {
            let mut st = State::default();
            for (oid, o) in c
                .get("objects")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let o = object_state(o)?;
                if o.head != *"" {
                    for vh in std::iter::once(&o.head).chain(o.timeline.iter().map(|s| &s.version))
                    {
                        if !versions.contains_key(vh) {
                            return Err(failed(
                                "an object names a version that is not in the archive".into(),
                            ));
                        }
                    }
                }
                st.objects.insert(oid.clone(), o);
            }
            for grp in c
                .get("ledger")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let class = text(grp, "class").to_string();
                let key = canonical(grp.get("ledger_key").unwrap_or(&Value::Null))?;
                for it in grp
                    .get("entries")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let eh = text(it, "entry_hash");
                    let rec = entry_recs
                        .get(eh)
                        .ok_or_else(|| failed(format!("entry {eh} is not in the archive")))?;
                    let body = chunks
                        .get(text(rec, "body_ref"))
                        .ok_or_else(|| failed(format!("the body of entry {eh} is missing")))?;
                    let opt = |k: &str| rec.get(k).filter(|v| !v.is_null()).cloned();
                    let e = Entry {
                        entry_id: text(rec, "entry_id").to_string(),
                        class: text(rec, "class").to_string(),
                        ledger_key: rec.get("ledger_key").cloned().unwrap_or(Value::Null),
                        valid_at: text(rec, "valid_at").to_string(),
                        body: body.clone(),
                        corrects: opt("corrects").and_then(|v| serde_json::from_value(v).ok()),
                        correction_type: opt("correction_type")
                            .and_then(|v| v.as_str().map(str::to_string)),
                        posting_group: opt("posting_group")
                            .and_then(|v| v.as_str().map(str::to_string)),
                        seq: it.get("seq").and_then(Value::as_u64).unwrap_or_default(),
                        cseq: it.get("cseq").and_then(Value::as_u64).unwrap_or_default(),
                        entry_hash: eh.to_string(),
                        source_seq: it.get("source_seq").filter(|v| !v.is_null()).cloned(),
                        provenance_ref: opt("provenance_ref")
                            .and_then(|v| v.as_str().map(str::to_string)),
                        source_refs: opt("source_refs")
                            .and_then(|v| serde_json::from_value(v).ok()),
                    };
                    st.queue(&class, &key).push(e);
                }
            }
            let root = text(&c["record"], "root");
            if ubos_tree::tree_root(&self.state_entries(&st)?)? != root {
                return Err(failed(format!(
                    "the state of commit {cid} does not give its root"
                )));
            }
            states.insert(cid.clone(), st);
        }
        // commits in parent order
        let mut order: Vec<String> = Vec::new();
        let mut done: HashSet<String> = HashSet::new();
        let mut pending: Vec<String> = commits.keys().cloned().collect();
        pending.sort_by(|a, b| {
            let ca = commits[a]["record"]["cseq"].as_u64();
            let cb = commits[b]["record"]["cseq"].as_u64();
            (ca, a).cmp(&(cb, b))
        });
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|cid| {
                let ready = commits[cid]["record"]["parents"]
                    .as_array()
                    .is_some_and(|ps| {
                        ps.iter()
                            .all(|p| done.contains(p.as_str().unwrap_or_default()))
                    });
                if ready {
                    order.push(cid.clone());
                    done.insert(cid.clone());
                }
                !ready
            });
            if pending.len() == before {
                return Err(failed("the commits of the archive form a cycle".into()));
            }
        }
        let mut ids: HashMap<String, u64> = HashMap::new();
        let mut heads: HashMap<String, u64> = HashMap::new(); // branch → engine id of its head
        let mut written_versions: HashSet<String> = HashSet::new();
        let empty = ubos_tree::build(&[])?;
        for cid in &order {
            let c = &commits[cid];
            let rec = &c["record"];
            let branch = text(rec, "branch").to_string();
            let cseq = rec["cseq"].as_u64().unwrap_or_default();
            let parent_ids: Vec<String> = rec["parents"]
                .as_array()
                .map(|ps| {
                    ps.iter()
                        .map(|p| p.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let parents: Vec<u64> = parent_ids.iter().map(|p| ids[p]).collect();
            // the branch reference, created with the branch's first own commit
            let expected_head = match heads.get(&branch) {
                Some(h) => *h,
                None => {
                    let r = if cseq == 0 {
                        BranchRef {
                            head: 0,
                            head_cseq: 0,
                            parent: None,
                        }
                    } else {
                        let from = text(&refs[&branch], "created_from");
                        let fid = *ids
                            .get(from)
                            .ok_or_else(|| failed("a branch names a missing commit".into()))?;
                        let fc = self.commit(fid)?;
                        BranchRef {
                            head: fid,
                            head_cseq: fc.cseq,
                            parent: Some((fc.branch.clone(), fc.cseq)),
                        }
                    };
                    if !self.store.create_branch(&branch, &r)? {
                        return Err(BpaError::new("TXN.ALREADY_EXISTS", "branch exists"));
                    }
                    r.head
                }
            };
            let st = &states[cid];
            let parent_state = parent_ids.first().map(|p| &states[p]);
            let hidden: BTreeSet<String> = c["hidden"]
                .as_array()
                .map(|h| {
                    h.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // write set: objects and entries that differ from the first parent's state
            let mut objects = Vec::new();
            let mut tree: BTreeMap<String, Option<String>> = BTreeMap::new();
            let mut versions_out = Vec::new();
            let oids: BTreeSet<&String> = st
                .objects
                .keys()
                .chain(parent_state.map(|p| p.objects.keys()).into_iter().flatten())
                .collect();
            for oid in oids {
                let new = st.objects.get(oid);
                let old = parent_state.and_then(|p| p.objects.get(oid));
                if new == old || (new.is_none() && hidden.contains(oid)) {
                    continue;
                }
                let any = new.or(old).expect("changed object");
                if any.kind != "system" {
                    tree.insert(format!("o/{oid}"), new.map(|o| o.head.clone()));
                }
                if let Some(o) = new {
                    for vh in std::iter::once(&o.head).chain(o.timeline.iter().map(|s| &s.version))
                    {
                        if written_versions.insert(vh.clone()) {
                            let env = &versions[vh];
                            let body = chunks.get(text(env, "body_ref")).ok_or_else(|| {
                                failed(format!("the body of version {vh} is missing"))
                            })?;
                            versions_out.push((vh.clone(), stored_version(env, body)));
                        }
                    }
                }
                // the field index rows of the head (DSN-DVM-902), rebuilt on import
                let mut index = Vec::new();
                if let Some(o) = new
                    && (!self.reg.indexed_fields(&o.class).is_empty()
                        || crate::state::has_relationships(self.reg, &o.class))
                    && let Some(env) = versions.get(&o.head)
                    && let Some(body) = chunks.get(text(env, "body_ref"))
                {
                    index =
                        crate::state::index_rows(self.reg, &o.class, &stored_version(env, body));
                }
                objects.push(ObjectRow {
                    object_id: oid.clone(),
                    class: any.class.clone(),
                    instant: self
                        .reg
                        .class(&any.class)
                        .map(|c| c.valid_time == "instant")
                        .unwrap_or(false),
                    state: new.map(serde_json::to_value).transpose().map_err(storage)?,
                    index,
                });
            }
            let mut entries = Vec::new();
            for (class, key, q) in &st.ledger {
                let old = parent_state.map(|p| p.entries(class, key)).unwrap_or(&[]);
                for e in q.iter().filter(|e| {
                    !old.iter()
                        .any(|o| o.entry_hash == e.entry_hash && o.seq == e.seq)
                }) {
                    tree.insert(Self::entry_key(e)?, Some(e.entry_hash.clone()));
                    entries.push(EntryRow {
                        entry_id: e.entry_id.clone(),
                        class: class.clone(),
                        ledger_key: key.clone(),
                        entry: serde_json::to_value(e).map_err(storage)?,
                    });
                }
            }
            let written = match parent_ids.first() {
                Some(p) => {
                    let prev = text(&commits[p]["record"], "root");
                    ubos_tree::update(&mut StoreNodes(&mut *self.store), prev, &tree)?
                }
                None => {
                    if !tree.is_empty() {
                        return Err(failed(format!("commit {cid} has no parent but a state")));
                    }
                    ubos_tree::Written {
                        root: empty.root.clone(),
                        nodes: empty.nodes.clone(),
                    }
                }
            };
            if written.root != text(rec, "root") {
                return Err(failed(format!(
                    "the state of commit {cid} does not give its root"
                )));
            }
            let id = self.store.next_commit_id()?;
            let mut extra = Map::new();
            extra.insert(
                "signatures".into(),
                rec.get("signatures").cloned().unwrap_or(json!([])),
            );
            extra.insert(
                "objects".into(),
                serde_json::to_value(&objects).map_err(storage)?,
            );
            let record = CommitRecord {
                parents,
                cseq,
                branch: branch.clone(),
                root: written.root.clone(),
                commit_id: cid.clone(),
                tx_time: rec
                    .get("tx_time")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                ops: serde_json::from_value(c.get("ops").cloned().unwrap_or(json!({})))
                    .unwrap_or_default(),
                hidden: hidden.iter().cloned().collect(),
                extra,
            };
            let f = Flush {
                versions: versions_out,
                commit_id: id,
                commit: serde_json::to_value(&record).map_err(storage)?,
                branch: branch.clone(),
                cseq,
                expected_head,
                objects,
                entries,
                nodes: written
                    .nodes
                    .iter()
                    .map(|(h, n)| (h.clone(), n.to_json()))
                    .collect(),
                ..Flush::default()
            };
            self.branches.remove(&branch);
            if !self.store.flush(&f)? {
                return Err(storage(format!("cannot import commit {cid}")));
            }
            self.cache.insert(id, record);
            ids.insert(cid.clone(), id);
            heads.insert(branch, id);
        }
        // branches without own commits, metadata, head roots and tags
        let mut names: Vec<&String> = refs.keys().collect();
        names.sort();
        let mut out = Map::new();
        for name in names {
            let r = &refs[name];
            let head_cid = text(r, "head");
            let head = *ids
                .get(head_cid)
                .ok_or_else(|| failed("a branch names a missing commit".into()))?;
            if !heads.contains_key(name) {
                let hc = self.commit(head)?;
                let b = BranchRef {
                    head,
                    head_cseq: hc.cseq,
                    parent: Some((hc.branch.clone(), hc.cseq)),
                };
                if !self.store.create_branch(name, &b)? {
                    return Err(BpaError::new("TXN.ALREADY_EXISTS", "branch exists"));
                }
            } else if heads[name] != head {
                return Err(failed(format!("the head of {name} is not its last commit")));
            }
            let hc = self.commit(head)?;
            if manifest["heads"][name]["root"] != json!(hc.root) {
                return Err(failed(format!(
                    "the head root of {name} differs from the manifest"
                )));
            }
            let mut pl = r.get("parent_line").filter(|p| p.is_object()).cloned();
            if let Some(p) = pl.as_mut()
                && let Some(pin) = p.get("pin").and_then(Value::as_str)
            {
                let pid = *ids
                    .get(pin)
                    .ok_or_else(|| failed("a parent line pins a missing commit".into()))?;
                p["pin"] = json!(pid);
            }
            let from = *ids
                .get(text(r, "created_from"))
                .ok_or_else(|| failed("a branch names a missing commit".into()))?;
            self.store.put_branch_meta_rec(
                name,
                &json!({"kind": r["kind"], "created_at": r["created_at"], "active_at": r["active_at"],
                    "parent_line": pl, "created_from": from}),
            )?;
            self.branches.remove(name);
            out.insert(name.clone(), json!(hc.cseq));
        }
        for (name, t) in a
            .get("tags")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let cid = text(t, "commit");
            let id = *ids
                .get(cid)
                .ok_or_else(|| failed("a tag names a missing commit".into()))?;
            self.store.put_tag_rec(
                name,
                &json!({"commit": id, "commit_id": cid, "branch": t["branch"], "signer": t["signer"],
                    "deleted": t["deleted"] == json!(true), "signature": t["signature"]}),
            )?;
        }
        Ok(json!({"branches": out}))
    }
}
