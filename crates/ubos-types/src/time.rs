//! Dates, times, instants, durations and local-time conversion (STD-FND-024).

use crate::error::{BpaError, invalid};
use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use regex::Regex;
use std::sync::OnceLock;

fn re(key: usize) -> &'static Regex {
    static R: OnceLock<[Regex; 4]> = OnceLock::new();
    &R.get_or_init(|| {
        [
            Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$").expect("date"),
            Regex::new(r"^([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\.[0-9]{6})?$").expect("time"),
            Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]\.[0-9]{6}Z$").expect("instant"),
            Regex::new(r"^P([0-9]+Y)?([0-9]+M)?([0-9]+W)?([0-9]+D)?(T([0-9]+H)?([0-9]+M)?([0-9]+(\.[0-9]{1,6})?S)?)?$")
                .expect("duration"),
        ]
    })[key]
}

/// Parse a calendar date `YYYY-MM-DD` (proleptic Gregorian, years 0001–9999).
pub fn parse_date(s: &str) -> Result<NaiveDate, BpaError> {
    if !re(0).is_match(s) {
        return Err(invalid("not a date"));
    }
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| invalid("not a calendar date"))?;
    if s.starts_with("0000") {
        return Err(invalid("not a calendar date"));
    }
    Ok(d)
}

/// True for a canonical time of day.
pub fn is_time(s: &str) -> bool {
    re(1).is_match(s)
}

/// Parse a canonical instant (UTC, `Z`, six fraction digits).
pub fn parse_instant(s: &str) -> Result<DateTime<Utc>, BpaError> {
    if !re(2).is_match(s) {
        return Err(invalid("not a canonical instant"));
    }
    parse_date(&s[..10])?;
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.6fZ")
        .map(|n| n.and_utc())
        .map_err(|_| invalid("not a canonical instant"))
}

/// Canonical text of an instant.
pub fn format_instant(t: &DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// True for an ISO 8601 duration.
pub fn is_duration(s: &str) -> bool {
    if s == "P" || !re(3).is_match(s) {
        return false;
    }
    match s.find('T') {
        Some(i) => s[i + 1..].starts_with(|c: char| c.is_ascii_digit()),
        None => true,
    }
}

/// Local date and time in a named zone to an instant (STD-FND-024): a non-existent local
/// time resolves to the transition instant after the gap; a repeated one to the earlier
/// instant unless `disambiguation` is `later`.
pub fn local_to_instant(
    date: &str,
    time: &str,
    tz: &str,
    disambiguation: Option<&str>,
) -> Result<String, BpaError> {
    let d = parse_date(date)?;
    if !is_time(time) {
        return Err(invalid("not a time"));
    }
    let t = NaiveTime::parse_from_str(time, "%H:%M:%S%.f").map_err(|_| invalid("not a time"))?;
    let zone: Tz = tz.parse().map_err(|_| invalid("unknown time zone"))?;
    let naive = d.and_time(t);
    let utc = match zone.from_local_datetime(&naive) {
        LocalResult::Single(x) => x.with_timezone(&Utc),
        LocalResult::Ambiguous(a, b) => {
            let (early, late) = if a < b { (a, b) } else { (b, a) };
            if disambiguation == Some("later") {
                late
            } else {
                early
            }
            .with_timezone(&Utc)
        }
        LocalResult::None => {
            // first instant whose local time is at or after the requested one
            let mut lo = naive.and_utc() - Duration::hours(15);
            let mut hi = naive.and_utc() + Duration::hours(15);
            while hi - lo > Duration::microseconds(1) {
                let mid = lo + (hi - lo) / 2;
                if mid.with_timezone(&zone).naive_local() >= naive {
                    hi = mid
                } else {
                    lo = mid
                }
            }
            hi
        }
    };
    Ok(format_instant(&utc))
}
