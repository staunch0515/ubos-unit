---
id: FIN-DOC-12
title: Data and Integration
status: frozen
---

# Data and Integration

### FIN-DI-001 — Master-data import
- **Requirement:** The system shall import the chart of accounts, customers, vendors, tax codes, exchange rates and fixed assets from CSV or spreadsheet files with a mapping step, validation of every row before any change, a preview of the changes, and an import report.
- **Priority:** Must
- **Basis:** practice
- **Acceptance:**
  1. Given the sample-company CSV files, when they are imported in order, then all master data of `20-sample-company.md` exist and the import report shows 0 rejected rows.
  2. Given a vendor row with an invalid state code, then the whole file is rejected with the row and reason, and nothing changes.

### FIN-DI-002 — Migration of open items and history
- **Requirement:** The system shall migrate opening balances, open receivables and payables, fixed assets with accumulated depreciation, outstanding bank items and, optionally, historical journal entries from a legacy system's extracts, and produce a reconciliation report of control totals (trial balance per month, open items, asset values) against the source.
- **Priority:** Must
- **Basis:** migration practice; audit
- **Acceptance:**
  1. Given the opening files, when migration completes, then FIN-EXP-01 is reproduced and every control total matches the source with 0.00 difference.

### FIN-DI-003 — Data-quality decisions
- **Requirement:** The system shall let the migrating user resolve data-quality problems (unknown accounts, duplicate customers, unbalanced batches) by recorded decisions that are part of the migration report.
- **Priority:** Should
- **Basis:** audit of migrations
- **Acceptance:**
  1. Given two legacy customers that are the same company, when they are merged during migration, then the decision, user and time are in the migration report.

### FIN-DI-004 — Payroll journal import
- **Requirement:** The system shall import payroll results from an external provider as summary journals with a configurable mapping of provider codes to accounts and departments.
- **Priority:** Must
- **Basis:** scope (payroll excluded)
- **Acceptance:**
  1. Given the provider file for January, when it is imported, then PAYROLL-2601 is created as in FIN-EXP-02.

### FIN-DI-005 — Application programming interface
- **Requirement:** The system shall provide a documented, authenticated API to read master data, balances and reports and to create draft documents (invoices, bills, journal entries, receipts) that follow the same validation, approval and posting rules as the user interface.
- **Priority:** Must
- **Basis:** integration practice
- **Acceptance:**
  1. Given an API client creates a bill above the approval limit, then the bill waits for approval exactly as one entered in the user interface.
  2. Given an API client without posting permission, when it tries to post, then it is refused.

### FIN-DI-006 — Idempotent interfaces
- **Requirement:** The system shall accept an idempotency key or external reference on API calls and imports so that a retried request does not create a duplicate document.
- **Priority:** Must
- **Basis:** reliability
- **Acceptance:**
  1. Given the same invoice request sent twice with the same key, then one invoice exists.

### FIN-DI-007 — Events for integration
- **Requirement:** The system should notify subscribed systems of posted documents, approvals and period closes by webhooks or messages with retry.
- **Priority:** Should
- **Basis:** integration practice
- **Acceptance:**
  1. Given a subscription to "invoice posted", when INV-1004 is posted, then one notification with its number and total is delivered.

### FIN-DI-008 — Full data export
- **Requirement:** The system shall export all company data (master data, documents, entries, attachments, audit trail) in open formats at any time by an authorised user.
- **Priority:** Must
- **Basis:** data portability; FIN-CT-021
- **Acceptance:**
  1. Given a full export, when the number of posted lines is counted, then it equals the number in the system.

### FIN-DI-009 — Notifications
- **Requirement:** The system shall notify users of tasks assigned to them (approvals, close tasks, exceptions) in the application and by e-mail, with a link to the task.
- **Priority:** Must
- **Basis:** practice
- **Acceptance:**
  1. Given JE-0002 submitted, then the controller receives an in-app task and an e-mail linking to it.
