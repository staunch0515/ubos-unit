//! L1 expressions in the engine (PH-1 step 5a, DEC-055): the evaluator of custom value
//! types installed on the registry (STD-FND-026), the pure adapter operations
//! `check_expr`, `eval_expr` and `validate_value` with value types, and the criteria
//! query over a snapshot (STD-EXPR-010, -011; placed here until `ubos-query`, PH-1b).

use crate::relations;
use crate::stateful::Engine;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;
use ubos_expr::{Env, Evaluator, Node, OrderKey, Resolver, Ty, Val, from_json};
use ubos_model::{CustomTypes, Registry};
use ubos_types::BpaError;
use ubos_types::values::validate_value;
use ubos_version::{Repo, State, at};

/// Largest page of a query.
pub const MAX_LIMIT: u64 = 1000;
/// Default page of a query.
pub const DEFAULT_LIMIT: u64 = 100;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn bad_query(m: &str) -> BpaError {
    BpaError::new("QRY.INVALID_QUERY", m)
}

/// The evaluator of custom value types over `ubos-expr`.
#[derive(Debug, Default)]
pub struct ExprTypes;

/// A registry with the expression evaluator installed.
pub fn registry() -> Registry {
    Registry::new().with_custom(Arc::new(ExprTypes))
}

impl CustomTypes for ExprTypes {
    fn check(&self, d: &Value) -> Result<(), BpaError> {
        let base = ubos_expr::field_type(
            &json!({"type": s(d, "base").unwrap_or_default()}),
            &Default::default(),
        );
        for (member, want) in [("normalize", base.clone()), ("validate", Ty::Boolean)] {
            let Some(src) = d.get(member).filter(|v| !v.is_null()) else {
                continue;
            };
            let mut env = Env::new(None, None, None, None);
            env.vars.insert("value".into(), base.clone());
            let t = ubos_expr::check(&ubos_expr::parse(src)?, &mut env)?;
            if ubos_expr::value::compat(&t, &want) != Some(want.clone()) && !t.is_dyn() {
                return Err(BpaError::with_details(
                    "LOGIC.TYPE_ERROR",
                    json!({"position": 0, "message": format!("{member} returns {}", ubos_expr::type_name(&t))}),
                ));
            }
        }
        Ok(())
    }

    fn value(
        &self,
        d: &Value,
        value: &Value,
        constraints: Option<&Value>,
    ) -> Result<Value, BpaError> {
        ubos_expr::custom_value(d, value, constraints)
    }
}

/// `validate_value`; since 1.0-rc1 with custom value types given in `value_types`
/// (STD-FND-026).
pub fn validate(i: &Value) -> Result<Value, BpaError> {
    let type_ = s(i, "type").unwrap_or_default();
    let value = i.get("value").unwrap_or(&Value::Null);
    let Some(decls) = i.get("value_types").and_then(Value::as_array) else {
        return validate_value(type_, i.get("constraints"), value);
    };
    let mut known = BTreeSet::new();
    for d in decls {
        ubos_model::check_value_type(d, &known)?;
        ExprTypes.check(d)?;
        known.insert(s(d, "name").unwrap_or_default().to_string());
    }
    match decls.iter().find(|d| s(d, "name") == Some(type_)) {
        Some(d) => ubos_expr::custom_value(d, value, i.get("constraints")),
        None => validate_value(type_, i.get("constraints"), value),
    }
}

/// Reads objects of a snapshot by canonical URI, typed by their class (the `ref()` and
/// `exists()` of a query's expressions).
struct Reader<'a, 'r> {
    repo: RefCell<&'r mut Repo<'a>>,
    st: &'r State,
    reg: &'r Registry,
    authority: String,
    asof: Option<String>,
    default_valid: &'r dyn Fn(&str) -> String,
}

impl Resolver for Reader<'_, '_> {
    fn resolve(&self, uri: &str) -> Result<Option<Val>, BpaError> {
        let Some(oid) = relations::resolve(self.st, uri, &self.authority) else {
            return Ok(None);
        };
        let o = &self.st.objects[&oid];
        let a = self
            .asof
            .clone()
            .unwrap_or_else(|| (self.default_valid)(&o.class));
        let Some(seg) = at(&o.timeline, &a) else {
            return Ok(None);
        };
        let body = self.repo.borrow_mut().version(&seg.version)?["body"].clone();
        Ok(Some(from_json(
            &ubos_expr::class_type(self.reg, &o.class)?,
            &body,
        )))
    }
}

/// A candidate row of a query: the object, its head version at the coordinates and the
/// typed `this`.
struct Row {
    oid: String,
    this: Val,
}

/// What a repeated scan of the order-by pushdown carries over (step 13d): the rows that
/// passed so far with their sort keys, the key the next scan starts from, the objects
/// already read at that key, and the index rows read so far (the scan budget).
struct Carry {
    rows: Vec<(Row, Vec<Option<OrderKey>>)>,
    lo: Vec<u8>,
    seen: BTreeSet<String>,
    scanned: u64,
}

impl Engine {
    /// `query` (STD-EXPR-010, -011; DSN-DVM-901…908): the objects of a class (and its
    /// subclasses unless `only`) at a snapshot that satisfy `where`, ordered by `order_by`
    /// then by object id, paged by `limit` and a signed cursor bound to the query and the
    /// commit. Candidates come from a declared field index when a comparison of the
    /// `where` yields a key range (DSN-DVM-902), else from the class, within the scan
    /// budget of the profile (`QRY.SCAN_LIMIT`, DSN-DVM-903); the `where` is evaluated on
    /// every candidate. `states` filters by lifecycle state, `select` returns field
    /// values (`rows`), `group_by` and `aggregates` return `groups` (DSN-DVM-905).
    pub fn query(&mut self, i: &Value) -> Result<Value, BpaError> {
        self.query_in(i, None, None)
    }

    /// `query` with the number of index rows the order-by pushdown reads (step 13d): when
    /// the query orders by the indexed field of its best bound, ascending, over one class,
    /// the index scan reads `chunk` rows in key order and the page is complete once
    /// `offset + limit` rows pass the predicates; otherwise the scan continues from the
    /// last key read with four times the rows (`carry`), up to the scan budget.
    fn query_in(
        &mut self,
        i: &Value,
        chunk: Option<usize>,
        carry: Option<Carry>,
    ) -> Result<Value, BpaError> {
        let spec = ubos_query::spec_of(i)?;
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let key = self.cursor_key();
        let mut offset: u64 = 0;
        let sel = match s(i, "cursor").filter(|c| !c.is_empty()) {
            Some(c) => {
                let cur = ubos_query::Cursor::decode(&key, c)?;
                if cur.q != spec.hash || cur.branch != branch {
                    return Err(bad_query("the cursor belongs to another query"));
                }
                offset = cur.offset;
                json!({"commit": cur.commit})
            }
            None => {
                let mut sel = json!({});
                for k in ["time", "commit", "tag", "tx_time"] {
                    if let Some(v) = i.get(k).filter(|v| !v.is_null()) {
                        sel[k] = v.clone();
                    }
                }
                sel
            }
        };
        let cls = spec.from.clone();
        if self.registry.class(&cls).is_err() {
            return Err(bad_query(&format!("unknown class {cls}")));
        }
        let limit = spec.limit;
        let this_t = ubos_expr::class_type(&self.registry, &cls)?;
        let Ty::Obj(members) = &this_t else {
            unreachable!("a class type is an object")
        };
        let env = Env::new(Some(this_t.clone()), None, None, None);
        let where_ = match &spec.where_ {
            Some(w) => {
                let ast = ubos_expr::parse(&json!(w))?;
                let t = ubos_expr::check(&ast, &mut env.clone())?;
                if t != Ty::Boolean && !t.is_dyn() {
                    return Err(BpaError::with_details(
                        "LOGIC.TYPE_ERROR",
                        json!({"position": 0, "message": "where is a boolean expression"}),
                    ));
                }
                Some(ast)
            }
            None => None,
        };
        let path_ast = |p: &str| -> Result<Node, BpaError> {
            let text = if p == "this" || p.starts_with("this.") || p.starts_with("this?.") {
                p.to_string()
            } else {
                format!("this.{p}")
            };
            let ast = ubos_expr::parse(&json!(text))?;
            if !matches!(ast, Node::Member(..) | Node::Path(..)) {
                return Err(bad_query("a path names a member of this"));
            }
            ubos_expr::check(&ast, &mut env.clone())?;
            Ok(ast)
        };
        let mut orders: Vec<(Node, bool)> = Vec::new();
        for o in &spec.order_by {
            let ast = ubos_expr::parse(&json!(o.path))?;
            if !matches!(ast, Node::Member(..) | Node::Path(..)) {
                return Err(bad_query("order_by names a path"));
            }
            ubos_expr::check(&ast, &mut env.clone())?;
            orders.push((ast, o.desc));
        }
        let selects: Vec<(String, Node)> = spec
            .select
            .iter()
            .map(|p| path_ast(p).map(|a| (p.clone(), a)))
            .collect::<Result<_, _>>()?;
        let groups: Vec<Node> = spec
            .group_by
            .iter()
            .map(|p| path_ast(p))
            .collect::<Result<_, _>>()?;
        let aggs: Vec<Option<Node>> = spec
            .aggregates
            .iter()
            .map(|a| match s(a, "path") {
                Some(p) => path_ast(p).map(Some),
                None => Ok(None),
            })
            .collect::<Result<_, _>>()?;
        let asof = spec.asof.clone();
        let only = spec.only;
        let reaches = where_.as_ref().is_some_and(ubos_query::reaches_objects)
            || orders.iter().any(|(a, _)| ubos_query::reaches_objects(a))
            || selects.iter().any(|(_, a)| ubos_query::reaches_objects(a))
            || groups.iter().any(ubos_query::reaches_objects)
            || aggs.iter().flatten().any(ubos_query::reaches_objects);
        let profile = s(i, "profile").unwrap_or("standard");
        let budget = self
            .max_scan
            .unwrap_or_else(|| ubos_query::scan_budget(profile));

        // row filters (DSN-DVM-820): with a principal and a policy set on the branch,
        // every row is subject to `read`; denied rows are left out (a residual post-filter
        // until partial evaluation is measured in step 13d)
        let principal = Engine::principal_of(i);
        let policy = match &principal {
            Some(_) => self.policy_set_at(&branch, None)?,
            None => None,
        };
        let access_ctx = match &principal {
            Some(_) => Some(self.access_ctx(i, &branch)?),
            None => None,
        };
        let branch_kind = self.branch_meta(&branch)?.kind;
        let mut denied_log: Vec<Value> = Vec::new();
        let now = self.now.clone();
        let now_text = self.now.clone();
        // the view path (a parent line, a tag or transaction-time coordinate, or
        // expressions reaching other objects) materialises the snapshot; the index path
        // reads the candidates of the class or of a field index (DSN-DVM-902, -903)
        let view_path = reaches
            || ["tag", "tx_time"].iter().any(|k| sel.get(*k).is_some())
            || self.branch_meta(&branch)?.parent_line.is_some();
        // a rebuilt index serves the branch from its rebuild commit (step 13d)
        let lineage = self.registry.class(&cls)?.linearization.clone();
        let mut usable: BTreeSet<String> = BTreeSet::new();
        if !view_path {
            let snap_cseq = {
                let mut r = Repo::new(self.store.as_mut(), &self.registry);
                let snap = r.select(&branch, &sel)?;
                r.meta(snap.commit)?.cseq
            };
            for (f, _) in self.registry.indexed_fields(&cls) {
                if self.index_usable(&branch, snap_cseq, &lineage, &f)? {
                    usable.insert(f);
                }
            }
        }
        let mut pushdown: Option<(usize, bool)> = None;
        let mut scanned_rows: u64 = 0;
        let mut next_lo: Option<(Vec<u8>, BTreeSet<String>)> = None;
        let reg = &self.registry;
        let default_valid = move |c: &str| match reg.class(c).map(|c| c.valid_time.as_str()) {
            Ok("instant") => now.clone(),
            _ => now[..10].to_string(),
        };
        let authority = self.store.authority();
        let mut repo = Repo::new(self.store.as_mut(), reg);
        let (st, commit_id, evaluated_at): (State, String, Value) = if view_path {
            let (view, snap) = repo.view(&branch, &sel)?;
            let commit_id = repo.meta(snap.commit)?.commit_id;
            (view.state, commit_id, json!(view.evaluated_at))
        } else {
            let snap = repo.select(&branch, &sel)?;
            let c = repo.meta(snap.commit)?;
            let (cseq, commit_id) = (c.cseq, c.commit_id);
            let classes: Vec<String> = if only {
                vec![cls.clone()]
            } else {
                reg.subclasses(&cls)
            };
            let indexed: Vec<(String, String)> = reg
                .indexed_fields(&cls)
                .into_iter()
                .filter(|(f, _)| usable.contains(f))
                .collect();
            let bounds = where_
                .as_ref()
                .map(|w| ubos_query::bounds(w, &indexed))
                .unwrap_or_default();
            let best = ubos_query::best_bound(&bounds).cloned();
            // order-by pushdown (step 13d): one class, ordered by the bound field
            if let Some(b) = &best
                && classes.len() == 1
                && spec.aggregates.is_empty()
                && spec.order_by.len() == 1
                && !spec.order_by[0].desc
                && spec.order_by[0]
                    .path
                    .strip_prefix("this.")
                    .unwrap_or(&spec.order_by[0].path)
                    == b.field
            {
                let want = (offset + limit) as usize;
                let fetch = chunk
                    .unwrap_or_else(|| want.max(64).saturating_mul(2))
                    .min(budget as usize + 1);
                pushdown = Some((fetch, false));
            }
            let fetch = match pushdown {
                Some((f, _)) => f.saturating_add(1),
                None => (budget as usize).saturating_add(1),
            };
            let lo: Option<&[u8]> = match (&carry, &best) {
                (Some(c), _) => Some(c.lo.as_slice()),
                (None, Some(b)) => b.lo.as_deref(),
                (None, None) => None,
            };
            let scanned = match &best {
                Some(b) => repo.field_scan(
                    &branch,
                    cseq,
                    &classes,
                    &b.field,
                    lo,
                    b.hi.as_deref(),
                    fetch,
                )?,
                None => None,
            };
            let objects = match scanned {
                Some(mut rows) => {
                    match pushdown.as_mut() {
                        Some((f, more)) => {
                            *more = rows.len() > *f;
                            rows.truncate(*f);
                            scanned_rows = rows.len() as u64;
                            if let Some(last) = rows.last() {
                                let key = last.1.clone();
                                let mut seen: BTreeSet<String> = rows
                                    .iter()
                                    .filter(|(_, k)| *k == key)
                                    .map(|(id, _)| id.clone())
                                    .collect();
                                if let Some(c) = &carry
                                    && c.lo == key
                                {
                                    seen.extend(c.seen.iter().cloned());
                                }
                                next_lo = Some((key, seen));
                            }
                            if let Some(c) = &carry {
                                rows.retain(|(id, _)| !c.seen.contains(id));
                            }
                        }
                        None => {
                            if rows.len() as u64 > budget {
                                return Err(scan_limit(budget, rows.len() as u64, &cls, None));
                            }
                        }
                    }
                    let ids: Vec<String> = rows.into_iter().map(|(id, _)| id).collect();
                    repo.objects_in_ids(&branch, cseq, &ids)?
                }
                None => {
                    let mut n = 0u64;
                    for c in &classes {
                        n += repo.count_in(&branch, cseq, Some(c))?;
                    }
                    if n > budget {
                        let hint = where_.as_ref().and_then(ubos_query::suggested_index);
                        return Err(scan_limit(budget, n, &cls, hint.as_deref()));
                    }
                    let mut all = std::collections::BTreeMap::new();
                    for c in &classes {
                        all.extend(repo.objects_in(&branch, cseq, Some(c))?);
                    }
                    all
                }
            };
            let st = State {
                objects,
                ledger: Vec::new(),
            };
            let ev = json!({&branch: commit_id});
            (st, commit_id, ev)
        };
        // the candidates: class, liveness, the segment at asof, the lifecycle state
        let mut cands: Vec<Row> = Vec::new();
        // the versions of the candidates in one read (step 13d)
        let mut hashes: Vec<String> = Vec::new();
        for o in st.objects.values() {
            let lin = &reg.class(&o.class)?.linearization;
            if !(if only {
                o.class == cls
            } else {
                lin.contains(&cls)
            }) {
                continue;
            }
            let a = asof.clone().unwrap_or_else(|| default_valid(&o.class));
            if let Some(seg) = at(&o.timeline, &a) {
                hashes.push(seg.version.clone());
            }
        }
        let fetched = repo.versions(&hashes)?;
        for (oid, o) in &st.objects {
            let lin = &reg.class(&o.class)?.linearization;
            let hit = if only {
                o.class == cls
            } else {
                lin.contains(&cls)
            };
            if !hit {
                continue;
            }
            let a = asof.clone().unwrap_or_else(|| default_valid(&o.class));
            let Some(seg) = at(&o.timeline, &a) else {
                continue;
            };
            let v = match fetched.get(&seg.version) {
                Some(v) => v.clone(),
                None => repo.version(&seg.version)?,
            };
            if v["tombstone"] == json!(true) {
                continue;
            }
            if !spec.states.is_empty()
                && !v
                    .get("state")
                    .and_then(Value::as_str)
                    .is_some_and(|st| spec.states.iter().any(|x| x == st))
            {
                continue;
            }
            let body: serde_json::Map<String, Value> = v["body"]
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| members.contains_key(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if let (Some(p), Some((model, compiled)), Some(ctx)) =
                (&principal, &policy, &access_ctx)
            {
                let resource = ubos_authz::Resource::Object {
                    oid: oid.clone(),
                    class: o.class.clone(),
                    lineage: lin.clone(),
                    branch: branch.clone(),
                    branch_kind: branch_kind.clone(),
                    slug: o.slug.clone(),
                    state: v.get("state").and_then(Value::as_str).map(str::to_string),
                    body: v["body"].as_object().cloned().unwrap_or_default(),
                };
                let d =
                    ubos_authz::authorize(model, compiled, p, "read", &resource, &authority, ctx)?;
                if !d.allow {
                    denied_log.push(json!({"principal": p.id, "action": "read", "branch": branch,
                        "resource": {"kind": "object", "class": o.class, "object_id": oid},
                        "outcome": "deny", "policies": d.policies, "at": now_text, "query": spec.hash}));
                    continue;
                }
            }
            cands.push(Row {
                oid: oid.clone(),
                this: from_json(&this_t, &Value::Object(body)),
            });
        }
        let reader = Reader {
            repo: RefCell::new(&mut repo),
            st: &st,
            reg,
            authority,
            asof: asof.clone(),
            default_valid: &default_valid,
        };
        let eval = |ast: &Node, this: &Val| -> Result<Val, BpaError> {
            let mut e = Evaluator::new(&env);
            e.this = Some(this.clone());
            e.resolver = Some(&reader);
            e.eval(ast)
        };
        let mut rows: Vec<(Row, Vec<Option<OrderKey>>)> = Vec::new();
        for row in cands {
            if let Some(w) = &where_
                && eval(w, &row.this)? != Val::Bool(true)
            {
                continue;
            }
            let mut keys = Vec::with_capacity(orders.len());
            for (ast, _) in &orders {
                keys.push(ubos_query::sort_key(&eval(ast, &row.this)?)?);
            }
            rows.push((row, keys));
        }
        let (carried, scanned_total) = match carry {
            Some(c) => (c.rows, c.scanned + scanned_rows),
            None => (Vec::new(), scanned_rows),
        };
        if let Some((fetch, more)) = pushdown
            && more
            && ((carried.len() + rows.len()) as u64) < offset + limit
        {
            if scanned_total >= budget {
                return Err(scan_limit(budget, scanned_total, &cls, None));
            }
            let Some((lo, seen)) = next_lo else {
                unreachable!("a scan with more rows read at least one")
            };
            drop(reader);
            drop(repo);
            let mut passed = carried;
            passed.extend(rows);
            let next = fetch
                .saturating_mul(4)
                .min((budget - scanned_total) as usize);
            return self.query_in(
                i,
                Some(next.max(1)),
                Some(Carry {
                    rows: passed,
                    lo,
                    seen,
                    scanned: scanned_total,
                }),
            );
        }
        let mut rows = if carried.is_empty() {
            rows
        } else {
            let mut all = carried;
            all.extend(rows);
            all
        };
        if !spec.aggregates.is_empty() {
            let mut grouped = Vec::with_capacity(rows.len());
            for (row, _) in &rows {
                let ks: Vec<Val> = groups
                    .iter()
                    .map(|g| eval(g, &row.this))
                    .collect::<Result<_, _>>()?;
                let vs: Vec<Val> = aggs
                    .iter()
                    .map(|a| match a {
                        Some(ast) => eval(ast, &row.this),
                        None => Ok(Val::Int(1)),
                    })
                    .collect::<Result<_, _>>()?;
                grouped.push((ks, vs));
            }
            let groups = ubos_query::aggregate(&spec, grouped)?;
            drop(reader);
            drop(repo);
            self.decision_log.extend(denied_log);
            return Ok(json!({"groups": groups, "evaluated_at": evaluated_at}));
        }
        rows.sort_by(|a, b| a.0.oid.cmp(&b.0.oid));
        for (n, (_, desc)) in orders.iter().enumerate().rev() {
            rows.sort_by(|a, b| {
                let o = a.1[n].cmp(&b.1[n]);
                if *desc { o.reverse() } else { o }
            });
        }
        let total = rows.len() as u64;
        let page: Vec<&Row> = rows
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(r, _)| r)
            .collect();
        let mut out = json!({"objects": page.iter().map(|r| json!(r.oid)).collect::<Vec<_>>(),
            "evaluated_at": evaluated_at});
        if !selects.is_empty() {
            let mut items = Vec::with_capacity(page.len());
            for r in &page {
                let mut fields = serde_json::Map::new();
                for (p, ast) in &selects {
                    fields.insert(p.clone(), ubos_expr::to_json(&eval(ast, &r.this)?)?);
                }
                items.push(json!({"object_id": r.oid, "fields": fields}));
            }
            out["rows"] = Value::Array(items);
        }
        if offset + limit < total {
            let c = ubos_query::Cursor {
                q: spec.hash.clone(),
                branch,
                commit: commit_id,
                offset: offset + limit,
            };
            out["cursor"] = json!(c.encode(&key)?);
        }
        drop(reader);
        drop(repo);
        self.decision_log.extend(denied_log);
        Ok(out)
    }
}

/// `QRY.SCAN_LIMIT` with the budget, the estimate and the suggested index (DSN-DVM-903).
fn scan_limit(budget: u64, scanned: u64, class: &str, field: Option<&str>) -> BpaError {
    let mut d = json!({"budget": budget, "estimated": scanned, "class": class});
    if let Some(f) = field {
        d["suggested_index"] = json!({"class": class, "field": f});
    }
    BpaError::with_details("QRY.SCAN_LIMIT", d)
}
