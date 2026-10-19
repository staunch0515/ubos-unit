//! Governance sheets in the kernel (STD-LGS-002, -010, -020, -022, -023; DSN-DVM-701,
//! -703…706, -710): the sheet body derived at commit, the checks of the sheet set at
//! commit, and the cascade of an object at a coordinate over the sheets effective there,
//! compiled once per version hash and indexed per sheet set (`Engine::rules`).

use crate::relations::{extensions_of, resolve, uri_of};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use ubos_expr::{Env, Evaluator, Val, from_json};
use ubos_model::Registry;
use ubos_rules::{Cache, CompiledExpr, CompiledSheet, Matcher, SheetRef};
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

/// The live sheets and property declarations of a state at a coordinate: the head
/// versions in a transaction, or the versions at `asof`. Sheets come compiled from the
/// cache by version hash (DSN-DVM-701); property declarations are read (they are few).
pub(crate) struct SheetSet {
    pub refs: Vec<SheetRef>,
    pub properties: Vec<Value>,
}

impl SheetSet {
    fn compiled(&self) -> Vec<Arc<CompiledSheet>> {
        self.refs.iter().map(|r| r.sheet.clone()).collect()
    }

    fn versions(&self) -> Vec<String> {
        self.refs.iter().map(|r| r.version.clone()).collect()
    }
}

pub(crate) fn live_versions(
    repo: &mut Repo<'_>,
    tx: Option<&Tx>,
    st: &State,
    asof: Option<&str>,
    classes: &Registry,
    cache: &mut Cache,
) -> Result<SheetSet, BpaError> {
    let mut out = SheetSet {
        refs: Vec::new(),
        properties: Vec::new(),
    };
    let authority = repo.store.authority();
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
        if o.class == "PropertyDecl" {
            let v = version_of(repo, tx, &vh)?;
            if v["tombstone"] != json!(true) {
                out.properties.push(v["body"].clone());
            }
            continue;
        }
        // a tombstone is never cached: the version is read when the sheet is not compiled
        let compiled = match cache.sheet(&vh, classes, || {
            let v = version_of(repo, tx, &vh)?;
            if v["tombstone"] == json!(true) {
                return Err(BpaError::new("IAM.NOT_FOUND", "ended sheet"));
            }
            Ok(v["body"].clone())
        }) {
            Ok(c) => c,
            Err(e) if e.code == "IAM.NOT_FOUND" && e.message == "ended sheet" => continue,
            Err(e) => return Err(e),
        };
        out.refs.push(SheetRef {
            id: oid.clone(),
            uri: uri_of(&authority, &o.class, &o.slug),
            version: vh,
            sheet: compiled,
        });
    }
    Ok(out)
}

/// The checks of the sheets and property declarations written by a commit, over the live
/// sheets of the branch (STD-LGS-002, -010, -022).
pub fn check_sheets(
    repo: &mut Repo<'_>,
    tx: &Tx,
    written: &[(String, String)],
    cache: &mut Cache,
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
    let classes: &Registry = repo.reg;
    let set = live_versions(repo, Some(tx), &tx.state, None, classes, cache)?;
    let reg = ubos_rules::Registry::new(&set.properties)?;
    let mut order: Vec<&SheetRef> = set.refs.iter().collect();
    order.sort_by_key(|r| !wr.contains(&r.id));
    for r in order {
        ubos_rules::check_sheet(&r.sheet.sheet, classes, &reg)?;
    }
    ubos_rules::check_set(&reg, classes, &set.refs, &wr)
}

/// True when a compiled qualifier expression holds on an object; an evaluation failure
/// (a type error at run time, a null access) does not match (STD-LGS-020).
fn eval_compiled(expr: &CompiledExpr, body: &Value) -> Result<bool, BpaError> {
    let env = Env::new(Some(expr.this.clone()), None, None, None);
    let mut ev = Evaluator::new(&env);
    ev.this = Some(from_json(&expr.this, body));
    match ev.eval(&expr.node) {
        Ok(Val::Bool(true)) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) if matches!(e.code, "LOGIC.TYPE_ERROR" | "LOGIC.NULL_ACCESS") => Ok(false),
        Err(e) => Err(e),
    }
}

/// A version record, looking first at the versions of an open transaction.
pub(crate) fn version_of(
    repo: &mut Repo<'_>,
    tx: Option<&Tx>,
    vh: &str,
) -> Result<Value, BpaError> {
    match tx {
        Some(tx) => repo.version_in(tx, vh),
        None => repo.version(vh),
    }
}

/// Qualifier evaluation over a state at a coordinate. On the index path the state holds
/// only the sheets and the object evaluated; the whole state is materialised when a
/// `:rel` qualifier resolves a reference from the slug index at the coordinate (`lazy`).
struct StateMatcher<'a, 'r> {
    st: &'r State,
    repo: &'r mut Repo<'a>,
    tx: Option<&'r Tx>,
    asof: String,
    authority: String,
    lazy: Option<(String, Option<u64>)>,
}

impl StateMatcher<'_, '_> {
    fn resolve(
        &mut self,
        uri: &str,
    ) -> Result<Option<(String, ubos_version::ObjectState)>, BpaError> {
        if let Some(tid) = resolve(self.st, uri, &self.authority) {
            return Ok(Some((tid.clone(), self.st.objects[&tid].clone())));
        }
        let Some((branch, time)) = &self.lazy else {
            return Ok(None);
        };
        // the object at the URI from the slug index at the coordinate (PH-2 step 0b), not a
        // view of the whole branch
        let rest = uri.strip_prefix("ubos://").unwrap_or_default();
        let mut parts = rest.split('/');
        let (Some(auth), Some(class), Some(slug), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Ok(None);
        };
        if auth != self.authority {
            return Ok(None);
        }
        let cseq = self.repo.ordinal(branch, *time)?;
        self.repo.object_by_slug(branch, cseq, class, slug)
    }
}

impl Matcher for StateMatcher<'_, '_> {
    fn attr(&mut self, expr: &CompiledExpr, body: &Value) -> Result<bool, BpaError> {
        eval_compiled(expr, body)
    }

    fn rel(&mut self, member: &str, expr: &CompiledExpr, body: &Value) -> Result<bool, BpaError> {
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
            let Some((_, t)) = self.resolve(&uri)? else {
                continue;
            };
            if version_of(self.repo, self.tx, &t.head)?["tombstone"] == json!(true) {
                continue;
            }
            let Some(seg) = at(&t.timeline, &self.asof) else {
                continue;
            };
            let tbody = version_of(self.repo, self.tx, &seg.version)?["body"].clone();
            if eval_compiled(expr, &tbody)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// The cascade of an object body of `class` over a state at `asof` in a context: the
/// effective values with their candidates, the sheet versions bound and the property
/// registry of the coordinate (STD-LGS-020, -022). The object itself need not exist in
/// the state (the defaults of a create, STD-LGS-011 `field_default`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn cascade_state(
    repo: &mut Repo<'_>,
    tx: Option<&Tx>,
    st: &State,
    reg_ext: &Registry,
    cache: &mut Cache,
    class: &str,
    body: &Value,
    asof: &str,
    ctx: &Map<String, Value>,
    state: Option<&str>,
) -> Result<(ubos_rules::Effective, Vec<String>, ubos_rules::Registry), BpaError> {
    cascade_lazy(
        repo, tx, st, reg_ext, cache, class, body, asof, ctx, state, None,
    )
}

/// `cascade_state` over a partial state (the sheets and the object), the whole state of
/// the coordinate materialised only for a `:rel` qualifier.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cascade_lazy(
    repo: &mut Repo<'_>,
    tx: Option<&Tx>,
    st: &State,
    reg_ext: &Registry,
    cache: &mut Cache,
    class: &str,
    body: &Value,
    asof: &str,
    ctx: &Map<String, Value>,
    state: Option<&str>,
    lazy: Option<(String, Option<u64>)>,
) -> Result<(ubos_rules::Effective, Vec<String>, ubos_rules::Registry), BpaError> {
    // the `:state` qualifier matches the lifecycle state (STD-CLS-004) or a field `state`
    let with_state;
    let body = match state {
        Some(st) => {
            let mut b = body.clone();
            b["state"] = json!(st);
            with_state = b;
            &with_state
        }
        None => body,
    };
    let set = live_versions(repo, tx, st, Some(asof), reg_ext, cache)?;
    let reg = ubos_rules::Registry::new(&set.properties)?;
    let index = cache.index(&set.versions(), &set.compiled());
    let lin = reg_ext.class(class)?.linearization.clone();
    let authority = repo.store.authority();
    let mut m = StateMatcher {
        st,
        repo,
        tx,
        asof: asof.to_string(),
        authority,
        lazy,
    };
    let (out, bindings) = ubos_rules::cascade(&reg, &set.refs, &index, &lin, body, ctx, &mut m)?;
    Ok((out, bindings, reg))
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
        let (o, _, ev, view) = self.object_view(&branch, input, &oid)?;
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
        // the view path resolves the whole state; the index path reads the sheets, the
        // property declarations and the extensions by class (DSN-DVM-703) and materialises
        // the rest only for a `:rel` qualifier
        let (st, lazy) = match view {
            Some(s) => (s, None),
            None => {
                let cseq = repo.ordinal(&branch, time)?;
                let mut st = State::default();
                for c in SHEET_CLASSES.iter().chain(["ClassExtension"].iter()) {
                    st.objects.extend(repo.objects_in(&branch, cseq, Some(c))?);
                }
                st.objects.insert(oid.clone(), o.clone());
                (st, Some((branch.clone(), time)))
            }
        };
        let seg = at(&o.timeline, &asof)
            .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
        let version = seg.version.clone();
        let v = repo.version(&version)?;
        let body = v["body"].clone();
        let state = crate::flow::state_of(&v);
        let exts = extensions_of(&mut repo, &st)?;
        let reg_ext: Cow<Registry> = if exts.is_empty() {
            Cow::Borrowed(&self.registry)
        } else {
            Cow::Owned(self.registry.with_extensions(&exts)?)
        };
        let mut repo = Repo::new(self.store.as_mut(), &reg_ext);
        let mut ctx: Map<String, Value> = input
            .get("ctx")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ctx.entry("branch_kind".to_string())
            .or_insert_with(|| Value::String(kind));
        let uri = uri_of(&repo.store.authority(), &o.class, &o.slug);
        let (out, bindings, reg) = cascade_lazy(
            &mut repo,
            None,
            &st,
            &reg_ext,
            &mut self.rules,
            &o.class,
            &body,
            &asof,
            &ctx,
            state.as_deref(),
            lazy,
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
