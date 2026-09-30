//! Stored prolly-tree nodes: full build, batch update and diff (DSN-DVM-007).
//!
//! Chunk boundaries depend on the key alone ([`is_boundary`]), so a node at level `l` holds
//! the keys after one boundary key up to and including the next. A batch of key changes
//! therefore rewrites only the nodes whose key range contains a changed key, plus a
//! following node when a closing boundary key disappears; the parent level receives the
//! replaced and new node entries as its own batch. The result equals [`tree_root`] over the
//! changed entry set (VER-PROP-0004).
//!
//! [`tree_root`]: crate::tree_root

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde_json::{Value, json};
use ubos_types::BpaError;

use crate::{is_boundary, node_hash};

/// A tree node: its level and its `[key, hash]` entries, sorted by key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// 0 for leaves.
    pub level: u32,
    /// Leaf entries `[key, value hash]` or child entries `[first key, node hash]`.
    pub entries: Vec<(String, String)>,
}

impl Node {
    /// The node as stored: `{"level": l, "entries": [[k, h], …]}`.
    pub fn to_json(&self) -> Value {
        let e: Vec<Value> = self.entries.iter().map(|(k, h)| json!([k, h])).collect();
        json!({"level": self.level, "entries": e})
    }

    /// A node from its stored form.
    pub fn from_json(v: &Value) -> Result<Node, BpaError> {
        let bad = || BpaError::new("NOD.STORAGE_UNAVAILABLE", "malformed tree node");
        let level = v["level"]
            .as_u64()
            .and_then(|l| u32::try_from(l).ok())
            .ok_or_else(bad)?;
        let entries = v["entries"]
            .as_array()
            .ok_or_else(bad)?
            .iter()
            .map(|e| match (e[0].as_str(), e[1].as_str()) {
                (Some(k), Some(h)) => Ok((k.to_string(), h.to_string())),
                _ => Err(bad()),
            })
            .collect::<Result<_, _>>()?;
        Ok(Node { level, entries })
    }

    /// The node hash (STD-PROOF-001).
    pub fn hash(&self) -> Result<String, BpaError> {
        node_hash(self.level, &self.entries)
    }
}

/// Where stored nodes are read from.
pub trait NodeSource {
    /// The node with this hash.
    fn node(&mut self, hash: &str) -> Result<Node, BpaError>;
}

/// Nodes held in memory, for tests and for building before a store write.
#[derive(Default, Debug, Clone)]
pub struct MemNodes {
    /// Nodes by hash.
    pub nodes: HashMap<String, Node>,
}

impl MemNodes {
    /// Add nodes.
    pub fn extend(&mut self, nodes: impl IntoIterator<Item = (String, Node)>) {
        self.nodes.extend(nodes);
    }
}

impl NodeSource for MemNodes {
    fn node(&mut self, hash: &str) -> Result<Node, BpaError> {
        self.nodes
            .get(hash)
            .cloned()
            .ok_or_else(|| BpaError::new("NOD.STORAGE_UNAVAILABLE", "missing tree node"))
    }
}

/// Result of a build or an update: the root and the nodes to store.
#[derive(Debug, Clone, Default)]
pub struct Written {
    /// Root hash.
    pub root: String,
    /// New nodes (possibly already stored; stores deduplicate by hash).
    pub nodes: Vec<(String, Node)>,
}

/// Chunk sorted entries of one level into nodes by the boundary rule; the last node may
/// end without a boundary key.
fn chunk(level: u32, entries: Vec<(String, String)>) -> Vec<Vec<(String, String)>> {
    let mut nodes = Vec::new();
    let mut current = Vec::new();
    for (k, h) in entries {
        let closes = is_boundary(level, &k);
        current.push((k, h));
        if closes {
            nodes.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        nodes.push(current);
    }
    nodes
}

/// Chunk sorted entries whose boundary flags are known.
fn chunk_flagged(entries: Vec<(String, String, bool)>) -> Vec<Vec<(String, String)>> {
    let mut nodes = Vec::new();
    let mut current = Vec::new();
    for (k, h, closes) in entries {
        current.push((k, h));
        if closes {
            nodes.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        nodes.push(current);
    }
    nodes
}

/// Entries of a stored node with their boundary flags: every entry but the last is not a
/// boundary, and the last one is unless the node ends its level.
fn flagged(level: u32, entries: Vec<(String, String)>) -> Vec<(String, String, bool)> {
    let last = entries.len().saturating_sub(1);
    let closes = entries.last().is_some_and(|(k, _)| is_boundary(level, k));
    entries
        .into_iter()
        .enumerate()
        .map(|(i, (k, h))| (k, h, i == last && closes))
        .collect()
}

/// Build levels from `level` upward until one node remains.
fn build_from(
    mut level: u32,
    mut items: Vec<(String, String)>,
    out: &mut Vec<(String, Node)>,
) -> Result<String, BpaError> {
    if items.is_empty() {
        let n = Node {
            level: 0,
            entries: vec![],
        };
        let h = n.hash()?;
        out.push((h.clone(), n));
        return Ok(h);
    }
    loop {
        let chunks = chunk(level, items);
        let mut next = Vec::with_capacity(chunks.len());
        for c in chunks {
            let first = c[0].0.clone();
            let n = Node { level, entries: c };
            let h = n.hash()?;
            out.push((h.clone(), n));
            next.push((first, h));
        }
        if next.len() == 1 {
            return Ok(next.remove(0).1);
        }
        items = next;
        level += 1;
    }
}

/// Build the tree over `[key, hash]` entries; the root equals [`crate::tree_root`].
pub fn build(entries: &[(String, String)]) -> Result<Written, BpaError> {
    let mut items = entries.to_vec();
    items.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    if items.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(BpaError::new("MODEL.INVALID_VALUE", "duplicate tree key"));
    }
    let mut nodes = Vec::new();
    let root = build_from(0, items, &mut nodes)?;
    Ok(Written { root, nodes })
}

/// Nodes read during one update, with the nodes created by it.
struct Reader<'a> {
    src: &'a mut dyn NodeSource,
    cache: HashMap<String, Rc<Node>>,
}

impl Reader<'_> {
    fn get(&mut self, hash: &str) -> Result<Rc<Node>, BpaError> {
        if let Some(n) = self.cache.get(hash) {
            return Ok(n.clone());
        }
        let n = Rc::new(self.src.node(hash)?);
        self.cache.insert(hash.to_string(), n.clone());
        Ok(n)
    }
}

/// Path from the root to a node: `(node, chosen entry index)` for each level above it.
type Path = Vec<(Rc<Node>, usize)>;

/// Index of the entry covering `key`: the last entry whose key is not greater, else 0.
fn covering(entries: &[(String, String)], key: &str) -> usize {
    entries
        .partition_point(|(k, _)| k.as_bytes() <= key.as_bytes())
        .saturating_sub(1)
}

/// The node at `level` whose key range holds `key`, with its path.
fn descend(
    r: &mut Reader,
    root: &Rc<Node>,
    key: &str,
    level: u32,
) -> Result<(Rc<Node>, Path), BpaError> {
    let mut path = Vec::new();
    let mut node = root.clone();
    while node.level > level {
        let i = covering(&node.entries, key);
        let child = r.get(&node.entries[i].1)?;
        path.push((node, i));
        node = child;
    }
    Ok((node, path))
}

/// The next node of the same level after the node reached by `path`.
fn successor(r: &mut Reader, path: &Path) -> Result<Option<(Rc<Node>, Path)>, BpaError> {
    let mut p = path.clone();
    while let Some((node, i)) = p.pop() {
        if i + 1 < node.entries.len() {
            let mut child = r.get(&node.entries[i + 1].1)?;
            p.push((node, i + 1));
            // Descend leftmost to the original level (path length is the depth).
            while p.len() < path.len() {
                let next = r.get(&child.entries[0].1)?;
                p.push((child, 0));
                child = next;
            }
            return Ok(Some((child, p)));
        }
    }
    Ok(None)
}

/// Apply sorted changes to sorted entries.
fn merge_changes(
    entries: Vec<(String, String)>,
    changes: &[(String, Option<String>)],
) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(entries.len() + changes.len());
    let mut it = changes.iter().peekable();
    for (k, h) in entries {
        while let Some((ck, cv)) = it.peek() {
            if ck.as_bytes() < k.as_bytes() {
                if let Some(v) = cv {
                    out.push((ck.clone(), v.clone()));
                }
                it.next();
            } else {
                break;
            }
        }
        match it.peek() {
            Some((ck, cv)) if *ck == k => {
                if let Some(v) = cv {
                    out.push((k, v.clone()));
                }
                it.next();
            }
            _ => out.push((k, h)),
        }
    }
    for (ck, cv) in it {
        if let Some(v) = cv {
            out.push((ck.clone(), v.clone()));
        }
    }
    out
}

/// Apply sorted changes to flagged entries; a key keeps its flag, a new key gets its own.
fn merge_flagged(
    level: u32,
    entries: Vec<(String, String, bool)>,
    changes: &[(String, Option<String>)],
) -> Vec<(String, String, bool)> {
    let mut out = Vec::with_capacity(entries.len() + changes.len());
    let mut it = changes.iter().peekable();
    for (k, h, b) in entries {
        while let Some((ck, cv)) = it.peek() {
            if ck.as_bytes() < k.as_bytes() {
                if let Some(v) = cv {
                    out.push((ck.clone(), v.clone(), is_boundary(level, ck)));
                }
                it.next();
            } else {
                break;
            }
        }
        match it.peek() {
            Some((ck, cv)) if *ck == k => {
                if let Some(v) = cv {
                    out.push((k, v.clone(), b));
                }
                it.next();
            }
            _ => out.push((k, h, b)),
        }
    }
    for (ck, cv) in it {
        if let Some(v) = cv {
            out.push((ck.clone(), v.clone(), is_boundary(level, ck)));
        }
    }
    out
}

/// Rewrite the affected nodes of one level below the root; returns the changes for the
/// parent level.
fn rewrite_level(
    r: &mut Reader,
    root: &Rc<Node>,
    level: u32,
    changes: &BTreeMap<String, Option<String>>,
    out: &mut Vec<(String, Node)>,
) -> Result<BTreeMap<String, Option<String>>, BpaError> {
    // Affected nodes by first key, with their paths and the changes they cover.
    type Affected = (Rc<Node>, Path, Vec<(String, Option<String>)>);
    let mut affected: BTreeMap<String, Affected> = BTreeMap::new();
    for (k, v) in changes {
        let (node, path) = descend(r, root, k, level)?;
        let first = node.entries[0].0.clone();
        affected
            .entry(first)
            .or_insert_with(|| (node, path, Vec::new()))
            .2
            .push((k.clone(), v.clone()));
    }
    let mut parent: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut created: Vec<(String, String)> = Vec::new();
    let mut buffer: Vec<(String, String, bool)> = Vec::new();
    let mut current: Option<Affected> = None;
    loop {
        let (node, path, ch) = match current.take() {
            Some(a) => a,
            None => match affected.pop_first() {
                Some((_, a)) => a,
                None => break,
            },
        };
        parent.insert(node.entries[0].0.clone(), None);
        buffer.extend(merge_flagged(
            level,
            flagged(level, node.entries.clone()),
            &ch,
        ));
        let open = buffer.last().is_some_and(|e| !e.2);
        let mut chunks = chunk_flagged(std::mem::take(&mut buffer));
        if open {
            let tail: Vec<(String, String, bool)> = chunks
                .pop()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, h)| (k, h, false))
                .collect();
            match successor(r, &path)? {
                Some((next, next_path)) => {
                    let first = next.entries[0].0.clone();
                    let ch = affected.remove(&first).map(|a| a.2).unwrap_or_default();
                    current = Some((next, next_path, ch));
                    buffer = tail;
                }
                None => chunks.push(tail.into_iter().map(|(k, h, _)| (k, h)).collect()),
            }
        }
        for c in chunks {
            let first = c[0].0.clone();
            let n = Node { level, entries: c };
            let h = n.hash()?;
            out.push((h.clone(), n));
            created.push((first, h));
        }
    }
    for (k, h) in created {
        parent.insert(k, Some(h));
    }
    Ok(parent)
}

/// Apply key changes (`Some(hash)` to set, `None` to remove) to the tree with root `root`.
pub fn update(
    src: &mut dyn NodeSource,
    root: &str,
    changes: &BTreeMap<String, Option<String>>,
) -> Result<Written, BpaError> {
    let mut r = Reader {
        src,
        cache: HashMap::new(),
    };
    let top = r.get(root)?;
    let mut nodes: Vec<(String, Node)> = Vec::new();
    if changes.is_empty() {
        return Ok(Written {
            root: root.to_string(),
            nodes,
        });
    }
    let mut level = 0;
    let mut ch = changes.clone();
    while level < top.level {
        ch = rewrite_level(&mut r, &top, level, &ch, &mut nodes)?;
        level += 1;
    }
    let pairs: Vec<(String, Option<String>)> = ch.into_iter().collect();
    let items = merge_changes(top.entries.clone(), &pairs);
    let mut root = if items.is_empty() {
        build_from(0, items, &mut nodes)?
    } else {
        build_from(top.level, items, &mut nodes)?
    };
    // A root above the leaves with a single entry is replaced by its child.
    let made: HashMap<String, Node> = nodes.iter().cloned().collect();
    loop {
        let n = match made.get(&root) {
            Some(n) => Rc::new(n.clone()),
            None => r.get(&root)?,
        };
        if n.level > 0 && n.entries.len() == 1 {
            root = n.entries[0].1.clone();
        } else {
            break;
        }
    }
    Ok(Written { root, nodes })
}

/// Entries of a set of nodes one level down.
fn expand(r: &mut Reader, items: &[(String, String)]) -> Result<Vec<(String, String)>, BpaError> {
    let mut out = Vec::new();
    for (_, h) in items {
        out.extend(r.get(h)?.entries.iter().cloned());
    }
    Ok(out)
}

/// One differing key of a tree diff: key, value in the first tree, value in the second.
pub type Change = (String, Option<String>, Option<String>);

/// Key-wise difference of two trees: `(key, value in a, value in b)` for every key whose
/// value differs, sorted by key. Subtrees with equal hashes are skipped.
pub fn diff(src: &mut dyn NodeSource, a: &str, b: &str) -> Result<Vec<Change>, BpaError> {
    let mut r = Reader {
        src,
        cache: HashMap::new(),
    };
    if a == b {
        return Ok(Vec::new());
    }
    let na = r.get(a)?;
    let nb = r.get(b)?;
    let (mut la, mut lb) = (na.level, nb.level);
    let (mut ea, mut eb) = (na.entries.clone(), nb.entries.clone());
    while la > lb {
        ea = expand(&mut r, &ea)?;
        la -= 1;
    }
    while lb > la {
        eb = expand(&mut r, &eb)?;
        lb -= 1;
    }
    loop {
        let sa: BTreeMap<String, String> = ea.into_iter().collect();
        let sb: BTreeMap<String, String> = eb.into_iter().collect();
        let only_a: Vec<(String, String)> = sa
            .iter()
            .filter(|(k, h)| sb.get(*k) != Some(*h))
            .map(|(k, h)| (k.clone(), h.clone()))
            .collect();
        let only_b: Vec<(String, String)> = sb
            .iter()
            .filter(|(k, h)| sa.get(*k) != Some(*h))
            .map(|(k, h)| (k.clone(), h.clone()))
            .collect();
        if la == 0 {
            let ma: BTreeMap<String, String> = only_a.into_iter().collect();
            let mb: BTreeMap<String, String> = only_b.into_iter().collect();
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
            keys.dedup();
            return Ok(keys
                .into_iter()
                .map(|k| (k.clone(), ma.get(k).cloned(), mb.get(k).cloned()))
                .collect());
        }
        ea = expand(&mut r, &only_a)?;
        eb = expand(&mut r, &only_b)?;
        la -= 1;
    }
}
