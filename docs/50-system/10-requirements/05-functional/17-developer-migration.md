---
id: UBS-REQ-05-17
title: Functional Requirements — DEV (Developer Experience) and MIG (Migration and Import)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-05]
---

# Functional Requirements — DEV and MIG

## CAP-DEV-01 — CLI

### FR-DEV-011 — The `forge` command line
- **Statement:** Logic Forge MUST provide a CLI with commands for:
  - project scaffolding (`new`) and checks (`check`);
  - type generation (`types`), tests (`test`) and simulation (`sim`, from PH-2);
  - packing (`pack`) and verification (`verify`);
  - installation into a VAE (`install`), publishing (`publish`, from PH-4);
  - conformance runs (`conform`).
- **Rationale:** A scriptable loop for developers and CI (L40:SPEC-28).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper, PER-IsvDeveloper
- **Acceptance:**
  1. Given a new project, when `forge new`, `check`, `test` and `pack` run, then a valid archive results.
- **Verification:** SCN
- **Origin:** L40:SPEC-28
- **Phase note:** PH-1 step 19d (DEC-087): PH-1 has `forge init`, `check`, `types`, `test`, `conform` and `dev`; `forge new` and `forge pack` follow with the Buk format in PH-2 step 7.

### FR-DEV-012 — Machine-readable output
- **Statement:** Every CLI command MUST support JSON output and stable exit codes.
- **Rationale:** CI and AI agents drive the CLI.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper, PER-AiAgent
- **Acceptance:**
  1. Given `forge check --json`, when findings exist, then the output validates against the published schema, and the exit code is non-zero.
- **Verification:** CONF
- **Origin:** NEW

### FR-DEV-013 — Fast loop
- **Statement:** The edit–check–test loop for a typical Buk (100 classes, 200 rules) MUST complete within the NR-PERF developer-loop bound (target: 10 seconds).
- **Rationale:** PER-LogicDeveloper success signal.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the reference Buk, when benchmarked, then the loop meets the target.
- **Verification:** BENCH
- **Origin:** NEW

## CAP-DEV-02 — Local environment

### FR-DEV-021 — Headless Box for development
- **Statement:** Developers MUST be able to run a headless Box locally with one command, seeded with genesis and optional sample data.
- **Rationale:** A local, offline development environment.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given `forge dev up`, when run, then a local VAE is available within 30 seconds.
- **Verification:** SCN
- **Origin:** NEW
- **Phase note:** moved to PH-4 by DEC-024 and DEC-025 (SQLite, Box, devices and sync).

### FR-DEV-022 — Reproducible test VAEs
- **Statement:** Test VAEs MUST be creatable from fixtures deterministically, and disposable.
- **Rationale:** Reliable tests.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the same fixture, when two VAEs are created, then their head roots match.
- **Verification:** CONF
- **Origin:** NEW

## CAP-DEV-03 — Type generation

### FR-DEV-031 — Generated types
- **Statement:** Forge MUST generate type definitions from the effective model for Rhai (type-check metadata), TypeScript and Rust (SDK and WASM components).
- **Rationale:** Typed logic and clients.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a class change, when types are regenerated, then the code using a removed field fails to compile.
- **Verification:** CONF
- **Origin:** IMP-05

### FR-DEV-032 — Types per branch and version
- **Statement:** Type generation MUST target a branch and point in time, so that developers can build against a release.
- **Rationale:** Version-accurate development.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given the tag `1.2.0`, when types are generated, then they reflect that release.
- **Verification:** CONF
- **Origin:** NEW

## CAP-DEV-04 — Simulator

### FR-DEV-041 — Replay history with changed logic
- **Statement:** The simulator MUST replay recorded processes, or run scenarios, over a time window on a sandbox branch with changed logic, rules or classes, and MUST report the output differences.
- **Rationale:** Change safety for developers and analysts (SCN-014, SCN-106).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, DVM
- **Personas:** PER-LogicDeveloper, PER-BusinessAnalyst
- **Acceptance:**
  1. Given a changed fee logic, when a month of accrual processes is replayed, then the per-accrual differences are reported.
- **Verification:** SCN, CONF
- **Origin:** NEW

### FR-DEV-042 — No external effects in simulation
- **Statement:** Simulations MUST use recorded connector and model responses and MUST NOT perform external effects.
- **Rationale:** Safety.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a simulation of payment processes, when run, then no connector calls occur.
- **Verification:** CONF
- **Origin:** NEW

## CAP-DEV-05 — Test framework

### FR-DEV-051 — Test kinds
- **Statement:** Forge MUST run the following test kinds deterministically:
  - unit tests for logic assets;
  - rule tests (CAP-RULE-08);
  - scenario tests (Given/When/Then over UBTP);
  - property tests for declared invariants.
- **Rationale:** Quality of Buks.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a scenario test, when run twice, then the results are identical.
- **Verification:** CONF
- **Origin:** L40:SPEC-28

### FR-DEV-052 — Controlled time and randomness
- **Statement:** Tests MUST be able to fix `ctx.now`, the random seeds and the connector responses.
- **Rationale:** Deterministic tests.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a test fixing now to 2026-10-01, when logic reads now, then it gets that value.
- **Verification:** CONF
- **Origin:** NEW

### FR-DEV-053 — Test reports
- **Statement:** Test runs MUST produce JUnit-compatible and JSON reports with coverage of logic and rules.
- **Rationale:** CI integration.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a CI run, when completed, then the reports are produced.
- **Verification:** CONF
- **Origin:** NEW

## CAP-DEV-06 — SDKs

### FR-DEV-061 — TypeScript and Rust SDKs
- **Statement:** Official SDKs MUST exist for TypeScript and Rust, implementing UBTP with typed models, authentication, subscriptions, idempotency and retries.
- **Rationale:** All clients use one SDK (L40 hard rule 11).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the scenario suite, when run through each SDK, then it passes.
- **Verification:** SCN, CONF
- **Origin:** L40:SPEC-26

### FR-DEV-062 — Additional language SDKs
- **Statement:** SDKs for Python and Java SHOULD be provided, generated from the protocol schemas.
- **Rationale:** Integration with data-science and enterprise stacks.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** SDK
- **Personas:** PER-DataAnalyst, PER-LogicDeveloper
- **Acceptance:**
  1. Given the Python SDK, when the query scenarios run, then they pass.
- **Verification:** CONF
- **Origin:** NEW

### FR-DEV-063 — Semantic versioning of SDKs
- **Statement:** SDKs MUST follow semantic versioning aligned with protocol versions and MUST document compatibility.
- **Rationale:** Predictable upgrades.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a protocol minor release, when the SDK is updated, then the old code compiles unchanged.
- **Verification:** INSP
- **Origin:** NEW

## CAP-DEV-07 — IDE integration

### FR-DEV-071 — Language server
- **Statement:** Forge MUST provide a Language Server Protocol server for LGS, L1 expressions, Rhai logic and view definitions, with diagnostics, completion from the model, hover documentation and go-to-definition.
- **Rationale:** Productivity.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper, PER-BusinessAnalyst
- **Acceptance:**
  1. Given a misspelled field, when typed, then the IDE shows the diagnostic.
- **Verification:** CONF
- **Origin:** NEW

### FR-DEV-072 — Editor extension
- **Statement:** A VS Code-compatible extension SHOULD package the language server, tests and VAE explorer.
- **Rationale:** Adoption.
- **Priority:** Could · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the extension, when installed, then the tests run from the editor.
- **Verification:** USE
- **Origin:** NEW

## CAP-DEV-08 — Documentation generation

### FR-DEV-081 — Generated Buk reference
- **Statement:** Forge MUST generate reference documentation for a Buk: classes, fields, ports, lifecycles, rules, events and examples, from the model and knowledge.
- **Rationale:** Documentation stays in sync with the model.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a Buk, when documentation is generated, then every class is documented with its descriptions.
- **Verification:** INSP
- **Origin:** NEW

### FR-DEV-082 — Change logs
- **Statement:** Forge MUST generate change logs between Buk versions from the diffs and the change-set descriptions.
- **Rationale:** Upgrade communication.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given two versions, when a change log is generated, then all class changes are listed.
- **Verification:** INSP
- **Origin:** NEW

## CAP-DEV-09 — Debugging and tracing

### FR-DEV-091 — Execution traces
- **Statement:** Developers with permission MUST be able to capture step traces of logic executions (instructions, values after masking, timing) and of rule cascades on non-production branches, and on production only with an audit-logged justification.
- **Rationale:** Diagnose behaviour.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a trace request on a draft, when the action runs, then the trace lists each instruction.
- **Verification:** CONF
- **Origin:** NEW

### FR-DEV-092 — Replay-based debugging
- **Statement:** Developers MUST be able to replay a production process (FR-LOGIC-073) in a local environment with the recorded inputs, subject to masking.
- **Rationale:** Reproduce production issues exactly.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a failed production process, when replayed locally, then the same failure occurs.
- **Verification:** SCN
- **Origin:** NEW

## CAP-MIG-01 — Spreadsheet import

### FR-MIG-011 — Model inference from spreadsheets
- **Statement:** The importer MUST infer classes, field types, keys and relationships (lookup columns) from XLSX and CSV files, including merged headers and multiple sheets, and MUST propose them for confirmation (SCN-015).
- **Rationale:** Fast onboarding from spreadsheets.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, STU
- **Personas:** PER-ImplementationConsultant, PER-PersonalUser
- **Acceptance:**
  1. Given the SCN-015 workbook, when analysed, then the proposal matches the expected classes.
- **Verification:** SCN, USE
- **Origin:** NEW

### FR-MIG-012 — Row-level import results
- **Statement:** Imports MUST report per row whether it was imported, updated, unchanged or rejected, with reasons.
- **Rationale:** Trustworthy migration.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given SCN-015, when imported, then 2,391 imported and 9 rejected rows are reported with reasons.
- **Verification:** SCN
- **Origin:** NEW

### FR-MIG-013 — Idempotent re-import
- **Statement:** Re-importing the same source MUST match rows by key and MUST create no versions for unchanged rows.
- **Rationale:** SCN-015 A1.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given an unchanged file, when re-imported, then zero new versions are created.
- **Verification:** CONF
- **Origin:** NEW

## CAP-MIG-02 — Document import

### FR-MIG-021 — DOCX to Live Doc templates
- **Statement:** The importer MUST convert DOCX documents into Live Doc templates, preserving structure and styles within the widget catalogue, and MUST propose bindings for recognised entities (party names, dates, amounts).
- **Rationale:** Migrating contract templates.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** BRG, WSP
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given a contract DOCX, when imported, then the headings and clauses are preserved, and binding suggestions are listed.
- **Verification:** SCN, USE
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-MIG-022 — Clause extraction
- **Statement:** The importer SHOULD split imported documents into clause-library candidates.
- **Rationale:** Build clause libraries from existing contracts.
- **Priority:** Could · **Phase:** PH-5 · **Systems:** BRG, AGT
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given 50 contracts, when analysed, then similar clauses are grouped as candidates.
- **Verification:** SCN
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## CAP-MIG-03 — Legacy bulk load

### FR-MIG-031 — Load with history and valid time
- **Statement:** Bulk loads MUST be able to import historical versions with their original valid times and source timestamps, recorded as provenance. The transaction time is the load time.
- **Rationale:** Migrating legacy history for bitemporal queries.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given 5 years of price history, when loaded, then `asof` queries for past dates return the migrated values.
- **Verification:** SCN
- **Origin:** NEW

### FR-MIG-032 — Bulk process semantics
- **Statement:** Large loads MUST use bulk processes (FR-TXN-015) with an all-or-nothing staging option.
- **Rationale:** Consistent migrations.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an interrupted load, when resumed, then it completes without duplicates.
- **Verification:** FAULT
- **Origin:** NEW

### FR-MIG-033 — Opening balances for ledgers
- **Statement:** Ledger migrations MUST support opening-balance entries with references to legacy sources, as well as full-history import.
- **Rationale:** Practical accounting cut-over.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given opening balances, when loaded, then the balance projections equal the legacy trial balance.
- **Verification:** SCN
- **Origin:** NEW

## CAP-MIG-04 — Mapping definitions

### FR-MIG-041 — Versioned mappings
- **Statement:** Import mappings (source fields to class fields, transformations, lookups, defaults) MUST be versioned objects reusable across imports.
- **Rationale:** Repeatable migrations (SCN-015).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a saved mapping, when the next file is imported, then it applies without manual steps.
- **Verification:** CONF
- **Origin:** NEW

### FR-MIG-042 — Mapping tests
- **Statement:** Mappings MUST have sample-based tests that run before an import executes.
- **Rationale:** Early error detection.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a failing mapping test, when the import starts, then it is blocked.
- **Verification:** CONF
- **Origin:** NEW

## CAP-MIG-05 — Migration reconciliation

### FR-MIG-051 — Control totals
- **Statement:** Every import MUST produce control totals (counts, sums of amount fields, hash totals) for source and target, and MUST flag mismatches.
- **Rationale:** Audit-grade migration.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant, PER-InternalAuditor
- **Acceptance:**
  1. Given an import, when finished, then source and target totals are shown side by side.
- **Verification:** SCN
- **Origin:** NEW

### FR-MIG-052 — Record-level reconciliation
- **Statement:** The platform MUST support record-level comparison between the legacy extract and the migrated objects, with mismatch reports.
- **Rationale:** Cut-over sign-off.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an intentionally altered record, when reconciled, then it is reported.
- **Verification:** SCN
- **Origin:** NEW

### FR-MIG-053 — Signed migration report
- **Statement:** A migration run MUST produce a signed report with its mapping version, source hashes, totals and results, retained as evidence.
- **Rationale:** Regulators ask how legacy data was moved.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a migration, when completed, then the report verifies with the verifier.
- **Verification:** CONF, AUDIT
- **Origin:** NEW
