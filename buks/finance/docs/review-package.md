# Review package for the accounting reviewer — finance Buk, PH-2 step 10 (DEC-156)

This package asks the accounting reviewer (MET-BIZ-003, EXIT-2-16) to confirm two things:

1. that the **expected results** of the sample company — `docs/finance-requirements/21-expected-results.md`,
   sections FIN-EXP-01…17 of fin-req-v1.0 — are correct US GAAP accounting for the transactions of
   `docs/finance-requirements/sample-company/`;
2. that the **finance Buk** produces exactly those results, and that the accounting choices it made
   where the requirements leave room (section 4) are acceptable.

No knowledge of the system is needed to review sections 1, 3 and 4. Section 2 says how a
technical colleague reproduces the results.

## 1. What was compared

The sample company is Northwind Components, Inc., a US C corporation with a calendar fiscal year,
from its opening balances at 2025-12-31 through January and February 2026: invoices and credit
memos with Texas and Oregon sales tax, a euro invoice, receipts, bills with Form 1099 vendors,
payment runs, payroll, accruals, depreciation of three assets, a bank reconciliation, the January
close, February items and a back-dated correction.

`docs/tie-out.md` lists the 77 checks and their 2,169 compared figures; all are equal on both
storage engines. Every expected-result section is compared twice: once from the journal lines of
the dataset, and once with every document entered as a clerk would — through the documents,
approvals, runs, the reconciliation and the close (`docs/step-10-record.md`, section 1).

`docs/fin-coverage.md` shows, for each of the 153 requirements, how it is met; no Must
requirement is open in the scope of this step.

## 2. How the results are reproduced

From the repository root, with Rust and PostgreSQL 16:

```
cargo build --release -p ubos-forge
./target/release/forge test buks/finance
./target/release/forge test buks/finance --backend postgres --database "host=127.0.0.1 user=postgres dbname=bpa"
python3 buks/finance/tools/tieout_report.py --database "host=127.0.0.1 user=postgres dbname=bpa"
```

The tests are generated from the frozen requirements by `buks/finance/tools/gen_tests.py`; no
expected figure is typed by hand into a test.

## 3. Results to confirm

| Section | Content | Result |
|---|---|---|
| FIN-EXP-01 | opening trial balance at 2025-12-31 | equal |
| FIN-EXP-02 | January postings, line by line | equal |
| FIN-EXP-03 | trial balance at 2026-01-31 before closing entries; the January close artefact | equal |
| FIN-EXP-04 | income statement, January | equal |
| FIN-EXP-05 | balance sheet at 2026-01-31 | equal |
| FIN-EXP-06 | statement of cash flows (indirect method), January | equal |
| FIN-EXP-07 | statement of stockholders' equity, January | equal |
| FIN-EXP-08 | receivables aging at 2026-01-31, INV-1005 at its revalued amount | equal |
| FIN-EXP-09 | payables aging at 2026-01-31 | equal |
| FIN-EXP-10 | bank reconciliation of account 1010, January | equal |
| FIN-EXP-11 | fixed-asset register and January depreciation | equal |
| FIN-EXP-12 | INV-1005 in euros: invoice, revaluation, reversal, receipt | equal |
| FIN-EXP-13 | sales tax report, January (with the state and local parts of TX-AUSTIN) | equal |
| FIN-EXP-14 | Form 1099 summary 2026 to 2026-01-31 | equal |
| FIN-EXP-15 | February postings after the January close | equal |
| FIN-EXP-16 | balances at 2026-02-05 as known on 2026-02-05 and on 2026-02-28 | equal |
| FIN-EXP-17 | selected balances at 2026-02-28 | equal |

## 4. Accounting choices to confirm

Where the requirements leave a choice, the Buk made the following ones. Each is a rule of the
Buk's data or logic and can be changed without changing the platform.

| Topic | Choice | Requirement |
|---|---|---|
| Sales tax rounding | per invoice and tax code on the sum of the taxable lines, half away from zero to the cent | FIN-TX-003 |
| Combined tax codes | the tax of a code is allocated to its jurisdictions in proportion to their rates, the rest to the last | FIN-TX-001 |
| Rate changes | a document is taxed at the rate valid on its date; a credit memo at the rate of the invoice it credits | FIN-TX-001, -005 |
| Foreign currency | spot rate of the document date; conversion rounded half to even to the cent; period-end remeasurement of open items at the closing rate, reversed on the first day of the next period; the realized gain or loss measured against the carrying amount (after the reversal, the invoice-date amount) | FIN-FX-003…005 |
| Depreciation | per asset year: straight line, or double-declining or 150% declining balance with the switch to straight line; the month's amount is the difference of cumulative amounts rounded half away from zero, so rounding residues fall in the last month of each asset year; full-month convention unless the class says mid-month or next-month | FIN-FA-003, -004 |
| Changes in estimates | the net book value less the new salvage is spread straight line over the remaining months from the month of the change; earlier months are not restated | FIN-FA-006 |
| Allowance | the suggestion applies loss rates by aging bucket, rounded per bucket; the adjustment is booked by a journal entry | FIN-AR-011 |
| Early-payment discounts | taken only by the discount date; allowed to customers to 4900 Sales Returns and Allowances, taken from vendors to 5000 Cost of Goods Sold in the tests — **the sample chart has no discount accounts; please confirm or name them** | FIN-AR-002, FIN-AP-012 |
| Disposal gains and losses | to the class's disposal account; the tests add 7400 for it — **the sample chart has none; please confirm or name it** | FIN-FA-007 |
| Form 1099 | cash basis by payment date; the payment allocated to the forms of the bill's lines in proportion to their amounts; card payments left out (reported on Form 1099-K); voids reduce the year's amount | FIN-AP-020, -021 |
| Late documents | a bill of a closed period posts in the open period with its own document date for aging (BILL-OS-0120) | FIN-PC-005, -007 |
| Approval thresholds | manual, imported and opening entries and bills over 10,000.00, every payment run, write-offs, credit-limit overrides and bank-detail changes need a controller who did not prepare them | FIN-GL-015, FIN-AP-006, -011, FIN-AR-012, -013, FIN-AP-003 |

## 5. Known limits at this step

- A posted document cannot change its ledger lines, but its document record can still be edited
  by a privileged change until the next BPA release candidate lets the Buk freeze it (DEC-154).
  The ledger and the reports are not affected.
- The posting date of a ledger line is the date it is committed; a document dated in an earlier
  open period posts with that date as its business date (DEC-147). The next release candidate
  adds a posting-date field.
- Printed documents and exports (invoices, 1099 copies, payment files, PDF and Excel reports),
  the workbench and imports come in later steps (`docs/fin-coverage.md`, status `assigned`).

## 6. Sign-off

| Item | Decision (accepted / findings) | Reviewer | Date |
|---|---|---|---|
| Expected results FIN-EXP-01…17 are correct | | | |
| The Buk's results equal them (`docs/tie-out.md`) | | | |
| The accounting choices of section 4 | | | |
| Discount and disposal accounts (section 4) | | | |

Findings, with the section and figure concerned:

1.
