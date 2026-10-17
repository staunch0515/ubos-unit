---
id: UBS-SYS-FRG-02
title: Logic Forge — Design Requirements
status: draft
phase: PH-1
depends_on: [UBS-SYS-FRG-01]
---

# Logic Forge — Design Requirements (DSN-FRG-*)

| Range | Sub-area |
|---|---|
| DSN-FRG-0xx | CLI, project model and build |
| DSN-FRG-1xx | checks, types and verification records |
| DSN-FRG-2xx | tests, simulator and debugging |
| DSN-FRG-3xx | Buk packaging, dependencies, signing and verification |
| DSN-FRG-4xx | conformance runner |
| DSN-FRG-5xx | developer experience: dev environment, language server, docs, WASM toolchain |

## CLI, project model and build

### DSN-FRG-001 — The `forge` CLI
- **Statement:** The `forge` CLI MUST provide the commands `init`, `check`, `types`, `test`, `dev`, `simulate`, `replay`, `pack`, `sign`, `verify`, `install`, `publish`, `conform`, `docs`, `lsp` (FR-DEV-011). Every command MUST support `--json` output with a stable schema and meaningful exit codes (FR-DEV-012).
- **Rationale:** FR-DEV-011, FR-DEV-012.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given every command with `--json`, when output is validated against its schema, then it conforms.
- **Verification:** CONF
- **Origin:** FR-DEV-011, FR-DEV-012
- **Phase note:** PH-1 step 7b (DEC-060): `init`, `check`, `types`, `test` and `conform` exist, each with `--json` and exit status 0, 1 or 2; `dev` follows at gate 1b, the others with their phases.

### DSN-FRG-002 — Project model
- **Statement:** A Forge project MUST be a directory with `forge.toml` and source folders `model/`, `sheets/`, `logic/`, `components/`, `views/`, `seed/`, `tests/`, `docs/`. Sources MUST be text formats (YAML or the textual grammars) so that they diff and merge in Git.
- **Rationale:** Git-friendly development next to branch-based governance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given `forge init`, when run, then the skeleton passes `forge check`.
- **Verification:** CONF
- **Origin:** FR-DEV-011, FR-PKG-011
- **Phase note:** PH-1 step 7b (DEC-060): `forge.toml` and YAML (or JSON) sources under `model/`, `logic/`, `seed/`, `tests/`; `sheets/`, `components/`, `views/` and `docs/` are created by `forge init` and compiled from gate 1b. The skeleton passes `forge check` and `forge test`.

### DSN-FRG-003 — Incremental build graph
- **Statement:** The build MUST compute a content-addressed dependency graph over sources and cache compiled outputs by input hash, so that a one-file change rebuilds only its dependants (FR-DEV-013).
- **Rationale:** NR-PERF-016 developer loop.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a 500-class project, when one script changes, then `forge check` completes within NR-PERF-016.
- **Verification:** BENCH
- **Origin:** FR-DEV-013, NR-PERF-016
- **Phase note:** Not needed at gate 1a (DEC-060): one pass checks a project in milliseconds; the content-addressed cache follows with `forge dev` at gate 1b, where the developer loop of NR-PERF-016 is measured.

### DSN-FRG-004 — Same engine everywhere
- **Statement:** The CLI, the NOD-linked service (CTR-012) and the EXC verification service MUST use the same `ubos-forge` library version pinned to the kernel ABI, and produce identical findings for identical inputs.
- **Rationale:** CTR-012 guarantees.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the same project, when checked by CLI and by the Forge service, then findings are identical.
- **Verification:** CONF
- **Origin:** CTR-012, NR-DET-002
- **Phase note:** PH-1 step 7b (DEC-060): `forge check` uses the class registry of `ubos-model` and the analyser of the verification gate of `ubos-dvm`, so its findings are the engine's; the NOD-linked service follows with gate 1b.

## Checks, types and verification records

### DSN-FRG-101 — Check pipeline
- **Statement:** `forge check` MUST run, in order: parse → model checks (linearization, narrowing, kinds; DSN-DVM-103, DSN-DVM-104) → sheet compilation and cascade ambiguity → decision table completeness → script type checks → static analysis (DSN-DVM-642) → lens totality → view resolution checks → seed data validation. All findings MUST be reported with file, line, rule and severity.
- **Rationale:** Earliest possible feedback.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a project with one error per stage, when checked, then every error is reported in one run.
- **Verification:** CONF
- **Origin:** FR-LOGIC-042, FR-LOGIC-091, FR-RULE-042, FR-MODEL-086
- **Phase note:** PH-1 step 7b (DEC-060): parse → model → sheets and tables (warning: gate 1b) → scripts and static analysis → port implementations → seed data, every stage in one run; a project with one error per stage reports all of them (`verify/reports/forge-7b/`). Lens totality and view resolution follow with their gates.

### DSN-FRG-102 — Type generation
- **Statement:** `forge types` MUST generate TypeScript and Rust types (and Python from PH-3) for classes, value types, ports, events and queries from a snapshot (FR-DEV-031), versioned per branch and model hash (FR-DEV-032).
- **Rationale:** FR-DEV-031, FR-DEV-032.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG, SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given generated TypeScript types, when an application uses a removed field, then the TypeScript compiler fails.
- **Verification:** CONF
- **Origin:** FR-DEV-031, FR-DEV-032
- **Phase note:** PH-1 step 7b (DEC-060): `types.ts` and `types.rs` for classes, value types and ports, with the branch and model hash in the header; Python follows in PH-3. PH-1 step 17d (DEC-078): the generated code is the typed API of DSN-SDK-012 over the SDKs' typed layers (typed reads, unions of a class and its subclasses, query builders, typed port calls).

### DSN-FRG-103 — Verification records
- **Statement:** For each logic asset, Forge MUST produce a signed verification record (STD-CTX-023) binding the asset hash, ABI, passed checks, test results, coverage and inferred capabilities. Records MUST be signed with the verifier key (developer, CI or EXC service key).
- **Rationale:** FR-LOGIC-083, DSN-DVM-641.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a verified asset, when the DVM activates it on `main`, then the record is accepted; with one byte changed, it is rejected.
- **Verification:** CONF, SEC
- **Origin:** FR-LOGIC-083, STD-CTX-023
- **Phase note:** PH-1 step 7b (DEC-060): `forge test` signs one record per asset with the verifier key (asset hash, ABI, static analysis, type check, capability and test checks, coverage in permille, inferred and declared capabilities); the DVM accepts Forge-signed records at activation with the install path of gate 1b.

### DSN-FRG-104 — Coverage gates
- **Statement:** Verification MUST enforce configurable coverage thresholds for scripts (branch coverage) and rules (rule and table-row coverage, FR-RULE-083), defaulting to 80% and 100% of decision-table rows for protected-branch activation.
- **Rationale:** FR-RULE-083, FR-LOGIC-083.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given coverage at 70% with an 80% gate, when verifying, then no record is produced, and uncovered lines are listed.
- **Verification:** CONF
- **Origin:** FR-RULE-083, FR-LOGIC-083

### DSN-FRG-105 — Lens verification
- **Statement:** Forge MUST verify lenses by totality analysis and by property tests that apply the lens to generated instances of the source schema and validate results against the target schema (FR-MODEL-086).
- **Rationale:** FR-MODEL-086.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a lens that produces an invalid value for 1 in 1,000 inputs, when verified, then a counter-example is reported.
- **Verification:** PROP
- **Origin:** FR-MODEL-086

## Tests, simulator and debugging

### DSN-FRG-201 — Test kinds
- **Statement:** The test framework MUST support unit tests of scripts and ports, rule tests for sheets and decision tables, scenario tests over multi-step processes, property tests, and conformance tests (FR-DEV-051). Tests MUST be written in YAML (declarative) or in the script language.
- **Rationale:** FR-DEV-051.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given one test of each kind, when `forge test` runs, then all execute against an in-process DVM.
- **Verification:** CONF
- **Origin:** FR-DEV-051, FR-RULE-081
- **Phase note:** PH-1 step 7b (DEC-060): `scenario`, `unit`, `port` and `property` tests in YAML against the in-memory VAE of the project; rule tests follow with the rules of gate 1b, conformance tests are `forge conform`.

### DSN-FRG-202 — Controlled time and randomness
- **Statement:** Tests MUST run with a controlled clock (settable, advanceable) and seeded entropy through the host-service traits (FR-DEV-052), and with mocked connectors and models.
- **Rationale:** FR-DEV-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a test that advances time by 30 days, when a schedule is due, then it fires deterministically.
- **Verification:** CONF
- **Origin:** FR-DEV-052
- **Phase note:** PH-1 step 7b (DEC-060): the deterministic services of `ubos-dvm::host` (fixed clock set by the test and advanced by `advance` steps, seeded entropy, no connectors or models); mocked connectors follow with the connector runtime of gate 1b.

### DSN-FRG-203 — Reproducible test VAEs
- **Statement:** Test fixtures MUST be built from seed data into in-memory VAEs whose genesis and seed roots are reproducible (FR-DEV-022).
- **Rationale:** FR-DEV-022.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the same seed, when built twice, then roots are equal.
- **Verification:** CONF
- **Origin:** FR-DEV-022
- **Phase note:** PH-1 step 7b (DEC-060): the fixture is built from the model and seed into an in-memory VAE; the report carries the genesis and seed roots, which repeat run after run.

### DSN-FRG-204 — Test reports
- **Statement:** Test runs MUST produce JUnit XML and a JSON report with timings, coverage and failure diffs (FR-DEV-053).
- **Rationale:** FR-DEV-053.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a CI run, when the report is uploaded, then standard CI systems display it.
- **Verification:** CONF
- **Origin:** FR-DEV-053
- **Phase note:** PH-1 step 7b (DEC-060): JSON (timings, roots, coverage per asset, failure diffs) and JUnit XML.

### DSN-FRG-205 — Simulator
- **Statement:** `forge simulate` MUST pull a history range from a VAE (with the user's permissions), replay its processes with changed logic or rules (FR-DEV-041), with external effects disabled (FR-DEV-042), and report changed outcomes (DSN-DVM-761).
- **Rationale:** FR-DEV-041, FR-DEV-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a changed fee script and one month of history, when simulated, then every changed fee is listed with old and new values.
- **Verification:** CONF, SCN
- **Origin:** FR-DEV-041, FR-DEV-042, NR-PERF-017

### DSN-FRG-206 — Execution traces and replay debugging
- **Statement:** Forge MUST display instruction traces (DSN-DVM-634) and support step-through replay with breakpoints on instructions and script lines (FR-DEV-091, FR-DEV-092).
- **Rationale:** FR-DEV-091, FR-DEV-092.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a failed production process journal, when replayed in the debugger, then the developer can step to the failing line.
- **Verification:** SCN
- **Origin:** FR-DEV-091, FR-DEV-092

## Buk packaging, dependencies, signing and verification

### DSN-FRG-301 — Buk archive
- **Statement:** `forge pack` MUST produce a canonical, content-addressed Buk archive (FR-PKG-012): a manifest (name, namespace, version, ABI range, dependencies, capabilities requested, licence terms, seed data), content organised as chunks addressed by hash, and a root hash over the manifest and content tree.
- **Rationale:** FR-PKG-011, FR-PKG-012.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given the same sources, when packed twice on different machines, then root hashes are equal.
- **Verification:** CONF
- **Origin:** FR-PKG-011, FR-PKG-012, NR-MAINT-004

### DSN-FRG-302 — Seed data with temporal attributes
- **Statement:** Seed data MUST support valid-time periods and ledger opening entries (FR-PKG-013), validated against the model at pack time.
- **Rationale:** FR-PKG-013.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given seed data with overlapping periods, when packed, then packing fails.
- **Verification:** CONF
- **Origin:** FR-PKG-013

### DSN-FRG-303 — Dependency resolution and lock
- **Statement:** Dependencies MUST use semantic version ranges (FR-PKG-021), resolved deterministically by a PubGrub-style solver into `forge.lock` with exact versions and root hashes (FR-PKG-022). Conflicts MUST be explained with the derivation (FR-PKG-023).
- **Rationale:** FR-PKG-021, FR-PKG-022, FR-PKG-023.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given two dependencies requiring incompatible versions of a third, when resolved, then the explanation names both paths.
- **Verification:** CONF, PROP
- **Origin:** FR-PKG-021, FR-PKG-022, FR-PKG-023

### DSN-FRG-304 — Signing
- **Statement:** `forge sign` MUST sign the archive root with the publisher key (Ed25519, `did:ubos`) and optionally with Sigstore keyless signing for open-source Buks; signatures MUST be detached and listed in the archive trailer.
- **Rationale:** NR-SEC-007.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a signed Buk, when verified offline, then the publisher identity is shown.
- **Verification:** SEC
- **Origin:** NR-SEC-007, FR-PKG-061, CR-NIST-003

### DSN-FRG-305 — Buk verification
- **Statement:** `forge verify` MUST check signatures, manifest schema, namespace ownership, ABI compatibility, full checks, tests, coverage gates, capability requests versus inferred use, and migration presence for breaking changes, and produce a signed Buk verification record (FR-PKG-061), reproducible for the same inputs (FR-PKG-063).
- **Rationale:** FR-PKG-061, FR-PKG-062, FR-PKG-063.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given a Buk requesting fewer capabilities than it uses, when verified, then verification fails, listing the missing capability.
- **Verification:** CONF, SEC
- **Origin:** FR-PKG-061, FR-PKG-062, FR-PKG-063

### DSN-FRG-306 — Install plan
- **Statement:** `forge install` MUST compute an install plan (classes, sheets, assets, seed data, required configuration, permissions requested) (FR-PKG-033) and install through a loader branch and change set (FR-PKG-031).
- **Rationale:** FR-PKG-031, FR-PKG-033.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, NOD
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an install, when previewed, then the plan lists all objects and permissions before any commit.
- **Verification:** CONF
- **Origin:** FR-PKG-031, FR-PKG-033, FR-PKG-032

### DSN-FRG-307 — Migrations shipped with Buks
- **Statement:** Breaking changes between Buk versions MUST ship migration definitions (lenses or migration processes) (FR-PKG-042); Forge MUST test them against the previous version's seed and test fixtures.
- **Rationale:** FR-PKG-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given v1 fixtures, when migrated to v2, then v2 tests pass on migrated data.
- **Verification:** CONF
- **Origin:** FR-PKG-042, FR-PKG-044

### DSN-FRG-308 — Publish
- **Statement:** `forge publish` MUST upload the archive to EXC (CTR-015), request verification, and publish the version on success; deprecations MUST be publishable with notices (FR-PKG-052).
- **Rationale:** CTR-015, FR-PKG-052.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** FRG, EXC
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a failing verification on EXC, when publishing, then the findings are shown locally.
- **Verification:** SCN
- **Origin:** CTR-015, FR-PKG-052

## Conformance runner

### DSN-FRG-401 — Vector runner
- **Statement:** `forge conform` MUST load a conformance package (CTR-014), select vectors by claimed profiles, run them through an adapter (the in-process DVM or an external implementation through the adapter protocol) and produce a report (DAT-ConformanceReport) (FR-STD-021).
- **Rationale:** FR-STD-021.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** FRG, BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the reference DVM, when run on all profiles, then 100% pass.
- **Verification:** CONF
- **Origin:** FR-STD-021, CTR-014

### DSN-FRG-402 — Adapter protocol
- **Statement:** External implementations MUST be driven through the adapter of STD-CONF-002: UBTP messages (or the same message structures in-process), plus the test-only operations `reset`, `set_now`, `set_entropy_seed`, `load_genesis` and `export_state`. For convenience, Forge MUST also offer the same messages as JSON lines over stdin/stdout. The protocol MUST be versioned within the standard's major version.
- **Rationale:** Language-neutral conformance.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** FRG, BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given a sample adapter in Python wrapping the reference DVM's C ABI, when run, then results equal the in-process run.
- **Verification:** CONF
- **Origin:** FR-STD-021, STD-CONF-002, NR-DET-002

### DSN-FRG-403 — Signed reports
- **Statement:** Conformance reports MUST be signed by the runner key and include runner version, package version, profiles, per-vector outcomes and environment (FR-STD-023).
- **Rationale:** FR-STD-023, FR-STD-051.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given a modified report, when verified, then the signature fails.
- **Verification:** SEC
- **Origin:** FR-STD-023, FR-STD-051

### DSN-FRG-404 — Vector authoring tools
- **Statement:** Forge MUST provide tools to record vectors from reference runs, minimise them, and check coverage of Must clauses (FR-STD-022) and alignment with formal models (FR-STD-032).
- **Rationale:** FR-STD-022, FR-STD-032.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** FRG, BPA
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given the standard's Must clauses, when coverage is computed, then uncovered clauses are listed.
- **Verification:** INSP
- **Origin:** FR-STD-022, FR-STD-032

## Developer experience

### DSN-FRG-501 — Local dev environment
- **Statement:** `forge dev` MUST start a local development node — the in-process host on a local PostgreSQL until PH-4, the headless Box afterwards (FR-DEV-021, DEC-024) — with the project installed, watch sources, re-install changes on a dev branch within NR-PERF-016, and expose UBTP for local clients.
- **Rationale:** FR-DEV-021, NR-PERF-016.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a saved script, when changed, then the running dev VAE uses it within NR-PERF-016.
- **Verification:** BENCH
- **Origin:** FR-DEV-021, NR-PERF-016

### DSN-FRG-502 — Language server
- **Statement:** `forge lsp` MUST implement the Language Server Protocol for model YAML, sheets, scripts and tests: diagnostics from the check pipeline, completion from the model, hover with effective definitions and cascade explanations, go-to-definition across files (FR-DEV-071).
- **Rationale:** FR-DEV-071.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a typo in a field name in a script, when edited, then a diagnostic appears within 500 ms.
- **Verification:** BENCH, USE
- **Origin:** FR-DEV-071

### DSN-FRG-503 — Editor extension
- **Statement:** A VS Code extension MUST bundle the language server, test explorer integration, debugger adapter (DAP) for replay debugging, and commands for check, test, simulate and install (FR-DEV-072).
- **Rationale:** FR-DEV-072.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the extension, when a test fails, then the developer can start a replay debug session from the test explorer.
- **Verification:** USE
- **Origin:** FR-DEV-072

### DSN-FRG-504 — Documentation and change logs
- **Statement:** `forge docs` MUST generate a Buk reference (classes, fields, ports, events, sheets, decision tables, examples) (FR-DEV-081) and a change log between versions from semantic diffs (FR-DEV-082).
- **Rationale:** FR-DEV-081, FR-DEV-082.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given two versions, when the change log is generated, then every breaking change is listed first.
- **Verification:** CONF
- **Origin:** FR-DEV-081, FR-DEV-082

### DSN-FRG-505 — WASM component toolchain
- **Statement:** Forge MUST build L3 components from Rust (`cargo component`), TypeScript (componentize-js) and Go (TinyGo) against the UBOS WIT world, check the deterministic subset (FR-LOGIC-055) by inspecting imports, and produce verification records.
- **Rationale:** FR-LOGIC-051, FR-LOGIC-052, FR-LOGIC-055.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a component importing a WASI clock, when built, then verification fails naming the import.
- **Verification:** SEC, CONF
- **Origin:** FR-LOGIC-051, FR-LOGIC-052, FR-LOGIC-055

### DSN-FRG-506 — Machine interface for AI iteration
- **Statement:** Forge findings MUST be available as structured data with stable rule IDs, locations and suggested fixes, so that AGT can iterate on drafts (FR-AI-072).
- **Rationale:** FR-AI-072.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** FRG, AGT
- **Personas:** PER-AiAgent
- **Acceptance:**
  1. Given a generated draft with errors, when checked through the Forge service, then each finding has a rule ID and location.
- **Verification:** CONF
- **Origin:** FR-AI-072, FR-DEV-012
