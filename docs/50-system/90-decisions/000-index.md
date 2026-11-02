---
id: UBS-DEC-000
title: Decision Log
status: complete
phase: ALL
depends_on: [UBS-META-00, UBS-META-06]
---

# Decision Log

Decisions are never deleted. A replaced decision is marked
`Superseded by DEC-<NNN>`.

| ID | Title | Status |
|---|---|---|
| DEC-146 | PH-2 step 10 opened in parts 10a–10h; part 10a-1: journal entries with maker–checker approval, the control-account rule and gap-free numbering per fiscal year and source; `ref()` in decisions reads at the decision's coordinates | Accepted |
| DEC-147 | PH-2 step 10, part 10a-2: recurring entries, automatic reversal on opening a period, governed reopening, the fiscal-year close with superseded closes; two gaps of BPA 1.0 for the next release candidate (a ledger's posting-date field, explicit coordinates of `read.projection`) | Accepted |
| DEC-148 | PH-2 step 10, part 10b: receivables and sales tax — customers, tax codes, invoices and credit memos with tax, receipts with application and reapplication, the open-item and tax subledgers, aging with the control account, the sales tax report and customer statements | Accepted |
| DEC-149 | PH-2 step 10, part 10c: payables and Form 1099 — vendors, bills with use tax, payment runs, the open-item subledger, aging with the control account, and the 1099 summary on a cash basis against a threshold table by tax year | Accepted |
| DEC-150 | PH-2 step 10, part 10d: the bank — idempotent statement import, bank items recorded from statement lines, document-level reconciliation with prior outstanding items; January driven entirely by documents ties out FIN-EXP-03 and FIN-EXP-10 | Accepted |
| DEC-151 | PH-2 step 10, part 10e: fixed assets — asset classes, the register with capitalisation from bills, straight-line and double-declining schedules with cumulative rounding, an idempotent depreciation run, disposal with gain or loss, register and roll-forward; FIN-EXP-11 ties out | Accepted |

---

The table rows and records of DEC-001 … DEC-029 are in `01-records-dec-001-029.md`
(UBS-DEC-01); those of DEC-030 … DEC-059 in `02-records-dec-030-059.md` (UBS-DEC-02);
those of DEC-060 … DEC-089 in `03-records-dec-060-089.md` (UBS-DEC-03); those of
DEC-090 … DEC-109 in `04-records-dec-090-109.md` (UBS-DEC-04); those of DEC-110 …
DEC-126 in `05-records-dec-110-126.md` (UBS-DEC-05); those of DEC-127 … DEC-145 in
`06-records-dec-127-145.md` (UBS-DEC-06); the records from DEC-146 on follow.

### DEC-146 — The finance Buk, part 10a-1: journal entries, approval and numbering (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** PH-2 step 10 (UBS-PH-2 §11) builds part 2 of the `finance` Buk: receivables, payables with 1099, bank reconciliation, fixed assets, multi-currency and sales tax. It also runs SCN-501…510 in process (EXIT-2-16 in process) and covers the general-ledger items step 9 left to it (DEC-145). The owner confirmed the plan with the default suggestions.
- **Decision:**
  1. **Parts.**
     - 10a: the general-ledger items of DEC-145, in two halves. 10a-1 covers entries, approval, control accounts and numbering. 10a-2 covers recurring and reversing entries, fiscal years and year-end close, governed reopening, and statements "as known on".
     - 10b: receivables and sales tax.
     - 10c: payables and 1099.
     - 10d: bank.
     - 10e: fixed assets.
     - 10f: multi-currency.
     - 10g: SCN-501…510 in process and the tie-out driven by documents.
     - 10h: closing the step, with the review package for the accounting reviewer.
  2. **Journal entries.**
     - The class `JournalEntry` holds a number (a class key), source, kind, posting and document dates, description, party, lines, the declared `total` and an optional `control_exception`. Its state is `draft` until posted.
     - The query states submitted, approved and rejected are the states of the entry's pending change (STD-TXN-060). A change to a submitted entry therefore invalidates its approval (FIN-GL-014).
     - The command port `post` (L2 asset `ubos.finance.entries`) refuses an entry whose lines do not add up to its total. It appends a `PostingNumber` entry and the `JournalLine` entries in one process, and sets the state to `posted`.
  3. **Numbering.**
     - `PostingNumber` is a ledger keyed by fiscal year and source. The gap-free sequence of STD-KIND-022 is the entry's posted number.
     - Drafts and rejected entries take none (FIN-GL-013).
  4. **Controls as a sheet.** The seed holds the sheet `ubos.finance.controls`.
     - Manual entries whose total exceeds 10,000.00 require one approval by the role `controller` (STD-LGS-041). Maker–checker and one decision per principal are kernel rules (STD-LGS-042).
     - A manual entry that posts to a control account without a logged exception is denied (`RULE.DECISION_DENIED`, FIN-GL-005).
     - `GlAccount` gains `control` (receivables, payables, fixed assets, bank).
  5. **Kernel gap: references in decisions.**
     - A decision's `when` was evaluated without a resolver, so `ref()` and `exists()` found nothing.
     - The decisions now read referenced objects at the decision's coordinates: the process state, else the branch at its snapshot, at the class's default valid time. STD-EXPR-005 already states this for `ref()`.
     - The change is generic: `StateReader` in `crates/ubos-dvm/src/rules.rs`.
  6. **Total.** The approval rule compares the declared money `total` rather than summing the untyped JSON lines. Rule expressions cannot convert their texts to amounts, and `post` refuses a total that differs from the lines.
- **Consequences:**
  - Five new tests pass in memory and on PostgreSQL 16. They cover approval, maker–checker and eligibility (JE-0002); posting without approval (JE-0004); refusal without `defer`; invalidation on change; the control-account refusal and exception; and numbering without gaps.
  - All 17 finance tests pass.
  - A field default is not stored (STD-OBJ-003), so created entries state `draft` explicitly.
- **Alternatives considered:**
  - Typing the lines as composites was not used: the standard does not yet fix how a composite declares its members (an open point for the BPA 1.0 release notes).
  - Raising the control-account refusal from the script was rejected: a sheet decision gives the reason and is changed through review.
- **Affects:** DEC-141, DEC-145, FIN-GL-005, FIN-GL-010, FIN-GL-013, FIN-GL-014, FIN-GL-015, STD-KIND-022, STD-TXN-060, STD-LGS-031, STD-LGS-041, STD-LGS-042, STD-EXPR-005

### DEC-147 — The finance Buk, part 10a-2: recurring and reversing entries, reopening and year-end close (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 10a-2 of step 10 (DEC-146) covers recurring entries (FIN-GL-017), automatic reversal (FIN-GL-018), the fiscal calendar and year-end close (FIN-PC-001, -008, FIN-SCN-11), governed reopening (FIN-PC-006, SCN-507), and statements "as known on" a date (FIN-RP-020, SCN-508).
- **Decision:**
  1. **Recurring entries.** `RecurringEntry` is a template with code, description, source, lines, total and its first and last period. Its command port `run(period_end)` creates the draft entry `<code>-<yyyy-mm>` once; a second run of the period creates nothing. The entry posts like any other, under the same approval rules.
  2. **Automatic reversal.**
     - `JournalEntry` gains `auto_reverse_on`, `reverses`, `reversed_by` and `adjustment_period`.
     - The command port `FiscalPeriod.open` posts the reversal `<number>-R` of every posted entry whose `auto_reverse_on` falls in the period, once. The reversal has opposite amounts, the kind `<kind> reversal` and a reference to the original.
     - Reversals are exempt from the approval of manual entries, since the original was approved.
  3. **Fiscal calendar.**
     - `FiscalPeriod` gains `fiscal_year` and `number`.
     - `FiscalYear` holds the year, its first and last day, its state, the retained-earnings account and its closes.
     - Its command port `close` posts a closing entry `CLOSE-<year>-<n>` that brings every income and expense account to zero against retained earnings, at the year's last day.
     - A later close, after an adjustment, posts the remaining difference and marks the earlier closes superseded.
     - The income statement, the cash-flow net income and the equity statement's net income leave out closing entries; the balance sheet is unchanged by the close.
  4. **Governed reopening.**
     - `FiscalPeriod` declares the command port `reopen`, and the sheet `ubos.finance.controls` requires one approval by a controller for closed periods (`FiscalPeriod:closed`).
     - `reopen_period` then waits as a pending change (STD-KIND-025 since rc10): the requester cannot approve it, a rejection keeps the period closed, and the approval reopens it.
     - A second close writes the artefact `close-2026-01-2`; the first artefact keeps the balances as first closed.
  5. **Forge.** A port call of a scenario now counts toward script coverage for the function that implements the port, as a port test does (DSN-FRG-201). All five assets have coverage 1.0.
  6. **Gaps of BPA 1.0, for the next release candidate.** Two acceptance criteria need features the standard names but does not specify:
     - **Posting date of a ledger class (STD-KIND-025).** The clause allows a class to declare a posting-date field, but no member names it, and the kernel takes the commit date. A correction posted into a reopened January after January has ended therefore falls in February, and the second close cannot include it (FIN-PC-006 acceptance 1, the correction part).
     - **Explicit coordinates of `read.projection` (STD-ISA-037).** The clause says "or at explicit coordinates", but its parameters have none. A port therefore cannot render the statements "as known on" a date (FIN-RP-020 for statements; FIN-PC-006 acceptance 3). The trial balance as known on a date works through the adapter operation (DEC-143), and a close artefact keeps the balances as first closed.

     Both are standard changes: a new release candidate (1.0-rc13) with clauses, vectors and the kernel. The owner decides when; until then the two criteria stay open in the step-10 coverage record.
  7. **Not in 10a-2.** Period 13 in the statements (annual but not December) goes to step 15 with comparatives (FIN-PC-001 acceptance 2). The entry `adjustment_period` marks such entries now.
- **Consequences:**
  - Five new tests pass in memory and on PostgreSQL 16; all 22 finance tests pass.
  - The year-end test runs on a release branch, a kind without a lifetime (STD-VER-011), because it spans a year. It verifies the Buk's assets there first, since a release branch runs verified assets only (STD-CTX-023).
- **Alternatives considered:**
  - Posting reversals by a timer (BecameEffective events) was rejected for now: FIN-GL-018 ties the reversal to opening the period, and a port keeps it explicit and idempotent.
  - Treating the posting date as the entry's `valid_at` inside the Buk was rejected: the kernel's period rules and close artefacts follow the posting date of STD-KIND-025, and a Buk cannot change them.
- **Affects:** DEC-146, FIN-GL-017, FIN-GL-018, FIN-PC-001, FIN-PC-006, FIN-PC-008, FIN-RP-020, STD-KIND-025, STD-KIND-026, STD-ISA-037, STD-VER-011, STD-CTX-023, DSN-FRG-201

### DEC-148 — The finance Buk, part 10b: receivables and sales tax (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 10b of step 10 (DEC-146) builds receivables and sales tax (FIN-AR, FIN-TX) and SCN-503. It ties out the January receivables documents against FIN-EXP-02, the aging against FIN-EXP-08 and the sales tax report against FIN-EXP-13.
- **Decision:**
  1. **Master data.**
     - `TaxCode` holds the code, rate, category (taxable, exempt, non-taxable, export), whether a certificate is required, and the payable account.
     - `Customer` holds the code, name, location, currency, default tax code, terms, certificate with its expiry, and receivable account.
     - Both are Definition objects, so rates and terms are effective-dated by their versions.
  2. **Documents.**
     - `Invoice` holds both invoices and credit memos; a credit memo can apply to an invoice. Its port `post`:
       - computes tax per tax code on the sum of the code's lines, half away from zero to the cent (`div` with `half_up`);
       - computes the due date from the terms;
       - creates and posts a `JournalEntry` of source `receivables`, so every subledger document reaches the general ledger at once (FIN-GL-021);
       - appends the open item (`ArItem`) and the tax detail (`TaxLine`), and numbers the document in the gap-free series `sales-documents` (FIN-AR-004).
     - `Receipt` posts cash against receivables. It applies its amount to open items, and the rest stays as an unapplied item. Its port `reapply` moves an applied amount between items, keeping both entries as history (FIN-AR-008).
  3. **Subledgers and reports.**
     - The projections `ar-open` (by customer and document), `ar-activity` (by customer and kind) and `tax-detail` (by tax code and document) are in the seed.
     - The `Receivables` object answers three query ports: `aging(as_of)`, with due-date buckets and the control account's balance and difference (FIN-AR-010, FIN-RP-007); `tax_report(opening, closing)` (FIN-TX-008); and `statement(customer, opening, closing)` (FIN-AR-009).
  4. **Certificates.** The sheet `ubos.finance.controls` denies posting an invoice with a line under a code that requires a certificate when the customer's certificate is missing or expired at the invoice date (FIN-TX-004).
  5. **Test data.** Tax codes, customers and opening open items come from the frozen CSVs. The documents are built from the CSV's journal lines. Two facts appear only in the documents' descriptions: the service line of INV-1004 is non-taxable (NT), and CM-2001 applies to INV-1004. The generator states them as data. The expected figures come only from the tables of the expected results.
  6. **INV-1005.** It is recorded at its US-dollar amount at the invoice-date rate until part 10f makes it a euro invoice with revaluation. The aging test therefore expects INV-1005 at 54,250.00, the published row less the revaluation of FXR-2601 in FIN-EXP-02.
  7. **Forge.** Script coverage resolves a scenario port call to its implementation through the target's URI or the objects the scenario created, now that several classes have a port `post`.
- **Consequences:**
  - Six new tests pass in memory and on PostgreSQL 16; all 28 finance tests pass, with script coverage 1.0.
  - The tests cover: the GL lines of the nine January receivables documents; the aging; the tax report; the statement of C300; reapplying a misapplied receipt with its history; and the certificate refusal.
- **Open, assigned to parts of step 10 or later steps:**
  - multi-currency invoices and revaluation (10f);
  - the payment of sales tax to the authority, STX-PAY-2512 (10d with the bank);
  - use tax on purchases (FIN-TX-007, Should; 10c);
  - the allowance and write-offs with approval (FIN-AR-011, -012), the credit-limit check (FIN-AR-013, Should) and recurring invoices (FIN-AR-014, Should): 10g;
  - deferred service revenue (FIN-AR-015, Could): PH-3;
  - invoice documents as PDF (FIN-AR-005): step 15;
  - jurisdictions combined into codes (FIN-TX-001): the sample codes are single rates, so 10g decides whether a combination is needed.
- **Affects:** DEC-146, FIN-AR-001…010, FIN-TX-002…006, FIN-TX-008, FIN-GL-021, FIN-RP-007, DSN-FRG-201

### DEC-149 — The finance Buk, part 10c: payables and Form 1099 (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 10c of step 10 (DEC-146) builds payables and Form 1099 (FIN-AP) and SCN-504. It ties out the January bills and payment runs against FIN-EXP-02, the aging against FIN-EXP-09 and the 1099 summary against FIN-EXP-14.
- **Decision:**
  1. **Master data.**
     - `Vendor` holds the code, name, entity type, Form 1099 form and box, whether a TIN is on file, terms and payable account.
     - `Form1099Threshold` holds the reporting threshold per tax year. FIN-EXP-14 requires the threshold to be read from a table, never hard-coded.
  2. **Documents.**
     - `Bill` holds the document number, the vendor's invoice number and lines with accounts. Its port `post` posts expenses against payables through a `JournalEntry` of source `payables` and records the open item (`ApItem`). The due date comes from the terms.
     - A bill line with a `use_tax_code` accrues the use tax the vendor did not charge, against that code's payable account (FIN-TX-007).
     - `PaymentRun` pays open items from one bank account. A payment above an item's open amount is refused, and each payment reduces its item.
  3. **Reports.** The `Payables` object answers two query ports:
     - `aging(as_of)`, in bill-date order, with the control account's balance and difference (FIN-AP-010, FIN-RP-007);
     - `form_1099(tax_year, through)`: the payments made in the tax year to vendors with a reportable form (cash basis), each with whether it reaches the year's threshold.
  4. **Test data.** Vendors, thresholds and opening open items come from the frozen CSVs; bills and payment runs are built from the journal lines and descriptions.
     - The vendor invoice numbers are the document numbers without `BILL-`, except two that the expected aging and the payment-run descriptions name: MP-2026-01 and DC-2026-01.
     - The expected figures come only from the tables.
  5. **Vendor tax IDs.** The sample data records only whether a TIN is on file. Stored lookup tokens for vendor tax IDs (DSN-DVM-833, DEC-140) are therefore not needed in step 10 and stay with step 19.
- **Consequences:**
  - Four new tests pass in memory and on PostgreSQL 16. They cover the GL lines of the seven January bills and two payment runs; the aging with control 46,300.00; the 1099 summary (V200 9,000.00 and V300 8,500.00 reportable, V800 1,500.00 below the threshold of 2,000.00); and use tax.
  - All 32 finance tests pass, with script coverage 1.0. No kernel code was added.
- **Open, assigned to parts of step 10 or later steps:**
  - the capitalisation of BILL-TS-5520 as asset FA-003 (10e);
  - early-payment discounts, three-way matching and vendor statements, wherever FIN-AP asks for them (10g reviews FIN-AP-001…019 against the record);
  - 1099 forms as documents, and their e-filing (step 15 and step 12).
- **Affects:** DEC-146, DEC-140, FIN-AP-001…019, FIN-TX-007, FIN-GL-021, FIN-RP-007, DSN-DVM-833

### DEC-150 — The finance Buk, part 10d: bank reconciliation (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 10d of step 10 (DEC-146) builds the bank (FIN-BK) and SCN-505. It ties out the reconciliation of the operating account 1010 for January against FIN-EXP-10, whose statement is `bank-statement-2026-01.csv`.
- **Decision:**
  1. **Import.** `BankStatementLine` is a ledger with `ledger.statement` (STD-KIND-030). A statement is imported with the kernel's `ingest_statement`, so a second import of the same statement appends nothing (FIN-BK-003). The statement's opening and closing balances are members of the reconciliation.
  2. **Reconciliation.** `BankReconciliation` belongs to one bank account and one statement. It holds the period, the statement balances, the items outstanding at the previous reconciliation, and a date window.
     - `record(line, number, account, description)` posts a bank-originated item (interest, fees) from a statement line as an entry of source `bank` (FIN-BK-005).
     - `reconcile()` matches each statement line, in date order, to a posted entry on the bank account with the same amount within the window; failing that, to an item outstanding before.
     - It reports the statement's balances and activity, the outstanding entries, the adjusted bank balance, the book balance from `gl-balance` and the difference. It keeps the matches, and the state becomes `reconciled` when nothing is left.
  3. **Document level, not kernel matching.** The kernel's matching (STD-KIND-027, -028) pairs ledger entries and records them by entry id, and it is an adapter operation. The reconciliation needs documents and their dates, and logic cannot read an entry by its id. The Buk therefore matches statement lines with posted entries. Kernel matching remains available for line-level matching of subledgers, and a logic instruction for it is a candidate for a later release of BPA.
  4. **The month from documents.** The test generator now builds a whole month from its documents:
     - receivables and payables documents through their subledgers;
     - the others as journal entries: journal (manual, or import when the description says so), depreciation (fixed assets), bank and tax payments (bank), and revaluation with its automatic reversal date;
     - manual and imported entries over 10,000.00 are approved by a controller;
     - manual entries touching a control account carry a logged exception.

     The approval rule of `ubos.finance.controls` now covers imported entries too, as FIN-EXP-02 says for the payroll import.
  5. **Sales-tax payment.** STX-PAY-2512 is an entry of source `bank` with kind `tax payment` (FIN-TX-006).
- **Consequences:**
  - Two new tests pass in memory and on PostgreSQL 16. The first imports the statement, records interest and fee from it, and reconciles. The result equals FIN-EXP-10: outstanding PAYROLL-2601 (45,000.00), CHK-1045 matched as outstanding before, difference 0.00. The two recorded entries post the lines of FIN-EXP-02.
  - In that test, January built entirely from documents gives the trial balance FIN-EXP-03.
  - The second test shows that a repeated import appends nothing.
  - All 34 finance tests pass, with script coverage 1.0. No kernel code was added.
- **Open, assigned:**
  - the savings account's statement (no sample statement): not needed;
  - automatic rules for recurring bank items, reconciliation sign-off with approval, and statement file formats (BAI2, camt): step 12 (bank file formats) and 10g for the rules;
  - a logic instruction for kernel matching: the next BPA release candidate, with the two items of DEC-147.
- **Affects:** DEC-146, DEC-147, FIN-BK-001…011, FIN-TX-006, STD-KIND-027, STD-KIND-028, STD-KIND-030

### DEC-151 — The finance Buk, part 10e: fixed assets (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 10e of step 10 (DEC-146) builds fixed assets (FIN-FA). It ties out the depreciation entry DEP-2601 against FIN-EXP-02, the register against FIN-EXP-11, and the acceptance figures of FIN-FA-005, -007 and -009.
- **Decision:**
  1. **Model.**
     - `AssetClass` holds the cost, accumulated-depreciation, expense and disposal accounts, the default method and life, the full-month convention and the capitalisation threshold.
     - `FixedAsset` holds the code, class, cost, salvage, in-service date, method (straight line or double-declining balance), life, acquisition, location, custodian and status.
     - The ledger `AssetBook` holds the cost and accumulated depreciation per asset by entry, with the kinds opening, addition, depreciation and disposal.
     - The projections are `fa-book` (by asset) and `fa-roll` (by kind).
     - The `AssetRegister` object carries the runs and reports.
  2. **Schedule** (FIN-FA-003, -004).
     - The depreciation of each asset year is computed first. Straight line takes cost less salvage over the years. Double-declining takes twice the net book value at the year's start over the years, or the straight-line rest over the remaining years when that is more. The last year takes the rest.
     - Month *m* of an asset year depreciates round(year · m / 12) − round(year · (m − 1) / 12), half away from zero. The residue of rounding thus falls in the year's last month, and the life's total is cost less salvage.
     - This reproduces the sample: FA-002 has 10,000.00 after six months of its first asset year and 1,666.67 in January. Rounding each month instead would give 10,000.02.
     - The full-month convention counts the month placed in service as month 1.
  3. **Runs and documents.**
     - `capitalize` creates an asset from the lines of a posted bill on the class's cost account, refused below the class's threshold. FA-003 comes from BILL-TS-5520 (FIN-FA-002).
     - `run_depreciation(period_end)` posts one entry `DEP-<yymm>` of source `fixed_assets`, summarised per expense and accumulated-depreciation account, and an `AssetBook` entry per asset. A second run of the month creates nothing, and a run for a closed period is refused (FIN-FA-005).
     - `FixedAsset.dispose` depreciates up to the disposal month as far as not yet run, removes cost and accumulated depreciation, books the proceeds and posts the gain or loss to the class's disposal account (FIN-FA-007).
  4. **Reports.** `register(opening, closing)` lists each asset with cost, accumulated depreciation at both dates, the period's depreciation and the net book value. `roll_forward(opening, closing)` gives cost and accumulated depreciation as opening, additions, disposals, depreciation and closing (FIN-FA-009).
  5. **Test data.** Classes, the assets in service before 2026 and their opening book come from `fixed-assets.csv`. The sample chart has no disposal account, so the disposal test adds 7400 to its copy of the books.
- **Consequences:**
  - Three new tests pass in memory and on PostgreSQL 16:
    - the January run posts the lines of FIN-EXP-02 once; the register equals FIN-EXP-11; the roll-forward gives cost 190,000.00 + 12,000.00 = 202,000.00 and accumulated depreciation 58,000.00 + 4,000.00 = 62,000.00;
    - selling FA-002 for 60,000.00 gives the gain of 1,666.67;
    - a run for a closed period is refused.
  - All 37 finance tests pass, with script coverage 1.0. No kernel code was added.
  - The refusal of a closed period is raised by the script and reads as `LOGIC.TYPE_ERROR`. A decision cannot look up the period of a parameter date; a clearer error code goes to 10g.
- **Open, assigned:**
  - changes in estimates (FIN-FA-006) and 150% declining balance: 10g;
  - impairment (FIN-FA-008, Could) and the tax book (FIN-FA-010, Could): PH-3;
  - the projected depreciation schedule as a report: step 15.
- **Affects:** DEC-146, FIN-FA-001…007, FIN-FA-009

### DEC-152 — The finance Buk, part 10f: multi-currency (PH-2 step 10)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 10f of step 10 (DEC-146) builds multi-currency receivables (FIN-FX, ASC 830) and FIN-SCN-09 (SCN-510). INV-1005 becomes a euro invoice from invoice to receipt. The tests tie out FIN-EXP-12, the published aging FIN-EXP-08 with INV-1005 at its revalued amount, the lines of FXR-2601, FXR-2601-R and RCPT-0005 (FIN-EXP-02, -15), and the FX balances of FIN-EXP-17.
- **Decision:**
  1. **Rates are kernel objects.** The rates of `fx-rates.csv` are one `FxRate` object (STD-FND-029) of source `company`, pair EUR/USD and type `spot`. Each rate is a version valid from its date, so the rate of a date is the version valid then. A correction is a new version from its date that keeps the old one (FIN-FX-002). The `Receivables` object names the functional currency, the rate type, and the realized and unrealized gain-or-loss accounts (7200, 7210).
  2. **Conversion at the document date.** L2 has no `fx_convert`, and a process's `asof` is its start date, not the document's date. The logic therefore finds the `FxRate` object of the key with `search.query` and reads the version valid at the document date with `read.timeline`. The read is pinned to the process snapshot and recorded like every other read. The functional amount is the document amount times the rate, rounded half-even to the cent, as `fx_convert` rounds. A rate entered on the document takes precedence (FIN-FX-003).
  3. **Both amounts everywhere.**
     - Invoices keep the document currency, the rate and the functional total.
     - Every journal line of a foreign-currency document carries `original` beside its functional `amount`.
     - Open items (`ArItem`) carry `original`, and the projection `ar-open` sums it.
     - The aging shows each item in both currencies (FIN-FX-007).
     - The sales-tax detail is kept in the functional currency, so FIN-EXP-13 shows the export sale at 54,250.00.
     - On an invoice, the receivable line is the sum of the converted lines, so the entry balances.
  4. **Settlement** (FIN-FX-004). A receipt applies its document-currency amount to an open item at the item's carrying amount, which is proportional when the item is settled in part. The difference to the receipt's functional amount is posted to the realized account and recorded in the ledger `FxGain`. RCPT-0005 thus posts 1010 54,000.00, 7200 250.00 and 1200 (54,250.00).
  5. **Remeasurement** (FIN-FX-005). `Receivables.revalue(period_end)` remeasures every open item whose document currency differs from the functional currency, at the rate of the period end.
     - It posts one entry `FXR-<yymm>` of source `revaluation`, between the control account and the unrealized account, with `auto_reverse_on` the next day; `FiscalPeriod.open` posts the reversal (DEC-147).
     - The open item and the `FxGain` ledger record the remeasurement at the period end and its reversal on the next day, so the subledger agrees with the control account on both dates.
     - A second run of the period posts nothing, and a closed period is refused.
     - With `simulate`, the run is computed without posting, from the carrying amounts before the posted run of the period. It shows the posted total and the difference. On the original branch the simulation reproduces the posted run (difference 0.00). In a test-copy branch with a corrected rate, it shows the difference.
  6. **Report.** `fx_report(opening, closing)` lists realized and unrealized gains and losses by document and posting entry for a period (FIN-FX-007).
  7. **The month from documents.** The generator builds INV-1005 and RCPT-0005 in euros from the notes of the sample ("EUR 50,000.00 at 1.0850"), and the January revaluation through `revalue`.
- **Consequences:**
  - Two new tests (`tests/sample-fx.yaml`) pass in memory and on PostgreSQL 16:
    - FIN-SCN-09 from invoice to receipt with FIN-EXP-12 at each date: 54,250.00, 54,600.00 with the gain of 350.00, 54,250.00 after the reversal, and the receipt at 54,000.00 with the loss of 250.00;
    - the lines of INV-1005, FXR-2601, FXR-2601-R and RCPT-0005;
    - 7200 at 250.00 and 7210 at 0.00 on 2026-02-28 (FIN-EXP-17), and C400's balance at 0.00;
    - the January and February FX reports;
    - the simulation in the test copy with a corrected rate of 1.0930: total 400.00 against 350.00 posted.
  - The aging test now runs the revaluation and equals the published FIN-EXP-08 (total 158,735.00 = account 1200). The adjustment of 10b is gone.
  - All 39 finance tests pass, with script coverage 1.0. No kernel code was added.
- **Open, assigned:**
  - foreign-currency payables and bank accounts (FIN-FX-003 for bills and payments, FIN-FX-006, Should): the sample has none, so they go to 10g with the review of the other FIN-AP gaps;
  - `fx_convert` and explicit coordinates in L2, so that logic converts at a given date without reading the rate object itself: the next BPA release candidate, with the items of DEC-147;
  - the currency columns of the reports in the workbench: step 15.
- **Affects:** DEC-146, DEC-147, DEC-148, FIN-FX-001…005, FIN-FX-007, STD-FND-029

### DEC-153 — The finance Buk, part 10g-1: the sample company in process and SCN-501…510 (PH-2 step 10)
- **Date:** 2026-11-02 · **Status:** Accepted
- **Context:** Part 10g of step 10 (DEC-146) runs SCN-501…510 in process and ties out every expected result from documents. It also takes the open items that DEC-147…152 assigned to it. The owner confirmed the plan with the default suggestions. The part is done in two halves: 10g-1 (this record) covers the scenarios and the tie-out, and 10g-2 covers the open items.
- **Decision:**
  1. **The company in process.** The generator walks January and February of `transactions.csv` and sends each document through the port that owns it, at noon of its recording date:
     - invoices, credit memos and receipts through `Invoice` and `Receipt`;
     - bills and payment runs through `Bill` and `PaymentRun`, with the bill of FA-003 capitalised;
     - the depreciation run through `AssetRegister.run_depreciation` and the revaluation through `Receivables.revalue`;
     - the other entries through `JournalEntry.post`, with those over the limit approved by a controller.

     On the evening of 31 January the statement is imported and reconciled, with interest and fee recorded from it. On 1 February `FiscalPeriod.open` posts FXR-2601-R. On 2 February January is closed through its checklist. February follows.
  2. **The close checklist** (FIN-PC-004, SCN-506). `FiscalPeriod.checklist` runs five automatic checks, each with its result and evidence:
     - no unposted entry or document dated in the period;
     - every bank account with statement lines in the period reconciled for it;
     - receivables and payables equal to their control accounts;
     - the depreciation, revaluation and recurring runs of the period done;
     - the accounts marked `suspense` at zero.

     `soft_close` moves the period to `closing` only when every check passes, keeps the results on the period and otherwise is refused naming the failing checks. `close_period` then closes it as before.
  3. **Posting date and closed periods** (FIN-PC-005).
     - The kernel refuses postings by commit date (STD-KIND-025). The Buk now also refuses an entry whose posting date lies in a closed period.
     - `Bill` gains an optional `posting_date`. BILL-OS-0120 therefore keeps its document date 2026-01-20 for aging and posts on 2026-02-10. The open item takes the posting date as its valid time.
     - Since the posting date is the commit date, a statement's interest and fee must be recorded before the period's close to fall in it.
  4. **Entries are checked before approval** (FIN-GL-010, SCN-501). Before approval is asked, `JournalEntry.post` refuses an entry whose lines do not balance or that names an account it cannot read. The ledger's own check would otherwise come only when the approved change is applied.
  5. **Opening balances** (SCN-501). The opening balances can post as one entry of source `opening`. The sheet `ubos.finance.controls` now requires a controller's approval for opening entries over 10,000.00, as for manual and imported ones.
  6. **Kernel fix: simulation of calls without a process record** (STD-LGS-044).
     - `simulate` cut the sandbox of a historical call before the commit named in the call's process record. Calls without an explicit process id, and deferred calls, have no such record, so they re-ran on the head. Every document then answered "already posted" and was reported as changed.
     - Such a call now re-runs on the snapshot its journal pinned (`snapshots`), which is the state it ran on.
     - The change is in `crates/ubos-dvm/src/simulation.rs`. The release candidate after rc12 gets a conformance vector for it.
  7. **Scenario coverage.**

     | Scenario | Shown by |
     |---|---|
     | SCN-501 | opening entry approved by a controller; refusals of an unbalanced entry and an unknown account; FIN-EXP-01 |
     | SCN-502 | approval, maker–checker and invalidation (DEC-146); a posted line is immutable (`LEDG.IMMUTABLE_ENTRY`); correction by a reversal naming the original; gap-free numbers |
     | SCN-503 | receivables documents, credit memo, partial receipt, reapplication, statement and aging (DEC-148); the company run |
     | SCN-504 | bills, payment runs and 1099 (DEC-149); the company run |
     | SCN-505 | idempotent import, recorded items and reconciliation equal to FIN-EXP-10 (DEC-150) |
     | SCN-506 | depreciation run, close checklist, close artefact equal to FIN-EXP-03, statements FIN-EXP-04…07; close refused while 1010 is not reconciled |
     | SCN-507 | refusal after the close, governed reopening and second close (DEC-147); a January bill refused after the close |
     | SCN-508 | FIN-EXP-16 as known on 2026-02-05 and 2026-02-28 in the company run |
     | SCN-509 | thresholds of 5,000.00 and 1,000.00 simulated on January's 24 calls of `post` (0 and 1 changed, the other calls reproduced); 5,000.00 published from 2026-03-01; the posting of JE-0003 records the January version in its rule binding |
     | SCN-510 | FIN-SCN-09 (DEC-152) |
- **Consequences:**
  - Eight new tests pass in memory and on PostgreSQL 16.
  - `tests/sample-company.yaml` runs the company from its documents:
    - January ties out FIN-EXP-01…03, the statements FIN-EXP-04…07, the agings FIN-EXP-08 and -09, the reconciliation FIN-EXP-10, the register FIN-EXP-11, the tax report FIN-EXP-13 and the 1099 summary FIN-EXP-14;
    - the January close artefact equals FIN-EXP-03 with every check passed;
    - February ties out FIN-EXP-12, -15, -16 and -17;
    - a late January bill is refused.
  - `tests/sample-scenarios.yaml` holds the flows of the table that the tie-out does not show.
  - All 47 finance tests pass, with script coverage 1.0.
- **Open, assigned:**
  - posted documents frozen against edits: 10g-2 (moved to the next BPA release candidate by DEC-154);
  - a registered error code for a script's business refusal (now `LOGIC.TYPE_ERROR`), with the closed period of DEC-151: the next BPA release candidate;
  - the conformance vector of the simulation fix: the next BPA release candidate;
  - SCN-504 A1 (duplicate bills) and bill approval: 10g-2 with FIN-AP-005, -006;
  - SCN-505 automatic matching rules and undoing a match: 10g-2;
  - SCN-506 drill-down, SCN-508 impact report on reports, SCN-509 expense-coding rules and the chart review of SCN-501: steps 14 and 15 (workbench and reports).
- **Affects:** DEC-146…152, FIN-PC-004, FIN-PC-005, FIN-GL-010, SCN-501…510, STD-LGS-044, STD-KIND-025

### DEC-154 — The finance Buk, part 10g-2: the open items of step 10 (PH-2 step 10)
- **Date:** 2026-11-02 · **Status:** Accepted
- **Context:** Part 10g-2 of step 10 takes the open items that DEC-147…153 assigned to 10g: payables, receivables, fixed assets, foreign-currency payables, the bank, the freeze of posted documents and the review of the FIN-AP and FIN-TX-001 gaps. The owner confirmed it with the default suggestions.
- **Decision:**
  1. **Payables.**
     - Duplicate bills (FIN-AP-005, SCN-504 A1). `Bill` gains the key `(vendor, vendor_invoice)`, so the kernel refuses a second bill with `MODEL.KEY_CONFLICT`. A bill matching another bill's vendor, amount and date under a different number posts only with a `duplicate_reason`.
     - Bill approval (FIN-AP-006). `Bill.amount` is the total as received. `post` refuses lines that do not add up to it, and the sheet requires a controller's approval of bills over 10,000.00. An unapproved bill has no open item, so it cannot be paid.
     - Vendor credits and prepayments (FIN-AP-008). A credit is a `Bill` of kind `credit`, applied to a bill or left open. A prepayment is a payment of a run marked `prepayment`. `Payables.apply` applies either to an open bill.
     - The payment proposal (FIN-AP-010). `Payables.propose_run` creates a draft run of the posted, unheld items due by a date.
     - Run approval (FIN-AP-011). The sheet requires a controller's approval of every draft run; maker–checker excludes its preparer.
     - Voids (FIN-AP-014). `PaymentRun.void` reverses one payment by an entry dated in an open period. The item opens again and the original payment stays.
  2. **Receivables.**
     - Write-off and recovery (FIN-AR-012). `WriteOff` posts against the allowance with a controller's approval and marks the invoice written off. A recovery reinstates the item before it is received.
     - Allowance (FIN-AR-011). `Receivables.allowance` applies the loss rates by bucket to the aging and compares the result with the allowance account. The adjustment is posted by a journal entry.
     - Credit limit (FIN-AR-013). An invoice over the customer's credit limit warns, or, when the customer is set to `approve`, needs a `credit_override` that a controller approves.
     - Recurring invoices (FIN-AR-014). `RecurringInvoice.run` creates and posts the invoice of a period once.
  3. **Fixed assets.**
     - The 150% declining balance (`DB150`, FIN-FA-003).
     - The mid-month and next-month conventions (FIN-FA-004). Mid-month counts in half-months.
     - Changes in estimates (FIN-FA-006). `FixedAsset.change_estimate` keeps the accumulated depreciation before the change and spreads the net book value less the new salvage straight-line over the remaining months. Earlier months are not restated, and a later change starts from the earlier one.
  4. **Foreign-currency payables** (FIN-FX-003…005, as DEC-152 for receivables).
     - Bills convert at the rate of their date and keep both amounts on lines and open items.
     - Payment runs convert at the run date and post the realized gain or loss against the carrying amount. They keep, per payment, the functional amount and the carrying amount, which a void reverses.
     - `Payables.revalue` posts FXP-<yymm> and reverses on the next day.
     - The close checklist also requires the payables revaluation.
  5. **Bank.**
     - Matching rules with a rule, a confidence and a reason (FIN-BK-004): first the bank reference equal to an entry's number, then the amount within the date window, then the items outstanding before.
     - Bank rules pre-fill the account of an entry recorded from a statement line (FIN-BK-005).
     - Manual matches and their undoing (FIN-BK-005, -006). They are records of the append-only ledger `BankMatch`, so the history keeps both. The latest record of a line is in force, and a manual match may name several entries, or several lines one entry.
     - Completion (FIN-BK-007, -008). `sign_off` is refused while a difference remains. A second person approves it (a sheet decision, maker–checker). The result is kept as signed, and the reconciliation no longer changes.
     - Carry-forward (FIN-BK-009). Without given prior items, the outstanding items of the account's previous reconciliation are carried forward. Items older than `stale_days` (default 90) are flagged.
     - Transfers (FIN-BK-002). `BankTransfer` posts between two bank accounts.
     - Correction: DEC-150 and the Buk cited the bank requirements as FIN-CB-*, which do not exist. The citations now read FIN-BK-001…011.
  6. **The freeze of posted documents goes to the next release candidate.**
     - A sheet's `read_only` would freeze them, but the kernel evaluates it on the version a change writes. A rule matching `posted` would therefore refuse the posting itself.
     - The release candidate after rc12 clarifies that `read_only` matches the version before the change (STD-LGS-011). The Buk then adds the rule for `JournalEntry`, `Invoice`, `Receipt`, `Bill` and `PaymentRun`.
     - Until then, posted ledger lines are immutable (`LEDG.IMMUTABLE_ENTRY`, DEC-153), and a second posting posts nothing.
  7. **Review of the remaining FIN-AP and FIN-TX-001 gaps.** These are Musts that need new structures, so they go to a part 10g-3:
     - FIN-TX-001: jurisdictions combined into codes, with dated rates read at the invoice date and the report by state and local parts;
     - FIN-AP-002: TIN, W-9 and backup-withholding warning, with masking by permission;
     - FIN-AP-003: second-person control of bank-detail changes, holding payments;
     - FIN-AP-020: 1099 boxes, per-line overrides and the exclusion of card payments;
     - FIN-AP-021 acceptance 2: a test year with another threshold;
     - FIN-AP-022: the review report.

     The PDF copies and e-filing export of FIN-AP-022 and the payment files of FIN-AP-013 and FIN-BK-011 go to step 12, with the statement formats and import check of FIN-BK-003 (DEC-150). FIN-AP-023, FIN-FX-006 and FIN-BK-010 (Should) also go to 10g-3. 10g-3 starts with a table of every FIN Must against its port and test, since the reviews of 10b…10d missed several gaps.
- **Consequences:**
  - Sixteen new tests pass in memory and on PostgreSQL 16:
    - payables: duplicates, approval before payment and the run's second person, credit and prepayment, the proposal of FIN-AP-010 (P-7781 and CPL-1225, 32,300.00), and the void of DC-2025-12;
    - receivables: allowance 1,987.35 against 4,000.00, write-off and recovery, credit limit in both modes, and the recurring invoice;
    - fixed assets: FA-001 in February at 1,489.36, and DB150 with mid-month and SL with next-month;
    - FX: a euro bill from 10,850.00 to a payment with a gain of 50.00, and its void;
    - bank: rules and matches, undo, refused completion and sign-off, carry-forward, and transfer.
  - The company run and every tie-out pass unchanged, now with bill and payment-run approvals.
  - All 63 finance tests pass, with script coverage 1.0. No kernel code was added.
- **Open, assigned:**
  - the gaps of decision 7: 10g-3;
  - `read_only` on the version before a change, the business-refusal code (DEC-153) and the simulation vector (DEC-153): the next BPA release candidate.
- **Affects:** DEC-147…153, FIN-AP-005, -006, -008, -010, -011, -014, FIN-AR-011…014, FIN-FA-003, -004, -006, FIN-FX-003…005, FIN-BK-001…011, STD-LGS-011

### DEC-155 — The finance Buk, part 10g-3: every Must requirement accounted for (PH-2 step 10)
- **Date:** 2026-11-02 · **Status:** Accepted
- **Context:** DEC-154 found Must requirements that 10b…10d had missed, and assigned a part 10g-3. It starts with a table of every finance requirement against the Buk, then builds the Buk's gaps. The owner confirmed it with the default suggestions.
- **Decision:**
  1. **The coverage table.** The source `buks/finance/tools/fin-status.txt` gives each requirement that no test name carries one status with its evidence:
     - `covered`: the test names given must exist;
     - `platform`: the kernel or platform reference;
     - `assigned`: a later PH-2 step or PH-3;
     - `gap`.

     `tools/fin_coverage.py` reads the 153 requirements of fin-req-v1.0, the test names and that file, checks them and writes `docs/fin-coverage.md` (generated). With `--closing`, it also fails while a Must requirement is a gap. CI runs it with `--check --closing`.
  2. **General ledger.**
     - The trial balance (FIN-RP-001) gives, per account, the opening balance, debits, credits and the closing balance, with totals. On request it rolls up summary accounts along `parent`. Journal lines now carry `side` and `date`.
     - The account inquiry (FIN-GL-022) gives the opening balance, the lines by document with a running balance, and the closing balance.
     - Both are ports of the new `GeneralLedger` object (asset `ubos.finance.ledger`), over the projections `gl-sides` and `gl-lines`.
     - An entry balances in each transaction currency too (FIN-GL-011). A payment run's bank line therefore carries its original amount, and a run pays in one foreign currency at most.
  3. **Kernel fix: accounts of subclasses** (STD-KIND-029).
     - The checks that an account is live, a leaf and in the entry's currency applied only to objects of the class `LedgerAccount` itself, and so did the roll-up of `read.projection`. The clause lets packages extend or subclass it, and `GlAccount` is such a subclass.
     - Both now cover `LedgerAccount` and every class that extends it (`crates/ubos-dvm/src/finance.rs`, `projections.rs`). A posting to a summary account is refused, and an ended account is not found.
     - The release candidate after rc12 gets a conformance vector for it.
  4. **Periods** (FIN-PC-003).
     - A soft-closed (`closing`) period takes only entries marked `adjusting`, which a controller approves.
     - `FiscalPeriod.close_subledger` closes a subledger for the period while the general ledger stays open.
  5. **Receivables and payables documents.**
     - `PaymentTerms` give net days, an end-of-month variant and an early-payment discount. The discount is taken on a receipt or a payment by the discount date and posted to the discount account (FIN-AR-002, FIN-AP-012). A void also reverses a discount taken.
     - Customers gain addresses, contacts and status; an inactive customer takes no invoice (FIN-AR-001).
     - Invoice lines take quantity and unit price (FIN-AR-003).
     - `Invoice.void` reverses a posted invoice on which nothing was received, by an entry in an open period (FIN-AR-004).
     - `Receivables.refund` pays back a credit standing alone (FIN-AR-006).
     - Bills record a received date.
  6. **Vendors.**
     - The vendor master holds a trade name, remit-to address, contacts, currency, default account, payment method and status (FIN-AP-001).
     - It holds TIN, TIN type, W-9 and its verification, and backup withholding. A 1099 vendor without a TIN gets a warning on its bill (FIN-AP-002).
     - Bank details change through `request_bank_change` and `apply_bank_change`. A controller who did not request the change approves it (`incompatible`, FIN-AP-003, FIN-CT-001). Payment runs and proposals hold the vendor's items meanwhile.
     - TINs and account numbers are `confidential` and masked `partial` for the role `payables_clerk` (STD-LGS-052, FIN-SC-004).
     - `BankAccount` maps one bank account to one GL account (FIN-BK-001).
  7. **Form 1099.**
     - Each payment writes its reportable amounts to the ledger `Form1099Amount`, by the bill lines' forms (a line may override the vendor's or say `none`). Card payments are left out, and a void writes the negative (FIN-AP-020).
     - The 1099 report reads that ledger by vendor and form against the year's threshold. FIN-EXP-14 still ties out, and a test year's threshold makes V800 reportable (FIN-AP-021).
     - `review_1099` lists the paid vendors with their exceptions (FIN-AP-022).
  8. **Sales tax.**
     - `TaxJurisdiction` holds jurisdictions, and a tax code lists its `components`, taken from the sample's note. The tax is allocated to them in the ledger `TaxPart`, and the tax report shows the parts (FIN-TX-001).
     - A document is taxed at the code's version valid on its date, so a rate change is a version from its effective date.
     - A credit memo applied to an invoice is taxed at the rate of that invoice's date (FIN-TX-005).
  9. **Assigned.** The two Should gaps go to PH-2 step 12, with the statement and e-filing formats: foreign-currency bank accounts (FIN-FX-006) and 1099 corrections (FIN-AP-023).
- **Consequences:**
  - Fourteen new tests pass in memory and on PostgreSQL 16, in `sample-ledger.yaml`, `sample-documents.yaml` and `sample-vendors.yaml`. All 77 finance tests pass, with script coverage 1.0. Every earlier tie-out passes unchanged, and the January tax report now also ties out its state and local parts.
  - The table: of 130 Must requirements, 63 are tested by name, 27 covered, 7 met by the platform and 33 assigned to later steps. None is a gap. Of the 153 requirements, 68 are tested by name.
  - Two kernel gaps found in step 10 (DEC-153, this record) are fixed in code and wait for their conformance vectors.
- **Open, assigned:**
  - the conformance vectors of the kernel fixes of DEC-153 and this record, the business-refusal code (DEC-153) and `read_only` on the version before a change (DEC-154): the next BPA release candidate;
  - the assignments of `fin-status.txt`: their steps.
- **Affects:** DEC-147…154, FIN-GL-003, -004, -011, -022, FIN-PC-003, FIN-AR-001…004, -006, FIN-AP-001…003, -012, -020…023, FIN-BK-001, FIN-TX-001, -005, FIN-RP-001, FIN-CT-001, FIN-SC-004, FIN-FX-006, STD-KIND-029, STD-LGS-042, STD-LGS-052

