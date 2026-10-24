# Formal models (PH-0 step 2, EXIT-0-02; PH-1 step 2, EXIT-1-07; PH-2 step 2, EXIT-2-02)

TLA+ models of the core semantics, checked with TLC before any kernel code exists
(AR-033, DEC-030). Each model states its properties as TLC invariants. `group_commit` has a
constant `Mutation` that selects the deliberately wrong variants of the sensitivity checks
(`reports/README.md` §5); `approvals` and `saga` have one too (§6). The configurations use
`"none"`.

| Model | Verification item | Clauses and design items | Properties (invariants) |
|---|---|---|---|
| `commit_cas.tla` | VER-FORM-0001 | STD-TXN-020…023, STD-VER-003, DSN-DVM-208, DSN-DVM-515 | `LinearHistory` (first-parent chain), `DenseCseq`, `NoLostUpdate`, `SerializableReads`, `FailuresJustified` (the head moving for unrelated objects never fails a flush) |
| `merge.tla` | VER-FORM-0002 | STD-VER-040, STD-VER-050, STD-VER-052, STD-VER-054, STD-VER-056, STD-KIND-022, DSN-DVM-240, DSN-DVM-243, DSN-DVM-244, DSN-DVM-249 | `MergeParents` (parents `[target, source]`), `LedgerWellFormed` (gap-free sequences, no duplicated entries), `NoLostEntries`, `ConflictsComplete`, `Determinism` (base and result independent of argument order) |
| `timeline.tla` | VER-FORM-0003 | STD-TIME-010…012, STD-TIME-031, DSN-DVM-303, DSN-DVM-326, AR-016 | `NoOverlap`, `IndexMatchesLog` (materialised index equals the replayed timeline), `PastAnswersStable` (answers at past `(cseq, asof)` never change), `ImpactComplete` (impact set ⊇ results whose inputs changed) |
| `group_commit.tla` | VER-FORM-0004 | DSN-DVM-006, DSN-DVM-402, DSN-DVM-404, DSN-DVM-424, STD-TXN-021, STD-TXN-024, STD-KIND-022 (W-8) | `DenseCseq`, `GapFree` (sequences 1…n per key and counter row equal to n, also after failed storage transactions), `PerKeyOrder`, `ProcessAtomic` (a transaction is committed at most once and whole), `SerialEquivalent` (each commit equals running its process alone at its position: bases current, next sequence, balance invariant), `BalanceSafe`, `NoFalseConflict` (append-only processes fail only on the balance invariant), `FailuresJustified` (only the offending process of a batch is aborted) |
| `approvals.tla` | VER-FORM-0100 | STD-TXN-060, STD-VER-021, STD-LGS-041, STD-LGS-042, DSN-DVM-550, DSN-DVM-551, DSN-DVM-746 | `NoCommitWithoutApprovals`, `StaleNeverCount` (counted approvals were given on the applied preview), `SoDHolds` (no maker, one decision per principal, no incompatible duty), `NoLostUpdate` (a pending change applies on its bases), `AppliedOnce` |
| `saga.tla` | VER-FORM-0101 | STD-TXN-080, DSN-DVM-564, DSN-DVM-566 | `AtMostOnce` (each step and compensation commits at most once), `CompAfterExec`, `ReverseOrder`, `CompletedOK`, `CompensatedOK`, `FailedJustified`, `EndsCompletedOrCompensated`; liveness `Termination` |

## Running

```sh
sh verify/models/check.sh ci     # reduced bounds (<model>.ci.cfg), run by CI
sh verify/models/check.sh full   # verification-item bounds (<model>.cfg); long runs
```

The script downloads TLC (release 1.7.4 of `tla2tools.jar`, TLC 2.19) once into
`verify/models/.tools/` and checks its SHA-256. Java 11 or later is needed. `TLC_WORKERS` sets the worker count.

## Results

The evidence of each run (TLC version, bounds, states, depth, duration, verdict) is in
`reports/README.md` (bounds per DEC-031, findings, mutation checks, scope limits). The summary
for the owner is `docs/reports/ph0-plan-zh.md`.
