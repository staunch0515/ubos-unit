//! `ubos-dvm` — engine layer facade (UBS-ARC-07 §2).
//!
//! Host API facade (CTR-001) and host-service traits (CTR-002).
//! Allowed dependencies: the engine crates (AR-031).
//!
//! PH-0 prototype scope (DEC-033): the conformance adapter, which executes the adapter
//! operations of STD-CONF-005 for the runner (`ubos-forge`).

pub mod adapter;
pub mod audit;
pub mod authz;
pub mod execution;
pub mod expressions;
pub mod finance;
pub mod flow;
pub mod host;
pub mod keys;
pub mod matching;
pub mod periods;
pub mod process;
pub mod projections;
pub mod proofs;
pub mod relations;
pub mod restate;
pub mod rules;
pub mod sheets;
pub mod stateful;
pub mod transactions;
pub mod ubtp;
pub mod vectors;
pub mod verification;

pub use ubos_authz as authz_engine;
/// The engine crates a host reaches through this facade (AR-031: hosts depend on `ubos-dvm`).
pub use ubos_crypto as crypto;
pub use ubos_expr as expr;
pub use ubos_flow as flow_engine;
pub use ubos_model as model;
pub use ubos_query as query;
pub use ubos_runtime as runtime;
/// Memory probe of the L2 host, for host binaries that install a counting allocator
/// (DSN-DVM-612).
pub use ubos_runtime::memory;
pub use ubos_store as store;
pub use ubos_types as types;
pub mod verify;
pub mod versioning;
pub mod views;

pub use adapter::{Adapter, STATEFUL_OPERATIONS};
pub use stateful::Engine;
pub use ubos_types::{BpaError, canonical, content_id};
