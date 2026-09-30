# EXIT-0-05 — Prototype storage benchmark (preparation)

EXIT-0-05 is met by VER-BENCH-0001 on ENV-REF-SERVER (MET-PERF-001…003). This folder
collects the preparation done before the rented hosts are available, in three parts:

| Part | Content | Status |
|---|---|---|
| 5a | stored prolly-tree nodes with batch update and diff (DSN-DVM-007), VER-PROP-0004, tree benchmark | done |
| 5b | storage layout of DSN-DVM-001…010 on PostgreSQL: commit deltas with ordinals, kernel indexes, tree nodes written in the flush | open |
| 5c | WL-GENERIC-10M generator and the benchmark driver for point reads, commits at 500 commits/s and 10-change diffs | open |

## 5a — Tree maintenance

`ubos-tree` stores nodes (`Node`, `NodeSource`) and applies a batch of key changes to a
stored root, rewriting only the nodes whose key range holds a changed key (plus a following
node when a closing boundary key is removed). The parent level receives the replaced and
new node entries as its own batch. `diff` compares two roots and skips subtrees with equal
hashes.

**VER-PROP-0004** (`ubos-props --only tree_incremental`): trees of 0 to 20,000 keys, one to
four batches of 1 to 1,000 changes (or removal of every key); after each batch the root
equals a full rebuild and the diff equals the key-wise difference. Result: see
`tree-property.json` (100,000 cases, 0 failures). Sensitivity: two seeded defects were
detected before the run — a removed closing boundary that does not pull in the next node,
and a root with a single child that is not replaced by the child.

**Tree benchmark** (`ubos-bench tree --keys 1000000 --changes 10 --rounds 300`), in memory,
on the 4-vCPU workstation without SHA-256 CPU instructions:

| Measure | Result |
|---|---|
| Tree | 1,000,000 keys, 4 levels, 15,804 nodes; build 4.2 s |
| Nodes written per batch of 10 changes | mean 23.2, p95 24, max 26 (DSN-DVM-007: about 4 × 10) |
| Update of a batch | p50 3.3 ms, p95 5.1 ms |
| Diff of the two roots | p50 2.4 ms, p95 3.9 ms |

Findings:

- T-1: Node hashing takes about 72% of the update time: 17 to 25 nodes of about 8.7 KB each
  are hashed per batch, and this CPU has no SHA-256 instructions (software hashing about
  200 MB/s). The canonical node form is fixed by STD-PROOF-001, so the cost can only fall
  with hardware support. ENV-REF-SERVER must report whether its CPU has SHA extensions;
  the benchmark report records the CPU flags.
- T-2: Canonical JSON serialisation (`ubos-types`) now writes plain ASCII strings and keys
  directly and hex-encodes with a table. The output is unchanged: all 551 BPA 0.1 vectors
  pass, including the Proof vectors that fix tree roots.
- T-3: Boundary flags of stored entries are known from their node (only the last entry of a
  node can close it), so only inserted keys need a boundary hash.

Reproduce:

```sh
cargo build --release -p ubos-dvm --bins
./target/release/ubos-props --only tree_incremental --cases 100000 --report tree-property.json
./target/release/ubos-bench tree --keys 1000000 --changes 10 --rounds 300
```
