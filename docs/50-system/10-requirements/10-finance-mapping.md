---
id: UBS-REQ-10
title: Annex — Basic Finance Requirements Mapped to the Platform
status: draft
phase: PH-2
depends_on: [UBS-REQ-03, UBS-REQ-04, DEC-022, DEC-026, DEC-029]
---

# Annex — Basic Finance Requirements Mapped to the Platform

The implementation-neutral finance requirements live in `docs/finance-requirements/`
(DEC-026). This annex maps version 1.0 (git tag `fin-req-v1.0`, DEC-029). They use their own IDs (`FIN-<AREA>-<NNN>`) and never refer to the platform. This
annex states, per finance area, which platform capabilities, requirements, design items and
scenarios realise them on UBOS, and which part is **Buk content** (classes, rules, views and
reports of the `finance` Buk) rather than kernel or system functionality. Nothing in the
finance area may add finance-specific kernel code (EXIT-2-04).

## 1. Area mapping

| Finance area (FIN) | Platform capabilities | Platform requirements and design items | Built in the `finance` Buk | Platform scenarios |
|---|---|---|---|---|
| GL — chart of accounts, journals, posting, approvals, recurring and reversing entries, grid entry | CAP-LEDG-01, CAP-LEDG-02, CAP-LEDG-04, CAP-LEDG-05, CAP-FLOW-01, CAP-FLOW-04, CAP-OFFICE-04, CAP-MIG-01 | FR-LEDG-011, FR-LEDG-021, FR-LEDG-041, FR-LEDG-051, FR-LEDG-052, FR-FLOW-041, FR-FLOW-043, FR-FLOW-044, FR-FLOW-045, DSN-WSP-401, DSN-WSP-116 | `Account`, `JournalEntry` lifecycle, `GlLine` ledger class, approval sheets, recurring templates | SCN-501, SCN-502 |
| PC — calendar, period states, checklist, close, reopen, year-end | CAP-LEDG-06, CAP-FLOW-05, CAP-VER-06, CAP-TIME-04 | FR-LEDG-061, FR-LEDG-062, FR-LEDG-063, FR-TIME-031, FR-VER-031, DSN-WSP-119 | `FiscalCalendar`, `Period`, close checklist rules, year-end closing logic | SCN-506, SCN-507, SCN-511 |
| AR — customers, invoices, credit memos, receipts, aging, write-off | CAP-LEDG-03, CAP-FLOW-01, CAP-ANL-03, CAP-OFFICE-08 | FR-LEDG-013, FR-LEDG-031, FR-OFFICE-081, FR-ANL-031, DSN-WSP-117 | `Customer`, `Invoice`, `CreditMemo`, `Receipt`, application logic, aging report | SCN-503 |
| AP — vendors, W-9, bills, duplicate control, payment runs, Form 1099 | CAP-FLOW-04, CAP-RULE-03, CAP-IAM-06, CAP-INT-07 | FR-FLOW-041, FR-INT-074, FR-RULE-031, FR-ANL-031, DSN-NOD-305 | `Vendor`, `Bill`, `Payment`, 1099 classification and threshold table, duplicate rule | SCN-504 |
| BK — statement import, matching, reconciliation, payment files | CAP-LEDG-07, CAP-INT-07 | FR-LEDG-071, FR-LEDG-072, FR-LEDG-073, FR-LEDG-074, FR-INT-071, FR-INT-072, DSN-NOD-305, DSN-BRG-102 | `BankAccount`, `BankStatementLine`, `ReconMatch`, matching rules | SCN-505 |
| FA — register, depreciation, disposal | CAP-FLOW-06, CAP-RULE-04 | FR-LEDG-051, FR-TIME-034 | `FixedAsset`, `DepreciationSchedule`, depreciation logic and run | SCN-506 |
| FX — rates, foreign-currency documents, remeasurement | CAP-LOC-02, CAP-TIME-03 | FR-LEDG-053, FR-LOC-021, FR-LOC-022, FR-TIME-034 | revaluation run, realized and unrealized posting logic | SCN-510 |
| TX — jurisdictions, rates, exemptions, returns data | CAP-RULE-04, CAP-RULE-06, CAP-AUD-03 | FR-RULE-041, FR-RULE-022 | `TaxCode`, rate decision tables with effective periods, certificates | SCN-503 |
| RP — statements, drill-down, "as of" and "as known on", reproducible runs | CAP-ANL-03, CAP-TIME-03, CAP-TIME-06, CAP-UX-05 | FR-ANL-031, FR-ANL-032, FR-TIME-021, FR-TIME-041, FR-TIME-042, DSN-WSP-118, DSN-WSP-120 | statement layouts, cash-flow classification, report definitions | SCN-506, SCN-508 |
| CT, SC — segregation of duties, approvals, audit trail, tamper evidence, access, retention | CAP-FLOW-04, CAP-RULE-07, CAP-AUD-01, CAP-AUD-06, CAP-AUD-07, CAP-PROOF-03, CAP-PROOF-05, CAP-PROOF-06, CAP-IAM-02, CAP-IAM-04, CAP-IAM-06, CAP-IAM-08 | FR-RULE-071, FR-RULE-072, FR-RULE-073, FR-AUD-021, FR-AUD-041, FR-AUD-061, FR-PROOF-041, FR-PROOF-051, FR-PROOF-061, CR-SOX-001, CR-SOX-002, CR-SOX-003, CR-SOX-005, CR-SOX-006 | approval and SoD sheets, role definitions, retention schedule | SCN-502, SCN-509, SCN-511 |
| DI — import, migration, API, events | CAP-MIG-01, CAP-MIG-03, CAP-MIG-05, CAP-INT-02, CAP-INT-03 | FR-MIG-011, FR-MIG-033, FR-MIG-051 | import mappings for the sample company and payroll files | SCN-501 |
| NF, UI — volumes, performance, workbench, accessibility | CAP-UX-02, CAP-UX-03, CAP-OFFICE-04, CAP-AUD-03 | NR-PERF-014, NR-PERF-017, DSN-WSP-116, DSN-WSP-117, DSN-WSP-118, DSN-WSP-119, DSN-WSP-120 | views, list layouts, keyboard maps | SCN-501, SCN-506 |

## 2. Beyond the finance requirements

Some platform capabilities go beyond what the finance requirements ask and are shown in the
PH-3 scenarios: anchored evidence packs verified offline by the CPA (SCN-511) and finance
agents working under review (SCN-512). They are measured under FIN-MET-12 and reported as
additional capability in the comparison, not as requirement coverage.

## 3. Acceptance on the platform

The platform passes the finance requirements when SUITE-PH2-FINANCE runs the finance
acceptance scenarios on the HA Server and reproduces every expected result of the sample
company (EXIT-2-16). The comparison data are recorded under the protocol of the finance
requirements (EXIT-2-19, EXIT-3-15).
