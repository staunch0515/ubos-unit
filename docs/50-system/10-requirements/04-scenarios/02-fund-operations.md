---
id: UBS-REQ-04-02
title: Scenarios — Fund Operations (Deferred, DEC-022)
status: complete
phase: PH-2
depends_on: [UBS-REQ-04]
---

# Scenarios — Fund Operations (SCN-101 … SCN-110)

> **Deferred (DEC-022).** These scenarios are kept for a later business package and are not
> assigned to any phase exit.

## Domain context (informative)

The Fund Operations Buk models the following classes. Kinds follow IMP-01.

| Class | Kind | Notes |
|---|---|---|
| `Fund`, `ShareClass`, `FeeSchedule`, `InvestmentRestriction`, `DealingCalendar` | Definition | master data and rules, bitemporal |
| `Investor` (extends `Party`), `InvestorAccount` | Definition | investor register; personal fields encrypted |
| `DealingOrder` (subscription, redemption, switch) | Definition with lifecycle | `received → validated → accepted → priced → settled`, or `rejected` / `cancelled` |
| `UnitMovement`, `CashMovement`, `JournalEntry`, `FeeAccrual` | Ledger | append-only postings |
| `Price` | Definition | bitemporal market data; FX rates are objects of the genesis class `FxRate` (DSN-DVM-410, DEC-099) |
| `NavCalculation` | Definition (result) | one per share class and valuation date, with lineage |
| `Holding`, `UnitBalance`, `AccountBalance` | Projection | derived from ledger classes |

### SCN-101 — Launch a new share class
- **Status:** Deferred (DEC-022)
- **Goal:** A new share class is configured, reviewed and published for a future launch date.
- **Personas:** PER-FundOperationsManager, PER-ComplianceOfficer, PER-Approver
- **Systems:** STU, WSP, DVM
- **Phase:** PH-2
- **Preconditions:** 1. Fund `GLOBAL-EQ` exists with share classes A and I.
- **Main flow:**
  1. The operations manager starts a change named "Launch class R" → a draft is opened. The user interface says "draft", not "branch".
  2. The manager creates `ShareClass R` with currency, minimum investment, fee schedule (a new version derived from class A's schedule) and dealing calendar, all valid from 2026-11-02.
  3. The change set is submitted → the compliance review task shows the diff and the simulation result: no existing investor is affected, and the fee schedule passes all restriction rules.
  4. The compliance officer and the approver approve → the change is published, and class R is visible for `asof ≥ 2026-11-02` only.
- **Alternate and failure flows:**
  - A1. The simulation shows that class R's fee exceeds the prospectus maximum rule → submission is blocked with an explanation, and the manager must correct the draft.
- **Postconditions (observable):** 1. Dealing orders for class R with a dealing date before 2026-11-02 are rejected with `FLOW.GUARD_FAILED` ("class not yet launched").
- **Business rules exercised:** FR-VER-031, FR-TIME-051, FR-RULE-071, FR-FLOW-041, FR-UX-041
- **Verification:** SCN, USE, PILOT

### SCN-102 — Subscription order to settled units
- **Status:** Deferred (DEC-022)
- **Goal:** A subscription is processed before cut-off, units are allotted at the correct NAV, and the register is updated.
- **Personas:** PER-InvestorServicesClerk, PER-FundAccountant
- **Systems:** WSP, DVM, BRG
- **Phase:** PH-2
- **Preconditions:** 1. Investor account `INV-778` passes KYC. 2. The class A cut-off is 12:00 America/New_York on dealing days.
- **Main flow:**
  1. At 11:40 local time the clerk records a subscription of 250,000 USD → the order is `received` with dealing date today.
  2. Validation rules run (KYC valid, minimum met, account not blocked) → `validated`, then `accepted`.
  3. After NAV is published (SCN-103), the order is priced → a `UnitMovement` of +2,381.4526 units and a `CashMovement` are appended, and the order becomes `priced`.
  4. The settlement confirmation arrives through the Bridge → `settled`.
- **Alternate and failure flows:**
  - A1. The order is recorded at 12:05 → the dealing date moves to the next business day by the dealing calendar, and the clerk sees the reason.
  - A2. KYC expires between `accepted` and `priced` → the order is held, and an exception task is created.
- **Postconditions (observable):** 1. The `UnitBalance` projection for `INV-778` includes the new units. 2. The register as-of today matches the ledger.
- **Business rules exercised:** FR-FLOW-011, FR-FLOW-031, FR-LEDG-011, FR-LEDG-031, FR-LOC-041, FR-INT-041
- **Verification:** SCN, PILOT

### SCN-103 — Daily NAV calculation with lineage
- **Status:** Deferred (DEC-022)
- **Goal:** NAV per share class is calculated, checked, approved, published and fully explainable.
- **Personas:** PER-FundAccountant, PER-FundOperationsManager, PER-InternalAuditor
- **Systems:** DVM, WSP, BRG
- **Phase:** PH-2
- **Preconditions:** 1. Prices and FX rates for the valuation date are loaded (Definition, bitemporal). 2. Holdings come from ledger projections.
- **Main flow:**
  1. The scheduled NAV job starts at 18:00 → for each share class a `NavCalculation` is computed by verified logic and decision tables: valuation, accruals, fees, class allocation.
  2. Tolerance rules compare to the previous NAV → one class moves by 3.1% against a 2% tolerance, and an exception task is created.
  3. The accountant inspects lineage → the explanation lists the price moves that contributed most, with price source and version.
  4. The accountant accepts the exception with a comment, and the manager approves publication → NAVs are published, and a `NavPublished` event is emitted to distribution channels.
- **Alternate and failure flows:**
  - A1. A price is missing → the calculation for the affected class stops with `FUND.PRICE_MISSING`, and the other classes continue.
- **Postconditions (observable):** 1. Each published NAV records the logic versions, rule versions, input object versions and the process ID. 2. Replay reproduces the NAV exactly (SCN-011).
- **Business rules exercised:** FR-EVT-041, FR-RULE-041, FR-AUD-021, FR-TIME-061, FR-EVT-011
- **Verification:** SCN, PILOT, PROP

### SCN-104 — Back-dated price correction and NAV restatement
- **Status:** Deferred (DEC-022)
- **Goal:** A wrong price discovered later is corrected, and every affected NAV and investor transaction is identified and restated.
- **Personas:** PER-FundAccountant, PER-FundOperationsManager, PER-ComplianceOfficer
- **Systems:** DVM, WSP
- **Phase:** PH-2
- **Preconditions:** 1. On 2026-10-08 it is discovered that the price of security X for 2026-10-02 was wrong. 2. NAVs from 2026-10-02 to 2026-10-07 were published, and 37 dealing orders were priced on them.
- **Main flow:**
  1. The accountant commits a corrected price with valid time 2026-10-02 → the platform identifies 5 affected `NavCalculation` objects and 37 orders by lineage (dependency on the price version).
  2. The platform computes restated NAVs on a correction branch → the diff shows old and new NAV per date and class.
  3. The materiality rule (the error exceeds 0.5% of NAV) classifies the error as material → the restatement workflow requires compliance approval, and investor compensation entries are proposed.
  4. After approval, the restated NAVs are published as new versions with the same valid time. Compensation `UnitMovement` and `CashMovement` entries are appended and reference the original movements.
- **Alternate and failure flows:**
  - A1. The error is below the materiality threshold → the NAVs are restated, no compensation is proposed, and the decision is recorded with its rule version.
- **Postconditions (observable):** 1. A query `asof=2026-10-03&time=2026-10-07` returns the original NAV. 2. A query `asof=2026-10-03&time=now` returns the restated NAV. 3. The ledger holds no updated entries, only new entries.
- **Business rules exercised:** FR-TIME-041, FR-AUD-021, FR-LEDG-021, FR-RULE-041, FR-FLOW-041
- **Verification:** SCN, PILOT, PROP

### SCN-105 — Fee calculation in Smart Grid
- **Status:** Deferred (DEC-022)
- **Goal:** Management and performance fees are computed by shared logic and reviewed in a spreadsheet-like view.
- **Personas:** PER-FundAccountant
- **Systems:** WSP, DVM
- **Phase:** PH-2 (calculation), PH-3 (Smart Grid view)
- **Preconditions:** 1. The fee schedule uses a tiered decision table and a high-water mark rule.
- **Main flow:**
  1. The month-end fee job appends `FeeAccrual` entries per class.
  2. In PH-3, the accountant opens the Smart Grid "Fees October" → rows are accruals, and computed columns come from the shared fee logic, not from cell formulas.
  3. The accountant overrides one accrual with a reason → the override is committed as an adjustment entry, and the column logic is unchanged.
- **Alternate and failure flows:**
  - A1. The accountant edits a computed column formula in the grid → it is refused. The grid offers to open the shared logic in Studio as a change set.
- **Postconditions (observable):** 1. The totals in the grid equal the ledger projection.
- **Business rules exercised:** FR-RULE-041, FR-LEDG-021, FR-OFFICE-041
- **Verification:** SCN, USE

### SCN-106 — Investment restriction change simulated against history
- **Status:** Deferred (DEC-022)
- **Goal:** A compliance rule change is evaluated against 12 months of data before publication.
- **Personas:** PER-ComplianceOfficer, PER-FundOperationsManager
- **Systems:** STU, DVM, FRG
- **Phase:** PH-2
- **Preconditions:** 1. The restriction "max 10% in a single issuer" is active. 2. A change to 8% is proposed.
- **Main flow:**
  1. The officer edits the rule on a draft and runs simulation over 12 months of daily holdings.
  2. The simulation report lists 14 historical breaches that the new rule would have caused, on 9 dates, with issuers and positions.
  3. The officer sets the valid time of the new rule to next quarter start and adds a transition declaration.
  4. The change set is approved and published.
- **Alternate and failure flows:**
  - A1. The simulation exceeds the budget of the simulation profile → it stops with partial results and asks for a narrower range.
- **Postconditions (observable):** 1. The simulation run is stored as an object attached to the change set, as evidence for the approval.
- **Business rules exercised:** FR-RULE-071, FR-DEV-041, FR-TIME-021, FR-VER-031
- **Verification:** SCN, BENCH

### SCN-107 — Month-end close and a post-close correction
- **Status:** Deferred (DEC-022)
- **Goal:** A period is closed, and a later correction is handled without reopening it.
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Systems:** DVM, WSP
- **Phase:** PH-2
- **Preconditions:** 1. October 2026 is closed.
- **Main flow:**
  1. The accountant closes October → the ledgers for October are locked, and the trial-balance projection is frozen as a tagged state.
  2. In November, an October accrual error is found → the correction entry is posted in November with valid time referencing October and the corrected entry.
  3. The October report "as originally closed" still shows the closed figures. The October report "as known now" shows the correction.
- **Alternate and failure flows:**
  - A1. Posting with a transaction date in October after close → rejected with `LEDG.PERIOD_CLOSED`.
- **Postconditions (observable):** 1. The close record is signed by the closing principal.
- **Business rules exercised:** FR-LEDG-061, FR-LEDG-021, FR-VER-061, FR-TIME-031
- **Verification:** SCN, PILOT

### SCN-108 — Custodian cash reconciliation with breaks
- **Status:** Deferred (DEC-022)
- **Goal:** Daily custodian statements are matched against internal cash ledgers, and breaks are managed.
- **Personas:** PER-FundAccountant
- **Systems:** BRG, DVM, WSP
- **Phase:** PH-2
- **Preconditions:** 1. A custodian statement file (ISO 20022 camt.053 or CSV) arrives by SFTP daily.
- **Main flow:**
  1. The Bridge connector ingests the statement as a `StatementLine` ledger class, with recorded file hash.
  2. Matching rules (amount, value date, reference, tolerance) match 1,183 of 1,190 lines.
  3. 7 breaks create tasks, grouped by rule. The accountant resolves 5 by matching manually with a reason and books 2 adjustments.
- **Alternate and failure flows:**
  - A1. The same file arrives twice → it is detected by hash, and no duplicate lines are created.
- **Postconditions (observable):** 1. Every match records rule version and actor. 2. The reconciliation status projection is 100% resolved.
- **Business rules exercised:** FR-LEDG-071, FR-INT-041, FR-TXN-031, FR-FLOW-051
- **Verification:** SCN, PILOT

### SCN-109 — Examination request answered with an evidence pack
- **Status:** Deferred (DEC-022)
- **Goal:** An examiner receives verifiable books and records for a question within hours.
- **Personas:** PER-ComplianceOfficer, PER-RegulatorExaminer
- **Systems:** NTY, DVM, WSP
- **Phase:** PH-3
- **Preconditions:** 1. Anchoring has run daily since go-live.
- **Main flow:**
  1. The examiner asks for the state of fund `GLOBAL-EQ` (holdings, NAVs, investor register totals, active restrictions) as known on 2026-10-05 for valid date 2026-10-02.
  2. The officer creates an evidence pack with scope and bitemporal coordinates → the pack contains objects, versions, rule versions, signatures, Merkle inclusion proofs and anchor receipts.
  3. The examiner verifies the pack with the open verifier → every proof passes, and the anchors match the external timestamps.
- **Alternate and failure flows:**
  - A1. The scope includes personal data → the pack redacts fields by policy, and the redaction is itself provable (hash of the redacted value included).
- **Postconditions (observable):** 1. Pack creation is logged. 2. The pack is reproducible from the same parameters.
- **Business rules exercised:** FR-PROOF-051, FR-PROOF-061, FR-PROOF-041, FR-TIME-031, FR-IAM-061
- **Verification:** SCN, PILOT, AUDIT

### SCN-110 — Fee schedule change in the middle of a period
- **Status:** Deferred (DEC-022)
- **Goal:** A fee rate change effective mid-month is prorated correctly, including when approved after the effective date.
- **Personas:** PER-FundAccountant, PER-FundOperationsManager
- **Systems:** DVM
- **Phase:** PH-2
- **Preconditions:** 1. The management fee is 1.00% p.a. 2. A reduction to 0.85% effective 2026-10-15 is approved on 2026-10-20 (late).
- **Main flow:**
  1. The approval commits the new fee schedule version with valid time from 2026-10-15.
  2. The platform identifies the daily accruals 2026-10-15 … 2026-10-19 computed with the old rate (rule-version binding) → adjustment accruals are proposed.
  3. Month-end fee totals equal 14 days at 1.00% plus 17 days at 0.85%.
- **Alternate and failure flows:**
  - A1. The new version's valid time overlaps an existing future version → rejected with `TIME.OVERLAPPING_VALIDITY`.
- **Postconditions (observable):** 1. Each accrual shows the fee schedule version used.
- **Business rules exercised:** FR-TIME-041, FR-TIME-061, FR-LEDG-021
- **Verification:** SCN, PROP
