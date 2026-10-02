//! Syntax tree of L1 expressions (STD-EXPR-001). Positions are zero-based code-point
//! offsets of the offending token (STD-EXPR-002).

use crate::value::{Ty, Val};

/// A node of the syntax tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A literal with its static type.
    Lit(usize, Val, Ty),
    /// A list literal.
    List(usize, Vec<Node>),
    /// A root or variable name.
    Path(usize, String),
    /// Member access; `safe` for `?.`.
    Member(usize, Box<Node>, String, bool),
    /// Indexing.
    Index(usize, Box<Node>, Box<Node>),
    /// Unary minus.
    Neg(usize, Box<Node>),
    /// Arithmetic (`+`, `-`, `*`, `/`, `%`).
    Arith(usize, char, Box<Node>, Box<Node>),
    /// Comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`).
    Cmp(usize, &'static str, Box<Node>, Box<Node>),
    /// Membership.
    In(usize, Box<Node>, Box<Node>),
    /// `is null` / `is not null`.
    IsNull(usize, Box<Node>, bool),
    /// `matches` with a compiled pattern.
    Matches(usize, Box<Node>, String),
    /// Conjunction.
    And(usize, Box<Node>, Box<Node>),
    /// Disjunction.
    Or(usize, Box<Node>, Box<Node>),
    /// Negation.
    Not(usize, Box<Node>),
    /// Conditional.
    Cond(usize, Box<Node>, Box<Node>, Box<Node>),
    /// Function call.
    Call(usize, String, Vec<Node>),
    /// Collection function with a lambda: name, list, variable, body.
    Lambda(usize, String, Box<Node>, String, Box<Node>),
}

impl Node {
    /// Position of the node.
    pub fn pos(&self) -> usize {
        match self {
            Node::Lit(p, ..)
            | Node::List(p, ..)
            | Node::Path(p, ..)
            | Node::Member(p, ..)
            | Node::Index(p, ..)
            | Node::Neg(p, ..)
            | Node::Arith(p, ..)
            | Node::Cmp(p, ..)
            | Node::In(p, ..)
            | Node::IsNull(p, ..)
            | Node::Matches(p, ..)
            | Node::And(p, ..)
            | Node::Or(p, ..)
            | Node::Not(p, ..)
            | Node::Cond(p, ..)
            | Node::Call(p, ..)
            | Node::Lambda(p, ..) => *p,
        }
    }
}

/// Roots of paths.
pub const ROOTS: &[&str] = &["this", "ctx", "params", "old"];
/// Collection functions that take a lambda.
pub const COLL_FN: &[&str] = &[
    "any", "all", "count", "sum", "min", "max", "filter", "map", "first", "sort_by",
];
