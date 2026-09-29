---
id: FIN-DOC-30
title: Acceptance Scenarios
status: frozen
---

# Acceptance Scenarios

Both implementations are accepted by running these scenarios on the sample company
(`20-sample-company.md`). A scenario passes when every step behaves as stated and every
referenced expected result is reproduced exactly. **Covers** lists the requirements the
scenario demonstrates; `90-traceability.md` is generated from it.

### FIN-SCN-01 — Set up the books
- **Roles:** ROLE-Controller, ROLE-Accountant, ROLE-SystemAdministrator
- **Steps:**
  1. The administrator creates users and assigns roles; the conflict report shows no conflicts.
  2. The controller creates the chart of accounts from `chart-of-accounts.csv` (or the template adapted to it), the fiscal calendar 2026 and the approval rules of `20-sample-company.md` §4.
  3. Customers, vendors (with W-9 data), tax codes, exchange rates and fixed assets are imported.
  4. The opening balances and open items are loaded; an unbalanced test file is rejected first.
- **Expected:** the trial balance at 2025-12-31 equals FIN-EXP-01; AR, AP and asset subledgers equal their control accounts; TINs are masked for the payables clerk.
- **Covers:** FIN-GL-001, FIN-GL-002, FIN-GL-003, FIN-GL-005, FIN-PC-001, FIN-PC-002, FIN-AR-001, FIN-AP-001, FIN-AP-002, FIN-FA-001, FIN-FA-002, FIN-FX-001, FIN-FX-002, FIN-TX-001, FIN-DI-001, FIN-DI-002, FIN-SC-002, FIN-SC-004, FIN-BK-001

### FIN-SCN-02 — Journal entries with maker–checker approval
- **Roles:** ROLE-Accountant, ROLE-Controller
- **Steps:**
  1. The accountant enters JE-0001 (15,000.00) with a support document and submits it; the controller approves it; it posts.
  2. The accountant enters JE-0002 (25,000.00) in the grid and submits it; the accountant tries to approve it → refused; the accountant changes the memo after the controller's approval → approval invalidated; the controller approves again; it posts.
  3. JE-0003 is generated from the recurring template; running the template again creates nothing.
  4. JE-0004 (1,362.90) posts without approval; the audit trail shows the rule version evaluated.
  5. A user tries to change a posted amount and to post to account 1200 directly → both refused.
- **Expected:** GL lines as in FIN-EXP-02; gap-free numbers; approvals and rule versions in the audit trail.
- **Covers:** FIN-GL-010, FIN-GL-011, FIN-GL-012, FIN-GL-013, FIN-GL-014, FIN-GL-015, FIN-GL-016, FIN-GL-017, FIN-GL-020, FIN-CT-001, FIN-CT-002, FIN-CT-003, FIN-CT-010, FIN-UI-002, FIN-UI-003, FIN-DI-009

### FIN-SCN-03 — Invoice to cash with sales tax
- **Roles:** ROLE-ReceivablesClerk, ROLE-Accountant
- **Steps:**
  1. RCPT-0001 is recorded and applied to INV-1001.
  2. INV-1004 is issued (component line taxable, service line NT); the invoice PDF is e-mailed.
  3. CM-2001 is issued for returned components and applied.
  4. INV-1006 (Oregon) and INV-1007 (resale with certificate) are issued.
  5. RCPT-0002 is applied to INV-1002; RCPT-0003 is applied partly to INV-1003; a receipt is applied to the wrong invoice and re-applied (not in the expected results; its history is checked).
  6. Customer statement for C300 and the aging at 2026-01-31 are produced.
- **Expected:** tax 3,300.00 and 165.00 as in FIN-EXP-02; aging equals FIN-EXP-08; sales tax report equals FIN-EXP-13.
- **Covers:** FIN-AR-002, FIN-AR-003, FIN-AR-004, FIN-AR-005, FIN-AR-006, FIN-AR-007, FIN-AR-008, FIN-AR-009, FIN-AR-010, FIN-TX-002, FIN-TX-003, FIN-TX-004, FIN-TX-005, FIN-TX-006, FIN-TX-008, FIN-GL-021, FIN-UI-007

### FIN-SCN-04 — Bills to payment with Form 1099
- **Roles:** ROLE-PayablesClerk, ROLE-Controller, ROLE-Treasurer
- **Steps:**
  1. Bills of January are entered; P-7902 (22,000.00) waits for controller approval; a second entry of vendor invoice P-7902 is blocked.
  2. BILL-TS-5520 coded to 1520 creates asset FA-003.
  3. PAY-RUN-01 is proposed on 2026-01-08, approved by the controller and released by the treasurer with a second factor; the NACHA file validates.
  4. A change of V200's bank details holds V200's bill until a second person approves it.
  5. STX-PAY-2512 is recorded; PAY-RUN-02 is paid.
  6. The 1099 report for 2026 is run with the threshold table; the review report lists no exceptions.
- **Expected:** AP aging equals FIN-EXP-09; the 1099 summary equals FIN-EXP-14; generating PAY-RUN-02's file twice is refused.
- **Covers:** FIN-AP-003, FIN-AP-004, FIN-AP-005, FIN-AP-006, FIN-AP-007, FIN-AP-009, FIN-AP-010, FIN-AP-011, FIN-AP-012, FIN-AP-013, FIN-AP-015, FIN-AP-020, FIN-AP-021, FIN-AP-022, FIN-BK-011, FIN-SC-001

### FIN-SCN-05 — Bank reconciliation
- **Roles:** ROLE-Accountant, ROLE-Controller
- **Steps:**
  1. `bank-statement-2026-01.csv` is imported (and, if supported, the same statement as BAI2 and camt.053); importing it again adds nothing.
  2. Automatic matching proposes the eight expected matches; the accountant accepts them.
  3. Entries are created from the fee and interest lines.
  4. A match is undone and redone.
  5. The reconciliation is completed and signed off by preparer and reviewer.
- **Expected:** the reconciliation equals FIN-EXP-10; reprinting it after February postings gives the identical report.
- **Covers:** FIN-BK-003, FIN-BK-004, FIN-BK-005, FIN-BK-006, FIN-BK-007, FIN-BK-008

### FIN-SCN-06 — Month-end close and financial statements
- **Roles:** ROLE-Accountant, ROLE-Controller, ROLE-Executive
- **Steps:**
  1. The depreciation run posts DEP-2601; a second run posts nothing.
  2. The revaluation run posts FXR-2601 marked auto-reversing.
  3. PAYROLL-2601 is imported and approved; JE-0004 is posted.
  4. The close checklist first fails on an unreconciled account (before FIN-SCN-05) and passes afterwards; the controller soft-closes, then closes January.
  5. The controller runs the trial balance, balance sheet, income statement, cash-flow statement and equity statement, drills from "Professional fees" to the documents, and exports to PDF and Excel.
- **Expected:** FIN-EXP-03 … FIN-EXP-07 and FIN-EXP-11 reproduced; the close artifact contains FIN-EXP-03; subledgers equal control accounts.
- **Covers:** FIN-FA-003, FIN-FA-004, FIN-FA-005, FIN-FA-009, FIN-FX-005, FIN-GL-018, FIN-GL-019, FIN-DI-004, FIN-PC-003, FIN-PC-004, FIN-PC-005, FIN-CT-005, FIN-RP-001, FIN-RP-002, FIN-RP-003, FIN-RP-004, FIN-RP-005, FIN-RP-006, FIN-RP-007, FIN-RP-008, FIN-RP-010, FIN-UI-004, FIN-UI-005, FIN-UI-006, FIN-GL-022

### FIN-SCN-07 — Closed period and governed reopen
- **Roles:** ROLE-Accountant, ROLE-Controller
- **Steps:**
  1. The accountant tries to post a bill dated 2026-01-20 → refused (period closed).
  2. BILL-OS-0120 is posted with document date 2026-01-20 and posting date 2026-02-10.
  3. The accountant requests a reopen of January for a test correction; the controller rejects it; January stays closed. A second request is approved; a test entry is posted and reversed; January is closed again.
- **Expected:** the prior-period report lists BILL-OS-0120; two close artifacts exist, the first superseded; January statements "as known on" the first close equal those of FIN-SCN-06.
- **Covers:** FIN-PC-006, FIN-PC-007, FIN-RP-009, FIN-RP-020

### FIN-SCN-08 — Back-dated correction and "as known on" reporting
- **Roles:** ROLE-Accountant, ROLE-Controller, ROLE-ExternalAuditor
- **Steps:**
  1. BILL-P-8010 is posted on 2026-02-03 to 6400 in error; RCPT-0004 is posted on 2026-02-05; the trial balance as of 2026-02-05 is issued.
  2. On 2026-02-20 JE-0005 reclassifies P-8010 effective 2026-02-03.
  3. The trial balance as of 2026-02-05 is run as known on 2026-02-05 and as known on 2026-02-28.
- **Expected:** FIN-EXP-16; the issued report of step 1 is reproduced identically.
- **Covers:** FIN-RP-020, FIN-RP-009, FIN-UI-008

### FIN-SCN-09 — Foreign-currency receivable
- **Roles:** ROLE-ReceivablesClerk, ROLE-Accountant
- **Steps:**
  1. INV-1005 for EUR 50,000.00 is issued on 2026-01-12.
  2. The January revaluation posts the unrealized gain; it reverses on 2026-02-01.
  3. RCPT-0005 of EUR 50,000.00 is recorded on 2026-02-20.
  4. The 2026-01-31 rate is corrected in a test copy and the revaluation is simulated again.
- **Expected:** FIN-EXP-12 and FIN-EXP-17; the aging shows EUR and USD amounts; the original revaluation run is reproducible.
- **Covers:** FIN-FX-003, FIN-FX-004, FIN-FX-005, FIN-FX-007, FIN-FX-002

### FIN-SCN-10 — Rule change with impact preview
- **Roles:** ROLE-Controller
- **Steps:**
  1. The controller prepares a change of the journal approval threshold from 10,000.00 to 5,000.00 effective 2026-03-01 and previews its effect on January.
  2. The change is approved and published.
- **Expected:** January entries keep the 10,000.00 rule version in their explanation; the change and approver are in the audit trail.
- **Covers:** FIN-CT-002, FIN-CT-004, FIN-SC-005

### FIN-SCN-11 — Year-end close
- **Roles:** ROLE-Controller
- **Steps:**
  1. Using a generated full year that follows the dataset rules, the controller closes December and then the fiscal year 2026.
  2. An audit adjustment is posted to period 13; the year is closed again.
- **Expected:** revenue and expense accounts start 2027 at zero; retained earnings increases by the year's net income including the adjustment; the first year-end artifact is superseded.
- **Covers:** FIN-PC-001, FIN-PC-008

### FIN-SCN-12 — Audit support and integrity
- **Roles:** ROLE-ExternalAuditor, ROLE-Controller, ROLE-SystemAdministrator
- **Steps:**
  1. The auditor, limited to fiscal 2026, requests all manual entries above 10,000.00 in January with approvals, the user-access report and the January bank reconciliation.
  2. The integrity verification runs; in a test copy one posted line is altered at database level and verification runs again.
  3. A legal hold is placed on V200's records; deletion of a 2026 invoice is attempted.
  4. The books of January are exported to the open archive format and the trial balance is recomputed from it.
- **Expected:** the package contains the manual entries JE-0001 and JE-0002 with their approvals; the altered line is detected; deletion is refused; the recomputed trial balance equals FIN-EXP-03.
- **Covers:** FIN-CT-010, FIN-CT-011, FIN-CT-012, FIN-CT-020, FIN-CT-021, FIN-SC-002, FIN-SC-003, FIN-DI-008

### FIN-SCN-13 — Interfaces
- **Roles:** integration client
- **Steps:**
  1. An API client creates a test bill T-9001 of 12,000.00 (above the approval limit) twice with the same idempotency key.
  2. An API client without posting permission tries to post it.
  3. The client reads the January trial balance through the API.
- **Expected:** one bill exists and waits for approval; posting is refused; the trial balance equals FIN-EXP-03.
- **Covers:** FIN-DI-005, FIN-DI-006

### FIN-SCN-14 — Operations and quality
- **Roles:** ROLE-SystemAdministrator, test team
- **Steps:**
  1. Install the system from its documentation and upgrade it to the next release on the sample books.
  2. Run the performance test at the volumes of FIN-NF-001 with 50 users, including a concurrency and crash test.
  3. Run a restore drill.
  4. Run the usability study and the accessibility audit on the four supported browsers.
  5. Search logs for sensitive data.
- **Expected:** all expected results unchanged after upgrade; targets of FIN-NF-002, FIN-NF-003 and FIN-NF-005 met; FIN-UI-001 and FIN-UI-009 met; no sensitive data in logs.
- **Covers:** FIN-NF-001, FIN-NF-002, FIN-NF-003, FIN-NF-005, FIN-NF-006, FIN-NF-007, FIN-UI-001, FIN-UI-009, FIN-UI-010

### FIN-SCN-15 — Adjustments and exceptions on a copy of the January books
- **Roles:** ROLE-Controller, ROLE-Accountant, ROLE-ReceivablesClerk, ROLE-PayablesClerk, ROLE-Treasurer
- **Steps:** on a copy of the books after FIN-SCN-06 (these steps are not part of FIN-EXP-*):
  1. The controller renames an account, deactivates another and tries to delete an account with postings (refused); posting to the inactive account is refused.
  2. A 500.00 test invoice is written off against the allowance with approval.
  3. A vendor credit of 500.00 is applied to a 1,200.00 bill; a check payment is voided on 2026-02-10.
  4. 50,000.00 is transferred from savings to operating.
  5. FA-001's remaining life is extended by 12 months from 2026-02; FA-002 is sold on 2026-01-31 for 60,000.00.
- **Expected:** each step matches the acceptance criteria of the covered requirements (February depreciation of FA-001 1,489.36; gain on sale 1,666.67); the original books are unchanged.
- **Covers:** FIN-GL-004, FIN-AR-012, FIN-AP-008, FIN-AP-014, FIN-BK-002, FIN-FA-006, FIN-FA-007
