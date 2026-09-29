//! `ubos-version` — engine layer (UBS-ARC-07 §2).
//!
//! Commits, branches, change sets, merge, revert, inheritance resolution.
//! Allowed dependencies: `ubos-model`, `ubos-store`, `ubos-tree` (AR-031).
//!
//! PH-0 prototype scope: commit construction, zero-copy branches and head CAS
//! (DSN-DVM-201, DSN-DVM-204, DSN-DVM-208) and the merge pipeline with ledger append
//! (DSN-DVM-240…244), minimal and sufficient for the BPA 0.1 vectors (UBS-PH-0 §2, §3).
