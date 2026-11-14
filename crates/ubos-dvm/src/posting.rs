//! Double entry as a general mechanism of the kernel (STD-KIND-024, DSN-DVM-440,
//! DSN-DVM-441; FR-LEDG-051, FR-LEDG-052; PH-2 step 3b, DEC-098).
//!
//! - **Balanced posting groups.** A ledger class declares `ledger.double_entry: true`
//!   (STD-CLS-001); the entries of one commit that share a posting group then net to zero
//!   per currency, or the whole commit fails with `LEDG.UNBALANCED`, whose details name
//!   every group that does not balance and the difference per currency.
//! - **Hierarchies of ledger keys.** The nodes are Definition objects of the genesis class
//!   `LedgerNode` or of a Buk class below it (`parent`, `posting`, `currencies`). A ledger
//!   class takes part by declaring a key field that references such a class: its entries
//!   are then posted to nodes that are valid leaves at the valid time of the entry, and
//!   the host operations `ledger_rollup` and `ledger_hierarchy` read the totals of a
//!   subtree and the tree at the coordinates of a read (`branch`, a commit, `asof`).
//!   Both are kernel host API, not adapter operations of STD-CONF-005.
//!
//! The mechanisms are those of `ubos_ledger::posting`; what a node or a group means is
//! declared by Buks.

use crate::relations::uri_of;
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_ledger::posting::{Groups, Hierarchy, NO_UNIT, NodeDecl, Sums};
use ubos_model::Registry;
use ubos_types::error::invalid;
use ubos_types::time::parse_date;
use ubos_types::{BpaError, canonical};
use ubos_version::{Entry, Repo, State, Tx, at};

/// The genesis class of the nodes of a hierarchy of ledger keys (DSN-DVM-441).
pub const NODE_CLASS: &str = "LedgerNode";

/// `(object, version)` of the nodes of a hierarchy, by canonical URI.
type Held = BTreeMap<String, (String, String)>;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn is_node(reg: &Registry, class: &str) -> bool {
    reg.class(class)
        .is_ok_and(|c| c.linearization.iter().any(|l| l == NODE_CLASS))
}

fn names(ledger: Option<&Map<String, Value>>, member: &str) -> Vec<String> {
    ledger
        .and_then(|l| l.get(member))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The key fields of a ledger class that reference nodes of a hierarchy.
fn node_fields(reg: &Registry, class: &str) -> Vec<String> {
    let Ok(c) = reg.class(class) else {
        return Vec::new();
    };
    names(c.ledger.as_ref(), "key")
        .into_iter()
        .filter(|k| {
            c.fields.get(k).is_some_and(|f| {
                s(f, "type") == Some("reference")
                    && f.get("constraints")
                        .and_then(|c| s(c, "target_class"))
                        .is_some_and(|t| is_node(reg, t))
            })
        })
        .collect()
}

/// STD-KIND-024, DSN-DVM-440: the entries appended in a commit to classes with
/// `double_entry: true` balance per posting group and currency. A group belongs to its
/// class; an entry without a posting group is a group of its own. The amounts are those
/// of every declared amount field.
pub(crate) fn check_groups(reg: &Registry, appended: &[Entry]) -> Result<(), BpaError> {
    let mut groups = Groups::new();
    for e in appended {
        let c = reg.class(&e.class)?;
        let double = c.ledger.as_ref().and_then(|l| l.get("double_entry")) == Some(&json!(true));
        if !double {
            continue;
        }
        let sums = groups.group(&e.class, e.posting_group.as_deref().unwrap_or(&e.entry_id));
        for a in names(c.ledger.as_ref(), "amounts") {
            sums.add_value(e.body.get(&a).unwrap_or(&Value::Null))?;
        }
    }
    groups.check()
}

/// The hierarchy of node objects `(object, URI, version)`, by canonical URI, with the
/// object and version of each node.
fn hierarchy_of(
    objects: Vec<(String, String, String)>,
    versions: &mut dyn FnMut(&str) -> Result<Value, BpaError>,
) -> Result<(Hierarchy, Held), BpaError> {
    let mut decls = Vec::new();
    let mut held = BTreeMap::new();
    for (oid, uri, version) in objects {
        let v = versions(&version)?;
        if v["tombstone"] == json!(true) {
            continue;
        }
        decls.push(NodeDecl::of(&uri, &v["body"])?);
        held.insert(uri, (oid, version));
    }
    Ok((Hierarchy::build(decls)?, held))
}

/// The node objects of a state valid at `asof`: `(object, URI, version)`.
fn nodes_valid(
    reg: &Registry,
    st: &State,
    authority: &str,
    asof: &str,
) -> Vec<(String, String, String)> {
    st.objects
        .iter()
        .filter(|(_, o)| is_node(reg, &o.class))
        .filter_map(|(oid, o)| {
            let seg = at(&o.timeline, asof)?;
            Some((
                oid.clone(),
                uri_of(authority, &o.class, &o.slug),
                seg.version.clone(),
            ))
        })
        .collect()
}

/// The hierarchy as the transaction sees it at a valid time.
fn hierarchy_in(repo: &mut Repo<'_>, tx: &mut Tx, asof: &str) -> Result<Hierarchy, BpaError> {
    let reg = repo.reg;
    for class in reg.subclasses(NODE_CLASS) {
        repo.load_class(tx, &class)?;
    }
    let authority = repo.store.authority();
    let objects = nodes_valid(reg, &tx.state, &authority, asof);
    let tx = &*tx;
    Ok(hierarchy_of(objects, &mut |h| repo.version_in(tx, h))?.0)
}

/// The checks of a commit (in place of `Repo::check_double_entry`):
///
/// 1. the posting groups balance (`check_groups`);
/// 2. DSN-DVM-441, FR-LEDG-052: an entry appended to a class whose key references nodes
///    names a node that is valid at the valid time of the entry
///    (`MODEL.REFERENCE_INVALID`), is a leaf that takes entries and accepts the currencies
///    of the amounts (`MODEL.CONSTRAINT_VIOLATED`). Corrections of entries (reversals,
///    adjustments) keep the key of the entry they correct and are not checked again;
/// 3. the node objects written by the commit (`result.versions`) are well formed, and the
///    hierarchy at the start of each written version has known parents and no cycle.
pub(crate) fn check_commit(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    result: &Value,
) -> Result<(), BpaError> {
    let reg = repo.reg;
    check_groups(reg, &tx.appended)?;
    // the valid times at which the hierarchy is read, each once
    let mut cache: BTreeMap<String, Hierarchy> = BTreeMap::new();
    let appended: Vec<Entry> = tx
        .appended
        .iter()
        .filter(|e| e.corrects.is_none())
        .cloned()
        .collect();
    for e in &appended {
        let fields = node_fields(reg, &e.class);
        if fields.is_empty() {
            continue;
        }
        if !cache.contains_key(&e.valid_at) {
            let h = hierarchy_in(repo, tx, &e.valid_at)?;
            cache.insert(e.valid_at.clone(), h);
        }
        let h = &cache[&e.valid_at];
        let c = reg.class(&e.class)?;
        let mut sums = Sums::new();
        for a in names(c.ledger.as_ref(), "amounts") {
            sums.add_value(e.body.get(&a).unwrap_or(&Value::Null))?;
        }
        let currencies: Vec<&str> = sums
            .iter()
            .map(|(u, _)| u)
            .filter(|u| *u != NO_UNIT)
            .collect();
        for f in fields {
            let uri = s(&e.ledger_key, &f).unwrap_or_default();
            h.check_posting(uri, currencies.iter().copied())
                .map_err(|mut err| {
                    if let Some(d) = err.details.as_mut() {
                        d["entry_id"] = json!(e.entry_id);
                        d["field"] = json!(f);
                        d["valid_at"] = json!(e.valid_at);
                    }
                    err
                })?;
        }
    }
    let written: Vec<String> = result["versions"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, v)| v.as_str().map(str::to_string))
        .collect();
    let mut starts = BTreeSet::new();
    for vh in written {
        let v = repo.version_in(tx, &vh)?;
        if !is_node(reg, s(&v, "class").unwrap_or_default()) {
            continue;
        }
        if v["tombstone"] != json!(true) {
            NodeDecl::of("", &v["body"])?;
        }
        if let Some(from) = s(&v["valid"], "from") {
            starts.insert(from.to_string());
        }
    }
    for from in starts {
        hierarchy_in(repo, tx, &from)?;
    }
    Ok(())
}

impl Engine {
    /// The hierarchy at the coordinates of a read: `branch`, the commit that `time`,
    /// `commit`, `tag` or `tx_time` select (else the head) and the valid date `asof`.
    fn hierarchy_at(
        &mut self,
        branch: &str,
        input: &Value,
        asof: &str,
    ) -> Result<(Hierarchy, Held, BTreeMap<String, String>), BpaError> {
        let mut sel = json!({});
        for k in ["time", "commit", "tag", "tx_time", "evaluated_at"] {
            if let Some(v) = input.get(k).filter(|v| !v.is_null()) {
                sel[k] = v.clone();
            }
        }
        let view = self.needs_view(branch, &sel)?;
        let reg = &self.registry;
        let authority = self.store.authority();
        let mut repo = Repo::new(self.store.as_mut(), reg);
        let (st, ev) = if view {
            let (v, _) = repo.view(branch, &sel)?;
            (v.state, v.evaluated_at)
        } else {
            let snap = repo.select(branch, &sel)?;
            let c = repo.meta(snap.commit)?;
            let mut st = State::default();
            for class in reg.subclasses(NODE_CLASS) {
                st.objects
                    .extend(repo.objects_in(branch, c.cseq, Some(&class))?);
            }
            (st, BTreeMap::from([(branch.to_string(), c.commit_id)]))
        };
        let objects = nodes_valid(reg, &st, &authority, asof);
        let hashes: Vec<String> = objects.iter().map(|o| o.2.clone()).collect();
        let found = repo.versions(&hashes)?;
        let (h, held) = hierarchy_of(objects, &mut |h| match found.get(h) {
            Some(v) => Ok(v.clone()),
            None => repo.version(h),
        })?;
        Ok((h, held, ev))
    }

    fn rollup_date(&self, i: &Value) -> Result<String, BpaError> {
        let d = s(i, "asof")
            .map(str::to_string)
            .unwrap_or_else(|| self.now[..10].to_string());
        parse_date(d.get(..10).unwrap_or(&d))?;
        Ok(d)
    }

    /// `ledger_hierarchy {node?, branch?, time?, asof?}` (DSN-DVM-441): the nodes of the
    /// hierarchy at the coordinates, or of the subtree of `node`, in pre-order: each with
    /// its parent, depth, pre-order interval `[lo, hi]`, whether it is a leaf and takes
    /// entries, the currencies it accepts and the object and version that hold it.
    pub fn ledger_hierarchy(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let asof = self.rollup_date(i)?;
        let (h, held, ev) = self.hierarchy_at(&branch, i, &asof)?;
        let ids: Vec<&str> = match s(i, "node") {
            Some(n) => h.subtree(n)?,
            None => h.ids().collect(),
        };
        let mut nodes = Vec::new();
        for id in ids {
            let d = h.node(id)?;
            let (lo, hi) = h.interval(id)?;
            let leaf = h.is_leaf(id)?;
            nodes.push(
                json!({"node": id, "parent": d.parent, "depth": h.depth(id)?,
                "lo": lo, "hi": hi, "leaf": leaf,
                "posting": leaf && d.posting != Some(false), "currencies": d.currencies,
                "object_id": held[id].0, "version": held[id].1}),
            );
        }
        Ok(json!({"nodes": nodes, "asof": asof, "evaluated_at": ev}))
    }

    /// `ledger_rollup {class, node, key_field?, ledger_key?, detail?, branch?, time?,
    /// asof?}` (DSN-DVM-441, FR-LEDG-052): the totals of the amount fields of a ledger
    /// class over the entries posted to `node` and to every node below it, at the
    /// coordinates of the read: the entries of the commit with a valid time up to `asof`,
    /// over the hierarchy valid at `asof` (else today). `key_field` names the key field
    /// that references the nodes when the class has several; `ledger_key` gives the values
    /// of the other key fields. Money is totalled per currency and never across
    /// currencies. With `detail`, the totals of each node of the subtree are listed in
    /// pre-order.
    pub fn ledger_rollup(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let class = s(i, "class").unwrap_or_default().to_string();
        let c = self.registry.class(&class)?;
        if c.ledger.is_none() {
            return Err(invalid(format!("{class} is not a ledger class")));
        }
        let amounts = names(c.ledger.as_ref(), "amounts");
        let key = names(c.ledger.as_ref(), "key");
        let candidates = node_fields(&self.registry, &class);
        let field = match (s(i, "key_field"), candidates.as_slice()) {
            (Some(f), _) if candidates.iter().any(|x| x == f) => f.to_string(),
            (None, [one]) => one.clone(),
            (Some(f), _) => return Err(invalid(format!("{f} is no key field of nodes"))),
            (None, []) => {
                return Err(invalid(format!("no key field of {class} references nodes")));
            }
            (None, _) => return Err(invalid("key_field names the key field of the nodes")),
        };
        let mut fixed = match i.get("ledger_key") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(m)) => m.clone(),
            Some(_) => return Err(invalid("ledger_key gives the other key fields")),
        };
        fixed.remove(&field);
        if key.iter().any(|k| *k != field && !fixed.contains_key(k))
            || fixed.keys().any(|k| !key.contains(k))
        {
            return Err(invalid("ledger_key gives the other key fields"));
        }
        let node = s(i, "node").ok_or_else(|| invalid("node is the URI of a node"))?;
        let asof = self.rollup_date(i)?;
        let (h, held, ev) = self.hierarchy_at(&branch, i, &asof)?;
        let members: Vec<String> = h.subtree(node)?.into_iter().map(str::to_string).collect();
        // the sums of each node, per amount field
        let mut own: BTreeMap<&str, BTreeMap<String, Sums>> = BTreeMap::new();
        for f in &amounts {
            own.insert(f, BTreeMap::new());
        }
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut evaluated_at = json!(ev);
        for m in &members {
            let mut read = i.clone();
            let mut lk = fixed.clone();
            lk.insert(field.clone(), json!(m));
            read["ledger_key"] = Value::Object(lk);
            read["asof"] = json!(asof);
            let got = self.read_ledger(&read)?;
            evaluated_at = got["evaluated_at"].clone();
            let entries = got["entries"].as_array().cloned().unwrap_or_default();
            counts.insert(m, entries.len());
            for f in &amounts {
                let sums = own
                    .get_mut(f.as_str())
                    .expect("amount field")
                    .entry(m.clone())
                    .or_default();
                for e in &entries {
                    sums.add_value(e["body"].get(f).unwrap_or(&Value::Null))?;
                }
            }
        }
        let mut totals = Map::new();
        for f in &amounts {
            totals.insert(f.clone(), h.roll_up(node, &own[f.as_str()])?.to_value()?);
        }
        let hierarchy: Vec<Value> = members
            .iter()
            .map(|m| json!({"node": m, "object_id": held[m].0, "version": held[m].1}))
            .collect();
        let mut out = json!({"node": node, "key_field": field, "totals": totals,
            "entries": counts.values().sum::<usize>(), "nodes": members.len(),
            "leaves": h.leaves(node)?.len(), "hierarchy": hierarchy,
            "ledger_key": canonical(&Value::Object(fixed))?,
            "asof": asof, "evaluated_at": evaluated_at});
        if i.get("detail") == Some(&json!(true)) {
            let mut rows = Vec::new();
            for m in &members {
                let mut own_totals = Map::new();
                let mut rolled = Map::new();
                for f in &amounts {
                    let of = &own[f.as_str()];
                    own_totals.insert(f.clone(), of[m].to_value()?);
                    rolled.insert(f.clone(), h.roll_up(m, of)?.to_value()?);
                }
                rows.push(
                    json!({"node": m, "depth": h.depth(m)?, "leaf": h.is_leaf(m)?,
                    "entries": counts[m.as_str()], "own": own_totals, "totals": rolled}),
                );
            }
            out["members"] = json!(rows);
        }
        Ok(out)
    }
}
