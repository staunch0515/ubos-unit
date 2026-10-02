//! Recursive-descent parser of L1 expressions (STD-EXPR-001).

use crate::ast::{COLL_FN, Node};
use crate::lexer::{Kind, Token, lex, terr};
use crate::value::{Ty, Val};
use serde_json::Value;
use ubos_model::types::BpaError;
use ubos_model::types::decimal::Decimal;
use ubos_model::types::time::{parse_date, parse_instant};
use ubos_model::types::values::minor_unit;

struct Parser {
    toks: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self, k: usize) -> &Token {
        &self.toks[(self.i + k).min(self.toks.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let t = self.toks[self.i.min(self.toks.len() - 1)].clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }

    fn at(&self, kind: Kind, text: Option<&str>) -> bool {
        let t = self.peek(0);
        t.kind == kind && text.is_none_or(|x| t.text == x)
    }

    fn at_k(&self, k: usize, kind: Kind, text: Option<&str>) -> bool {
        let t = self.peek(k);
        t.kind == kind && text.is_none_or(|x| t.text == x)
    }

    fn expect(&mut self, kind: Kind, text: Option<&str>) -> Result<Token, BpaError> {
        if !self.at(kind.clone(), text) {
            let what = text
                .map(str::to_string)
                .unwrap_or_else(|| format!("{kind:?}").to_lowercase());
            return Err(terr(self.peek(0).pos, &format!("expected {what}")));
        }
        Ok(self.next())
    }

    fn parse(&mut self) -> Result<Node, BpaError> {
        let e = self.expr()?;
        if !self.at(Kind::End, None) {
            return Err(terr(self.peek(0).pos, "unexpected token"));
        }
        Ok(e)
    }

    fn expr(&mut self) -> Result<Node, BpaError> {
        let c = self.or_()?;
        if self.at(Kind::Op, Some("?")) {
            let pos = self.next().pos;
            let a = self.expr()?;
            self.expect(Kind::Op, Some(":"))?;
            let b = self.expr()?;
            return Ok(Node::Cond(pos, Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn or_(&mut self) -> Result<Node, BpaError> {
        let mut e = self.and_()?;
        while self.at(Kind::Kw, Some("or")) {
            let pos = self.next().pos;
            let r = self.and_()?;
            e = Node::Or(pos, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn and_(&mut self) -> Result<Node, BpaError> {
        let mut e = self.not_()?;
        while self.at(Kind::Kw, Some("and")) {
            let pos = self.next().pos;
            let r = self.not_()?;
            e = Node::And(pos, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn not_(&mut self) -> Result<Node, BpaError> {
        if self.at(Kind::Kw, Some("not")) {
            let pos = self.next().pos;
            let e = self.cmp()?;
            return Ok(Node::Not(pos, Box::new(e)));
        }
        self.cmp()
    }

    fn cmp(&mut self) -> Result<Node, BpaError> {
        let e = self.add()?;
        let t = self.peek(0).clone();
        if t.kind == Kind::Op {
            let op: Option<&'static str> = match t.text.as_str() {
                "==" => Some("=="),
                "!=" => Some("!="),
                "<" => Some("<"),
                "<=" => Some("<="),
                ">" => Some(">"),
                ">=" => Some(">="),
                _ => None,
            };
            if let Some(op) = op {
                self.next();
                let r = self.add()?;
                return Ok(Node::Cmp(t.pos, op, Box::new(e), Box::new(r)));
            }
        }
        if t.kind == Kind::Kw && t.text == "in" {
            self.next();
            if self.at(Kind::Op, Some("[")) {
                let p = self.primary()?;
                return Ok(Node::In(t.pos, Box::new(e), Box::new(p)));
            }
            let p = self.postfix()?;
            if !matches!(p, Node::Path(..) | Node::Member(..)) {
                return Err(terr(p.pos(), "in takes a list literal or a path"));
            }
            return Ok(Node::In(t.pos, Box::new(e), Box::new(p)));
        }
        if t.kind == Kind::Kw && t.text == "is" {
            self.next();
            let neg = if self.at(Kind::Kw, Some("not")) {
                self.next();
                true
            } else {
                false
            };
            self.expect(Kind::Kw, Some("null"))?;
            return Ok(Node::IsNull(t.pos, Box::new(e), neg));
        }
        if t.kind == Kind::Kw && t.text == "matches" {
            self.next();
            let s = self.expect(Kind::Str, None)?;
            if regex::Regex::new(&format!("^(?:{})$", s.text)).is_err() {
                return Err(terr(s.pos, "invalid pattern"));
            }
            return Ok(Node::Matches(t.pos, Box::new(e), s.text));
        }
        Ok(e)
    }

    fn add(&mut self) -> Result<Node, BpaError> {
        let mut e = self.mul()?;
        while self.at(Kind::Op, Some("+")) || self.at(Kind::Op, Some("-")) {
            let t = self.next();
            let r = self.mul()?;
            e = Node::Arith(
                t.pos,
                t.text.chars().next().unwrap_or('+'),
                Box::new(e),
                Box::new(r),
            );
        }
        Ok(e)
    }

    fn mul(&mut self) -> Result<Node, BpaError> {
        let mut e = self.unary()?;
        while self.at(Kind::Op, Some("*"))
            || self.at(Kind::Op, Some("/"))
            || self.at(Kind::Op, Some("%"))
        {
            let t = self.next();
            let r = self.unary()?;
            e = Node::Arith(
                t.pos,
                t.text.chars().next().unwrap_or('*'),
                Box::new(e),
                Box::new(r),
            );
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Node, BpaError> {
        if self.at(Kind::Op, Some("-")) {
            let pos = self.next().pos;
            let e = self.postfix()?;
            return Ok(Node::Neg(pos, Box::new(e)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Node, BpaError> {
        let mut e = self.primary()?;
        loop {
            if self.at(Kind::Op, Some(".")) || self.at(Kind::Op, Some("?.")) {
                let t = self.next();
                let m = self.peek(0).clone();
                if !matches!(m.kind, Kind::Ident | Kind::Kw) {
                    return Err(terr(m.pos, "expected a member name"));
                }
                self.next();
                e = Node::Member(t.pos, Box::new(e), m.text, t.text == "?.");
            } else if self.at(Kind::Op, Some("[")) {
                let pos = self.next().pos;
                let idx = self.expr()?;
                self.expect(Kind::Op, Some("]"))?;
                e = Node::Index(pos, Box::new(e), Box::new(idx));
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<Node, BpaError> {
        let t = self.next();
        let pos = t.pos;
        match t.kind {
            Kind::Num => {
                if t.text.contains('.') {
                    Ok(Node::Lit(
                        pos,
                        Val::Dec(Decimal::parse(&t.text)?),
                        Ty::Decimal,
                    ))
                } else {
                    let i: i128 = t.text.parse().map_err(|_| terr(pos, "number too large"))?;
                    Ok(Node::Lit(pos, Val::Int(i), Ty::Integer))
                }
            }
            Kind::Money => {
                let (amount, cur) = t.text.split_at(t.text.len() - 3);
                if minor_unit(cur).is_none() {
                    return Err(terr(pos, "unknown currency"));
                }
                Ok(Node::Lit(
                    pos,
                    Val::Money(Decimal::parse(amount)?, cur.to_string()),
                    Ty::Money,
                ))
            }
            Kind::Str => Ok(Node::Lit(pos, Val::Text(t.text), Ty::Text)),
            Kind::Date => match parse_date(&t.text) {
                Ok(d) => Ok(Node::Lit(pos, Val::Date(d), Ty::Date)),
                Err(_) => Err(terr(pos, "invalid date literal")),
            },
            Kind::Inst => match parse_instant(&t.text) {
                Ok(d) => Ok(Node::Lit(pos, Val::Instant(d), Ty::Instant)),
                Err(_) => Err(terr(pos, "invalid instant literal")),
            },
            Kind::Kw if t.text == "true" || t.text == "false" => {
                Ok(Node::Lit(pos, Val::Bool(t.text == "true"), Ty::Boolean))
            }
            Kind::Kw if t.text == "null" => Ok(Node::Lit(pos, Val::Null, Ty::Null)),
            Kind::Op if t.text == "(" => {
                let e = self.expr()?;
                self.expect(Kind::Op, Some(")"))?;
                Ok(e)
            }
            Kind::Op if t.text == "[" => {
                let mut items = Vec::new();
                if !self.at(Kind::Op, Some("]")) {
                    items.push(self.expr()?);
                    while self.at(Kind::Op, Some(",")) {
                        self.next();
                        items.push(self.expr()?);
                    }
                }
                self.expect(Kind::Op, Some("]"))?;
                Ok(Node::List(pos, items))
            }
            Kind::Ident => {
                if self.at(Kind::Op, Some("(")) {
                    self.next();
                    let name = t.text;
                    let mut args = Vec::new();
                    if COLL_FN.contains(&name.as_str()) && !self.at(Kind::Op, Some(")")) {
                        let first = self.expr()?;
                        if self.at(Kind::Op, Some(","))
                            && self.at_k(1, Kind::Ident, None)
                            && self.at_k(2, Kind::Op, Some("->"))
                        {
                            self.next();
                            let var = self.next().text;
                            self.next();
                            let body = self.expr()?;
                            self.expect(Kind::Op, Some(")"))?;
                            return Ok(Node::Lambda(
                                pos,
                                name,
                                Box::new(first),
                                var,
                                Box::new(body),
                            ));
                        }
                        args.push(first);
                    } else if !self.at(Kind::Op, Some(")")) {
                        args.push(self.expr()?);
                    }
                    while self.at(Kind::Op, Some(",")) {
                        self.next();
                        args.push(self.expr()?);
                    }
                    self.expect(Kind::Op, Some(")"))?;
                    return Ok(Node::Call(pos, name, args));
                }
                Ok(Node::Path(pos, t.text))
            }
            _ => Err(terr(pos, "unexpected token")),
        }
    }
}

/// Parse an expression given as JSON text.
pub fn parse(src: &Value) -> Result<Node, BpaError> {
    let Some(text) = src.as_str() else {
        return Err(terr(0, "an expression is text"));
    };
    let toks = lex(text)?;
    Parser { toks, i: 0 }.parse()
}
