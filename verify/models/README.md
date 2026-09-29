# Formal models (PH-0 step 2, EXIT-0-02)

TLA+ models of the core semantics, checked with TLC before any kernel code exists
(AR-033, DEC-030). Each model states its properties as TLC invariants.

| Model | Verification item | Clauses and design items | Properties (invariants) |
|---|---|---|---|
| `commit_cas.tla` | VER-FORM-0001 | STD-TXN-020…023, STD-VER-003, DSN-DVM-208, DSN-DVM-515 | `LinearHistory` (first-parent chain), `DenseCseq`, `NoLostUpdate`, `SerializableReads`, `FailuresJustified` (the head moving for unrelated objects never fails a flush) |
| `merge.tla` | VER-FORM-0002 | STD-VER-040, STD-VER-050, STD-VER-052, STD-VER-054, STD-VER-056, STD-KIND-022, DSN-DVM-240, DSN-DVM-243, DSN-DVM-244, DSN-DVM-249 | `MergeParents` (parents `[target, source]`), `LedgerWellFormed` (gap-free sequences, no duplicated entries), `NoLostEntries`, `ConflictsComplete`, `Determinism` (base and result independent of argument order) |
| `timeline.tla` | VER-FORM-0003 | STD-TIME-010…012, STD-TIME-031, DSN-DVM-303, DSN-DVM-326, AR-016 | `NoOverlap`, `IndexMatchesLog` (materialised index equals the replayed timeline), `PastAnswersStable` (answers at past `(cseq, asof)` never change), `ImpactComplete` (impact set ⊇ results whose inputs changed) |

## Running

```sh
sh verify/models/check.sh ci     # reduced bounds (<model>.ci.cfg), run by CI
sh verify/models/check.sh full   # verification-item bounds (<model>.cfg); long runs
```

The script downloads TLC (`tla2tools.jar` 1.8.0) once into `verify/models/.tools/` and
checks its SHA-256. Java 11 or later is needed. `TLC_WORKERS` sets the worker count.

## Results

The evidence of each run (TLC version, bounds, states, depth, duration, verdict) is in
`reports/`. The summary for the owner is `docs/reports/ph0-plan-zh.md`.
