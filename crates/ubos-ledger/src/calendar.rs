//! Business calendars and cut-offs (FR-LOC-041, FR-LOC-042, DSN-DVM-447).
//!
//! A calendar is the L1 calendar value `{weekend, holidays}` of STD-EXPR-005: ISO weekdays
//! (Monday = 1) and dates that are not business days. The union of calendars has the
//! weekend days and holidays of all of them. Business-day arithmetic follows the L1
//! functions: steps exclude the start date, and a calendar without a business day within
//! ten years is an error. A cut-off is a local time in a zone; an instant after the
//! cut-off of its local date, or on a day that is not a business day, belongs to the next
//! business day.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ubos_types::BpaError;
use ubos_types::error::invalid;
use ubos_types::time::{format_instant, is_time, local_to_instant, parse_date, parse_instant};

/// The days a search for a business day looks at before it gives up (as the L1 functions).
pub const MAX_STEPS: u32 = 3660;

/// A business calendar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Calendar {
    weekend: BTreeSet<u32>,
    holidays: BTreeSet<NaiveDate>,
}

fn weekdays(v: &Value) -> Result<BTreeSet<u32>, BpaError> {
    let list = v
        .as_array()
        .ok_or_else(|| invalid("weekend is a list of ISO weekdays"))?;
    list.iter()
        .map(|d| {
            d.as_u64()
                .filter(|n| (1..=7).contains(n))
                .map(|n| n as u32)
                .ok_or_else(|| invalid("a weekday is 1 (Monday) to 7 (Sunday)"))
        })
        .collect()
}

fn dates(v: &Value) -> Result<BTreeSet<NaiveDate>, BpaError> {
    let list = v
        .as_array()
        .ok_or_else(|| invalid("holidays is a list of dates"))?;
    list.iter()
        .map(|d| parse_date(d.as_str().ok_or_else(|| invalid("a holiday is a date"))?))
        .collect()
}

fn present<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.get(k).filter(|x| !x.is_null())
}

impl Calendar {
    /// A calendar of weekend days and holidays.
    pub fn new(
        weekend: impl IntoIterator<Item = u32>,
        holidays: impl IntoIterator<Item = NaiveDate>,
    ) -> Result<Calendar, BpaError> {
        let weekend: BTreeSet<u32> = weekend.into_iter().collect();
        if weekend.iter().any(|d| !(1..=7).contains(d)) {
            return Err(invalid("a weekday is 1 (Monday) to 7 (Sunday)"));
        }
        Ok(Calendar {
            weekend,
            holidays: holidays.into_iter().collect(),
        })
    }

    /// The calendar of an L1 calendar value; without `weekend` the weekend is Saturday
    /// and Sunday, as the L1 functions read it.
    pub fn of(v: &Value) -> Result<Calendar, BpaError> {
        if !v.is_object() {
            return Err(invalid("a calendar is {weekend, holidays}"));
        }
        Ok(Calendar {
            weekend: match present(v, "weekend") {
                Some(w) => weekdays(w)?,
                None => BTreeSet::from([6, 7]),
            },
            holidays: match present(v, "holidays") {
                Some(h) => dates(h)?,
                None => BTreeSet::new(),
            },
        })
    }

    /// The L1 calendar value `{weekend, holidays}`, both in order.
    pub fn to_value(&self) -> Value {
        json!({
            "weekend": self.weekend.iter().collect::<Vec<_>>(),
            "holidays": self.holidays.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect::<Vec<_>>(),
        })
    }

    /// The weekend days.
    pub fn weekend(&self) -> &BTreeSet<u32> {
        &self.weekend
    }

    /// The holidays.
    pub fn holidays(&self) -> &BTreeSet<NaiveDate> {
        &self.holidays
    }

    /// The union with another calendar: a day is a business day only in both.
    pub fn union(&self, other: &Calendar) -> Calendar {
        Calendar {
            weekend: self.weekend.union(&other.weekend).copied().collect(),
            holidays: self.holidays.union(&other.holidays).copied().collect(),
        }
    }

    /// The union of calendars; of none, the calendar in which every day is a business day.
    pub fn union_all<'a>(calendars: impl IntoIterator<Item = &'a Calendar>) -> Calendar {
        calendars
            .into_iter()
            .fold(Calendar::default(), |acc, c| acc.union(c))
    }

    /// True when a date is neither a weekend day nor a holiday.
    pub fn is_business_day(&self, d: NaiveDate) -> bool {
        !self.weekend.contains(&d.weekday().number_from_monday()) && !self.holidays.contains(&d)
    }

    fn step(&self, mut d: NaiveDate, dir: i64) -> Result<NaiveDate, BpaError> {
        for _ in 0..MAX_STEPS {
            d = d
                .checked_add_signed(Duration::days(dir))
                .ok_or_else(|| invalid("date out of range"))?;
            if self.is_business_day(d) {
                return Ok(d);
            }
        }
        Err(invalid("the calendar has no business day within ten years"))
    }

    /// The first business day after a date.
    pub fn next_business_day(&self, d: NaiveDate) -> Result<NaiveDate, BpaError> {
        self.step(d, 1)
    }

    /// The last business day before a date.
    pub fn previous_business_day(&self, d: NaiveDate) -> Result<NaiveDate, BpaError> {
        self.step(d, -1)
    }

    /// The date `n` business days after a date (before it for a negative `n`); the start
    /// date does not count, and `n = 0` gives the date itself.
    pub fn add_business_days(&self, mut d: NaiveDate, n: i64) -> Result<NaiveDate, BpaError> {
        for _ in 0..n.unsigned_abs() {
            d = self.step(d, n.signum())?;
        }
        Ok(d)
    }

    /// A date itself when it is a business day, else the next business day.
    pub fn on_or_after(&self, d: NaiveDate) -> Result<NaiveDate, BpaError> {
        if self.is_business_day(d) {
            Ok(d)
        } else {
            self.next_business_day(d)
        }
    }
}

/// A cut-off rule: a local time of day in a time zone (FR-LOC-042).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cutoff {
    /// The local time, `HH:MM:SS` with optional microseconds.
    pub time: String,
    /// The IANA time zone.
    pub tz: String,
}

impl Cutoff {
    /// A cut-off of a canonical time of day and a known zone.
    pub fn new(time: &str, tz: &str) -> Result<Cutoff, BpaError> {
        if !is_time(time) {
            return Err(invalid("a cut-off time is a time of day"));
        }
        tz.parse::<Tz>().map_err(|_| invalid("unknown time zone"))?;
        Ok(Cutoff {
            time: time.to_string(),
            tz: tz.to_string(),
        })
    }

    fn zone(&self) -> Tz {
        self.tz.parse().expect("checked by new")
    }

    /// The cut-off instant of a date: the cut-off time in the zone on the date, or on the
    /// next business day when the date is not one (the L1 `cutoff`).
    pub fn instant(&self, d: NaiveDate, cal: &Calendar) -> Result<DateTime<Utc>, BpaError> {
        let d = cal.on_or_after(d)?;
        let s = local_to_instant(
            &d.format("%Y-%m-%d").to_string(),
            &self.time,
            &self.tz,
            Some("earlier"),
        )?;
        parse_instant(&s)
    }

    /// The cut-off instant of a date as canonical text.
    pub fn instant_text(&self, d: NaiveDate, cal: &Calendar) -> Result<String, BpaError> {
        self.instant(d, cal).map(|t| format_instant(&t))
    }

    /// The business date an instant belongs to: its local date in the zone when that is a
    /// business day and the instant is not after the cut-off, else the next business day.
    pub fn effective_date(
        &self,
        at: &DateTime<Utc>,
        cal: &Calendar,
    ) -> Result<NaiveDate, BpaError> {
        let local = at.with_timezone(&self.zone()).date_naive();
        if cal.is_business_day(local) && *at <= self.instant(local, cal)? {
            Ok(local)
        } else {
            cal.next_business_day(local)
        }
    }
}

/// What a stored calendar declares (DSN-DVM-447): its own weekend days and holidays, the
/// calendars it is the union of, and its cut-off rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The days the calendar declares itself.
    pub own: Calendar,
    /// The references of the calendars it also includes.
    pub union: Vec<String>,
    /// The cut-off rule, when declared.
    pub cutoff: Option<Cutoff>,
}

impl Declaration {
    /// The declaration of a stored body `{weekend?, holidays?, union?, cutoff_time?,
    /// cutoff_tz?}`. A calendar without `weekend` has Saturday and Sunday, unless it is a
    /// union: then the weekend comes from its members. `cutoff_time` and `cutoff_tz` are
    /// declared together.
    pub fn of(body: &Value) -> Result<Declaration, BpaError> {
        let union: Vec<String> = match present(body, "union") {
            Some(u) => u
                .as_array()
                .ok_or_else(|| invalid("union is a list of calendar references"))?
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| invalid("union is a list of calendar references"))
                })
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
        };
        let mut own = Calendar::of(&json!({
            "weekend": body.get("weekend").cloned().unwrap_or(Value::Null),
            "holidays": body.get("holidays").cloned().unwrap_or(Value::Null),
        }))?;
        if !union.is_empty() && present(body, "weekend").is_none() {
            own.weekend.clear();
        }
        let cutoff = match (
            present(body, "cutoff_time").and_then(Value::as_str),
            present(body, "cutoff_tz").and_then(Value::as_str),
        ) {
            (Some(t), Some(z)) => Some(Cutoff::new(t, z)?),
            (None, None) => None,
            _ => return Err(invalid("a cut-off has a time and a time zone")),
        };
        Ok(Declaration { own, union, cutoff })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Rng;

    fn date(s: &str) -> NaiveDate {
        parse_date(s).expect("date")
    }

    fn at(s: &str) -> DateTime<Utc> {
        parse_instant(s).expect("instant")
    }

    /// US market holidays of late 2026 as test data.
    fn market() -> Calendar {
        Calendar::of(&json!({"weekend": [6, 7], "holidays": ["2026-11-26", "2026-12-25"]})).unwrap()
    }

    #[test]
    fn settlement_skips_a_holiday() {
        // FR-LOC-041 acceptance 1: T+2 from the day before Thanksgiving (Wed 25 Nov 2026)
        // skips Thursday 26 and the weekend: Friday 27, then Monday 30
        let cal = market();
        assert_eq!(
            cal.add_business_days(date("2026-11-25"), 2).unwrap(),
            date("2026-11-30")
        );
        assert!(!cal.is_business_day(date("2026-11-26")));
        assert!(cal.is_business_day(date("2026-11-27")));
        assert_eq!(
            cal.previous_business_day(date("2026-11-30")).unwrap(),
            date("2026-11-27")
        );
        assert_eq!(
            cal.add_business_days(date("2026-11-30"), -2).unwrap(),
            date("2026-11-25")
        );
        assert_eq!(
            cal.add_business_days(date("2026-11-28"), 0).unwrap(),
            date("2026-11-28")
        );
    }

    #[test]
    fn calendars_compose_by_union() {
        let exchange = market();
        let bank = Calendar::of(&json!({"holidays": ["2026-11-11", "2026-11-26"]})).unwrap();
        let both = exchange.union(&bank);
        assert_eq!(
            both.to_value(),
            json!({"weekend": [6, 7], "holidays": ["2026-11-11", "2026-11-26", "2026-12-25"]})
        );
        assert!(exchange.is_business_day(date("2026-11-11")));
        assert!(!both.is_business_day(date("2026-11-11")));
        assert_eq!(Calendar::union_all([&exchange, &bank]), both);
        // a Friday–Saturday weekend joined with a Saturday–Sunday one
        let gulf = Calendar::of(&json!({"weekend": [5, 6]})).unwrap();
        assert_eq!(
            gulf.union(&exchange).to_value()["weekend"],
            json!([5, 6, 7])
        );
        assert!(Calendar::union_all([]).is_business_day(date("2026-11-28")));
    }

    #[test]
    fn calendar_values_are_checked() {
        assert_eq!(
            Calendar::of(&json!({})).unwrap().to_value(),
            json!({"weekend": [6, 7], "holidays": []})
        );
        assert_eq!(
            Calendar::of(&json!({"weekend": []})).unwrap().to_value()["weekend"],
            json!([])
        );
        for bad in [
            json!({"weekend": [0]}),
            json!({"weekend": [8]}),
            json!({"weekend": "sat"}),
            json!({"holidays": ["2026-02-30"]}),
            json!({"holidays": [20261126]}),
            json!([6, 7]),
        ] {
            assert_eq!(Calendar::of(&bad).unwrap_err().code, "MODEL.INVALID_VALUE");
        }
        let closed = Calendar::new(1..=7, []).unwrap();
        assert_eq!(
            closed
                .next_business_day(date("2026-01-01"))
                .unwrap_err()
                .code,
            "MODEL.INVALID_VALUE"
        );
        assert!(Calendar::new([9], []).is_err());
    }

    #[test]
    fn an_instant_after_the_cut_off_moves_to_the_next_business_day() {
        // FR-LOC-042 acceptance 1: a 12:00 New York cut-off; Tuesday 24 November 2026 is
        // in winter time (UTC-5)
        let cal = market();
        let cut = Cutoff::new("12:00:00", "America/New_York").unwrap();
        assert_eq!(
            cut.instant_text(date("2026-11-24"), &cal).unwrap(),
            "2026-11-24T17:00:00.000000Z"
        );
        let on = |s: &str| cut.effective_date(&at(s), &cal).unwrap();
        assert_eq!(on("2026-11-24T16:55:00.000000Z"), date("2026-11-24"));
        assert_eq!(on("2026-11-24T17:00:00.000000Z"), date("2026-11-24"));
        // 12:05 in New York
        assert_eq!(on("2026-11-24T17:05:00.000000Z"), date("2026-11-25"));
        // after the cut-off on the day before the holiday: Friday
        assert_eq!(on("2026-11-25T18:00:00.000000Z"), date("2026-11-27"));
        // on the holiday and on a Saturday, at any time
        assert_eq!(on("2026-11-26T14:00:00.000000Z"), date("2026-11-27"));
        assert_eq!(on("2026-11-28T14:00:00.000000Z"), date("2026-11-30"));
        // the local date counts: 03:00 UTC on Wednesday is still Tuesday evening in New York
        assert_eq!(on("2026-11-25T03:00:00.000000Z"), date("2026-11-25"));
        // the cut-off of a day that is not a business day is that of the next one
        assert_eq!(
            cut.instant_text(date("2026-11-26"), &cal).unwrap(),
            "2026-11-27T17:00:00.000000Z"
        );
        // summer time (UTC-4)
        assert_eq!(
            cut.instant_text(date("2026-07-01"), &cal).unwrap(),
            "2026-07-01T16:00:00.000000Z"
        );
        assert!(Cutoff::new("noon", "America/New_York").is_err());
        assert!(Cutoff::new("12:00:00", "Mars/Olympus").is_err());
    }

    #[test]
    fn declarations_of_stored_calendars() {
        let plain = Declaration::of(&json!({"name": "x", "holidays": ["2026-12-25"]})).unwrap();
        assert_eq!(plain.own.to_value()["weekend"], json!([6, 7]));
        assert!(plain.union.is_empty() && plain.cutoff.is_none());
        let joined = Declaration::of(&json!({"union": ["ubos://a/main/BusinessCalendar/x"],
            "cutoff_time": "12:00:00", "cutoff_tz": "America/New_York"}))
        .unwrap();
        assert!(joined.own.weekend().is_empty());
        assert_eq!(joined.union.len(), 1);
        assert_eq!(joined.cutoff.unwrap().tz, "America/New_York");
        let own = Declaration::of(&json!({"union": ["u"], "weekend": [5]})).unwrap();
        assert_eq!(own.own.to_value()["weekend"], json!([5]));
        for bad in [
            json!({"cutoff_time": "12:00:00"}),
            json!({"cutoff_tz": "UTC"}),
            json!({"cutoff_time": "12:00:00", "cutoff_tz": "Nowhere"}),
            json!({"union": "x"}),
            json!({"union": [1]}),
        ] {
            assert_eq!(
                Declaration::of(&bad).unwrap_err().code,
                "MODEL.INVALID_VALUE"
            );
        }
    }

    fn random_calendar(rng: &mut Rng) -> Calendar {
        let weekend: Vec<u32> = (1..=7).filter(|_| rng.below(4) == 0).take(5).collect();
        let holidays: Vec<NaiveDate> = (0..rng.below(12)).map(|_| rng.date(120)).collect();
        Calendar::new(weekend, holidays).unwrap()
    }

    #[test]
    fn property_business_day_arithmetic() {
        let mut rng = Rng(41);
        for _ in 0..300 {
            let cal = random_calendar(&mut rng);
            let d = rng.date(120);
            let n = rng.below(15) as i64;
            let fwd = cal.add_business_days(d, n).unwrap();
            // the result of a step is a business day, strictly after the start
            if n > 0 {
                assert!(cal.is_business_day(fwd) && fwd > d);
                // exactly n business days lie in (d, fwd]
                let count = d
                    .iter_days()
                    .skip(1)
                    .take_while(|x| *x <= fwd)
                    .filter(|x| cal.is_business_day(*x))
                    .count() as i64;
                assert_eq!(count, n);
                // stepping is additive
                let k = rng.below(n as u64 + 1) as i64;
                assert_eq!(
                    cal.add_business_days(cal.add_business_days(d, k).unwrap(), n - k)
                        .unwrap(),
                    fwd
                );
                // from a business day, going back returns to it
                if cal.is_business_day(d) {
                    assert_eq!(cal.add_business_days(fwd, -n).unwrap(), d);
                }
            } else {
                assert_eq!(fwd, d);
            }
            let next = cal.next_business_day(d).unwrap();
            assert_eq!(cal.previous_business_day(next).unwrap(), {
                if cal.is_business_day(d) {
                    d
                } else {
                    cal.previous_business_day(d).unwrap()
                }
            });
        }
    }

    #[test]
    fn property_union() {
        let mut rng = Rng(43);
        for _ in 0..200 {
            let (a, b, c) = (
                random_calendar(&mut rng),
                random_calendar(&mut rng),
                random_calendar(&mut rng),
            );
            // commutative, associative, idempotent
            assert_eq!(a.union(&b), b.union(&a));
            assert_eq!(a.union(&b).union(&c), a.union(&b.union(&c)));
            assert_eq!(a.union(&a), a);
            let u = Calendar::union_all([&a, &b, &c]);
            for _ in 0..20 {
                let d = rng.date(120);
                // a business day of the union is one of every member
                assert_eq!(
                    u.is_business_day(d),
                    a.is_business_day(d) && b.is_business_day(d) && c.is_business_day(d)
                );
                // a union never moves a settlement date earlier
                assert!(u.add_business_days(d, 3).unwrap() >= a.add_business_days(d, 3).unwrap());
            }
            // the value round-trips
            assert_eq!(Calendar::of(&u.to_value()).unwrap(), u);
        }
    }

    #[test]
    fn property_cut_off() {
        let mut rng = Rng(47);
        let zones = ["America/New_York", "Europe/London", "Asia/Tokyo", "UTC"];
        for _ in 0..300 {
            let cal = random_calendar(&mut rng);
            let cut = Cutoff::new(
                &format!("{:02}:{:02}:00", rng.below(24), rng.below(60)),
                zones[rng.below(4) as usize],
            )
            .unwrap();
            let t = at("2026-01-01T00:00:00.000000Z")
                + Duration::minutes(rng.below(120 * 24 * 60) as i64);
            let eff = cut.effective_date(&t, &cal).unwrap();
            let local = t.with_timezone(&cut.zone()).date_naive();
            // the effective date is a business day, never before the local date, and the
            // instant is not after its cut-off
            assert!(cal.is_business_day(eff) && eff >= local);
            assert!(t <= cut.instant(eff, &cal).unwrap());
            // it is the first such day
            if eff > local {
                assert!(!cal.is_business_day(local) || t > cut.instant(local, &cal).unwrap());
                assert_eq!(eff, cal.next_business_day(local).unwrap());
            }
            // later instants never get an earlier date
            let later = t + Duration::minutes(rng.below(3000) as i64);
            assert!(cut.effective_date(&later, &cal).unwrap() >= eff);
        }
    }
}
