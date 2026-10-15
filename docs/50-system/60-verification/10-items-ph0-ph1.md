---
id: UBS-VER-10
title: Verification Items — PH-0 and PH-1
status: draft
phase: PH-1
depends_on: [UBS-VER-01, UBS-PH-0, UBS-PH-1]
---

# Verification Items — PH-0 and PH-1

Items follow template T11. Each item names the exit criteria it evidences (**Supports**).
Numbers follow UBS-VER-00 §3.

## PH-0

### VER-FORM-0001 — Commit and compare-and-set model
- **Verifies:** STD-TXN-021, STD-TXN-022, STD-VER-003, DSN-DVM-208, DSN-DVM-515
- **Suite:** SUITE-DVM-FORM · **Phase:** PH-0
- **Environment:** model-checking workstation
- **Procedure:**
  1. Model processes, nodes, branch heads, base validation and read-set validation in TLA+.
  2. State properties: linear first-parent history per branch; no lost update; serializable histories for serializable processes; dense `cseq`.
  3. Check with TLC at 3 processes × 2 nodes × 3 objects × 4 commits (exhaustive, or per DEC-031).
- **Pass criterion:** 0 violated properties; state count reported.
- **Evidence:** `commit_cas.tla`, configuration, TLC output.
- **Supports:** EXIT-0-02

### VER-FORM-0002 — Three-way merge with ledger append model
- **Verifies:** STD-VER-050, STD-VER-054, STD-VER-056, DSN-DVM-240, DSN-DVM-244, DSN-DVM-249
- **Suite:** SUITE-DVM-FORM · **Phase:** PH-0
- **Environment:** model-checking workstation
- **Procedure:**
  1. Model branches, Definition objects with fields, ledger keys with entries, merge base and the merge pipeline.
  2. State properties: determinism; complete conflict reporting; no lost or duplicated ledger entries; gap-free target sequences; correct merge parents.
  3. Check at 3 branches × 4 commits × 3 objects × 2 ledger keys (exhaustive, or per DEC-031).
- **Pass criterion:** 0 violated properties.
- **Evidence:** `merge.tla`, TLC output.
- **Supports:** EXIT-0-02

### VER-FORM-0003 — Timeline algebra model
- **Verifies:** STD-TIME-010, STD-TIME-011, STD-TIME-012, DSN-DVM-303, DSN-DVM-326
- **Suite:** SUITE-DVM-FORM · **Phase:** PH-0
- **Environment:** model-checking workstation
- **Procedure:**
  1. Model the four timeline operations and bitemporal reads.
  2. State properties: no overlapping segments; answers at past `(cseq, asof)` never change; impact set ⊇ results whose inputs changed.
  3. Check at 3 objects × 5 operations × 4 commits (exhaustive, or per DEC-031).
- **Pass criterion:** 0 violated properties.
- **Evidence:** `timeline.tla`, TLC output.
- **Supports:** EXIT-0-02

### VER-INSP-9000 — BPA 0.1 release inspection
- **Verifies:** FR-STD-011, FR-STD-013, FR-STD-014, FR-STD-021, DSN-BPA-001, DSN-BPA-002
- **Suite:** SUITE-BPA-COVERAGE · **Phase:** PH-0
- **Environment:** release pipeline
- **Procedure:**
  1. Verify the release tag signature and manifest hashes.
  2. Count vectors by profile and check the schema of every vector against DAT-ConformanceVector.
  3. Compute MUST-clause coverage for Core, Bitemporal and Ledger.
- **Pass criterion:** signature valid; ≥ 300 vectors; 0 schema errors; coverage ≥ 80% (MET-QUAL-003).
- **Evidence:** inspection record, coverage report.
- **Supports:** EXIT-0-01

### VER-CONF-8400 — Prototype conformance on PostgreSQL
- **Verifies:** FR-STD-021, DSN-DVM-001, DSN-FRG-401, DSN-FRG-402
- **Suite:** SUITE-DVM-CONF · **Phase:** PH-0
- **Environment:** ENV-REF-SERVER (PostgreSQL)
- **Procedure:**
  1. Build the prototype with the PostgreSQL adapter.
  2. Run `forge conform --profile prototype` against PostgreSQL (SQLite from PH-4, DEC-024).
- **Pass criterion:** 100% pass.
- **Evidence:** signed runner report.
- **Supports:** EXIT-0-03

### VER-PROP-0001 — Root history independence
- **Verifies:** STD-PROOF-001, STD-PROOF-002, NR-DET-003, DSN-DVM-007, DSN-DVM-930
- **Suite:** SUITE-DVM-PROP · **Phase:** PH-0
- **Environment:** CI workers
- **Procedure:**
  1. Generate pairs of operation sequences that reach the same key–value state by different orders, including deletes and re-inserts.
  2. Compute roots after each sequence.
- **Pass criterion:** equal roots in 100% of ≥ 1,000,000 pairs.
- **Evidence:** property report.
- **Supports:** EXIT-0-04, EXIT-1-04

### VER-PROP-0002 — Timeline non-overlap
- **Verifies:** STD-TIME-011, DSN-DVM-303
- **Suite:** SUITE-DVM-PROP · **Phase:** PH-0
- **Environment:** CI workers
- **Procedure:**
  1. Generate sequences of up to eight timeline operations (`create`, `supersede_from`, `set_period`, `replace_future`, `end`) over twenty dates.
  2. After every accepted operation, check that segments are non-empty, sorted and non-overlapping.
- **Pass criterion:** 0 violations in ≥ 1,000,000 sequences (UBS-PH-0 §7.4).
- **Evidence:** property report.
- **Supports:** EXIT-0-04

### VER-PROP-0003 — Merge determinism
- **Verifies:** STD-VER-040, STD-VER-050, STD-VER-052, DSN-DVM-240
- **Suite:** SUITE-DVM-PROP · **Phase:** PH-0
- **Environment:** CI workers
- **Procedure:**
  1. Generate a base with one to three objects and two branches with one to four random changes each (field updates and clears, valid-time supersedes, ends).
  2. Merge the right branch into the left one, and, from an identical history, the left into the right.
  3. Compare the outcomes: the same conflict set, or the same body and valid period of every object at every date.
- **Pass criterion:** equal outcomes in 100% of ≥ 1,000,000 cases (UBS-PH-0 §7.4).
- **Evidence:** property report.
- **Supports:** EXIT-0-04

### VER-PROP-0004 — Incremental tree maintenance
- **Verifies:** DSN-DVM-007, STD-PROOF-001
- **Suite:** SUITE-DVM-PROP · **Phase:** PH-0
- **Environment:** CI workers
- **Procedure:**
  1. Build a tree of 0 to 20,000 keys and store its nodes.
  2. Apply one to four batches of inserts, updates and removals (1 to 1,000 keys, or every key) to the stored nodes.
  3. After each batch compare the root with a full rebuild (STD-PROOF-001), and the tree diff of the two roots with the key-wise difference of the entry sets.
- **Pass criterion:** equal roots and diffs in 100% of ≥ 100,000 cases.
- **Evidence:** property report.
- **Supports:** EXIT-0-05

### VER-BENCH-0001 — Prototype storage benchmark
- **Verifies:** ASM-003, DEC-020, NR-PERF-001, NR-PERF-002, NR-PERF-005
- **Suite:** SUITE-DVM-BENCH · **Phase:** PH-0
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Load WL-GENERIC-10M with 24 months of history.
  2. Run point reads, simple commits at 500 commits/s and 10-change diffs for 30 minutes each, three times.
- **Pass criterion:** MET-PERF-001, MET-PERF-002 and MET-PERF-003 met in the median run, or a DEC entry records the fallback.
- **Evidence:** benchmark report with histograms and environment hash.
- **Supports:** EXIT-0-05
- **Phase note:** carried over to PH-1 (DEC-035); due before the PH-1 storage layout is fixed, at the latest at gate 1a. Tools: `ubos-bench`; set-up `verify/envs/ENV-REF-SERVER-ph0.md`.

### VER-SEC-0001 — Rhai sandbox spike
- **Verifies:** ASM-008, NR-SEC-002 (initial), DSN-DVM-612
- **Suite:** SUITE-DVM-SEC · **Phase:** PH-0
- **Environment:** CI workers
- **Procedure:**
  1. Run the 100 initial L2 escape cases (file, network, time, entropy, float, host abuse, resource exhaustion).
- **Pass criterion:** 100 of 100 blocked (MET-SEC-001).
- **Evidence:** sandbox report.
- **Supports:** EXIT-0-06

### VER-INSP-9003 — PH-1 re-baseline after PH-0
- **Verifies:** CST-015, DEC-020, ASM-003, ASM-008
- **Suite:** SUITE-BPA-COVERAGE · **Phase:** PH-0
- **Environment:** planning records
- **Procedure:**
  1. Review PH-0 findings (benchmarks, models, sandbox spike) against the PH-1 plan.
  2. Update UBS-PH-1 durations, risks and dependencies; record accepted changes as DEC entries.
- **Pass criterion:** updated UBS-PH-1 approved by the Phase Acceptance Board; every PH-0 finding has a disposition.
- **Evidence:** updated plan, decision entries.
- **Supports:** EXIT-0-07

## PH-1 gate 1a

### VER-CONF-8401 — Engine-profile conformance
- **Verifies:** all STD clauses of Core, Bitemporal, Ledger, Proof (roots, signatures, inclusion) and the instruction set used by them; NR-DET-002
- **Suite:** SUITE-DVM-CONF · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER; kernel on Linux x86-64, Linux ARM64 and macOS ARM64 against PostgreSQL
- **Procedure:**
  1. Run all vectors of the claimed profiles (VER-CONF-0001…4399) on PostgreSQL from each platform.
  2. Diff outcomes across platforms.
- **Pass criterion:** 100% pass everywhere; 0 cross-platform differences.
- **Evidence:** signed runner reports (3 runs).
- **Supports:** EXIT-1-02

### VER-CONF-8402 — Storage adapter suite on PostgreSQL
- **Verifies:** DSN-DVM-001, DSN-DVM-002, DSN-DVM-010, CST-002
- **Suite:** SUITE-DVM-STORE · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Run the storage suite (transactions, snapshot reads, index maintenance, sequences, chunk placement, RLS) on PostgreSQL.
  2. Inspect the storage trait for backend-specific types, so that the SQLite adapter (PH-4, DEC-024) needs no trait change.
- **Pass criterion:** 100% pass; 0 backend-specific types in the trait.
- **Evidence:** suite report.
- **Supports:** EXIT-1-03

### VER-PROP-0100 — Kernel property set
- **Verifies:** DSN-DVM-005, DSN-DVM-092, DSN-DVM-230, DSN-DVM-242, DSN-DVM-303, DSN-DVM-420, DSN-DVM-421, DSN-DVM-908, NR-DET-003
- **Suite:** SUITE-DVM-PROP · **Phase:** PH-1
- **Environment:** CI workers (long mode)
- **Procedure:**
  1. Run properties: index rebuild equality; diff cost bound; merge determinism and ledger completeness; timeline non-overlap; balance = Σ entries at any `(asof, cseq)`; cursor completeness; roots history independence.
  2. Long mode: ≥ 1,000,000 cases per property.
- **Pass criterion:** 0 failures.
- **Evidence:** property report `verify/reports/exit-1-04/` (PH-1 step 8a, DEC-061: nine properties, the five of VER-PROP-0100 and VER-PROP-0001…0003 at 1,000,000 cases each and VER-PROP-0004 at 100,000, 0 failures).
- **Supports:** EXIT-1-04

### VER-SIM-0001 — Concurrency and crash simulation
- **Verifies:** DSN-DVM-006, DSN-DVM-208, DSN-DVM-508, DSN-DVM-510, DSN-DVM-511, DSN-DVM-513, DSN-DVM-516, FR-TXN-023
- **Suite:** SUITE-DVM-SIM · **Phase:** PH-1
- **Environment:** simulation cluster
- **Procedure:**
  1. Simulate 2–4 nodes, 1–3 branches, contended objects and ledger keys, crashes at random points and clock jumps.
  2. After each step check: linear history, dense `cseq`, no lost acknowledged commit, gap-free ledger sequences; run an Elle-style checker on serializable histories.
  3. Run 10,000 seeds nightly and 1,000,000 before the gate.
- **Pass criterion:** 0 violations; 30 consecutive clean nights.
- **Evidence:** simulation report with seed ranges `verify/reports/exit-1-05/` (PH-1 step 8b, DEC-061: `ubos-sim`, 1,000,000 seeds, 0 violations (54,305,916 commits, 6,274,491 aborted flushes, 3,137,413 crashes after apply, 7,453,874 clock jumps); the sensitivity run with `--inject lost_ack` reports violations in every run); the 30 consecutive nights accumulate from the nightly workflow of step 8c (`nights.md` there).
- **Supports:** EXIT-1-05

### VER-FAULT-0001 — Durability under real crashes
- **Verifies:** NR-DUR-001, FR-TXN-013, DSN-DVM-503, DSN-DVM-508, DSN-DVM-402
- **Suite:** SUITE-DVM-FAULT · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER (PostgreSQL)
- **Procedure:**
  1. Run WL-MIXED-OLTP with a client-side log of acknowledged commits.
  2. Inject 5,000 SIGKILL and 5,000 power-loss events at random times.
  3. After each recovery compare the acknowledgement log with the store; run `verify indexes`, `verify sequences` and `verify roots`.
- **Pass criterion:** 0 acknowledged commits missing; 0 partial commits; 0 verifier findings.
- **Evidence:** fault report `verify/reports/exit-1-06/` (PH-1 step 8b, DEC-061: `ubos-fault` on the workstation's PostgreSQL 16, 10,000 events (5,000 node crashes by SIGKILL of every worker, 5,000 power losses of the server), 31,732 acknowledged commits, 0 lost, 0 partial, 0 verifier findings); the ENV-REF-SERVER run follows at step 9.
- **Supports:** EXIT-1-06

### VER-FORM-0004 — Group commit and ledger sequence model
- **Verifies:** DSN-DVM-006, DSN-DVM-402, DSN-DVM-404, DSN-DVM-424
- **Suite:** SUITE-DVM-FORM · **Phase:** PH-1
- **Environment:** model-checking workstation
- **Procedure:**
  1. Extend the commit model with batches, per-process abort within a batch, and ledger counters.
  2. Check gap-freeness, per-key total order and batch atomicity per process.
  3. Bound (DEC-031, DEC-046): 3 processes × 2 nodes × 2 objects × 2 ledger keys × batch 3 × 4 commits, with one failed storage transaction; model `verify/models/group_commit.tla`.
- **Pass criterion:** 0 violated properties.
- **Evidence:** model and checker output.
- **Supports:** EXIT-1-07

### VER-CONF-8500 — Replay fidelity across platforms
- **Verifies:** NR-DET-001, NR-DET-002, FR-LOGIC-073, FR-LOGIC-074, DSN-DVM-631, DSN-DVM-632
- **Suite:** SUITE-DVM-REPLAY · **Phase:** PH-1
- **Environment:** Linux x86-64, Linux ARM64, macOS ARM64
- **Procedure:**
  1. Replay the ≥ 10,000-process corpus on each platform.
  2. Compare outcome hashes with the recorded ones.
- **Pass criterion:** 100% identical on all platforms.
- **Evidence:** replay report `verify/reports/exit-1-08/` (PH-1 step 8c, DEC-061: `ubos-replay`, 10,000 generated processes, 10,000 of 10,000 outcomes identical and 1,000 of 1,000 journal replays identical on Linux x86-64; Linux ARM64 and macOS ARM64 from the nightly workflow, logged in `verify/reports/exit-1-05/nights.md`).
- **Supports:** EXIT-1-08

### VER-BENCH-0100 — Kernel performance gate 1a
- **Verifies:** NR-PERF-001, NR-PERF-002, NR-PERF-004, NR-PERF-005, NR-PERF-006, NR-PERF-007, NR-PERF-010
- **Suite:** SUITE-DVM-BENCH · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER (PostgreSQL)
- **Procedure:**
  1. Load WL-GENERIC-10M with 24 months of history; apply WL-MIXED-OLTP at 70% of maximum sustainable throughput.
  2. Measure each NR at its stated boundary per UBS-VER-01 BENCH procedure.
- **Pass criterion:** MET-PERF-010…015 and NR-PERF-010 met in the median of three runs.
- **Evidence:** benchmark report (`verify/reports/exit-1-09/`, after the ENV-REF-SERVER run of step 9b); the driver `ubos-bench run --what read|commit|diff|branch|merge|asof [--background R]` is complete (PH-1 step 9a, DEC-062) and runs on a small dataset in CI; the runbook is `docs/reports/ph1-gate1a-runbook-zh.md`; workstation pre-run of the merge preview after the fix of step 9b in `verify/reports/exit-1-09/workstation/` (1,000 changed objects on 20,000: p50 0.37 s, p95 0.40 s; 36 s before).
- **Supports:** EXIT-1-09

### VER-SEC-0100 — L2 escape suite
- **Verifies:** NR-SEC-002, FR-LOGIC-062, FR-LOGIC-065, DSN-DVM-603, DSN-DVM-612, DSN-DVM-616, DSN-DVM-621
- **Suite:** SUITE-DVM-SEC · **Phase:** PH-1
- **Environment:** CI workers
- **Procedure:**
  1. Run the L2 escape corpus (≥ 300 cases).
  2. For each case, assert termination or capability denial and absence of any external effect (file, network, clock, cross-tenant memory).
- **Pass criterion:** 100% blocked (MET-SEC-010).
- **Evidence:** security report `verify/reports/exit-1-10/` (382 of 382 blocked, PH-1 step 5d-2, DEC-058).
- **Supports:** EXIT-1-10

### VER-INSP-0001 — Coverage, mutation and dependency graph
- **Verifies:** NR-MAINT-002, NR-MAINT-003, AR-031, AR-032
- **Suite:** SUITE-DVM-CONF · **Phase:** PH-1
- **Environment:** CI
- **Procedure:**
  1. Measure line coverage and mutation score (cargo-mutants) for core crates.
  2. Run the dependency-graph lint against the declared crate graph (UBS-ARC-07).
- **Pass criterion:** coverage ≥ 85%; mutation score ≥ 70%; 0 violations.
- **Evidence:** `verify/reports/exit-1-11/` (PH-1 step 8a, DEC-061: line coverage of the eleven kernel crates 90.1% by the verification suites, `coverage-by-crate.json`; mutation score 89.9% on a workstation sample of 237 viable mutants (213 detected); `check_deps.py` 0 violations); CI runs the dependency check and the test bodies on every push, nightly CI (step 8c) the coverage and every mutation shard.
- **Supports:** EXIT-1-11

### VER-CONF-8403 — Portable export and import round trip
- **Verifies:** NR-PORT-003, STD-PROOF-070, DSN-DVM-950, DSN-DVM-951
- **Suite:** SUITE-DVM-STORE · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Export 20 test VAEs (including one with WL-GENERIC-10M) from one PostgreSQL database; import into a second PostgreSQL database; export again and import into the first (SQLite round trip from PH-4, DEC-024).
  2. Compare head roots, tags, ledger sequences and query results.
- **Pass criterion:** identical roots and results in 100% of VAEs.
- **Evidence:** round-trip report `verify/reports/exit-1-12/` (PH-1 step 6c, DEC-059: 3 VAEs on the workstation, identical after import and re-import, archives verified by `ubos-verify`); the ENV-REF-SERVER run of the procedure with 20 VAEs follows at step 9.
- **Supports:** EXIT-1-12

### VER-SCN-1000 — Gate-1a scenarios in process
- **Verifies:** SCN-002, SCN-003, SCN-004, SCN-005, SCN-006, SCN-007, SCN-008, SCN-010, SCN-011, SCN-018
- **Suite:** SUITE-PH1-SCN · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL (in-process host on PostgreSQL)
- **Procedure:**
  1. Run each scenario's main and alternate flows as Forge scenario tests.
- **Pass criterion:** all steps and acceptance statements pass.
- **Evidence:** scenario report `verify/reports/exit-1-13/` (PH-1 step 7c, DEC-060: 22 of 22 scenario tests of the ten scenarios, main and alternate flows, on PostgreSQL and in memory on the workstation; SCN-003 step 3, SCN-002 postcondition 2 and SCN-018 A1 belong to gate 1b); the ENV-REF-SMALL run follows at step 9.
- **Supports:** EXIT-1-13

### VER-INSP-9001 — BPA 1.0-rc1 coverage
- **Verifies:** FR-STD-022, DSN-BPA-003
- **Suite:** SUITE-BPA-COVERAGE · **Phase:** PH-1
- **Environment:** release pipeline
- **Procedure:**
  1. Compute MUST-clause coverage per profile for the rc1 profiles.
- **Pass criterion:** 100% coverage.
- **Evidence:** coverage report, release manifest.
- **Supports:** EXIT-1-01

## PH-1 gate 1b

### VER-INSP-9004 — BPA 1.0-rc2 coverage
- **Verifies:** FR-STD-022, DSN-BPA-003
- **Suite:** SUITE-BPA-COVERAGE · **Phase:** PH-1
- **Environment:** release pipeline
- **Procedure:**
  1. Compute MUST-clause coverage per profile for the rc2 profiles (Core, Bitemporal, Ledger, Proof, Governance, Protocol), excluding the superseded vectors of 1.0-rc1 (STD-CONF-030).
- **Pass criterion:** 100% coverage.
- **Evidence:** coverage report and release manifest `standard/release/1.0-rc2/` (PH-1 step 10e, DEC-069: 165 of 165 MUST clauses, 1,586 vectors); tag `bpa-v1.0.0-rc2`.
- **Supports:** EXIT-1-14

### VER-CONF-8404 — Governance profile and remaining 1b vectors
- **Verifies:** STD-EXPR-*, STD-LGS-*, STD-ADDR-*, STD-PROTO-*, STD-ERR-* clauses in the claimed profiles
- **Suite:** SUITE-DVM-CONF · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Run VER-CONF-4400…5599 and VER-CONF-6000…6099 on PostgreSQL, plus all rc1 vectors.
- **Pass criterion:** 100% pass.
- **Evidence:** signed runner reports.
- **Supports:** EXIT-1-14

### VER-CONF-7000 — UBTP server conformance
- **Verifies:** FR-INT-011, FR-INT-012, FR-INT-013, FR-INT-014, FR-INT-015, DSN-NOD-010…017
- **Suite:** SUITE-NOD-PROTO · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL
- **Procedure:**
  1. Run the UBTP suite over WebSocket, HTTP and in-process bindings.
  2. Run version-skew cases (client N−1).
- **Pass criterion:** 100% pass; identical results across bindings.
- **Evidence:** suite report.
- **Supports:** EXIT-1-15

### VER-CONF-7500 — SDK protocol suite
- **Verifies:** FR-DEV-061, DSN-SDK-001…007, DSN-SDK-010…014
- **Suite:** SUITE-SDK-PROTO · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL
- **Procedure:**
  1. Run the shared SDK protocol suite with the Rust and TypeScript SDKs.
- **Pass criterion:** 100% pass for both SDKs.
- **Evidence:** suite reports.
- **Supports:** EXIT-1-15

### VER-FAULT-4500 — SDK reconnection and idempotent retries
- **Verifies:** DSN-SDK-002, DSN-SDK-003, DSN-SDK-004, AR-010
- **Suite:** SUITE-SDK-NET · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL with network emulation
- **Procedure:**
  1. Drop connections after server commit and before response; partition for 30 s; resume subscriptions.
- **Pass criterion:** exactly one commit per logical call; all missed updates received in order.
- **Evidence:** fault report.
- **Supports:** EXIT-1-15

### VER-SEC-2000 — Tenant isolation suite
- **Verifies:** NR-SEC-003, FR-TEN-021, FR-TEN-023, DSN-NOD-405, DSN-NOD-406, DSN-DVM-010
- **Suite:** SUITE-NOD-ISO · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL
- **Procedure:**
  1. Create two tenants with overlapping class names and IDs.
  2. Run the isolation corpus (≥ 500 cases) through UBTP, blobs, events and jobs.
- **Pass criterion:** 0 successful cross-tenant accesses (MET-SEC-011).
- **Evidence:** security report.
- **Supports:** EXIT-1-16

### VER-SEC-2001 — Secret and log hygiene; security event coverage
- **Verifies:** NR-SEC-009, NR-SEC-010, FR-AUD-012, DSN-NOD-111, DSN-NOD-130
- **Suite:** SUITE-NOD-AUTH · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL
- **Procedure:**
  1. Seed canary secrets in connectors and credentials; run the full PH-1 scenario suite.
  2. Scan logs, traces and diagnostic bundles for canaries.
  3. Compare produced security event types with the FR-AUD-012 list.
- **Pass criterion:** 0 canary matches; 100% event types present.
- **Evidence:** security report.
- **Supports:** EXIT-1-16

### VER-BENCH-2000 — Edge performance gate 1b
- **Verifies:** NR-PERF-008, NR-PERF-009, NR-PERF-011, NR-PERF-012
- **Suite:** SUITE-NOD-BENCH · **Phase:** PH-1
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Run indexed queries with row filters over 10 M objects; 10,000-block sheet evaluation; commit-to-handler latency under WL-MIXED-OLTP; became-effective timing with 1,000 future changes.
- **Pass criterion:** MET-PERF-016, MET-PERF-017, NR-PERF-009 and NR-PERF-012 met.
- **Evidence:** benchmark report (NR-PERF-009 on the workstation at PH-1 step 11d, DEC-070: `verify/reports/exit-1-14/workstation/`, `ubos-bench sheets` with 10,000 blocks and 20 candidates — warm p95 0.41 ms per object; the reference run follows at step 19).
- **Supports:** EXIT-1-17

### VER-FAULT-2000 — Installation, backup and point-in-time restore
- **Verifies:** NR-DUR-004, NR-OPER-003, NR-OPER-005, FR-OPS-021, DSN-NOD-602, DSN-NOD-603, DSN-NOD-615, DSN-NOD-620
- **Suite:** SUITE-NOD-OPS · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL
- **Procedure:**
  1. Install from the guide on a clean host; record time.
  2. Run the invalid configuration corpus (≥ 200).
  3. Under load, restore to 10 random points in the last 7 days; verify roots and run `verify`.
  4. Produce a diagnostic bundle.
- **Pass criterion:** 100% invalid configurations rejected; 10 of 10 restores verified; bundle ≤ 5 minutes.
- **Evidence:** operations report.
- **Supports:** EXIT-1-18

### VER-SCN-1001 — All PH-1 scenarios over UBTP
- **Verifies:** SCN-001…SCN-011, SCN-017, SCN-018
- **Suite:** SUITE-PH1-SCN · **Phase:** PH-1
- **Environment:** ENV-REF-SMALL (single-node Server)
- **Procedure:**
  1. Run all 13 scenarios through the SDKs over UBTP, including alternate flows.
- **Pass criterion:** 13 of 13 pass (MET-QUAL-015).
- **Evidence:** scenario report.
- **Supports:** EXIT-1-19

### VER-INSP-9500 — PH-1 traceability
- **Verifies:** all PH-1 Must FR, NR and CR
- **Suite:** SUITE-REGRESSION · **Phase:** PH-1
- **Environment:** CI
- **Procedure:**
  1. Generate the traceability report (`gen_traceability.py`) and the verification results index.
  2. List Must requirements without a passing mapped item.
- **Pass criterion:** the list is empty.
- **Evidence:** traceability report.
- **Supports:** EXIT-1-20
