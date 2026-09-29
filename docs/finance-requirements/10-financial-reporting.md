---
id: FIN-DOC-10
title: Financial Reporting
status: frozen
---

# Financial Reporting

### FIN-RP-001 — Trial balance
- **Requirement:** The system shall produce the trial balance for any period or date range with opening balance, debits, credits and closing balance per account, totals that balance, optional roll-up and dimensions, before or after closing entries.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given the sample data, when the trial balance at 2026-01-31 is run, then it equals FIN-EXP-03 with totals 826,012.90.

### FIN-RP-002 — Balance sheet
- **Requirement:** The system shall produce a classified balance sheet (current and non-current assets and liabilities, stockholders' equity) at any date, with comparative columns (prior month end, prior year end), from a configurable statement layout that maps accounts to lines.
- **Priority:** Must
- **Basis:** ASC 210
- **Acceptance:**
  1. Given 2026-01-31, then the balance sheet equals FIN-EXP-05 and total assets equal total liabilities and equity (617,415.00).
  2. Given an account without a statement line, when the layout is validated, then the account is reported as unmapped and the statement cannot be issued.

### FIN-RP-003 — Income statement
- **Requirement:** The system shall produce a multi-step income statement (revenue, cost of goods sold, gross profit, operating expenses, operating income, other income and expense, income before taxes, income tax, net income) for any period, with month, quarter-to-date and year-to-date columns and comparatives.
- **Priority:** Must
- **Basis:** ASC 220
- **Acceptance:**
  1. Given January 2026, then the income statement equals FIN-EXP-04 with net income 5,127.10.

### FIN-RP-004 — Statement of cash flows
- **Requirement:** The system shall produce the statement of cash flows by the indirect method with operating, investing and financing sections, reconciliation to the change in cash and cash equivalents, non-cash investing and financing disclosure, and supplemental interest and income taxes paid; account classification for cash-flow purposes shall be configurable and non-cash items (depreciation, unrealized gains and losses, assets acquired on account) shall be identified from their sources.
- **Priority:** Must
- **Basis:** ASC 230
- **Acceptance:**
  1. Given January 2026, then the statement equals FIN-EXP-06, including 12,000.00 of equipment acquired on account as a non-cash investing activity and net decrease in cash of 38,320.00.

### FIN-RP-005 — Statement of stockholders' equity
- **Requirement:** The system shall produce the statement of changes in stockholders' equity by component (common stock, additional paid-in capital, retained earnings, other components) with net income, dividends and other movements.
- **Priority:** Must
- **Basis:** ASC 505; Regulation S-X 3-04 format as practice
- **Acceptance:**
  1. Given January 2026, then the statement equals FIN-EXP-07.

### FIN-RP-006 — Drill-down
- **Requirement:** The system shall let a user drill from any statement or report figure to the accounts behind it, from an account to its entries, and from an entry to its source document and attachments, keeping the report's "as of" and "as known on" settings.
- **Priority:** Must
- **Basis:** audit and review practice (FIN-UI-004)
- **Acceptance:**
  1. Given the income statement line "Professional fees" 34,000.00, when the user drills down, then BILL-DC-2601, BILL-JR-014 and JE-0002 are shown and each opens its document.

### FIN-RP-007 — Subledger reconciliation reports
- **Requirement:** The system shall report, for any date, each control account against its subledger total (receivables, payables, fixed assets, bank) and list differences.
- **Priority:** Must
- **Basis:** close control
- **Acceptance:**
  1. Given 2026-01-31, then accounts 1200, 2000, 1500 + 1510 + 1520, 1590 equal their subledgers with 0.00 difference.

### FIN-RP-008 — General ledger and journal reports
- **Requirement:** The system shall produce the general-ledger detail report and the journal register by date, number, source, user and account, including preparer and approver of each entry.
- **Priority:** Must
- **Basis:** audit
- **Acceptance:**
  1. Given January, then the journal register lists every document of FIN-EXP-02 in number order with preparer and approver.

### FIN-RP-009 — Report runs are reproducible
- **Requirement:** The system shall store each issued report run with its parameters, the report-definition version and a content hash, and shall reproduce the identical output later on request.
- **Priority:** Must
- **Basis:** audit; record retention
- **Acceptance:**
  1. Given the January balance sheet issued on 2026-02-03, when it is reproduced in 2027 after later postings, then the output is identical and its hash matches.

### FIN-RP-010 — Export formats
- **Requirement:** The system shall export every report to PDF (print quality, page headers with company, report, period, run time and page numbers) and to Excel-compatible and CSV formats with numeric cells.
- **Priority:** Must
- **Basis:** practice
- **Acceptance:**
  1. Given the trial balance exported to Excel, then amounts are numeric cells whose sum equals the totals.

### FIN-RP-011 — Report layouts
- **Requirement:** The system should allow a controller to define statement and management-report layouts (rows mapped to accounts or account ranges, subtotals, columns for periods, comparatives and variances) without programming, with versioning of layouts.
- **Priority:** Should
- **Basis:** practice
- **Acceptance:**
  1. Given a new layout version, then reports issued with the old version reproduce with the old version.

### FIN-RP-012 — Notes support
- **Requirement:** The system should provide schedules that support common notes: property and equipment roll-forward, receivables and allowance, accrued liabilities, debt, and foreign-exchange gains and losses.
- **Priority:** Should
- **Basis:** ASC 235 disclosures
- **Acceptance:**
  1. Given January, then the property and equipment schedule equals the roll-forward of FIN-FA-009.

### FIN-RP-020 — "As of" and "as known on" reporting
- **Requirement:** The system shall run every financial report as of a posting date and as known on a recorded-on date, so that a report issued in the past can be reproduced exactly as it was, and the effect of later back-dated entries can be shown as a difference.
- **Priority:** Must
- **Basis:** ASC 250 (reissued statements, restatements); audit
- **Acceptance:**
  1. Given JE-0005 effective 2026-02-03 recorded 2026-02-20, when the trial balance as of 2026-02-05 is run as known on 2026-02-05 and as known on 2026-02-28, then they equal FIN-EXP-16 and differ only in 5000 and 6400 by 15,000.00.

### FIN-RP-021 — Dashboard
- **Requirement:** The system should provide a dashboard for controllers and executives with cash position, receivables and payables aging summaries, month-to-date revenue and expenses, and close status, each linked to the underlying report.
- **Priority:** Should
- **Basis:** practice
- **Acceptance:**
  1. Given 2026-01-31, then the dashboard shows cash 311,680.00, receivables 158,735.00 and payables 46,300.00, each opening its report.
