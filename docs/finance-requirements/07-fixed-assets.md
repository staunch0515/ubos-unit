---
id: FIN-DOC-07
title: Fixed Assets
status: frozen
---

# Fixed Assets

### FIN-FA-001 — Asset classes
- **Requirement:** The system shall define asset classes with cost account, accumulated-depreciation account, depreciation-expense account, default method, default useful life, default convention and capitalization threshold.
- **Priority:** Must
- **Basis:** ASC 360; accepted practice
- **Acceptance:**
  1. Given the class "Computer equipment" with life 36 months, straight-line, when FA-003 is created in it, then these defaults apply.

### FIN-FA-002 — Asset register
- **Requirement:** The system shall maintain assets with ID, description, class, cost, acquisition source (bill or journal), in-service date, method, useful life, salvage value, convention, location, custodian and status (in service, fully depreciated, disposed), with the history of changes.
- **Priority:** Must
- **Basis:** ASC 360
- **Acceptance:**
  1. Given `fixed-assets.csv`, when it is loaded, then FA-001 … FA-003 exist and the register totals equal accounts 1500, 1510, 1520 and 1590 at 2026-01-31 after the January run.

### FIN-FA-003 — Depreciation methods
- **Requirement:** The system shall compute book depreciation by straight-line and by declining balance (double-declining and 150%) with automatic switch to straight-line when it yields more, and should support units-of-production.
- **Priority:** Must
- **Basis:** ASC 360-10-35
- **Acceptance:**
  1. Given FA-001 (120,000.00, 60 months, straight-line), then monthly depreciation is 2,000.00.
  2. Given FA-002 (70,000.00, 84 months, double-declining), then January 2026 depreciation is 1,666.67 as in FIN-EXP-11.
  3. Given a declining-balance asset in a year where straight-line on the remaining value exceeds declining balance, then straight-line is used from that year.

### FIN-FA-004 — Conventions and rounding
- **Requirement:** The system shall apply a configurable convention per class (full month in the month placed in service; mid-month; next month) and round per asset and month, placing any rounding residue in the last month of each asset year and of the useful life so that total depreciation equals cost minus salvage.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given FA-003 placed in service 2026-01-15 with the full-month convention, then January depreciation is 333.33.
  2. Given FA-003 over its whole life, then the total of all months equals exactly 12,000.00.

### FIN-FA-005 — Depreciation run
- **Requirement:** The system shall run depreciation per period for all assets, post one summarized entry per class and account combination with a detail report per asset, be idempotent for a period, and refuse to run for a closed period.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given the January run, then DEP-2601 of 4,000.00 is posted; a second run for January posts nothing.

### FIN-FA-006 — Changes in estimates
- **Requirement:** The system shall apply changes in useful life or salvage prospectively from the change period, without restating prior periods.
- **Priority:** Must
- **Basis:** ASC 250-10-45-17 (change in estimate)
- **Acceptance:**
  1. Given FA-001's remaining life extended by 12 months from 2026-02, then February depreciation is 70,000.00 / 47 months = 1,489.36 and January is unchanged.

### FIN-FA-007 — Disposal
- **Requirement:** The system shall record disposals (sale, scrap, write-off) with date and proceeds, depreciate up to the disposal date by the convention, remove cost and accumulated depreciation, and post the gain or loss.
- **Priority:** Must
- **Basis:** ASC 360-10-40
- **Acceptance:**
  1. Given FA-002 sold on 2026-01-31 for 60,000.00 after January depreciation, then a gain of 1,666.67 is posted (proceeds 60,000.00 − net book value 58,333.33).

### FIN-FA-008 — Impairment
- **Requirement:** The system should record an impairment loss on an asset by journal with a link to the asset, reducing its carrying amount and future depreciation.
- **Priority:** Could
- **Basis:** ASC 360-10-35 (impairment)
- **Acceptance:**
  1. Given an impairment of 10,000.00 on FA-001, then future monthly depreciation is recomputed on the reduced carrying amount.

### FIN-FA-009 — Asset reports
- **Requirement:** The system shall produce the asset register, the depreciation schedule (past and projected), and the roll-forward of cost and accumulated depreciation (opening, additions, disposals, depreciation, closing) for any period.
- **Priority:** Must
- **Basis:** ASC 360 disclosures; audit
- **Acceptance:**
  1. Given January 2026, when the roll-forward is run, then cost goes from 190,000.00 to 202,000.00 (additions 12,000.00) and accumulated depreciation from 58,000.00 to 62,000.00.

### FIN-FA-010 — Tax depreciation book
- **Requirement:** The system could keep a second depreciation book for US tax (MACRS) for information, without posting it to the general ledger.
- **Priority:** Could
- **Basis:** IRS Publication 946
- **Acceptance:**
  1. Given a tax book with MACRS 5-year for FA-003, then the tax schedule is shown beside the book schedule and nothing is posted.
