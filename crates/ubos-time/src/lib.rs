//! `ubos-time` — engine layer (UBS-ARC-07 §2).
//!
//! Bitemporal timelines, `asof`/`time` resolution, retroactive impact.
//! Allowed dependencies: `ubos-version` (AR-031).
//!
//! PH-0 prototype scope: the timeline algebra of STD-TIME-011 (DEC-032 reading of
//! `set_period`), the changed valid range of an operation (STD-TIME-030) and as-of
//! resolution (STD-TIME-012) (DSN-DVM-302, -303, -305).

pub use ubos_version::{Segment, at, lt};

/// A timeline operation failed; the code is the registered error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineError(pub &'static str);

fn min_end<'a>(a: Option<&'a str>, b: Option<&'a str>) -> Option<&'a str> {
    if lt(b, a) { b } else { a }
}

/// Remove the valid-time interval `[a, b)` from a timeline, splitting segments.
pub fn remove(tl: &[Segment], a: &str, b: Option<&str>) -> Vec<Segment> {
    let mut out = Vec::new();
    for s in tl {
        if s.from.as_str() < a {
            out.push(Segment {
                from: s.from.clone(),
                to: min_end(s.to.as_deref(), Some(a)).map(str::to_string),
                version: s.version.clone(),
            });
        }
        if let Some(b) = b
            && lt(Some(b), s.to.as_deref())
        {
            let from = if s.from.as_str() < b {
                b
            } else {
                s.from.as_str()
            };
            out.push(Segment {
                from: from.to_string(),
                to: s.to.clone(),
                version: s.version.clone(),
            });
        }
    }
    out.retain(|s| lt(Some(&s.from), s.to.as_deref()));
    out
}

fn next_start(tl: &[Segment], a: &str) -> Option<String> {
    tl.iter()
        .filter(|s| s.from.as_str() > a)
        .map(|s| s.from.clone())
        .min()
}

/// Apply a timeline operation of STD-TIME-011 with version `v`.
pub fn apply(
    tl: &[Segment],
    kind: &str,
    v: &str,
    a: &str,
    b: Option<&str>,
) -> Result<Vec<Segment>, TimelineError> {
    let seg = |from: &str, to: Option<&str>| Segment {
        from: from.to_string(),
        to: to.map(str::to_string),
        version: v.to_string(),
    };
    if kind == "create" {
        if !tl.is_empty() {
            return Err(TimelineError("TXN.ALREADY_EXISTS"));
        }
        return Ok(vec![seg(a, None)]);
    }
    if tl.is_empty() {
        return Err(TimelineError("QRY.NOT_FOUND_AT_TIME"));
    }
    let mut out = match kind {
        "supersede_from" => {
            let e = next_start(tl, a);
            let mut o = remove(tl, a, e.as_deref());
            o.push(seg(a, e.as_deref()));
            o
        }
        "set_period" => {
            if !lt(Some(a), b) {
                return Err(TimelineError("MODEL.INVALID_VALUE"));
            }
            for s in tl {
                let intersects = lt(Some(&s.from), b) && lt(Some(a), s.to.as_deref());
                let inside = s.from.as_str() >= a && !lt(b, s.to.as_deref());
                if intersects && !inside {
                    return Err(TimelineError("TIME.OVERLAPPING_VALIDITY"));
                }
            }
            let mut o = remove(tl, a, b);
            o.push(seg(a, b));
            o
        }
        "replace_future" => {
            let mut o = remove(tl, a, None);
            o.push(seg(a, None));
            o
        }
        "end" => remove(tl, a, None),
        _ => return Err(TimelineError("MODEL.INVALID_VALUE")),
    };
    out.sort_by(|x, y| x.from.cmp(&y.from));
    Ok(out)
}

/// Valid range `[from, to)` changed by an operation (STD-TIME-030, STD-TIME-031).
pub fn range(tl: &[Segment], kind: &str, a: &str, b: Option<&str>) -> (String, Option<String>) {
    match kind {
        "supersede_from" => (a.to_string(), next_start(tl, a)),
        "set_period" => (a.to_string(), b.map(str::to_string)),
        _ => (a.to_string(), None),
    }
}
