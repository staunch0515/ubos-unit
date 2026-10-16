//! Schedules of the platform (PH-1 step 15d, DEC-076; DSN-NOD-224…-226; FR-EVT-041…-044).
//!
//! * [`Rule`]: a cron rule (`m h dom mon dow`, numbers, `*`, lists, ranges and steps;
//!   `@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`) or a business-calendar rule
//!   (`{calendar: {weekend, holidays}, at: "HH:MM", nth?}`: every business day, or the nth
//!   business day of each month, at a local time), evaluated in the schedule's IANA time
//!   zone. A local time that does not exist (the spring-forward gap) has no slot; one that
//!   occurs twice (the fall-back overlap) has one slot, at its earlier instant.
//! * [`plan`]: what an evaluation does with the slots in `(cursor, now]`: a slot within the
//!   grace of `now` is on time and fires; older slots are missed (an outage, DSN-NOD-226)
//!   and follow the misfire policy (`skip`, `fire_once`, `fire_all`, FR-EVT-041); slots
//!   outside the active window are skipped; while paused every slot is recorded `paused`,
//!   and on resume the recorded slots follow the same policy (DSN-NOD-225).
//! * [`ScheduleStore`]: the cursor of each schedule and its slots, unique by
//!   `(schedule, slot)` (PostgreSQL `schedule_slot`, or memory): a slot fires at most once
//!   whatever the number of leaders (DSN-NOD-224). A fired slot's job has an identifier
//!   derived from `(schedule, slot)`, so a job enqueued twice is one job.

use crate::db::{Db, Param};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use ubos_dvm::types::BpaError;

/// Slots within this many seconds of `now` are on time.
pub const GRACE_SECS: i64 = 60;
/// The most slots one evaluation considers (older ones of a long outage are dropped).
pub const MAX_SLOTS: usize = 10_000;

fn invalid(m: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m.into())
}

// ---- cron ----

/// A parsed cron rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: BTreeSet<u32>,
    hours: BTreeSet<u32>,
    days: BTreeSet<u32>,
    months: BTreeSet<u32>,
    weekdays: BTreeSet<u32>,
    any_day: bool,
    any_weekday: bool,
}

fn field(text: &str, lo: u32, hi: u32, what: &str) -> Result<(BTreeSet<u32>, bool), BpaError> {
    let mut out = BTreeSet::new();
    let any = text == "*" || text == "?";
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (
                r,
                s.parse::<u32>()
                    .map_err(|_| invalid(format!("cron {what}: step {s:?}")))?,
            ),
            None => (part, 1),
        };
        if step == 0 {
            return Err(invalid(format!("cron {what}: a step is positive")));
        }
        let (a, b) = match range {
            "*" | "?" => (lo, hi),
            r => match r.split_once('-') {
                Some((x, y)) => (num(x, what)?, num(y, what)?),
                None => {
                    let n = num(r, what)?;
                    (n, if part.contains('/') { hi } else { n })
                }
            },
        };
        if a < lo || b > hi || a > b {
            return Err(invalid(format!(
                "cron {what}: {part:?} is outside {lo}…{hi}"
            )));
        }
        out.extend((a..=b).step_by(step as usize));
    }
    Ok((out, any))
}

fn num(s: &str, what: &str) -> Result<u32, BpaError> {
    s.parse::<u32>()
        .map_err(|_| invalid(format!("cron {what}: {s:?} is not a number")))
}

impl Cron {
    /// Parse a five-field rule or a macro.
    pub fn parse(text: &str) -> Result<Cron, BpaError> {
        let t = match text.trim() {
            "@hourly" => "0 * * * *",
            "@daily" | "@midnight" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" | "@annually" => "0 0 1 1 *",
            other => other,
        };
        let f: Vec<&str> = t.split_whitespace().collect();
        if f.len() != 5 {
            return Err(invalid(
                "a cron rule has five fields: minute hour day month weekday",
            ));
        }
        let (minutes, _) = field(f[0], 0, 59, "minute")?;
        let (hours, _) = field(f[1], 0, 23, "hour")?;
        let (days, any_day) = field(f[2], 1, 31, "day")?;
        let (months, _) = field(f[3], 1, 12, "month")?;
        let (mut weekdays, any_weekday) = field(f[4], 0, 7, "weekday")?;
        if weekdays.remove(&7) {
            weekdays.insert(0);
        }
        Ok(Cron {
            minutes,
            hours,
            days,
            months,
            weekdays,
            any_day,
            any_weekday,
        })
    }

    fn date_matches(&self, d: NaiveDate) -> bool {
        if !self.months.contains(&d.month()) {
            return false;
        }
        let dom = self.days.contains(&d.day());
        let dow = self.weekdays.contains(&d.weekday().num_days_from_sunday());
        // classic cron: with both restricted, either matches
        match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (false, true) => dom,
            (true, false) => dow,
            (false, false) => dom || dow,
        }
    }

    fn times(&self) -> Vec<NaiveTime> {
        let mut out = Vec::new();
        for h in &self.hours {
            for m in &self.minutes {
                if let Some(t) = NaiveTime::from_hms_opt(*h, *m, 0) {
                    out.push(t);
                }
            }
        }
        out
    }
}

// ---- rules ----

/// A business calendar: weekend days (`sat`, `sun`, …) and holiday dates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Calendar {
    weekend: BTreeSet<u32>,
    holidays: BTreeSet<NaiveDate>,
}

impl Calendar {
    /// From `{weekend?: ["sat", "sun"], holidays?: ["YYYY-MM-DD", …]}`.
    pub fn from_json(v: &Value) -> Result<Calendar, BpaError> {
        let names = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
        let weekend = match v.get("weekend").and_then(Value::as_array) {
            Some(a) => a
                .iter()
                .map(|x| {
                    let s = x.as_str().unwrap_or_default().to_ascii_lowercase();
                    names
                        .iter()
                        .position(|n| s.starts_with(n))
                        .map(|p| p as u32)
                        .ok_or_else(|| invalid(format!("unknown weekday {s:?}")))
                })
                .collect::<Result<_, _>>()?,
            None => [0, 6].into_iter().collect(),
        };
        let holidays = v
            .get("holidays")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|x| {
                x.as_str()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .ok_or_else(|| invalid("a holiday is YYYY-MM-DD"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Calendar { weekend, holidays })
    }

    /// Whether a date is a business day.
    pub fn is_business_day(&self, d: NaiveDate) -> bool {
        !self.weekend.contains(&d.weekday().num_days_from_sunday()) && !self.holidays.contains(&d)
    }
}

/// A schedule rule in a time zone.
#[derive(Debug, Clone)]
pub enum Kind {
    /// A cron rule.
    Cron(Cron),
    /// Every business day at a time, or the nth business day of each month.
    Calendar {
        /// The calendar.
        calendar: Calendar,
        /// The local time.
        at: NaiveTime,
        /// The nth business day of each month (1-based), if any.
        nth: Option<u32>,
    },
}

/// A schedule's rule with its time zone.
#[derive(Debug, Clone)]
pub struct Rule {
    kind: Kind,
    tz: Tz,
}

impl Rule {
    /// From a schedule body: `rule` (`{cron}` or `{calendar, at, nth?}`) and `time_zone`
    /// (`UTC` when absent).
    pub fn from_body(b: &Value) -> Result<Rule, BpaError> {
        let tz: Tz = b
            .get("time_zone")
            .and_then(Value::as_str)
            .unwrap_or("UTC")
            .parse()
            .map_err(|_| invalid("time_zone is an IANA time zone"))?;
        let r = &b["rule"];
        let kind = if let Some(c) = r.get("cron").and_then(Value::as_str) {
            Kind::Cron(Cron::parse(c)?)
        } else if let Some(cal) = r.get("calendar") {
            let at = r
                .get("at")
                .and_then(Value::as_str)
                .and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok())
                .ok_or_else(|| invalid("a calendar rule has at: \"HH:MM\""))?;
            let nth = r.get("nth").and_then(Value::as_u64).map(|n| n as u32);
            if nth.is_some_and(|n| n == 0 || n > 23) {
                return Err(invalid("nth is 1 to 23"));
            }
            Kind::Calendar {
                calendar: Calendar::from_json(cal)?,
                at,
                nth,
            }
        } else {
            return Err(invalid("a rule is {cron} or {calendar, at}"));
        };
        Ok(Rule { kind, tz })
    }

    /// The local times of a date that are slots.
    fn local_slots(&self, d: NaiveDate) -> Vec<NaiveTime> {
        match &self.kind {
            Kind::Cron(c) => {
                if c.date_matches(d) {
                    c.times()
                } else {
                    Vec::new()
                }
            }
            Kind::Calendar { calendar, at, nth } => {
                if !calendar.is_business_day(d) {
                    return Vec::new();
                }
                if let Some(n) = nth {
                    let first = d.with_day(1).unwrap_or(d);
                    let k = (0..d.day0())
                        .filter_map(|i| first.checked_add_signed(Duration::days(i64::from(i))))
                        .filter(|x| calendar.is_business_day(*x))
                        .count() as u32
                        + 1;
                    if k != *n {
                        return Vec::new();
                    }
                }
                vec![*at]
            }
        }
    }

    /// The first slot after `after`, searching up to five years ahead.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let start = after.with_timezone(&self.tz).date_naive();
        for i in 0..(366 * 5 + 2) {
            let d = start.checked_add_signed(Duration::days(i))?;
            for t in self.local_slots(d) {
                let utc = match self.tz.from_local_datetime(&d.and_time(t)) {
                    chrono::LocalResult::Single(x) => x.with_timezone(&Utc),
                    chrono::LocalResult::Ambiguous(a, _) => a.with_timezone(&Utc),
                    chrono::LocalResult::None => continue,
                };
                if utc > after {
                    return Some(utc);
                }
            }
        }
        None
    }

    /// The slots in `(after, until]`, at most `MAX_SLOTS` (the latest ones), and how many
    /// older ones were dropped.
    pub fn slots(&self, after: DateTime<Utc>, until: DateTime<Utc>) -> (Vec<DateTime<Utc>>, usize) {
        let mut out = std::collections::VecDeque::new();
        let mut dropped = 0;
        let mut t = after;
        while let Some(n) = self.next_after(t) {
            if n > until {
                break;
            }
            out.push_back(n);
            if out.len() > MAX_SLOTS {
                out.pop_front();
                dropped += 1;
            }
            t = n;
        }
        (out.into_iter().collect(), dropped)
    }
}

// ---- the plan of an evaluation ----

/// An active window: from (inclusive) and to (exclusive), each optional.
pub type Window = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);

/// What one evaluation decides for a schedule.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Slots to fire, in order.
    pub fire: Vec<DateTime<Utc>>,
    /// Slots recorded without firing, with their status (`missed`, `skipped`, `paused`).
    pub record: Vec<(DateTime<Utc>, &'static str)>,
    /// Recorded `paused` slots decided on resume: fired (`caught_up`) or `skipped`.
    pub resume: Vec<(DateTime<Utc>, &'static str)>,
    /// The new cursor.
    pub cursor: DateTime<Utc>,
}

/// The misfire policies of FR-EVT-041.
pub const MISFIRE: &[&str] = &["skip", "fire_once", "fire_all"];

/// Decide an evaluation: `due` are the slots in `(cursor, now]`, `paused_slots` the slots
/// recorded while the schedule was paused (decided once it is no longer paused). Slots
/// outside the window are skipped; while paused, the others are recorded `paused`. Then the
/// slots within the grace of `now` fire, and the backlog (missed and paused slots) follows
/// the misfire policy: `fire_all` fires all of it in slot order, `fire_once` its latest slot
/// when no slot is on time, `skip` none.
pub fn plan(
    due: &[DateTime<Utc>],
    paused_slots: &[DateTime<Utc>],
    now: DateTime<Utc>,
    paused: bool,
    misfire: &str,
    window: Window,
) -> Plan {
    let mut p = Plan {
        cursor: now,
        ..Plan::default()
    };
    let in_window =
        |s: &DateTime<Utc>| window.0.is_none_or(|f| *s >= f) && window.1.is_none_or(|t| *s < t);
    let mut on_time = Vec::new();
    let mut backlog: Vec<(DateTime<Utc>, bool)> = paused_slots.iter().map(|s| (*s, true)).collect();
    for s in due {
        if !in_window(s) {
            p.record.push((*s, "skipped"));
        } else if paused {
            p.record.push((*s, "paused"));
        } else if now - *s <= Duration::seconds(GRACE_SECS) {
            on_time.push(*s);
        } else {
            backlog.push((*s, false));
        }
    }
    if paused {
        return p;
    }
    backlog.sort();
    let last = backlog.last().map(|x| x.0);
    for (s, was_paused) in backlog {
        let fire = match misfire {
            "fire_all" => true,
            "fire_once" => on_time.is_empty() && Some(s) == last,
            _ => false,
        };
        match (was_paused, fire) {
            (true, f) => p.resume.push((s, if f { "caught_up" } else { "skipped" })),
            (false, true) => p.fire.push(s),
            (false, false) => p.record.push((s, "missed")),
        }
    }
    p.fire.extend(on_time);
    p.fire.sort();
    p
}

/// The misfire policy of a schedule body (`fire_once` when absent).
pub fn misfire_of(b: &Value) -> Result<&str, BpaError> {
    match b.get("misfire").and_then(Value::as_str) {
        None => Ok("fire_once"),
        Some(m) if MISFIRE.contains(&m) => Ok(m),
        Some(m) => Err(invalid(format!(
            "misfire {m:?} is skip, fire_once or fire_all"
        ))),
    }
}

/// The active window of a schedule body (`{from?, to?}` instants).
pub fn window_of(b: &Value) -> Result<Window, BpaError> {
    let w = &b["window"];
    let at = |k: &str| w.get(k).and_then(Value::as_str).map(parse).transpose();
    Ok((at("from")?, at("to")?))
}

/// What one evaluation of a schedule did.
#[derive(Debug, Default)]
pub struct Evaluated {
    /// The job specifications to enqueue (slots now `firing`).
    pub specs: Vec<Value>,
    /// The slots to mark `fired` once enqueued.
    pub firing: Vec<DateTime<Utc>>,
    /// Older slots of a long outage left out (beyond `MAX_SLOTS`).
    pub dropped: usize,
}

/// Evaluate a schedule (`id`, its body) at `now` against its store: slots are recorded
/// before their jobs are enqueued (DSN-NOD-224), and a slot left `firing` by a crash is
/// enqueued again (the job identifier is the slot's, so this makes no second job).
pub fn evaluate(
    store: &dyn ScheduleStore,
    id: &str,
    b: &Value,
    now: DateTime<Utc>,
) -> Result<Evaluated, BpaError> {
    let mut out = Evaluated::default();
    let enabled = b.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let cursor = store.cursor(id)?;
    let Some(cursor) = cursor.filter(|_| enabled) else {
        // a first evaluation starts now; a disabled schedule has no slots
        store.set_cursor(id, now)?;
        return Ok(out);
    };
    let rule = Rule::from_body(b)?;
    let misfire = misfire_of(b)?;
    let window = window_of(b)?;
    let paused = b.get("paused").and_then(Value::as_bool).unwrap_or(false);
    for f in store.slots(id, Some("firing"), 1000)? {
        let slot = parse(f["slot"].as_str().unwrap_or_default())?;
        out.specs
            .push(crate::platform::schedule_job(id, b, &iso(slot), false)?);
        out.firing.push(slot);
    }
    let (due, dropped) = rule.slots(cursor, now);
    out.dropped = dropped;
    let paused_slots: Vec<DateTime<Utc>> = if paused {
        Vec::new()
    } else {
        store
            .slots(id, Some("paused"), MAX_SLOTS as i64)?
            .iter()
            .filter_map(|v| v["slot"].as_str().and_then(|t| parse(t).ok()))
            .collect()
    };
    let p = plan(&due, &paused_slots, now, paused, misfire, window);
    for (slot, status) in &p.record {
        store.record(id, *slot, status, None)?;
    }
    for slot in &p.fire {
        let t = iso(*slot);
        let job = crate::platform::schedule_job_id(id, &t)?;
        if store.record(id, *slot, "firing", Some(&job))? {
            out.specs
                .push(crate::platform::schedule_job(id, b, &t, false)?);
            out.firing.push(*slot);
        }
    }
    for (slot, status) in &p.resume {
        let t = iso(*slot);
        if *status == "caught_up" {
            let job = crate::platform::schedule_job_id(id, &t)?;
            if store.transition(id, *slot, "paused", "firing", Some(&job))? {
                out.specs
                    .push(crate::platform::schedule_job(id, b, &t, true)?);
                out.firing.push(*slot);
            }
        } else {
            store.transition(id, *slot, "paused", "skipped", None)?;
        }
    }
    store.set_cursor(id, p.cursor)?;
    Ok(out)
}

/// Mark enqueued slots `fired`.
pub fn mark_fired(
    store: &dyn ScheduleStore,
    id: &str,
    slots: &[DateTime<Utc>],
) -> Result<(), BpaError> {
    for s in slots {
        store.transition(id, *s, "firing", "fired", None)?;
    }
    Ok(())
}

// ---- the store ----

/// The cursor and the slots of a VAE's schedules.
pub trait ScheduleStore: Send + Sync {
    /// The cursor of a schedule.
    fn cursor(&self, schedule: &str) -> Result<Option<DateTime<Utc>>, BpaError>;
    /// Advance the cursor (never backwards).
    fn set_cursor(&self, schedule: &str, at: DateTime<Utc>) -> Result<(), BpaError>;
    /// Record a slot; `false` when it is recorded already (another leader's).
    fn record(
        &self,
        schedule: &str,
        slot: DateTime<Utc>,
        status: &str,
        job_id: Option<&str>,
    ) -> Result<bool, BpaError>;
    /// Change a slot's status from `from` to `to`; `false` when it was not `from`.
    fn transition(
        &self,
        schedule: &str,
        slot: DateTime<Utc>,
        from: &str,
        to: &str,
        job_id: Option<&str>,
    ) -> Result<bool, BpaError>;
    /// The slots of a schedule, oldest first (`status` filters), at most `limit`.
    fn slots(
        &self,
        schedule: &str,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Value>, BpaError>;
    /// Forget the cursors of the schedules not in `live` (deleted or disabled ones: their
    /// next evaluation starts afresh, and a VAE without live schedules has no schedule work).
    fn retain(&self, live: &[String]) -> Result<(), BpaError>;
}

fn iso(t: DateTime<Utc>) -> String {
    ubos_dvm::types::time::format_instant(&t)
}

/// Parse an instant (canonical, or any RFC 3339 instant).
pub fn parse(t: &str) -> Result<DateTime<Utc>, BpaError> {
    ubos_dvm::types::time::parse_instant(t).or_else(|_| {
        DateTime::parse_from_rfc3339(t)
            .map(|x| x.with_timezone(&Utc))
            .map_err(|_| invalid(format!("{t:?} is not an instant")))
    })
}

/// The store on PostgreSQL (migration 0012).
pub struct PgScheduleStore {
    db: Arc<Db>,
    tenant: String,
    vae: String,
}

impl PgScheduleStore {
    /// The schedule state of a VAE.
    pub fn new(db: Arc<Db>, tenant: &str, vae: &str) -> PgScheduleStore {
        PgScheduleStore {
            db,
            tenant: tenant.to_string(),
            vae: vae.to_string(),
        }
    }
}

const TS: &str = "'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'";

impl ScheduleStore for PgScheduleStore {
    fn cursor(&self, schedule: &str) -> Result<Option<DateTime<Utc>>, BpaError> {
        let rows = self.db.query(
            &format!("SELECT to_char(cursor_at AT TIME ZONE 'UTC', {TS}) AS c FROM schedule_cursor WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND schedule_id = $3"),
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(schedule)],
        )?;
        rows.first()
            .and_then(|r| r["c"].as_str())
            .map(parse)
            .transpose()
    }
    fn set_cursor(&self, schedule: &str, at: DateTime<Utc>) -> Result<(), BpaError> {
        self.db.execute(
            "INSERT INTO schedule_cursor (tenant_id, vae_id, schedule_id, cursor_at) VALUES ($1::uuid, $2::uuid, $3, $4::timestamptz) \
             ON CONFLICT (tenant_id, vae_id, schedule_id) DO UPDATE SET cursor_at = greatest(schedule_cursor.cursor_at, EXCLUDED.cursor_at)",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(schedule), Param::Text(&iso(at))],
        )?;
        Ok(())
    }
    fn record(
        &self,
        schedule: &str,
        slot: DateTime<Utc>,
        status: &str,
        job_id: Option<&str>,
    ) -> Result<bool, BpaError> {
        let n = self.db.execute(
            "INSERT INTO schedule_slot (tenant_id, vae_id, schedule_id, slot, status, job_id) VALUES ($1::uuid, $2::uuid, $3, $4::timestamptz, $5, nullif($6, '')::uuid) ON CONFLICT DO NOTHING",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(schedule), Param::Text(&iso(slot)), Param::Text(status), Param::Text(job_id.unwrap_or_default())],
        )?;
        Ok(n == 1)
    }
    fn transition(
        &self,
        schedule: &str,
        slot: DateTime<Utc>,
        from: &str,
        to: &str,
        job_id: Option<&str>,
    ) -> Result<bool, BpaError> {
        let n = self.db.execute(
            "UPDATE schedule_slot SET status = $6, job_id = coalesce(nullif($7, '')::uuid, job_id), recorded_at = now() \
             WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND schedule_id = $3 AND slot = $4::timestamptz AND status = $5",
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(schedule), Param::Text(&iso(slot)), Param::Text(from), Param::Text(to), Param::Text(job_id.unwrap_or_default())],
        )?;
        Ok(n == 1)
    }
    fn slots(
        &self,
        schedule: &str,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Value>, BpaError> {
        self.db.query(
            &format!("SELECT to_char(slot AT TIME ZONE 'UTC', {TS}) AS slot, status, job_id::text AS job_id FROM schedule_slot \
              WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND schedule_id = $3 AND ($4 = '' OR status = $4) ORDER BY slot LIMIT $5"),
            &[Param::Text(&self.tenant), Param::Text(&self.vae), Param::Text(schedule), Param::Text(status.unwrap_or_default()), Param::Int(limit.clamp(1, 10_000))],
        )
    }
    fn retain(&self, live: &[String]) -> Result<(), BpaError> {
        let ids = serde_json::to_string(live).unwrap_or_else(|_| "[]".into());
        self.db.execute(
            "DELETE FROM schedule_cursor WHERE tenant_id = $1::uuid AND vae_id = $2::uuid \
             AND schedule_id NOT IN (SELECT jsonb_array_elements_text($3::jsonb))",
            &[
                Param::Text(&self.tenant),
                Param::Text(&self.vae),
                Param::Text(&ids),
            ],
        )?;
        Ok(())
    }
}

/// Slots by `(schedule, slot)`: status and job.
type SlotMap = BTreeMap<(String, DateTime<Utc>), (String, Option<String>)>;

/// The store in memory (the memory preset, suites).
#[derive(Default)]
pub struct MemScheduleStore {
    cursors: Mutex<BTreeMap<String, DateTime<Utc>>>,
    slots: Mutex<SlotMap>,
}

fn lock_err() -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", "schedule store lock")
}

impl ScheduleStore for MemScheduleStore {
    fn cursor(&self, schedule: &str) -> Result<Option<DateTime<Utc>>, BpaError> {
        Ok(self
            .cursors
            .lock()
            .map_err(|_| lock_err())?
            .get(schedule)
            .copied())
    }
    fn set_cursor(&self, schedule: &str, at: DateTime<Utc>) -> Result<(), BpaError> {
        let mut c = self.cursors.lock().map_err(|_| lock_err())?;
        let e = c.entry(schedule.to_string()).or_insert(at);
        if at > *e {
            *e = at;
        }
        Ok(())
    }
    fn record(
        &self,
        schedule: &str,
        slot: DateTime<Utc>,
        status: &str,
        job_id: Option<&str>,
    ) -> Result<bool, BpaError> {
        let mut s = self.slots.lock().map_err(|_| lock_err())?;
        let k = (schedule.to_string(), slot);
        if s.contains_key(&k) {
            return Ok(false);
        }
        s.insert(k, (status.to_string(), job_id.map(str::to_string)));
        Ok(true)
    }
    fn transition(
        &self,
        schedule: &str,
        slot: DateTime<Utc>,
        from: &str,
        to: &str,
        job_id: Option<&str>,
    ) -> Result<bool, BpaError> {
        let mut s = self.slots.lock().map_err(|_| lock_err())?;
        match s.get_mut(&(schedule.to_string(), slot)) {
            Some(v) if v.0 == from => {
                v.0 = to.to_string();
                if let Some(j) = job_id {
                    v.1 = Some(j.to_string());
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn slots(
        &self,
        schedule: &str,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Value>, BpaError> {
        let s = self.slots.lock().map_err(|_| lock_err())?;
        Ok(s.iter()
            .filter(|((id, _), (st, _))| id == schedule && status.is_none_or(|x| x == st))
            .take(limit.clamp(1, 10_000) as usize)
            .map(|((_, t), (st, j))| json!({"slot": iso(*t), "status": st, "job_id": j}))
            .collect())
    }
    fn retain(&self, live: &[String]) -> Result<(), BpaError> {
        self.cursors
            .lock()
            .map_err(|_| lock_err())?
            .retain(|k, _| live.contains(k));
        Ok(())
    }
}
