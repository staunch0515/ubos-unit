---
id: UBS-PHR-0
title: PH-0 — Findings and Dispositions
status: draft
phase: PH-0
depends_on: [UBS-PH-0, UBS-PH-1, DEC-035, DEC-036]
---

# PH-0 — Findings and Dispositions

This document closes the review of VER-INSP-9003: every finding of PH-0 has a disposition
(DEC-036), and the work it creates for PH-1 is listed in §3 and in UBS-PH-1. Finding labels
are those of the reports named in the Source column.

## 1. Exit criteria

| # | Exit | Status | Evidence |
|---|---|---|---|
| 1 | EXIT-0-01 | met: BPA 0.1 released with 551 vectors, MUST coverage 94%; tag `bpa-v0.1.0` signed by the owner | `standard/release/0.1/`, DEC-032 |
| 2 | EXIT-0-02 | met: three models checked per DEC-031, no violation after the fix of F-1 | `verify/models/reports/` |
| 3 | EXIT-0-03 | met: 453 of 453 `prototype` vectors on PostgreSQL 16, signed reports | `verify/reports/exit-0-03/` |
| 4 | EXIT-0-04 | met: VER-PROP-0001…0003 with 1,000,000 cases each, 0 failures | `verify/reports/exit-0-04/` |
| 5 | EXIT-0-05 | carried over to PH-1 (DEC-035); preparation done, local trial only | `verify/reports/exit-0-05/` |
| 6 | EXIT-0-06 | met: 100 of 100 initial L2 escape cases blocked; ASM-008 confirmed for L2 (DEC-034) | `verify/reports/exit-0-06/` |
| 7 | EXIT-0-07 | met by this document, DEC-036 and the re-baselined UBS-PH-1 | this document |

## 2. Findings

| Source | Finding | Disposition |
|---|---|---|
| models F-1 | stale group view in the sequencer fails unrelated processes | fixed in DSN-DVM-208; model re-checked |
| models F-2 | `set_period` lineage undefined (OQ-004) | resolved by DEC-032 (STD-TIME-011) |
| models F-3 | virtual merge base undefined (OQ-005) | resolved by DEC-032 (STD-VER-040) |
| BPA 0.1 | branch kinds differ between STD-VER-013 and DSN-DVM-206 (OQ-006) | resolved by DEC-032 addendum |
| BPA 0.1 | 4 of 72 MUST clauses in scope without a vector range (coverage 94%) | PH-1: 100% for BPA 1.0-rc1 (EXIT-1-01), W-11 |
| prototype | synchronous `postgres` client instead of sqlx (DEC-033) | PH-1: kernel returns to sqlx, W-7 |
| prototype | minimal class registry; adapter operations `linearize` and `effective_class` unsupported | PH-1: W-10 |
| sandbox F-1 | Rhai's core package carries `sleep`, `exit`, `parse_json`, reflection | fixed in DSN-DVM-612 (empty engine, explicit packages) |
| sandbox F-2 | engine size limits do not bound total memory | DSN-DVM-612 amended; PH-1: per-worker memory count, W-2 |
| sandbox F-3 | one unit can be a native call on a large value; time checks every 8 units; metering costs about 4 ns per unit | PH-1: weighted units and a per-thread CPU clock, measured against NR-PERF-010, W-3 |
| sandbox F-4 | Rhai's size checks make element-wise growth quadratic | PH-1: benchmark, then patch or lower defaults, W-4 |
| sandbox F-5 | Rhai's decimal is 96-bit and differs from the kernel decimal (OQ-008) | decided by DEC-036: the kernel decimal becomes the L2 decimal type, W-5 |
| sandbox F-6 | STD-CTX-004 lists no L2 sizes (OQ-007) | decided by DEC-036: sizes added to STD-CTX-004 from BPA 1.0-rc1, W-6 |
| sandbox F-7 | unknown-function errors are catchable by scripts | accepted for L2 (DEC-034); revisited with the full instruction set in PH-1 |
| sandbox F-8 | RSK-004 did not occur | RSK-004 closed (DEC-036) |
| sandbox scope | the WASM half of ASM-008 is not exercised | verified with the L3 host; L3 is out of scope for PH-1 (UBS-PH-1 §5), so this is PH-3 (DEC-036 corrects DEC-034 item 3) |
| tree T-1 | node hashing dominates tree updates on CPUs without SHA extensions | RSK-017; ENV-REF-SERVER requires SHA extensions |
| tree T-2, T-3 | canonical JSON fast path; boundary flags from stored nodes | done in PH-0 |
| layout 5b | no row-level security or tenant partitions (DSN-DVM-010); no relationship, unique-key or derivation indexes (DSN-DVM-005); no chunk placement, compression or encryption (DSN-DVM-002, -003) | PH-1: W-7 |
| layout 5c | node, version and commit caches return cached content without checking that the store holds it | PH-1: the index and tree verifier (DSN-DVM-092) also checks stored nodes, W-7 |
| benchmark C-1 | commit capacity bound by sequencer CPU (about 57 commits/s of 1–10 objects on the workstation) | RSK-016; carried benchmark decides (DEC-035); PH-1: W-8 |
| benchmark C-2 | reads and 10-change diffs far inside the targets at 200,000 objects | to be confirmed on WL-GENERIC-10M (DEC-035) |
| benchmark C-3 | generator lives in `ubos-bench`, not in `ubos-verify gen generic` (UBS-VER-03) | PH-1: W-9 |
| benchmark C-4 | the full dataset needs about 75 GB before indexes and WAL | recorded in `verify/envs/ENV-REF-SERVER-ph0.md` |

## 3. Work items for PH-1

| Item | Content | Design or requirement |
|---|---|---|
| W-1 | run VER-BENCH-0001 on ENV-REF-SERVER and record ASM-003 by decision | DEC-035, EXIT-0-05 |
| W-2 | memory accounted per execution in the kernel worker (thread-attributed count or one execution per process) | DSN-DVM-612, STD-CTX-004 |
| W-3 | per-thread CPU clock and weighted metering units for native calls on large values | STD-CTX-005, NR-PERF-010 |
| W-4 | cost of Rhai's data-size checks: benchmark, then patch or lower defaults | DSN-DVM-612 |
| W-5 | kernel decimal of `ubos-expr` as the L2 decimal type | DSN-DVM-612, STD-FND-022 |
| W-6 | vectors for the L2 sizes of STD-CTX-004 in BPA 1.0-rc1 | STD-CTX-004, STD-CTX-020 |
| W-7 | storage: sqlx; row-level security and tenant partitions; remaining kernel indexes; chunk placement, compression and encryption; verifier covering indexes and tree nodes | DSN-DVM-001…011, DSN-DVM-092, DEC-033 |
| W-8 | commit capacity: prepare the commits of a group in parallel and serialise only the tree update; model the group commit (EXIT-1-07) | DSN-DVM-006, DSN-DVM-007, RSK-016 |
| W-9 | move the WL-GENERIC generator into `ubos-verify gen generic` | UBS-VER-03 |
| W-10 | complete the class registry (C3 linearisation, narrowing) and the adapter operations `linearize` and `effective_class` | STD-CONF-005, DEC-033 |
| W-11 | MUST coverage 100% for BPA 1.0-rc1 | EXIT-1-01 |
| W-12 | grow the L2 escape suite from 100 to at least 300 cases | EXIT-1-10 |

## 4. Assumptions and risks

| # | Item | Disposition |
|---|---|---|
| 1 | ASM-003 | open until the carried benchmark (DEC-035) |
| 2 | ASM-008 | confirmed for L2 (DEC-034); the WASM part is verified in PH-3 |
| 3 | RSK-001 | raised to likelihood H; continued in PH-1 as RSK-016 |
| 4 | RSK-002 | not observed: 2.3 nodes written per change in a 1,000,000-key tree (early warning > 6) |
| 5 | RSK-003 | occurred once (F-1) and was handled before code; model-first rule kept |
| 6 | RSK-004 | closed (DEC-036) |
| 7 | RSK-005 | open: the external implementer review is part of the external review (DEC-037) |
