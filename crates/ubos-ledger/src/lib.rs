//! Money and periods of the kernel (UBS-ARC-07 §2, PH-2 step 3, DEC-098).
//!
//! A pure library: the kernel (`ubos-dvm`) integrates it with storage, commits and the
//! adapter operations. It holds general mechanisms only and no finance vocabulary
//! (EXIT-2-04, VER-INSP-0002); charts of accounts and the like are data of the `finance`
//! Buk. Modules are added by the parts of step 3:
//!
//! - 3a: currency reference data, bitemporal FX rates, currency-safe aggregation, business
//!   calendars and cut-offs (DSN-DVM-409, DSN-DVM-410, DSN-DVM-447);
//! - 3b: balanced posting groups and account hierarchies (DSN-DVM-440, DSN-DVM-441);
//! - 3c: periods, locks, close artefacts, reopen, restatement (DSN-DVM-322, -323, -442…-444);
//! - 3d: matching, match records, statement ingestion (DSN-DVM-445, DSN-DVM-446);
//! - 3e: balance projections and their rebuild (DSN-DVM-420, DSN-DVM-423).

pub mod matching;
