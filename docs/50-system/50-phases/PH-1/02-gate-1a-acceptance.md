---
id: UBS-PHA-1
title: PH-1 — Gate 1a Acceptance Record
status: draft
phase: PH-1
depends_on: [UBS-PH-1, UBS-VER-10, DEC-038, DEC-061]
---

# PH-1 — Gate 1a Acceptance Record

The evidence for the exit criteria of gate 1a (UBS-PH-1 §9), one row per criterion: what
the workstation runs established, what the reference environments must repeat (step 9,
`docs/reports/ph1-gate1a-runbook-zh.md`), and what the calendar accumulates. The Phase
Acceptance Board (the owner and the external reviewer, DEC-038) decides on this record;
the decision is DEC-062.

## 1. Evidence

| Criterion | Exit | Workstation evidence | Reference environment | Status |
|---|---|---|---|---|
| BPA 1.0-rc1 with 100% MUST coverage | EXIT-1-01 | `standard/release/1.0-rc1/` (DEC-045), `verify/reports/exit-1-02/` | — | met |
| 100% of engine-profile vectors on PostgreSQL | EXIT-1-02 | `verify/reports/exit-1-02/`: 1,227 of 1,227 (DEC-060) | `forge conform --backend postgres` on ENV-REF-SERVER | met on the workstation |
| storage suite 100% on PostgreSQL; trait inspection | EXIT-1-03 | `verify/reports/exit-1-03/`: 20 of 20 (DEC-047) | storage suite on ENV-REF-SERVER | met on the workstation |
| 0 property failures at ≥ 1,000,000 cases | EXIT-1-04 | `verify/reports/exit-1-04/`: nine properties, 0 failures (DEC-061) | — (platform-independent) | met |
| 1,000,000 seeds, 0 violations; 30 clean nights | EXIT-1-05 | `verify/reports/exit-1-05/`: 1,000,000 seeds, 0 violations | nightly log `exit-1-05/nights.md`: streak ≥ 30 (2 clean nights on 2026-10-06 and -07; the runs from 2026-10-08 on got no runner from GitHub Actions) | seeds met; nights accumulating |
| 0 lost acknowledged commits in ≥ 10,000 crashes | EXIT-1-06 | `verify/reports/exit-1-06/`: 10,000 events, 0 lost, 0 partial | fault suite on ENV-REF-SERVER | met on the workstation |
| extended formal models, 0 violations | EXIT-1-07 | `verify/models/reports/` (DEC-046) | — | met |
| 100% replay fidelity on three platforms, ≥ 10,000 processes | EXIT-1-08 | `verify/reports/exit-1-08/`: Linux x86-64 10,000 of 10,000 | nightly log 2026-10-06 and 2026-10-07: Linux ARM64 and macOS ARM64 10,000 of 10,000, replays 1,000 of 1,000 | met (three platforms, two nights; every night since in the log) |
| MET-PERF-010…015 on ENV-REF-SERVER | EXIT-1-09 | driver `ubos-bench` (read, commit, diff, branch, merge, asof, `--background`), small-dataset runs in CI | VER-BENCH-0100 on ENV-REF-SERVER, three runs each, median | open; finding 1 blocks MET-PERF-014 |
| 100% of ≥ 300 L2 escape cases blocked | EXIT-1-10 | `verify/reports/exit-1-10/`: 382 of 382 (DEC-058) | — | met |
| coverage ≥ 85%, mutation ≥ 70%, 0 dependency violations | EXIT-1-11 | `verify/reports/exit-1-11/`: 90.1%, 89.9% on a sample, 0 (DEC-061) | nightly mutation shards complete the score | met on the workstation |
| portable round trip, identical roots | EXIT-1-12 | `verify/reports/exit-1-12/`: 3 VAEs (DEC-059) | 20 VAEs incl. WL-GENERIC-10M on ENV-REF-SERVER | met on the workstation |
| ten gate-1a scenarios through the in-process binding | EXIT-1-13 | `verify/reports/exit-1-13/`: 22 of 22 (DEC-060) | scenarios on ENV-REF-SMALL | met on the workstation |

Carried from PH-0: EXIT-0-05, the storage benchmark W-1 on ENV-REF-SERVER (DEC-035), runs in
the same rental before the PH-1 benchmarks; the external review of PH-0 (DEC-037) takes place
with it.

## 2. Findings before the reference run

1. **Merge preview is linear in the objects of the VAE.** `ubos-bench run --what merge`
   (step 9a) measures `merge_preview` of a branch with 1,000 changed objects: 0.4 s on 300
   objects, 36 s on 20,000 objects on the workstation — the preview builds the full states
   of both branches instead of walking the tree diff (STD-VER-031, DSN-DVM-230). MET-PERF-014
   (≤ 2 s p95 at 10,000,000 objects) cannot be met by this implementation. Disposition: a
   tree-diff-based merge preview before the ENV-REF-SERVER run (step 9b), recorded under
   DSN-DVM-240/242 with a phase note; the diff benchmark (MET-PERF-013) already uses the
   tree diff and is unaffected.
2. **The benchmark driver's remaining gaps.** `--background` runs the mixed commits on one
   connection; VER-BENCH-0100's "70% of the sustainable throughput" is chosen by the
   operator from the `commit` runs first (runbook §3). The dataset's bitemporal history gives
   `asof` reads their past segments; the overhead is reported as the p95 ratio.

## 3. Decision

Recorded as DEC-062 when the board decides; until then the status column above is the
state of the evidence.
