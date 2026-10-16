# EXIT-1-17 — workstation runs of PH-1 step 13d (NR-PERF-008 / MET-PERF-016)

Runs of `ubos-bench query` (DEC-073, part 13d; DEC-074) on the development workstation
(Intel Xeon @ 2.10 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94, release build; PostgreSQL 16 on
the same machine, default settings). The reference-environment run over 10,000,000 objects
is VER-BENCH-2000 on ENV-REF-SERVER (step 19). Reports: `query-mem-100000.json`, `query-mem-300000.json`, `query-pg-300000.json`.

## The suite

One class `Account` (`number` a unique key, `country` one of 100 values and `balance` a
decimal in 0…9,999.99 both declared indexes, two unindexed fields), created in commits of
5,000 objects; 50 rounds of each query with a parameter drawn per round; the latency of the
first page (`limit 100`):

- `eq`: `from Account where this.country == "Cnn" order by this.country limit 100` — the
  equality matches 1% of the objects; the order-by pushdown reads `limit` index rows in
  key order and stops (DEC-074 item 2);
- `range`: `this.balance >= x and this.balance < x + 100`, ordered by `balance` (1%);
- `page2`: the second page of `eq` through its cursor (`offset + limit` rows);
- `key`: a lookup by the unique key;
- `eq_filter`: `eq` with a principal and a policy set on the branch — the residual row
  filter of DSN-DVM-820 (DEC-074 item 1); the policy denies one country in ten, so five
  rounds in fifty ask for a page the policy denies entirely.

Load through the commit path: memory 100,000: 29 s, memory 300,000: 95 s, postgres 300,000: 506 s. The 1,000,000-object run in memory was killed for
memory (the memory store keeps every version as JSON); PostgreSQL holds the larger
datasets.

## Results

| Backend | Objects | Query | p50 (ms) | p95 (ms) | p99 (ms) | max (ms) | Rows per page | ≤ 100 ms p95 |
|---|---|---|---|---|---|---|---|---|
| memory | 100,000 | `eq` | 2.587 | 3.905 | 5.891 | 5.891 | 100 | yes |
| memory | 100,000 | `range` | 3.169 | 4.469 | 5.112 | 5.112 | 100 | yes |
| memory | 100,000 | `page2` | 4.858 | 6.534 | 7.457 | 7.457 | 100 | yes |
| memory | 100,000 | `key` | 0.220 | 0.530 | 0.856 | 0.856 | 1 | yes |
| memory | 100,000 | `eq_filter` | 38.743 | 197.115 | 245.656 | 245.656 | 90 | no |
| memory | 300,000 | `eq` | 5.518 | 7.252 | 8.757 | 8.757 | 100 | yes |
| memory | 300,000 | `range` | 7.829 | 8.900 | 9.794 | 9.794 | 100 | yes |
| memory | 300,000 | `page2` | 9.659 | 11.805 | 12.820 | 12.820 | 100 | yes |
| memory | 300,000 | `key` | 0.901 | 3.365 | 5.287 | 5.287 | 1 | yes |
| memory | 300,000 | `eq_filter` | 44.442 | 668.935 | 763.318 | 763.318 | 90 | no |
| postgres | 300,000 | `eq` | 25.584 | 36.391 | 95.803 | 95.803 | 100 | yes |
| postgres | 300,000 | `range` | 28.543 | 36.227 | 42.998 | 42.998 | 100 | yes |
| postgres | 300,000 | `page2` | 29.977 | 36.919 | 40.709 | 40.709 | 100 | yes |
| postgres | 300,000 | `key` | 4.536 | 6.413 | 9.364 | 9.364 | 1 | yes |
| postgres | 300,000 | `eq_filter` | 57.646 | 818.843 | 1089.263 | 1089.263 | 90 | no |

## Reading

1. **Indexed pages.** `eq`, `range`, `page2` and `key` read `offset + limit` index rows and
   their versions; the latency depends on the page size, not on the number of objects
   (memory 100,000 → 300,000: `eq` p95 3.9 → 7.3 ms; PostgreSQL 300,000: `eq`
   p95 36.4 ms, `key` 6.4 ms). On PostgreSQL the page is one `field_index`
   range query, one `objects_by_id` and one bulk version read (step 13d: before the bulk
   read, 200 single-row version reads cost about 70 ms per page). Extrapolated to
   10,000,000 objects the B-tree range read grows by the depth of the index only: the
   target p95 ≤ 100 ms for the first 100 rows is met with a wide margin on the workstation
   and is expected on ENV-REF-SERVER.
2. **Row filters.** With a policy set every candidate row is authorized (about 0.2 ms per
   row for the entity construction and the Cedar evaluation): a page the policy mostly
   permits costs `eq_filter` p50 44 ms in memory and 58 ms on PostgreSQL at 300,000
   objects; a page the policy denies entirely evaluates the whole key range (1% of the
   objects here, up to the scan budget of 100,000 rows): p95 669 ms in memory and
   819 ms on PostgreSQL at 300,000 objects, and at 10,000,000 objects such a page would
   reach the budget (`QRY.SCAN_LIMIT`). This is the measured limit of the residual
   post-filter (DEC-074 item 1): the ordinary page meets the target; the denied page needs
   the residual predicate pushed into the scan, which DEC-074 defers until Cedar's partial
   evaluation stabilises or the ENV-REF-SERVER run fails the gate.
3. **What made the numbers.** The light commit metadata (`Store::commit_meta`) on the
   history walks and snapshot selection: a commit of 5,000 creates cost about 40 ms to
   parse per read, and a query read it two or three times; the order-by pushdown with its
   continued scan; the bulk version read of the candidates.

## Findings

1. The commit path loads about 100 objects per second into PostgreSQL in commits of 5,000
   (the 300,000-object load took 506 s): the flush writes the versions, the three index
   tables and the tree nodes row by row in one transaction. The bulk loader of step 5
   (`ubos-bench load`, COPY) is the path for datasets of this size; the commit path's
   batch writes are a candidate for the ENV-REF-SERVER preparation of step 19.
2. The memory store needs about 15 GB for 1,000,000 objects of this shape: the benchmark
   and conformance backend, not a capacity reference.
