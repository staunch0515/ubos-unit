---
id: FIN-DOC-02
title: General Ledger
status: frozen
---

# General Ledger

## 1. Chart of accounts

### FIN-GL-001 — Account definition
- **Requirement:** The system shall maintain a chart of accounts in which each account has a unique code, a name, an account type (asset, liability, equity, revenue, expense, other income or expense, income tax), a normal balance (debit or credit), a financial-statement classification line, a status (active or inactive) and an optional parent for grouping.
- **Priority:** Must
- **Basis:** ASC 210 and 220 presentation; accepted practice
- **Acceptance:**
  1. Given the sample company's `chart-of-accounts.csv`, when it is loaded, then all 36 accounts exist with the given attributes.
  2. Given an existing code, when a second account with the same code is created, then the system refuses it.

### FIN-GL-002 — Standard chart template
- **Requirement:** The system shall offer a standard US chart-of-accounts template for a small commercial company that a controller can copy and adapt when setting up the books.
- **Priority:** Should
- **Basis:** accepted practice
- **Acceptance:**
  1. Given new books, when the controller selects the template, then a proposed chart is shown for review before it is saved.

### FIN-GL-003 — Account hierarchy and roll-up
- **Requirement:** The system shall allow summary accounts that group postable accounts, and shall roll up balances along the hierarchy in reports. Only postable (leaf) accounts accept postings.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a summary account with two children, when the trial balance is run with roll-up, then the summary shows the sum of its children.
  2. Given a summary account, when a line is posted to it, then the posting is refused.

### FIN-GL-004 — Changes to accounts
- **Requirement:** The system shall keep the history of every change to an account (name, classification, status, parent) with who and when. An account with postings shall not be deleted; it may be made inactive, after which it accepts no new postings but remains in reports.
- **Priority:** Must
- **Basis:** SOX change control; audit trail
- **Acceptance:**
  1. Given an account with postings, when a user deletes it, then the system refuses and offers deactivation.
  2. Given an account renamed on March 3, when the account history is shown, then the old and new names, the user and the time are listed.
  3. Given an inactive account, when a line is posted to it, then the posting is refused.

### FIN-GL-005 — Control accounts
- **Requirement:** The system shall mark accounts receivable, accounts payable, fixed-asset cost and accumulated depreciation, and bank accounts as control accounts that only their subledger may post to. A manual journal line to a control account shall be refused unless the controller has granted an explicit, logged exception for that entry.
- **Priority:** Must
- **Basis:** subledger integrity; COSO control activities
- **Acceptance:**
  1. Given account 1200, when an accountant posts a manual journal line to it, then the posting is refused with the reason.
  2. At any date, the balance of each control account equals the total of its subledger (FIN-RP-007).

### FIN-GL-006 — Dimensions
- **Requirement:** The system should support at least two optional analysis dimensions on journal lines (for example department and location) with validation lists and reporting by dimension.
- **Priority:** Should
- **Basis:** management reporting practice
- **Acceptance:**
  1. Given lines tagged with departments, when the income statement is run by department, then the department columns add up to the total.

## 2. Journal entries and posting

### FIN-GL-010 — Entry structure
- **Requirement:** The system shall record a journal entry with a unique number, a posting date, a document date, a description, a source (manual, receivables, payables, bank, fixed assets, revaluation, import, closing), optional attachments and two or more lines, each with an account, a debit or credit amount, an optional memo and optional dimensions.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given JE-0002 of the sample company, when it is displayed, then all header fields and both lines appear with their amounts.

### FIN-GL-011 — Balanced entries
- **Requirement:** The system shall post an entry only if its total debits equal its total credits in the functional currency, and for foreign-currency entries also in the transaction currency.
- **Priority:** Must
- **Basis:** double-entry bookkeeping
- **Acceptance:**
  1. Given an entry with debits 25,000.00 and credits 24,999.99, when it is posted, then it is refused and the difference 0.01 is shown.
  2. At every date, the sum of all posted debits equals the sum of all posted credits.

### FIN-GL-012 — Immutability of posted entries
- **Requirement:** The system shall never change or delete a posted entry or line. A correction shall be made by a reversal entry or an adjusting entry that references the original.
- **Priority:** Must
- **Basis:** audit trail; ASC 250 error corrections; SOX
- **Acceptance:**
  1. Given a posted entry, when any user (including an administrator) tries to change an amount, account or date through the user interface or an interface, then the change is refused.
  2. Given a posted entry, when it is reversed, then the reversal has the opposite amounts, references the original, and both remain visible.

### FIN-GL-013 — Gap-free numbering
- **Requirement:** The system shall number posted entries in a gap-free sequence per fiscal year and source. Drafts and rejected entries shall not consume posted numbers.
- **Priority:** Must
- **Basis:** audit expectations for completeness
- **Acceptance:**
  1. Given the posted entries of January, when their numbers are listed per source, then there are no gaps and no duplicates.
  2. Given a draft that is deleted, when the next entry is posted, then it receives the next number without a gap.

### FIN-GL-014 — Entry lifecycle
- **Requirement:** The system shall support the entry states draft, submitted, approved, posted and rejected. Drafts may be edited or deleted by their preparer. A submitted entry is locked; any change returns it to draft and invalidates prior approvals.
- **Priority:** Must
- **Basis:** maker–checker control
- **Acceptance:**
  1. Given a submitted entry, when the preparer changes an amount, then it returns to draft and its approval task is withdrawn.
  2. Given an approved entry, when it is changed before posting, then the approval is invalidated (FIN-CT-003).

### FIN-GL-015 — Approval of manual entries
- **Requirement:** The system shall route manual journal entries for approval according to configurable rules (for example amount above a threshold, accounts touched, entries after period end), require an approver other than the preparer, and post the entry only after approval.
- **Priority:** Must
- **Basis:** SOX §404 control over manual journal entries
- **Acceptance:**
  1. Given the rule "manual entries above 10,000.00 need controller approval", when JE-0002 (25,000.00) is submitted, then it waits for a controller's approval and posts only after it.
  2. Given an entry of 1,000.00, when it is submitted, then it posts without approval, and the audit trail records that no rule required approval and which rule version was evaluated.
  3. Given the preparer holds the controller role, when they try to approve their own entry, then the system refuses (FIN-CT-001).

### FIN-GL-016 — Supporting documents
- **Requirement:** The system shall allow attaching supporting documents (PDF, images, spreadsheets) to entries and subledger documents, keep them immutable after posting, and show them with the entry.
- **Priority:** Must
- **Basis:** audit evidence; IRS record keeping
- **Acceptance:**
  1. Given a posted entry with an attached PDF, when an auditor opens the entry, then the PDF is available and its content is unchanged since posting (content hash shown).

### FIN-GL-017 — Recurring entries
- **Requirement:** The system shall generate recurring entries from templates with a schedule, fixed or formula amounts, start and end dates, and the same approval rules as manual entries.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given a monthly template "amortize prepaid insurance 1,000.00", when the January run is executed, then JE-0003 is created once, and a second run for January creates nothing.

### FIN-GL-018 — Automatic reversing entries
- **Requirement:** The system shall allow marking an entry as auto-reversing on a given date (by default the first day of the next period) and shall post the reversal automatically on that date.
- **Priority:** Must
- **Basis:** accrual accounting practice
- **Acceptance:**
  1. Given FXR-2601 marked auto-reversing, when February 1 is opened, then FXR-2601-R is posted with opposite amounts and a reference to FXR-2601.

### FIN-GL-019 — Journal import
- **Requirement:** The system shall import journal entries from a spreadsheet or CSV file, validate all lines before posting anything, report errors per row, and apply the same approval rules as manual entries.
- **Priority:** Must
- **Basis:** accepted practice; payroll summary imports
- **Acceptance:**
  1. Given a file with one unbalanced entry and one invalid account, when it is imported, then nothing is posted and both rows are reported with reasons.
  2. Given PAYROLL-2601 as a CSV file, when it is imported, then one balanced entry with four lines is created.

### FIN-GL-020 — Bulk journal entry grid
- **Requirement:** The system shall provide a spreadsheet-like grid for entering and editing entries with many lines by keyboard, including paste from a spreadsheet, running debit and credit totals, and inline validation.
- **Priority:** Must
- **Basis:** usability for accountants (FIN-UI-003)
- **Acceptance:**
  1. Given a 50-line journal copied from a spreadsheet, when it is pasted into the grid, then all lines appear, invalid cells are highlighted, and the totals show the difference until the entry balances.

### FIN-GL-021 — Posting from subledgers
- **Requirement:** The system shall post receivables, payables, bank, fixed-asset, revaluation and closing transactions to the general ledger automatically and at the time the subledger transaction is posted, with a link from each GL line to its source document.
- **Priority:** Must
- **Basis:** subledger integrity
- **Acceptance:**
  1. Given INV-1004 is posted, when the general ledger is viewed, then four lines exist with the amounts of FIN-EXP-02 and each opens INV-1004.

### FIN-GL-022 — Account inquiry
- **Requirement:** The system shall show for any account and date range the opening balance, each posted line with its source and running balance, and the closing balance.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given account 1010 for January, when the inquiry is run, then the opening balance is 250,000.00, the closing balance is 211,555.00, and every line opens its source document.
