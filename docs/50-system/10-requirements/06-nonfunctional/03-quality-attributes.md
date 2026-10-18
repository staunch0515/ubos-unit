---
id: UBS-REQ-06-03
title: Non-Functional Requirements — OBS, OPER, USE, ACC, PORT, COMPAT, MAINT, LOC
status: complete
phase: PH-1
depends_on: [UBS-REQ-06]
---

# Non-Functional Requirements — Observability, Operability, Usability, Accessibility, Portability, Compatibility, Maintainability, Localisation

## OBS — Observability

### NR-OBS-001 — Trace coverage
- **Statement:** Requests and processes MUST be traceable end to end.
- **Target:** At least 99% of UBTP requests, processes, jobs and connector calls in the scenario suite produce spans linked by trace ID and process ID.
- **Rationale:** FR-OPS-041.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the scenario suite with tracing, when analysed, then the coverage is at least 99%.
- **Verification:** CONF
- **Origin:** IMP-12

### NR-OBS-002 — Metrics catalogue
- **Statement:** Nodes MUST expose a documented metrics catalogue.
- **Target:** The catalogue covers the RED metrics (rate, errors, duration) for every API, and saturation metrics for storage, runtime store, queues, projections, sync and gateway. Each metric has name, unit, labels and meaning.
- **Rationale:** Operability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the catalogue, when compared to exported metrics, then they match.
- **Verification:** INSP
- **Origin:** NEW
- **Phase note:** PH-2 step 11 (DEC-087): moved from PH-1; its design items DSN-NOD-612 and DSN-DVM-514 are PH-2. The PH-1 Server exports OTLP metrics without a documented catalogue.

### NR-OBS-003 — SIEM delivery latency
- **Statement:** Security events MUST reach external SIEM promptly.
- **Target:** p95 ≤ 60 s from event to SIEM sink.
- **Rationale:** FR-AUD-013.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given events, when delivered, then 95% arrive within 60 s.
- **Verification:** BENCH
- **Origin:** NEW

### NR-OBS-004 — SLO burn alerts
- **Statement:** SLO violations MUST be alerted with multi-window burn-rate alerts.
- **Target:** Alerts fire within 5 minutes for fast burns (2% of the monthly error budget in 1 hour).
- **Rationale:** Operational response.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an injected error spike, when observed, then the alert fires within 5 minutes.
- **Verification:** FAULT
- **Origin:** NEW

## OPER — Operability

### NR-OPER-001 — Installation effort
- **Statement:** Server installation MUST be straightforward.
- **Target:** A trained operator installs a production-configured ENV-REF-SMALL Server in ≤ 30 minutes following the guide; the Box installs in ≤ 5 minutes.
- **Rationale:** Adoption.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator, PER-PersonalUser
- **Acceptance:**
  1. Given timed installation trials with 3 operators, when measured, then all finish within the targets.
- **Verification:** USE
- **Origin:** NEW

### NR-OPER-002 — Upgrade effort
- **Statement:** Minor upgrades MUST be routine.
- **Target:** A rolling minor upgrade of ENV-REF-SERVER completes in ≤ 60 minutes of operator time, with no downtime.
- **Rationale:** FR-OPS-031.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-405, when timed, then ≤ 60 minutes.
- **Verification:** SCN
- **Origin:** NEW

### NR-OPER-003 — Configuration validation coverage
- **Statement:** Invalid configuration MUST be caught before apply.
- **Target:** 100% of the invalid-configuration test cases (at least 200) are rejected before apply.
- **Rationale:** FR-OPS-091.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the cases, when applied, then all are rejected.
- **Verification:** CONF
- **Origin:** NEW

### NR-OPER-004 — Runbook coverage
- **Statement:** Every alert MUST have a runbook.
- **Target:** 100% of shipped alert rules link to a runbook with diagnosis and remediation steps.
- **Rationale:** Operability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the alert catalogue, when checked, then every alert links to a runbook.
- **Verification:** INSP
- **Origin:** NEW

### NR-OPER-005 — Diagnostic bundle speed
- **Statement:** Diagnostics MUST be quick to collect.
- **Target:** A diagnostic bundle is produced in ≤ 5 minutes on ENV-REF-SERVER.
- **Rationale:** Support efficiency.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a request, when timed, then ≤ 5 minutes.
- **Verification:** BENCH
- **Origin:** NEW

## USE — Usability

### NR-USE-001 — Task success for business users
- **Statement:** Business users MUST complete core tasks without help.
- **Target:** At least 90% unassisted task success for the defined core tasks (create record, submit for approval, approve with diff, find a record, view history as of a date, sign a document), with at least 8 participants per persona, from PH-2.
- **Rationale:** PER-BusinessUser success signal.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser, PER-Approver
- **Acceptance:**
  1. Given the usability study, when analysed, then success ≥ 90% per task.
- **Verification:** USE
- **Origin:** NEW

### NR-USE-002 — Perceived usability
- **Statement:** The Workspace MUST be perceived as usable.
- **Target:** System Usability Scale (SUS) score ≥ 75 for business-layer personas and ≥ 70 for builder-layer personas.
- **Rationale:** Adoption.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP, STU
- **Personas:** PER-BusinessUser, PER-BusinessArchitect
- **Acceptance:**
  1. Given the study questionnaire, when scored, then the targets are met.
- **Verification:** USE
- **Origin:** NEW

### NR-USE-003 — Review efficiency
- **Statement:** Reviews of change sets MUST be efficient.
- **Target:** Median review time ≤ 3 minutes for change sets of up to 50 items with summaries; approval decision median ≤ 2 minutes for single-object approvals.
- **Rationale:** PER-Approver and PER-AgentSupervisor success signals.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-Approver, PER-AgentSupervisor
- **Acceptance:**
  1. Given pilot telemetry, when measured, then the medians meet the targets.
- **Verification:** USE, PILOT
- **Origin:** IMP-10

### NR-USE-004 — Builder productivity
- **Statement:** Builders MUST deliver new business objects quickly.
- **Target:** A new class with lifecycle, 3 rules and views is released in ≤ 1 working day by a trained business architect, and in ≤ 2 hours with the AI builder (PH-3).
- **Rationale:** PER-BusinessArchitect success signal.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** STU
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given timed exercises, when measured, then the targets are met.
- **Verification:** USE
- **Origin:** NEW

### NR-USE-005 — Learnability
- **Statement:** Business users MUST become productive quickly.
- **Target:** After ≤ 2 hours of training, at least 80% of new business users complete core tasks at or below the legacy baseline time.
- **Rationale:** Rollout cost.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a pilot cohort, when measured, then the target is met.
- **Verification:** PILOT, USE
- **Origin:** NEW

### NR-USE-006 — Understandable errors
- **Statement:** Errors shown to users MUST be understandable and actionable.
- **Target:** 100% of user-facing error codes have localised explanation templates; at least 85% of study participants can state what to do next after an error.
- **Rationale:** FR-AUD-031, FR-AUD-032.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the error registry, when checked, then all user-facing codes have templates.
- **Verification:** INSP, USE
- **Origin:** NEW

### NR-USE-007 — Hidden platform vocabulary
- **Statement:** Business-layer screens MUST NOT expose platform vocabulary.
- **Target:** 0 occurrences of the forbidden terms (FR-UX-041) in business-layer strings and screens in automated scans.
- **Rationale:** IMP-11.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the scan, when run, then 0 occurrences are found.
- **Verification:** INSP
- **Origin:** IMP-11

## ACC — Accessibility

### NR-ACC-001 — WCAG 2.2 AA
- **Statement:** Web, desktop and mobile clients MUST conform to WCAG 2.2 Level AA.
- **Target:** 0 AA violations in automated scans of all catalogue widgets and core screens, plus manual audit pass of the core tasks; an Accessibility Conformance Report (VPAT) published.
- **Rationale:** CR-WCAG, Section 508.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser, PER-ExternalParty
- **Acceptance:**
  1. Given the audit, when completed, then no open AA issues remain.
- **Verification:** AUDIT, CONF
- **Origin:** NEW

### NR-ACC-002 — Keyboard operability
- **Statement:** All functionality MUST be operable by keyboard.
- **Target:** 100% of core tasks completed by keyboard only in the audit.
- **Rationale:** WCAG 2.1.1.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given keyboard-only testing, when run, then all tasks complete.
- **Verification:** USE
- **Origin:** NEW

### NR-ACC-003 — Assistive technology support
- **Statement:** Clients MUST work with major screen readers.
- **Target:** Core Web tasks pass with NVDA and JAWS on Windows and VoiceOver on macOS (PH-3); VoiceOver on iOS and TalkBack on Android apply to the mobile apps from PH-4 (DEC-025).
- **Rationale:** Real-world accessibility.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given assistive-technology testing, when run, then the core tasks pass.
- **Verification:** USE, AUDIT
- **Origin:** NEW

### NR-ACC-004 — Accessible documents
- **Statement:** Generated documents MUST be accessible when templates declare structure.
- **Target:** PDF/UA validation passes for 100% of accessible-template outputs in the rendering vector set.
- **Rationale:** FR-OFFICE-084.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given the vectors, when validated, then all pass.
- **Verification:** CONF
- **Origin:** NEW

## PORT — Portability

### NR-PORT-001 — Client platforms
- **Statement:** Clients MUST run on current mainstream platforms.
- **Target:** Web (from PH-2): the latest two major versions of Chrome, Edge, Firefox and Safari. Box and desktop (from PH-4, DEC-024): Windows 10 22H2+, macOS 13+, Ubuntu 22.04+ (x86_64 and arm64). Mobile (from PH-4, DEC-025): iOS 17+ and Android 13+.
- **Rationale:** Reach.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP, NOD
- **Personas:** PER-PersonalUser, PER-FieldWorker
- **Acceptance:**
  1. Given the support matrix, when the smoke suite runs on each, then it passes.
- **Verification:** SCN
- **Origin:** NEW

### NR-PORT-002 — Server platforms
- **Statement:** The Server MUST run on common infrastructure.
- **Target:** Linux x86_64 and arm64; Kubernetes 1.28+ or bare metal; PostgreSQL 16+ (self-managed and the major managed offerings); S3-compatible object stores.
- **Rationale:** Deployment flexibility.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given each supported combination, when the scenario suite runs, then it passes.
- **Verification:** SCN
- **Origin:** DEC-007

### NR-PORT-003 — Data portability between implementations
- **Statement:** Exports MUST move between implementations without loss.
- **Target:** Export → import round trips on the reference DVM on PostgreSQL (PH-1), between PostgreSQL and SQLite (PH-4, DEC-024) and with any certified implementation (PH-4) yields identical head roots in 100% of test datasets.
- **Rationale:** FR-TEN-062.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** BPA, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the datasets, when round-tripped, then the roots match.
- **Verification:** CONF
- **Origin:** EXT-BPU

### NR-PORT-004 — No proprietary lock-in formats
- **Statement:** All data exports, events, APIs and telemetry MUST use documented open formats.
- **Target:** 100% of export and integration formats are documented in the standard or are open standards (JSON, JSON Schema, OpenAPI, CloudEvents, OpenTelemetry, Parquet, Iceberg, PDF/A).
- **Rationale:** Principle P6.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the format inventory, when reviewed, then every format is documented or standard.
- **Verification:** INSP
- **Origin:** IMP-12

## COMPAT — Compatibility and evolution

### NR-COMPAT-001 — ABI support window
- **Statement:** Logic compiled for an ABI major version MUST keep running.
- **Target:** At least 24 months of support after the successor major is released; 100% of the previous major's vectors pass during the window.
- **Rationale:** FR-LOGIC-012, FR-STD-041.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, BPA
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a new major, when released, then the old vectors run and pass.
- **Verification:** CONF
- **Origin:** NEW

### NR-COMPAT-002 — Client–server skew
- **Statement:** Clients and servers MUST tolerate version skew.
- **Target:** A server version N supports clients N and N−1 (minor); SDK version N works with servers N and N+1 (minor).
- **Rationale:** Rolling upgrades and mobile update lag.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, SDK
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the skew matrix, when the scenario suite runs, then it passes.
- **Verification:** SCN
- **Origin:** NEW

### NR-COMPAT-003 — Export format longevity
- **Statement:** Exports MUST remain importable across minor versions.
- **Target:** Exports from every minor version since 1.0 of the same major import successfully with root equality.
- **Rationale:** FR-STD-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, BPA
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the archived exports, when imported, then all succeed.
- **Verification:** CONF
- **Origin:** NEW

### NR-COMPAT-004 — Buk compatibility across kernel minors
- **Statement:** Verified Buks MUST keep working after kernel minor upgrades.
- **Target:** 100% of the scenarios of the first-party Buks (Basic Finance, DEC-022) and of the published Exchange Buks pass on each new kernel minor before release.
- **Rationale:** Ecosystem trust.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, EXC
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a kernel release candidate, when the Buk scenarios run, then all pass.
- **Verification:** SCN
- **Origin:** NEW

## MAINT — Maintainability

### NR-MAINT-001 — Small kernel
- **Statement:** The DVM kernel SHOULD stay small ("less is more").
- **Target:** The core DVM crates (storage, model, versioning, transactions, runtime, rules, authorization) stay ≤ 80,000 lines of Rust excluding tests and generated code; growth beyond this requires a DEC record.
- **Rationale:** EXT-TRI "the kernel as a crystal": fewer defects and faster verification.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the release, when measured, then the size is within the budget.
- **Verification:** INSP
- **Origin:** EXT-TRI

### NR-MAINT-002 — Test depth
- **Statement:** The kernel MUST be deeply tested.
- **Target:** ≥ 85% line coverage and ≥ 70% mutation score for core DVM crates; 100% of Must standard clauses covered by vectors.
- **Rationale:** Correctness is the product.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the CI reports, when reviewed, then the targets are met.
- **Verification:** INSP
- **Origin:** NEW

### NR-MAINT-003 — Enforced module boundaries
- **Statement:** Dependency rules between modules and systems MUST be checked automatically.
- **Target:** 0 violations of the declared dependency graph in CI (for example no host crate depends on business logic; clients depend only on the SDK).
- **Rationale:** Architectural integrity.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD, SDK
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given CI, when a forbidden dependency is introduced, then the build fails.
- **Verification:** INSP
- **Origin:** L40:SPEC-10

### NR-MAINT-004 — Reproducible builds
- **Statement:** Builds MUST be reproducible.
- **Target:** Two independent builds of the same commit produce bit-identical artifacts for the Rust binaries and WASM components.
- **Rationale:** Supply-chain trust (NR-SEC-007).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, FRG
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given two builders, when comparing artifacts, then the hashes match.
- **Verification:** INSP
- **Origin:** NEW

### NR-MAINT-005 — API documentation completeness
- **Statement:** Public interfaces MUST be documented.
- **Target:** 100% of public UBTP messages, instructions, SDK APIs and CLI commands have reference documentation with examples.
- **Rationale:** Developer experience.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** SDK, FRG, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the API inventory, when compared with the docs, then there are no gaps.
- **Verification:** INSP
- **Origin:** NEW

## LOC — Internationalisation

### NR-LOC-001 — Launch languages
- **Statement:** The product MUST be available in key languages.
- **Target:** English (US) in PH-2; Spanish, French, German, Japanese and Simplified Chinese by PH-4, for all business-layer and builder-layer UI strings.
- **Rationale:** International customers after US launch.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** WSP, STU
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given each language, when the string catalogue is checked, then it is 100% translated.
- **Verification:** INSP
- **Origin:** NEW

### NR-LOC-002 — Externalised strings
- **Statement:** No user-facing text MUST be hard-coded.
- **Target:** 100% of user-facing strings are externalised (automated scan).
- **Rationale:** Translatability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP, STU
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the scan, when run, then 0 hard-coded strings are found.
- **Verification:** INSP
- **Origin:** NEW

### NR-LOC-003 — Time zone database currency
- **Statement:** Time-zone data MUST stay current.
- **Target:** IANA tz database updates shipped within 30 days of release.
- **Rationale:** Correct scheduling.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a tz release, when tracked, then the update ships within 30 days.
- **Verification:** AUDIT
- **Origin:** NEW
