//! `ubos-forge` — standard layer (UBS-ARC-07 §2).
//!
//! CLI and library: check, types, test, simulate, pack, verify, conform, publish.
//! Allowed dependencies: `ubos-dvm`, `ubos-sdk` (AR-031).
//!
//! PH-0 prototype scope (DSN-FRG-401, DSN-FRG-402, DEC-033): the conformance runner. It
//! executes BPA vectors (STD-CONF-001) through the in-process adapter (STD-CONF-002,
//! STD-CONF-005) and produces a signed `DAT-ConformanceReport` (STD-CONF-003).

pub mod check;
pub mod dev;
pub mod json;
pub mod project;
pub mod records;
pub mod runner;
pub mod test;
pub mod types;
