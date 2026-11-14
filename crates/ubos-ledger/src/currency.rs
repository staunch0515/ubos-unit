//! Currency reference data (FR-LOC-021, DSN-DVM-410): access to the ISO 4217 table of
//! `ubos-types`, the default scale of a currency and rounding to its minor unit
//! (STD-FND-023).

use serde_json::{Value, json};
use ubos_types::BpaError;
pub use ubos_types::currency::{CURRENCIES, Currency, TABLE_VERSION};
use ubos_types::decimal::{Decimal, Rounding};
use ubos_types::error::invalid;

/// The currency of a code; a code the table does not list is `MODEL.INVALID_VALUE`.
pub fn currency(code: &str) -> Result<&'static Currency, BpaError> {
    ubos_types::currency::currency(code)
        .ok_or_else(|| invalid(format!("unknown ISO 4217 currency {code}")))
}

/// The default scale of a currency: its minor unit (STD-FND-023).
pub fn minor_unit(code: &str) -> Result<u32, BpaError> {
    currency(code).map(|c| c.minor_unit)
}

/// True when the code is in use on a date (`YYYY-MM-DD`).
pub fn in_use(code: &str, date: &str) -> Result<bool, BpaError> {
    ubos_types::time::parse_date(date)?;
    currency(code).map(|c| c.in_use(date))
}

/// An amount rounded to the minor unit of its currency.
pub fn round(amount: &Decimal, code: &str, mode: Rounding) -> Result<Decimal, BpaError> {
    Ok(amount.round(minor_unit(code)?, mode))
}

/// The money value `{amount, currency}` of an amount, written with the minor unit of the
/// currency, or with the scale of the amount when that is larger.
pub fn money(amount: &Decimal, code: &str) -> Result<Value, BpaError> {
    let unit = minor_unit(code)?;
    Ok(json!({"amount": amount.format(Some(unit.max(amount.scale()))), "currency": code}))
}

/// The amount and the currency of a money value `{amount, currency}`.
pub fn parse_money(v: &Value) -> Result<(Decimal, String), BpaError> {
    let o = v
        .as_object()
        .filter(|o| o.len() == 2)
        .ok_or_else(|| invalid("money is {amount, currency}"))?;
    let amount = match o.get("amount") {
        Some(Value::String(s)) => Decimal::parse(s)?,
        _ => return Err(invalid("money is {amount, currency}")),
    };
    let code = o
        .get("currency")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("money is {amount, currency}"))?;
    currency(code)?;
    Ok((amount, code.to_string()))
}

/// The reference record of a currency: `{code, minor_unit, from, to, table_version}`.
pub fn describe(c: &Currency) -> Value {
    json!({"code": c.code, "minor_unit": c.minor_unit, "from": c.from, "to": c.to,
        "table_version": TABLE_VERSION})
}

/// The reference records of the currencies in use on a date, or of every listed currency.
pub fn list(date: Option<&str>) -> Result<Vec<Value>, BpaError> {
    if let Some(d) = date {
        ubos_types::time::parse_date(d)?;
    }
    Ok(CURRENCIES
        .iter()
        .filter(|c| date.is_none_or(|d| c.in_use(d)))
        .map(describe)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).expect("decimal")
    }

    #[test]
    fn the_default_scale_is_the_minor_unit() {
        // FR-LOC-021 acceptance 1: JPY has scale 0 by default
        assert_eq!(minor_unit("JPY").unwrap(), 0);
        assert_eq!(
            money(&d("1200"), "JPY").unwrap(),
            json!({"amount": "1200", "currency": "JPY"})
        );
        assert_eq!(
            money(&d("1"), "USD").unwrap(),
            json!({"amount": "1.00", "currency": "USD"})
        );
        assert_eq!(
            money(&d("1"), "KWD").unwrap(),
            json!({"amount": "1.000", "currency": "KWD"})
        );
        assert_eq!(money(&d("1.005"), "USD").unwrap()["amount"], json!("1.005"));
        assert_eq!(
            round(&d("1.005"), "USD", Rounding::HalfEven).unwrap(),
            d("1.00")
        );
        assert_eq!(round(&d("1.5"), "JPY", Rounding::HalfEven).unwrap(), d("2"));
        assert_eq!(minor_unit("XYZ").unwrap_err().code, "MODEL.INVALID_VALUE");
    }

    #[test]
    fn currencies_are_effective_dated() {
        assert!(in_use("HRK", "2022-06-30").unwrap());
        assert!(!in_use("HRK", "2023-01-01").unwrap());
        assert!(!in_use("SLE", "2022-06-30").unwrap());
        assert!(in_use("EUR", "2026-11-14").unwrap());
        let before: Vec<Value> = list(Some("2022-12-31")).unwrap();
        let after: Vec<Value> = list(Some("2023-01-01")).unwrap();
        assert!(before.iter().any(|c| c["code"] == "HRK"));
        assert!(!after.iter().any(|c| c["code"] == "HRK"));
        assert_eq!(list(None).unwrap().len(), CURRENCIES.len());
        assert_eq!(
            describe(currency("JPY").unwrap())["table_version"],
            json!(TABLE_VERSION)
        );
    }

    #[test]
    fn money_values_parse() {
        let (a, c) = parse_money(&json!({"amount": "12.50", "currency": "EUR"})).unwrap();
        assert_eq!((a, c.as_str()), (d("12.5"), "EUR"));
        assert!(parse_money(&json!({"amount": "1", "currency": "XYZ"})).is_err());
        assert!(parse_money(&json!({"amount": 1, "currency": "EUR"})).is_err());
        assert!(parse_money(&json!("1 EUR")).is_err());
    }
}
