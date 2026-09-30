---
id: UBS-SYS-DVM-02-01
title: DVM — Storage Engine Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02, UBS-ARC-04, UBS-STD-08]
---

# DVM — Storage Engine Design (DSN-DVM-0xx)

### DSN-DVM-001 — Storage adapter abstraction
- **Statement:** `ubos-store` MUST define a transactional storage trait (`StoreTx`) with the operations the DVM needs:
  - `put_chunks`, `put_versions`, `put_entries`, `put_commit`, `cas_branch_head`;
  - `upsert_index_rows`, `put_tree_nodes`, `alloc_sequences`;
  - `outbox_append`, `idempotency_put`;
  - snapshot reads by commit ordinal.

  The operations are split into `StoreTx` (one storage transaction) and `StoreRead` (snapshot reads by commit ordinal); both are asynchronous and use only backend-neutral row types (DEC-047).

  The PostgreSQL 16+ adapter MUST exist from PH-1. The SQLite 3.45+ adapter follows in PH-4 (DEC-024) and MUST then pass the same storage conformance suite; the trait MUST stay backend-neutral so that SQLite needs no kernel change.
- **Rationale:** One kernel, several editions (AR-002, CST-002).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the storage suite, when run against the PostgreSQL adapter, then 100% pass (PH-1).
  2. Given the SQLite adapter (PH-4), when the same suite runs, then 100% pass with results identical to PostgreSQL.
- **Verification:** CONF, PROP
- **Origin:** DEC-020, CTR-005, NR-PORT-003

### DSN-DVM-002 — Logical identity versus physical form
- **Statement:** Content IDs MUST be computed over the logical canonical bytes (STD-FND-010), which include field ciphertexts (STD-OBJ-010). The physical form applied by the store (zstd compression, then AEAD encryption with the tenant data key) MUST NOT affect content IDs. The store MUST verify on read that decrypting and decompressing reproduces bytes whose hash equals the ID. The stored form is `0x01 | key id | 24-byte nonce | XChaCha20-Poly1305 ciphertext` with the tenant and content ID as associated data (DEC-048).
- **Rationale:** Cross-implementation equality of roots with per-tenant physical protection.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given two tenants storing identical logical content, when compared, then their content IDs are equal and their stored bytes differ.
  2. Given a corrupted stored chunk, when read, then `NOD.BLOB_UNAVAILABLE` with an integrity flag is raised.
- **Verification:** CONF, FAULT
- **Origin:** STD-FND-010, AR-020, FR-IAM-092

### DSN-DVM-003 — Chunk placement
- **Statement:** Chunks up to 8 KiB (physical size) MUST be stored inline in the `chunk` table. Larger chunks MUST be stored in the object store under `t/<tenant-hash>/c/<content-id>`, through the host blob service, before the flush transaction starts. Only their descriptors are written in the transaction.
- **Rationale:** Small objects stay fast; large content avoids database bloat.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a 3 MB document, when committed, then its chunks are in the object store and the database holds descriptors only.
- **Verification:** CONF, BENCH
- **Origin:** STD-OBJ-008, CTR-006

### DSN-DVM-004 — Commit ordinals
- **Statement:** Each branch MUST assign every commit a dense ordinal `cseq` (1, 2, …) along its first-parent history. Kernel indexes MUST use `cseq` intervals (`cseq_from`, `cseq_to`) instead of timestamps. A `time → cseq` mapping (the commit table sorted by `tx_time`) MUST resolve `time=` selectors.
- **Rationale:** Exact snapshot semantics with integer comparisons.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given commits 1…1,000, when reading at `cseq=500`, then the results equal the state after commit 500.
- **Verification:** PROP
- **Origin:** STD-VER-090, FR-VER-101

### DSN-DVM-005 — Kernel index maintenance in the flush
- **Statement:** The flush MUST update, in the same transaction:
  - `head_index`: local current versions per branch;
  - `temporal_index`: timeline segments with `cseq` intervals;
  - `relationship_index`, `unique_key_index`;
  - `derivation_edge` (for impact analysis).

  The index rows MUST be a pure function of the committed changes. A background verifier (DSN-DVM-092) MUST be able to rebuild and compare any of them.
- **Rationale:** Consistent reads without authoritative indexes (AR-015, AR-016).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a random workload, when all indexes are rebuilt from commits, then they equal the maintained indexes.
- **Verification:** PROP, SIM
- **Origin:** AR-016, UBS-ARC-04

### DSN-DVM-006 — Branch sequencer with group commit
- **Statement:** Commits to a branch MUST be serialised by a sequencer that holds the branch's head row lock (`SELECT … FOR UPDATE` on `branch_ref`) for the duration of the flush. A node MUST batch concurrently ready processes targeting the same branch into one database transaction, producing consecutive commits (group commit, default batch ≤ 64 or 2 ms). Hot branches MAY be assigned a sequencer leader node by lease, and other nodes forward flushes to it. A flush has two phases (W-8, DEC-046): the processes of a batch are prepared in parallel without the lock (snapshot, object writes with their bases, ledger appends without sequences); the sequencer then, under the lock and in batch order, validates each process against its view (DSN-DVM-208), allocates ledger sequences (DSN-DVM-402), evaluates balance invariants (DSN-DVM-424), aborts only an offending process, and writes the accepted commits, counter rows and tree update in one storage transaction. If that transaction fails, the accepted processes are sequenced again; nothing of the batch remains (checked by VER-FORM-0004).
- **Rationale:** NR-PERF-003 throughput with strict per-branch order.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 500 concurrent single-object processes on `main`, when benchmarked, then the throughput meets NR-PERF-003, and `cseq` is dense.
  2. Given a failed process inside a batch, when flushing, then only that process aborts, and the other commits succeed.
- **Verification:** BENCH, SIM, FAULT, FORM
- **Origin:** DEC-020, NR-PERF-002, NR-PERF-003, STD-TXN-021

### DSN-DVM-007 — Prolly tree maintenance
- **Statement:** `ubos-tree` MUST apply each commit's key changes to the previous root in batch, re-chunking only the affected paths (STD-PROOF-001). New nodes are written to `merkle_node` (content-addressed, deduplicated). A process-local node cache and a node-level shared LRU cache (default 256 MB) MUST be used.
- **Rationale:** Root per commit at O(changes × log n).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 10 changes in a 10,000,000-key tree, when committed, then at most ~4 × 10 nodes are written, and the root matches a full rebuild.
- **Verification:** PROP, BENCH
- **Origin:** STD-PROOF-001, NR-DET-003

### DSN-DVM-008 — Snapshot reads
- **Statement:** A process snapshot MUST be represented as a map `branch → cseq`, fixed at first read of each branch. Every read MUST filter index rows by `cseq_from ≤ s < cseq_to`. On PostgreSQL, reads within a process MAY run in one REPEATABLE READ transaction for efficiency, but correctness MUST NOT depend on it.
- **Rationale:** STD-TXN-002 on any backend.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given concurrent commits during a long process, when it reads repeatedly, then it always sees its snapshot.
- **Verification:** SIM, PROP
- **Origin:** STD-TXN-002, FR-VER-103

### DSN-DVM-009 — Caches
- **Statement:** The DVM MUST provide bounded caches, each keyed by content ID or `(branch, cseq)`, and never by mutable names alone (AR-028):
  - version bodies (by hash);
  - effective class definitions (by the hash of input versions);
  - compiled sheets, expressions and scripts (by hash);
  - inheritance negative results (by branch and `cseq`).

  The default memory budget of each cache MUST be configurable.
- **Rationale:** Performance without correctness risk.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given all caches disabled, when the conformance suite runs, then results are identical to runs with caches enabled.
- **Verification:** CONF, PROP
- **Origin:** AR-028, NR-PERF-001

### DSN-DVM-010 — PostgreSQL specifics
- **Statement:** The PostgreSQL adapter MUST:
  - enable row-level security on every tenant table, keyed on `current_setting('ubos.tenant')`;
  - hash-partition `object_version`, `ledger_entry`, `temporal_index` and `chunk` by tenant (64 partitions by default);
  - use GiST or range indexes for valid periods;
  - use synchronous commit;
  - avoid long-held row locks outside the sequencer.

  The adapter works as an application role that does not own the tables, so that the policies apply; migrations and bulk loads run as the owner (DEC-048).
- **Rationale:** Isolation and scale (AR-017).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a session without `ubos.tenant`, when querying any tenant table, then zero rows are returned.
- **Verification:** SEC, BENCH
- **Origin:** AR-017, FR-TEN-021

### DSN-DVM-011 — SQLite specifics
- **Statement:** The SQLite adapter MUST use WAL mode, a single writer connection (the sequencer), `synchronous=FULL`, and page-level encryption of the database file with the device key (SQLCipher-compatible) on Box, mobile and browser.
- **Rationale:** Durable, encrypted local storage.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given power loss during commit on Box, when restarted, then the last acknowledged commit is present.
- **Verification:** FAULT, SEC
- **Origin:** NR-DUR-001, FR-SYNC-043
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-DVM-012 — Cold-tier reads
- **Statement:** When a chunk is marked tiered, reads MUST fetch it from the cold tier through the host blob service, verify its hash, and optionally rehydrate it into the hot tier per policy.
- **Rationale:** Tiering stays transparent (FR-VER-092).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM, NOD
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a tiered version, when read, then it is returned within NR-PERF-021, and the proofs verify.
- **Verification:** BENCH
- **Origin:** FR-VER-091, FR-VER-092, NR-PERF-021

### DSN-DVM-013 — Garbage handling of unreferenced chunks
- **Statement:** Chunks written to the object store for processes that later abort MUST be collected by a mark-and-sweep job that removes chunks unreferenced by any version or journal and older than 24 hours. The job MUST never remove referenced or WORM-locked content.
- **Rationale:** Storage hygiene without risk.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 1,000 aborted uploads, when the sweep runs after 24 hours, then they are removed, and no referenced chunk is touched.
- **Verification:** FAULT
- **Origin:** NEW — hygiene of pre-transaction uploads (DSN-DVM-003)

## Verification of indexes

### DSN-DVM-092 — Index and projection verifier
- **Statement:** The DVM MUST provide `verify_indexes(branch, range)`. It rebuilds kernel indexes for the range from commits, and compares them by order-independent checksums per partition, reporting mismatching keys.
- **Rationale:** AR-015 and NR-DUR-005.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an injected index corruption, when verified, then the affected keys are reported.
- **Verification:** FAULT
- **Origin:** AR-015, FR-ANL-022
