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
