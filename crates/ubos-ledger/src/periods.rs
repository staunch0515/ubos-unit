//! Periods, close artefacts and restatement (PH-2 step 3c, DEC-098): the pure part of
//! STD-KIND-025, STD-KIND-026, STD-TIME-041 and STD-TIME-042 of BPA 1.0-rc8 and of the
//! design items DSN-DVM-322, -323 and -442…-444.
//!
//! Nothing here reads a store or makes a commit. The kernel (`ubos-dvm`) reads the period
//! objects, the entries and the journals, and asks this module:
//!
//! - whether a period may move from one state to another, by an update or by one of the
//!   period operations ([`check_transition`], [`check_close`], [`check_reopen`]);
//! - whether a posting date falls in a closed period ([`check_posting`]);
//! - what a close writes: the tag, the slug and the body of the close artefact with the
//!   balances per ledger key ([`tag_name`], [`artefact_slug`], [`balances`],
//!   [`artefact_body`]);
//! - how a restatement proceeds: the mode ([`Mode`]), the correction branch
//!   ([`correction_branch`]), the logic version a re-run takes ([`logic_version`]), the
//!   processes to re-run and their order ([`plan`]) and the reported changes ([`change`]);
//! - which adjustments compensate restated postings ([`proposals`], DSN-DVM-323).
//!
//! Period classes, ledger classes and their amount fields are declared by Buks; this
//! module knows only the three states and the members `start`, `end` and `state`.

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::decimal::Decimal;
use ubos_types::values::minor_unit;
use ubos_types::{BpaError, canonical};

/// State of a period that accepts appends.
pub const OPEN: &str = "open";
/// State of a period in soft close: appends still commit.
pub const CLOSING: &str = "closing";
/// State of a closed period: appends with a posting date in it fail.
pub const CLOSED: &str = "closed";
/// The states of a period (STD-KIND-025).
pub const STATES: &[&str] = &[OPEN, CLOSING, CLOSED];
/// The genesis System class of close artefacts (STD-KIND-026).
pub const ARTEFACT_CLASS: &str = "CloseArtefact";
/// The branch kind of a restatement (STD-TIME-042).
pub const CORRECTION_KIND: &str = "correction";
/// The posting date of an entry whose commit has no transaction time: after every period.
const NO_DATE: &str = "9999-12-31";

fn invalid(message: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", message)
}

fn incomplete(message: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", message)
}

// ---- transitions (STD-KIND-025) ----

/// What asks for a transition of a period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// An update of the period object by a commit.
    Update,
    /// `close_period`.
    Close,
    /// `reopen_period`.
    Reopen,
}

/// The error of a transition a period may not make.
pub fn transition_error(old: Option<&str>, new: Option<&str>) -> BpaError {
    BpaError::with_details(
        "MODEL.CONSTRAINT_VIOLATED",
        json!({"period_transition": [old, new]}),
    )
}

/// STD-KIND-025: an update moves a period between `open` and `closing` only; `closed` is
/// entered by `close_period` (from `closing`) and left by `reopen_period` (to `open`).
/// A change that keeps the state is no transition.
pub fn check_transition(old: Option<&str>, new: Option<&str>, via: Via) -> Result<(), BpaError> {
    if old == new {
        return Ok(());
    }
    let allowed = match (old, new) {
        (Some(OPEN), Some(CLOSING)) | (Some(CLOSING), Some(OPEN)) => true,
        (Some(CLOSING), Some(CLOSED)) => via == Via::Close,
        (Some(CLOSED), Some(OPEN)) => via == Via::Reopen,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(transition_error(old, new))
    }
}

/// STD-KIND-026: only a period in `closing` is closed.
pub fn check_close(state: Option<&str>) -> Result<(), BpaError> {
    if state == Some(CLOSING) {
        Ok(())
    } else {
        Err(transition_error(state, Some(CLOSED)))
    }
}

/// STD-KIND-025: only a closed period is reopened.
pub fn check_reopen(state: Option<&str>) -> Result<(), BpaError> {
    if state == Some(CLOSED) {
        Ok(())
    } else {
        Err(transition_error(state, Some(OPEN)))
    }
}

/// The reason of a reopening: a non-empty text (`MODEL.DECLARATION_INCOMPLETE` without).
pub fn reopen_reason(reason: Option<&Value>) -> Result<&str, BpaError> {
    reason
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| incomplete("a reopening states its reason"))
}

// ---- posting dates (STD-KIND-025) ----

/// A period object: the half-open interval `[start, end)` of dates and a state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Period {
    /// First day.
    pub start: String,
    /// The day after the last.
    pub end: String,
    /// `open`, `closing` or `closed`.
    pub state: String,
}

impl Period {
    /// The period of a period object's body.
    pub fn of(body: &Value) -> Period {
        let text = |k: &str| {
            body.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Period {
            start: text("start"),
            end: text("end"),
            state: text("state"),
        }
    }

    /// Whether a day falls in the period; the end is exclusive.
    pub fn contains(&self, day: &str) -> bool {
        self.start.as_str() <= day && day < self.end.as_str()
    }

    /// Whether the period rejects an append with this posting date.
    pub fn locks(&self, day: &str) -> bool {
        self.state == CLOSED && self.contains(day)
    }
}

/// STD-KIND-025: an append with a posting date in a closed period fails with
/// `LEDG.PERIOD_CLOSED`. `periods` are the period objects of the class's period scope.
pub fn check_posting(periods: &[Period], day: &str) -> Result<(), BpaError> {
    if periods.iter().any(|p| p.locks(day)) {
        return Err(BpaError::new(
            "LEDG.PERIOD_CLOSED",
            "posting date in a closed period",
        ));
    }
    Ok(())
}

// ---- close artefacts (STD-KIND-026) ----

/// The tag of the `n`-th close of a period: `close-<period slug>-<n>`.
pub fn tag_name(period_slug: &str, n: u64) -> String {
    format!("close-{period_slug}-{n}")
}

/// The slug of the `n`-th close artefact of a period: `<period slug>-close-<n>`.
pub fn artefact_slug(period_slug: &str, n: u64) -> String {
    format!("{period_slug}-close-{n}")
}

/// The sequence of the next close of a period: one more than the close artefacts that
/// name it. `artefacts` are the bodies of the close artefacts of the branch.
pub fn next_sequence<'a>(artefacts: impl IntoIterator<Item = &'a Value>, period: &str) -> u64 {
    1 + artefacts
        .into_iter()
        .filter(|b| b.get("period").and_then(Value::as_str) == Some(period))
        .count() as u64
}

/// An entry as the balances read it.
#[derive(Debug, Clone)]
pub struct Posted<'a> {
    /// Ledger class.
    pub class: &'a str,
    /// Ledger key values.
    pub ledger_key: &'a Value,
    /// The date of the transaction time of the commit that appended the entry on the
    /// branch; `None` when that commit has none.
    pub posting_date: Option<&'a str>,
    /// Field values.
    pub body: &'a Value,
}

/// Sums per currency; a plain decimal counts under the empty currency.
type Sums = BTreeMap<String, Decimal>;

fn amount_of(v: &Value) -> Result<(&str, Decimal), BpaError> {
    let (currency, amount) = match v {
        Value::Object(m) => (
            m.get("currency")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("an amount has a currency"))?,
            m.get("amount"),
        ),
        other => ("", Some(other)),
    };
    let d = Decimal::parse(
        amount
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("not a decimal string"))?,
    )?;
    Ok((currency, d))
}

fn add(acc: &mut Sums, v: &Value, negate: bool) -> Result<(), BpaError> {
    let (currency, d) = amount_of(v)?;
    let d = if negate { d.neg() } else { d };
    let sum = match acc.get(currency) {
        Some(cur) => cur.add(&d),
        None => d,
    };
    acc.insert(currency.to_string(), sum);
    Ok(())
}

/// A total as a value: a decimal for plain decimals, else a list of money values in
/// currency order, each at the minor unit of its currency.
fn total(acc: &Sums) -> Result<Value, BpaError> {
    if acc.len() == 1
        && let Some(d) = acc.get("")
    {
        return Ok(json!(d.format(None)));
    }
    acc.iter()
        .map(|(c, d)| {
            let scale = minor_unit(c).ok_or_else(|| invalid("unknown ISO 4217 currency"))?;
            Ok(json!({"amount": d.format(Some(scale)), "currency": c}))
        })
        .collect::<Result<Vec<Value>, BpaError>>()
        .map(Value::Array)
}

/// STD-KIND-026: the balances per ledger key of the entries with a posting date before
/// `end`: `{class, ledger_key, entries, amounts}` in the order of class and canonical
/// ledger key; a key without such an entry is left out. `entries` are the entries of the
/// classes the period scopes, in ledger order; `amounts` gives the amount fields of each
/// class.
pub fn balances(
    entries: &[Posted<'_>],
    amounts: &BTreeMap<String, Vec<String>>,
    end: &str,
) -> Result<Vec<Value>, BpaError> {
    let mut keys: BTreeMap<(String, String), Vec<&Posted<'_>>> = BTreeMap::new();
    for e in entries {
        keys.entry((e.class.to_string(), canonical(e.ledger_key)?))
            .or_default()
            .push(e);
    }
    let mut out = Vec::new();
    for ((class, _), q) in &keys {
        let fields = amounts.get(class).map(Vec::as_slice).unwrap_or_default();
        let mut sums: Vec<Sums> = vec![Sums::new(); fields.len()];
        let mut n = 0u64;
        for e in q {
            if e.posting_date.unwrap_or(NO_DATE) >= end {
                continue;
            }
            n += 1;
            for (f, acc) in fields.iter().zip(sums.iter_mut()) {
                if let Some(v) = e.body.get(f).filter(|v| !v.is_null()) {
                    add(acc, v, false)?;
                }
            }
        }
        if n == 0 {
            continue;
        }
        let mut shown = Map::new();
        for (f, acc) in fields.iter().zip(&sums) {
            if !acc.is_empty() {
                shown.insert(f.clone(), total(acc)?);
            }
        }
        out.push(
            json!({"class": class, "ledger_key": q[0].ledger_key, "entries": n,
                        "amounts": shown}),
        );
    }
    Ok(out)
}

/// The commit a close extends: `{cseq, commit_id, root}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsOf {
    /// Commit ordinal on the branch.
    pub cseq: u64,
    /// Commit identifier.
    pub commit_id: String,
    /// Root of the state tree.
    pub root: String,
}

/// STD-KIND-026: the body of a close artefact.
pub fn artefact_body(
    period_id: &str,
    period_slug: &str,
    period: &Period,
    sequence: u64,
    as_of: &AsOf,
    balances: Vec<Value>,
) -> Value {
    json!({"period": period_id, "period_slug": period_slug, "sequence": sequence,
           "start": period.start, "end": period.end, "tag": tag_name(period_slug, sequence),
           "as_of": {"cseq": as_of.cseq, "commit_id": as_of.commit_id, "root": as_of.root},
           "balances": balances})
}

// ---- restatement (STD-TIME-041, STD-TIME-042) ----

/// A re-evaluation mode (STD-TIME-041).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The logic versions the original process ran.
    AsOriginally,
    /// The versions whose timelines cover the recomputed date; the default.
    AsEffective,
    /// The head versions.
    Current,
}

impl Mode {
    /// The mode of a request: `as_effective` when none is given.
    pub fn parse(mode: Option<&Value>) -> Result<Mode, BpaError> {
        match mode {
            None => Ok(Mode::AsEffective),
            Some(v) => match v.as_str() {
                Some("as_originally") => Ok(Mode::AsOriginally),
                Some("as_effective") => Ok(Mode::AsEffective),
                Some("current") => Ok(Mode::Current),
                _ => Err(invalid("mode is as_originally, as_effective or current")),
            },
        }
    }

    /// The name of the mode, as derivation records hold it.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::AsOriginally => "as_originally",
            Mode::AsEffective => "as_effective",
            Mode::Current => "current",
        }
    }
}

/// STD-TIME-042: a restatement runs on a branch of kind `correction`; the name of the
/// branch starts with the kind.
pub fn correction_branch(name: Option<&Value>) -> Result<&str, BpaError> {
    name.and_then(Value::as_str)
        .filter(|n| n.split('/').next() == Some(CORRECTION_KIND))
        .ok_or_else(|| invalid("a restatement runs on a correction branch"))
}

/// STD-TIME-041: the valid date a re-run recomputes — the `asof` of the original
/// invocation, else of its journaled context, else the day the original process ran.
pub fn recomputed_date(
    invocation_asof: Option<&str>,
    journal_asof: Option<&str>,
    journal_now: Option<&str>,
) -> String {
    let d = [invocation_asof, journal_asof, journal_now]
        .into_iter()
        .flatten()
        .find(|d| !d.is_empty())
        .unwrap_or_default();
    d.get(..10).unwrap_or(d).to_string()
}

/// STD-TIME-041: the version of a logic asset a re-run takes. `covering` is the version
/// whose timeline segment covers the recomputed date, `original` the logic versions of
/// the original process in the order it used them, `is_version` whether a hash is a
/// version on the asset's timeline. A mode that finds no version takes the head.
pub fn logic_version(
    mode: Mode,
    head: &str,
    covering: Option<&str>,
    original: &[String],
    is_version: impl Fn(&str) -> bool,
) -> String {
    match mode {
        Mode::Current => head.to_string(),
        Mode::AsEffective => covering.unwrap_or(head).to_string(),
        Mode::AsOriginally => original
            .iter()
            .find(|vh| vh.as_str() == head || is_version(vh))
            .map_or(head, String::as_str)
            .to_string(),
    }
}

/// An impacted result and the process that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Produced {
    /// The producing process.
    pub process: String,
    /// The commit ordinal at which the result entered the branch.
    pub cseq: u64,
}

/// The processes of a restatement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Processes to re-run, in the order of their first commits.
    pub rerun: Vec<String>,
    /// Processes that cannot be re-run: their results were written by direct input with a
    /// derivation, so no invocation is recorded.
    pub not_rerun: Vec<String>,
}

/// STD-TIME-042: every process that produced an impacted result, once, in the order of
/// the first commit among its impacted results; `recorded` tells whether the invocation
/// of a process is recorded.
pub fn plan(results: &[Produced], recorded: impl Fn(&str) -> bool) -> Plan {
    let mut first: Vec<(u64, usize, &str)> = Vec::new();
    for (i, r) in results.iter().enumerate() {
        match first.iter_mut().find(|x| x.2 == r.process) {
            Some(x) => x.0 = x.0.min(r.cseq),
            None => first.push((r.cseq, i, &r.process)),
        }
    }
    first.sort();
    let mut out = Plan::default();
    for (_, _, p) in first {
        if recorded(p) {
            out.rerun.push(p.to_string());
        } else {
            out.not_rerun.push(p.to_string());
        }
    }
    out
}

/// STD-TIME-042: a reported change `{object_id, class, old, new}` of an object the
/// correction branch changed; `old` is null for an object the restatement created.
pub fn change(object_id: &str, class: &str, old: Option<&Value>, new: &Value) -> Value {
    json!({"object_id": object_id, "class": class, "old": old, "new": new})
}

// ---- compensation proposals (DSN-DVM-323) ----

/// A posting of a process: an entry it appended.
#[derive(Debug, Clone)]
pub struct Posting {
    /// Entry identifier.
    pub entry_id: String,
    /// Ledger class.
    pub class: String,
    /// Ledger key values.
    pub ledger_key: Value,
    /// Business effect date or instant.
    pub valid_at: String,
    /// Field values.
    pub body: Value,
}

/// The date a proposed adjustment takes effect (DSN-DVM-323).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dating {
    /// The valid time of the original postings.
    Original,
    /// The current day, which lies in an open period.
    Current,
}

impl Dating {
    /// The dating of a request: `original` when none is given.
    pub fn parse(v: Option<&Value>) -> Result<Dating, BpaError> {
        match v {
            None | Some(Value::Null) => Ok(Dating::Original),
            Some(v) => match v.as_str() {
                Some("original") => Ok(Dating::Original),
                Some("current") => Ok(Dating::Current),
                _ => Err(invalid("the dating of proposals is original or current")),
            },
        }
    }
}

/// DSN-DVM-323: the adjustments that compensate restated postings. For every ledger key,
/// the difference between the restated and the original postings per amount field; a key
/// whose difference is zero in every field and currency proposes nothing.
///
/// A proposal is `{class, ledger_key, valid_at, amounts, original, restated}`: `amounts`
/// holds the non-zero differences in the form of the balances of a close artefact,
/// `original` and `restated` the entries compared, `valid_at` the valid time of the
/// first original posting of the key (of the first restated one when the key is new) or,
/// with [`Dating::Current`], `today`. Proposals come in the order of class and canonical
/// ledger key. They are proposals: nothing is appended.
pub fn proposals(
    original: &[Posting],
    restated: &[Posting],
    amounts: &BTreeMap<String, Vec<String>>,
    dating: Dating,
    today: &str,
) -> Result<Vec<Value>, BpaError> {
    type Group<'a> = (Vec<&'a Posting>, Vec<&'a Posting>);
    let mut keys: BTreeMap<(String, String), Group<'_>> = BTreeMap::new();
    for (side, list) in [original, restated].into_iter().enumerate() {
        for p in list {
            let g = keys
                .entry((p.class.clone(), canonical(&p.ledger_key)?))
                .or_default();
            if side == 0 { &mut g.0 } else { &mut g.1 }.push(p);
        }
    }
    let mut out = Vec::new();
    for ((class, _), (old, new)) in &keys {
        let fields = amounts.get(class).map(Vec::as_slice).unwrap_or_default();
        let mut shown = Map::new();
        for f in fields {
            let mut acc = Sums::new();
            for (list, negate) in [(new, false), (old, true)] {
                for p in list {
                    if let Some(v) = p.body.get(f).filter(|v| !v.is_null()) {
                        add(&mut acc, v, negate)?;
                    }
                }
            }
            acc.retain(|_, d| !d.is_zero());
            if !acc.is_empty() {
                shown.insert(f.clone(), total(&acc)?);
            }
        }
        if shown.is_empty() {
            continue;
        }
        let Some(first) = old.first().or(new.first()) else {
            continue;
        };
        let valid_at = match dating {
            Dating::Original => first.valid_at.as_str(),
            Dating::Current => today,
        };
        let ids = |l: &[&Posting]| {
            l.iter()
                .map(|p| p.entry_id.clone())
                .collect::<BTreeSet<_>>()
        };
        out.push(
            json!({"class": class, "ledger_key": first.ledger_key, "valid_at": valid_at,
                        "amounts": shown, "original": ids(old), "restated": ids(new)}),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- transitions ----

    #[test]
    fn an_update_moves_between_open_and_closing_only() {
        for (old, new, ok) in [
            (OPEN, CLOSING, true),
            (CLOSING, OPEN, true),
            (OPEN, CLOSED, false),
            (CLOSING, CLOSED, false),
            (CLOSED, OPEN, false),
            (CLOSED, CLOSING, false),
        ] {
            let r = check_transition(Some(old), Some(new), Via::Update);
            assert_eq!(r.is_ok(), ok, "{old} -> {new}");
            if let Err(e) = r {
                assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED");
                assert_eq!(
                    e.details,
                    Some(json!({"period_transition": [old, new]})),
                    "{old} -> {new}"
                );
            }
        }
    }

    #[test]
    fn closed_is_entered_by_close_and_left_by_reopen() {
        assert!(check_transition(Some(CLOSING), Some(CLOSED), Via::Close).is_ok());
        assert!(check_transition(Some(CLOSED), Some(OPEN), Via::Reopen).is_ok());
        // each operation makes its own transition only
        assert!(check_transition(Some(CLOSING), Some(CLOSED), Via::Reopen).is_err());
        assert!(check_transition(Some(CLOSED), Some(OPEN), Via::Close).is_err());
        assert!(check_transition(Some(OPEN), Some(CLOSED), Via::Close).is_err());
        assert!(check_transition(Some(CLOSED), Some(CLOSING), Via::Reopen).is_err());
    }

    #[test]
    fn a_change_that_keeps_the_state_is_no_transition() {
        for s in [Some(OPEN), Some(CLOSING), Some(CLOSED), Some("other"), None] {
            assert!(check_transition(s, s, Via::Update).is_ok());
        }
        // a state outside the three, or none, is never reached or left by a transition
        assert!(check_transition(None, Some(OPEN), Via::Update).is_err());
        assert!(check_transition(Some(OPEN), None, Via::Update).is_err());
        assert!(check_transition(Some("draft"), Some(OPEN), Via::Update).is_err());
    }

    #[test]
    fn the_operations_check_the_state_they_start_from() {
        assert!(check_close(Some(CLOSING)).is_ok());
        for s in [Some(OPEN), Some(CLOSED), None] {
            let e = check_close(s).expect_err("not closing");
            assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED");
            assert_eq!(e.details, Some(json!({"period_transition": [s, CLOSED]})));
        }
        assert!(check_reopen(Some(CLOSED)).is_ok());
        for s in [Some(OPEN), Some(CLOSING), None] {
            let e = check_reopen(s).expect_err("not closed");
            assert_eq!(e.details, Some(json!({"period_transition": [s, OPEN]})));
        }
    }

    #[test]
    fn a_reopening_states_its_reason() {
        assert_eq!(
            reopen_reason(Some(&json!("late invoice"))).ok(),
            Some("late invoice")
        );
        for bad in [None, Some(json!("")), Some(json!(null)), Some(json!(7))] {
            let e = reopen_reason(bad.as_ref()).expect_err("no reason");
            assert_eq!(e.code, "MODEL.DECLARATION_INCOMPLETE");
        }
    }

    // ---- posting dates ----

    fn period(start: &str, end: &str, state: &str) -> Period {
        Period::of(&json!({"start": start, "end": end, "state": state}))
    }

    #[test]
    fn a_closed_period_locks_its_half_open_interval() {
        let march = period("2026-03-01", "2026-04-01", CLOSED);
        assert!(march.locks("2026-03-01"));
        assert!(march.locks("2026-03-31"));
        assert!(!march.locks("2026-04-01"), "the end is exclusive");
        assert!(!march.locks("2026-02-28"));
        for state in [OPEN, CLOSING] {
            assert!(!period("2026-03-01", "2026-04-01", state).locks("2026-03-15"));
        }
    }

    #[test]
    fn a_posting_date_in_a_closed_period_fails() {
        let periods = [
            period("2026-03-01", "2026-04-01", CLOSED),
            period("2026-04-01", "2026-05-01", OPEN),
        ];
        let e = check_posting(&periods, "2026-03-15").expect_err("closed");
        assert_eq!(e.code, "LEDG.PERIOD_CLOSED");
        assert!(check_posting(&periods, "2026-04-01").is_ok());
        // a day no period covers is not locked
        assert!(check_posting(&periods, "2026-06-01").is_ok());
        assert!(check_posting(&[], "2026-03-15").is_ok());
    }

    // ---- close artefacts ----

    #[test]
    fn tags_and_slugs_count_the_closes() {
        assert_eq!(tag_name("2026-03", 1), "close-2026-03-1");
        assert_eq!(artefact_slug("2026-03", 2), "2026-03-close-2");
        let bodies = [
            json!({"period": "p-1"}),
            json!({"period": "p-2"}),
            json!({"period": "p-1"}),
        ];
        assert_eq!(next_sequence(&bodies, "p-1"), 3);
        assert_eq!(next_sequence(&bodies, "p-3"), 1);
    }

    fn usd(amount: &str) -> Value {
        json!({"amount": amount, "currency": "USD"})
    }

    fn fields(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(c, f)| (c.to_string(), f.iter().map(|x| x.to_string()).collect()))
            .collect()
    }

    #[test]
    fn balances_sum_the_entries_posted_before_the_end() {
        let (k1, k2, k3) = (
            json!({"account": "1010"}),
            json!({"account": "4000"}),
            json!({"account": "5000"}),
        );
        let (b1, b2, b3, b4) = (
            json!({"amount": usd("100.00")}),
            json!({"amount": usd("-100.00")}),
            json!({"amount": usd("30.00")}),
            json!({"amount": usd("7.00")}),
        );
        let e = |key, date, body| Posted {
            class: "Entry",
            ledger_key: key,
            posting_date: date,
            body,
        };
        let entries = [
            e(&k2, Some("2026-03-15"), &b2),
            e(&k1, Some("2026-03-15"), &b1),
            e(&k1, Some("2026-03-31"), &b3),
            // posted on the end day or later, or by a commit without a time: left out
            e(&k1, Some("2026-04-01"), &b4),
            e(&k3, Some("2026-04-10"), &b4),
            e(&k3, None, &b4),
        ];
        let out =
            balances(&entries, &fields(&[("Entry", &["amount"])]), "2026-04-01").expect("balances");
        assert_eq!(
            json!(out),
            json!([
                {"class": "Entry", "ledger_key": {"account": "1010"}, "entries": 2,
                 "amounts": {"amount": [usd("130.00")]}},
                {"class": "Entry", "ledger_key": {"account": "4000"}, "entries": 1,
                 "amounts": {"amount": [usd("-100.00")]}},
            ])
        );
    }

    #[test]
    fn balances_keep_currencies_apart_and_plain_decimals_plain() {
        let key = json!({"item": "bolt"});
        let bodies = [
            json!({"quantity": "5", "value": {"amount": "10.00", "currency": "USD"}}),
            json!({"quantity": "2.50", "value": {"amount": "300", "currency": "JPY"}}),
            json!({"quantity": "-7.5", "value": {"amount": "-10.00", "currency": "USD"}}),
        ];
        let entries: Vec<Posted<'_>> = bodies
            .iter()
            .map(|b| Posted {
                class: "Move",
                ledger_key: &key,
                posting_date: Some("2026-01-01"),
                body: b,
            })
            .collect();
        let out = balances(
            &entries,
            &fields(&[("Move", &["quantity", "value"])]),
            "2026-02-01",
        )
        .expect("balances");
        assert_eq!(
            out[0]["amounts"],
            json!({"quantity": "0",
                   "value": [{"amount": "300", "currency": "JPY"}, {"amount": "0.00", "currency": "USD"}]})
        );
        assert_eq!(out[0]["entries"], 3);
    }

    #[test]
    fn balances_reject_what_is_no_amount() {
        let key = json!({"k": 1});
        let body = json!({"amount": {"amount": "1.00", "currency": "XXX"}});
        let entries = [Posted {
            class: "E",
            ledger_key: &key,
            posting_date: Some("2026-01-01"),
            body: &body,
        }];
        let e = balances(&entries, &fields(&[("E", &["amount"])]), "2026-02-01")
            .expect_err("unknown currency");
        assert_eq!(e.code, "MODEL.INVALID_VALUE");
    }

    #[test]
    fn the_artefact_holds_the_period_the_sequence_and_the_commit_it_extends() {
        let body = artefact_body(
            "p-1",
            "2026-03",
            &period("2026-03-01", "2026-04-01", CLOSING),
            2,
            &AsOf {
                cseq: 7,
                commit_id: "c".into(),
                root: "r".into(),
            },
            vec![json!({"entries": 1})],
        );
        assert_eq!(
            body,
            json!({"period": "p-1", "period_slug": "2026-03", "sequence": 2, "start": "2026-03-01",
                   "end": "2026-04-01", "tag": "close-2026-03-2",
                   "as_of": {"cseq": 7, "commit_id": "c", "root": "r"},
                   "balances": [{"entries": 1}]})
        );
    }

    // ---- restatement ----

    #[test]
    fn as_effective_is_the_default_mode() {
        assert_eq!(Mode::parse(None).ok(), Some(Mode::AsEffective));
        for m in ["as_originally", "as_effective", "current"] {
            assert_eq!(Mode::parse(Some(&json!(m))).map(Mode::as_str).ok(), Some(m));
        }
        for bad in [json!("latest"), json!(null), json!(1)] {
            let e = Mode::parse(Some(&bad)).expect_err("unknown mode");
            assert_eq!(e.code, "MODEL.INVALID_VALUE");
        }
    }

    #[test]
    fn a_restatement_names_a_correction_branch() {
        assert_eq!(
            correction_branch(Some(&json!("correction/rate-fix"))).ok(),
            Some("correction/rate-fix")
        );
        assert!(correction_branch(Some(&json!("correction"))).is_ok());
        for bad in [
            Some(json!("feature/rate-fix")),
            Some(json!("corrections/x")),
            Some(json!(3)),
            None,
        ] {
            let e = correction_branch(bad.as_ref()).expect_err("not a correction branch");
            assert_eq!(e.code, "MODEL.INVALID_VALUE");
        }
    }

    #[test]
    fn the_recomputed_date_is_the_asof_of_the_original_process() {
        assert_eq!(
            recomputed_date(
                Some("2026-02-15"),
                Some("2026-02-01"),
                Some("2026-02-16T09:00:00.000000Z")
            ),
            "2026-02-15"
        );
        assert_eq!(
            recomputed_date(
                None,
                Some("2026-02-01T10:00:00Z"),
                Some("2026-02-16T09:00:00.000000Z")
            ),
            "2026-02-01"
        );
        assert_eq!(
            recomputed_date(None, None, Some("2026-02-16T09:00:00.000000Z")),
            "2026-02-16"
        );
    }

    #[test]
    fn each_mode_takes_its_logic_version() {
        let original = vec!["v0".to_string(), "v1".to_string()];
        let on_timeline = |vh: &str| vh == "v1" || vh == "v2";
        assert_eq!(
            logic_version(Mode::Current, "v2", Some("v1"), &original, on_timeline),
            "v2"
        );
        assert_eq!(
            logic_version(Mode::AsEffective, "v2", Some("v1"), &original, on_timeline),
            "v1"
        );
        // no segment covers the date: the head
        assert_eq!(
            logic_version(Mode::AsEffective, "v2", None, &original, on_timeline),
            "v2"
        );
        // the first version the original process ran that is a version of this asset
        assert_eq!(
            logic_version(Mode::AsOriginally, "v2", Some("v2"), &original, on_timeline),
            "v1"
        );
        assert_eq!(
            logic_version(Mode::AsOriginally, "v2", None, &[], on_timeline),
            "v2"
        );
        assert_eq!(
            logic_version(Mode::AsOriginally, "v9", None, &["v9".to_string()], |_| {
                false
            }),
            "v9"
        );
    }

    fn produced(process: &str, cseq: u64) -> Produced {
        Produced {
            process: process.into(),
            cseq,
        }
    }

    #[test]
    fn processes_rerun_once_in_the_order_of_their_first_commits() {
        let results = [
            produced("p-b", 9),
            produced("p-a", 4),
            produced("p-m", 6),
            produced("p-b", 3),
            produced("p-a", 12),
        ];
        let p = plan(&results, |p| p != "p-m");
        assert_eq!(p.rerun, ["p-b", "p-a"]);
        assert_eq!(p.not_rerun, ["p-m"]);
        assert_eq!(plan(&[], |_| true), Plan::default());
    }

    #[test]
    fn a_change_reports_old_and_new() {
        assert_eq!(
            change(
                "o-1",
                "Fee",
                Some(&json!({"rate": "0.1"})),
                &json!({"rate": "0.12"})
            ),
            json!({"object_id": "o-1", "class": "Fee", "old": {"rate": "0.1"}, "new": {"rate": "0.12"}})
        );
        assert_eq!(change("o-2", "Fee", None, &json!({}))["old"], Value::Null);
    }

    // ---- compensation proposals ----

    fn posting(id: &str, key: &str, valid_at: &str, amount: &str) -> Posting {
        Posting {
            entry_id: id.into(),
            class: "Entry".into(),
            ledger_key: json!({"account": key}),
            valid_at: valid_at.into(),
            body: json!({"amount": usd(amount)}),
        }
    }

    #[test]
    fn a_restated_posting_proposes_the_difference_on_its_key() {
        // DSN-DVM-323, acceptance 1: a restated fee that changes by +120
        let original = [
            posting("e-1", "fees", "2026-02-15", "1000.00"),
            posting("e-2", "cash", "2026-02-15", "-1000.00"),
        ];
        let restated = [
            posting("e-3", "fees", "2026-02-15", "1120.00"),
            posting("e-4", "cash", "2026-02-15", "-1120.00"),
        ];
        let amounts = fields(&[("Entry", &["amount"])]);
        let out = proposals(
            &original,
            &restated,
            &amounts,
            Dating::Original,
            "2026-03-05",
        )
        .expect("proposals");
        assert_eq!(
            json!(out),
            json!([
                {"class": "Entry", "ledger_key": {"account": "cash"}, "valid_at": "2026-02-15",
                 "amounts": {"amount": [usd("-120.00")]}, "original": ["e-2"], "restated": ["e-4"]},
                {"class": "Entry", "ledger_key": {"account": "fees"}, "valid_at": "2026-02-15",
                 "amounts": {"amount": [usd("120.00")]}, "original": ["e-1"], "restated": ["e-3"]},
            ])
        );
        let now = proposals(
            &original,
            &restated,
            &amounts,
            Dating::Current,
            "2026-03-05",
        )
        .expect("proposals");
        assert!(now.iter().all(|p| p["valid_at"] == "2026-03-05"));
    }

    #[test]
    fn unchanged_keys_propose_nothing_and_new_or_dropped_keys_their_whole_amount() {
        let original = [
            posting("e-1", "a", "2026-02-15", "10.00"),
            posting("e-2", "gone", "2026-02-10", "4.00"),
        ];
        let restated = [
            posting("e-3", "a", "2026-02-15", "10.00"),
            posting("e-4", "new", "2026-02-20", "6.00"),
        ];
        let out = proposals(
            &original,
            &restated,
            &fields(&[("Entry", &["amount"])]),
            Dating::Original,
            "2026-03-05",
        )
        .expect("proposals");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["ledger_key"], json!({"account": "gone"}));
        assert_eq!(out[0]["amounts"]["amount"], json!([usd("-4.00")]));
        assert_eq!(out[0]["restated"], json!([]));
        assert_eq!(out[1]["ledger_key"], json!({"account": "new"}));
        assert_eq!(out[1]["valid_at"], "2026-02-20");
        assert_eq!(out[1]["original"], json!([]));
        assert_eq!(
            Dating::parse(Some(&json!("current"))).ok(),
            Some(Dating::Current)
        );
        assert_eq!(Dating::parse(None).ok(), Some(Dating::Original));
        assert!(Dating::parse(Some(&json!("later"))).is_err());
    }

    // ---- properties ----

    /// A small deterministic generator (SplitMix64).
    struct Gen(u64);

    impl Gen {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn state(g: &mut Gen) -> Option<&'static str> {
        [Some(OPEN), Some(CLOSING), Some(CLOSED), None][g.below(4) as usize]
    }

    /// FR-LEDG-061: whatever sequence of updates and operations is tried, a period is
    /// `closed` exactly when its last successful transition was a close, and every close
    /// started from `closing`.
    #[test]
    fn property_closed_is_reached_through_close_only() {
        let mut g = Gen(7);
        for _ in 0..500 {
            let mut cur = Some(OPEN);
            let mut closes = 0u64;
            for _ in 0..40 {
                let (new, via) = (
                    state(&mut g),
                    [Via::Update, Via::Close, Via::Reopen][g.below(3) as usize],
                );
                // the operations name their own target state
                let new = match via {
                    Via::Close => Some(CLOSED),
                    Via::Reopen => Some(OPEN),
                    Via::Update => new,
                };
                let ok = match via {
                    Via::Close => check_close(cur).is_ok(),
                    Via::Reopen => check_reopen(cur).is_ok(),
                    Via::Update => true,
                } && check_transition(cur, new, via).is_ok();
                if !ok {
                    continue;
                }
                if new == Some(CLOSED) && cur != new {
                    assert_eq!((cur, via), (Some(CLOSING), Via::Close));
                    closes += 1;
                }
                if cur == Some(CLOSED) && cur != new {
                    assert_eq!((new, via), (Some(OPEN), Via::Reopen));
                }
                assert!(new.is_some(), "a period never loses its state");
                cur = new;
            }
            let _ = closes;
        }
    }

    fn random_entries(g: &mut Gen, n: u64) -> Vec<(Value, String, Value)> {
        (0..n)
            .map(|_| {
                let key = json!({"account": format!("a{}", g.below(4))});
                let date = format!("2026-{:02}-{:02}", 1 + g.below(4), 1 + g.below(28));
                let cents = g.below(20_000) as i64 - 10_000;
                let amount = format!(
                    "{}{}.{:02}",
                    if cents < 0 { "-" } else { "" },
                    cents.abs() / 100,
                    cents.abs() % 100
                );
                let currency = ["USD", "USD", "EUR"][g.below(3) as usize];
                (
                    key,
                    date,
                    json!({"amount": {"amount": amount, "currency": currency}}),
                )
            })
            .collect()
    }

    fn cents(v: &Value) -> i64 {
        let t = v["amount"].as_str().expect("amount").replace('.', "");
        t.parse().expect("cents")
    }

    /// FR-LEDG-062: the balances count every entry posted before the end exactly once —
    /// entry counts and sums per currency add up to those of the entries — do not depend
    /// on the order of the keys, and moving the end later never loses an entry.
    #[test]
    fn property_balances_account_for_every_entry_before_the_end() {
        let mut g = Gen(11);
        let amounts = fields(&[("Entry", &["amount"])]);
        for _ in 0..300 {
            let n = 1 + g.below(30);
            let raw = random_entries(&mut g, n);
            let end = format!("2026-{:02}-01", 1 + g.below(6));
            let posted = |raw: &'_ [(Value, String, Value)]| -> Vec<Value> {
                let entries: Vec<Posted<'_>> = raw
                    .iter()
                    .map(|(k, d, b)| Posted {
                        class: "Entry",
                        ledger_key: k,
                        posting_date: Some(d),
                        body: b,
                    })
                    .collect();
                balances(&entries, &amounts, &end).expect("balances")
            };
            let out = posted(&raw);
            let before: Vec<&(Value, String, Value)> = raw.iter().filter(|e| e.1 < end).collect();
            let counted: u64 = out
                .iter()
                .map(|b| b["entries"].as_u64().expect("entries"))
                .sum();
            assert_eq!(counted, before.len() as u64);
            for currency in ["USD", "EUR"] {
                let want: i64 = before
                    .iter()
                    .filter(|e| e.2["amount"]["currency"] == currency)
                    .map(|e| cents(&e.2["amount"]))
                    .sum();
                let got: i64 = out
                    .iter()
                    .flat_map(|b| {
                        b["amounts"]["amount"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                    })
                    .filter(|m| m["currency"] == currency)
                    .map(|m| cents(&m))
                    .sum();
                assert_eq!(got, want, "{currency}");
            }
            // keys come in order, each once
            let keys: Vec<String> = out
                .iter()
                .map(|b| {
                    b["ledger_key"]["account"]
                        .as_str()
                        .expect("key")
                        .to_string()
                })
                .collect();
            let mut sorted = keys.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(keys, sorted);
            // the order of entries of different keys does not matter (sums are exact)
            let mut reversed = raw.clone();
            reversed.reverse();
            assert_eq!(json!(posted(&reversed)), json!(out));
        }
    }

    /// FR-TIME-045: applying the proposals to the original postings gives the restated
    /// totals — per key and currency, original plus proposed equals restated — and
    /// restating to the same postings proposes nothing.
    #[test]
    fn property_proposals_close_the_gap_between_original_and_restated() {
        let mut g = Gen(13);
        let amounts = fields(&[("Entry", &["amount"])]);
        let postings = |raw: &[(Value, String, Value)], prefix: &str| -> Vec<Posting> {
            raw.iter()
                .enumerate()
                .map(|(i, (k, d, b))| Posting {
                    entry_id: format!("{prefix}{i}"),
                    class: "Entry".into(),
                    ledger_key: k.clone(),
                    valid_at: d.clone(),
                    body: b.clone(),
                })
                .collect()
        };
        let sums = |list: &[Posting]| -> BTreeMap<(String, String), i64> {
            let mut m = BTreeMap::new();
            for p in list {
                let k = (
                    p.ledger_key["account"].as_str().expect("key").to_string(),
                    p.body["amount"]["currency"]
                        .as_str()
                        .expect("currency")
                        .to_string(),
                );
                *m.entry(k).or_insert(0) += cents(&p.body["amount"]);
            }
            m
        };
        for _ in 0..300 {
            let (n, m) = (g.below(12), g.below(12));
            let old = postings(&random_entries(&mut g, n), "o");
            let new = postings(&random_entries(&mut g, m), "n");
            let out =
                proposals(&old, &new, &amounts, Dating::Original, "2026-06-01").expect("proposals");
            let mut total = sums(&old);
            for p in &out {
                for m in p["amounts"]["amount"].as_array().expect("money") {
                    assert_ne!(cents(m), 0, "a zero difference proposes nothing");
                    let k = (
                        p["ledger_key"]["account"]
                            .as_str()
                            .expect("key")
                            .to_string(),
                        m["currency"].as_str().expect("currency").to_string(),
                    );
                    *total.entry(k).or_insert(0) += cents(m);
                }
            }
            total.retain(|_, v| *v != 0);
            let mut want = sums(&new);
            want.retain(|_, v| *v != 0);
            assert_eq!(total, want);
            assert!(
                proposals(&old, &old, &amounts, Dating::Original, "2026-06-01")
                    .expect("same")
                    .is_empty()
            );
        }
    }

    /// STD-TIME-042: a plan names every producing process exactly once, split by whether
    /// it is recorded, and a process never runs before one whose first commit is earlier.
    #[test]
    fn property_a_plan_orders_every_process_once() {
        let mut g = Gen(17);
        for _ in 0..300 {
            let results: Vec<Produced> = (0..g.below(20))
                .map(|_| produced(&format!("p{}", g.below(6)), g.below(30)))
                .collect();
            let p = plan(&results, |p| p != "p0");
            let all: BTreeSet<&str> = results.iter().map(|r| r.process.as_str()).collect();
            let named: Vec<&str> = p
                .rerun
                .iter()
                .chain(&p.not_rerun)
                .map(String::as_str)
                .collect();
            assert_eq!(named.len(), all.len());
            assert_eq!(named.iter().copied().collect::<BTreeSet<_>>(), all);
            assert!(p.not_rerun.iter().all(|x| x == "p0") && !p.rerun.iter().any(|x| x == "p0"));
            let first = |x: &str| {
                results
                    .iter()
                    .filter(|r| r.process == x)
                    .map(|r| r.cseq)
                    .min()
            };
            assert!(p.rerun.windows(2).all(|w| first(&w[0]) <= first(&w[1])));
        }
    }
}
