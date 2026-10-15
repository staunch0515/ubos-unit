//! The textual grammar of governance sheets (STD-LGS-001): parser and canonical form.

use crate::ast::{Block, Declaration, Qualifier, Selector, Sheet, SheetValue};
use serde_json::{Value, json};
use ubos_expr::{Node, Val};
use ubos_model::types::BpaError;

/// Context keys of a `:ctx(…)` qualifier (STD-LGS-001).
pub const CTX_KEYS: &[&str] = &["branch_kind", "channel", "role", "principal_kind", "vae"];

fn parse_error(position: usize, message: impl Into<String>) -> BpaError {
    BpaError::with_details(
        "RULE.PARSE_ERROR",
        json!({"position": position, "message": message.into()}),
    )
}

/// A scanner over the characters of a sheet.
pub(crate) struct Scanner {
    s: Vec<char>,
    i: usize,
}

fn is_member_start(c: char) -> bool {
    c.is_ascii_lowercase()
}

fn is_member(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

impl Scanner {
    pub(crate) fn new(source: &Value) -> Result<Scanner, BpaError> {
        let Some(text) = source.as_str() else {
            return Err(parse_error(0, "a sheet is text"));
        };
        Ok(Scanner {
            s: text.chars().collect(),
            i: 0,
        })
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, BpaError> {
        Err(parse_error(self.i, message))
    }

    /// Skip whitespace and `//` comments.
    fn ws(&mut self) {
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                self.i += 1;
            } else if c == '/' && self.s.get(self.i + 1) == Some(&'/') {
                while self.i < self.s.len() && self.s[self.i] != '\n' {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }

    fn starts_with(&self, text: &str) -> bool {
        let mut j = self.i;
        for c in text.chars() {
            if self.s.get(j) != Some(&c) {
                return false;
            }
            j += 1;
        }
        true
    }

    fn peek(&mut self, text: &str) -> bool {
        self.ws();
        self.starts_with(text)
    }

    fn eat(&mut self, text: &str) -> Result<(), BpaError> {
        if !self.peek(text) {
            return self.err(format!("expected {text:?}"));
        }
        self.i += text.chars().count();
        Ok(())
    }

    /// A word `first` then up to `max_rest` characters of `rest` (a bounded name).
    fn word(
        &mut self,
        first: fn(char) -> bool,
        rest: fn(char) -> bool,
        max_rest: usize,
        what: &str,
    ) -> Result<String, BpaError> {
        self.ws();
        if !self.s.get(self.i).is_some_and(|c| first(*c)) {
            return self.err(format!("expected {what}"));
        }
        let start = self.i;
        self.i += 1;
        let mut n = 0;
        while n < max_rest && self.s.get(self.i).is_some_and(|c| rest(*c)) {
            self.i += 1;
            n += 1;
        }
        Ok(self.s[start..self.i].iter().collect())
    }

    fn member_name(&mut self, what: &str) -> Result<String, BpaError> {
        self.word(is_member_start, is_member, 63, what)
    }

    /// The text between matching brackets, string literals respected.
    fn bracketed(&mut self, open: char, close: char) -> Result<String, BpaError> {
        self.eat(&open.to_string())?;
        let start = self.i;
        let mut level = 1;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == '"' {
                self.i += 1;
                while self.i < self.s.len() && self.s[self.i] != '"' {
                    self.i += if self.s[self.i] == '\\' { 2 } else { 1 };
                }
                if self.i >= self.s.len() {
                    return self.err("unterminated string");
                }
            } else if c == open {
                level += 1;
            } else if c == close {
                level -= 1;
                if level == 0 {
                    let text: String = self.s[start..self.i].iter().collect();
                    self.i += 1;
                    return Ok(text.trim().to_string());
                }
            }
            self.i += 1;
        }
        self.err(format!("expected {close:?}"))
    }

    pub(crate) fn at_end(&mut self) -> bool {
        self.ws();
        self.i >= self.s.len()
    }

    /// An L1 expression kept as text: it must parse (STD-EXPR-001); whitespace is collapsed.
    fn expr_text(&self, text: &str, position: usize) -> Result<String, BpaError> {
        if text.is_empty() {
            return Err(parse_error(position, "empty expression"));
        }
        if let Err(e) = ubos_expr::parse(&Value::String(text.to_string())) {
            let message = e
                .details
                .as_ref()
                .and_then(|d| d.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or(e.message);
            return Err(parse_error(position, format!("expression: {message}")));
        }
        Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    /// A constant: an L1 literal, a list literal or a map literal.
    fn constant(&mut self) -> Result<SheetValue, BpaError> {
        self.ws();
        if self.peek("{") {
            self.eat("{")?;
            let mut out: Vec<(String, SheetValue)> = Vec::new();
            if self.peek("}") {
                self.eat("}")?;
                return Ok(SheetValue::Map(out));
            }
            loop {
                let k = self.member_name("member name")?;
                if out.iter().any(|(x, _)| *x == k) {
                    return self.err(format!("duplicate member {k}"));
                }
                self.eat(":")?;
                let v = self.constant()?;
                out.push((k, v));
                if self.peek(",") {
                    self.eat(",")?;
                    continue;
                }
                self.eat("}")?;
                return Ok(SheetValue::Map(out));
            }
        }
        if self.peek("[") {
            self.eat("[")?;
            let mut out = Vec::new();
            if self.peek("]") {
                self.eat("]")?;
                return Ok(SheetValue::List(out));
            }
            loop {
                out.push(self.constant()?);
                if self.peek(",") {
                    self.eat(",")?;
                    continue;
                }
                self.eat("]")?;
                return Ok(SheetValue::List(out));
            }
        }
        if self.peek("\"") {
            let start = self.i;
            self.i += 1;
            while self.i < self.s.len() && self.s[self.i] != '"' {
                self.i += if self.s[self.i] == '\\' { 2 } else { 1 };
            }
            if self.i >= self.s.len() {
                return self.err("unterminated string");
            }
            self.i += 1;
            let lit: String = self.s[start..self.i].iter().collect();
            return match ubos_expr::parse(&Value::String(lit)) {
                Ok(Node::Lit(_, Val::Text(t), _)) => Ok(SheetValue::Text(t)),
                Ok(_) => Err(parse_error(start, "expected a text literal")),
                Err(e) => Err(parse_error(start, format!("string: {}", e.message))),
            };
        }
        self.number_like()
    }

    /// A date, a number (with an optional currency code), `true`, `false` or `null`.
    fn number_like(&mut self) -> Result<SheetValue, BpaError> {
        for (kw, v) in [
            ("true", SheetValue::Bool(true)),
            ("false", SheetValue::Bool(false)),
            ("null", SheetValue::Null),
        ] {
            if self.starts_with(kw) {
                self.i += kw.len();
                return Ok(v);
            }
        }
        let rest: String = self.s[self.i..].iter().take(16).collect();
        let b = rest.as_bytes();
        // date literal `dddd-dd-dd`
        if b.len() >= 10
            && b[..10].iter().enumerate().all(|(k, c)| {
                if k == 4 || k == 7 {
                    *c == b'-'
                } else {
                    c.is_ascii_digit()
                }
            })
        {
            self.i += 10;
            return Ok(SheetValue::Text(rest[..10].to_string()));
        }
        let start = self.i;
        let mut j = self.i;
        if self.s.get(j) == Some(&'-') {
            j += 1;
        }
        let digits = |s: &[char], mut k: usize| {
            let from = k;
            while s.get(k).is_some_and(char::is_ascii_digit) {
                k += 1;
            }
            k - from
        };
        if digits(&self.s, j) == 0 {
            return self.err("expected a value");
        }
        j += digits(&self.s, j);
        let mut decimal = false;
        if self.s.get(j) == Some(&'.') && digits(&self.s, j + 1) > 0 {
            decimal = true;
            j += 1 + digits(&self.s, j + 1);
        }
        let num: String = self.s[start..j].iter().collect();
        let mut currency = String::new();
        while currency.len() < 3 && self.s.get(j).is_some_and(char::is_ascii_uppercase) {
            currency.push(self.s[j]);
            j += 1;
        }
        self.i = j;
        if currency.len() == 3 {
            return Ok(SheetValue::Map(vec![
                ("amount".into(), SheetValue::Text(num)),
                ("currency".into(), SheetValue::Text(currency)),
            ]));
        }
        if !currency.is_empty() {
            self.i = start + num.chars().count();
        }
        if decimal {
            return Ok(SheetValue::Text(num));
        }
        match num.parse::<i64>() {
            Ok(i) => Ok(SheetValue::Int(i)),
            Err(_) => Err(parse_error(start, "integer out of range")),
        }
    }

    /// One selector.
    pub(crate) fn selector(&mut self) -> Result<Selector, BpaError> {
        self.ws();
        let type_name = if self.peek("*") {
            self.eat("*")?;
            "*".to_string()
        } else {
            self.word(
                |c| c.is_ascii_uppercase(),
                |c| c.is_ascii_alphanumeric(),
                63,
                "class name or *",
            )?
        };
        let mut qualifiers = Vec::new();
        loop {
            if self.peek("[") {
                let where_ = self.i;
                let text = self.bracketed('[', ']')?;
                qualifiers.push(Qualifier::Attr(self.expr_text(&text, where_)?));
            } else if self.peek(":tag(") {
                self.eat(":tag(")?;
                let tag = self.slug()?;
                self.eat(")")?;
                qualifiers.push(Qualifier::Tag(tag));
            } else if self.peek(":ctx(") {
                self.eat(":ctx(")?;
                let key = self.word(
                    |c| c.is_ascii_lowercase() || c == '_',
                    |c| c.is_ascii_lowercase() || c == '_',
                    usize::MAX,
                    "context key",
                )?;
                if !CTX_KEYS.contains(&key.as_str()) {
                    return self.err(format!("unknown context key {key}"));
                }
                self.ws();
                let op = if self.starts_with("!=") {
                    "!="
                } else if self.starts_with("=") {
                    "="
                } else if self.starts_with("in") {
                    "in"
                } else {
                    return self.err("expected comparison");
                };
                self.i += op.len();
                let value = self.constant()?;
                if op == "in" {
                    let ok = matches!(&value, SheetValue::List(items)
                        if items.iter().all(|v| matches!(v, SheetValue::Text(_))));
                    if !ok {
                        return self.err("in takes a list of texts");
                    }
                } else if !matches!(value, SheetValue::Text(_)) {
                    return self.err("a context condition compares a text");
                }
                self.eat(")")?;
                qualifiers.push(Qualifier::Ctx {
                    key,
                    op: op.to_string(),
                    value,
                });
            } else if self.peek(":rel(") {
                self.eat(":rel(")?;
                let member = self.member_name("member name")?;
                self.eat(")")?;
                let where_ = self.i;
                let text = self.bracketed('[', ']')?;
                qualifiers.push(Qualifier::Rel {
                    member,
                    expr: self.expr_text(&text, where_)?,
                });
            } else if self.peek(":") {
                self.eat(":")?;
                qualifiers.push(Qualifier::State(self.member_name("state name")?));
            } else {
                break;
            }
        }
        Ok(Selector {
            type_name,
            qualifiers,
        })
    }

    /// A slug `[a-z0-9]+(-[a-z0-9]+)*`.
    fn slug(&mut self) -> Result<String, BpaError> {
        self.ws();
        let start = self.i;
        let alnum = |c: &char| c.is_ascii_lowercase() || c.is_ascii_digit();
        loop {
            if !self.s.get(self.i).is_some_and(alnum) {
                return self.err("expected slug");
            }
            while self.s.get(self.i).is_some_and(alnum) {
                self.i += 1;
            }
            if self.s.get(self.i) == Some(&'-') && self.s.get(self.i + 1).is_some_and(alnum) {
                self.i += 1;
                continue;
            }
            return Ok(self.s[start..self.i].iter().collect());
        }
    }

    /// A property path `member(.member)*`.
    fn property(&mut self) -> Result<String, BpaError> {
        let mut out = self.member_name("property")?;
        while self.s.get(self.i) == Some(&'.')
            && self.s.get(self.i + 1).is_some_and(|c| is_member_start(*c))
        {
            self.i += 1;
            out.push('.');
            out.push_str(&self.member_name("property")?);
        }
        Ok(out)
    }

    /// The whole sheet.
    fn sheet(&mut self) -> Result<Sheet, BpaError> {
        let mut blocks = Vec::new();
        while !self.at_end() {
            let mut selectors = vec![self.selector()?];
            while self.peek(",") {
                self.eat(",")?;
                selectors.push(self.selector()?);
            }
            self.eat("{")?;
            let mut declarations = Vec::new();
            while !self.peek("}") {
                let property = self.property()?;
                self.eat(":")?;
                let value = self.constant()?;
                let locked = if self.peek("!locked") {
                    self.eat("!locked")?;
                    true
                } else {
                    false
                };
                self.eat(";")?;
                declarations.push(Declaration {
                    property,
                    value,
                    locked,
                });
            }
            self.eat("}")?;
            if declarations.is_empty() {
                return self.err("a block declares at least one property");
            }
            blocks.push(Block {
                selectors,
                declarations,
            });
        }
        Ok(Sheet { blocks })
    }
}

/// STD-LGS-001: the AST of a sheet given as text; `RULE.PARSE_ERROR` with the position.
pub fn parse(source: &Value) -> Result<Sheet, BpaError> {
    Scanner::new(source)?.sheet()
}

/// One selector given as text (the operation `specificity`).
pub fn parse_selector(text: &Value) -> Result<Selector, BpaError> {
    let mut sc = Scanner::new(text)?;
    let sel = sc.selector()?;
    if !sc.at_end() {
        return sc.err("trailing text");
    }
    Ok(sel)
}

/// `{ast, source}`: the AST and the canonical text, which must round-trip (STD-LGS-002).
pub fn parse_sheet(source: &Value) -> Result<Value, BpaError> {
    let sheet = parse(source)?;
    let text = sheet.print();
    let again = parse(&Value::String(text.clone()))?;
    if ubos_model::types::canonical(&again.to_json())?
        != ubos_model::types::canonical(&sheet.to_json())?
    {
        return Err(parse_error(0, "the sheet does not round-trip"));
    }
    Ok(json!({"ast": sheet.to_json(), "source": text}))
}
