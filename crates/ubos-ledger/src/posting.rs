//! Balanced posting groups and hierarchies of ledger keys (STD-KIND-024, DSN-DVM-440,
//! DSN-DVM-441; FR-LEDG-051, FR-LEDG-052).
//!
//! Two general mechanisms, without storage:
//!
//! - **Posting groups.** The entries of one group are appended together and their signed
//!   amounts net to zero per unit. A unit is a currency code, or the empty unit of amounts
//!   without a currency. `Groups` collects the amounts and reports every group that does
//!   not balance, with the difference per unit, as `LEDG.UNBALANCED`.
//! - **Hierarchies.** The values of a ledger key can be the nodes of a tree declared as
//!   data (`NodeDecl`: parent, whether entries may be posted to the node, the currencies
//!   it accepts). `Hierarchy` checks the tree (known parents, no cycles), numbers the
//!   nodes in pre-order so that the subtree of a node is one interval (as the class tree
//!   of DSN-DVM-107), tells whether a node takes entries (leaves only) and rolls sums up.
//!
//! What the nodes and the groups mean is declared by Buks; nothing here knows.

use crate::currency;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ubos_types::BpaError;
use ubos_types::decimal::Decimal;
use ubos_types::error::invalid;

/// The unit of amounts without a currency.
pub const NO_UNIT: &str = "";

fn violated(details: Value) -> BpaError {
    BpaError::with_details("MODEL.CONSTRAINT_VIOLATED", details)
}

/// Signed sums per unit (a currency code, or `NO_UNIT`). Amounts of different units are
/// never added to each other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sums(BTreeMap<String, Decimal>);

impl Sums {
    /// No sums.
    pub fn new() -> Sums {
        Sums::default()
    }

    /// Add a signed amount of a unit.
    pub fn add(&mut self, unit: &str, amount: &Decimal) {
        let t = match self.0.get(unit) {
            Some(t) => t.add(amount),
            None => amount.clone(),
        };
        self.0.insert(unit.to_string(), t);
    }

    /// Add the value of an amount field: a money value `{amount, currency}` of a listed
    /// currency, a decimal text (no unit), or nothing (`null`).
    pub fn add_value(&mut self, v: &Value) -> Result<(), BpaError> {
        match v {
            Value::Null => {}
            Value::String(d) => self.add(NO_UNIT, &Decimal::parse(d)?),
            m => {
                let (a, c) = currency::parse_money(m)?;
                self.add(&c, &a);
            }
        }
        Ok(())
    }

    /// Add other sums, unit by unit.
    pub fn merge(&mut self, other: &Sums) {
        for (u, a) in &other.0 {
            self.add(u, a);
        }
    }

    /// The sum of a unit.
    pub fn get(&self, unit: &str) -> Option<&Decimal> {
        self.0.get(unit)
    }

    /// The sums as `(unit, amount)`, ordered by unit.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Decimal)> {
        self.0.iter().map(|(u, a)| (u.as_str(), a))
    }

    /// True without any amount.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// True when the sum of every unit is zero.
    pub fn is_balanced(&self) -> bool {
        self.0.values().all(Decimal::is_zero)
    }

    /// The units whose sum is not zero, with the sum.
    pub fn differences(&self) -> Vec<(&str, &Decimal)> {
        self.iter().filter(|(_, a)| !a.is_zero()).collect()
    }

    /// The sums as a value: the decimal text of amounts without a currency, else the list
    /// of money values ordered by currency (empty without amounts). Amounts with and
    /// without a currency in one sum are `MODEL.CURRENCY_MISMATCH`.
    pub fn to_value(&self) -> Result<Value, BpaError> {
        match (self.0.get(NO_UNIT), self.0.len()) {
            (Some(p), 1) => Ok(json!(p.format(None))),
            (Some(_), _) => Err(crate::money::mismatch("a decimal amount", "a money amount")),
            (None, _) => Ok(Value::Array(
                self.iter()
                    .map(|(c, a)| currency::money(a, c))
                    .collect::<Result<_, _>>()?,
            )),
        }
    }
}

/// A posting group: `scope` is where group identifiers are unique (the kernel uses the
/// ledger class), `group` the identifier the entries share.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupId {
    /// Where the group identifier is unique.
    pub scope: String,
    /// The identifier the entries of the group share.
    pub group: String,
}

/// A group that does not balance, with the sum left per unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imbalance {
    /// The group.
    pub id: GroupId,
    /// `(unit, difference)` of every unit whose signed amounts do not net to zero.
    pub differences: Vec<(String, Decimal)>,
}

impl Imbalance {
    /// `{scope, posting_group, differences: [{currency, difference}]}`; an amount without
    /// a currency has `currency: null`.
    pub fn to_value(&self) -> Value {
        let differences: Vec<Value> = self
            .differences
            .iter()
            .map(|(u, d)| {
                let unit = if u == NO_UNIT { Value::Null } else { json!(u) };
                let scale = currency::minor_unit(u).map_or(d.scale(), |m| m.max(d.scale()));
                json!({"currency": unit, "difference": d.format(Some(scale))})
            })
            .collect();
        json!({"scope": self.id.scope, "posting_group": self.id.group,
            "differences": differences})
    }
}

/// The posting groups of one atomic commit (STD-KIND-024).
#[derive(Debug, Clone, Default)]
pub struct Groups(BTreeMap<GroupId, Sums>);

impl Groups {
    /// No groups.
    pub fn new() -> Groups {
        Groups::default()
    }

    /// The sums of a group, created empty on first use (so that a group without amounts
    /// is known and balances).
    pub fn group(&mut self, scope: &str, group: &str) -> &mut Sums {
        self.0
            .entry(GroupId {
                scope: scope.to_string(),
                group: group.to_string(),
            })
            .or_default()
    }

    /// The number of groups.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True without groups.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The groups that do not balance, ordered by scope and group.
    pub fn unbalanced(&self) -> Vec<Imbalance> {
        self.0
            .iter()
            .filter(|(_, s)| !s.is_balanced())
            .map(|(id, s)| Imbalance {
                id: id.clone(),
                differences: s
                    .differences()
                    .into_iter()
                    .map(|(u, d)| (u.to_string(), d.clone()))
                    .collect(),
            })
            .collect()
    }

    /// `LEDG.UNBALANCED` naming every group that does not balance and its difference per
    /// unit (DSN-DVM-440 acceptance 1), unless every group balances.
    pub fn check(&self) -> Result<(), BpaError> {
        let bad = self.unbalanced();
        let Some(first) = bad.first() else {
            return Ok(());
        };
        let groups: Vec<Value> = bad.iter().map(Imbalance::to_value).collect();
        let d = &groups[0]["differences"][0];
        let message = format!(
            "posting group {} does not balance: difference {} {}",
            first.id.group,
            d["difference"].as_str().unwrap_or_default(),
            d["currency"].as_str().unwrap_or_default()
        );
        let mut e = BpaError::with_details("LEDG.UNBALANCED", json!({"groups": groups}));
        e.message = message.trim_end().to_string();
        Err(e)
    }
}

/// The declaration of one node of a hierarchy of ledger keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDecl {
    /// The identifier of the node (the kernel uses the canonical URI of its object).
    pub id: String,
    /// The node above, `None` for a root.
    pub parent: Option<String>,
    /// `Some(false)`: no entries are posted to the node even while it has no nodes below.
    pub posting: Option<bool>,
    /// The currencies entries of the node may carry; empty: any.
    pub currencies: Vec<String>,
}

impl NodeDecl {
    /// The declaration in the body of a node object: `{parent?, posting?, currencies?}`.
    pub fn of(id: &str, body: &Value) -> Result<NodeDecl, BpaError> {
        let parent = match body.get("parent") {
            None | Some(Value::Null) => None,
            Some(Value::String(p)) => Some(p.clone()),
            Some(_) => return Err(invalid("parent is the reference of a node")),
        };
        let posting = match body.get("posting") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => return Err(invalid("posting is a boolean")),
        };
        let mut currencies = Vec::new();
        match body.get("currencies") {
            None | Some(Value::Null) => {}
            Some(Value::Array(list)) => {
                for c in list {
                    let code = c
                        .as_str()
                        .ok_or_else(|| invalid("currencies are ISO 4217 codes"))?;
                    currency::currency(code)?;
                    if !currencies.iter().any(|x| x == code) {
                        currencies.push(code.to_string());
                    }
                }
            }
            Some(_) => return Err(invalid("currencies are ISO 4217 codes")),
        }
        Ok(NodeDecl {
            id: id.to_string(),
            parent,
            posting,
            currencies,
        })
    }
}

#[derive(Debug, Clone)]
struct Node {
    decl: NodeDecl,
    /// The position of the last node of the subtree in pre-order.
    hi: usize,
    depth: usize,
    children: usize,
}

/// A checked hierarchy: a forest whose nodes are numbered in pre-order (children by
/// identifier), so that the subtree of the node at position `lo` is the interval
/// `lo..=hi`.
#[derive(Debug, Clone, Default)]
pub struct Hierarchy {
    /// The nodes in pre-order.
    nodes: Vec<Node>,
    index: BTreeMap<String, usize>,
}

impl Hierarchy {
    /// The hierarchy of a set of declarations. Two declarations of one identifier are
    /// `MODEL.INVALID_VALUE`, a parent that is not declared is `MODEL.REFERENCE_INVALID`,
    /// a node below itself is `MODEL.CONSTRAINT_VIOLATED`.
    pub fn build(decls: Vec<NodeDecl>) -> Result<Hierarchy, BpaError> {
        let mut by_id: BTreeMap<String, NodeDecl> = BTreeMap::new();
        for d in decls {
            if by_id.contains_key(&d.id) {
                return Err(invalid(format!("node {} is declared twice", d.id)));
            }
            by_id.insert(d.id.clone(), d);
        }
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut roots: Vec<&str> = Vec::new();
        for d in by_id.values() {
            match &d.parent {
                None => roots.push(&d.id),
                Some(p) if by_id.contains_key(p) => {
                    children.entry(p).or_default().push(&d.id);
                }
                Some(p) => {
                    return Err(BpaError::with_details(
                        "MODEL.REFERENCE_INVALID",
                        json!({"node": d.id, "parent": p,
                            "message": "the parent is not a node at the coordinates"}),
                    ));
                }
            }
        }
        // pre-order from the roots; a node no root reaches is on a cycle
        let mut nodes: Vec<Node> = Vec::with_capacity(by_id.len());
        let mut index = BTreeMap::new();
        // (node, depth); `None` closes the subtree opened at the position
        let mut stack: Vec<(Result<&str, usize>, usize)> =
            roots.iter().rev().map(|r| (Ok(*r), 0)).collect();
        while let Some((item, depth)) = stack.pop() {
            match item {
                Err(pos) => nodes[pos].hi = nodes.len() - 1,
                Ok(id) => {
                    let pos = nodes.len();
                    let below = children.get(id).map(Vec::as_slice).unwrap_or_default();
                    index.insert(id.to_string(), pos);
                    nodes.push(Node {
                        decl: by_id[id].clone(),
                        hi: pos,
                        depth,
                        children: below.len(),
                    });
                    stack.push((Err(pos), depth));
                    stack.extend(below.iter().rev().map(|c| (Ok(*c), depth + 1)));
                }
            }
        }
        if nodes.len() != by_id.len() {
            let on_cycle: Vec<&String> = by_id.keys().filter(|k| !index.contains_key(*k)).collect();
            return Err(violated(json!({"nodes": on_cycle,
                    "message": "a node of a hierarchy is not below itself"})));
        }
        Ok(Hierarchy { nodes, index })
    }

    fn pos(&self, id: &str) -> Result<usize, BpaError> {
        self.index.get(id).copied().ok_or_else(|| {
            BpaError::with_details(
                "MODEL.REFERENCE_INVALID",
                json!({"node": id, "message": "no such node at the coordinates"}),
            )
        })
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True without nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// True when the hierarchy has the node.
    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// The declaration of a node.
    pub fn node(&self, id: &str) -> Result<&NodeDecl, BpaError> {
        Ok(&self.nodes[self.pos(id)?].decl)
    }

    /// The pre-order interval `(lo, hi)` of a node: its own position and that of the last
    /// node below it.
    pub fn interval(&self, id: &str) -> Result<(usize, usize), BpaError> {
        let lo = self.pos(id)?;
        Ok((lo, self.nodes[lo].hi))
    }

    /// The depth of a node; a root has 0.
    pub fn depth(&self, id: &str) -> Result<usize, BpaError> {
        Ok(self.nodes[self.pos(id)?].depth)
    }

    /// True when no node is below the node.
    pub fn is_leaf(&self, id: &str) -> Result<bool, BpaError> {
        Ok(self.nodes[self.pos(id)?].children == 0)
    }

    /// True when `id` is `root` or below it.
    pub fn is_within(&self, id: &str, root: &str) -> Result<bool, BpaError> {
        let (lo, hi) = self.interval(root)?;
        let p = self.pos(id)?;
        Ok(lo <= p && p <= hi)
    }

    /// The node and the nodes below it, in pre-order.
    pub fn subtree(&self, id: &str) -> Result<Vec<&str>, BpaError> {
        let (lo, hi) = self.interval(id)?;
        Ok(self.nodes[lo..=hi]
            .iter()
            .map(|n| n.decl.id.as_str())
            .collect())
    }

    /// The leaves of the subtree of a node, in pre-order.
    pub fn leaves(&self, id: &str) -> Result<Vec<&str>, BpaError> {
        let (lo, hi) = self.interval(id)?;
        Ok(self.nodes[lo..=hi]
            .iter()
            .filter(|n| n.children == 0)
            .map(|n| n.decl.id.as_str())
            .collect())
    }

    /// The nodes above a node, nearest first.
    pub fn ancestors(&self, id: &str) -> Result<Vec<&str>, BpaError> {
        let mut out = Vec::new();
        let mut at = self.pos(id)?;
        while let Some(p) = &self.nodes[at].decl.parent {
            at = self.index[p];
            out.push(self.nodes[at].decl.id.as_str());
        }
        Ok(out)
    }

    /// Every node in pre-order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().map(|n| n.decl.id.as_str())
    }

    /// The check of an entry posted to a node with amounts of the given currencies
    /// (FR-LEDG-052, DSN-DVM-441): the node exists (`MODEL.REFERENCE_INVALID`), is a leaf
    /// that takes entries and accepts the currencies (`MODEL.CONSTRAINT_VIOLATED`).
    pub fn check_posting<'c>(
        &self,
        id: &str,
        currencies: impl IntoIterator<Item = &'c str>,
    ) -> Result<(), BpaError> {
        let n = &self.nodes[self.pos(id)?];
        if n.children > 0 {
            return Err(violated(json!({"node": id,
                "message": "entries are posted to the leaves of a hierarchy"})));
        }
        if n.decl.posting == Some(false) {
            return Err(violated(json!({"node": id,
                "message": "the node takes no entries"})));
        }
        if !n.decl.currencies.is_empty() {
            for c in currencies {
                if c != NO_UNIT && !n.decl.currencies.iter().any(|x| x == c) {
                    return Err(violated(json!({"node": id, "currency": c,
                        "allowed": n.decl.currencies,
                        "message": "the node does not accept the currency"})));
                }
            }
        }
        Ok(())
    }

    /// The roll-up of a node: the sums of the node and of every node below it, per unit.
    /// Nodes without sums count as nothing.
    pub fn roll_up(&self, id: &str, own: &BTreeMap<String, Sums>) -> Result<Sums, BpaError> {
        let (lo, hi) = self.interval(id)?;
        let mut out = Sums::new();
        for n in &self.nodes[lo..=hi] {
            if let Some(s) = own.get(&n.decl.id) {
                out.merge(s);
            }
        }
        Ok(out)
    }

    /// The roll-up of every node, in one pass from the last node in pre-order to the first.
    pub fn roll_up_all(&self, own: &BTreeMap<String, Sums>) -> BTreeMap<String, Sums> {
        let mut acc: Vec<Sums> = self
            .nodes
            .iter()
            .map(|n| own.get(&n.decl.id).cloned().unwrap_or_default())
            .collect();
        for pos in (0..self.nodes.len()).rev() {
            if let Some(p) = &self.nodes[pos].decl.parent {
                let below = acc[pos].clone();
                acc[self.index[p]].merge(&below);
            }
        }
        self.nodes
            .iter()
            .zip(acc)
            .map(|(n, s)| (n.decl.id.clone(), s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Rng;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).expect("decimal")
    }

    fn usd(a: &str) -> Value {
        json!({"amount": a, "currency": "USD"})
    }

    fn node(id: &str, parent: Option<&str>) -> NodeDecl {
        NodeDecl {
            id: id.to_string(),
            parent: parent.map(str::to_string),
            posting: None,
            currencies: Vec::new(),
        }
    }

    #[test]
    fn a_balanced_group_passes() {
        let mut g = Groups::new();
        g.group("L", "g1").add_value(&usd("100.00")).unwrap();
        g.group("L", "g1").add_value(&usd("-60.00")).unwrap();
        g.group("L", "g1").add_value(&usd("-40.00")).unwrap();
        // a group without amounts balances
        g.group("L", "g2").add_value(&Value::Null).unwrap();
        assert_eq!(g.len(), 2);
        assert!(g.unbalanced().is_empty());
        g.check().unwrap();
    }

    #[test]
    fn an_unbalanced_group_reports_the_difference() {
        // FR-LEDG-051 acceptance 1 (100 against 99) and DSN-DVM-440 acceptance 1 (off by
        // 0.01)
        let mut g = Groups::new();
        g.group("L", "g1").add_value(&usd("100.00")).unwrap();
        g.group("L", "g1").add_value(&usd("-99.00")).unwrap();
        g.group("L", "g2").add_value(&usd("50.00")).unwrap();
        g.group("L", "g2").add_value(&usd("-49.99")).unwrap();
        g.group("L", "g3").add_value(&usd("7.00")).unwrap();
        g.group("L", "g3").add_value(&usd("-7.00")).unwrap();
        let e = g.check().unwrap_err();
        assert_eq!(e.code, "LEDG.UNBALANCED");
        assert_eq!(
            e.details.expect("details")["groups"],
            json!([
                {"scope": "L", "posting_group": "g1",
                    "differences": [{"currency": "USD", "difference": "1.00"}]},
                {"scope": "L", "posting_group": "g2",
                    "differences": [{"currency": "USD", "difference": "0.01"}]}
            ])
        );
        assert!(e.message.contains("g1") && e.message.contains("1.00 USD"));
    }

    #[test]
    fn groups_balance_per_currency_and_per_scope() {
        // equal numbers of different currencies do not balance
        let mut g = Groups::new();
        g.group("L", "g").add_value(&usd("100.00")).unwrap();
        g.group("L", "g")
            .add_value(&json!({"amount": "-100.00", "currency": "EUR"}))
            .unwrap();
        let bad = g.unbalanced();
        assert_eq!(bad.len(), 1);
        assert_eq!(
            bad[0].differences,
            vec![
                ("EUR".to_string(), d("-100.00")),
                ("USD".to_string(), d("100.00"))
            ]
        );
        // each currency balancing on its own balances the group
        g.group("L", "g").add_value(&usd("-100.00")).unwrap();
        g.group("L", "g")
            .add_value(&json!({"amount": "100.00", "currency": "EUR"}))
            .unwrap();
        g.check().unwrap();
        // the same identifier in another scope is another group
        g.group("M", "g").add_value(&json!("3")).unwrap();
        let e = g.check().unwrap_err();
        assert_eq!(
            e.details.expect("details")["groups"],
            json!([{"scope": "M", "posting_group": "g",
                "differences": [{"currency": null, "difference": "3"}]}])
        );
        // amounts are money of a listed currency or decimal texts
        assert!(g.group("L", "x").add_value(&json!(3)).is_err());
        assert!(
            g.group("L", "x")
                .add_value(&json!({"amount": "1", "currency": "XYZ"}))
                .is_err()
        );
    }

    #[test]
    fn sums_are_values_per_unit() {
        let mut s = Sums::new();
        assert_eq!(s.to_value().unwrap(), json!([]));
        s.add_value(&usd("10.5")).unwrap();
        s.add_value(&json!({"amount": "3", "currency": "JPY"}))
            .unwrap();
        assert_eq!(
            s.to_value().unwrap(),
            json!([{"amount": "3", "currency": "JPY"}, {"amount": "10.50", "currency": "USD"}])
        );
        let mut plain = Sums::new();
        plain.add_value(&json!("2.5")).unwrap();
        plain.add_value(&json!("-0.5")).unwrap();
        assert_eq!(plain.to_value().unwrap(), json!("2"));
        plain.merge(&s);
        assert_eq!(
            plain.to_value().unwrap_err().code,
            "MODEL.CURRENCY_MISMATCH"
        );
    }

    #[test]
    fn property_a_group_balances_exactly_when_every_unit_nets_to_zero() {
        let units = ["USD", "EUR", "JPY", NO_UNIT];
        let mut rng = Rng(440);
        for round in 0..300 {
            // amounts in thousandths, per unit; the last line of each unit closes it
            let mut g = Groups::new();
            let mut net: BTreeMap<&str, i64> = BTreeMap::new();
            let lines = 1 + rng.below(12);
            for _ in 0..lines {
                let u = units[rng.below(4) as usize];
                let a = rng.below(2_000_001) as i64 - 1_000_000;
                *net.entry(u).or_default() += a;
                g.group("L", "g")
                    .add(u, &d(&a.to_string()).mul(&d("0.001")));
            }
            let balanced = net.values().all(|n| *n == 0);
            assert_eq!(g.check().is_ok(), balanced, "round {round}");
            // the reported differences are the nets
            let reported: BTreeMap<String, Decimal> = g
                .unbalanced()
                .into_iter()
                .flat_map(|i| i.differences)
                .collect();
            for (u, n) in &net {
                let want = d(&n.to_string()).mul(&d("0.001"));
                match reported.get(*u) {
                    Some(got) => assert_eq!(got, &want),
                    None => assert_eq!(*n, 0),
                }
            }
            // closing every unit balances the group
            for (u, n) in &net {
                g.group("L", "g")
                    .add(u, &d(&(-n).to_string()).mul(&d("0.001")));
            }
            g.check().unwrap();
            // an error of the smallest step in one unit is found
            g.group("L", "g").add("USD", &d("0.001"));
            assert_eq!(g.check().unwrap_err().code, "LEDG.UNBALANCED");
        }
    }

    /// A four-level tree: `top` over `a` (`a1` over `a11`, `a12`; `a2`) and `b` (`b1`).
    fn four_levels() -> Hierarchy {
        Hierarchy::build(vec![
            node("b1", Some("b")),
            node("a12", Some("a1")),
            node("top", None),
            node("a", Some("top")),
            node("a1", Some("a")),
            node("a11", Some("a1")),
            node("a2", Some("a")),
            node("b", Some("top")),
            node("other", None),
        ])
        .unwrap()
    }

    #[test]
    fn a_hierarchy_numbers_its_nodes_in_pre_order() {
        let h = four_levels();
        assert_eq!(
            h.ids().collect::<Vec<_>>(),
            vec!["other", "top", "a", "a1", "a11", "a12", "a2", "b", "b1"]
        );
        assert_eq!(h.interval("top").unwrap(), (1, 8));
        assert_eq!(h.interval("a1").unwrap(), (3, 5));
        assert_eq!(h.interval("a12").unwrap(), (5, 5));
        assert_eq!(h.subtree("a").unwrap(), vec!["a", "a1", "a11", "a12", "a2"]);
        assert_eq!(h.leaves("top").unwrap(), vec!["a11", "a12", "a2", "b1"]);
        assert_eq!(h.ancestors("a11").unwrap(), vec!["a1", "a", "top"]);
        assert_eq!(h.depth("a11").unwrap(), 3);
        assert!(h.is_within("a12", "a").unwrap());
        assert!(!h.is_within("b1", "a").unwrap());
        assert!(!h.is_within("other", "top").unwrap());
        assert!(h.is_leaf("other").unwrap() && !h.is_leaf("a1").unwrap());
        assert_eq!(h.len(), 9);
        assert_eq!(
            h.interval("nowhere").unwrap_err().code,
            "MODEL.REFERENCE_INVALID"
        );
    }

    #[test]
    fn a_hierarchy_has_known_parents_and_no_cycles() {
        let e = Hierarchy::build(vec![node("a", Some("missing"))]).unwrap_err();
        assert_eq!(e.code, "MODEL.REFERENCE_INVALID");
        let e = Hierarchy::build(vec![node("a", Some("a"))]).unwrap_err();
        assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED");
        let e = Hierarchy::build(vec![
            node("root", None),
            node("x", Some("z")),
            node("y", Some("x")),
            node("z", Some("y")),
        ])
        .unwrap_err();
        assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED");
        assert_eq!(e.details.expect("details")["nodes"], json!(["x", "y", "z"]));
        let e = Hierarchy::build(vec![node("a", None), node("a", None)]).unwrap_err();
        assert_eq!(e.code, "MODEL.INVALID_VALUE");
        assert!(Hierarchy::build(Vec::new()).unwrap().is_empty());
    }

    #[test]
    fn entries_are_posted_to_leaves_that_take_them() {
        let mut decls = vec![node("top", None), node("leaf", Some("top"))];
        decls.push(
            NodeDecl::of(
                "header",
                &json!({"parent": "top", "posting": false, "currencies": []}),
            )
            .unwrap(),
        );
        decls.push(
            NodeDecl::of(
                "eur",
                &json!({"parent": "top", "currencies": ["EUR", "CHF", "EUR"]}),
            )
            .unwrap(),
        );
        let h = Hierarchy::build(decls).unwrap();
        h.check_posting("leaf", ["USD", "JPY"]).unwrap();
        assert_eq!(
            h.check_posting("top", ["USD"]).unwrap_err().code,
            "MODEL.CONSTRAINT_VIOLATED"
        );
        assert_eq!(
            h.check_posting("header", ["USD"]).unwrap_err().code,
            "MODEL.CONSTRAINT_VIOLATED"
        );
        assert_eq!(
            h.check_posting("gone", ["USD"]).unwrap_err().code,
            "MODEL.REFERENCE_INVALID"
        );
        h.check_posting("eur", ["EUR", "CHF", NO_UNIT]).unwrap();
        let e = h.check_posting("eur", ["EUR", "USD"]).unwrap_err();
        assert_eq!(e.code, "MODEL.CONSTRAINT_VIOLATED");
        assert_eq!(e.details.expect("details")["currency"], json!("USD"));
        assert_eq!(h.node("eur").unwrap().currencies, vec!["EUR", "CHF"]);
        // malformed declarations
        assert!(NodeDecl::of("x", &json!({"parent": 3})).is_err());
        assert!(NodeDecl::of("x", &json!({"posting": "no"})).is_err());
        assert!(NodeDecl::of("x", &json!({"currencies": ["XYZ"]})).is_err());
    }

    #[test]
    fn a_top_level_roll_up_is_the_sum_of_its_leaves() {
        // DSN-DVM-441 acceptance 1
        let h = four_levels();
        let mut own: BTreeMap<String, Sums> = BTreeMap::new();
        for (leaf, usd_, eur) in [
            ("a11", "100.00", "5.00"),
            ("a12", "-30.00", "0"),
            ("a2", "12.50", "-1.25"),
            ("b1", "0.75", "0"),
            ("other", "999.00", "0"),
        ] {
            let s = own.entry(leaf.to_string()).or_default();
            s.add("USD", &d(usd_));
            s.add("EUR", &d(eur));
        }
        let top = h.roll_up("top", &own).unwrap();
        assert_eq!(top.get("USD"), Some(&d("83.25")));
        assert_eq!(top.get("EUR"), Some(&d("3.75")));
        let mut leaves = Sums::new();
        for l in h.leaves("top").unwrap() {
            leaves.merge(&own[l]);
        }
        assert_eq!(top, leaves);
        assert_eq!(h.roll_up("a1", &own).unwrap().get("USD"), Some(&d("70.00")));
        assert_eq!(h.roll_up("b", &own).unwrap().get("USD"), Some(&d("0.75")));
        assert_eq!(h.roll_up_all(&own)["top"], top);
    }

    #[test]
    fn property_roll_ups_are_the_sums_over_the_subtrees() {
        let units = ["USD", "EUR", NO_UNIT];
        let mut rng = Rng(441);
        for round in 0..150 {
            // a random forest: node i has a parent among the nodes before it, or none
            let n = 1 + rng.below(40) as usize;
            let mut decls = Vec::new();
            let mut parent_of: Vec<Option<usize>> = Vec::new();
            for i in 0..n {
                let parent = if i == 0 || rng.below(5) == 0 {
                    None
                } else {
                    Some(rng.below(i as u64) as usize)
                };
                parent_of.push(parent);
                decls.push(node(
                    &format!("n{i:02}"),
                    parent.map(|p| format!("n{p:02}")).as_deref(),
                ));
            }
            // declared in a shuffled order
            for i in (1..decls.len()).rev() {
                decls.swap(i, rng.below(i as u64 + 1) as usize);
            }
            let h = Hierarchy::build(decls).unwrap();
            assert_eq!(h.len(), n);
            let mut own: BTreeMap<String, Sums> = BTreeMap::new();
            let mut raw: Vec<Vec<(usize, i64)>> = vec![Vec::new(); n];
            for (i, lines) in raw.iter_mut().enumerate() {
                for _ in 0..rng.below(4) {
                    let u = rng.below(3) as usize;
                    let a = rng.below(200_001) as i64 - 100_000;
                    lines.push((u, a));
                    own.entry(format!("n{i:02}"))
                        .or_default()
                        .add(units[u], &d(&a.to_string()).mul(&d("0.01")));
                }
            }
            let all = h.roll_up_all(&own);
            for i in 0..n {
                let id = format!("n{i:02}");
                // the subtree by walking the parents, not by the interval
                let mut want = [0i64; 3];
                let mut members = 0;
                for (j, lines) in raw.iter().enumerate() {
                    let mut at = Some(j);
                    while let Some(k) = at {
                        if k == i {
                            break;
                        }
                        at = parent_of[k];
                    }
                    if at.is_some() {
                        members += 1;
                        for (u, a) in lines {
                            want[*u] += a;
                        }
                    }
                }
                let got = h.roll_up(&id, &own).unwrap();
                assert_eq!(got, all[&id], "round {round} {id}");
                assert_eq!(h.subtree(&id).unwrap().len(), members);
                let (lo, hi) = h.interval(&id).unwrap();
                assert_eq!(hi - lo + 1, members);
                for (u, w) in want.iter().enumerate() {
                    let sum = got.get(units[u]).cloned().unwrap_or(d("0"));
                    assert_eq!(sum, d(&w.to_string()).mul(&d("0.01")), "round {round} {id}");
                }
                // every node of the subtree is within it, and its ancestors lead to the node
                for m in h.subtree(&id).unwrap() {
                    assert!(h.is_within(m, &id).unwrap());
                    assert!(m == id || h.ancestors(m).unwrap().contains(&id.as_str()));
                }
            }
        }
    }
}
