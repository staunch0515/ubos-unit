//! Money and calendars in the kernel (PH-2 step 3a, DEC-098): the acceptance criteria of
//! DSN-DVM-409 (multi-currency postings), DSN-DVM-410 (currency reference data and
//! bitemporal exchange rates) and DSN-DVM-447 (business calendars and cut-offs as
//! objects), with FR-LOC-021, -022, -041, -042, FR-QRY-042 and FR-LEDG-053.
//!
//! Every test runs on the in-memory store and, when `UBOS_TEST_DATABASE` names a
//! PostgreSQL connection string, on PostgreSQL as well (each test in its own VAE scope).

use serde_json::{Value, json};
use ubos_dvm::Adapter;
use ubos_ledger::calendar::Calendar;
use ubos_ledger::fx::{Rate, RateBook, Selector};

const NOW: &str = "2026-11-02T09:00:00.000000Z";

/// The adapters a test runs on: `(backend, adapter)`.
fn adapters(test: &str) -> Vec<(&'static str, Adapter)> {
    let mut out = vec![("memory", Adapter::new().expect("memory adapter"))];
    match std::env::var("UBOS_TEST_DATABASE") {
        Ok(url) => {
            // one tenant per test, so that the tests of this file run side by side
            let n = test
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
            let tenant = format!("01890a5d-3a00-7000-8000-{:012x}", n);
            let vae = format!("01890a5d-3a00-7001-8000-{:012x}", n);
            out.push((
                "postgres",
                Adapter::postgres_scope(&url, &tenant, &vae).expect("postgres adapter"),
            ));
        }
        Err(_) => eprintln!("UBOS_TEST_DATABASE not set: {test} runs in memory only"),
    }
    out
}

fn ok(a: &mut Adapter, op: &str, input: Value) -> Value {
    a.execute(op, &input)
        .unwrap_or_else(|e| panic!("{op} {input}: {e} {:?}", e.details))
}

fn code(a: &mut Adapter, op: &str, input: Value) -> &'static str {
    match a.execute(op, &input) {
        Ok(v) => panic!("{op} {input} gave {v}"),
        Err(e) => e.code,
    }
}

fn uuid(n: u32) -> String {
    format!("01890a5d-ac96-7{:03x}-8000-{:012x}", n & 0xfff, n)
}

fn field(name: &str, type_: &str) -> Value {
    json!({"name": name, "type": type_, "label": name, "description": name})
}

fn class(name: &str, fields: Vec<Value>) -> Value {
    json!({"name": name, "label": name, "description": name, "namespace": "t.money",
        "parent": "DataObject", "kind": "definition", "schema_version": 1, "fields": fields})
}

fn start(a: &mut Adapter, classes: Vec<Value>) -> String {
    ok(a, "reset", json!({}));
    ok(a, "set_now", json!({"now": NOW}));
    ok(a, "load_genesis", json!({"classes": classes}));
    a.engine_mut().store.authority()
}

fn commit(a: &mut Adapter, changes: Value) -> Value {
    ok(a, "commit", json!({"branch": "main", "changes": changes}))
}

fn rate_body(source: &str, from: &str, to: &str, rate: &str) -> Value {
    json!({"source": source, "from": from, "to": to, "rate_type": "close", "rate": rate})
}

/// Create a rate series with its first rate from a rate date; returns the version.
fn create_rate(a: &mut Adapter, n: u32, slug: &str, body: Value, from: &str) -> String {
    let r = commit(
        a,
        json!([{"op": "create", "class": "FxRate", "object_id": uuid(n), "slug": slug,
            "body": body, "valid": {"from": from}}]),
    );
    r["versions"][uuid(n)]
        .as_str()
        .expect("version")
        .to_string()
}

/// Record the rate of a rate date on an existing series; returns `(cseq, version)`.
fn set_rate(a: &mut Adapter, n: u32, base: &str, rate: &str, from: &str) -> (u64, String) {
    let r = commit(
        a,
        json!([{"op": "update", "object_id": uuid(n), "base": base, "body": {"rate": rate},
            "valid": {"kind": "supersede_from", "from": from}}]),
    );
    (
        r["cseq"].as_u64().expect("cseq"),
        r["versions"][uuid(n)]
            .as_str()
            .expect("version")
            .to_string(),
    )
}

// ---- FR-LOC-021, DSN-DVM-410: currency reference data ----

#[test]
fn currencies_are_reference_data() {
    for (backend, mut a) in adapters("currencies") {
        let a = &mut a;
        start(a, vec![class("Holding", vec![field("amount", "money")])]);
        let jpy = ok(a, "currencies", json!({"code": "JPY"}));
        assert_eq!(jpy["minor_unit"], json!(0), "{backend}");
        assert!(jpy["table_version"].is_string());
        assert_eq!(
            code(a, "currencies", json!({"code": "XYZ"})),
            "MODEL.INVALID_VALUE"
        );
        // effective-dated: a withdrawn code is listed until its end date
        let listed = |v: &Value, c: &str| {
            v["currencies"]
                .as_array()
                .expect("list")
                .iter()
                .any(|x| x["code"] == c)
        };
        assert!(listed(
            &ok(a, "currencies", json!({"date": "2022-12-31"})),
            "HRK"
        ));
        assert!(!listed(
            &ok(a, "currencies", json!({"date": "2023-01-01"})),
            "HRK"
        ));
        assert!(listed(&ok(a, "currencies", json!({})), "HRK"));
        // FR-LOC-021 acceptance 1: money in JPY is stored with scale 0 by default
        commit(
            a,
            json!([
                {"op": "create", "class": "Holding", "object_id": uuid(1), "slug": "yen",
                    "body": {"amount": {"amount": "1200", "currency": "JPY"}}},
                {"op": "create", "class": "Holding", "object_id": uuid(2), "slug": "dinar",
                    "body": {"amount": {"amount": "7.5", "currency": "KWD"}}},
                {"op": "create", "class": "Holding", "object_id": uuid(3), "slug": "aud",
                    "body": {"amount": {"amount": "3", "currency": "AUD"}}},
            ]),
        );
        let amount = |a: &mut Adapter, n: u32| {
            ok(a, "read", json!({"object_id": uuid(n)}))["body"]["amount"]["amount"].clone()
        };
        assert_eq!(amount(a, 1), json!("1200"), "{backend}");
        assert_eq!(amount(a, 2), json!("7.500"), "{backend}");
        assert_eq!(amount(a, 3), json!("3.00"), "{backend}");
        assert_eq!(
            code(
                a,
                "commit",
                json!({"branch": "main", "changes": [{"op": "create", "class": "Holding",
                    "object_id": uuid(4), "slug": "none",
                    "body": {"amount": {"amount": "1", "currency": "XYZ"}}}]})
            ),
            "MODEL.INVALID_VALUE"
        );
    }
}

// ---- FR-LOC-022, DSN-DVM-410: bitemporal exchange rates ----

#[test]
fn a_corrected_rate_keeps_the_original_conversion_reproducible() {
    for (backend, mut a) in adapters("rates") {
        let a = &mut a;
        start(a, vec![]);
        let v1 = create_rate(
            a,
            1,
            "eur-usd",
            rate_body("ecb", "EUR", "USD", "1.10"),
            "2026-03-02",
        );
        let (c2, v2) = set_rate(a, 1, &v1, "1.12", "2026-03-03");
        let hundred = json!({"amount": "100.00", "currency": "EUR"});
        let first = ok(
            a,
            "convert",
            json!({"amount": hundred, "to": "USD", "asof": "2026-03-02"}),
        );
        assert_eq!(
            first["amount"],
            json!({"amount": "110.00", "currency": "USD"}),
            "{backend}"
        );
        // the evidence names the rate version used
        assert_eq!(first["rate"]["version"], json!(v1));
        assert_eq!(first["rate"]["object_id"], json!(uuid(1)));
        assert_eq!(
            first["rate"]["valid"],
            json!({"from": "2026-03-02", "to": "2026-03-03"})
        );
        let evidence = first["evaluated_at"].clone();
        assert!(evidence["main"].is_string());

        // the rate of 2 March is corrected retroactively
        let (c3, v3) = set_rate(a, 1, &v2, "1.11", "2026-03-02");
        assert!(c3 > c2);
        // DSN-DVM-410 acceptance 1: at the original commit the original rate is used …
        for coordinate in [json!({"time": c2}), json!({"commit": evidence["main"]})] {
            let mut input = json!({"amount": hundred, "to": "USD", "asof": "2026-03-02"});
            input["time"] = coordinate["time"].clone();
            input["commit"] = coordinate["commit"].clone();
            let again = ok(a, "convert", input);
            assert_eq!(again["amount"], first["amount"], "{backend}");
            assert_eq!(again["rate"]["version"], json!(v1));
            assert_eq!(again["evaluated_at"], evidence);
        }
        // … and at the current commit the corrected one
        let now = ok(
            a,
            "convert",
            json!({"amount": hundred, "to": "USD", "asof": "2026-03-02"}),
        );
        assert_eq!(
            now["amount"],
            json!({"amount": "111.00", "currency": "USD"}),
            "{backend}"
        );
        assert_eq!(now["rate"]["version"], json!(v3));
        // the correction ends at the next rate date
        let next = ok(
            a,
            "fx_rate",
            json!({"from": "EUR", "to": "USD", "asof": "2026-03-03"}),
        );
        assert_eq!(
            (&next["rate"], &next["version"]),
            (&json!("1.12"), &json!(v2))
        );
        // FR-LEDG-053: the recorded rate version reproduces the amount whatever came later
        let by_version = ok(
            a,
            "convert",
            json!({"amount": hundred, "to": "USD", "rate_version": v1}),
        );
        assert_eq!(by_version["amount"], first["amount"]);
        assert_eq!(by_version["rate"]["version"], json!(v1));

        // rounding to the minor unit of the target
        create_rate(
            a,
            2,
            "usd-jpy",
            rate_body("boj", "USD", "JPY", "151.235"),
            "2026-03-02",
        );
        let yen = |a: &mut Adapter, rounding: &str| {
            ok(
                a,
                "convert",
                json!({"amount": {"amount": "10.50", "currency": "USD"}, "to": "JPY",
                    "asof": "2026-03-05", "rounding": rounding}),
            )["amount"]["amount"]
                .clone()
        };
        assert_eq!(yen(a, "half_even"), json!("1588"));
        assert_eq!(yen(a, "down"), json!("1587"));
        // a conversion needs a rate of the pair in that direction, on that date
        for (from, to, asof) in [
            ("USD", "EUR", "2026-03-02"),
            ("EUR", "USD", "2026-03-01"),
            ("GBP", "USD", "2026-03-02"),
        ] {
            assert_eq!(
                code(a, "fx_rate", json!({"from": from, "to": to, "asof": asof})),
                "MODEL.CURRENCY_MISMATCH",
                "{backend} {from}->{to} {asof}"
            );
        }
        for unknown in ["nope".to_string(), format!("sha256:{}", "0".repeat(64))] {
            assert_eq!(
                code(
                    a,
                    "convert",
                    json!({"amount": hundred, "to": "USD", "rate_version": unknown})
                ),
                "MODEL.REFERENCE_INVALID",
                "{backend}"
            );
        }
        // same currency: no rate
        let same = ok(a, "convert", json!({"amount": hundred, "to": "EUR"}));
        assert_eq!(same, json!({"amount": hundred, "rate": null}));
    }
}

#[test]
fn rate_series_are_keyed_and_checked() {
    for (backend, mut a) in adapters("rate-keys") {
        let a = &mut a;
        start(a, vec![]);
        create_rate(
            a,
            1,
            "eur-usd-ecb",
            rate_body("ecb", "EUR", "USD", "1.10"),
            "2026-03-02",
        );
        let create = |n: u32, slug: &str, body: Value| {
            json!({"branch": "main", "changes": [{"op": "create", "class": "FxRate",
                "object_id": uuid(n), "slug": slug, "body": body, "valid": {"from": "2026-03-02"}}]})
        };
        // one series per (source, from, to, rate type)
        assert_eq!(
            code(
                a,
                "commit",
                create(2, "again", rate_body("ecb", "EUR", "USD", "1.2"))
            ),
            "MODEL.KEY_CONFLICT",
            "{backend}"
        );
        for body in [
            rate_body("ecb", "EUR", "EUR", "1"),
            rate_body("ecb", "EUR", "XYZ", "1"),
            rate_body("ecb", "EUR", "GBP", "0"),
            rate_body("ecb", "EUR", "GBP", "-0.8"),
        ] {
            assert_eq!(
                code(a, "commit", create(3, "bad", body.clone())),
                "MODEL.INVALID_VALUE",
                "{body}"
            );
        }
        assert_eq!(
            code(
                a,
                "commit",
                create(3, "bad", rate_body("", "EUR", "GBP", "0.8"))
            ),
            "MODEL.INVALID_VALUE"
        );
        // a second source of the pair: the caller names the series
        let head = create_rate(
            a,
            4,
            "eur-usd-fed",
            rate_body("fed", "EUR", "USD", "1.09"),
            "2026-03-02",
        );
        let ask = json!({"from": "EUR", "to": "USD", "asof": "2026-03-02"});
        assert_eq!(code(a, "fx_rate", ask.clone()), "MODEL.INVALID_VALUE");
        let mut fed = ask.clone();
        fed["source"] = json!("fed");
        fed["rate_type"] = json!("close");
        let r = ok(a, "fx_rate", fed);
        assert_eq!(r["rate"], json!("1.09"));
        assert_eq!(
            r["uri"]
                .as_str()
                .map(|u| u.ends_with("/FxRate/eur-usd-fed")),
            Some(true)
        );
        // an update cannot make a rate invalid either
        assert_eq!(
            code(
                a,
                "commit",
                json!({"branch": "main", "changes": [{"op": "update", "object_id": uuid(4),
                    "base": head, "body": {"rate": "0"}}]})
            ),
            "MODEL.INVALID_VALUE"
        );
    }
}

/// SplitMix64, for the property tests.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % n
    }
}

#[test]
fn property_stored_rates_follow_the_bitemporal_model() {
    // random rate dates and corrections: every lookup at (commit, valid date) equals the
    // pure model of `ubos-ledger` (DSN-DVM-410, verification PROP)
    for (backend, mut a) in adapters("rate-property") {
        let a = &mut a;
        start(a, vec![]);
        let mut rng = Rng(5);
        let mut book = RateBook::new();
        let day = |k: u64| format!("2026-03-{:02}", k + 1);
        let model = |rate: &str| Rate::of(&rate_body("s", "EUR", "USD", rate)).expect("rate");
        let first = format!("1.{:04}", rng.below(9000) + 1000);
        let mut head = create_rate(
            a,
            1,
            "eur-usd",
            rate_body("s", "EUR", "USD", &first),
            &day(10),
        );
        book.record(1, model(&first), &day(10), &head)
            .expect("record");
        let mut last = 1;
        for _ in 0..25 {
            let rate = format!("1.{:04}", rng.below(9000) + 1000);
            // rate dates on or after the first one
            let from = day(10 + rng.below(18));
            let (cseq, v) = set_rate(a, 1, &head, &rate, &from);
            book.record(cseq, model(&rate), &from, &v).expect("record");
            head = v;
            last = cseq;
        }
        let sel = Selector {
            from: "EUR",
            to: "USD",
            source: None,
            rate_type: None,
        };
        for _ in 0..120 {
            let (date, at) = (day(rng.below(30)), 1 + rng.below(last));
            let got = a.execute(
                "fx_rate",
                &json!({"from": "EUR", "to": "USD", "asof": date, "time": at}),
            );
            match book.rate(&sel, &date, at) {
                Ok(q) => {
                    let got = got.unwrap_or_else(|e| panic!("{backend} {date}@{at}: {e}"));
                    assert_eq!(got["version"], json!(q.version), "{backend} {date}@{at}");
                    assert_eq!(got["rate"], json!(q.rate.rate.format(None)));
                    assert_eq!(got["valid"]["from"], json!(q.valid_from));
                    assert_eq!(got["valid"]["to"], json!(q.valid_to));
                }
                Err(e) => assert_eq!(
                    got.expect_err("no rate").code,
                    e.code,
                    "{backend} {date}@{at}"
                ),
            }
        }
    }
}

// ---- DSN-DVM-409, FR-LEDG-053, FR-QRY-042: several currencies ----

#[test]
fn entries_of_several_currencies_total_per_currency() {
    for (backend, mut a) in adapters("totals") {
        let a = &mut a;
        let mut movement = class(
            "Movement",
            vec![
                field("book", "text"),
                field("amount", "money"),
                field("base_amount", "money"),
                field("rate_version", "text"),
            ],
        );
        movement["kind"] = json!("ledger");
        movement["ledger"] =
            json!({"key": ["book"], "amounts": ["amount", "base_amount"], "double_entry": false});
        let mut count = class(
            "Count",
            vec![field("item", "text"), field("quantity", "decimal")],
        );
        count["kind"] = json!("ledger");
        count["ledger"] = json!({"key": ["item"], "amounts": ["quantity"], "double_entry": false});
        start(a, vec![movement, count]);
        let v1 = create_rate(
            a,
            1,
            "eur-usd",
            rate_body("ecb", "EUR", "USD", "1.0842"),
            "2026-10-01",
        );
        let append = |n: u32, amount: Value, extra: Value| {
            let mut body = json!({"book": "b1", "amount": amount});
            for (k, v) in extra.as_object().into_iter().flatten() {
                body[k] = v.clone();
            }
            json!({"op": "append", "class": "Movement", "entry_id": uuid(n),
                "ledger_key": {"book": "b1"}, "valid_at": "2026-10-15", "body": body})
        };
        // a EUR entry with its base amount derived by an explicit conversion whose rate
        // version the entry records
        let eur = json!({"amount": "250.00", "currency": "EUR"});
        let conv = ok(
            a,
            "convert",
            json!({"amount": eur, "to": "USD", "asof": "2026-10-15"}),
        );
        assert_eq!(conv["amount"]["amount"], json!("271.05"), "{backend}");
        commit(
            a,
            json!([
                append(
                    10,
                    json!({"amount": "100.00", "currency": "USD"}),
                    json!({})
                ),
                append(
                    11,
                    eur.clone(),
                    json!({"base_amount": conv["amount"], "rate_version": conv["rate"]["version"]})
                ),
                append(
                    12,
                    json!({"amount": "-30.00", "currency": "USD"}),
                    json!({})
                ),
            ]),
        );
        // DSN-DVM-409 acceptance 1: USD and EUR on one key give two currency balances
        let key = json!({"class": "Movement", "ledger_key": {"book": "b1"}});
        let t = ok(a, "ledger_totals", key.clone());
        assert_eq!(
            t["totals"]["amount"],
            json!([{"amount": "250.00", "currency": "EUR"}, {"amount": "70.00", "currency": "USD"}]),
            "{backend}"
        );
        assert_eq!(
            t["totals"]["base_amount"],
            json!([{"amount": "271.05", "currency": "USD"}])
        );
        assert_eq!(t["entries"], json!(3));
        assert!(t["evaluated_at"]["main"].is_string());
        // the totals follow the coordinates of the read
        let mut early = key.clone();
        early["asof"] = json!("2026-10-14");
        assert_eq!(ok(a, "ledger_totals", early)["totals"]["amount"], json!([]));

        // FR-LEDG-053 acceptance 1: the entry references the rate version, and the base
        // amount recomputes identically after the rate was corrected
        set_rate(a, 1, &v1, "1.0900", "2026-10-01");
        let entries = ok(a, "read_ledger", key.clone());
        let entry = entries["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .find(|e| e["entry_id"] == json!(uuid(11)))
            .expect("the EUR entry")
            .clone();
        assert_eq!(entry["body"]["rate_version"], json!(v1));
        let again = ok(
            a,
            "convert",
            json!({"amount": entry["body"]["amount"], "to": "USD",
                "rate_version": entry["body"]["rate_version"]}),
        );
        assert_eq!(again["amount"], entry["body"]["base_amount"], "{backend}");
        let fresh = ok(
            a,
            "convert",
            json!({"amount": eur, "to": "USD", "asof": "2026-10-15"}),
        );
        assert_eq!(fresh["amount"]["amount"], json!("272.50"));

        // a decimal amount field has one total
        commit(
            a,
            json!([
                {"op": "append", "class": "Count", "entry_id": uuid(20), "ledger_key": {"item": "i"},
                    "valid_at": "2026-10-15", "body": {"item": "i", "quantity": "2.5"}},
                {"op": "append", "class": "Count", "entry_id": uuid(21), "ledger_key": {"item": "i"},
                    "valid_at": "2026-10-15", "body": {"item": "i", "quantity": "4"}},
            ]),
        );
        let q = ok(
            a,
            "ledger_totals",
            json!({"class": "Count", "ledger_key": {"item": "i"}}),
        );
        assert_eq!(q["totals"]["quantity"], json!("6.5"));
        assert_eq!(
            code(
                a,
                "ledger_totals",
                json!({"class": "FxRate", "ledger_key": {}})
            ),
            "MODEL.INVALID_VALUE"
        );
    }
}

#[test]
fn aggregates_group_money_by_currency_unless_converted() {
    for (backend, mut a) in adapters("aggregates") {
        let a = &mut a;
        start(
            a,
            vec![class(
                "Holding",
                vec![field("desk", "text"), field("amount", "money")],
            )],
        );
        let holding = |n: u32, desk: &str, amount: &str, cur: &str| {
            json!({"op": "create", "class": "Holding", "object_id": uuid(n), "slug": format!("h{n}"),
                "body": {"desk": desk, "amount": {"amount": amount, "currency": cur}}})
        };
        commit(
            a,
            json!([
                holding(1, "a", "100.00", "USD"),
                holding(2, "a", "50.00", "EUR"),
                holding(3, "b", "20.00", "EUR"),
                holding(4, "b", "5.00", "USD"),
                holding(5, "b", "1000", "JPY"),
            ]),
        );
        let head = create_rate(
            a,
            10,
            "eur-usd",
            rate_body("ecb", "EUR", "USD", "1.10"),
            "2026-01-01",
        );
        let sum = json!({"from": "Holding", "aggregates": [{"fn": "sum", "path": "amount", "as": "total"}]});
        // FR-QRY-042 acceptance 1: without a conversion there is one total per currency
        let plain = ok(a, "query", sum.clone());
        assert_eq!(
            plain["groups"],
            json!([
                {"currency": "EUR", "total": {"amount": "70.00", "currency": "EUR"}},
                {"currency": "JPY", "total": {"amount": "1000", "currency": "JPY"}},
                {"currency": "USD", "total": {"amount": "105.00", "currency": "USD"}},
            ]),
            "{backend}"
        );
        // an explicit conversion with given rates: one total
        let mut given = sum.clone();
        given["convert"] = json!({"currency": "USD", "rates": {"EUR": "1.10", "JPY": "0.0066"}});
        let g = ok(a, "query", given);
        assert_eq!(
            g["groups"],
            json!([{"currency": "USD", "total": {"amount": "188.60", "currency": "USD"}}]),
            "{backend}"
        );
        assert_eq!(g["rates"], json!([]));
        // a currency without a rate cannot be converted
        let mut missing = sum.clone();
        missing["convert"] = json!({"currency": "USD", "rates": {"EUR": "1.10"}});
        assert_eq!(code(a, "query", missing), "MODEL.CURRENCY_MISMATCH");
        // the stored rates of the coordinates, referenced by the result
        let mut stored = json!({"from": "Holding", "where": "this.desk == \"a\"",
            "aggregates": [{"fn": "sum", "path": "amount", "as": "total"}]});
        stored["convert"] = json!({"currency": "USD", "source": "ecb"});
        let s = ok(a, "query", stored.clone());
        assert_eq!(
            s["groups"],
            json!([{"currency": "USD", "total": {"amount": "155.00", "currency": "USD"}}]),
            "{backend}"
        );
        assert_eq!(s["rates"][0]["object_id"], json!(uuid(10)));
        assert_eq!(s["rates"][0]["rate"], json!("1.1"));
        assert!(s["rates"][0]["version"].is_string());
        // no stored rate for JPY
        let mut all = stored.clone();
        all["where"] = Value::Null;
        assert_eq!(code(a, "query", all), "MODEL.CURRENCY_MISMATCH");

        // L1: `convert` takes the URI of a stored rate object, read at the query's asof
        let authority = a.engine_mut().store.authority();
        let uri = format!("ubos://{authority}/FxRate/eur-usd");
        let q = |asof: &str| {
            json!({"from": "Holding", "asof": asof, "order_by": [{"path": "this.desk"}],
                "where": format!("currency(this.amount) == \"EUR\" and convert(this.amount, \"USD\", \"{uri}\") > 50.00USD")})
        };
        commit(
            a,
            json!([{"op": "update", "object_id": uuid(10), "base": head, "body": {"rate": "0.90"},
                "valid": {"kind": "supersede_from", "from": "2026-12-01"}}]),
        );
        assert_eq!(
            ok(a, "query", q("2026-11-02"))["objects"],
            json!([uuid(2)]),
            "{backend}"
        );
        assert_eq!(
            ok(a, "query", q("2026-12-01"))["objects"],
            json!([]),
            "{backend}"
        );
        let unknown = json!({"from": "Holding",
            "where": "convert(this.amount, \"USD\", \"ubos://x/FxRate/none\") > 1.00USD"});
        assert_eq!(code(a, "query", unknown), "MODEL.REFERENCE_INVALID");
    }
}

// ---- DSN-DVM-447, FR-LOC-041, FR-LOC-042: calendars and cut-offs ----

fn calendar(n: u32, slug: &str, body: Value) -> Value {
    json!({"op": "create", "class": "BusinessCalendar", "object_id": uuid(n), "slug": slug,
        "body": body, "valid": {"from": "2026-01-01"}})
}

/// Three calendars: an exchange, a bank and their union with a 12:00 New York cut-off.
fn calendars(a: &mut Adapter, authority: &str) -> (String, String) {
    let uri = |slug: &str| format!("ubos://{authority}/BusinessCalendar/{slug}");
    let r = commit(
        a,
        json!([
            calendar(
                1,
                "exchange",
                json!({"name": "exchange", "weekend": [6, 7],
                "holidays": ["2026-11-26", "2026-12-25"]})
            ),
            calendar(
                2,
                "bank",
                json!({"name": "bank", "holidays": ["2026-11-11"]})
            ),
        ]),
    );
    let bank_version = r["versions"][uuid(2)]
        .as_str()
        .expect("version")
        .to_string();
    commit(
        a,
        json!([calendar(
            3,
            "settlement",
            json!({"name": "settlement",
            "union": [uri("exchange"), uri("bank")],
            "cutoff_time": "12:00:00", "cutoff_tz": "America/New_York"})
        )]),
    );
    (uri("settlement"), bank_version)
}

#[test]
fn a_union_calendar_is_resolved_at_the_coordinates_of_the_read() {
    for (backend, mut a) in adapters("calendars") {
        let a = &mut a;
        let authority = start(a, vec![]);
        let (settlement, bank_v1) = calendars(a, &authority);
        let c = ok(a, "calendar", json!({"calendar": settlement}));
        assert_eq!(
            c["calendar"],
            json!({"weekend": [6, 7], "holidays": ["2026-11-11", "2026-11-26", "2026-12-25"]}),
            "{backend}"
        );
        assert_eq!(
            c["cutoff"],
            json!({"time": "12:00:00", "tz": "America/New_York"})
        );
        assert_eq!(c["versions"].as_array().map(Vec::len), Some(3));
        let bd = |a: &mut Adapter, extra: Value| {
            let mut i = json!({"calendar": settlement});
            for (k, v) in extra.as_object().into_iter().flatten() {
                i[k] = v.clone();
            }
            ok(a, "business_day", i)["value"].clone()
        };
        // FR-LOC-041 acceptance 1: T+2 from the day before Thanksgiving skips the holiday
        assert_eq!(
            bd(
                a,
                json!({"fn": "add_business_days", "date": "2026-11-25", "days": 2})
            ),
            json!("2026-11-30"),
            "{backend}"
        );
        assert_eq!(
            bd(a, json!({"fn": "is_business_day", "date": "2026-11-11"})),
            json!(false)
        );
        assert_eq!(
            bd(
                a,
                json!({"fn": "previous_business_day", "date": "2026-11-12"})
            ),
            json!("2026-11-10")
        );

        // DSN-DVM-447 acceptance 1: a holiday is added to one member from a valid date
        let before = ok(a, "calendar", json!({"calendar": settlement}))["evaluated_at"].clone();
        let r = commit(
            a,
            json!([{"op": "update", "object_id": uuid(2), "base": bank_v1,
                "body": {"holidays": ["2026-11-11", "2026-12-24"]},
                "valid": {"kind": "supersede_from", "from": "2026-12-01"}}]),
        );
        let added = r["cseq"].as_u64().expect("cseq");
        let step = json!({"fn": "add_business_days", "date": "2026-12-23", "days": 1});
        let at = |a: &mut Adapter, extra: Value| {
            let mut i = step.clone();
            for (k, v) in extra.as_object().into_iter().flatten() {
                i[k] = v.clone();
            }
            bd(a, i)
        };
        // read before the valid date: the holiday is ignored
        assert_eq!(
            at(a, json!({"asof": "2026-11-30"})),
            json!("2026-12-24"),
            "{backend}"
        );
        assert_eq!(at(a, json!({})), json!("2026-12-24"), "{backend} (today)");
        // read on or after it: the 24th is skipped, then Christmas and the weekend
        assert_eq!(
            at(a, json!({"asof": "2026-12-01"})),
            json!("2026-12-28"),
            "{backend}"
        );
        assert_eq!(
            at(a, json!({"asof": "2026-12-23"})),
            json!("2026-12-28"),
            "{backend}"
        );
        // at the commit before the change, the holiday is unknown at every valid date
        assert_eq!(
            at(a, json!({"asof": "2026-12-23", "time": added - 1})),
            json!("2026-12-24"),
            "{backend}"
        );
        let then = ok(
            a,
            "calendar",
            json!({"calendar": settlement, "asof": "2026-12-23", "commit": before["main"]}),
        );
        assert_eq!(then["evaluated_at"], before);
        assert_eq!(
            then["calendar"]["holidays"].as_array().map(Vec::len),
            Some(3)
        );
        // the versions the union was resolved from change with the member
        let v_old = ok(
            a,
            "calendar",
            json!({"calendar": settlement, "asof": "2026-11-30"}),
        )["versions"]
            .clone();
        let v_new = ok(
            a,
            "calendar",
            json!({"calendar": settlement, "asof": "2026-12-01"}),
        )["versions"]
            .clone();
        assert_ne!(v_old, v_new);

        // before the calendars are valid, and for an unknown calendar
        assert_eq!(
            code(
                a,
                "calendar",
                json!({"calendar": settlement, "asof": "2025-12-31"})
            ),
            "QRY.NOT_FOUND_AT_TIME"
        );
        assert_eq!(
            code(
                a,
                "calendar",
                json!({"calendar": format!("ubos://{authority}/BusinessCalendar/none")})
            ),
            "IAM.NOT_FOUND"
        );
    }
}

#[test]
fn cut_offs_are_declared_on_the_calendar() {
    for (backend, mut a) in adapters("cutoffs") {
        let a = &mut a;
        let authority = start(a, vec![]);
        let (settlement, _) = calendars(a, &authority);
        let eff = |a: &mut Adapter, at: &str| {
            ok(
                a,
                "business_day",
                json!({"calendar": settlement, "fn": "effective_date", "at": at}),
            )["value"]
                .clone()
        };
        // FR-LOC-042 acceptance 1: 12:00 New York; an order at 12:05 moves to the next
        // business day (24 November 2026 is UTC-5)
        assert_eq!(
            eff(a, "2026-11-24T16:59:00.000000Z"),
            json!("2026-11-24"),
            "{backend}"
        );
        assert_eq!(
            eff(a, "2026-11-24T17:05:00.000000Z"),
            json!("2026-11-25"),
            "{backend}"
        );
        // after the cut-off before the holiday: Friday
        assert_eq!(eff(a, "2026-11-25T17:05:00.000000Z"), json!("2026-11-27"));
        assert_eq!(
            ok(
                a,
                "business_day",
                json!({"calendar": settlement, "fn": "cutoff", "date": "2026-11-26"})
            )["value"],
            json!("2026-11-27T17:00:00.000000Z")
        );
        // another cut-off than the declared one
        assert_eq!(
            ok(
                a,
                "business_day",
                json!({"calendar": settlement, "fn": "effective_date", "at": "2026-11-24T17:05:00.000000Z",
                    "cutoff_time": "16:00:00", "cutoff_tz": "America/New_York"})
            )["value"],
            json!("2026-11-24")
        );
        // a calendar without a cut-off
        let exchange = format!("ubos://{authority}/BusinessCalendar/exchange");
        assert_eq!(
            code(
                a,
                "business_day",
                json!({"calendar": exchange, "fn": "cutoff", "date": "2026-11-24"})
            ),
            "MODEL.INVALID_VALUE"
        );
        // the cut-off is declared with a time and a known zone
        for (body, error) in [
            (
                json!({"name": "x", "cutoff_time": "12:00:00"}),
                "MODEL.INVALID_VALUE",
            ),
            (
                json!({"name": "x", "cutoff_time": "12:00:00", "cutoff_tz": "Mars/Olympus"}),
                "MODEL.INVALID_VALUE",
            ),
            (
                json!({"name": "x", "cutoff_time": "noon", "cutoff_tz": "UTC"}),
                "MODEL.INVALID_VALUE",
            ),
            (
                json!({"name": "x", "weekend": [8]}),
                "MODEL.CONSTRAINT_VIOLATED",
            ),
            (
                json!({"name": "x", "holidays": ["2026-02-30"]}),
                "MODEL.INVALID_VALUE",
            ),
        ] {
            assert_eq!(
                code(
                    a,
                    "commit",
                    json!({"branch": "main", "changes": [calendar(9, "bad", body.clone())]})
                ),
                error,
                "{body}"
            );
        }
        // a union names calendars that exist
        let ghost = format!("ubos://{authority}/BusinessCalendar/ghost");
        assert_eq!(
            code(
                a,
                "commit",
                json!({"branch": "main", "changes": [calendar(9, "bad", json!({"name": "x", "union": [ghost]}))]})
            ),
            "MODEL.REFERENCE_INVALID"
        );
    }
}

#[test]
fn stored_calendars_serve_l1_functions_guards_and_programs() {
    for (backend, mut a) in adapters("calendar-l1") {
        let a = &mut a;
        let mut order = class(
            "Order",
            vec![
                field("trade_date", "date"),
                field("settle_date", "date"),
                field("received", "instant"),
                field("market", "text"),
            ],
        );
        order["ports"] = json!([{"name": "place", "kind": "command", "description": "place",
            "impl": "t.orders#place", "permission": "place", "params": [],
            "guards": ["this.received <= cutoff(this.trade_date, \"12:00:00\", \"America/New_York\", this.market)"]}]);
        let authority = start(a, vec![order]);
        let (settlement, bank_v1) = calendars(a, &authority);
        let new_order = |n: u32, trade: &str, received: &str| {
            json!({"op": "create", "class": "Order", "object_id": uuid(n), "slug": format!("o{n}"),
                "body": {"trade_date": trade, "received": received, "market": settlement}})
        };
        commit(
            a,
            json!([
                {"op": "create", "class": "Logic", "object_id": uuid(50), "slug": "t-orders",
                    "body": {"name": "t.orders", "abi": "1.0", "tier": "host", "state": "committed",
                        "profile": "standard", "capabilities": ["commit", "read"], "exports": ["place"],
                        "source": {"functions": {"place": {"params": [], "body": [
                            {"do": "read.get", "as": "o", "args": {"target": "self_id"}},
                            {"do": "commit.update", "args": {"target": "self_id",
                                "set": {"settle_date": "add_business_days(this.trade_date, 2, this.market)"}}},
                            {"return": "add_business_days(this.trade_date, 2, this.market)"}]}}}}},
                new_order(60, "2026-11-25", "2026-11-25T16:30:00.000000Z"),
                new_order(61, "2026-11-25", "2026-11-25T17:05:00.000000Z"),
                new_order(62, "2026-12-23", "2026-12-23T15:00:00.000000Z"),
            ]),
        );
        ok(a, "verify_asset", json!({"object_id": uuid(50)}));
        // queries: the calendar functions take the URI of the stored calendar
        let q = |asof: Option<&str>, expr: &str| {
            let mut i =
                json!({"from": "Order", "where": expr, "order_by": [{"path": "this.trade_date"}]});
            if let Some(d) = asof {
                i["asof"] = json!(d);
            }
            i
        };
        let t2 = "add_business_days(this.trade_date, 2, this.market) == date(2026, 11, 30)";
        assert_eq!(
            ok(a, "query", q(None, t2))["objects"],
            json!([uuid(60), uuid(61)]),
            "{backend}"
        );
        let literal = format!("is_business_day(date(2026, 11, 11), \"{settlement}\")");
        assert_eq!(
            ok(a, "query", q(None, &literal))["objects"],
            json!([]),
            "{backend}"
        );
        let exchange = format!(
            "is_business_day(date(2026, 11, 11), \"ubos://{authority}/BusinessCalendar/exchange\")"
        );
        assert_eq!(
            ok(a, "query", q(None, &exchange))["objects"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        assert_eq!(
            code(
                a,
                "query",
                q(
                    None,
                    "is_business_day(this.trade_date, \"ubos://x/BusinessCalendar/none\")"
                )
            ),
            "MODEL.REFERENCE_INVALID"
        );
        // guards: the port is available for the order received before the cut-off only
        let available = |a: &mut Adapter, n: u32| {
            let r = ok(a, "actions", json!({"object_id": uuid(n)}));
            r["actions"][0]["available"].clone()
        };
        assert_eq!(available(a, 60), json!(true), "{backend}");
        assert_eq!(available(a, 61), json!(false), "{backend}");
        let run = |n: u32| {
            json!({"branch": "main", "target": uuid(n), "port": "place", "params": {},
                "entropy_seed": "1111111111111111111111111111111111111111111111111111111111111111"})
        };
        assert_eq!(
            code(a, "execute", run(61)),
            "FLOW.GUARD_FAILED",
            "{backend}"
        );
        // programs: the settlement date is computed on the stored calendar
        let done = ok(a, "execute", run(60));
        assert_eq!(done["result"], json!("2026-11-30"), "{backend}");
        let o = ok(a, "read", json!({"object_id": uuid(60)}));
        assert_eq!(o["body"]["settle_date"], json!("2026-11-30"));
        // the evidence: the derivation of the change names the calendar versions used
        let derivation = ok(a, "derivation", json!({"object_id": uuid(60)})).to_string();
        for member in [1, 2, 3] {
            assert!(
                derivation.contains(&uuid(member)),
                "{backend}: {derivation}"
            );
        }

        // a holiday added to a member from 1 December: the query follows its asof
        commit(
            a,
            json!([{"op": "update", "object_id": uuid(2), "base": bank_v1,
                "body": {"holidays": ["2026-11-11", "2026-12-24"]},
                "valid": {"kind": "supersede_from", "from": "2026-12-01"}}]),
        );
        let eve = "add_business_days(this.trade_date, 1, this.market) == date(2026, 12, 24)";
        assert_eq!(
            ok(a, "query", q(Some("2026-11-30"), eve))["objects"],
            json!([uuid(62)]),
            "{backend}"
        );
        assert_eq!(
            ok(a, "query", q(Some("2026-12-02"), eve))["objects"],
            json!([]),
            "{backend}"
        );
    }
}

#[test]
fn property_stored_calendars_agree_with_the_l1_functions() {
    // the L1 functions over a calendar value and the arithmetic of `ubos-ledger` over the
    // same calendar give the same dates (DSN-DVM-447, verification PROP)
    let mut a = Adapter::new().expect("adapter");
    let mut rng = Rng(77);
    let day = |k: u64| {
        (chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date")
            + chrono::Duration::days(k as i64))
        .format("%Y-%m-%d")
        .to_string()
    };
    for _ in 0..150 {
        let weekend: Vec<u64> = (1..=7).filter(|_| rng.below(4) == 0).take(5).collect();
        let holidays: Vec<String> = (0..rng.below(10)).map(|_| day(rng.below(90))).collect();
        let value = json!({"weekend": weekend, "holidays": holidays});
        let cal = Calendar::of(&value).expect("calendar");
        let d = day(rng.below(90));
        let n = rng.below(21) as i64 - 10;
        let date = ubos_types::time::parse_date(&d).expect("date");
        let l1 = |a: &mut Adapter, expr: String| {
            let (y, m, dd) = (
                &d[..4],
                d[5..7].trim_start_matches('0'),
                d[8..].trim_start_matches('0'),
            );
            let expr = expr.replace("D", &format!("date({y}, {m}, {dd})"));
            ok(
                a,
                "eval_expr",
                json!({"expr": expr, "ctx": {"now": NOW}, "calendars": {"c": value}}),
            )["value"]
                .clone()
        };
        let text = |x: chrono::NaiveDate| json!(x.format("%Y-%m-%d").to_string());
        assert_eq!(
            l1(&mut a, "is_business_day(D, \"c\")".into()),
            json!(cal.is_business_day(date))
        );
        assert_eq!(
            l1(&mut a, format!("add_business_days(D, {n}, \"c\")")),
            text(cal.add_business_days(date, n).expect("step"))
        );
        assert_eq!(
            l1(&mut a, "next_business_day(D, \"c\")".into()),
            text(cal.next_business_day(date).expect("next"))
        );
        assert_eq!(
            l1(&mut a, "previous_business_day(D, \"c\")".into()),
            text(cal.previous_business_day(date).expect("previous"))
        );
        let cut =
            ubos_ledger::calendar::Cutoff::new("12:00:00", "America/New_York").expect("cut-off");
        assert_eq!(
            l1(
                &mut a,
                "cutoff(D, \"12:00:00\", \"America/New_York\", \"c\")".into()
            ),
            json!(cut.instant_text(date, &cal).expect("instant"))
        );
    }
}
