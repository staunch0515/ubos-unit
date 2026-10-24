---
id: UBS-VER-12
title: Verification Items — PH-2
status: draft
phase: PH-2
depends_on: [UBS-VER-01, UBS-PH-2]
---

# Verification Items — PH-2

### VER-CONF-8405 — BPA 1.0 conformance on PostgreSQL
- **Verifies:** all STD clauses of BPA 1.0 claimed profiles; FR-STD-011, FR-STD-041
- **Suite:** SUITE-DVM-CONF · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Run VER-CONF-0001…6299 (claimed profiles) on PostgreSQL, including all PH-1 vectors (SQLite from PH-4, DEC-024).
  2. Run the previous release's vectors to confirm compatibility (DSN-BPA-006).
- **Pass criterion:** 100% pass; 0 regressions of rc vectors.
- **Evidence:** signed runner reports, release manifest.
- **Supports:** EXIT-2-01

### VER-INSP-9006 — BPA 1.0-rc8 coverage
- **Verifies:** FR-STD-022, DSN-BPA-003
- **Suite:** SUITE-BPA-COVERAGE · **Phase:** PH-2
- **Environment:** release pipeline
- **Procedure:**
  1. Compute MUST-clause coverage per profile for the rc8 profiles (Core, Bitemporal, Ledger, Proof, Governance, Protocol), excluding the superseded vectors (STD-CONF-030).
- **Pass criterion:** 100% coverage.
- **Evidence:** coverage report and release manifest `standard/release/1.0-rc8/` (PH-2 step 1h, DEC-096: 200 of 200 MUST clauses, 2,053 vectors); tag `bpa-v1.0.0-rc8`.
- **Supports:** EXIT-2-01

### VER-FORM-0100 — Approvals and deferred commits model
- **Verifies:** STD-TXN-060, STD-VER-021, STD-LGS-041, STD-LGS-042, DSN-DVM-550, DSN-DVM-551, DSN-DVM-746, FR-FLOW-043, FR-FLOW-045
- **Suite:** SUITE-DVM-FORM · **Phase:** PH-2
- **Environment:** model-checking workstation
- **Procedure:**
  1. Model pending changes, approvals with preview hashes, concurrent edits, SoD constraints.
  2. Check: no commit without valid approvals; stale approvals never count; SoD always holds; no lost update; a proposal is applied at most once.
  3. Bound (DEC-031, DEC-097): 2 approvers × 3 editors × 3 objects, 2 proposals, object versions up to 2, one source commit per change set; model `verify/models/approvals.tla`.
- **Pass criterion:** 0 violations at 2 approvers × 3 editors × 3 objects.
- **Evidence:** model and checker output (`verify/models/reports/README.md` §6).
- **Supports:** EXIT-2-02

### VER-FORM-0101 — Saga model
- **Verifies:** STD-TXN-080, DSN-DVM-564, DSN-DVM-566, FR-TXN-061, FR-TXN-063
- **Suite:** SUITE-DVM-FORM · **Phase:** PH-2
- **Environment:** model-checking workstation
- **Procedure:**
  1. Model a 3-step saga with timeouts and coordinator crashes.
  2. Check: every started saga ends completed, compensated or failed, and fails only when a compensation fails or forward recovery is exhausted; under backward recovery with compensations that do not fail it ends completed or compensated; steps and compensations run at most once per step, compensations in reverse order (DEC-097).
  3. Bound: 3 steps, 2 attempts per step, 2 coordinator crashes, both recovery modes; model `verify/models/saga.tla`.
- **Pass criterion:** 0 violations.
- **Evidence:** model and checker output (`verify/models/reports/README.md` §6).
- **Supports:** EXIT-2-02

### VER-SCN-2000 — Basic Finance scenario family and accounting correctness
- **Verifies:** SCN-501, SCN-502, SCN-503, SCN-504, SCN-505, SCN-506, SCN-507, SCN-508, SCN-509, SCN-510, SCN-012, MET-BIZ-003; FR-LEDG-011, FR-LEDG-013, FR-LEDG-021, FR-LEDG-031, FR-LEDG-041, FR-LEDG-051, FR-LEDG-052, FR-LEDG-053, FR-LEDG-061, FR-LEDG-062, FR-LEDG-063, FR-LEDG-071, FR-LEDG-072, FR-LEDG-073, FR-LEDG-074, FR-TIME-021, FR-TIME-031, FR-TIME-034, FR-TIME-041, FR-TIME-042, FR-RULE-071, FR-RULE-072, FR-RULE-073, FR-FLOW-043, FR-FLOW-044, FR-FLOW-045, FR-ANL-031, FR-ANL-032
- **Suite:** SUITE-PH2-FINANCE · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER (HA) with the Northwind sample company and WL-FIN-M
- **Procedure:**
  1. Install the Forge-verified `finance` Buk through a change set.
  2. Run each scenario's main and alternate flows via SDK scripts and Playwright through the Web Workspace.
  3. After SCN-506 export the trial balance, balance sheet, income statement, cash-flow statement, statement of equity, AR and AP agings and the 1099 summary, and compare them line by line with the expected results signed by the accounting reviewer.
  4. For SCN-508 and SCN-509 compare the "as known on" and simulated statements with the independently computed values.
  5. For SCN-510 compare revaluation gains and losses with the reference workbook.
  6. Assert in every period that total debits equal total credits.
- **Pass criterion:** all steps pass; every statement line equals the expected result to the cent (MET-BIZ-003 = 0).
- **Evidence:** scenario report with trace IDs; accounting-correctness report; reference workbook hash; Buk verification record.
- **Supports:** EXIT-2-16

### VER-SCN-2100 — Contract scenario family
- **Status:** Deferred (DEC-022). Kept with its number for the contract package; it verifies nothing until a decision schedules that package.
- **Suite:** SUITE-PH2-CONTRACTS · **Phase:** deferred
- **Procedure:**
  1. Install the verified `contracts` Buk; run SCN-201…208; verify executed PDFs with the independent verifier offline.
- **Pass criterion:** all steps pass; independent verification succeeds.
- **Evidence:** scenario report; verifier report.

### VER-INSP-0002 — Buk purity of the kernel
- **Verifies:** CST-005, MET-BIZ-001
- **Suite:** SUITE-PH2-FINANCE · **Phase:** PH-2
- **Environment:** repository
- **Procedure:**
  1. Scan kernel crates for finance identifiers (account, journal, invoice, bill, vendor, customer, 1099, depreciation, revaluation, sales tax) outside tests, fixtures and generic ledger primitives defined by the standard.
  2. Review every kernel change of PH-2 tagged "kernel gap" and confirm it is generic and standardised.
- **Pass criterion:** 0 findings.
- **Evidence:** inspection record.
- **Supports:** EXIT-2-04

### VER-BENCH-2100 — Throughput, freshness and experience latency
- **Verifies:** NR-PERF-003, NR-PERF-013, NR-PERF-014, NR-PERF-015, NR-PERF-016, NR-PERF-017
- **Suite:** SUITE-NOD-BENCH · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER, ENV-REF-WAN, developer workstation (developer loop)
- **Procedure:**
  1. Sustain 2,000 commits/s and 5,000 ledger entries/s for 30 minutes.
  2. Measure commit-to-searchable, detail-view server time, commit-to-client update over WAN, the developer loop for the reference Buk, and simulation throughput on WL-FIN-M.
- **Pass criterion:** all six NR targets met (MET-PERF-020…023).
- **Evidence:** benchmark report.
- **Supports:** EXIT-2-05

### VER-BENCH-0200 — Scale
- **Verifies:** NR-SCAL-001 (PH-2), NR-SCAL-003, NR-SCAL-004, NR-SCAL-005
- **Suite:** SUITE-DVM-BENCH · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER (1 to 8 application nodes)
- **Procedure:**
  1. Load 50,000,000 current objects and 500,000,000 versions; re-run NR-PERF-001 and -002.
  2. Run 5,000 concurrent sessions with subscriptions.
  3. Scale application nodes 1→8 and measure read and commit scaling efficiency.
  4. Load a model of 5,000 classes and 10,000 sheet blocks; measure NR-PERF-009.
- **Pass criterion:** targets met; read efficiency ≥ 0.8, commit efficiency ≥ 0.6.
- **Evidence:** scale report.
- **Supports:** EXIT-2-06

### VER-FAULT-2100 — HA fault matrix, split brain and rolling upgrade
- **Verifies:** NR-AVAIL-002, NR-AVAIL-003, NR-AVAIL-006, FR-OPS-031, FR-OPS-061, FR-OPS-062, DSN-NOD-624, DSN-NOD-625, DSN-NOD-630, DSN-NOD-632
- **Suite:** SUITE-NOD-OPS · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER (HA)
- **Procedure:**
  1. Under WL-MIXED-OLTP kill application nodes, the database primary, and partition the primary from its standby.
  2. Perform a rolling minor upgrade and a faulty release that must roll back automatically.
- **Pass criterion:** no failed requests beyond client retries on node loss; ≤ 60 s write unavailability on primary loss; only the legitimate primary accepts writes; 0 minutes of planned downtime; automatic rollback succeeds.
- **Evidence:** fault report.
- **Supports:** EXIT-2-07

### VER-FAULT-2101 — Disaster recovery drill
- **Verifies:** NR-DUR-002, NR-DUR-003, FR-OPS-063, DSN-NOD-626, SCN-406
- **Suite:** SUITE-NOD-OPS · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER with standby site
- **Procedure:**
  1. Simulate loss of the primary site during load.
  2. Execute the DR runbook; measure RPO from the acknowledgement log and RTO from failure to service.
- **Pass criterion:** RPO and RTO within targets (MET-OPS-021).
- **Evidence:** DR drill report.
- **Supports:** EXIT-2-07

### VER-SEC-2900 — Penetration test and supply chain
- **Verifies:** NR-SEC-001, NR-SEC-007 (SLSA L2), NR-SEC-011, CR-SOC2-004
- **Suite:** SUITE-NOD-PEN · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER staging with production configuration
- **Procedure:**
  1. External pen test of UBTP, REST, Workspace, SSO and SCIM endpoints, file upload and webhooks.
  2. Verify SBOMs and signatures for all release artifacts and SLSA L2 provenance.
- **Pass criterion:** 0 open critical or high findings; 100% artifacts signed with SBOM and provenance.
- **Evidence:** pen-test report, supply-chain report.
- **Supports:** EXIT-2-08

### VER-SCN-2200 — Privacy erasure and subject access
- **Verifies:** SCN-016, NR-PRIV-001, NR-PRIV-002, NR-PRIV-005, CR-GDPR-002, CR-GDPR-004, CR-CCPA-002, DSN-DVM-830, DSN-DVM-831
- **Suite:** SUITE-PH2-FINANCE · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER
- **Procedure:**
  1. Create a sole-proprietor customer and a 1099 vendor with personal data (name, address, TIN) across versions, documents and ledger references.
  2. Produce a subject export; measure time.
  3. Approve erasure; verify values erased in all versions and exports, proofs still verify, ledger amounts and balances are unchanged, records under the IRS retention schedule are refused with the reason, and read logging recorded all personal reads.
- **Pass criterion:** export ≤ 1 h; technical erasure ≤ 24 h; 100% proofs verify; 100% personal reads logged.
- **Evidence:** privacy report.
- **Supports:** EXIT-2-09

### VER-USE-2000 — Accountant and builder usability study
- **Verifies:** NR-USE-001, NR-USE-002, NR-USE-004, NR-USE-006, NR-USE-007
- **Suite:** SUITE-WSP-USE · **Phase:** PH-2
- **Environment:** ENV-REF-WAN with the HA Server
- **Procedure:**
  1. Run moderated sessions: ≥ 8 accountants and clerks, ≥ 8 approvers (controllers), ≥ 6 business architects (ASM-013).
  2. Core tasks: enter an invoice, enter and submit a journal, approve with diff, find a transaction, view a report as of a date, correct an error; builders: new class with lifecycle, 3 rules and views.
- **Pass criterion:** MET-USE-001…003 met; ≥ 85% can state next action after an error; 0 forbidden terms found by the vocabulary scan.
- **Evidence:** study report.
- **Supports:** EXIT-2-10

### VER-INSP-5000 — Accessibility audit
- **Verifies:** NR-ACC-001, NR-ACC-002, CR-WCAG-001, CR-WCAG-002
- **Suite:** SUITE-WSP-A11Y · **Phase:** PH-2
- **Environment:** ENV-REF-BROWSER plus screen readers
- **Procedure:**
  1. Automated axe scans of all catalogue widgets and core screens.
  2. Manual audit of core tasks by keyboard only and with NVDA and VoiceOver by an external auditor.
  3. Publish the Accessibility Conformance Report (VPAT).
- **Pass criterion:** 0 AA violations; 100% of core tasks keyboard-completable; ACR published.
- **Evidence:** audit report, ACR.
- **Supports:** EXIT-2-11

### VER-INSP-5001 — E-SIGN and UETA legal review
- **Status:** Deferred (DEC-022). Moved with the e-signature ceremony to PH-5; see VER-SCN-5100.
- **Suite:** SUITE-PH2-CONTRACTS · **Phase:** deferred
- **Procedure:**
  1. Counsel reviews ceremony flows, consent texts, audit trail, retention and copies against E-SIGN and UETA.
- **Pass criterion:** signed legal memo with no open blocking comments.
- **Evidence:** legal memo.

### VER-SCN-2700 — Sample-company migration reconciliation
- **Verifies:** FR-MIG-011, FR-MIG-031, FR-MIG-033, FR-MIG-051, FR-MIG-052, FR-MIG-053, SCN-015, SCN-501, MET-BIZ-002
- **Suite:** SUITE-BRG-MIG · **Phase:** PH-2
- **Environment:** ENV-REF-SERVER with the migration corpus
- **Procedure:**
  1. Import the sample company's chart of accounts, customers, vendors, open invoices and bills, fixed-asset register, opening balances and one year of journal history from the spreadsheet and CSV extracts.
  2. Resolve the deliberate data-quality defects through the import wizard with recorded decisions.
  3. Produce control totals (trial balance per month, open AR and AP, asset net book value) and record-level reconciliation; seal the migration report.
- **Pass criterion:** 0 unexplained differences against the source extracts; every resolved defect has a recorded decision.
- **Evidence:** sealed reconciliation report.
- **Supports:** EXIT-2-17

### VER-SCN-2701 — Installation and upgrade operability
- **Verifies:** NR-OPER-001, NR-OPER-002, NR-OPER-004
- **Suite:** SUITE-NOD-OPS · **Phase:** PH-2
- **Environment:** ENV-REF-SMALL, ENV-REF-SERVER
- **Procedure:**
  1. A trained operator unfamiliar with the build installs a production-configured Server from the guide.
  2. The operator performs a rolling minor upgrade.
  3. Check that every shipped alert links to a runbook.
- **Pass criterion:** install ≤ 30 min; upgrade ≤ 60 min operator time without downtime; 100% alerts linked.
- **Evidence:** operations report.
- **Supports:** EXIT-2-14

### VER-INSP-9501 — PH-2 traceability and regression
- **Verifies:** all PH-2 Must FR, NR and CR; all PH-1 suites
- **Suite:** SUITE-REGRESSION · **Phase:** PH-2
- **Environment:** CI
- **Procedure:**
  1. Generate the traceability report; list uncovered Must requirements.
  2. Confirm all PH-1 suites passed on the release candidate.
- **Pass criterion:** empty list; all PH-1 suites green.
- **Evidence:** traceability and regression reports.
- **Supports:** EXIT-2-15

### VER-USE-2001 — Finance workbench acceptance
- **Verifies:** FR-OFFICE-041…046, DSN-WSP-116, DSN-WSP-117, DSN-WSP-118, DSN-WSP-119, DSN-WSP-120, DSN-WSP-401, MET-USE-004
- **Suite:** SUITE-WSP-USE · **Phase:** PH-2
- **Environment:** ENV-REF-WAN with the HA Server and the sample company
- **Procedure:**
  1. With the participants of VER-USE-2000, run keyboard-only journal entry of a 50-line journal in the Smart Grid, including paste from a spreadsheet and correction of validation errors.
  2. Run register, report drill-down (statement line → account → entry → source document) and close-cockpit tasks for one month-end.
  3. Record task time, errors and severity-rated usability findings.
- **Pass criterion:** MET-USE-004 met; ≥ 90% success on workbench tasks; 0 open severity-1 findings.
- **Evidence:** study report.
- **Supports:** EXIT-2-10, EXIT-2-18

### VER-INSP-9506 — PH-2 comparison data set
- **Verifies:** MET-BIZ-004
- **Suite:** SUITE-REGRESSION · **Phase:** PH-2
- **Environment:** project records
- **Procedure:**
  1. Check that every PH-2 finance work item has the records required by the DEC-026 comparison protocol (effort, size, defects, change lead time, requirement coverage).
  2. Check that scope differences against the finance requirements document are logged per requirement.
- **Pass criterion:** 100% of work items recorded; data set signed by the owner.
- **Evidence:** comparison data set.
- **Supports:** EXIT-2-19
