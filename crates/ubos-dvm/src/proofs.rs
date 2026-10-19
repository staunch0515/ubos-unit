//! Node identity and signed commit records of the adapter (STD-PROOF-010, -011,
//! STD-ADDR-010; PH-1 step 6a, DEC-059). The pure proof functions live in `ubos-proof`;
//! this module reads the store for them.

use crate::stateful::Engine;
use serde_json::{Value, json};
use ubos_store::node;
use ubos_types::BpaError;
use ubos_version::{Repo, branch_kind, hlc};

fn not_found(m: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", m)
}

impl Engine {
    /// `did_document` (STD-ADDR-010): derived for a key-form DID; the node's document for
    /// its registered DID; `IAM.NOT_FOUND` for any other registered DID.
    pub fn did_document(&mut self, did: &Value) -> Result<Value, BpaError> {
        let p = ubos_types::addr::parse_did(did)?;
        if p["form"] == "key" {
            return ubos_types::addr::key_document(did);
        }
        let doc = node::did(self.store.as_mut())?;
        if *did != doc["id"] {
            return Err(BpaError::new(
                "IAM.NOT_FOUND",
                format!("no DID document for {}", did.as_str().unwrap_or_default()),
            ));
        }
        Ok(doc)
    }

    /// `rotate_node_key` (STD-PROOF-010): the current key ends, and the new one starts, at
    /// the clock reading after the latest transaction time of any commit, so that every
    /// signed commit record stays verifiable at its `tx_time` (STD-PROOF-011).
    pub fn rotate_node_key(&mut self, seed: &Value) -> Result<Value, BpaError> {
        ubos_crypto::unhex32(seed, "seed")?;
        let mut latest: Option<String> = None;
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        for name in repo.store.branch_names()? {
            let snap = repo.select(&name, &Value::Null)?;
            let tx = repo.meta(snap.commit)?.tx_time;
            if tx > latest {
                latest = tx;
            }
        }
        let at = hlc(&self.now, latest.as_deref())?;
        node::rotate(self.store.as_mut(), seed, &at)
    }

    /// `commit_record` (STD-PROOF-011): the signed record of the selected commit of a branch.
    pub fn commit_record(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = i.get("branch").and_then(Value::as_str).unwrap_or("main");
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, i)?;
        repo.commit_record(snap.commit)
    }

    /// `prove` (STD-PROOF-020): the inclusion proof of an object or an entry in the state
    /// tree of the selected commit of a branch, with the signed commit record.
    pub fn prove(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = i.get("branch").and_then(Value::as_str).unwrap_or("main");
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, i)?;
        let st = repo.commit_state(snap.commit)?;
        let ents = repo.state_entries(&st)?;
        let key = if let Some(oid) = i.get("object_id").and_then(Value::as_str) {
            format!("o/{oid}")
        } else {
            let eid = i
                .get("entry_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let e = st
                .find_entry(eid)
                .ok_or_else(|| not_found("unknown entry"))?;
            ents.iter()
                .find(|(k, h)| *h == e.entry_hash && k.starts_with("l/"))
                .map(|(k, _)| k.clone())
                .ok_or_else(|| not_found("unknown entry"))?
        };
        if !ents.iter().any(|(k, _)| *k == key) {
            return Err(not_found("not in the state tree of the branch"));
        }
        let mut proof = ubos_proof::inclusion_proof(&ents, &key)?;
        let mut record = repo.commit_record(snap.commit)?;
        if let Some(r) = record.as_object_mut() {
            r.remove("commit_id");
        }
        proof["commit"] = record;
        Ok(json!({"key": key, "proof": proof}))
    }

    /// `consistency_proof` (STD-PROOF-021): the signed commit records from the later commit
    /// back to the earlier one along first parents.
    pub fn consistency_proof(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = i.get("branch").and_then(Value::as_str).unwrap_or("main");
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let b = repo.commit_at(branch, i.get("to").and_then(Value::as_u64))?;
        let a = repo.commit_at(branch, i.get("from").and_then(Value::as_u64))?;
        if repo.meta(a)?.cseq > repo.meta(b)?.cseq {
            return Err(not_found("the earlier commit is after the later one"));
        }
        let mut records = Vec::new();
        for c in repo.first_parents(b)? {
            let mut r = repo.commit_record(c)?;
            if let Some(o) = r.as_object_mut() {
                o.remove("commit_id");
            }
            records.push(r);
            if c == a {
                break;
            }
        }
        Ok(json!({"length": records.len(), "records": records,
            "from_commit": repo.meta(a)?.commit_id, "to_commit": repo.meta(b)?.commit_id}))
    }

    /// `tenant_root` (STD-PROOF-030): the root over the heads of the active main, release
    /// and overlay branches, named `<vae>/<branch>`, signed by the node.
    pub fn tenant_root(&mut self) -> Result<Value, BpaError> {
        let now = self.now.clone();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let vae = repo.store.authority();
        let mut names = Vec::new();
        let mut heads = Vec::new();
        for name in repo.store.branch_names()? {
            if !matches!(branch_kind(&name), "main" | "release" | "overlay")
                || repo.branch_state(&name, &now)? != "active"
            {
                continue;
            }
            let head = repo.head(&name)?;
            heads.push((format!("{vae}/{name}"), repo.meta(head)?.commit_id));
            names.push(name);
        }
        let root = ubos_tree::tree_root(&heads)?;
        let heads: Vec<Value> = heads.iter().map(|(k, v)| json!([k, v])).collect();
        Ok(json!({"branches": names, "heads": heads, "root": root,
            "signature": node::sign(self.store.as_mut(), &root, "node")?}))
    }

    /// `export_archive` (STD-PROOF-070).
    pub fn export_archive(&mut self) -> Result<Value, BpaError> {
        let decls = self.registry.declarations();
        Repo::new(self.store.as_mut(), &self.registry).export_archive(&decls)
    }

    /// `import_archive` (STD-PROOF-070): verify the archive without the store
    /// (`ubos-proof`), then load it into the empty store with every state root recomputed.
    pub fn import_archive(&mut self, archive: &Value) -> Result<Value, BpaError> {
        let empty = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let head = repo.head("main")?;
            repo.store.branch_names()? == ["main"] && repo.meta(head)?.cseq == 0
        } && self.registry.declarations().is_empty();
        if !empty {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "an archive is imported into an empty store",
            ));
        }
        ubos_proof::verify_archive(archive)?;
        let chunks = archive.get("chunks").and_then(Value::as_object);
        let classes: Vec<Value> = archive["manifest"]["classes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                chunks
                    .and_then(|ch| ch.get(c.as_str().unwrap_or_default()))
                    .cloned()
            })
            .collect();
        let registry = crate::expressions::registry().load(&classes, None)?;
        self.store.reset()?;
        self.registry = registry;
        self.instances.clear();
        self.branch_cache.clear();
        let out = Repo::new(self.store.as_mut(), &self.registry).import_archive(archive)?;
        for c in archive["commits"].as_object().into_iter().flatten() {
            for o in c.1["objects"].as_object().into_iter().flatten() {
                if let Some(cls) = o.1.get("class").and_then(Value::as_str) {
                    self.instances.insert(cls.to_string());
                }
            }
        }
        self.mark_ref_rows()?;
        Ok(out)
    }
}
