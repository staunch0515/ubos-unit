//! `ubos-dvm` — engine layer facade (UBS-ARC-07 §2).
//!
//! Host API facade (CTR-001) and host-service traits (CTR-002).
//! Allowed dependencies: the engine crates (AR-031).
//!
//! PH-0 prototype scope (DEC-033): the conformance adapter, which executes the adapter
//! operations of STD-CONF-005 for the runner (`ubos-forge`).

pub mod adapter;
pub mod execution;
pub mod expressions;
pub mod relations;
pub mod stateful;
pub mod transactions;
pub mod verify;
pub mod versioning;

pub use adapter::{Adapter, STATEFUL_OPERATIONS};
pub use stateful::Engine;
pub use ubos_types::{BpaError, canonical, content_id};
