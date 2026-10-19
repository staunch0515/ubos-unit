//! Unique keys at commit time (STD-CLS-001 `keys`, STD-ISA-010, DSN-DVM-111; PH-2 step
//! 0b-3).
//!
//! A key declared on a class is unique among the live objects of that class and its
//! subclasses on the branch: no two of them carry the same values in all the key's fields.
//! A key with an absent or null field constrains nothing. The objects a commit writes are
//! checked against each other and against the branch at the base: through the field index
//! (the key's fields are indexed without a declaration, DSN-DVM-902), or, on a branch with
//! a parent line or before the index serves the branch, over the objects of the classes.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_model::Registry;
use ubos_types::BpaError;
use ubos_version::{Repo, Tx};

fn conflict(class: &str, fields: &[String], a: &str, b: &str) -> BpaError {
    BpaError::with_details(
        "MODEL.KEY_CONFLICT",
        json!({"class": class, "fields": fields, "objects": [a, b]}),
    )
}

/// The values of a key in a body, or `None` when a field is absent or null.
fn values(body: &Value, fields: &[String]) -> Option<Vec<Value>> {
    fields
        .iter()
        .map(|f| body.get(f).filter(|v| !v.is_null()).cloned())
        .collect()
}

/// Check the unique keys of the versions written by a commit (`(object, version)`):
/// `MODEL.KEY_CONFLICT` naming the class that declares the key and the two objects.
/// `indexed`: the field index serves the branch at the base.
pub fn check(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    reg: &Registry,
    written: &[(String, String)],
    indexed: bool,
) -> Result<(), BpaError> {
    // the live keys the commit writes: (declaring class, fields, values) → object
    let mut mine: BTreeMap<(String, Vec<String>, String), String> = BTreeMap::new();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for (oid, vh) in written {
        ids.insert(oid.clone());
        let Some(class) = tx.state.objects.get(oid).map(|o| o.class.clone()) else {
            continue;
        };
        let keys = reg.unique_keys(&class);
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
                return Err(conflict(&decl, &fields, other, oid));
            }
            mine.insert(k, oid.clone());
        }
    }
    if mine.is_empty() {
        return Ok(());
    }
    // the other objects of the branch that may carry one of these keys
    let by_key: BTreeMap<(String, Vec<String>), Vec<String>> =
        mine.keys().fold(BTreeMap::new(), |mut m, (d, f, v)| {
            m.entry((d.clone(), f.clone())).or_default().push(v.clone());
            m
        });
    let full = !tx.inherited.is_empty() || !tx.hidden.is_empty();
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
        let via_index = indexed
            && !full
            && tx.inherited.is_empty()
            && ubos_types::values::indexable(&first_type);
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
                return Err(conflict(&decl, &fields, &id, mine_id));
            }
        }
    }
    Ok(())
}
