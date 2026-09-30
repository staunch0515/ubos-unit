# EXIT-0-05 — Prototype storage benchmark (preparation)

EXIT-0-05 is met by VER-BENCH-0001 on ENV-REF-SERVER (MET-PERF-001…003). This folder
collects the preparation done before the rented hosts are available, in three parts:

| Part | Content | Status |
|---|---|---|
| 5a | stored prolly-tree nodes with batch update and diff (DSN-DVM-007), VER-PROP-0004, tree benchmark | done |
| 5b | storage layout of DSN-DVM-001…010 on PostgreSQL: commit deltas with ordinals, kernel indexes, tree nodes written in the flush | done |
| 5c | WL-GENERIC-10M generator and the benchmark driver for point reads, commits at 500 commits/s and 10-change diffs | done (local trial; the measured run needs ENV-REF-SERVER) |

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

## 5c — Workload generator and benchmark driver

`ubos-bench` (crate `ubos-dvm`):

| Command | Content |
|---|---|
| `load` | WL-GENERIC (UBS-VER-03): 200 classes in 3 inheritance levels with 5–40 effective fields; objects spread over the classes; 10% of the objects changed each month (`supersede_from` at the first day of the month). History: one commit with every creation (`cseq` 1), then one commit per month. Rows are written with COPY in the 5b layout, with the tree built once and updated per month; everything is a function of `(seed, object, month)`. Defaults: 10,000,000 objects, 24 months, seed 20260930. |
| `check` | loads a small dataset in bulk and commits the same history through the kernel on the memory store: every commit's state root and sampled head reads must agree; then the same 60 commits run one by one (memory) and in groups with a stale commit in each group (PostgreSQL), and the roots must agree again. |
| `run --what read` | point reads of random objects at the head, open loop at a fixed rate over worker connections (MET-PERF-001) |
| `run --what commit` | commits of 1–10 random objects (NR-PERF-002) at a fixed rate into one sequencer, which flushes the ready commits of up to 64 per transaction (group commit, DSN-DVM-006) (MET-PERF-002) |
| `run --what diff` | diffs between commits ten single-object commits apart, after appending 200 such commits (MET-PERF-003) |

Latency is measured from the scheduled start of each operation, so queueing is counted.
Each report holds percentiles, a histogram, errors, the achieved rate and the environment
(CPU model and SHA extensions, cores, memory, kernel, PostgreSQL settings) with its content
identifier. `verify/envs/ENV-REF-SERVER-ph0.md` defines the rented set-up;
`docs/reports/ph0-step5-runbook-zh.md` is the owner's runbook.

Kernel changes made for the benchmark (all prototype vectors still pass on both backends):

- diff compares the state trees of the two coordinates and reads only the objects under
  differing keys plus System-kind objects; ledger differences use the full comparison
  (10 of the 12 diff calls of the vectors take the tree path);
- group commit: `Engine::commit_group` builds consecutive commits over an in-memory overlay
  (`ubos_store::Group`) and writes them with one transaction (DSN-DVM-006); a commit that
  fails validation is left out alone;
- the PostgreSQL flush is one statement (data-modifying CTEs gated by the head
  compare-and-set); commit identifiers are fetched 64 at a time; tree nodes, versions and
  commit records are cached by content (DSN-DVM-007, -009), parsed tree nodes in a
  process-local cache; a view caches branch references for its duration (one snapshot per
  operation, DSN-DVM-008);
- the commit path no longer copies every class declaration per commit.

### Local trial (not EXIT-0-05 evidence)

The 4-vCPU workstation without SHA extensions with a local PostgreSQL 16 (128 MB shared
buffers), 200,000 objects × 24 months (the full dataset is 50 times larger):

| Measure | Result |
|---|---|
| Load | 679,479 versions (3.40 per object; expected 1 + 24 × 0.1 = 3.4), 70,099 tree nodes, 26 commits, 86 s, 1.5 GB |
| Point reads at 1,000/s, 4 connections, 60 s | p50 0.55 ms, p95 0.94 ms, p99 1.28 ms, 0 errors |
| 10-change diffs at 50/s, 60 s | p50 5.9 ms, p95 11.1 ms, p99 23.5 ms, 10.0 changes per diff |
| Single-object commits at 150/s, 60 s | p50 5.8 ms, p95 15.0 ms, p99 67.3 ms (service time about 5.1 ms) |
| Commits of 1–10 objects at 60/s, 60 s | saturated at 57/s (about 18 ms of sequencer time per commit), so queueing dominates the latency |

Findings:

- C-1: Commit capacity is bound by sequencer CPU, not by the database: about 55% of a
  commit's CPU is SHA-256 over tree nodes (each changed object rewrites its leaf and part
  of the parents, about 8 KB each), and random objects hit cold leaves that must be read.
  Group commit removes the per-commit flush and fsync but not this work. At 1–10 objects
  per commit this workstation reaches about 57 commits/s; MET-PERF-002 needs 500/s. The
  run on ENV-REF-SERVER (SHA extensions, faster cores) decides; if it still falls short,
  step 7 records the remedy (prepare the commits of a group in parallel and serialise only
  the tree update; deferred tree maintenance as named for RSK-001; or the DEC-020 fallback).
- C-2: Point reads and 10-change diffs are far inside MET-PERF-001 (6.25 ms) and
  MET-PERF-003 (62.5 ms) at this scale; the 10M dataset and the rented hosts must show
  whether this holds with a cold cache and a larger tree.
- C-3: The generator lives in `ubos-bench` (engine layer) because it writes the store
  layout directly; UBS-VER-03 names `ubos-verify gen generic`, which is the PH-1 home once
  that crate exists (step 7).
- C-4: The full dataset needs about 75 GB of tables before indexes and WAL; the database
  host needs at least 400 GB free.

Reproduce:

```sh
cargo build --release -p ubos-dvm --bins
./target/release/ubos-props --only tree_incremental --cases 100000 --report tree-property.json
./target/release/ubos-bench tree --keys 1000000 --changes 10 --rounds 300
DB="host=127.0.0.1 user=postgres dbname=bpa"
./target/release/ubos-bench check --objects 5000 --months 24 --database "$DB"
./target/release/ubos-bench load --objects 200000 --months 24 --database "$DB"
./target/release/ubos-bench run --what read --objects 200000 --months 24 --seconds 60 --rate 1000 --workers 4 --database "$DB"
```
