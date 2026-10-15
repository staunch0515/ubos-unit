//! The sheet object: the lossless AST of STD-LGS-002 and its canonical text (STD-LGS-001).

use serde_json::{Map, Value, json};
use ubos_model::types::BpaError;
use ubos_model::types::error::invalid;

/// A constant of a declaration or of a context condition: an L1 literal, a list literal or
/// a map literal. Map members keep their authored order, which the canonical text keeps.
#[derive(Debug, Clone, PartialEq)]
pub enum SheetValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer literal.
    Int(i64),
    /// Text; a decimal or date literal is kept as its text, as canonical JSON does.
    Text(String),
    /// A list literal.
    List(Vec<SheetValue>),
    /// A map literal (a money literal is the map `{amount, currency}`).
    Map(Vec<(String, SheetValue)>),
}

fn is_decimal(s: &str) -> bool {
    let t = s.strip_prefix('-').unwrap_or(s);
    let Some((a, b)) = t.split_once('.') else {
        return false;
    };
    !a.is_empty()
        && !b.is_empty()
        && a.bytes().all(|c| c.is_ascii_digit())
        && b.bytes().all(|c| c.is_ascii_digit())
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl SheetValue {
    /// The canonical JSON of the value (`DAT-Sheet.ast`).
    pub fn to_json(&self) -> Value {
        match self {
            SheetValue::Null => Value::Null,
            SheetValue::Bool(b) => Value::Bool(*b),
            SheetValue::Int(i) => json!(i),
            SheetValue::Text(s) => Value::String(s.clone()),
            SheetValue::List(items) => {
                Value::Array(items.iter().map(SheetValue::to_json).collect())
            }
            SheetValue::Map(members) => Value::Object(
                members
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
        }
    }

    /// A value from its JSON form (`MODEL.INVALID_VALUE` for a non-sheet value).
    pub fn from_json(v: &Value) -> Result<SheetValue, BpaError> {
        Ok(match v {
            Value::Null => SheetValue::Null,
            Value::Bool(b) => SheetValue::Bool(*b),
            Value::Number(n) => {
                SheetValue::Int(n.as_i64().ok_or_else(|| invalid("not a sheet value"))?)
            }
            Value::String(s) => SheetValue::Text(s.clone()),
            Value::Array(items) => SheetValue::List(
                items
                    .iter()
                    .map(SheetValue::from_json)
                    .collect::<Result<_, _>>()?,
            ),
            Value::Object(o) => SheetValue::Map(
                o.iter()
                    .map(|(k, v)| SheetValue::from_json(v).map(|x| (k.clone(), x)))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }

    /// The literal text of the value in the canonical form.
    pub fn text(&self) -> String {
        match self {
            SheetValue::Null => "null".into(),
            SheetValue::Bool(true) => "true".into(),
            SheetValue::Bool(false) => "false".into(),
            SheetValue::Int(i) => i.to_string(),
            SheetValue::Text(s) if is_decimal(s) || is_date(s) => s.clone(),
            SheetValue::Text(s) => quote(s),
            SheetValue::List(items) => {
                format!(
                    "[{}]",
                    items
                        .iter()
                        .map(SheetValue::text)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            SheetValue::Map(members) => {
                if members.len() == 2
                    && let Some((_, SheetValue::Text(amount))) =
                        members.iter().find(|(k, _)| k == "amount")
                    && let Some((_, SheetValue::Text(cur))) =
                        members.iter().find(|(k, _)| k == "currency")
                {
                    return format!("{amount}{cur}");
                }
                format!(
                    "{{{}}}",
                    members
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", v.text()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }

    /// The text of a `Text` value.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            SheetValue::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// A qualifier of a selector (STD-LGS-001).
#[derive(Debug, Clone, PartialEq)]
pub enum Qualifier {
    /// `[expr]`: an L1 boolean over `this`, kept as text.
    Attr(String),
    /// `:state`.
    State(String),
    /// `:tag(slug)`.
    Tag(String),
    /// `:ctx(key op value)`: `op` is `=`, `!=` or `in`; the value a text or a list of texts.
    Ctx {
        /// Context key.
        key: String,
        /// `=`, `!=` or `in`.
        op: String,
        /// A text, or a list of texts for `in`.
        value: SheetValue,
    },
    /// `:rel(member)[expr]`: one reference hop.
    Rel {
        /// Reference member of the selected class.
        member: String,
        /// L1 boolean over the referenced object.
        expr: String,
    },
}

impl Qualifier {
    /// The kind name of the AST: `attr`, `state`, `tag`, `ctx` or `rel`.
    pub fn kind(&self) -> &'static str {
        match self {
            Qualifier::Attr(_) => "attr",
            Qualifier::State(_) => "state",
            Qualifier::Tag(_) => "tag",
            Qualifier::Ctx { .. } => "ctx",
            Qualifier::Rel { .. } => "rel",
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Qualifier::Attr(e) => json!({"kind": "attr", "expr": e}),
            Qualifier::State(s) => json!({"kind": "state", "state": s}),
            Qualifier::Tag(t) => json!({"kind": "tag", "tag": t}),
            Qualifier::Ctx { key, op, value } => {
                json!({"kind": "ctx", "key": key, "op": op, "value": value.to_json()})
            }
            Qualifier::Rel { member, expr } => {
                json!({"kind": "rel", "member": member, "expr": expr})
            }
        }
    }

    fn from_json(v: &Value) -> Result<Qualifier, BpaError> {
        let s = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| invalid(format!("a qualifier has {k}")))
        };
        Ok(match v.get("kind").and_then(Value::as_str) {
            Some("attr") => Qualifier::Attr(s("expr")?),
            Some("state") => Qualifier::State(s("state")?),
            Some("tag") => Qualifier::Tag(s("tag")?),
            Some("ctx") => Qualifier::Ctx {
                key: s("key")?,
                op: s("op")?,
                value: SheetValue::from_json(v.get("value").unwrap_or(&Value::Null))?,
            },
            Some("rel") => Qualifier::Rel {
                member: s("member")?,
                expr: s("expr")?,
            },
            _ => return Err(invalid("a qualifier has a kind")),
        })
    }

    fn text(&self) -> String {
        match self {
            Qualifier::Attr(e) => format!("[{e}]"),
            Qualifier::State(s) => format!(":{s}"),
            Qualifier::Tag(t) => format!(":tag({t})"),
            Qualifier::Ctx { key, op, value } => format!(":ctx({key} {op} {})", value.text()),
            Qualifier::Rel { member, expr } => format!(":rel({member})[{expr}]"),
        }
    }
}

/// A selector: a type selector (a class name or `*`) with qualifiers.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    /// Class name or `*`.
    pub type_name: String,
    /// Qualifiers in order.
    pub qualifiers: Vec<Qualifier>,
}

impl Selector {
    /// The JSON form `{type, qualifiers?}`.
    pub fn to_json(&self) -> Value {
        let mut out = json!({"type": self.type_name});
        if !self.qualifiers.is_empty() {
            out["qualifiers"] =
                Value::Array(self.qualifiers.iter().map(Qualifier::to_json).collect());
        }
        out
    }

    /// A selector from its JSON form.
    pub fn from_json(v: &Value) -> Result<Selector, BpaError> {
        let type_name = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("a selector has a type"))?
            .to_string();
        let qualifiers = match v.get("qualifiers") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(qs)) => qs
                .iter()
                .map(Qualifier::from_json)
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(invalid("qualifiers is a list")),
        };
        Ok(Selector {
            type_name,
            qualifiers,
        })
    }

    /// The canonical text of the selector.
    pub fn text(&self) -> String {
        let mut out = self.type_name.clone();
        for q in &self.qualifiers {
            out.push_str(&q.text());
        }
        out
    }
}

/// A declaration `property: value [!locked];`.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Property path (a registered name, or a map property followed by one key).
    pub property: String,
    /// Declared value.
    pub value: SheetValue,
    /// `!locked`.
    pub locked: bool,
}

/// A rule block: a selector list with its declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The selector list; the block applies to every selector.
    pub selectors: Vec<Selector>,
    /// At least one declaration.
    pub declarations: Vec<Declaration>,
}

/// A parsed sheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sheet {
    /// Blocks in source order.
    pub blocks: Vec<Block>,
}

impl Sheet {
    /// The JSON AST `{blocks: [{selectors, declarations}]}` (STD-LGS-002).
    pub fn to_json(&self) -> Value {
        let blocks: Vec<Value> = self
            .blocks
            .iter()
            .map(|b| {
                let decls: Vec<Value> = b
                    .declarations
                    .iter()
                    .map(|d| {
                        let mut m = Map::new();
                        m.insert("property".into(), Value::String(d.property.clone()));
                        m.insert("value".into(), d.value.to_json());
                        if d.locked {
                            m.insert("locked".into(), Value::Bool(true));
                        }
                        Value::Object(m)
                    })
                    .collect();
                json!({
                    "selectors": b.selectors.iter().map(Selector::to_json).collect::<Vec<_>>(),
                    "declarations": decls,
                })
            })
            .collect();
        json!({"blocks": blocks})
    }

    /// A sheet from its JSON AST (`MODEL.INVALID_VALUE` when malformed).
    pub fn from_json(ast: &Value) -> Result<Sheet, BpaError> {
        let blocks = ast
            .get("blocks")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("a sheet AST is {blocks}"))?;
        let mut out = Vec::with_capacity(blocks.len());
        for b in blocks {
            let sels = b.get("selectors").and_then(Value::as_array);
            let decls = b.get("declarations").and_then(Value::as_array);
            let (Some(sels), Some(decls)) = (sels, decls) else {
                return Err(invalid("a block has selectors and declarations"));
            };
            if sels.is_empty() || decls.is_empty() {
                return Err(invalid("a block has selectors and declarations"));
            }
            let selectors = sels
                .iter()
                .map(Selector::from_json)
                .collect::<Result<_, _>>()?;
            let mut declarations = Vec::with_capacity(decls.len());
            for d in decls {
                let property = d
                    .get("property")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("a declaration has a property"))?
                    .to_string();
                let value = SheetValue::from_json(d.get("value").unwrap_or(&Value::Null))?;
                let locked = match d.get("locked") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(b)) => *b,
                    Some(_) => return Err(invalid("locked is boolean")),
                };
                declarations.push(Declaration {
                    property,
                    value,
                    locked,
                });
            }
            out.push(Block {
                selectors,
                declarations,
            });
        }
        Ok(Sheet { blocks: out })
    }

    /// The canonical text (STD-LGS-001 notes): one block per paragraph, selectors joined by
    /// `, `, one declaration per line indented by two spaces, a trailing newline.
    pub fn print(&self) -> String {
        let mut paragraphs = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            let mut lines = vec![format!(
                "{} {{",
                b.selectors
                    .iter()
                    .map(Selector::text)
                    .collect::<Vec<_>>()
                    .join(", ")
            )];
            for d in &b.declarations {
                lines.push(format!(
                    "  {}: {}{};",
                    d.property,
                    d.value.text(),
                    if d.locked { " !locked" } else { "" }
                ));
            }
            lines.push("}".into());
            paragraphs.push(lines.join("\n"));
        }
        let mut out = paragraphs.join("\n\n");
        out.push('\n');
        out
    }
}
