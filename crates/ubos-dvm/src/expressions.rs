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
use ubos_expr::{Env, Evaluator, Node, OrderKey, Resolver, Ty, Val, from_json, order_key};
use ubos_model::{CustomTypes, Registry};
use ubos_types::values::validate_value;
use ubos_types::{BpaError, canonical, content_id};
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

/// Base64url without padding to bytes.
fn b64u_decode(s: &str) -> Option<Vec<u8>> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = A.iter().position(|&x| x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
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

/// Sort key of an `order_by` value: nulls first, then by the value's order key.
fn sort_key(v: &Val) -> Result<Option<OrderKey>, BpaError> {
    if v.is_null() {
        return Ok(None);
    }
    order_key(v).map(Some).ok_or_else(|| {
        BpaError::with_details(
            "LOGIC.TYPE_ERROR",
            json!({"position": 0, "message": "values are not ordered"}),
        )
    })
}

impl Engine {
    /// `query` (STD-EXPR-011): the objects of a class (and its subclasses unless `only`)
    /// at a snapshot that satisfy `where`, ordered by `order_by` then by object id, paged
    /// by `limit` and an opaque cursor bound to the query and the commit.
    pub fn query(&mut self, i: &Value) -> Result<Value, BpaError> {
        let spec = json!({
            "from": i.get("from").cloned().unwrap_or(Value::Null),
            "only": i.get("only").cloned().unwrap_or(Value::Null),
            "where": i.get("where").cloned().unwrap_or(Value::Null),
            "order_by": i.get("order_by").cloned().unwrap_or(Value::Null),
            "limit": i.get("limit").cloned().unwrap_or(Value::Null),
            "asof": i.get("asof").cloned().unwrap_or(Value::Null),
        });
        let qhash = content_id("ubos.chunk.v1", &spec)?;
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let mut offset: u64 = 0;
        let sel = match s(i, "cursor").filter(|c| !c.is_empty()) {
            Some(c) => {
                let cur: Value = b64u_decode(c)
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .ok_or_else(|| bad_query("unknown cursor"))?;
                if !cur.is_object()
                    || s(&cur, "q") != Some(qhash.as_str())
                    || s(&cur, "branch") != Some(branch.as_str())
                {
                    return Err(bad_query("the cursor belongs to another query"));
                }
                offset = cur.get("offset").and_then(Value::as_u64).unwrap_or(0);
                json!({"commit": cur.get("commit").cloned().unwrap_or(Value::Null)})
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
        let cls = s(&spec, "from")
            .filter(|c| self.registry.class(c).is_ok())
            .ok_or_else(|| {
                bad_query(&format!(
                    "unknown class {}",
                    s(&spec, "from").unwrap_or_default()
                ))
            })?
            .to_string();
        let limit = match &spec["limit"] {
            Value::Null => DEFAULT_LIMIT,
            Value::Number(n) => match n.as_u64() {
                Some(l) if (1..=MAX_LIMIT).contains(&l) => l,
                _ => return Err(bad_query(&format!("limit is 1 to {MAX_LIMIT}"))),
            },
            _ => return Err(bad_query(&format!("limit is 1 to {MAX_LIMIT}"))),
        };
        let this_t = ubos_expr::class_type(&self.registry, &cls)?;
        let Ty::Obj(members) = &this_t else {
            unreachable!("a class type is an object")
        };
        let env = Env::new(Some(this_t.clone()), None, None, None);
        let where_ = match spec.get("where").filter(|w| !w.is_null()) {
            Some(w) => {
                let ast = ubos_expr::parse(w)?;
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
        let mut orders: Vec<(Node, bool)> = Vec::new();
        for o in spec["order_by"].as_array().into_iter().flatten() {
            let dir = o.get("dir").map(|d| d.as_str()).unwrap_or(Some("asc"));
            if !o.is_object() || !matches!(dir, Some("asc" | "desc")) {
                return Err(bad_query("order_by items are {path, dir}"));
            }
            let ast = ubos_expr::parse(o.get("path").unwrap_or(&Value::Null))?;
            if !matches!(ast, Node::Member(..) | Node::Path(..)) {
                return Err(bad_query("order_by names a path"));
            }
            ubos_expr::check(&ast, &mut env.clone())?;
            orders.push((ast, dir == Some("desc")));
        }
        let asof = s(&spec, "asof").map(str::to_string);
        let only = spec["only"] == json!(true);

        let now = self.now.clone();
        let reg = &self.registry;
        let default_valid = move |c: &str| match reg.class(c).map(|c| c.valid_time.as_str()) {
            Ok("instant") => now.clone(),
            _ => now[..10].to_string(),
        };
        let authority = self.store.authority();
        let mut repo = Repo::new(self.store.as_mut(), reg);
        let (view, snap) = repo.view(&branch, &sel)?;
        let st = &view.state;
        // candidate objects and their bodies, by object id
        let mut cands: Vec<(String, Val)> = Vec::new();
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
            let v = repo.version(&seg.version)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            let body: serde_json::Map<String, Value> = v["body"]
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| members.contains_key(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            cands.push((oid.clone(), from_json(&this_t, &Value::Object(body))));
        }
        let commit_id = repo.commit(snap.commit)?.commit_id;
        let reader = Reader {
            repo: RefCell::new(&mut repo),
            st,
            reg,
            authority,
            asof: asof.clone(),
            default_valid: &default_valid,
        };
        let mut rows: Vec<(String, Vec<Option<OrderKey>>)> = Vec::new();
        for (oid, this) in cands {
            if let Some(w) = &where_ {
                let mut e = Evaluator::new(&env);
                e.this = Some(this.clone());
                e.resolver = Some(&reader);
                if e.eval(w)? != Val::Bool(true) {
                    continue;
                }
            }
            let mut keys = Vec::with_capacity(orders.len());
            for (ast, _) in &orders {
                let mut e = Evaluator::new(&env);
                e.this = Some(this.clone());
                e.resolver = Some(&reader);
                keys.push(sort_key(&e.eval(ast)?)?);
            }
            rows.push((oid, keys));
        }
        // sort: by object id, then stably by each key from the last to the first
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        for (n, (_, desc)) in orders.iter().enumerate().rev() {
            rows.sort_by(|a, b| {
                let o = a.1[n].cmp(&b.1[n]);
                if *desc { o.reverse() } else { o }
            });
        }
        let total = rows.len() as u64;
        let page: Vec<Value> = rows
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(oid, _)| json!(oid))
            .collect();
        let mut out = json!({"objects": page, "evaluated_at": view.evaluated_at});
        if offset + limit < total {
            let c = json!({"branch": branch, "commit": commit_id, "offset": offset + limit, "q": qhash});
            out["cursor"] = json!(ubos_types::addr::b64u(canonical(&c)?.as_bytes()));
        }
        Ok(out)
    }
}
