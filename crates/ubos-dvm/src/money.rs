//! Money and calendars in the kernel (DSN-DVM-409, DSN-DVM-410, DSN-DVM-447; PH-2 step
//! 3a, DEC-098): exchange rates and business calendars are Definition objects of the
//! genesis classes `FxRate` and `BusinessCalendar`, read at the coordinates of a read
//! (`branch`, a commit, the valid time `asof`), so that a conversion or a business-day
//! computation is reproducible from its evidence.
//!
//! - The checks of the bodies of both classes at commit.
//! - `Snapshot`: the calendars (unions resolved) and the rates valid at a coordinate, by
//!   canonical URI. It is the reference data of L1 evaluations in the kernel
//!   (`ubos_expr::with_reference`): `convert(m, cur, rate_ref)` and the calendar functions
//!   take the URI of a stored object, and the snapshot records the versions used.
//! - The host operations `currencies`, `fx_rate`, `convert`, `calendar`, `business_day`
//!   and `ledger_totals`. They are kernel host API, not adapter operations of
//!   STD-CONF-005.
//!
//! The mechanisms are those of `ubos-ledger`; nothing here knows what the amounts mean.

use crate::relations::uri_of;
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use ubos_expr::{ReferenceData, Val};
use ubos_ledger::calendar::{Calendar, Cutoff, Declaration};
use ubos_ledger::fx::{self, Quote, Rate, Selector};
use ubos_ledger::money::Totals;
use ubos_ledger::{currency, money};
use ubos_model::Registry;
use ubos_types::decimal::{Decimal, Rounding};
use ubos_types::error::invalid;
use ubos_types::time::{parse_date, parse_instant};
use ubos_types::{BpaError, canonical};
use ubos_version::{Repo, State, at};

/// The genesis class of exchange rates (DSN-DVM-410).
pub const RATE_CLASS: &str = "FxRate";
/// The genesis class of business calendars (DSN-DVM-447).
pub const CALENDAR_CLASS: &str = "BusinessCalendar";
/// The deepest nesting of calendar unions.
const MAX_UNION_DEPTH: usize = 16;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn is_a(reg: &Registry, class: &str, base: &str) -> bool {
    reg.class(class)
        .is_ok_and(|c| c.linearization.iter().any(|l| l == base))
}

/// The checks of a rate or a calendar body at commit: the currencies of a rate are
/// listed and differ and the rate is greater than zero; the weekdays, the holidays and
/// the cut-off of a calendar are well formed.
pub(crate) fn check_definition(reg: &Registry, class: &str, body: &Value) -> Result<(), BpaError> {
    if is_a(reg, class, RATE_CLASS) {
        Rate::of(body)?;
    } else if is_a(reg, class, CALENDAR_CLASS) {
        Declaration::of(body)?;
    }
    Ok(())
}

/// A calendar at a coordinate: its days with those of the calendars it is the union of,
/// its cut-off, and the versions it was resolved from.
#[derive(Debug, Clone)]
pub(crate) struct StoredCalendar {
    pub calendar: Calendar,
    pub cutoff: Option<Cutoff>,
    /// `(object, version)` of the calendar and of every calendar it includes.
    pub versions: BTreeSet<(String, String)>,
}

/// A rate at a coordinate.
#[derive(Debug, Clone)]
pub(crate) struct StoredRate {
    pub quote: Quote,
    pub oid: String,
    pub uri: String,
}

impl StoredRate {
    /// The reference a conversion records: the rate, its series, the object and the
    /// version that hold it and the valid period of that version.
    pub(crate) fn reference(&self) -> Value {
        let r = &self.quote.rate;
        json!({"rate": r.rate.format(None), "from": r.key.from, "to": r.key.to,
            "source": r.key.source, "rate_type": r.key.rate_type, "fixing_time": r.fixing_time,
            "object_id": self.oid, "uri": self.uri, "version": self.quote.version,
            "valid": {"from": self.quote.valid_from, "to": self.quote.valid_to}})
    }
}

/// One declared calendar before its union is resolved.
struct Declared {
    oid: String,
    version: String,
    decl: Result<Declaration, BpaError>,
}

/// The calendars and rates valid at a coordinate, by canonical URI.
#[derive(Debug, Default)]
pub(crate) struct Snapshot {
    calendars: BTreeMap<String, Result<StoredCalendar, BpaError>>,
    rates: BTreeMap<String, StoredRate>,
    /// The URIs of the calendars and rates of the state, valid at the coordinate or not.
    known: BTreeSet<String>,
    used: RefCell<BTreeSet<(String, String)>>,
}

fn resolve_union(
    uri: &str,
    declared: &BTreeMap<String, Declared>,
    path: &mut Vec<String>,
) -> Result<StoredCalendar, BpaError> {
    let Some(d) = declared.get(uri) else {
        return Err(BpaError::new(
            "MODEL.REFERENCE_INVALID",
            format!("no calendar {uri} at the coordinates"),
        ));
    };
    if path.iter().any(|p| p == uri) || path.len() >= MAX_UNION_DEPTH {
        return Err(invalid(format!("the union of calendar {uri} does not end")));
    }
    let decl = d.decl.clone()?;
    let mut out = StoredCalendar {
        calendar: decl.own,
        cutoff: decl.cutoff,
        versions: BTreeSet::from([(d.oid.clone(), d.version.clone())]),
    };
    path.push(uri.to_string());
    for member in &decl.union {
        let m = resolve_union(member, declared, path)?;
        out.calendar = out.calendar.union(&m.calendar);
        out.versions.extend(m.versions);
    }
    path.pop();
    Ok(out)
}

impl Snapshot {
    /// The snapshot of the calendar and rate objects of a state: of each, the version
    /// valid at `asof` (else at the default valid time of its class); ended objects and
    /// objects not valid there are left out.
    pub(crate) fn build(
        repo: &mut Repo<'_>,
        st: &State,
        authority: &str,
        asof: Option<&str>,
        default_valid: &dyn Fn(&str) -> String,
    ) -> Result<Snapshot, BpaError> {
        let reg = repo.reg;
        let mut out = Snapshot::default();
        let mut segs = Vec::new();
        for (oid, o) in &st.objects {
            let rate = is_a(reg, &o.class, RATE_CLASS);
            if !rate && !is_a(reg, &o.class, CALENDAR_CLASS) {
                continue;
            }
            let uri = uri_of(authority, &o.class, &o.slug);
            out.known.insert(uri.clone());
            let a = asof
                .map(str::to_string)
                .unwrap_or_else(|| default_valid(&o.class));
            if let Some(seg) = at(&o.timeline, &a) {
                segs.push((oid.clone(), uri, rate, seg.clone()));
            }
        }
        let hashes: Vec<String> = segs.iter().map(|x| x.3.version.clone()).collect();
        let versions = repo.versions(&hashes)?;
        let mut declared: BTreeMap<String, Declared> = BTreeMap::new();
        for (oid, uri, rate, seg) in segs {
            let v = match versions.get(&seg.version) {
                Some(v) => v.clone(),
                None => repo.version(&seg.version)?,
            };
            if v["tombstone"] == json!(true) {
                continue;
            }
            if rate {
                // a body that no longer passes the checks (a merge of fields) is no rate
                if let Ok(r) = Rate::of(&v["body"]) {
                    let quote = Quote {
                        rate: r,
                        valid_from: seg.from.clone(),
                        valid_to: seg.to.clone(),
                        version: seg.version.clone(),
                    };
                    out.rates
                        .insert(uri.clone(), StoredRate { quote, oid, uri });
                }
            } else {
                declared.insert(
                    uri,
                    Declared {
                        oid,
                        version: seg.version.clone(),
                        decl: Declaration::of(&v["body"]),
                    },
                );
            }
        }
        for uri in declared.keys() {
            let c = resolve_union(uri, &declared, &mut Vec::new());
            out.calendars.insert(uri.clone(), c);
        }
        Ok(out)
    }

    fn note(&self, versions: impl IntoIterator<Item = (String, String)>) {
        self.used.borrow_mut().extend(versions);
    }

    /// The calendar of a URI, recorded as used; `None` when there is none at the
    /// coordinate.
    pub(crate) fn stored_calendar(&self, uri: &str) -> Result<Option<StoredCalendar>, BpaError> {
        match self.calendars.get(uri) {
            None => Ok(None),
            Some(Err(e)) => Err(e.clone()),
            Some(Ok(c)) => {
                self.note(c.versions.iter().cloned());
                Ok(Some(c.clone()))
            }
        }
    }

    /// The rate object of a URI, recorded as used.
    pub(crate) fn stored_rate(&self, uri: &str) -> Option<StoredRate> {
        let r = self.rates.get(uri)?;
        self.note([(r.oid.clone(), r.quote.version.clone())]);
        Some(r.clone())
    }

    /// The rate of a selector on a date among the rates of the snapshot, recorded as used.
    pub(crate) fn select_rate(
        &self,
        sel: &Selector<'_>,
        date: &str,
    ) -> Result<StoredRate, BpaError> {
        let quotes: Vec<Quote> = self.rates.values().map(|r| r.quote.clone()).collect();
        let q = fx::select(&quotes, sel, date)?;
        let r = self
            .rates
            .values()
            .find(|r| r.quote.version == q.version && r.quote.rate.key == q.rate.key)
            .expect("selected among the rates");
        self.note([(r.oid.clone(), r.quote.version.clone())]);
        Ok(r.clone())
    }

    /// True when the state holds a calendar or a rate object of the URI, valid at the
    /// coordinate or not.
    pub(crate) fn knows(&self, uri: &str) -> bool {
        self.known.contains(uri)
    }

    /// The `(object, version)` pairs used since the last call.
    pub(crate) fn take_used(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.used.borrow_mut())
            .into_iter()
            .collect()
    }
}

impl ReferenceData for Snapshot {
    fn calendar(&self, name: &str) -> Result<Option<Value>, BpaError> {
        Ok(self.stored_calendar(name)?.map(|c| c.calendar.to_value()))
    }

    fn rate(&self, rate_ref: &str) -> Result<Option<Value>, BpaError> {
        Ok(self.stored_rate(rate_ref).map(|r| {
            let k = &r.quote.rate.key;
            json!({"from": k.from, "to": k.to, "rate": r.quote.rate.rate.format(None)})
        }))
    }
}

/// Run `f` with the snapshot, if any, as the reference data of its L1 evaluations.
pub(crate) fn scoped<T>(data: &Option<Rc<Snapshot>>, f: impl FnOnce() -> T) -> T {
    match data {
        Some(d) => ubos_expr::with_reference(d.clone(), f),
        None => f(),
    }
}

fn versions_json(versions: &BTreeSet<(String, String)>) -> Value {
    versions
        .iter()
        .map(|(o, v)| json!({"object_id": o, "version": v}))
        .collect()
}

fn rounding_of(i: &Value) -> Result<Rounding, BpaError> {
    Rounding::parse(s(i, "rounding").unwrap_or("half_even"))
}

/// Rates of an aggregate conversion (FR-QRY-042): the rates given with the request, or the
/// stored rates at the coordinates of the query.
pub(crate) enum Conversion {
    /// `rates: {currency: rate}` given with the request (STD-ISA-081).
    Given(Map<String, Value>),
    /// The stored rates of a source and rate type.
    Stored {
        snapshot: Snapshot,
        source: Option<String>,
        rate_type: Option<String>,
        date: String,
    },
}

/// An explicit conversion of the money values of an aggregate query to one currency.
pub(crate) struct Converter {
    target: String,
    rates: Conversion,
    /// The references of the stored rates used, by source currency.
    used: BTreeMap<String, Value>,
}

impl Converter {
    pub(crate) fn new(target: &str, rates: Conversion) -> Result<Converter, BpaError> {
        currency::currency(target)?;
        Ok(Converter {
            target: target.to_string(),
            rates,
            used: BTreeMap::new(),
        })
    }

    /// A value of an aggregate: money of another currency is converted and rounded
    /// half-even to the minor unit of the target; a currency without a rate is
    /// `MODEL.CURRENCY_MISMATCH`.
    pub(crate) fn value(&mut self, v: Val) -> Result<Val, BpaError> {
        let Val::Money(amount, cur) = v else {
            return Ok(v);
        };
        if cur == self.target {
            return Ok(Val::Money(amount, cur));
        }
        let rate = match &self.rates {
            Conversion::Given(rates) => {
                let r = match rates.get(&cur) {
                    Some(Value::String(r)) => Decimal::parse(r)?,
                    Some(Value::Number(n)) => Decimal::parse(&n.to_string())?,
                    _ => {
                        return Err(BpaError::with_details(
                            "MODEL.CURRENCY_MISMATCH",
                            json!({"from": cur, "to": self.target, "message": "no rate"}),
                        ));
                    }
                };
                Rate {
                    key: fx::RateKey {
                        source: String::new(),
                        from: cur.clone(),
                        to: self.target.clone(),
                        rate_type: String::new(),
                    },
                    rate: r,
                    fixing_time: None,
                }
            }
            Conversion::Stored {
                snapshot,
                source,
                rate_type,
                date,
            } => {
                let sel = Selector {
                    from: &cur,
                    to: &self.target,
                    source: source.as_deref(),
                    rate_type: rate_type.as_deref(),
                };
                let r = snapshot.select_rate(&sel, date)?;
                self.used
                    .entry(cur.clone())
                    .or_insert_with(|| r.reference());
                r.quote.rate
            }
        };
        let converted = fx::convert(&amount, &cur, &self.target, &rate, Rounding::HalfEven)?;
        Ok(Val::Money(converted, self.target.clone()))
    }

    /// The references of the stored rates used, ordered by source currency.
    pub(crate) fn references(&self) -> Vec<Value> {
        self.used.values().cloned().collect()
    }
}

impl Engine {
    /// The calendars and rates at the coordinates of a read: `branch`, the commit that
    /// `time`, `commit`, `tag` or `tx_time` select (else the head) and the valid time
    /// `asof` (else today), with the evaluation evidence.
    pub(crate) fn reference_snapshot(
        &mut self,
        branch: &str,
        input: &Value,
        asof: Option<&str>,
    ) -> Result<(Snapshot, BTreeMap<String, String>), BpaError> {
        let mut sel = json!({});
        for k in ["time", "commit", "tag", "tx_time", "evaluated_at"] {
            if let Some(v) = input.get(k).filter(|v| !v.is_null()) {
                sel[k] = v.clone();
            }
        }
        let view = self.needs_view(branch, &sel)?;
        let now = self.now.clone();
        let reg = &self.registry;
        let default_valid = move |c: &str| match reg.class(c).map(|c| c.valid_time.as_str()) {
            Ok("instant") => now.clone(),
            _ => now[..10].to_string(),
        };
        let authority = self.store.authority();
        let mut repo = Repo::new(self.store.as_mut(), reg);
        let (st, ev) = if view {
            let (v, _) = repo.view(branch, &sel)?;
            (v.state, v.evaluated_at)
        } else {
            let snap = repo.select(branch, &sel)?;
            let c = repo.meta(snap.commit)?;
            let mut st = State::default();
            for base in [RATE_CLASS, CALENDAR_CLASS] {
                for class in reg.subclasses(base) {
                    st.objects
                        .extend(repo.objects_in(branch, c.cseq, Some(&class))?);
                }
            }
            (st, BTreeMap::from([(branch.to_string(), c.commit_id)]))
        };
        let snapshot = Snapshot::build(&mut repo, &st, &authority, asof, &default_valid)?;
        Ok((snapshot, ev))
    }

    /// The snapshot of the calendars and rates of a state a process or a cascade has read
    /// in part: with `lazy` (`branch`, commit sequence) the objects of the two classes are
    /// read from the branch, else the state holds them.
    pub(crate) fn reference_in(
        &mut self,
        st: &State,
        lazy: Option<(String, Option<u64>)>,
        asof: Option<&str>,
    ) -> Result<Snapshot, BpaError> {
        let now = self.now.clone();
        let reg = &self.registry;
        let default_valid = move |c: &str| match reg.class(c).map(|c| c.valid_time.as_str()) {
            Ok("instant") => now.clone(),
            _ => now[..10].to_string(),
        };
        let authority = self.store.authority();
        let mut repo = Repo::new(self.store.as_mut(), reg);
        match lazy {
            Some((branch, Some(cseq))) => {
                let mut part = State::default();
                for base in [RATE_CLASS, CALENDAR_CLASS] {
                    for class in reg.subclasses(base) {
                        part.objects
                            .extend(repo.objects_in(&branch, cseq, Some(&class))?);
                    }
                }
                Snapshot::build(&mut repo, &part, &authority, asof, &default_valid)
            }
            _ => Snapshot::build(&mut repo, st, &authority, asof, &default_valid),
        }
    }

    fn valid_date(&self, i: &Value) -> Result<String, BpaError> {
        let d = s(i, "asof")
            .map(str::to_string)
            .unwrap_or_else(|| self.now[..10].to_string());
        parse_date(d.get(..10).unwrap_or(&d))?;
        Ok(d)
    }

    /// `currencies {code?, date?}` (FR-LOC-021, DSN-DVM-410): the reference record of one
    /// currency, or the records of the currencies in use on a date (of all without one).
    pub fn currencies(&self, i: &Value) -> Result<Value, BpaError> {
        match s(i, "code") {
            Some(code) => Ok(currency::describe(currency::currency(code)?)),
            None => Ok(json!({"currencies": currency::list(s(i, "date"))?,
                "table_version": currency::TABLE_VERSION})),
        }
    }

    /// `fx_rate {branch?, time?, asof?, from, to, source?, rate_type?}` (FR-LOC-022,
    /// DSN-DVM-410): the rate of the pair for the valid date `asof` (else today) as known
    /// at the commit of the read, with the object and version that hold it and the
    /// evaluation evidence. No rate at the coordinates is `MODEL.CURRENCY_MISMATCH`;
    /// several series need `source` and `rate_type`.
    pub fn fx_rate(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let date = self.valid_date(i)?;
        let (from, to) = (
            s(i, "from").ok_or_else(|| invalid("from is a currency"))?,
            s(i, "to").ok_or_else(|| invalid("to is a currency"))?,
        );
        currency::currency(from)?;
        currency::currency(to)?;
        let (snapshot, ev) = self.reference_snapshot(&branch, i, Some(&date))?;
        let sel = Selector {
            from,
            to,
            source: s(i, "source"),
            rate_type: s(i, "rate_type"),
        };
        let mut out = snapshot.select_rate(&sel, &date)?.reference();
        out["asof"] = json!(date);
        out["evaluated_at"] = json!(ev);
        Ok(out)
    }

    /// `convert {amount, to, branch?, time?, asof?, source?, rate_type?, rounding?,
    /// rate_version?}` (STD-FND-023, FR-LEDG-053, DSN-DVM-409, -410): a money value
    /// converted to another currency by the stored rate at the coordinates and rounded
    /// to the minor unit of the target (`half_even` by default). The result names the rate
    /// version used; given back as `rate_version`, the conversion is repeated with that
    /// version whatever was recorded since.
    pub fn convert(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let (amount, from) = currency::parse_money(i.get("amount").unwrap_or(&Value::Null))?;
        let to = s(i, "to").ok_or_else(|| invalid("to is a currency"))?;
        currency::currency(to)?;
        let mode = rounding_of(i)?;
        if from == to {
            return Ok(json!({"amount": currency::money(&amount, to)?, "rate": null}));
        }
        if let Some(vh) = s(i, "rate_version") {
            let well_formed = vh.strip_prefix("sha256:").is_some_and(|h| {
                h.len() == 64
                    && h.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
            if !well_formed {
                return Err(BpaError::new(
                    "MODEL.REFERENCE_INVALID",
                    "rate_version is a version of a rate object",
                ));
            }
            let found =
                Repo::new(self.store.as_mut(), &self.registry).versions(&[vh.to_string()])?;
            let v = found.get(vh).cloned().unwrap_or(Value::Null);
            let class = s(&v, "class").unwrap_or_default();
            if !is_a(&self.registry, class, RATE_CLASS) || v["tombstone"] == json!(true) {
                return Err(BpaError::new(
                    "MODEL.REFERENCE_INVALID",
                    "rate_version is a version of a rate object",
                ));
            }
            let rate = Rate::of(&v["body"])?;
            let converted = fx::convert(&amount, &from, to, &rate, mode)?;
            return Ok(json!({"amount": currency::money(&converted, to)?,
                "rate": {"rate": rate.rate.format(None), "from": rate.key.from, "to": rate.key.to,
                    "source": rate.key.source, "rate_type": rate.key.rate_type,
                    "fixing_time": rate.fixing_time, "object_id": v["object_id"], "version": vh}}));
        }
        let date = self.valid_date(i)?;
        let (snapshot, ev) = self.reference_snapshot(&branch, i, Some(&date))?;
        let sel = Selector {
            from: &from,
            to,
            source: s(i, "source"),
            rate_type: s(i, "rate_type"),
        };
        let r = snapshot.select_rate(&sel, &date)?;
        let converted = fx::convert(&amount, &from, to, &r.quote.rate, mode)?;
        Ok(
            json!({"amount": currency::money(&converted, to)?, "rate": r.reference(),
            "asof": date, "evaluated_at": ev}),
        )
    }

    fn calendar_at(
        &mut self,
        i: &Value,
    ) -> Result<(StoredCalendar, String, BTreeMap<String, String>), BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let uri = s(i, "calendar").ok_or_else(|| invalid("calendar is the URI of a calendar"))?;
        let date = self.valid_date(i)?;
        let (snapshot, ev) = self.reference_snapshot(&branch, i, Some(&date))?;
        match snapshot.stored_calendar(uri)? {
            Some(c) => Ok((c, date, ev)),
            None if snapshot.knows(uri) => Err(BpaError::new(
                "QRY.NOT_FOUND_AT_TIME",
                "no calendar version at asof",
            )),
            None => Err(BpaError::new("IAM.NOT_FOUND", "unknown calendar")),
        }
    }

    /// `calendar {calendar, branch?, time?, asof?}` (FR-LOC-041, DSN-DVM-447): the L1
    /// calendar value of a stored calendar at the coordinates, its union resolved there,
    /// with its cut-off, the versions it was resolved from and the evaluation evidence.
    pub fn calendar(&mut self, i: &Value) -> Result<Value, BpaError> {
        let (c, date, ev) = self.calendar_at(i)?;
        Ok(json!({"calendar": c.calendar.to_value(),
            "cutoff": c.cutoff.as_ref().map(|k| json!({"time": k.time, "tz": k.tz})),
            "versions": versions_json(&c.versions), "asof": date, "evaluated_at": ev}))
    }

    /// `business_day {calendar, fn, date?, days?, at?, cutoff_time?, cutoff_tz?, branch?,
    /// time?, asof?}` (FR-LOC-041, FR-LOC-042, DSN-DVM-447): business-day arithmetic on a
    /// stored calendar at the coordinates. `fn` is `is_business_day`, `add_business_days`
    /// (`days`), `next_business_day`, `previous_business_day`, `cutoff` (the cut-off
    /// instant of `date`) or `effective_date` (the business date the instant `at` belongs
    /// to); the cut-off is the one the calendar declares unless given.
    pub fn business_day(&mut self, i: &Value) -> Result<Value, BpaError> {
        let (c, asof, ev) = self.calendar_at(i)?;
        let cal = &c.calendar;
        let date = || parse_date(s(i, "date").ok_or_else(|| invalid("date is a date"))?);
        let cutoff = || -> Result<Cutoff, BpaError> {
            match (s(i, "cutoff_time"), s(i, "cutoff_tz"), &c.cutoff) {
                (Some(t), Some(z), _) => Cutoff::new(t, z),
                (None, None, Some(k)) => Ok(k.clone()),
                (None, None, None) => Err(invalid("the calendar declares no cut-off")),
                _ => Err(invalid("a cut-off has a time and a time zone")),
            }
        };
        let text = |d: chrono::NaiveDate| json!(d.format("%Y-%m-%d").to_string());
        let value = match s(i, "fn").unwrap_or_default() {
            "is_business_day" => json!(cal.is_business_day(date()?)),
            "add_business_days" => {
                let n = i
                    .get("days")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| invalid("days is an integer"))?;
                text(cal.add_business_days(date()?, n)?)
            }
            "next_business_day" => text(cal.next_business_day(date()?)?),
            "previous_business_day" => text(cal.previous_business_day(date()?)?),
            "cutoff" => json!(cutoff()?.instant_text(date()?, cal)?),
            "effective_date" => {
                let at = parse_instant(s(i, "at").ok_or_else(|| invalid("at is an instant"))?)?;
                text(cutoff()?.effective_date(&at, cal)?)
            }
            _ => return Err(invalid("unknown business-day function")),
        };
        Ok(
            json!({"value": value, "versions": versions_json(&c.versions), "asof": asof,
            "evaluated_at": ev}),
        )
    }

    /// `ledger_totals {class, ledger_key, branch?, time?, asof?}` (DSN-DVM-409): the totals
    /// of the amount fields of a ledger class over the entries of a key at the coordinates.
    /// Money is totalled per currency and never across currencies; a decimal amount field
    /// has one total.
    pub fn ledger_totals(&mut self, i: &Value) -> Result<Value, BpaError> {
        let class = s(i, "class").unwrap_or_default();
        let c = self.registry.class(class)?;
        let Some(ledger) = &c.ledger else {
            return Err(invalid(format!("{class} is not a ledger class")));
        };
        let amounts: Vec<String> = ledger
            .get("amounts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let read = self.read_ledger(i)?;
        let entries = read["entries"].as_array().cloned().unwrap_or_default();
        let mut totals = Map::new();
        for f in &amounts {
            let mut per_currency = Totals::new();
            let mut plain: Option<Decimal> = None;
            for e in &entries {
                match e["body"].get(f) {
                    None | Some(Value::Null) => {}
                    Some(Value::String(d)) => {
                        let d = Decimal::parse(d)?;
                        plain = Some(plain.map_or(d.clone(), |p| p.add(&d)));
                    }
                    Some(m) => per_currency.add_money(m)?,
                }
            }
            let total = match (plain, per_currency.is_empty()) {
                (Some(p), true) => json!(p.format(None)),
                (None, _) => Value::Array(per_currency.to_values()?),
                (Some(_), false) => {
                    return Err(money::mismatch("a decimal amount", "a money amount"));
                }
            };
            totals.insert(f.clone(), total);
        }
        Ok(json!({"totals": totals, "entries": entries.len(),
            "ledger_key": canonical(i.get("ledger_key").unwrap_or(&Value::Null))?,
            "evaluated_at": read["evaluated_at"]}))
    }

    /// The conversion an aggregate query asks for with `convert: {currency, rates}` (the
    /// rates given, STD-ISA-081) or `convert: {currency, source?, rate_type?}` (the stored
    /// rates at the commit and valid date of the query).
    pub(crate) fn converter(
        &mut self,
        convert: &Value,
        branch: &str,
        commit_id: &str,
        asof: Option<&str>,
    ) -> Result<Converter, BpaError> {
        let target = s(convert, "currency").ok_or_else(|| invalid("convert names a currency"))?;
        if let Some(rates) = convert.get("rates").filter(|r| !r.is_null()) {
            let rates = rates
                .as_object()
                .cloned()
                .ok_or_else(|| invalid("rates are {currency: rate}"))?;
            return Converter::new(target, Conversion::Given(rates));
        }
        let date = self.valid_date(&json!({"asof": asof}))?;
        let (snapshot, _) =
            self.reference_snapshot(branch, &json!({"commit": commit_id}), Some(&date))?;
        Converter::new(
            target,
            Conversion::Stored {
                snapshot,
                source: s(convert, "source").map(str::to_string),
                rate_type: s(convert, "rate_type").map(str::to_string),
                date,
            },
        )
    }
}
