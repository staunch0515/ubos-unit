---
id: FIN-DOC-11
title: Controls, Audit Trail and Security
status: frozen
---

# Controls, Audit Trail and Security

## 1. Internal controls

### FIN-CT-001 — Segregation of duties
- **Requirement:** The system shall enforce configurable segregation-of-duties rules, at least: the preparer of an entry, bill, payment run, write-off or reopen request cannot approve it; a user cannot both maintain vendor bank details and release payments; a user cannot both maintain users and roles and post transactions. It shall also report users whose role combinations conflict.
- **Priority:** Must
- **Basis:** SOX §404; COSO principle 10
- **Acceptance:**
  1. Given the accountant prepared JE-0002, when the accountant tries to approve it, then it is refused even if the accountant also holds the controller role.
  2. Given a user with both payables-clerk and treasurer roles, when the conflict report is run, then the user is listed.

### FIN-CT-002 — Approval rules are versioned
- **Requirement:** The system shall version approval rules and limits, record which version decided each approval requirement, and require controller approval for rule changes.
- **Priority:** Must
- **Basis:** SOX change control
- **Acceptance:**
  1. Given the threshold lowered from 10,000.00 to 5,000.00 effective 2026-03-01, when an entry of January is inspected, then its approval decision shows the 10,000.00 rule version.

### FIN-CT-003 — Approval integrity
- **Requirement:** The system shall bind every approval to the exact content approved; any later change to the content invalidates the approval.
- **Priority:** Must
- **Basis:** SOX
- **Acceptance:**
  1. Given an approved entry, when a line amount is changed before posting, then it needs approval again.

### FIN-CT-004 — Impact preview of rule changes
- **Requirement:** The system should show, before a change to an approval rule, account-mapping rule or tax rate is published, which past transactions of a chosen period would have been treated differently.
- **Priority:** Should
- **Basis:** change-management practice
- **Acceptance:**
  1. Given the threshold change to 5,000.00, when the preview is run on January, then it lists the manual entries between 5,000.00 and 10,000.00 that were posted without approval (none in the sample January data) and states the number of entries evaluated.

### FIN-CT-005 — Suspense and clearing accounts
- **Requirement:** The system shall flag suspense and clearing accounts and include "balance is zero" in the close checklist for them.
- **Priority:** Must
- **Basis:** close control
- **Acceptance:**
  1. Given an unapplied-cash balance at period end, then the close checklist shows the item as failing.

## 2. Audit trail

### FIN-CT-010 — Complete audit trail
- **Requirement:** The system shall record for every create, change, approval, rejection, posting, reversal, match, import, configuration change, login and permission change: the user (or system process), the time (UTC), the object, the before and after values and the reason where one is required. The audit trail shall be append-only and not changeable by any user including administrators.
- **Priority:** Must
- **Basis:** SOX; PCAOB AS 2201; IRS record keeping
- **Acceptance:**
  1. Given a vendor bank-detail change, when the audit trail is searched for V200, then old and new values (masked), the user, time and approver are shown.
  2. Given database-level access by an administrator, when an audit record is altered, then the tampering is detected by the integrity check (FIN-CT-011).

### FIN-CT-011 — Tamper evidence
- **Requirement:** The system shall make the ledger and audit trail tamper-evident (for example by hash chaining with periodic sealing) and provide a verification function that reports any altered or missing record.
- **Priority:** Must
- **Basis:** audit reliance on electronic records
- **Acceptance:**
  1. Given a posted line altered outside the application, when verification runs, then the altered line is identified.

### FIN-CT-012 — Audit evidence package
- **Requirement:** The system shall produce, for an audit request, a package of the requested reports, entries, approvals, access lists and supporting documents for a period, with a manifest and integrity data that the auditor can verify without access to the system.
- **Priority:** Should
- **Basis:** audit practice
- **Acceptance:**
  1. Given the request "all manual entries above 10,000.00 in January with approvals", then the package contains JE-0001 and JE-0002 with their approvals and a manifest the auditor verifies offline.

## 3. Access and security

### FIN-SC-001 — Authentication
- **Requirement:** The system shall authenticate users individually (no shared accounts), support single sign-on by SAML 2.0 or OpenID Connect, require multi-factor authentication for payment release, vendor bank changes and administration, and lock sessions after inactivity.
- **Priority:** Must
- **Basis:** SOX IT general controls; NIST SP 800-63B practice
- **Acceptance:**
  1. Given a treasurer releases a payment run, then a second factor is required.

### FIN-SC-002 — Role-based access
- **Requirement:** The system shall grant permissions only through roles (FIN-DOC-01 §2), limit approval rights by amount, and allow read-only roles for auditors limited to a period.
- **Priority:** Must
- **Basis:** least privilege
- **Acceptance:**
  1. Given ROLE-ExternalAuditor limited to fiscal 2026, when the auditor opens a 2025 entry, then access is refused.

### FIN-SC-003 — Access review
- **Requirement:** The system shall produce a user-access report (users, roles, permissions, last login, changes in period) for periodic review and record the reviewer's sign-off.
- **Priority:** Must
- **Basis:** SOX IT general controls
- **Acceptance:**
  1. Given a quarterly review, then the report lists every user with roles and the reviewer's sign-off is stored.

### FIN-SC-004 — Sensitive data protection
- **Requirement:** The system shall encrypt data in transit and at rest, mask TINs and bank account numbers except for authorised roles, log every display of unmasked sensitive data, and support deletion or anonymisation of personal data after the retention period.
- **Priority:** Must
- **Basis:** privacy practice; state privacy laws
- **Acceptance:**
  1. Given a clerk without tax-data permission, then TINs are masked in screens, exports and reports.

### FIN-SC-005 — Configuration change control
- **Requirement:** The system shall record changes to configuration (chart of accounts, tax rates, approval rules, report layouts, integrations) with the approver and allow promotion from a test configuration to production.
- **Priority:** Should
- **Basis:** SOX change management
- **Acceptance:**
  1. Given a new tax rate entered in test, when it is promoted, then the approver and time are recorded.

## 4. Record retention

### FIN-CT-020 — Retention
- **Requirement:** The system shall retain posted entries, subledger documents, attachments, reports issued, reconciliations, close artifacts and the audit trail for a configurable period (default 7 years after the fiscal year end), prevent their deletion before expiry, and support legal holds that suspend deletion.
- **Priority:** Must
- **Basis:** IRS Publication 583 and Rev. Proc. 98-25; audit practice
- **Acceptance:**
  1. Given a 2026 invoice, when a user tries to delete it in 2030, then it is refused.
  2. Given a legal hold on V200's records, when their retention expires, then they are kept until the hold is released.

### FIN-CT-021 — Readable archives
- **Requirement:** The system shall export the books of any closed fiscal year to an open, documented format (CSV or JSON with a schema, plus PDFs of issued reports) that can be read without the system.
- **Priority:** Must
- **Basis:** Rev. Proc. 98-25 (machine-sensible records)
- **Acceptance:**
  1. Given fiscal 2026 exported, when the trial balance is recomputed from the export with a spreadsheet, then it equals the system's trial balance.
