# PH-2 step 0b-1 — processes read their branch on demand (DEC-087)

| Item | Value |
|---|---|
| Finding | PH-1 step 19c (`verify/reports/exit-1-17/edge/`): a process (port call, handler, job) ran against a view of its whole branch. A command port took 287 s on a branch of 300,000 objects. |
| Change | The execution engine (`crates/ubos-dvm/src/execution.rs`) reads its pinned snapshot on demand: the governance objects (policies, lifecycles, class extensions, sheets, property declarations) and the logic assets as a whole, every other object on its first use from the index at the snapshot. URI targets resolve from a new slug index (`head_slug`, migration 0016; `Repo::object_by_slug`), ledger entries by identifier, and `:rel` qualifiers of decisions read at the snapshot. A branch with a parent line keeps the whole view. The URI read of UBTP (`resolve`) and lazy `:rel` qualifiers use the slug index too. |
| Result | **A command port on the 300,000-object branch: 60 ms p50 (287 s before)**, on the small branch 65 ms (175 ms before). WL-MIXED-OLTP capacity of one VAE 57.9 operations per second (37.8 before); at 70 %, p95 of all operations 302 ms (423 ms before). Commit to handler start p95 555 ms (578 ms before), still above NR-PERF-011's 500 ms on the workstation; p99 643 ms within 2 s |
| Semantics | Unchanged: conformance 1,703 of 1,703; replay corpus 10,000 of 10,000 outcomes and 1,000 of 1,000 replays identical; simulation 2,000 seeds, properties 10,000 cases, L2 382 of 382, storage 20 of 20, fault 40 events, node 12 of 12, SUITE-NOD-EVENTS 13 of 13, SUITE-NOD-JOBS 13 of 13, SUITE-NOD-ISO 544 of 544, scenarios 28 of 28 in memory, on PostgreSQL and over UBTP (WebSocket and HTTP) |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, release build, 2026-10-19; `ubos-edge-bench --parts query,mixed,ports` (`bench-pg-300k.json`, signed) |

## Before and after (300,000 objects, PostgreSQL, ms)

| Measure | PH-1 step 19c | After 0b-1 |
|---|---|---|
| command port, branch of 300,000 objects (p50) | 287,032 | 60.1 |
| command port, branch of about 720 objects (p50) | 174.7 | 64.7 |
| WL-MIXED-OLTP capacity (operations per second) | 37.8 | 57.9 |
| WL-MIXED-OLTP at 70 %, all operations p50 / p95 | 307 / 423 | 204 / 302 |
| commit to handler start, idle p50 / p95 | 121 / 152 | 104 / 127 |
| commit to handler start, at 70 % p95 / p99 | 578 / 659 | 555 / 643 |
| subscription, commit request to emit p95 | 627 | 427 |
| indexed pages through the edge (`eq`, `page2`, `range`, `key`) p95 | 60.9–98.4 | 14.5–85.3 |

## Remaining

1. **NR-PERF-011 on the workstation.** Commit to handler start is still above 500 ms at p95
   at 70 % load. One actor per VAE serialises reads, commits and the handler's commit, so the
   handler waits behind the load. The ENV-REF-SERVER run decides the gate (DEC-087 item 3).
2. **Commits still read the whole branch in some models.** The commit path (`commit_on`,
   DEC-050) loads the whole branch state when any of these holds:
   - the registry declares a reference field;
   - the branch holds class extensions or sheets;
   - the commit writes one of them.

   The checks of STD-CLS-030, -040 and -041 (reference integrity, uniqueness, relationships)
   and the sheet checks run over it. The benchmark model has no reference fields, so this did
   not show here. A finance model (references everywhere) would pay it on every commit. This
   is proposed to the owner as part 0b-1b: the checks over indexes (reference targets by
   identifier, unique keys by the field index, sheets by class), as in this part.
