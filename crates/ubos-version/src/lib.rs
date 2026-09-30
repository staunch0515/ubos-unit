//! `ubos-version` — engine layer (UBS-ARC-07 §2).
//!
//! Commits, branches, change sets, merge, revert, inheritance resolution.
//! Allowed dependencies: `ubos-model`, `ubos-store`, `ubos-tree` (AR-031).
//!
//! PH-0 prototype scope (UBS-PH-0 §3, DEC-033): branch state with object timelines and
//! ledgers, commit construction with head compare-and-set (DSN-DVM-201, -204, -208), ledger
//! appends, reversals, double entry and periods (STD-KIND-020…025), merge with best common
//! ancestor and virtual base, per-segment three-way merge and ledger append (DSN-DVM-240…244,
//! STD-VER-040…056), diff (STD-VER-030) and the state tree entries (STD-PROOF-001).
//! Commits are written as deltas to the store's indexes with the new state-tree root; reads
//! resolve a branch through its parents at their fork points (PH-0 step 5, DSN-DVM-004…008).

pub mod merge;
pub mod state;

pub use state::{Entry, ObjectState, Repo, Segment, State, Tx, at, lt};
