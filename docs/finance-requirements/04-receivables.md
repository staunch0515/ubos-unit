---
id: FIN-DOC-04
title: Accounts Receivable
status: frozen
---

# Accounts Receivable

### FIN-AR-001 — Customer master
- **Requirement:** The system shall maintain customers with a unique ID, legal name, billing and shipping addresses, contacts, currency, payment terms, credit limit, default tax code, exemption certificates, and status, keeping the history of changes.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given `customers.csv`, when it is loaded, then the four customers exist with their currency, terms and tax codes.
  2. Given a customer's address changed on 2026-02-01, when an invoice dated 2026-01-15 is reprinted, then it shows the address valid on the invoice date.

### FIN-AR-002 — Payment terms
- **Requirement:** The system shall support payment terms with net days, optional early-payment discount (for example 2/10 net 30), and end-of-month variants, and compute due dates and discount dates from the invoice date.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given terms net 30 and invoice date 2026-01-06, then the due date is 2026-02-05.
  2. Given terms 2/10 net 30, when a receipt arrives within 10 days, then the discount is offered and posted to a sales-discount account if taken.

### FIN-AR-003 — Customer invoice
- **Requirement:** The system shall create invoices with header (customer, invoice date, due date, currency, terms, ship-to for tax) and lines (description, quantity, unit price, revenue account, tax code, dimensions), compute tax (FIN-TX-003) and totals, and on posting debit receivables and credit revenue and sales tax payable.
- **Priority:** Must
- **Basis:** ASC 606 (point-in-time sale of goods and simple services); accepted practice
- **Acceptance:**
  1. Given INV-1004 with components 40,000.00 (TX-AUSTIN) and services 10,000.00 (NT), when it is posted, then tax is 3,300.00, the total is 53,300.00 and the GL lines equal FIN-EXP-02.

### FIN-AR-004 — Invoice numbering and immutability
- **Requirement:** The system shall number posted invoices and credit memos in a gap-free sequence and shall not change a posted invoice; corrections are made by credit memo or by voiding with a reversing entry dated in an open period.
- **Priority:** Must
- **Basis:** audit completeness
- **Acceptance:**
  1. Given a posted invoice, when a clerk tries to change a line amount, then it is refused and the credit-memo action is offered.

### FIN-AR-005 — Invoice document
- **Requirement:** The system shall produce a printable and e-mailable invoice document (PDF) with company details, customer, lines, tax per jurisdiction, totals, terms, due date and remittance instructions, and shall keep an immutable copy of each document sent.
- **Priority:** Must
- **Basis:** accepted practice; record keeping
- **Acceptance:**
  1. Given INV-1004 was e-mailed, when it is re-printed later, then the stored copy is identical to the one sent.

### FIN-AR-006 — Credit memos
- **Requirement:** The system shall issue credit memos referencing an invoice or standing alone, reverse revenue (or post to a returns-and-allowances account) and tax proportionally, and apply them to open invoices or refund them.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given CM-2001 for 2,000.00 of returned components on INV-1004, when it is posted and applied, then tax of 165.00 is reversed and the open amount of INV-1004 is 51,135.00.

### FIN-AR-007 — Cash receipts
- **Requirement:** The system shall record customer receipts (check, ACH, wire, card settlement) with date, amount, method, reference and deposit bank account, and post them to cash and receivables or to unapplied cash.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given RCPT-0003 of 20,000.00 from C300, when it is posted, then cash increases by 20,000.00 and it is applied to INV-1003, leaving 10,000.00 open.

### FIN-AR-008 — Application of receipts and credits
- **Requirement:** The system shall apply receipts and credits to one or more open invoices, fully or partially, with suggestions by reference and amount, allow unapplied balances, and allow unapplying and reapplying with full history.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a receipt applied to the wrong customer, when it is unapplied and applied to the right one, then both actions appear in the history and the aging "as of" a date before the correction is reproducible.

### FIN-AR-009 — Customer statements
- **Requirement:** The system shall produce customer statements for a period with opening balance, invoices, credits, receipts and closing balance, and an open-item variant with aging.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given C300 at 2026-01-31, when the open-item statement is produced, then INV-1003 (10,000.00, 1–30 days past due) and INV-1007 (25,000.00, current) are listed.

### FIN-AR-010 — Receivables aging
- **Requirement:** The system shall produce the receivables aging as of any date by due date or by invoice date, in configurable buckets (default current, 1–30, 31–60, 61–90, over 90), by customer and in total, and the total shall equal the receivables control account at that date.
- **Priority:** Must
- **Basis:** ASC 310 and 326 support; audit
- **Acceptance:**
  1. Given the sample data, when the aging at 2026-01-31 is run by due date, then it equals FIN-EXP-08 and totals 158,735.00.
  2. Given a later receipt, when the aging at 2026-01-31 is re-run, then it is unchanged.

### FIN-AR-011 — Allowance for credit losses
- **Requirement:** The system shall support recording the allowance for doubtful accounts by journal entry, and should compute a suggested allowance from configurable loss rates by aging bucket.
- **Priority:** Should
- **Basis:** ASC 326 (current expected credit losses, simplified for trade receivables)
- **Acceptance:**
  1. Given loss rates 1% current, 5% 1–30 days, when the suggestion is run at 2026-01-31, then it computes 1% × 148,735.00 + 5% × 10,000.00 = 1,987.35 and shows the adjustment against the existing allowance of 4,000.00.

### FIN-AR-012 — Write-off and recovery
- **Requirement:** The system shall write off uncollectible invoices against the allowance with approval, and record recoveries of written-off amounts.
- **Priority:** Must
- **Basis:** ASC 326; accepted practice
- **Acceptance:**
  1. Given an invoice of 500.00 approved for write-off, when it is posted, then the allowance is debited, receivables credited, and the invoice shows status written off.

### FIN-AR-013 — Credit limit check
- **Requirement:** The system should warn when a new invoice would take a customer above their credit limit and should allow a controller to require approval in that case.
- **Priority:** Should
- **Basis:** credit control practice
- **Acceptance:**
  1. Given a credit limit of 100,000.00 and open items of 95,000.00, when an invoice of 10,000.00 is posted, then a warning or approval requirement is raised as configured.

### FIN-AR-014 — Recurring invoices
- **Requirement:** The system should generate recurring invoices (for example monthly service fees) from templates with a schedule.
- **Priority:** Should
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a monthly template for C200, when the February run is executed twice, then one invoice is created.

### FIN-AR-015 — Deferred service revenue (simple)
- **Requirement:** The system should allow an invoice line for a service period to be recognised ratably over the months of the service period through a schedule that posts from deferred revenue to revenue.
- **Priority:** Could
- **Basis:** ASC 606 (single performance obligation satisfied over time)
- **Acceptance:**
  1. Given a 12,000.00 annual support invoice from 2026-01-01 to 2026-12-31, when each month's run is executed, then 1,000.00 moves from deferred revenue to revenue per month.
