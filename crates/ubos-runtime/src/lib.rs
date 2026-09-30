//! Prototype L2 host of `ubos-runtime` (UBS-ARC-07): a Rhai engine hardened as
//! STD-CTX-020 and DSN-DVM-612 require.
//!
//! This is the PH-0 sandbox spike (EXIT-0-06, VER-SEC-0001, DEC-034). It carries only
//! what the escape suite needs: execution profiles and metering (STD-CTX-004,
//! STD-CTX-005), an immutable `ctx`, a `sys` gateway with journaled entropy
//! (STD-CTX-010), typed object handles generated from a class definition, and the
//! mapping of every engine failure to a registered `LOGIC.*` code (UBS-STD-17).
//! The instruction set, WASM host, journal persistence and replay are PH-1 work.

mod sandbox;

pub use sandbox::{Context, Failure, FieldType, Limits, Outcome, Sandbox};
