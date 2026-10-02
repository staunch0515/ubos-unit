//! References, relationships and class extensions at commit time (STD-CLS-030, -040,
//! -041; DEC-050).
//!
//! The checks run on the full branch state loaded into the transaction: reference
//! integrity of written versions and appended entries (targets resolve, fit the target
//! class and are valid for the period or instant), link objects of relationships with an
//! attributes class, the `on_end` rules and compositions when objects end, cardinality, and
//! every extension written in the commit applying to its target.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_model::{Class, Registry};
use ubos_types::BpaError;
use ubos_version::{Repo, Segment, State, Tx, at, lt};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid_ref(m: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.REFERENCE_INVALID", m)
}

/// Canonical URI of an object (STD-ADDR-001).
pub fn uri_of(authority: &str, class: &str, slug: &str) -> String {
    format!("ubos://{authority}/{class}/{slug}")
}

/// Object identifier a canonical URI names on a state, if any.
pub fn resolve(st: &State, uri: &str, authority: &str) -> Option<String> {
    let rest = uri.strip_prefix("ubos://")?;
    let mut parts = rest.split('/');
    let (auth, class, slug) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || auth != authority {
        return None;
    }
    st.objects
        .iter()
        .find(|(_, o)| o.class == class && o.slug == slug)
        .map(|(id, _)| id.clone())
}

/// True when the segments of a timeline cover `[f, t)` without a gap.
fn covered(tl: &[Segment], f: &str, t: Option<&str>) -> bool {
    let mut segs: Vec<&Segment> = tl.iter().collect();
    segs.sort_by(|a, b| a.from.cmp(&b.from));
    let mut at_: Option<&str> = Some(f);
    for seg in segs {
        if lt(at_, Some(&seg.from)) {
            break;
        }
        if lt(at_, seg.to.as_deref()) || seg.to.is_none() {
            at_ = seg.to.as_deref();
            if at_.is_none() || (t.is_some() && !lt(at_, t)) {
                return true;
            }
        }
    }
    at_.is_none() || (t.is_some() && !lt(at_, t))
}

/// The commit-time checks over a transaction.
pub struct Checks<'a, 'r> {
    /// The repository view (store and registry with the state's extensions).
    pub repo: &'a mut Repo<'r>,
    /// The transaction with the full branch state.
    pub tx: &'a mut Tx,
    /// Default valid-time start per class.
    pub default_valid: &'a dyn Fn(&str) -> String,
}

impl Checks<'_, '_> {
    fn version(&mut self, head: &str) -> Result<Value, BpaError> {
        self.repo.version_in(self.tx, head)
    }

    fn live(&mut self, oid: &str) -> Result<bool, BpaError> {
        let Some(o) = self.tx.state.objects.get(oid) else {
            return Ok(false);
        };
        let head = o.head.clone();
        Ok(self.version(&head)?["tombstone"] != json!(true))
    }

    fn class(&self, name: &str) -> Result<Class, BpaError> {
        self.repo.reg.class(name).cloned()
    }

    fn target_ok(
        &mut self,
        uri: &str,
        field: &str,
        eff: &Class,
        period: Option<(&str, Option<&str>)>,
        instant: Option<&str>,
    ) -> Result<String, BpaError> {
        let oid = resolve(&self.tx.state, uri, &self.repo.store.authority())
            .ok_or_else(|| invalid_ref(format!("reference {uri} does not resolve")))?;
        let o = self.tx.state.objects[&oid].clone();
        let f = &eff.fields[field];
        let c = f.get("constraints");
        let tc = c.and_then(|c| s(c, "target_class")).or_else(|| {
            c.and_then(|c| c.get("item_type"))
                .and_then(|i| i.get("constraints"))
                .and_then(|c| s(c, "target_class"))
        });
        if let Some(tc) = tc
            && !self.class(&o.class)?.linearization.iter().any(|l| l == tc)
        {
            return Err(invalid_ref(format!("target is not a {tc}")));
        }
        if let Some((f, t)) = period
            && !covered(&o.timeline, f, t)
        {
            return Err(invalid_ref("target not valid for the whole period"));
        }
        if let Some(i) = instant
            && at(&o.timeline, i).is_none()
        {
            return Err(invalid_ref("target not valid at the entry time"));
        }
        Ok(oid)
    }

    /// Link objects of relationships with an attributes class (STD-CLS-040).
    fn links(&mut self, oid: &str, eff: &Class, body: &Value) -> Result<(), BpaError> {
        let o = &self.tx.state.objects[oid];
        let me = uri_of(&self.repo.store.authority(), &o.class, &o.slug);
        for (name, f) in &eff.fields {
            let Some(ac) = f
                .get("relationship")
                .and_then(|r| s(r, "attributes_class"))
                .map(str::to_string)
            else {
                continue;
            };
            let Some(v) = body.get(name) else { continue };
            if v.is_null() {
                continue;
            }
            let uris: Vec<String> = match v {
                Value::Array(a) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Value::String(u) => vec![u.clone()],
                _ => vec![],
            };
            for u in uris {
                let link = resolve(&self.tx.state, &u, &self.repo.store.authority())
                    .map(|id| self.tx.state.objects[&id].clone());
                let Some(link) = link else {
                    return Err(invalid_ref(format!("link is not a {ac}")));
                };
                if !self.class(&link.class)?.linearization.contains(&ac) {
                    return Err(invalid_ref(format!("link is not a {ac}")));
                }
                let lb = self.version(&link.head)?;
                if s(&lb["body"], "source") != Some(me.as_str()) {
                    return Err(invalid_ref("link does not reference its source"));
                }
            }
        }
        Ok(())
    }

    /// `(source, field, relationship, target)` over live sources.
    fn relations(&mut self) -> Result<Vec<(String, String, Value, String)>, BpaError> {
        let mut out = Vec::new();
        let ids: Vec<String> = self.tx.state.objects.keys().cloned().collect();
        for oid in ids {
            if !self.live(&oid)? {
                continue;
            }
            let o = self.tx.state.objects[&oid].clone();
            let eff = self.class(&o.class)?;
            let body = self.version(&o.head)?["body"].clone();
            for (field, uri) in eff.refs(&body) {
                if let (Some(rel), Some(tid)) = (
                    eff.relationship(&field),
                    resolve(&self.tx.state, &uri, &self.repo.store.authority()),
                ) {
                    out.push((oid.clone(), field, rel.clone(), tid));
                }
            }
        }
        Ok(out)
    }

    /// Reference integrity of the versions written and the entries appended in this commit
    /// (STD-CLS-041).
    pub fn integrity(&mut self, written: &[(String, String)]) -> Result<(), BpaError> {
        for (oid, vh) in written {
            let v = self.version(vh)?;
            if v["tombstone"] == json!(true) || !self.live(oid)? {
                continue;
            }
            let eff = self.class(s(&v, "class").unwrap_or_default())?;
            let from = s(&v["valid"], "from").unwrap_or_default().to_string();
            let to = s(&v["valid"], "to").map(str::to_string);
            for (field, uri) in eff.refs(&v["body"]) {
                self.target_ok(&uri, &field, &eff, Some((&from, to.as_deref())), None)?;
            }
            self.links(oid, &eff, &v["body"])?;
        }
        let appended = self.tx.appended.clone();
        for e in &appended {
            let eff = self.class(&e.class)?;
            for (field, uri) in eff.refs(&e.body) {
                self.target_ok(&uri, &field, &eff, None, Some(&e.valid_at))?;
            }
        }
        Ok(())
    }

    fn end_object(
        &mut self,
        oid: &str,
        d: &str,
        result: &mut Value,
        ended: &mut Vec<(String, String)>,
    ) -> Result<(), BpaError> {
        let o = self.tx.state.objects[oid].clone();
        let base = self.version(&o.head)?;
        let body = base["body"].as_object().cloned().unwrap_or_default();
        let vh = self.repo.new_version(
            self.tx,
            oid,
            &o.class,
            &o.slug,
            &body,
            d,
            None,
            Some(&o.head),
            true,
            None,
        )?;
        let tl = ubos_time::apply(&o.timeline, "end", &vh, d, None)
            .map_err(|e| BpaError::new(e.0, "timeline operation rejected"))?;
        let obj = self.tx.state.objects.get_mut(oid).expect("object");
        obj.timeline = tl;
        obj.head = vh.clone();
        result["versions"][oid] = json!(vh);
        ended.push((oid.to_string(), d.to_string()));
        if d < (self.default_valid)(&o.class).as_str() {
            result["retroactive"]
                .as_array_mut()
                .expect("list")
                .push(json!({"object_id": oid, "from": d, "to": null}));
        }
        Ok(())
    }

    /// `on_end` rules, compositions and cardinality (STD-CLS-040).
    pub fn relationships(
        &mut self,
        ended: &mut Vec<(String, String)>,
        result: &mut Value,
    ) -> Result<(), BpaError> {
        let mut done: BTreeSet<String> = BTreeSet::new();
        loop {
            let todo: Vec<(String, String)> = ended
                .iter()
                .filter(|(x, _)| !done.contains(x))
                .cloned()
                .collect();
            if todo.is_empty() {
                break;
            }
            for (x, d) in todo {
                done.insert(x.clone());
                let o = self.tx.state.objects[&x].clone();
                let eff = self.class(&o.class)?;
                let body = self.version(&o.head)?["body"].clone();
                for (field, uri) in eff.refs(&body) {
                    let composition = eff
                        .relationship(&field)
                        .is_some_and(|r| s(r, "ownership") == Some("composition"));
                    if let Some(tid) = resolve(&self.tx.state, &uri, &self.repo.store.authority())
                        && composition
                        && self.live(&tid)?
                    {
                        self.end_object(&tid, &d, result, ended)?;
                    }
                }
                for (src, field, rel, tid) in self.relations()? {
                    if tid != x || src == x {
                        continue;
                    }
                    match s(&rel, "on_end").unwrap_or("restrict") {
                        "restrict" => {
                            return Err(BpaError::new(
                                "MODEL.REFERENCE_RESTRICTED",
                                format!("referenced by {src}"),
                            ));
                        }
                        "cascade" => {
                            if self.live(&src)? {
                                self.end_object(&src, &d, result, ended)?;
                            }
                        }
                        _ => self.detach(&src, &field, &x, &d, result)?,
                    }
                }
            }
        }
        self.cardinality()
    }

    fn detach(
        &mut self,
        src: &str,
        field: &str,
        target: &str,
        d: &str,
        result: &mut Value,
    ) -> Result<(), BpaError> {
        let o = self.tx.state.objects[src].clone();
        let mut body = self.version(&o.head)?["body"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let t = &self.tx.state.objects[target];
        let uri = uri_of(&self.repo.store.authority(), &t.class, &t.slug);
        match body.get(field).cloned() {
            Some(Value::Array(a)) => {
                body.insert(
                    field.to_string(),
                    Value::Array(a.into_iter().filter(|u| u.as_str() != Some(&uri)).collect()),
                );
            }
            _ => {
                body.remove(field);
            }
        }
        let body = self
            .repo
            .reg
            .check_body(&o.class, &Value::Object(body), false)?;
        let (from, to) = ubos_time::range(&o.timeline, "supersede_from", d, None);
        let vh = self.repo.new_version(
            self.tx,
            src,
            &o.class,
            &o.slug,
            &body,
            &from,
            to.as_deref(),
            Some(&o.head),
            false,
            None,
        )?;
        let tl = ubos_time::apply(&o.timeline, "supersede_from", &vh, d, None)
            .map_err(|e| BpaError::new(e.0, "timeline operation rejected"))?;
        let obj = self.tx.state.objects.get_mut(src).expect("object");
        obj.timeline = tl;
        obj.head = vh.clone();
        result["versions"][src] = json!(vh);
        if d < (self.default_valid)(&o.class).as_str() {
            result["retroactive"]
                .as_array_mut()
                .expect("list")
                .push(json!({"object_id": src, "from": from, "to": to}));
        }
        Ok(())
    }

    /// Cardinality of the relations of the state (STD-CLS-040).
    pub fn cardinality_check(&mut self) -> Result<(), BpaError> {
        self.cardinality()
    }

    /// End an object at `d` (reverts end the objects a reverted commit created).
    pub fn end_object_pub(
        &mut self,
        oid: &str,
        d: &str,
        result: &mut Value,
        ended: &mut Vec<(String, String)>,
    ) -> Result<(), BpaError> {
        self.end_object(oid, d, result, ended)
    }

    fn cardinality(&mut self) -> Result<(), BpaError> {
        let mut counts: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for (src, field, rel, tid) in self.relations()? {
            if matches!(s(&rel, "cardinality"), Some("one_to_one" | "one_to_many")) {
                counts.entry((field, tid)).or_default().insert(src);
            }
        }
        if counts.values().any(|v| v.len() > 1) {
            return Err(BpaError::new("MODEL.CARDINALITY_VIOLATED", ""));
        }
        Ok(())
    }

    /// Every extension written in this commit applies to its target (STD-CLS-030).
    pub fn extensions_apply(
        &mut self,
        base: &Registry,
        written: &[(String, String)],
    ) -> Result<(), BpaError> {
        let mut any = false;
        for (oid, vh) in written {
            if self.tx.state.objects.get(oid).map(|o| o.class.as_str()) != Some("ClassExtension") {
                continue;
            }
            any = true;
            let target = s(&self.version(vh)?["body"], "target")
                .unwrap_or_default()
                .to_string();
            base.decl(&target)
                .map_err(|_| invalid_ref(format!("unknown target class {target}")))?;
        }
        if any {
            let mut exts = Vec::new();
            let objs: Vec<_> = self
                .tx
                .state
                .objects
                .values()
                .filter(|o| o.class == "ClassExtension")
                .map(|o| o.head.clone())
                .collect();
            for head in objs {
                let v = self.version(&head)?;
                if v["tombstone"] != json!(true) {
                    exts.push(v["body"].clone());
                }
            }
            base.with_extensions(&exts)?;
        }
        Ok(())
    }

    /// `traverse` (STD-CLS-040): the targets of a relationship of an object, or the
    /// sources of its inverse.
    pub fn traverse(&mut self, object_id: &str, relation: &str) -> Result<Value, BpaError> {
        let Some(o) = self.tx.state.objects.get(object_id).cloned() else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        };
        let eff = self.class(&o.class)?;
        let mut out = BTreeSet::new();
        if let Some(rel) = eff.relationship(relation).cloned() {
            let body = self.version(&o.head)?["body"].clone();
            for (fld, uri) in eff.refs(&body) {
                if fld != relation {
                    continue;
                }
                let mut tid = resolve(&self.tx.state, &uri, &self.repo.store.authority());
                if let Some(t) = &tid
                    && rel.get("attributes_class").is_some()
                {
                    let link = self.tx.state.objects[t].clone();
                    let lb = self.version(&link.head)?;
                    tid = resolve(
                        &self.tx.state,
                        s(&lb["body"], "target").unwrap_or_default(),
                        &self.repo.store.authority(),
                    );
                }
                if let Some(t) = tid
                    && self.live(&t)?
                {
                    out.insert(t);
                }
            }
            return Ok(json!({"objects": out}));
        }
        let mut known = false;
        for (src, _, rel, tid) in self.relations()? {
            if s(&rel, "inverse") != Some(relation) {
                continue;
            }
            known = true;
            if rel.get("attributes_class").is_some() {
                let link = self.tx.state.objects[&tid].clone();
                let lb = self.version(&link.head)?;
                if resolve(
                    &self.tx.state,
                    s(&lb["body"], "target").unwrap_or_default(),
                    &self.repo.store.authority(),
                )
                .as_deref()
                    == Some(object_id)
                {
                    out.insert(src);
                }
            } else if tid == object_id {
                out.insert(src);
            }
        }
        let declared = self.repo.reg.classes().any(|c| {
            c.fields
                .values()
                .any(|f| f.get("relationship").and_then(|r| s(r, "inverse")) == Some(relation))
        });
        if !known && !declared {
            return Err(invalid_ref(format!("unknown relation {relation}")));
        }
        Ok(json!({"objects": out}))
    }
}

/// Bodies of the live `ClassExtension` objects of a state (STD-CLS-030).
pub fn extensions_of(repo: &mut Repo<'_>, st: &State) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for o in st.objects.values() {
        if o.class != "ClassExtension" {
            continue;
        }
        let v = repo.version(&o.head)?;
        if v["tombstone"] != json!(true) {
            out.push(v["body"].clone());
        }
    }
    Ok(out)
}

/// True when the registry declares references or relationships, so that commits need
/// the full state for the checks above.
pub fn needs_full_state(reg: &Registry) -> bool {
    reg.classes().any(|c| {
        c.fields.values().any(|f| {
            s(f, "type") == Some("reference")
                || f.get("constraints")
                    .and_then(|c| c.get("item_type"))
                    .and_then(|i| s(i, "type"))
                    == Some("reference")
        })
    })
}
