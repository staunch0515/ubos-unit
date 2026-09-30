# EXIT-0-05 — Prototype storage benchmark (preparation)

EXIT-0-05 is met by VER-BENCH-0001 on ENV-REF-SERVER (MET-PERF-001…003). This folder
collects the preparation done before the rented hosts are available, in three parts:

| Part | Content | Status |
|---|---|---|
| 5a | stored prolly-tree nodes with batch update and diff (DSN-DVM-007), VER-PROP-0004, tree benchmark | done |
| 5b | storage layout of DSN-DVM-001…010 on PostgreSQL: commit deltas with ordinals, kernel indexes, tree nodes written in the flush | done |
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
`tree-property.json` (100,000 cases, 0 failures, 628 s on 4 threads). Sensitivity: two seeded defects were
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

## 5b — Storage layout

Tables (memory backend with the same row model):

| Table | Content | Design |
|---|---|---|
| `bpa_commit` | commit record (parents, branch, `cseq`, state root), unique `(branch, cseq)` | DSN-DVM-004 |
| `bpa_branch` | head commit, head `cseq`, parent branch and fork `cseq` | DSN-DVM-204, -208 |
| `bpa_object` | object index: `(branch, object_id, cseq_from, cseq_to)` with the object state (class, kind, slug, head version, timeline); `cseq_to` is set when a later commit replaces the row | DSN-DVM-005 (`head_index`, `temporal_index`) |
| `bpa_entry` | ledger entries in append order with branch, `cseq`, class and canonical ledger key | STD-KIND-020 |
| `bpa_tree_node` | content-addressed tree nodes, deduplicated | DSN-DVM-007 |
| `bpa_version` | version records by hash | DSN-DVM-002 (without compression and encryption) |

A flush moves the head by compare-and-set and writes the version records, the commit, the
changed object rows (closing the previous row), the new entries and the new tree nodes in
one transaction. The new root is computed from the previous commit's root with the batch
update of 5a. A read of branch `b` at ordinal `s` uses `b`'s own rows with
`cseq_from ≤ s < cseq_to`; for objects without such a row it falls back to the parent at the
fork ordinal (DSN-DVM-008). A commit loads only the objects and ledger keys it touches; a
merge loads the whole state.

Checks: all 453 vectors tagged `prototype` pass on memory and on PostgreSQL 16, and
`export_state` fails if the stored root differs from a full rebuild, so every Proof vector
also checks the tree maintained in the flush. VER-PROP-0001…0003 pass on the new layout.

Not in the prototype layout (PH-1): row-level security and tenant partitioning
(DSN-DVM-010, no tenants in PH-0), relationship, unique-key and derivation indexes
(DSN-DVM-005, not used by BPA 0.1), group commit (DSN-DVM-006), chunk compression and
encryption (DSN-DVM-002, -003).

Reproduce:

```sh
cargo build --release -p ubos-dvm --bins
./target/release/ubos-props --only tree_incremental --cases 100000 --report tree-property.json
./target/release/ubos-bench tree --keys 1000000 --changes 10 --rounds 300
```
