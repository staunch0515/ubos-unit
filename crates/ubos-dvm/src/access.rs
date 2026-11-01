//! Authorization relations and relation-based access rules (BPA 1.0-rc8; PH-2 step 8a,
//! DEC-136; STD-LGS-050, -051; DSN-DVM-810…812).
//!
//! * **Relations** (STD-LGS-050). A reference field, or a list of references, may declare
//!   `authz_relation: <name>`. The relations of an object are derived from its version at
//!   the snapshot of the request, one tuple per referenced object, and are not kept in a
//!   store of their own. A class may declare `principal_field`, the text field holding the
//!   DID of the principal its object stands for.
//! * **Rules** (STD-LGS-051). The kernel property `access` holds `{action, allow_if}` rules,
//!   where `allow_if` is a path `principal in this(.relation)*` of at most four relations.
//!   When rules for an action (or `*`) apply to an object, the action is allowed only if a
//!   rule's path, followed from the object at the snapshot, reaches an object whose
//!   principal field holds the principal. A refused read is `IAM.NOT_FOUND`, and a refused
//!   port call is `IAM.DENIED`. A query made as a viewer leaves out, inside its execution,
//!   the objects the viewer may not read.
//! * **Cost.** Without a live sheet that declares `access`, nothing is evaluated. A query
//!   made as a viewer with rules present evaluates every candidate object of its class
//!   once (the benchmarks of step 18 decide whether that needs an index).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use ubos_types::BpaError;
use ubos_version::state::Repo;

/// An object's class and body at a snapshot.
type ClassedBody = (String, Map<String, Value>);

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The snapshot selectors of a request.
pub(crate) fn selectors(i: &Value) -> Value {
    let mut out = json!({});
    for k in ["commit", "tag", "tx_time", "time", "evaluated_at", "asof"] {
        if let Some(v) = i.get(k).filter(|v| !v.is_null()) {
            out[k] = v.clone();
        }
    }
    out
}

/// The viewer of a read or query made `as: {principal, clearances?, ctx?}`.
pub(crate) fn viewer_of(i: &Value) -> Result<Option<Value>, BpaError> {
    let Some(v) = i.get("as").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    if !v.is_object() || s(v, "principal").is_none() {
        return Err(BpaError::new(
            "MODEL.DECLARATION_INCOMPLETE",
            "as is {principal, clearances, ctx}",
        ));
    }
    Ok(Some(v.clone()))
}

impl Engine {
    /// The relation fields of a class: field → relation.
    fn relation_fields(&self, class: &str) -> Vec<(String, String)> {
        self.registry
            .class(class)
            .map(|c| {
                c.fields
                    .iter()
                    .filter_map(|(n, f)| s(f, "authz_relation").map(|r| (n.clone(), r.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The principal field of a class, the nearest one its linearization declares.
    fn principal_field(&self, class: &str) -> Option<String> {
        let lin = self.registry.class(class).ok()?.linearization.clone();
        lin.iter().find_map(|c| {
            self.registry
                .decl(c)
                .ok()
                .and_then(|d| s(d, "principal_field"))
                .map(str::to_string)
        })
    }

    /// The class and body of an object at the snapshot, `None` when it is not there.
    fn body_at(
        &mut self,
        branch: &str,
        sel: &Value,
        oid: &str,
    ) -> Result<Option<ClassedBody>, BpaError> {
        let mut i = sel.clone();
        i["branch"] = json!(branch);
        i["object_id"] = json!(oid);
        match self.read_classed(&i) {
            Ok((r, class)) => Ok(Some((
                class,
                r["body"].as_object().cloned().unwrap_or_default(),
            ))),
            Err(e) if matches!(e.code, "IAM.NOT_FOUND" | "QRY.NOT_FOUND_AT_TIME") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The object a reference value names at the snapshot.
    fn target_of(
        &mut self,
        branch: &str,
        sel: &Value,
        v: &str,
    ) -> Result<Option<String>, BpaError> {
        if !v.starts_with("ubos://") {
            return Ok(self.body_at(branch, sel, v)?.map(|_| v.to_string()));
        }
        let Ok(p) = ubos_types::addr::parse_uri(&json!(v)) else {
            return Ok(None);
        };
        let (Some(class), Some(slug)) = (s(&p, "class"), s(&p, "slug")) else {
            return Ok(None);
        };
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, sel)?;
        let cseq = repo.commit(snap.commit)?.cseq;
        Ok(repo
            .object_by_slug(branch, cseq, class, slug)?
            .map(|(id, _)| id))
    }

    /// The targets of one relation of an object at the snapshot, in field and list order.
    fn targets(
        &mut self,
        branch: &str,
        sel: &Value,
        oid: &str,
        relation: &str,
    ) -> Result<Vec<String>, BpaError> {
        let Some((class, body)) = self.body_at(branch, sel, oid)? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (f, rel) in self.relation_fields(&class) {
            if rel != relation {
                continue;
            }
            let vals: Vec<Value> = match body.get(&f) {
                Some(Value::Array(a)) => a.clone(),
                Some(v) => vec![v.clone()],
                None => Vec::new(),
            };
            for v in vals {
                if let Some(t) = v.as_str()
                    && let Some(id) = self.target_of(branch, sel, t)?
                    && !out.contains(&id)
                {
                    out.push(id);
                }
            }
        }
        Ok(out)
    }

    /// Adapter operation `relations {branch, object_id}` with snapshot selectors
    /// (STD-LGS-050): `{object_id, relations, principal?}`.
    pub fn relations(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let oid = s(i, "object_id").unwrap_or_default().to_string();
        let sel = selectors(i);
        let (class, body) = self
            .body_at(&branch, &sel, &oid)?
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown object"))?;
        let names: BTreeSet<String> = self
            .relation_fields(&class)
            .into_iter()
            .map(|(_, r)| r)
            .collect();
        let mut rels = Map::new();
        for r in names {
            let t = self.targets(&branch, &sel, &oid, &r)?;
            rels.insert(r, json!(t));
        }
        let mut out = json!({"object_id": oid, "relations": rels});
        if let Some(pf) = self.principal_field(&class)
            && let Some(p) = body.get(&pf).filter(|p| !p.is_null())
        {
            out["principal"] = p.clone();
        }
        Ok(out)
    }

    /// Whether a path followed from an object reaches one whose principal field holds the
    /// principal.
    fn holds(
        &mut self,
        branch: &str,
        sel: &Value,
        oid: &str,
        principal: &str,
        path: &[String],
    ) -> Result<bool, BpaError> {
        let mut objs = vec![oid.to_string()];
        for rel in path {
            let mut next: Vec<String> = Vec::new();
            for x in &objs {
                for t in self.targets(branch, sel, x, rel)? {
                    if !next.contains(&t) {
                        next.push(t);
                    }
                }
            }
            objs = next;
        }
        for x in objs {
            if let Some((class, body)) = self.body_at(branch, sel, &x)?
                && let Some(pf) = self.principal_field(&class)
                && body.get(&pf) == Some(&json!(principal))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether a live sheet of the branch declares `access` at the snapshot.
    pub(crate) fn has_access_rules(&mut self, branch: &str, sel: &Value) -> Result<bool, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, sel)?;
        let cseq = repo.commit(snap.commit)?.cseq;
        for (_, o) in repo.objects_in(branch, cseq, Some("GovernanceSheet"))? {
            let v = repo.version(&o.head)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            let declares = v["body"]["ast"]["blocks"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|b| b["declarations"].as_array().cloned().unwrap_or_default())
                .any(|d| {
                    s(&d, "property").is_some_and(|p| p == "access" || p.starts_with("access."))
                });
            if declares {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `(allowed, rule)`: the access rules of an object for an action (STD-LGS-051);
    /// without rules for the action it is not restricted by relations.
    pub(crate) fn access_allows(
        &mut self,
        branch: &str,
        sel: &Value,
        oid: &str,
        principal: &str,
        action: &str,
        ctx: &Value,
    ) -> Result<(bool, Option<String>), BpaError> {
        if !self.has_access_rules(branch, sel)? {
            return Ok((true, None));
        }
        let mut i = sel.clone();
        i["branch"] = json!(branch);
        i["object_id"] = json!(oid);
        let mut c = ctx.as_object().cloned().unwrap_or_default();
        c.insert("principal".into(), json!(principal));
        i["ctx"] = Value::Object(c);
        let props = self.cascade(&i)?;
        let rules: Vec<Value> = props["properties"]["access"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| matches!(s(r, "action"), Some(a) if a == action || a == "*"))
            .cloned()
            .collect();
        if rules.is_empty() {
            return Ok((true, None));
        }
        for r in rules {
            let text = s(&r, "allow_if").unwrap_or_default().to_string();
            let path = ubos_rules::registry::parse_access(&text)?;
            if self.holds(branch, sel, oid, principal, &path)? {
                return Ok((true, Some(text)));
            }
        }
        Ok((false, None))
    }

    /// Adapter operation `check_access {branch, object_id, principal, action, ctx}` with
    /// snapshot selectors (STD-LGS-051): `{allowed, rule}`.
    pub fn check_access(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let oid = s(i, "object_id").unwrap_or_default().to_string();
        let sel = selectors(i);
        if self.body_at(&branch, &sel, &oid)?.is_none() {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        }
        let (Some(principal), Some(action)) = (s(i, "principal"), s(i, "action")) else {
            return Err(BpaError::new(
                "MODEL.DECLARATION_INCOMPLETE",
                "principal and action",
            ));
        };
        let (allowed, rule) = self.access_allows(
            &branch,
            &sel,
            &oid,
            principal,
            action,
            i.get("ctx").unwrap_or(&Value::Null),
        )?;
        Ok(json!({ "allowed": allowed, "rule": rule }))
    }

    /// The objects of a class (and its subclasses unless `only`) at the snapshot that a
    /// viewer may not read: a query made as the viewer leaves them out (FR-IAM-062). A query
    /// that filters or sorts on `names` fails with `QRY.INVALID_QUERY` when one of them is
    /// masked for the viewer on an object it may read (STD-LGS-052).
    pub(crate) fn unreadable(
        &mut self,
        branch: &str,
        sel: &Value,
        class: &str,
        only: bool,
        viewer: &Value,
        names: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, BpaError> {
        let mut out = BTreeSet::new();
        let rules = self.has_access_rules(branch, sel)?;
        if !rules && names.is_empty() {
            return Ok(out);
        }
        let classes: Vec<String> = self
            .registry
            .classes()
            .filter(|c| {
                if only {
                    c.name == class
                } else {
                    c.linearization.iter().any(|x| x == class)
                }
            })
            .map(|c| c.name.clone())
            .collect();
        let oids: Vec<String> = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            if repo.branch_meta(branch)?.parent_line.is_some() {
                let (v, _) = repo.view(branch, sel)?;
                v.state
                    .objects
                    .iter()
                    .filter(|(_, o)| classes.contains(&o.class))
                    .map(|(id, _)| id.clone())
                    .collect()
            } else {
                let snap = repo.select(branch, sel)?;
                let cseq = repo.commit(snap.commit)?.cseq;
                let mut v = Vec::new();
                for c in &classes {
                    v.extend(
                        repo.objects_in(branch, cseq, Some(c))?
                            .into_iter()
                            .filter(|(_, o)| &o.class == c)
                            .map(|(id, _)| id),
                    );
                }
                v
            }
        };
        let principal = s(viewer, "principal").unwrap_or_default().to_string();
        let ctx = viewer.get("ctx").cloned().unwrap_or(Value::Null);
        for oid in oids {
            let Some((oclass, _)) = self.body_at(branch, sel, &oid)? else {
                continue;
            };
            if rules {
                let (ok, _) = self.access_allows(branch, sel, &oid, &principal, "read", &ctx)?;
                if !ok {
                    out.insert(oid);
                    continue;
                }
            }
            if !names.is_empty() {
                let modes = self.mask_modes(branch, sel, &oid, &oclass, viewer)?;
                let hidden: Vec<&String> =
                    names.iter().filter(|n| modes.contains_key(*n)).collect();
                if !hidden.is_empty() {
                    return Err(BpaError::with_details(
                        "QRY.INVALID_QUERY",
                        json!({ "masked": hidden }),
                    ));
                }
            }
        }
        Ok(out)
    }
}
