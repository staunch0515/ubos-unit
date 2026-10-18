---
id: UBS-REQ-05-03
title: Functional Requirements — TIME (Bitemporal) and LEDG (Ledger State)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-01, UBS-REQ-05-02]
---

# Functional Requirements — TIME and LEDG

Terminology used in this file:
- **Transaction time** is when the system learned a fact (commit time).
- **Valid time** is when the fact is true in the business world.
- `time=` selects transaction time. `asof=` selects valid time.

## CAP-TIME-01 — Transaction time

### FR-TIME-011 — Kernel-assigned transaction time
- **Statement:** The DVM MUST assign the transaction time of every commit at commit, as a UTC instant with microsecond precision. Clients and logic MUST NOT be able to set it.
- **Rationale:** Transaction time is evidence, so it cannot be supplied by the writer.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor, PER-RegulatorExaminer
- **Acceptance:**
  1. Given a client payload containing `transaction_time`, when committed, then the value is ignored or rejected with `TIME.RESERVED_FIELD`, and the kernel time is recorded.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-14; NEW — trusted time-stamping for SEC 17a-4 recordkeeping

### FR-TIME-012 — Monotonic transaction time per branch
- **Statement:** Transaction times MUST be strictly increasing along each branch's first-parent history, even when the host clock moves backwards. The DVM MUST use a hybrid logical clock and MUST raise an operational alert when skew exceeds a configured bound.
- **Rationale:** `time=` queries require a total order that matches commit order.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator, PER-KernelEngineer
- **Acceptance:**
  1. Given the host clock set back 5 seconds, when commits continue, then their transaction times still increase, and an alert `NOD.CLOCK_SKEW` is emitted.
- **Verification:** SIM, FAULT
- **Origin:** NEW — correctness of transaction-time travel under clock faults.

### FR-TIME-013 — Every version inherits its commit's transaction time
- **Statement:** Every object version and ledger entry MUST carry the transaction time of the commit that created it.
- **Rationale:** Uniform "as known at" semantics.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a commit with 5 changes, when the versions are read, then all 5 show the same transaction time as the commit.
- **Verification:** CONF
- **Origin:** L40:SPEC-14

## CAP-TIME-02 — Valid time

### FR-TIME-021 — Valid-time periods on Definition versions
- **Statement:** Every Definition-kind version MUST carry a valid-time period `[valid_from, valid_to)`. `valid_to` may be open. When the writer does not specify `valid_from`, it MUST default to the commit's transaction time, converted to the class's valid-time granularity.
- **Rationale:** Effective dating is required for prices, fees, contracts, roles and rules (IMP-02).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-ComplianceOfficer
- **Acceptance:**
  1. Given a version written without valid time on 2026-10-09, when read, then `valid_from` is 2026-10-09 (date granularity) and `valid_to` is open.
- **Verification:** CONF
- **Origin:** IMP-02

### FR-TIME-022 — Timeline edit operations
- **Statement:** Valid-time changes MUST be expressed with three operations:
  - `supersede_from(date)`: effective from the date until the next already-known change;
  - `set_period(from, to)`: effective exactly for the period;
  - `replace_future(from)`: effective from the date, removing later known versions from the current timeline.

  A `set_period` that overlaps other current periods of the same object MUST be rejected with `TIME.OVERLAPPING_VALIDITY`.
- **Rationale:** Explicit operations avoid ambiguous bitemporal updates (SCN-110).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-BusinessArchitect
- **Acceptance:**
  1. Given versions from 2026-01-01 and 2026-11-01, when `supersede_from(2026-10-15)` is committed, then the new version is effective for `[2026-10-15, 2026-11-01)`.
  2. Given an overlapping `set_period`, when committed, then it fails with `TIME.OVERLAPPING_VALIDITY`.
- **Verification:** CONF, PROP
- **Origin:** IMP-02

### FR-TIME-023 — Valid-time granularity and business time zone
- **Statement:** Each class MUST declare its valid-time granularity (`date` or `instant`). Date granularity MUST be interpreted in the business time zone of the VAE unless the object declares its own time-zone field.
- **Rationale:** "Effective 1 November" means midnight in New York for a US fund.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given VAE time zone America/New_York, when `asof=2026-11-01` is resolved, then it covers 2026-11-01T04:00Z to 2026-11-02T05:00Z (with the DST change applied).
- **Verification:** CONF
- **Origin:** NEW — correctness of date-granular effective dating.

### FR-TIME-024 — Valid time on ledger entries
- **Statement:** Every ledger entry MUST carry a valid time (for example value date or effective date), which may differ from its transaction time.
- **Rationale:** Back-valued postings are normal in finance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a posting on 2026-10-09 with value date 2026-10-07, when balances `asof=2026-10-08` are computed, then the posting is included only for `time` values after 2026-10-09.
- **Verification:** CONF, PROP
- **Origin:** IMP-01, IMP-02

### FR-TIME-025 — Object timeline
- **Statement:** The platform MUST return an object's timeline: the sequence of valid-time periods as currently known, each with its version, and optionally the timelines as known at earlier transaction times.
- **Rationale:** Users and auditors need to see effective history at a glance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, SDK
- **Personas:** PER-BusinessUser, PER-InternalAuditor
- **Acceptance:**
  1. Given SCN-006, when the timeline is requested, then two periods are listed with their versions.
- **Verification:** CONF
- **Origin:** IMP-02

## CAP-TIME-03 — Bitemporal queries

### FR-TIME-031 — Reads and queries with `asof` and `time`
- **Statement:** Every read and query MUST accept `asof` (valid time) and `time` (transaction time) independently. Defaults are `asof = now` in the VAE business time zone and `time = head`.
- **Rationale:** This is the core bitemporal capability (SCN-007).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, SDK
- **Personas:** PER-InternalAuditor, PER-FundAccountant
- **Acceptance:**
  1. Given SCN-007, when both queries run, then the pre-correction and corrected values are returned respectively.
- **Verification:** CONF, PROP
- **Origin:** IMP-02

### FR-TIME-032 — References resolve in the same temporal context
- **Statement:** When a read or query follows a reference or relationship, the target MUST be resolved at the same `asof` and `time` as the source, unless the reference pins a specific version.
- **Rationale:** Mixed temporal contexts produce inconsistent pictures.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given an order that references a share class whose fee changed later, when read `asof` the order date, then the fee effective at that date is returned.
- **Verification:** CONF, PROP
- **Origin:** IMP-02

### FR-TIME-033 — Temporal range queries
- **Statement:** The platform MUST answer "versions valid during [a, b)" and "changes recorded during [c, d)" queries, and their combination, for an object, a class or a query result.
- **Rationale:** Period reporting and change audits.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-DataAnalyst, PER-InternalAuditor
- **Acceptance:**
  1. Given prices changed 3 times in October, when "valid during October" is queried, then the 3 periods are returned with their boundaries clipped to October.
- **Verification:** CONF
- **Origin:** IMP-02

### FR-TIME-034 — Reproducible temporal answers
- **Statement:** Every bitemporal answer MUST include the commit IDs (one per branch) that it was evaluated against. Repeating the query with those commits MUST return identical results.
- **Rationale:** Evidence must be reproducible (SCN-109).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given a result with `evaluated_at` commits, when re-queried a year later with those commits, then it is byte-identical.
- **Verification:** CONF, PROP
- **Origin:** IMP-02, IMP-03

## CAP-TIME-04 — Retroactive correction

### FR-TIME-041 — Retroactive changes are recognised and governed
- **Statement:** A commit that changes a Definition object with `valid_from` earlier than the start of the current business day MUST be classified as retroactive. Policy MAY require a change set, a reason and approvals for retroactive changes per class.
- **Rationale:** Back-dated changes are high-risk in regulated domains.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-ComplianceOfficer
- **Acceptance:**
  1. Given a policy requiring approval for retroactive `Price` changes, when a back-dated price is committed directly, then it fails with `VER.CHANGE_SET_REQUIRED`.
- **Verification:** CONF, SCN
- **Origin:** IMP-02

### FR-TIME-042 — Impact identification through lineage
- **Statement:** For a retroactive change, the platform MUST identify the derived results and ledger entries whose recorded inputs include a superseded version in the affected valid-time range, transitively through derivation chains.
- **Rationale:** Restatement starts with knowing what is wrong (SCN-104).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given SCN-104, when the correction is committed on a correction branch, then exactly 5 NAV results and 37 orders are identified.
- **Verification:** SCN, PROP
- **Origin:** IMP-02, L40:SPEC-15

### FR-TIME-043 — Restatement on a correction branch
- **Statement:** The platform MUST recompute the identified results on a `correction` branch with the corrected inputs and the logic versions effective at each valid date (FR-TIME-064). It MUST present old and new results as a diff, with materiality evaluated by rules.
- **Rationale:** Restatements are reviewed before publication.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, STU, WSP
- **Personas:** PER-FundAccountant, PER-FundOperationsManager
- **Acceptance:**
  1. Given SCN-104, when the restatement runs, then the diff shows 5 NAV pairs, and the materiality flag is set by the rule.
- **Verification:** SCN
- **Origin:** IMP-02

### FR-TIME-044 — Restated results keep their valid time
- **Statement:** Publishing a restatement MUST create new versions of the restated results with the same valid time as the originals. The original versions MUST remain readable with `time=` before the restatement.
- **Rationale:** "What we published then" and "what is true now" are both answerable.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given SCN-104 postconditions, when both queries run, then the original and restated NAVs are returned respectively.
- **Verification:** CONF, SCN
- **Origin:** IMP-02

### FR-TIME-045 — Compensation proposals for ledger impacts
- **Statement:** When restated results affect ledger entries, the platform MUST propose compensating or adjustment entries by rule, referencing the original entries, for approval with the restatement.
- **Rationale:** Investors are made whole by new entries, never by rewriting.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a material restatement, when proposals are generated, then each affected order receives one compensation proposal with an amount computed by rule.
- **Verification:** SCN
- **Origin:** IMP-01, IMP-02

## CAP-TIME-05 — Future-dated change

### FR-TIME-051 — Future valid time takes effect without jobs
- **Statement:** Versions with a future `valid_from` MUST become effective for `asof = now` reads automatically at the valid time, by read-time resolution, without any scheduled job.
- **Rationale:** No job means no job failure on the effective date (SCN-006).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a future price version and the scheduler stopped, when `now` passes `valid_from`, then reads return the new price.
- **Verification:** CONF, FAULT
- **Origin:** IMP-02

### FR-TIME-052 — Became-effective events
- **Statement:** For classes that declare it, the platform MUST emit a `BecameEffective` event at the valid time of each future-dated version, delivered at least once.
- **Rationale:** Some reactions (notifications, cache warm-up, external sync) must happen when a change takes effect.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a subscribed class and a version effective at 2026-11-01T00:00 local time, when that instant passes, then the event is emitted within the NR-PERF bound.
- **Verification:** CONF, SIM
- **Origin:** NEW — bridge between valid time and the event model.

### FR-TIME-053 — Upcoming changes
- **Statement:** The platform MUST list future-effective versions across a VAE or class, ordered by effective time.
- **Rationale:** Operations teams review what changes next week.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, WSP
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given 3 future-dated changes, when "upcoming" is requested, then they are listed with their effective dates and change sets.
- **Verification:** SCN
- **Origin:** NEW

## CAP-TIME-06 — Rule-version binding

### FR-TIME-061 — Commits record the rules they were checked with
- **Statement:** Every commit MUST record a rule binding: the versions (commit IDs) of the class definitions, governance sheets and logic assets that validated or computed it.
- **Rationale:** "Which rule version accepted this?" must always be answerable.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer, PER-InternalAuditor
- **Acceptance:**
  1. Given SCN-009, when a checked commit is inspected, then its rule binding lists the sheet version in effect.
- **Verification:** CONF
- **Origin:** EXT-WP ("old data follows old logic")

### FR-TIME-062 — Derived results record their producers
- **Statement:** Every object produced by logic MUST record the logic asset versions, decision tables, rule versions and input object versions used to produce it.
- **Rationale:** Lineage and restatement depend on it.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Acceptance:**
  1. Given a NAV result, when inspected, then its producers and inputs are listed with versions.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

### FR-TIME-063 — Re-evaluation modes
- **Statement:** Re-evaluation of logic or rules for a past valid date MUST support three modes:
  - `as_originally`: the recorded binding;
  - `as_effective`: the versions effective at the valid date as known now;
  - `current`: the current versions.

  The mode MUST be recorded on the result.
- **Rationale:** Restatements, audits and what-if analysis need different semantics.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-BusinessAnalyst
- **Acceptance:**
  1. Given a fee rule changed after an accrual date, when re-evaluated in each mode, then the three results correspond to the three rule versions.
- **Verification:** CONF, PROP
- **Origin:** EXT-WP, IMP-02

### FR-TIME-064 — Old data follows old logic by default
- **Statement:** When the platform recomputes results for a past valid date (restatement, replay of a valid-dated process), the default mode MUST be `as_effective`.
- **Rationale:** Historical transactions are processed with the rules that applied to them (EXT-WP §2.2).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a restatement of September after an October rule change, when run with defaults, then the September rules are applied.
- **Verification:** CONF
- **Origin:** EXT-WP

## CAP-LEDG-01 — Ledger classes

### FR-LEDG-011 — Ledger class declaration
- **Statement:** A Ledger-kind class MUST declare its ledger key (the fields that partition sequences, for example `account` or `fund`), its entry fields, its amount fields with sign conventions, and its correction policy.
- **Rationale:** Uniform semantics for postings, movements, events and decisions.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-FundAccountant
- **Acceptance:**
  1. Given `CashMovement` keyed by `account`, when entries are appended for two accounts, then each account has its own sequence.
- **Verification:** CONF
- **Origin:** IMP-01

### FR-LEDG-012 — Append-only instruction
- **Statement:** Ledger entries MUST be created only through the append instruction, which assigns sequence number, transaction time and entry ID. Update and delete instructions MUST be rejected for Ledger kind.
- **Rationale:** The instruction set itself enforces immutability.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given logic calling update on an entry, when executed, then it fails with `LEDG.IMMUTABLE_ENTRY`.
- **Verification:** CONF
- **Origin:** IMP-01

### FR-LEDG-013 — Entry correlation
- **Statement:** Every entry MUST reference its originating process and MAY reference source objects (for example the dealing order). The platform MUST support queries from a source object to its entries and back.
- **Rationale:** Traceability from business event to accounting impact.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Acceptance:**
  1. Given a priced order, when its entries are queried, then the unit and cash movements are returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

### FR-LEDG-014 — Append cost independent of history
- **Statement:** Appending an entry MUST NOT require reading or rewriting other entries of the ledger, except for the synchronous projections and invariants declared on it.
- **Rationale:** Ledger throughput must not degrade with history size.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given ledgers with 1,000 and 100,000,000 entries, when appends are benchmarked, then the latency percentiles are within the same NR-PERF bound.
- **Verification:** BENCH
- **Origin:** IMP-01

## CAP-LEDG-02 — Reversal and adjustment

### FR-LEDG-021 — Reversal
- **Statement:** The reverse instruction MUST append an entry that negates the amounts of the referenced entry, references it, carries a reason and has the valid time of the original unless specified otherwise.
- **Rationale:** Standard correction method (SCN-008).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given entry #1041, when reversed, then the balance effect of #1041 and its reversal is zero at every `asof`.
- **Verification:** CONF, PROP
- **Origin:** IMP-01, EXT-TRI

### FR-LEDG-022 — Adjustment
- **Statement:** An adjustment entry MUST reference the entry or entries it corrects and MUST carry its own amounts and valid time.
- **Rationale:** Partial corrections without reversal and repost.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a 9,000 overstatement, when a −9,000 adjustment referencing #1041 is appended, then both appear in the correction chain.
- **Verification:** CONF
- **Origin:** IMP-01

### FR-LEDG-023 — At most one reversal per entry
- **Statement:** An entry MUST be reversible at most once. A reversal entry MUST NOT itself be reversed. The correct action is a new entry.
- **Rationale:** It prevents reversal loops and ambiguous chains.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a reversed entry, when reversed again, then it fails with `LEDG.ALREADY_REVERSED`.
- **Verification:** CONF
- **Origin:** NEW — unambiguous correction chains.

### FR-LEDG-024 — Correction chains
- **Statement:** The platform MUST return the full correction chain of any entry (original, reversals, adjustments, reposts) in sequence order.
- **Rationale:** Audit reconstruction (SCN-008).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given SCN-008, when the chain of #1041 is requested, then #1041, #1187 and #1188 are returned.
- **Verification:** CONF
- **Origin:** IMP-01

## CAP-LEDG-03 — Balances as projections

### FR-LEDG-031 — Declared balance projections
- **Statement:** A ledger class MUST be able to declare balance projections: group-by keys, aggregated fields and maintenance mode (`sync`, updated in the same commit, or `async`, updated after commit with a watermark).
- **Rationale:** Balances are derived, not stored facts (IMP-01).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-LogicDeveloper
- **Acceptance:**
  1. Given `UnitBalance` grouped by `(investor_account, share_class)` in `sync` mode, when a movement is committed, then the balance read in the same commit's snapshot includes it.
- **Verification:** CONF
- **Origin:** IMP-01, IMP-09
- **Phase note:** PH-2 step 3 (DEC-087): moved from PH-1; declared balance projections are built with double entry and periods. PH-1 computes balances from the entries at read (STD-TIME-020).

### FR-LEDG-032 — Bitemporal balances
- **Statement:** Balance projections MUST answer at any `asof` and `time`, consistent with the bitemporal semantics of the entries.
- **Rationale:** Positions at a past date as known at another date.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-RegulatorExaminer
- **Acceptance:**
  1. Given the back-valued posting of FR-TIME-024, when balances are queried for the four combinations of `asof` and `time`, then the results match the property model.
- **Verification:** PROP, CONF
- **Origin:** IMP-02

### FR-LEDG-033 — Projection freshness
- **Statement:** Every projection read MUST return the watermark (the last commit included). A reader MUST be able to request "at least commit C" and wait up to a timeout.
- **Rationale:** Asynchronous projections must never be mistaken for current.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an async projection lagging by 2 commits, when read with `min_commit` equal to the head, then the read waits or fails with `ANL.PROJECTION_STALE` after the timeout.
- **Verification:** CONF
- **Origin:** IMP-09
- **Phase note:** PH-2 step 3 (DEC-087): moved from PH-1 with the balance projections (FR-LEDG-031).

### FR-LEDG-034 — Invariants over balances
- **Statement:** Invariants over synchronous projections (for example "units ≥ 0" or "cash ≥ −overdraft limit") MUST be evaluated in the commit, and violating commits MUST be rejected.
- **Rationale:** Business safety limits apply at posting time.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a redemption exceeding held units, when committed, then it fails with `RULE.INVARIANT_VIOLATED`.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-18

## CAP-LEDG-04 — Sequencing

### FR-LEDG-041 — Gap-free sequences
- **Statement:** Within each ledger key on a protected branch, entry sequence numbers MUST be contiguous, starting at 1, without gaps, including after crashes and aborted processes.
- **Rationale:** Gap-free numbering is an audit control.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given crash injection during concurrent appends, when recovered, then the sequences have no gaps and no duplicates.
- **Verification:** FAULT, SIM, PROP
- **Origin:** NEW — audit control in financial recordkeeping.

### FR-LEDG-042 — Total order per ledger key
- **Statement:** Entries of the same ledger key MUST have a total order consistent with commit order. Concurrent appends to the same key MUST serialise without lost entries.
- **Rationale:** Running balances depend on order.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 100 concurrent appenders, when completed, then all entries exist in a single order.
- **Verification:** SIM, PROP
- **Origin:** IMP-01

### FR-LEDG-043 — Sequence verification
- **Statement:** The platform MUST provide a verification operation that checks sequence continuity and hash-chain integrity of a ledger key and reports any anomaly.
- **Rationale:** Periodic control evidence.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given an intact ledger, when verified, then the result is "ok" with the count and last sequence.
- **Verification:** CONF
- **Origin:** NEW

## CAP-LEDG-05 — Double entry

### FR-LEDG-051 — Balanced posting groups
- **Statement:** A ledger class MAY declare double-entry mode. Entries are then grouped into posting groups, and the DVM MUST reject any commit whose posting group does not balance (sum of debits equals sum of credits per currency).
- **Rationale:** Accounting correctness enforced by the kernel.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a group with debits 100 and credits 99 USD, when committed, then it fails with `LEDG.UNBALANCED`.
- **Verification:** CONF, PROP
- **Origin:** NEW — first-vertical accounting.

### FR-LEDG-052 — Chart of accounts
- **Statement:** Accounts MUST be Definition objects with type (asset, liability, equity, income, expense), currency rules and hierarchy. Postings MUST reference active accounts at the entry's valid time.
- **Rationale:** Accounting structure is master data with effective dating.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given an account closed from 2026-10-01, when a posting with valid time 2026-10-05 references it, then it fails with `MODEL.REFERENCE_INVALID`.
- **Verification:** CONF
- **Origin:** NEW

### FR-LEDG-053 — Multi-currency postings
- **Statement:** Postings in several currencies MUST reference the FX rate objects and versions used, and base-currency amounts MUST be derivable reproducibly.
- **Rationale:** Funds hold multi-currency positions.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a EUR posting in a USD-based fund, when inspected, then the FX rate version is referenced, and the base amount recomputes identically.
- **Verification:** CONF, PROP
- **Origin:** NEW

## CAP-LEDG-06 — Period close

### FR-LEDG-061 — Periods and locks
- **Statement:** A ledger scope MUST support accounting periods with states `open`, `closing` and `closed`. Entries with a posting date in a closed period MUST be rejected with `LEDG.PERIOD_CLOSED`. Corrections go to an open period and reference the valid time of the original.
- **Rationale:** SCN-107.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given October closed, when an October-dated posting is appended, then it fails with `LEDG.PERIOD_CLOSED`.
- **Verification:** CONF, SCN
- **Origin:** NEW

### FR-LEDG-062 — Close artefacts
- **Statement:** Closing a period MUST create a signed tag and freeze the period's trial-balance projection as an immutable report object.
- **Rationale:** "As originally closed" must be reproducible.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Acceptance:**
  1. Given SCN-107, when the October report "as closed" is requested after a November correction, then the frozen figures are returned.
- **Verification:** SCN
- **Origin:** NEW

### FR-LEDG-063 — Governed reopen
- **Statement:** Reopening a closed period MUST require an approved change set with a reason. The reopen and re-close MUST be recorded as distinct events.
- **Rationale:** Reopen is exceptional and auditable.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-ComplianceOfficer
- **Acceptance:**
  1. Given a reopen without approval, when attempted, then it fails with `VER.CHANGE_SET_REQUIRED`.
- **Verification:** CONF
- **Origin:** NEW

## CAP-LEDG-07 — Reconciliation

### FR-LEDG-071 — Matching rules
- **Statement:** The platform MUST support declarative matching rules between two ledger or statement sources: one-to-one, one-to-many and many-to-one, with exact and tolerance comparisons on amount, date and reference fields, evaluated in priority order.
- **Rationale:** SCN-108.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given SCN-108's rules, when run, then 1,183 of 1,190 lines match, and each match records the rule.
- **Verification:** SCN, CONF
- **Origin:** NEW

### FR-LEDG-072 — Break management
- **Statement:** Unmatched items MUST become breaks with age, owner, category and resolution codes, managed as tasks.
- **Rationale:** Operational control of exceptions.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, WSP
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given 7 breaks, when listed, then each shows age and owner, and resolving requires a code.
- **Verification:** SCN
- **Origin:** NEW

### FR-LEDG-073 — Immutable match records
- **Statement:** Matches and unmatches MUST be recorded as ledger entries that reference both sides, the rule version and the actor.
- **Rationale:** Reconciliation decisions are audit evidence.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a manual match later undone, when audited, then both the match and the unmatch entries are visible.
- **Verification:** CONF
- **Origin:** IMP-01

### FR-LEDG-074 — Idempotent statement ingestion
- **Statement:** Statement files and messages MUST be ingested idempotently by content hash and source identifiers. A repeated ingestion MUST create no duplicate lines.
- **Rationale:** SCN-108 A1.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given the same file twice, when ingested, then the second ingestion reports "duplicate" and appends nothing.
- **Verification:** CONF
- **Origin:** L40:SPEC-15
