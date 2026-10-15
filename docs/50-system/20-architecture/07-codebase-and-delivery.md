---
id: UBS-ARC-07
title: Codebase Organisation and Delivery
status: draft
phase: PH-0
depends_on: [UBS-ARC-01, DEC-007]
---

# Codebase Organisation and Delivery

## 1. Repository and components (DEC-018, DEC-028)

All components live in the single repository `staunch0515/ubos-unit` (DEC-028). The
component names below are used throughout this volume; each maps to a top-level folder.

| Component | Folder in `ubos-unit` | Content | Licence (DEC-019) |
|---|---|---|---|
| `bpa-standard` | `standard/` | normative text, grammars, schemas, profiles, conformance vectors, formal models | text CC BY 4.0; vectors and schemas Apache-2.0 |
| `ubos` (monorepo) | `crates/`, `packages/` | kernel crates, node hosts, Forge, SDKs, clients, platform services | kernel, node and Forge: AGPL-3.0 open edition plus commercial licence; SDKs Apache-2.0 |
| `ubos-verify` | `verify/` | open verifier (CLI, library, static web page) | Apache-2.0 |
| `ubos-buks` | `buks/` | first-party Buks: `system`, `ontology`, `finance` (`fund-ops` and `contracts` deferred, DEC-022) | commercial (source available to customers) |
| `ubos-docs` | `docs/` | this specification, the finance requirements (`docs/finance-requirements/`, self-contained, DEC-029), tools and reports | internal |

## 2. Rust crates of the monorepo

| Crate | Layer | Responsibility | May depend on |
|---|---|---|---|
| `ubos-types` | engine | IDs, URIs, value types, canonical JSON (RFC 8785), hashing, error type | — |
| `ubos-crypto` | engine | signing, verification, AEAD encryption, key-service traits | `ubos-types` |
| `ubos-store` | engine | storage traits; PostgreSQL and SQLite adapters; chunk store; kernel index maintenance | `ubos-types`, `ubos-crypto` |
| `ubos-tree` | engine | prolly tree: roots, diffs, inclusion and consistency proofs, have/want | `ubos-types`, `ubos-store` |
| `ubos-model` | engine | classes, traits, C3, value types, constraints, extensions, lenses, reflection, schema export | `ubos-types` |
| `ubos-version` | engine | commits, branches, change sets, merge, revert, inheritance resolution | `ubos-model`, `ubos-store`, `ubos-tree` |
| `ubos-time` | engine | bitemporal timelines, `asof`/`time` resolution, retroactive impact | `ubos-version` |
| `ubos-ledger` | engine | ledger classes, sequencing, reversal, double entry, periods, balance projections | `ubos-version` |
| `ubos-expr` | engine | L1 expression language: parser, type checker, evaluator | `ubos-model` |
| `ubos-rules` | engine | governance sheets, cascade, validations, invariants, decision tables, decisions | `ubos-expr`, `ubos-model` |
| `ubos-authz` | engine | policy engine integration (the `cedar-policy` crate, DEC-065), relations, masking, row filters | `ubos-model` |
| `ubos-runtime` | engine | instruction set, `ctx` and `sys`, Rhai host, WASM host, profiles, metering, journal, replay, static analysis | `ubos-expr`, `ubos-authz` |
| `ubos-flow` | engine | lifecycles, ports, guards, approvals, tasks, workflows, sagas | `ubos-runtime`, `ubos-rules` |
| `ubos-query` | engine | criteria queries, aggregation, traversal, history queries, pagination | `ubos-time`, `ubos-expr`, `ubos-model`, `ubos-crypto`, `ubos-authz` |
| `ubos-txn` | engine | processes, unit of work, flush orchestration, CAS, idempotency, outbox, provenance | all engine crates above |
| `ubos-proof` | engine | evidence formats, seal format, verification primitives (shared with `ubos-verify`) | `ubos-tree`, `ubos-crypto` |
| `ubos-dvm` | engine | host API facade (CTR-001) and host-service traits (CTR-002) | engine crates |
| `ubos-proto` | host | UBTP message schemas and codecs | `ubos-types` |
| `ubos-node` | host | Server and Cell host: transports, sessions, jobs, schedules, connectors, secrets, telemetry, sync, admin endpoint | `ubos-dvm`, `ubos-proto` |
| `ubos-box` | host | desktop and mobile host (Tauri 2) | `ubos-dvm`, `ubos-proto` |
| `ubos-worker` | host | worker role | `ubos-runtime`, `ubos-proto` |
| `ubos-wasm-kernel` | host | browser kernel packaging | `ubos-dvm` |
| `ubos-sdk` | host | Rust SDK | `ubos-proto` |
| `ubos-forge` | standard | CLI and library: check, types, test, simulate, pack, verify, conform, publish | `ubos-dvm`, `ubos-sdk` |
| `ubos-lsp` | standard | language server | `ubos-forge` |
| `ubos-agent-hub` | intelligence | AGT service | `ubos-sdk`, `ubos-proto` |
| `ubos-notary` | services | NTY service | `ubos-sdk`, `ubos-proof` |
| `ubos-bridge` | services | BRG service | `ubos-sdk` |
| `ubos-exchange` | services | EXC service | `ubos-sdk`, `ubos-proof` |
| `ubos-control` | services | CTL service | `ubos-proto` (admin contract only) |
| `ubos-fed` | services | FED registry and node module | `ubos-proto`, `ubos-crypto` |

## 3. TypeScript packages

| Package | Responsibility | May depend on |
|---|---|---|
| `@ubos/sdk` | UBTP client, typed models, subscriptions, offline queue | generated protocol types |
| `@ubos/render` | widget catalogue renderers (React), view protocol | `@ubos/sdk` |
| `@ubos/workspace` | Workspace application | `@ubos/render`, `@ubos/sdk` |
| `@ubos/studio` | Studio application | `@ubos/render`, `@ubos/sdk` |
| `@ubos/portal` | external portal | `@ubos/render`, `@ubos/sdk` |
| `@ubos/office` | Live Doc editor, Smart Grid, War Room components | `@ubos/render` |
| `@ubos/verify-web` | static web verifier | `ubos-verify` compiled to WASM |

### AR-031 — Enforced dependency graph
- **Rule:** The crate and package dependency graph in sections 2 and 3 is normative. CI MUST fail on any dependency not listed.
- **Realises:** NR-MAINT-003, AR-001
- **Rationale:** Architecture erosion is prevented mechanically.

### AR-032 — Engine crates are pure with respect to I/O
- **Rule:** Engine-layer crates MUST NOT perform network, file-system or clock access except through host-service traits (CTR-002) and storage traits.
- **Realises:** AR-002, FR-LOGIC-071
- **Rationale:** Determinism, testability, WASM compilation.

## 4. Build and quality gates (CI)

| Gate | When | Blocks merge |
|---|---|---|
| Format, lint (`cargo fmt`, `clippy -D warnings`, `eslint`, `tsc --strict`) | every change | yes |
| Unit and property tests (quick profile) | every change | yes |
| Dependency-graph check | every change | yes |
| Conformance vectors for the changed crates' profiles | every change | yes |
| Full conformance, cross-backend (PostgreSQL and SQLite) | every merge to main | yes (for the release branch) |
| Deterministic simulation (short seeds) | every merge to main | yes |
| Deterministic simulation (long runs), fault injection matrix | nightly | release blocker if red |
| Isolation and sandbox-escape suites | nightly and release | release blocker |
| Benchmarks with regression thresholds (±5% from baseline) | nightly | release blocker if outside threshold without an approved DEC |
| Scenario suites on all bindings | release candidate | yes |
| SBOM, signing, provenance | release | yes |

## 5. Release trains

| Artefact | Cadence | Support |
|---|---|---|
| BPA standard | minor every 6 months; patches as needed | major ≥ 24 months after the successor (FR-STD-041) |
| Kernel and node | minor every 6–8 weeks; patches as needed | the current and previous minor; LTS minor yearly (18 months support) |
| SDKs | aligned with protocol minors | N and N−1 servers (NR-COMPAT-002) |
| Clients (WSP, STU) | continuous for hosted; aligned with node minors for Server | the widget catalogue is negotiated |
| First-party Buks | independent semantic versions | tested against the supported kernel minors (NR-COMPAT-004) |

### AR-033 — Standard before code
- **Rule:** Any change to kernel semantics MUST first land as a BPA change with conformance vectors (CST-013). Implementation merges only when the new vectors pass.
- **Realises:** FR-STD-011, FR-STD-022
- **Rationale:** The standard is the product. Code follows it.

## 6. Reuse of the source repositories

The eight research repositories are inputs, not a codebase to extend (DEC-018). Code from
them MAY be ported into `ubos-unit` crate by crate when all of the following hold:
1. It matches the specification.
2. It passes the relevant conformance vectors.
3. Its design is recorded against the `DSN-*` items it implements.

The analysis in `docs/20-analysis/` names the most mature sources:
- `ubos-system` and `logrum-system` for the store, protocol and packages;
- `ubos_core` for jobs, anchors and loaders.
