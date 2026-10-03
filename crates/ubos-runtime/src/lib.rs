//! `ubos-runtime` — engine layer (UBS-ARC-07 §2): the instruction set of ABI 1.0 and its
//! gateway (STD-ISA-001…007, STD-CTX-003), the execution context (STD-CTX-001, -002), the
//! profiles, metering and termination (STD-CTX-004…006), deterministic entropy and the
//! journal records of a run (STD-CTX-010), the interpreter of host-tier programs, the
//! in-process host binding of STD-ISA-007 (PH-1 step 5b, DEC-056), and the L2 host of
//! STD-CTX-020 with per-execution memory accounting (step 5d, DEC-058).
//!
//! The store-facing half of every instruction is supplied by the engine through the
//! [`Host`] trait; this crate holds only what is pure: the catalogue, the checks of the
//! gateway, the budget, the program state, the statements and the script engine. Allowed
//! dependency: `ubos-expr` (AR-031); the model and type layers are reached through it.

pub mod isa;
pub mod l2;
pub mod memory;
pub mod program;

pub use isa::Instruction;
pub use l2::Sizes;
pub use program::{Core, Frame, Host, Pending, Replay};
