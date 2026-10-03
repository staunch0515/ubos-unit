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
| DEC-040 | BPA 1.0-rc1 object and class model: clarifications of ports, selectors, extensions, relationships, reference integrity and blind indexes | Accepted |
| DEC-041 | BPA 1.0-rc1 versioning, time and transactions: clarifications of branch kinds, change sets, previews, resolutions, tags, revert, inheritance, snapshots, evidence, impact sets, rule binding, derivations, lineage, idempotency, outbox and bulk processes | Accepted |
| DEC-042 | BPA 1.0-rc1 proofs: signatures and key histories, inclusion and consistency proofs, tenant roots, portable archive format, algorithm registry | Accepted |
| DEC-043 | BPA 1.0-rc1 L1 expressions, queries, addressing and custom value types; part 1e of step 1 split into 1e-1 and 1e-2 | Accepted |
| DEC-044 | BPA 1.0-rc1 execution: host programs, context, gateway, profiles, metering, entropy, journal and replay, logic assets and verification, instructions, BPU views and automatic retry | Accepted |
| DEC-045 | Release of BPA 1.0-rc1: notes in the manifest, signed tag `bpa-v1.0.0-rc1`, frozen once tagged | Accepted |
| DEC-046 | Group commit and ledger sequence model (VER-FORM-0004): two-phase flush, bounds, mutation checks, design clarifications | Accepted |
| DEC-047 | Storage kernel (PH-1 step 3): parts 3a–3c, asynchronous storage traits on sqlx, migrations, transition layout, local blob and key providers, storage suite environment | Accepted |
| DEC-048 | Tenancy and physical form of the storage kernel (step 3b): partitions, row-level security with an application role, stored-byte format, chunk placement, adapter reset | Accepted |
| DEC-049 | Verifiers and storage suite (step 3c): index, sequence and root verifiers, SUITE-DVM-STORE with 20 cases, inspection record, EXIT-1-03 | Accepted |
| DEC-050 | Engine at production quality (PH-1 step 4): parts 4a–4d; 4a full class registry, relationships and extensions in the kernel; custom value types wait for the L1 evaluator | Accepted |
| DEC-051 | Versioning in the kernel (step 4b): commit records with identity and transaction time, branch metadata and lifetimes, parent-line views, change sets, previews, resolutions, tags, releases, revert, processes | Accepted |
| DEC-052 | Time, ledger and transactions in the kernel (step 4c): evaluation evidence and consistent coordinates, range queries and history, derivation records and impact sets, idempotent requests, outbox and became-effective events, bulk processes, AEAD registry | Accepted |
| DEC-053 | Production identities of the kernel schema (step 4d-1): commits keyed by content ID within the VAE, UUID object and entry identifiers, timestamptz valid times with per-row granularity, class names with declaration IDs, head rows with ordinal intervals replacing `object_state`, write sets in commit records, verifier rebuilt from them | Accepted |
| DEC-054 | Metadata tables, tenant scope and addressing (step 4d-2): branch, tag, process, change-set, derivation, outbox, idempotency and timer records in their tables, effects, idempotency records, derivation records and timers written in the flush, the VAE authority and principal as engine scope, `parse_uri`, `resolve`, `parse_did`, `key_did`, interim signed runner report of EXIT-1-02 | Accepted |
| DEC-055 | Runtime (PH-1 step 5): parts 5a–5d; 5a the L1 expression language in `ubos-expr` (lexer, parser, type checker, tree-walking evaluator, ABI 1.0 function library), custom value types evaluated through a registry hook, `check_expr`, `eval_expr`, `validate_value` with value types, criteria queries in `ubos-dvm` until `ubos-query` | Accepted |
| DEC-056 | Instruction set and executor (step 5b): the catalogue, error registry and profiles of ABI 1.0 embedded from the clause text, the gateway, execution context, metering and entropy in `ubos-runtime`, the host-program interpreter, the store half of the instructions as the engine's `Host`, `execute` with automatic retry, journal and metering records | Accepted |
| DEC-057 | Verification gate, replay and BPU views (step 5c): static analysis, type check, capabilities, tests and publisher signature of logic assets, node-signed verification records with a minimal Ed25519 node key in `ubos-crypto`, journals and metering records as chunks referenced from the process record, replay with divergence reporting, `bpu_descriptor`, `bpu_state`, `verify_bpu`, `node_did` | Accepted |
| DEC-058 | L2 host at production quality (step 5d): Rhai engine per profile on a worker thread with the instruction gateway bound as `sys.<family>.<op>`, read-only `ctx`, typed handles, kernel decimal and money as script numbers, metering by operations, thread CPU time, wall time and per-execution memory through a counting allocator, compiled scripts cached by asset version; the conformance adapter keeps L2 off; escape suite of 382 cases all blocked (EXIT-1-10), W-2…W-5 and W-12 closed | Accepted |
| DEC-059 | Proofs (step 6, parts 6a–6c): every commit record carries the node signature over its commit ID, written at flush by the version layer through the node record of the store; signature verification against DID documents with key histories in `ubos-crypto`; new crate `ubos-proof` with the storage-free inclusion-proof, commit-record, consistency and archive checks shared with the verifier; node key rotation; adapter operations `sign`, `public_key`, `verify_signature`, `did_document`, `rotate_node_key`, `commit_record`, `inclusion_proof`, `verify_inclusion`, `verify_consistency` (6a) and `prove`, `consistency_proof`, `tenant_root`, `export_archive`, `import_archive` (6b); verifier `ubos-verify` under `verify/` and the EXIT-1-12 round trip (6c); 1,227 of 1,227 vectors pass on both backends, EXIT-1-12 met | Accepted |
| DEC-060 | In-process host (step 7a): the CTR-002 host services as traits of `ubos-dvm::host` with a `Vae` host API (CTR-001) over the engine; new crate `ubos-node` with the PostgreSQL implementations — tenant registry, key hierarchy (root KEK → tenant KEK → derived data and node signing keys), node signer behind signed commit records, blob table, secrets by handle, connectors with an allow-list journaled before use, event log, jobs with leases and poison, redacted telemetry; migration 0006; suite SUITE-NOD-HOST 12/12; conformance unchanged at 1,227 | Accepted |

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

### DEC-040 — BPA 1.0-rc1 object and class model (step 1b)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 1b of PH-1 step 1 (DEC-039) writes the oracle and vectors for STD-OBJ-012 and STD-CLS-020, -021, -022, -030, -040 and -041. Several clauses left details open that an implementation needs and that vectors must fix.
- **Decision:** The clauses are clarified as follows (text in the clauses):
  1. STD-OBJ-012: the blind index is `hmac-sha256:<hex>` of HMAC-SHA-256 under the 32-byte tenant lookup key over the canonical JSON of the validated value.
  2. STD-CLS-020: an implementation is a port declaration with `impl`; a port without any implementation or without any declaration is not implemented; extension ports are named `<namespace>.<port>` and belong to the extension target.
  3. STD-CLS-021: parameters are matched by name, "more general" is the reverse of STD-CLS-013, and the port kind must be equal.
  4. STD-CLS-022: depth counts parent steps from `Object` (0), traits have depth 1; `only` matches the named class itself; class filters return live objects.
  5. STD-CLS-030 and STD-CLS-010: `ClassExtension` is a genesis class below `ConfigAsset`; extensions apply to their target and its subclasses, after the fold, ordered by target generality, `order` and namespace; refinements use full member keys; visibility follows the extension object's current version, from the commit after the one that writes it; invalid extensions fail the commit.
  6. STD-CLS-040: relationship members and values (`one_to_one`, `many_to_one`, `one_to_many`, `many_to_many`; `association`, `composition`; `restrict`, `cascade`, `detach`), their rules over live objects within the commit, and link objects with `source` and `target` fields for `attributes_class`.
  7. STD-CLS-041: references resolve by class and current slug in the state after the commit (objects of the same commit included); the target's segments must cover the period without a gap; `target_class` is checked against the linearization.
  8. STD-CONF-005 (1.0-rc1): operations `dispatch`, `type_selector`, `blind_index`, `list_objects`, `traverse`, `class_at`, and `effective_class` with `extensions` and `ports`; reference values in adapter inputs are `ubos://acme/<Class>/<slug>`. Changes made by relationship rules are reported in `versions` and, when retroactive, in `retroactive`.
- **Consequences:** 84 vectors (VER-CONF-0405…0409, 0720…0730, 0740…0749, 0755…0766, 0780…0794, 0810…0827, 0840…0852), not tagged `prototype`; MUST coverage of 1.0-rc1 76/145. The kernel implements them in step 4.
- **Alternatives considered:** Plain object identifiers as reference values in adapter inputs were rejected: STD-FND-020 defines references as canonical URIs. Applying extensions per class during the fold was rejected in favour of the clause text of STD-CLS-012 (extensions after the fold).
- **Affects:** STD-OBJ-012, STD-CLS-010, STD-CLS-020, STD-CLS-021, STD-CLS-022, STD-CLS-030, STD-CLS-040, STD-CLS-041, STD-CONF-005

### DEC-041 — BPA 1.0-rc1 versioning, time and transactions (step 1c)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 1c of PH-1 step 1 (DEC-039) writes the oracle and vectors for the clauses of versioning, bitemporal semantics and processes that BPA 0.1 did not cover: STD-VER-011, -012, -020, -021, -031, -055, -057, -058, -060, -061, -070, -071, -080, -081, -090; STD-TIME-013, -014, -015, -031, -040, -050; STD-TXN-030, -031, -040, -050, -070. Several clauses left details open that vectors must fix.
- **Decision:** The clauses are clarified as follows (text in the clauses):
  1. STD-VER-011: the merge column restricts merges into `main`, `release` and `overlay` (`VER.MERGE_NOT_ALLOWED`); other targets accept every kind but `sandbox`; "via change set" is the write policy of STD-VER-012, except that `agent` branches reach protected targets only by change set; lifetimes count from creation (`draft`: from the last commit) and archived branches refuse commits (`VER.BRANCH_ARCHIVED`); the "Created by" and "Direct commits" columns are checked with the Governance profile. The direct `merge` of BPA 0.1 stays valid, so the 0.1 vectors are unchanged.
  2. STD-VER-012: the effective write policy applies to direct commits and merges, including changes made by relationship rules. Because `ClassExtension` is a ConfigAsset class, four vectors of DEC-040 (VER-CONF-0791…0794) now commit extensions on working branches.
  3. STD-VER-020, -021: genesis class `ChangeSet` (below `Object`, write policy `direct`) on the target branch with member `state`; transitions outside the table fail with `FLOW.TRANSITION_NOT_ALLOWED`; one approval is final without the Governance profile; approvals are checked against the current preview at the latest when the change set is next read or acted on.
  4. STD-VER-031: cost is not observable; vectors check the tree difference, MET-PERF-003 measures the cost.
  5. STD-VER-055, -057, -058: the merged state is validated relative to the target (bodies, references into and out of changed objects, cardinality, double entry, period rule); the preview content ID omits `base`, `source_head` and `target_head`; resolutions come with the merge, and `value_hash` is the content ID of the chosen value.
  6. STD-VER-060, -061: tags per VAE with slug names; genesis class `Release`; a second release of a version on a branch fails with `MODEL.KEY_CONFLICT`.
  7. STD-VER-070, -071: revert is the three-way merge of the state before the commit into the head with the state after it as base; created objects are ended; conflicts fail with `VER.MERGE_CONFLICT`; dependents are descendants by STD-TXN-031.
  8. STD-VER-080, -081, -090: an inheriting branch starts empty; the time of a snapshot is the transaction time of the selected commit, and a head read resolves a live parent at its head; only parent-line objects can be hidden; ledgers are not inherited; provenance is the holding branch and its snapshot commit; one selector at most (`QRY.INVALID_QUERY`), commits and tags off the first-parent history select nothing.
  9. STD-TIME-013, -014, -015: stored references carry no version pin; range bounds follow the class granularity; transaction ranges list first-parent commits with their operations; `evaluated_at` maps every consulted branch, parent lines included, to its commit.
  10. STD-TIME-031, -040, -050: supersession per `asof`; closure over all derivation records of the branch; the rule binding lists the linearization class versions, applied extensions and derivation logic of the commit; became-effective events go through the outbox with a fixed effect ID.
  11. STD-TXN-030: sorted and deduplicated inputs; `provenance_ref` is the record's content ID and enters the version envelope and the entry hash (`provenance_ref` added to `DAT-LedgerEntry`); `version_range` in commit sequence numbers.
  12. STD-TXN-031, -040, -050, -070: `event` and `job` triggers need an existing parent; the payload hash covers branch and changes, and only committed requests are recorded; effect IDs `<process_id>/<n>` delivered until acknowledged; bulk children are `job` processes, and resumption divides the work again.
  13. STD-ERR-002: new codes `VER.MERGE_NOT_ALLOWED` and `VER.BRANCH_ARCHIVED`.
  14. STD-CONF-005 (1.0-rc1): conventions 8 (the adapter clock is `now`; transaction times follow the hybrid logical clock) and 9 (snapshot selectors); operations `branch_info`, `set_parent_line`, `commit_info`, `read_range`, `history`, `tree_diff`, `merge_preview`, `change_set`, `tag`, `delete_tag`, `tag_info`, `release`, `revert`, `process_info`, `lineage`, `derivation`, `impact`, `outbox`, `ack`, `bulk`; `commit`, `read`, `read_ledger`, `merge`, `create_branch` and `set_limits` extended; `hide`, `unhide` and `derivation` in `DAT-AdapterChange`.
  15. Extensions (STD-CLS-030) stay visible by commit and inheritance only: the valid period of an extension object is not considered in 1.0-rc1.
- **Consequences:** 187 vectors (VER-CONF-1250…1265, 1280…1289, 1300…1315, 1330…1334, 1380…1382, 1525…1531, 1550…1555, 1560…1570, 1575…1590, 1605…1609, 1615…1628, 1645…1648, 1660…1668, 1800…1802, 1810…1817, 1825…1828, 1860…1864, 1875…1879, 1900…1905, 2120…2127, 2135…2141, 2150…2155, 2165…2171, 2215…2220), not tagged `prototype`; the 453 prototype vectors and the 0.1 vectors are unchanged. MUST coverage of 1.0-rc1 102/145. The kernel implements them in step 4.
- **Alternatives considered:** Requiring a change set for every merge into a protected branch was rejected: it would invalidate 44 direct merges of the frozen BPA 0.1 vectors. Eager invalidation of approvals by a kernel commit after every source or target change was rejected in favour of checking at the next action, which gives the same observable states. A Merkle tree over derivation inputs was replaced by the content ID of the sorted list, which already commits to the whole list.
- **Affects:** STD-VER-011, STD-VER-012, STD-VER-020, STD-VER-021, STD-VER-031, STD-VER-055, STD-VER-057, STD-VER-058, STD-VER-060, STD-VER-061, STD-VER-070, STD-VER-071, STD-VER-080, STD-VER-081, STD-VER-090, STD-TIME-013, STD-TIME-014, STD-TIME-015, STD-TIME-031, STD-TIME-040, STD-TIME-050, STD-TXN-030, STD-TXN-031, STD-TXN-040, STD-TXN-050, STD-TXN-070, STD-KIND-020, STD-ERR-002, STD-CONF-005

### DEC-042 — BPA 1.0-rc1 proofs (step 1d)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 1d of PH-1 step 1 (DEC-039) writes the oracle and vectors for the proof clauses of 1.0-rc1 that BPA 0.1 did not cover: STD-PROOF-010, -011, -020, -021, -030 and -070. STD-CONF-011 (algorithm registry) belongs with them because its algorithms are the ones proofs use. Several clauses left formats open that vectors must fix.
- **Decision:**
  1. STD-PROOF-010, -011: `sig` is base64url without padding; malformed signatures are `MODEL.INVALID_VALUE`, failed ones `PROOF.SIGNATURE_INVALID`. DID documents list keys with half-open validity, and the signer must be the subject of the document. A rotation closes the retired key after every transaction time it signed. Commit records carry the node signature over the commit ID.
  2. STD-PROOF-020: nodes are `{level, entries}` as hashed by STD-PROOF-001, and each parent holds `[first key, hash]` of the node below. With a commit, the verifier checks the commit ID, its signatures at `tx_time` and its root.
  3. STD-PROOF-021: records run from the later commit back to the earlier one; first-parent links and strictly decreasing transaction times are checked.
  4. STD-PROOF-030: entries `[<vae>/<branch>, head commit ID]`, with the node signature over the root.
  5. STD-PROOF-070: archive format `ubos.archive.v1` with a signed manifest. The importer verifies every content ID, signature, state root and head root, and imports into an empty store.
  6. STD-CONF-011: AEAD inputs are key, nonce (12 or 24 bytes), associated data and ciphertext with the tag appended; XChaCha20-Poly1305 follows draft-irtf-cfrg-xchacha.
  7. STD-ERR-002: new code `PROOF.VERIFICATION_FAILED`.
  8. STD-CONF-005 (1.0-rc1): convention 10 (the adapter's node key and signed commit records). Pure operations `public_key`, `sign`, `verify_signature`, `aead_decrypt`, `inclusion_proof`, `verify_inclusion` and `verify_consistency`; stateful operations `node_did`, `rotate_node_key`, `commit_record`, `prove`, `consistency_proof`, `tenant_root`, `export_archive` and `import_archive`.
  9. The oracle implements Ed25519, ChaCha20-Poly1305 with HChaCha20, and AES-256-GCM in plain Python so that it needs no third-party package. Each algorithm was checked against the published test vectors of RFC 8032, RFC 8439, draft-irtf-cfrg-xchacha, FIPS 197 and the GCM specification. This code is reference code for vectors only.
- **Consequences:** 66 vectors (VER-CONF-2360…2370, 2375…2383, 2390…2402, 2410…2418, 2420…2423, 2510…2520, 6200…6208), not tagged `prototype`. Tampered proofs and archives, and one archive written by the oracle, are embedded as literals, so importing an archive of another implementation is tested. The earlier vectors are unchanged. MUST coverage of 1.0-rc1 is 109/146 (STD-PROOF-011 now states a MUST). The kernel implements this in step 6.
- **Alternatives considered:** The `cryptography` package was rejected for the oracle: it is a native dependency that is not present in every environment. Comparing archive counts of chunks was rejected because chunking is an implementation choice; version records, entries, commits, refs and tags are compared instead.
- **Affects:** STD-PROOF-010, STD-PROOF-011, STD-PROOF-020, STD-PROOF-021, STD-PROOF-030, STD-PROOF-070, STD-CONF-011, STD-ERR-002, STD-CONF-005

### DEC-043 — BPA 1.0-rc1 L1 expressions, queries, addressing and value types (step 1e-1)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 1e of PH-1 step 1 (DEC-039) covers execution: STD-EXPR, STD-ADDR, STD-CTX, STD-ISA and STD-BPU, plus STD-FND-026 and STD-TXN-023. That is 53 clauses, 37 of them MUST clauses without vectors. The owner confirmed on 2026-09-30 that it is split into two pull requests. Part 1e-1 (this decision) writes the oracle and vectors for STD-EXPR-001…005 and -011, STD-ADDR-001, -002 and -010, and STD-FND-026.
- **Decision:**
  1. Step 1e is delivered as 1e-1 (L1 expressions, queries, addressing and custom value types) and 1e-2 (execution context, instruction set, BPU and automatic retry). In 1e-2, execution vectors express logic as host programs: JSON sequences of instruction calls with L1 arguments, which STD-ISA-007 allows as a binding. Rhai-specific behaviour is verified by the L2 escape suite (W-12, EXIT-1-10).
  2. STD-EXPR-001: lexical rules for numbers, money literals, strings, keywords and `matches` patterns; `sort_by` joins the lambda functions; lambda variables must not hide other names; syntax errors are `LOGIC.TYPE_ERROR` with a position.
  3. STD-EXPR-002: integer arithmetic stays integer except `/`, which needs the target field's scale; `%` takes integers and keeps the sign of the dividend. `enum` and `reference` are `text`. `json` fields and `ref` results are typed `any` and checked at run time. A list literal has the common type of its items, or `list<any>` when they differ. Error details carry the zero-based position.
  4. STD-EXPR-003, -004: `.` on `null` is an error, and `?.` is the null-safe form; conditionals, `in`, functions and aggregates have fixed null rules. Evaluation charges at least one unit per operator, call and item, and 10 units for `ref` and `exists`. `max_items` bounds lists.
  5. STD-EXPR-005: exact semantics of the text, number, money, date, calendar, period, reference and identifier functions. Calendars and rate objects are inputs. `allocate` uses the largest remainder. `add_months` clamps to the end of the month.
  6. STD-EXPR-011: `null` in `where` excludes the object, `order_by` puts nulls first with the object ID as the last key, `limit` is 1 to 1,000, and cursors bind the snapshot and the query. Row visibility and masking come with the Governance profile.
  7. STD-ADDR-001, -002: malformed URIs are `MODEL.INVALID_VALUE`; `@vN` counts versions along the first-parent history; slug redirects use earlier slugs; `key` is not supported in 1.0-rc1; duplicate selectors conflict; the branch of a commit is the one in its record.
  8. STD-ADDR-010: key form with the multicodec prefix `0xed 0x01`; derived documents have one key. The adapter's DIDs (node, default initiator, example principals in vectors) now use the registered form `did:ubos:acme:<uuid>`. This changes the principal strings, and the signatures and hashes over them, in 76 vectors of DEC-041 and DEC-042; their tested behaviour is unchanged.
  9. STD-FND-026: the value is bound as `value`. Order of checks: canonical base value, normaliser, constraints, validation. A validation result of `null` is not violated. The base is a scalar type.
  10. STD-CONF-005 (1.0-rc1): pure operations `check_expr`, `eval_expr`, `parse_uri`, `parse_did` and `key_did`; stateful operations `resolve`, `query` and `did_document`; `validate_value` and `load_genesis` take `value_types`. `cursor` is compared only through `same_as`.
  11. The oracle has an L1 interpreter in plain Python (`standard/tools/bpa/expr.py`).
- **Consequences:** 174 vectors (VER-CONF-0260…0269, 4400…4422, 4460…4480, 4500…4515, 4520…4530, 4540…4587, 4750…4760, 5200…5215, 5240…5246, 5280…5290), not tagged `prototype`. MUST coverage of 1.0-rc1 is 119/146; the remaining 27 MUST clauses are those of 1e-2. The kernel implements L1 in step 5.
- **Alternatives considered:** A Python interpreter of Rhai in the oracle was rejected as large and risky; Rhai behaviour is tested against the real engine by the escape suite. Strictly homogeneous list literals were rejected because they make multi-result vectors unreadable; static typing remains exact for homogeneous lists.
- **Affects:** STD-EXPR-001, STD-EXPR-002, STD-EXPR-003, STD-EXPR-004, STD-EXPR-005, STD-EXPR-011, STD-ADDR-001, STD-ADDR-002, STD-ADDR-010, STD-FND-026, STD-CONF-005, DEC-039

### DEC-044 — BPA 1.0-rc1 execution (step 1e-2)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 1e-2 of PH-1 step 1 (DEC-043) writes the oracle and vectors for STD-CTX-001…006, -010…012, -020, -022…024 and -030, STD-ISA-001…007, -010…016, -030…034, -050…052 and -055, STD-BPU-001, -002, -004, -010 and -011, and STD-TXN-023. As DEC-043 decided, the vectors express logic as host programs.
- **Decision:**
  1. Host programs: logic assets of tier `host` (a conformance tier, STD-CONF-002) with JSON statements whose arguments are L1 expressions; port functions see `this` and `self_id`. They are defined in STD-CONF-005 convention 11. The genesis class `Logic` below `LogicAsset` holds `DAT-LogicAsset`.
  2. STD-CTX-001…006: the caller-owned and implementation-owned context members; identity parameters fail with `LOGIC.PARAMETER_FORBIDDEN`; capabilities name families or instructions; instructions of the ABI that rc1 does not provide fail with `LOGIC.NOT_AVAILABLE`; limits may be lowered, raising them needs an entitlement (`IAM.DENIED`); exact unit counting for host programs; CPU, memory and wall time are measured by benchmarks and the escape suite.
  3. STD-CTX-010…012: the entropy stream (SHA-256 counter mode over a journaled seed; version 4 UUIDs); journal records with content IDs of arguments and results; the replay divergence report; replay neither writes nor meters.
  4. STD-CTX-020…024, -030: static rules for L2 sources; an adapter without an L2 engine reports `LOGIC.NOT_AVAILABLE`; the verification gate compares code identity; finding rules; nesting limit 32 per function; pinning per asset object.
  5. STD-ISA-001…007: descriptors read from the family files; ABI checks; how items are counted for costs; grants in place of policies until the Governance profile, with read denials reported as not found; logic raises registered codes only.
  6. STD-ISA-010…055: object IDs from the entropy; logic writes carry derivation records; merged changes within a process; adjustments correct one entry; links without attributes classes in rc1; the first read fixes the snapshot; port implementations name `<asset>#<function>`; library imports `{name, version}`; job requests as outbox effects.
  7. STD-BPU-001, -002, -011: non-BPU classes; descriptor members; the five checks of class verification, with `lifecycle` and `tests` not applicable until the Governance profile.
  8. STD-TXN-023: `retryable` on the exported function, `max_attempts` counts all attempts, and entropy per attempt.
  9. STD-CONF-005 (1.0-rc1): operations `instructions`, `profile_limits`, `execute`, `journal`, `metering`, `metering_total`, `replay`, `verify_asset`, `bpu_descriptor`, `bpu_state` and `verify_bpu`; `source_refs` in `DAT-AdapterChange`; a case step may omit its expectation (it must still succeed).
- **Consequences:** 160 vectors (VER-CONF-2100…2103, 2600…2603, 2610…2613, 2635…2637, 2650…2652, 2660…2663, 3000…3001, 3010…3014, 3030…3032, 3040…3045, 3050…3054, 3060…3061, 3100…3105, 3120…3124, 3150, 3165, 3170, 3195…3196, 3215…3216, 3300…3303, 3330, 3340, 3350, 3360…3361, 3400…3403, 3430…3431, 3440…3441, 3480, 4000…4005, 4020…4022, 4030…4036, 4050…4060, 4080…4083, 4100…4102, 4120…4124, 4150…4152, 4165…4169, 4200…4206, 4270…4276, 4285…4292, 4305…4312, 4345…4346), not tagged `prototype`. MUST coverage of 1.0-rc1 is 146/146 (100%). The earlier vectors are unchanged. The kernel implements the runtime in step 5. The growth of the L2 escape suite to 300 cases (W-12, EXIT-1-10) remains part of step 5.
- **Alternatives considered:** Leaving the nesting limit of host programs at 64 was rejected: such a program is nested deeper than the default recursion limit of common JSON parsers (128). The limit follows the L2 expression depth in functions instead. Executing L2 scripts in the oracle was rejected as in DEC-043.
- **Affects:** STD-CTX-001…006, STD-CTX-010…012, STD-CTX-020, STD-CTX-022…024, STD-CTX-030, STD-ISA-001…007, STD-ISA-010…016, STD-ISA-030, STD-ISA-050, STD-ISA-052, STD-ISA-055, STD-BPU-001, STD-BPU-002, STD-BPU-011, STD-TXN-023, STD-CONF-005

### DEC-045 — Release of BPA 1.0-rc1 (step 1f)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 1f of PH-1 step 1 (DEC-039) releases BPA 1.0-rc1. After parts 1a–1e-2 the release holds 171 clauses and 1,227 vectors, and all 146 MUST clauses with a vector range are covered (EXIT-1-01).
- **Decision:**
  1. **Release notes.** `standard/release/<version>/notes.md` is written by hand, like `scope.json`, and lists the content, the changes since the previous release and the known limits. The builder lists it in the manifest when it exists; BPA 0.1 has none and stays frozen.
  2. **Tag.** After merge the owner creates the signed tag `bpa-v1.0.0-rc1` on the merge commit, with the manifest `content_id` in the tag message (DSN-BPA-001, DEC-030).
  3. **Freeze.** Once tagged, BPA 1.0-rc1 is frozen like BPA 0.1 (DEC-039): its clauses and vectors are not changed. `regen_all.sh` keeps building 1.0-rc1 and its output must stay identical until a decision opens BPA 1.0-rc2 (gate 1b, EXIT-1-14). Defects found later are recorded in `standard/errata/` and corrected in the next release.
  4. **Known limits** stated in the notes: vectors added after 0.1 are not tagged `prototype` and are run by the kernel (EXIT-1-02); L2-specific behaviour is verified by the escape suite (W-12, EXIT-1-10); rules that need the principal and policy model, and link objects of attributes classes, follow in 1.0-rc2.
- **Consequences:** EXIT-1-01 is met when the tag exists. PH-1 step 1 is complete; step 2 (extended formal models, EXIT-1-07) is next.
- **Alternatives considered:** Keeping the notes outside the manifest was rejected: the known limits belong to what the signature covers. Changing a tagged release in place was rejected: a signed manifest must stay verifiable.
- **Affects:** DSN-BPA-001, EXIT-1-01, DEC-039

### DEC-046 — Group commit and ledger sequence model (step 2)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** PH-1 step 2 (DEC-038) checks the extended formal model of VER-FORM-0004 (EXIT-1-07). W-8 asks for the commits of a batch to be prepared in parallel with only the sequencing and tree update serialised, to relieve the sequencer CPU bound found in PH-0 (RSK-016). VER-FORM-0004 states no bound.
- **Decision:**
  1. **Model.** `verify/models/group_commit.tla` models one branch with a two-phase flush: parallel preparation without the lock (snapshot, object writes with bases, ledger appends without sequences), then serial sequencing under the branch lock (validation against storage plus the commits accepted earlier in the batch, sequence allocation from the counter row, balance invariant `balance >= 0` after the earlier deltas, abort of the offending process only), then one storage transaction for commits, counter rows and tree update, which may fail; a failed transaction leaves storage unchanged and the batch is sequenced again. Retries follow STD-TXN-023. The tree root is a function of the committed state, so serial equivalence of the history covers the tree update.
  2. **Properties.** `DenseCseq`, `GapFree`, `PerKeyOrder`, `ProcessAtomic`, `SerialEquivalent`, `BalanceSafe`, `NoFalseConflict`, `FailuresJustified` (`verify/models/README.md`).
  3. **Bound.** VER-FORM-0004 is checked at 3 processes × 2 nodes × 2 objects × 2 ledger keys × batch 3 × 4 commits with one failed storage transaction, under the rule of DEC-031; CI runs 1 object, 1 ledger key and 3 commits.
  4. **Sensitivity.** A constant `Mutation` selects seven wrong variants; each must violate its target property (`verify/models/reports/README.md` §5.3).
  5. **Design clarifications.** DSN-DVM-006 states the two-phase flush; DSN-DVM-402 excludes non-transactional counters such as a PostgreSQL `SEQUENCE` and counts earlier appends of the batch; DSN-DVM-424 is added to VER-FORM-0004; DSN-DVM-006, -402, -404 and -424 list FORM among their verification methods.
- **Consequences:** EXIT-1-07 is met by the runs recorded in `verify/models/reports/README.md` §5. The kernel of PH-1 steps 3 and 4 implements the two-phase flush; its fault and simulation suites (EXIT-1-05, EXIT-1-06) exercise the same properties on PostgreSQL.
- **Alternatives considered:** Extending `commit_cas.tla` was rejected: it would change the PH-0 evidence of VER-FORM-0001. Modelling the lock-free head compare-and-set again was rejected: `commit_cas.tla` already checks it (DSN-DVM-208); the new model holds the branch lock as DSN-DVM-006 requires.
- **Affects:** VER-FORM-0004, EXIT-1-07, DSN-DVM-006, DSN-DVM-402, DSN-DVM-404, DSN-DVM-424, RSK-016

### DEC-047 — Storage kernel (step 3)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** PH-1 step 3 (DEC-038) builds the storage kernel of W-7: the storage trait on sqlx, row-level security and tenant partitions, the kernel indexes, chunk placement, compression and encryption, and the verifier, closing with the storage suite (VER-CONF-8402, EXIT-1-03). The PH-0 prototype stored engine records in `bpa_*` tables through the synchronous `postgres` crate (DEC-033). The engine still identifies commits by integers and classes by name; production identities follow in step 4. The storage layout stays open until W-1 is decided (DEC-038). The owner confirmed the plan on 2026-09-30.
- **Decision:**
  1. **Parts.** Step 3 is split into three pull requests. 3a: the storage traits, the sqlx adapter, the kernel schema as migrations, and the two-phase group commit. 3b: row-level security, tenant partitions, range indexes for valid periods, physical form (zstd, then AEAD with the tenant data key, verified on read) and chunk placement. 3c: the verifiers of indexes, sequences and roots (DSN-DVM-092), the storage suite, and the inspection record (EXIT-1-03).
  2. **Traits.** `ubos-store` defines the asynchronous traits `StoreTx` (one storage transaction: `lock_branch`, `create_branch`, `put_chunks`, `put_versions`, `put_entries`, `put_commit`, `cas_branch_head`, `upsert_index_rows`, `put_tree_nodes`, `alloc_sequences`, `outbox_append`, `idempotency_put`, `put_meta`, `commit`, `rollback`) and `StoreRead` (snapshot reads by `cseq`). They use only plain integers, strings and byte arrays (DSN-DVM-001). The PostgreSQL adapter runs them on Tokio through sqlx (DEC-007). The engine's synchronous `Store` view runs them on its own runtime until the kernel host of step 7 calls them directly.
  3. **Schema.** The kernel schema is created by versioned migrations in `crates/ubos-store/migrations/`, which follow UBS-SYS-DVM-04. If the results of W-1 require a layout change, it is made by a new migration before gate 1a.
  4. **Transition layout.** Until the engine carries production identities (step 4), the following columns stand in for them. Commits are keyed by the engine's integer `commit_no`; `commit_id` holds the content ID of the commit body. Class and object identifiers are text, and valid times are text in their canonical form. The table `object_state` keeps the engine's per-object state with `cseq` intervals, and `head_index` and `temporal_index` are maintained from it in the same flush. `kernel_meta` holds adapter settings.
  5. **Sequences.** Under the branch lock, `alloc_sequences` allocates ledger sequences from `ledger_sequence` rows (DSN-DVM-402). A new row starts after the entries its branch inherits through its read-through parents. Entries carry a hash chain per key (`prev_hash`). The engine's own sequence of each entry must equal the allocated one; a difference fails the flush.
  6. **Local providers.** Until the host services of step 7, the object store is a local directory and tenant data keys come from a local key provider.
  7. **Environment.** The storage suite runs on PostgreSQL 16 on the development workstation and in CI. The run on ENV-REF-SERVER named by VER-CONF-8402 is made at gate-1a acceptance (step 9) together with the benchmarks.
  8. **Verification.** The storage suite is verification work required by EXIT-1-03 and was requested with the step plan (DEC-038).
- **Consequences:** The `postgres` crate and the `bpa_*` tables are gone. Existing prototype databases must be recreated. The 453 prototype vectors pass on the new adapter. The benchmark driver loads and commits through it. Step 4 replaces the transition columns and `object_state`.
- **Alternatives considered:** A synchronous trait was rejected: it would block the Tokio reactor of the node host (DSN-NOD-001, DSN-DVM-617). Keeping the `bpa_*` layout until step 4 was rejected: the kernel indexes, counter rows and verifier of this step need the DVM-04 tables. Fixing the layout before W-1 was rejected (DEC-038).
- **Affects:** DSN-DVM-001, DSN-DVM-005, DSN-DVM-006, DSN-DVM-402, VER-CONF-8402, EXIT-1-03, UBS-SYS-DVM-04, DEC-033, DEC-038

### DEC-048 — Tenancy and physical form of the storage kernel (step 3b)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 3b of PH-1 step 3 (DEC-047) implements DSN-DVM-002, -003 and -010 on the kernel schema of 3a. Four questions were open. First, row-level security does not apply to a table's owner or to superusers. Second, PostgreSQL rejects `COPY FROM` on tables with row-level security. Third, emptying 15 tables with hundreds of partitions for every conformance vector is slow. Fourth, valid times are still text in the transition layout.
- **Decision:**
  1. **Partitions.** `chunk`, `object_version`, `ledger_entry`, `temporal_index` and `merkle_node` are hash-partitioned by `tenant_id` into 64 partitions (migration `0002_tenancy.sql`).
  2. **Row-level security.** Every table has the policy `tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid` for reads and writes. A session without `ubos.tenant` therefore sees and writes no rows.
  3. **Roles.** Migrations run as the connecting owner role. The adapter's pool then works as the role `ubos_app`, which is never the owner, so the policies apply. Each connection sets `ubos.tenant`, `synchronous_commit = on` and `plan_cache_mode = force_custom_plan`. The last setting lets the planner prune partitions for every statement. Generic plans scanned all 64 partitions in updates.
  4. **Ledger immutability.** `ubos_app` has no UPDATE or DELETE privilege on `ledger_entry`. A trigger raises `LEDG.IMMUTABLE_ENTRY` for any role (STD-KIND-020).
  5. **Range index.** `temporal_index` has a GiST index over `int8range(cseq_from, cseq_to)` (extension `btree_gist`). The valid-period range of UBS-SYS-DVM-04 follows when valid times become `timestamptz` in step 4.
  6. **Stored-byte format.** Stored bytes have the form `0x01 | key id (u32) | nonce (24 bytes) | XChaCha20-Poly1305 ciphertext with tag` over the zstd-compressed logical bytes (STD-CONF-011). The associated data is `"ubos.physical.v1" 0x00 tenant 0x00 content-id`. Version envelopes and chunks use this form. A failed authentication or decompression, or a chunk whose content ID differs, raises `NOD.BLOB_UNAVAILABLE` with `{"integrity": true}` (DSN-DVM-002). Version IDs are computed over the envelope, not over the stored engine record, so for versions the authentication tag is the integrity check until step 4.
  7. **Keys.** `ubos-crypto` provides the key-service trait. It also provides `LocalKeys`, which derives tenant data keys from `UBOS_LOCAL_MASTER_KEY` or a fixed development secret and is never used for production (DEC-047).
  8. **Chunk placement.** A chunk whose stored form is at most 8 KiB is inline. Larger chunks go to the blob store under `t/<tenant-hash>/c/<content-id>` before the flush transaction (DSN-DVM-003). The blob store is a local directory (`UBOS_BLOB_DIR`) until step 7. The engine starts to chunk bodies (STD-OBJ-008) in step 4.
  9. **Adapter reset.** The conformance adapter's `reset` moves to a fresh VAE of the same tenant instead of truncating (STD-CONF-002). Benchmark datasets keep a fixed scope and purge all tables. Bulk loads with `COPY` run as the owner role.
- **Consequences:** The 453 prototype vectors pass on PostgreSQL in about 6 s. The benchmark tools run unchanged. On the development workstation, custom planning over 64 partitions adds about 2 ms per statement to the commit path. W-1 decides whether the partition count or statement shape must change before the layout is fixed (DEC-038, RSK-016). The storage suite of 3c checks these properties.
- **Alternatives considered:** Connecting as the owner with `FORCE ROW LEVEL SECURITY` was rejected: superusers still bypass it. Truncating on every reset was rejected: a conformance run took 7.5 minutes instead of 6 s. AES-256-GCM was rejected for stored bytes: random 12-byte nonces limit the number of writes per key, while 24-byte nonces do not.
- **Affects:** DSN-DVM-002, DSN-DVM-003, DSN-DVM-010, UBS-SYS-DVM-04, DEC-047, RSK-016

### DEC-049 — Verifiers and storage suite (step 3c)
- **Status:** Accepted · **Date:** 2026-09-30
- **Context:** Part 3c of PH-1 step 3 (DEC-047) closes EXIT-1-03. It needs two things: the verifier of DSN-DVM-092, and the storage suite of VER-CONF-8402 with its inspection of the storage traits. VER-CONF-8402 lists transactions, snapshot reads, index maintenance, sequences, chunk placement and row-level security. It does not fix the cases.
- **Decision:**
  1. **Index verifier.** `ubos_store::verify::verify_indexes(branch, from, to)` works only through `StoreRead`. It rebuilds the `temporal_index` rows of the object rows written in a `cseq` range and the whole `head_index` from the committed object rows (`object_state` in the transition layout). It compares checksums per bucket of object identifiers (XOR of row digests, 16 buckets) and lists the differing rows of every bucket that disagrees, by object key.
  2. **Sequence verifier.** `ubos_store::verify::verify_sequences(branch)` checks, for each ledger key, that the branch's own entries continue densely after the inherited ones and that `prev_hash` links them. It also checks that the counter row agrees with the last sequence and hash (DSN-DVM-402).
  3. **Root verifier.** `ubos_dvm::verify::verify_roots(branch, from, to)` recomputes each commit's state root from the committed state and compares it with the recorded root. It then checks that every stored tree node reachable from the roots hashes to its identifier (DSN-DVM-007, STD-PROOF-001).
  4. **Storage suite.** The binary `ubos-store-suite` runs 20 cases on PostgreSQL after a workload of objects, updates, appends, a child branch and a merge. The cases cover transactions, snapshot reads, index maintenance with an injected corruption, sequences (10,000 appends with 5% injected flush failures, 200 concurrent appends, branches), chunk placement, the physical form, tenancy, and roots with an injected corruption. They use the acceptance criteria of the design items as pass criteria. One case is an automated aid to the trait inspection.
  5. **Evidence.** The report and the inspection record are in `verify/reports/exit-1-03/`. CI runs the suite with 1,000 appends.
- **Consequences:** EXIT-1-03 is met on PostgreSQL 16 on the development workstation and in CI. The run on ENV-REF-SERVER named by VER-CONF-8402 follows at gate-1a acceptance (DEC-047). PH-1 step 3 is complete; step 4 (model, versioning, time, ledger and transactions at production quality) is next. When step 4 replaces the transition layout, the verifiers and the suite are adapted with it.
- **Alternatives considered:** Rebuilding the indexes from versions instead of from the committed object rows was rejected for now. In the transition layout the engine's object rows are authoritative, and versions do not carry branch intervals. Step 4 revisits this together with the layout. A separate SQL-only verifier was rejected: the verifiers must run unchanged on the SQLite adapter (DSN-DVM-001).
- **Affects:** DSN-DVM-092, DSN-DVM-001, DSN-DVM-002, DSN-DVM-003, DSN-DVM-005, DSN-DVM-402, DSN-DVM-404, DSN-DVM-010, VER-CONF-8402, EXIT-1-03, DEC-047

### DEC-050 — Engine at production quality (step 4)
- **Status:** Accepted · **Date:** 2026-10-02
- **Context:** PH-1 step 4 (DEC-038) brings model, versioning, time, ledger and transactions to production quality (EXIT-1-02, W-10) and replaces the transition layout of DEC-047. Of the 1,227 vectors of BPA 1.0-rc1, 453 tagged `prototype` passed before this step; 397 further vectors of the areas FND, OBJ, CLS, VER, TIME, TXN, ERR and CONF are this step's target. Runtime (CTX, ISA, EXPR, ADDR, BPU) belongs to step 5 and proofs (PROOF) to step 6. The owner confirmed the plan on 2026-10-02.
- **Decision:**
  1. **Parts.** Step 4 is four pull requests. 4a: the class registry (STD-CLS-001…030, -040, -041; DSN-DVM-101…114) with the adapter operations `linearize`, `effective_class`, `dispatch`, `type_selector`, `traverse`, `class_at`, `list_objects`, `blind_index` and `set_limits`. 4b: versioning (STD-VER, DSN-DVM-2xx). 4c: time, ledger and transactions (STD-TIME, STD-TXN, STD-CONF-011; DSN-DVM-3xx, 4xx, 5xx). 4d: production identities and tenancy — the transition layout of DEC-047 is replaced by migration, tenant scope and principal and node DIDs are implemented, verifiers and the storage suite are adapted, and a signed runner report on PostgreSQL is produced.
  2. **Registry (4a).** `ubos-model` keeps declarations as the JSON given to the adapter and computes effective definitions as the reference oracle does: C3 linearization with traits, kinds along the parent line, narrowing of refined fields, port overrides and dispatch, type selectors, and class extensions in the order most general target, `order`, namespace. The genesis model declares `Object`, `DataObject`, `LogicAsset`, `ViewAsset`, `ConfigAsset`, `SystemObject`, `ClassExtension`, `ChangeSet`, `Logic` and `Release`.
  3. **Commit-time checks (4a).** A commit loads the full branch state when the registry declares references or relationships, when extensions are in force, or when the commit writes a `ClassExtension`. It then applies the `on_end` rules and compositions of ended objects, checks reference integrity of written versions and appended entries (resolution, target class, validity over the period or at the instant, link objects), checks cardinality, and checks that every written extension applies to its target. Object URIs use the authority `acme` of the conformance adapter (STD-CONF-005).
  4. **Custom value types.** STD-FND-026 declares `normalize` and `validate` as L1 expressions. The kernel evaluates them with `ubos-expr` in step 5; until then the registry accepts the declarations, and a body or `validate_value` that needs one fails with `ADAPTER.UNSUPPORTED`. The nine vectors of STD-FND-026 therefore move from 4a to step 5; this changes nothing in the plan's exit criteria.
  5. **Reading rule.** The vectors of BPA 1.0-rc1 are frozen (DEC-045). A clause left open by the vectors is clarified in the clause text and recorded here; a defect of a vector goes to `standard/errata/` for 1.0-rc2.
- **Consequences:** After 4a, 1,170 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes (453 prototype; CLS 176, OBJ 10, ERR 1 of this step; 530 of other areas), 57 fail and the rest are not applicable. The storage suite and the benchmark tools run unchanged. The benchmark workload declares no references, so its commits do not load the full state.
- **Alternatives considered:** Translating declarations into typed Rust structures was rejected for 4a: the effective definitions must compare byte for byte with the oracle's JSON, and the vectors fix that shape. Evaluating custom value types without the L1 evaluator was rejected: the expressions are L1 by STD-FND-026.
- **Affects:** STD-CLS-001…030, STD-CLS-040, STD-CLS-041, STD-FND-026, STD-OBJ-009, STD-OBJ-012, DSN-DVM-101…114, EXIT-1-02, DEC-047

### DEC-051 — Versioning in the kernel (step 4b)
- **Status:** Accepted · **Date:** 2026-10-02
- **Context:** Part 4b of PH-1 step 4 (DEC-050) implements the versioning clauses of BPA 1.0-rc1 in the kernel: STD-VER-011, -012, -020, -021, -031, -055, -057, -058, -060, -061, -070, -071, -080, -081 and -090 (122 vectors). The prototype's commit record held parents, ordinal, branch and root only, and branches had no metadata.
- **Decision:**
  1. **Commit records.** A commit record carries its content ID (`ubos.commit.v1` over VAE, branch, parent commit IDs, ordinal, transaction time and root), its transaction time from a hybrid logical clock over the adapter clock (one microsecond after the parent when the clock did not advance), the operations per object, the hide markers in force, and the members `rule_binding`, `process_id`, `resolutions`, `change_set`, `reverts`, `retroactive` and `appended` (DEC-041). Commit results report `commit_id` and `tx_time`.
  2. **Branch metadata.** Kind, creation and activity instants, parent line and origin are kept per branch in the store's settings (`branch:<name>`), tags in `tag:<name>`, change sets in `cs:<id>` and explicit processes in `process:<id>`, until step 4d gives them columns. Lifetimes, archiving (`VER.BRANCH_ARCHIVED`), merge targets (`VER.MERGE_NOT_ALLOWED`, `VER.CHANGE_SET_REQUIRED`) and the write policy of protected branches follow STD-VER-011 and -012.
  3. **Parent lines.** A branch created with a parent line starts with its own empty genesis commit and no read-through parent. A view resolves the parent at its head (live), at the pin (pinned), or at the snapshot time; own versions and hide markers override it (STD-VER-080). A commit on such a branch loads the view: unchanged inherited objects are not written, overrides are, and hide and unhide change the markers of the commit record. Reads that need the view (a parent line, or the selectors `commit`, `tag`, `tx_time`, `evaluated_at`, or `provenance`) use it; other reads use the object index as before.
  4. **Merges.** A merge is prepared (base, three-way merge with recorded resolutions, STD-VER-058), then checked by the engine: write policy, validation of the merged state relative to the target (bodies of changed objects, references of changed and referencing objects, link objects, cardinality, double entry, period rule, extensions; STD-VER-055), and written with its rule binding. A preview keeps conflicting objects as on the target and reports changes, conflicts, validation and counts; its content ID excludes the heads (STD-VER-057).
  5. **Change sets, tags, releases, revert** follow the reference oracle: change-set state in the `ChangeSet` object on the target with approval invalidation by preview ID; tags signed by a content ID until step 6 brings signatures; releases as tag plus `Release` object; revert as the three-way merge of the state before a commit into the head with the state after it as base, objects created by it ended, reversals appended, validated as a merge.
  6. **Processes.** Processes named by the caller are recorded with their kind, trigger, initiator, state and commits (STD-TXN-031); generated identifiers are not recorded, since nothing can refer to them.
- **Consequences:** All 122 versioning vectors pass; 784 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes. The remaining engine vectors are of time, ledger and transactions (4c). The commit path now reads the branch metadata (cached per adapter), the extensions in force, and the parent commit for the transaction time, and writes the activity instant of `draft` branches; on the development workstation this adds about 6 ms of busy time per commit, which W-1 measures on the reference host. The adapter's `reset` no longer restarts the commit identifier sequence: identifiers are internal and other VAEs of the same database keep theirs.
- **Alternatives considered:** Keeping branch metadata in the kernel tables now was rejected: step 4d changes the layout anyway. Writing hide markers as index rows was rejected: a marker is part of the commit record in the standard, and the view must subtract it from the parent's objects, not from the branch's own.
- **Affects:** STD-VER-011, STD-VER-012, STD-VER-020, STD-VER-021, STD-VER-031, STD-VER-055, STD-VER-057, STD-VER-058, STD-VER-060, STD-VER-061, STD-VER-070, STD-VER-071, STD-VER-080, STD-VER-081, STD-VER-090, STD-TXN-031, DEC-050

### DEC-052 — Time, ledger and transactions in the kernel (step 4c)
- **Status:** Accepted · **Date:** 2026-10-02
- **Context:** Part 4c of PH-1 step 4 (DEC-050) implements the time, ledger and transaction clauses of BPA 1.0-rc1 that the engine still answered with `ADAPTER.UNSUPPORTED` or without evidence: STD-TIME-013, -014, -015, -031, -040 and -050, STD-TXN-030, -040, -050 and -070, and the AEAD algorithms of STD-CONF-011 (57 vectors). The adapter gains `read_range`, `history`, `derivation`, `impact`, `outbox`, `ack`, `bulk` and `aead_decrypt`; reads gain `evaluated_at` and `follow`. STD-TXN-023 (retryable processes, 4 vectors) needs `execute` and stays with step 5; the signature vector of STD-CONF-011 stays with step 6.
- **Decision:**
  1. **Evidence and coordinates.** Every bitemporal answer carries `evaluated_at`: on the view path the branches the view consulted, on the index path the branch and the commit the selectors resolved (STD-TIME-015). `follow` resolves the targets of a reference field on the view at the `(asof, snapshot)` of the source, omitting targets without a segment (STD-TIME-013). `read_range` clips the timeline to `[from, to)` (STD-TIME-014); `history` lists the operations on an object per commit of the first-parent history, filtered by transaction time (STD-TIME-040).
  2. **Derivation records.** A change with a `derivation` is validated at commit (logic list, mode, inputs as `{object, version, asof}` naming an existing version of that object or an entry hash) and its record is stored under its content ID (`ubos.chunk.v1`); versions and entries carry it as `provenance_ref` inside the hashed envelope, entries likewise carry `source_refs`; the commit record lists the derived outputs (`derived`) and the rule binding names their logic (STD-TXN-030, STD-TIME-040). Inputs beyond the configured maximum are summarised per class with the range of commit ordinals in which the input versions first appeared on the branch. Version records stay content-addressed and carry no ordinal: the version table is shared by the VAEs of a tenant, so an ordinal stored with a version would be that of its first writer; the ordinal is derived from the branch history (step 4d answers it from the temporal index).
  3. **Impact set.** The retroactive ranges of a commit seed the set; the closure runs over the derived outputs of the branch's ancestors in commit order, a derivation record's inputs hitting when their version was superseded at `asof` or was itself hit (STD-TIME-031).
  4. **Idempotent requests.** A process with an `idempotency_key` is keyed by `(tenant, initiator, key)`; the hash of `{branch, changes}` and the result are stored with the commit and replayed without effect, and another payload under the key fails with `TXN.IDEMPOTENCY_MISMATCH` (STD-TXN-040). The retention sweep (DSN-DVM-531) comes with the table in step 4d.
  5. **Outbox and became-effective events.** The effects of a committed process are released under `<process_id>/<n>`; future segment starts of classes that declare `emit_became_effective` are scheduled and emitted as `BecameEffective` under `became-effective/<branch>/<object_id>/<start>` when the adapter clock reaches them and the segment still starts there with the same version; `set_now` and `outbox` run the schedule, `ack` acknowledges (STD-TXN-050, STD-TIME-050). An aborted or failed process releases nothing.
  6. **Bulk processes.** A bulk job commits its changes in chunks of at most `max_changes`, each as a child process (`trigger: job`, `parent_process` and `bulk` set), on the branch (`independent`) or on a staging branch merged once on completion (`all_or_nothing`); a failed job is resumed after its last committed child; attempts are counted as its children (STD-TXN-070). The children are implicit processes and not recorded (DEC-051, item 6).
  7. **Transition storage.** Derivation records, the outbox, the schedule, idempotency records and bulk state live under keys of the store's settings in the transition layout (UBS-SYS-DVM-04, DEC-047), like the branch and process metadata of DEC-051; step 4d moves the outbox and idempotency records into the kernel's `outbox` and `idempotency` rows written in the flush transaction (DSN-DVM-530, -534) and gives the rest tables.
  8. **AEAD registry and runner.** `ubos-crypto` provides `aead_decrypt` for AES-256-GCM (crate `aes-gcm`) and XChaCha20-Poly1305 with the error codes of the oracle (STD-CONF-011). The runner's list of implementation-dependent members removed before an exact comparison is aligned with STD-CONF-005 rule 6 as of 1.0-rc1 (`evaluated_at`, `commit_id`, `tx_time`, `provenance_ref`, … in addition to the five of release 0.1).
- **Consequences:** All TIME, TXN (except TXN-023) and the AEAD vectors of CONF-011 pass; 841 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes, 0 failures; the remaining 386 need the runtime (step 5: `execute`, `eval_expr`, `check_expr`, `query`, `resolve`, custom value types), proofs and signatures (step 6) or the node and BPU operations. The storage suite stays 20/20 and the benchmark check passes; the commit path adds the derivation check per change and the schedule check per written object, both without storage reads when no derivation or emitting class is involved.
- **Alternatives considered:** Storing the commit ordinal in the version record (as the oracle does in memory) was rejected after the PostgreSQL run showed the shared version table returning the ordinal of another VAE. Writing outbox rows in the flush transaction now was deferred to 4d together with the layout change, so that the engine's outbox is written once, into its final form.
- **Affects:** STD-TIME-013, STD-TIME-014, STD-TIME-015, STD-TIME-031, STD-TIME-040, STD-TIME-050, STD-TXN-030, STD-TXN-040, STD-TXN-050, STD-TXN-070, STD-CONF-005, STD-CONF-011, DSN-DVM-306, DSN-DVM-307, DSN-DVM-308, DSN-DVM-321, DSN-DVM-325, DSN-DVM-330, DSN-DVM-331, DSN-DVM-530, DSN-DVM-531, DSN-DVM-533, DSN-DVM-534, DSN-DVM-560, DEC-050, DEC-051

### DEC-053 — Production identities of the kernel schema (step 4d-1)
- **Status:** Accepted · **Date:** 2026-10-02
- **Context:** Part 4d of PH-1 step 4 (DEC-050) replaces the transition layout of DEC-047 and implements tenant scope, principal and node DIDs, adapts the verifiers and the storage suite and produces the signed runner report of EXIT-1-02. The owner confirmed on 2026-10-02 that 4d is split: 4d-1 the identities and the layout (this record), 4d-2 the metadata tables, tenant scope and DIDs, addressing operations and the report. Three points of UBS-SYS-DVM-04 could not be implemented as written: classes have no UUID in the standard (a class is identified by its name within a VAE and by the content ID of its declaration); the head of an object is a per-commit fact (STD-TXN-020 validates `base` against the head at the snapshot, and the tombstone version of an `end` is not in the timeline), so it cannot be derived from `temporal_index` at an earlier ordinal; and the identity of a ledger entry is its UUID `entry_id`, the content ID being its `entry_hash`.
- **Decision:**
  1. **Commits.** `commit` is keyed by `(tenant_id, vae_id, commit_id)`, the content ID of the canonical record; `commit_parent` names parents by content ID; `tx_time` (NULL for a genesis commit), `process_id` (explicit processes only, DEC-051) and `author` (the initiator DID) are columns. The engine's integer `commit_no` stays as an internal handle with a unique index, a surrogate like `branch_id`; `branch_ref` carries `head_commit` beside it. The VAE is part of the key because the engine's canonical record still names the VAE by the constant `acme` (4d-2 gives it the scope).
  2. **Identifiers and times.** Object and entry identifiers are `uuid`; valid times (`valid_from`, `valid_to`, `valid_at`) are `timestamptz` in UTC with `'infinity'` for an open end, and every index row records the granularity of its class (date or instant) so that the engine's canonical text is reproduced exactly on read; the valid-period range index of `temporal_index` promised by DEC-048 exists. Classes stay `class_name` with `class_version` (the content ID of the declaration); `object_version` carries class version, kind and valid period as columns.
  3. **Object state.** `object_state` is gone. `head_index` rows carry `cseq` intervals, kind, slug, granularity and the hidden flag; the engine's state of an object at an ordinal is its head row and its `temporal_index` segments visible there. Reads make two direct lookups rather than one join: planning a join over the partitioned `temporal_index` cost more than both executions.
  4. **Write sets.** Every commit record carries the object rows it wrote (`objects`: identifier, class, granularity, state or absence). The kernel indexes are a pure function of this write set (DSN-DVM-005): `verify_indexes` replays the commit records of a branch in order and rebuilds `head_index` and `temporal_index` rows with their intervals, so that the verifier no longer depends on a stored copy of the engine state (DSN-DVM-092).
  5. **Migration.** Migration `0003_identities.sql` drops the transition tables and recreates them; the transition layout carried no production data, so the kernel starts empty, and benchmark datasets are reloaded. `idempotency_record` points at commits by content ID.
- **Consequences:** All 841 passing vectors pass on PostgreSQL as in memory (0 differences); the storage suite stays 20 of 20 with the rebuilt verifier; the benchmark check (bulk load equals the commit path) passes. On the development workstation the commit path costs about 1 ms more of busy time (the second lookup) and the flush is unchanged within noise. UBS-SYS-DVM-04 is updated to the layout as implemented: class names with declaration IDs instead of `class_id`/`class_ord`, `head_index` with ordinal intervals, `entry_id uuid` with `entry_hash`, `commit_no` as handle, `granularity` columns.
- **Alternatives considered:** Keeping `object_state` under another name was rejected: the state is derivable from the two index tables and the write set. Rebuilding the indexes by replaying operations with the timeline algebra was rejected: merge commits record no operations, and the state tree holds heads but not timelines. Giving the engine content-ID commit handles now was rejected: the handle is internal, and the change would touch every layer for no visible gain.
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-004, DSN-DVM-005, DSN-DVM-092, DEC-047, DEC-048, DEC-049, DEC-050

### DEC-054 — Metadata tables, tenant scope and addressing (step 4d-2)
- **Status:** Accepted · **Date:** 2026-10-02
- **Context:** Second half of PH-1 step 4d (DEC-050, DEC-053). Since steps 4b and 4c the engine kept branch metadata, tags, processes, change sets, derivation records, the outbox, acknowledgements, idempotency records, bulk-job state and the schedule of future segment starts under keys of `kernel_meta`, and wrote effects and idempotency records after the flush, against DSN-DVM-530 and -534. The engine named the VAE by a constant (`acme`) in URIs and commit identities and the default principal by a constant. The addressing operations of STD-ADDR-001, -002 and -010 were not implemented.
- **Decision:**
  1. **Tables.** Migration `0004_metadata.sql` gives the records their tables: branch metadata as columns of `branch_ref` (kind, creation and activity instants, parent line, origin commit), `tag`, `process_record` (with the bulk-job state of STD-TXN-070 as a column), `change_set`, `derivation_edge` (one row per input of a derivation record, whose record is a chunk under its content ID), `outbox` with a stable `effect_id` and acknowledgement as `released_at`, `idempotency_record` scoped by `<vae>:<initiator>`, and `timer` for the future segment starts that emit BecameEffective. `kernel_meta` stays for adapter settings.
  2. **Written in the flush.** Effects (STD-TXN-050), the idempotency record with the commit's result (STD-TXN-040), derivation records with their edges (STD-TXN-030) and timers (STD-TIME-050) travel in the transaction and are written in the flush transaction of the commit; they exist if and only if the commit exists (DSN-DVM-530, -533, -534). BecameEffective events are released by the clock tick in their own transaction.
  3. **Interface.** The synchronous `Store` trait gains typed record methods with defaults over the settings keys (the memory store and commit-group overlays keep that form), which the PostgreSQL store overrides with its tables; the kernel traits `StoreTx` and `StoreRead` gain the corresponding row types and operations, and stay free of backend types. Commit handles and content IDs are translated at the store boundary.
  4. **Scope.** The store's scope carries the VAE's authority (STD-ADDR-001) beside tenant and VAE identifiers; the engine takes the authority from the store for URIs and the `vae` member of the commit identity, and the default principal from its own configuration. The conformance adapter uses `acme` and the principal of DEC-041. Commit identities keep naming the VAE by authority, so the commit table stays keyed within the VAE (DEC-053).
  5. **Addressing.** `ubos-types` gains the pure operations `parse_uri` (STD-ADDR-001, -002), `parse_did` and `key_did` (STD-ADDR-010, base58btc and base64url without dependencies); the engine gains `resolve`, which follows slug redirects through the versions of an object, selects by branch, commit, tag or time with the conflict rules of STD-ADDR-002, and addresses the n-th version on the branch line.
  6. **Report.** `verify/reports/exit-1-02/` holds an interim signed runner report on PostgreSQL after step 4: every vector of the areas implemented so far passes, and the areas of steps 5 and 6 are reported as not applicable. EXIT-1-02 is met when the full set passes at gate-1a acceptance (step 9).
- **Consequences:** 869 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes (28 more than step 4c: addressing); the remaining 358 need the runtime (step 5), proofs and signatures (step 6) or the node and BPU operations. The storage suite stays 20 of 20 (row-level security now checked on 18 tables) and the benchmark check passes. Every engine record now has a row with typed columns; the keys of `kernel_meta` are no longer used by the engine. STD-TXN-023 (4 vectors) stays with step 5.
- **Alternatives considered:** Routing the settings keys to tables inside the adapter was rejected: the engine would still speak in keys, and the SQLite adapter of PH-4 would have to repeat the routing. Giving the engine the VAE identifier instead of the authority in commit identities was rejected for now: the conformance oracle names the VAE by authority, and the step-6 commit records (STD-PROOF-010) are compared through it.
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-001, DSN-DVM-530, DSN-DVM-533, DSN-DVM-534, STD-ADDR-001, STD-ADDR-002, STD-ADDR-010, STD-TXN-040, STD-TXN-050, STD-TIME-050, DEC-041, DEC-051, DEC-052, DEC-053, EXIT-1-02

### DEC-055 — Runtime: L1 expressions (PH-1 step 5, part 5a)
- **Status:** Accepted · **Date:** 2026-10-02
- **Context:** PH-1 step 5 (runtime; EXIT-1-10) covers the L1 expression language, the L2 execution tiers and the instruction set, the journal and replay, and the Rhai host at production quality. The plan confirmed by the owner splits it into four parts, one pull request each: 5a L1 expressions (STD-EXPR-001…005, -010, -011, STD-FND-026); 5b the instruction set and executor with host-tier statement bodies, the evaluation context and profiles (STD-CTX-001…010, STD-ISA, STD-TXN-023); 5c journal and replay, the verification gate with a node-signed verification record (a minimal ed25519 node key in `ubos-crypto`), static analysis and the BPU operations; 5d the Rhai host (W-2…W-5), the escape suite of 300 or more cases and the EXIT-1-10 report. Until 5a the registry accepted custom value types but a body or `validate_value` that needed one failed with `ADAPTER.UNSUPPORTED` (DEC-050).
- **Decision:**
  1. **Crate.** `ubos-expr` (engine layer, depends on `ubos-model`, AR-031) implements the oracle semantics of `standard/tools/bpa/expr.py`: a hand-written lexer and recursive-descent parser with code-point positions (STD-EXPR-001, DEC-043), the static type checker over the effective class, parameter and context types (STD-EXPR-002), the three-valued null semantics (STD-EXPR-003), totality with the operation budget and the item limit of the strict profile (STD-EXPR-004, STD-CTX-004) and the ABI 1.0 function library (STD-EXPR-005) over exact decimals, dates, instants, durations, periods and money. Run-time values convert to and from canonical JSON; a `Resolver` trait supplies the objects behind `ref()` and `exists()`.
  2. **Evaluator form.** The evaluator walks the checked syntax tree. The bytecode form and the compile cache of DSN-DVM-610 follow with the performance work of part 5d, where the 1 µs criterion is measured; the semantics do not depend on the form.
  3. **Custom value types.** `ubos-model` gains the trait `CustomTypes` and `Registry::with_custom`: the engine installs the expression evaluator, the registry type-checks `normalize` and `validate` when value types are loaded and normalises then validates bodies through it (STD-FND-026). `ubos-model` stays free of the expression language.
  4. **Operations.** The adapter gains the pure operations `check_expr` and `eval_expr`, and `validate_value` accepts `value_types`; the engine gains the stateful `query` (STD-EXPR-010, -011): a class and its subclasses at a snapshot, `where` and `order_by` over typed bodies with references read from the same snapshot, pages of 1 to 1,000 objects and an opaque cursor bound to the query hash, the branch and the commit. `query` lives in `ubos-dvm` until `ubos-query` is created in PH-1b (UBS-ARC-07 unchanged).
- **Consequences:** 1,009 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes (140 more than step 4d-2: EXPR 130 and STD-FND-026 10); the remaining 218 need the execution tiers, the context and the instruction set (5b), the journal and the BPU operations (5c), or proofs and signatures (step 6). Storage suite 20 of 20 and benchmark check unchanged. The ten STD-FND-026 vectors moved from 4a (DEC-050) now pass.
- **Alternatives considered:** Compiling to bytecode in 5a was rejected: the vectors fix the semantics, not the form, and a second form before the semantics are green doubles the surface to debug. Making `ubos-model` depend on `ubos-expr` for custom types was rejected by AR-031 (`ubos-expr` depends on `ubos-model` for class types). Creating `ubos-query` now was rejected: PH-1b owns its scope (aggregation, traversal, history queries).
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-610, DSN-DVM-611, STD-EXPR-001…005, STD-EXPR-010, STD-EXPR-011, STD-FND-026, DEC-050, EXIT-1-10

### DEC-056 — Instruction set and executor (PH-1 step 5, part 5b)
- **Status:** Accepted · **Date:** 2026-10-03
- **Context:** Second part of PH-1 step 5 (DEC-055). After 5a the engine evaluated L1 expressions but ran no logic: the instructions of ABI 1.0 (STD-ISA-*), the execution context and its gateway (STD-CTX-001…006), deterministic entropy and the journal records (STD-CTX-010, -011), logic assets and version pinning (STD-CTX-022, -030) and the automatic retry of STD-TXN-023 were not implemented, and `ubos-runtime` held only the PH-0 Rhai sandbox spike (DEC-034). The oracle reads the instruction catalogue, the error registry and the profile table from the clause text of the standard.
- **Decision:**
  1. **Catalogue as data.** `standard/tools/gen_isa_json.py` writes `crates/ubos-runtime/src/isa.json` (instructions with parameters, costs, determinism and authorization; the error registry with its catchable flags; the profile limits and families) from the clause text through the oracle's reader, and `regen_all.sh` runs it; the file is generated and never edited by hand. The kernel embeds it, so the catalogue, the `instructions` and `profile_limits` operations and the gateway checks cannot drift from the standard (DSN-DVM-605, phase note).
  2. **`ubos-runtime`** (engine layer, allowed dependency `ubos-expr`, which re-exports the model layer) holds what is pure: the gateway of STD-CTX-003 (instruction-call limit, profile families, declared capabilities, ABI version, identity parameters, unknown and missing parameters, base cost, availability), the execution context of STD-CTX-001 with the members a caller may set and `LOGIC.CONTEXT_IMMUTABLE` for the rest, the budget with `LOGIC.LIMIT_EXCEEDED` and `LOGIC.CANCELLED` (STD-CTX-004…006), deterministic entropy and the `sys.*` and `adapter.observe` instructions with their journal records (STD-CTX-010), the unit of work with derivations, and the interpreter of host-tier programs (`let`, `do`, `if`, `each`, `repeat`, `try`, `return`, `fail`) over L1 templates, the in-process host binding of STD-ISA-007. The Rhai spike stays as `sandbox` until step 5d.
  3. **`Host` trait.** The store-facing half of every instruction (`commit.*`, `read.*`, `run.*`, `emit.event`) is implemented in `ubos-dvm` as the engine's `Host`: the pinned snapshot of the branch, targets by identifier or URI, pending objects, logic assets pinned per process with the rules of STD-CTX-022 (protected branches run verified assets only; the verification records come with step 5c), dispatch through the registry with the extensions in force, and the typed references of `ref()` and `exists()` inside programs (DSN-DVM-601, phase note).
  4. **`execute`.** One process per invocation (STD-TXN-001): the function or port runs on the snapshot, an interfering commit of the adapter may land between the run and the flush, the unit of work is committed through the same path as `commit` with the process's read set and `serializable` flag, and `TXN.HEAD_MOVED` or `TXN.READ_SET_CHANGED` starts another attempt with a derived seed when the function is retryable, up to five attempts (STD-TXN-023). Effects travel with the commit; a run without changes releases them against the head. The journal (context, snapshot pins, logic versions, seed, records, output hashes), the metering record and the invocation are stored under settings keys of the store, and the `journal`, `metering` and `metering_total` operations read them; the journal table and replay follow in step 5c.
- **Consequences:** 1,127 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes (118 more than step 5a: STD-CTX 53, STD-ISA 58, STD-TXN-023 4, and three Ledger vectors whose setups execute logic); the remaining 100 need the verification gate, replay and the BPU views (5c: 43) or proofs, signatures and the node key (step 6: 57). Storage suite 20 of 20 and benchmark check unchanged. `ubos-dvm` depends on `ubos-runtime`; UBS-ARC-07 is unchanged.
- **Alternatives considered:** Implementing the instructions as handlers inside `ubos-runtime` with store access (the literal reading of DSN-DVM-601) was rejected: AR-031 lets `ubos-runtime` depend on `ubos-expr` and `ubos-authz` only, and the engine already owns the commit path; the gateway and the semantics stay in one place, the store access in another. Typing the catalogue by hand in Rust was rejected: it would be a second copy of the standard.
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-601, DSN-DVM-602, DSN-DVM-603, DSN-DVM-605, STD-CTX-001…006, STD-CTX-010, STD-CTX-011, STD-CTX-022, STD-CTX-030, STD-ISA-001…007, STD-ISA-010…, STD-TXN-001, STD-TXN-023, DEC-034, DEC-055, EXIT-1-10

### DEC-057 — Verification gate, replay and BPU views (PH-1 step 5, part 5c)
- **Status:** Accepted · **Date:** 2026-10-03
- **Context:** Third part of PH-1 step 5 (DEC-055, DEC-056). After 5b logic ran on draft branches only: protected branches require verified assets (STD-CTX-022) and no verification record existed; journals and metering records lived under settings keys; replay (STD-CTX-012) and the BPU views (STD-BPU-001, -002, -011) were not implemented, and the verification record of STD-CTX-023 is signed by the node, which needed a signing key before step 6.
- **Decision:**
  1. **Verification gate.** `verify_asset` runs the five checks of STD-CTX-023 over the head version of a logic asset: static analysis (source size, statement nesting, unknown constructs, unbounded recursion, and for L2 scripts the forbidden constructs, closures over the gateway and bracket depth, for L3 components imports outside the world; STD-CTX-024), the type check of every L1 expression of a host program, the declared capabilities, the asset's tests (each run as a simulation on the branch, discarded afterwards) and the publisher signature of L3 components. A passing record is signed by the node, stored under its record ID, and recorded on the asset by a commit that sets `state` to `verified` and `verification` to the record ID; the engine accepts an asset on a protected branch when that record passed and its code identity still matches.
  2. **Node key.** `ubos-crypto` gains Ed25519 signatures over content IDs (`ed25519_public`, `ed25519_sign`, `sign` as DAT-Signature with the registered roles). The engine's node identity (`did:ubos:acme:…0001`, seed of the adapter convention, key `#k1`) lives in the store's settings and signs verification records; `node_did` reports it. Verification of signatures, key rotation and the DID documents follow with step 6.
  3. **Journal and metering as chunks.** Migration `0005_journal.sql` adds `journal_id` and `metering` to `process_record`; the `Store` trait gains `record` and `put_record` (a chunk under its content ID, written in its own transaction), and `execute` stores the journal (with the invocation it needs for replay), the metering record and their references; the `journal`, `metering` and `metering_total` operations read them. A process the caller did not name keeps its references under a run index.
  4. **Replay.** `replay` re-executes a process with its journaled context, snapshot pins, logic versions, entropy seed and records, with the adapter clock set to the journaled `now`; a tampered or missing record is reported as the first diverging instruction (`LOGIC.REPLAY_INPUT_MISSING` when a record is missing), and the result, change and effect hashes are compared with the journal's outputs. A replay commits nothing and records nothing.
  5. **BPU views.** `bpu_descriptor` (DAT-BpuDescriptor: fields with origin and classification, ledgers that reference the class, ports, events, JSON-schema identity), `bpu_state` (timeline and the entries that name the object) and `verify_bpu` (ports verified, events emitted, documentation; lifecycle and tests not applicable in 1.0-rc1) are ports of the oracle; `read.effective` with `descriptor` returns the descriptor.
- **Consequences:** 1,165 of 1,227 vectors pass in memory and on PostgreSQL with identical outcomes (38 more than step 5b: the verification gate, replay and BPU vectors of STD-CTX and STD-BPU); the remaining 62 need the node operations, signatures and proofs of step 6. Storage suite 20 of 20 and benchmark check unchanged. The `kernel_meta` keys `journal/…`, `metering/…` and `invocation/…` of step 5b are no longer used. Step 5 continues with 5d (Rhai host W-2…W-5, escape suite, EXIT-1-10 report).
- **Alternatives considered:** Keeping journals under settings keys until step 6 was rejected: DSN-DVM-630 names chunks and the process-record reference, and the migration is small. Deferring the node key to step 6 was rejected: the verification record is signed, and four execute vectors wait on verified assets. Giving the test runs of the gate their own process records was rejected: the oracle discards everything a test run touches.
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-630, DSN-DVM-631, STD-CTX-012, STD-CTX-022, STD-CTX-023, STD-CTX-024, STD-BPU-001, STD-BPU-002, STD-BPU-011, STD-PROOF-011, DEC-048, DEC-055, DEC-056, EXIT-1-10

### DEC-058 — L2 host at production quality (PH-1 step 5, part 5d)
- **Status:** Accepted · **Date:** 2026-10-03
- **Context:** Last part of PH-1 step 5 (DEC-055…057). The L2 host was the PH-0 sandbox spike (DEC-034: `ubos-runtime::sandbox`, binary `ubos-sandbox-spike`, 100 escape cases blocked), with the findings W-2 (memory bounded only per value), W-3 (unweighted units, no CPU clock), W-4 (quadratic size checks) and W-5 (Rhai's 96-bit decimal instead of the kernel decimal) and the work item W-12 (300 or more escape cases) assigned to this step by DEC-036. Logic assets of tier `l2` failed with `LOGIC.NOT_AVAILABLE`. The workspace forbids unsafe code, and `ubos-runtime` may depend on `ubos-expr` only (AR-031).
- **Decision:**
  1. **One gateway.** Scripts call instructions as `sys.<family>.<op>(#{…})`, `sys.uuid()` and `sys.random_bytes(n)`; every call goes through the gateway of step 5b (`program::instruction`: capabilities, profile families, ABI, identity parameters, parameter checks, base cost, availability) and the same budget, and is charged by the size of its arguments and result (W-3). `ctx` is a read-only constant of the execution context; `this` is a typed handle generated from the effective definition (`LOGIC.UNKNOWN_FIELD`, `LOGIC.TYPE_ERROR`), bound as the script's `this`; `params` and, for ports, `self_id` are constants. Host errors and limit terminations are not catchable by scripts; handles and the gateway cannot leave a script.
  2. **Worker thread.** A script runs on a dedicated worker thread of the calling thread, one per nesting level; the run state and the engine host stay with the caller, which serves the worker's gateway calls one at a time until the script returns. Only JSON crosses the threads, so the host needs no unsafe code and no shared mutable state; a nested script takes the next worker.
  3. **Engine.** Rhai starts without packages and receives the pure packages; `eval`, `print`, `debug`, closures, modules, floating point and custom syntax are off (STD-CTX-020, DSN-DVM-612); the hashing seed is fixed (STD-CTX-010); operation count, call depth, expression depth, string, array, map, variable and function sizes follow the profile (the PH-0 sandbox report's sizes, W-4 reviewed in part 5d-2); optimisation is off so that cached scripts carry no constants of a run; compiled scripts are cached per worker by profile and asset version hash. Float literals, float functions and `/` on decimals fail before they can lose precision.
  4. **Numbers (W-5).** The kernel decimal (`ubos-types`, STD-FND-022) and money are the script number types: `dec("1.25")`, `dec(3)`, `money("10.50", "USD")`, `+`, `-`, `*`, comparisons, `abs`, `round(x, scale, mode)`, `div(a, b, scale, mode)`, `amount`, `currency`; integers promote to decimals; `rust_decimal` and Rhai's `decimal` feature are gone. Temporal values enter scripts as ISO text; typed temporal handles follow with the L2 library of PH-1b.
  5. **Metering (W-2, W-3).** The progress callback counts operations against the units left in the run's budget and the cancellation point, checks the live-memory growth against the profile's memory size when a counting allocator is installed (`cap` in the host binaries, registered through `memory::install_probe`), and every eight operations the wall time and the thread CPU time (`cpu-time`) against the profile's clocks. One execution runs at a time in the adapter and the suites, so the process-wide count is the execution's; a multi-worker host accounts per worker process (step 7).
  6. **Conformance adapter.** `Engine::l2` is off in the conformance adapter, whose rc1 vectors assume the oracle's declaration that L2 execution is unavailable (VER-CONF-4206); `forge conform --l2` enables it. BPA 1.0-rc2 (PH-1b) tags that case as conditional on the declaration. The L2 host is verified by the escape suite of part 5d-2 (VER-SEC-0100, EXIT-1-10).
- **Consequences:** Part 5d-2: the suite grew to 382 cases (L2E-001…382; the PH-0 cases migrated to the production host), all blocked on two runs each (`verify/reports/exit-1-10/`, VER-SEC-0100); EXIT-1-10 is met. W-4 was measured: with the engine's size checks 65,536 pushes took 2.4 s; the checks are off when memory is accounted (the host binaries install the counting allocator) and `pad` is bounded per call by the profile size, after which the same growth takes 45.27 ms; W-2, W-3, W-5 and W-12 are closed by the report. NR-PERF-010 on the workstation: L1 guard 5.53 µs median (parse, check, evaluate) and 1.28 µs with the checked tree reused through the cache of `program::eval` (DSN-DVM-610), warm L2 invocation 661.7 µs median and 848.8 µs p95 through `execute`; the bytecode form of DSN-DVM-610 is not needed for NR-PERF-010 and is reconsidered against its own 1 µs mean on ENV-REF-SERVER at step 9. The function-pointer and reflection keywords (`Fn`, `call`, `curry`, `is_def_fn`, `is_def_var`, `is_shared`) are disabled in the engine; operation names that are reserved words of the script language (`super`, `async`) are also bound with a trailing underscore. The spike module and binary are removed; the CI job runs the suite with `ubos-l2-suite`. Conformance stays at 1,165 of 1,227 on both backends. DSN-DVM-613 (modules by pinned asset version) is met through `run.logic` over the asset's `imports`; Rhai's own `import` stays disabled. Step 5 is complete.
- **Alternatives considered:** Reaching the run state from engine callbacks through raw pointers was rejected: the workspace forbids unsafe code (DEC-033). A per-thread allocator wrapper for memory accounting was rejected for the same reason; the process-wide count of `cap` with one execution at a time is exact for the adapter and the suites. Keeping Rhai's decimal with a conversion at the boundary was rejected by DEC-036 (one decimal semantics, STD-FND-022).
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-603, DSN-DVM-610, DSN-DVM-612, DSN-DVM-613, STD-CTX-020, STD-CTX-004, STD-CTX-005, STD-FND-022, NR-PERF-010, NR-SEC-002, DEC-034, DEC-036, DEC-056, EXIT-1-10, W-2, W-3, W-4, W-5, W-12

### DEC-059 — Proofs: signed commit records, node key history and the pure verification primitives (PH-1 step 6, part 6a)
- **Status:** Accepted · **Date:** 2026-10-03
- **Context:** PH-1 step 6 (proofs, portable export and import, verifier library; EXIT-1-12) starts with the 62 vectors still open after step 5: the PROOF suite's signatures, inclusion and consistency proofs, tenant roots and archives, and five Core vectors of DID documents and key rotation. BPA 1.0-rc1 requires every commit record to carry at least one signature that verifies against a DID document at the record's `tx_time` (STD-PROOF-011), while the kernel of step 4 wrote unsigned records and the node key of step 5c (DEC-057) signed verification records only. The verifier of step 6c must check proofs without a kernel (DSN-DVM-931), and `ubos-version` cannot depend on `ubos-crypto` directly (AR-031).
- **Decision:**
  1. **Signed commit records.** The version layer signs every commit record, the genesis record included, with the node's current key (role `node`) over the commit ID at flush time, and stores the signature in the record's `signatures` list; the commit ID stays the content ID of the record without its signatures (STD-VER-001, DEC-053). The node record `{seed, n, keys}` moves from the DVM facade to the store (`ubos_store::node`, settings key `node`), where the version layer can reach it; the facade and the verification gate delegate to it. The key service of the node host (step 7) replaces the seed in the record.
  2. **Verification in `ubos-crypto`.** `verify(signature, content_id, did, at)` checks the DAT-Signature shape and the registered algorithm (`MODEL.INVALID_VALUE`), the signer against the document's subject, the key by `key_id`, its validity at `at` as the half-open interval `[valid_from, valid_to)`, and the Ed25519 signature over the UTF-8 bytes of the content ID (`PROOF.SIGNATURE_INVALID`). `ubos-crypto` re-exports `ubos-types` for crates whose allowed dependencies name only it.
  3. **Crate `ubos-proof`.** Depends on `ubos-tree` and `ubos-crypto` only and reads no store: `inclusion_proof(entries, key)` (the canonical nodes from the leaf to the root, STD-PROOF-020), `verify_inclusion(proof, root?, dids)` against a root, a signed commit record carried by the proof, or both, `commit_check(record, dids)` (recomputed commit ID and every signature verified at `tx_time`) and `verify_consistency(records, from?, to?, dids)` over a first-parent chain with strictly increasing transaction times (STD-PROOF-021). Archive verification (STD-PROOF-070) joins it in part 6b.
  4. **Key history.** `rotate_node_key(seed)` ends the current key and starts `#k<n+1>` at the clock reading after the latest transaction time of any branch head (`hlc`), so that every signed record stays verifiable at its own `tx_time`; `did_document` derives the document of a key-form DID and returns the node's document for its registered DID (`IAM.NOT_FOUND` otherwise, STD-ADDR-010). The `Store` trait gains `branch_names` (memory, group overlay and PostgreSQL) for the rotation and for the tenant root of part 6b.
  5. **Adapter operations.** `sign`, `public_key`, `verify_signature`, `inclusion_proof`, `verify_inclusion` and `verify_consistency` are pure and dispatch to `ubos-crypto` and `ubos-proof`; `did_document`, `rotate_node_key` and `commit_record` (the record in its portable form with parent commit IDs plus `commit_id`) read the store and join the stateful operations.
  6. **Store-reading proofs and the archive (part 6b).** `prove` (an object or entry of the selected commit, the proof carrying the signed commit record), `consistency_proof` (first-parent records from the later commit back to the earlier one, `IAM.NOT_FOUND` when they are reversed) and `tenant_root` (the root over the heads of the active main, release and overlay branches named `<vae>/<branch>`, signed by the node, STD-PROOF-030) read the store in `ubos-dvm`. `export_archive` and `import_archive` (STD-PROOF-070, format `ubos.archive.v1`) live in `ubos-version`: the export walks every commit reachable from a branch head and writes its state (objects with their timelines, ledgers), the version envelopes and entry records it references, the chunks, branches, tags and the node's DID document under a manifest signed by the node (role `signer`); the import runs the storage-free checks of `ubos-proof` (`verify_archive`: manifest signature, every content ID, commit signatures, parents, heads, counts), then loads the commits in parent order through the same flush as `write`, with the delta of each state against its first parent as the write set and the tree nodes from the parent's root, and rejects the archive when a recomputed state root or a head root differs. The stored version record now keeps `predecessor`, so that the envelope of STD-OBJ-002 can be rebuilt from it; the `Store` trait gains `tag_names`.
  7. **Verifier and round trip (part 6c).** The independent verifier `ubos-verify` (DEC-028) lives in `verify/ubos-verify/` under Apache-2.0 as a command line and a library over `ubos-proof` only: `inclusion`, `consistency`, `signature` and `archive`, each producing a structured report (DSN-DVM-938: every check with pass or fail, the registered error code and the reason naming the object); the archive checks of `ubos-proof` are exposed step by step for it. The round trip of EXIT-1-12 is the tool `ubos-roundtrip` of `ubos-dvm`: per VAE it builds a dataset on one VAE scope of a PostgreSQL database, exports it, verifies the archive with `ubos-verify`, imports it into a second scope, compares every branch's state and info, the tenant root, every tag, ledger and object read, re-exports and imports into a third scope and compares again; CI runs it with a small dataset on every push.
- **Consequences:** After part 6b all 1,227 vectors pass in memory and on PostgreSQL with identical outcomes (part 6a: 1,203). The storage suite stays at 20 of 20. Part 6c: EXIT-1-12 is met on the workstation (`verify/reports/exit-1-12/`: 3 VAEs of 206 objects, 300 entries, 20 commits, 5 branches and 3 tags each, identical after import and after re-import, the second export byte-identical to the first, every archive verified); the ENV-REF-SERVER run of VER-CONF-8403 with 20 VAEs follows at step 9. Step 6 is complete. Every commit written by the kernel from this part on is node-signed; records written before it (none outside tests and conformance runs, which start from `reset`) would fail `commit_check`. DSN-DVM-937 (node co-signature) is brought forward to PH-1 as the single signature of the conformance adapter's records; author signatures and the separate signature table of DSN-DVM-522 keep their phases. 
- **Alternatives considered:** A signing callback passed from the DVM into `Repo::write` was rejected: every caller of the version layer would have to supply it, and the node record is store state anyway. Keeping the DAT-Signature checks in the DVM facade was rejected because the verifier of part 6c must share them without a kernel (DSN-DVM-931). Deferring commit signatures to the node host of step 7 was rejected because STD-PROOF-011 and the PROOF vectors need them now; the host replaces only the key source.
- **Affects:** UBS-SYS-DVM-04, DSN-DVM-931, DSN-DVM-933, DSN-DVM-934, DSN-DVM-937, DSN-DVM-950, DSN-DVM-951, DSN-DVM-522, STD-PROOF-010, STD-PROOF-011, STD-PROOF-020, STD-PROOF-021, STD-PROOF-030, STD-PROOF-070, STD-ADDR-010, AR-031, DEC-053, DEC-057, EXIT-1-12

### DEC-060 — In-process host and host services on PostgreSQL (PH-1 step 7, part 7a)
- **Status:** Accepted · **Date:** 2026-10-03
- **Context:** PH-1 step 7 (EXIT-1-02, EXIT-1-13) brings the in-process host and the CTR-002 host services on PostgreSQL for gate 1a, Forge `check`, `types` and `test`, and the ten gate-1a scenarios (UBS-PH-1 §2, §3). The kernel of steps 3–6 kept everything a host owns in local stand-ins: the local key provider and blob directory (DEC-047), the node record with a seed in the settings (DEC-059), a deterministic process seed, no connectors or models (`LOGIC.NOT_AVAILABLE`), effects left in the store outbox for the adapter's `outbox` and `ack`. AR-031 lets `ubos-node` depend on `ubos-dvm` (and `ubos-proto`) only, and `ubos-version` sign at flush through the store (DEC-059).
- **Decision:**
  1. **Host boundary in `ubos-dvm`.** `ubos_dvm::host` defines the CTR-002 services as traits — `Clock`, `Entropy`, `Secrets`, `Connectors`, `Models`, `OutboxSink`, `Jobs`, `Telemetry`, with `BlobStore` and `KeyService` of the engine crates — bundled as `Services`, and the host API of CTR-001 as `Vae`: `begin_process`, `invoke_port`, `read`, `query`, `commit`, `merge`, `preview`, `replay`, `export`, `import`, `verify`, `explain`, `reflect` over the engine, reading the clock and firing due timers before every operation, releasing the outbox to the host after it, and emitting one telemetry event per operation. The engine carries the services: a process is seeded from `entropy.fill` and the seed is recorded with the process, so a replay needs no host; `call.connector` and `ask.model` run through the services and are journaled before use (`program::journaled`), so a replay serves the recorded result and never calls the host (SCN-011 A1); without services the instructions stay `LOGIC.NOT_AVAILABLE` and the conformance adapter is unchanged. Deterministic services (`FixedClock`, `SeededEntropy`, a recorder) serve tests (DSN-FRG-202). The engine persists the class declarations with the VAE (`genesis`) and reopens an existing VAE (`open_existing`); hosts always learn the process identifier of an invocation.
  2. **Crate `ubos-node`.** The PostgreSQL implementations (host layer, UBS-ARC-07 §2): the tenant registry (`tenant`, `vae`; DSN-NOD-401, -402) read as the owner role; the key hierarchy root KEK → tenant KEK → derived keys (DSN-NOD-131): the tenant KEK is stored wrapped by the root KEK (XChaCha20-Poly1305, root from `UBOS_ROOT_KEK` or a file, the development key otherwise), data keys and node signing keys are derived with HKDF-SHA-256 and never stored, data key generations rotate (FR-IAM-033); the node signer (DSN-NOD-134) installed on the store, so the version layer signs commit records with the VAE's registered DID `did:ubos:<authority>:<vae id>` and its public key history in `node_key`; the blob table (DSN-NOD-320, -321), secrets sealed under the current data key and resolved only inside connector execution (DSN-NOD-130), connectors registered in the node as the allow-list (DSN-NOD-301, -302), the event log receiving released effects (DSN-NOD-201, -202), jobs with leases, retries and poison (DSN-NOD-220, -221, -223), telemetry redacted to operational members (DSN-NOD-610, -611); migration `0006_host.sql` with row-level security on the tenant tables. Models stay `AI.PROVIDER_UNAVAILABLE` until PH-3.
  3. **Suite.** `ubos-node-suite` (SUITE-NOD-HOST, `verify/reports/suite-nod-host/`) exercises every service, tenant isolation of blobs and secrets, signed records through the key hierarchy, key rotation, reopening, journaled connector calls replayed without the host, and runs in CI.
  4. **Gate 1b** builds the Server on these services: UBTP, sessions and local credentials, schedules, OpenTelemetry export, health, backup and PITR.
- **Consequences:** Conformance stays at 1,227 of 1,227 on both backends (the adapter has no services); the storage suite 20/20 and the escape suite 382/382 are unchanged. The metering record counts connector calls. A host program may now call connectors and models when a host registers them. The table of UBS-SYS-NOD gains the in-process host of gate 1a. Parts 7b (Forge `check`, `types`, `test` with the deterministic services) and 7c (the ten scenarios, EXIT-1-13 and the final EXIT-1-02 report) follow.
- **Alternatives considered:** Running the host services from a separate process over UBTP was rejected for gate 1a: the plan verifies the scenarios through the in-process binding, and the Server of gate 1b adds the transport. Storing the node signing seed in the settings as before was rejected: the key hierarchy keeps every private key derived in memory. A separate migration set for the host tables was rejected because sqlx runs one migrator per database; the tables live in the kernel migrations with the host as their only writer.
- **Affects:** UBS-SYS-DVM, UBS-SYS-NOD, CTR-001, CTR-002, DSN-NOD-130, DSN-NOD-131, DSN-NOD-134, DSN-NOD-201, DSN-NOD-202, DSN-NOD-220, DSN-NOD-221, DSN-NOD-223, DSN-NOD-301, DSN-NOD-302, DSN-NOD-320, DSN-NOD-401, DSN-NOD-402, DSN-NOD-602, DSN-NOD-610, DSN-NOD-611, DSN-FRG-202, STD-CTX-011, STD-PROOF-010, DEC-047, DEC-059, EXIT-1-02, EXIT-1-13
