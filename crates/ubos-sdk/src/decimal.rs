//! Exact decimals and money (DSN-SDK-013, STD-FND-020, STD-FND-022): string-backed values
//! in canonical form, never floating point; [`reject_floats`] refuses a JSON value that
//! carries a non-integer number at the API boundary (`LOGIC.FLOAT_TO_DECIMAL`).

use crate::error::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
use std::fmt;
use std::str::FromStr;

/// A decimal in the canonical form of STD-FND-022 (no exponent, no leading `+`, no
/// leading zeros, no negative zero, no trailing fraction zeros).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decimal(String);

fn invalid(m: &str) -> Error {
    Error::new(
        "MODEL.INVALID_VALUE",
        m,
        json!({"reason": "decimal_syntax"}),
    )
}

impl Decimal {
    /// Parse a decimal string and put it in canonical form.
    pub fn new(text: &str) -> Result<Decimal> {
        let (neg, rest) = match text.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, text),
        };
        let (int, frac) = rest.split_once('.').unwrap_or((rest, ""));
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(int) || (rest.contains('.') && !digits(frac)) {
            return Err(invalid("a decimal is [-]digits[.digits]"));
        }
        let int = int.trim_start_matches('0');
        let int = if int.is_empty() { "0" } else { int };
        let frac = frac.trim_end_matches('0');
        let zero = int == "0" && frac.is_empty();
        let mut s = String::new();
        if neg && !zero {
            s.push('-');
        }
        s.push_str(int);
        if !frac.is_empty() {
            s.push('.');
            s.push_str(frac);
        }
        Ok(Decimal(s))
    }

    /// The canonical text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The text with exactly `scale` fraction digits (a declared scale, STD-FND-022);
    /// `None` when more digits would be lost.
    pub fn with_scale(&self, scale: usize) -> Option<String> {
        let (int, frac) = self.0.split_once('.').unwrap_or((&self.0, ""));
        if frac.len() > scale {
            return None;
        }
        if scale == 0 {
            return Some(int.to_string());
        }
        Some(format!("{int}.{frac:0<scale$}"))
    }
}

impl FromStr for Decimal {
    type Err = Error;
    fn from_str(s: &str) -> Result<Decimal> {
        Decimal::new(s)
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<i64> for Decimal {
    fn from(n: i64) -> Decimal {
        Decimal(n.to_string())
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Decimal {
    /// A decimal is a JSON string; a JSON number is refused.
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Decimal, D::Error> {
        let s = String::deserialize(d)?;
        Decimal::new(&s).map_err(serde::de::Error::custom)
    }
}

/// An amount of money `{amount, currency}` (STD-FND-020).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    /// The amount.
    pub amount: Decimal,
    /// The ISO 4217 currency code.
    pub currency: String,
}

impl Money {
    /// Money from an amount text and a currency code (three upper-case letters).
    pub fn new(amount: &str, currency: &str) -> Result<Money> {
        if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(Error::new(
                "MODEL.INVALID_VALUE",
                "a currency is an ISO 4217 code",
                json!({"reason": "currency"}),
            ));
        }
        Ok(Money {
            amount: Decimal::new(amount)?,
            currency: currency.to_string(),
        })
    }
}

/// Refuse a value carrying a non-integer JSON number anywhere (`LOGIC.FLOAT_TO_DECIMAL`
/// with the path): decimals and money travel as strings (DSN-SDK-013).
pub fn reject_floats(v: &Value) -> Result<()> {
    fn walk(v: &Value, path: &mut Vec<String>) -> Result<()> {
        match v {
            Value::Number(n) if n.is_f64() => Err(Error::new(
                "LOGIC.FLOAT_TO_DECIMAL",
                "a floating-point number; send decimals as strings",
                json!({"path": path.join(".")}),
            )),
            Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    path.push(i.to_string());
                    walk(x, path)?;
                    path.pop();
                }
                Ok(())
            }
            Value::Object(o) => {
                for (k, x) in o {
                    path.push(k.clone());
                    walk(x, path)?;
                    path.pop();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    walk(v, &mut Vec::new())
}
