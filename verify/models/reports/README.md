# Model-checking report — PH-0 step 2 (EXIT-0-02)

| Item | Value |
|---|---|
| Date | 2026-09-29 |
| Tool | TLC 2.19 of 08 August 2024 (`tla2tools.jar` release 1.7.4, SHA-256 `936a2620…050e88`) |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 4 workers |
| Rule | DEC-031: exhaustive where TLC completes; otherwise the largest completing bound plus 1,000,000 simulated behaviours at the stated bound |
| Verdict | **no property violated** in any run below (after the fix of finding F-1) |

## 1. Runs

| Model | Stated bound (VER item) | Exhaustive run | Distinct states | Depth | Time | Simulation at the stated bound | Evidence |
|---|---|---|---|---|---|---|---|
| `commit_cas` | 3 processes × 2 nodes × 3 objects × 4 commits (VER-FORM-0001) | 3 × 2 × 3 × **2 commits** | 279,220 | 21 | 11 s | 1,000,000 behaviours, depth ≤ 60, 508,210,073 states checked, 17 min | `commit_cas.exhaustive.txt`, `commit_cas.simulation.txt` |
| `merge` | 3 branches × 4 commits × 3 objects × 2 ledger keys (VER-FORM-0002) | 3 × **2 commits** × 3 × 2 | 510,037 | 7 | 44 s | 1,000,000 behaviours, depth ≤ 60, 278,194,526 states checked, 1 h 44 min | `merge.exhaustive.txt`, `merge.simulation.txt` |
| `timeline` | 3 objects × 5 operations × 4 commits (VER-FORM-0003) | 3 × 5 × **2 commits** | 19,478,778 | 9 | 11 min | 1,000,000 behaviours, depth ≤ 60, 396,619,754 states checked, 1 h 16 min | `timeline.exhaustive.txt`, `timeline.simulation.txt` |

Bounds that did not complete (stopped, no violation found before stopping; these runs used an
earlier TLC build, 2026.09.25):

| Model | Bound | Reached before stopping |
|---|---|---|
| `commit_cas` | 3 commits | > 151 million distinct states in 50 min, queue still growing |
| `merge` | 3 commits per branch | > 30 million distinct states in 90 min, queue still growing |
| `timeline` | 4 commits (stated bound) | > 61 million distinct states in 1 h 45 min, stopped by memory pressure |

Symmetry reduction is used over interchangeable model values (objects, fields, ledger keys,
nodes and non-serializable processes). Invariants are listed in `../README.md`.

## 2. Findings

| ID | Model | Finding | Resolution |
|---|---|---|---|
| F-1 | `commit_cas` | A node's open commit group formed before another node's commit has a view older than the snapshot of a process that joins it later. Validating that process against the stale view fails its flush with `TXN.HEAD_MOVED` although none of its objects changed, which contradicts STD-TXN-021 ("the branch head moving for unrelated objects MUST NOT cause failure"). Counterexample (depth 8): p1 commits x1 through n1 while p2 waits in a group of n2 formed at head 0; p1 starts again at head 1 and flushes x1 through n2; n2 validates against head 0 and rejects. | DSN-DVM-208 now states that a sequencer must not validate a process against a view older than its snapshot; the process waits for the next group. The model admits a process into a group only then. |
| F-2 | `timeline` | STD-TIME-011 `set_period` depends on an undefined "version lineage". Under the conservative reading used by the model, `set_period` can never split an existing segment (a mutation of the splitting branch of `Remove` is not detectable because that branch is unreachable). | Open question OQ-004. |
| F-3 | `merge` | STD-VER-040 does not define the base of each recursive merge nor conflict handling inside a virtual base. | Open question OQ-005; the model's rule is proposed for the clause. |

## 3. Sensitivity of the properties (mutation checks)

Each model was changed deliberately and re-checked at reduced bounds; the named property
must fail.

| Model | Mutation | Property that failed |
|---|---|---|
| `commit_cas` | install without head compare-and-set | `LinearHistory` |
| `commit_cas` | no read-set validation for serializable processes | `SerializableReads` |
| `commit_cas` | no base validation | `NoLostUpdate` |
| `commit_cas` | every validation fails | `FailuresJustified` |
| `commit_cas` | admission rule of F-1 removed | `FailuresJustified` (at the stated bounds) |
| `merge` | both-sides change taken from the target without conflict | `ConflictsComplete` |
| `merge` | source entries appended without removing those already present | `LedgerWellFormed` |
| `merge` | merge parents `[source, target]` | `MergeParents` |
| `merge` | merge base computed asymmetrically | `Determinism` |
| `merge` | appended entries numbered from 1 | `LedgerWellFormed` |
| `timeline` | impact range of `supersede_from` reduced to its start date | `ImpactComplete` |
| `timeline` | index not updated at one commit | `IndexMatchesLog` |
| `timeline` | `replace_future` without removing later segments | `NoOverlap` |
| `timeline` | segment split after `b` dropped in `Remove` | not detected — unreachable under OQ-004 reading (F-2) |

## 4. Scope limits

- `merge`: no object creation or natural-key conflicts, only the `three_way` field strategy,
  no ledger corrections (`corrects`, `LEDG.CORRECTION_TARGET_MISSING`), no period locks, no
  validation of the merged result (STD-VER-055).
- `timeline`: one level of derivation for the impact set; valid time has three dates.
- `commit_cas`: one branch; retries limited to two attempts per transaction.
- DSN-BPA-004 (model traces converted into vectors) is part of PH-0 steps 3 and 4.

These limits and the bounds reached are for the external reviewer (DEC-030, DEC-031).

## 5. PH-1 step 2 — group commit and ledger sequences (EXIT-1-07, DEC-046)

| Item | Value |
|---|---|
| Date | 2026-09-30 |
| Tool | TLC 2.19 (`tla2tools.jar` release 1.7.4), as in §1 |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 4 workers |
| Model | `group_commit.tla`, VER-FORM-0004 |
| Verdict | **no property violated** in any run below |

### 5.1 Runs

| Stated bound (DEC-046) | Exhaustive run | Distinct states | Depth | Time | Simulation at the stated bound | Evidence |
|---|---|---|---|---|---|---|
| 3 processes × 2 nodes × 2 objects × 2 ledger keys × batch 3 × 4 commits, 5 transactions, 1 failed storage transaction | 3 × 2 × 2 × 2 × 3 × **2 commits**, 3 transactions | 551,470 | 21 | 27 s | 1,000,000 behaviours, depth ≤ 60, 737,314,895 states checked, 1 h 22 min | `group_commit.exhaustive.txt`, `group_commit.simulation.txt` |

The CI configuration (1 object, 1 ledger key, 3 commits, 4 transactions) completes
exhaustively: 398,687 distinct states, depth 29, 14 s.

Bound that did not complete: 3 commits with 4 transactions, stopped after 34 min with
43,338,581 distinct states and 17,618,872 states still on the queue, no violation found
before stopping.

### 5.2 Findings

None. The model confirms the two-phase flush of W-8 as now stated in DSN-DVM-006. Writing it
made two points of the design explicit (DEC-046): the ledger counter must be transactional
(DSN-DVM-402; a PostgreSQL `SEQUENCE` is not rolled back, see mutation `no_rollback`), and
sequences and balances inside a batch must count the appends accepted earlier in the same
batch (DSN-DVM-402, DSN-DVM-424; mutations `seq_from_storage`, `pre_batch_balance`).

### 5.3 Sensitivity of the properties (mutation checks)

Each mutation is selected by the constant `Mutation` and checked at the CI bounds against
its target property alone; the property must fail.

| Mutation | Meaning | Property that failed |
|---|---|---|
| `no_rollback` | the counter row advances although the storage transaction failed | `GapFree` |
| `seq_from_storage` | sequences allocated from storage only, ignoring earlier appends of the batch | `GapFree`, `PerKeyOrder` |
| `pre_batch_balance` | balance invariant evaluated on the stored balance, ignoring earlier deltas of the batch | `BalanceSafe`, `SerialEquivalent` |
| `no_batch_view` | bases validated against storage only, ignoring earlier writes of the batch | `SerialEquivalent` |
| `appends_cas` | appends take part in compare-and-set on the counter | `NoFalseConflict` |
| `whole_batch_abort` | one failing process aborts every process of its batch | `FailuresJustified` |
| `partial_install` | the last commit of a batch is stored without its object writes | `ProcessAtomic` |

Command (per mutation `M` and property `P`): copy `group_commit.ci.cfg`, set
`Mutation = "M"` and `INVARIANTS P`, and run TLC on it.

### 5.4 Scope limits

- One branch; the branch lock is held from sequencing to the end of the storage transaction,
  as DSN-DVM-006 requires. The lock-free head compare-and-set between nodes is checked by
  `commit_cas` (§1, DSN-DVM-208); leader leases and forwarding are not modelled.
- One balance invariant per key (`balance >= 0`) with deposits and withdrawals of one amount;
  at most one append per key and transaction.
- Child-branch sequences and their reassignment on merge are checked by `merge` (§1,
  DSN-DVM-244).
- The tree is abstracted as a function of the committed state; its incremental update is
  covered by property VER-PROP-0004.

## 6. PH-2 step 2 — approvals and sagas (EXIT-2-02, DEC-097)

| Item | Value |
|---|---|
| Date | 2026-10-24 |
| Tool | TLC 2.19 (`tla2tools.jar` release 1.7.4), as in §1 |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 4 workers |
| Models | `approvals.tla` (VER-FORM-0100), `saga.tla` (VER-FORM-0101) |
| Verdict | **no property violated** in any run below |

### 6.1 Runs

| Model | Stated bound (VER item, DEC-097) | Exhaustive run | Distinct states | Depth | Time | Simulation at the stated bound | Evidence |
|---|---|---|---|---|---|---|---|
| `approvals` | 2 approvers × 3 editors × 3 objects, 2 proposals, versions ≤ 2, 1 source commit per change set (VER-FORM-0100) | 2 × 3 × 3, 2 proposals, **versions ≤ 1**, 1 source commit | 10,360,550 | 14 | 6 min 11 s | 1,000,000 behaviours, depth ≤ 60, 64,907,119 states checked, 1 min 50 s | `approvals.exhaustive.txt`, `approvals.simulation.txt` |
| `saga` | 3 steps, 2 attempts, 2 coordinator crashes, both recovery modes, failing and non-failing compensations (VER-FORM-0101) | the stated bound (exhaustive, with `Termination`) | 2,268 | 26 | 2 s | not needed | `saga.exhaustive.txt` |

The approvals run at the stated bound was stopped after 22 min with 37,319,314 distinct
states and 23,317,365 states still on the queue, no violation found before stopping; the
exhaustive run therefore lowers the version bound (DEC-031). The exhaustive run uses
`approvals.cfg` with `MaxVer = 1`.

A larger saga run (4 steps, 3 attempts, 3 crashes) also completes: 180,048 distinct
states, depth 46, 16 s, no violation.

The CI configurations complete exhaustively: `approvals.ci.cfg` (2 approvers × 2 editors ×
2 objects) 8,297,161 distinct states, 2 min 34 s; `saga.ci.cfg` (2 steps, 1 crash) 528
distinct states, depth 18.

### 6.2 Findings

- **F-2 (design text).** STD-TXN-080 (BPA 1.0-rc8) gives a saga the terminal state
  `failed` when a compensation fails or forward recovery is exhausted, but DSN-DVM-566 and
  VER-FORM-0101 required every started saga to end completed or compensated. The model
  shows that the stronger property holds only under backward recovery with compensations
  that do not fail (`EndsCompletedOrCompensated`); in general a saga ends in one of the
  three terminal states and fails only for those two causes (`FailedJustified`,
  `Termination`). DSN-DVM-566 and VER-FORM-0101 are reworded (DEC-097).
- **F-3 (design text).** DSN-DVM-564 called the saga a System-kind object; STD-TXN-080
  makes it an object of the genesis class `Saga` whose state members only the execution
  writes. DSN-DVM-564 is reworded (DEC-097).
- No defect was found in the approval rules of STD-TXN-060, STD-VER-021 and STD-LGS-041,
  -042. Writing the model made one point explicit: an approver's own commit through an
  incompatible port gives it a duty on that object, so its later decision on a proposal
  touching the object is refused (mutation `no_duty_check`).

### 6.3 Sensitivity of the properties (mutation checks)

Each mutation is selected by the constant `Mutation`; the property must fail. The approvals
mutations are checked at the CI bounds, the saga mutations at the stated bound
(`forward_order` needs 3 steps).

| Model | Mutation | Meaning | Property that failed |
|---|---|---|---|
| `approvals` | `count_stale` | approvals given on an earlier preview of a change set count | `NoCommitWithoutApprovals` |
| `approvals` | `no_maker_check` | the maker may approve its own proposal | `SoDHolds` |
| `approvals` | `double_vote` | a principal may decide twice | `SoDHolds` |
| `approvals` | `no_duty_check` | a principal holding an incompatible duty on an object of the proposal may decide | `SoDHolds` |
| `approvals` | `no_revalidate` | a pending change is not re-validated against the heads before approval and apply | `NoLostUpdate` |
| `saga` | `rerun` | after a crash the coordinator runs a step again although its process record exists | `AtMostOnce` |
| `saga` | `forward_order` | compensations run in step order | `ReverseOrder` |
| `saga` | `comp_all` | every step is compensated, including steps that never committed | `CompAfterExec` |
| `saga` | `lost_wait` | a crash leaves the coordinator waiting for the lost process | `Termination` |

Command (per mutation `M` and property `P`): copy the configuration, set `Mutation = "M"`
and `INVARIANTS P` (or `PROPERTY P`), and run TLC on it.

### 6.4 Scope limits

- **Approvals.** Object versions are counters, and a preview is the base for a pending
  change or the source revision for a change set; preview hashes are abstracted as these
  values. One approval requirement per proposal (a count) with one incompatible-duty set;
  expiry is a nondeterministic event, not a clock. Approval evidence (STD-LGS-043) and the
  simulation of rule changes (STD-LGS-044) are covered by vectors, not by the model.
- **Sagas.** One saga, one coordinator; steps on other branches or VAEs are abstracted as
  processes that commit their effect and their record atomically (STD-TXN-080), so a crash
  loses at most the process the coordinator waits for. Timeouts are a step outcome. Every
  step has a compensation (a step without one counts as compensated). Saga
  definitions and their migration are covered by vectors.
