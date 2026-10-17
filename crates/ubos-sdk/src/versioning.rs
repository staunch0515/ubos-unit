//! Versioning operations and introspection (IF-SDK-004; BPA 1.0-rc4, STD-PROTO-014, -015;
//! PH-1 step 18c, DEC-079): branches, change sets, merges and their previews, reverts and
//! bulk processes as mutations with idempotency keys and the retry policy; the effective
//! class, port dispatch, commit records, ledger entries, process records and replays as
//! queries at the coordinates of [`Selectors`].

use crate::error::Result;
use crate::ops::{CallOptions, MutateResult, Selectors};
use crate::session::Client;
use serde_json::{Value, json};

/// A change set action (STD-VER-020, -021).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeSetAction {
    /// The state of the change set.
    Get,
    /// Submit for approval (the merge preview must be clean).
    Submit,
    /// Approve as the session's principal.
    Approve,
    /// Reject as the session's principal.
    Reject,
    /// Merge an approved change set.
    Merge,
    /// Abandon an open or submitted change set.
    Abandon,
}

impl ChangeSetAction {
    fn name(self) -> &'static str {
        match self {
            ChangeSetAction::Get => "get",
            ChangeSetAction::Submit => "submit",
            ChangeSetAction::Approve => "approve",
            ChangeSetAction::Reject => "reject",
            ChangeSetAction::Merge => "merge",
            ChangeSetAction::Abandon => "abandon",
        }
    }
}

impl Client {
    /// Create branch `name` from branch `from` at its head (`branch.create`).
    pub async fn create_branch(
        &self,
        name: &str,
        from: &str,
        o: &CallOptions,
    ) -> Result<MutateResult> {
        self.mutate(json!({"create_branch": {"name": name, "from": from}}), o)
            .await
    }

    /// Open change set `id` (a UUID the caller chooses) from `source` into the branch of
    /// the call options (default `main`).
    pub async fn open_change_set(
        &self,
        id: &str,
        source: &str,
        title: &str,
        o: &CallOptions,
    ) -> Result<MutateResult> {
        let mut cs = json!({"action": "open", "id": id, "source": source, "title": title});
        if let Some(b) = &o.branch {
            cs["target"] = json!(b);
        }
        self.mutate(json!({"change_set": cs}), o).await
    }

    /// An action on change set `id`; `approve` and `reject` are made by the session's
    /// principal, `merge` takes optional `resolutions`.
    pub async fn change_set(
        &self,
        id: &str,
        action: ChangeSetAction,
        resolutions: Option<Value>,
        o: &CallOptions,
    ) -> Result<MutateResult> {
        let mut cs = json!({"action": action.name(), "id": id});
        if let Some(r) = resolutions {
            cs["resolutions"] = r;
        }
        self.mutate(json!({"change_set": cs}), o).await
    }

    /// Merge branch `source` into the branch of the call options (`branch.merge`).
    pub async fn merge(
        &self,
        source: &str,
        resolutions: Option<Value>,
        o: &CallOptions,
    ) -> Result<MutateResult> {
        let mut m = json!({"source": source});
        if let Some(r) = resolutions {
            m["resolutions"] = r;
        }
        self.mutate(json!({"merge": m}), o).await
    }

    /// Revert the commits of a process (`{process_id}`), a merged change set
    /// (`{change_set}`) or one commit (`{time}` or `{commit}`) on the branch of the call
    /// options, with `reversal_ids` for ledger entries and `dry_run` (`revert`).
    pub async fn revert(&self, spec: Value, o: &CallOptions) -> Result<MutateResult> {
        self.mutate(json!({"revert": spec}), o).await
    }

    /// A bulk process (`{changes, max_changes?, mode?, staging?}`, or `{resume: process_id}`
    /// with the changes of a failed one) on the branch of the call options (`commit.raw`).
    pub async fn bulk(&self, spec: Value, o: &CallOptions) -> Result<MutateResult> {
        self.mutate(json!({"bulk": spec}), o).await
    }

    async fn inspect(&self, kind: &str, request: Value, sel: &Selectors) -> Result<Value> {
        let mut b = self.body(json!({ kind: request }));
        sel.apply(&mut b);
        self.ask(b).await
    }

    async fn inspect_one(&self, kind: &str, request: Value, sel: &Selectors) -> Result<Value> {
        let r = self.inspect(kind, request, sel).await?;
        Ok(r["items"][0].clone())
    }

    /// The effective class `name` at the coordinates, with the extensions in force.
    pub async fn effective_class(&self, name: &str, sel: &Selectors) -> Result<Value> {
        self.inspect_one("class", json!({"name": name}), sel).await
    }

    /// The class whose implementation of `port` runs for an object of `class`
    /// (`{class, impl}`).
    pub async fn dispatch(&self, class: &str, port: &str, sel: &Selectors) -> Result<Value> {
        self.inspect_one("dispatch", json!({"class": class, "port": port}), sel)
            .await
    }

    /// The commit the coordinates select, with its state root.
    pub async fn commit_info(&self, sel: &Selectors) -> Result<Value> {
        self.inspect_one("commit_info", json!({}), sel).await
    }

    /// The preview of merging the head of `source` into the head of the coordinates' branch.
    pub async fn merge_preview(&self, source: &str, sel: &Selectors) -> Result<Value> {
        self.inspect_one("merge_preview", json!({"source": source}), sel)
            .await
    }

    /// The entries of a ledger key in sequence order, filtered by `asof`.
    pub async fn ledger(&self, class: &str, key: Value, sel: &Selectors) -> Result<Vec<Value>> {
        let r = self
            .inspect("ledger", json!({"class": class, "ledger_key": key}), sel)
            .await?;
        Ok(r["items"].as_array().cloned().unwrap_or_default())
    }

    /// The record of a process, with its journal and metering record when asked.
    pub async fn process(&self, process_id: &str, journal: bool, metering: bool) -> Result<Value> {
        self.inspect_one(
            "process",
            json!({"process_id": process_id, "journal": journal, "metering": metering}),
            &Selectors::default(),
        )
        .await
    }

    /// Re-execute a process from its journal: `{outcome: identical}` or
    /// `{outcome: diverged, divergence}`.
    pub async fn replay(&self, process_id: &str) -> Result<Value> {
        self.inspect_one(
            "replay",
            json!({"process_id": process_id}),
            &Selectors::default(),
        )
        .await
    }
}
