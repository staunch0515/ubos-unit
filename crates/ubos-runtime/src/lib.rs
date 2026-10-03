//! `ubos-runtime` — engine layer (UBS-ARC-07 §2): the instruction set of ABI 1.0 and its
//! gateway (STD-ISA-001…007, STD-CTX-003), the execution context (STD-CTX-001, -002), the
//! profiles, metering and termination (STD-CTX-004…006), deterministic entropy and the
//! journal records of a run (STD-CTX-010), and the interpreter of host-tier programs, the
//! in-process host binding of STD-ISA-007 (PH-1 step 5b, DEC-056).
//!
//! The store-facing half of every instruction is supplied by the engine through the
//! [`Host`] trait; this crate holds only what is pure: the catalogue, the checks of the
//! gateway, the budget, the program state and the statements. Allowed dependency:
//! `ubos-expr` (AR-031); the model and type layers are reached through it.
//!
//! `sandbox` is the PH-0 Rhai sandbox spike (EXIT-0-06, VER-SEC-0001, DEC-034), kept
//! until the L2 host of step 5d replaces it (W-2…W-5).

pub mod isa;
pub mod program;
mod sandbox;

pub use isa::Instruction;
pub use program::{Core, Frame, Host, Pending, Replay};
pub use sandbox::{Context, Failure, FieldType, Limits, Outcome, Sandbox};
