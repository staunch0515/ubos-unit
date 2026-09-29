---
id: FIN-README
title: US Basic Finance System — Requirements
status: frozen
---

# US Basic Finance System — Requirements

This folder specifies a **basic finance system for one US company**, following US GAAP and
common US compliance practice. It is written to be **implementation-neutral**: it does not
assume any platform, language, database or architecture. The same requirements are
implemented by two independent implementations, each in its own repository. Both are
accepted against the same dataset and scenarios and then compared
(`40-comparison-protocol.md`).

## Version

| Field | Value |
|---|---|
| Version | **1.0** (frozen 2026-09-29) |
| Git tag in the source repository | `fin-req-v1.0` |
| Change record | `99-change-log.md` |

This folder is self-contained: it has no link to any implementation and can be copied as a
whole into any repository. A copy is never edited where it is used; every change is made in
the source repository, gets a new version and tag, and is then copied again
(`99-change-log.md`).

The system is not sold. It is a complete, production-quality demonstration of a basic
finance system; market differentiation is not a goal.

## Scope in one table

| In scope | Out of scope |
|---|---|
| general ledger and chart of accounts; journals with approval; recurring and reversing entries | payroll calculation (payroll results are imported as summary journals) |
| fiscal calendar, periods, month-end and year-end close, governed reopen | inventory and costing |
| accounts receivable: customers, invoices, credit memos, receipts, statements, aging, write-offs | multi-entity and consolidation |
| accounts payable: vendors, bills, approvals, payment runs, ACH and check files, Form 1099 | budgeting and forecasting |
| cash and bank: bank accounts, statement import, matching, reconciliation, transfers | project and job costing |
| fixed assets: register, book depreciation, disposals | revenue recognition beyond simple point-in-time and ratable service (complex ASC 606) |
| multi-currency transactions and remeasurement (ASC 830) | lease accounting (ASC 842) |
| basic US state sales tax: rates, exemptions, returns data | purchase orders and receiving; tax filing services; income-tax return preparation |
| financial statements and management reports with drill-down | |
| audit trail, internal controls, access control, record retention | |

## How to read this folder

| File | Content |
|---|---|
| `00-conventions.md` | wording, IDs, requirement format, priorities, rounding, dates, glossary |
| `01-scope-and-context.md` | company profile, users and roles, compliance baseline, assumptions |
| `02-general-ledger.md` | chart of accounts, dimensions, journals, posting, approvals, recurring and reversing entries |
| `03-periods-and-close.md` | fiscal calendar, period states, close checklist, month-end, year-end, reopen |
| `04-receivables.md` | customers, invoices, credit memos, receipts, application, statements, aging, allowance, write-off |
| `05-payables.md` | vendors, tax identification, bills, approval, duplicate control, payment runs, Form 1099 |
| `06-cash-and-bank.md` | bank accounts, statement import, matching, reconciliation, transfers, payment files, positive pay |
| `07-fixed-assets.md` | asset register, capitalization, depreciation methods and conventions, disposals |
| `08-multi-currency.md` | currencies, rates, foreign-currency documents, revaluation, realized and unrealized gains and losses |
| `09-sales-tax.md` | tax codes and jurisdictions, exemptions, calculation, returns data |
| `10-financial-reporting.md` | trial balance, the four financial statements, subledger reports, drill-down, "as of" reporting |
| `11-controls-audit-security.md` | segregation of duties, approvals, audit trail, access control, SOX-style controls, retention |
| `12-data-and-integration.md` | import, export, migration, interfaces, data protection |
| `13-non-functional.md` | performance, availability, usability and user interface, accessibility, operations |
| `20-sample-company.md` | the sample company Northwind Components, Inc. and the dataset rules |
| `21-expected-results.md` | expected results of the dataset (generated) |
| `sample-company/*.csv` | the dataset in machine-readable form (generated) |
| `30-acceptance-scenarios.md` | shared acceptance scenarios FIN-SCN-01 … FIN-SCN-15 |
| `40-comparison-protocol.md` | how the two implementations are measured and compared |
| `90-traceability.md` | requirement → scenario coverage (generated) |
| `99-change-log.md` | versions, freeze rule and change procedure |
| `templates/*.csv` | empty work-item and defect record templates for the comparison protocol |
| `tools/` | generator of the dataset and expected results; checker and traceability generator |

## Maintenance

Run from this folder (Python 3.8 or later, standard library only):

```sh
sh tools/regen.sh
```

- `tools/gen_finance_expected.py` defines the dataset and generates the CSV files and the
  expected results.
- `tools/check_finance.py` checks this folder: unique IDs, resolvable references, complete
  requirement blocks, and coverage of every Must requirement by an acceptance scenario; it
  also generates `90-traceability.md`. It must report `errors: 0`.
- In a copy, running `sh tools/regen.sh` must leave every file unchanged; this proves the
  copy is intact.
- IDs are never renumbered or reused. A withdrawn requirement keeps its ID and is marked
  `**Status:** Withdrawn` with a reason.
- Each implementation maps these requirements to its own design in its own repository; this
  folder never refers to any implementation.
