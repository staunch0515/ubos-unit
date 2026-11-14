<!-- Input for the coordinator (PH-2 step 2a). Part A goes into reports/README.md, part B into
     verify/models/README.md, part C lists what check.sh needs, part D the findings on clauses and design items. -->

# Part A — section for `reports/README.md`

## N. PH-2 step 2a — approvals and deferred commits (EXIT-2-02)

| Item | Value |
|---|---|
| Date | 2026-11-14 |
| Tool | TLC 2.19 (`tla2tools.jar` release 1.7.4), as in §1 |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 2 workers (another model checker ran on the other two CPUs) |
| Model | `approval.tla` (safety), `approval_live.tla` (the same model with the liveness configuration), VER-FORM-0100 |
| Verdict | **no property violated** in any run below; the stated bound completed exhaustively |

### N.1 Runs

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

### N.2 Properties

Invariants of `approval.cfg`: `TypeOK`, `NoCommitWithoutValidApprovals`,
`StaleApprovalsNeverCount`, `SoDHolds`, `CommittedEqualsApproved`; action property
`TargetUnchangedWhilePending`. Temporal property of `approval_live.cfg` under weak fairness
of the applying process and of expiry: `Terminates` (every pending change ends applied,
rejected, expired, or invalidated without a further re-run).

### N.3 Sensitivity of the properties (mutation checks)

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

### N.4 Findings

See part D (F-1 … F-8). None is a violation of a checked property; F-1, F-3, F-4 and F-5 are
places where the model had to choose between the clause and the design item, or where a
stronger property does not hold.

### N.5 Scope limits

- One branch; one requirement per pending change with step `approval`, `count` 1 or 2,
  named principals and `order: any`. Not modelled: `order: sequence`, eligibility by role,
  the steps `second_person` and `signature`, several requirements per pending change (F-7).
- The preview hash is injective (the preview is the change set with its bases). A re-run
  writes the same objects from the current heads.
- The commit of the applying process is one atomic step; head compare-and-set between
  concurrent flushes is checked by `commit_cas` and `group_commit`. Between the final approval
  and that commit (state `approved` of the model) no decision and no expiry is accepted (F-8).
- Validation failures of the applying process other than moved bases
  (`invalid_reason` ≠ `TXN.HEAD_MOVED`) are not modelled; they end in `invalidated` as well.
- Time is not modelled: a pending change may expire at any moment; delegations are the
  `on_behalf_of` principal without validity periods (DSN-DVM-745).
- Voided approvals are dropped from the state (STD-LGS-043 keeps them visible); change-set
  approvals (STD-VER-021 proper) share the evidence item but are not modelled separately —
  the stale-approval rule is checked on pending changes.
- A pending change that ended keeps only the fields the properties refer to.
- Liveness at one pending change (see N.1).

# Part B — row and paragraph for `verify/models/README.md`

| `approval.tla` (liveness: `approval_live.tla`) | VER-FORM-0100 | STD-TXN-060, STD-VER-021, STD-LGS-041, STD-LGS-042, STD-LGS-043, DSN-DVM-550, DSN-DVM-551, DSN-DVM-552, DSN-DVM-746 | `NoCommitWithoutValidApprovals` (an applied change has at least `count` approvals of distinct eligible approvers, made on the committed preview, and no rejection), `StaleApprovalsNeverCount` (every approval a commit references was made on the committed preview; every approval that is not void was made on the current preview), `SoDHolds` (the initiator is neither approver nor `on_behalf_of`; no principal decides twice, directly or through `on_behalf_of`; no actor had made a committed call of an incompatible port on an object of the change), `CommittedEqualsApproved` (the commit writes exactly the objects of the approved preview, each over the base the approvers saw), `TargetUnchangedWhilePending` (action property: a head moves only by a direct edit or by the commit applying an approved change), `Terminates` (liveness under fairness: every pending change ends applied, rejected, expired or invalidated) |

`approval` models pending changes with base versions and preview, approvals and rejections
with `on_behalf_of`, the applying process, invalidation, re-run and expiry, against
concurrent direct edits and committed calls of incompatible ports. Like `group_commit` it has
a constant `Mutation` for the sensitivity checks; the configurations use `"none"`.
`approval_live.tla` only extends `approval.tla`, so that the liveness property has its own
configurations (`FairSpec`, no symmetry, one pending change).

# Part C — integration notes

- `check.sh`: add `approval approval_live` to the default model list and VER-FORM-0100 /
  EXIT-2-02 to the header comment. Both run today without a change of the script:
  `sh verify/models/check.sh ci approval approval_live` (about 75 s with 2 workers) and
  `sh verify/models/check.sh full approval approval_live` (1 h 23 min + 2 min).
- Files: `approval.tla`, `approval.cfg`, `approval.ci.cfg`, `approval_live.tla`,
  `approval_live.cfg`, `approval_live.ci.cfg`, `reports/approval.exhaustive.txt`,
  `reports/approval.simulation.txt`, `reports/approval_live.exhaustive.txt`.
- CI time grows by about 75 s; if that is too much, `Counts = {2}` in `approval.ci.cfg`
  gives 328,817 distinct states in about 30 s.

# Part D — findings on clauses and design items

| ID | Items | Finding | Proposed fix |
|---|---|---|---|
| F-1 | DSN-DVM-551 vs STD-TXN-060 (and `approvals.py`) | The clause and the reference model end a pending change whose bases moved as `invalidated` (final, approvals void, approvers notified). DSN-DVM-551 instead re-runs the original process and asks for re-approval. The two are compatible only if the re-run comes after the invalidation, which is how the model reads them (`Invalidate`/`Apply` then `Rerun`). DSN-DVM-551 does not say whether the re-run reuses the pending change (STD-TXN-060: "identified by the process that requested it") or creates a new one. | Reword DSN-DVM-551: "If bases changed, the pending change becomes `invalidated` and its approvals void (STD-TXN-060). When the original process is re-executable, it MAY be re-run as a new process, which stores a new pending change with a new preview (linked to the invalidated one) and needs new approvals." Its acceptance criterion already matches this. |
| F-2 | DSN-DVM-551 | "require re-approval when the preview hash differs" is always true: the preview is the content ID of `{branch, changes}` and the changes contain their bases (STD-TXN-060 note *Identity*), so a re-run after moved bases never has the old preview. The model confirms that keeping approvals across a re-run breaks `StaleApprovalsNeverCount` (mutation `keep_approvals`). | Drop the condition: a re-run always needs new approvals. If approvals are meant to survive unrelated base changes (STD-VER-021 note), the preview would have to exclude bases, which contradicts STD-TXN-060 — not recommended. |
| F-3 | STD-LGS-043, STD-TXN-060, `approvals.py` (`approve`) | An approval request does not carry the preview the approver decided on; the reference model records the current preview of the pending change. That is safe only while a pending change never changes its preview (invalidation is final). If a pending change were re-run in place (one reading of DSN-DVM-551), an approver who looked at the old preview would be recorded for the new one (mutation `no_preview_check` breaks `StaleApprovalsNeverCount`). The model therefore requires the request to name the preview. | STD-LGS-043: "An approval request names the preview it decides on; when it is not the current preview of the pending change the request is denied." With the fix of F-1 (new pending change per re-run) the rule becomes a cheap safeguard rather than a necessity. |
| F-4 | STD-TXN-060, STD-LGS-041 (`expires_in`), DSN-DVM-552 | Termination needs expiry (mutation `no_expiry` breaks `Terminates`): a pending change can be unable to collect `count` approvals — e.g. `count` 2 with two named approvers when one of them is the initiator, acted as `on_behalf_of` of the other, or holds an incompatible duty — and then stays pending for ever. In the standard `expires_in` is optional and the reference model sets no `expires_at` without it; DSN-DVM-552 is only *Should*. DSN-DVM-551 does not say that a re-run keeps the original expiry; if it restarted the period, repeated edits could keep a change alive without bound. | STD-TXN-060: a pending change without `expires_in` expires after an implementation default (DSN-DVM-552: 30 days). Raise DSN-DVM-552 to *Must*. State in DSN-DVM-551 that a re-run does not extend the expiry of the original request. Optionally deny a deferral whose requirement cannot be met (`count` greater than the eligible principals other than the initiator). |
| F-5 | STD-LGS-042 (`incompatible_duty`) | The check is made when the approval is given, in one direction only. A principal can approve (or be the `on_behalf_of`) and afterwards make a committed call of the incompatible port on the same object; if that call does not write an object of the pending change, nothing is invalidated and the change is applied with an approver who holds the incompatible duty at commit time. The stronger invariant `SoDAtCommit` of the model is violated in 6 states: submit by e1 writing x1; a1 approves on behalf of e2; e2 calls the incompatible port on x1 without writing it; a2 approves; apply. `SoDHolds` as checked is separation of duties at the time of each decision. | Either state in STD-LGS-042 that separation of duties is evaluated at the time of the decision (then FR-FLOW-043 "across the whole approval chain" should say so too), or — recommended — have the applying process re-evaluate the `incompatible_duty` check for the referenced approvals and invalidate the pending change (`invalid_reason` `IAM.DENIED`) when one fails. |
| F-6 | STD-LGS-041, STD-LGS-042, DSN-DVM-745, `approvals.py` (`_eligible`) | With `on_behalf_of`, eligibility is checked for the approver only, the separation-of-duties checks for both. The clause does not say whose eligibility counts under a delegation: as written, a delegate must itself be a named principal, which makes the delegation pointless, and an eligible approver may act for a principal who is not eligible. DSN-DVM-745 ("delegation MUST NOT exceed the delegator's rights") suggests that the delegator must be eligible. The model follows the reference model. | STD-LGS-041: "With `on_behalf_of`, the principal acted for MUST be eligible, and the approver MUST hold a valid delegation from it (DSN-DVM-745); the approval counts once, for the principal acted for." |
| F-7 | STD-LGS-041, `approvals.py` (`approve`, `_satisfied`) | A pending change can have several `approval` requirements (one per object whose decision required it). The clause does not say how an approval is attributed. The reference model checks eligibility, `incompatible` and `order` against the first `approval` requirement only, and counts every approval towards every requirement, so the principals and incompatible duties of the further requirements are not enforced. The model has one requirement and checks incompatible duties on every object of the change. | STD-LGS-041: "An approval counts for every requirement for which its approver is eligible; the checks of STD-LGS-042 use the `incompatible` duties of all requirements; the change is applied when every requirement has its `count`." Add a vector with two requirements naming different principals. |
| F-8 | DSN-DVM-551, DSN-DVM-550, STD-TXN-060, DSN-DVM-746, VER-FORM-0100 | Wording. (a) The reference model applies the change inside the final approval call; DSN-DVM-551 starts a process. In that window the design must not accept further decisions or expire the change — the model has the state `approved` for it; the standard has no such state. (b) The final states are four — `applied`, `rejected`, `expired`, `invalidated` — where the task wording of the liveness property names three. (c) DSN-DVM-550 ends the deferring process in state `deferred`, STD-TXN-060 ends it `committed`. (d) The bases cover written objects only: an object the decision read but the change does not write (a threshold, the counterparty) can change without invalidating the pending change; STD-VER-021 speaks of "the relevant target state". | (a) DSN-DVM-551: "from the final approval until the applying process ends, the pending change accepts no decision and does not expire". (b) List the four final states in DSN-DVM-746. (c) Align DSN-DVM-550 with STD-TXN-060 (the kernel state `deferred` maps to the BPA result `committed` with `pending`). (d) Decide whether `DAT-PendingChange` records the versions of the decision inputs and the applying process re-validates them; until then it is a stated limit. |
