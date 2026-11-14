//! Money arithmetic that never mixes currencies (STD-FND-023, DSN-DVM-409, FR-QRY-042):
//! totals are kept per currency, and a single total of several currencies exists only
//! through an explicit conversion (`fx`).

use crate::currency;
use serde_json::Value;
use std::collections::BTreeMap;
use ubos_types::BpaError;
use ubos_types::decimal::Decimal;

/// `MODEL.CURRENCY_MISMATCH` naming the two currencies.
pub fn mismatch(a: &str, b: &str) -> BpaError {
    BpaError::new(
        "MODEL.CURRENCY_MISMATCH",
        format!("{a} and {b} are different currencies"),
    )
}

/// The sum of two amounts of one currency; different currencies are
/// `MODEL.CURRENCY_MISMATCH`.
pub fn add(a: (&Decimal, &str), b: (&Decimal, &str)) -> Result<Decimal, BpaError> {
    if a.1 != b.1 {
        return Err(mismatch(a.1, b.1));
    }
    Ok(a.0.add(b.0))
}

/// Totals per currency.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Totals(BTreeMap<String, Decimal>);

impl Totals {
    /// No totals.
    pub fn new() -> Totals {
        Totals::default()
    }

    /// Add an amount of a listed currency.
    pub fn add(&mut self, amount: &Decimal, code: &str) -> Result<(), BpaError> {
        currency::currency(code)?;
        let t = match self.0.get(code) {
            Some(t) => t.add(amount),
            None => amount.clone(),
        };
        self.0.insert(code.to_string(), t);
        Ok(())
    }

    /// Add a money value `{amount, currency}`.
    pub fn add_money(&mut self, v: &Value) -> Result<(), BpaError> {
        let (a, c) = currency::parse_money(v)?;
        self.add(&a, &c)
    }

    /// The total of a currency.
    pub fn get(&self, code: &str) -> Option<&Decimal> {
        self.0.get(code)
    }

    /// The currencies with a total, in order.
    pub fn currencies(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// The totals as `(currency, amount)`, ordered by currency.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Decimal)> {
        self.0.iter().map(|(c, a)| (c.as_str(), a))
    }

    /// The number of currencies.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True without any total.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The one total when every amount had the same currency, `None` without amounts;
    /// several currencies are `MODEL.CURRENCY_MISMATCH`.
    pub fn single(&self) -> Result<Option<(&str, &Decimal)>, BpaError> {
        let mut it = self.iter();
        match (it.next(), it.next()) {
            (None, _) => Ok(None),
            (Some(one), None) => Ok(Some(one)),
            (Some(a), Some(b)) => Err(mismatch(a.0, b.0)),
        }
    }

    /// The totals as money values, ordered by currency.
    pub fn to_values(&self) -> Result<Vec<Value>, BpaError> {
        self.iter().map(|(c, a)| currency::money(a, c)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Rng;
    use serde_json::json;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).expect("decimal")
    }

    #[test]
    fn totals_are_kept_per_currency() {
        // DSN-DVM-409 acceptance 1, FR-QRY-042 acceptance 1
        let mut t = Totals::new();
        t.add_money(&json!({"amount": "100.00", "currency": "USD"}))
            .unwrap();
        t.add_money(&json!({"amount": "40.00", "currency": "EUR"}))
            .unwrap();
        t.add_money(&json!({"amount": "-25.50", "currency": "USD"}))
            .unwrap();
        assert_eq!(
            t.to_values().unwrap(),
            vec![
                json!({"amount": "40.00", "currency": "EUR"}),
                json!({"amount": "74.50", "currency": "USD"})
            ]
        );
        assert_eq!(t.single().unwrap_err().code, "MODEL.CURRENCY_MISMATCH");
        assert!(t.add(&d("1"), "XYZ").is_err());
    }

    #[test]
    fn arithmetic_across_currencies_fails() {
        assert_eq!(add((&d("1"), "USD"), (&d("2"), "USD")).unwrap(), d("3"));
        assert_eq!(
            add((&d("1"), "USD"), (&d("2"), "EUR")).unwrap_err().code,
            "MODEL.CURRENCY_MISMATCH"
        );
        let mut one = Totals::new();
        assert_eq!(one.single().unwrap(), None);
        one.add(&d("5"), "JPY").unwrap();
        assert_eq!(one.single().unwrap(), Some(("JPY", &d("5"))));
    }

    #[test]
    fn property_totals_do_not_depend_on_the_order() {
        let codes = ["USD", "EUR", "JPY", "KWD"];
        let mut rng = Rng(7);
        for _ in 0..200 {
            let n = rng.below(30) as usize;
            let items: Vec<(Decimal, &str)> = (0..n)
                .map(|_| {
                    let cents = rng.below(2_000_000) as i64 - 1_000_000;
                    (
                        d(&format!("{}", cents)).mul(&d("0.001")),
                        codes[rng.below(4) as usize],
                    )
                })
                .collect();
            let mut forward = Totals::new();
            for (a, c) in &items {
                forward.add(a, c).unwrap();
            }
            let mut backward = Totals::new();
            for (a, c) in items.iter().rev() {
                backward.add(a, c).unwrap();
            }
            assert_eq!(forward, backward);
            // every total is the sum of the amounts of its currency, and of no other
            for code in codes {
                let want = items
                    .iter()
                    .filter(|(_, c)| *c == code)
                    .fold(None, |acc: Option<Decimal>, (a, _)| {
                        Some(acc.map_or(a.clone(), |x| x.add(a)))
                    });
                assert_eq!(forward.get(code), want.as_ref());
            }
        }
    }
}
