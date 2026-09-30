---
id: UBS-DEC-000
title: Decision Log
status: complete
phase: ALL
depends_on: [UBS-META-00, UBS-META-06]
---

# Decision Log

Decisions are never deleted. A replaced decision is marked
`Superseded by DEC-<NNN>`.

| ID | Title | Status |
|---|---|---|
| DEC-001 | Location and title of the volume | Accepted |
| DEC-002 | The enterprise-internal worker pool is kept; the public compute grid is excluded | Accepted |
| DEC-003 | US-first compliance, GDPR considered | Accepted |
| DEC-004 | First vertical: fund and asset-management back office plus contract management | Superseded by DEC-022 |
| DEC-005 | Six phases, PH-0 … PH-5 | Accepted |
| DEC-006 | Reference durations with an explicit team assumption; phases gated by exit criteria | Accepted |
| DEC-007 | Technology stack of the reference implementation | Accepted |
| DEC-008 | The earlier volume `40-spec` is kept as history | Accepted |
| DEC-009 | Report per batch, in Chinese | Accepted |
| DEC-010 | Writing order, with a confirmation stop after the requirements volume | Accepted |
| DEC-011 | The "six tables only" rule is withdrawn | Accepted |
| DEC-012 | Long-term economic and social narratives are excluded | Accepted |
| DEC-013 | The system decomposition (thirteen systems) | Accepted |
| DEC-014 | PH-1 has two exit gates: PH-1a (engine core) and PH-1b (governance and services) | Accepted |
| DEC-015 | Both first-vertical Buks stay in PH-2 | Superseded by DEC-022 |
| DEC-016 | The first commercial form is Server (on-premises and private cloud); hosted Cell follows in PH-4 | Accepted, revised by DEC-023 |
| DEC-017 | Architecture-level ID kinds AR (architecture rule) and CTR (inter-system contract) | Accepted |
| DEC-018 | Repository layout and reuse of the research repositories | Accepted, revised by DEC-028 |
| DEC-019 | Licensing of the standard, reference implementation and tools | Accepted |
| DEC-020 | Storage engine design: PostgreSQL/SQLite with kernel indexes and prolly trees | Accepted |
| DEC-021 | Policy engine: Cedar-class analysable policies plus relationship tuples | Accepted |
| DEC-022 | First business package: US standard basic finance; fund operations and contract management deferred | Accepted |
| DEC-023 | Purpose of PH-0…PH-3 is platform demonstration, not sale | Accepted |
| DEC-024 | PostgreSQL only until PH-4; SQLite, Box and device sync move to PH-4 | Accepted |
| DEC-025 | PH-2 user interface is Web only, professional and practical; Smart Grid moves to PH-2 | Accepted |
| DEC-026 | Implementation-neutral finance requirements shared with a Java reference system for comparison | Accepted |
| DEC-027 | The specification moves to the repository `ubos-unit` | Accepted |
| DEC-028 | `ubos-unit` is the single implementation monorepo | Accepted |
| DEC-029 | Finance requirements frozen at version 1.0, distributed by copy, work items recorded per protocol | Accepted |
| DEC-030 | PH-0 execution model: owner with AI sessions, external review at acceptance | Accepted |
| DEC-031 | Model-checking bounds: exhaustive where feasible, simulation at the stated bound otherwise | Accepted |
| DEC-032 | BPA 0.1 release: scope, build from the specification, adapter operations, generated vectors, clarifications | Accepted |
| DEC-033 | PH-0 prototype: crate set, PostgreSQL client, property-test harness, pull-request split | Accepted |
| DEC-034 | Rhai sandbox spike: ASM-008 confirmed for L2 with conditions, crate `ubos-runtime`, open questions on L2 sizes and decimals | Accepted |
| DEC-035 | EXIT-0-05 (storage benchmark) carried over into PH-1 for lack of rented hosts | Accepted |
| DEC-036 | PH-1 re-baseline after PH-0: finding dispositions, OQ-007 and OQ-008, risks, dependencies | Accepted |
| DEC-037 | PH-0 acceptance with EXIT-0-05 carried over and external review at the benchmark | Accepted |
| DEC-038 | PH-1 execution: owner with AI sessions, gate-1a steps, verification requested, environments and CI | Accepted |
| DEC-039 | Scope of BPA 1.0-rc1 by rule; releases in development; step 1 in six parts | Accepted |

---

### DEC-001 — Location and title of the volume
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** A new, self-contained specification is needed. It must stay separate from the research volume and the earlier specification.
- **Decision:** The volume lives in `ubos-platform/docs/50-system/` on branch `claude/gifted-edison-cmqbu9` (location revised by DEC-027: `ubos-unit/docs/50-system/`). Its title is *UBOS Business Processing Architecture — System Specification*.
- **Consequences:** `docs/README.md` points to this volume as the current authority. New ID namespaces avoid collisions with `40-spec`.
- **Alternatives considered:** Rewriting `40-spec` in place was rejected because it would lose the history. A new repository was rejected because it would split the documentation.
- **Affects:** UBS-META-00, UBS-META-01

### DEC-002 — The enterprise-internal worker pool is kept; the public compute grid is excluded
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The input documents describe a public compute grid paid in tokens. The product owner excluded "compute as currency". Sandboxed batch and AI work on an organisation's own machines has independent value.
- **Decision:** A **Worker role** of the UBOS Node runs sandboxed, verified WASM jobs on machines the organisation owns or controls. It has no payment, token or public participation. It is delivered in PH-4.
- **Consequences:** The `EVT` domain covers worker registration, placement, result verification and data-classification limits.
- **Alternatives considered:** Dropping distributed execution entirely was rejected because it gives up useful batch capacity. A public grid was rejected by DEC-012.
- **Affects:** FR-EVT, UBS-SYS-NOD, PH-4

### DEC-003 — US-first compliance, GDPR considered
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The positioning targets US enterprises. The first vertical is regulated financial services.
- **Decision:** The compliance catalogue targets the following, in priority order:
  - SOC 2 (Security, Availability, Confidentiality, Processing Integrity);
  - SEC Rule 17a-4 and Rule 204-2 recordkeeping;
  - FINRA Rule 4511;
  - SOX IT general controls;
  - HIPAA (for a later healthcare vertical);
  - CCPA/CPRA;
  - E-SIGN/UETA;
  - WCAG 2.2 AA and Section 508.

  GDPR is designed for from the start, including erasure through crypto-shredding. NIST SP 800-53 is the control vocabulary. China-specific localisation (Xinchuang) is not a target.
- **Consequences:** Crypto-shredding, legal hold, WORM retention and segregation of duties become Must requirements.
- **Alternatives considered:** A global-neutral catalogue was rejected as too vague to verify.
- **Affects:** UBS-REQ-08, NR-SEC, NR-PRIV

### DEC-004 — First vertical: fund and asset-management back office plus contract management
- **Status:** Superseded by DEC-022 · **Date:** 2026-09-29
- **Context:** The architecture needs a flagship "whole product" to prove its value. Regulated finance values immutability, bitemporality and proofs most. One of the source repositories came from the fund domain.
- **Decision:** The first vertical Buk set is:
  1. Fund Operations: fund and share-class master data; subscriptions and redemptions; NAV and fee calculation with bitemporal corrections; investor register; compliance rules.
  2. Contract Management: authoring, negotiation, approval, signature, obligations, amendments.

  Both ship in PH-2 and are hardened in PH-3.
- **Consequences:** Scenarios SCN-1xx (fund operations) and SCN-2xx (contracts) are reference scenarios for verification.
- **Alternatives considered:** Generic CRM was rejected as weakly differentiated. Healthcare was rejected because of heavier regulation for a first pilot.
- **Affects:** UBS-REQ-04, PH-2, PH-3

### DEC-005 — Six phases, PH-0 … PH-5
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** The phases are:
  - PH-0 Foundation;
  - PH-1 Kernel;
  - PH-2 Application;
  - PH-3 Trust and AI;
  - PH-4 Platform and Ecosystem;
  - PH-5 Federation.

  Each phase has a verification plan and binary exit criteria.
- **Consequences:** Every requirement carries exactly one phase.
- **Alternatives considered:** Three coarse phases were rejected because verification gates would be too far apart.
- **Affects:** `50-phases/`, every FR/NR/CR

### DEC-006 — Reference durations with an explicit team assumption; phases gated by exit criteria
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** Each phase states a non-normative reference duration. The assumption is a core team of 8–12 engineers, growing to 15–20 by PH-4. A phase ends only when every exit criterion is evidenced, not on a date.
- **Affects:** `50-phases/`

### DEC-007 — Technology stack of the reference implementation
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:**
  - **Kernel and servers:** Rust stable with Tokio, axum and sqlx.
  - **Storage:** PostgreSQL 16+ (Server, Cell) and SQLite (Box, browser, mobile).
  - **Runtime store:** Redis-compatible, non-authoritative.
  - **Object storage:** S3-compatible.
  - **Logic:** Rhai for L2 scripts; a WASM component-model runtime (wasmtime class) for L3.
  - **Clients:** TypeScript and React; Tauri 2 for desktop and mobile; kernel compiled to WASM for local-first web.
  - **Policy:** a Cedar-class policy engine.
  - **Formal models:** TLA+.

  The BPA standard remains language-neutral.
- **Alternatives considered:** Java/JVM was rejected for the reference implementation. The reasons are the embedded, mobile and WASM targets, sandbox control and determinism. Java implementations remain possible as third-party BPA implementations.
- **Affects:** all systems

### DEC-008 — The earlier volume `40-spec` is kept as history
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** `docs/40-spec/` and the research folders stay unchanged. This volume cites them only as `L40:<ID>` in Origin fields.
- **Affects:** UBS-META-01 §6

### DEC-009 — Report per batch, in Chinese
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** Writing proceeds in batches of about 5–10 files. After each batch the AI validates, commits, pushes and reports to the product owner in Chinese.
- **Affects:** UBS-META-03

### DEC-010 — Writing order, with a confirmation stop after the requirements volume
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** The order is: meta → requirements → (**stop for confirmation**) → architecture → standard → systems → phases → verification → indexes.
- **Affects:** UBS-META-03

### DEC-011 — The "six tables only" rule is withdrawn
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The earlier rule allowed exactly six authoritative tables. It kept the kernel simple, but it forced ledger state, bitemporal indexes, content-addressed objects, job state and sync state into shapes that do not fit them.
- **Decision:**
  1. The kernel MAY define any number of **system tables**. Each is documented with its purpose, owner system, write path and retention.
  2. **Business classes** do not create DDL by default. They remain metadata stored as versioned objects.
  3. A class MAY declare **typed projection tables** for performance or reporting. Projections are non-authoritative and can always be rebuilt from authoritative state.
  4. Authoritative state is written only by the commit path.
- **Consequences:** The storage design in `UBS-SYS-DVM` lists all tables. The conformance suite checks that projections can be rebuilt.
- **Alternatives considered:** Keeping six tables was rejected (product owner instruction). Per-class physical tables for all classes were rejected because they break metadata-driven evolution.
- **Affects:** FR-ANL, UBS-SYS-DVM, UBS-SYS-NOD

### DEC-012 — Long-term economic and social narratives are excluded
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** The following are excluded:
  - "the end of the corporation";
  - the intent economy and reverse consumer marketplace;
  - rebate advertising;
  - compute as currency, tokens and the public compute grid.

  Fiat licensing and billing remain in scope.
- **Affects:** UBS-META-00 §3.2

### DEC-013 — The system decomposition (thirteen systems)
- **Status:** Accepted · **Date:** 2026-09-29
- **Decision:** The system codes are BPA, DVM, FRG, NOD, CTL, WSP, STU, AGT, EXC, FED, NTY, BRG and SDK, as defined in `UBS-META-06` §5.
- **Affects:** `20-architecture/`, `40-systems/`

### DEC-014 — PH-1 has two exit gates: PH-1a (engine core) and PH-1b (governance and services)
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** PH-1 holds 341 requirements (OQ-001). A single exit gate would come too late to catch kernel design errors.
- **Decision:** PH-1 keeps its phase code in requirement blocks, but has two sequential exit gates with their own verification plans and exit criteria (`EXIT-1-*` items are tagged `gate: 1a` or `gate: 1b`). The gate of a PH-1 requirement is determined by its domain, with the listed exceptions:

  | Gate | Domains | Exceptions moved into this gate |
  |---|---|---|
  | **PH-1a Engine core** | MODEL, VER, TIME, LEDG, TXN, LOGIC, PROOF, FILE, ANL, STD, DEV (except CAP-DEV-06) | CAP-IAM-03 (principals and keys), CAP-IAM-09 (secrets and keys), CAP-TEN-01 and CAP-TEN-02 (tenants, VAEs, isolation), CAP-OPS-01 and CAP-OPS-02 (bootstrap, backup), CAP-PKG-07 (genesis), CAP-LOC-03 (time zones) |
  | **PH-1b Governance and services** | RULE, FLOW, EVT, QRY, IAM, TEN, AUD, INT, PKG, OPS, LOC, SYNC, BILL, and CAP-DEV-06 (SDKs) | — |

  PH-1a is verified through the in-process binding and the conformance runner. PH-1b adds UBTP bindings and SDKs.
- **Consequences:** The phase plan (`50-phases/PH-1`) has two gates. Requirements depend only on the same or an earlier gate. A PH-1a requirement that needs a PH-1b capability is a defect to be reported as an open question.
- **Alternatives considered:** Renumbering all phases (PH-1 … PH-6) was rejected because it would churn 960 requirement IDs' phase fields and the scenario map. Keeping one gate was rejected (OQ-001).
- **Affects:** UBS-PH-1, 60-verification, UBS-REQ-04 (PH-1 scenarios are assigned to gates in the phase plan)

### DEC-015 — Both first-vertical Buks stay in PH-2
- **Status:** Superseded by DEC-022 · **Date:** 2026-09-29
- **Context:** OQ-002 asked whether to defer one vertical.
- **Decision:** Fund Operations and Contract Management both ship in PH-2. Contract Management is lighter, and it is the proving ground for Live Doc, templates and electronic signatures. Fund Operations proves bitemporality, ledgers and restatement.
- **Consequences:** The PH-2 verification plan includes both scenario families (SCN-1xx and SCN-2xx).
- **Alternatives considered:** Deferring contracts was rejected because Office and signature capabilities would lack a real-world driver in PH-2.
- **Affects:** UBS-PH-2, DEC-004

### DEC-016 — The first commercial form is Server (on-premises and private cloud); hosted Cell follows in PH-4
- **Status:** Accepted, revised by DEC-023 · **Date:** 2026-09-29
- **Context:** OQ-003. Regulated first customers prefer controlled deployments, and a multi-tenant hosted service needs the Control Plane (PH-4).
- **Decision:** The first sellable product is the Server edition, deployed by the customer or a partner in their data centre or private cloud, with UBOS-provided support. A single-tenant managed Server operated by UBOS for design partners is allowed from PH-3. Multi-tenant Cells start in PH-4.
- **Consequences:** Operability (NR-OPER-*), air-gapped installation (FR-OPS-013) and offline licences (FR-PKG-083) gain importance. SOC 2 scope in PH-4 covers the Cell service.
- **Alternatives considered:** SaaS-first was rejected because it needs the Control Plane and SOC 2 earlier.
- **Affects:** UBS-REQ-01 §6, UBS-PH-2, UBS-PH-3, UBS-PH-4

### DEC-017 — Architecture-level ID kinds AR (architecture rule) and CTR (inter-system contract)
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The architecture volume states cross-system rules and contracts that are not system-internal design requirements.
- **Decision:** Two ID kinds are added:
  - `AR-<NNN>`: a normative architecture rule, defined in `20-architecture`;
  - `CTR-<NNN>`: a contract between two systems (caller, callee, protocol, guarantees), defined in `20-architecture`. Its operations are detailed as `IF-*` items in the system chapters.
- **Affects:** UBS-META-01, tools/check_system.py

### DEC-018 — Repository layout and reuse of the research repositories
- **Status:** Accepted, revised by DEC-028 (the four repositories become top-level folders of `ubos-unit`) · **Date:** 2026-09-29
- **Context:** The eight research repositories contain useful code but diverge from the specification.
- **Decision:** The reference implementation lives in a new monorepo (`ubos`), with separate repositories for the standard (`bpa-standard`), the open verifier (`ubos-verify`) and first-party Buks (`ubos-buks`). Research-repository code may be ported only under the rules of UBS-ARC-07 §6.
- **Consequences:** Clean dependency graph (AR-031). Porting is traceable.
- **Alternatives considered:** Extending `ubos-system` in place was rejected because its layout predates the 13-system decomposition. It remains the first porting source.
- **Affects:** UBS-ARC-07

### DEC-019 — Licensing of the standard, reference implementation and tools
- **Status:** Accepted · **Date:** 2026-09-29 (confirmed by the product owner as proposed)
- **Context:** The ARM-style positioning needs an open standard, a trustworthy open verifier and a commercially licensable engine. The whitepaper (EXT-WP) used AGPLv3.
- **Decision:**
  - BPA text under CC BY 4.0;
  - vectors, schemas, SDKs and verifier under Apache-2.0;
  - kernel, node and Forge under AGPL-3.0 as an open edition, with a commercial licence for embedding and hosted use without AGPL obligations;
  - first-party Buks under a commercial licence.
- **Consequences:** Third parties can implement the standard freely. Embedding vendors buy commercial licences (FR-STD-061). Legal review is required before publication.
- **Alternatives considered:** Fully proprietary was rejected because it contradicts the standard strategy. Apache-2.0 for everything was rejected because it removes the embedding licence revenue.
- **Affects:** UBS-ARC-07, FR-STD-062

### DEC-020 — Storage engine design: PostgreSQL/SQLite with kernel indexes and prolly trees
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The design needs content addressing, fast head and bitemporal reads, Merkle roots per commit and gap-free ledgers, on standard databases.
- **Decision:** Authoritative content (chunks, versions, entries, commits, refs) is stored in PostgreSQL or SQLite tables, with large chunks in the object store. Derived kernel indexes (head, temporal, relationship, unique key) and prolly-tree nodes are maintained in the flush transaction (AR-016). Commits on one branch go through a per-branch sequencer with group commit. Ledger sequences are allocated from row-locked counters inside the flush.
- **Consequences:** Standard operations tooling (backups, replication). ASM-003 is validated in PH-1a benchmarks. Fallback: a log-structured object layer in front of PostgreSQL.
- **Alternatives considered:** A custom storage engine was rejected for operational risk. Pure event sourcing with replay was rejected because read latency would be too high.
- **Affects:** UBS-ARC-04, UBS-SYS-DVM

### DEC-021 — Policy engine: Cedar-class analysable policies plus relationship tuples
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** IMP-06 requires analysable policies and relationship-based permissions.
- **Decision:** Authorization policies use a Cedar-compatible language, evaluated in-process by the DVM. Relations come from the model's authorization relations, materialised in the relationship index and exposed to the policy engine as entity parents and attributes. Policy analysis (who-can) uses the language's analysis tooling.
- **Consequences:** No separate authorization service. Consistency is guaranteed by snapshot evaluation (FR-IAM-053).
- **Alternatives considered:** OPA/Rego was rejected because it is harder to analyse. A Zanzibar-style external service was rejected because of consistency and operational cost.
- **Affects:** UBS-ARC-06, UBS-SYS-DVM

### DEC-022 — First business package: US standard basic finance
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The product owner decided that the first business package must be a standard basic finance system according to US practice (US GAAP basis, single company), because accounting rules are universal, precise and easy to verify, and because a finance ledger exercises the platform's hardest guarantees (immutability, bitemporality, proofs, governed change). Fund operations and contract management add domain risk without adding platform evidence at this stage.
- **Decision:**
  1. The first Buk is **Basic Finance** (`finance`): general ledger and chart of accounts, journals with approval, periods and close, accounts receivable, accounts payable with 1099 reporting, cash and bank reconciliation, fixed assets and depreciation, multi-currency (ASC 830), basic US state sales tax, financial statements (balance sheet, income statement, cash flow statement, statement of stockholders' equity), audit trail and internal controls. Single company.
  2. Excluded from the finance scope: payroll, inventory and costing, multi-entity consolidation, budgeting, project accounting, complex revenue recognition (ASC 606 multiple performance obligations) and lease accounting (ASC 842).
  3. Fund operations (SCN-101…110) and contract management (SCN-201…208) are **Deferred**; their IDs are kept. Capabilities that only contract management needs (Live Doc, clause library, negotiation, electronic signatures and seals of signed documents, document import to templates) move to PH-5.
  4. Fund and contract examples inside generic kernel requirements stay as illustrative examples; they do not create fund- or contract-specific scope.
  5. The detailed finance requirements live in an implementation-neutral document (DEC-026). The platform volume references them through the finance scenarios SCN-501…512.
- **Consequences:** PH-2 delivers the finance Buk; PH-3 replaces the regulated fund pilot with an external CPA audit drill on finance data; the scenario families, workloads (WL-FIN-M), suites (SUITE-PH2-FINANCE) and phase exits change accordingly.
- **Alternatives considered:** Keeping fund operations first was rejected (domain risk, needs a design partner). Doing finance and contracts together was rejected (scope).
- **Affects:** DEC-004, DEC-015, UBS-REQ-01, UBS-REQ-04, UBS-PH-2, UBS-PH-3, UBS-VER-04

### DEC-023 — Purpose of PH-0…PH-3 is platform demonstration, not sale
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The software is not sold for the time being. The finance system exists to show the capabilities the platform imposes: reproducibility at any past point, immutable and provable entries, reviewed and simulated change, explainable rules, and AI work under human review. Market differentiation is not a design driver.
- **Decision:** PH-2 delivers a production-quality demonstration product instead of a sellable product. Commercial exit criteria (design-partner migration, legal review of signatures, commercial packaging) are removed from PH-2 and PH-3. DEC-016 remains the intended commercial form when sales start. PH-4 and PH-5 remain specified but are not scheduled for execution.
- **Consequences:** Goals G2 and G3 are restated (UBS-REQ-01 §7). PH-3 acceptance uses an external CPA audit drill instead of a customer pilot.
- **Alternatives considered:** Keeping commercial criteria was rejected because they would drive work that the owner does not want now.
- **Affects:** DEC-016, UBS-REQ-01, UBS-PH-00, UBS-PH-2, UBS-PH-3

### DEC-024 — PostgreSQL only until PH-4
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** Supporting two storage adapters from PH-1 doubles storage testing. The product owner agreed to start with PostgreSQL.
- **Decision:** PH-0…PH-3 use PostgreSQL only. The SQLite adapter, the Box edition (desktop, mobile, headless SQLite), device sync and PostgreSQL↔SQLite portability move to PH-4. The PH-1a in-process host runs on PostgreSQL. The browser kernel stays in PH-5. The storage trait stays backend-neutral so that SQLite can be added without kernel changes.
- **Consequences:** G1 is measured on PostgreSQL; cross-backend equivalence becomes a PH-4 criterion. DEC-020 is unchanged in design.
- **Alternatives considered:** Dropping SQLite entirely was rejected; the Box and browser editions need it later.
- **Affects:** DEC-020, UBS-REQ-01 §6, UBS-SYS-DVM, UBS-SYS-NOD, UBS-PH-1, UBS-PH-4

### DEC-025 — PH-2 user interface is Web only; Smart Grid moves to PH-2
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The owner wants a professional, practical Web interface in PH-2 and no desktop or mobile apps at this stage. Batch journal entry benefits from a spreadsheet-like grid.
- **Decision:** PH-2 ships the Web Workspace and a lean Web Studio. Desktop and mobile apps move to PH-4 with the Box. Smart Grid (FR-OFFICE-041…046) moves from PH-3 to PH-2. Professional accounting workbench patterns (keyboard-first entry, drill-down from statements to entries, register views, print-quality reports) become design requirements of WSP.
- **Consequences:** WSP design items and the PH-2 usability targets change; the PH-3 device exit criteria move to PH-4.
- **Affects:** UBS-SYS-WSP, UBS-PH-2, UBS-PH-3, UBS-PH-4

### DEC-026 — Implementation-neutral finance requirements shared for comparison
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The owner will implement the same finance requirements on this platform and on a separate Java system, then compare development characteristics and efficiency.
- **Decision:** The finance requirements are written in English as a separate, implementation-neutral document in `docs/finance-requirements/` with their own ID scheme (`FIN-*`). They do not use UBOS concepts. The document includes acceptance criteria, a common sample-company dataset with expected results, shared acceptance scenarios and a comparison protocol. A separate annex in this volume maps the finance requirements to platform capabilities.
- **Consequences:** Both implementations are measured against the same acceptance tests. The platform volume refers to the finance document through the scenarios SCN-501…512 and the suite SUITE-PH2-FINANCE.
- **Affects:** UBS-PH-2, UBS-VER-03, UBS-VER-04

### DEC-027 — The specification moves to the repository `ubos-unit`
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The owner continues the work from another Claude account with the same GitHub account and wants a clean repository that holds only the documentation work, without the Java code of the research phase.
- **Decision:** The whole `docs/` folder (system specification, finance requirements, tools, reports and the research volume kept as history) moves with its git history to `staunch0515/ubos-unit`, together with `CLAUDE.md` (working agreement) and `docs/HANDOFF.md` (status and next steps). `ubos-platform` keeps the same state on its `main` branch as an archive. Further work continues in `ubos-unit`. The implementation repository layout of DEC-018 is not changed by this decision; whether `ubos-unit` also becomes the implementation monorepo is decided separately.
- **Consequences:** DEC-001's location is revised; UBS-ARC-07 names `ubos-unit` as the current home of the specification.
- **Alternatives considered:** Continuing in `ubos-platform` was rejected (mixed with research code); copying files without history was rejected (loses the change record).
- **Affects:** DEC-001, DEC-018, UBS-ARC-07

### DEC-028 — `ubos-unit` is the single implementation monorepo
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** DEC-018 planned four implementation repositories (`ubos`, `bpa-standard`, `ubos-verify`, `ubos-buks`) plus a documentation repository. The owner made `ubos-unit` the home of the work (DEC-027) and wants one repository, provided no planned function is lost.
- **Decision:** The implementation lives in `staunch0515/ubos-unit` next to `docs/`. The four DEC-018 repositories become top-level folders with the same content, licences and dependency rules: `standard/` (formerly `bpa-standard`), `verify/` (formerly `ubos-verify`), `crates/` and `packages/` (formerly the `ubos` monorepo: Rust crates and TypeScript packages), `buks/` (formerly `ubos-buks`). The former repository names remain the **component names** used throughout this volume; a path such as `bpa-standard/vectors/` means `standard/vectors/` and `ubos-verify/models/` means `verify/models/`. Each folder carries its own licence file per DEC-019. Signed releases are tagged per component (`bpa-vX.Y.Z`, `verify-vX.Y.Z`, `kernel-vX.Y.Z`, `buk-<name>-vX.Y.Z`). When a component is published (for example the standard under CC BY 4.0 and Apache-2.0), it is exported from its folder with history into a public mirror; `ubos-unit` stays the source.
- **Consequences:** One pull request can change the standard, its vectors and the kernel together, which suits AR-033 (standard before code) and keeps `bash docs/tools/regen_all.sh` and CI in one place. The dependency-graph check of AR-031 also forbids dependencies from `standard/` on any other folder, and from `verify/` on anything except `standard/` and the shared crates `ubos-types`, `ubos-crypto` and `ubos-proof`. `buks/` depends only on the published Forge and SDK interfaces. The finance requirements folder (`docs/finance-requirements/`) stays self-contained (DEC-029).
- **Alternatives considered:** Separate repositories as in DEC-018 were rejected for now: no public release is planned before PH-3 (DEC-023), and one repository removes cross-repository version pinning during PH-0…PH-2. Splitting is still possible later by exporting a folder with history.
- **Affects:** DEC-018, DEC-019, UBS-ARC-07, AR-031

### DEC-029 — Finance requirements frozen at version 1.0, distributed by copy
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** The owner's Java implementation of the finance requirements is developed in a separate, unrelated repository at the same time as the UBOS implementation (ASM-014). Both must start from the same, unchanging requirements, and the requirements must contain nothing specific to this repository.
- **Decision:** `docs/finance-requirements/` is a self-contained folder: it no longer names either implementation or any path outside itself, and it carries its own generator and checker (`tools/`, run by `tools/regen.sh`; `docs/tools/regen_all.sh` calls it). Version 1.0 is frozen on 2026-09-29 with the git tag `fin-req-v1.0`; the folder's `99-change-log.md` states the freeze rule and the change procedure. Other repositories receive an unchanged copy of a tagged version; changes are made only here and released as new versions. Work items are recorded as fixed in the folder's comparison protocol §2: one effort row per person, work item and day in `work-items.csv`, defects in `defects.csv` (templates in `templates/`), commit messages starting with the work item ID so that size is derived from git history. In the records the UBOS implementation uses system code `U` and the Java implementation `J`.
- **Consequences:** The mapping annex UBS-REQ-10 refers to version 1.0. A change to the finance requirements after the freeze needs a new version and is applied to both implementations at an agreed date.
- **Alternatives considered:** A git submodule or subtree in the Java repository was rejected (it links the Java repository to this one); estimating effort after the fact was rejected (protocol principle 3).
- **Affects:** DEC-026, UBS-REQ-10, ASM-014

### DEC-030 — PH-0 execution model: owner with AI sessions, external review at acceptance
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** ASM-001 assumes a core team of 8–12 engineers from PH-0, and UBS-PH-0 §11 assumes 6–8 engineers for three months. In practice the owner develops with AI assistance. PH-0 exits are verification results (model checking, conformance vectors, property tests, benchmarks, escape cases), while the working agreement forbids tests unless the owner asks for them.
- **Decision:**
  1. PH-0 is executed by the product owner with Claude sessions. The sessions draft the formal models, standard clauses, schemas, vectors, prototype crates, runner and reports; the owner reviews and merges each step as one pull request.
  2. The verification work that PH-0 exit criteria require (VER items of UBS-PH-0 §7.2: models, vectors, properties, benchmarks and the 100 sandbox escape cases) is explicitly requested by the owner. No other testing and no security review is done unless asked.
  3. An external reviewer with database and formal-methods expertise reviews the model and benchmark reports at acceptance (UBS-PH-0 §7.5).
  4. PH-0 proceeds in eight steps, each one pull request: (0) specification alignment; (1) monorepo skeleton with CI; (2) formal models; (3) BPA 0.1 clauses, schemas and vectors; (4) prototype crates, adapter and runner, property tests; (5) benchmarks on ENV-REF-SERVER; (6) sandbox spike; (7) PH-1 re-baseline and acceptance. Steps 2 and 3 precede code (AR-033).
  5. The owner rents the ENV-REF-SERVER database host and one application node for the benchmark step only; the environment definition is kept in `verify/envs/` (UBS-VER-02).
  6. Signed releases (for example BPA 0.1, EXIT-0-01) are tagged and signed by the owner with the owner's key; the sessions prepare the release manifest and checksums.
- **Consequences:** UBS-PH-0 §11 no longer states a calendar duration; progress is reported per step in `docs/reports/ph0-plan-zh.md`. ASM-001 carries a phase note and is re-assessed at EXIT-0-07.
- **Alternatives considered:** Waiting for a hired team was rejected (PH-0 is a de-risking phase that the owner wants to start now); skipping the external review was rejected (the benchmark and model verdicts decide ASM-003 and the storage design).
- **Affects:** ASM-001, UBS-PH-0, UBS-VER-02

### DEC-031 — Model-checking bounds: exhaustive where feasible, simulation at the stated bound otherwise
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** PH-0 step 2 showed that exhaustive TLC runs grow beyond practical limits at the bounds stated in VER-FORM-0001…0003 and UBS-PH-0 §7.4: the merge model already has tens of millions of distinct states at 3 branches × 3 commits, and 3 branches × 4 commits is out of reach on the model-checking workstation.
- **Decision:**
  1. A formal model is checked exhaustively at its stated bound when TLC completes that run.
  2. Otherwise it is checked exhaustively at the largest bound that completes (at least two commits per branch or process, all other dimensions as stated), and additionally by TLC simulation of at least 1,000,000 random behaviours at the stated bound.
  3. The model report states the bound reached, states and depth for the exhaustive run, behaviours and depth for the simulation, the TLC version, and the verdict. Any violation in either run fails the item.
  4. CI runs reduced bounds (`<model>.ci.cfg`) on every change; the full runs are repeated when a model changes and at PH-0 acceptance.
- **Consequences:** EXIT-0-02, UBS-PH-0 §7.4 and VER-FORM-0001…0003 refer to this rule. The external reviewer (DEC-030) judges whether the reached bounds suffice.
- **Alternatives considered:** Keeping the stated bounds as exhaustive requirements was rejected (not achievable without much larger hardware); lowering the bounds without simulation was rejected (loses coverage of deeper histories).
- **Affects:** EXIT-0-02, UBS-PH-0, VER-FORM-0001, VER-FORM-0002, VER-FORM-0003, DSN-DVM-249, DSN-DVM-326, DSN-DVM-515

### DEC-032 — BPA 0.1 release: scope, build, adapter operations, generated vectors, clarifications
- **Status:** Accepted · **Date:** 2026-09-29
- **Context:** PH-0 step 3 (DEC-030) releases BPA 0.1 with Core, Bitemporal and Ledger clauses and at least 300 conformance vectors (EXIT-0-01, MET-QUAL-002, MET-QUAL-003). The vectors need an operation set that implementations can run before UBTP exists, and writing them exposes gaps in the clause text.
- **Decision:**
  1. **Single source.** The normative text stays in `docs/50-system/30-standard/`. `standard/tools/build_release.py` builds the release into `standard/`: the clauses in scope (`standard/release/0.1/scope.json`), JSON Schemas generated from the `DAT-*` blocks, the vectors, the coverage report and a manifest of SHA-256 hashes. Release files are never edited by hand.
  2. **Scope of 0.1.** FND (all); OBJ 001–009; CLS 001, 002, 010–015; KIND (all); VER 001–003, 010, 013, 030, 040, 050–054, 056; TIME 001, 010–012, 020, 030, 031; TXN 001–004, 020–024; PROOF 001–003; ERR 001–002; CONF 001–005. Other clauses follow in later releases.
  3. **Adapter operations.** STD-CONF-005 defines the operations of 0.1 and `DAT-AdapterChange`; `DAT-ConformanceVector` gains `tags`, where `prototype` marks the vectors that the PH-0 prototype must pass (EXIT-0-03).
  4. **Generated vectors.** Vectors are produced by `standard/tools/genvectors.py` from a reference oracle in `standard/tools/bpa/` (Apache-2.0). Expected results are computed, never typed; merge and timeline rules follow the checked TLA+ models. Each vector lies in the VER-CONF range of its clause.
  5. **Clarifications and resolutions.** OQ-004 is resolved by the conservative reading of `set_period` (STD-TIME-011); OQ-005 by the virtual-base rule of the merge model (STD-VER-040). STD-OBJ-002 states that a single-chunk body is identified by its chunk identifier. New error codes `MODEL.INVALID_VALUE`, `MODEL.UNKNOWN_TYPE` and `MODEL.INVALID_NAME` are registered (STD-ERR-002).
  6. **Release.** After merge the owner creates the signed tag `bpa-v0.1.0`; the manifest lists every released file with its hash (DSN-BPA-001).
- **Consequences:** The PH-0 prototype (step 4) implements the adapter operations of its vectors. Further clarifications found while writing vectors are recorded in this decision's addenda before the release.
- **Alternatives considered:** Copying the clauses into `standard/` by hand was rejected (two sources drift); hand-written expected values were rejected (error-prone at 300+ vectors); waiting for UBTP was rejected (PH-0 needs executable vectors).
- **Addendum (2026-09-30, step 3b):** STD-PROOF-001 now states the byte encoding of the level in the boundary hash, the node-closing rule, the level construction and the class and ledger-key hashes; STD-CONF-005 adds error `details` in expectations (`TXN.HEAD_MOVED`, `VER.MERGE_CONFLICT`), the `retroactive` member of commit results (STD-TIME-030) and the `merge` and `diff` results; OQ-006 is resolved by aligning DSN-DVM-206 with STD-VER-013. Four MUST clauses of 0.1 have no vector (STD-FND-026 custom value types, STD-OBJ-009 size limits, STD-TIME-031 impact set, STD-TXN-023 automatic retry) because they need L1 expressions, very large bodies, derivation records or retry control that 0.1 does not expose; they are covered in later releases.
- **Affects:** OQ-004, OQ-005, OQ-006, STD-TIME-011, STD-VER-040, STD-OBJ-002, STD-PROOF-001, STD-ERR-002, STD-CONF-001, STD-CONF-005, DSN-DVM-206, EXIT-0-01, EXIT-0-03, DSN-BPA-001

### DEC-033 — PH-0 prototype: crate set, PostgreSQL client, property-test harness
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** PH-0 step 4 (DEC-030) builds the prototype that must pass every vector tagged `prototype` on PostgreSQL through the runner (EXIT-0-03) and check root history independence and other properties with at least 1,000,000 generated cases (EXIT-0-04). UBS-PH-0 §2 lists the prototype crates `ubos-types`, `ubos-store`, `ubos-tree`, `ubos-version` and `ubos-time` and the FRG runner.
- **Decision:**
  1. Three more crates are added in their UBS-ARC-07 §2 layers: a minimal `ubos-model` (class registry with the field types, kinds and ledger declarations the vectors use; C3 and narrowing follow in PH-1), `ubos-dvm` (the adapter operations of STD-CONF-005) and `ubos-forge` (the runner `forge conform`, producing `DAT-ConformanceReport`).
  2. The prototype accesses PostgreSQL with the synchronous `postgres` crate instead of sqlx (DEC-007); the PH-1 kernel returns to sqlx.
  3. Third-party crates: serde and serde_json, sha2, unicode-normalization, num-bigint, chrono and chrono-tz, postgres, ed25519-dalek.
  4. Property tests use a seeded generator in a release-mode binary (no property-test framework); CI runs 10,000 cases per property, and the 1,000,000-case runs are reported in `verify/reports/`.
  5. Step 4 is delivered in three pull requests: 4a pure operations and runner; 4b stores, model, versioning, time and PostgreSQL (EXIT-0-03); 4c property tests (EXIT-0-04).
- **Consequences:** The dependency checker (AR-031) covers the added crates; UBS-PH-0 §2 refers to this decision.
- **Alternatives considered:** Using sqlx already in PH-0 was rejected (asynchronous runtime without benefit for a single-threaded adapter); proptest was rejected (a fixed-seed generator reports and replays each case deterministically, as DSN-DVM-516 asks).
- **Affects:** UBS-PH-0, DEC-007, UBS-ARC-07

### DEC-034 — Rhai sandbox spike: ASM-008 confirmed for L2 with conditions
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** PH-0 step 6 (DEC-030) runs the 100 initial L2 escape cases of VER-SEC-0001 against a Rhai host hardened per STD-CTX-020 and DSN-DVM-612 (EXIT-0-06, MET-SEC-001). ASM-008 assumes that Rhai and a WASM component runtime meet the determinism and sandboxing requirements with the planned hardening; RSK-004 is the risk that Rhai cannot be made deterministic without floats.
- **Decision:**
  1. The prototype L2 host is built in crate `ubos-runtime` (its UBS-ARC-07 §2 layer, no internal dependencies yet) with the binary `ubos-sandbox-spike`; the suite is `verify/sandbox/l2-escape-cases.json` (L2E-001…100) and the report `verify/reports/exit-0-06/`. Third-party crates: `rhai` (features `no_float`, `decimal`, `no_time`, `no_module`, `no_closure`, `no_custom_syntax`), `rust_decimal`, and `cap` (counting allocator, spike binary only).
  2. All 100 cases are blocked and replay identically. ASM-008 is **confirmed for the L2 language (Rhai)**, without a fork, under these conditions, which DSN-DVM-612 now states: the engine starts empty with an explicit list of pure packages (Rhai's language-core package carries `sleep`, `exit`, `parse_json` and function reflection); memory is accounted per execution outside the engine and checked at every metering point; host errors are not catchable by scripts; elapsed time is checked often enough to keep the 10 ms bound of STD-CTX-005 when a single unit is a native call on a large value.
  3. The WASM half of ASM-008 stays an assumption until the L3 host is verified in PH-1.
  4. Open questions OQ-007 (L2 sizes per profile missing from STD-CTX-004) and OQ-008 (L2 decimal range versus the kernel decimal) are decided at the PH-1 re-baseline (step 7), together with closing RSK-004, which did not occur.
- **Consequences:** EXIT-0-06 is met. The per-worker memory count, a weighted unit cost for native calls on large values, and the quadratic cost of Rhai's size checks (findings F-2…F-4 of the report) are inputs to the PH-1 plan and to the NR-PERF-010 benchmark.
- **Alternatives considered:** Forking Rhai was not needed (RSK-004 did not occur). Enforcing memory only through engine size limits was rejected: they bound single values, and 1,000 variables reached about 450 MB under the 64 MB `strict` profile. Disabling `try … catch` was rejected: host errors and terminations are already uncatchable, and catching an unknown-function error reaches no capability.
- **Affects:** ASM-008, RSK-004, DSN-DVM-612, STD-CTX-004, STD-CTX-020, UBS-PH-0, UBS-ARC-07

### DEC-035 — EXIT-0-05 carried over into PH-1
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** EXIT-0-05 needs VER-BENCH-0001 on ENV-REF-SERVER, which the owner rents (DEC-030). The owner cannot rent the hosts at present. The preparation is complete: storage layout, tree maintenance, WL-GENERIC generator and loader, benchmark driver, group commit, environment definition and runbook (`verify/reports/exit-0-05/`, `docs/reports/ph0-step5-runbook-zh.md`). The local trial found that commit capacity is bound by sequencer CPU (finding C-1), so ASM-003 and RSK-001 remain open.
- **Decision:**
  1. EXIT-0-05 and VER-BENCH-0001 are carried over into PH-1 under the entry rule of UBS-PH-00 §3.1. Owner of the carried criterion: the product owner (hosts) with an AI session (run and report). Due: before the storage layout of the PH-1 kernel is fixed, and at the latest at gate 1a.
  2. The criterion, the metrics MET-PERF-001…003, the procedure and the tools are unchanged; the result is recorded in a DEC that confirms ASM-003 or records the remedy or the fallback of DEC-020.
  3. Until then, PH-1 work that depends on the storage layout (DSN-DVM-001…011) proceeds on the prototype layout and may change with the result.
- **Consequences:** EXIT-0-05 and VER-BENCH-0001 carry phase notes; UBS-PH-1 lists the benchmark as a dependency at start and ENV-REF-SERVER as needed at start instead of month 2 (DEC-036).
- **Alternatives considered:** Keeping PH-0 open until the hosts are rented was rejected: the remaining PH-0 work does not depend on it. Accepting local trial numbers as evidence was rejected: the workstation lacks SHA extensions and the database host differs from ENV-REF-SERVER.
- **Affects:** EXIT-0-05, VER-BENCH-0001, ASM-003, RSK-001, UBS-PH-0, UBS-PH-1

### DEC-036 — PH-1 re-baseline after PH-0
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** EXIT-0-07 requires UBS-PH-1 to be re-baselined with the PH-0 findings (VER-INSP-9003). DEC-034 left OQ-007 and OQ-008 open for this step and asked to close RSK-004.
- **Decision:**
  1. Every PH-0 finding has a disposition in UBS-PHR-0.
  2. OQ-007: the L2 sizes of the spike become part of STD-CTX-004 (strings 1 MiB, arrays and maps 65,536 elements for `strict`, scaled ×8 for `standard` and `agent`, ×64 for `batch`, ×128 for `simulation`; 1,024 variables, 256 functions, 64 KiB source, expression depth 64 and 32). The clause is not in the scope of BPA 0.1, so the change applies from BPA 1.0-rc1.
  3. OQ-008: DSN-DVM-612 stands: the L2 decimal type is the kernel decimal of `ubos-expr`, registered as a Rhai custom type with its operators; Rhai's built-in 96-bit decimal is not used by the kernel. The spike's use of it was a prototype shortcut.
  4. RSK-004 is closed: it did not occur (DEC-034). RSK-001 stays open with likelihood H and is continued in PH-1 as RSK-016; the dependence of commit capacity on SHA-256 throughput is RSK-017.
  5. UBS-PH-1: the carried VER-BENCH-0001 is a dependency at start; ENV-REF-SERVER is needed at start; the work items of UBS-PHR-0 §3 are added to the plan; the non-normative reference duration is unchanged and stated as PH-0 stated it (owner with AI sessions, progress per step).
  6. DEC-034 item 3 is corrected: L3 components are out of scope for PH-1 (UBS-PH-1 §5), so the WASM part of ASM-008 is verified with the L3 host in PH-3.
- **Consequences:** STD-CTX-004 gains the size limits with a note on BPA 1.0-rc1; OQ-007 and OQ-008 are resolved; UBS-PH-0 marks RSK-001 raised and RSK-004 closed.
- **Alternatives considered:** Keeping Rhai's decimal (bounding L1 decimals to 28 digits) was rejected: STD-FND-022, released in BPA 0.1, requires exact decimal arithmetic without a digit bound and division with an explicit scale and rounding mode, which Rhai's decimal division does not ask for. Leaving the sizes to implementations was rejected: STD-CTX-020 requires them to follow the profile, which needs numbers.
- **Affects:** UBS-PH-1, UBS-PHR-0, STD-CTX-004, DSN-DVM-612, RSK-001, RSK-004, EXIT-0-07

### DEC-037 — PH-0 acceptance
- **Status:** Accepted · **Date:** 2026-09-30 (proposed), confirmed by the owner 2026-09-30
- **Context:** PH-0 exits EXIT-0-01…04, EXIT-0-06 and EXIT-0-07 are met; EXIT-0-05 is carried over (DEC-035). DEC-030 asks for an external reviewer with database and formal-methods expertise at acceptance; the owner has not yet named one.
- **Decision:** PH-0 is accepted with EXIT-0-05 carried over; PH-1 may start. The external review takes place together with the carried benchmark, on the acceptance package `docs/reports/ph0-acceptance-zh.md`; its findings are dispositioned in UBS-PHR-0.
- **Consequences:** PH-1 starts (DEC-038); the external review is recorded in an addendum.
- **Alternatives considered:** A separate external review now was not chosen by the owner, to combine both external engagements.
- **Affects:** UBS-PH-0, UBS-PH-1, DEC-030

### DEC-038 — PH-1 execution
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** PH-1 has 338 requirements (237 at gate 1a, 101 at gate 1b; DEC-014) and a reference team of 10–12 engineers (UBS-PH-1 §11). As in PH-0 (DEC-030), the product owner executes it with AI sessions. PH-0 was accepted with EXIT-0-05 carried over (DEC-035, DEC-037).
- **Decision:**
  1. PH-1 is executed by the product owner with AI sessions, one pull request per step (a step may be split), merged by the owner; progress is reported per step. No calendar duration is fixed.
  2. Only gate 1a is planned in steps now (UBS-PH-1 §11): (0) alignment; (1) BPA 1.0-rc1 clauses and vectors, standard before code (AR-033); (2) extended formal models; (3) storage kernel; (4) model, versioning, time, ledger and transactions; (5) runtime; (6) proofs, export and import; (7) in-process host services, Forge and gate-1a scenarios; (8) verification close-out; (9) gate-1a acceptance. Gate 1b is planned in steps at the gate-1a acceptance.
  3. Confirming this plan requests the verification work that the PH-1 exit criteria require (CLAUDE.md rule 4 exception, as DEC-030 for PH-0).
  4. The prototype crates of PH-0 evolve into the kernel crates; the kernel accesses PostgreSQL through sqlx (DEC-007, DEC-033). The storage layout stays open to change until the carried benchmark (W-1, DEC-035) is decided.
  5. Environments: the owner rents ENV-REF-SERVER at least twice — as early as possible for W-1, and for the gate-1a benchmarks (EXIT-1-09) — and ENV-REF-SMALL for gate 1b. Performance requirements are Must and cannot be carried over (UBS-PH-00 §3.7); if the hosts are still unavailable at gate 1a, a decision re-phases them with its reason.
  6. CI: GitHub-hosted runners, including nightly jobs (simulation seeds for 30 consecutive nights, long property runs, benchmark regression) and Linux ARM64 and macOS ARM64 runners for the replay corpus (EXIT-1-08). If the Actions minutes do not suffice, the AI session reports it and the owner decides on more minutes or self-hosted runners.
  7. Fault injection: process kills run in CI; power-loss simulation with dm-flakey is tried on GitHub-hosted Ubuntu runners and otherwise runs on the rented host.
  8. The Phase Acceptance Board of PH-1 is the owner and the external reviewer; the external review of PH-0 takes place with W-1 (DEC-037).
- **Consequences:** UBS-PH-1 §11 lists the steps; `docs/reports/ph1-plan-zh.md` tracks their status.
- **Alternatives considered:** Planning both gates in steps now was rejected: gate 1b depends on the kernel interfaces fixed in gate 1a (DEC-014). Rewriting the kernel from scratch was rejected: the prototype crates already pass the BPA 0.1 vectors and properties.
- **Affects:** UBS-PH-1, DEC-014, DEC-030, DEC-035, DEC-037

### DEC-039 — Scope of BPA 1.0-rc1
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** EXIT-1-01 requires BPA 1.0-rc1 with vectors for 100% of the MUST clauses of Core, Bitemporal, Ledger and Proof (roots, signatures, inclusion). The standard holds 240 clauses, 79 of them in BPA 0.1. Several Core clauses serve later phases (for example anchors, sync, L3 components), and UBS-PH-1 §7.2 placed the expression and addressing vectors at gate 1b although both areas are Core and serve gate-1a requirements. The release builder did not read the instruction-set folder of the standard.
- **Decision:**
  1. The scope of BPA 1.0-rc1 is the scope of BPA 0.1 plus every clause of the profiles Core, Bitemporal, Ledger and Proof whose earliest satisfied requirement is in PH-0 or in PH-1 gate 1a (gate from UBS-PHF-1); a clause without a **Satisfies** line is definitional and included when its profile is. `standard/release/1.0-rc1/scope.json` states this rule, and the release builder rejects a scope that differs from it. The result is 171 clauses, 145 of them MUST clauses with a vector range.
  2. Expressions (EXPR) and addressing (ADDR) are in 1.0-rc1 with the gate-1a clauses. Governance (LGS) and Protocol (PROTO) clauses belong to 1.0-rc2 (gate 1b) even where they serve a gate-1a requirement. Clauses of BPA 0.1 whose requirements were later moved to PH-2 (DEC-022) stay in the release. UBS-PH-1 §7.2 is corrected accordingly.
  3. BPA 0.1 is frozen as tagged (`bpa-v0.1.0`); its coverage report and manifest are no longer rebuilt. `docs/tools/regen_all.sh` builds the release in development, now 1.0-rc1. Vectors are added, never changed; the vectors of 0.1 keep their identifiers and content.
  4. The clause loader reads subfolders of the standard (the instruction set).
  5. Adapter operations are added to STD-CONF-005 with the release that adds them; 1.0-rc1 adds `set_limits`.
  6. Step 1 of PH-1 is delivered in six parts: 1a scope and tooling; 1b data model (OBJ, CLS, KIND); 1c versioning, time and transactions; 1d proofs; 1e execution (CTX, ISA, BPU, EXPR, ADDR); 1f release. STD-FND-026 (custom value types need L1 expressions) and STD-TXN-023 (retries need logic) are covered in 1e, STD-TIME-031 (impact sets need derivation records) in 1c.
- **Consequences:** The coverage report of 1.0-rc1 shows the remaining clauses; it reaches 100% at part 1f, when the owner signs `bpa-v1.0.0-rc1`. New vectors are not tagged `prototype` until the kernel implements them (steps 4–7).
- **Alternatives considered:** Taking every Core, Bitemporal, Ledger and Proof clause was rejected: it would pull PH-2 to PH-5 features into gate 1a. Listing the scope by hand was rejected: the rule keeps it consistent with the requirement phases and is checked by the builder.
- **Affects:** UBS-PH-1, STD-CONF-005, STD-OBJ-009, EXIT-1-01, DEC-032

