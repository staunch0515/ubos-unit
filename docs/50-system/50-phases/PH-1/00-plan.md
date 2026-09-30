---
id: UBS-PH-1
title: PH-1 — Kernel and Single-Node Server (gates PH-1a and PH-1b)
status: draft
phase: PH-1
depends_on: [UBS-PH-00, UBS-PH-0, UBS-SYS-DVM-00, UBS-SYS-NOD-00, UBS-PHR-0, DEC-035, DEC-036]
---

# PH-1 — Kernel and Single-Node Server

## 1. Goal and business value

PH-1 builds the reference kernel (DVM) to production quality and wraps it in a single-node
Server on PostgreSQL (DEC-024), so that the mechanism is proven end to end: typed, inherited,
bitemporal, branchable, provable business state with deterministic governed logic. It
delivers business goal **G1** (UBS-REQ-01 §7): 100% of the BPA conformance vectors of the
claimed profiles pass on the reference DVM on PostgreSQL. SQLite follows in PH-4 (DEC-024).

PH-1 has two sequential gates (DEC-014):
- **PH-1a Engine core** — the kernel and its in-process host: storage, model, versioning,
  time, ledger, transactions, L1/L2 runtime, proofs, portable export and import, Forge
  check/types/test, in-process host services on PostgreSQL.
- **PH-1b Governance and services** — governance sheets, decisions, lifecycles and ports,
  events and jobs, queries with authorization, UBTP server, SDKs, local identity, secrets,
  single-node Server operations, and an engineering UI for acceptance.

Measurable outcomes:
1. BPA 1.0-rc conformance: 100% of vectors of Core, Bitemporal, Ledger, Proof (roots,
   signatures, inclusion) and Governance profiles pass on PostgreSQL (MET-QUAL-010).
2. PH-1 NR performance targets met on ENV-REF-SERVER (MET-PERF-010…017).
3. 0 lost acknowledged commits in 10,000 injected crashes (NR-DUR-001, MET-QUAL-013).
4. 100% replay fidelity on 10,000 processes (NR-DET-001, MET-QUAL-014).
5. All 13 PH-1 scenarios pass on a single-node Server (MET-QUAL-015).

## 2. Systems and components in scope

| System | Components | Gate | Depth |
|---|---|---|---|
| BPA | 1.0-rc clauses for Core, Bitemporal, Ledger, Proof (subset), Governance; vectors covering all MUST clauses of these profiles | 1a, 1b | extended |
| DVM | all engine crates at production quality for storage, model, versioning, time, ledger, transactions, runtime (L1, L2), proofs, export/import (1a); rules, flow core, authorization (RBAC, ABAC), query, view resolution basics (1b) | 1a, 1b | new |
| NOD | in-process host and CTR-002 host services on PostgreSQL (1a); Server single node: UBTP server, local authentication, sessions, secrets, jobs, schedules, outbox release, events, blobs, telemetry, health, backup and PITR, configuration, genesis Buks (1b) | 1a, 1b | new |
| FRG | check, types, test, verification records for L2, conformance runner, `forge dev` (1b) | 1a, 1b | new |
| SDK | Rust and TypeScript SDKs over UBTP | 1b | new |
| WSP | engineering UI (generated forms, lists, details, history) used only for acceptance | 1b | prototype |
| NTY | verifier library functions (inclusion proofs, signatures) | 1a | prototype |

## 3. Feature list

The generated list is UBS-PHF-1: 304 FR, 32 NR, 5 CR and 13 scenarios, each tagged with its
gate. Gate assignment of FR follows DEC-014 (domains and capability exceptions). Gate
assignment of NR and CR follows this rule: NR areas DET, MAINT, PORT and COMPAT, and the
items NR-PERF-001, -002, -004, -005, -006, -007, -010, NR-DUR-001, NR-SEC-002 and
NR-SEC-004 are verified at gate 1a; all other PH-1 NR and CR items at gate 1b.

Scenario assignment to gates:

| Gate | Scenarios |
|---|---|
| 1a (verified through the in-process binding and Forge tests) | SCN-002, SCN-003, SCN-004, SCN-005, SCN-006, SCN-007, SCN-008, SCN-010, SCN-011, SCN-018 |
| 1b (verified on a single-node Server through UBTP and SDKs) | SCN-001, SCN-009, SCN-017, and a re-run of all 1a scenarios over UBTP |

Design coverage: all DSN-DVM items with **Phase** PH-1; DSN-NOD items with **Phase** PH-1;
DSN-FRG-001…004, DSN-FRG-101…103, DSN-FRG-201…204, DSN-FRG-401…404, DSN-FRG-501;
DSN-SDK items with **Phase** PH-1; DSN-BPA-003, DSN-BPA-004, DSN-BPA-010.

## 4. Dependencies

| Dependency | Kind | Needed by |
|---|---|---|
| PH-0 exit criteria EXIT-0-01…04, EXIT-0-06 and EXIT-0-07; EXIT-0-05 carried over (DEC-035) | prior exit | start |
| DEC entries from PH-0 on sandbox (DEC-034) and re-baseline (DEC-036) | decision | start |
| carried VER-BENCH-0001 on ENV-REF-SERVER and its decision on storage (DEC-020, ASM-003) | prior exit | before the storage layout is fixed; at the latest gate 1a |
| ENV-REF-SERVER provisioned (PH-0 set-up `verify/envs/ENV-REF-SERVER-ph0.md`, CPU with SHA extensions) | external | start (DEC-036) |
| ENV-REF-SMALL, ENV-REF-BOX reference environments provisioned and scripted | external | month 2 |
| fault-injection harness (process kill, power-loss simulation via dm-flakey or equivalent) | internal | month 3 |
| Cedar-compatible policy engine library selected (DEC-021) | external | gate 1b start |
| PostgreSQL 16+ | external | start |

## 5. Out of scope for this phase

- HA Server, REST facade, webhooks, OIDC/SAML/SCIM, notifications, file scanning and
  previews, text search (PH-2).
- Lenses, decision tables, approvals and deferred commits, workflows, sagas, ReBAC, masking,
  crypto-shredding, simulation (PH-2).
- Production Workspace and Studio, Buks of the verticals (PH-2).
- L3 WASM, AI, legal holds, anchoring (PH-3).
- SQLite adapter, Box (headless, desktop, mobile), device sync (PH-4, DEC-024).
- Cells, Control Plane, Exchange, workers, tiering (PH-4); federation and browser kernel (PH-5).

## 6. Deliverables

| Deliverable | Gate |
|---|---|
| BPA 1.0-rc1 (engine profiles) with vectors | 1a |
| `ubos` kernel crates release `0.1.0` (engine) and `ubos-box` headless | 1a |
| Forge `0.1` (check, types, test, conform) | 1a |
| Benchmark report 1a on ENV-REF-SERVER and ENV-REF-BOX | 1a |
| Fault-injection and simulation reports 1a | 1a |
| BPA 1.0-rc2 (adds Governance profile) | 1b |
| `ubos-node` Server `0.2.0` single node, with installation guide and operator CLI | 1b |
| Rust and TypeScript SDKs `0.2` | 1b |
| Engineering UI (internal) | 1b |
| Security report (L2 escape suite, isolation suite, secret scanning) | 1b |
| Scenario run report (all PH-1 scenarios over UBTP) | 1b |

## 7. Verification plan

### 7.1 Methods used and why

| Method | Gate | Why |
|---|---|---|
| CONF | 1a, 1b | G1 is defined by conformance; vectors are the contract for third parties |
| PROP | 1a, 1b | algebraic properties of timelines, merges, balances, roots and row filters |
| SIM | 1a, 1b | concurrency and crash interleavings cannot be covered by examples |
| FORM | 1a | models from PH-0 are extended to group commit and ledger sequences |
| FAULT | 1a, 1b | durability claims (NR-DUR-001) require real crashes on real databases |
| BENCH | 1a, 1b | NR performance targets |
| SEC | 1a, 1b | sandbox containment (1a), isolation, secrets and security events (1b) |
| SCN | 1a, 1b | end-to-end evidence of the PH-1 scenarios |
| INSP | 1a, 1b | dependency graph, coverage, configuration validation |

### 7.2 Suites and verification ranges

| Scope | Suite | Gate | VER range |
|---|---|---|---|
| standard vectors of Core, Bitemporal, Ledger, Proof (subset) and the instruction set | SUITE-DVM-CONF | 1a | VER-CONF-0001…4399 (clause areas FND…CTX) |
| storage adapter parity | SUITE-DVM-STORE | 1a | VER-CONF-8400…8499 |
| kernel properties | SUITE-DVM-PROP | 1a | VER-PROP-0100…0399 |
| concurrency and crash simulation (10,000 seeds per night) | SUITE-DVM-SIM | 1a | VER-SIM-0001…0199 |
| real-database fault matrix | SUITE-DVM-FAULT | 1a | VER-FAULT-0001…0199 |
| extended formal models | SUITE-DVM-FORM | 1a | VER-FORM-0001…0099 |
| kernel benchmarks | SUITE-DVM-BENCH | 1a | VER-BENCH-0100…0199 |
| L2 sandbox escape suite (≥ 300 cases) | SUITE-DVM-SEC | 1a | VER-SEC-0100…0499 |
| replay corpus across OS and CPU | SUITE-DVM-REPLAY | 1a | VER-CONF-8500…8549 |
| Forge commands and checks | SUITE-FRG-CLI, SUITE-FRG-CHECK | 1a | VER-CONF-7700…7799 |
| standard vectors of expressions, Governance profile, addressing, UBTP and errors | SUITE-DVM-CONF | 1b | VER-CONF-4400…5599, VER-CONF-6000…6099 |
| UBTP protocol | SUITE-NOD-PROTO | 1b | VER-CONF-7000…7099 |
| isolation suite | SUITE-NOD-ISO | 1b | VER-SEC-2000…2099 |
| authentication and sessions (local) | SUITE-NOD-AUTH | 1b | VER-SEC-2100…2199 |
| jobs, schedules, timers | SUITE-NOD-JOBS | 1b | VER-SIM-2000…2099 |
| events and subscriptions | SUITE-NOD-EVENTS | 1b | VER-SIM-2100…2199 |
| install, backup, PITR | SUITE-NOD-OPS | 1b | VER-FAULT-2000…2099 |
| SDK protocol and network faults | SUITE-SDK-PROTO, SUITE-SDK-NET | 1b | VER-CONF-7500…7599, VER-FAULT-4500…4549 |
| edge benchmarks | SUITE-NOD-BENCH | 1b | VER-BENCH-2000…2099 |
| PH-1 scenarios | SUITE-PH1-SCN | 1a, 1b | VER-SCN-1000…1099 |

VER numbering follows the allocation of the verification volume (UBS-VER-00 §3).

### 7.3 Environments and datasets

| Use | Environment |
|---|---|
| kernel and edge benchmarks, fault matrix on PostgreSQL | ENV-REF-SERVER |
| single-node Server installation and scenario runs | ENV-REF-SMALL |

Datasets and workloads: WL-GENERIC-10M with 24 months of history; WL-MIXED-OLTP load
profile; a 10,000-process replay corpus recorded from the scenario suite and synthetic
generators; the fixture models of the conformance package.

### 7.4 Pass thresholds

Gate 1a:

| Check | Threshold |
|---|---|
| engine-profile vectors | 100% pass on PostgreSQL |
| storage suite | 100% pass on PostgreSQL |
| properties | ≥ 1,000,000 cases per property per release candidate; 0 failures (NR-DET-003) |
| simulation | 10,000 seeds per night for 30 consecutive nights with 0 violations; 1,000,000 seeds before gate |
| fault matrix | 0 acknowledged commits lost across 10,000 injected crashes (NR-DUR-001) |
| replay | 100% identical outcome hashes on ≥ 10,000 processes, on Linux x86-64, Linux ARM64 and macOS ARM64 (NR-DET-001, NR-DET-002) |
| performance | NR-PERF-001, -002, -004, -005, -006, -007, -010 targets met on ENV-REF-SERVER |
| sandbox | 100% of ≥ 300 L2 escape cases blocked (NR-SEC-002) |
| maintainability | ≥ 85% line coverage and ≥ 70% mutation score for core crates; 0 dependency-graph violations (NR-MAINT-002, NR-MAINT-003) |
| portability | export from one PostgreSQL VAE imported into another yields identical head roots in 100% of test VAEs (NR-PORT-003, PH-1 part) |

Gate 1b:

| Check | Threshold |
|---|---|
| Governance-profile vectors | 100% pass on PostgreSQL |
| UBTP and SDK suites | 100% pass on WebSocket, HTTP and in-process bindings |
| isolation | 100% pass; 0 cross-tenant access (NR-SEC-003) |
| secrets | 0 matches of secret scanning over logs and traces of the full scenario suite (NR-SEC-009) |
| security events | 100% of FR-AUD-012 event types present (NR-SEC-010) |
| performance | NR-PERF-008, -009, -011, -012 targets met |
| observability | ≥ 99% of requests traced end to end (NR-OBS-001) |
| configuration | 100% of ≥ 200 invalid configurations rejected (NR-OPER-003) |
| operations | PITR restore verified at 10 sampled points; diagnostic bundle ≤ 5 minutes (NR-DUR-004, NR-OPER-005) |
| scenarios | all 13 PH-1 scenarios pass over UBTP on ENV-REF-SMALL |

### 7.5 Acceptance

Gate 1a is accepted by the Phase Acceptance Board with the engineering, quality and security
leads, on the evidence of signed runner reports, simulation and fault reports, benchmark
reports and the coverage report. Gate 1b is accepted by the same board plus the operations
lead, adding the scenario run report and the security report. Both acceptances are recorded
as DEC entries.

### 7.6 Regression policy

From gate 1a, every kernel pull request runs the fast conformance subset (≤ 10 minutes),
properties (short mode) and the dependency-graph check; every night runs the full vectors,
properties (long mode), 10,000 simulation seeds and the benchmark suite. A benchmark
regression over 10% or any simulation failure blocks the next release candidate.

## 8. Metrics

| Metric | Name | Target | Gate |
|---|---|---|---|
| MET-QUAL-010 | conformance vectors passing, claimed profiles, PostgreSQL | 100% | 1a, 1b |
| MET-QUAL-011 | simulation seeds without violation before gate | ≥ 1,000,000 | 1a |
| MET-QUAL-012 | core crate mutation score | ≥ 70% | 1a |
| MET-QUAL-013 | acknowledged commits lost in fault injection | 0 of ≥ 10,000 crashes | 1a |
| MET-QUAL-014 | replay fidelity on the replay corpus | 100% | 1a |
| MET-QUAL-015 | PH-1 scenarios passing over UBTP | 13 of 13 | 1b |
| MET-PERF-010 | point read p95 / p99 (NR-PERF-001) | ≤ 5 ms / ≤ 20 ms | 1a |
| MET-PERF-011 | simple commit p50 / p99 at 500 commits/s (NR-PERF-002) | ≤ 15 ms / ≤ 60 ms | 1a |
| MET-PERF-012 | branch creation p99 at 10 M objects (NR-PERF-004) | ≤ 10 ms | 1a |
| MET-PERF-013 | diff of 10 changes in 10 M objects p95 (NR-PERF-005) | ≤ 50 ms | 1a |
| MET-PERF-014 | merge preview of 1,000 objects p95 (NR-PERF-006) | ≤ 2 s | 1a |
| MET-PERF-015 | bitemporal read overhead (NR-PERF-007) | ≤ 2× point read | 1a |
| MET-PERF-016 | indexed query first 100 rows p95 (NR-PERF-008) | ≤ 100 ms | 1b |
| MET-PERF-017 | commit to handler start p95 (NR-PERF-011) | ≤ 500 ms | 1b |
| MET-SEC-010 | L2 escape cases blocked | 100% of ≥ 300 | 1a |
| MET-SEC-011 | cross-tenant accesses in the isolation suite | 0 | 1b |
| MET-OPS-010 | invalid configurations rejected | 100% of ≥ 200 | 1b |

## 9. Exit criteria

| Exit | Gate | Criterion | Evidence |
|---|---|---|---|
| EXIT-1-01 | 1a | BPA 1.0-rc1 released with vectors covering 100% of MUST clauses of Core, Bitemporal, Ledger and Proof (roots, signatures, inclusion) | release manifest, coverage report |
| EXIT-1-02 | 1a | 100% of engine-profile vectors pass on PostgreSQL | signed runner reports |
| EXIT-1-03 | 1a | Storage suite passes 100% on PostgreSQL, with the storage trait free of backend-specific types (inspection) | SUITE-DVM-STORE report, inspection record |
| EXIT-1-04 | 1a | 0 property failures at ≥ 1,000,000 cases per property | property report |
| EXIT-1-05 | 1a | 1,000,000 simulation seeds with 0 violations, including 30 consecutive clean nights | simulation report |
| EXIT-1-06 | 1a | 0 lost acknowledged commits in ≥ 10,000 injected crashes on PostgreSQL | fault report |
| EXIT-1-07 | 1a | Extended formal models (group commit, ledger sequences) checked with 0 violations | model reports |
| EXIT-1-08 | 1a | 100% replay fidelity across three platforms on ≥ 10,000 processes | replay report |
| EXIT-1-09 | 1a | MET-PERF-010…015 met on ENV-REF-SERVER | benchmark report |
| EXIT-1-10 | 1a | 100% of ≥ 300 L2 escape cases blocked | security report |
| EXIT-1-11 | 1a | Coverage ≥ 85% and mutation score ≥ 70% for core crates; 0 dependency-graph violations | CI reports |
| EXIT-1-12 | 1a | Portable export/import round trip between PostgreSQL VAEs yields identical roots | round-trip report |
| EXIT-1-13 | 1a | Gate-1a scenarios (10) pass through the in-process binding | scenario report |
| EXIT-1-14 | 1b | BPA 1.0-rc2 adds the Governance profile with full MUST coverage; 100% pass on PostgreSQL | release manifest, runner reports |
| EXIT-1-15 | 1b | UBTP and SDK suites pass on all PH-1 bindings | suite reports |
| EXIT-1-16 | 1b | Isolation suite 100% pass; 0 secret-scanning matches; 100% security event coverage | security report |
| EXIT-1-17 | 1b | MET-PERF-016 and MET-PERF-017 met, and NR-PERF-009 and NR-PERF-012 met | benchmark report |
| EXIT-1-18 | 1b | Single-node Server installed from the guide on ENV-REF-SMALL; PITR verified at 10 points; diagnostic bundle ≤ 5 minutes | operations report |
| EXIT-1-19 | 1b | All 13 PH-1 scenarios pass over UBTP on the single-node Server | scenario report |
| EXIT-1-20 | 1b | Traceability report shows every PH-1 Must requirement covered by at least one passing verification item | traceability report |

## 10. Risks and mitigations

| Risk | Description | L | I | Mitigation | Early warning | Owner |
|---|---|---|---|---|---|---|
| RSK-010 | PH-1 scope (341 requirements) exceeds capacity | H | H | two gates (DEC-014); weekly burn-up per gate; Should items carried over by decision | burn-up below 70% of plan at month 3 | product owner |
| RSK-011 | Determinism breaks across platforms (decimal, time zones, hashing) | M | H | replay corpus on three platforms nightly; decimal library with vectors; pinned tzdb | any replay divergence | kernel lead |
| RSK-012 | Group commit and sequencer create latency spikes under contention | M | M | per-branch queues with priorities; leader lease; SIM of hot branches | p99 commit > 60 ms in nightly bench | kernel engineer |
| RSK-013 | Policy engine partial evaluation too slow for list queries | M | M | pre-compiled residuals cached by policy hash; index-friendly policy patterns | NR-PERF-008 miss in 1b benchmarks | kernel engineer |
| RSK-014 | Standard and code drift | M | H | standard-before-code rule (DSN-BPA-010) enforced in CI | kernel PR without accepted standard change | standards steward |
| RSK-015 | Fault-injection harness gives false confidence | L | H | use two independent crash methods (SIGKILL and dm-flakey power loss); external review | none found in 10,000 crashes too early | quality lead |
| RSK-016 | PostgreSQL commit path misses NR-PERF-002 at 500 commits/s: the PH-0 trial is bound by sequencer CPU (continues RSK-001, DEC-036) | H | H | group commit (built in PH-0); parallel preparation of the commits of a group with only the tree update serialised (W-8); deferred tree maintenance; fallback of DEC-020 | carried VER-BENCH-0001 (DEC-035) below 500 commits/s | kernel lead |
| RSK-017 | Tree maintenance is bound by SHA-256 throughput; hosts without SHA extensions fall far short | M | M | require SHA extensions in ENV-REF-SERVER; canonical node hashing without intermediate values (PH-0); cache upper levels | commit CPU share of hashing above 50% on the reference host | kernel engineer |

## 11. Reference duration and team assumption

**Re-baseline (DEC-036):** PH-0 findings and their dispositions are in UBS-PHR-0; its work
items W-1…W-12 belong to this phase. W-1 (the carried storage benchmark) comes first. As in
PH-0, the phase is executed by the product owner with AI sessions and reported per step;
the figures below remain a reference only.

Non-normative. 9 months: gate 1a after 5 months, gate 1b after 4 more. Team of 10–12
engineers: 5 kernel (Rust), 1 formal methods, 2 node/host, 1 SDK/TypeScript, 1 quality and
performance, 1 security (half-time), the standards steward, the engineering lead and the
product owner.
