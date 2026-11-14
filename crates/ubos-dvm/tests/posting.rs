//! Double entry as a general mechanism (PH-2 step 3b, DEC-098): the acceptance criteria of
//! DSN-DVM-440 (balanced posting groups, STD-KIND-024) and DSN-DVM-441 (hierarchies of
//! ledger keys and their roll-ups), with FR-LEDG-051, FR-LEDG-052 and the balance per
//! currency of DSN-DVM-409 / FR-LEDG-053.
//!
//! The classes of the tests are neutral: `Bucket` is a class below the genesis class
//! `LedgerNode`, `Line` a ledger class in double entry whose key references it.
//!
//! Every test runs on the in-memory store and, when `UBOS_TEST_DATABASE` names a
//! PostgreSQL connection string, on PostgreSQL as well (each test in its own VAE scope).

use serde_json::{Value, json};
use std::collections::BTreeMap;
use ubos_dvm::Adapter;
use ubos_types::BpaError;

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
            let tenant = format!("01890a5d-3b00-7000-8000-{:012x}", n);
            let vae = format!("01890a5d-3b00-7001-8000-{:012x}", n);
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

fn fails(a: &mut Adapter, op: &str, input: Value) -> BpaError {
    match a.execute(op, &input) {
        Ok(v) => panic!("{op} {input} gave {v}"),
        Err(e) => e,
    }
}

fn uuid(n: u32) -> String {
    format!("01890a5d-ac97-7{:03x}-8000-{:012x}", n & 0xfff, n)
}

fn field(name: &str, type_: &str) -> Value {
    json!({"name": name, "type": type_, "label": name, "description": name})
}

fn class(name: &str, parent: &str, fields: Vec<Value>) -> Value {
    json!({"name": name, "label": name, "description": name, "namespace": "t.posting",
        "parent": parent, "kind": "definition", "schema_version": 1, "fields": fields})
}

fn ledger(name: &str, fields: Vec<Value>, decl: Value) -> Value {
    let mut c = class(name, "DataObject", fields);
    c["kind"] = json!("ledger");
    c["ledger"] = decl;
    c
}

fn node_ref() -> Value {
    let mut f = field("node", "reference");
    f["constraints"] = json!({"target_class": "Bucket"});
    f
}

/// `Bucket` below `LedgerNode`; `Line` in double entry, keyed by a node; `Split` in double
/// entry, keyed by a unit and a node, with a money and a decimal amount; `Slip` without
/// double entry.
fn classes() -> Vec<Value> {
    vec![
        class("Bucket", "LedgerNode", vec![field("category", "text")]),
        ledger(
            "Line",
            vec![node_ref(), field("amount", "money"), field("memo", "text")],
            json!({"key": ["node"], "amounts": ["amount"], "double_entry": true}),
        ),
        ledger(
            "Split",
            vec![
                field("unit", "text"),
                node_ref(),
                field("amount", "money"),
                field("quantity", "decimal"),
            ],
            json!({"key": ["unit", "node"], "amounts": ["amount", "quantity"],
                "double_entry": true}),
        ),
        ledger(
            "Slip",
            vec![field("item", "text"), field("quantity", "decimal")],
            json!({"key": ["item"], "amounts": ["quantity"], "double_entry": false}),
        ),
    ]
}

/// A started adapter; returns the URI prefix of the nodes.
fn start(a: &mut Adapter) -> String {
    ok(a, "reset", json!({}));
    ok(a, "set_now", json!({"now": NOW}));
    ok(a, "load_genesis", json!({"classes": classes()}));
    format!("ubos://{}/Bucket/", a.engine_mut().store.authority())
}

fn commit(a: &mut Adapter, changes: Value) -> Value {
    ok(a, "commit", json!({"branch": "main", "changes": changes}))
}

fn commit_fails(a: &mut Adapter, changes: Value) -> BpaError {
    fails(a, "commit", json!({"branch": "main", "changes": changes}))
}

/// The creation of a node valid from 2026-01-01.
fn node(n: u32, pre: &str, slug: &str, parent: Option<&str>, extra: Value) -> Value {
    let mut body = json!({"name": slug});
    if let Some(p) = parent {
        body["parent"] = json!(format!("{pre}{p}"));
    }
    for (k, v) in extra.as_object().into_iter().flatten() {
        body[k] = v.clone();
    }
    json!({"op": "create", "class": "Bucket", "object_id": uuid(n), "slug": slug,
        "body": body, "valid": {"from": "2026-01-01"}})
}

fn money(amount: &str, currency: &str) -> Value {
    json!({"amount": amount, "currency": currency})
}

/// An entry of `Line` on a node.
fn line(n: u32, pre: &str, slug: &str, amount: Value, group: &str, valid_at: &str) -> Value {
    let uri = format!("{pre}{slug}");
    json!({"op": "append", "class": "Line", "entry_id": uuid(n), "ledger_key": {"node": uri},
        "valid_at": valid_at, "posting_group": group, "body": {"node": uri, "amount": amount}})
}

fn usd(n: u32, pre: &str, slug: &str, amount: &str, group: &str) -> Value {
    line(n, pre, slug, money(amount, "USD"), group, "2026-10-15")
}

fn entries(a: &mut Adapter, pre: &str, slug: &str) -> usize {
    ok(
        a,
        "read_ledger",
        json!({"class": "Line", "ledger_key": {"node": format!("{pre}{slug}")}}),
    )["entries"]
        .as_array()
        .expect("entries")
        .len()
}

fn rollup(a: &mut Adapter, pre: &str, slug: &str, extra: Value) -> Value {
    let mut i = json!({"class": "Line", "node": format!("{pre}{slug}")});
    for (k, v) in extra.as_object().into_iter().flatten() {
        i[k] = v.clone();
    }
    ok(a, "ledger_rollup", i)
}

/// A four-level tree: `top` over `a` (`a1` over `a11` and `a12`; `a2`) and `b` (`b1`), and
/// a second root `other`.
fn four_levels(a: &mut Adapter, pre: &str) -> Value {
    commit(
        a,
        json!([
            node(1, pre, "top", None, json!({})),
            node(2, pre, "a", Some("top"), json!({})),
            node(3, pre, "a1", Some("a"), json!({})),
            node(4, pre, "a11", Some("a1"), json!({})),
            node(5, pre, "a12", Some("a1"), json!({})),
            node(6, pre, "a2", Some("a"), json!({})),
            node(7, pre, "b", Some("top"), json!({})),
            node(8, pre, "b1", Some("b"), json!({})),
            node(9, pre, "other", None, json!({})),
        ]),
    )
}

// ---- DSN-DVM-440, FR-LEDG-051, STD-KIND-024: balanced posting groups ----

#[test]
fn an_unbalanced_group_rejects_the_whole_commit() {
    for (backend, mut a) in adapters("unbalanced") {
        let a = &mut a;
        let pre = start(a);
        four_levels(a, &pre);
        // DSN-DVM-440 acceptance 1: a group off by 0.01 fails and reports the difference
        let e = commit_fails(
            a,
            json!([
                usd(20, &pre, "a11", "100.00", "g1"),
                usd(21, &pre, "b1", "-99.99", "g1"),
                // the rest of the commit is sound and is rejected with it
                usd(22, &pre, "a12", "5.00", "g2"),
                usd(23, &pre, "b1", "-5.00", "g2"),
                node(30, &pre, "late", Some("top"), json!({})),
            ]),
        );
        assert_eq!(e.code, "LEDG.UNBALANCED", "{backend}");
        assert_eq!(
            e.details.expect("details")["groups"],
            json!([{"scope": "Line", "posting_group": "g1",
                "differences": [{"currency": "USD", "difference": "0.01"}]}]),
            "{backend}"
        );
        for leaf in ["a11", "a12", "b1"] {
            assert_eq!(entries(a, &pre, leaf), 0, "{backend} {leaf}");
        }
        assert_eq!(
            fails(a, "read", json!({"branch": "main", "object_id": uuid(30)})).code,
            "IAM.NOT_FOUND",
            "{backend}"
        );
        // FR-LEDG-051 acceptance 1: 100 against 99
        let e = commit_fails(
            a,
            json!([
                usd(20, &pre, "a11", "100.00", "g1"),
                usd(21, &pre, "b1", "-99.00", "g1")
            ]),
        );
        assert_eq!(e.code, "LEDG.UNBALANCED", "{backend}");
        assert_eq!(
            e.details.expect("details")["groups"][0]["differences"],
            json!([{"currency": "USD", "difference": "1.00"}])
        );
        // every unbalanced group is named; an entry without a group is its own group
        let mut alone = usd(26, &pre, "a2", "3.00", "");
        alone
            .as_object_mut()
            .expect("change")
            .remove("posting_group");
        let e = commit_fails(
            a,
            json!([
                usd(20, &pre, "a11", "100.00", "g1"),
                usd(21, &pre, "b1", "-100.00", "g1"),
                usd(22, &pre, "a12", "7.00", "g2"),
                usd(23, &pre, "b1", "-6.00", "g2"),
                alone,
            ]),
        );
        let groups = e.details.expect("details")["groups"].clone();
        assert_eq!(groups.as_array().expect("groups").len(), 2, "{backend}");
        assert_eq!(groups[1]["posting_group"], json!("g2"));
        assert_eq!(
            groups[0]["differences"],
            json!([{"currency": "USD", "difference": "3.00"}])
        );
        // the balanced commit goes through: three entries of one group, two of another
        commit(
            a,
            json!([
                usd(20, &pre, "a11", "100.00", "g1"),
                usd(21, &pre, "b1", "-60.00", "g1"),
                usd(22, &pre, "a12", "-40.00", "g1"),
                usd(23, &pre, "a2", "7.00", "g2"),
                usd(24, &pre, "b1", "-7.00", "g2"),
            ]),
        );
        assert_eq!(entries(a, &pre, "b1"), 2, "{backend}");
    }
}

#[test]
fn a_group_balances_per_currency() {
    for (backend, mut a) in adapters("per-currency") {
        let a = &mut a;
        let pre = start(a);
        four_levels(a, &pre);
        let at = "2026-10-15";
        // equal numbers of two currencies do not balance (DSN-DVM-409, STD-KIND-024)
        let e = commit_fails(
            a,
            json!([
                line(20, &pre, "a11", money("100.00", "USD"), "g1", at),
                line(21, &pre, "b1", money("-100.00", "EUR"), "g1", at),
            ]),
        );
        assert_eq!(e.code, "LEDG.UNBALANCED", "{backend}");
        assert_eq!(
            e.details.expect("details")["groups"][0]["differences"],
            json!([{"currency": "EUR", "difference": "-100.00"},
                {"currency": "USD", "difference": "100.00"}]),
            "{backend}"
        );
        // a group of several currencies balances when each currency does
        commit(
            a,
            json!([
                line(20, &pre, "a11", money("100.00", "USD"), "g1", at),
                line(21, &pre, "b1", money("-100.00", "USD"), "g1", at),
                line(22, &pre, "a11", money("250.00", "EUR"), "g1", at),
                line(23, &pre, "b1", money("-250.00", "EUR"), "g1", at),
                line(24, &pre, "a12", money("5000", "JPY"), "g1", at),
                line(25, &pre, "b1", money("-5000", "JPY"), "g1", at),
            ]),
        );
        // DSN-DVM-409 acceptance 1 on a roll-up: one total per currency
        let r = rollup(a, &pre, "a", json!({}));
        assert_eq!(
            r["totals"]["amount"],
            json!([
                money("250.00", "EUR"),
                money("5000", "JPY"),
                money("100.00", "USD")
            ]),
            "{backend}"
        );
        assert_eq!(
            rollup(a, &pre, "top", json!({}))["totals"]["amount"],
            json!([
                money("0.00", "EUR"),
                money("0", "JPY"),
                money("0.00", "USD")
            ]),
            "{backend}"
        );
        // every amount field counts: a money amount and a decimal quantity
        let split = |n: u32, slug: &str, amount: &str, quantity: &str| {
            let uri = format!("{pre}{slug}");
            json!({"op": "append", "class": "Split", "entry_id": uuid(n),
                "ledger_key": {"unit": "u1", "node": uri}, "valid_at": at, "posting_group": "s1",
                "body": {"unit": "u1", "node": uri, "amount": money(amount, "USD"),
                    "quantity": quantity}})
        };
        let e = commit_fails(
            a,
            json!([
                split(30, "a11", "10.00", "2"),
                split(31, "b1", "-10.00", "-3")
            ]),
        );
        assert_eq!(e.code, "LEDG.UNBALANCED", "{backend}");
        assert_eq!(
            e.details.expect("details")["groups"],
            json!([{"scope": "Split", "posting_group": "s1",
                "differences": [{"currency": null, "difference": "-1"}]}]),
            "{backend}"
        );
        commit(
            a,
            json!([
                split(30, "a11", "10.00", "2"),
                split(31, "b1", "-10.00", "-2")
            ]),
        );
        // a class with several key fields rolls up with the other fields given
        let r = ok(
            a,
            "ledger_rollup",
            json!({"class": "Split", "node": format!("{pre}a"), "ledger_key": {"unit": "u1"}}),
        );
        assert_eq!(
            r["totals"],
            json!({"amount": [money("10.00", "USD")], "quantity": "2"}),
            "{backend}"
        );
        assert_eq!(r["entries"], json!(1));
        assert_eq!(
            fails(
                a,
                "ledger_rollup",
                json!({"class": "Split", "node": format!("{pre}a")})
            )
            .code,
            "MODEL.INVALID_VALUE"
        );
    }
}

#[test]
fn groups_of_other_classes_reversals_and_merges() {
    for (backend, mut a) in adapters("groups") {
        let a = &mut a;
        let pre = start(a);
        four_levels(a, &pre);
        // a class without double entry takes single entries
        commit(
            a,
            json!([{"op": "append", "class": "Slip", "entry_id": uuid(40),
                "ledger_key": {"item": "bolt"}, "posting_group": "g1",
                "body": {"item": "bolt", "quantity": "5"}}]),
        );
        commit(
            a,
            json!([
                usd(20, &pre, "a11", "100.00", "g1"),
                usd(21, &pre, "b1", "-100.00", "g1")
            ]),
        );
        // the reversal of one entry of a group does not balance; of the whole group it does
        let rev =
            |n: u32, m: u32| json!({"op": "reverse", "entry_id": uuid(n), "new_entry_id": uuid(m)});
        let e = commit_fails(a, json!([rev(20, 50)]));
        assert_eq!(e.code, "LEDG.UNBALANCED", "{backend}");
        assert_eq!(
            e.details.expect("details")["groups"],
            json!([{"scope": "Line", "posting_group": "g1/reversal",
                "differences": [{"currency": "USD", "difference": "-100.00"}]}])
        );
        commit(a, json!([rev(20, 50), rev(21, 51)]));
        assert_eq!(
            rollup(a, &pre, "a", json!({}))["totals"]["amount"],
            json!([money("0.00", "USD")]),
            "{backend}"
        );
        // entries of a branch are checked on the branch and again at the merge
        ok(
            a,
            "create_branch",
            json!({"name": "feature/x", "from": "main"}),
        );
        let on_branch = |changes: Value| json!({"branch": "feature/x", "changes": changes});
        assert_eq!(
            fails(
                a,
                "commit",
                on_branch(json!([usd(60, &pre, "a12", "9.00", "g9")]))
            )
            .code,
            "LEDG.UNBALANCED",
            "{backend}"
        );
        ok(
            a,
            "commit",
            on_branch(json!([
                usd(60, &pre, "a12", "9.00", "g9"),
                usd(61, &pre, "b1", "-9.00", "g9")
            ])),
        );
        assert_eq!(entries(a, &pre, "a12"), 0);
        ok(a, "merge", json!({"source": "feature/x", "target": "main"}));
        assert_eq!(entries(a, &pre, "a12"), 1, "{backend}");
        assert_eq!(
            rollup(a, &pre, "a", json!({}))["totals"]["amount"],
            json!([money("9.00", "USD")]),
            "{backend}"
        );
        assert_eq!(
            rollup(a, &pre, "a", json!({"branch": "feature/x"}))["entries"],
            json!(3)
        );
    }
}

// ---- DSN-DVM-441, FR-LEDG-052: hierarchies of ledger keys ----

#[test]
fn a_top_level_roll_up_is_the_sum_of_its_leaves() {
    for (backend, mut a) in adapters("rollup") {
        let a = &mut a;
        let pre = start(a);
        four_levels(a, &pre);
        commit(
            a,
            json!([
                usd(20, &pre, "a11", "100.00", "g1"),
                usd(21, &pre, "a12", "-30.00", "g1"),
                usd(22, &pre, "a2", "12.50", "g1"),
                usd(23, &pre, "b1", "0.75", "g1"),
                usd(24, &pre, "other", "-83.25", "g1"),
                line(25, &pre, "a11", money("40.00", "EUR"), "g2", "2026-10-20"),
                line(
                    26,
                    &pre,
                    "other",
                    money("-40.00", "EUR"),
                    "g2",
                    "2026-10-20"
                ),
            ]),
        );
        // DSN-DVM-441 acceptance 1: the balance of the top of a 4-level tree is the sum of
        // its leaves
        let top = rollup(a, &pre, "top", json!({"detail": true}));
        assert_eq!(
            top["totals"]["amount"],
            json!([money("40.00", "EUR"), money("83.25", "USD")]),
            "{backend}"
        );
        assert_eq!(top["nodes"], json!(8));
        assert_eq!(top["leaves"], json!(4));
        assert_eq!(top["entries"], json!(5));
        let mut leaves: BTreeMap<String, f64> = BTreeMap::new();
        for leaf in ["a11", "a12", "a2", "b1"] {
            let t = ok(
                a,
                "ledger_totals",
                json!({"class": "Line", "ledger_key": {"node": format!("{pre}{leaf}")}}),
            );
            for m in t["totals"]["amount"].as_array().expect("totals") {
                *leaves
                    .entry(m["currency"].as_str().expect("currency").to_string())
                    .or_default() += m["amount"]
                    .as_str()
                    .expect("amount")
                    .parse::<f64>()
                    .unwrap();
            }
        }
        assert_eq!(
            leaves,
            BTreeMap::from([("EUR".to_string(), 40.0), ("USD".to_string(), 83.25)]),
            "{backend}"
        );
        // the levels between: each node is the sum of the nodes below it
        let members = top["members"].as_array().expect("members");
        let order: Vec<&str> = members
            .iter()
            .map(|m| {
                m["node"]
                    .as_str()
                    .expect("node")
                    .rsplit('/')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(order, ["top", "a", "a1", "a11", "a12", "a2", "b", "b1"]);
        let of = |slug: &str| {
            members
                .iter()
                .find(|m| m["node"] == json!(format!("{pre}{slug}")))
                .expect("member")
                .clone()
        };
        assert_eq!(
            of("a1")["totals"]["amount"],
            json!([money("40.00", "EUR"), money("70.00", "USD")])
        );
        assert_eq!(
            of("a")["totals"]["amount"],
            json!([money("40.00", "EUR"), money("82.50", "USD")])
        );
        assert_eq!(of("a")["own"]["amount"], json!([]));
        assert_eq!(of("a")["depth"], json!(1));
        assert_eq!(of("b1")["leaf"], json!(true));
        assert_eq!(
            rollup(a, &pre, "a1", json!({}))["totals"],
            of("a1")["totals"]
        );
        // the tree itself, with the pre-order interval of each node
        let h = ok(a, "ledger_hierarchy", json!({"node": format!("{pre}a")}));
        let rows: Vec<(String, u64, u64, bool)> = h["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .map(|n| {
                (
                    n["node"]
                        .as_str()
                        .unwrap()
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .to_string(),
                    n["lo"].as_u64().unwrap(),
                    n["hi"].as_u64().unwrap(),
                    n["posting"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("a".to_string(), 2, 6, false),
                ("a1".to_string(), 3, 5, false),
                ("a11".to_string(), 4, 4, true),
                ("a12".to_string(), 5, 5, true),
                ("a2".to_string(), 6, 6, true),
            ],
            "{backend}"
        );
        assert_eq!(
            ok(a, "ledger_hierarchy", json!({}))["nodes"]
                .as_array()
                .expect("nodes")
                .len(),
            9
        );
        assert_eq!(
            fails(
                a,
                "ledger_rollup",
                json!({"class": "Line", "node": format!("{pre}nowhere")})
            )
            .code,
            "MODEL.REFERENCE_INVALID"
        );
        assert_eq!(
            fails(
                a,
                "ledger_rollup",
                json!({"class": "Slip", "node": format!("{pre}top")})
            )
            .code,
            "MODEL.INVALID_VALUE"
        );
    }
}

#[test]
fn entries_are_posted_to_valid_leaves() {
    for (backend, mut a) in adapters("leaves") {
        let a = &mut a;
        let pre = start(a);
        let made = four_levels(a, &pre);
        commit(
            a,
            json!([
                node(10, &pre, "header", Some("top"), json!({"posting": false})),
                node(11, &pre, "eur", Some("top"), json!({"currencies": ["EUR"]})),
            ]),
        );
        let pair = |slug: &str, amount: Value, at: &str| {
            json!([
                line(20, &pre, slug, amount.clone(), "g1", at),
                line(
                    21,
                    &pre,
                    "b1",
                    {
                        let mut m = amount;
                        let v = m["amount"].as_str().unwrap().to_string();
                        m["amount"] = json!(format!("-{v}"));
                        m
                    },
                    "g1",
                    at
                ),
            ])
        };
        // a node with nodes below takes no entries
        let e = commit_fails(a, pair("a1", money("10.00", "USD"), "2026-10-15"));
        assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED", "{backend}");
        let d = e.details.expect("details");
        assert_eq!(d["node"], json!(format!("{pre}a1")));
        assert_eq!(d["entry_id"], json!(uuid(20)));
        // a leaf that declares `posting: false` neither
        assert_eq!(
            commit_fails(a, pair("header", money("10.00", "USD"), "2026-10-15")).code,
            "MODEL.CONSTRAINT_VIOLATED",
            "{backend}"
        );
        // a node accepts the currencies it declares
        let e = commit_fails(a, pair("eur", money("10.00", "USD"), "2026-10-15"));
        assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED", "{backend}");
        assert_eq!(e.details.expect("details")["currency"], json!("USD"));
        // a node that does not exist
        assert_eq!(
            commit_fails(a, pair("nowhere", money("10.00", "USD"), "2026-10-15")).code,
            "MODEL.REFERENCE_INVALID",
            "{backend}"
        );
        // before the node exists
        assert_eq!(
            commit_fails(a, pair("a11", money("10.00", "USD"), "2025-12-31")).code,
            "MODEL.REFERENCE_INVALID",
            "{backend}"
        );
        // FR-LEDG-052 acceptance 1: a node closed from 2026-10-01 takes no entry valid on
        // 2026-10-05, and still takes one valid before
        let base = made["versions"][uuid(6)].as_str().expect("version");
        commit(
            a,
            json!([{"op": "end", "object_id": uuid(6), "base": base,
                "valid": {"from": "2026-10-01"}}]),
        );
        assert_eq!(
            commit_fails(a, pair("a2", money("10.00", "USD"), "2026-10-05")).code,
            "MODEL.REFERENCE_INVALID",
            "{backend}"
        );
        commit(a, pair("a2", money("10.00", "USD"), "2026-09-30"));
        commit(
            a,
            json!([
                line(22, &pre, "eur", money("10.00", "EUR"), "g2", "2026-10-15"),
                line(23, &pre, "b1", money("-10.00", "EUR"), "g2", "2026-10-15"),
            ]),
        );
        // the closed node is in the tree of September and not in that of October
        assert_eq!(
            rollup(a, &pre, "a", json!({"asof": "2026-09-30"}))["totals"]["amount"],
            json!([money("10.00", "USD")]),
            "{backend}"
        );
        let october = rollup(a, &pre, "a", json!({"asof": "2026-10-31"}));
        assert_eq!(october["totals"]["amount"], json!([]), "{backend}");
        assert_eq!(october["nodes"], json!(4));
        // a correction of an entry keeps its key: the entry of the closed node is reversed
        commit(
            a,
            json!([
                {"op": "reverse", "entry_id": uuid(20), "new_entry_id": uuid(24)},
                {"op": "reverse", "entry_id": uuid(21), "new_entry_id": uuid(25)},
            ]),
        );
        assert_eq!(
            rollup(a, &pre, "a", json!({"asof": "2026-09-30"}))["totals"]["amount"],
            json!([money("0.00", "USD")]),
            "{backend}"
        );
    }
}

#[test]
fn a_hierarchy_has_no_cycles_and_known_parents() {
    for (backend, mut a) in adapters("cycles") {
        let a = &mut a;
        let pre = start(a);
        let made = four_levels(a, &pre);
        let version = |n: u32| {
            made["versions"][uuid(n)]
                .as_str()
                .expect("version")
                .to_string()
        };
        // the top below one of its own nodes
        let e = commit_fails(
            a,
            json!([{"op": "update", "object_id": uuid(1), "base": version(1),
                "body": {"parent": format!("{pre}a11")},
                "valid": {"kind": "supersede_from", "from": "2026-01-01"}}]),
        );
        assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED", "{backend}");
        assert_eq!(
            e.details.expect("details")["nodes"]
                .as_array()
                .expect("nodes")
                .len(),
            8
        );
        // a node below itself
        assert_eq!(
            commit_fails(
                a,
                json!([{"op": "update", "object_id": uuid(9), "base": version(9),
                    "body": {"parent": format!("{pre}other")},
                    "valid": {"kind": "supersede_from", "from": "2026-01-01"}}]),
            )
            .code,
            "MODEL.CONSTRAINT_VIOLATED",
            "{backend}"
        );
        // a node that still has nodes below does not end
        assert_eq!(
            commit_fails(
                a,
                json!([{"op": "end", "object_id": uuid(3), "base": version(3),
                    "valid": {"from": "2026-06-01"}}]),
            )
            .code,
            "MODEL.REFERENCE_INVALID",
            "{backend}"
        );
        // currencies of a node are listed currencies
        assert_eq!(
            commit_fails(
                a,
                json!([node(
                    12,
                    &pre,
                    "bad",
                    Some("top"),
                    json!({"currencies": ["XYZ"]})
                )])
            )
            .code,
            "MODEL.INVALID_VALUE",
            "{backend}"
        );
        // the tree is unchanged
        assert_eq!(
            ok(a, "ledger_hierarchy", json!({}))["nodes"]
                .as_array()
                .expect("nodes")
                .len(),
            9,
            "{backend}"
        );
    }
}

#[test]
fn a_roll_up_is_read_at_the_coordinates_of_the_read() {
    for (backend, mut a) in adapters("coordinates") {
        let a = &mut a;
        let pre = start(a);
        let made = four_levels(a, &pre);
        let c1 = commit(
            a,
            json!([
                line(20, &pre, "a11", money("100.00", "USD"), "g1", "2026-09-10"),
                line(21, &pre, "b1", money("-100.00", "USD"), "g1", "2026-09-10"),
            ]),
        )["cseq"]
            .as_u64()
            .expect("cseq");
        commit(
            a,
            json!([
                line(22, &pre, "a2", money("30.00", "USD"), "g2", "2026-10-10"),
                line(23, &pre, "b1", money("-30.00", "USD"), "g2", "2026-10-10"),
            ]),
        );
        // `a2` moves below `b` from 2026-10-20
        let moved = commit(
            a,
            json!([{"op": "update", "object_id": uuid(6),
                "base": made["versions"][uuid(6)], "body": {"parent": format!("{pre}b")},
                "valid": {"kind": "supersede_from", "from": "2026-10-20"}}]),
        )["cseq"]
            .as_u64()
            .expect("cseq");
        let amount = |r: &Value| r["totals"]["amount"].clone();
        // valid time: the entries up to the date over the tree of the date
        let sep = rollup(a, &pre, "a", json!({"asof": "2026-09-30"}));
        assert_eq!(amount(&sep), json!([money("100.00", "USD")]), "{backend}");
        let mid = rollup(a, &pre, "a", json!({"asof": "2026-10-15"}));
        assert_eq!(amount(&mid), json!([money("130.00", "USD")]), "{backend}");
        assert_eq!(mid["nodes"], json!(5));
        let now = rollup(a, &pre, "a", json!({}));
        assert_eq!(amount(&now), json!([money("100.00", "USD")]), "{backend}");
        assert_eq!(now["nodes"], json!(4));
        assert_eq!(now["asof"], json!("2026-11-02"));
        assert_eq!(
            amount(&rollup(a, &pre, "b", json!({}))),
            json!([money("-100.00", "USD")]),
            "{backend}"
        );
        // the top does not change when a node moves within it
        for asof in ["2026-10-15", "2026-11-02"] {
            assert_eq!(
                amount(&rollup(a, &pre, "top", json!({"asof": asof}))),
                json!([money("0.00", "USD")]),
                "{backend} {asof}"
            );
        }
        // transaction time: at the first commit of entries the second group is not known,
        // and before the move the tree of today still has `a2` below `a`
        let first = rollup(a, &pre, "a", json!({"time": c1}));
        assert_eq!(amount(&first), json!([money("100.00", "USD")]), "{backend}");
        assert_eq!(first["nodes"], json!(5));
        let before = rollup(a, &pre, "a", json!({"time": moved - 1}));
        assert_eq!(
            amount(&before),
            json!([money("130.00", "USD")]),
            "{backend}"
        );
        // the evidence names the commit read, and the read repeats at that commit
        let again = rollup(
            a,
            &pre,
            "a",
            json!({"commit": before["evaluated_at"]["main"]}),
        );
        assert_eq!(again["totals"], before["totals"], "{backend}");
        assert_eq!(again["hierarchy"], before["hierarchy"], "{backend}");
        assert_ne!(now["hierarchy"], before["hierarchy"]);
        // a later commit does not change a read at earlier coordinates
        commit(
            a,
            json!([
                line(24, &pre, "a11", money("1.00", "USD"), "g3", "2026-09-01"),
                line(25, &pre, "b1", money("-1.00", "USD"), "g3", "2026-09-01"),
            ]),
        );
        assert_eq!(
            amount(&rollup(
                a,
                &pre,
                "a",
                json!({"time": moved, "asof": "2026-09-30"})
            )),
            json!([money("100.00", "USD")]),
            "{backend}"
        );
        assert_eq!(
            amount(&rollup(a, &pre, "a", json!({"asof": "2026-09-30"}))),
            json!([money("101.00", "USD")]),
            "{backend}"
        );
    }
}

/// SplitMix64.
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

fn cents(v: &Value, currency: &str) -> i64 {
    v.as_array()
        .expect("totals")
        .iter()
        .find(|m| m["currency"] == currency)
        .map_or(0, |m| {
            let text = m["amount"].as_str().expect("amount").replace('.', "");
            text.parse::<i64>().expect("cents")
        })
}

#[test]
fn property_roll_ups_equal_the_sums_of_the_subtrees() {
    for (backend, mut a) in adapters("property") {
        let a = &mut a;
        let pre = start(a);
        let mut rng = Rng(441);
        // a random forest of 14 nodes
        let n = 14usize;
        let mut parent: Vec<Option<usize>> = Vec::new();
        let mut creates = Vec::new();
        for i in 0..n {
            let p = if i == 0 || rng.below(6) == 0 {
                None
            } else {
                Some(rng.below(i as u64) as usize)
            };
            parent.push(p);
            creates.push(node(
                100 + i as u32,
                &pre,
                &format!("n{i:02}"),
                p.map(|p| format!("n{p:02}")).as_deref(),
                json!({}),
            ));
        }
        commit(a, Value::Array(creates));
        let leaves: Vec<usize> = (0..n).filter(|i| !parent.contains(&Some(*i))).collect();
        // random groups over the leaves; the last entry of each currency closes the group
        let mut own = vec![[0i64; 2]; n];
        let mut id = 1000u32;
        for g in 0..6 {
            let mut changes = Vec::new();
            let mut net = [0i64; 2];
            let mut post = |leaf: usize, cur: usize, c: i64| {
                id += 1;
                let sign = if c < 0 { "-" } else { "" };
                let text = format!("{sign}{}.{:02}", c.abs() / 100, c.abs() % 100);
                line(
                    id,
                    &pre,
                    &format!("n{leaf:02}"),
                    money(&text, ["USD", "EUR"][cur]),
                    &format!("g{g}"),
                    "2026-10-15",
                )
            };
            for _ in 0..(2 + rng.below(5)) {
                let leaf = leaves[rng.below(leaves.len() as u64) as usize];
                let cur = rng.below(2) as usize;
                let c = rng.below(200_001) as i64 - 100_000;
                net[cur] += c;
                own[leaf][cur] += c;
                changes.push(post(leaf, cur, c));
            }
            // one cent short of balance fails, the closing entries balance
            let leaf = leaves[rng.below(leaves.len() as u64) as usize];
            let mut short = changes.clone();
            short.push(post(leaf, 0, -net[0] - 1));
            short.push(post(leaf, 1, -net[1]));
            for cur in 0..2 {
                own[leaf][cur] -= net[cur];
                changes.push(post(leaf, cur, -net[cur]));
            }
            assert_eq!(commit_fails(a, Value::Array(short)).code, "LEDG.UNBALANCED");
            commit(a, Value::Array(changes));
        }
        for i in 0..n {
            let mut want = [0i64; 2];
            for (j, sums) in own.iter().enumerate() {
                let mut at = Some(j);
                while let Some(k) = at {
                    if k == i {
                        break;
                    }
                    at = parent[k];
                }
                if at.is_some() {
                    want[0] += sums[0];
                    want[1] += sums[1];
                }
            }
            let r = rollup(a, &pre, &format!("n{i:02}"), json!({}));
            let got = &r["totals"]["amount"];
            assert_eq!(
                [cents(got, "USD"), cents(got, "EUR")],
                want,
                "{backend} n{i:02}"
            );
        }
        // every root nets to the entries posted below it, and all roots together to zero
        let mut all = [0i64; 2];
        for (i, _) in parent.iter().enumerate().filter(|(_, p)| p.is_none()) {
            let r = rollup(a, &pre, &format!("n{i:02}"), json!({}));
            all[0] += cents(&r["totals"]["amount"], "USD");
            all[1] += cents(&r["totals"]["amount"], "EUR");
        }
        assert_eq!(all, [0, 0], "{backend}");
    }
}
