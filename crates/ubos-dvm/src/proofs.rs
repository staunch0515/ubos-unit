//! Node identity and signed commit records of the adapter (STD-PROOF-010, -011,
//! STD-ADDR-010; PH-1 step 6a, DEC-059). The pure proof functions live in `ubos-proof`;
//! this module reads the store for them.

use crate::stateful::Engine;
use serde_json::Value;
use ubos_store::node;
use ubos_types::BpaError;
use ubos_version::{Repo, hlc};

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
            let tx = repo.commit(snap.commit)?.tx_time;
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
}
