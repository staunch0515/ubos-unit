//! Governance sheets of the Governance profile (BPA 1.0-rc2, STD-LGS-001…023; PH-1 step
//! 11, DSN-DVM-701…706): the textual grammar and its lossless AST, the property registry
//! with the kernel properties of ABI 1.0, authoring checks, specificity, the rank and the
//! overlap analysis of the cascade. The engine is pure: the sheet objects, matching over a
//! state and the cascade of an object are the kernel's (`ubos-dvm`).

pub mod ast;
pub mod check;
pub mod parse;
pub mod registry;

pub use ast::{Block, Declaration, Qualifier, Selector, Sheet, SheetValue};
pub use check::{Key, check_sheet, depth, may_overlap, rank, reason, specificity};
pub use parse::{parse, parse_selector, parse_sheet};
pub use registry::{LAYERS, Property, Registry, kernel_properties};

use serde_json::{Value, json};
use ubos_model::types::BpaError;

/// The adapter operations of STD-CONF-005 (since 1.0-rc2): `parse_sheet`, `check_sheet`
/// and `specificity`.
pub mod ops {
    use super::*;

    fn classes(i: &Value) -> Result<ubos_model::Registry, BpaError> {
        let list: Vec<Value> = i
            .get("classes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        ubos_model::registry_of(&list, &[])
    }

    /// `{source}` → `{ast, source}` (STD-LGS-001, -002).
    pub fn parse_sheet(i: &Value) -> Result<Value, BpaError> {
        super::parse_sheet(i.get("source").unwrap_or(&Value::Null))
    }

    /// `{source | ast, classes, properties?}` → `true` (STD-LGS-010, -011, -020).
    pub fn check_sheet(i: &Value) -> Result<Value, BpaError> {
        let sheet = match i.get("source") {
            Some(src) => parse(src)?,
            None => Sheet::from_json(i.get("ast").unwrap_or(&Value::Null))?,
        };
        let props: Vec<Value> = i
            .get("properties")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let reg = Registry::new(&props)?;
        super::check_sheet(&sheet, &classes(i)?, &reg)?;
        Ok(Value::Bool(true))
    }

    /// `{classes, selector}` → `{specificity}` (STD-LGS-021).
    pub fn specificity(i: &Value) -> Result<Value, BpaError> {
        let sel = parse_selector(i.get("selector").unwrap_or(&Value::Null))?;
        Ok(json!({"specificity": super::specificity(&classes(i)?, &sel)?}))
    }
}
