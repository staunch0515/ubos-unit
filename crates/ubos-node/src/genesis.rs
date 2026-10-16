//! The genesis Buks embedded in the node (PH-1 step 16b, DEC-077; DSN-NOD-420; FR-PKG-071,
//! -072; SCN-001).
//!
//! PH-1 form of a genesis Buk (the Buk format of FR-PKG-011 follows in PH-2): a manifest
//! `{name, version, publisher, description, classes}` and the platform publisher's Ed25519
//! signature over its content hash, both embedded in the binary (`genesis/*.json` and
//! `genesis/*.sig`).
//!
//! * `system`: the platform classes `Subscription`, `Job`, `Schedule`, `Tenant` and `Buk`.
//! * `ontology`: the party–role core `Party` (abstract), `Person`, `Organisation`, `Role`
//!   and `PartyRole`; the full content of FR-MODEL-091 follows in PH-2.
//!
//! [`install`] verifies the signature before it writes anything (`PKG.SIGNATURE_INVALID`
//! leaves the store as it was), declares the classes the VAE lacks, refuses a class of the
//! same name in a foreign namespace (`PKG.NAMESPACE_VIOLATION`), and records the Buk as a
//! `Buk` object on `main` by one signed platform commit. A Buk recorded with the same
//! content hash is installed already: the second installation changes nothing, and an
//! interrupted one completes (FR-PKG-072).
//!
//! The PH-1 signatures are made with the development publisher key (its seed is derived
//! from a public text, like the development root key); the platform publisher's production
//! key and its signing ceremony come with the release process.

use serde_json::{Value, json};
use ubos_dvm::crypto;
use ubos_dvm::host::Vae;
use ubos_dvm::types::{BpaError, content_id};

/// The `system` Buk's manifest.
pub const SYSTEM: &str = include_str!("../genesis/system.json");
/// Its signature.
pub const SYSTEM_SIG: &str = include_str!("../genesis/system.sig");
/// The `ontology` Buk's manifest.
pub const ONTOLOGY: &str = include_str!("../genesis/ontology.json");
/// Its signature.
pub const ONTOLOGY_SIG: &str = include_str!("../genesis/ontology.sig");

/// The text the development publisher key's seed is derived from.
pub const DEV_PUBLISHER: &str = "ubos-development-publisher-key";

/// The development publisher key's seed (PH-1 genesis signatures).
pub fn dev_publisher_seed() -> [u8; 32] {
    let h = content_id("ubos.chunk.v1", &json!({"publisher": DEV_PUBLISHER})).unwrap_or_default();
    let hex = h.trim_start_matches("sha256:");
    crypto::unhex32(&json!(hex), "seed").unwrap_or([0u8; 32])
}

/// The publisher key the node trusts for genesis Buks (hex).
pub fn publisher_key() -> String {
    hex(&crypto::ed25519_public(&dev_publisher_seed()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn invalid(m: impl Into<String>) -> BpaError {
    BpaError::new("PKG.MANIFEST_INVALID", m.into())
}

/// A genesis Buk: its manifest, content hash and signature.
#[derive(Debug, Clone)]
pub struct Buk {
    /// The manifest.
    pub manifest: Value,
    /// Its content hash.
    pub hash: String,
    /// The signature `{alg, key, sig}`.
    pub signature: Value,
}

impl Buk {
    /// A Buk from its manifest text and signature text.
    pub fn parse(manifest: &str, signature: &str) -> Result<Buk, BpaError> {
        let manifest: Value =
            serde_json::from_str(manifest).map_err(|e| invalid(format!("manifest: {e}")))?;
        for k in ["name", "version", "publisher"] {
            if !manifest[k].is_string() {
                return Err(invalid(format!("the manifest names its {k}")));
            }
        }
        if !manifest["classes"].is_array() {
            return Err(invalid("the manifest lists its classes"));
        }
        let signature: Value = serde_json::from_str(signature).unwrap_or(Value::Null);
        let hash = content_id("ubos.chunk.v1", &json!({"buk": manifest}))?;
        Ok(Buk {
            manifest,
            hash,
            signature,
        })
    }

    /// The name.
    pub fn name(&self) -> &str {
        self.manifest["name"].as_str().unwrap_or_default()
    }

    /// The version.
    pub fn version(&self) -> &str {
        self.manifest["version"].as_str().unwrap_or_default()
    }

    /// Verify the signature against the trusted publisher key (`PKG.SIGNATURE_INVALID`).
    pub fn verify(&self) -> Result<(), BpaError> {
        let bad = |m: &str| {
            BpaError::with_details(
                "PKG.SIGNATURE_INVALID",
                json!({"buk": self.name(), "reason": m}),
            )
        };
        if self.signature["alg"] != "ed25519" {
            return Err(bad("not an ed25519 signature"));
        }
        let key = self.signature["key"].as_str().unwrap_or_default();
        if key != publisher_key() {
            return Err(bad("not signed by the platform publisher"));
        }
        let (Some(k), Some(s)) = (unhex(key), self.signature["sig"].as_str().and_then(unhex))
        else {
            return Err(bad("malformed signature"));
        };
        let (Ok(k), Ok(s)) = (<[u8; 32]>::try_from(k), <[u8; 64]>::try_from(s)) else {
            return Err(bad("malformed signature"));
        };
        if !crypto::ed25519_verify(&k, self.hash.as_bytes(), &s) {
            return Err(bad("the signature does not match the content"));
        }
        Ok(())
    }

    /// A class declaration of the Buk.
    pub fn class(&self, name: &str) -> Option<Value> {
        self.manifest["classes"]
            .as_array()?
            .iter()
            .find(|c| c["name"] == json!(name))
            .cloned()
    }
}

/// Sign a manifest with a publisher seed: the signature text of `genesis/*.sig`.
pub fn sign(manifest: &str, seed: &[u8; 32]) -> Result<String, BpaError> {
    let b = Buk::parse(manifest, "{}")?;
    let sig = crypto::ed25519_sign(seed, b.hash.as_bytes());
    Ok(serde_json::to_string_pretty(
        &json!({"alg": "ed25519", "key": hex(&crypto::ed25519_public(seed)), "sig": hex(&sig), "content_hash": b.hash}),
    )
    .unwrap_or_default()
        + "\n")
}

/// The embedded genesis Buks, in installation order (`system`, then `ontology`).
pub fn embedded() -> Result<Vec<Buk>, BpaError> {
    Ok(vec![
        Buk::parse(SYSTEM, SYSTEM_SIG)?,
        Buk::parse(ONTOLOGY, ONTOLOGY_SIG)?,
    ])
}

/// The embedded `system` Buk (the platform classes).
pub fn system() -> Buk {
    Buk::parse(SYSTEM, SYSTEM_SIG).expect("the embedded system Buk parses")
}

/// The object identifier of a Buk's record on `main`.
pub fn record_id(name: &str) -> Result<String, BpaError> {
    let h = content_id("ubos.chunk.v1", &json!({"buk_record": name}))?;
    let x: String = h.trim_start_matches("sha256:").chars().take(32).collect();
    Ok(format!(
        "{}-{}-8{}-8{}-{}",
        &x[0..8],
        &x[8..12],
        &x[13..16],
        &x[17..20],
        &x[20..32]
    ))
}

/// Install a Buk into a VAE (FR-PKG-071, -072): `{buk, version, status}` where `status`
/// is `installed`, or `present` when the same content is recorded already.
pub fn install(vae: &mut Vae, buk: &Buk) -> Result<Value, BpaError> {
    buk.verify()?;
    let rid = record_id(buk.name())?;
    if let Ok(r) = vae.call("read", &json!({"branch": "main", "object_id": rid})) {
        let b = &r["body"];
        if b["content_hash"] == json!(buk.hash) {
            return Ok(json!({"buk": buk.name(), "version": buk.version(), "status": "present"}));
        }
        return Err(BpaError::with_details(
            "PKG.MANIFEST_INVALID",
            json!({"buk": buk.name(), "reason": "another version is installed; genesis upgrades come in PH-2 (FR-PKG-073)",
                "installed": b["version"]}),
        ));
    }
    // the classes the VAE lacks; one of the same name in a foreign namespace is refused
    let mut missing = Vec::new();
    for c in buk.manifest["classes"].as_array().into_iter().flatten() {
        let name = c["name"].as_str().unwrap_or_default();
        match vae.engine_mut().registry.decl(name) {
            Ok(d) if d["namespace"] == c["namespace"] => {}
            Ok(d) => {
                return Err(BpaError::with_details(
                    "PKG.NAMESPACE_VIOLATION",
                    json!({"buk": buk.name(), "class": name, "namespace": d["namespace"]}),
                ));
            }
            Err(_) => missing.push(c.clone()),
        }
    }
    if !missing.is_empty() {
        vae.call("load_genesis", &json!({"classes": missing}))?;
    }
    vae.call(
        "system_commit",
        &json!({"branch": "main", "changes": [{"op": "create", "class": "Buk", "object_id": rid, "slug": format!("buk-{}", buk.name()),
            "body": {"name": buk.name(), "version": buk.version(), "content_hash": buk.hash,
                "publisher": buk.manifest["publisher"], "signature": buk.signature["sig"]}}],
            "process": {"kind": "import", "message": format!("genesis Buk {} {}", buk.name(), buk.version())}}),
    )?;
    Ok(json!({"buk": buk.name(), "version": buk.version(), "status": "installed"}))
}

/// Install the embedded genesis Buks into a VAE, `system` first.
pub fn install_all(vae: &mut Vae) -> Result<Vec<Value>, BpaError> {
    embedded()?.iter().map(|b| install(vae, b)).collect()
}
