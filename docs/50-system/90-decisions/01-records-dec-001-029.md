---
id: UBS-DEC-01
title: Decision Records DEC-001 … DEC-029
status: complete
phase: ALL
depends_on: [UBS-DEC-000]
---

# Decision Records DEC-001 … DEC-029

The table rows and records of the decisions taken before PH-0 execution (DEC-030). The
decisions DEC-030 … DEC-059 are in `02-records-dec-030-059.md`, those from DEC-060 on in
`000-index.md`. Moved here unchanged on 2026-10-03 when the
log passed 800 lines (the checker's size warning); IDs are permanent.

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
