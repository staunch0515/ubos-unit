//! Compiled sheets and the selector index (DSN-DVM-701, -703): a sheet is compiled once per
//! version hash — its AST, the specificity of every selector and the type-checked
//! qualifier expressions — and the blocks of a sheet set are bucketed by the class of
//! their type selectors, so that the cascade of an object visits only the buckets of its
//! linearization.

use crate::ast::{Qualifier, Selector, Sheet};
use crate::check::{compile_expr, specificity};
use crate::registry::Registry;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use ubos_expr::{Node, Ty};
use ubos_model::Registry as Classes;
use ubos_model::types::BpaError;

/// A type-checked L1 boolean over `this` of a class.
#[derive(Debug, Clone)]
pub struct CompiledExpr {
    /// The expression text.
    pub text: String,
    /// The parsed expression.
    pub node: Node,
    /// The class the expression was checked against.
    pub class: String,
    /// The static type of `this`.
    pub this: Ty,
}

/// A selector with its specificity and its compiled qualifier expressions (aligned with
/// the qualifiers; `None` for a qualifier without an expression).
#[derive(Debug, Clone)]
pub struct CompiledSelector {
    /// The selector.
    pub selector: Selector,
    /// Its text.
    pub text: String,
    /// `(d, a, s, x)` (STD-LGS-021).
    pub specificity: [usize; 4],
    /// The compiled `[expr]` and `:rel(m)[expr]` qualifiers.
    pub exprs: Vec<Option<Arc<CompiledExpr>>>,
}

/// A rule block with its compiled selectors.
#[derive(Debug, Clone)]
pub struct CompiledBlock {
    /// The selectors in order.
    pub selectors: Vec<CompiledSelector>,
}

/// The `(path, value)` pairs of every declaration of a sheet, per block and declaration.
pub type Normalised = Vec<Vec<Vec<(String, Value)>>>;

/// A sheet version compiled against a class registry.
#[derive(Debug)]
pub struct CompiledSheet {
    /// The AST.
    pub sheet: Sheet,
    /// `DAT-Sheet.layer`.
    pub layer: String,
    /// `DAT-Sheet.priority` (default 0).
    pub priority: i64,
    /// `DAT-Sheet.scope`, when given.
    pub scope: Option<Value>,
    /// The blocks, aligned with `sheet.blocks`.
    pub blocks: Vec<CompiledBlock>,
    /// The declarations normalised against a property registry (by its fingerprint).
    normalised: Mutex<Option<(u64, Arc<Normalised>)>>,
}

impl CompiledSheet {
    /// The normalised declarations of the sheet under a property registry (STD-LGS-010:
    /// values validated, whole-map declarations split per key), computed once per registry.
    pub fn normalised(&self, reg: &Registry) -> Result<Arc<Normalised>, BpaError> {
        let mut guard = self.normalised.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((fp, n)) = &*guard
            && *fp == reg.fingerprint()
        {
            return Ok(n.clone());
        }
        let mut out = Vec::with_capacity(self.sheet.blocks.len());
        for b in &self.sheet.blocks {
            let mut decls = Vec::with_capacity(b.declarations.len());
            for d in &b.declarations {
                decls.push(reg.normalise(d)?);
            }
            out.push(decls);
        }
        let n = Arc::new(out);
        *guard = Some((reg.fingerprint(), n.clone()));
        Ok(n)
    }

    /// Compile a sheet body (`DAT-Sheet` with its `ast`) against the classes; unknown
    /// classes and ill-typed qualifiers are the errors of `check_sheet`.
    pub fn compile(body: &Value, classes: &Classes) -> Result<CompiledSheet, BpaError> {
        let sheet = Sheet::from_json(body.get("ast").unwrap_or(&Value::Null))?;
        let mut blocks = Vec::with_capacity(sheet.blocks.len());
        for b in &sheet.blocks {
            let mut selectors = Vec::with_capacity(b.selectors.len());
            for sel in &b.selectors {
                let cls = if sel.type_name == "*" {
                    "Object"
                } else {
                    sel.type_name.as_str()
                };
                let mut exprs = Vec::with_capacity(sel.qualifiers.len());
                for q in &sel.qualifiers {
                    exprs.push(match q {
                        Qualifier::Attr(e) => Some(Arc::new(compile_expr(classes, cls, e)?)),
                        Qualifier::Rel { member, expr } => {
                            let target = crate::check::rel_target(classes, cls, member)?;
                            Some(Arc::new(compile_expr(classes, &target, expr)?))
                        }
                        _ => None,
                    });
                }
                selectors.push(CompiledSelector {
                    selector: sel.clone(),
                    text: sel.text(),
                    specificity: specificity(classes, sel)?,
                    exprs,
                });
            }
            blocks.push(CompiledBlock { selectors });
        }
        Ok(CompiledSheet {
            sheet,
            layer: body
                .get("layer")
                .and_then(Value::as_str)
                .unwrap_or(crate::registry::LAYERS[0])
                .to_string(),
            priority: body.get("priority").and_then(Value::as_i64).unwrap_or(0),
            scope: body.get("scope").filter(|v| v.is_object()).cloned(),
            blocks,
            normalised: Mutex::new(None),
        })
    }
}

/// The selector index of a sheet set (DSN-DVM-703): `(sheet, block)` pairs bucketed by
/// the class of a type selector of the block, and the blocks with a `*` selector.
#[derive(Debug, Default)]
pub struct Index {
    /// Blocks by the class of their type selectors.
    pub by_class: HashMap<String, Vec<(usize, usize)>>,
    /// Blocks with a universal selector.
    pub star: Vec<(usize, usize)>,
}

impl Index {
    /// Build the index over the compiled sheets in order.
    pub fn build(sheets: &[Arc<CompiledSheet>]) -> Index {
        let mut ix = Index::default();
        for (si, sh) in sheets.iter().enumerate() {
            for (bi, b) in sh.blocks.iter().enumerate() {
                let mut seen: Vec<&str> = Vec::new();
                for sel in &b.selectors {
                    let t = sel.selector.type_name.as_str();
                    if seen.contains(&t) {
                        continue;
                    }
                    seen.push(t);
                    if t == "*" {
                        ix.star.push((si, bi));
                    } else {
                        ix.by_class.entry(t.to_string()).or_default().push((si, bi));
                    }
                }
            }
        }
        ix
    }

    /// The blocks that can match an object of the linearization, in sheet and block order.
    pub fn candidates(&self, lin: &[String]) -> Vec<(usize, usize)> {
        let mut out: BTreeMap<(usize, usize), ()> = BTreeMap::new();
        for c in lin {
            if let Some(v) = self.by_class.get(c) {
                out.extend(v.iter().map(|p| (*p, ())));
            }
        }
        out.extend(self.star.iter().map(|p| (*p, ())));
        out.into_keys().collect()
    }
}

/// Compiled sheets by version hash and the index of the last sheet set evaluated
/// (DSN-DVM-701: cached by sheet version hash; DSN-DVM-703: per sheet set).
#[derive(Debug, Default)]
pub struct Cache {
    sheets: HashMap<String, Arc<CompiledSheet>>,
    index: Option<(Vec<String>, Arc<Index>)>,
}

impl Cache {
    /// The compiled sheet of a version, compiling it from the body `fetch` returns when it
    /// is not cached.
    pub fn sheet(
        &mut self,
        version: &str,
        classes: &Classes,
        fetch: impl FnOnce() -> Result<Value, BpaError>,
    ) -> Result<Arc<CompiledSheet>, BpaError> {
        if let Some(c) = self.sheets.get(version) {
            return Ok(c.clone());
        }
        let c = Arc::new(CompiledSheet::compile(&fetch()?, classes)?);
        self.sheets.insert(version.to_string(), c.clone());
        Ok(c)
    }

    /// The index of a sheet set given by its version hashes in order.
    pub fn index(&mut self, versions: &[String], sheets: &[Arc<CompiledSheet>]) -> Arc<Index> {
        if let Some((v, ix)) = &self.index
            && v == versions
        {
            return ix.clone();
        }
        let ix = Arc::new(Index::build(sheets));
        self.index = Some((versions.to_vec(), ix.clone()));
        ix
    }

    /// Forget everything (the class registry changed).
    pub fn clear(&mut self) {
        self.sheets.clear();
        self.index = None;
    }

    /// Number of compiled sheets held.
    pub fn len(&self) -> usize {
        self.sheets.len()
    }

    /// True when nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }
}
