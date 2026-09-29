---
id: FIN-DOC-01
title: Scope and Context
status: frozen
---

# Scope and Context

## 1. Company profile

The system keeps the books of **one** US company:

- a C-corporation with a calendar fiscal year and monthly periods;
- functional and reporting currency USD; some customers and vendors invoice in foreign
  currencies;
- accrual basis of accounting under US GAAP;
- sells products and services to customers in several US states and abroad; buys goods and
  services from US vendors, some of whom are 1099-reportable;
- two to five bank accounts at US banks; payments by ACH and check;
- tens of fixed assets;
- a finance team of 3 to 10 people and an external CPA.

Volumes the system must handle are stated in FIN-NF-001.

Multi-company, consolidation and intercompany are out of scope. The data model must not
prevent a later extension to several companies, but no requirement tests it.

## 2. Roles

Roles are sets of permissions (FIN-SC-002). One person may hold several roles unless a
segregation-of-duties rule forbids the combination (FIN-CT-001).

| Role | Responsibility |
|---|---|
| ROLE-Controller | owns the books: chart of accounts, periods, close, approval of material entries, financial statements |
| ROLE-Accountant | prepares journal entries, reconciliations, accruals, depreciation and revaluation runs, close tasks |
| ROLE-ReceivablesClerk | maintains customers, issues invoices and credit memos, records and applies receipts |
| ROLE-PayablesClerk | maintains vendors, enters bills, prepares payment runs, maintains 1099 data |
| ROLE-Approver | approves bills, payments or entries within an approval limit |
| ROLE-Treasurer | releases payment runs to the bank, manages bank accounts and positive-pay files |
| ROLE-Executive | reads statements and dashboards |
| ROLE-ExternalAuditor | read-only access to the books, the audit trail and evidence for an audit period |
| ROLE-SystemAdministrator | manages users, roles, configuration and integrations; has no posting rights |

## 3. Compliance baseline

| Area | Baseline |
|---|---|
| Accounting | US GAAP for a private company: accrual basis, balanced double entry, period cut-off, statements per ASC 205/210/220/230/235 |
| Internal control | SOX §404-style controls as good practice: segregation of duties, approvals, immutable audit trail, access reviews, change control over configuration |
| Tax information | W-9 data, TIN validation status, Forms 1099-NEC and 1099-MISC data with the threshold of the tax year, backup-withholding flag |
| Sales tax | destination-based rates by jurisdiction, exemption certificates, data for state returns |
| Records | IRS record-retention: books and supporting records kept and reproducible for at least 7 years (configurable) |
| Privacy | personal data of individual customers and vendors (names, addresses, TINs, bank details) protected, masked and only disclosed to authorised roles |
| Accessibility | WCAG 2.2 AA for the user interface |

## 4. Assumptions

1. Payroll is calculated by an external provider; its results are imported as summary
   journals (FIN-DI-004).
2. Filing of sales tax returns and of Forms 1099 is done by the company or a service; the
   system produces the data and files needed (FIN-TX-008, FIN-AP-022).
3. Bank connectivity uses files (statements, payment files) or a bank API; the requirements
   are stated on the files (FIN-BK-003, FIN-BK-011).
4. The owner supplies current tax rates and thresholds as dated configuration.

## 5. Out of scope

Payroll calculation, inventory and costing, purchase orders and receiving, consolidation
and intercompany, budgeting and forecasting, project and job costing, complex revenue
recognition (multiple performance obligations, variable consideration), lease accounting
(ASC 842), income-tax return preparation, deferred-tax accounting beyond a manual journal,
and customer or vendor portals.
