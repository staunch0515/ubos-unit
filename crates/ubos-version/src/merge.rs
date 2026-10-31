//! Merge (STD-VER-040…056, DEC-032) and diff (STD-VER-030).

use crate::state::{Entry, ObjectState, Repo, Segment, State, Tx, at};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_model::types::{BpaError, canonical, content_id};

/// Recorded resolutions supplied with a merge (STD-VER-058), keyed by object and path.
#[derive(Debug, Default, Clone)]
pub struct Resolver {
    given: BTreeMap<(String, Option<String>), Value>,
    /// Resolutions applied, as recorded in the merge commit.
    pub used: BTreeMap<(String, Option<String>), Value>,
}

impl Resolver {
    /// Parse the resolutions of a merge input.
    pub fn new(items: &[Value]) -> Result<Resolver, BpaError> {
        let mut given = BTreeMap::new();
        for r in items {
            let ok = r.is_object()
                && r.get("object_id")
                    .and_then(Value::as_str)
                    .is_some_and(|o| !o.is_empty())
                && r.get("resolver")
                    .and_then(Value::as_str)
                    .is_some_and(|o| !o.is_empty())
                && r.get("choice").is_some();
            if !ok {
                return Err(BpaError::new(
                    "MODEL.DECLARATION_INCOMPLETE",
                    "a resolution names object_id, choice and resolver",
                ));
            }
            let choice = r["choice"].as_str().unwrap_or_default();
            if !matches!(choice, "source" | "target" | "base" | "custom")
                || (choice == "custom") != r.get("value").is_some()
            {
                return Err(BpaError::new(
                    "MODEL.INVALID_VALUE",
                    "choice is source, target, base, or custom with a value",
                ));
            }
            let key = (
                r["object_id"].as_str().unwrap_or_default().to_string(),
                r.get("path").and_then(Value::as_str).map(str::to_string),
            );
            given.insert(key, r.clone());
        }
        Ok(Resolver {
            given,
            used: BTreeMap::new(),
        })
    }

    /// Resolutions that matched no conflict.
    pub fn unused(&self) -> bool {
        self.given.keys().any(|k| !self.used.contains_key(k))
    }

    /// The resolutions applied, in object and path order.
    pub fn records(&self) -> Vec<Value> {
        self.used.values().cloned().collect()
    }
}

/// A chosen value of a resolution: a version hash, a field value, or absence.
enum Chosen {
    Version(Option<String>),
    Field(Option<Value>),
}

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Absent,
    Version(String),
    Merged(Map<String, Value>),
}

/// The merge base of two commit sets as a preview reports it (STD-VER-057): one commit
/// ID, or the content ID of the sorted best ancestors' commit IDs.
pub fn base_id(ids: &[String]) -> Result<String, BpaError> {
    if ids.len() == 1 {
        return Ok(ids[0].clone());
    }
    content_id("ubos.chunk.v1", &json!(ids))
}

/// Conflicts without repetition, in order of first occurrence.
pub fn unique(items: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for c in items {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// A prepared merge (STD-VER-050…058).
pub struct MergePlan {
    /// The transaction on the target holding the merged state.
    pub tx: Tx,
    /// The target state before the merge.
    pub target_state: State,
    /// Merge parents `[target, source]`.
    pub parents: Vec<u64>,
    /// Resolutions applied (`DAT-Resolution`).
    pub resolutions: Vec<Value>,
    /// Entries appended by the merge: `entry_id → {seq, source_seq}`.
    pub appended: Map<String, Value>,
}

fn same(a: Option<&ObjectState>, b: Option<&ObjectState>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.timeline == y.timeline && x.head == y.head,
        _ => false,
    }
}

impl Repo<'_> {
    fn ancestors(&mut self, id: u64) -> Result<BTreeSet<u64>, BpaError> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(c) = stack.pop() {
            if seen.insert(c) {
                stack.extend(self.commit(c)?.parents);
            }
        }
        Ok(seen)
    }

    /// True when `anc` is an ancestor of (or equal to) `id`.
    pub fn ancestors_contains(&mut self, id: u64, anc: u64) -> Result<bool, BpaError> {
        Ok(self.ancestors(id)?.contains(&anc))
    }

    fn ancestors_of(&mut self, ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>, BpaError> {
        let mut out = BTreeSet::new();
        for id in ids {
            out.extend(self.ancestors(*id)?);
        }
        Ok(out)
    }

    /// Best common ancestors of two commit sets, sorted by commit ID (STD-VER-040).
    pub fn best_ancestors(
        &mut self,
        xs: &BTreeSet<u64>,
        ys: &BTreeSet<u64>,
    ) -> Result<Vec<u64>, BpaError> {
        let common: BTreeSet<u64> = self
            .ancestors_of(xs)?
            .intersection(&self.ancestors_of(ys)?)
            .copied()
            .collect();
        // A common ancestor is dominated when it is a strict ancestor of another one: one
        // walk from the parents of every common commit marks them all (O(history)).
        let mut dominated: BTreeSet<u64> = BTreeSet::new();
        let mut stack: Vec<u64> = Vec::new();
        for c in &common {
            stack.extend(self.commit(*c)?.parents);
        }
        while let Some(c) = stack.pop() {
            if dominated.insert(c) {
                stack.extend(self.commit(c)?.parents);
            }
        }
        let mut best = Vec::new();
        for c in common.difference(&dominated) {
            best.push((self.meta(*c)?.commit_id, *c));
        }
        best.sort();
        Ok(best.into_iter().map(|(_, c)| c).collect())
    }

    /// Merge base of two sets of commits; a virtual base for several best ancestors (STD-VER-040).
    pub fn base_state(
        &mut self,
        xs: &BTreeSet<u64>,
        ys: &BTreeSet<u64>,
        tx: &mut Tx,
    ) -> Result<State, BpaError> {
        self.base_state_in(xs, ys, tx, None)
    }

    /// `base_state` restricted to the objects `ids` when given (merge previews): the
    /// objects outside the set are equal on every side, so the virtual base needs none of
    /// them (its ledger is never consulted by `merge_states`).
    pub fn base_state_in(
        &mut self,
        xs: &BTreeSet<u64>,
        ys: &BTreeSet<u64>,
        tx: &mut Tx,
        ids: Option<&BTreeSet<String>>,
    ) -> Result<State, BpaError> {
        let best = self.best_ancestors(xs, ys)?;
        self.base_state_of(&best, tx, ids)
    }

    /// `base_state_in` for best ancestors already found.
    fn base_state_of(
        &mut self,
        best: &[u64],
        tx: &mut Tx,
        ids: Option<&BTreeSet<String>>,
    ) -> Result<State, BpaError> {
        let Some(&first) = best.first() else {
            // step 13d: branches without a common ancestor (an overlay and a branch of
            // another line) have no merge base
            return Err(BpaError::new(
                "VER.MERGE_NOT_ALLOWED",
                "the branches share no history: no merge base",
            ));
        };
        let mut acc = self.state_of(first, ids)?;
        let mut tips: BTreeSet<u64> = [first].into();
        for c in best.iter().skip(1) {
            let b = self.base_state_in(&tips, &[*c].into(), tx, ids)?;
            let other = self.state_of(*c, ids)?;
            acc = self
                .merge_states(&b, &other, &acc, true, None, None, false, &mut None, tx)?
                .0;
            tips.insert(*c);
        }
        Ok(acc)
    }

    /// Three-way merge of states (STD-VER-050…054). `keep` leaves conflicting objects as
    /// on the target (previews); `res` supplies recorded resolutions (STD-VER-058).
    #[allow(clippy::too_many_arguments)]
    pub fn merge_states(
        &mut self,
        b: &State,
        s: &State,
        t: &State,
        virtual_: bool,
        cseq: Option<u64>,
        branch: Option<&str>,
        keep: bool,
        res: &mut Option<Resolver>,
        tx: &mut Tx,
    ) -> Result<(State, Vec<Value>), BpaError> {
        let mut conflicts = Vec::new();
        let mut out = State {
            objects: Default::default(),
            ledger: t.ledger.clone(),
        };
        let ids: BTreeSet<&String> = b
            .objects
            .keys()
            .chain(s.objects.keys())
            .chain(t.objects.keys())
            .collect();
        for oid in ids {
            let (o, c) = self.merge_object(
                oid,
                b.objects.get(oid),
                s.objects.get(oid),
                t.objects.get(oid),
                virtual_,
                res,
                tx,
            )?;
            let o = if !c.is_empty() && keep && !virtual_ {
                t.objects.get(oid).cloned()
            } else {
                o
            };
            conflicts.extend(c);
            if let Some(o) = o {
                out.objects.insert(oid.clone(), o);
            }
        }
        for (class, key, q) in &s.ledger {
            for e in q {
                let tq = out.queue(class, key);
                if tq.iter().any(|x| x.entry_id == e.entry_id) {
                    continue;
                }
                for target in e.corrects.iter().flatten() {
                    if !tq.iter().any(|x| &x.entry_id == target) && !virtual_ {
                        return Err(BpaError::new(
                            "LEDG.CORRECTION_TARGET_MISSING",
                            "corrected entry missing on the target",
                        ));
                    }
                }
                let mut ne: Entry = e.clone();
                ne.seq = tq.len() as u64 + 1;
                ne.source_seq = Some(json!({"branch": branch, "seq": e.seq}));
                if let Some(c) = cseq {
                    ne.cseq = c;
                }
                tq.push(ne);
            }
        }
        Ok((out, conflicts))
    }

    /// The resolved value of a conflict, recording the resolution (STD-VER-058).
    #[allow(clippy::too_many_arguments)]
    fn resolution(
        &mut self,
        res: &mut Option<Resolver>,
        oid: &str,
        path: Option<&str>,
        b: Option<&Value>,
        s: Option<&Value>,
        t: Option<&Value>,
        tx: &mut Tx,
    ) -> Result<Option<Chosen>, BpaError> {
        let Some(r) = res.as_ref() else {
            return Ok(None);
        };
        let key = (oid.to_string(), path.map(str::to_string));
        let Some(rule) = r.given.get(&key).cloned() else {
            return Ok(None);
        };
        let choice = rule["choice"].as_str().unwrap_or_default();
        let picked: Option<Value> = match choice {
            "custom" => {
                if path.is_none() {
                    return Err(BpaError::new(
                        "MODEL.INVALID_VALUE",
                        "an object-level conflict has no custom value",
                    ));
                }
                Some(rule["value"].clone())
            }
            "source" => s.cloned(),
            "target" => t.cloned(),
            _ => b.cloned(),
        };
        let (chosen, out) = match path {
            None => {
                let h = picked.as_ref().and_then(Value::as_str).map(str::to_string);
                let body = match &h {
                    Some(h) => self.version_in(tx, h)?["body"].clone(),
                    None => Value::Null,
                };
                (body, Chosen::Version(h))
            }
            Some(_) => (picked.clone().unwrap_or(Value::Null), Chosen::Field(picked)),
        };
        let mut rec = json!({"object_id": oid, "choice": choice,
            "value_hash": content_id("ubos.chunk.v1", &chosen)?, "resolver": rule["resolver"]});
        if let Some(p) = path {
            rec["path"] = json!(p);
        }
        if let Some(reason) = rule.get("reason")
            && !reason.is_null()
        {
            rec["reason"] = reason.clone();
        }
        res.as_mut().expect("resolver").used.insert(key, rec);
        Ok(Some(out))
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_object(
        &mut self,
        oid: &str,
        bo: Option<&ObjectState>,
        so: Option<&ObjectState>,
        to: Option<&ObjectState>,
        virtual_: bool,
        res: &mut Option<Resolver>,
        tx: &mut Tx,
    ) -> Result<(Option<ObjectState>, Vec<Value>), BpaError> {
        if same(so, to) || same(so, bo) {
            return Ok((to.cloned(), vec![]));
        }
        if same(to, bo) {
            return Ok((so.cloned(), vec![]));
        }
        let meta = to.or(so).expect("an object present on one side").clone();
        if meta.kind == "system" {
            return Ok((to.cloned(), vec![]));
        }
        let tls: Vec<Vec<Segment>> = [bo, so, to]
            .iter()
            .map(|x| x.map(|o| o.timeline.clone()).unwrap_or_default())
            .collect();
        let mut cuts: BTreeSet<String> = BTreeSet::new();
        for tl in &tls {
            for seg in tl {
                cuts.insert(seg.from.clone());
                if let Some(e) = &seg.to {
                    cuts.insert(e.clone());
                }
            }
        }
        let cuts: Vec<String> = cuts.into_iter().collect();
        let mut pieces: Vec<(String, Option<String>, Piece)> = Vec::new();
        let mut conflicts = Vec::new();
        for (i, a) in cuts.iter().enumerate() {
            let b = cuts.get(i + 1).cloned();
            let v: Vec<Option<String>> = tls
                .iter()
                .map(|tl| at(tl, a).map(|s| s.version.clone()))
                .collect();
            let (vb, vs, vt) = (&v[0], &v[1], &v[2]);
            let piece = |x: &Option<String>| x.clone().map_or(Piece::Absent, Piece::Version);
            let r = if vs == vt || vs == vb {
                piece(vt)
            } else if vt == vb {
                piece(vs)
            } else if vs.is_none() || vt.is_none() || meta.kind == "raw_document" {
                let kind = if vs.is_some() && vt.is_some() {
                    "raw_document"
                } else {
                    "ended_vs_changed"
                };
                let chosen = if virtual_ {
                    None
                } else {
                    let (jb, js, jt) = (
                        vb.clone().map(Value::from),
                        vs.clone().map(Value::from),
                        vt.clone().map(Value::from),
                    );
                    self.resolution(res, oid, None, jb.as_ref(), js.as_ref(), jt.as_ref(), tx)?
                };
                match chosen {
                    Some(Chosen::Version(h)) => piece(&h),
                    _ => {
                        conflicts.push(json!({"object_id": oid, "kind": kind}));
                        piece(vb)
                    }
                }
            } else {
                let mut none = None;
                let (r, c) = self.merge_body(
                    oid,
                    vb.as_deref(),
                    vs.as_deref().unwrap_or_default(),
                    vt.as_deref().unwrap_or_default(),
                    if virtual_ { &mut none } else { res },
                    tx,
                )?;
                conflicts.extend(c);
                r
            };
            pieces.push((a.clone(), b, r));
        }
        if !conflicts.is_empty() && !virtual_ {
            return Ok((None, conflicts));
        }
        let mut coalesced: Vec<(String, Option<String>, Piece)> = Vec::new();
        for (a, b, r) in pieces {
            if r == Piece::Absent {
                continue;
            }
            match coalesced.last_mut() {
                Some(last) if last.1.as_deref() == Some(a.as_str()) && last.2 == r => last.1 = b,
                _ => coalesced.push((a, b, r)),
            }
        }
        let mut timeline = Vec::new();
        let mut created = Vec::new();
        for (a, b, r) in coalesced {
            let version = match r {
                Piece::Merged(body) => {
                    let pred = to
                        .and_then(|o| at(&o.timeline, &a))
                        .map(|s| s.version.clone());
                    // the slug and the lifecycle state (STD-CLS-004) of the predecessor
                    let (slug, state) = match &pred {
                        Some(p) => {
                            let v = self.version_in(tx, p)?;
                            (
                                v["slug"].as_str().unwrap_or_default().to_string(),
                                v.get("state").and_then(Value::as_str).map(str::to_string),
                            )
                        }
                        None => (meta.slug.clone(), None),
                    };
                    let vh = self.new_version(
                        tx,
                        oid,
                        &meta.class,
                        &slug,
                        &body,
                        &a,
                        b.as_deref(),
                        pred.as_deref(),
                        false,
                        None,
                        state.as_deref(),
                    )?;
                    created.push(vh.clone());
                    vh
                }
                Piece::Version(h) => h,
                Piece::Absent => unreachable!(),
            };
            timeline.push(Segment {
                from: a,
                to: b,
                version,
            });
        }
        let head = if let Some(h) = created.last() {
            h.clone()
        } else if so.is_some_and(|o| o.timeline == timeline) {
            so.map(|o| o.head.clone()).unwrap_or_default()
        } else {
            to.or(so).map(|o| o.head.clone()).unwrap_or_default()
        };
        let slug = self.version_in(tx, &head)?["slug"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Ok((
            Some(ObjectState {
                class: meta.class,
                kind: meta.kind,
                slug,
                head,
                timeline,
            }),
            conflicts,
        ))
    }

    /// The three-way merge of one object's bodies (STD-VER-082): base `vb`, source `vs`,
    /// target `vt`, with recorded resolutions (STD-VER-058); the merged body and the
    /// conflicts left.
    pub fn merge_object_body(
        &mut self,
        oid: &str,
        vb: Option<&str>,
        vs: &str,
        vt: &str,
        resolutions: &[Value],
        tx: &mut Tx,
    ) -> Result<(Map<String, Value>, Vec<Value>), BpaError> {
        let mut res = Some(Resolver::new(resolutions)?);
        let (piece, conflicts) = self.merge_body(oid, vb, vs, vt, &mut res, tx)?;
        let body = match piece {
            Piece::Merged(m) => m,
            _ => Map::new(),
        };
        Ok((body, conflicts))
    }

    fn merge_body(
        &mut self,
        oid: &str,
        vb: Option<&str>,
        vs: &str,
        vt: &str,
        res: &mut Option<Resolver>,
        tx: &mut Tx,
    ) -> Result<(Piece, Vec<Value>), BpaError> {
        // STD-CLS-061 (BPA 1.0-rc8, PH-2 step 7a): base, source and target in the current
        // schema version
        let bb = match vb {
            Some(h) => {
                let v = self.version_in(tx, h)?;
                crate::state::aligned_body(self, &v)?
            }
            None => Map::new(),
        };
        let v = self.version_in(tx, vs)?;
        let sb = crate::state::aligned_body(self, &v)?;
        let v = self.version_in(tx, vt)?;
        let tb = crate::state::aligned_body(self, &v)?;
        let keys: BTreeSet<&String> = bb.keys().chain(sb.keys()).chain(tb.keys()).collect();
        let mut out = Map::new();
        let mut conflicts = Vec::new();
        for k in keys {
            let (b, s, t) = (bb.get(k), sb.get(k), tb.get(k));
            let r: Option<Value> = if s == t || s == b {
                t.cloned()
            } else if t == b {
                s.cloned()
            } else {
                let path = format!("/{k}");
                match self.resolution(res, oid, Some(&path), b, s, t, tx)? {
                    Some(Chosen::Field(v)) => v,
                    _ => {
                        conflicts.push(json!({"object_id": oid, "kind": "field", "path": path}));
                        b.cloned()
                    }
                }
            };
            if let Some(v) = r {
                out.insert(k.clone(), v);
            }
        }
        Ok((Piece::Merged(out), conflicts))
    }

    /// Prepare the merge of `source` into `target` (STD-VER-050…056, -058): the transaction
    /// holding the merged state, the merge parents and the resolutions applied. `None`
    /// when the source is already merged.
    pub fn merge_prepare(
        &mut self,
        source: &str,
        target: &str,
        resolutions: &[Value],
    ) -> Result<Option<MergePlan>, BpaError> {
        let (s, t) = (self.head(source)?, self.head(target)?);
        if self.ancestors(t)?.contains(&s) {
            return Ok(None);
        }
        let mut tx = self.begin(target)?;
        self.load_all(&mut tx)?;
        let base = self.base_state(&[t].into(), &[s].into(), &mut tx)?;
        let sstate = self.commit_state(s)?;
        let tstate = tx.state.clone();
        let cseq = tx.cseq;
        let mut res = Some(Resolver::new(resolutions)?);
        let (st, conflicts) = self.merge_states(
            &base,
            &sstate,
            &tstate,
            false,
            Some(cseq),
            Some(source),
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
        let res = res.expect("resolver");
        if res.unused() {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "a resolution matches no conflict",
            ));
        }
        let mut appended = Map::new();
        for (_, _, q) in &st.ledger {
            for e in q {
                if e.cseq == cseq && e.source_seq.is_some() {
                    appended.insert(e.entry_id.clone(), json!({"seq": e.seq, "source_seq": e.source_seq.as_ref().map(|x| x["seq"].clone())}));
                }
            }
        }
        tx.state = st;
        Ok(Some(MergePlan {
            tx,
            target_state: tstate,
            parents: vec![t, s],
            resolutions: res.records(),
            appended,
        }))
    }

    /// The state after a commit, or only its objects `ids` (no ledger) when given.
    fn state_of(&mut self, id: u64, ids: Option<&BTreeSet<String>>) -> Result<State, BpaError> {
        match ids {
            None => self.commit_state(id),
            Some(ids) => {
                let c = self.commit(id)?;
                let ids: Vec<String> = ids.iter().cloned().collect();
                Ok(State {
                    objects: self.objects_in_ids(&c.branch, c.cseq, &ids)?,
                    ledger: Vec::new(),
                })
            }
        }
    }

    /// The objects a merge preview must look at (DSN-DVM-240 steps 3 and 4): the keys of
    /// the state tree that differ between each merge base and the source head, and between
    /// the merge bases themselves (STD-VER-031). Every other object is equal on the source
    /// and on every base, so the merge keeps the target's version of it. `None` when a
    /// branch involved has a parent line (STD-VER-080), whose tree does not hold the
    /// inherited objects.
    fn preview_objects(
        &mut self,
        bases: &[u64],
        source_head: u64,
        target_head: u64,
    ) -> Result<Option<BTreeSet<String>>, BpaError> {
        let mut branches: BTreeSet<String> = [self.commit(target_head)?.branch].into();
        let mut roots = Vec::new();
        for id in bases.iter().chain([&source_head]) {
            let c = self.commit(*id)?;
            branches.insert(c.branch.clone());
            roots.push(c.root);
        }
        for b in &branches {
            if self.branch_meta(b)?.parent_line.is_some() {
                return Ok(None);
            }
        }
        let source_root = roots.pop().expect("the source root");
        let mut out = BTreeSet::new();
        let collect = |keys: Vec<String>, out: &mut BTreeSet<String>| {
            out.extend(
                keys.into_iter()
                    .filter(|k| k.starts_with("o/"))
                    .map(|k| k[2..].to_string()),
            );
        };
        for (i, root) in roots.iter().enumerate() {
            collect(self.tree_diff(root, &source_root)?, &mut out);
            if i > 0 {
                collect(self.tree_diff(&roots[0], root)?, &mut out);
            }
        }
        Ok(Some(out))
    }

    /// The ledger keys a merge preview must look at: those of the entries appended by the
    /// commits reachable from the source head but not from the target head (their commit
    /// records list them, DEC-041). Entries of other keys are on the target already.
    /// `None` when a commit record does not list its appended entries.
    fn preview_queues(
        &mut self,
        source: &str,
        source_head: u64,
        on_target: &BTreeSet<u64>,
    ) -> Result<Option<BTreeSet<(String, String)>>, BpaError> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![source_head];
        let mut entry_ids: BTreeSet<String> = BTreeSet::new();
        while let Some(c) = stack.pop() {
            if on_target.contains(&c) || !seen.insert(c) {
                continue;
            }
            let rec = self.commit(c)?;
            let Some(appended) = rec.extra.get("appended").and_then(Value::as_array) else {
                return Ok(None);
            };
            entry_ids.extend(
                appended
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string),
            );
            stack.extend(rec.parents);
        }
        let cseq = self.meta(source_head)?.cseq;
        let mut out = BTreeSet::new();
        for id in &entry_ids {
            let Some(e) = self
                .entries_in(source, cseq, None, Some(id))?
                .into_iter()
                .next()
            else {
                return Ok(None);
            };
            out.insert((e.class.clone(), canonical(&e.ledger_key)?));
        }
        Ok(Some(out))
    }

    /// The states a merge preview compares (STD-VER-057): the merge bases (STD-VER-040), the
    /// target state, the merged state (conflicting objects as on the target) and the
    /// conflicts. Only the objects and ledger keys the merge can change are loaded
    /// (`preview_objects`, `preview_queues`), so the cost follows the size of the change and
    /// not the size of the VAE (NR-PERF-005, MET-PERF-014); the full states are loaded when
    /// a branch has a parent line or a commit record lacks its appended entries.
    pub fn preview_states(
        &mut self,
        source: &str,
        target: &str,
        tx: &mut Tx,
    ) -> Result<(Vec<u64>, State, State, Vec<Value>), BpaError> {
        let (s, t) = (self.head(source)?, self.head(target)?);
        let bases = self.best_ancestors(&[t].into(), &[s].into())?;
        let on_target = self.ancestors(t)?;
        if on_target.contains(&s) {
            // Nothing to merge: the preview is empty whatever the states hold.
            return Ok((bases, State::default(), State::default(), vec![]));
        }
        let (Some(ids), Some(queues)) = (
            self.preview_objects(&bases, s, t)?,
            self.preview_queues(source, s, &on_target)?,
        ) else {
            let tstate = self.commit_state(t)?;
            for (id, o) in &tstate.objects {
                tx.loaded.insert(id.clone(), Some(o.clone()));
            }
            for (c, k, q) in &tstate.ledger {
                tx.loaded_queues.insert((c.clone(), k.clone()));
                tx.loaded_entries
                    .extend(q.iter().map(|e| e.entry_id.clone()));
            }
            let (st, conflicts) = self.merge_result(source, target, tx)?;
            return Ok((bases, tstate, st, conflicts));
        };
        let (sc, tc) = (self.meta(s)?.cseq, self.meta(t)?.cseq);
        let mut sstate = self.state_of(s, Some(&ids))?;
        let mut tstate = self.state_of(t, Some(&ids))?;
        for (class, key) in &queues {
            let sq = self.entries_in(source, sc, Some((class, key)), None)?;
            let tq = self.entries_in(target, tc, Some((class, key)), None)?;
            tx.loaded_queues.insert((class.clone(), key.clone()));
            tx.loaded_entries
                .extend(tq.iter().map(|e| e.entry_id.clone()));
            sstate.ledger.push((class.clone(), key.clone(), sq));
            tstate.ledger.push((class.clone(), key.clone(), tq));
        }
        for oid in &ids {
            tx.loaded
                .insert(oid.clone(), tstate.objects.get(oid).cloned());
        }
        tx.state = tstate.clone();
        let base = self.base_state_of(&bases, tx, Some(&ids))?;
        let cseq = tx.cseq;
        let (st, conflicts) = self.merge_states(
            &base,
            &sstate,
            &tstate,
            false,
            Some(cseq),
            Some(source),
            true,
            &mut None,
            tx,
        )?;
        Ok((bases, tstate, st, unique(conflicts)))
    }

    /// The merged state of `source` into `target` for a preview (STD-VER-057): conflicting
    /// objects stay as on the target, and the conflicts are listed.
    pub fn merge_result(
        &mut self,
        source: &str,
        target: &str,
        tx: &mut Tx,
    ) -> Result<(State, Vec<Value>), BpaError> {
        let (s, t) = (self.head(source)?, self.head(target)?);
        let base = self.base_state(&[t].into(), &[s].into(), tx)?;
        let sstate = self.commit_state(s)?;
        let tstate = self.commit_state(t)?;
        let cseq = tx.cseq;
        let (st, conflicts) = self.merge_states(
            &base,
            &sstate,
            &tstate,
            false,
            Some(cseq),
            Some(source),
            true,
            &mut None,
            tx,
        )?;
        Ok((st, unique(conflicts)))
    }

    /// Merge the head of `source` into `target` without the checks of the engine (tools).
    pub fn merge(&mut self, source: &str, target: &str) -> Result<Value, BpaError> {
        let Some(plan) = self.merge_prepare(source, target, &[])? else {
            let t = self.head(target)?;
            return Ok(json!({"cseq": self.meta(t)?.cseq, "merged": false}));
        };
        let cseq = plan.tx.cseq;
        let appended = plan.appended.clone();
        self.write(plan.tx, plan.parents)?;
        Ok(json!({"cseq": cseq, "merged": true, "appended": appended}))
    }

    /// Changes between two states as `DAT-DiffEntry` items (STD-VER-030).
    pub fn diff_states(&mut self, sa: &State, sb: &State) -> Result<Vec<Value>, BpaError> {
        self.diff_states_in(sa, sb, None)
    }

    /// `diff_states` with versions that a transaction created but did not write (previews).
    pub fn diff_states_in(
        &mut self,
        sa: &State,
        sb: &State,
        tx: Option<&Tx>,
    ) -> Result<Vec<Value>, BpaError> {
        let mut out = Vec::new();
        let ids: BTreeSet<&String> = sa.objects.keys().chain(sb.objects.keys()).collect();
        for oid in ids {
            out.extend(self.diff_object(oid, sa.objects.get(oid), sb.objects.get(oid), tx)?);
        }
        let seen: BTreeSet<&String> = sa
            .ledger
            .iter()
            .flat_map(|x| x.2.iter())
            .map(|e| &e.entry_id)
            .collect();
        for (_, _, q) in &sb.ledger {
            for e in q {
                if !seen.contains(&e.entry_id) {
                    out.push(json!({"object_id": e.entry_id, "class": e.class, "op": "appended", "versions": {"a": null, "b": e.entry_hash}}));
                }
            }
        }
        Ok(out)
    }

    /// One object entry of a diff, or `None` when the object is unchanged.
    fn diff_object(
        &mut self,
        oid: &str,
        x: Option<&ObjectState>,
        y: Option<&ObjectState>,
        tx: Option<&Tx>,
    ) -> Result<Option<Value>, BpaError> {
        if let (Some(x), Some(y)) = (x, y)
            && x.head == y.head
            && x.timeline == y.timeline
        {
            return Ok(None);
        }
        if x.is_none() && y.is_none() {
            return Ok(None);
        }
        let class = y.or(x).map(|o| o.class.clone()).unwrap_or_default();
        let mut ent = json!({"object_id": oid, "class": class,
            "versions": {"a": x.map(|o| o.head.clone()), "b": y.map(|o| o.head.clone())}});
        match (x, y) {
            (None, _) => ent["op"] = json!("added"),
            (Some(x), Some(y)) => {
                let (vx, vy) = match tx {
                    Some(tx) => (self.version_in(tx, &x.head)?, self.version_in(tx, &y.head)?),
                    None => (self.version(&x.head)?, self.version(&y.head)?),
                };
                if vy["tombstone"] == true && vx["tombstone"] != true {
                    ent["op"] = json!("ended");
                } else {
                    let (bx, by) = (
                        vx["body"].as_object().cloned().unwrap_or_default(),
                        vy["body"].as_object().cloned().unwrap_or_default(),
                    );
                    let keys: BTreeSet<&String> = bx.keys().chain(by.keys()).collect();
                    let fields: Vec<Value> = keys
                        .into_iter()
                        .filter(|k| bx.get(*k) != by.get(*k))
                        .map(|k| json!({"path": format!("/{k}"), "old": bx.get(k), "new": by.get(k)}))
                        .collect();
                    let moved = vx["slug"] != vy["slug"] && fields.is_empty();
                    ent["op"] = json!(if moved { "moved" } else { "changed" });
                    if !fields.is_empty() {
                        ent["fields"] = json!(fields);
                    }
                }
            }
            (Some(_), None) => ent["op"] = json!("ended"),
        }
        Ok(Some(ent))
    }

    /// Diff between two coordinates `{branch, time}` (STD-VER-030).
    ///
    /// The state trees of the two coordinates are compared, and only the objects under
    /// differing keys, plus the System-kind objects (which the tree leaves out), are read, so
    /// the cost follows the size of the change (NR-PERF-005). When ledger keys differ, whose
    /// tree keys do not name the entry, the full comparison is used.
    pub fn diff(&mut self, a: &Value, b: &Value) -> Result<Value, BpaError> {
        let coord = |v: &Value| {
            (
                v["branch"].as_str().unwrap_or("main").to_string(),
                v.get("time").and_then(Value::as_u64),
            )
        };
        let (ab, at_) = coord(a);
        let (bb, bt) = coord(b);
        let system: Vec<String> = self
            .reg
            .declarations()
            .iter()
            .filter_map(|d| d.get("name").and_then(Value::as_str))
            .filter(|n| {
                self.reg
                    .class(n)
                    .is_ok_and(|c| c.kind.as_deref() == Some("system"))
            })
            .map(str::to_string)
            .collect();
        let ra = self.root_at(&ab, at_)?;
        let rb = self.root_at(&bb, bt)?;
        let keys = self.tree_diff(&ra, &rb)?;
        if keys.iter().all(|k| k.starts_with("o/")) {
            let mut ids: BTreeSet<String> = keys.iter().map(|k| k[2..].to_string()).collect();
            for class in &system {
                let (ca, cb) = (self.ordinal(&ab, at_)?, self.ordinal(&bb, bt)?);
                ids.extend(self.objects_in(&ab, ca, Some(class))?.into_keys());
                ids.extend(self.objects_in(&bb, cb, Some(class))?.into_keys());
            }
            let mut out = Vec::new();
            for oid in ids {
                let x = self.object_at(&ab, at_, &oid)?;
                let y = self.object_at(&bb, bt, &oid)?;
                out.extend(self.diff_object(&oid, x.as_ref(), y.as_ref(), None)?);
            }
            return Ok(Value::Array(out));
        }
        let sa = self.state_at(&ab, at_)?;
        let sb = self.state_at(&bb, bt)?;
        Ok(Value::Array(self.diff_states(&sa, &sb)?))
    }
}
