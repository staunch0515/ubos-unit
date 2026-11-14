---
id: UBS-REQ-04-06
title: Scenarios — Basic Finance (First Business Package)
status: draft
phase: PH-2
depends_on: [UBS-REQ-04]
---

# Scenarios — Basic Finance (SCN-501 … SCN-512)

These scenarios are the platform-side acceptance backbone of the Basic Finance Buk
(DEC-022). The detailed, implementation-neutral finance requirements (`FIN-*`) live in
`docs/finance-requirements/` (DEC-026); these scenarios show how the platform's imposed
capabilities appear in finance work. All amounts are USD unless stated. The sample company
is **Northwind Components, Inc.**, a US C-corporation with a calendar fiscal year.

## Domain context (informative)

| Class | Kind | Notes |
|---|---|---|
| `Account`, `AccountType`, `FiscalCalendar`, `Period` | Definition | chart of accounts (hierarchical), fiscal calendar, open/soft-closed/closed periods |
| `JournalEntry` (header) | Definition with lifecycle | `draft → submitted → approved → posted`, or `rejected`; posting creates ledger lines |
| `GlLine` | Ledger | append-only, balanced per posting group, gap-free sequence per ledger |
| `Customer`, `Vendor` (extend `Party`), `Item`, `TaxCode` | Definition | master data, bitemporal; currencies are kernel reference data and FX rates objects of the genesis class `FxRate` (DSN-DVM-410, DEC-099) |
| `Invoice`, `CreditMemo`, `Receipt`, `Bill`, `Payment` | Definition with lifecycle | subledger documents that post to `GlLine` |
| `BankAccount`, `BankStatementLine`, `ReconMatch` | Definition / Ledger | statement import and immutable match records |
| `FixedAsset`, `DepreciationSchedule` | Definition | book depreciation (straight-line, declining balance) |
| `TrialBalance`, `BalanceSheet`, `IncomeStatement`, `CashFlowStatement`, `EquityStatement` | report definitions | reproducible report runs |
| `AccountBalance`, `CustomerBalance`, `VendorBalance` | Projection | derived from `GlLine` and subledgers |

### SCN-501 — Set up the company books
- **Goal:** A controller sets up Northwind's books from a standard US chart of accounts and opening balances, reviewed before publication.
- **Personas:** PER-Controller, PER-Accountant, PER-Approver
- **Systems:** WSP, STU, BRG, DVM
- **Phase:** PH-2
- **Preconditions:** 1. A VAE with the genesis and `finance` Buks installed. 2. A spreadsheet of the legacy trial balance at 2025-12-31.
- **Main flow:**
  1. The controller starts a change "Set up books" → a draft is opened.
  2. The controller selects the standard chart of accounts template, adds 12 company-specific accounts and sets the fiscal calendar (calendar year, monthly periods).
  3. The accountant imports the legacy trial balance → opening balances are proposed as one balanced opening entry dated 2025-12-31.
  4. The change set is submitted → the approver sees the chart diff, the opening entry and a check that debits equal credits, then approves → the books are published.
- **Alternate and failure flows:**
  - A1. The imported trial balance does not balance → the import is rejected with the difference shown; nothing is posted.
  - A2. An imported account code has no mapping → the row is reported, and the change set cannot be submitted.
- **Postconditions (observable):** 1. The trial balance at 2025-12-31 equals the legacy trial balance to the cent. 2. The chart of accounts has a published version with its approver recorded.
- **Business rules exercised:** FR-LEDG-051, FR-LEDG-052, FR-MIG-011, FR-MIG-033, FR-MIG-051, FR-VER-031, FR-FLOW-041
- **Verification:** SCN, USE

### SCN-502 — Journal entry with maker–checker approval
- **Goal:** A manual journal entry is prepared, approved by a different person and posted; posted lines can never be edited.
- **Personas:** PER-Accountant, PER-Controller
- **Systems:** WSP, DVM
- **Phase:** PH-2
- **Preconditions:** 1. SCN-501 completed. 2. Approval rule: manual entries above 10,000 USD need controller approval.
- **Main flow:**
  1. The accountant enters an accrual entry of 25,000 USD with two lines and attaches the support document.
  2. The entry is submitted → the controller receives an approval task with the entry, its support and the effect on the trial balance.
  3. The controller approves → the entry is posted; the ledger lines receive consecutive sequence numbers.
- **Alternate and failure flows:**
  - A1. The accountant tries to approve their own entry → refused (segregation of duties).
  - A2. After posting, someone tries to change an amount → refused; the only way is a reversal entry, which references the original.
  - A3. The entry changes after approval but before posting → the approval becomes invalid, and the task reopens.
- **Postconditions (observable):** 1. The audit trail shows preparer, approver, times and the approved version. 2. The general-ledger sequence has no gaps.
- **Business rules exercised:** FR-LEDG-011, FR-LEDG-021, FR-LEDG-041, FR-LEDG-051, FR-FLOW-041, FR-FLOW-043, FR-FLOW-044, FR-FLOW-045
- **Verification:** SCN, SEC

### SCN-503 — Customer invoice to cash receipt
- **Goal:** Sales are invoiced with state sales tax, cash receipts are applied, and the aging is correct at any date.
- **Personas:** PER-ReceivablesClerk, PER-Accountant
- **Systems:** WSP, DVM
- **Phase:** PH-2
- **Preconditions:** 1. Customers in Texas and Oregon; tax codes for both states.
- **Main flow:**
  1. The clerk issues an invoice to the Texas customer with taxable and non-taxable lines → tax is computed by the tax code; the invoice posts to receivables, revenue and sales tax payable.
  2. The clerk issues a credit memo for a returned item and applies it.
  3. A receipt arrives; the clerk applies it partly to the invoice → the remaining balance ages from the invoice due date.
  4. The clerk prints the customer statement and the aging report as of month end.
- **Alternate and failure flows:**
  - A1. A receipt is applied to the wrong customer → the application is reversed and re-applied; both actions are visible in history.
- **Postconditions (observable):** 1. The receivables control account equals the sum of open customer balances. 2. Aging as of any past date is reproducible.
- **Business rules exercised:** FR-LEDG-013, FR-LEDG-031, FR-RULE-041, FR-TIME-031, FR-OFFICE-081
- **Verification:** SCN

### SCN-504 — Vendor bill to payment with 1099 tracking
- **Goal:** Bills are entered, approved and paid once; 1099-reportable payments are tracked.
- **Personas:** PER-PayablesClerk, PER-Controller
- **Systems:** WSP, DVM, NOD
- **Phase:** PH-2
- **Preconditions:** 1. Vendor "Delta Consulting LLC" marked 1099-NEC reportable with a TIN on file.
- **Main flow:**
  1. The clerk enters three bills (one for Delta Consulting) and codes them to expense accounts.
  2. Bills above the approval limit route to the controller; after approval they become payable.
  3. The clerk prepares a payment run → a payment file (ACH, ISO 20022 pain.001) is produced, and the bills are marked paid.
  4. At year end the 1099-NEC report lists Delta Consulting with the paid total.
- **Alternate and failure flows:**
  - A1. A bill with the same vendor, invoice number and amount is entered twice → the duplicate is blocked.
- **Postconditions (observable):** 1. The payables control account equals open vendor balances. 2. Each payment references the bills it pays.
- **Business rules exercised:** FR-LEDG-013, FR-FLOW-041, FR-INT-074, FR-RULE-031, FR-ANL-031
- **Verification:** SCN

### SCN-505 — Bank reconciliation with statement import
- **Goal:** The operating bank account is reconciled from an imported statement; matches are immutable and explainable.
- **Personas:** PER-Accountant, PER-Controller
- **Systems:** WSP, BRG, NOD, DVM
- **Phase:** PH-2
- **Preconditions:** 1. January activity from SCN-502…504. 2. A BAI2 or camt.053 statement for January.
- **Main flow:**
  1. The accountant imports the statement → lines are ingested once, even if the file is imported again.
  2. Matching rules match receipts, payments and fees; 40 lines remain open.
  3. The accountant matches the rest manually, records bank fees as a new entry, and completes the reconciliation → a reconciliation report shows book balance, bank balance and reconciling items.
- **Alternate and failure flows:**
  - A1. A match is undone → a new record supersedes the old one; the history shows both.
- **Postconditions (observable):** 1. Reconciled book balance equals statement balance adjusted for outstanding items. 2. The reconciliation report is reproducible later.
- **Business rules exercised:** FR-LEDG-071, FR-LEDG-072, FR-LEDG-073, FR-LEDG-074, FR-INT-071, FR-INT-072
- **Verification:** SCN

### SCN-506 — Month-end close and financial statements
- **Goal:** January is closed with accruals and depreciation, then locked; the statements are produced and reproducible.
- **Personas:** PER-Accountant, PER-Controller, PER-Executive
- **Systems:** WSP, DVM, NOD
- **Phase:** PH-2
- **Preconditions:** 1. SCN-502…505 completed. 2. Two fixed assets (straight-line, 5 years; double-declining, 7 years).
- **Main flow:**
  1. The depreciation run posts January depreciation for both assets.
  2. The close checklist shows open items (unposted entries, unreconciled accounts) → the accountant resolves them.
  3. The controller soft-closes, reviews the trial balance and closes January → a sealed close artefact records balances and the state root.
  4. The controller runs the balance sheet, income statement, cash flow statement (indirect method) and statement of stockholders' equity; drills down from a statement line to entries.
- **Alternate and failure flows:**
  - A1. The close checklist has an unreconciled bank account → close is refused.
- **Postconditions (observable):** 1. The statements re-run later from their run records produce identical output. 2. The balance sheet balances, and the cash flow statement reconciles to the change in cash.
- **Business rules exercised:** FR-LEDG-061, FR-LEDG-062, FR-ANL-031, FR-ANL-032, FR-AUD-021, FR-FLOW-011
- **Verification:** SCN, USE

### SCN-507 — Posting to a closed period and governed reopen
- **Goal:** Closed periods are protected; corrections go to the current period or through a governed reopen.
- **Personas:** PER-Accountant, PER-Controller
- **Systems:** WSP, DVM
- **Phase:** PH-2
- **Preconditions:** 1. January closed (SCN-506).
- **Main flow:**
  1. The accountant tries to post an entry dated January 20 → refused ("period closed").
  2. The accountant posts the correction in February as a prior-period adjustment with a reference to the January item.
  3. For a material error, the controller requests a reopen of January → a change set with reason and approver is required; after approval January reopens, the entry posts, and January is closed again with a new close artefact that supersedes the first.
- **Alternate and failure flows:**
  - A1. The reopen is not approved → January stays closed.
- **Postconditions (observable):** 1. Both close artefacts exist; the first is marked superseded. 2. Reports "as known at the first close" still reproduce the original January statements.
- **Business rules exercised:** FR-LEDG-061, FR-LEDG-063, FR-TIME-031, FR-VER-031
- **Verification:** SCN, SEC

### SCN-508 — Back-dated correction and "as known on" reporting
- **Goal:** A back-dated correction is visible in current reports while prior reports remain reproducible as originally issued.
- **Personas:** PER-Controller, PER-ExternalAuditor
- **Systems:** WSP, DVM
- **Phase:** PH-2
- **Preconditions:** 1. February is open; a February 5 trial balance was sent to the bank.
- **Main flow:**
  1. On February 20 the accountant records a correction effective February 3.
  2. The controller runs the trial balance "as of February 5, as known today" and "as of February 5, as known on February 5" → the two differ exactly by the correction.
  3. The impact report lists reports and derived values that used the old figures.
- **Postconditions (observable):** 1. The bank's February 5 figures are reproducible byte for byte.
- **Business rules exercised:** FR-TIME-021, FR-TIME-031, FR-TIME-034, FR-TIME-041, FR-TIME-042
- **Verification:** SCN

### SCN-509 — Accounting rule change simulated against history
- **Goal:** A change to the approval threshold and to an account-mapping rule is simulated on last quarter before publication.
- **Personas:** PER-Controller, PER-BusinessAnalyst
- **Systems:** STU, DVM
- **Phase:** PH-2
- **Preconditions:** 1. One quarter of activity.
- **Main flow:**
  1. The controller lowers the manual-entry approval threshold from 10,000 to 5,000 USD and changes the expense-coding rule for software subscriptions.
  2. The simulation replays the quarter → it reports 37 entries that would have needed approval and 12 bills that would have been coded differently.
  3. The change set is approved and published with an effective date of the next month.
- **Postconditions (observable):** 1. Entries before the effective date keep their original rule version, which is visible in their explanation.
- **Business rules exercised:** FR-RULE-071, FR-RULE-072, FR-RULE-073, FR-TIME-064, FR-RULE-022
- **Verification:** SCN, USE

### SCN-510 — Foreign-currency invoice and period-end revaluation
- **Goal:** A EUR invoice is booked at the transaction-date rate and revalued at month end under ASC 830.
- **Personas:** PER-ReceivablesClerk, PER-Accountant
- **Systems:** WSP, DVM
- **Phase:** PH-2
- **Preconditions:** 1. EUR/USD rates loaded daily.
- **Main flow:**
  1. The clerk invoices a German customer 50,000 EUR → the functional-currency amount uses the invoice-date rate.
  2. At month end the revaluation run posts unrealised gain or loss using the closing rate.
  3. The receipt next month posts realised gain or loss and reverses the unrealised amount.
- **Alternate and failure flows:**
  - A1. A rate is corrected retroactively → the revaluation re-run shows the difference; the original run stays reproducible.
- **Postconditions (observable):** 1. Transaction and functional amounts are both stored on every line.
- **Business rules exercised:** FR-LEDG-053, FR-LOC-021, FR-LOC-022, FR-TIME-034
- **Verification:** SCN

### SCN-511 — Year-end close and CPA audit drill
- **Goal:** Year-end close is performed, and an external CPA audits the ledger using evidence packs, reproducible reports and the open verifier.
- **Personas:** PER-Controller, PER-ExternalAuditor, PER-Accountant
- **Systems:** WSP, NTY, DVM
- **Phase:** PH-3
- **Preconditions:** 1. A full year of activity (WL-FIN-M). 2. Anchoring active.
- **Main flow:**
  1. December is closed; the year-end close posts net income to retained earnings and opens the new year.
  2. The auditor receives an auditor role and requests: trial balance roll-forward, journal entries above a threshold, entries posted after close, user access list and approval evidence.
  3. The system produces an evidence pack with reports, entries, approvals, anchor receipts and proofs; the auditor verifies it offline with the open verifier.
  4. The auditor re-performs a sample of statements from their run records and obtains identical outputs.
- **Alternate and failure flows:**
  - A1. The auditor modifies a byte of the pack → verification fails and names the item.
  - A2. A litigation hold on one vendor's records blocks their disposal under the retention schedule.
- **Postconditions (observable):** 1. The auditor signs the drill report stating that the evidence was sufficient and verifiable.
- **Business rules exercised:** FR-PROOF-041, FR-PROOF-051, FR-PROOF-061, FR-AUD-041, FR-AUD-061, FR-ANL-032
- **Verification:** SCN, PILOT

### SCN-512 — Finance agents under review and Copilot explanations
- **Goal:** A bank-reconciliation agent and an AP coding agent do routine work as proposals; Copilot explains a variance from the ledger.
- **Personas:** PER-AiAgent, PER-AgentSupervisor, PER-Accountant, PER-PayablesClerk
- **Systems:** AGT, WSP, DVM
- **Phase:** PH-3
- **Preconditions:** 1. Agents `recon-bot` and `ap-coder` with scoped roles and daily budgets.
- **Main flow:**
  1. `ap-coder` proposes account codes for 60 new bills with confidence and reasons, on its own branch.
  2. The clerk reviews the change set, corrects 4 codes and accepts the rest.
  3. The controller asks Copilot why travel expense rose 40% in March → Copilot answers with cited entries and the rule versions involved.
- **Alternate and failure flows:**
  - A1. `ap-coder` tries to approve a bill → refused; agents cannot approve or merge their own work.
- **Postconditions (observable):** 1. Accepted items record the agent, model version and approving human, and can be reverted as a set.
- **Business rules exercised:** FR-AI-021, FR-AI-025, FR-AI-031, FR-AI-054, FR-AI-064, FR-VER-031
- **Verification:** SCN, PILOT
