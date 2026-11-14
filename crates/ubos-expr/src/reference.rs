//! Stored reference data for the calendar and money functions (DSN-DVM-410, DSN-DVM-447).
//!
//! STD-EXPR-005 names a calendar and a rate object by text. An evaluation that is given
//! `calendars` and `rates` (the adapter operation `eval_expr`) finds them there. Inside
//! the kernel the text is the canonical URI of a stored calendar or rate object, and the
//! kernel supplies the objects valid at the coordinates of the evaluation through
//! [`with_reference`]: a name the evaluator does not hold is looked up in the reference
//! data in scope.

use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use ubos_model::types::BpaError;

/// Calendars and rate objects by name at the coordinates of an evaluation.
pub trait ReferenceData {
    /// The L1 calendar value `{weekend, holidays}` named `name`, or `None`.
    fn calendar(&self, name: &str) -> Result<Option<Value>, BpaError>;
    /// The rate object `{from, to, rate}` named `rate_ref`, or `None`.
    fn rate(&self, rate_ref: &str) -> Result<Option<Value>, BpaError>;
}

thread_local! {
    static SCOPE: RefCell<Vec<Rc<dyn ReferenceData>>> = const { RefCell::new(Vec::new()) };
}

struct Leave;

impl Drop for Leave {
    fn drop(&mut self) {
        SCOPE.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

/// Run `f` with `data` as the reference data of the evaluations it makes on this thread;
/// scopes nest, and the innermost one is used.
pub fn with_reference<T>(data: Rc<dyn ReferenceData>, f: impl FnOnce() -> T) -> T {
    SCOPE.with(|s| s.borrow_mut().push(data));
    let _leave = Leave;
    f()
}

/// The reference data in scope, if any.
pub(crate) fn in_scope() -> Option<Rc<dyn ReferenceData>> {
    SCOPE.with(|s| s.borrow().last().cloned())
}

/// True when the text of an expression (or of anything that embeds expressions) may call
/// a function that names a calendar or a rate object; callers use it to decide whether
/// reference data is worth resolving. It never misses a call and may report one that is
/// not there.
pub fn mentions_reference(text: &str) -> bool {
    ["business_day", "cutoff", "convert"]
        .iter()
        .any(|f| text.contains(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Env, Evaluator, Val, check, parse};
    use serde_json::json;

    struct Fixed;

    impl ReferenceData for Fixed {
        fn calendar(&self, name: &str) -> Result<Option<Value>, BpaError> {
            Ok((name == "stored").then(|| json!({"weekend": [6, 7], "holidays": ["2026-11-26"]})))
        }

        fn rate(&self, rate_ref: &str) -> Result<Option<Value>, BpaError> {
            Ok((rate_ref == "stored").then(|| json!({"from": "EUR", "to": "USD", "rate": "1.10"})))
        }
    }

    fn eval(text: &str, given: bool) -> Result<Val, BpaError> {
        let ast = parse(&json!(text))?;
        let mut env = Env::new(None, None, None, None);
        check(&ast, &mut env)?;
        let mut ev = Evaluator::new(&env);
        if given {
            ev.calendars
                .insert("stored".into(), json!({"weekend": [], "holidays": []}));
        }
        ev.eval(&ast)
    }

    #[test]
    fn names_resolve_in_the_scope_only() {
        let expr = r#"add_business_days(date(2026, 11, 25), 2, "stored")"#;
        let conv = r#"convert(10.00EUR, "USD", "stored")"#;
        assert_eq!(
            eval(expr, false).unwrap_err().code,
            "MODEL.REFERENCE_INVALID"
        );
        assert_eq!(
            eval(conv, false).unwrap_err().code,
            "MODEL.REFERENCE_INVALID"
        );
        let date = |s: &str| Val::Date(ubos_model::types::time::parse_date(s).unwrap());
        with_reference(Rc::new(Fixed), || {
            assert_eq!(eval(expr, false).unwrap(), date("2026-11-30"));
            assert!(matches!(eval(conv, false).unwrap(), Val::Money(_, c) if c == "USD"));
            // a calendar given to the evaluation comes first
            assert_eq!(eval(expr, true).unwrap(), date("2026-11-27"));
            assert_eq!(
                eval(r#"is_business_day(date(2026, 11, 25), "other")"#, false)
                    .unwrap_err()
                    .code,
                "MODEL.REFERENCE_INVALID"
            );
        });
        // the scope ends with the call
        assert_eq!(
            eval(expr, false).unwrap_err().code,
            "MODEL.REFERENCE_INVALID"
        );
        assert!(mentions_reference(expr) && mentions_reference(conv));
        assert!(mentions_reference("cutoff(d, t, z, c)") && !mentions_reference("this.a + 1"));
    }
}
