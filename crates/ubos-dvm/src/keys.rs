//! Unique keys and slugs at commit and at merge (STD-CLS-001 `keys`, STD-ISA-010,
//! STD-VER-050, DSN-DVM-111, FR-MODEL-013; PH-2 steps 0b-3 and 0b-4).
//!
//! A key declared on a class is unique among the live objects of that class and its
//! subclasses on the branch: no two of them carry the same values in all the key's fields.
//! A key with an absent or null field constrains nothing; an ended object releases its key.
//! A slug is unique among the objects of a class on the branch, ended ones included: an
//! ended object stays readable by its URI (STD-OBJ-004, -005). A slug is checked when an
//! object takes it (create, a slug change, or a merge bringing the object).
//!
//! The objects a commit writes are checked against each other and against the branch at
//! the base: keys through the field index (the key's fields are indexed without a
//! declaration, DSN-DVM-902), or, on a branch with a parent line or before the index serves
//! the branch, over the objects of the classes; slugs through the slug index. A merge runs
//! the same check over the objects it brings into the target, and reports what it finds as
//! key conflicts (STD-VER-050).

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_model::Registry;
use ubos_types::BpaError;
use ubos_version::{Repo, Tx};

/// A conflict: the class, the key's fields or the slug, and the two objects (the object
/// already holding the key first, the written object second).
fn key_conflict(class: &str, fields: &[String], a: &str, b: &str) -> Value {
    json!({"class": class, "fields": fields, "objects": [a, b]})
}

fn slug_conflict(class: &str, slug: &str, a: &str, b: &str) -> Value {
    json!({"class": class, "slug": slug, "objects": [a, b]})
}

/// The values of a key in a body, or `None` when a field is absent or null.
fn values(body: &Value, fields: &[String]) -> Option<Vec<Value>> {
    fields
        .iter()
        .map(|f| body.get(f).filter(|v| !v.is_null()).cloned())
        .collect()
}

/// Check the keys and slugs of the versions written by a commit (`(object, version)`):
/// `MODEL.KEY_CONFLICT` naming the class and the key's fields (or the slug) and the two
/// objects. `indexed`: the field index serves the branch at the base.
pub fn check(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    reg: &Registry,
    written: &[(String, String)],
    indexed: bool,
) -> Result<(), BpaError> {
    match conflicts(repo, tx, reg, written, indexed, true)?
        .into_iter()
        .next()
    {
        Some(c) => Err(BpaError::with_details("MODEL.KEY_CONFLICT", c)),
        None => Ok(()),
    }
}

/// The key conflicts of a merge (STD-VER-050): the objects the merge writes into the
/// target (`(object, version)`) checked as a commit would be, as merge conflicts of kind
/// `key` on the object brought by the merge.
pub fn merge_conflicts(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    reg: &Registry,
    written: &[(String, String)],
    indexed: bool,
) -> Result<Vec<Value>, BpaError> {
    Ok(conflicts(repo, tx, reg, written, indexed, false)?
        .into_iter()
        .map(|c| {
            let mut out = json!({"object_id": c["objects"][1], "kind": "key", "class": c["class"],
                                 "with": c["objects"][0]});
            for k in ["fields", "slug"] {
                if let Some(v) = c.get(k) {
                    out[k] = v.clone();
                }
            }
            out
        })
        .collect())
}

fn conflicts(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    reg: &Registry,
    written: &[(String, String)],
    indexed: bool,
    first: bool,
) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    // the live keys the commit writes: (declaring class, fields, values) → object
    let mut mine: BTreeMap<(String, Vec<String>, String), String> = BTreeMap::new();
    // the slugs objects take: (class, slug) → object
    let mut slugs: BTreeMap<(String, String), String> = BTreeMap::new();
    let ids: BTreeSet<String> = written.iter().map(|(o, _)| o.clone()).collect();
    for (oid, vh) in written {
        let Some(o) = tx.state.objects.get(oid).cloned() else {
            continue;
        };
        let taken = tx
            .loaded
            .get(oid)
            .and_then(Option::as_ref)
            .is_none_or(|b| b.slug != o.slug || b.class != o.class);
        if taken {
            let k = (o.class.clone(), o.slug.clone());
            if let Some(other) = slugs.get(&k) {
                out.push(slug_conflict(&o.class, &o.slug, other, oid));
                if first {
                    return Ok(out);
                }
            }
            slugs.insert(k, oid.clone());
        }
        let keys = reg.unique_keys(&o.class);
        if keys.is_empty() {
            continue;
        }
        let v = repo.version_in(tx, vh)?;
        if v["tombstone"] == json!(true) {
            continue;
        }
        for (decl, fields) in keys {
            let Some(vals) = values(&v["body"], &fields) else {
                continue;
            };
            let k = (decl.clone(), fields.clone(), Value::Array(vals).to_string());
            if let Some(other) = mine.get(&k)
                && other != oid
            {
                out.push(key_conflict(&decl, &fields, other, oid));
                if first {
                    return Ok(out);
                }
            }
            mine.insert(k, oid.clone());
        }
    }
    let full = !tx.inherited.is_empty() || !tx.hidden.is_empty();
    // slugs: the objects of the branch with the slug, through the slug index
    if !slugs.is_empty() {
        let mut found: BTreeSet<String> = BTreeSet::new();
        if full {
            found.extend(
                tx.state
                    .objects
                    .iter()
                    .filter(|(_, o)| slugs.contains_key(&(o.class.clone(), o.slug.clone())))
                    .map(|(id, _)| id.clone()),
            );
        } else {
            let pairs: BTreeSet<(String, String)> = slugs.keys().cloned().collect();
            repo.prefetch_slugs(tx, &pairs)?;
            for k in &pairs {
                found.extend(tx.slugs.get(k).into_iter().flat_map(|m| m.keys().cloned()));
            }
        }
        found.retain(|id| !ids.contains(id));
        repo.load_ids(tx, &found)?;
        for id in found {
            let Some(o) = tx.state.objects.get(&id) else {
                continue;
            };
            if let Some(mine_id) = slugs.get(&(o.class.clone(), o.slug.clone())) {
                out.push(slug_conflict(&o.class, &o.slug, &id, mine_id));
                if first {
                    return Ok(out);
                }
            }
        }
    }
    if mine.is_empty() {
        return Ok(out);
    }
    // the other objects of the branch that may carry one of these keys
    let by_key: BTreeMap<(String, Vec<String>), Vec<String>> =
        mine.keys().fold(BTreeMap::new(), |mut m, (d, f, v)| {
            m.entry((d.clone(), f.clone())).or_default().push(v.clone());
            m
        });
    for ((decl, fields), vals) in by_key {
        let family = reg.subclasses(&decl);
        let mut cands: BTreeSet<String> = BTreeSet::new();
        let first_type = reg
            .class(&decl)
            .ok()
            .and_then(|c| {
                c.fields
                    .get(&fields[0])
                    .and_then(|f| f["type"].as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let via_index = indexed && !full && ubos_types::values::indexable(&first_type);
        let mut scanned = false;
        if via_index {
            let keys: Vec<Vec<u8>> = vals
                .iter()
                .filter_map(|v| serde_json::from_str::<Vec<Value>>(v).ok())
                .filter_map(|vs| ubos_types::values::index_key(&first_type, &vs[0]))
                .collect();
            if let Some(rows) = repo.field_keys(&tx.branch, tx.base, &family, &fields[0], &keys)? {
                cands.extend(rows.into_iter().map(|(id, _)| id));
                scanned = true;
            }
        }
        if !scanned {
            if !full {
                for c in &family {
                    repo.load_class(tx, c)?;
                }
            }
            cands.extend(
                tx.state
                    .objects
                    .iter()
                    .filter(|(_, o)| family.contains(&o.class))
                    .map(|(id, _)| id.clone()),
            );
        }
        cands.retain(|id| !ids.contains(id));
        repo.load_ids(tx, &cands)?;
        for id in cands {
            let Some(o) = tx.state.objects.get(&id).cloned() else {
                continue;
            };
            if !family.contains(&o.class) {
                continue;
            }
            let v = repo.version_in(tx, &o.head)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            let Some(theirs) = values(&v["body"], &fields) else {
                continue;
            };
            let k = (
                decl.clone(),
                fields.clone(),
                Value::Array(theirs).to_string(),
            );
            if let Some(mine_id) = mine.get(&k) {
                out.push(key_conflict(&decl, &fields, &id, mine_id));
                if first {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}
