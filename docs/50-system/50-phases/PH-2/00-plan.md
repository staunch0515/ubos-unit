---
id: UBS-PH-2
title: PH-2 — First Business Package (Basic Finance) and Production Server
status: draft
phase: PH-2
depends_on: [UBS-PH-00, UBS-PH-1]
---

# PH-2 — First Business Package (Basic Finance) and Production Server

## 1. Goal and business value

PH-2 delivers the first complete product (DEC-016 as revised by DEC-023): a highly available
Server with a professional Web Workspace, a lean Studio and Bridge, and the first business
package — the **Basic Finance** Buk, a US-standard general-ledger, receivables, payables, bank,
fixed-asset, multi-currency and close system for one company (DEC-022). The product is not
sold; it exists to demonstrate what the platform imposes on every business package
(versioned history, maker–checker, "as known on" queries, simulation, evidence, explanations)
and to be compared with the same requirements implemented in the owner's Java system
(DEC-026). It delivers business goal **G2**: the finance package runs with zero kernel code
specific to it. It also completes the governance kernel (lenses, decision tables, approvals,
workflows, sagas, ReBAC, masking, crypto-shredding, simulation, periods and close, matching)
and the production edge (REST, webhooks, SSO, SCIM, notifications, files, search).

Measurable outcomes:
1. The Basic Finance Buk passes SCN-501…510 on an HA Server, and the financial statements
   of the sample company equal the independently computed expected results (MET-BIZ-003),
   with 0 lines of finance-specific kernel code (MET-BIZ-001).
2. BPA 1.0 final is released with vectors for all PH-2 kernel features, on PostgreSQL
   (MET-QUAL-020, DEC-024).
3. Usability with accountants: ≥ 90% unassisted task success and SUS ≥ 75 (NR-USE-001,
   NR-USE-002), and keyboard-first journal entry in the Smart Grid (FR-OFFICE-041…046).
4. HA and DR: node loss causes no failed requests beyond retries; RPO/RTO met (NR-AVAIL-002,
   NR-DUR-002, NR-DUR-003).
5. The sample company's opening balances and one year of history are migrated from
   spreadsheet and CSV extracts with a reconciliation report showing 0 unexplained
   differences (MET-BIZ-002).
6. Development-effort and defect data are recorded under the comparison protocol
   (MET-BIZ-004, DEC-026).

## 2. Systems and components in scope

| System | Components | Depth |
|---|---|---|
| BPA | 1.0 final; errata process; extension registry for first-party Buks | hardened |
| DVM | lenses and schema evolution, decision tables, approvals and deferred commits, workflows, sagas, ReBAC, masking, crypto-shredding structures, simulation, projections at scale, impact and restatement, matching engine, periods and close — on the PostgreSQL adapter only | extended |
| NOD | HA Server, REST facade, webhooks, brokers, OIDC, SAML, SCIM, step-up, KMS adapters, notifications and action messages, files (scanning, previews, extraction), text search, DR, rolling upgrades, bank file formats (BAI2, camt.053, MT940 in; NACHA, pain.001, positive pay out) | extended |
| FRG | Buk pack, sign, verify, dependency solver, simulator, replay debugger, LSP, VS Code extension, docs, Forge service | extended |
| SDK | optimistic UI support, skew suite | extended |
| WSP | Web-only production workspace (DEC-025): renderer, catalogue 1.0, shell, forms, lists, details, timeline and as-of, diff review, tasks, explanations, Smart Grid (DSN-WSP-401), keyboard-first entry, registers, report viewer with drill-down, close cockpit, financial number presentation (DSN-WSP-116…120), deterministic rendering, localisation, accessibility | new |
| STU | lean: model, sheet, decision-table, lifecycle and view editors; change sets; simulation; releases; import wizard | new (lean) |
| BRG | spreadsheet and CSV import, bulk loads with history, mappings, reconciliation, bank statement and payment adapters, mail adapter | new |
| NTY | evidence packs in NOD, verifier CLI | prototype |
| Buks | `finance` (chart of accounts, journals, periods and close, receivables, payables with 1099, bank reconciliation, fixed assets, multi-currency under ASC 830, basic state sales tax, financial statements, audit trail and controls) | new |

Not in scope: SQLite, Box, desktop and mobile apps (PH-4, DEC-024, DEC-025); the deferred
fund-operations and contract packages (DEC-022).

## 3. Feature list

The generated list is UBS-PHF-2: 265 FR, 52 NR, 15 CR and 17 scenarios. Design coverage: all
DSN items with **Phase** PH-2 across DVM, NOD, FRG, SDK, WSP, STU and BRG.

The Basic Finance Buk is specified by the scenario family SCN-501…510 (PH-2 part of
UBS-REQ-04-06) and by the implementation-neutral finance requirements
(`docs/finance-requirements/`, DEC-026). It is built only from kernel features; any kernel gap
found while building it is raised as a standard change (DSN-BPA-010), never as
finance-specific kernel code. The generic scenarios SCN-012…016 use finance examples.

## 4. Dependencies

| Dependency | Kind | Needed by |
|---|---|---|
| EXIT-1-01…20 | prior exit | start |
| finance requirements document and sample-company dataset with expected results (DEC-026, ASM-012) | internal | month 1 |
| accounting reviewer (US CPA or equivalent) to check the expected results and the chart of accounts | external | month 2 |
| HA reference environment ENV-REF-SERVER with standby site | external | month 3 |
| accessibility auditor and usability-study participants (accountants, controllers, clerks; ASM-013) | external | month 6 |
| penetration-test vendor | external | month 8 |

## 5. Out of scope for this phase

- SQLite adapter, Box, desktop and mobile apps, offline sync (PH-4, DEC-024, DEC-025).
- External portal, War Room, negotiation rounds, contract-only capabilities (PH-5, DEC-022).
- Fund-operations and contract-management packages (deferred, DEC-022).
- AI of any kind (`ask.model` returns `LOGIC.NOT_AVAILABLE`), vector search (PH-3).
- L3 WASM components (PH-3).
- Anchoring service, legal holds, WORM retention jobs (PH-3).
- Payroll, inventory, consolidation, budgeting, project accounting, complex ASC 606 and
  ASC 842 (outside the finance scope, DEC-022).
- Multi-tenant Cells, Control Plane, public Exchange, workers (PH-4).

## 6. Deliverables

| Deliverable | Form |
|---|---|
| BPA 1.0 | signed release; conformance package |
| UBOS Server 1.0 | signed binaries, OCI images, Helm chart, installation and operations guides, runbooks |
| Workspace and Studio 1.0 | Web clients served by the Server |
| Forge 1.0 and VS Code extension | signed binaries, marketplace listing |
| SDKs 1.0 (Rust, TypeScript) | crates.io and npm packages |
| `finance` Buk 1.0 | signed, verified Buk archive with its chart-of-accounts template and report definitions |
| Sample company | Northwind Components, Inc. books: opening balances, one year of transactions, expected statements |
| Migration toolkit (BRG) | import wizard, mappings, reconciliation reports |
| Comparison data set | effort, size, defect and change-lead-time records under the DEC-026 protocol |
| Evidence | scenario, accounting-correctness, performance, security, accessibility, usability and DR reports |

## 7. Verification plan

### 7.1 Methods used and why

| Method | Why in PH-2 |
|---|---|
| CONF | new kernel features become standard (1.0 final) |
| PROP, SIM, FORM | lenses, merge with schemas, approvals, sagas and period close are new correctness risks |
| FAULT | HA failover, split brain, DR, rolling upgrades |
| SCN | the phase goal is defined by the finance scenario family |
| BENCH | throughput and scale targets (NR-PERF-003, NR-SCAL-*) enter in PH-2, with WL-FIN-M |
| SEC | first externally exposed product: penetration test, SSO, SCIM, masking, erasure |
| USE | first business-user product: usability with accountants and accessibility targets |
| INSP | accounting review of expected results, accessibility conformance report, SBOM and provenance, finance-purity inspection |

### 7.2 Suites and verification ranges

| Scope | Suite | VER range |
|---|---|---|
| standard vectors for lenses, decision tables, approvals, deferred commits, sagas, ReBAC, masking (new clauses of 1.0 in their areas) | SUITE-DVM-CONF | VER-CONF-0001…6299 (all 1.0 areas) |
| new kernel properties (lens application, row filters, approvals) | SUITE-DVM-PROP | VER-PROP-0400…0599 |
| approvals and sagas models | SUITE-DVM-FORM | VER-FORM-0100…0199 |
| HA, failover, split brain, upgrades, DR | SUITE-NOD-OPS | VER-FAULT-2100…2299 |
| REST, webhooks, brokers, SSO, SCIM | SUITE-NOD-PROTO, SUITE-NOD-AUTH, SUITE-NOD-EVENTS | VER-CONF-7100…7299, VER-SEC-2200…2299 |
| files and search | SUITE-NOD-FILES | VER-CONF-7300…7349, VER-SEC-2300…2349 |
| penetration test | SUITE-NOD-PEN | VER-SEC-2900…2999 |
| Buk tooling | SUITE-FRG-REPRO, SUITE-FRG-RESOLVE, SUITE-FRG-DX | VER-CONF-7800…7899, VER-PROP-4000…4049, VER-BENCH-4000…4049 |
| workspace end to end | SUITE-WSP-E2E | VER-SCN-2500…2699 |
| accessibility, rendering, performance, security | SUITE-WSP-A11Y, SUITE-WSP-RENDER, SUITE-WSP-PERF, SUITE-WSP-SEC | VER-INSP-5000…5099, VER-CONF-9000…9049, VER-BENCH-5000…5049, VER-SEC-5000…5049 |
| usability studies | SUITE-WSP-USE, SUITE-STU-USE | VER-USE-2000…2049 |
| migration | SUITE-BRG-MIG | VER-SCN-2700…2749 |
| finance scenario family, statements against expected results | SUITE-PH2-FINANCE | VER-SCN-2000…2199 |
| edge and kernel benchmarks at scale | SUITE-NOD-BENCH, SUITE-DVM-BENCH | VER-BENCH-2100…2199, VER-BENCH-0200…0299 |

VER numbering follows the allocation of the verification volume (UBS-VER-00 §3).

### 7.3 Environments and datasets

| Use | Environment |
|---|---|
| HA, failover, DR, scale benchmarks | ENV-REF-SERVER (with standby site) |
| installation and small-deployment profile | ENV-REF-SMALL |
| client-perceived latency and usability sessions | ENV-REF-WAN |

Datasets: the Northwind sample company with expected results (UBS-VER-03), WL-FIN-M (finance
workload), WL-GENERIC-10M scaled to NR-SCAL-001 PH-2 size (50,000,000 current objects), bank
statement and payment file fixtures, and the rendering vector set.

### 7.4 Pass thresholds

| Check | Threshold |
|---|---|
| BPA 1.0 vectors | 100% pass on PostgreSQL, including all PH-1 vectors |
| finance scenarios | 100% of SCN-501…510 pass on the HA Server |
| accounting correctness | trial balance, balance sheet, income statement, cash-flow statement, statement of equity, AR and AP agings and the 1099 summary equal the expected results to the cent; debits equal credits in every period |
| Buk purity | 0 kernel commits referencing finance names or Buk-specific logic (automated scan plus inspection) |
| throughput | NR-PERF-003 met for 30 minutes |
| scale | NR-SCAL-001 (PH-2 target), NR-SCAL-003, NR-SCAL-004, NR-SCAL-005 met |
| availability | NR-AVAIL-002, NR-AVAIL-003 and NR-AVAIL-006 met in the fault matrix |
| durability | NR-DUR-002 and NR-DUR-003 met in the DR drill; NR-DUR-005 audit running |
| security | NR-SEC-001: 0 open critical or high findings; NR-SEC-005, -006, -007 (SLSA L2), -011 met |
| privacy | NR-PRIV-001, -002, -005 met in the privacy scenario (SCN-016) |
| usability | NR-USE-001 ≥ 90%; NR-USE-002 SUS ≥ 75 (accountants) and ≥ 70 (builders); NR-USE-004 ≤ 1 working day; NR-USE-006 and NR-USE-007 met |
| accessibility | NR-ACC-001 and NR-ACC-002 met; Accessibility Conformance Report published |
| operations | NR-OPER-001, -002, -004 met |
| determinism of rendering | NR-DET-004 met |

### 7.5 Acceptance

The Phase Acceptance Board accepts PH-2. The accounting reviewer signs the expected results
and the accounting-correctness report. The owner signs the comparison data set for PH-2.
Records are DEC entries with evidence hashes.

### 7.6 Regression policy

All PH-1 suites and scenarios re-run on every PH-2 release candidate; kernel minors must
pass the finance Buk's scenarios before release (NR-COMPAT-004).

## 8. Metrics

| Metric | Name | Target |
|---|---|---|
| MET-BIZ-001 | finance-specific kernel code | 0 lines |
| MET-BIZ-002 | sample-company migration reconciliation differences unexplained | 0 |
| MET-BIZ-003 | financial-statement lines differing from the expected results | 0 |
| MET-BIZ-004 | comparison-protocol records captured for PH-2 work items (DEC-026) | 100% |
| MET-QUAL-020 | BPA 1.0 vectors passing on PostgreSQL | 100% |
| MET-QUAL-021 | finance scenarios passing on HA Server | 100% |
| MET-PERF-020 | sustained simple commits/s (NR-PERF-003) | ≥ 2,000 |
| MET-PERF-021 | sustained ledger entries/s (NR-PERF-003) | ≥ 5,000 |
| MET-PERF-022 | detail view server time p95 (NR-PERF-014) | ≤ 150 ms |
| MET-PERF-023 | commit to client update p95 on WAN (NR-PERF-015) | ≤ 1 s |
| MET-OPS-020 | write unavailability on primary loss (NR-AVAIL-002) | ≤ 60 s |
| MET-OPS-021 | DR RTO (NR-DUR-003) | ≤ 30 min |
| MET-USE-001 | unassisted task success, accountants (NR-USE-001) | ≥ 90% |
| MET-USE-002 | SUS accountants / builders (NR-USE-002) | ≥ 75 / ≥ 70 |
| MET-USE-003 | new class with lifecycle, 3 rules and views released by a trained architect (NR-USE-004) | ≤ 1 working day |
| MET-USE-004 | 50-line journal entered in the Smart Grid by keyboard only, median | ≤ 5 min |
| MET-SEC-020 | open critical or high penetration-test findings | 0 |

## 9. Exit criteria

| Exit | Criterion | Evidence |
|---|---|---|
| EXIT-2-01 | BPA 1.0 is released; 100% of its vectors pass on PostgreSQL (DEC-024) | release manifest, runner reports |
| EXIT-2-02 | New formal models (approvals, sagas) checked with 0 violations | model reports |
| EXIT-2-03 | **Withdrawn** (DEC-022): fund and contract Buks deferred; replaced by EXIT-2-16 | — |
| EXIT-2-04 | 0 lines of finance-specific kernel code (MET-BIZ-001) | code scan and inspection record |
| EXIT-2-05 | NR-PERF-003, -013, -014, -015, -016, -017 met | benchmark report |
| EXIT-2-06 | NR-SCAL-001 (PH-2), -003, -004, -005 met | scale report |
| EXIT-2-07 | HA fault matrix, split-brain test and rolling upgrade pass; DR drill meets RPO and RTO | fault and DR reports |
| EXIT-2-08 | Penetration test with 0 open critical or high findings; SBOM and signed artifacts with SLSA L2 provenance | pen-test report, supply-chain report |
| EXIT-2-09 | Privacy scenario SCN-016 passes with NR-PRIV-001, -002 and -005 met | scenario and privacy report |
| EXIT-2-10 | Usability study with accountants meets MET-USE-001…004 | study report |
| EXIT-2-11 | WCAG 2.2 AA audit passed; Accessibility Conformance Report published | audit report, ACR |
| EXIT-2-12 | **Withdrawn** (DEC-022): E-SIGN/UETA ceremony moves to PH-5 (EXIT-5-09) | — |
| EXIT-2-13 | **Withdrawn** (DEC-023): no design partner; replaced by EXIT-2-17 | — |
| EXIT-2-14 | Installation (≤ 30 min) and rolling upgrade (≤ 60 min operator time) meet NR-OPER-001 and NR-OPER-002; all alerts link to runbooks | operations report |
| EXIT-2-15 | Traceability report shows every PH-2 Must requirement covered by passing verification items, and all PH-1 suites still pass | traceability and regression reports |
| EXIT-2-16 | The `finance` Buk is verified by Forge; SCN-501…510 pass on the HA Server; the sample company's statements equal the expected results signed by the accounting reviewer (MET-BIZ-003) | Buk verification record, scenario report, accounting-correctness report |
| EXIT-2-17 | The sample company's opening balances and one year of history are migrated from spreadsheet and CSV extracts with 0 unexplained reconciliation differences (MET-BIZ-002) | reconciliation report |
| EXIT-2-18 | The finance workbench (Smart Grid, registers, report drill-down, close cockpit) is accepted by accountants in the usability study: MET-USE-004 met and no open severity-1 usability finding | study report |
| EXIT-2-19 | The PH-2 comparison data set is complete under the DEC-026 protocol (MET-BIZ-004) and signed by the owner | comparison data set |

## 10. Risks and mitigations

| Risk | Description | L | I | Mitigation | Early warning | Owner |
|---|---|---|---|---|---|---|
| RSK-020 | Finance package needs a kernel feature not in the standard | H | M | fast standard change process with vectors; kernel team embedded with Buk team | Buk backlog items tagged "kernel gap" > 5 open | product owner |
| RSK-021 | Accountants find branching concepts confusing (ASM-007) | M | H | progressive disclosure, accounting vocabulary, early usability tests at month 3 and 6 | SUS < 70 in the month-3 test | design lead |
| RSK-022 | **Withdrawn** (DEC-023): no design partner in PH-2 | — | — | — | — | — |
| RSK-023 | Lens and schema-merge complexity causes defects | M | H | property tests with generated schemas; FORM for schema alignment | property failures in nightly runs | kernel lead |
| RSK-024 | Accessibility retrofitting late | M | M | accessible widget catalogue from the start; axe checks in CI | axe violations trending up | design lead |
| RSK-025 | **Withdrawn** (DEC-022): E-SIGN ceremony moves to PH-5 | — | — | — | — | — |
| RSK-026 | Accounting errors in the Buk (rounding, currency revaluation, period cut-off, 1099 thresholds) go unnoticed because the team lacks accounting expertise | M | H | expected results computed independently and signed by the accounting reviewer; property tests that debits equal credits; statement tie-outs in every scenario | any statement difference from the expected results | Buk lead |
| RSK-027 | Comparison with the Java system is not like for like (different scope, team or effort recording) | M | M | one requirement document, one sample company, one recording protocol (DEC-026); scope differences logged per requirement | comparison records missing for > 10% of work items | product owner |

## 11. Reference duration and team assumption

Non-normative. 9 months. Team of 12–14: 4 kernel, 3 node/platform, 3 front-end (WSP, STU),
2 Buk developers with an accounting analyst, 1 Forge, 1 quality, 1 security, plus design, the
standards steward, the engineering lead and the product owner. The growth beyond the 8–12
core follows ASM-001.

**Execution (DEC-085):** as in PH-1, the phase is executed by the product owner with AI
sessions, one pull request per step part, merged by the owner; `main` is the development line
of PH-2 and PH-1 is kept on the frozen line `release/ph1`. Standard before code (AR-033): a
step that needs a kernel capability without clauses starts with the clauses and vectors of a
BPA 1.0 release candidate. The figures above remain a reference only. Steps:

| Step | Content | Exits |
|---|---|---|
| 0 | alignment: DEC-085, this section, status tracking (`docs/reports/ph2-plan-zh.md`) | — |

Gate 2a — the standard, the kernel and the finance core on a single node:

| Step | Content | Exits |
|---|---|---|
| 1 | BPA 1.0 release candidates for the PH-2 kernel: schema evolution and lenses, decision tables (STD-LGS-040), approvals and deferred commits, workflows and sagas, relationship authorization, masking and crypto-shredding, simulation, periods and close, matching, the REST facade (STD-PROTO-021); standard before code, split by clause area | EXIT-2-01 (candidates) |
| 2 | extended formal models: approvals, sagas | EXIT-2-02 |
| 3 | money and periods: currencies and rates, business calendars, double entry, period close, retroactive correction and restatement, reconciliation and matching (CAP-LOC-02, -04, CAP-LEDG-05, -06, -07, CAP-TIME-04, -06) | — |
| 4 | approvals and work: deferred commits for `require` decisions, approvals and segregation of duties, tasks and work queues, SLAs and escalation, delegation (CAP-FLOW-04, -05, -08, -09) | — |
| 5 | decision tables and rule operations: decision tables, attach and detach with effective periods, simulation and impact analysis, rule testing, the simulator (CAP-RULE-04, -06, -07, -08, CAP-DEV-04) | — |
| 6 | durable work: workflows and sagas, long-running processes, post-commit effects, replay and backfill (CAP-FLOW-06, -07, CAP-TXN-05, -06, CAP-EVT-07) | — |
| 7 | models and packages: schema evolution and lenses, base ontology, model export; the Buk format, dependencies, installation, upgrade, removal, verification and signing (CAP-MODEL-08, -09, -10, CAP-PKG-01…07) | — |
| 8 | protection: relationship authorization, masking and filtering, encrypted fields and crypto-shredding, access logging (CAP-IAM-05, -06, CAP-PROOF-07, CAP-AUD-05) | — |
| 9 | `finance` Buk, part 1: chart of accounts, journals, periods and close, the general ledger and the financial statements of the sample company, tied out to the expected results in process | EXIT-2-04 (inspection) |
| 10 | `finance` Buk, part 2: receivables, payables with 1099, bank reconciliation, fixed assets, multi-currency (ASC 830), basic state sales tax; SCN-501…510 in process; review by the accounting reviewer | EXIT-2-16 (in process) |

Gate 2b — the production edge, the clients and acceptance:

| Step | Content | Exits |
|---|---|---|
| 11 | HA Server and disaster recovery: replication and failover, split-brain protection, rolling upgrades, the VAE hierarchy and tenant lifecycle and portability (CAP-OPS-03, -06, CAP-TEN-04, -05, -06) | EXIT-2-07, EXIT-2-14 |
| 12 | integration and identity edge: REST facade, webhooks, connector framework, API credentials, bank file formats, mail; OIDC, SAML, SCIM, step-up, sessions and devices, KMS adapters (CAP-INT-02, -03, -04, -06, -08, CAP-LOGIC-10, CAP-IAM-02, -07, -09, -10) | — |
| 13 | notifications, files and search: notification rules, channels and digests; content safety, previews and extraction, VFS paths; full-text search, saved queries, aggregation, reports (CAP-NTF-01…03, CAP-FILE-02…04, CAP-QRY-02, -04, -06, CAP-ANL-03) | — |
| 14 | Web workspace, part 1 (DEC-081): renderer and catalogue, shell, forms, lists and details, progressive disclosure, history and as-of, change review, explanations (CAP-UX-01…06); an early usability check with accountants (RSK-021) | — |
| 15 | Web workspace, part 2: Smart Grid and keyboard-first entry, registers, report viewer with drill-down, close cockpit, rendering and export, localisation and formats, accessibility (CAP-OFFICE-04, -08, CAP-LOC-01, -05) | — |
| 16 | Studio (lean) and Forge 1.0: model, sheet, decision-table, lifecycle and view editors, change sets, simulation, releases; replay debugger, LSP, VS Code extension, documentation (CAP-DEV-01, -07, -08, -09) | — |
| 17 | Bridge: spreadsheet and CSV import, bulk loads with history, mappings, reconciliation; the sample company migrated (CAP-MIG-01, -03, -04, -05) | EXIT-2-17 |
| 18 | finance on the HA Server: SCN-501…510 and SCN-012…016 through the Workspace and UBTP; the accounting-correctness report signed by the reviewer; benchmarks and scale | EXIT-2-04, -05, -06, -16 |
| 19 | external verification and release: penetration test, SBOM and SLSA provenance, privacy scenario SCN-016, WCAG 2.2 AA audit, usability study; BPA 1.0 final | EXIT-2-01, -08, -09, -10, -11, -18 |
| 20 | PH-2 acceptance: traceability and PH-1 regression, the comparison data set (DEC-026), acceptance decision | EXIT-2-15, EXIT-2-19, all |

