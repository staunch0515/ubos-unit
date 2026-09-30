//! `ubos-tree` — engine layer (UBS-ARC-07 §2).
//!
//! Prolly tree: roots, diffs, inclusion and consistency proofs, have/want.
//! Allowed dependencies: `ubos-types`, `ubos-store` (AR-031).
//!
//! PH-0 prototype scope: the state tree and its root (STD-PROOF-001, -002; DSN-DVM-001…011).
//! A key closes its node at level l when the first two bytes of
//! SHA-256("ubos.tree.boundary.v1" 0x00 decimal(l) 0x00 key), read big-endian, are below
//! 1024; level l+1 holds `[first key, node hash]` of every node of level l.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ubos_types::{BpaError, content_id};

/// Boundary threshold of the first two hash bytes (target fan-out 64).
pub const BOUNDARY_THRESHOLD: u16 = 1024;

/// True when `key` closes its node at `level`.
pub fn is_boundary(level: u32, key: &str) -> bool {
    let mut h = Sha256::new();
    h.update(b"ubos.tree.boundary.v1\x00");
    h.update(level.to_string().as_bytes());
    h.update([0u8]);
    h.update(key.as_bytes());
    let d = h.finalize();
    u16::from_be_bytes([d[0], d[1]]) < BOUNDARY_THRESHOLD
}

/// Hash of a node (`ubos.tree.leaf.v1` at level 0, `ubos.tree.node.v1` above).
pub fn node_hash(level: u32, entries: &[(String, String)]) -> Result<String, BpaError> {
    let list: Vec<Value> = entries.iter().map(|(k, h)| json!([k, h])).collect();
    let domain = if level == 0 {
        "ubos.tree.leaf.v1"
    } else {
        "ubos.tree.node.v1"
    };
    content_id(domain, &json!({"level": level, "entries": list}))
}

/// Root of the prolly tree over `[key, hash]` entries (STD-PROOF-001).
pub fn tree_root(entries: &[(String, String)]) -> Result<String, BpaError> {
    let mut items: Vec<(String, String)> = entries.to_vec();
    items.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    if items.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(BpaError::new("MODEL.INVALID_VALUE", "duplicate tree key"));
    }
    if items.is_empty() {
        return node_hash(0, &[]);
    }
    let mut level = 0u32;
    loop {
        let mut nodes: Vec<Vec<(String, String)>> = Vec::new();
        let mut current = Vec::new();
        for (k, h) in items {
            let closes = is_boundary(level, &k);
            current.push((k, h));
            if closes {
                nodes.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            nodes.push(current);
        }
        if nodes.len() == 1 {
            return node_hash(level, &nodes[0]);
        }
        items = nodes
            .iter()
            .map(|n| Ok((n[0].0.clone(), node_hash(level, n)?)))
            .collect::<Result<_, BpaError>>()?;
        level += 1;
    }
}
