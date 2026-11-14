//! Exchange rates (FR-LOC-022, DSN-DVM-410): the rate of a key `(source, from, to, rate
//! type)` for a valid date among the rates known at a transaction time, and the
//! conversion of an amount by a rate, rounded to the minor unit of the target currency.
//!
//! A rate converts its `from` currency to its `to` currency only: one unit of `from` is
//! `rate` units of `to`. Neither the inverse of a rate nor a path over a third currency
//! is derived (STD-EXPR-005: a rate for other currencies is `MODEL.CURRENCY_MISMATCH`);
//! whoever needs the opposite direction enters that rate.
//!
//! In valid time a rate applies from its rate date until the next rate date of its key
//! (the `supersede_from` edit of STD-TIME-011). `RateBook` is the pure model of that
//! history in both times; the kernel keeps the same history as the versions of
//! Definition objects and gives `select` the quotes visible at the coordinates of a read.

use crate::currency;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ubos_types::BpaError;
use ubos_types::decimal::{Decimal, Rounding};
use ubos_types::error::invalid;

/// The key of a rate series.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RateKey {
    /// Who publishes the rate.
    pub source: String,
    /// The currency converted from (the base).
    pub from: String,
    /// The currency converted to (the quote).
    pub to: String,
    /// The kind of rate, for example a closing or an average rate.
    pub rate_type: String,
}

/// A rate: one unit of `key.from` is `rate` units of `key.to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rate {
    /// The series.
    pub key: RateKey,
    /// The rate, greater than zero.
    pub rate: Decimal,
    /// The instant of the fixing, when recorded.
    pub fixing_time: Option<String>,
}

fn text<'a>(body: &'a Value, k: &str) -> Result<&'a str, BpaError> {
    body.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("a rate names its {k}")))
}

impl Rate {
    /// The rate of a stored body `{source, from, to, rate_type, rate, fixing_time?}`: the
    /// currencies are listed and differ, and the rate is greater than zero.
    pub fn of(body: &Value) -> Result<Rate, BpaError> {
        let key = RateKey {
            source: text(body, "source")?.to_string(),
            from: text(body, "from")?.to_string(),
            to: text(body, "to")?.to_string(),
            rate_type: text(body, "rate_type")?.to_string(),
        };
        currency::currency(&key.from)?;
        currency::currency(&key.to)?;
        if key.from == key.to {
            return Err(invalid("a rate converts between two currencies"));
        }
        let rate = match body.get("rate") {
            Some(Value::String(s)) => Decimal::parse(s)?,
            _ => return Err(invalid("a rate is a decimal")),
        };
        if rate <= Decimal::parse("0")? {
            return Err(invalid("a rate is greater than zero"));
        }
        let fixing_time = match body.get("fixing_time").filter(|v| !v.is_null()) {
            Some(v) => {
                let t = v
                    .as_str()
                    .ok_or_else(|| invalid("fixing_time is an instant"))?;
                ubos_types::time::parse_instant(t)?;
                Some(t.to_string())
            }
            None => None,
        };
        Ok(Rate {
            key,
            rate,
            fixing_time,
        })
    }
}

/// A rate with the valid period it applies to and the version that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    /// The rate.
    pub rate: Rate,
    /// The rate date: the first date the rate applies to.
    pub valid_from: String,
    /// The next rate date of the series, when there is one (exclusive end).
    pub valid_to: Option<String>,
    /// The identifier of the version that holds the rate.
    pub version: String,
}

impl Quote {
    /// True when the quote applies on a date.
    pub fn covers(&self, date: &str) -> bool {
        self.valid_from.as_str() <= date && self.valid_to.as_deref().is_none_or(|t| date < t)
    }
}

/// What a conversion asks for: the pair, and the source and rate type when several
/// series quote the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selector<'a> {
    /// The currency converted from.
    pub from: &'a str,
    /// The currency converted to.
    pub to: &'a str,
    /// The source, or any.
    pub source: Option<&'a str>,
    /// The rate type, or any.
    pub rate_type: Option<&'a str>,
}

impl Selector<'_> {
    /// True for a key the selector accepts.
    pub fn accepts(&self, k: &RateKey) -> bool {
        k.from == self.from
            && k.to == self.to
            && self.source.is_none_or(|s| s == k.source)
            && self.rate_type.is_none_or(|t| t == k.rate_type)
    }
}

/// The quote of a selector on a date among the quotes known at a transaction time.
/// Without a quote of the pair on the date the conversion is impossible
/// (`MODEL.CURRENCY_MISMATCH` with `{from, to, date}`); quotes of several series are
/// `MODEL.INVALID_VALUE`: the caller names the source and the rate type.
pub fn select<'a>(
    quotes: &'a [Quote],
    sel: &Selector<'_>,
    date: &str,
) -> Result<&'a Quote, BpaError> {
    ubos_types::time::parse_date(date)?;
    let mut found: Option<&Quote> = None;
    for q in quotes {
        if !sel.accepts(&q.rate.key) || !q.covers(date) {
            continue;
        }
        match found {
            None => found = Some(q),
            Some(f) if f.rate.key != q.rate.key => {
                return Err(invalid(format!(
                    "several rates convert {} to {}: name the source and the rate type",
                    sel.from, sel.to
                )));
            }
            // one series never has two quotes on a date; the later version wins if it does
            Some(f) if f.version < q.version => found = Some(q),
            Some(_) => {}
        }
    }
    found.ok_or_else(|| {
        BpaError::with_details(
            "MODEL.CURRENCY_MISMATCH",
            json!({"from": sel.from, "to": sel.to, "date": date, "message": "no rate"}),
        )
    })
}

/// An amount of `from` converted to `to` by a rate and rounded to the minor unit of
/// `to`; a rate for other currencies is `MODEL.CURRENCY_MISMATCH`.
pub fn convert(
    amount: &Decimal,
    from: &str,
    to: &str,
    rate: &Rate,
    mode: Rounding,
) -> Result<Decimal, BpaError> {
    if rate.key.from != from || rate.key.to != to {
        return Err(BpaError::new(
            "MODEL.CURRENCY_MISMATCH",
            format!("the rate converts {} to {}", rate.key.from, rate.key.to),
        ));
    }
    currency::round(&amount.mul(&rate.rate), to, mode)
}

/// One recorded rate of a `RateBook`.
#[derive(Debug, Clone)]
struct Edit {
    tx: u64,
    rate: Rate,
    valid_from: String,
    version: String,
}

/// A bitemporal history of rates: every edit is recorded at a transaction time and sets
/// the rate of its series from a rate date until the next rate date known then.
#[derive(Debug, Clone, Default)]
pub struct RateBook {
    edits: Vec<Edit>,
}

impl RateBook {
    /// An empty history.
    pub fn new() -> RateBook {
        RateBook::default()
    }

    /// Record a rate at transaction time `tx` for the rate date `valid_from`; transaction
    /// times never decrease.
    pub fn record(
        &mut self,
        tx: u64,
        rate: Rate,
        valid_from: &str,
        version: &str,
    ) -> Result<(), BpaError> {
        ubos_types::time::parse_date(valid_from)?;
        if self.edits.last().is_some_and(|e| e.tx > tx) {
            return Err(invalid("transaction time does not go back"));
        }
        self.edits.push(Edit {
            tx,
            rate,
            valid_from: valid_from.to_string(),
            version: version.to_string(),
        });
        Ok(())
    }

    /// The quotes as known at transaction time `tx`: per series and rate date the rate
    /// recorded last, applying until the next rate date of the series.
    pub fn known_at(&self, tx: u64) -> Vec<Quote> {
        let mut series: BTreeMap<&RateKey, BTreeMap<&str, &Edit>> = BTreeMap::new();
        for e in self.edits.iter().filter(|e| e.tx <= tx) {
            // later edits replace earlier ones of the same rate date
            series
                .entry(&e.rate.key)
                .or_default()
                .insert(e.valid_from.as_str(), e);
        }
        let mut out = Vec::new();
        for dates in series.values() {
            let mut it = dates.values().peekable();
            while let Some(e) = it.next() {
                out.push(Quote {
                    rate: e.rate.clone(),
                    valid_from: e.valid_from.clone(),
                    valid_to: it.peek().map(|n| n.valid_from.clone()),
                    version: e.version.clone(),
                });
            }
        }
        out
    }

    /// The quote of a selector for a valid date as known at transaction time `tx`.
    pub fn rate(&self, sel: &Selector<'_>, date: &str, tx: u64) -> Result<Quote, BpaError> {
        select(&self.known_at(tx), sel, date).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Rng;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).expect("decimal")
    }

    fn rate(source: &str, from: &str, to: &str, r: &str) -> Rate {
        Rate::of(
            &json!({"source": source, "from": from, "to": to, "rate_type": "close", "rate": r}),
        )
        .expect("rate")
    }

    fn pair<'a>(from: &'a str, to: &'a str) -> Selector<'a> {
        Selector {
            from,
            to,
            source: None,
            rate_type: None,
        }
    }

    #[test]
    fn rate_bodies_are_checked() {
        let ok = json!({"source": "ecb", "from": "EUR", "to": "USD", "rate_type": "close",
            "rate": "1.0842", "fixing_time": "2026-03-02T15:00:00.000000Z"});
        let r = Rate::of(&ok).unwrap();
        assert_eq!(r.rate, d("1.0842"));
        assert_eq!(
            r.fixing_time.as_deref(),
            Some("2026-03-02T15:00:00.000000Z")
        );
        let bad = |k: &str, v: Value| {
            let mut b = ok.clone();
            b[k] = v;
            Rate::of(&b).unwrap_err().code
        };
        assert_eq!(bad("from", json!("XYZ")), "MODEL.INVALID_VALUE");
        assert_eq!(bad("to", json!("EUR")), "MODEL.INVALID_VALUE");
        assert_eq!(bad("rate", json!("0")), "MODEL.INVALID_VALUE");
        assert_eq!(bad("rate", json!("-1.1")), "MODEL.INVALID_VALUE");
        assert_eq!(bad("rate", json!(1.1)), "MODEL.INVALID_VALUE");
        assert_eq!(bad("source", json!("")), "MODEL.INVALID_VALUE");
        assert_eq!(bad("fixing_time", json!("noon")), "MODEL.INVALID_VALUE");
    }

    #[test]
    fn a_corrected_rate_is_seen_from_its_transaction_time() {
        // DSN-DVM-410 acceptance 1, FR-LOC-022 acceptance 1
        let mut book = RateBook::new();
        book.record(1, rate("ecb", "EUR", "USD", "1.10"), "2026-03-02", "v1")
            .unwrap();
        book.record(2, rate("ecb", "EUR", "USD", "1.12"), "2026-03-03", "v2")
            .unwrap();
        // the rate of 2 March is corrected at transaction time 5
        book.record(5, rate("ecb", "EUR", "USD", "1.11"), "2026-03-02", "v3")
            .unwrap();
        let sel = pair("EUR", "USD");
        let original = book.rate(&sel, "2026-03-02", 4).unwrap();
        assert_eq!(
            (original.rate.rate.clone(), original.version.as_str()),
            (d("1.10"), "v1")
        );
        let corrected = book.rate(&sel, "2026-03-02", 5).unwrap();
        assert_eq!(
            (corrected.rate.rate.clone(), corrected.version.as_str()),
            (d("1.11"), "v3")
        );
        // the correction ends at the next rate date
        assert_eq!(corrected.valid_to.as_deref(), Some("2026-03-03"));
        assert_eq!(book.rate(&sel, "2026-03-03", 9).unwrap().version, "v2");
        assert_eq!(book.rate(&sel, "2026-03-09", 9).unwrap().version, "v2");
        // before the first rate date, before the first record, and for another pair
        for (date, tx, s) in [
            ("2026-03-01", 9, sel),
            ("2026-03-02", 0, sel),
            ("2026-03-02", 9, pair("USD", "EUR")),
        ] {
            assert_eq!(
                book.rate(&s, date, tx).unwrap_err().code,
                "MODEL.CURRENCY_MISMATCH"
            );
        }
    }

    #[test]
    fn several_series_need_a_source() {
        let mut book = RateBook::new();
        book.record(1, rate("ecb", "EUR", "USD", "1.10"), "2026-03-02", "a")
            .unwrap();
        book.record(1, rate("fed", "EUR", "USD", "1.09"), "2026-03-02", "b")
            .unwrap();
        assert_eq!(
            book.rate(&pair("EUR", "USD"), "2026-03-02", 1)
                .unwrap_err()
                .code,
            "MODEL.INVALID_VALUE"
        );
        let fed = Selector {
            source: Some("fed"),
            rate_type: Some("close"),
            ..pair("EUR", "USD")
        };
        assert_eq!(book.rate(&fed, "2026-03-02", 1).unwrap().version, "b");
    }

    #[test]
    fn conversion_rounds_to_the_minor_unit_of_the_target() {
        let r = rate("ecb", "EUR", "USD", "1.0842");
        assert_eq!(
            convert(&d("100.00"), "EUR", "USD", &r, Rounding::HalfEven).unwrap(),
            d("108.42")
        );
        assert_eq!(
            convert(&d("0.05"), "EUR", "USD", &r, Rounding::HalfEven).unwrap(),
            d("0.05")
        );
        let jpy = rate("boj", "USD", "JPY", "151.235");
        assert_eq!(
            convert(&d("10.50"), "USD", "JPY", &jpy, Rounding::HalfEven).unwrap(),
            d("1588")
        );
        assert_eq!(
            convert(&d("10.50"), "USD", "JPY", &jpy, Rounding::Down).unwrap(),
            d("1587")
        );
        // neither the inverse nor another pair
        for (f, t) in [("USD", "EUR"), ("EUR", "JPY"), ("GBP", "USD")] {
            assert_eq!(
                convert(&d("1"), f, t, &r, Rounding::HalfEven)
                    .unwrap_err()
                    .code,
                "MODEL.CURRENCY_MISMATCH"
            );
        }
    }

    #[test]
    fn property_conversion_is_within_half_a_minor_unit() {
        let mut rng = Rng(11);
        let targets = [("USD", "0.01"), ("JPY", "1"), ("KWD", "0.001")];
        for _ in 0..500 {
            let (to, unit) = targets[rng.below(3) as usize];
            let r = rate(
                "s",
                "EUR",
                to,
                &format!("{}.{:06}", rng.below(300), rng.below(999_999) + 1),
            );
            let amount = d(&format!("{}.{:02}", rng.below(1_000_000), rng.below(100)));
            let exact = amount.mul(&r.rate);
            let got = convert(&amount, "EUR", to, &r, Rounding::HalfEven).unwrap();
            assert!(got.scale() <= currency::minor_unit(to).unwrap());
            let diff = got.sub(&exact);
            let diff = if diff < d("0") { diff.neg() } else { diff };
            assert!(diff.add(&diff) <= d(unit), "{amount:?} at {:?}", r.rate);
            // the result is reproducible: the same inputs give the same amount
            assert_eq!(
                got,
                convert(&amount, "EUR", to, &r, Rounding::HalfEven).unwrap()
            );
            // negation commutes with half-even rounding
            assert_eq!(
                convert(&amount.neg(), "EUR", to, &r, Rounding::HalfEven).unwrap(),
                got.neg()
            );
        }
    }

    /// A reference lookup written from the definition: among the edits recorded up to
    /// `tx`, the greatest rate date not after `date`, and its latest edit.
    fn oracle<'a>(edits: &'a [(u64, String, String)], date: &str, tx: u64) -> Option<&'a str> {
        let best = edits
            .iter()
            .filter(|(t, vf, _)| *t <= tx && vf.as_str() <= date)
            .map(|(_, vf, _)| vf.as_str())
            .max()?;
        edits
            .iter()
            .rfind(|(t, vf, _)| *t <= tx && vf == best)
            .map(|(_, _, v)| v.as_str())
    }

    #[test]
    fn property_lookups_are_bitemporal() {
        let mut rng = Rng(23);
        for round in 0..100 {
            let mut book = RateBook::new();
            let mut edits: Vec<(u64, String, String)> = Vec::new();
            let mut tx = 0;
            for n in 0..rng.below(25) + 1 {
                tx += rng.below(3);
                let vf = rng.date(20).format("%Y-%m-%d").to_string();
                let version = format!("r{round}-{n}");
                let r = rate("s", "EUR", "USD", &format!("1.{:04}", rng.below(9999) + 1));
                book.record(tx, r, &vf, &version).unwrap();
                edits.push((tx, vf, version));
            }
            let sel = pair("EUR", "USD");
            let mut seen: BTreeMap<(String, u64), Option<String>> = BTreeMap::new();
            for _ in 0..40 {
                let date = rng.date(24).format("%Y-%m-%d").to_string();
                let at = rng.below(tx + 2);
                let got = book.rate(&sel, &date, at).ok().map(|q| q.version);
                assert_eq!(got.as_deref(), oracle(&edits, &date, at), "{date} at {at}");
                seen.insert((date, at), got);
            }
            // records at later transaction times never change what was known before
            let later = tx + 5;
            book.record(later, rate("s", "EUR", "USD", "9.9"), "2026-01-01", "late")
                .unwrap();
            for ((date, at), was) in seen {
                assert_eq!(book.rate(&sel, &date, at).ok().map(|q| q.version), was);
            }
            // the quotes of a series never overlap in valid time
            let quotes = book.known_at(later);
            for w in quotes.windows(2) {
                assert_eq!(w[0].valid_to.as_deref(), Some(w[1].valid_from.as_str()));
            }
        }
    }
}
