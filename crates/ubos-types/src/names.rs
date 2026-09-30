//! Names and identifiers (STD-FND-015, STD-FND-016, STD-VER-013).

use crate::error::{BpaError, invalid};
use regex::Regex;
use std::sync::OnceLock;

/// Branch kinds of STD-VER-013 (besides `main`).
pub const BRANCH_KINDS: &[&str] = &[
    "release",
    "draft",
    "feature",
    "agent",
    "overlay",
    "loader",
    "evolution",
    "correction",
    "sandbox",
    "device",
];

const SLUG: &str = "[a-z0-9][a-z0-9_-]{0,62}";

fn grammar(name: &str) -> Option<&'static Regex> {
    static G: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    let g = G.get_or_init(|| {
        let r = |p: String| Regex::new(&format!("^(?:{p})$")).expect("grammar");
        vec![
            ("slug", r(SLUG.into())),
            ("class-name", r("[A-Z][A-Za-z0-9]{0,63}".into())),
            ("member-name", r("[a-z][a-z0-9_]{0,63}".into())),
            ("namespace", r(format!("{SLUG}(\\.{SLUG})*"))),
            (
                "uuid",
                r(
                    "[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}"
                        .into(),
                ),
            ),
            (
                "uuid-v7",
                r("[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}".into()),
            ),
        ]
    });
    g.iter().find(|(n, _)| *n == name).map(|(_, r)| r)
}

/// True when `text` matches the named grammar (branch names excluded).
pub fn matches(name: &str, text: &str) -> bool {
    grammar(name).is_some_and(|r| r.is_match(text))
}

/// Check a name against a grammar: `slug`, `class-name`, `member-name`, `namespace`,
/// `uuid`, `uuid-v7` or `branch-name`.
pub fn check_name(name: &str, text: &str) -> Result<(), BpaError> {
    if name == "branch-name" {
        let parts: Vec<&str> = text.split('/').collect();
        let ok = text == "main"
            || (parts.len() >= 2
                && BRANCH_KINDS.contains(&parts[0])
                && parts.iter().all(|p| matches("slug", p)));
        return if ok {
            Ok(())
        } else {
            Err(BpaError::new("VER.INVALID_BRANCH_NAME", text))
        };
    }
    match grammar(name) {
        None => Err(invalid("unknown grammar")),
        Some(r) if r.is_match(text) => Ok(()),
        Some(_) => Err(BpaError::new("MODEL.INVALID_NAME", text)),
    }
}
