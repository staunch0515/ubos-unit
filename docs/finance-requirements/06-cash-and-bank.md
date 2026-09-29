---
id: FIN-DOC-06
title: Cash and Bank
status: frozen
---

# Cash and Bank

### FIN-BK-001 — Bank accounts
- **Requirement:** The system shall maintain bank accounts with bank name, routing number, account number (masked), currency, the GL cash account, statement format, and payment-file settings; each bank account maps to exactly one GL account.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given the operating and savings accounts of the sample company, when they are set up, then each maps to 1010 and 1050 respectively and account numbers are masked for users without the treasury role.

### FIN-BK-002 — Bank transfers
- **Requirement:** The system shall record transfers between the company's bank accounts as one transaction that credits the source and debits the target, with in-transit handling when dates differ.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a transfer of 50,000.00 from savings to operating, when it is posted, then 1050 decreases and 1010 increases by 50,000.00 and both reconciliations can match it.

### FIN-BK-003 — Statement import
- **Requirement:** The system shall import bank statements in BAI2, ISO 20022 camt.053, and CSV with a configurable layout (and should import OFX), storing each statement line once even if a file is imported twice, and checking that opening balance plus lines equals closing balance.
- **Priority:** Must
- **Basis:** bank practice
- **Acceptance:**
  1. Given `bank-statement-2026-01.csv`, when it is imported, then 10 lines are stored and the closing balance 256,555.00 is verified.
  2. Given the same file imported again, when the import completes, then no line is duplicated and the user is told so.
  3. Given a file whose lines do not add up to its closing balance, when it is imported, then it is refused.

### FIN-BK-004 — Automatic matching
- **Requirement:** The system shall match statement lines to book transactions by configurable rules (amount and date window, reference, payee, batch total for payment runs), propose matches with a confidence and reason, and let the user accept them in bulk.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given the January statement and book transactions, when matching runs, then at least the eight lines whose amounts equal one book transaction within 3 days are proposed with the expected match of the CSV file.

### FIN-BK-005 — Manual matching and bank-originated entries
- **Requirement:** The system shall allow manual one-to-one, one-to-many and many-to-one matches, and creating entries from unmatched statement lines (fees, interest) with rules that pre-fill the account.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given the unmatched lines "ACCOUNT SERVICE FEE" 45.00 and "LOAN INTEREST" 300.00, when entries are created from them, then BANK-FEE-2601 and BANK-INT-2601 are posted and matched.

### FIN-BK-006 — Match history
- **Requirement:** The system shall keep matches immutable; undoing a match creates a new record, and the history shows both.
- **Priority:** Must
- **Basis:** audit trail
- **Acceptance:**
  1. Given a match undone and redone, when its history is shown, then both actions, users and times are listed.

### FIN-BK-007 — Reconciliation
- **Requirement:** The system shall reconcile a bank account for a statement date showing statement balance, outstanding deposits, outstanding payments and other reconciling items, adjusted bank balance, book balance and difference, and shall complete the reconciliation only when the difference is zero.
- **Priority:** Must
- **Basis:** COSO control activity; audit
- **Acceptance:**
  1. Given January matching is complete, when the reconciliation at 2026-01-31 is produced, then it equals FIN-EXP-10 with PAYROLL-2601 (45,000.00) outstanding and difference 0.00.
  2. Given a difference of 10.00, when the accountant tries to complete it, then it is refused.

### FIN-BK-008 — Reconciliation report and sign-off
- **Requirement:** The system shall store the completed reconciliation with preparer and reviewer sign-off (different persons), and reproduce it later exactly, even after later transactions or matches.
- **Priority:** Must
- **Basis:** SOX evidence
- **Acceptance:**
  1. Given the January reconciliation signed off on 2026-02-03, when it is reprinted in March, then it is identical.

### FIN-BK-009 — Outstanding items carried forward
- **Requirement:** The system shall carry unmatched book items forward to the next reconciliation and flag checks outstanding longer than a configurable number of days (default 90) for follow-up and escheatment review.
- **Priority:** Should
- **Basis:** unclaimed-property practice
- **Acceptance:**
  1. Given a check outstanding for 95 days, when the stale-check report is run, then it is listed.

### FIN-BK-010 — Cash position
- **Requirement:** The system should show the current book balance and last statement balance of every bank account and the expected receipts and payments of the next 30 days from open invoices and approved bills.
- **Priority:** Should
- **Basis:** treasury practice
- **Acceptance:**
  1. Given the sample data on 2026-01-31, when the cash position is shown, then 1010 shows book 211,555.00 and statement 256,555.00, and expected payments include P-7902 22,000.00 due 2026-02-08.

### FIN-BK-011 — Payment files and positive pay
- **Requirement:** The system shall generate payment files for payment runs (NACHA, pain.001, check print or check file) and positive-pay files for issued checks (check number, date, amount, payee), store every generated file immutably with its hash, and prevent generating a second file for the same run unless the first is cancelled with a reason.
- **Priority:** Must
- **Basis:** NACHA Operating Rules; fraud prevention
- **Acceptance:**
  1. Given PAY-RUN-02, when the file is generated twice without cancelling, then the second attempt is refused.
  2. Given a check run, when the positive-pay file is produced, then it lists each check with number, date, amount and payee.
