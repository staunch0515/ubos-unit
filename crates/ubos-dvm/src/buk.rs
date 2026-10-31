//! The Buk archive (PH-2 step 7c, DEC-129; DSN-FRG-301, -303, -304; FR-PKG-011, -012).
//!
//! A Buk is a format of the UBOS product, not of BPA 1.0 (DEC-127). This module holds what
//! Forge and the node share: the archive with its integrity check, the root hash, detached
//! signatures, and semantic versions with ranges. The format is described in
//! `docs/50-system/40-systems/04-frg/09-buk-format.md`.
//!
//! An archive is one canonical JSON document (RFC 8785):
//!
//! ```text
//! {"format": "ubos.buk.v1",
//!  "manifest": {...},
//!  "tree": {"<path>": "<chunk hash>", ...},
//!  "chunks": {"<chunk hash>": <content>, ...},
//!  "root": "<root hash>",
//!  "signatures": [<DAT-Signature over the root>, ...]}
//! ```
//!
//! * A chunk is the parsed content of one source file (JSON; a text file is a string); its
//!   hash is `content_id("ubos.chunk.v1", content)`.
//! * The root is `content_id("ubos.chunk.v1", {"buk": {"manifest": manifest, "tree":
//!   tree}})`, the wrapper the genesis Buks use (DEC-077): it binds
//!   the manifest and every path to its content, so equal sources give equal roots on every
//!   machine.
//! * Signatures are detached: they sign the root and are listed in the trailer member
//!   `signatures`, which the root does not cover.

use crate::types::{BpaError, canonical, content_id};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// The format tag.
pub const FORMAT: &str = "ubos.buk.v1";

/// The ABI major version this implementation runs (BPA 1.0).
pub const ABI_MAJOR: u64 = 1;

fn invalid(m: impl Into<String>) -> BpaError {
    BpaError::new("PKG.MANIFEST_INVALID", m.into())
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// A Buk archive.
#[derive(Debug, Clone, PartialEq)]
pub struct Archive {
    /// The manifest: `name`, `namespace`, `version`, `abi`, `dependencies`, `capabilities`,
    /// `licence`, `description`, `contents`.
    pub manifest: Value,
    /// Path → chunk hash.
    pub tree: BTreeMap<String, String>,
    /// Chunk hash → content.
    pub chunks: BTreeMap<String, Value>,
    /// Detached signatures over the root.
    pub signatures: Vec<Value>,
}

/// The hash of a chunk.
pub fn chunk_hash(content: &Value) -> Result<String, BpaError> {
    content_id("ubos.chunk.v1", content)
}

/// The fields every manifest has.
pub fn check_manifest(m: &Value) -> Result<(), BpaError> {
    if !m.is_object() {
        return Err(invalid("the manifest is an object"));
    }
    for k in ["name", "namespace", "version", "abi"] {
        if s(m, k).is_none_or(str::is_empty) {
            return Err(invalid(format!("the manifest names its {k}")));
        }
    }
    let name = s(m, "name").unwrap_or_default();
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(invalid(
            "a Buk name is lowercase letters, digits, '-' and '_'",
        ));
    }
    let ns = s(m, "namespace").unwrap_or_default();
    if !ns.split('.').all(|p| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }) {
        return Err(invalid("a namespace is dot-separated lowercase segments"));
    }
    Version::parse(s(m, "version").unwrap_or_default())?;
    abi_major(s(m, "abi").unwrap_or_default())?;
    if let Some(d) = m.get("dependencies").filter(|d| !d.is_null()) {
        let d = d
            .as_object()
            .ok_or_else(|| invalid("dependencies map Buk IDs to version ranges"))?;
        for (id, r) in d {
            split_id(id)?;
            Range::parse(r.as_str().ok_or_else(|| invalid("a range is a string"))?)?;
        }
    }
    if let Some(c) = m.get("capabilities").filter(|c| !c.is_null())
        && !c.as_array().is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(invalid("capabilities are a list of names"));
    }
    Ok(())
}

/// The major version of an ABI string (`1.0` → 1).
pub fn abi_major(abi: &str) -> Result<u64, BpaError> {
    abi.split('.')
        .next()
        .and_then(|m| m.parse().ok())
        .filter(|_| abi.split('.').all(|p| p.parse::<u64>().is_ok()))
        .ok_or_else(|| invalid("abi is <major>.<minor>"))
}

/// A Buk ID `<namespace>/<name>` split into its parts.
pub fn split_id(id: &str) -> Result<(&str, &str), BpaError> {
    id.split_once('/')
        .filter(|(n, b)| !n.is_empty() && !b.is_empty() && !b.contains('/'))
        .ok_or_else(|| invalid(format!("{id}: a Buk ID is <namespace>/<name>")))
}

impl Archive {
    /// An archive of a manifest and files (path → content), unsigned.
    pub fn new(manifest: Value, files: BTreeMap<String, Value>) -> Result<Archive, BpaError> {
        check_manifest(&manifest)?;
        let mut tree = BTreeMap::new();
        let mut chunks = BTreeMap::new();
        for (path, content) in files {
            check_path(&path)?;
            let h = chunk_hash(&content)?;
            tree.insert(path, h.clone());
            chunks.insert(h, content);
        }
        Ok(Archive {
            manifest,
            tree,
            chunks,
            signatures: Vec::new(),
        })
    }

    /// The Buk ID `<namespace>/<name>`.
    pub fn id(&self) -> String {
        format!(
            "{}/{}",
            s(&self.manifest, "namespace").unwrap_or_default(),
            s(&self.manifest, "name").unwrap_or_default()
        )
    }

    /// The version.
    pub fn version(&self) -> &str {
        s(&self.manifest, "version").unwrap_or_default()
    }

    /// The root hash over the manifest and the tree.
    pub fn root(&self) -> Result<String, BpaError> {
        content_id(
            "ubos.chunk.v1",
            &json!({"buk": {"manifest": self.manifest, "tree": self.tree}}),
        )
    }

    /// The content of a path.
    pub fn file(&self, path: &str) -> Option<&Value> {
        self.tree.get(path).and_then(|h| self.chunks.get(h))
    }

    /// The archive document.
    pub fn to_json(&self) -> Result<Value, BpaError> {
        Ok(
            json!({"format": FORMAT, "manifest": self.manifest, "tree": self.tree,
                  "chunks": self.chunks, "root": self.root()?, "signatures": self.signatures}),
        )
    }

    /// The archive bytes: canonical JSON.
    pub fn encode(&self) -> Result<Vec<u8>, BpaError> {
        Ok(canonical(&self.to_json()?)?.into_bytes())
    }

    /// Read an archive and check its integrity: the format, the manifest, every chunk
    /// against its hash, every path against a chunk, no chunk without a path, and the
    /// stated root (`PKG.MANIFEST_INVALID`).
    pub fn decode(bytes: &[u8]) -> Result<Archive, BpaError> {
        let bad = |m: &str| BpaError::new("PKG.MANIFEST_INVALID", m.to_string());
        let v: Value =
            serde_json::from_slice(bytes).map_err(|e| bad(&format!("not a Buk archive: {e}")))?;
        if v["format"] != FORMAT {
            return Err(bad("not a ubos.buk.v1 archive"));
        }
        let manifest = v["manifest"].clone();
        check_manifest(&manifest)?;
        let tree: BTreeMap<String, String> = serde_json::from_value(v["tree"].clone())
            .map_err(|_| bad("the tree maps paths to chunk hashes"))?;
        let chunks: BTreeMap<String, Value> = v["chunks"]
            .as_object()
            .ok_or_else(|| bad("chunks are a map"))?
            .iter()
            .map(|(k, x)| (k.clone(), x.clone()))
            .collect();
        for (h, c) in &chunks {
            if &chunk_hash(c)? != h {
                return Err(bad(&format!("chunk {h} does not match its hash")));
            }
        }
        for (p, h) in &tree {
            check_path(p).map_err(|_| bad(&format!("{p}: not a relative path")))?;
            if !chunks.contains_key(h) {
                return Err(bad(&format!("{p}: chunk {h} is missing")));
            }
        }
        if chunks.keys().any(|h| !tree.values().any(|x| x == h)) {
            return Err(bad("a chunk belongs to no path"));
        }
        let signatures = match &v["signatures"] {
            Value::Null => Vec::new(),
            Value::Array(a) => a.clone(),
            _ => return Err(bad("signatures are a list")),
        };
        let a = Archive {
            manifest,
            tree,
            chunks,
            signatures,
        };
        if v["root"] != json!(a.root()?) {
            return Err(bad("the root does not match the manifest and the tree"));
        }
        Ok(a)
    }

    /// Add a signature over the root (DSN-FRG-304); a signature of the same signer and key
    /// is replaced.
    pub fn sign(&mut self, seed: &[u8; 32]) -> Result<Value, BpaError> {
        let public = crate::types::addr::b64u(&crate::crypto::ed25519_public(seed));
        let did = crate::types::addr::key_did(&json!(public))?;
        let key_id = format!("{did}#{}", did.trim_start_matches("did:ubos:k:"));
        let sig = crate::crypto::sign(seed, &self.root()?, &did, &key_id, "signer")?;
        self.signatures
            .retain(|x| !(x["signer"] == json!(did) && x["key_id"] == json!(key_id)));
        self.signatures.push(sig.clone());
        Ok(sig)
    }

    /// Check every signature offline against the key-form DID it names (STD-ADDR-010):
    /// the signers, in order (`PROOF.SIGNATURE_INVALID` for a signature that fails).
    pub fn verify_signatures(&self) -> Result<Vec<String>, BpaError> {
        let root = self.root()?;
        let mut out = Vec::new();
        for sig in &self.signatures {
            let signer = sig["signer"].clone();
            let doc = crate::types::addr::key_document(&signer).map_err(|_| {
                BpaError::new(
                    "PROOF.SIGNATURE_INVALID",
                    "the signer is not a key-form DID",
                )
            })?;
            crate::crypto::verify(sig, &root, &doc, None)?;
            out.push(signer.as_str().unwrap_or_default().to_string());
        }
        Ok(out)
    }
}

fn check_path(p: &str) -> Result<(), BpaError> {
    let ok = !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && p.split('/').all(|x| !x.is_empty() && x != "." && x != "..");
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("{p}: a path is relative, '/'-separated")))
    }
}

// ---- semantic versions and ranges (DSN-FRG-303, FR-PKG-021) ----

/// A semantic version `major.minor.patch[-pre]` (build metadata is not allowed).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major, minor, patch.
    pub core: (u64, u64, u64),
    /// Whether the version is a release (no pre-release part); releases order after their
    /// pre-releases.
    pub release: bool,
    /// Pre-release identifiers.
    pub pre: Vec<Pre>,
}

/// A pre-release identifier: numeric ones order before alphanumeric ones.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pre {
    /// Numeric.
    Num(u64),
    /// Alphanumeric.
    Text(String),
}

impl Version {
    /// Parse a version.
    pub fn parse(t: &str) -> Result<Version, BpaError> {
        let bad = || invalid(format!("{t}: a version is <major>.<minor>.<patch>[-<pre>]"));
        let (core, pre) = match t.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (t, None),
        };
        let n: Vec<u64> = core
            .split('.')
            .map(|x| {
                if x.is_empty() || (x.len() > 1 && x.starts_with('0')) {
                    None
                } else {
                    x.parse().ok()
                }
            })
            .collect::<Option<_>>()
            .ok_or_else(bad)?;
        if n.len() != 3 {
            return Err(bad());
        }
        let pre = match pre {
            None => Vec::new(),
            Some(p) => p
                .split('.')
                .map(|x| {
                    if x.is_empty() || !x.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                        None
                    } else if x.chars().all(|c| c.is_ascii_digit()) {
                        x.parse().ok().map(Pre::Num)
                    } else {
                        Some(Pre::Text(x.to_string()))
                    }
                })
                .collect::<Option<_>>()
                .ok_or_else(bad)?,
        };
        Ok(Version {
            core: (n[0], n[1], n[2]),
            release: pre.is_empty(),
            pre,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.core.0, self.core.1, self.core.2)?;
        if !self.pre.is_empty() {
            let p: Vec<String> = self
                .pre
                .iter()
                .map(|x| match x {
                    Pre::Num(n) => n.to_string(),
                    Pre::Text(t) => t.clone(),
                })
                .collect();
            write!(f, "-{}", p.join("."))?;
        }
        Ok(())
    }
}

/// A version range: a list of alternatives (`||`), each a conjunction of comparators.
#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    /// The text.
    pub text: String,
    alts: Vec<Vec<(Op, Version)>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
}

impl Range {
    /// Parse a range: `*`, `1.2.3` or `^1.2.3` (compatible), `~1.2.3` (patch-level),
    /// `=1.2.3`, comparators `>=1.0.0 <2.0.0` (also comma-separated), alternatives `||`.
    /// Partial versions `1` and `1.2` are allowed in caret, tilde and bare forms.
    pub fn parse(t: &str) -> Result<Range, BpaError> {
        let mut alts = Vec::new();
        for alt in t.split("||") {
            let mut cs = Vec::new();
            for part in alt.split([',', ' ']).filter(|x| !x.is_empty()) {
                cs.extend(comparator(part)?);
            }
            if alt.trim().is_empty() {
                return Err(invalid(format!("{t}: an empty alternative")));
            }
            alts.push(cs);
        }
        Ok(Range {
            text: t.to_string(),
            alts,
        })
    }

    /// Whether a version satisfies the range. A pre-release satisfies it only when a
    /// comparator of the same alternative names a pre-release of the same core version.
    pub fn matches(&self, v: &Version) -> bool {
        self.alts.iter().any(|cs| {
            let ok = cs.iter().all(|(op, x)| match op {
                Op::Eq => v == x,
                Op::Gt => v > x,
                Op::Ge => v >= x,
                Op::Lt => v < x,
                Op::Le => v <= x,
            });
            ok && (v.release || cs.iter().any(|(_, x)| !x.release && x.core == v.core))
        })
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

fn partial(t: &str) -> Result<(Vec<u64>, Version), BpaError> {
    let (core, pre) = match t.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (t, None),
    };
    let n: Vec<u64> = core
        .split('.')
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()
        .ok_or_else(|| invalid(format!("{t}: not a version")))?;
    if n.is_empty() || n.len() > 3 || (pre.is_some() && n.len() != 3) {
        return Err(invalid(format!("{t}: not a version")));
    }
    let mut full = n.clone();
    full.resize(3, 0);
    let text = match pre {
        Some(p) => format!("{}.{}.{}-{p}", full[0], full[1], full[2]),
        None => format!("{}.{}.{}", full[0], full[1], full[2]),
    };
    Ok((n, Version::parse(&text)?))
}

fn upper(major: u64, minor: u64, patch: u64) -> Version {
    // the exclusive upper bound below every pre-release of the next version
    Version {
        core: (major, minor, patch),
        release: false,
        pre: vec![Pre::Num(0)],
    }
}

fn comparator(p: &str) -> Result<Vec<(Op, Version)>, BpaError> {
    if p == "*" {
        return Ok(vec![(Op::Ge, Version::parse("0.0.0")?)]);
    }
    for (pfx, op) in [
        (">=", Op::Ge),
        ("<=", Op::Le),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(rest) = p.strip_prefix(pfx) {
            return Ok(vec![(op, Version::parse(rest)?)]);
        }
    }
    if let Some(rest) = p.strip_prefix('~') {
        let (n, v) = partial(rest)?;
        let hi = if n.len() == 1 {
            upper(n[0] + 1, 0, 0)
        } else {
            upper(n[0], n[1] + 1, 0)
        };
        return Ok(vec![(Op::Ge, v), (Op::Lt, hi)]);
    }
    let rest = p.strip_prefix('^').unwrap_or(p);
    let (n, v) = partial(rest)?;
    let (a, b, c) = v.core;
    let hi = if a > 0 || n.len() == 1 {
        upper(a + 1, 0, 0)
    } else if b > 0 || n.len() == 2 {
        upper(0, b + 1, 0)
    } else {
        upper(0, 0, c + 1)
    };
    Ok(vec![(Op::Ge, v), (Op::Lt, hi)])
}

/// The manifest of an archive as a summary for reports.
pub fn summary(a: &Archive) -> Result<Value, BpaError> {
    let mut m = Map::new();
    m.insert("id".into(), json!(a.id()));
    m.insert("version".into(), json!(a.version()));
    m.insert("root".into(), json!(a.root()?));
    m.insert("files".into(), json!(a.tree.len()));
    m.insert("chunks".into(), json!(a.chunks.len()));
    Ok(Value::Object(m))
}
