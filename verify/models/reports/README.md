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
