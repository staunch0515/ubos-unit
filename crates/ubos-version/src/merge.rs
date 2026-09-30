//! Merge (STD-VER-040…056, DEC-032) and diff (STD-VER-030).

use crate::state::{Entry, ObjectState, Repo, Segment, State, Tx, at};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use ubos_model::types::BpaError;

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Absent,
    Version(String),
    Merged(Map<String, Value>),
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

    fn ancestors_of(&mut self, ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>, BpaError> {
        let mut out = BTreeSet::new();
        for id in ids {
            out.extend(self.ancestors(*id)?);
        }
        Ok(out)
    }

    /// Merge base of two sets of commits; a virtual base for several best ancestors (STD-VER-040).
    fn base_state(
        &mut self,
        xs: &BTreeSet<u64>,
        ys: &BTreeSet<u64>,
        tx: &mut Tx,
    ) -> Result<State, BpaError> {
        let common: BTreeSet<u64> = self
            .ancestors_of(xs)?
            .intersection(&self.ancestors_of(ys)?)
            .copied()
            .collect();
        let mut best = Vec::new();
        for c in &common {
            let mut dominated = false;
            for d in &common {
                if c != d && self.ancestors(*d)?.contains(c) {
                    dominated = true;
                    break;
                }
            }
            if !dominated {
                best.push(*c);
            }
        }
        let mut acc = self.commit_state(best[0])?;
        let mut tips: BTreeSet<u64> = [best[0]].into();
        for c in best.iter().skip(1) {
            let b = self.base_state(&tips, &[*c].into(), tx)?;
            let other = self.commit_state(*c)?;
            acc = self.merge_states(&b, &other, &acc, true, None, None, tx)?.0;
            tips.insert(*c);
        }
        Ok(acc)
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_states(
        &mut self,
        b: &State,
        s: &State,
        t: &State,
        virtual_: bool,
        cseq: Option<u64>,
        branch: Option<&str>,
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
                tx,
            )?;
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

    fn merge_object(
        &mut self,
        oid: &str,
        bo: Option<&ObjectState>,
        so: Option<&ObjectState>,
        to: Option<&ObjectState>,
        virtual_: bool,
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
            } else if vs.is_none() || vt.is_none() {
                conflicts.push(json!({"object_id": oid, "kind": "ended_vs_changed"}));
                piece(vb)
            } else if meta.kind == "raw_document" {
                conflicts.push(json!({"object_id": oid, "kind": "raw_document"}));
                piece(vb)
            } else {
                let (r, c) = self.merge_body(
                    oid,
                    vb.as_deref(),
                    vs.as_deref().unwrap_or_default(),
                    vt.as_deref().unwrap_or_default(),
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
                    let slug = match &pred {
                        Some(p) => self.version_in(tx, p)?["slug"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        None => meta.slug.clone(),
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

    fn merge_body(
        &mut self,
        oid: &str,
        vb: Option<&str>,
        vs: &str,
        vt: &str,
        tx: &mut Tx,
    ) -> Result<(Piece, Vec<Value>), BpaError> {
        let body = |v: Value| v["body"].as_object().cloned().unwrap_or_default();
        let bb = match vb {
            Some(h) => body(self.version_in(tx, h)?),
            None => Map::new(),
        };
        let sb = body(self.version_in(tx, vs)?);
        let tb = body(self.version_in(tx, vt)?);
        let keys: BTreeSet<&String> = bb.keys().chain(sb.keys()).chain(tb.keys()).collect();
        let mut out = Map::new();
        let mut conflicts = Vec::new();
        for k in keys {
            let (b, s, t) = (bb.get(k), sb.get(k), tb.get(k));
            let r = if s == t || s == b {
                t
            } else if t == b {
                s
            } else {
                conflicts.push(json!({"object_id": oid, "kind": "field", "path": format!("/{k}")}));
                b
            };
            if let Some(v) = r {
                out.insert(k.clone(), v.clone());
            }
        }
        Ok((Piece::Merged(out), conflicts))
    }

    /// Merge the head of `source` into `target` (STD-VER-050…056).
    pub fn merge(&mut self, source: &str, target: &str) -> Result<Value, BpaError> {
        let (s, t) = (self.head(source)?, self.head(target)?);
        if self.ancestors(t)?.contains(&s) {
            return Ok(json!({"cseq": self.commit(t)?.cseq, "merged": false}));
        }
        let mut tx = self.begin(target)?;
        self.load_all(&mut tx)?;
        let base = self.base_state(&[t].into(), &[s].into(), &mut tx)?;
        let sstate = self.commit_state(s)?;
        let tstate = tx.state.clone();
        let cseq = tx.cseq;
        let (st, conflicts) = self.merge_states(
            &base,
            &sstate,
            &tstate,
            false,
            Some(cseq),
            Some(source),
            &mut tx,
        )?;
        if !conflicts.is_empty() {
            let mut uniq: Vec<Value> = Vec::new();
            for c in conflicts {
                if !uniq.contains(&c) {
                    uniq.push(c);
                }
            }
            return Err(BpaError::with_details(
                "VER.MERGE_CONFLICT",
                json!({"conflicts": uniq}),
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
        self.write(tx, vec![t, s])?;
        Ok(json!({"cseq": cseq, "merged": true, "appended": appended}))
    }

    /// Diff between two coordinates `{branch, time}` (STD-VER-030).
    pub fn diff(&mut self, a: &Value, b: &Value) -> Result<Value, BpaError> {
        let coord = |v: &Value| {
            (
                v["branch"].as_str().unwrap_or("main").to_string(),
                v.get("time").and_then(Value::as_u64),
            )
        };
        let (ab, at_) = coord(a);
        let (bb, bt) = coord(b);
        let sa = self.state_at(&ab, at_)?;
        let sb = self.state_at(&bb, bt)?;
        let mut out = Vec::new();
        let ids: BTreeSet<&String> = sa.objects.keys().chain(sb.objects.keys()).collect();
        for oid in ids {
            let (x, y) = (sa.objects.get(oid), sb.objects.get(oid));
            if let (Some(x), Some(y)) = (x, y)
                && x.head == y.head
                && x.timeline == y.timeline
            {
                continue;
            }
            let class = y.or(x).map(|o| o.class.clone()).unwrap_or_default();
            let mut ent = json!({"object_id": oid, "class": class,
                "versions": {"a": x.map(|o| o.head.clone()), "b": y.map(|o| o.head.clone())}});
            match (x, y) {
                (None, _) => ent["op"] = json!("added"),
                (Some(x), Some(y)) => {
                    let vx = self.version(&x.head)?;
                    let vy = self.version(&y.head)?;
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
            out.push(ent);
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
        Ok(Value::Array(out))
    }
}
