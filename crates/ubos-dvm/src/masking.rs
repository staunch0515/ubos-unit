//! Field masking for viewers (BPA 1.0-rc8; PH-2 step 8b, DEC-137; STD-LGS-052;
//! DSN-DVM-821, -822).
//!
//! Outputs made `as` a viewer `{principal, clearances, ctx}` are masked after authorization:
//! read bodies, the selected values of query rows, and outbox payloads.
//! * **Masks.** A field with a declared `mask` (the sheet property, with context
//!   qualifiers selected by the viewer's `ctx`) is shown in its mode:
//!   - `hide` removes it;
//!   - `partial` keeps the last four characters of a text and stars the rest;
//!   - `hash` gives `hmac-sha256:` and the keyed hash under the tenant lookup key
//!     (`lookup_key`).
//!
//!   `partial` on a value that is not a text, and `hash` without a lookup key or on an
//!   erased marker, hide the field.
//! * **Clearances.** A field whose classification the viewer's clearances do not cover is
//!   hidden unless a mask declares another mode. Clearances default to `internal`.
//! * **Queries.** A query made as a viewer that filters or sorts on a field masked for the
//!   viewer fails with `QRY.INVALID_QUERY`.

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::BpaError;
use ubos_types::values::{LEVELS, classification_covers};
use ubos_version::state::Repo;

/// Kernel metadata key of the tenant lookup key (STD-OBJ-012).
pub const LOOKUP_KEY: &str = "lookup_key";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The clearances of a viewer (`internal` by default); an unknown level is
/// `MODEL.INVALID_VALUE`.
pub(crate) fn clearances_of(viewer: &Value) -> Result<Vec<String>, BpaError> {
    let list = match viewer.get("clearances").filter(|c| !c.is_null()) {
        None => vec!["internal".to_string()],
        Some(Value::Array(a)) => a
            .iter()
            .map(|c| c.as_str().unwrap_or("?").to_string())
            .collect(),
        Some(_) => {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "clearances are classification levels",
            ));
        }
    };
    if list.iter().any(|c| !LEVELS.contains(&c.as_str())) {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "clearances are classification levels",
        ));
    }
    Ok(list)
}

/// Whether a value is an erased marker (STD-OBJ-011).
fn is_marker(v: &Value) -> bool {
    v.as_object().is_some_and(|o| {
        o.get("erased") == Some(&json!(true)) && o.len() == 2 && o.contains_key("commitment")
    })
}

/// The `partial` form of a text: its last four characters, the others starred.
pub fn partial(v: &str) -> String {
    let chars: Vec<char> = v.chars().collect();
    let keep = if chars.len() > 4 { 4 } else { 0 };
    let mut out: String = "*".repeat(chars.len() - keep);
    out.extend(&chars[chars.len() - keep..]);
    out
}

/// The members of `this` an expression text names (`this.f`, or a bare path `f`).
pub(crate) fn this_members(text: &str, bare: bool) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let t = text.trim();
    if bare && !t.starts_with("this") {
        if let Some(f) = t.split(['.', '?']).next().filter(|f| !f.is_empty()) {
            out.insert(f.to_string());
        }
        return out;
    }
    let bytes = t.as_bytes();
    let mut i = 0;
    while let Some(pos) = t[i..].find("this") {
        let start = i + pos;
        let before_ok =
            start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let mut j = start + 4;
        if before_ok && j < bytes.len() && bytes[j] == b'?' {
            j += 1;
        }
        if before_ok && j < bytes.len() && bytes[j] == b'.' {
            let k = j + 1;
            let end = t[k..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(t.len(), |e| k + e);
            if end > k {
                out.insert(t[k..end].to_string());
            }
        }
        i = start + 4;
    }
    out
}

impl Engine {
    /// Adapter operation `lookup_key {key}`: set the tenant lookup key (STD-OBJ-012), 32
    /// bytes in lowercase hexadecimal, used by `hash` masks.
    pub fn set_lookup_key(&mut self, i: &Value) -> Result<Value, BpaError> {
        let key = s(i, "key").unwrap_or_default();
        let ok = key.len() == 64
            && key
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if !ok {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "a lookup key is 32 bytes in lowercase hexadecimal",
            ));
        }
        self.store.put_meta(LOOKUP_KEY, &json!(key))?;
        Ok(Value::Bool(true))
    }

    /// The tenant lookup key, when set.
    fn lookup_key(&mut self) -> Result<Option<Vec<u8>>, BpaError> {
        Ok(self.store.meta(LOOKUP_KEY)?.and_then(|k| {
            let k = k.as_str()?.to_string();
            (0..32)
                .map(|j| u8::from_str_radix(k.get(2 * j..2 * j + 2)?, 16).ok())
                .collect()
        }))
    }

    /// `{field: mode}` for a viewer: the declared masks, and `hide` for the fields whose
    /// classification the viewer's clearances do not cover.
    pub(crate) fn mask_modes(
        &mut self,
        branch: &str,
        sel: &Value,
        oid: &str,
        class: &str,
        viewer: &Value,
    ) -> Result<BTreeMap<String, String>, BpaError> {
        let mut modes = BTreeMap::new();
        if self.sheets_declare(branch, sel, "mask")? {
            let mut i = sel.clone();
            i["branch"] = json!(branch);
            i["object_id"] = json!(oid);
            let mut c = viewer
                .get("ctx")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            c.insert("principal".into(), viewer["principal"].clone());
            i["ctx"] = Value::Object(c);
            let props = self.cascade(&i)?;
            for (k, v) in props["properties"].as_object().into_iter().flatten() {
                if k == "mask" {
                    for (f, m) in v.as_object().into_iter().flatten() {
                        if let Some(m) = m.as_str() {
                            modes.insert(f.clone(), m.to_string());
                        }
                    }
                } else if let (Some(f), Some(m)) = (k.strip_prefix("mask."), v.as_str()) {
                    modes.insert(f.to_string(), m.to_string());
                }
            }
        }
        let clearances = clearances_of(viewer)?;
        let fields = self.registry.class(class)?.fields.clone();
        for (f, d) in fields {
            let level = s(&d, "classification").unwrap_or("internal");
            if !modes.contains_key(&f) && !classification_covers(&clearances, level)? {
                modes.insert(f, "hide".into());
            }
        }
        Ok(modes)
    }

    /// A body with the masks applied.
    pub(crate) fn mask_body(
        &mut self,
        body: &mut Map<String, Value>,
        modes: &BTreeMap<String, String>,
    ) -> Result<(), BpaError> {
        let key = if modes.values().any(|m| m == "hash") {
            self.lookup_key()?
        } else {
            None
        };
        for (f, m) in modes {
            let Some(v) = body.get(f).filter(|v| !v.is_null()).cloned() else {
                continue;
            };
            match (m.as_str(), &v) {
                ("partial", Value::String(t)) => {
                    body.insert(f.clone(), json!(partial(t)));
                }
                ("hash", _) if key.is_some() && !is_marker(&v) => {
                    let mac = ubos_crypto::hmac_sha256(
                        key.as_deref().unwrap_or_default(),
                        ubos_types::canonical(&v)?.as_bytes(),
                    );
                    let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
                    body.insert(f.clone(), json!(format!("hmac-sha256:{hex}")));
                }
                _ => {
                    body.remove(f);
                }
            }
        }
        Ok(())
    }

    /// Whether a live sheet of the branch declares a property (or one of its keys) at the
    /// snapshot.
    pub(crate) fn sheets_declare(
        &mut self,
        branch: &str,
        sel: &Value,
        property: &str,
    ) -> Result<bool, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, sel)?;
        let cseq = repo.commit(snap.commit)?.cseq;
        let dotted = format!("{property}.");
        for (_, o) in repo.objects_in(branch, cseq, Some("GovernanceSheet"))? {
            let v = repo.version(&o.head)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            let declares = v["body"]["ast"]["blocks"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|b| b["declarations"].as_array().cloned().unwrap_or_default())
                .any(|d| {
                    s(&d, "property").is_some_and(|p| p == property || p.starts_with(&dotted))
                });
            if declares {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Adapter operation `outbox`, masked for a viewer when given `as` (STD-LGS-052): each
    /// payload by the masks and classifications of its effect's object.
    pub fn outbox_for(&mut self, i: &Value) -> Result<Value, BpaError> {
        let mut out = self.outbox()?;
        let Some(viewer) = crate::access::viewer_of(i)? else {
            return Ok(out);
        };
        let mut effects = out["effects"].as_array().cloned().unwrap_or_default();
        for e in effects.iter_mut() {
            let (Some(oid), Some(branch)) = (
                s(e, "object_id").map(str::to_string),
                s(e, "branch").map(str::to_string),
            ) else {
                continue;
            };
            if !e["payload"].is_object() {
                continue;
            }
            let Ok((_, class)) = self.read_classed(&json!({"branch": branch, "object_id": oid}))
            else {
                continue;
            };
            let modes = self.mask_modes(&branch, &json!({}), &oid, &class, &viewer)?;
            let mut p = e["payload"].as_object().cloned().unwrap_or_default();
            self.mask_body(&mut p, &modes)?;
            e["payload"] = Value::Object(p);
        }
        out["effects"] = Value::Array(effects);
        Ok(out)
    }
}
