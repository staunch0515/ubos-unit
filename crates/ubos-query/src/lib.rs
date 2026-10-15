//! Criteria queries of BPA 1.0 (STD-EXPR-010, STD-EXPR-011, STD-ISA-080; PH-1 step 13a,
//! DEC-073; DSN-DVM-901…908): the textual form and its `DAT-Query`, the normalised query
//! specification and its hash, the index bounds a `where` yields for the planner, the
//! scan budgets of the execution profiles (DSN-DVM-903), signed snapshot cursors
//! (DSN-DVM-908) and the aggregates of the query form (DSN-DVM-905). The engine is pure:
//! the snapshot, the field indexes and the evaluation of the rows are the kernel's
//! (`ubos-dvm`).

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use ubos_expr::{Node, OrderKey, Val, order_key};
use ubos_model::types::decimal::{Decimal, Rounding};
use ubos_model::types::values::index_key;
use ubos_model::types::{BpaError, canonical, content_id};

/// Largest page (STD-EXPR-011).
pub const MAX_LIMIT: u64 = 1000;
/// Default page (STD-EXPR-010).
pub const DEFAULT_LIMIT: u64 = 100;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// `QRY.INVALID_QUERY`.
pub fn bad_query(m: &str) -> BpaError {
    BpaError::new("QRY.INVALID_QUERY", m)
}

// ---- the textual form (STD-EXPR-010) ----

fn is_class_name(t: &str) -> bool {
    let mut cs = t.chars();
    cs.next().is_some_and(|c| c.is_ascii_uppercase()) && cs.all(|c| c.is_ascii_alphanumeric())
}

/// The position of a keyword clause ` kw ` at a token boundary in `rest`, searching from
/// the start; `kw` is given with its single spaces.
fn find_clause(rest: &str, words: &[&str]) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < rest.len() {
        if i == 0 || bytes[i - 1].is_ascii_whitespace() {
            let mut j = i;
            let mut ok = true;
            for (n, w) in words.iter().enumerate() {
                if n > 0 {
                    let k = j;
                    while j < rest.len() && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j == k {
                        ok = false;
                        break;
                    }
                }
                if rest[j..].starts_with(w) {
                    j += w.len();
                } else {
                    ok = false;
                    break;
                }
            }
            if ok && (j == rest.len() || bytes[j].is_ascii_whitespace()) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// STD-EXPR-010: the `DAT-Query` of `from <Class> [only] where <expr> order by <path>
/// [asc|desc], … limit n`; the result equals the reference model's.
pub fn parse_query(text: &Value) -> Result<Value, BpaError> {
    let Some(text) = text.as_str() else {
        return Err(bad_query("a query text is text"));
    };
    let src = text.trim();
    let Some(after_from) = src.strip_prefix("from") else {
        return Err(bad_query("a query starts with from <Class>"));
    };
    if !after_from.starts_with(|c: char| c.is_whitespace()) {
        return Err(bad_query("a query starts with from <Class>"));
    }
    let after_from = after_from.trim_start();
    let class_end = after_from
        .find(|c: char| c.is_whitespace())
        .unwrap_or(after_from.len());
    let class = &after_from[..class_end];
    if !is_class_name(class) {
        return Err(bad_query("a query starts with from <Class>"));
    }
    let mut out = Map::new();
    out.insert("from".into(), json!(class));
    let mut rest = after_from[class_end..].trim_start();
    if let Some(r) = rest.strip_prefix("only")
        && (r.is_empty() || r.starts_with(|c: char| c.is_whitespace()))
    {
        out.insert("only".into(), json!(true));
        rest = r.trim_start();
    }
    let mut rest = rest.trim().to_string();
    // limit at the end
    let mut limit: Option<u64> = None;
    if let Some(i) = rest.rfind("limit") {
        let before_ok = i == 0 || rest.as_bytes()[i - 1].is_ascii_whitespace();
        let tail = rest[i + 5..].trim();
        if before_ok
            && rest[i + 5..].starts_with(|c: char| c.is_whitespace())
            && !tail.is_empty()
            && tail.bytes().all(|b| b.is_ascii_digit())
        {
            limit = Some(
                tail.parse()
                    .map_err(|_| bad_query("a limit is a number at the end"))?,
            );
            rest = rest[..i].trim().to_string();
        }
    }
    // order by
    let mut order: Option<String> = None;
    if let Some(i) = find_clause(&rest, &["order", "by"]) {
        let after = &rest[i..];
        let j = after.find("by").map(|k| k + 2).unwrap_or(after.len());
        order = Some(after[j..].trim().to_string());
        rest = rest[..i].trim().to_string();
    }
    if !rest.is_empty() {
        let Some(w) = rest.strip_prefix("where ") else {
            return Err(bad_query(&format!(
                "unexpected text {:?}",
                rest.chars().take(20).collect::<String>()
            )));
        };
        let w = w.trim();
        if w.is_empty() {
            return Err(bad_query("where needs an expression"));
        }
        out.insert("where".into(), json!(w));
    }
    if let Some(o) = order {
        let mut items = Vec::new();
        for part in o.split(',') {
            let toks: Vec<&str> = part.split_whitespace().collect();
            if toks.is_empty()
                || toks.len() > 2
                || (toks.len() == 2 && !matches!(toks[1], "asc" | "desc"))
            {
                return Err(bad_query("order by takes <path> [asc|desc]"));
            }
            items.push(
                json!({"path": toks[0], "dir": if toks.len() == 2 { toks[1] } else { "asc" }}),
            );
        }
        out.insert("order_by".into(), Value::Array(items));
    }
    if let Some(l) = limit {
        out.insert("limit".into(), json!(l));
    }
    Ok(Value::Object(out))
}

// ---- the specification ----

/// One `order_by` item.
#[derive(Debug, Clone)]
pub struct Order {
    /// The path text (`this.<field>…`).
    pub path: String,
    /// Descending.
    pub desc: bool,
}

/// A normalised criteria query (`DAT-Query`).
#[derive(Debug, Clone)]
pub struct Spec {
    /// Class name.
    pub from: String,
    /// Exclude subclasses.
    pub only: bool,
    /// The `where` text.
    pub where_: Option<String>,
    /// Lifecycle states (STD-CLS-004).
    pub states: Vec<String>,
    /// Field paths to return.
    pub select: Vec<String>,
    /// Sort order.
    pub order_by: Vec<Order>,
    /// Grouping paths.
    pub group_by: Vec<String>,
    /// Aggregates `{fn, path, as}`.
    pub aggregates: Vec<Value>,
    /// Page size.
    pub limit: u64,
    /// Valid-time coordinate.
    pub asof: Option<String>,
    /// The content identifier of the query (the members of the reference model's hash:
    /// `from`, `only`, `where`, `order_by`, `limit`, `asof`), which binds a cursor.
    pub hash: String,
}

/// Normalise a query input: a `text` is parsed and its members win over the JSON ones
/// given beside it (as the reference model does).
pub fn spec_of(input: &Value) -> Result<Spec, BpaError> {
    let mut i = input.as_object().cloned().unwrap_or_default();
    if let Some(t) = i.get("text").filter(|t| !t.is_null()).cloned() {
        let parsed = parse_query(&t)?;
        let mut merged = i.clone();
        merged.remove("text");
        for (k, v) in parsed.as_object().into_iter().flatten() {
            merged.insert(k.clone(), v.clone());
        }
        i = merged;
    }
    let g = |k: &str| i.get(k).cloned().unwrap_or(Value::Null);
    let spec = json!({"from": g("from"), "only": g("only"), "where": g("where"),
        "order_by": g("order_by"), "limit": g("limit"), "asof": g("asof")});
    let hash = content_id("ubos.chunk.v1", &spec)?;
    let from = match i.get("from") {
        Some(Value::String(c)) => c.clone(),
        _ => return Err(bad_query("unknown class")),
    };
    let limit = match i.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(Value::Number(n)) => match n.as_u64() {
            Some(l) if (1..=MAX_LIMIT).contains(&l) => l,
            _ => return Err(bad_query(&format!("limit is 1 to {MAX_LIMIT}"))),
        },
        _ => return Err(bad_query(&format!("limit is 1 to {MAX_LIMIT}"))),
    };
    let mut order_by = Vec::new();
    for o in i
        .get("order_by")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let dir = o.get("dir").map(|d| d.as_str()).unwrap_or(Some("asc"));
        if !o.is_object() || !matches!(dir, Some("asc" | "desc")) {
            return Err(bad_query("order_by items are {path, dir}"));
        }
        let Some(path) = s(o, "path") else {
            return Err(bad_query("order_by names a path"));
        };
        order_by.push(Order {
            path: path.to_string(),
            desc: dir == Some("desc"),
        });
    }
    let texts = |k: &str| -> Result<Vec<String>, BpaError> {
        match i.get(k) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(a)) => a
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| bad_query(&format!("{k} is a list of texts")))
                })
                .collect(),
            _ => Err(bad_query(&format!("{k} is a list of texts"))),
        }
    };
    let aggregates = match i.get("aggregates") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => {
            for x in a {
                let ok = x.is_object()
                    && matches!(s(x, "fn"), Some("count" | "sum" | "min" | "max" | "avg"))
                    && (s(x, "fn") == Some("count") || s(x, "path").is_some())
                    && s(x, "as").is_some_and(|n| !n.is_empty());
                if !ok {
                    return Err(bad_query(
                        "aggregates are {fn, path, as} with fn count, sum, min, max or avg",
                    ));
                }
            }
            a.clone()
        }
        _ => return Err(bad_query("aggregates is a list")),
    };
    Ok(Spec {
        from,
        only: i.get("only") == Some(&Value::Bool(true)),
        where_: i.get("where").and_then(Value::as_str).map(str::to_string),
        states: texts("states")?,
        select: texts("select")?,
        order_by,
        group_by: texts("group_by")?,
        aggregates,
        limit,
        asof: i.get("asof").and_then(Value::as_str).map(str::to_string),
        hash,
    })
}

// ---- index bounds (DSN-DVM-901, -902) ----

/// A range of value keys of one indexed field that every matching object satisfies.
#[derive(Debug, Clone, Default)]
pub struct Bound {
    /// Field name.
    pub field: String,
    /// Lower key bound (inclusive).
    pub lo: Option<Vec<u8>>,
    /// Upper key bound (inclusive).
    pub hi: Option<Vec<u8>>,
    /// An equality (the most selective bound).
    pub eq: bool,
}

fn this_field(n: &Node) -> Option<&str> {
    match n {
        Node::Member(_, base, f, _) => match base.as_ref() {
            Node::Path(_, root) if root == "this" => Some(f),
            _ => None,
        },
        _ => None,
    }
}

/// The next key after `k` in byte order (for exclusive bounds).
fn succ(mut k: Vec<u8>) -> Vec<u8> {
    k.push(0);
    k
}

/// The last key before `k` in byte order, for keys that are not empty: the key with its
/// last byte decremented and padded with 0xFF.
fn pred(mut k: Vec<u8>) -> Option<Vec<u8>> {
    while let Some(last) = k.pop() {
        if last > 0 {
            k.push(last - 1);
            k.extend(std::iter::repeat_n(0xFF, 8));
            return Some(k);
        }
    }
    None
}

/// The bounds a `where` yields for the indexed fields (`(field, type)`): the comparisons
/// of `this.<field>` with a literal in the conjunction at the top of the expression. A
/// bound never excludes a matching object; the `where` is evaluated on the candidates.
pub fn bounds(where_: &Node, indexed: &[(String, String)]) -> Vec<Bound> {
    let mut out: BTreeMap<String, Bound> = BTreeMap::new();
    let mut todo = vec![where_];
    while let Some(n) = todo.pop() {
        match n {
            Node::And(_, a, b) => {
                todo.push(a);
                todo.push(b);
            }
            Node::Cmp(_, op, l, r) => {
                let (field, lit, flipped) = match (this_field(l), r.as_ref()) {
                    (Some(f), Node::Lit(_, v, _)) => (f, v, false),
                    _ => match (l.as_ref(), this_field(r)) {
                        (Node::Lit(_, v, _), Some(f)) => (f, v, true),
                        _ => continue,
                    },
                };
                let Some((_, t)) = indexed.iter().find(|(f, _)| f == field) else {
                    continue;
                };
                let Ok(j) = ubos_expr::to_json(lit) else {
                    continue;
                };
                if j.is_null() {
                    continue;
                }
                let Some(key) = index_key(t, &j) else {
                    continue;
                };
                let op = match (*op, flipped) {
                    ("<", true) => ">",
                    (">", true) => "<",
                    ("<=", true) => ">=",
                    (">=", true) => "<=",
                    (o, _) => o,
                };
                let b = out.entry(field.to_string()).or_insert_with(|| Bound {
                    field: field.to_string(),
                    ..Default::default()
                });
                let raise_lo = |b: &mut Bound, k: Vec<u8>| {
                    if b.lo.as_ref().is_none_or(|l| *l < k) {
                        b.lo = Some(k);
                    }
                };
                let lower_hi = |b: &mut Bound, k: Vec<u8>| {
                    if b.hi.as_ref().is_none_or(|h| *h > k) {
                        b.hi = Some(k);
                    }
                };
                match op {
                    "==" => {
                        raise_lo(b, key.clone());
                        lower_hi(b, key);
                        b.eq = true;
                    }
                    ">=" => raise_lo(b, key),
                    ">" => raise_lo(b, succ(key)),
                    "<=" => lower_hi(b, key),
                    "<" => {
                        if let Some(p) = pred(key) {
                            lower_hi(b, p);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out.into_values()
        .filter(|b| b.lo.is_some() || b.hi.is_some())
        .collect()
}

/// The bound to scan: an equality first, then the one with both ends, then any.
pub fn best_bound(bounds: &[Bound]) -> Option<&Bound> {
    bounds
        .iter()
        .max_by_key(|b| (b.eq, b.lo.is_some() && b.hi.is_some()))
}

/// Whether an expression calls into other objects (`ref`, `exists`), which the index path
/// cannot resolve without the whole snapshot.
pub fn reaches_objects(n: &Node) -> bool {
    match n {
        Node::Call(_, name, args) => {
            matches!(name.as_str(), "ref" | "exists") || args.iter().any(reaches_objects)
        }
        Node::List(_, items) => items.iter().any(reaches_objects),
        Node::Member(_, b, _, _) | Node::Neg(_, b) | Node::Not(_, b) | Node::Matches(_, b, _) => {
            reaches_objects(b)
        }
        Node::IsNull(_, b, _) => reaches_objects(b),
        Node::Index(_, a, b)
        | Node::Arith(_, _, a, b)
        | Node::Cmp(_, _, a, b)
        | Node::In(_, a, b)
        | Node::And(_, a, b)
        | Node::Or(_, a, b) => reaches_objects(a) || reaches_objects(b),
        Node::Cond(_, a, b, c) => reaches_objects(a) || reaches_objects(b) || reaches_objects(c),
        Node::Lambda(_, _, l, _, body) => reaches_objects(l) || reaches_objects(body),
        Node::Lit(..) | Node::Path(..) => false,
    }
}

/// The first field of `this` a `where` compares, as the suggested index of a refused scan
/// (DSN-DVM-903).
pub fn suggested_index(where_: &Node) -> Option<String> {
    let mut todo = vec![where_];
    while let Some(n) = todo.pop() {
        match n {
            Node::And(_, a, b) | Node::Or(_, a, b) => {
                todo.push(b);
                todo.push(a);
            }
            Node::Cmp(_, _, l, r) => {
                if let Some(f) = this_field(l).or_else(|| this_field(r)) {
                    return Some(f.to_string());
                }
            }
            _ => {}
        }
    }
    None
}

// ---- scan budgets (DSN-DVM-903) ----

/// The largest unindexed scan of a profile, in objects: strict 10,000; standard 100,000;
/// agent 100,000; batch 10,000,000; simulation unbounded. The adapter and the host may
/// lower them (`set_limits.max_scan`).
pub fn scan_budget(profile: &str) -> u64 {
    match profile {
        "strict" => 10_000,
        "batch" => 10_000_000,
        "simulation" => u64::MAX,
        _ => 100_000,
    }
}

// ---- signed snapshot cursors (DSN-DVM-908) ----

/// A cursor: the query, the branch, the snapshot commit bound at the first page and the
/// position; signed with the tenant's cursor key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Query hash (`Spec::hash`).
    pub q: String,
    /// Branch.
    pub branch: String,
    /// Commit identifier of the snapshot.
    pub commit: String,
    /// Rows already returned.
    pub offset: u64,
}

const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64u(b: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let n = (b.len() - i).min(3);
        let mut acc: u32 = 0;
        for k in 0..3 {
            acc <<= 8;
            if k < n {
                acc |= u32::from(b[i + k]);
            }
        }
        for k in 0..(n + 1) {
            out.push(A[((acc >> (18 - 6 * k)) & 63) as usize] as char);
        }
        i += 3;
    }
    out
}

fn b64u_decode(t: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in t.bytes() {
        if c == b'=' {
            break;
        }
        let v = A.iter().position(|&x| x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

impl Cursor {
    /// The opaque cursor text: the HMAC tag (16 bytes) and the canonical record.
    pub fn encode(&self, key: &[u8]) -> Result<String, BpaError> {
        let rec = json!({"branch": self.branch, "commit": self.commit, "offset": self.offset, "q": self.q});
        let text = canonical(&rec)?;
        let tag = ubos_crypto::hmac_sha256(key, text.as_bytes());
        let mut bytes = tag[..16].to_vec();
        bytes.extend_from_slice(text.as_bytes());
        Ok(b64u(&bytes))
    }

    /// The cursor of a text, or `QRY.INVALID_QUERY` when it is not one signed with `key`
    /// (`unknown cursor`).
    pub fn decode(key: &[u8], text: &str) -> Result<Cursor, BpaError> {
        let unknown = || bad_query("unknown cursor");
        let bytes = b64u_decode(text).ok_or_else(unknown)?;
        if bytes.len() < 16 {
            return Err(unknown());
        }
        let (tag, payload) = bytes.split_at(16);
        let want = ubos_crypto::hmac_sha256(key, payload);
        if tag != &want[..16] {
            return Err(unknown());
        }
        let rec: Value = serde_json::from_slice(payload).map_err(|_| unknown())?;
        Ok(Cursor {
            q: s(&rec, "q").ok_or_else(unknown)?.to_string(),
            branch: s(&rec, "branch").ok_or_else(unknown)?.to_string(),
            commit: s(&rec, "commit").ok_or_else(unknown)?.to_string(),
            offset: rec
                .get("offset")
                .and_then(Value::as_u64)
                .ok_or_else(unknown)?,
        })
    }
}

// ---- aggregates (DSN-DVM-905) ----

/// Groups under construction: group key → (grouping values, the values of each aggregate).
type Groups = BTreeMap<Vec<Option<OrderKey>>, (Vec<Val>, Vec<Vec<Val>>)>;

/// Sort key of an `order_by` or grouping value: nulls first, then the value's order key;
/// values without an order are a type error.
pub fn sort_key(v: &Val) -> Result<Option<OrderKey>, BpaError> {
    if v.is_null() {
        return Ok(None);
    }
    order_key(v).map(Some).ok_or_else(|| {
        BpaError::with_details(
            "LOGIC.TYPE_ERROR",
            json!({"position": 0, "message": "values are not ordered"}),
        )
    })
}

/// One aggregate over the values of a group.
fn aggregate_one(f: &str, values: &[Val]) -> Result<Value, BpaError> {
    let present: Vec<&Val> = values.iter().filter(|v| !v.is_null()).collect();
    match f {
        "count" => Ok(json!(present.len())),
        "min" | "max" => {
            let mut best: Option<(OrderKey, &Val)> = None;
            for v in &present {
                let k = sort_key(v)?.expect("present");
                let better = match &best {
                    None => true,
                    Some((bk, _)) => {
                        if f == "min" {
                            k < *bk
                        } else {
                            k > *bk
                        }
                    }
                };
                if better {
                    best = Some((k, v));
                }
            }
            match best {
                Some((_, v)) => ubos_expr::to_json(v),
                None => Ok(Value::Null),
            }
        }
        "sum" | "avg" => {
            let mut sum = Decimal::parse("0")?;
            let mut currency: Option<String> = None;
            let mut scale: u32 = 0;
            let mut n = 0u32;
            for v in &present {
                let d = match v {
                    Val::Int(i) => Decimal::parse(&i.to_string())?,
                    Val::Dec(d) => d.clone(),
                    Val::Money(a, c) => {
                        if currency.as_deref().is_some_and(|x| x != c) {
                            return Err(BpaError::new(
                                "MODEL.CURRENCY_MISMATCH",
                                "a money sum groups by currency",
                            ));
                        }
                        currency = Some(c.clone());
                        a.clone()
                    }
                    _ => {
                        return Err(BpaError::with_details(
                            "LOGIC.TYPE_ERROR",
                            json!({"position": 0, "message": format!("{f} takes numbers or money")}),
                        ));
                    }
                };
                scale = scale.max(d.scale());
                sum = sum.add(&d);
                n += 1;
            }
            if f == "avg" {
                if n == 0 {
                    return Ok(Value::Null);
                }
                scale = scale.max(2);
                sum = sum.div(&Decimal::parse(&n.to_string())?, scale, Rounding::HalfEven)?;
            }
            // money keeps the minor unit of its currency, decimals the largest scale seen
            Ok(match currency {
                Some(c) => {
                    let unit = ubos_model::types::values::minor_unit(&c).unwrap_or(scale);
                    json!({"amount": sum.format(Some(unit.max(scale))), "currency": c})
                }
                None => json!(sum.format(Some(scale))),
            })
        }
        _ => Err(bad_query("unknown aggregate")),
    }
}

/// The groups of an aggregate query: `rows` carry the grouping values and the values of
/// each aggregate in the order of `spec.aggregates`; money sums group by currency as well.
/// Groups are listed in the order of their keys (nulls first).
pub fn aggregate(spec: &Spec, rows: Vec<(Vec<Val>, Vec<Val>)>) -> Result<Vec<Value>, BpaError> {
    let mut groups: Groups = BTreeMap::new();
    for (keys, vals) in rows {
        let mut gk: Vec<Option<OrderKey>> = keys.iter().map(sort_key).collect::<Result<_, _>>()?;
        let mut gv = keys.clone();
        // money sums and averages group by currency
        for (a, v) in spec.aggregates.iter().zip(&vals) {
            if matches!(s(a, "fn"), Some("sum" | "avg"))
                && let Val::Money(_, c) = v
            {
                gk.push(Some(OrderKey::Text(c.clone())));
                gv.push(Val::Text(c.clone()));
            }
        }
        let g = groups
            .entry(gk)
            .or_insert_with(|| (gv, vec![Vec::new(); spec.aggregates.len()]));
        for (i, v) in vals.into_iter().enumerate() {
            g.1[i].push(v);
        }
    }
    let mut out = Vec::new();
    for (_, (keys, cols)) in groups {
        let mut g = Map::new();
        for (p, v) in spec.group_by.iter().zip(&keys) {
            g.insert(p.clone(), ubos_expr::to_json(v)?);
        }
        if keys.len() > spec.group_by.len() {
            g.insert(
                "currency".into(),
                ubos_expr::to_json(&keys[spec.group_by.len()])?,
            );
        }
        for (a, col) in spec.aggregates.iter().zip(&cols) {
            g.insert(
                s(a, "as").unwrap_or_default().to_string(),
                aggregate_one(s(a, "fn").unwrap_or_default(), col)?,
            );
        }
        out.push(Value::Object(g));
    }
    Ok(out)
}
