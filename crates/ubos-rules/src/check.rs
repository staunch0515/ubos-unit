//! Authoring checks of a sheet against the classes and the registry (STD-LGS-001, -010,
//! -020), specificity (STD-LGS-021) and the static overlap analysis and rank of the
//! cascade (STD-LGS-022).

use crate::ast::{Qualifier, Selector, Sheet};
use crate::registry::{LAYERS, Registry};
use serde_json::{Value, json};
use ubos_expr::{Env, Ty};
use ubos_model::Registry as Classes;
use ubos_model::types::BpaError;
use ubos_model::types::error::invalid;

fn class_of<'a>(classes: &Classes, type_name: &'a str) -> Result<&'a str, BpaError> {
    if type_name == "*" {
        return Ok("Object");
    }
    classes.class(type_name)?;
    Ok(type_name)
}

/// A qualifier expression is a boolean over `this` of the class (`LOGIC.TYPE_ERROR`).
fn check_expr(classes: &Classes, cls: &str, text: &str, selector: &str) -> Result<(), BpaError> {
    let wrap = |e: BpaError| {
        if e.code == "LOGIC.TYPE_ERROR" {
            let message = e
                .details
                .as_ref()
                .and_then(|d| d.get("message"))
                .cloned()
                .unwrap_or_else(|| Value::String(e.message.clone()));
            BpaError::with_details(
                "LOGIC.TYPE_ERROR",
                json!({"selector": selector, "message": message}),
            )
        } else {
            e
        }
    };
    let ast = ubos_expr::parse(&Value::String(text.to_string())).map_err(wrap)?;
    let this = ubos_expr::class_type(classes, cls).map_err(wrap)?;
    let mut env = Env::new(Some(this), None, None, None);
    let t = ubos_expr::check(&ast, &mut env).map_err(wrap)?;
    if !matches!(t, Ty::Boolean | Ty::Any) {
        return Err(BpaError::with_details(
            "LOGIC.TYPE_ERROR",
            json!({"selector": selector, "message": "a qualifier is boolean"}),
        ));
    }
    Ok(())
}

/// STD-LGS-001, -010, -020: the selectors are checked against the classes and the
/// declarations against the property registry.
pub fn check_sheet(sheet: &Sheet, classes: &Classes, reg: &Registry) -> Result<(), BpaError> {
    for b in &sheet.blocks {
        for sel in &b.selectors {
            let cls = class_of(classes, &sel.type_name)?;
            let eff = classes.class(cls)?;
            let raw = eff.kind.as_deref() == Some("raw_document");
            let where_ = sel.text();
            for q in &sel.qualifiers {
                match q {
                    Qualifier::Attr(expr) => {
                        if raw {
                            return Err(BpaError::new(
                                "RULE.UNTYPED_ACCESS",
                                "selector on RawDocument content",
                            ));
                        }
                        check_expr(classes, cls, expr, &where_)?;
                    }
                    Qualifier::Rel { member, expr } => {
                        if raw {
                            return Err(BpaError::new(
                                "RULE.UNTYPED_ACCESS",
                                "selector on RawDocument content",
                            ));
                        }
                        let f = if cls == "Object" {
                            None
                        } else {
                            eff.fields.get(member)
                        };
                        let Some(f) = f else {
                            return Err(BpaError::new(
                                "MODEL.REFERENCE_INVALID",
                                format!("unknown member {member}"),
                            ));
                        };
                        let item = if f.get("type").and_then(Value::as_str) == Some("list") {
                            f.get("constraints")
                                .and_then(|c| c.get("item_type"))
                                .unwrap_or(f)
                        } else {
                            f
                        };
                        if item.get("type").and_then(Value::as_str) != Some("reference") {
                            return Err(invalid(":rel needs a reference member"));
                        }
                        let target = item
                            .get("constraints")
                            .and_then(|c| c.get("target_class"))
                            .and_then(Value::as_str)
                            .unwrap_or("Object");
                        classes.class(target)?;
                        check_expr(classes, target, expr, &where_)?;
                    }
                    _ => {}
                }
            }
        }
        for d in &b.declarations {
            reg.check_declaration(&d.property, &d.value.to_json())?;
        }
    }
    Ok(())
}

/// Position depth of a class in the hierarchy (STD-CLS-022): `Object` 0, one more per
/// parent step, a trait 1.
pub fn depth(classes: &Classes, name: &str) -> Result<usize, BpaError> {
    let decl = classes.decl(name)?;
    if decl.get("trait") == Some(&Value::Bool(true)) {
        return Ok(1);
    }
    let mut n = 0;
    let mut cur = decl;
    while let Some(p) = cur.get("parent").and_then(Value::as_str) {
        cur = classes.decl(p)?;
        n += 1;
        if n > 1000 {
            break;
        }
    }
    Ok(n)
}

/// STD-LGS-021: the specificity `(d, a, s, x)` of one selector.
pub fn specificity(classes: &Classes, sel: &Selector) -> Result<[usize; 4], BpaError> {
    let d = if sel.type_name == "*" {
        0
    } else {
        depth(classes, class_of(classes, &sel.type_name)?)?
    };
    let (mut a, mut s, mut x) = (0, 0, 0);
    for q in &sel.qualifiers {
        match q {
            Qualifier::Attr(_) | Qualifier::Rel { .. } => a += 1,
            Qualifier::State(_) | Qualifier::Tag(_) => s += 1,
            Qualifier::Ctx { .. } => x += 1,
        }
    }
    Ok([d, a, s, x])
}

fn is_trait(classes: &Classes, name: &str) -> bool {
    classes
        .decl(name)
        .ok()
        .is_some_and(|d| d.get("trait") == Some(&Value::Bool(true)))
}

/// The conservative overlap analysis of STD-LGS-022: two selectors may match the same
/// object unless their type selectors are disjoint, their `:state` qualifiers differ, or a
/// `:ctx` condition on the same key admits no common value.
pub fn may_overlap(classes: &Classes, a: &Selector, b: &Selector) -> Result<bool, BpaError> {
    let (ta, tb) = (a.type_name.as_str(), b.type_name.as_str());
    if ta != "*" && tb != "*" && ta != tb && !is_trait(classes, ta) && !is_trait(classes, tb) {
        let la = classes.linearize(ta)?;
        let lb = classes.linearize(tb)?;
        if !lb.iter().any(|x| x == ta) && !la.iter().any(|x| x == tb) {
            let mut common = false;
            for c in classes.classes() {
                if c.is_abstract || is_trait(classes, &c.name) {
                    continue;
                }
                if c.linearization.iter().any(|x| x == ta)
                    && c.linearization.iter().any(|x| x == tb)
                {
                    common = true;
                    break;
                }
            }
            if !common {
                return Ok(false);
            }
        }
    }
    fn states(s: &Selector) -> Vec<&str> {
        s.qualifiers
            .iter()
            .filter_map(|q| match q {
                Qualifier::State(st) => Some(st.as_str()),
                _ => None,
            })
            .collect()
    }
    let (sa, sb) = (states(a), states(b));
    if !sa.is_empty() && !sb.is_empty() {
        let same = sa.len() == sb.len()
            && sa.iter().all(|x| sb.contains(x))
            && sb.iter().all(|x| sa.contains(x));
        if !same {
            return Ok(false);
        }
    }
    let values = |v: &crate::ast::SheetValue| -> Vec<String> {
        match v {
            crate::ast::SheetValue::Text(t) => vec![t.clone()],
            crate::ast::SheetValue::List(items) => items
                .iter()
                .filter_map(|x| x.as_text().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    };
    for ka in &a.qualifiers {
        let Qualifier::Ctx { key, op, value } = ka else {
            continue;
        };
        if op == "!=" {
            continue;
        }
        for kb in &b.qualifiers {
            let Qualifier::Ctx {
                key: kb_key,
                op: kb_op,
                value: kb_value,
            } = kb
            else {
                continue;
            };
            if kb_op == "!=" || kb_key != key {
                continue;
            }
            let va = values(value);
            let vb = values(kb_value);
            if !va.iter().any(|x| vb.contains(x)) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// The rank key of a declaration: its lock flag, layer, specificity and sheet priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key<'a> {
    /// `!locked`.
    pub locked: bool,
    /// Layer of the sheet.
    pub layer: &'a str,
    /// Specificity of the matching selector.
    pub spec: [usize; 4],
    /// Priority of the sheet.
    pub priority: i64,
}

/// The rank of a declaration (STD-LGS-022): locked declarations first, among them the
/// lowest layer wins; among unlocked the highest layer wins; then specificity and priority.
pub fn rank(k: &Key<'_>) -> (u8, i64, [usize; 4], i64) {
    let li = LAYERS.iter().position(|l| *l == k.layer).unwrap_or(0) as i64;
    if k.locked {
        (1, -li, k.spec, k.priority)
    } else {
        (0, li, k.spec, k.priority)
    }
}

/// Why a losing candidate `c` lost to the winner `w` (STD-LGS-023).
pub fn reason(c: &Key<'_>, w: &Key<'_>) -> &'static str {
    if w.locked && !c.locked {
        "locked_elsewhere"
    } else if c.layer != w.layer {
        "lower_layer"
    } else if c.spec != w.spec {
        "less_specific"
    } else if c.priority != w.priority {
        "lower_priority"
    } else {
        "earlier_position"
    }
}
