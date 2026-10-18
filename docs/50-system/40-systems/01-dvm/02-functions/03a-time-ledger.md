---
id: UBS-SYS-DVM-02-03
title: DVM — Time and Ledger Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-02, UBS-STD-04, UBS-STD-06, UBS-STD-07]
---

# DVM — Time and Ledger Design (DSN-DVM-3xx, DSN-DVM-4xx)

This file designs the crates `ubos-time` and `ubos-ledger`; `ubos-txn` is in `03b-transactions.md`. Reading order:
1. transaction time and timelines (301–312);
2. retroactive changes and impact (320–326);
3. rule bindings and future effect (330–334);
4. ledger storage and appends (401–409);
5. projections and balances (420–426);
6. periods, double entry and matching (440–446).

## Transaction time and timelines

### DSN-DVM-301 — Transaction time assignment
- **Statement:** Transaction time MUST be assigned by the sequencer, under the branch lock, as `max(host clock now, parent commit tx_time + 1 µs)` (STD-TIME-001). The host clock MUST come from CTR-002. If the host clock is behind the parent by more than the configured skew limit (default 500 ms), the flush MUST fail with `NOD.CLOCK_SKEW` instead of silently advancing time.
- **Rationale:** Monotonic transaction time per branch (FR-TIME-012) without trusting clients.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a host clock that jumps back 100 ms, when committing, then transaction time still increases.
  2. Given a clock 2 s behind, when committing, then `NOD.CLOCK_SKEW` is returned, and an operator alert is raised.
- **Verification:** SIM, FAULT
- **Origin:** FR-TIME-011, FR-TIME-012, FR-TIME-013, STD-TIME-001, CR-SEC-002

### DSN-DVM-302 — Timeline representation
- **Statement:** The timeline of a Definition object MUST be stored as the sequence of its versions, each with a valid period `[valid_from, valid_to)`. `temporal_index` MUST hold one row per `(object, valid segment, cseq interval)`. A GiST (PostgreSQL) or R*-tree (SQLite) index over `(valid_from, valid_to)` MUST support point and range lookups.
- **Rationale:** Bitemporal reads at index cost (NR-PERF-007).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 1,000,000 objects with 10 segments each, when reading at random `(asof, cseq)` points, then the overhead versus a current read meets NR-PERF-007.
- **Verification:** BENCH, PROP
- **Origin:** FR-TIME-021, FR-TIME-025, STD-TIME-010, STD-TIME-011, NR-PERF-007

### DSN-DVM-303 — Timeline edit algebra
- **Statement:** `ubos-time` MUST implement the four timeline operations (`supersede_from`, `set_period`, `replace_future`, `end`) as pure functions `(timeline, op) → (new timeline, segment changes)`. Operations MUST NOT create overlapping segments (`TIME.OVERLAPPING_VALIDITY`). Business code MUST NOT write the reserved temporal fields directly (`TIME.RESERVED_FIELD`).
- **Rationale:** Correct, reviewable timeline edits (FR-TIME-022).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given random sequences of the four operations, when applied, then segments never overlap, and the timeline covers exactly the union of intended periods (property test).
  2. Given the timeline vectors, when applied, then results equal the vectors.
- **Verification:** PROP, CONF
- **Origin:** FR-TIME-022, STD-TIME-010, STD-TIME-011

### DSN-DVM-304 — Granularity and business time zone
- **Statement:** Valid-time values MUST be stored as UTC instants. Each class MAY declare a valid-time granularity (instant or date). For date granularity, dates MUST be converted to instants at midnight in the VAE's business time zone, using the IANA time-zone database version recorded in the commit.
- **Rationale:** Date-based business rules without ambiguity (FR-TIME-023).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a VAE in `America/New_York` and a date across a DST change, when stored and read, then the business date is preserved.
- **Verification:** CONF
- **Origin:** FR-TIME-023, STD-FND-024, NR-LOC-003

### DSN-DVM-305 — As-of resolution
- **Statement:** A read with `(cseq, asof)` MUST select the `temporal_index` row where `cseq_from ≤ cseq < cseq_to` and `valid_from ≤ asof < valid_to` (STD-TIME-012). If none exists, the result MUST be "not found at time" (`QRY.NOT_FOUND_AT_TIME`) rather than a generic not found.
- **Rationale:** Precise answers for historical questions.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given the as-of vectors, when resolved, then results equal the vectors, including not-found cases.
- **Verification:** CONF
- **Origin:** FR-TIME-031, STD-TIME-012

### DSN-DVM-306 — Consistent temporal context for references
- **Statement:** The read context MUST carry `(branch snapshot, asof)` into reference resolution, traversals and expressions, so that referenced objects are read at the same coordinates (STD-TIME-013). A reference MAY override `asof` only through an explicit selector.
- **Rationale:** FR-TIME-032.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a fund whose manager changed on 1 March, when read as of 15 February with the manager expanded, then the February manager is returned.
- **Verification:** CONF
- **Origin:** FR-TIME-032, STD-TIME-013

### DSN-DVM-307 — Range queries
- **Statement:** Temporal range queries (STD-TIME-014) MUST return, for each matching object, the list of segments intersecting the range, clipped to the range, in valid-time order.
- **Rationale:** FR-TIME-033.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given a range covering three segments, when queried, then three clipped segments are returned in order.
- **Verification:** CONF
- **Origin:** FR-TIME-033, STD-TIME-014

### DSN-DVM-308 — Evaluation evidence
- **Statement:** Every read result MUST be able to produce evaluation evidence (STD-TIME-015): the commit ID, `asof`, the version IDs used, and the model hash. Reproducing a result from its evidence MUST give identical bytes.
- **Rationale:** Reproducible answers (FR-TIME-034).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given evidence recorded one year earlier, when the read is replayed, then the bytes are identical.
- **Verification:** CONF, PROP
- **Origin:** FR-TIME-034, STD-TIME-015, CR-SOX-006

### DSN-DVM-309 — Valid time on ledger entries
- **Statement:** Ledger entries MUST carry a valid time (the business date) and inherit the commit's transaction time. Bitemporal inclusion (STD-TIME-020) MUST be decided by `(entry.valid_at ≤ asof) ∧ (entry.cseq ≤ cseq)`.
- **Rationale:** FR-TIME-024.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a backdated posting committed today, when reading the balance as of last month at yesterday's commit, then the posting is excluded; at today's commit it is included.
- **Verification:** CONF, PROP
- **Origin:** FR-TIME-024, STD-TIME-020

## Retroactive changes and impact

### DSN-DVM-320 — Retroactivity detection
- **Statement:** The flush MUST mark every change whose valid time starts before the commit's transaction time (minus a configurable grace of 0 by default) as retroactive and record the earliest affected valid instant per object. Retroactive changes to classes whose policy requires review MUST require a change set (STD-TIME-030).
- **Rationale:** Retroactive changes are recognised and governed (FR-TIME-041).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a backdated price change on `main` for a review-required class, when flushed directly, then it is rejected with `VER.CHANGE_SET_REQUIRED`.
- **Verification:** CONF
- **Origin:** FR-TIME-041, STD-TIME-030

### DSN-DVM-321 — Impact set computation
- **Statement:** The impact set of a retroactive change (STD-TIME-031) MUST be computed by a breadth-first walk of `derivation_edge` from the changed versions, restricted to derived results whose evaluation valid time is at or after the earliest affected instant. The walk MUST be bounded by a configurable depth and size and MUST report truncation.
- **Rationale:** FR-TIME-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a price change affecting 3 NAVs and 12 investor allocations, when the impact set is computed, then all 15 are listed with paths.
- **Verification:** CONF, PROP
- **Origin:** FR-TIME-042, STD-TIME-031

### DSN-DVM-322 — Restatement on a correction branch
- **Statement:** The DVM MUST provide `restate(impact set)` that creates a correction branch, re-runs the producing processes of the impacted results with their recorded inputs replaced by the corrected ones, and records the new results with their original valid times (FR-TIME-044).
- **Rationale:** FR-TIME-043.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given the impact set above, when restated, then the correction branch contains 15 new versions with unchanged valid times, and the diff to main shows only these.
- **Verification:** SCN, CONF
- **Origin:** FR-TIME-043, FR-TIME-044

### DSN-DVM-323 — Compensation proposals
- **Statement:** When restatement changes values that were posted to ledgers, the DVM MUST compute compensation proposals (adjustment entries per ledger key, STD-KIND-023) as the difference between original and restated postings, dated per policy (original date or current open period).
- **Rationale:** FR-TIME-045.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a restated fee that changes by +120, when proposals are computed, then one adjustment of +120 on the right ledger key is proposed.
- **Verification:** CONF
- **Origin:** FR-TIME-045, STD-KIND-023

### DSN-DVM-324 — Upcoming changes
- **Statement:** The DVM MUST answer "what becomes effective between t1 and t2" from `temporal_index` by a range scan on `valid_from`.
- **Rationale:** FR-TIME-053.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given 10 future-dated changes, when listing the next 30 days, then those in range are returned in order.
- **Verification:** CONF
- **Origin:** FR-TIME-053

### DSN-DVM-325 — Became-effective emission
- **Statement:** The DVM MUST expose `due_effective(t_from, t_to)` returning segments whose `valid_from` falls in the interval. The host (NOD) MUST call it on a schedule and emit became-effective events through the outbox (STD-TIME-050). No job is needed for the state itself to take effect (FR-TIME-051).
- **Rationale:** Future changes take effect by reading, events are best-effort notifications with a declared latency.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a future change due at 09:00, when no scheduler runs, then reads after 09:00 still return the new value.
  2. Given the scheduler running, when 09:00 passes, then the event is emitted within NR-PERF-012.
- **Verification:** CONF, BENCH
- **Origin:** FR-TIME-051, FR-TIME-052, STD-TIME-050, NR-PERF-012

### DSN-DVM-326 — Retroactivity formal model
- **Statement:** The timeline algebra, bitemporal inclusion and impact set MUST be modelled formally with the properties: no overlapping segments; past `(cseq, asof)` answers never change; impact set is a superset of results whose inputs changed.
- **Rationale:** Bitemporal correctness is subtle.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the model with 3 objects and 5 operations, when checked, then no property is violated.
- **Verification:** FORM
- **Origin:** STD-TIME-011, STD-TIME-031, NR-DET-001

## Rule bindings and re-evaluation

### DSN-DVM-330 — Rule binding capture
- **Statement:** During a process, the runtime MUST record every sheet, decision, logic asset and model version used, as a `RuleBinding` (STD-TIME-040). The flush MUST store the binding set once per commit (deduplicated by hash) and reference it from each derived version.
- **Rationale:** FR-TIME-061, FR-TIME-062.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a derived fee, when inspected, then the sheet, decision and logic versions used are listed.
- **Verification:** CONF
- **Origin:** FR-TIME-061, FR-TIME-062, STD-TIME-040

### DSN-DVM-331 — Re-evaluation modes
- **Statement:** Re-evaluation of a derived result MUST support the modes of STD-TIME-041: `as-recorded` (default; the original bindings), `as-of-valid-time` (bindings effective at the result's valid time) and `current` (current bindings). The mode used MUST be recorded on the new version.
- **Rationale:** Old data follows old logic by default (FR-TIME-064).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a fee rule changed in March, when a January fee is re-evaluated in the default mode, then the January rule is used.
- **Verification:** CONF
- **Origin:** FR-TIME-063, FR-TIME-064, STD-TIME-041

## Ledger storage and appends

### DSN-DVM-401 — Ledger entry table
- **Statement:** Ledger entries MUST be stored in `ledger_entry`, partitioned by tenant and keyed by `(branch, ledger class, ledger key, seq)`. Rows MUST be insert-only: the adapter MUST revoke UPDATE and DELETE on the table for the application role, and a trigger MUST reject them (`LEDG.IMMUTABLE_ENTRY`).
- **Rationale:** Append-only enforced in depth (STD-KIND-021).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given the application role, when an UPDATE on `ledger_entry` is attempted, then the database rejects it.
- **Verification:** SEC, CONF
- **Origin:** FR-LEDG-011, FR-LEDG-012, STD-KIND-020, STD-KIND-021

### DSN-DVM-402 — Gap-free sequence allocation
- **Statement:** Sequence numbers per ledger key MUST be allocated under the branch sequencer lock from a `ledger_sequence` counter row updated in the flush transaction. Because the flush is atomic, an aborted flush MUST leave no gap; a non-transactional counter (for example a PostgreSQL `SEQUENCE`) does not qualify, since it is not rolled back (VER-FORM-0004, DEC-046). Within a batch, the next sequence of a key counts the appends accepted earlier in the same batch. Sequences on child branches MUST be local to the branch and reassigned on merge (DSN-DVM-244).
- **Rationale:** FR-LEDG-041, STD-KIND-022.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given 10,000 appends with 5% injected flush failures, when sequences are verified, then they are gap-free.
- **Verification:** FAULT, PROP, FORM
- **Origin:** FR-LEDG-041, FR-LEDG-042, STD-KIND-022

### DSN-DVM-403 — Append cost independent of history
- **Statement:** An append MUST touch only the counter row, the entry row, the projection delta rows and the tree path of the entry key. It MUST NOT read prior entries.
- **Rationale:** FR-LEDG-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given ledger keys with 10 and with 10,000,000 entries, when appending, then latencies differ by less than 10%.
- **Verification:** BENCH
- **Origin:** FR-LEDG-014, NR-SCAL-007

### DSN-DVM-404 — Ledger appends and concurrency
- **Statement:** Appends MUST NOT take part in object-level compare-and-set: two processes appending to the same key MUST both succeed, ordered by the sequencer (STD-TXN-024). Only balance invariants (DSN-DVM-424) can cause a conflict, and they MUST be re-checked in the flush against the latest balance.
- **Rationale:** High-contention accounts without false conflicts.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given 200 concurrent appends to one cash account, when flushed, then all succeed with consecutive sequences.
- **Verification:** SIM, BENCH, FORM
- **Origin:** STD-TXN-024, FR-LEDG-042

### DSN-DVM-405 — Reversal and adjustment
- **Statement:** `commit.reverse` MUST append an entry with negated amounts that references the original, and MUST record the reversal in a `ledger_reversal` unique index so that a second reversal fails with `LEDG.ALREADY_REVERSED`. `commit.adjust` MUST append a correcting entry referencing its target; a missing target MUST fail with `LEDG.CORRECTION_TARGET_MISSING`. Non-reversible entry types MUST fail with `LEDG.REVERSAL_NOT_REVERSIBLE`.
- **Rationale:** FR-LEDG-021, FR-LEDG-022, FR-LEDG-023.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given two concurrent reversals of the same entry, when flushed, then exactly one succeeds.
- **Verification:** CONF, SIM
- **Origin:** FR-LEDG-021, FR-LEDG-022, FR-LEDG-023, STD-KIND-023, STD-ISA-015

### DSN-DVM-406 — Correction chains
- **Statement:** The DVM MUST provide `correction_chain(entry)` returning the original entry and all reversals and adjustments that reference it, transitively, in sequence order.
- **Rationale:** FR-LEDG-024.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given an entry adjusted twice and then reversed, when the chain is read, then four entries are returned in order.
- **Verification:** CONF
- **Origin:** FR-LEDG-024

### DSN-DVM-407 — Entry correlation
- **Statement:** Entries MUST carry a correlation ID and the source process ID. The DVM MUST index entries by correlation ID for retrieval across ledgers.
- **Rationale:** FR-LEDG-013.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a subscription posting to 4 ledgers, when queried by correlation ID, then the 4 entries are returned.
- **Verification:** CONF
- **Origin:** FR-LEDG-013

### DSN-DVM-408 — Sequence verification
- **Statement:** The DVM MUST provide `verify_sequences(ledger, key range)` that checks gap-freeness, total order and the hash chain of entries per key, and reports violations.
- **Rationale:** FR-LEDG-043.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given an injected gap, when verified, then the key and missing number are reported.
- **Verification:** FAULT
- **Origin:** FR-LEDG-043

### DSN-DVM-409 — Multi-currency postings
- **Statement:** Entries MUST carry money values with currency (STD-FND-023). Balances MUST be kept per currency. Arithmetic across currencies MUST fail with `MODEL.CURRENCY_MISMATCH` unless an explicit conversion with a recorded rate reference is used.
- **Rationale:** FR-LEDG-053.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given USD and EUR postings on one key, when the balance is read, then two currency balances are returned.
- **Verification:** CONF
- **Origin:** FR-LEDG-053, STD-FND-023

### DSN-DVM-410 — Currency reference data and bitemporal FX rates
- **Statement:** Currencies (ISO 4217 with minor units) MUST be kernel reference data (FR-LOC-021). FX rates MUST be Definition objects keyed by `(source, base, quote, rate type)` with valid time per rate date, so that conversions at any `(asof, cseq)` are reproducible (FR-LOC-022). A conversion MUST record the rate version used in the evaluation evidence.
- **Rationale:** FR-LOC-021, FR-LOC-022; restatements depend on the rates known at the time.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given an FX rate corrected retroactively, when a past conversion is re-read at its original commit, then the original rate is used; at the current commit, the corrected rate is used.
- **Verification:** CONF, PROP
- **Origin:** FR-LOC-021, FR-LOC-022, FR-TIME-034

## Projections and balances

### DSN-DVM-420 — Balance projection as deltas
- **Statement:** Declared balance projections (FR-LEDG-031) MUST be maintained in the flush as rows `(projection, key, valid_at, cseq, delta)` in `balance_delta`. Checkpoint rows `(projection, key, valid_at, cseq, balance)` MUST be written by a background task every N deltas per key (default 1,000). A balance read MUST sum deltas after the nearest checkpoint satisfying the bitemporal inclusion rule.
- **Rationale:** Bitemporal balances with bounded read cost (FR-LEDG-032).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given 10,000,000 entries on one key, when reading balances at random `(asof, cseq)` points, then p95 latency meets NR-PERF-001 plus 5 ms.
  2. Given random workloads, when balances are recomputed from entries, then they equal the projected balances.
- **Verification:** BENCH, PROP
- **Origin:** FR-LEDG-031, FR-LEDG-032, FR-LEDG-033, AR-016
- **Phase note:** PH-2 step 3 (DEC-087): moved from PH-1 with FR-LEDG-031.

### DSN-DVM-421 — Backdated entries and checkpoints
- **Statement:** A backdated delta (valid_at before a checkpoint's valid_at) MUST NOT modify existing checkpoints. Reads MUST include such deltas by their `(valid_at, cseq)` coordinates. The checkpoint task MUST create new checkpoints at later `cseq` that include them.
- **Rationale:** Checkpoints are immutable, derived data (AR-015).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a backdated posting before a checkpoint, when reading as of a date after the posting at the latest commit, then it is included; at an earlier commit, it is not.
- **Verification:** PROP
- **Origin:** FR-LEDG-032, AR-015

### DSN-DVM-422 — Generic projections
- **Statement:** Besides balances, classes MAY declare projections as a fold of `(group key, aggregate: sum | count | min | max | last)` over entries or over Definition versions. The flush MUST maintain `sum` and `count` incrementally; `min`, `max` and `last` MUST be maintained incrementally for insert-only sources and recomputed per group otherwise.
- **Rationale:** Common aggregates stay cheap without an external engine.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a count-by-status projection, when objects change status, then counts stay equal to a full recount.
- **Verification:** PROP
- **Origin:** FR-LEDG-031, AR-016

### DSN-DVM-423 — Projection rebuild
- **Statement:** Every projection MUST be rebuildable from its sources (`rebuild_projection(name, branch)`), writing into a shadow table and swapping atomically. The verifier (DSN-DVM-092) MUST cover projections.
- **Rationale:** Derived data is rebuildable (AR-015).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a corrupted projection, when rebuilt, then it equals the verifier's expected values, and reads were never served from the partial shadow table.
- **Verification:** FAULT
- **Origin:** AR-015, NR-DUR-005, FR-ANL-023

### DSN-DVM-424 — Invariants over balances
- **Statement:** Invariants over balances (for example "cash ≥ 0", FR-LEDG-034) MUST be evaluated in the flush on the balances after the batch's deltas are applied. Violations MUST abort only the offending process (`RULE.INVARIANT_VIOLATED`).
- **Rationale:** Exact enforcement under concurrency.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a balance of 100 and two concurrent withdrawals of 60, when flushed, then exactly one succeeds.
- **Verification:** SIM, CONF, FORM
- **Origin:** FR-LEDG-034, STD-LGS-030

### DSN-DVM-425 — Typed projection tables
- **Statement:** Classes MAY declare typed projection tables (FR-ANL-011): a flat table per class and branch in a separate `projection_<vae>` schema with typed columns, maintained in the flush (`sync` mode) or by a projector following the outbox (`async` mode) (FR-ANL-012). Projection tables MUST be marked derived, carry the source `cseq` watermark, and MUST never be read by the commit path (FR-ANL-013, AR-015).
- **Rationale:** Fast SQL-friendly reads without making tables authoritative (AR-018).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given an async projection, when read, then its watermark is available, and rows equal a rebuild at that watermark.
  2. Given a dropped projection schema, when rebuilt, then it equals the previous content.
- **Verification:** CONF, PROP
- **Origin:** FR-ANL-011, FR-ANL-012, FR-ANL-013, AR-015, AR-018

### DSN-DVM-426 — Report definitions and reproducible runs
- **Statement:** Report definitions (FR-ANL-031) MUST be Definition objects combining saved queries, projections and a layout. Each run MUST record `(report version, snapshot map, asof, parameters)` and a hash of the output, so that re-running with the same record yields the same output (FR-ANL-032). Scheduled distribution (FR-ANL-033) MUST use jobs and permission-aware delivery.
- **Rationale:** FR-ANL-031, FR-ANL-032, CR-SOX-006.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a report run from last quarter, when re-run from its run record, then the output hash is identical.
- **Verification:** CONF
- **Origin:** FR-ANL-031, FR-ANL-032, FR-ANL-033, CR-SOX-006

## Periods, double entry and matching

### DSN-DVM-440 — Balanced posting groups
- **Statement:** Entries appended in one process with the same posting-group ID MUST be checked in the flush for balance per currency (debits = credits, STD-KIND-024). An unbalanced group MUST fail with `LEDG.UNBALANCED`.
- **Rationale:** FR-LEDG-051.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a group off by 0.01, when flushed, then `LEDG.UNBALANCED` reports the difference.
- **Verification:** CONF
- **Origin:** FR-LEDG-051, STD-KIND-024

### DSN-DVM-441 — Chart of accounts
- **Statement:** Accounts MUST be Definition objects of a kernel base class with a hierarchy (parent account). Postings MUST reference leaf accounts. Roll-up balances MUST be computed by summing leaf projections over the account subtree (using a pre-order interval like DSN-DVM-107).
- **Rationale:** FR-LEDG-052.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a 4-level chart, when reading a top-level balance, then it equals the sum of its leaves.
- **Verification:** CONF, PROP
- **Origin:** FR-LEDG-052

### DSN-DVM-442 — Periods and locks
- **Statement:** Periods (STD-KIND-025) MUST be Definition objects with states open, soft-closed and closed. The flush MUST reject entries whose valid time falls in a closed period (`LEDG.PERIOD_CLOSED`) and entries in a soft-closed period without the override capability.
- **Rationale:** FR-LEDG-061.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a closed March, when posting a March entry, then it fails; an April entry succeeds.
- **Verification:** CONF
- **Origin:** FR-LEDG-061, STD-KIND-025, CR-SOX-005

### DSN-DVM-443 — Close artefacts
- **Statement:** Closing a period MUST produce a close artefact: the balances per key at close, the commit ID, and the state root, sealed (STD-PROOF-055) as one object.
- **Rationale:** FR-LEDG-062.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a closed period, when the artefact is verified, then its balances equal balances read at the close commit.
- **Verification:** CONF
- **Origin:** FR-LEDG-062, STD-PROOF-055, CR-SOX-005, CR-SEC-006

### DSN-DVM-444 — Governed reopen
- **Statement:** Reopening a closed period MUST require a change set approved per the reopen decision, and MUST record the reason and the superseded close artefact.
- **Rationale:** FR-LEDG-063.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a reopen without approval, when flushed, then it fails with `VER.CHANGE_SET_REQUIRED`.
- **Verification:** CONF
- **Origin:** FR-LEDG-063, CR-SOX-005

### DSN-DVM-445 — Matching engine
- **Statement:** The DVM MUST provide a deterministic matching operator: given two entry sets and a matching rule (keys, tolerances, date windows, many-to-one allowed), it produces match records and breaks. Match records MUST be immutable objects (FR-LEDG-073); un-matching MUST create a new record that supersedes the old one.
- **Rationale:** FR-LEDG-071, FR-LEDG-072, FR-LEDG-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given the matching vectors, when matched twice, then results are identical, and breaks equal the vectors.
- **Verification:** CONF, PROP
- **Origin:** FR-LEDG-071, FR-LEDG-072, FR-LEDG-073

### DSN-DVM-446 — Idempotent statement ingestion
- **Statement:** Statement lines MUST be ingested with an idempotency key derived from `(source, account, statement ID, line number, content hash)`, so a re-delivered statement creates no duplicates.
- **Rationale:** FR-LEDG-074.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, BRG
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given the same bank statement ingested three times, when counted, then each line exists once.
- **Verification:** CONF
- **Origin:** FR-LEDG-074, STD-TXN-040
