//! Governance sheets in the kernel (STD-LGS-002, -010, -020, -022, -023; DSN-DVM-703…706,
//! -710): the sheet body derived at commit, the checks of the sheet set at commit, and the
//! cascade of an object at a coordinate over the sheets effective there.

use crate::relations::{extensions_of, resolve, uri_of};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_expr::{Env, Evaluator, Val, from_json};
use ubos_model::Registry;
use ubos_rules::{Matcher, SheetRef};
use ubos_types::error::invalid;
use ubos_types::{BpaError, canonical};
use ubos_version::{Repo, State, Tx, at};

/// Classes whose objects take part in the sheet checks.
pub const SHEET_CLASSES: &[&str] = &["GovernanceSheet", "PropertyDecl"];

/// `ast` is derived from `source` at commit and `source` stored in its canonical form; an
/// `ast` given with the source must agree (STD-LGS-002, DEC-066 item 5).
pub fn sheet_body(class: &str, body: &mut Map<String, Value>) -> Result<(), BpaError> {
    if class != "GovernanceSheet" {
        return Ok(());
    }
    let Some(src) = body.get("source").filter(|v| v.is_string()).cloned() else {
        return Ok(());
    };
    let p = ubos_rules::parse_sheet(&src)?;
    if let Some(ast) = body.get("ast").filter(|a| !a.is_null())
        && canonical(ast)? != canonical(&p["ast"])?
    {
        return Err(invalid("ast does not match source"));
    }
    body.insert("ast".into(), p["ast"].clone());
    body.insert("source".into(), p["source"].clone());
    Ok(())
}

/// Live sheet and property-declaration bodies of a state at a coordinate: the head
/// versions in a transaction, or the versions at `asof`.
struct SheetSet {
    sheets: Vec<(String, String, String, Value)>,
    properties: Vec<Value>,
}

fn live_versions(
    repo: &mut Repo<'_>,
    tx: Option<&Tx>,
    st: &State,
    asof: Option<&str>,
) -> Result<SheetSet, BpaError> {
    let mut out = SheetSet {
        sheets: Vec::new(),
        properties: Vec::new(),
    };
    for (oid, o) in &st.objects {
        if !SHEET_CLASSES.contains(&o.class.as_str()) {
            continue;
        }
        let vh = match asof {
            None => o.head.clone(),
            Some(a) => match at(&o.timeline, a) {
                Some(seg) => seg.version.clone(),
                None => continue,
            },
        };
        let v = match tx {
            Some(tx) => repo.version_in(tx, &vh)?,
            None => repo.version(&vh)?,
        };
        if v["tombstone"] == json!(true) {
            continue;
        }
        let body = v["body"].clone();
        if o.class == "PropertyDecl" {
            out.properties.push(body);
        } else {
            let uri = uri_of(&repo.store.authority(), &o.class, &o.slug);
            out.sheets.push((oid.clone(), uri, vh, body));
        }
    }
    Ok(out)
}

fn refs(set: &SheetSet) -> Vec<SheetRef<'_>> {
    set.sheets
        .iter()
        .map(|(id, uri, vh, body)| SheetRef {
            id: id.clone(),
            uri: uri.clone(),
            version: vh.clone(),
            body,
        })
        .collect()
}

/// The checks of the sheets and property declarations written by a commit, over the live
/// sheets of the branch (STD-LGS-002, -010, -022).
pub fn check_sheets(
    repo: &mut Repo<'_>,
    tx: &Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    let wr: BTreeSet<String> = written
        .iter()
        .filter(|(oid, _)| {
            tx.state
                .objects
                .get(oid)
                .is_some_and(|o| SHEET_CLASSES.contains(&o.class.as_str()))
        })
        .map(|(oid, _)| oid.clone())
        .collect();
    if wr.is_empty() {
        return Ok(());
    }
    let set = live_versions(repo, Some(tx), &tx.state, None)?;
    let reg = ubos_rules::Registry::new(&set.properties)?;
    let classes: &Registry = repo.reg;
    let mut order: Vec<&(String, String, String, Value)> = set.sheets.iter().collect();
    order.sort_by_key(|(id, ..)| !wr.contains(id));
    for (_, _, _, body) in order {
        let sheet = ubos_rules::Sheet::from_json(body.get("ast").unwrap_or(&Value::Null))?;
        ubos_rules::check_sheet(&sheet, classes, &reg)?;
    }
    ubos_rules::check_set(&reg, classes, &refs(&set), &wr)
}

/// True when a qualifier expression holds on an object; an evaluation failure (a type
/// error at run time, a null access) does not match (STD-LGS-020).
fn eval_bool(reg: &Registry, class: &str, body: &Value, expr: &str) -> Result<bool, BpaError> {
    let run = || -> Result<Val, BpaError> {
        let ast = ubos_expr::parse(&Value::String(expr.to_string()))?;
        let t = ubos_expr::class_type(reg, class)?;
        let mut env = Env::new(Some(t.clone()), None, None, None);
        ubos_expr::check(&ast, &mut env)?;
        let mut ev = Evaluator::new(&env);
        ev.this = Some(from_json(&t, body));
        ev.eval(&ast)
    };
    match run() {
        Ok(Val::Bool(true)) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) if matches!(e.code, "LOGIC.TYPE_ERROR" | "LOGIC.NULL_ACCESS") => Ok(false),
        Err(e) => Err(e),
    }
}

/// Qualifier evaluation over a state at a coordinate.
struct StateMatcher<'a, 'r> {
    reg: &'r Registry,
    st: &'r State,
    repo: &'r mut Repo<'a>,
    asof: String,
    authority: String,
}

impl Matcher for StateMatcher<'_, '_> {
    fn attr(&mut self, class: &str, body: &Value, expr: &str) -> Result<bool, BpaError> {
        eval_bool(self.reg, class, body, expr)
    }

    fn rel(&mut self, member: &str, body: &Value, expr: &str) -> Result<bool, BpaError> {
        let uris: Vec<String> = match body.get(member) {
            Some(Value::String(u)) => vec![u.clone()],
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };
        for uri in uris {
            let Some(tid) = resolve(self.st, &uri, &self.authority) else {
                continue;
            };
            let t = &self.st.objects[&tid];
            if self.repo.version(&t.head)?["tombstone"] == json!(true) {
                continue;
            }
            let Some(seg) = at(&t.timeline, &self.asof) else {
                continue;
            };
            let tbody = self.repo.version(&seg.version)?["body"].clone();
            if eval_bool(self.reg, &t.class, &tbody, expr)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// The cascade of an object: `(effective, bindings, evaluated_at, object URI, version)`.
type Cascade = (
    ubos_rules::Effective,
    Vec<String>,
    BTreeMap<String, String>,
    String,
    String,
    ubos_rules::Registry,
);

impl Engine {
    fn cascade_of(&mut self, input: &Value) -> Result<Cascade, BpaError> {
        let branch = input
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let oid = input
            .get("object_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let kind = self.branch_meta(&branch)?.kind;
        let (o, _, ev, st) = self.object_view(&branch, input, &oid)?;
        let asof = input
            .get("asof")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.default_valid_of(&o.class));
        let time = input.get("time").and_then(Value::as_u64);
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        if repo.version(&o.head)?["tombstone"] == json!(true) {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        }
        let st = match st {
            Some(s) => s,
            None => repo.state_at(&branch, time)?,
        };
        let seg = at(&o.timeline, &asof)
            .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
        let version = seg.version.clone();
        let body = repo.version(&version)?["body"].clone();
        let exts = extensions_of(&mut repo, &st)?;
        let reg_ext = self.registry.with_extensions(&exts)?;
        let mut repo = Repo::new(self.store.as_mut(), &reg_ext);
        let mut ctx: Map<String, Value> = input
            .get("ctx")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ctx.entry("branch_kind".to_string())
            .or_insert_with(|| Value::String(kind));
        let set = live_versions(&mut repo, None, &st, Some(&asof))?;
        let reg = ubos_rules::Registry::new(&set.properties)?;
        let lin = reg_ext.class(&o.class)?.linearization.clone();
        let authority = repo.store.authority();
        let uri = uri_of(&authority, &o.class, &o.slug);
        let mut m = StateMatcher {
            reg: &reg_ext,
            st: &st,
            repo: &mut repo,
            asof: asof.clone(),
            authority,
        };
        let (out, bindings) = ubos_rules::cascade(
            &reg,
            &reg_ext,
            &refs(&set),
            &o.class,
            &lin,
            &body,
            &ctx,
            &mut m,
        )?;
        Ok((out, bindings, ev, uri, version, reg))
    }

    /// `cascade` (STD-CONF-005): the effective declarations of an object in a context.
    pub fn cascade(&mut self, input: &Value) -> Result<Value, BpaError> {
        let (out, bindings, ev, _, _, _) = self.cascade_of(input)?;
        let properties: Map<String, Value> = out
            .iter()
            .map(|(p, (v, _))| (p.clone(), v.clone()))
            .collect();
        Ok(json!({"properties": properties, "bindings": bindings, "evaluated_at": ev}))
    }

    /// `explain_cascade`: `DAT-CascadeExplanation` of one property (STD-LGS-023).
    pub fn explain_cascade(&mut self, input: &Value) -> Result<Value, BpaError> {
        let property = input
            .get("property")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("property is text"))?
            .to_string();
        let (out, _, ev, uri, version, reg) = self.cascade_of(input)?;
        reg.split_path(&property)?;
        let (effective, candidates) = match out.get(&property) {
            Some((v, cs)) => (v.clone(), ubos_rules::explain(cs)),
            None => (Value::Null, Vec::new()),
        };
        Ok(
            json!({"object": uri, "version_hash": version, "property": property, "effective": effective,
            "candidates": candidates, "evaluated_at": ev}),
        )
    }
}
