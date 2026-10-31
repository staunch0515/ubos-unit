//! Actions that ask for input (PH-2 step 6f-1, DEC-124): the instruction `ask.input` of BPA
//! 1.0-rc12 (STD-ISA-101, DSN-DVM-735).
//!
//! A process that asks for input it was not given ends with the outcome `needs_input`,
//! committing nothing; the caller repeats the invocation with `answers` before the request
//! expires. The requests are kept in `kernel_meta` (`input_request:<id>`), keyed by the
//! content ID of the invocation and the request name, so a repeated call finds its request.
//! The answers are the instruction's journal record, so a replay needs no request.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use ubos_types::{BpaError, content_id};

/// The internal code of a process that ends with `needs_input`; its details are the request.
pub(crate) const NEEDS_INPUT: &str = "ADAPTER.NEEDS_INPUT";

thread_local! {
    /// The invocations being run (innermost last) and the requests each has answered.
    static RUNS: RefCell<Vec<(Value, Vec<String>)>> = const { RefCell::new(Vec::new()) };
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(what: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", what.to_string())
}

/// An invocation being run; leaving it (`take`, or dropping it on any other exit) ends it.
pub(crate) struct Dialog {
    open: bool,
}

/// Enter an invocation.
pub(crate) fn enter(i: &Value) -> Dialog {
    RUNS.with(|r| r.borrow_mut().push((i.clone(), Vec::new())));
    Dialog { open: true }
}

impl Dialog {
    /// Leave the invocation; the requests it answered.
    pub(crate) fn take(&mut self) -> Vec<String> {
        if !self.open {
            return Vec::new();
        }
        self.open = false;
        RUNS.with(|r| r.borrow_mut().pop().map(|x| x.1).unwrap_or_default())
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        self.take();
    }
}

impl Engine {
    /// STD-ISA-101: the answers of a request, or the request when the invocation has none.
    pub(crate) fn input_answers(&mut self, a: &Map<String, Value>) -> Result<Value, BpaError> {
        let fields = a.get("fields").and_then(Value::as_array).cloned();
        let ok = fields.as_ref().is_some_and(|f| {
            let mut names: Vec<&str> = f.iter().filter_map(|x| s(x, "name")).collect();
            let n = names.len();
            names.sort();
            names.dedup();
            !f.is_empty()
                && n == f.len()
                && names.len() == n
                && f.iter().all(|x| x["type"].is_string())
        });
        let Some(fields) = fields.filter(|_| ok) else {
            return Err(invalid(
                "fields are {name, type, required, description} items",
            ));
        };
        let (Some(request), Some(message)) = (
            a.get("request").and_then(Value::as_str),
            a.get("message").and_then(Value::as_str),
        ) else {
            return Err(invalid("request and message are text"));
        };
        let timeout = a
            .get("timeout")
            .and_then(Value::as_str)
            .unwrap_or("PT15M")
            .to_string();
        let (inv, answers) = RUNS.with(|r| {
            r.borrow()
                .last()
                .map(|(i, _)| {
                    let mut inv = i.as_object().cloned().unwrap_or_default();
                    for k in ["process", "answers", "max_attempts"] {
                        inv.remove(k);
                    }
                    (
                        Value::Object(inv),
                        i.get("answers").cloned().unwrap_or(Value::Null),
                    )
                })
                .unwrap_or((json!({}), Value::Null))
        });
        let rid = content_id(
            "ubos.chunk.v1",
            &json!({"invocation": inv, "request": request}),
        )?;
        let key = format!("input_request:{rid}");
        let req = self.store.meta(&key)?.filter(|v| !v.is_null());
        let Some(given) = answers.get(&rid).filter(|v| !v.is_null()) else {
            let fresh = match &req {
                None => true,
                Some(r) => {
                    r["consumed"] == json!(true)
                        || self.now.as_str() >= s(r, "expires_at").unwrap_or_default()
                }
            };
            let req = if fresh {
                let r = json!({"request": request, "fields": fields, "message": message,
                    "expires_at": crate::workflows::shift(&self.now, &timeout, 1)?});
                self.store.put_meta(&key, &r)?;
                r
            } else {
                req.unwrap_or_default()
            };
            return Err(BpaError::with_details(
                NEEDS_INPUT,
                json!({"request_id": rid, "request": req["request"], "fields": req["fields"],
                       "message": req["message"], "expires_at": req["expires_at"]}),
            ));
        };
        let Some(req) = req.filter(|r| r["consumed"] != json!(true)) else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown input request"));
        };
        if self.now.as_str() >= s(&req, "expires_at").unwrap_or_default() {
            return Err(BpaError::with_details(
                "FLOW.INPUT_EXPIRED",
                json!({"request_id": rid, "expires_at": req["expires_at"]}),
            ));
        }
        let Some(given) = given.as_object() else {
            return Err(invalid("answers are a map"));
        };
        let mut known: Vec<&Value> = fields.iter().collect();
        if given
            .keys()
            .any(|k| !known.iter().any(|f| s(f, "name") == Some(k.as_str())))
        {
            return Err(invalid("unknown field"));
        }
        known.sort_by_key(|f| s(f, "name").unwrap_or_default().to_string());
        let mut out = Map::new();
        for f in known {
            let name = s(f, "name").unwrap_or_default();
            match given.get(name).filter(|v| !v.is_null()) {
                None if f["required"] == json!(true) => {
                    return Err(invalid(&format!("missing field {name}")));
                }
                None => {}
                Some(v) => {
                    out.insert(
                        name.to_string(),
                        ubos_types::values::validate_value(
                            s(f, "type").unwrap_or_default(),
                            f.get("constraints").filter(|c| !c.is_null()),
                            v,
                        )?,
                    );
                }
            }
        }
        RUNS.with(|r| {
            if let Some(last) = r.borrow_mut().last_mut() {
                last.1.push(rid.clone());
            }
        });
        Ok(Value::Object(out))
    }

    /// The requests a committed process answered are consumed.
    pub(crate) fn input_consumed(&mut self, answered: &[String]) -> Result<(), BpaError> {
        for rid in answered {
            let key = format!("input_request:{rid}");
            if let Some(mut r) = self.store.meta(&key)?.filter(|v| !v.is_null()) {
                r["consumed"] = json!(true);
                self.store.put_meta(&key, &r)?;
            }
        }
        Ok(())
    }
}
