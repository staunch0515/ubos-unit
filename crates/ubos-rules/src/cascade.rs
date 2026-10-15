//! The cascade over a set of compiled sheets (STD-LGS-020, -022, -023) and the static
//! checks of a sheet set at commit (STD-LGS-022). Matching needs the state (qualifier
//! expressions over the object, one reference hop): the caller supplies it through
//! [`Matcher`].

use crate::ast::{Qualifier, SheetValue};
use crate::check::{Key, Overlap, qualifiers_may_overlap, rank, reason};
use crate::compiled::{CompiledExpr, CompiledSelector, CompiledSheet, Index};
use crate::registry::{LAYERS, Registry};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use ubos_model::Registry as Classes;
use ubos_model::types::{BpaError, canonical};

/// A live sheet object: its identifier, canonical URI, version hash and compiled form.
#[derive(Debug, Clone)]
pub struct SheetRef {
    /// Object identifier.
    pub id: String,
    /// Canonical URI of the sheet object.
    pub uri: String,
    /// Version hash of the sheet version in force.
    pub version: String,
    /// The compiled sheet.
    pub sheet: Arc<CompiledSheet>,
}

/// A declaration that matched an object (a row of `DAT-CascadeExplanation.candidates`).
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Sheet URI.
    pub sheet: String,
    /// Sheet version hash.
    pub sheet_version: String,
    /// Block index in the sheet.
    pub block: usize,
    /// Text of the matching selector.
    pub selector: String,
    /// Layer of the sheet.
    pub layer: String,
    /// Specificity of the selector.
    pub specificity: [usize; 4],
    /// Priority of the sheet.
    pub priority: i64,
    /// `!locked`.
    pub locked: bool,
    /// Declared value (one key of a map property, when the path is keyed).
    pub value: Value,
    /// Position `(sheet id, block, declaration)`; later wins among equal ranks.
    pub position: (String, usize, usize),
    /// Whether the candidate won or was combined.
    pub won: bool,
    /// Reason of STD-LGS-023.
    pub reason: &'static str,
}

impl Candidate {
    fn key(&self) -> Key<'_> {
        Key {
            locked: self.locked,
            layer: &self.layer,
            spec: self.specificity,
            priority: self.priority,
        }
    }

    /// The JSON row of the explanation.
    pub fn to_json(&self) -> Value {
        json!({
            "sheet": self.sheet, "sheet_version": self.sheet_version, "block": self.block,
            "selector": self.selector, "layer": self.layer, "specificity": self.specificity,
            "priority": self.priority, "locked": self.locked, "value": self.value,
            "won": self.won, "reason": self.reason,
        })
    }
}

/// Every `(path, selector, candidate)` a sheet declares (the static analysis takes every
/// selector of a block).
fn candidates_of(
    reg: &Registry,
    sheet: &SheetRef,
) -> Result<Vec<(String, usize, usize, Candidate)>, BpaError> {
    let sh = &sheet.sheet;
    let norm = sh.normalised(reg)?;
    let mut out = Vec::new();
    for (bi, b) in sh.sheet.blocks.iter().enumerate() {
        for (di, d) in b.declarations.iter().enumerate() {
            for (path, value) in &norm[bi][di] {
                for (si, sel) in sh.blocks[bi].selectors.iter().enumerate() {
                    out.push((
                        path.clone(),
                        bi,
                        si,
                        Candidate {
                            sheet: sheet.id.clone(),
                            sheet_version: sheet.version.clone(),
                            block: bi,
                            selector: sel.text.clone(),
                            layer: sh.layer.clone(),
                            specificity: sel.specificity,
                            priority: sh.priority,
                            locked: d.locked,
                            value: value.clone(),
                            position: (sheet.id.clone(), bi, di),
                            won: false,
                            reason: "won",
                        },
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// STD-LGS-022 at commit: over the live sheets, when the sheets `written` are committed,
/// a locked declaration overlapped by a higher-layer declaration of the same path is
/// `RULE.LOCKED_OVERRIDE`, and two candidates of equal rank from different sheets with
/// different values of an `override` property are `RULE.AMBIGUOUS_CASCADE`.
pub fn check_set(
    reg: &Registry,
    classes: &Classes,
    sheets: &[SheetRef],
    written: &BTreeSet<String>,
) -> Result<(), BpaError> {
    let mut by_path: BTreeMap<String, Vec<(usize, usize, usize, Candidate)>> = BTreeMap::new();
    for (i, sh) in sheets.iter().enumerate() {
        for (path, bi, si, c) in candidates_of(reg, sh)? {
            by_path.entry(path).or_default().push((i, bi, si, c));
        }
    }
    let sel = |i: usize, bi: usize, si: usize| &sheets[i].sheet.blocks[bi].selectors[si].selector;
    // candidates are compared per pair of type-selector classes that may overlap, so the
    // analysis scales with the candidates of a class chain, not of the whole sheet set
    let mut types = Overlap::default();
    for (path, cs) in &by_path {
        let combine = reg.combine_of(path)?;
        for (k, (ia, ba, sa, a)) in cs.iter().enumerate() {
            for (ib, bb, sb, b) in &cs[k + 1..] {
                if a.sheet == b.sheet
                    || (!written.contains(&a.sheet) && !written.contains(&b.sheet))
                {
                    continue;
                }
                let (x, y) = (sel(*ia, *ba, *sa), sel(*ib, *bb, *sb));
                if !types.types(classes, &x.type_name, &y.type_name)?
                    || !qualifiers_may_overlap(x, y)?
                {
                    continue;
                }
                for (lo, hi) in [(a, b), (b, a)] {
                    let li = |l: &str| LAYERS.iter().position(|x| *x == l).unwrap_or(0);
                    if lo.locked && li(&hi.layer) > li(&lo.layer) {
                        return Err(BpaError::with_details(
                            "RULE.LOCKED_OVERRIDE",
                            json!({"property": path, "locked_by": lo.sheet, "sheet": hi.sheet}),
                        ));
                    }
                }
                if rank(&a.key()) == rank(&b.key()) && a.value != b.value && combine == "override" {
                    let mut ids = vec![a.sheet.clone(), b.sheet.clone()];
                    ids.sort();
                    return Err(BpaError::with_details(
                        "RULE.AMBIGUOUS_CASCADE",
                        json!({"property": path, "sheets": ids}),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// What matching needs from the state: compiled qualifier expressions over an object and
/// over the objects one reference hop away (STD-LGS-020). An expression that fails to
/// evaluate does not match.
pub trait Matcher {
    /// `[expr]` on the object `body`.
    fn attr(&mut self, expr: &CompiledExpr, body: &Value) -> Result<bool, BpaError>;
    /// `:rel(member)[expr]`: true when any referenced object (resolved at the same
    /// coordinate) satisfies `expr`.
    fn rel(&mut self, member: &str, expr: &CompiledExpr, body: &Value) -> Result<bool, BpaError>;
}

/// The scope of a sheet includes the context and the object's linearization (STD-LGS-020).
pub fn sheet_applies(scope: Option<&Value>, ctx: &Map<String, Value>, lin: &[String]) -> bool {
    let Some(scope) = scope.and_then(Value::as_object) else {
        return true;
    };
    let list = |k: &str| -> Option<Vec<&str>> {
        scope
            .get(k)
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
            .map(|a| a.iter().filter_map(Value::as_str).collect())
    };
    if let Some(vaes) = list("vaes")
        && !ctx
            .get("vae")
            .and_then(Value::as_str)
            .is_some_and(|v| vaes.contains(&v))
    {
        return false;
    }
    if let Some(kinds) = list("branch_kinds")
        && !ctx
            .get("branch_kind")
            .and_then(Value::as_str)
            .is_some_and(|v| kinds.contains(&v))
    {
        return false;
    }
    if let Some(classes) = list("classes")
        && !lin.iter().any(|c| classes.contains(&c.as_str()))
    {
        return false;
    }
    true
}

/// A compiled selector matches an object in a context (STD-LGS-020).
pub fn matches(
    sel: &CompiledSelector,
    lin: &[String],
    body: &Value,
    ctx: &Map<String, Value>,
    m: &mut dyn Matcher,
) -> Result<bool, BpaError> {
    let s = &sel.selector;
    if s.type_name != "*" && !lin.contains(&s.type_name) {
        return Ok(false);
    }
    for (q, e) in s.qualifiers.iter().zip(&sel.exprs) {
        let ok = match q {
            Qualifier::Attr(_) => m.attr(e.as_ref().expect("compiled"), body)?,
            Qualifier::State(st) => body.get("state").and_then(Value::as_str) == Some(st),
            Qualifier::Tag(tag) => body
                .get("tags")
                .and_then(Value::as_array)
                .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(tag))),
            Qualifier::Ctx { key, op, value } => {
                let v = ctx.get(key);
                match op.as_str() {
                    "=" => v == Some(&value.to_json()),
                    "!=" => v != Some(&value.to_json()),
                    _ => match value {
                        SheetValue::List(items) => {
                            v.is_some_and(|v| items.iter().any(|x| x.to_json() == *v))
                        }
                        _ => false,
                    },
                }
            }
            Qualifier::Rel { member, .. } => m.rel(member, e.as_ref().expect("compiled"), body)?,
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The effective value and the candidates of every property path.
pub type Effective = BTreeMap<String, (Value, Vec<Candidate>)>;

/// The cascade of an object over the sheets effective at its coordinate (STD-LGS-022),
/// visiting only the blocks the index lists for its linearization (DSN-DVM-703):
/// `{path: (effective value, candidates)}` and the versions of the sheets whose scope
/// includes the context (the bindings).
#[allow(clippy::too_many_arguments)]
pub fn cascade(
    reg: &Registry,
    sheets: &[SheetRef],
    index: &Index,
    lin: &[String],
    body: &Value,
    ctx: &Map<String, Value>,
    m: &mut dyn Matcher,
) -> Result<(Effective, Vec<String>), BpaError> {
    let applies: Vec<bool> = sheets
        .iter()
        .map(|sh| sheet_applies(sh.sheet.scope.as_ref(), ctx, lin))
        .collect();
    let bindings: Vec<String> = sheets
        .iter()
        .zip(&applies)
        .filter(|(_, a)| **a)
        .map(|(sh, _)| sh.version.clone())
        .collect();
    let mut cands: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    for (si, bi) in index.candidates(lin) {
        if !applies[si] {
            continue;
        }
        let sh = &sheets[si];
        let block = &sh.sheet.blocks[bi];
        let mut best: Option<&CompiledSelector> = None;
        for sel in &block.selectors {
            if matches(sel, lin, body, ctx, m)?
                && best.is_none_or(|b| sel.specificity > b.specificity)
            {
                best = Some(sel);
            }
        }
        let Some(sel) = best else { continue };
        let norm = sh.sheet.normalised(reg)?;
        for (di, d) in sh.sheet.sheet.blocks[bi].declarations.iter().enumerate() {
            for (path, value) in &norm[bi][di] {
                cands.entry(path.clone()).or_default().push(Candidate {
                    sheet: sh.uri.clone(),
                    sheet_version: sh.version.clone(),
                    block: bi,
                    selector: sel.text.clone(),
                    layer: sh.sheet.layer.clone(),
                    specificity: sel.specificity,
                    priority: sh.sheet.priority,
                    locked: d.locked,
                    value: value.clone(),
                    position: (sh.id.clone(), bi, di),
                    won: false,
                    reason: "won",
                });
            }
        }
    }
    let mut out = BTreeMap::new();
    for (path, mut cs) in cands {
        let rule = reg.combine_of(&path)?.to_string();
        let top_rank = cs.iter().map(|c| rank(&c.key())).max().unwrap_or_default();
        let top: Vec<usize> = (0..cs.len())
            .filter(|i| rank(&cs[*i].key()) == top_rank)
            .collect();
        let value = if rule != "override" {
            // A combinable property combines every matching candidate: the rules are
            // commutative, so the rank only orders the explanation (DEC-066 item 8).
            let mut order: Vec<usize> = (0..cs.len()).collect();
            order.sort_by(|a, b| cs[*a].position.cmp(&cs[*b].position));
            let values: Vec<Value> = order.iter().map(|i| cs[*i].value.clone()).collect();
            for c in &mut cs {
                c.won = true;
                c.reason = "combined";
            }
            reg.combine(&path, &values)?
        } else {
            let sheets: BTreeSet<&str> = top.iter().map(|i| cs[*i].sheet.as_str()).collect();
            let values: BTreeSet<String> = top
                .iter()
                .map(|i| canonical(&cs[*i].value))
                .collect::<Result<_, _>>()?;
            if sheets.len() > 1 && values.len() > 1 {
                return Err(BpaError::with_details(
                    "RULE.AMBIGUOUS_CASCADE",
                    json!({"property": path, "sheets": sheets.iter().collect::<Vec<_>>()}),
                ));
            }
            let w = *top
                .iter()
                .max_by(|a, b| cs[**a].position.cmp(&cs[**b].position))
                .expect("a candidate");
            let winner = cs[w].clone();
            for (i, c) in cs.iter_mut().enumerate() {
                if i == w {
                    c.won = true;
                    c.reason = "won";
                } else if top.contains(&i) {
                    c.won = false;
                    c.reason = if c.sheet != winner.sheet {
                        "same_value"
                    } else {
                        "earlier_position"
                    };
                } else {
                    c.won = false;
                    c.reason = reason(&c.key(), &winner.key());
                }
            }
            winner.value
        };
        out.insert(path, (value, cs));
    }
    Ok((out, bindings))
}

/// The candidates of one property in rank order, as `DAT-CascadeExplanation.candidates`.
pub fn explain(cands: &[Candidate]) -> Vec<Value> {
    let mut order: Vec<&Candidate> = cands.iter().collect();
    order.sort_by(|a, b| (rank(&b.key()), &b.position).cmp(&(rank(&a.key()), &a.position)));
    order.iter().map(|c| c.to_json()).collect()
}
