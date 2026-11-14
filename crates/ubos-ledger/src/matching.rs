//! Deterministic matching of two ledger sides and its match records (STD-KIND-027,
//! STD-KIND-028; DSN-DVM-445), and the key of an idempotent ingestion (DSN-DVM-446).
//!
//! Pure functions over entries given as values: the kernel reads the sides and the match
//! records of a match set, calls [`run`], and appends the records. The functions agree with
//! the reference model of BPA 1.0-rc8 (`standard/tools/bpa/periods.py`):
//!
//! - the candidates of each side are ordered by `(valid_at, entry_id)`;
//! - the rules are applied in the order given; `one_to_one` takes the first fit,
//!   `one_to_many` and `many_to_one` take the whole group of entries with equal `equal`
//!   fields within the date window and match it when the sum is within the tolerance;
//! - amounts are compared per currency, and different currencies never match;
//! - no subset of a group is searched, so the cost grows with the number of entries.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use ubos_types::decimal::Decimal;
use ubos_types::{BpaError, canonical, content_id};

/// The genesis ledger class of match records (STD-KIND-028).
pub const RECORD_CLASS: &str = "MatchRecord";
/// The ledger key field of a match record.
pub const RECORD_KEY: &str = "match_set";
/// The domain of the derived identifier of a match record.
pub const RECORD_DOMAIN: &str = "ubos.match.v1";
/// The domain of the key of an ingested line (DSN-DVM-446).
pub const INGEST_DOMAIN: &str = "ubos.ingest.v1";

fn incomplete(message: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", message)
}

fn invalid(message: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", message)
}

/// The kind of a matching rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One left entry with one right entry.
    OneToOne,
    /// One left entry with a group of right entries.
    OneToMany,
    /// A group of left entries with one right entry.
    ManyToOne,
}

/// A checked matching rule (`DAT-MatchingRule`).
#[derive(Debug, Clone)]
pub struct Rule {
    /// Rule name, unique in the list.
    pub id: String,
    /// Kind.
    pub kind: Kind,
    /// `(left field, right field)` pairs whose values must be equal.
    pub equal: Vec<(String, String)>,
    /// The amount field of the left and of the right side.
    pub amount: (String, String),
    /// Largest absolute difference of the amounts per currency.
    pub tolerance: Decimal,
    /// Largest distance of the `valid_at` dates in days.
    pub window_days: u64,
    /// Compare the left amount with the negated right amount.
    pub negate: bool,
    /// The rule as given; its content ID is the `rule_ref` of a match.
    pub source: Value,
}

/// The truth of an optional member, as the reference model reads it.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn pair(v: &Value) -> Option<(String, String)> {
    match v.as_array().map(Vec::as_slice) {
        Some([Value::String(a), Value::String(b)]) => Some((a.clone(), b.clone())),
        _ => None,
    }
}

impl Rule {
    /// Check one rule: a rule without an `id`, a `kind` or `amount` is incomplete; a
    /// malformed `equal`, a negative tolerance or window is invalid (STD-KIND-027).
    pub fn parse(r: &Value) -> Result<Rule, BpaError> {
        let kind = match r.get("kind").and_then(Value::as_str) {
            Some("one_to_one") => Some(Kind::OneToOne),
            Some("one_to_many") => Some(Kind::OneToMany),
            Some("many_to_one") => Some(Kind::ManyToOne),
            _ => None,
        };
        let (Some(id), Some(kind)) = (r.get("id").and_then(Value::as_str), kind) else {
            return Err(incomplete("a matching rule has an id and a kind"));
        };
        let amount = r
            .get("amount")
            .and_then(pair)
            .ok_or_else(|| incomplete("a matching rule names the amount field of each side"))?;
        let mut equal = Vec::new();
        if truthy(r.get("equal")) {
            let bad = || invalid("equal is a list of [left field, right field]");
            for p in r["equal"].as_array().ok_or_else(bad)? {
                equal.push(pair(p).ok_or_else(bad)?);
            }
        }
        let tolerance = match r.get("tolerance") {
            None => Decimal::parse("0")?,
            Some(Value::String(t)) => Decimal::parse(t)?,
            Some(_) => return Err(invalid("not a decimal string")),
        };
        if tolerance < Decimal::parse("0")? {
            return Err(invalid("a tolerance is not negative"));
        }
        let window_days = match r.get("window_days") {
            None => 0,
            Some(w) => w
                .as_u64()
                .ok_or_else(|| invalid("window_days is a non-negative integer"))?,
        };
        Ok(Rule {
            id: id.to_string(),
            kind,
            equal,
            amount,
            tolerance,
            window_days,
            negate: truthy(r.get("negate")),
            source: r.clone(),
        })
    }

    /// The content ID of the rule as given: the rule version a match record names
    /// (FR-LEDG-073).
    pub fn rule_ref(&self) -> Result<String, BpaError> {
        content_id("ubos.chunk.v1", &self.source)
    }
}

/// Check a list of rules: not empty, every rule well formed, identifiers unique.
pub fn parse_rules(rules: Option<&Value>) -> Result<Vec<Rule>, BpaError> {
    let list = rules
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .ok_or_else(|| incomplete("rules"))?;
    let out = list
        .iter()
        .map(Rule::parse)
        .collect::<Result<Vec<_>, _>>()?;
    let ids: BTreeSet<&str> = out.iter().map(|r| r.id.as_str()).collect();
    if ids.len() != out.len() {
        return Err(invalid("rule ids are unique"));
    }
    Ok(out)
}

/// An entry of a side.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Entry identifier.
    pub id: String,
    /// Business effect date or instant.
    pub valid_at: String,
    /// Field values.
    pub body: Value,
}

/// A match made by a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The rule's identifier.
    pub rule: String,
    /// The content ID of the rule.
    pub rule_ref: String,
    /// Matched entries of the left side.
    pub left: Vec<String>,
    /// Matched entries of the right side.
    pub right: Vec<String>,
}

/// The result of a matching run: the matches in the order made and the unmatched entries
/// of each side in candidate order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Matches in the order made.
    pub matches: Vec<Match>,
    /// Unmatched entries of the left side.
    pub breaks_left: Vec<String>,
    /// Unmatched entries of the right side.
    pub breaks_right: Vec<String>,
}

type Sums = BTreeMap<String, Decimal>;

/// Add an amount (a decimal, or money `{amount, currency}`) to the sums per currency; a
/// plain decimal counts under the empty currency.
fn add(acc: &mut Sums, v: Option<&Value>) -> Result<(), BpaError> {
    let (currency, amount) = match v {
        Some(Value::Object(m)) => (
            m.get("currency")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("an amount has a currency"))?,
            m.get("amount"),
        ),
        other => ("", other),
    };
    let d = Decimal::parse(
        amount
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("not a decimal string"))?,
    )?;
    let sum = match acc.get(currency) {
        Some(cur) => cur.add(&d),
        None => d,
    };
    acc.insert(currency.to_string(), sum);
    Ok(())
}

/// Whether the sums of two groups agree within the rule's tolerance in every currency.
fn within(ls: &[&Candidate], rs: &[&Candidate], rule: &Rule) -> Result<bool, BpaError> {
    let (mut tl, mut tr) = (Sums::new(), Sums::new());
    for e in ls {
        add(&mut tl, e.body.get(&rule.amount.0))?;
    }
    for e in rs {
        add(&mut tr, e.body.get(&rule.amount.1))?;
    }
    if !tl.keys().eq(tr.keys()) {
        return Ok(false);
    }
    let zero = Decimal::parse("0")?;
    Ok(tl.iter().all(|(c, l)| {
        let r = &tr[c];
        let d = if rule.negate { l.add(r) } else { l.sub(r) };
        let d = if d < zero { d.neg() } else { d };
        d <= rule.tolerance
    }))
}

/// The day number of the date part of a `valid_at`.
fn day(valid_at: &str) -> Result<i64, BpaError> {
    let d = ubos_types::time::parse_date(valid_at.get(..10).unwrap_or(valid_at))?;
    let epoch = ubos_types::time::parse_date("1970-01-01")?;
    Ok((d - epoch).num_days())
}

/// The values of the `equal` fields of an entry, as a key; a missing field counts as null.
fn equal_key(e: &Candidate, rule: &Rule, right: bool) -> Result<String, BpaError> {
    let vals: Vec<Value> = rule
        .equal
        .iter()
        .map(|(a, b)| {
            e.body
                .get(if right { b } else { a })
                .cloned()
                .unwrap_or(Value::Null)
        })
        .collect();
    canonical(&Value::Array(vals))
}

/// A side prepared for one run: candidates in `(valid_at, entry_id)` order with their day.
struct Side<'a> {
    entries: Vec<&'a Candidate>,
    days: Vec<i64>,
    used: Vec<bool>,
}

impl<'a> Side<'a> {
    fn new(entries: &'a [Candidate], used: &BTreeSet<String>) -> Result<Side<'a>, BpaError> {
        let mut entries: Vec<&Candidate> = entries.iter().collect();
        entries.sort_by(|a, b| (&a.valid_at, &a.id).cmp(&(&b.valid_at, &b.id)));
        Ok(Side {
            days: entries
                .iter()
                .map(|e| day(&e.valid_at))
                .collect::<Result<_, _>>()?,
            used: entries.iter().map(|e| used.contains(&e.id)).collect(),
            entries,
        })
    }

    /// The positions of the entries by the value of their `equal` fields, in order.
    fn index(&self, rule: &Rule, right: bool) -> Result<HashMap<String, Vec<usize>>, BpaError> {
        let mut out: HashMap<String, Vec<usize>> = HashMap::new();
        for (n, e) in self.entries.iter().enumerate() {
            out.entry(equal_key(e, rule, right)?).or_default().push(n);
        }
        Ok(out)
    }

    fn breaks(&self) -> Vec<String> {
        (0..self.entries.len())
            .filter(|n| !self.used[*n])
            .map(|n| self.entries[n].id.clone())
            .collect()
    }
}

/// Run the rules in order over the entries of two sides (STD-KIND-027). `used_left` and
/// `used_right` are the entries that active match records of the match set reference: they
/// are neither candidates nor breaks. The same inputs, in any order of the entries, give
/// the same result.
pub fn run(
    rules: &[Rule],
    left: &[Candidate],
    right: &[Candidate],
    used_left: &BTreeSet<String>,
    used_right: &BTreeSet<String>,
) -> Result<Outcome, BpaError> {
    let mut l = Side::new(left, used_left)?;
    let mut r = Side::new(right, used_right)?;
    let mut matches = Vec::new();
    for rule in rules {
        let rule_ref = rule.rule_ref()?;
        // the side whose entries are taken one at a time, and the side searched for them
        let one_is_left = rule.kind != Kind::ManyToOne;
        let (one, many) = if one_is_left {
            (&mut l, &mut r)
        } else {
            (&mut r, &mut l)
        };
        // the entries of the searched side by their `equal` values: only they can satisfy
        // `equal`, and each bucket keeps the candidate order
        let index = many.index(rule, one_is_left)?;
        for x in 0..one.entries.len() {
            if one.used[x] {
                continue;
            }
            let ex = one.entries[x];
            let Some(bucket) = index.get(&equal_key(ex, rule, !one_is_left)?) else {
                continue;
            };
            let near = |y: usize| (one.days[x] - many.days[y]).unsigned_abs() <= rule.window_days;
            let mut group: Vec<usize> = Vec::new();
            if rule.kind == Kind::OneToOne {
                for &y in bucket {
                    if !many.used[y] && near(y) && within(&[ex], &[many.entries[y]], rule)? {
                        group.push(y);
                        break;
                    }
                }
            } else {
                let g: Vec<usize> = bucket
                    .iter()
                    .copied()
                    .filter(|&y| !many.used[y] && near(y))
                    .collect();
                let es: Vec<&Candidate> = g.iter().map(|&y| many.entries[y]).collect();
                let ok = !g.is_empty()
                    && if one_is_left {
                        within(&[ex], &es, rule)?
                    } else {
                        within(&es, &[ex], rule)?
                    };
                if ok {
                    group = g;
                }
            }
            if group.is_empty() {
                continue;
            }
            one.used[x] = true;
            let mut ids: Vec<String> = Vec::new();
            for &y in &group {
                many.used[y] = true;
                ids.push(many.entries[y].id.clone());
            }
            ids.sort();
            let (left, right) = if one_is_left {
                (vec![ex.id.clone()], ids)
            } else {
                (ids, vec![ex.id.clone()])
            };
            matches.push(Match {
                rule: rule.id.clone(),
                rule_ref: rule_ref.clone(),
                left,
                right,
            });
        }
    }
    Ok(Outcome {
        matches,
        breaks_left: l.breaks(),
        breaks_right: r.breaks(),
    })
}

// ---- match records (STD-KIND-028) ----

/// A match record of a match set as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Entry identifier.
    pub id: String,
    /// Body: `{match_set, left, right, rule, rule_ref, actor, unmatches?, reason?}`.
    pub body: Value,
}

fn undoes(r: &Record) -> Option<&str> {
    r.body
        .get("unmatches")
        .and_then(Value::as_str)
        .filter(|u| !u.is_empty())
}

/// The active records of a match set: those that are no unmatch and that no unmatch names.
pub fn active(records: &[Record]) -> Vec<&Record> {
    let undone: BTreeSet<&str> = records.iter().filter_map(undoes).collect();
    records
        .iter()
        .filter(|r| undoes(r).is_none() && !undone.contains(r.id.as_str()))
        .collect()
}

fn ids(v: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
}

/// The entries of the left and of the right side that the active records reference.
pub fn used(records: &[Record]) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut l, mut r) = (BTreeSet::new(), BTreeSet::new());
    for rec in active(records) {
        l.extend(ids(rec.body.get("left")));
        r.extend(ids(rec.body.get("right")));
    }
    (l, r)
}

/// The body of the record of a match.
pub fn record_body(match_set: &str, m: &Match, actor: &str) -> Value {
    json!({"match_set": match_set, "left": m.left, "right": m.right, "rule": m.rule,
           "rule_ref": m.rule_ref, "actor": actor})
}

/// The body of the record that undoes the record `entry_id` of a match set: the same
/// `left`, `right` and rule, with `unmatches` and an optional reason. An unknown record
/// fails with `IAM.NOT_FOUND`, one that is not active with `MODEL.CONSTRAINT_VIOLATED`.
pub fn undo_body(
    records: &[Record],
    match_set: &Value,
    entry_id: &str,
    actor: &str,
    reason: Option<&Value>,
) -> Result<Value, BpaError> {
    let target = records
        .iter()
        .find(|r| r.id == entry_id)
        .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown match record"))?;
    if !active(records).iter().any(|r| r.id == target.id) {
        return Err(BpaError::new(
            "MODEL.CONSTRAINT_VIOLATED",
            "the record is an unmatch or is already undone",
        ));
    }
    let mut body = Map::new();
    body.insert(RECORD_KEY.into(), match_set.clone());
    for k in ["left", "right", "rule", "rule_ref"] {
        body.insert(k.into(), target.body.get(k).cloned().unwrap_or(Value::Null));
    }
    body.insert("actor".into(), json!(actor));
    body.insert("unmatches".into(), json!(target.id));
    if let Some(r) = reason.filter(|r| !r.is_null()) {
        body.insert("reason".into(), r.clone());
    }
    Ok(Value::Object(body))
}

/// A UUID (version 8, RFC 9562) whose digits are the first 32 hexadecimal digits of the
/// SHA-256 of the canonical JSON `[domain, content]`, with the version and variant bits set.
pub fn derived_uuid(domain: &str, content: &Value) -> Result<String, BpaError> {
    let text = canonical(&json!([domain, content]))?;
    let h: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let variant = 8 | (u8::from_str_radix(&h[16..17], 16).unwrap_or_default() & 3);
    Ok(format!(
        "{}-{}-8{}-{variant:x}{}-{}",
        &h[..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    ))
}

/// The identifier of a match record, derived from its body: the same match in the same
/// set has the same identifier (STD-KIND-028).
pub fn record_id(body: &Value) -> Result<String, BpaError> {
    derived_uuid(RECORD_DOMAIN, body)
}

// ---- idempotent ingestion (DSN-DVM-446) ----

/// The idempotency key of one ingested line, derived from the source, the ledger key the
/// line is held under, the statement, the line number and the hash of the line's content:
/// a line delivered again has the same key and creates no duplicate (STD-TXN-040).
pub fn ingestion_key(
    source: &str,
    holder: &Value,
    statement: &str,
    line: u64,
    content: &Value,
) -> Result<String, BpaError> {
    let hash = content_id("ubos.chunk.v1", content)?;
    content_id(
        "ubos.chunk.v1",
        &json!([INGEST_DOMAIN, source, holder, statement, line, hash]),
    )
}

/// The identifier of the entry of an ingested line, derived from its key: a line delivered
/// again after its idempotency record expired still cannot be appended twice.
pub fn ingestion_id(key: &str) -> Result<String, BpaError> {
    derived_uuid(INGEST_DOMAIN, &json!(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, reference: &str, amount: &str, valid_at: &str, currency: &str) -> Candidate {
        Candidate {
            id: id.into(),
            valid_at: valid_at.into(),
            body: json!({"reference": reference,
                         "amount": {"amount": amount, "currency": currency}}),
        }
    }

    fn usd(id: &str, reference: &str, amount: &str) -> Candidate {
        line(id, reference, amount, "2026-03-10", "USD")
    }

    fn rule(id: &str, kind: &str, tolerance: &str, window: u64) -> Value {
        json!({"id": id, "kind": kind, "equal": [["reference", "reference"]],
               "amount": ["amount", "amount"], "tolerance": tolerance, "window_days": window})
    }

    fn go(rules: &[Value], left: &[Candidate], right: &[Candidate]) -> Outcome {
        let rules = parse_rules(Some(&json!(rules))).expect("rules");
        run(&rules, left, right, &BTreeSet::new(), &BTreeSet::new()).expect("run")
    }

    fn pairs(o: &Outcome) -> Vec<(Vec<&str>, Vec<&str>)> {
        o.matches
            .iter()
            .map(|m| {
                (
                    m.left.iter().map(String::as_str).collect(),
                    m.right.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn one_to_one_takes_the_first_fit_in_candidate_order() {
        let left = [usd("a", "INV-1", "100.00"), usd("b", "INV-2", "50.00")];
        let right = [usd("x", "INV-2", "50.00"), usd("y", "INV-1", "100.00")];
        let o = go(&[rule("exact", "one_to_one", "0", 0)], &left, &right);
        assert_eq!(
            pairs(&o),
            vec![(vec!["a"], vec!["y"]), (vec!["b"], vec!["x"])]
        );
        assert!(o.breaks_left.is_empty() && o.breaks_right.is_empty());
        assert_eq!(o.matches[0].rule, "exact");
        assert!(o.matches[0].rule_ref.starts_with("sha256:"));
    }

    #[test]
    fn a_tolerance_bounds_the_difference() {
        let left = [usd("a", "INV-1", "100.00")];
        let right = [usd("x", "INV-1", "99.95")];
        let o = go(&[rule("tol", "one_to_one", "0.10", 0)], &left, &right);
        assert_eq!(o.matches.len(), 1);
        let o = go(&[rule("tol", "one_to_one", "0.05", 0)], &left, &right);
        assert_eq!(o.matches.len(), 1, "the bound is inclusive");
        let o = go(&[rule("exact", "one_to_one", "0", 0)], &left, &right);
        assert!(o.matches.is_empty());
        assert_eq!(
            (o.breaks_left, o.breaks_right),
            (vec!["a".to_string()], vec!["x".to_string()])
        );
    }

    #[test]
    fn a_window_bounds_the_distance_of_the_dates() {
        let left = [
            line("a", "INV-1", "100.00", "2026-03-01", "USD"),
            line("b", "INV-2", "40.00", "2026-03-01T08:00:00.000000Z", "USD"),
        ];
        let right = [
            line("x", "INV-1", "100.00", "2026-03-04", "USD"),
            line("y", "INV-2", "40.00", "2026-03-09", "USD"),
        ];
        let o = go(&[rule("window", "one_to_one", "0", 3)], &left, &right);
        assert_eq!(pairs(&o), vec![(vec!["a"], vec!["x"])]);
        assert_eq!(o.breaks_left, vec!["b"]);
        assert_eq!(o.breaks_right, vec!["y"]);
    }

    #[test]
    fn groups_match_on_their_sum_without_a_subset_search() {
        let left = [usd("a", "INV-1", "100.00")];
        let right = [
            usd("z", "INV-1", "30.00"),
            usd("x", "INV-1", "60.00"),
            usd("y", "INV-1", "10.00"),
        ];
        let o = go(&[rule("split", "one_to_many", "0", 0)], &left, &right);
        assert_eq!(pairs(&o), vec![(vec!["a"], vec!["x", "y", "z"])]);
        // the sides exchanged
        let o = go(&[rule("split", "many_to_one", "0", 0)], &right, &left);
        assert_eq!(pairs(&o), vec![(vec!["x", "y", "z"], vec!["a"])]);
        // a group that sums to more is a break, although a subset would fit
        let more = [usd("x", "INV-1", "100.00"), usd("y", "INV-1", "5.00")];
        let o = go(&[rule("split", "one_to_many", "0", 0)], &left, &more);
        assert!(o.matches.is_empty());
        assert_eq!(o.breaks_right, vec!["x", "y"]);
    }

    #[test]
    fn currencies_never_mix_and_negate_compares_the_opposite_sign() {
        let left = [usd("a", "INV-1", "100.00")];
        let eur = [line("x", "INV-1", "100.00", "2026-03-10", "EUR")];
        assert!(
            go(&[rule("exact", "one_to_one", "1000", 0)], &left, &eur)
                .matches
                .is_empty()
        );
        let neg = [usd("x", "INV-1", "-100.00")];
        let mut r = rule("neg", "one_to_one", "0", 0);
        assert!(go(&[r.clone()], &left, &neg).matches.is_empty());
        r["negate"] = json!(true);
        assert_eq!(go(&[r], &left, &neg).matches.len(), 1);
        // plain decimals compare under no currency
        let l = [Candidate {
            id: "a".into(),
            valid_at: "2026-03-10".into(),
            body: json!({"amount": "5"}),
        }];
        let rr = [Candidate {
            id: "x".into(),
            valid_at: "2026-03-10".into(),
            body: json!({"amount": "5.0"}),
        }];
        let plain = json!({"id": "p", "kind": "one_to_one", "amount": ["amount", "amount"]});
        assert_eq!(go(&[plain], &l, &rr).matches.len(), 1);
    }

    #[test]
    fn rules_apply_in_priority_order_and_used_entries_are_no_candidates() {
        let left = [usd("a", "INV-1", "100.00"), usd("b", "INV-2", "50.00")];
        let right = [usd("x", "INV-1", "99.99"), usd("y", "INV-2", "50.00")];
        let rules = [
            rule("exact", "one_to_one", "0", 0),
            rule("tol", "one_to_one", "0.05", 0),
        ];
        let o = go(&rules, &left, &right);
        let by: Vec<&str> = o.matches.iter().map(|m| m.rule.as_str()).collect();
        assert_eq!(by, vec!["exact", "tol"]);
        assert_eq!(
            pairs(&o),
            vec![(vec!["b"], vec!["y"]), (vec!["a"], vec!["x"])]
        );
        // entries of active records are neither matched nor reported as breaks
        let rules = parse_rules(Some(&json!(rules))).expect("rules");
        let used_l: BTreeSet<String> = ["b".to_string()].into();
        let used_r: BTreeSet<String> = ["y".to_string()].into();
        let o = run(&rules, &left, &right, &used_l, &used_r).expect("run");
        assert_eq!(pairs(&o), vec![(vec!["a"], vec!["x"])]);
        assert!(o.breaks_left.is_empty() && o.breaks_right.is_empty());
    }

    #[test]
    fn malformed_rules_are_rejected() {
        let code = |v: Value| parse_rules(Some(&v)).expect_err("rejected").code;
        assert_eq!(code(json!([])), "MODEL.DECLARATION_INCOMPLETE");
        assert_eq!(code(json!("x")), "MODEL.DECLARATION_INCOMPLETE");
        assert_eq!(
            code(json!([{"kind": "one_to_one", "amount": ["a", "a"]}])),
            "MODEL.DECLARATION_INCOMPLETE"
        );
        assert_eq!(
            code(json!([{"id": "r", "kind": "subset", "amount": ["a", "a"]}])),
            "MODEL.DECLARATION_INCOMPLETE"
        );
        assert_eq!(
            code(json!([{"id": "r", "kind": "one_to_one"}])),
            "MODEL.DECLARATION_INCOMPLETE"
        );
        let with = |k: &str, v: Value| {
            let mut r = json!({"id": "r", "kind": "one_to_one", "amount": ["a", "a"]});
            r[k] = v;
            json!([r])
        };
        assert_eq!(code(with("equal", json!([["a"]]))), "MODEL.INVALID_VALUE");
        assert_eq!(
            code(with("tolerance", json!("-0.01"))),
            "MODEL.INVALID_VALUE"
        );
        assert_eq!(code(with("tolerance", json!(1))), "MODEL.INVALID_VALUE");
        assert_eq!(code(with("window_days", json!(-1))), "MODEL.INVALID_VALUE");
        assert_eq!(
            code(with("window_days", json!(true))),
            "MODEL.INVALID_VALUE"
        );
        let r = json!({"id": "r", "kind": "one_to_one", "amount": ["a", "a"]});
        assert_eq!(code(json!([r.clone(), r])), "MODEL.INVALID_VALUE");
    }

    /// The identifier and the rule reference of the first record of VER-CONF-1185.
    #[test]
    fn record_identifiers_are_those_of_the_vectors() {
        let r = Rule::parse(&rule("exact", "one_to_one", "0", 0)).expect("rule");
        let rule_ref = r.rule_ref().expect("ref");
        assert_eq!(
            rule_ref,
            "sha256:3624504dd6c2e84cc0989f7ad3185e86af2eb9ec86975a7d677044ad10695ee2"
        );
        let m = Match {
            rule: "exact".into(),
            rule_ref,
            left: vec!["01890a5d-ac96-700a-8000-00000000000a".into()],
            right: vec!["01890a5d-ac96-7014-8000-000000000014".into()],
        };
        let body = record_body(
            "bank-1010",
            &m,
            "did:ubos:acme:01900000-0000-7000-8000-000000000002",
        );
        assert_eq!(
            record_id(&body).expect("id"),
            "fa0c0e7b-7b0e-8c04-a771-ca1b71dde0fa"
        );
    }

    #[test]
    fn an_unmatch_supersedes_a_record_once() {
        let m = Match {
            rule: "r".into(),
            rule_ref: "sha256:00".into(),
            left: vec!["a".into()],
            right: vec!["x".into()],
        };
        let body = record_body("s", &m, "actor");
        let first = Record {
            id: record_id(&body).expect("id"),
            body,
        };
        let mut records = vec![first.clone()];
        assert_eq!(active(&records).len(), 1);
        assert_eq!(used(&records).0.len(), 1);
        let set = json!("s");
        let undo =
            undo_body(&records, &set, &first.id, "actor", Some(&json!("wrong"))).expect("undo");
        assert_eq!(undo["unmatches"], json!(first.id));
        assert_eq!(undo["reason"], "wrong");
        assert_eq!(undo["left"], json!(["a"]));
        let second = Record {
            id: record_id(&undo).expect("id"),
            body: undo,
        };
        assert_ne!(second.id, first.id);
        records.push(second.clone());
        // both records stay; none is active, and the entries are candidates again
        assert!(active(&records).is_empty());
        assert!(used(&records).0.is_empty());
        let again = undo_body(&records, &set, &first.id, "actor", None).expect_err("once");
        assert_eq!(again.code, "MODEL.CONSTRAINT_VIOLATED");
        let of_undo = undo_body(&records, &set, &second.id, "actor", None).expect_err("an unmatch");
        assert_eq!(of_undo.code, "MODEL.CONSTRAINT_VIOLATED");
        let unknown = undo_body(&records, &set, "nope", "actor", None).expect_err("unknown");
        assert_eq!(unknown.code, "IAM.NOT_FOUND");
    }

    #[test]
    fn ingestion_keys_follow_every_part() {
        let holder = json!({"holder": "h-1"});
        let content = json!({"body": {"amount": "1.00"}});
        let k = ingestion_key("feed", &holder, "2026-03", 1, &content).expect("key");
        assert_eq!(
            k,
            ingestion_key("feed", &holder, "2026-03", 1, &content).expect("key")
        );
        let others = [
            ingestion_key("other", &holder, "2026-03", 1, &content),
            ingestion_key("feed", &json!({"holder": "h-2"}), "2026-03", 1, &content),
            ingestion_key("feed", &holder, "2026-04", 1, &content),
            ingestion_key("feed", &holder, "2026-03", 2, &content),
            ingestion_key(
                "feed",
                &holder,
                "2026-03",
                1,
                &json!({"body": {"amount": "2.00"}}),
            ),
        ];
        for o in others {
            assert_ne!(o.expect("key"), k);
        }
        let id = ingestion_id(&k).expect("id");
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "8");
    }

    // ---- properties (DSN-DVM-445: matched twice, the results are identical) ----

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
        fn shuffle<T>(&mut self, v: &mut [T]) {
            for i in (1..v.len()).rev() {
                v.swap(i, self.below(i as u64 + 1) as usize);
            }
        }
    }

    fn side(g: &mut Gen, prefix: &str, n: u64) -> Vec<Candidate> {
        (0..n)
            .map(|i| {
                let amount = format!("{}.{:02}", g.below(4) as i64 - 1, g.below(3) * 5);
                let currency = ["USD", "USD", "EUR"][g.below(3) as usize];
                let mut body = json!({"amount": {"amount": amount, "currency": currency}});
                if g.below(8) > 0 {
                    body["reference"] = json!(format!("R{}", g.below(4)));
                }
                Candidate {
                    id: format!("{prefix}{:03}", g.below(1000) * 1000 + i),
                    valid_at: format!("2026-03-{:02}", 1 + g.below(6)),
                    body,
                }
            })
            .collect()
    }

    fn rules(g: &mut Gen) -> Vec<Value> {
        (0..1 + g.below(3))
            .map(|n| {
                let mut r = rule(
                    &format!("r{n}"),
                    ["one_to_one", "one_to_many", "many_to_one"][g.below(3) as usize],
                    ["0", "0.05", "1"][g.below(3) as usize],
                    g.below(4),
                );
                if g.below(4) == 0 {
                    r["equal"] = json!([]);
                }
                if g.below(4) == 0 {
                    r["negate"] = json!(true);
                }
                r
            })
            .collect()
    }

    /// The reference model's loops, entry against entry, without the index of `run`.
    fn naive(rules: &[Rule], left: &[Candidate], right: &[Candidate]) -> Outcome {
        let order = |v: &[Candidate]| {
            let mut v = v.to_vec();
            v.sort_by(|a, b| (&a.valid_at, &a.id).cmp(&(&b.valid_at, &b.id)));
            v
        };
        let (left, right) = (order(left), order(right));
        let (mut used_l, mut used_r) = (BTreeSet::new(), BTreeSet::new());
        let eq = |l: &Candidate, r: &Candidate, rule: &Rule| {
            rule.equal.iter().all(|(a, b)| {
                l.body.get(a).unwrap_or(&Value::Null) == r.body.get(b).unwrap_or(&Value::Null)
            })
        };
        let near = |l: &Candidate, r: &Candidate, rule: &Rule| {
            (day(&l.valid_at).expect("day") - day(&r.valid_at).expect("day")).unsigned_abs()
                <= rule.window_days
        };
        let mut matches = Vec::new();
        for rule in rules {
            let rule_ref = rule.rule_ref().expect("ref");
            let mut push = |left: Vec<String>, right: Vec<String>| {
                matches.push(Match {
                    rule: rule.id.clone(),
                    rule_ref: rule_ref.clone(),
                    left,
                    right,
                })
            };
            match rule.kind {
                Kind::OneToOne => {
                    for l in &left {
                        if used_l.contains(&l.id) {
                            continue;
                        }
                        for r in &right {
                            if used_r.contains(&r.id) || !eq(l, r, rule) || !near(l, r, rule) {
                                continue;
                            }
                            if within(&[l], &[r], rule).expect("within") {
                                used_l.insert(l.id.clone());
                                used_r.insert(r.id.clone());
                                push(vec![l.id.clone()], vec![r.id.clone()]);
                                break;
                            }
                        }
                    }
                }
                Kind::OneToMany => {
                    for x in &left {
                        if used_l.contains(&x.id) {
                            continue;
                        }
                        let group: Vec<&Candidate> = right
                            .iter()
                            .filter(|y| {
                                !used_r.contains(&y.id) && eq(x, y, rule) && near(x, y, rule)
                            })
                            .collect();
                        if group.is_empty() || !within(&[x], &group, rule).expect("within") {
                            continue;
                        }
                        used_l.insert(x.id.clone());
                        let mut ids: Vec<String> = group.iter().map(|y| y.id.clone()).collect();
                        used_r.extend(ids.iter().cloned());
                        ids.sort();
                        push(vec![x.id.clone()], ids);
                    }
                }
                Kind::ManyToOne => {
                    for x in &right {
                        if used_r.contains(&x.id) {
                            continue;
                        }
                        let group: Vec<&Candidate> = left
                            .iter()
                            .filter(|y| {
                                !used_l.contains(&y.id) && eq(y, x, rule) && near(y, x, rule)
                            })
                            .collect();
                        if group.is_empty() || !within(&group, &[x], rule).expect("within") {
                            continue;
                        }
                        used_r.insert(x.id.clone());
                        let mut ids: Vec<String> = group.iter().map(|y| y.id.clone()).collect();
                        used_l.extend(ids.iter().cloned());
                        ids.sort();
                        push(ids, vec![x.id.clone()]);
                    }
                }
            }
        }
        let rest = |v: &[Candidate], used: &BTreeSet<String>| {
            v.iter()
                .filter(|e| !used.contains(&e.id))
                .map(|e| e.id.clone())
                .collect()
        };
        Outcome {
            breaks_left: rest(&left, &used_l),
            breaks_right: rest(&right, &used_r),
            matches,
        }
    }

    #[test]
    fn matching_is_deterministic_and_partitions_the_candidates() {
        let mut g = Gen(20261114);
        let mut matched = 0usize;
        for _ in 0..600 {
            let n = 1 + g.below(12);
            let mut left = side(&mut g, "L", n);
            let m = 1 + g.below(12);
            let mut right = side(&mut g, "R", m);
            let rules = parse_rules(Some(&json!(rules(&mut g)))).expect("rules");
            let none = BTreeSet::new();
            let first = run(&rules, &left, &right, &none, &none).expect("run");
            // matched twice, the results are identical
            assert_eq!(
                first,
                run(&rules, &left, &right, &none, &none).expect("run")
            );
            // the order in which the entries are given does not matter
            g.shuffle(&mut left);
            g.shuffle(&mut right);
            assert_eq!(
                first,
                run(&rules, &left, &right, &none, &none).expect("run")
            );
            // the result is that of the reference model's loops
            assert_eq!(first, naive(&rules, &left, &right));
            // every entry is in exactly one match or is a break
            for (all, breaks, pick) in [
                (&left, &first.breaks_left, 0),
                (&right, &first.breaks_right, 1),
            ] {
                let mut seen: Vec<&String> = breaks.iter().collect();
                for m in &first.matches {
                    seen.extend(if pick == 0 { &m.left } else { &m.right });
                    assert!(!m.left.is_empty() && !m.right.is_empty());
                }
                let mut want: Vec<&String> = all.iter().map(|e| &e.id).collect();
                seen.sort();
                want.sort();
                assert_eq!(seen, want);
            }
            // with the matches recorded, a later run takes only entries that were breaks
            // (a group that shrank may now fit) and reports no matched entry as a break
            let records: Vec<Record> = first
                .matches
                .iter()
                .map(|m| {
                    let body = record_body("s", m, "actor");
                    Record {
                        id: record_id(&body).expect("id"),
                        body,
                    }
                })
                .collect();
            let (ul, ur) = used(&records);
            let second = run(&rules, &left, &right, &ul, &ur).expect("run");
            for (now, before, taken) in [
                (&second.breaks_left, &first.breaks_left, 0),
                (&second.breaks_right, &first.breaks_right, 1),
            ] {
                assert!(now.iter().all(|e| before.contains(e)));
                for m in &second.matches {
                    let side = if taken == 0 { &m.left } else { &m.right };
                    assert!(side.iter().all(|e| before.contains(e)));
                }
            }
            matched += first.matches.len();
        }
        assert!(matched > 300, "the generator produces matches: {matched}");
    }
}
