---
id: FIN-DOC-05
title: Accounts Payable and Form 1099
status: frozen
---

# Accounts Payable and Form 1099

## 1. Vendors and bills

### FIN-AP-001 — Vendor master
- **Requirement:** The system shall maintain vendors with a unique ID, legal name, "doing business as" name, remit-to addresses, contacts, currency, payment terms, default expense account, payment method and bank details, entity type, 1099 status and status, keeping the history of changes.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given `vendors.csv`, when it is loaded, then the eight vendors exist with their entity types and 1099 settings.

### FIN-AP-002 — Tax identification (Form W-9)
- **Requirement:** The system shall store each vendor's TIN (EIN or SSN), TIN type, the W-9 document and its date, the TIN-verification status, and a backup-withholding flag; it shall mask TINs for users without the tax-data permission.
- **Priority:** Must
- **Basis:** IRS Form W-9; 26 U.S.C. 3406; privacy
- **Acceptance:**
  1. Given V800 is an individual, when a payables clerk without the tax-data permission opens the vendor, then the SSN is shown as `***-**-1234`.
  2. Given a 1099 vendor without a TIN on file, when a bill is approved, then the system warns that backup withholding may apply.

### FIN-AP-003 — Bank-detail change control
- **Requirement:** The system shall require approval by a second person for any change to a vendor's bank details, shall hold payments to the new details until approved, and shall log the change with old and new values (masked).
- **Priority:** Must
- **Basis:** fraud prevention (business e-mail compromise); SOX
- **Acceptance:**
  1. Given a clerk changes V200's bank account, when a payment run is prepared before approval, then V200's bill is held with the reason "bank details pending approval".

### FIN-AP-004 — Vendor bill
- **Requirement:** The system shall record bills with vendor, vendor invoice number, invoice date, received date, due date, currency, terms, lines (description, amount, expense or asset account, tax, dimensions, 1099 box) and attachments, and on posting debit expense or asset accounts and credit payables.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given BILL-DC-2601 from V200 for 7,500.00, when it is posted, then 6400 is debited, 2000 credited and the 1099 box defaults to 1099-NEC box 1.

### FIN-AP-005 — Duplicate bill control
- **Requirement:** The system shall block a bill whose vendor and vendor invoice number match an existing bill, and shall warn when vendor, amount and invoice date match an existing bill with a different number.
- **Priority:** Must
- **Basis:** fraud and error prevention
- **Acceptance:**
  1. Given P-7902 from V100 is posted, when the same vendor invoice number is entered again, then the second bill is refused.
  2. Given a bill from V100 with a different number but the same amount and date, when it is entered, then a warning requires confirmation with a reason.

### FIN-AP-006 — Bill approval
- **Requirement:** The system shall route bills for approval by configurable rules (amount, vendor, account, dimension) and approval limits per approver, require approval before a bill can be paid, and keep the approval history.
- **Priority:** Must
- **Basis:** SOX; accepted practice
- **Acceptance:**
  1. Given the rule "bills above 10,000.00 need controller approval", when P-7902 (22,000.00) is posted, then it cannot be selected for payment until the controller approves.

### FIN-AP-007 — Capitalization from bills
- **Requirement:** The system shall allow a bill line coded to a fixed-asset cost account to create or update a fixed asset in the register (FIN-FA-002).
- **Priority:** Must
- **Basis:** ASC 360
- **Acceptance:**
  1. Given BILL-TS-5520 coded to 1520, when it is posted, then asset FA-003 exists with cost 12,000.00 and a link to the bill.

### FIN-AP-008 — Vendor credits and prepayments
- **Requirement:** The system shall record vendor credit memos and prepayments and apply them to bills.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a vendor credit of 500.00, when it is applied to a bill of 1,200.00, then 700.00 remains payable.

### FIN-AP-009 — Payables aging
- **Requirement:** The system shall produce the payables aging as of any date by due date, by vendor and in total, and the total shall equal the payables control account at that date.
- **Priority:** Must
- **Basis:** audit; cash planning
- **Acceptance:**
  1. Given the sample data, when the aging at 2026-01-31 is run, then it equals FIN-EXP-09 and totals 46,300.00.

## 2. Payments

### FIN-AP-010 — Payment run proposal
- **Requirement:** The system shall propose a payment run from approved, unheld, open bills selected by due date, discount date, vendor, currency and bank account, and allow the preparer to add or remove bills before submission.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given approved bills due up to 2026-01-20, when the run is proposed on 2026-01-08, then P-7781 and CPL-1225 (32,300.00) are proposed.

### FIN-AP-011 — Payment approval and release
- **Requirement:** The system shall require approval of a payment run by a person other than its preparer and release to the bank by a user with the treasury role; the run shall be locked after approval.
- **Priority:** Must
- **Basis:** segregation of duties (FIN-CT-001)
- **Acceptance:**
  1. Given PAY-RUN-01 prepared by the payables clerk, when the clerk tries to approve it, then it is refused.

### FIN-AP-012 — Payment posting
- **Requirement:** The system shall post each payment by debiting payables (and discounts taken) and crediting the bank account, mark the paid bills, and link each payment to the bills it pays.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given PAY-RUN-02 is released, then it posts 19,000.00 and each of DC-2025-12, MP-2026-01 and JR-014 shows paid with a link to the payment.

### FIN-AP-013 — Payment methods and files
- **Requirement:** The system shall pay by ACH (NACHA CCD or PPD file, or ISO 20022 pain.001), by check (printed check or check file with positive-pay file) and by wire (instruction record), and shall generate the file for the run (FIN-BK-011).
- **Priority:** Must
- **Basis:** NACHA Operating Rules; bank practice
- **Acceptance:**
  1. Given PAY-RUN-01 by ACH, when the file is generated, then it passes a NACHA file validator, its batch total equals 32,300.00 and its entry count is 2.

### FIN-AP-014 — Voids and stopped payments
- **Requirement:** The system shall void a payment (for example a stopped check) by a reversing entry dated in an open period, reopening the paid bills, and shall keep the original payment visible.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a check payment voided on 2026-02-10, then its bills are open again and the bank account is debited on 2026-02-10.

### FIN-AP-015 — Manual payments and non-vendor payments
- **Requirement:** The system shall record payments made outside a run (for example a tax payment to a state authority) with the same approval rules.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given STX-PAY-2512 to the Texas Comptroller, when it is posted, then sales tax payable is debited 3,300.00 and cash credited.

## 3. Form 1099

### FIN-AP-020 — 1099 classification
- **Requirement:** The system shall classify 1099-reportable vendors by form and box (at least 1099-NEC box 1 non-employee compensation and 1099-MISC box 1 rents, box 3 other income, box 10 gross proceeds to an attorney), allow per-line overrides on bills, and exclude payments by card or third-party network (reported on Form 1099-K by the processor).
- **Priority:** Must
- **Basis:** 26 U.S.C. 6041 and 6041A; IRS instructions for Forms 1099-NEC and 1099-MISC
- **Acceptance:**
  1. Given V300 set to 1099-MISC box 1, when PAY-RUN-02 pays MP-2026-01, then 8,500.00 is recorded as rents.
  2. Given a payment to V200 by credit card, then it is excluded from the 1099 amount.

### FIN-AP-021 — Reportable amounts and thresholds
- **Requirement:** The system shall compute reportable amounts per vendor, form and box from payments made in the calendar year (cash basis), and compare them with the threshold of the tax year read from a dated table.
- **Priority:** Must
- **Basis:** IRS instructions (thresholds change by tax year)
- **Acceptance:**
  1. Given the sample data and the 2026 threshold of 2,000.00, when the 1099 report is run for 2026, then it equals FIN-EXP-14: V200 9,000.00 and V300 8,500.00 reportable, V800 1,500.00 below threshold.
  2. Given the threshold table changed to 600.00 for a test year, when the report is re-run for that year, then V800 becomes reportable without any code change.

### FIN-AP-022 — 1099 outputs
- **Requirement:** The system shall produce, per tax year, a 1099 review report with missing or invalid TINs and addresses, recipient copies (PDF), and an export for electronic filing in the format of the IRS e-filing system or a filing service, including state amounts where a state requires them.
- **Priority:** Must
- **Basis:** IRS information-return filing
- **Acceptance:**
  1. Given a 1099 vendor without a TIN, when the review report is run, then the vendor is listed as an exception.
  2. Given the 2026 report, when the export is produced, then it contains one record per reportable vendor, form and box with the amounts of the report.

### FIN-AP-023 — 1099 corrections
- **Requirement:** The system should record corrections to a filed 1099 (changed amount or TIN) and produce corrected-return data.
- **Priority:** Should
- **Basis:** IRS corrected returns
- **Acceptance:**
  1. Given a filed amount of 9,000.00 later corrected to 9,500.00, when the correction export is produced, then it contains the corrected record marked as a correction.
