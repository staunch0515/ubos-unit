//! Value types, money and classification (STD-FND-020…027).

use crate::canon::{MAX_SAFE, canonical, nfc};
use crate::decimal::{Decimal, Rounding};
use crate::error::{BpaError, invalid, violated};
use crate::time::{is_duration, is_time, parse_date, parse_instant};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;

/// Value types of STD-FND-020.
pub const TYPES: &[&str] = &[
    "text",
    "integer",
    "decimal",
    "money",
    "quantity",
    "boolean",
    "date",
    "time",
    "instant",
    "period",
    "duration",
    "enum",
    "reference",
    "composite",
    "list",
    "map",
    "document",
    "blob",
    "json",
];
const BLOCKS: &[&str] = &[
    "paragraph",
    "heading",
    "list",
    "list_item",
    "table",
    "row",
    "cell",
    "quote",
    "code",
    "image",
    "section",
    "clause_ref",
    "binding",
    "logic_block",
    "page_break",
];
const MARKS: &[&str] = &[
    "bold",
    "italic",
    "underline",
    "strike",
    "link",
    "comment_ref",
];
/// Classification levels in order (STD-FND-027).
pub const LEVELS: &[&str] = &[
    "public",
    "internal",
    "confidential",
    "personal",
    "sensitive_personal",
];

/// The ISO 4217 codes of list one with a minor unit, as current on 2026-01-01
/// (STD-FND-028, BPA 1.0-rc9).
pub const ISO_4217: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT", "BGN",
    "BHD", "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD", "CAD",
    "CDF", "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUP", "CVE", "CZK",
    "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS",
    "GIP", "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IQD",
    "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD", "KYD",
    "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT",
    "MOP", "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD", "NGN", "NIO", "NOK",
    "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON", "RSD",
    "RUB", "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP",
    "STN", "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD", "TZS",
    "UAH", "UGX", "USD", "USN", "UYI", "UYU", "UYW", "UZS", "VED", "VES", "VND", "VUV", "WST",
    "XAF", "XCD", "XCG", "XOF", "XPF", "YER", "ZAR", "ZMW", "ZWG",
];

/// The minor units of a code of the ISO 4217 table (STD-FND-028), or `None`.
pub fn table_unit(currency: &str) -> Option<u32> {
    if ISO_4217.binary_search(&currency).is_err() {
        return None;
    }
    Some(match currency {
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX"
        | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "CLF" | "UYW" => 4,
        _ => 2,
    })
}

thread_local! {
    /// The codes added by `Currency` objects on the branch of the commit being made
    /// (STD-FND-028): set by the engine for the duration of a commit.
    static ADDED: std::cell::RefCell<std::collections::BTreeMap<String, u32>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Restores the codes added before [`added_currencies`] when dropped.
pub struct AddedGuard(std::collections::BTreeMap<String, u32>);

impl Drop for AddedGuard {
    fn drop(&mut self) {
        let prev = std::mem::take(&mut self.0);
        ADDED.with(|a| *a.borrow_mut() = prev);
    }
}

/// Make the codes added on a branch valid until the guard is dropped (STD-FND-028).
pub fn added_currencies(codes: std::collections::BTreeMap<String, u32>) -> AddedGuard {
    AddedGuard(ADDED.with(|a| std::mem::replace(&mut *a.borrow_mut(), codes)))
}

/// The minor units of a currency: the ISO 4217 table, then the codes added on the branch
/// of the current commit (STD-FND-023, -028).
pub fn minor_unit(currency: &str) -> Option<u32> {
    table_unit(currency).or_else(|| ADDED.with(|a| a.borrow().get(currency).copied()))
}

fn full_match(pattern: &str, text: &str) -> Result<bool, BpaError> {
    let r = Regex::new(&format!("^(?:{pattern})$")).map_err(|_| invalid("invalid pattern"))?;
    Ok(r.is_match(text))
}

fn dec_of(v: &Value) -> Result<Decimal, BpaError> {
    match v {
        Value::String(s) => Decimal::parse(s),
        Value::Number(n) => Decimal::parse(&n.to_string()),
        _ => Err(invalid("not a decimal")),
    }
}

fn bounds<T: Ord>(
    v: &T,
    c: &Map<String, Value>,
    conv: impl Fn(&Value) -> Result<T, BpaError>,
) -> Result<(), BpaError> {
    if let Some(min) = c.get("min")
        && v.cmp(&conv(min)?) == Ordering::Less
    {
        return Err(violated("below min"));
    }
    if let Some(max) = c.get("max")
        && v.cmp(&conv(max)?) == Ordering::Greater
    {
        return Err(violated("above max"));
    }
    Ok(())
}

fn rounding(c: &Map<String, Value>) -> Result<Rounding, BpaError> {
    Rounding::parse(
        c.get("rounding")
            .and_then(Value::as_str)
            .unwrap_or("half_even"),
    )
}

fn as_u32(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64).map(|x| x as u32)
}

/// Canonical money value with the constraints of a field (STD-FND-023).
pub fn money_value(v: &Value, c: &Map<String, Value>) -> Result<Value, BpaError> {
    let o = v
        .as_object()
        .filter(|o| o.len() == 2 && o.contains_key("amount") && o.contains_key("currency"));
    let o = o.ok_or_else(|| invalid("money is {amount, currency}"))?;
    let cur = o["currency"].as_str().ok_or_else(|| invalid("currency"))?;
    let unit = minor_unit(cur).ok_or_else(|| invalid("unknown ISO 4217 currency"))?;
    if let Some(list) = c.get("currencies").and_then(Value::as_array)
        && !list.iter().any(|x| x.as_str() == Some(cur))
    {
        return Err(violated("currency not allowed"));
    }
    let scale = as_u32(c.get("scale")).unwrap_or(unit);
    let amount = match &o["amount"] {
        Value::String(s) => Decimal::parse(s)?,
        _ => return Err(invalid("amount is a decimal string")),
    };
    let amount = amount.round(scale, rounding(c)?);
    bounds(&amount, c, dec_of)?;
    Ok(json!({"amount": amount.format(Some(scale)), "currency": cur}))
}

/// Money addition and subtraction; different currencies fail (STD-FND-023).
pub fn money_op(op: &str, a: &Value, b: &Value) -> Result<Value, BpaError> {
    let (a, b) = (money_value(a, &Map::new())?, money_value(b, &Map::new())?);
    if a["currency"] != b["currency"] {
        return Err(BpaError::new(
            "MODEL.CURRENCY_MISMATCH",
            "currencies differ",
        ));
    }
    let cur = a["currency"].as_str().unwrap_or_default().to_string();
    let (x, y) = (dec_of(&a["amount"])?, dec_of(&b["amount"])?);
    let r = match op {
        "add" => x.add(&y),
        "sub" => x.sub(&y),
        _ => return Err(invalid("unknown money operation")),
    };
    Ok(json!({"amount": r.format(minor_unit(&cur)), "currency": cur}))
}

/// Decimal operations of the adapter (STD-FND-022).
pub fn decimal_op(
    op: &str,
    a: &str,
    b: Option<&str>,
    scale: Option<u32>,
    rounding: Option<&str>,
) -> Result<String, BpaError> {
    let x = Decimal::parse(a)?;
    let mode = |r: Option<&str>| Rounding::parse(r.unwrap_or("half_even"));
    match op {
        "canonical" => {
            if let Some(s) = scale
                && x.scale() > s
            {
                return Err(invalid("more fraction digits than the scale"));
            }
            return Ok(x.format(scale));
        }
        "round" => {
            return Ok(x
                .round(scale.unwrap_or(0), mode(rounding)?)
                .format(scale.or(Some(0))));
        }
        _ => {}
    }
    let y = Decimal::parse(b.ok_or_else(|| invalid("second operand"))?)?;
    if op == "div" {
        let (Some(s), Some(r)) = (scale, rounding) else {
            return Err(BpaError::new(
                "LOGIC.TYPE_ERROR",
                "division needs a scale and a rounding mode",
            ));
        };
        return Ok(x.div(&y, s, Rounding::parse(r)?)?.format(Some(s)));
    }
    let v = match op {
        "add" => x.add(&y),
        "sub" => x.sub(&y),
        "mul" => x.mul(&y),
        _ => return Err(invalid("unknown decimal operation")),
    };
    Ok(match scale {
        Some(s) => v.round(s, mode(rounding)?).format(Some(s)),
        None => v.format(None),
    })
}

/// Whether a set of clearances covers a classification level (STD-FND-027).
pub fn classification_covers(clearances: &[String], level: &str) -> Result<bool, BpaError> {
    let rank = |l: &str| LEVELS.iter().position(|x| *x == l);
    for c in clearances.iter().map(String::as_str).chain([level]) {
        if rank(c).is_none() {
            return Err(invalid("unknown classification"));
        }
    }
    if clearances.iter().any(|c| c == level) {
        return Ok(true);
    }
    if level == "personal" || level == "sensitive_personal" {
        return Ok(false);
    }
    Ok(clearances.iter().any(|c| rank(c) > rank(level)))
}

fn integer(v: &Value) -> Result<i128, BpaError> {
    match v {
        Value::Number(n) => {
            let i = n.as_i64().ok_or_else(|| invalid("not an integer"))?;
            if i.abs() > MAX_SAFE {
                return Err(invalid("integer beyond 2^53-1 must be a string"));
            }
            Ok(i as i128)
        }
        Value::String(s) => {
            let body = s.strip_prefix('-').unwrap_or(s);
            let ok = !body.is_empty()
                && !body.starts_with('0')
                && body.bytes().all(|b| b.is_ascii_digit());
            let i: i128 = if ok {
                s.parse().map_err(|_| invalid("not an integer"))?
            } else {
                0
            };
            if ok && i.abs() > MAX_SAFE as i128 {
                Ok(i)
            } else {
                Err(invalid("not an integer"))
            }
        }
        _ => Err(invalid("not an integer")),
    }
}

fn document(node: &Value) -> Result<Value, BpaError> {
    let o = node
        .as_object()
        .filter(|o| o.contains_key("kind"))
        .ok_or_else(|| invalid("document node needs a kind"))?;
    if o.keys()
        .any(|k| !["kind", "attrs", "text", "marks", "children"].contains(&k.as_str()))
    {
        return Err(invalid("unknown document node member"));
    }
    let kind = o["kind"].as_str().unwrap_or_default();
    if kind != "text" && !BLOCKS.contains(&kind) {
        return Err(invalid("unknown document node kind"));
    }
    if let Some(marks) = o.get("marks").and_then(Value::as_array)
        && marks
            .iter()
            .any(|m| !m.as_str().is_some_and(|m| MARKS.contains(&m)))
    {
        return Err(invalid("unknown inline mark"));
    }
    if kind == "text" && o.contains_key("children") {
        return Err(invalid("text leaves have no children"));
    }
    let mut out = o.clone();
    if let Some(Value::String(t)) = o.get("text") {
        out.insert("text".into(), Value::String(nfc(t)));
    }
    if let Some(children) = o.get("children").and_then(Value::as_array) {
        out.insert(
            "children".into(),
            Value::Array(children.iter().map(document).collect::<Result<_, _>>()?),
        );
    }
    Ok(Value::Object(out))
}

fn count_ok(n: usize, c: &Map<String, Value>, min: &str, max: &str) -> bool {
    n >= as_u32(c.get(min)).unwrap_or(0) as usize
        && as_u32(c.get(max)).is_none_or(|m| n <= m as usize)
}

/// A canonical object URI without selectors or version suffix (STD-ADDR-001, STD-FND-020).
pub fn is_reference(uri: &str) -> bool {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let r = R.get_or_init(|| {
        let slug = "[a-z0-9][a-z0-9_-]{0,62}";
        regex::Regex::new(&format!(
            "^ubos://{slug}(\\.{slug})?/[A-Z][A-Za-z0-9]{{0,63}}/{slug}$"
        ))
        .expect("grammar")
    });
    r.is_match(uri)
}

/// Return the canonical value, or fail with `MODEL.INVALID_VALUE` (representation),
/// `MODEL.CONSTRAINT_VIOLATED` (constraint) or `MODEL.UNKNOWN_TYPE` (STD-FND-020).
pub fn validate_value(
    type_: &str,
    constraints: Option<&Value>,
    value: &Value,
) -> Result<Value, BpaError> {
    let empty = Map::new();
    let c = constraints.and_then(Value::as_object).unwrap_or(&empty);
    if !TYPES.contains(&type_) {
        return Err(BpaError::new("MODEL.UNKNOWN_TYPE", type_));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    match type_ {
        "text" => {
            let s = value.as_str().ok_or_else(|| invalid("not text"))?;
            let v = nfc(s);
            if !count_ok(v.chars().count(), c, "min_length", "max_length") {
                return Err(violated("length"));
            }
            if let Some(p) = c.get("pattern").and_then(Value::as_str)
                && !full_match(p, &v)?
            {
                return Err(violated("pattern"));
            }
            Ok(Value::String(v))
        }
        "integer" => {
            if value.is_boolean() {
                return Err(invalid("boolean is not an integer"));
            }
            let i = integer(value)?;
            bounds(&i, c, integer)?;
            Ok(value.clone())
        }
        "decimal" => {
            let mut d = dec_of_str(value)?;
            if let Some(s) = as_u32(c.get("scale")) {
                d = d.round(s, rounding(c)?);
            }
            if let Some(p) = as_u32(c.get("precision"))
                && d.digits() > p as usize
            {
                return Err(violated("precision"));
            }
            bounds(&d, c, dec_of)?;
            Ok(Value::String(d.format(as_u32(c.get("scale")))))
        }
        "money" => money_value(value, c),
        "quantity" => {
            let o = value
                .as_object()
                .filter(|o| {
                    o.len() == 2
                        && o.contains_key("value")
                        && o.get("unit").is_some_and(Value::is_string)
                })
                .ok_or_else(|| invalid("quantity is {value, unit}"))?;
            let d = dec_of_str(&o["value"])?;
            if let Some(units) = c.get("units").and_then(Value::as_array)
                && !units.contains(&o["unit"])
            {
                return Err(violated("unit not allowed"));
            }
            bounds(&d, c, dec_of)?;
            Ok(json!({"value": d.format(None), "unit": o["unit"]}))
        }
        "boolean" => value
            .is_boolean()
            .then(|| value.clone())
            .ok_or_else(|| invalid("not a boolean")),
        "date" => {
            let d = parse_date(value.as_str().ok_or_else(|| invalid("not a date"))?)?;
            bounds(&d, c, |v| parse_date(v.as_str().unwrap_or_default()))?;
            Ok(value.clone())
        }
        "time" => value
            .as_str()
            .filter(|s| is_time(s))
            .map(|_| value.clone())
            .ok_or_else(|| invalid("not a time")),
        "instant" => {
            let i = parse_instant(value.as_str().ok_or_else(|| invalid("not an instant"))?)?;
            bounds(&i, c, |v| parse_instant(v.as_str().unwrap_or_default()))?;
            Ok(value.clone())
        }
        "period" => {
            let o = value
                .as_object()
                .filter(|o| o.len() == 2 && o.contains_key("from") && o.contains_key("to"))
                .ok_or_else(|| invalid("period is {from, to}"))?;
            let instant = c.get("granularity").and_then(Value::as_str) == Some("instant");
            let key = |v: &Value| -> Result<String, BpaError> {
                let s = v.as_str().ok_or_else(|| invalid("period bound"))?;
                if instant {
                    parse_instant(s)?;
                } else {
                    parse_date(s)?;
                }
                Ok(s.to_string())
            };
            let from = key(&o["from"])?;
            if !o["to"].is_null() && key(&o["to"])? <= from {
                return Err(invalid("empty or inverted period"));
            }
            Ok(value.clone())
        }
        "duration" => value
            .as_str()
            .filter(|s| is_duration(s))
            .map(|_| value.clone())
            .ok_or_else(|| invalid("not a duration")),
        "enum" => {
            let s = value.as_str().ok_or_else(|| invalid("not an enum value"))?;
            let allowed = c.get("allowed_values").and_then(Value::as_array);
            if !allowed.is_some_and(|a| a.iter().any(|x| x.as_str() == Some(s))) {
                return Err(violated("not an allowed value"));
            }
            Ok(value.clone())
        }
        "reference" => {
            let u = value.as_str().ok_or_else(|| invalid("not a reference"))?;
            if !is_reference(u) {
                return Err(invalid("a reference is a canonical URI without selectors"));
            }
            Ok(value.clone())
        }
        "composite" => {
            let members = c
                .get("members")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let o = value
                .as_object()
                .ok_or_else(|| invalid("not a composite"))?;
            if o.keys().any(|k| !members.contains_key(k)) {
                return Err(invalid("undeclared member"));
            }
            let mut out = Map::new();
            for (name, m) in &members {
                if let Some(v) = o.get(name) {
                    out.insert(
                        name.clone(),
                        validate_value(
                            m["type"].as_str().unwrap_or_default(),
                            m.get("constraints"),
                            v,
                        )?,
                    );
                } else if m.get("required") == Some(&Value::Bool(true)) {
                    return Err(violated("required member missing"));
                }
            }
            Ok(Value::Object(out))
        }
        "list" => {
            let items = value.as_array().ok_or_else(|| invalid("not a list"))?;
            if !count_ok(items.len(), c, "min_items", "max_items") {
                return Err(violated("item count"));
            }
            let out: Vec<Value> = match c.get("item_type") {
                Some(t) => items
                    .iter()
                    .map(|v| {
                        validate_value(
                            t["type"].as_str().unwrap_or_default(),
                            t.get("constraints"),
                            v,
                        )
                    })
                    .collect::<Result<_, _>>()?,
                None => items.clone(),
            };
            if let Some(key) = c.get("key").and_then(Value::as_str) {
                let mut seen = std::collections::BTreeSet::new();
                for v in &out {
                    let k = match v.as_object() {
                        Some(o) => Some(canonical(o.get(key).unwrap_or(&Value::Null))?),
                        None => None,
                    };
                    if k.is_none() || !seen.insert(k) {
                        return Err(violated("keyed list needs unique keys"));
                    }
                }
            }
            Ok(Value::Array(out))
        }
        "map" => {
            let o = value.as_object().ok_or_else(|| invalid("not a map"))?;
            let mut out = Map::new();
            for (k, v) in o {
                if let Some(p) = c.get("key_pattern").and_then(Value::as_str)
                    && !full_match(p, k)?
                {
                    return Err(violated("key pattern"));
                }
                let v = match c.get("value_type") {
                    Some(t) => validate_value(
                        t["type"].as_str().unwrap_or_default(),
                        t.get("constraints"),
                        v,
                    )?,
                    None => v.clone(),
                };
                out.insert(k.clone(), v);
            }
            Ok(Value::Object(out))
        }
        "document" => document(value),
        "blob" => {
            let o = value.as_object().filter(|o| {
                o.len() == 4
                    && ["content_id", "size", "media_type", "name"]
                        .iter()
                        .all(|k| o.contains_key(*k))
            });
            let o = o.ok_or_else(|| invalid("blob is {content_id, size, media_type, name}"))?;
            let cid_ok = o["content_id"].as_str().is_some_and(|s| {
                s.len() == 71
                    && s.starts_with("sha256:")
                    && s[7..]
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
            if !cid_ok {
                return Err(invalid("not a content identifier"));
            }
            let size = o["size"].as_u64().ok_or_else(|| invalid("size"))?;
            if let Some(types) = c.get("media_types").and_then(Value::as_array)
                && !types.contains(&o["media_type"])
            {
                return Err(violated("media type"));
            }
            if as_u32(c.get("max_size")).is_some_and(|m| size > m as u64) {
                return Err(violated("size"));
            }
            Ok(value.clone())
        }
        "json" => {
            let text = canonical(value)?;
            if as_u32(c.get("max_size")).is_some_and(|m| text.len() > m as usize) {
                return Err(violated("size"));
            }
            Ok(value.clone())
        }
        _ => Err(BpaError::new("MODEL.UNKNOWN_TYPE", type_)),
    }
}

fn dec_of_str(v: &Value) -> Result<Decimal, BpaError> {
    match v {
        Value::String(s) => Decimal::parse(s),
        _ => Err(invalid("not a decimal string")),
    }
}

// ---- field index keys (DSN-DVM-902, PH-1 step 13a) ----

/// Tags of the order-preserving value keys; a key compares as the values of its type do.
const KEY_NULL: u8 = 0x00;
const KEY_BOOL: u8 = 0x01;
const KEY_NUM: u8 = 0x02;
const KEY_TEXT: u8 = 0x03;
const KEY_TIME: u8 = 0x04;
const KEY_MONEY: u8 = 0x05;

/// Order-preserving encoding of a decimal text: the sign byte (0 negative, 1 zero, 2
/// positive), the number of integer digits and the digits; negative values are the byte
/// complement of their magnitude, so byte order is numeric order.
fn decimal_key(text: &str) -> Option<Vec<u8>> {
    let d = Decimal::parse(text).ok()?;
    let t = d.format(None);
    if d.is_zero() {
        return Some(vec![1]);
    }
    let (negative, mag) = match t.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, t.as_str()),
    };
    let (int_part, frac) = mag.split_once('.').unwrap_or((mag, ""));
    let int_part = int_part.trim_start_matches('0');
    let mut body = Vec::with_capacity(4 + mag.len());
    body.extend_from_slice(&(int_part.len() as u32).to_be_bytes());
    body.extend_from_slice(int_part.as_bytes());
    body.push(b'.');
    body.extend_from_slice(frac.as_bytes());
    let mut out = Vec::with_capacity(1 + body.len());
    if negative {
        out.push(0);
        out.extend(body.iter().map(|b| 0xFF - b));
    } else {
        out.push(2);
        out.extend(body);
    }
    Some(out)
}

/// The order-preserving key of a field value for the field index (DSN-DVM-902): `None`
/// for values of types that are not indexed (composites, lists, maps, documents, blobs,
/// JSON, periods, quantities and durations). Equal values give equal keys and byte order
/// is the order of the type (`order by` and the comparisons of criteria queries).
pub fn index_key(type_name: &str, v: &Value) -> Option<Vec<u8>> {
    if v.is_null() {
        return Some(vec![KEY_NULL]);
    }
    let mut out = Vec::new();
    match type_name {
        "boolean" => {
            out.push(KEY_BOOL);
            out.push(u8::from(v.as_bool()?));
        }
        "integer" | "decimal" => {
            out.push(KEY_NUM);
            let text = match v {
                Value::Number(n) => n.to_string(),
                Value::String(t) => t.clone(),
                _ => return None,
            };
            out.extend(decimal_key(&text)?);
        }
        "money" => {
            out.push(KEY_MONEY);
            let o = v.as_object()?;
            out.extend_from_slice(o.get("currency")?.as_str()?.as_bytes());
            out.push(0);
            out.extend(decimal_key(o.get("amount")?.as_str()?)?);
        }
        "text" | "enum" | "reference" | "time" => {
            out.push(KEY_TEXT);
            out.extend_from_slice(v.as_str()?.as_bytes());
        }
        "date" | "instant" => {
            out.push(KEY_TIME);
            out.extend_from_slice(v.as_str()?.as_bytes());
        }
        _ => return None,
    }
    Some(out)
}

/// Whether a field type has index keys.
pub fn indexable(type_name: &str) -> bool {
    matches!(
        type_name,
        "boolean"
            | "integer"
            | "decimal"
            | "money"
            | "text"
            | "enum"
            | "reference"
            | "time"
            | "date"
            | "instant"
    )
}
