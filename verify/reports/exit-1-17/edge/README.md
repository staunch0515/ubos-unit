# EXIT-1-17 — Edge benchmarks of SUITE-NOD-BENCH on the workstation (PH-1 step 19c)

| Item | Value |
|---|---|
| Criterion | EXIT-1-17: MET-PERF-016 and MET-PERF-017 met, and NR-PERF-009 and NR-PERF-012 met (UBS-PH-1 §9) |
| Verification item | VER-BENCH-2000 (NR-PERF-008, NR-PERF-009, NR-PERF-011, NR-PERF-012), and FR-TEN-024 (fairness) |
| Result | **Indexed pages through the edge: p95 ≤ 100 ms at 300,000 objects (met for `eq`, `page2`, `range`, `key` on both bindings), and the edge adds 0–15 ms to the kernel. Not met on the workstation: the row-filtered page (p95 182–208 ms), the denied page (about 2 s, DEC-074), and commit to handler start under WL-MIXED-OLTP (p95 578 ms against 500 ms; p99 659 ms, within 2 s). A process runs against the full state of its branch: a command port on the 300,000-object branch takes 287 s** |
| Decision | DEC-085 (part 19c) |
| Runner | `ubos-edge-bench --database …` (`crates/ubos-node/src/bin/ubos-edge-bench.rs`); `--quick` runs in CI (job `rust workspace`) |
| Environment | development workstation (4 vCPU, 15 GB, linux x86_64), PostgreSQL 16 with default settings, release build, 2026-10-18 |
| Reports | `bench-pg-300k.json` (the full run, signed), `fairness-pg.json` (the fairness rerun, signed) |

## 1. The benchmark

One Server in the benchmark's process serves three fresh tenants of a PostgreSQL registry, each
with the VAE `acme`. Clients use UBTP over WebSocket (one session per client) and over HTTP (one
kept-alive connection with `Ubtp-Session`). The "kernel" columns run the same request on a second
engine over the same store, with the same principal.

| Part | What it measures |
|---|---|
| queries (MET-PERF-016) | The dataset of `ubos-bench query` (13d): 300,000 `Account` objects on `feature/bench`, loaded in commits of 5,000 (1,214 s). It has indexes on `country` and `balance` and a unique `number`. `feature/policy` holds a policy that denies one country in ten. The first page (`limit 100`) of `eq`, `page2`, `range` and `key`, plus `eq_filter` (a mostly permitted page) and `eq_denied` (a page the policy denies entirely); 50 rounds, 10 for `eq_denied`. |
| mixed (MET-PERF-017) | WL-MIXED-OLTP on `feature/oltp`: 70 % reads, 20 % commits of 1–10 objects with their bases, 5 % indexed queries and 5 % ledger appends, from 8 WebSocket sessions. The capacity is the throughput of the sessions unpaced for 20 s. The measurement runs at 70 % of it for 60 s, while two samplers run alongside. One measures commit to handler start on a `Pinged` subscription (kernel commit times of the ping and of its handler). The other measures the latency of a UBTP `target` subscription at another session. |
| fairness (FR-TEN-024) | Tenant B runs one session at 10 operations per second (85 % reads, 15 % commits). Tenant A is idle, then runs 10 sessions at 10 per second (ten times B), then 16 sessions unpaced, then 16 sessions unpaced under a per-principal limit of 20 requests per second (DSN-NOD-604). A warm-up precedes the phases, and a second idle baseline follows them. |
| ports | A command port on the small branch and on the dataset's branch, 5 calls each, and the time until the next read is answered. |

## 2. Results

### Indexed queries, 300,000 objects (ms)

| Page | kernel p50 / p95 | WebSocket p50 / p95 | HTTP p50 / p95 | Target p95 ≤ 100 |
|---|---|---|---|---|
| `eq` | 54.7 / 73.8 | 60.7 / 87.6 | 48.8 / 60.9 | met |
| `page2` | 61.7 / 89.4 | 76.6 / 98.4 | 66.6 / 85.3 | met |
| `range` | 57.3 / 76.6 | 62.9 / 76.2 | 47.6 / 64.7 | met |
| `key` | 10.8 / 14.1 | 13.7 / 16.6 | 11.8 / 18.0 | met |
| `eq_filter` | 122.1 / 192.4 | 141.2 / 207.8 | 125.7 / 182.3 | not met |
| `eq_denied` | 1,563.6 / 1,845.7 | 1,734.4 / 2,159.8 | 1,470.3 / 2,005.4 | not met (DEC-074) |

The edge adds 0–15 ms at p95 to the ordinary pages; the differences of a few ms either way are
within the run's noise. One WebSocket `eq` round took 7.5 s (p99). The VAE's actor answers one request at a time,
so a request waits behind any other work on the VAE.

### WL-MIXED-OLTP on one VAE (ms)

| Measure | Value |
|---|---|
| capacity (8 sessions unpaced) | 37.8 operations per second |
| at 70 % (target 26.5, achieved 24.6 per second, 0 errors) | reads p50 304 / p95 418; commits 313 / 426; queries 304 / 416; appends 309 / 434 |
| commit to handler start, idle | p50 121 / p95 152 / p99 467 |
| commit to handler start, at 70 % (63 samples, 0 missing) | p50 456 / **p95 578** / p99 659: p95 not met (≤ 500), p99 met (≤ 2,000) |
| subscription, commit request to emit | p50 389 / p95 627 (from the commit's result: p50 66 / p95 331) |

### Fairness (tenant B, ms)

| Phase | A's operations per second | B p50 / p95 | B p95 against baseline |
|---|---|---|---|
| baseline (A idle) | — | 21.7 / 36.5 | — |
| A at ten times B, paced | 86 | 55.6 / 98.8 | +171 % |
| A unpaced (16 sessions) | 33 | 21.7 / 36.7 | +1 % |
| A unpaced, limited to 20 requests per second | 22 (530,402 requests refused) | 30.1 / 46.1 | +26 % |
| baseline after | — | 23.5 / 47.1 | — |

### Port calls by branch size (ms)

| Branch | p50 | p95 |
|---|---|---|
| `feature/oltp` (about 720 objects) | 174.7 | 189.8 |
| `feature/bench` (300,000 objects) | 287,032 | 292,754 |

## 3. Reading

1. **The edge path of reads and queries (fixed in this step).** The first runs showed edge
   queries growing with the branch: 1.3 s for 100 rows at 20,000 objects and 47 s at 2,000 in a
   debug build, against 37 ms in the kernel. The UBTP binding read each row of a query result
   with a `commit` selector, and since 19a the authorized read did the same. A `commit` selector
   takes the view path, which materialises the whole branch state, once per row. The binding
   now handles them as follows:
   - It reads the rows of a query in one bulk read of object states and one of versions, at the
     commit the query evaluated. The query's row filter (DSN-DVM-820) has already authorized
     those rows for the same principal.
   - It names the commit of the authorized read by its sequence number.
   - Its `needs_view` uses the engine's cached branch metadata.

   A message that carries its own commit coordinates keeps the previous behaviour, so
   VER-CONF-5476 still holds. Conformance passes 1,703 of 1,703, the scenario tests 28 of 28
   over WebSocket and HTTP, and SUITE-NOD-ISO 544 of 544.
2. **Indexed pages.** Through the edge they meet NR-PERF-008 at 300,000 objects. Their latency
   depends on the page, not on the dataset (13d), so the 10,000,000-object run on
   ENV-REF-SERVER is expected to meet it too.
3. **Row filters.** A page the policy mostly permits evaluates every candidate with Cedar
   (`eq_filter`, p95 about 190 ms here). A page the policy denies entirely reads the key range
   until the scan budget. This is the residual post-filter of DEC-074, measured in 13d. Both are
   above the 100 ms target on the workstation.
4. **One actor per VAE.** A VAE answers one request at a time.
   - At 70 % of its capacity of 38 mixed operations per second, every operation waits behind
     the others, and the p95 of all kinds is about 420 ms.
   - A handler's commit waits in the same queue as the load, which is why commit to handler
     start misses the 500 ms p95.
   - The reference environment's faster disks and CPUs raise the capacity. The queueing at 70 %
     stays, so MET-PERF-017 on ENV-REF-SERVER depends on the VAE answering reads beside
     commits.
5. **Fairness.** Each tenant has its own VAE actor, so A's queue does not hold B's requests.
   Unpaced, A saturates its own actor (33 operations per second) and B is unaffected. Paced at
   ten times B, A has more requests in flight: B's p95 rises from 36 to 99 ms through the shared
   CPU and PostgreSQL. The per-principal limit caps A and keeps B within 26 %. NR-PERF-022's
   ≤ 30 % applies to PH-4 on ENV-REF-CELL.
6. **A process reads the whole branch.** The process runtime resolves its targets and reads
   against a view of its branch (`execution.rs`, `state()`), materialised from the store for
   every process. A command port takes 175 ms on a branch of about 720 objects and 287 s on a branch
   of 300,000 objects. The same holds for handler deliveries and jobs on such a branch. This is
   the cause of the "17–27 command-port calls per second" observed at step 15. It is not a
   PH-1 performance target by itself. However, it makes every port call, handler and job on a
   branch of realistic size unusable, and the finance Buk of PH-2 runs on such branches. It is
   reported to the owner with a proposal (reading on demand at the process's snapshot pins).

## 4. Reproduce

```
cargo build --release --locked -p ubos-node --bin ubos-edge-bench
psql -h 127.0.0.1 -U postgres -c "CREATE DATABASE edge"
./target/release/ubos-edge-bench --database "host=127.0.0.1 user=postgres dbname=edge" \
  --report verify/reports/exit-1-17/edge/bench-pg-300k.json
./target/release/ubos-edge-bench --database "host=127.0.0.1 user=postgres dbname=edge" \
  --objects 2000 --parts fairness --report verify/reports/exit-1-17/edge/fairness-pg.json
```

The fairness part of `bench-pg-300k.json` is superseded by `fairness-pg.json`. In the full run,
sessions 8–15 of tenant A's unpaced phases stopped at their first commit: the benchmark seeded
items for 8 owners only. Sessions now share the seeded owners. The rerun also has a warm-up and a
second baseline, and uses a database without the tenants of earlier runs: a Server opens, at
start, every VAE with pending work.
