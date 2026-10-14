# EXIT-1-09 — Performance on the reference environment (VER-BENCH-0100)

| Item | Value |
|---|---|
| Criterion | EXIT-1-09: MET-PERF-010…015 met on ENV-REF-SERVER with the WL-GENERIC-10M dataset |
| Status | **Open** — the reference run follows the rental of ENV-REF-SERVER (PH-1 step 9b, DEC-062); the runbook is `docs/reports/ph1-gate1a-runbook-zh.md` |
| Workstation pre-run | `workstation/`: the merge preview before and after the fix of finding 1 (step 9b) |

## Workstation pre-run of the merge preview (finding 1 of UBS-PHA-1)

`ubos-bench run --what merge` creates a branch of 1,000 changed objects (every object on a
dataset of 300) and measures `merge_preview` into `main`; workstation of DEC-047 (4 vCPU,
16 GB, PostgreSQL 16 on the same host).

| Dataset | Before (step 9a) | After (step 9b) |
|---|---|---|
| 20,000 objects, 6 months, 1,000 changed objects | p50 36.3 s, p95 54.9 s (`merge-20000-before.json`) | p50 0.37 s, p95 0.40 s, 15 samples (`merge-20000.json`) |
| 300 objects, 4 months, 300 changed objects | p50 0.41 s, p95 0.64 s (`merge-300-before.json`) | p50 0.12 s, p95 0.39 s (`merge-300.json`) |

The preview now reads only the objects whose state-tree keys differ between each merge base
and the source head (STD-VER-031) and the ledger keys of the entries the source commits
appended, in bulk from the indexes (`Store::objects_by_id`), so its cost follows the changes
and not the size of the VAE (DSN-DVM-240 phase note); the best-common-ancestor search walks the history once (DSN-DVM-241). The 20,000-object dataset is the bulk load of `ubos-bench load`; the p99 of the run after the fix is its first, cold sample (1.35 s). The reference run measures
MET-PERF-014 at 10,000,000 objects.
