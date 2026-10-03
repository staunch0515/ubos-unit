# EXIT-1-04 — Kernel properties at a million cases (SUITE-DVM-PROP, VER-PROP-0100)

| Item | Value |
|---|---|
| Criterion | EXIT-1-04: 0 property failures at ≥ 1,000,000 cases per property (VER-PROP-0100 long mode; VER-PROP-0004 at its own criterion of ≥ 100,000) |
| Result | **0 failures** in nine properties |
| Build | PH-1 step 8a (`cargo build --release --locked -p ubos-dvm --bin ubos-props`), DEC-061 |
| Generator | SplitMix64, seed 20260930, 3 threads; a case is replayed from its index (`--only <property> --cases N`) |
| Workstation | 4 vCPU, Linux x86-64, Rust 1.94; the runs shared the machine with the mutation and coverage jobs, so the times are upper bounds |

| Item | Property | Checks | Cases | Failures | Time |
|---|---|---|---|---|---|
| VER-PROP-0001 | root_history_independence | root history independence (and distinct states give distinct roots) | 1,000,000 | 0 | 169 s |
| VER-PROP-0002 | timeline_non_overlap | timeline non-overlap after every accepted operation | 1,000,000 | 0 | 1 s |
| VER-PROP-0003 | merge_determinism | merge determinism: same conflicts, or same bodies and periods, whichever branch is the source | 1,000,000 | 0 | 5068 s |
| VER-PROP-0004 | tree_incremental | incremental tree maintenance: incremental root = full rebuild, tree diff = key-wise difference | 100,000 | 0 | 574 s |
| VER-PROP-0100 | index_rebuild_equality | index rebuild equality: reads through the kernel indexes (`time: cseq`) equal reads through the state rebuilt from the commit records (`commit: id`), objects and ledger keys, every commit, four dates; root verifier clean | 1,000,000 | 0 | 14552 s |
| VER-PROP-0100 | diff_cost_bound | diff cost bound: a tree diff after `c` changes reads at most `2·(h+1)·(3·c+1)` nodes and lists exactly the changed keys | 1,000,000 | 0 | 2213 s |
| VER-PROP-0100 | merge_ledger_completeness | merge ledger completeness: own entries then every branch entry in branch order, dense sequences, same balance | 1,000,000 | 0 | 1860 s |
| VER-PROP-0100 | balance_bitemporal | bitemporal balance: the balance at any `(asof, cseq)` is the sum of the included entries, backdated ones included | 1,000,000 | 0 | 1703 s |
| VER-PROP-0100 | cursor_completeness | cursor completeness: pages under concurrent creates, updates and ends return the snapshot exactly once, in order | 1,000,000 | 0 | 4433 s |

Merge determinism outcomes: 561,759 conflicts and 438,241 merges that changed the target.
Diff cost: at most 116 node reads in one diff; the largest ratio to the bound
`2·(h+1)·(3·c+1)` was 750‰, so the bound holds with margin.

The five properties of step 8a (VER-PROP-0100) run on the in-memory engine through the
adapter; `index_rebuild_equality` compares two independent code paths of the kernel (the
kernel indexes and the state rebuilt from commit records) and runs the root verifier on
every branch. The PostgreSQL index and sequence verifiers run in the storage suite
(`verify/reports/exit-1-03/`) and in the fault suite of step 8b.

Raw results: `properties.json`. Reproduce (one property at a time keeps the report files
small; the whole set takes a few hours on four cores):

```sh
cargo build --release --locked -p ubos-dvm --bin ubos-props
./target/release/ubos-props --cases 1000000 --only index_rebuild_equality --report index.json
./target/release/ubos-props --cases 100000 --only tree_incremental --report tree.json
```

CI runs every property at 10,000 cases on each push; nightly CI (step 8c) runs the long mode.
