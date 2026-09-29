---
id: FIN-DOC-13
title: Non-Functional Requirements and User Interface
status: frozen
---

# Non-Functional Requirements and User Interface

## 1. Volumes and performance

### FIN-NF-001 — Volumes
- **Requirement:** The system shall support, for one company: 600 GL accounts, 5,000 customers, 2,000 vendors, 300,000 invoices and 200,000 bills per year, 2,500,000 GL lines per year with at least 3 years online (7 years retained, FIN-CT-020), 12 bank accounts, 150 fixed assets and 2 foreign currencies besides USD, with 50 concurrent users. This is the benchmark volume; the design shall not have a hard limit below twice these figures.
- **Priority:** Must
- **Basis:** sizing for a mid-sized US company
- **Acceptance:**
  1. Given a generated dataset of this size, when FIN-NF-002 is measured, then all targets are met.

### FIN-NF-002 — Response times
- **Requirement:** At the volumes of FIN-NF-001, the system shall meet (95th percentile, server side): posting an invoice or entry of up to 50 lines ≤ 1 s; opening a document ≤ 1 s; account inquiry of one month ≤ 2 s; trial balance for a month ≤ 5 s; balance sheet and income statement ≤ 5 s; aging ≤ 10 s; year-end close ≤ 10 min; depreciation run for 500 assets ≤ 1 min; matching 5,000 statement lines ≤ 2 min.
- **Priority:** Must
- **Basis:** usability for finance staff
- **Acceptance:**
  1. Given the load test with 50 users, then every target is met.

### FIN-NF-003 — Correctness under concurrency
- **Requirement:** The system shall keep balances, sequences and subledger totals correct when users post, approve and apply concurrently; no posted amount may be lost or counted twice after a crash.
- **Priority:** Must
- **Basis:** data integrity
- **Acceptance:**
  1. Given 20 users posting and applying receipts concurrently for 30 minutes with the server killed twice, then debits equal credits, sequences have no gaps, and every acknowledged posting exists exactly once.

## 2. Availability and recovery

### FIN-NF-004 — Availability
- **Requirement:** The system should be available 99.5% of business hours (06:00–22:00 company time, weekdays) with planned maintenance outside them.
- **Priority:** Should
- **Basis:** practice
- **Acceptance:**
  1. Given one month of operation, then measured availability in business hours is at least 99.5%.

### FIN-NF-005 — Backup and recovery
- **Requirement:** The system shall back up all data so that after a failure at most 15 minutes of acknowledged work is lost (RPO) and service is restored within 4 hours (RTO), and shall restore to a chosen point in time for testing.
- **Priority:** Must
- **Basis:** business continuity
- **Acceptance:**
  1. Given a restore drill, then RPO and RTO are met and the restored trial balance equals the one before the failure point.

## 3. User interface

### FIN-UI-001 — Professional workbench
- **Requirement:** The system shall provide a web user interface designed for daily professional use by finance staff: dense but readable lists and forms, consistent navigation by module, global search by number, name and amount, saved filters and column layouts, and no platform or technical vocabulary.
- **Priority:** Must
- **Basis:** usability for accountants
- **Acceptance:**
  1. Given a usability study with at least 8 accountants and clerks, then ≥ 90% of core tasks are completed without help and the System Usability Scale score is ≥ 75.

### FIN-UI-002 — Keyboard-first entry
- **Requirement:** The system shall allow complete entry of invoices, bills, receipts and journal entries by keyboard, with predictable tab order, type-ahead for accounts, customers and vendors, keyboard shortcuts for save, submit and new line, and amounts entered without typing separators.
- **Priority:** Must
- **Basis:** productivity
- **Acceptance:**
  1. Given a trained clerk, when a 5-line bill is entered using only the keyboard, then no mouse action is needed.

### FIN-UI-003 — Grid entry
- **Requirement:** The system shall provide the bulk entry grid of FIN-GL-020 with copy and paste to and from spreadsheets, fill-down, and undo.
- **Priority:** Must
- **Basis:** productivity
- **Acceptance:**
  1. Given a 50-line journal, when entered by keyboard in the grid, then the median time in the usability study is ≤ 5 minutes.

### FIN-UI-004 — Registers and drill-down
- **Requirement:** The system shall show registers (journal, invoice, bill, payment, receipt, bank) as sortable, filterable lists with totals, and allow drill-down from every total and statement line to detail (FIN-RP-006).
- **Priority:** Must
- **Basis:** review practice
- **Acceptance:**
  1. Given the invoice register filtered to January, then its total equals the sum of posted invoice totals and each row opens the invoice.

### FIN-UI-005 — Financial number presentation
- **Requirement:** The system shall right-align amounts, use thousands separators and a fixed number of decimals, show negatives in parentheses in reports, show currency codes when more than one currency is present, and never round displayed totals differently from stored totals.
- **Priority:** Must
- **Basis:** presentation practice
- **Acceptance:**
  1. Given the income statement, then negative amounts appear as `(2,000.00)` and totals equal the sum of displayed lines.

### FIN-UI-006 — Close workspace
- **Requirement:** The system shall provide a close workspace per period with the checklist, reconciliations, open exceptions, subledger status and the close action (FIN-PC-004, FIN-PC-009).
- **Priority:** Must
- **Basis:** controller workflow
- **Acceptance:**
  1. Given January with all checks passed, then the controller closes the period from the close workspace in one action.

### FIN-UI-007 — Explanations
- **Requirement:** The system shall explain, for any computed amount on a document (tax, depreciation, exchange gain or loss, approval requirement), how it was computed and which rule or rate version was used.
- **Priority:** Must
- **Basis:** auditability
- **Acceptance:**
  1. Given INV-1004's tax of 3,300.00, when the user asks why, then the taxable base, the TX-AUSTIN rate 8.25% and its effective date are shown.

### FIN-UI-008 — History view
- **Requirement:** The system shall show the full history of every document and master record (versions, who, when, what changed), and should allow viewing a record as it was on a past date.
- **Priority:** Must
- **Basis:** audit
- **Acceptance:**
  1. Given customer C100 changed twice, then both versions and the changes between them are shown.

### FIN-UI-009 — Accessibility
- **Requirement:** The user interface shall conform to WCAG 2.2 level AA.
- **Priority:** Must
- **Basis:** WCAG 2.2
- **Acceptance:**
  1. Given an external accessibility audit of the core tasks, then no level A or AA failure remains open.

### FIN-UI-010 — Browsers and locale
- **Requirement:** The system shall support the current versions of Chrome, Edge, Firefox and Safari on desktop screens from 1366 × 768, with US English and US formats; other locales are not required.
- **Priority:** Must
- **Basis:** scope
- **Acceptance:**
  1. Given each browser, then the core tasks of FIN-UI-001 pass.

## 4. Operations

### FIN-NF-006 — Installation and upgrade
- **Requirement:** The system shall be installable from documentation in ≤ 1 working day by an administrator and upgradable without data loss, with data migrations applied automatically and verified.
- **Priority:** Must
- **Basis:** operability
- **Acceptance:**
  1. Given an upgrade between two releases on the sample company's books, then the trial balance and all reports of FIN-EXP-* are unchanged afterwards.

### FIN-NF-007 — Monitoring
- **Requirement:** The system shall expose health, error and performance metrics and logs to standard monitoring tools, without sensitive data in logs.
- **Priority:** Must
- **Basis:** operability; privacy
- **Acceptance:**
  1. Given a search of one day's logs for TINs and bank account numbers, then none is found.
