---
id: UBS-VER-04
title: Verification Volume — Suite Registry
status: draft
phase: ALL
depends_on: [UBS-VER-00, UBS-VER-01]
---

# Verification Volume — Suite Registry

## 1. Phase-level suites (defined here)

These suites combine system suites into the scenario, pilot and acceptance runs named by the
phase plans.

| Suite | Content | Methods | Phase | Trigger | Owner |
|---|---|---|---|---|---|
| SUITE-PH1-SCN | the 13 PH-1 scenarios: gate-1a scenarios through the in-process binding, then all 13 over UBTP on a single-node Server | SCN | PH-1 | nightly from gate-1a start; release candidates | quality lead |
| SUITE-PH2-FINANCE | Basic Finance scenarios SCN-501…510 on the HA Server with the Northwind sample company and WL-FIN-M; statements, agings and 1099 summary compared with the signed expected results to the cent | SCN, BENCH, INSP | PH-2 | nightly; release candidates | finance Buk lead |
| SUITE-PH2-FUND | **Deferred (DEC-022).** fund-operations scenarios SCN-101…110 with WL-FUND-M and vertical fixtures | SCN, BENCH | deferred | when a decision schedules the package | — |
| SUITE-PH2-CONTRACTS | **Deferred (DEC-022).** contract scenarios SCN-201…208 with WL-CONTRACT-M, including independent seal verification | SCN, INSP | deferred | when a decision schedules the package | — |
| SUITE-PH3-PILOT | CPA audit drill and demonstration operation: monthly and year-end close, restatement, auditor requests with verified packs, re-performance of statements, agent acceptance and revert rates | PILOT, SCN | PH-3 | daily during the demonstration year; at each auditor request | audit-drill lead |
| SUITE-PH4-PLATFORM | platform scenarios SCN-403, SCN-404, SCN-407, SCN-410 on Cells with CTL and EXC | SCN, FAULT | PH-4 | nightly; release candidates | platform lead |
| SUITE-PH5-FED | federation scenarios SCN-408, SCN-409, SCN-411 across two organisations' nodes | SCN, SIM | PH-5 | nightly; release candidates | federation lead |
| SUITE-REGRESSION | union of all suites of completed phases, run on every release candidate (UBS-PH-00 §3.6) | all | PH-1…PH-5 | release candidates | quality lead |

## 2. System suites (defined in the system chapters)

| System | Suite | Content | Methods |
|---|---|---|---|
| DVM | SUITE-DVM-CONF | standard conformance vectors (VER-CONF-*), all profiles the phase claims | CONF |
| DVM | SUITE-DVM-STORE | storage adapter suite, identical on PostgreSQL and (from PH-4, DEC-024) SQLite | CONF |
| DVM | SUITE-DVM-PROP | property tests: timelines, merge, diff, tree history independence, balances, cursors, row filters | PROP |
| DVM | SUITE-DVM-SIM | deterministic simulation of concurrency, crashes and clock faults | SIM |
| DVM | SUITE-DVM-FAULT | fault injection on real PostgreSQL, SQLite and object store: kill -9, disk full, network partitions, corrupt chunks | FAULT |
| DVM | SUITE-DVM-FORM | TLA+ models: commit and CAS, merge, timelines, sagas, approvals | FORM |
| DVM | SUITE-DVM-BENCH | NR-PERF, NR-SCAL and NR-COST workloads on ENV-REF-SERVER, ENV-REF-SMALL and ENV-REF-BOX | BENCH |
| DVM | SUITE-DVM-SEC | sandbox escape suite (L2, L3), permission matrix, isolation suite, masking leak tests | SEC |
| DVM | SUITE-DVM-REPLAY | replay corpus across OS, CPU and browser build | CONF |
| DVM | SUITE-DVM-XIMPL | cross-implementation equivalence: export from one kernel build, import into another, compare roots and query results | CONF |
| NOD | SUITE-NOD-HOST | in-process host services on PostgreSQL: tenant registry, key hierarchy, node signer, blobs, secrets, journaled connectors, event log, jobs, telemetry redaction (PH-1 step 7a, DEC-060) | CONF, SEC |
| NOD | SUITE-NOD-PROTO | UBTP conformance on every binding; REST equivalence; version skew | CONF |
| NOD | SUITE-NOD-ISO | tenant isolation suite through every interface (DSN-NOD-406) | SEC |
| NOD | SUITE-NOD-AUTH | authentication, sessions, revocation latency, SCIM compliance, OAuth flows, step-up | SEC, CONF |
| NOD | SUITE-NOD-JOBS | jobs, leases with fencing, schedules exactly-once, timers, dead letters, poison detection | SIM, FAULT |
| NOD | SUITE-NOD-EVENTS | outbox release ordering, webhooks, brokers, replay, masking per recipient | SIM, FAULT, SEC |
| NOD | SUITE-NOD-FILES | resumable uploads, scanning, previews, WORM, download links | CONF, SEC |
| NOD | SUITE-NOD-SYNC | device sync conformance, resumability, scope reduction and wipe, efficiency (from PH-4, DEC-024) | CONF, SIM, BENCH |
| NOD | SUITE-NOD-OPS | install, bootstrap, backup and PITR, failover, split-brain, rolling upgrade, rollback, DR drill | FAULT, SCN |
| NOD | SUITE-NOD-BENCH | end-to-end NR-PERF through the edge; fairness; subscription latency | BENCH |
| NOD | SUITE-NOD-PEN | external penetration test and egress SSRF tests | SEC |
| CTL | SUITE-CTL-BLIND | architecture inspection and penetration test proving no business-data path | INSP, SEC |
| CTL | SUITE-CTL-ORCH | lifecycle, migration and wave orchestration with CTL and node crashes at every step | FAULT, SIM |
| CTL | SUITE-CTL-USAGE | chain verification, deduplication, gaps, air-gapped files, rating reproducibility | CONF, SEC |
| CTL | SUITE-CTL-SCALE | 1,000 nodes and 10,000 tenants simulated | BENCH |
| FRG | SUITE-FRG-CLI | every command, JSON schemas, exit codes | CONF |
| FRG | SUITE-FRG-CHECK | a corpus of faulty projects with expected findings per rule ID | CONF |
| FRG | SUITE-FRG-REPRO | pack and verify reproducibility across OS and CPU | CONF |
| FRG | SUITE-FRG-RESOLVE | dependency solver properties and conflict explanations | PROP |
| FRG | SUITE-FRG-CONFORM | runner against the reference DVM and a sample external adapter | CONF |
| FRG | SUITE-FRG-DX | developer loop benchmarks (NR-PERF-016) and usability sessions with developers (NR-USE-004, NR-USE-005) | BENCH, USE |
| SDK | SUITE-SDK-PROTO | protocol suite shared by all SDKs against a reference node: every operation, error mapping, retries, idempotency, paging | CONF |
| SDK | SUITE-SDK-NET | network faults (drops, latency, partitions) with reconnection and resume | FAULT, SIM |
| SDK | SUITE-SDK-SKEW | SDK N against node N−1 and N+1 minor | CONF |
| SDK | SUITE-SDK-TYPES | generated types compile and narrow correctly for the fixture models | CONF |
| SDK | SUITE-SDK-SIZE | bundle size budget (DSN-SDK-017) | BENCH |
| WSP | SUITE-WSP-E2E | Playwright end-to-end suites for the reference scenarios (fund back office, contracts) on web, desktop and mobile | SCN |
| WSP | SUITE-WSP-A11Y | automated axe checks per widget and screen; manual audit per release | INSP, USE |
| WSP | SUITE-WSP-RENDER | deterministic rendering (byte-equal PDFs), PDF/A and PDF/UA validation | CONF |
| WSP | SUITE-WSP-PERF | lab and field performance budgets | BENCH |
| WSP | SUITE-WSP-USE | usability studies with business users, approvers and auditors (NR-USE-001…003, NR-USE-007) | USE |
| WSP | SUITE-WSP-SEC | portal scoping, widget sandbox, XSS and CSP tests | SEC |
| STU | SUITE-STU-E2E | end-to-end builder scenarios: new class with lens, sheet change with simulation, decision table with gaps, release | SCN |
| STU | SUITE-STU-USE | builder productivity and learnability studies (NR-USE-004, NR-USE-005) | USE |
| STU | SUITE-STU-PATH | inspection that no Studio operation bypasses the commit path | INSP |
| AGT | SUITE-AGT-GOV | classification blocking and redaction, region pinning, budget enforcement, no-merge rule, scope denial | SEC |
| AGT | SUITE-AGT-KILL | kill switch latency and completeness under load | BENCH, SEC |
| AGT | SUITE-AGT-INJECT | prompt-injection corpus against agents with side-effect tools | SEC |
| AGT | SUITE-AGT-MCP | MCP conformance and delegation expiry | CONF |
| AGT | SUITE-AGT-EVAL | evaluation suites for the reference agents (reconciliation assistant, contract clause reviewer) | CONF |
| AGT | SUITE-AGT-SCN | agent scenarios SCN-3xx end to end | SCN |
| EXC | SUITE-EXC-PUB | publishing flow, namespace enforcement, immutability | CONF, SEC |
| EXC | SUITE-EXC-VERIFY | reproducibility of verification across workers | CONF |
| EXC | SUITE-EXC-LIC | licence signing, offline validation, grace and expiry behaviour on nodes | CONF, SEC |
| EXC | SUITE-EXC-MONEY | invoicing reconciliation with CTL, payouts balance in the ledger | CONF |
| FED | SUITE-FED-SHARED | shared-BPU protocol with 2–5 parties, partitions and crashes; equal roots after convergence | SIM, FORM |
| FED | SUITE-FED-TRUST | trust enforcement, DID-bound mTLS, revoked keys | SEC |
| FED | SUITE-FED-FALLBACK | remote parent outages and reconciliation | SIM |
| FED | SUITE-FED-PILOT | a two-organisation pilot (fund administrator and asset manager sharing investor registers) | PILOT |
| NTY | SUITE-NTY-ANCHOR | anchoring with TSA faults; receipt verification | CONF, FAULT |
| NTY | SUITE-NTY-PACK | pack reproducibility, redaction, tamper detection corpus | CONF, SEC |
| NTY | SUITE-NTY-VERIFIER | verifier against the conformance vectors for proofs and packs; offline web build | CONF |
| NTY | SUITE-NTY-EXAM | mock examination: produce records within 1 business day | SCN |
| BRG | SUITE-BRG-MIG | reference migration corpus (fund administrator legacy extracts): inference accuracy, idempotency, history load, reconciliation | SCN, CONF |
| BRG | SUITE-BRG-ADAPT | financial file adapters on sample corpora; crash-resume | CONF, FAULT |
| BRG | SUITE-BRG-EXPORT | exactly-once CDC export under faults; schema evolution; masking | FAULT, SEC |
| BPA | SUITE-BPA-COVERAGE | every MUST clause covered by vectors per profile | INSP |
| BPA | SUITE-BPA-MODELS | formal models checked; model traces pass on the reference DVM | FORM, CONF |
| BPA | SUITE-BPA-COMPAT | vectors of previous minor releases pass on each new release | CONF |
| BPA | SUITE-BPA-CERT | dry-run certification of the reference DVM and one independent implementation (PH-4) | INSP, CONF |

## 3. Triggers

| Trigger | Suites |
|---|---|
| every change (pull request) | fast subsets: conformance smoke (≤ 10 min), short properties, unit tests, dependency-graph check, secret scan |
| nightly | full conformance, long properties, 10,000 simulation seeds, fault matrix subset, benchmarks, E2E suites |
| weekly | 1,000,000 simulation seeds, full fault matrix, formal models full state space, DR drill (HA environments) |
| release candidate | SUITE-REGRESSION plus the phase suites of the current phase |
| phase gate | everything above plus pen tests, usability studies, audits and pilots named in the phase plan |
