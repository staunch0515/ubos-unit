//! `ubos-store` — engine layer (UBS-ARC-07 §2).
//!
//! Storage traits; the PostgreSQL adapter (SQLite follows in PH-4, DEC-024); chunk store;
//! kernel index maintenance. Allowed dependencies: `ubos-types`, `ubos-crypto` (AR-031).
//!
//! PH-0 prototype scope: DSN-DVM-001…011 (UBS-PH-0 §3, DEC-030 step 4).
