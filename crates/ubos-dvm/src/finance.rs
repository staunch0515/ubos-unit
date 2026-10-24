//! Commit checks of money and accounts (BPA 1.0-rc9, PH-2 step 3b, DEC-099): the codes that
//! `Currency` objects add (STD-FND-028), the declarations of FX rates (STD-FND-029) and
//! the chart of accounts with the account rule of postings (STD-KIND-029).

use crate::relations::{parse_uri, resolve};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ubos_types::BpaError;
use ubos_types::decimal::Decimal;
use ubos_types::values::{AddedGuard, added_currencies, minor_unit, table_unit};
use ubos_version::state::{Entry, Repo, Tx, at};

/// The genesis classes whose objects the commit checks here.
const CLASSES: &[&str] = &["Currency", "FxRate", "LedgerAccount"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn violated(details: Value) -> BpaError {
    BpaError::with_details("MODEL.CONSTRAINT_VIOLATED", details)
}

/// Money currencies outside the ISO 4217 table in a JSON value.
fn foreign_codes(v: &Value, out: &mut bool) {
    match v {
        Value::Object(o) => {
            if o.len() == 2
                && o.contains_key("amount")
                && let Some(c) = o.get("currency").and_then(Value::as_str)
                && table_unit(c).is_none()
            {
                *out = true;
            }
            o.values().for_each(|x| foreign_codes(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| foreign_codes(x, out)),
        _ => {}
    }
}

/// The live objects of a class as the transaction sees them: `(id, slug, head body)`.
fn live_of(repo: &mut Repo, tx: &mut Tx, class: &str) -> Result<Vec<(String, Value)>, BpaError> {
    repo.load_class(tx, class)?;
    let ids: Vec<(String, String)> = tx
        .state
        .objects
        .iter()
        .filter(|(_, o)| o.class == class)
        .map(|(id, o)| (id.clone(), o.head.clone()))
        .collect();
    let mut out = Vec::new();
    for (id, head) in ids {
        let v = repo.version_in(tx, &head)?;
        if v["tombstone"] != json!(true) {
            out.push((id, v["body"].clone()));
        }
    }
    Ok(out)
}

/// STD-FND-028: make the codes added by live `Currency` objects valid for the commit, when
/// the changes use a code outside the table or write a currency, rate or account.
pub(crate) fn currencies_for(
    repo: &mut Repo,
    tx: &mut Tx,
    changes: &[Value],
) -> Result<AddedGuard, BpaError> {
    let mut foreign = false;
    for ch in changes {
        if s(ch, "class").is_some_and(|c| CLASSES.contains(&c)) {
            foreign = true;
        }
        if let Some(b) = ch.get("body") {
            foreign_codes(b, &mut foreign);
        }
    }
    let mut codes = BTreeMap::new();
    if foreign {
        for (_, b) in live_of(repo, tx, "Currency")? {
            if let (Some(c), Some(u)) = (s(&b, "code"), b["minor_units"].as_u64()) {
                codes.insert(c.to_string(), u as u32);
            }
        }
    }
    Ok(added_currencies(codes))
}

/// The object a canonical URI names, loaded into the transaction.
fn resolve_uri(repo: &mut Repo, tx: &mut Tx, uri: &str) -> Result<Option<String>, BpaError> {
    let authority = repo.store.authority();
    if !tx.partial {
        return Ok(resolve(&tx.state, uri, &authority));
    }
    let Some((class, slug)) = parse_uri(uri, &authority) else {
        return Ok(None);
    };
    let found = repo.slug_in(tx, class, slug)?.map(|(id, _)| id);
    if let Some(id) = &found {
        repo.obj(tx, id)?;
    }
    Ok(found)
}

/// The body of an object at a valid date, or `None` when it is not live then.
fn body_at(repo: &mut Repo, tx: &mut Tx, oid: &str, date: &str) -> Result<Option<Value>, BpaError> {
    let Some(o) = repo.obj(tx, oid)? else {
        return Ok(None);
    };
    let Some(seg) = at(&o.timeline, date) else {
        return Ok(None);
    };
    let v = repo.version_in(tx, &seg.version)?;
    Ok((v["tombstone"] != json!(true)).then(|| v["body"].clone()))
}

/// STD-FND-028, -029, STD-KIND-029 (BPA 1.0-rc9): the currencies, rates and accounts the
/// commit wrote, and the account rule of the entries it appended.
pub(crate) fn check(
    repo: &mut Repo,
    tx: &mut Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    for (oid, vh) in written {
        let v = repo.version_in(tx, vh)?;
        let class = s(&v, "class").unwrap_or_default().to_string();
        if !CLASSES.contains(&class.as_str()) || v["tombstone"] == json!(true) {
            continue;
        }
        let live = repo.obj(tx, oid)?.is_some_and(|o| o.head == *vh);
        if !live {
            continue;
        }
        let b = &v["body"];
        match class.as_str() {
            "Currency" => check_currency(repo, tx, oid, b)?,
            "FxRate" => check_rate(repo, tx, oid, b)?,
            _ => check_account(repo, tx, oid, b)?,
        }
    }
    let appended: Vec<Entry> = tx.appended.clone();
    for e in &appended {
        let field = repo
            .reg
            .class(&e.class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("account").and_then(Value::as_str))
            .map(str::to_string);
        if let Some(f) = field {
            check_entry(repo, tx, e, &f)?;
        }
    }
    Ok(())
}

/// Another live object of `class` with the same key fails (STD-FND-028, -029).
fn unique(
    repo: &mut Repo,
    tx: &mut Tx,
    class: &str,
    oid: &str,
    body: &Value,
    keys: &[&str],
) -> Result<(), BpaError> {
    let key = |b: &Value| keys.iter().map(|k| b.get(*k).cloned()).collect::<Vec<_>>();
    let mine = key(body);
    for (other, b) in live_of(repo, tx, class)? {
        if other != oid && key(&b) == mine {
            let mut ids = vec![oid.to_string(), other];
            ids.sort();
            return Err(violated(json!({"duplicate": class, "objects": ids})));
        }
    }
    Ok(())
}

fn check_currency(repo: &mut Repo, tx: &mut Tx, oid: &str, b: &Value) -> Result<(), BpaError> {
    let code = s(b, "code").unwrap_or_default();
    let reason = if code.len() != 3
        || !code.bytes().all(|c| c.is_ascii_uppercase())
        || table_unit(code).is_some()
    {
        Some("code")
    } else if s(b, "numeric")
        .is_some_and(|n| n.len() != 3 || !n.bytes().all(|c| c.is_ascii_digit()))
    {
        Some("numeric")
    } else if b["minor_units"].as_u64().is_none_or(|u| u > 4) {
        Some("minor_units")
    } else {
        None
    };
    if let Some(r) = reason {
        return Err(violated(json!({"currency": code, "reason": r})));
    }
    unique(repo, tx, "Currency", oid, b, &["code"])
}

fn check_rate(repo: &mut Repo, tx: &mut Tx, oid: &str, b: &Value) -> Result<(), BpaError> {
    for k in ["base", "quote"] {
        if minor_unit(s(b, k).unwrap_or_default()).is_none() {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "unknown ISO 4217 currency",
            ));
        }
    }
    if b["base"] == b["quote"] {
        return Err(violated(json!({"rate": oid, "reason": "base"})));
    }
    let rate = Decimal::parse(s(b, "rate").unwrap_or("0"))?;
    if rate <= Decimal::parse("0")? {
        return Err(violated(json!({"rate": oid, "reason": "rate"})));
    }
    unique(
        repo,
        tx,
        "FxRate",
        oid,
        b,
        &["source", "base", "quote", "rate_type"],
    )
}

fn check_account(repo: &mut Repo, tx: &mut Tx, oid: &str, b: &Value) -> Result<(), BpaError> {
    if let Some(c) = s(b, "currency")
        && minor_unit(c).is_none()
    {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "unknown ISO 4217 currency",
        ));
    }
    let mut seen = vec![oid.to_string()];
    let mut cur = b.clone();
    while let Some(parent) = s(&cur, "parent").map(str::to_string) {
        let Some(p) = resolve_uri(repo, tx, &parent)? else {
            return Ok(()); // an unresolved reference is reported by the integrity check
        };
        if seen.contains(&p) {
            let o = repo.obj(tx, oid)?.expect("written object");
            let uri = crate::relations::uri_of(&repo.store.authority(), &o.class, &o.slug);
            return Err(violated(json!({"account": uri, "reason": "cycle"})));
        }
        seen.push(p.clone());
        let head = repo.obj(tx, &p)?.expect("resolved object").head;
        cur = repo.version_in(tx, &head)?["body"].clone();
    }
    Ok(())
}

/// STD-KIND-029: the account of an entry is live, a leaf and in the entry's currency at the
/// entry's valid time.
fn check_entry(repo: &mut Repo, tx: &mut Tx, e: &Entry, field: &str) -> Result<(), BpaError> {
    let Some(uri) = e
        .ledger_key
        .get(field)
        .or_else(|| e.body.get(field))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(());
    };
    let Some(oid) = resolve_uri(repo, tx, &uri)? else {
        return Ok(()); // reported by the reference checks
    };
    if repo
        .obj(tx, &oid)?
        .is_none_or(|o| o.class != "LedgerAccount")
    {
        return Ok(());
    }
    let fail = |r: &str| violated(json!({"account": uri, "reason": r}));
    let Some(acct) = body_at(repo, tx, &oid, &e.valid_at)? else {
        return Err(fail("inactive"));
    };
    let others: Vec<String> = {
        repo.load_class(tx, "LedgerAccount")?;
        tx.state
            .objects
            .iter()
            .filter(|(id, o)| o.class == "LedgerAccount" && **id != oid)
            .map(|(id, _)| id.clone())
            .collect()
    };
    for other in others {
        if let Some(b) = body_at(repo, tx, &other, &e.valid_at)?
            && let Some(p) = s(&b, "parent")
            && resolve_uri(repo, tx, p)?.as_deref() == Some(oid.as_str())
        {
            return Err(fail("not_leaf"));
        }
    }
    if let Some(cur) = s(&acct, "currency") {
        let amounts: Vec<String> = repo
            .reg
            .class(&e.class)?
            .ledger
            .as_ref()
            .and_then(|l| l.get("amounts"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
        for a in amounts {
            if let Some(c) = e.body.get(&a).and_then(|v| s(v, "currency"))
                && c != cur
            {
                return Err(fail("currency"));
            }
        }
    }
    Ok(())
}

/// A rate version a conversion uses (STD-FND-029).
pub struct RateUse {
    /// The rate as the conversion applies it.
    pub rate: ubos_expr::eval::FxRate,
    /// The `FxRate` object.
    pub object_id: String,
    /// Its version valid at the date.
    pub version_hash: String,
    /// The stored rate.
    pub stored: String,
    /// Whether the rate of the opposite key is used.
    pub inverse: bool,
}

/// STD-FND-029: the rate of `(source, base, quote, rate_type)` among the `FxRate` versions
/// valid at a date, given as `(object id, version hash, body)`.
pub fn pick_rate(
    rows: &[(String, String, Value)],
    base: &str,
    quote: &str,
    rate_type: &str,
    source: Option<&str>,
) -> Result<RateUse, BpaError> {
    let mut cands: Vec<(Option<&str>, bool, &Value)> = Vec::new();
    let mut ids: Vec<(&str, &str)> = Vec::new();
    for (oid, vh, b) in rows {
        if s(b, "rate_type") != Some(rate_type) || source.is_some_and(|x| s(b, "source") != Some(x))
        {
            continue;
        }
        let pair = (s(b, "base"), s(b, "quote"));
        if pair == (Some(base), Some(quote)) || pair == (Some(quote), Some(base)) {
            cands.push((s(b, "source"), pair.0 != Some(base), b));
            ids.push((oid, vh));
        }
    }
    let rate = ubos_expr::eval::pick_rate(&cands, base, quote, rate_type)?;
    let i = cands.iter().position(|c| !c.1).unwrap_or(0);
    Ok(RateUse {
        rate,
        object_id: ids[i].0.to_string(),
        version_hash: ids[i].1.to_string(),
        stored: s(cands[i].2, "rate").unwrap_or_default().to_string(),
        inverse: cands[i].1,
    })
}

/// The `FxRate` versions valid at a date among objects `(id, state)`.
pub fn rates_at(
    repo: &mut Repo,
    objects: &[(String, ubos_version::state::ObjectState)],
    date: &str,
) -> Result<Vec<(String, String, Value)>, BpaError> {
    let mut out = Vec::new();
    for (oid, o) in objects {
        if o.class != "FxRate" {
            continue;
        }
        if let Some(seg) = at(&o.timeline, date) {
            let v = repo.version(&seg.version)?;
            if v["tombstone"] != json!(true) {
                out.push((oid.clone(), seg.version.clone(), v["body"].clone()));
            }
        }
    }
    Ok(out)
}

impl crate::stateful::Engine {
    fn objects_of(
        &mut self,
        i: &Value,
        class: &str,
    ) -> Result<Vec<(String, ubos_version::state::ObjectState)>, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let mut r = Repo::new(self.store.as_mut(), &self.registry);
        let id = r.commit_at(&branch, i.get("time").and_then(Value::as_u64))?;
        let cseq = r.meta(id)?.cseq;
        Ok(r.objects_in(&branch, cseq, Some(class))?
            .into_iter()
            .collect())
    }

    /// Adapter operation `currency` (STD-FND-028).
    pub fn currency(&mut self, i: &Value) -> Result<Value, BpaError> {
        let code = s(i, "code").unwrap_or_default();
        if let Some(u) = table_unit(code) {
            return Ok(json!({"code": code, "minor_units": u, "source": "table"}));
        }
        for (oid, o) in self.objects_of(i, "Currency")? {
            let v = Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?;
            if v["tombstone"] != json!(true) && s(&v["body"], "code") == Some(code) {
                return Ok(
                    json!({"code": code, "minor_units": v["body"]["minor_units"], "source": oid}),
                );
            }
        }
        Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "unknown ISO 4217 currency",
        ))
    }

    /// Adapter operation `fx_rate` (STD-FND-029).
    pub fn fx_rate(&mut self, i: &Value) -> Result<Value, BpaError> {
        let objects = self.objects_of(i, "FxRate")?;
        let date = s(i, "date")
            .map(str::to_string)
            .unwrap_or_else(|| self.now[..10].to_string());
        let rows = rates_at(
            &mut Repo::new(self.store.as_mut(), &self.registry),
            &objects,
            &date,
        )?;
        let u = pick_rate(
            &rows,
            s(i, "base").unwrap_or_default(),
            s(i, "quote").unwrap_or_default(),
            s(i, "rate_type").unwrap_or_default(),
            s(i, "source"),
        )?;
        Ok(
            json!({"object_id": u.object_id, "version_hash": u.version_hash, "rate": u.stored, "inverse": u.inverse}),
        )
    }
}
