---
id: UBS-PH-0
title: PH-0 — Foundations and De-risking
status: draft
phase: PH-0
depends_on: [UBS-PH-00, UBS-STD-00, UBS-SYS-DVM-00, UBS-SYS-BPA-00, DEC-028, DEC-030, DEC-035, DEC-036]
---

# PH-0 — Foundations and De-risking

## 1. Goal and business value

PH-0 proves, before the product is built, that the three riskiest technical bets hold: (1)
content-addressed, bitemporal, branchable state on PostgreSQL meets the latency
targets (ASM-003, DEC-020); (2) the core semantics (commit and compare-and-set, three-way
merge with ledger append, timeline algebra) are correct as specified; (3) the standard can be
tested language-neutrally. The business value is risk removal: every later phase builds on
measured facts, not on assumptions.

Measurable outcomes:
1. A storage and prolly-tree prototype meets at least 80% of the PH-1 read and commit targets
   on ENV-REF-SERVER (MET-PERF-001, MET-PERF-002).
2. Three formal models are checked with no violated property (MET-QUAL-001).
3. BPA release 0.1 exists with Core, Bitemporal and Ledger clauses and at least 300 vectors
   (MET-QUAL-002).
4. The conformance runner executes vectors against the prototype through the adapter protocol.
5. The Rhai sandbox passes a first escape suite of 100 cases (ASM-008).

## 2. Systems and components in scope

| System | Components | Depth |
|---|---|---|
| BPA | `bpa-standard` component (folder `standard/`, DEC-028), release pipeline, clauses for FND, OBJ, CLS (core), KIND, VER (commit, branch, merge), TIME, TXN (commit, CAS), PROOF (tree, roots) | new |
| DVM | prototype crates (folder `crates/`) `ubos-types`, `ubos-store` (PostgreSQL; SQLite follows in PH-4, DEC-024), `ubos-tree`, minimal `ubos-version` and `ubos-time` sufficient for vectors | prototype |
| FRG | conformance runner and adapter protocol (DSN-FRG-401, DSN-FRG-402), crate `ubos-forge`; adapter crate `ubos-dvm` and minimal `ubos-model` (DEC-033) | prototype |
| NOD | in-process host services (clock, entropy, blobs) for the prototype | prototype |
| `ubos-verify` | folder `verify/`: TLA+ models for commit/CAS, merge, timelines; environment definitions | new |

## 3. Feature list

The generated list is UBS-PHF-0 (8 FR, all in the STD domain). In addition, PH-0 builds the
following design items as prototypes; they are re-verified in PH-1 at production quality:

| Purpose | Design items |
|---|---|
| storage adapter, logical vs physical form, chunks, commit ordinals, kernel indexes, sequencer, prolly tree, snapshots | DSN-DVM-001…011 |
| commit construction, zero-copy branches, head CAS | DSN-DVM-201, DSN-DVM-204, DSN-DVM-208 |
| merge pipeline including ledger append | DSN-DVM-240…244 |
| timeline representation, edit algebra, as-of resolution | DSN-DVM-302, DSN-DVM-303, DSN-DVM-305 |
| formal models (merge, timelines, concurrency) | DSN-DVM-249, DSN-DVM-326, DSN-DVM-515 |
| Rhai host skeleton for the sandbox spike, crate `ubos-runtime` (DEC-034) | DSN-DVM-612 |
| standard repository, profiles, coverage, formal models, compatibility, standard-before-code | DSN-BPA-001…004, DSN-BPA-006, DSN-BPA-010 |
| runner and adapter | DSN-FRG-401, DSN-FRG-402 |

## 4. Dependencies

| Dependency | Kind | Needed by |
|---|---|---|
| requirements, architecture, standard drafts and DVM design in this volume | internal | start |
| ENV-REF-SERVER database host and one application node, rented by the owner (DEC-030) | external | step 5 (benchmarks) |
| TLA+ tooling (TLC, Apalache); models drafted by AI sessions and reviewed by an external formal-methods expert at acceptance (DEC-030, ASM-001) | external | start (tooling), acceptance (review) |
| workload generators for WL-GENERIC-10M (UBS-REQ-06) | internal | step 5 |

## 5. Out of scope for this phase

- Any network protocol (UBTP), SDKs, UI, authentication, multi-tenancy.
- Governance sheets, decisions, lifecycles, authorization.
- L3 WASM, AI, sync, federation, Control Plane, Exchange.
- Production packaging, installation, backups, operations tooling.
- Performance tuning beyond what is needed to decide ASM-003.

## 6. Deliverables

| Deliverable | Form |
|---|---|
| BPA 0.1 release | tag `bpa-v0.1.0` signed by the owner (DEC-030) over `standard/` with clauses, schemas and ≥ 300 vectors |
| Formal models | `verify/models/{commit_cas,merge,timeline}.tla` with model-checking reports |
| Storage prototype | prototype crates and benchmark harness in `crates/` of `ubos-unit` (DEC-028) |
| Benchmark report | report on ENV-REF-SERVER against the PH-1 NR targets, with the ASM-003 verdict (ENV-REF-BOX is not used: SQLite and Box are PH-4, DEC-024) |
| Conformance runner prototype | `forge conform` running the 0.1 vectors against the prototype through the adapter |
| Sandbox spike report | Rhai hardening findings and first escape-suite results (ASM-008 verdict) |
| Decision records | DEC entries confirming or revising DEC-020 and ASM-003, ASM-008 |

## 7. Verification plan

### 7.1 Methods used and why

| Method | Why in PH-0 |
|---|---|
| FORM | the core algorithms must be proven before code depends on them |
| CONF | the standard must be testable language-neutrally from the start |
| PROP | history independence of roots and timeline invariants are universal properties |
| BENCH | ASM-003 is a quantitative assumption |
| SEC (spike) | ASM-008 needs early evidence |
| INSP | clause quality and coverage review |

### 7.2 Suites and verification ranges

| Scope in PH-0 | Suite | VER range |
|---|---|---|
| commit/CAS, merge, timeline models | SUITE-BPA-MODELS | VER-FORM-9000…9049 |
| coverage report of 0.1 MUST clauses | SUITE-BPA-COVERAGE | VER-INSP-9000…9049 |
| 0.1 vectors on the prototype (PostgreSQL) | SUITE-DVM-CONF | standard vectors VER-CONF-0001…2699 (Core, Bitemporal, Ledger areas) |
| roots history independence, timeline non-overlap, merge determinism | SUITE-DVM-PROP | VER-PROP-0001…0099 |
| prototype benchmarks | SUITE-DVM-BENCH | VER-BENCH-0001…0049 |
| first L2 escape cases | SUITE-DVM-SEC | VER-SEC-0001…0099 |

VER numbering follows the allocation of the verification volume (UBS-VER-00 §3).

### 7.3 Environments and datasets

| Use | Environment |
|---|---|
| PostgreSQL benchmarks with WL-GENERIC-10M | ENV-REF-SERVER |
| model checking (TLC) and property tests | developer workstations |

Datasets: WL-GENERIC-10M generated with a fixed seed; history of 24 months at 10% monthly
change for bitemporal reads.

### 7.4 Pass thresholds

| Check | Threshold |
|---|---|
| formal models | 0 violations; stated bounds 3 branches × 4 commits × 3 objects for merge, 3 processes × 2 nodes for CAS, 3 objects × 5 operations for timelines, checked per DEC-031: exhaustively where TLC completes, otherwise exhaustively at the largest completing bound plus at least 1,000,000 simulated behaviours at the stated bound |
| vectors | 100% pass on PostgreSQL for the vectors marked `prototype` in 0.1 |
| properties | ≥ 1,000,000 generated cases per property, 0 failures |
| benchmarks | point read p95 ≤ 6.25 ms and simple commit p50 ≤ 19 ms (80% of NR-PERF-001 and NR-PERF-002 targets, measured as target ÷ 0.8) |
| sandbox spike | 100% of 100 initial escape cases blocked |

### 7.5 Acceptance

The Phase Acceptance Board (UBS-PH-00 §3.5) accepts PH-0. The benchmark report and model
reports are reviewed by the engineering lead and an external reviewer with database and
formal-methods expertise. Acceptance is recorded as a DEC entry with the evidence bundle hash.

### 7.6 Regression policy

The PH-0 vectors, properties and models become permanent members of the PH-1 suites.

## 8. Metrics

| Metric | Name | Target | Source |
|---|---|---|---|
| MET-PERF-001 | prototype point read p95 on WL-GENERIC-10M | ≤ 6.25 ms | benchmark report |
| MET-PERF-002 | prototype simple commit p50 at 500 commits/s | ≤ 19 ms | benchmark report |
| MET-PERF-003 | prototype diff of 10 changes in 10,000,000 objects p95 | ≤ 62.5 ms | benchmark report |
| MET-QUAL-001 | formal models checked without violation | 3 of 3 | model reports |
| MET-QUAL-002 | conformance vectors in BPA 0.1 | ≥ 300 | release manifest |
| MET-QUAL-003 | MUST clauses of 0.1 profiles covered by vectors | ≥ 80% | coverage report |
| MET-SEC-001 | L2 escape cases blocked | 100 of 100 | sandbox report |

## 9. Exit criteria

| Exit | Criterion | Evidence |
|---|---|---|
| EXIT-0-01 | BPA 0.1 is released, signed, with Core, Bitemporal and Ledger clauses and ≥ 300 vectors | release manifest and signature |
| EXIT-0-02 | The three formal models are checked with no violation at the stated state-space bounds, as defined by DEC-031 | model-checking reports |
| EXIT-0-03 | The prototype passes 100% of `prototype` vectors on PostgreSQL through the runner | signed runner reports |
| EXIT-0-04 | Root history independence holds in ≥ 1,000,000 generated cases | property report |
| EXIT-0-05 | Benchmarks meet MET-PERF-001…003, or a decision records the storage fallback of DEC-020. **Carried over to PH-1 (DEC-035).** | benchmark report and DEC entry |
| EXIT-0-06 | The Rhai sandbox spike blocks all 100 initial escape cases, or a decision revises ASM-008 | sandbox report and DEC entry |
| EXIT-0-07 | The PH-1 plan is re-baselined with the PH-0 findings (durations, risks) | updated UBS-PH-1 |

## 10. Risks and mitigations

| Risk | Description | L | I | Mitigation | Early warning | Owner |
|---|---|---|---|---|---|---|
| RSK-001 | PostgreSQL cannot meet commit latency with in-flush index and tree maintenance (raised to H and continued as RSK-016 by DEC-036) | H | H | group commit, deferred tree node writes, fallback log-structured layer (DEC-020) | MET-PERF-002 above 25 ms in the first prototype benchmark (step 4) | engineering lead |
| RSK-002 | Prolly-tree write amplification too high on hot keys | M | M | tune chunking parameters, cache upper levels, batch per commit | node writes per change > 6 | kernel engineer |
| RSK-003 | Formal models find a semantic defect late | L | H | model first, then code (steps 2 and 3 before step 4); model review before each code step | any violation | formal-methods engineer |
| RSK-004 | Rhai cannot be made deterministic without float support (**closed** by DEC-036: did not occur) | L | M | disable floats, decimal type, fork if needed (ASM-008) | spike findings | kernel engineer |
| RSK-005 | Vector format too implementation-specific | M | M | review by an external implementer; adapter protocol through UBTP messages | reviewer findings | standards steward |

## 11. Execution and team

Non-normative. PH-0 is executed by the product owner with AI sessions in eight steps, each
one pull request (DEC-030): (0) specification alignment; (1) monorepo skeleton with CI;
(2) formal models; (3) BPA 0.1 clauses, schemas and vectors; (4) prototype crates, adapter,
runner and property tests; (5) benchmarks on ENV-REF-SERVER; (6) sandbox spike; (7) PH-1
re-baseline and acceptance. An external reviewer with database and formal-methods expertise
takes part in acceptance. No calendar duration is fixed; progress is reported per step.
The earlier reference of 3 months with 6–8 engineers (ASM-001) is kept only for comparison.
