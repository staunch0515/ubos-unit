# EXIT-1-14 — workstation runs of PH-1 step 11d (NR-PERF-009, DSN-DVM-708)

Runs of `ubos-bench sheets` and `ubos-bench invariants` (DEC-070, part 11d) on the
development workstation (Intel Xeon @ 2.10 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1,
release build, in-memory store). The reference-environment run is VER-BENCH-2000 on
ENV-REF-SERVER (step 19). Reports: `sheets-500.json`, `sheets-5000.json`,
`invariants-1.json`, `invariants-50.json`, `invariants-200.json`.

## NR-PERF-009 — effective-declaration evaluation (`ubos-bench sheets`)

Synthetic sheet set: 10,000 rule blocks in 100 sheets (100 blocks each, the candidates
spread over the sheets) over 2,500 classes — a chain `Base`, `C1`…`C4` five levels below
`DataObject` and 2,495 sibling classes below `Base`; 20 candidate blocks on the chain (four
per class: type selector, attribute qualifier, state qualifier, attribute and tag
qualifiers) and the rest on the siblings with the same shapes. The objects are of `C4` and
match every candidate shape; each evaluation is the adapter operation `cascade` of one
object (5 effective properties). The first round compiles the sheets (cold, DSN-DVM-701),
the later rounds run from the compiled cache and the selector index (warm, DSN-DVM-703).

| Objects | Rounds | Cold p50 / p95 / max (ms) | Warm p50 / p95 / p99 / max (ms) | Target p95 | Met |
|---|---|---|---|---|---|
| 500 | 3 | 0.219 / 0.265 / 1.074 | 0.232 / 0.41 / 0.508 / 2.523 | ≤ 1 ms | yes |
| 5,000 | 3 | 0.331 / 0.567 / 4.044 | 0.309 / 0.592 / 0.777 / 5.12 | ≤ 1 ms | yes |

Authoring: the commit of the 100 sheets (10,000 blocks: parsing, type checks of the
qualifiers, the overlap analysis of STD-LGS-022 over every pair of candidates of a property
path whose classes may overlap) took 21.9 s; the commit
of the 500 objects (defaults, enforcement: two cascades per object) 204 ms,
of the 5,000 objects 2.51 s.

What made the target: compiled sheets cached by version hash and the selector index
bucketed by type-selector class (the cascade visits 24 blocks of 10,000); the normalised
declarations cached per sheet and property registry; on the index path the operation reads
only the sheets, property declarations and extensions by class and materialises the rest of
the state only for a `:rel` qualifier; the evaluation evidence reads the commit identifier
without the commit record; the memory store keeps an object index by class.

## DSN-DVM-708 — invariant dependency (`ubos-bench invariants`)

One sheet declares N invariants on `*`, each scoped to one of N sibling classes; 200
commits create one object of `S0` each, before and after the sheet exists. Only the
invariant whose scope the commit touches is evaluated (the scope filter precedes the
evaluation); the cost of a commit on a branch with sheets has a fixed part (the full state
is loaded for the sheet checks and the `:rel` qualifiers, two cascades per object) and a
part proportional to the size of the declaration copied into the cascade candidates, not
to evaluations.

| Invariants declared | Commit p50 without sheet (ms) | Commit p50 / p95 with sheet (ms) |
|---|---|---|
| 1 | 0.377 | 3.628 / 5.866 |
| 50 | 0.361 | 4.062 / 6.084 |
| 200 | 0.502 | 6.248 / 7.633 |

## Findings

1. The authoring commit of a 10,000-block set is 22 s: the overlap analysis
   compares every pair of candidates of a property path whose type selectors may overlap
   (2,000 candidates per path here). Sheets are authored one at a time in practice (the
   pairs then involve the written sheet only); a per-class bucketing of the candidates is
   the next step if a large set is ever committed at once.
2. A commit on a branch that holds sheets loads the full branch state (as the reference
   model does): about 3 ms at 400 objects in memory, growing with the objects. The
   commit-latency targets of gate 1a (VER-BENCH-0100) are measured without sheets; bounding
   this cost by the class index (sheets, property declarations, extensions by class, the
   rest on demand) is noted for the commit path of step 13.
3. The per-invariant cost (about 13 µs per declared invariant and commit) comes from
   copying the `invariants` declaration into the cascade candidates and the explanation,
   not from evaluation.
