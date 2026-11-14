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

## 6. PH-2 step 2a — approvals and deferred commits (EXIT-2-02)

| Item | Value |
|---|---|
| Date | 2026-11-14 |
| Tool | TLC 2.19 (`tla2tools.jar` release 1.7.4), as in §1 |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 2 workers (another model checker ran on the other two CPUs) |
| Model | `approval.tla` (safety), `approval_live.tla` (the same model with the liveness configuration), VER-FORM-0100 |
| Verdict | **no property violated** in any run below; the stated bound completed exhaustively |

### 6.1 Runs

| Run | Bound | Distinct states | Depth | Time | Evidence |
|---|---|---|---|---|---|
| Safety, exhaustive at the stated bound (`approval.cfg`) | 2 approvers × 3 editors × 3 objects, 2 pending changes, `count` 1 or 2, 2 direct edits, 1 incompatible call, 1 re-run per pending change | 50,992,122 (160,761,591 generated) | 16 | 1 h 23 min | `approval.exhaustive.txt` |
| Safety, simulation at the stated bound (supplementary) | as above; 1,000,000 behaviours, depth ≤ 60 | more than 8,384,900 states checked | every behaviour runs to its end (7 to about 15 states) | 3 min 45 s, 1 worker | `approval.simulation.txt` |
| Liveness, exhaustive (`approval_live.cfg`) | 2 approvers × 3 editors × 3 objects, **1 pending change**, `count` 1 or 2, 2 direct edits, 1 incompatible call, 2 re-runs; `FairSpec`, no symmetry | 956,578 | 12 | 1 min 52 s, 1 worker | `approval_live.exhaustive.txt` |
| CI, safety (`approval.ci.cfg`) | 2 × 2 editors × 2 objects, 2 pending changes, `count` 1 or 2, 1 direct edit, 1 incompatible call, 1 re-run | 1,385,500 | 15 | 65 s | — |
| CI, liveness (`approval_live.ci.cfg`) | 2 × 2 × 2, 1 pending change, 2 direct edits, 1 re-run | 61,897 | 10 | 10 s | — |

The stated bound completed, so rule 1 of DEC-031 applies and the simulation is not required;
it is kept as additional evidence. The same bound with one direct edit completes in 26 min
(9,376,807 distinct states, depth 15, 1 worker). TLC estimates the probability of a
fingerprint collision in the stated-bound run at 3.0E-4 (calculated) and 2.2E-4 (actual).

Symmetry reduction is used over editors, approvers, objects and pending changes in the safety
runs. The liveness property is checked without symmetry (TLC's symmetry reduction is not sound
for liveness), therefore at one pending change; a pending change ends independently of other
pending changes, whose only influence — moving the heads — is covered by the direct edits.

### 6.2 Properties

Invariants of `approval.cfg`: `TypeOK`, `NoCommitWithoutValidApprovals`,
`StaleApprovalsNeverCount`, `SoDHolds`, `CommittedEqualsApproved`; action property
`TargetUnchangedWhilePending`. Temporal property of `approval_live.cfg` under weak fairness
of the applying process and of expiry: `Terminates` (every pending change ends applied,
rejected, expired, or invalidated without a further re-run).

### 6.3 Sensitivity of the properties (mutation checks)

Each mutation is selected by the constant `Mutation` and checked against its target
property alone; the property must fail. Bounds: `approval.ci.cfg` with `Counts = {2}`
(`Terminates`: `approval_live.ci.cfg`).

| Mutation | Meaning | Property that failed |
|---|---|---|
| `no_count_check` | the first approval already starts the applying process | `NoCommitWithoutValidApprovals` |
| `not_eligible` | principals outside the requirement may approve | `NoCommitWithoutValidApprovals` |
| `double_decision` | no `one_decision_per_principal` check (directly or through `on_behalf_of`) | `SoDHolds`, `NoCommitWithoutValidApprovals` |
| `no_maker_checker` | the initiator may approve, directly or as `on_behalf_of` | `SoDHolds` |
| `no_duty_check` | no `incompatible_duty` check | `SoDHolds` |
| `keep_approvals` | approvals are not voided at invalidation and survive the re-run | `StaleApprovalsNeverCount` |
| `no_preview_check` | an approval naming another preview than the current one is accepted | `StaleApprovalsNeverCount` |
| `no_base_check` | the applying process commits without re-validating the bases | `CommittedEqualsApproved` |
| `rerun_commits` | bases changed: the re-run result is committed on the strength of the old approvals | `CommittedEqualsApproved`, `StaleApprovalsNeverCount` |
| `eager_write` | deferring writes the target | `TargetUnchangedWhilePending` |
| `no_expiry` | pending changes never expire | `Terminates` |

All eleven mutations were detected. Command (per mutation `M` and property `P`): copy the
configuration, set `Mutation = "M"` and `INVARIANTS P` (or `PROPERTIES P`), and run TLC.

### 6.4 Findings

The findings carry the prefix `A-` to keep them apart from those of §2. None is a
violation of a checked property; A-1, A-3, A-4 and A-5 are places where the model had to choose between the
clause and the design item, or where a stronger property does not hold.

| ID | Items | Finding | Proposed fix |
|---|---|---|---|
| A-1 | DSN-DVM-551 vs STD-TXN-060 (and `approvals.py`) | The clause and the reference model end a pending change whose bases moved as `invalidated` (final, approvals void, approvers notified). DSN-DVM-551 instead re-runs the original process and asks for re-approval. The two are compatible only if the re-run comes after the invalidation, which is how the model reads them (`Invalidate`/`Apply` then `Rerun`). DSN-DVM-551 does not say whether the re-run reuses the pending change (STD-TXN-060: "identified by the process that requested it") or creates a new one. | Reword DSN-DVM-551: "If bases changed, the pending change becomes `invalidated` and its approvals void (STD-TXN-060). When the original process is re-executable, it MAY be re-run as a new process, which stores a new pending change with a new preview (linked to the invalidated one) and needs new approvals." Its acceptance criterion already matches this. |
| A-2 | DSN-DVM-551 | "require re-approval when the preview hash differs" is always true: the preview is the content ID of `{branch, changes}` and the changes contain their bases (STD-TXN-060 note *Identity*), so a re-run after moved bases never has the old preview. The model confirms that keeping approvals across a re-run breaks `StaleApprovalsNeverCount` (mutation `keep_approvals`). | Drop the condition: a re-run always needs new approvals. If approvals are meant to survive unrelated base changes (STD-VER-021 note), the preview would have to exclude bases, which contradicts STD-TXN-060 — not recommended. |
| A-3 | STD-LGS-043, STD-TXN-060, `approvals.py` (`approve`) | An approval request does not carry the preview the approver decided on; the reference model records the current preview of the pending change. That is safe only while a pending change never changes its preview (invalidation is final). If a pending change were re-run in place (one reading of DSN-DVM-551), an approver who looked at the old preview would be recorded for the new one (mutation `no_preview_check` breaks `StaleApprovalsNeverCount`). The model therefore requires the request to name the preview. | STD-LGS-043: "An approval request names the preview it decides on; when it is not the current preview of the pending change the request is denied." With the fix of F-1 (new pending change per re-run) the rule becomes a cheap safeguard rather than a necessity. |
| A-4 | STD-TXN-060, STD-LGS-041 (`expires_in`), DSN-DVM-552 | Termination needs expiry (mutation `no_expiry` breaks `Terminates`): a pending change can be unable to collect `count` approvals — e.g. `count` 2 with two named approvers when one of them is the initiator, acted as `on_behalf_of` of the other, or holds an incompatible duty — and then stays pending for ever. In the standard `expires_in` is optional and the reference model sets no `expires_at` without it; DSN-DVM-552 is only *Should*. DSN-DVM-551 does not say that a re-run keeps the original expiry; if it restarted the period, repeated edits could keep a change alive without bound. | STD-TXN-060: a pending change without `expires_in` expires after an implementation default (DSN-DVM-552: 30 days). Raise DSN-DVM-552 to *Must*. State in DSN-DVM-551 that a re-run does not extend the expiry of the original request. Optionally deny a deferral whose requirement cannot be met (`count` greater than the eligible principals other than the initiator). |
| A-5 | STD-LGS-042 (`incompatible_duty`) | The check is made when the approval is given, in one direction only. A principal can approve (or be the `on_behalf_of`) and afterwards make a committed call of the incompatible port on the same object; if that call does not write an object of the pending change, nothing is invalidated and the change is applied with an approver who holds the incompatible duty at commit time. The stronger invariant `SoDAtCommit` of the model is violated in 6 states: submit by e1 writing x1; a1 approves on behalf of e2; e2 calls the incompatible port on x1 without writing it; a2 approves; apply. `SoDHolds` as checked is separation of duties at the time of each decision. | Either state in STD-LGS-042 that separation of duties is evaluated at the time of the decision (then FR-FLOW-043 "across the whole approval chain" should say so too), or — recommended — have the applying process re-evaluate the `incompatible_duty` check for the referenced approvals and invalidate the pending change (`invalid_reason` `IAM.DENIED`) when one fails. |
| A-6 | STD-LGS-041, STD-LGS-042, DSN-DVM-745, `approvals.py` (`_eligible`) | With `on_behalf_of`, eligibility is checked for the approver only, the separation-of-duties checks for both. The clause does not say whose eligibility counts under a delegation: as written, a delegate must itself be a named principal, which makes the delegation pointless, and an eligible approver may act for a principal who is not eligible. DSN-DVM-745 ("delegation MUST NOT exceed the delegator's rights") suggests that the delegator must be eligible. The model follows the reference model. | STD-LGS-041: "With `on_behalf_of`, the principal acted for MUST be eligible, and the approver MUST hold a valid delegation from it (DSN-DVM-745); the approval counts once, for the principal acted for." |
| A-7 | STD-LGS-041, `approvals.py` (`approve`, `_satisfied`) | A pending change can have several `approval` requirements (one per object whose decision required it). The clause does not say how an approval is attributed. The reference model checks eligibility, `incompatible` and `order` against the first `approval` requirement only, and counts every approval towards every requirement, so the principals and incompatible duties of the further requirements are not enforced. The model has one requirement and checks incompatible duties on every object of the change. | STD-LGS-041: "An approval counts for every requirement for which its approver is eligible; the checks of STD-LGS-042 use the `incompatible` duties of all requirements; the change is applied when every requirement has its `count`." Add a vector with two requirements naming different principals. |
| A-8 | DSN-DVM-551, DSN-DVM-550, STD-TXN-060, DSN-DVM-746, VER-FORM-0100 | Wording. (a) The reference model applies the change inside the final approval call; DSN-DVM-551 starts a process. In that window the design must not accept further decisions or expire the change — the model has the state `approved` for it; the standard has no such state. (b) The final states are four — `applied`, `rejected`, `expired`, `invalidated` — where the task wording of the liveness property names three. (c) DSN-DVM-550 ends the deferring process in state `deferred`, STD-TXN-060 ends it `committed`. (d) The bases cover written objects only: an object the decision read but the change does not write (a threshold, the counterparty) can change without invalidating the pending change; STD-VER-021 speaks of "the relevant target state". | (a) DSN-DVM-551: "from the final approval until the applying process ends, the pending change accepts no decision and does not expire". (b) List the four final states in DSN-DVM-746. (c) Align DSN-DVM-550 with STD-TXN-060 (the kernel state `deferred` maps to the BPA result `committed` with `pending`). (d) Decide whether `DAT-PendingChange` records the versions of the decision inputs and the applying process re-validates them; until then it is a stated limit. |

### 6.5 Scope limits

- One branch; one requirement per pending change with step `approval`, `count` 1 or 2,
  named principals and `order: any`. Not modelled: `order: sequence`, eligibility by role,
  the steps `second_person` and `signature`, several requirements per pending change (A-7).
- The preview hash is injective (the preview is the change set with its bases). A re-run
  writes the same objects from the current heads.
- The commit of the applying process is one atomic step; head compare-and-set between
  concurrent flushes is checked by `commit_cas` and `group_commit`. Between the final approval
  and that commit (state `approved` of the model) no decision and no expiry is accepted (A-8).
- Validation failures of the applying process other than moved bases
  (`invalid_reason` ≠ `TXN.HEAD_MOVED`) are not modelled; they end in `invalidated` as well.
- Time is not modelled: a pending change may expire at any moment; delegations are the
  `on_behalf_of` principal without validity periods (DSN-DVM-745).
- Voided approvals are dropped from the state (STD-LGS-043 keeps them visible); change-set
  approvals (STD-VER-021 proper) share the evidence item but are not modelled separately —
  the stale-approval rule is checked on pending changes.
- A pending change that ended keeps only the fields the properties refer to.
- Liveness at one pending change (see 6.1).

## 7. PH-2 step 2b — sagas (EXIT-2-02)

| Item | Value |
|---|---|
| Date | 2026-11-14 |
| Tool | TLC 2.19 (`tla2tools.jar` release 1.7.4), as in §1 |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 2 workers (another model checker ran on the other two CPUs) |
| Model | `saga.tla` (safety), `saga_live.tla` (the same model with the liveness configurations), VER-FORM-0101, DSN-DVM-566 |
| Verdict | **no property violated** in any run below. The invariants completed exhaustively at the stated bound; the liveness properties completed exhaustively at two reduced bounds and were simulated at the stated bound (DEC-031) |

### 7.1 Runs

Stated bound (`saga.cfg`): 3 steps × 2 coordinators, 2 coordinator crashes, 1 lost
invocation, 2 attempts per step, every declaration of the saga (recovery `backward` or
`forward` × timeout action `compensate` or `escalate` per step × compensation present or
absent per step: 72 declarations), compensations may fail.

| Run | Bound | Distinct states | Depth | Time | Evidence |
|---|---|---|---|---|---|
| Safety, exhaustive at the stated bound (`saga.cfg`, `check.sh full saga`) | as stated | 16,730,520 (94,092,744 generated) | 49 | 7 min 40 s | `saga.exhaustive.txt` |
| Liveness, exhaustive (`saga_live.cfg`, `check.sh full saga_live`) | stated bound with **1 crash** and **1 attempt** | 3,846,168 | 46 | 23 min | `saga_live.exhaustive.txt` |
| Liveness, exhaustive (`saga_live_c1.cfg`) | stated bound with **1 coordinator** (2 crashes, 2 attempts) | 862,584 | 48 | 2 min 06 s | `saga_live.exhaustive.txt` |
| Liveness, simulation at the stated bound (`saga_sim.cfg`) | as stated; `num=1000000`, depth ≤ 100, 2 workers | 44,803,949 states checked — about 1.9 million behaviours, at least the 1,000,000 requested | every behaviour runs to its end (about 24 states) | 1 h 24 min | `saga.simulation.txt` |
| CI, safety (`saga.ci.cfg`) | stated bound with 1 crash and 1 attempt | 3,846,168 | 46 | 1 min 28 s | — |
| CI, liveness (`saga_live.ci.cfg`) | 2 coordinators, 1 crash, 1 lost invocation, backward recovery, timeout action `compensate` or `escalate`, every step with a compensation that does not fail (8 declarations) | 608,056 | 46 | 3 min 56 s | — |

Bound that did not complete: the liveness properties at the stated bound. A run of an
earlier variant of the model (128 declarations, of which 56 were redundant ones under
forward recovery; one fairness condition per key) was stopped after 60 min with 11,439,904
distinct states found, 2,343,559 states left on the queue and depth 29, no violation found
before stopping. TLC checks liveness at about 150,000 distinct states per minute on this
machine, against about 2,000,000 for invariants; the stated bound has 16.7 million states.
Rule 2 of DEC-031 is therefore applied to the liveness properties only.

Symmetry reduction is not used (the steps are ordered, and the coordinators appear in
liveness runs). The declaration of the saga is chosen in the initial state, so one run
covers all declarations. Two reductions of the declaration space are exact: a step without
a timeout is covered by a step that declares one (the timeout may pass at any moment or
never), and under forward recovery no compensation runs, so one compensation declaration
stands for all eight.

### 7.2 Properties

Invariants of `saga.cfg` and `saga.ci.cfg`:

| Invariant | Meaning |
|---|---|
| `TypeOK` | types |
| `ForwardAtMostOnce` | the forward effect of a step is committed at most once, over all attempts, repeats and coordinators |
| `CompensateAtMostOnce` | the compensation of a step is committed at most once (DSN-DVM-566) |
| `CompensationReverseOrder` | compensations are committed in reverse step order and none is passed over (FR-TXN-063) |
| `NoCompensationOfUnappliedStep` | only a step whose forward effect is committed is compensated |
| `NoForwardAfterCompensationStarts` | no forward effect is committed once the saga object has left `running` (compensating or ended) |
| `CompensationOnlyWhileCompensating` | compensations are committed only while the saga object is `compensating` |
| `StateMatchesEffects` | the lists `completed` and `compensated` of the saga object match the committed effects; the only gap is the current step between the commit of its process and the commit that records it (FR-TXN-062) |
| `CompletedAllAppliedOnce` | a `completed` saga has applied every step exactly once, in order, and compensated none |
| `CompensatedNetEmpty` | a `compensated` saga has no net effect: every applied step that declares a compensation is compensated exactly once |
| `TerminalStable` | a saga object in a terminal state is never changed |
| `EndJustified` | `compensated` only under backward recovery; `escalated` only by a step that declares it and whose attempt is recorded as timed out; `failed` only by a recorded failed compensation or by forward recovery whose every attempt is recorded as failed or timed out |

Temporal properties of `saga_live.cfg`, `saga_live_c1.cfg`, `saga_live.ci.cfg` and
`saga_sim.cfg` on `FairSpec` (weak fairness of each coordinator's work and invocations and
of the execution of invocations in flight; crashes, lost invocations and timeouts are not
fair):

| Property | Meaning |
|---|---|
| `Terminates` | every saga ends: `completed`, `compensated`, `failed` or `escalated` |
| `EndsCompletedOrCompensated` | DSN-DVM-566: under backward recovery without escalation every saga ends `completed` or `compensated`, unless a compensation fails (in `saga_live.ci.cfg` compensations do not fail, so the property is the statement of DSN-DVM-566 as written) |
| `EndsAsDeclared` | under backward recovery every saga ends `completed`, `compensated`, or `escalated` by a step that declares escalation, unless a compensation fails |

### 7.3 Sensitivity of the properties (mutation checks)

Each mutation is selected by the constant `Mutation` and checked against its target
property alone; the property must fail. Bounds: `saga_live.ci.cfg` (2 coordinators, 1 crash,
backward recovery, 8 declarations), with the invariant in place of the temporal properties;
`comp_error_skipped` with `CompCanFail = TRUE`.

| Mutation | Meaning | Property that failed (length of the counterexample) |
|---|---|---|
| `no_dedupe` | a step or compensation process runs again although its key has an outcome | `ForwardAtMostOnce` (10), `CompensateAtMostOnce` (21), `CompensationOnlyWhileCompensating` (24), `CompletedAllAppliedOnce` (25) |
| `rerun_failed` | a failed attempt is not recorded as final: a repeated invocation under its key runs again | `NoForwardAfterCompensationStarts` (11), `StateMatchesEffects` (11) |
| `no_fence` | on a timeout the coordinator records its decision in the saga object only, without taking the key of the attempt | `CompensatedNetEmpty` (11), `StateMatchesEffects` (9), `NoForwardAfterCompensationStarts` (9), `EndJustified` (7) |
| `timeout_as_ok` | a timed-out step is recorded as completed | `CompletedAllAppliedOnce` (20) |
| `comp_forward_order` | compensations run in step order | `CompensationReverseOrder` (25) |
| `comp_failed_step` | the failed step itself is added to `completed` and compensated | `NoCompensationOfUnappliedStep` (13) |
| `comp_error_skipped` | a failed compensation is recorded as compensated | `CompensatedNetEmpty` (22) |
| `no_cas` | the saga object is written without the compare-and-set on its head | `TerminalStable` (10) |
| `no_reinvoke` | a process is invoked once only: after a lost invocation nobody invokes it again | `Terminates`, `EndsCompletedOrCompensated`, `EndsAsDeclared` (18, ending in a state that only stutters) |

All nine mutations were detected, and every property of §7.2 except `TypeOK` is the target
of at least one. Command (per mutation `M` and property `P`): copy `saga_live.ci.cfg`, set
`Mutation = "M"`, `SPECIFICATION Spec` and `INVARIANTS P` (or keep `FairSpec` and set
`PROPERTIES P`), and run TLC on `saga_live.tla`.

### 7.4 Findings

The findings carry the prefix `S-`. None is a violation of a checked property in the
design as modelled; S-1, S-2 and S-5 are rules the model needs and the clauses do not
state (without them a property fails, see the mutations), S-3, S-4 and S-6 are wording.

| ID | Items | Finding | Proposed fix |
|---|---|---|---|
| S-1 | DSN-DVM-565, STD-TXN-080 (note *Timeouts*), DSN-DVM-564 | A timeout decided by the coordinator alone is unsafe when the step can still commit. STD-TXN-080 defines a timeout as a limit termination of the step's process: the process has ended and cannot commit later, which is sound but covers only a process the same kernel terminates. DSN-DVM-564 puts steps on other branches or VAEs, and DSN-DVM-565 speaks of "a step that never answers": there the coordinator's timer passes while the process may still commit. Counterexample (mutation `no_fence`, 11 states): step 1 is invoked; its timeout passes; the saga goes `compensating` and, nothing being completed, `compensated`; then the process of step 1 commits. The saga is `compensated` with the effect of step 1 in place and a step that is not in `completed` (`CompensatedNetEmpty`, `StateMatchesEffects`, `NoForwardAfterCompensationStarts` fail). | DSN-DVM-565: "On timeout the coordinator MUST first record the outcome `timed_out` under the key `(saga, step, attempt, phase)` of the attempt, by a process on the branch of the step, if no outcome is recorded under it. A step process whose key is taken MUST do nothing. If the step's process committed first, the timeout is void and the saga continues with the recorded outcome. Only then the declared action runs." Add the same sentence to the note *Timeouts* of STD-TXN-080 for steps whose process is not ended by the limit termination. |
| S-2 | STD-TXN-080 (note *Commits*), DSN-DVM-750, DSN-DVM-564, `workflows.py` (`saga_results`) | "A step whose process committed is not run again; its recorded outcome is used" covers committed processes only. A failed attempt must be recorded under the key as well and must not be run again: otherwise an invocation repeated after a crash, or made by a second coordinator, can succeed after the saga has started to compensate on the strength of the failure (mutation `rerun_failed`, 11 states, `NoForwardAfterCompensationStarts`). The reference model does this — `saga_results` keeps `ok` and `error` outcomes per `(saga, step, attempt, phase)` — but in a map in memory that survives the simulated crash; the clause and DSN-DVM-564 do not say where the outcome of a failed process is durable (DSN-DVM-508: an in-flight process leaves nothing). | STD-TXN-080: "The outcome of every step process, committed or failed, is recorded once under `(saga, step, attempt, phase)`; the saga's execution acts only on recorded outcomes, and a repeated invocation under a key that has an outcome does nothing." DSN-DVM-564: the outcome record is the process record of the step on the step's branch (FR-TXN-062), written in the same commit as the effect when the process commits and by a separate commit when it fails; DSN-DVM-750 (`(instance, step, attempt)`) names the phase as part of the key for sagas. |
| S-3 | DSN-DVM-566, VER-FORM-0101, STD-TXN-080 (note *States*) | "Every started saga ends completed or compensated" does not hold for the protocol of STD-TXN-080: a saga also ends `failed` (failed compensation; forward recovery that used `max_attempts`), and with DSN-DVM-565 by escalation. The model checks what holds: `Terminates`, `EndJustified`, `EndsCompletedOrCompensated` and `EndsAsDeclared` (§7.2). | Reword DSN-DVM-566 and the procedure of VER-FORM-0101: "every started saga ends (`completed`, `compensated`, `failed` or escalated); under backward recovery it ends `completed` or `compensated` unless a compensation fails or a step that declares escalation times out; compensations run at most once per step". |
| S-4 | DSN-DVM-565, STD-TXN-080 (`DAT-Saga`), FR-TXN-063 | The timeout declaration has no place in the standard: `DAT-Saga` has no member for a step's timeout or its action and no status for an escalated saga. DSN-DVM-565 says each step MUST have a timeout, FR-TXN-063 says a step MAY declare one. "Compensate or escalate" is not defined under forward recovery, where a failure is retried and never compensated (FR-TXN-063). The model reads `compensate` as "handled as a failure of the step under the saga's recovery" (compensation under backward recovery, the next attempt or `failed` under forward recovery; this is what the reference model does for a limit termination) and `escalate` as a terminal state `escalated` that waits for manual resolution (FR-TXN-064). | A new release candidate (needs a DEC record): `DAT-Saga` steps get `timeout` (duration, optional; a profile default applies, which reconciles MUST and MAY) and `on_timeout: fail (default) \| escalate`; `status` gets `escalated`, with `error.step` and `error.code` naming the timed-out step. DSN-DVM-565: "compensate" becomes "fail (handled per the saga's recovery)". Alternative without a new status: escalation ends the saga `failed` with `error.reason: timeout`; not recommended, because FR-TXN-064 lists stuck and failed sagas separately. |
| S-5 | DSN-DVM-564, `workflows.py` (`_saga_set`) | The design item does not say how two coordinators are kept apart (a restarted node and a slow one that is not dead; PH-2 has several nodes). The reference model commits every transition with the head of the saga object as base. That compare-and-set is sufficient: with it and S-1, S-2 all properties hold for two coordinators acting at once; without it a stale coordinator commits again on a saga that has ended (mutation `no_cas`, `TerminalStable`, 10 states). A lease on the saga (as for jobs, DEC-076) is then an optimisation, not a safety condition. | DSN-DVM-564: "Each state transition is a commit whose base is the head of the saga object; a coordinator whose commit fails with a conflict reads the saga object again. The coordinator derives every decision from the saga object and the recorded outcomes of the step processes, never from its memory." |
| S-6 | DSN-DVM-565, STD-TXN-080 (note *States*), STD-TXN-051 | Compensations. (a) DSN-DVM-565 does not say whether the timeout applies to compensation processes; the model has no timeout for them and re-invokes a compensation until an outcome is recorded. (b) The reference model ends the saga `failed` at the first failure of a compensation, without retry, also when `max_attempts` is greater than 1, whereas STD-TXN-051 retries the compensation of a pre-commit call until it succeeds ("at least once"). (c) "A step without a compensation counts as compensated": `CompensatedNetEmpty` therefore holds only over the steps that declare a compensation — a `compensated` saga can leave the effects of the others in place. | (a) DSN-DVM-565: a compensation that times out is fenced as in S-1 and ends the saga `failed` (or escalated), so that the saga appears in the stuck list. (b) Decide whether `max_attempts` applies to compensations; recommended: yes, a failed compensation is retried under a new attempt up to `max_attempts`, then `failed`. (c) State in the note *States* that `compensated` means "every declared compensation of a completed step has run", and let `validate` warn about a step without a compensation that is followed by another step. |

The model agrees with the reference model (`standard/tools/bpa/workflows.py`,
`_saga_loop`): `Trans` is its loop, branch by branch, with the timeout outcome and the
state `escalated` added (S-4). Without crashes and timeouts a completed saga of N steps
takes 3N + 2 commits — 2N + 2 on the saga object and N step processes — as in the vector
VER-CONF-2225 (8 commits for 2 steps); `completed` keeps the completed steps while
`compensated` lists them in reverse order (VER-CONF-2226).

### 7.5 Scope limits

- One saga. Sagas do not share state in the model; conflicts between the steps of different
  sagas on the same objects are ordinary commit conflicts (`commit_cas`, `group_commit`) and
  end a step process with a failure.
- A step process is one atomic step: its effect and its outcome record are one commit
  (DSN-DVM-508); a failed process records its failure (S-2). The effect of a step is
  abstract (applied or not); what a compensation writes is not modelled, only that it runs.
- The fence of S-1 is part of the model. An implementation without it is not covered by the
  result (it fails, see `no_fence`).
- Time is not modelled: a timeout may pass at any moment while no outcome is recorded.
  Compensations have no timeout (S-6); they fail only where the configuration allows it.
- Bounds of the fault injection: 2 coordinator crashes, 1 lost invocation, 2 attempts, 2
  coordinators. Commits on the saga object fail only by the compare-and-set; storage
  failures are covered by `group_commit`.
- Operator actions of FR-TXN-064 (retry, skip, cancel, resolution of a `failed` or
  `escalated` saga) are not modelled; both states are terminal.
- Liveness holds under weak fairness: a coordinator that is up keeps working, a crashed
  one restarts, an invocation in flight is eventually executed. It is exhaustive at the
  two reduced bounds of §7.1 only.
