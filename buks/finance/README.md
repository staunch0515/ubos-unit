# `ubos.finance/finance` — US Basic Finance Buk

The first-party finance Buk of PH-2 (DEC-022, DEC-141): US standard general ledger,
receivables, payables, bank, fixed assets, multi-currency and close, built only on BPA
features: no finance-specific kernel code (EXIT-2-04, MET-BIZ-001).

| Path | Content |
|---|---|
| `forge.toml` | project: namespace `ubos.finance`, version, capabilities |
| `model/` | classes: `GlAccount` (extends the genesis `LedgerAccount`, STD-KIND-029), `FiscalPeriod` (STD-KIND-025), the double-entry ledger `JournalLine` (STD-KIND-020…024), `FinancialStatement` layouts (FIN-RP-011), `JournalEntry` with `post` and the numbering ledger `PostingNumber` (FIN-GL-010…015) |
| `seed/` | configuration installed with the Buk: the `gl-balance`, `gl-document` and `gl-kind` projections (STD-TIME-021) the four US statement layouts, and the sheet `ubos.finance.controls` (approval of manual entries, control accounts) |
| `logic/` | the L2 assets `ubos.finance.statements` (`FinancialStatement.render`) and `ubos.finance.entries` (`JournalEntry.post`) |
| `tests/` | **generated** by `tools/gen_tests.py`: the tie-out of the sample company of `docs/finance-requirements` (fin-req-v1.0) to its expected results |
| `tools/gen_tests.py` | the test generator; `--check` fails when `tests/` is stale |
| `tools/tieout_report.py` | writes `docs/tie-out.md` from `forge test` in memory and on PostgreSQL |
| `docs/` | `tie-out.md` (generated) and `step-9-record.md` (EXIT-2-04 inspection and requirement coverage) |

Run the tie-out with `forge test buks/finance`, in memory or with `--backend postgres
--database …`. The sample company is loaded by the tests, never by the seed: the seed is
what every installation receives.

Status (PH-2 step 9, parts 9a–9e, DEC-141): 9a — chart of accounts, fiscal periods, the
journal, the balance projection and the opening trial balance FIN-EXP-01; 9b — the January
documents, the `gl-document` projection, FIN-EXP-02 and -03; 9c — the January close, the February
items and the back-dated correction, FIN-EXP-15…17; 9d — the financial statements, FIN-EXP-04…07; 9e — the tie-out report and
the step record. Step 9 is closed (DEC-145).
Step 10 (DEC-146, parts 10a–10h): 10a-1 — journal entries with approval, control accounts and
numbering; 10a-2 — recurring and reversing entries, governed reopening and the year-end close; 10b — receivables and
sales tax; 10c — payables and Form 1099; 10d — bank reconciliation.
