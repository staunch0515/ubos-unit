# `ubos.finance/finance` — US Basic Finance Buk

The first-party finance Buk of PH-2 (DEC-022, DEC-141): US standard general ledger,
receivables, payables, bank, fixed assets, multi-currency and close, built only on BPA
features: no finance-specific kernel code (EXIT-2-04, MET-BIZ-001).

| Path | Content |
|---|---|
| `forge.toml` | project: namespace `ubos.finance`, version, capabilities |
| `model/` | classes: `GlAccount` (extends the genesis `LedgerAccount`, STD-KIND-029), `FiscalPeriod` (STD-KIND-025), the double-entry ledger `JournalLine` (STD-KIND-020…024), `FinancialStatement` layouts (FIN-RP-011) |
| `seed/` | configuration installed with the Buk: the `gl-balance`, `gl-document` and `gl-kind` projections (STD-TIME-021) and the four US statement layouts |
| `logic/` | the L2 asset `ubos.finance.statements` behind the port `FinancialStatement.render` |
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
