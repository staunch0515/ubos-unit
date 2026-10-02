//! Lexer of L1 expressions (STD-EXPR-001, lexical rules of DEC-043).

use serde_json::json;
use ubos_model::types::BpaError;

/// Token kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A date literal `d'…'` (text inside the quotes).
    Date,
    /// An instant literal `t'…'`.
    Inst,
    /// A money literal.
    Money,
    /// A number.
    Num,
    /// A string (unescaped, NFC).
    Str,
    /// An identifier.
    Ident,
    /// A keyword.
    Kw,
    /// An operator or punctuation.
    Op,
    /// End of input.
    End,
}

/// A token: kind, text and position (code-point offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Kind.
    pub kind: Kind,
    /// Text (for strings, the unescaped value).
    pub text: String,
    /// Position.
    pub pos: usize,
}

const KEYWORDS: &[&str] = &[
    "and", "or", "not", "in", "is", "null", "true", "false", "matches",
];

/// A type error with position (STD-EXPR-002).
pub fn terr(pos: usize, msg: &str) -> BpaError {
    BpaError::with_details("LOGIC.TYPE_ERROR", json!({"position": pos, "message": msg}))
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokenise an expression.
pub fn lex(src: &str) -> Result<Vec<Token>, BpaError> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // date and instant literals
        if (c == 'd' || c == 't') && cs.get(i + 1) == Some(&'\'') {
            let mut j = i + 2;
            while j < cs.len() && cs[j] != '\'' {
                j += 1;
            }
            if j < cs.len() {
                let text: String = cs[i + 2..j].iter().collect();
                out.push(Token {
                    kind: if c == 'd' { Kind::Date } else { Kind::Inst },
                    text,
                    pos: i,
                });
                i = j + 1;
                continue;
            }
        }
        // numbers and money
        if c.is_ascii_digit() {
            let mut j = i;
            if cs[j] == '0' {
                j += 1;
            } else {
                while j < cs.len() && cs[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if cs.get(j) == Some(&'.') && cs.get(j + 1).is_some_and(|d| d.is_ascii_digit()) {
                j += 1;
                while j < cs.len() && cs[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let num_end = j;
            // money: three upper-case letters not followed by an identifier character
            if j + 3 <= cs.len()
                && cs[j..j + 3].iter().all(|x| x.is_ascii_uppercase())
                && !cs.get(j + 3).is_some_and(|x| is_ident(*x))
            {
                out.push(Token {
                    kind: Kind::Money,
                    text: cs[i..j + 3].iter().collect(),
                    pos: i,
                });
                i = j + 3;
                continue;
            }
            if !cs.get(num_end).is_some_and(|x| is_ident(*x)) {
                out.push(Token {
                    kind: Kind::Num,
                    text: cs[i..num_end].iter().collect(),
                    pos: i,
                });
                i = num_end;
                continue;
            }
            return Err(terr(i, "unexpected character"));
        }
        if c == '"' {
            let mut j = i + 1;
            let mut s = String::new();
            loop {
                let Some(&x) = cs.get(j) else {
                    return Err(terr(i, "unexpected character"));
                };
                if x == '"' {
                    break;
                }
                if x == '\\' {
                    let Some(&e) = cs.get(j + 1) else {
                        return Err(terr(i, "unexpected character"));
                    };
                    if e == 'u' {
                        let hex: String = cs.get(j + 2..j + 6).unwrap_or(&[]).iter().collect();
                        if hex.len() == 4 && hex.chars().all(|h| h.is_ascii_hexdigit()) {
                            let code = u32::from_str_radix(&hex, 16).unwrap_or_default();
                            s.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            j += 6;
                            continue;
                        }
                    }
                    let r = match e {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '/' => '/',
                        _ => return Err(terr(j, "unknown escape")),
                    };
                    s.push(r);
                    j += 2;
                } else {
                    s.push(x);
                    j += 1;
                }
            }
            out.push(Token {
                kind: Kind::Str,
                text: ubos_model::types::nfc(&s),
                pos: i,
            });
            i = j + 1;
            continue;
        }
        if is_ident_start(c) {
            let mut j = i;
            while j < cs.len() && is_ident(cs[j]) {
                j += 1;
            }
            let text: String = cs[i..j].iter().collect();
            let kind = if KEYWORDS.contains(&text.as_str()) {
                Kind::Kw
            } else {
                Kind::Ident
            };
            out.push(Token { kind, text, pos: i });
            i = j;
            continue;
        }
        let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
        if ["?.", "==", "!=", "<=", ">=", "->"].contains(&two.as_str()) {
            out.push(Token {
                kind: Kind::Op,
                text: two,
                pos: i,
            });
            i += 2;
            continue;
        }
        if "-+*/%<>()[],.?:".contains(c) {
            out.push(Token {
                kind: Kind::Op,
                text: c.to_string(),
                pos: i,
            });
            i += 1;
            continue;
        }
        return Err(terr(i, "unexpected character"));
    }
    out.push(Token {
        kind: Kind::End,
        text: String::new(),
        pos: cs.len(),
    });
    Ok(out)
}
