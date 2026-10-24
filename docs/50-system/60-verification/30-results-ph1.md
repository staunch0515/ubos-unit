---
id: UBS-VER-30
title: Verification results index — PH-1
status: draft
phase: PH-1
depends_on: [UBS-VER-04, UBS-VER-10, UBS-VER-22]
---

# Verification results index — PH-1

The result of every suite and verification item that verifies a PH-1 Must requirement, with its
evidence (VER-INSP-9500 procedure 1; PH-1 step 19d, DEC-085). `docs/tools/gen_results.py` joins
it with the traceability (UBS-VER-22) into the results by requirement (UBS-VER-31), whose gap
list is the EXIT-1-20 criterion. Results are recorded as of 2026-10-18 and are updated when a run
changes them.

Results of suites and items:

| Result | Meaning |
|---|---|
| `passed` | the PH-1 procedure ran and passed, in CI or on the workstation, and needs no reference environment |
| `workstation` | passed on the development workstation; the run its item names on a reference environment is pending |
| `partial` | ran; some targets are not met (requirement-level results below say which) |
| `pending` | not run yet (a reference environment, a rented service or an external party) |
| `not-built` | the suite is not built in PH-1 |
| `later` | the suite or item belongs to a later phase or to a system PH-1 does not build |

A suite's result covers what it runs in PH-1. The requirement-level results at the end take
precedence over the results of the suites: they record a requirement whose own target a suite
measured.

## Suites

| Result | Suite | Evidence |
|---|---|---|
| `passed` | SUITE-DVM-CONF | BPA 1.0-rc7 conformance 1,703 of 1,703 vectors in memory (CI `rust workspace`, `crates/ubos-forge/tests/conformance.rs`), the claimed profiles on PostgreSQL; `verify/reports/exit-1-02/` (VER-CONF-8401) |
| `passed` | SUITE-DVM-STORE | `ubos-store-suite` on PostgreSQL in CI; `verify/reports/exit-1-03/` (20 of 20) |
| `passed` | SUITE-DVM-PROP | ten properties at 1,000,000 cases, 0 failures (`verify/reports/exit-1-04/`); 10,000 cases in CI, 100,000 nightly |
| `passed` | SUITE-DVM-SIM | 1,000,000 seeds, 0 violations (`verify/reports/exit-1-05/`); 2,000 seeds in CI, 10,000 nightly |
| `passed` | SUITE-DVM-FAULT | 10,000 crash and power-loss events on PostgreSQL, 0 lost (`verify/reports/exit-1-06/`); 40 events in CI |
| `passed` | SUITE-DVM-FORM | TLA+ models `commit_cas`, `merge`, `timeline`, `group_commit` exhaustive (`verify/models/reports/`); reduced bounds in CI |
| `pending` | SUITE-DVM-BENCH | the gate runs of VER-BENCH-0001, -0100 and -2000 are on ENV-REF-SERVER; workstation runs so far: storage prototype (`verify/reports/exit-0-05/`), sheets (`verify/reports/exit-1-14/workstation/`), queries (`verify/reports/exit-1-17/workstation/`, `verify/reports/exit-1-17/edge/`), merge preview (`verify/reports/exit-1-09/workstation/`); requirement-level results below |
| `passed` | SUITE-DVM-SEC | L2 escape suite 382 of 382 blocked (`verify/reports/exit-1-10/`, in CI); policy enforcement and row filters in the conformance vectors |
| `passed` | SUITE-DVM-REPLAY | 10,000 processes identical on Linux x86-64, Linux ARM64 and macOS ARM64 (`verify/reports/exit-1-08/`) |
| `passed` | SUITE-DVM-XIMPL | the reference model and the kernel agree on every vector of the release (SUITE-DVM-CONF); export and import between builds verified by the independent verifier `ubos-verify` (`verify/reports/exit-1-12/`, VER-CONF-8403, in CI) |
| `passed` | SUITE-NOD-HOST | 12 of 12 (`verify/reports/suite-nod-host/`, in CI) |
| `passed` | SUITE-NOD-PROTO | the Protocol vectors on the in-process, WebSocket and HTTP bindings, version skew 8 of 8, REST equivalence (`verify/reports/exit-1-15/`, in CI) |
| `passed` | SUITE-NOD-ISO | 544 of 544 (`verify/reports/exit-1-16/`, in CI) |
| `passed` | SUITE-NOD-AUTH | 13 of 13 in memory and on PostgreSQL (`verify/reports/suite-nod-auth/`); secret hygiene and event coverage (`verify/reports/exit-1-16/`, in CI) |
| `passed` | SUITE-NOD-JOBS | 13 of 13 (`verify/reports/suite-nod-jobs/`, in CI) |
| `passed` | SUITE-NOD-EVENTS | 10 of 10 on PostgreSQL with the SIGKILL and failover cases (`verify/reports/suite-nod-events/`, in CI) |
| `passed` | SUITE-NOD-FILES | the PH-1 file service (DSN-NOD-320: resumable uploads, chunks, signed download links, personal downloads logged): the case `files` of `ubos-events-suite` (in CI) and the `files` group of SUITE-NOD-ISO; scanning, previews and WORM come with PH-2 |
| `workstation` | SUITE-NOD-OPS | 35 of 35 on the workstation (`verify/reports/exit-1-18/workstation/`), SCN-001 and SCN-017 on an installed Server (`verify/reports/exit-1-19/`, in CI); the ENV-REF-SMALL run is pending |
| `partial` | SUITE-NOD-BENCH | `ubos-edge-bench` on the workstation (`verify/reports/exit-1-17/edge/`, `verify/reports/exit-1-17/nod/`); requirement-level results below |
| `later` | SUITE-NOD-SYNC | device sync (DEC-024, PH-4) |
| `later` | SUITE-NOD-PEN | external penetration test (PH-2, VER-SEC-* of UBS-VER-12) |
| `passed` | SUITE-FRG-CHECK | `forge check` with one error per stage (`verify/reports/forge-7b/`), the skeleton and the scenario project in CI |
| `passed` | SUITE-FRG-CLI | `forge init`, `check`, `types`, `test`, `conform`, `dev` with their JSON reports in CI (`verify/reports/forge-7b/`, `verify/reports/forge-dev/`) |
| `passed` | SUITE-FRG-CONFORM | `forge conform` over the prototype vectors in memory and on PostgreSQL (`verify/reports/exit-0-03/`, in CI) |
| `not-built` | SUITE-FRG-REPRO | `forge pack` and `verify` come with packaging of Buks (PH-2) |
| `passed` | SUITE-SDK-PROTO | 27 of 27 per SDK and binding (`verify/reports/exit-1-15/sdk-suite.json`, CI `sdk suites`) |
| `passed` | SUITE-SDK-NET | reconnection and resume through a fault proxy, both SDKs (`verify/reports/exit-1-15/sdk-suite.json`) |
| `passed` | SUITE-SDK-TYPES | 4 of 4 (`verify/reports/exit-1-15/sdk-suite.json`) |
| `not-built` | SUITE-SDK-SKEW | SDK N against node N−1 and N+1 needs a second released version; the node's version skew is in SUITE-NOD-PROTO |
| `passed` | SUITE-BPA-COVERAGE | release coverage and manifests 1.0-rc1 … 1.0-rc7 (`standard/release/`) |
| `later` | SUITE-BPA-CERT | certification of other implementations (BPA 1.0 final) |
| `later` | SUITE-BPA-COMPAT | compatibility across standard versions (BPA 1.0 final) |
| `later` | SUITE-BPA-MODELS | formal models of the standard's later profiles |
| `later` | SUITE-CTL-BLIND | Control plane (PH-4) |
| `later` | SUITE-CTL-USAGE | Control plane (PH-4) |
| `later` | SUITE-NTY-ANCHOR | Notary (later phase) |
| `later` | SUITE-NTY-PACK | Notary (later phase) |
| `later` | SUITE-NTY-VERIFIER | Notary (later phase) |
| `later` | SUITE-FED-TRUST | federation (PH-5) |
| `later` | SUITE-EXC-LIC | Exchange (PH-4) |
| `later` | SUITE-EXC-PUB | Exchange (PH-4) |
| `later` | SUITE-WSP-A11Y | Web workspace (PH-2) |
| `later` | SUITE-WSP-E2E | Web workspace (PH-2) |
| `later` | SUITE-WSP-PERF | Web workspace (PH-2) |
| `later` | SUITE-WSP-RENDER | Web workspace (PH-2) |
| `later` | SUITE-WSP-USE | Web workspace (PH-2) |
| `later` | SUITE-STU-E2E | Studio (PH-2) |
| `later` | SUITE-STU-USE | Studio (PH-2) |
| `later` | SUITE-AGT-SCN | agents (PH-3) |
| `later` | SUITE-BRG-EXPORT | Bridge (PH-2) |

## Verification items

| Result | Item | Evidence |
|---|---|---|
| `passed` | VER-FORM-0001 | `verify/models/reports/commit_cas.exhaustive.txt` |
| `passed` | VER-FORM-0002 | `verify/models/reports/merge.exhaustive.txt` |
| `passed` | VER-FORM-0003 | `verify/models/reports/timeline.exhaustive.txt` |
| `passed` | VER-FORM-0004 | `verify/models/reports/group_commit.exhaustive.txt` |
| `passed` | VER-INSP-9000 | BPA 0.1 release inspection (PH-0 acceptance, `docs/reports/ph0-acceptance-zh.md`) |
| `passed` | VER-CONF-8400 | `verify/reports/exit-0-03/` (453 of 453) |
| `passed` | VER-PROP-0001 | `verify/reports/exit-0-04/`, `verify/reports/exit-1-04/` |
| `passed` | VER-PROP-0002 | `verify/reports/exit-1-04/` |
| `passed` | VER-PROP-0003 | `verify/reports/exit-1-04/` |
| `passed` | VER-PROP-0004 | `verify/reports/exit-1-04/` |
| `pending` | VER-BENCH-0001 | the ENV-REF-SERVER run of EXIT-0-05 (`verify/reports/exit-0-05/`: preparation and local trial) |
| `passed` | VER-SEC-0001 | `verify/reports/exit-0-06/` (100 of 100 blocked) |
| `passed` | VER-INSP-9003 | the PH-1 re-baseline (UBS-PH-1, DEC-030 and later decisions) |
| `passed` | VER-CONF-8401 | `verify/reports/exit-1-02/` |
| `passed` | VER-CONF-8402 | `verify/reports/exit-1-03/` |
| `passed` | VER-PROP-0100 | `verify/reports/exit-1-04/` |
| `passed` | VER-SIM-0001 | `verify/reports/exit-1-05/` |
| `passed` | VER-FAULT-0001 | `verify/reports/exit-1-06/` |
| `passed` | VER-CONF-8500 | `verify/reports/exit-1-08/` |
| `pending` | VER-BENCH-0100 | the ENV-REF-SERVER run (`verify/reports/exit-1-09/`, status open) |
| `passed` | VER-SEC-0100 | `verify/reports/exit-1-10/` (382 of 382) |
| `passed` | VER-INSP-0001 | `verify/reports/exit-1-11/` |
| `passed` | VER-CONF-8403 | `verify/reports/exit-1-12/` |
| `passed` | VER-SCN-1000 | `verify/reports/exit-1-13/` |
| `passed` | VER-INSP-9001 | `standard/release/1.0-rc1/` |
| `passed` | VER-INSP-9004 | `standard/release/1.0-rc2/` |
| `passed` | VER-INSP-9005 | `standard/release/1.0-rc3/` |
| `passed` | VER-CONF-8404 | the governance profile in the 1.0-rc7 conformance run (1,703 of 1,703) |
| `passed` | VER-CONF-7000 | `verify/reports/exit-1-15/` |
| `passed` | VER-CONF-7500 | `verify/reports/exit-1-15/sdk-suite.json` |
| `passed` | VER-FAULT-4500 | `verify/reports/exit-1-15/sdk-suite.json` |
| `passed` | VER-SEC-2000 | `verify/reports/exit-1-16/` |
| `passed` | VER-SEC-2001 | `verify/reports/exit-1-16/` |
| `partial` | VER-BENCH-2000 | `verify/reports/exit-1-17/` (workstation; requirement-level results below) |
| `workstation` | VER-FAULT-2000 | `verify/reports/exit-1-18/workstation/` (35 of 35); ENV-REF-SMALL pending |
| `workstation` | VER-SCN-1001 | `verify/reports/exit-1-19/`; ENV-REF-SMALL pending |
| `passed` | VER-INSP-9500 | this index and UBS-VER-31 (`verify/reports/exit-1-20/`) |
| `later` | VER-BENCH-0300 | PH-4 |
| `later` | VER-CONF-8408 | a later phase's item |
| `later` | VER-SCN-2000 | PH-2 |
| `later` | VER-INSP-9503 | a later phase's item |
| `later` | VER-SEC-0500 | a later phase's item |

## Requirement-level results

| Result | Requirement | Evidence |
|---|---|---|
| `met-workstation` | NR-PERF-008 | indexed pages through the edge at 300,000 objects p95 60.9–98.4 ms (`verify/reports/exit-1-17/edge/`); the row-filtered page (p95 182–208 ms) and the denied page (about 2 s) are the residual post-filter of DEC-074; the 10,000,000-object run on ENV-REF-SERVER is pending |
| `met-workstation` | NR-PERF-009 | warm p95 0.41 ms per object (`verify/reports/exit-1-14/workstation/`) |
| `not-met` | NR-PERF-011 | commit to handler start under WL-MIXED-OLTP at 70 % of one VAE's capacity: p95 578 ms against 500 ms, p99 659 ms within 2 s (`verify/reports/exit-1-17/edge/`); after PH-2 step 0b-1: p95 555 ms, p99 643 ms (`verify/reports/ph2-0b1/`) |
| `met` | FR-MODEL-013 | not met at the PH-1 close: found in PH-2 step 0b-1b that the commit checked no unique keys (`MODEL.KEY_CONFLICT` only from `release`) and that the conformance-suite coverage was structural only. Met from PH-2 step 0b-4: keys unique per branch at commit across subclasses (step 0b-3), slugs unique per class and branch, and the key conflict of a merge (acceptance 2) reported by `merge` and `merge_preview` (`crates/ubos-dvm/src/keys.rs`; scenarios KEY-1…KEY-6, SLUG-1…SLUG-3, MKEY-1…MKEY-3 in memory, on PostgreSQL and over UBTP; simulation with contended keys; `verify/reports/ph2-0b3/`, `verify/reports/ph2-0b4/`) |
| `met-workstation` | NR-PERF-012 | 1,000 became-effective events p99 0.51 s on PostgreSQL (`verify/reports/exit-1-17/nod/`) |
| `met-workstation` | FR-TEN-024 | a tenant at ten times another's load raises the other's p95 by 26 % under the per-principal limit (171 % without it; `verify/reports/exit-1-17/edge/`) |
| `met` | CR-GDPR-001 | the PH-1 part of privacy by default (DEC-086): classification by declaration, class default and `internal`, raise-only, rectification; PRIV-1…PRIV-4 in CI and the coverage inspection (`verify/reports/privacy-ph1/`); masking and purpose limitation follow with DSN-DVM-821/-823 in PH-2. The traceability maps it to Notary and federation suites only |
| `met` | FR-AUD-011 | the audit trail of PH-1 step 13c (`audit`: who, what, when, why, how and rules from the commit and process records); PRIV-4 checks the fields changed with old and new values, the valid period, the message and the reason (in CI). The traceability maps it to a Web workspace suite only |
| `pending` | FR-EVT-034 | per-tenant job limits and priorities pass functionally (SUITE-NOD-JOBS case `priorities_and_limits`: priority order, re-prioritisation, a tenant limit holds and lifts); the benchmark of the acceptance (tenant A with 100,000 queued jobs, tenant B's jobs starting within the fairness bound) has not run |
| `pending` | FR-STD-042 | exports carry their format version and 1.0 archives import with identical roots (`verify/reports/exit-1-12/`); importing a 1.0 export into a later minor version needs that version; moved to PH-3 by DEC-087 |
| `not-met` | CR-SOC2-003 | at rest: chunks sealed with XChaCha20-Poly1305 under the key hierarchy (DSN-DVM-002, DEC-048) and sealed backups (step 16e); in transit: the PH-1 Server listens on plain HTTP and WebSocket, with no TLS termination of its own and none in its packages (`server.tls_certificate` is only watched for expiry); moved to PH-2 step 11 by DEC-087 |
| `not-met` | NR-SEC-004 | Ed25519 signatures, SHA-256 content identifiers and ChaCha20-Poly1305 at rest are in place; TLS 1.3 for external and node-to-node traffic is not provided by the PH-1 Server (as CR-SOC2-003); moved to PH-2 step 11 by DEC-087 |
| `met-workstation` | FR-RULE-024 | as NR-PERF-009: 20 candidates among 10,000 blocks, warm p95 0.41 ms per object (`verify/reports/exit-1-14/workstation/`) |
| `met-workstation` | FR-ANL-014 | declared field indexes used by the planner: indexed pages read `offset + limit` index rows, independent of the number of objects (`verify/reports/exit-1-17/workstation/`, `verify/reports/exit-1-17/edge/`) |
| `met` | CR-SEC-002 | the conformance part through the requirements its acceptance names: FR-TIME-011 and FR-PROOF-012 (SUITE-DVM-CONF), FR-LEDG-041 (SUITE-DVM-FAULT, SUITE-DVM-PROP); the auditor's examination (AUDIT) comes with the CPA audit drill of PH-3. The traceability maps it to Notary suites only |
| `met` | FR-EVT-024 | SUITE-NOD-EVENTS case `handler_process_once`: the handler runs as a committed process of its service principal with its trigger recorded, and a redelivery does not run again (in CI). The traceability maps it to FAULT suites only |
| `met` | FR-EVT-041 | SUITE-NOD-JOBS schedule cases: cron and business-calendar rules in `America/New_York` across the DST change, misfire policies, port and logic targets (in CI; workflow targets in PH-2 by its Phase note) |
| `met` | FR-PKG-071 | bootstrap verifies and installs the embedded `system` and `ontology` Buks (SCN-001 with A2, `verify/reports/exit-1-19/`, in CI) and `genesis-verify` in CI |
| `met` | FR-VER-072 | SCN-018 main and alternate flows in process, on PostgreSQL and over UBTP (`verify/reports/exit-1-13/`, `verify/reports/exit-1-19/`, in CI) |
| `met` | NR-PRIV-006 | the PH-1 defaults of DEC-086: classification `internal` unless declared or defaulted by the class, raise-only; `forge check` coverage inspection and PRIV-1…PRIV-4 (`verify/reports/privacy-ph1/`) |
| `met` | FR-DEV-063 | the SDKs version 0.2 under semantic versioning, negotiate the protocol version and pass the version-skew cases (`verify/reports/exit-1-15/`); the test against two node minors runs when protocol 1.1 exists (Phase note of DSN-SDK-014) |
| `pending` | NR-PERF-001 | the ENV-REF-SERVER run of VER-BENCH-0100 (and VER-BENCH-0001 of PH-0) |
| `pending` | NR-PERF-002 | the ENV-REF-SERVER run of VER-BENCH-0100 (and VER-BENCH-0001 of PH-0) |
| `pending` | NR-PERF-004 | the ENV-REF-SERVER run of VER-BENCH-0100 |
| `pending` | NR-PERF-005 | the ENV-REF-SERVER run of VER-BENCH-0100 (and VER-BENCH-0001 of PH-0) |
| `pending` | NR-PERF-006 | the ENV-REF-SERVER run of VER-BENCH-0100; the workstation pre-run of the merge preview (`verify/reports/exit-1-09/workstation/`) |
| `pending` | NR-PERF-007 | the ENV-REF-SERVER run of VER-BENCH-0100 |
| `pending` | NR-PERF-010 | the ENV-REF-SERVER run of VER-BENCH-0100 |
| `pending` | FR-LEDG-014 | the ENV-REF-SERVER run (appends on ledgers of 1,000 and 100,000,000 entries) |
| `pending` | FR-VER-045 | the ENV-REF-SERVER run of VER-BENCH-0100 (diffs) |
| `pending` | FR-VER-043 | pageable and summarised diffs are not benchmarked at 1,000,000 changes; the ENV-REF-SERVER run |
| `pending` | FR-PROOF-013 | content-defined chunks of files (DSN-NOD-320) work (SUITE-NOD-EVENTS case `files`); the storage of a changed 10 MB document is not measured |
| `pending` | FR-FILE-011 | files are content-addressed chunked blobs linked to object versions (SUITE-NOD-EVENTS case `files`); replacing an attachment and reading both by version is not tested; moved to PH-2 step 0b by DEC-087 |
| `pending` | FR-TXN-032 | idempotency within a node: STD-TXN-040 vectors and SUITE-NOD-JOBS case `idempotent_execution`; the retention period and deduplication after a failover are not tested; moved to PH-2 step 0b by DEC-087 |
| `not-met` | FR-LEDG-031 | declared balance projections (group-by keys, `sync` and `async` maintenance) are not built; balances are computed from the entries at read (STD-TIME-020, property `balance_bitemporal`); moved to PH-2 step 3 by DEC-087 |
| `not-met` | FR-LEDG-033 | projection watermarks and `min_commit` reads are not built (as FR-LEDG-031); moved to PH-2 step 3 by DEC-087 |
| `met` | FR-DEV-011 | the PH-1 part (DEC-087): `forge init`, `check`, `types`, `test`, `conform` and `dev` with their JSON reports (`verify/reports/forge-7b/`, `verify/reports/forge-dev/`, in CI); `forge new` and `forge pack` follow in PH-2 step 7 |
| `not-met` | FR-ANL-021 | projection rebuild is realised by DSN-NOD-344 of PH-2 only; field indexes rebuild (`index_since`, `ubos-server verify`) but projections are not built; moved to PH-2 step 3 by DEC-087 |
| `not-met` | FR-TEN-031 | realised by DSN-NOD-407 of PH-2 only; the kernel has overlay branches (branch kind `overlay`, STD-VER-011 vectors, `branch_info.overlay`), the tenant customisation line on them is PH-2 design; moved to PH-2 step 11 by DEC-087 |
| `not-met` | NR-OBS-002 | realised by DSN-NOD-612 and DSN-DVM-514 of PH-2 only; the PH-1 Server exports metrics over OTLP (step 16c) without a documented catalogue; moved to PH-2 step 11 by DEC-087 |
