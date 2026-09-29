---
id: FIN-DOC-03
title: Periods and Close
status: frozen
---

# Periods and Close

### FIN-PC-001 — Fiscal calendar
- **Requirement:** The system shall define fiscal years and their periods (12 monthly periods by default, with an optional adjustment period 13 at year end), and derive each entry's period from its posting date.
- **Priority:** Must
- **Basis:** ASC 205; accepted practice
- **Acceptance:**
  1. Given the calendar-year calendar, when an entry dated 2026-01-31 is posted, then it belongs to period 2026-01.
  2. Given adjustment period 13 is enabled, when an audit adjustment is posted to it, then it appears in the annual statements but not in December's monthly statements.

### FIN-PC-002 — Opening balances
- **Requirement:** The system shall record opening balances of new books as one balanced opening entry dated the day before the first period, and opening subledger items (open invoices, open bills, fixed assets, outstanding bank items) whose totals equal the opening control-account balances.
- **Priority:** Must
- **Basis:** accepted practice; migration integrity
- **Acceptance:**
  1. Given the sample company's opening files, when they are loaded, then the trial balance at 2025-12-31 equals FIN-EXP-01 and the subledger totals equal accounts 1200, 2000, 1500, 1510 and 1590.
  2. Given an opening trial balance whose debits and credits differ, when it is loaded, then nothing is posted and the difference is shown.

### FIN-PC-003 — Period states
- **Requirement:** The system shall manage the period states *open* (all postings allowed), *soft-closed* (only users with the close permission may post, and only adjusting entries) and *closed* (no postings). Subledgers may be closed per period before the general ledger.
- **Priority:** Must
- **Basis:** period cut-off; SOX
- **Acceptance:**
  1. Given January is closed, when an accountant posts an entry dated 2026-01-20, then it is refused with the reason "period closed" (FIN-SCN-07).
  2. Given January is soft-closed, when an accountant without the close permission posts an entry dated in January, then it is refused; the controller may post an adjusting entry.
  3. Given receivables are closed for January but the general ledger is open, when a clerk posts an invoice dated in January, then it is refused.

### FIN-PC-004 — Close checklist
- **Requirement:** The system shall provide a configurable close checklist per period with tasks, owners, due dates, status and evidence, including automatic checks: no unposted or unapproved entries dated in the period; all bank accounts reconciled; subledgers equal their control accounts; recurring, depreciation and revaluation runs completed; suspense accounts at zero.
- **Priority:** Must
- **Basis:** COSO monitoring; close practice
- **Acceptance:**
  1. Given the operating bank account is not reconciled for January, when the controller closes January, then the close is refused and the failing check is named.
  2. Given all checks pass, when the checklist is shown, then each automatic check shows its result, time and evidence link.

### FIN-PC-005 — Closing a period
- **Requirement:** The system shall close a period only on the action of a user with the close permission after all mandatory checklist items pass, and shall record an immutable close artifact containing the period, the trial balance, subledger totals, checklist results, the user and the time.
- **Priority:** Must
- **Basis:** SOX; audit evidence
- **Acceptance:**
  1. Given January is closed, when the close artifact is opened, then it contains the trial balance of FIN-EXP-03, the checklist results, the controller's identity and the time.
  2. Given the close artifact, when it is compared with a later re-run of the January trial balance "as known on" the close time, then they are identical.

### FIN-PC-006 — Governed reopen
- **Requirement:** The system shall allow reopening a closed period only through a request with a reason that a controller other than the requester approves. After corrections, the period is closed again with a new close artifact; the earlier artifact remains and is marked superseded.
- **Priority:** Must
- **Basis:** ASC 250; SOX change control
- **Acceptance:**
  1. Given January is closed, when an accountant requests a reopen and the controller approves it, then January is open, the correction is posted, and closing again produces a second artifact that references the first.
  2. Given the reopen is rejected, then January stays closed.
  3. Given January was closed twice, when the January statements are run "as known on" the first close time, then they equal the originally issued statements.

### FIN-PC-007 — Prior-period items in the current period
- **Requirement:** The system shall allow recording a transaction whose document date falls in a closed period with a posting date in the current open period, and shall keep both dates and a reference to the prior period so that it can be reported as a prior-period item.
- **Priority:** Must
- **Basis:** ASC 250 (immaterial corrections in the current period)
- **Acceptance:**
  1. Given BILL-OS-0120 with document date 2026-01-20, when it is posted on 2026-02-10, then it is in period 2026-02 and the prior-period report lists it with its January document date.

### FIN-PC-008 — Year-end close
- **Requirement:** The system shall perform the year-end close by transferring the net income of the year from income and expense accounts to retained earnings with a closing entry (or by an equivalent calculation that leaves income-statement accounts at zero for the new year), and shall open the next fiscal year with balance-sheet balances carried forward.
- **Priority:** Must
- **Basis:** accepted practice; ASC 505 equity
- **Acceptance:**
  1. Given a fiscal year with net income N, when the year is closed, then retained earnings increases by N, all revenue and expense accounts start the new year at zero, and the balance sheet at year end is unchanged by the close.
  2. Given a posting after year-end close to the adjustment period, when the year is closed again, then retained earnings reflects the adjustment and the earlier close artifact is superseded.

### FIN-PC-009 — Close status overview
- **Requirement:** The system shall show a close overview for a period with the state of each subledger, each checklist task, each reconciliation, and the progress towards close.
- **Priority:** Should
- **Basis:** usability for controllers (FIN-UI-006)
- **Acceptance:**
  1. Given January with 2 of 10 tasks open, when the controller opens the overview, then the two open tasks, their owners and due dates are shown first.
