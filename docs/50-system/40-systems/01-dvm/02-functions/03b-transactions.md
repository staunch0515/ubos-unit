---
id: UBS-SYS-DVM-02-07
title: DVM — Transaction Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-03, UBS-STD-07]
---

# DVM — Transaction Design (DSN-DVM-5xx)

This file designs the crate `ubos-txn`. Reading order:
1. processes and the flush (501–508);
2. concurrency control (510–516);
3. signing (520–522);
4. idempotency, lineage and outbox (530–538);
5. deferred commits, bulk processes and sagas (550–566).

## Processes and the flush

### DSN-DVM-501 — Process object and lifecycle
- **Statement:** A process MUST be created by the host API with a context (STD-CTX-001) and MUST pass through states: created, running, staged, flushing, committed, aborted, or deferred (DSN-DVM-550). The in-memory process holds a snapshot map, a staging area, a read set, a journal and a metering record.
- **Rationale:** STD-TXN-001.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given every state transition, when exercised, then only the transitions of the state machine in UBS-SYS-DVM-05 are possible.
- **Verification:** CONF, PROP
- **Origin:** FR-TXN-011, STD-TXN-001

### DSN-DVM-502 — Staging area
- **Statement:** The staging area MUST hold, per object, the base version and the pending new state; per ledger, the pending entries in append order; and the pending events and effects. Reads within the process MUST see its own staged writes (read-your-writes) layered over the snapshot.
- **Rationale:** Unit of work (FR-TXN-012, FR-TXN-014).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a port that updates an object and calls another port that reads it, when run, then the inner port sees the update.
- **Verification:** CONF
- **Origin:** FR-TXN-012, FR-TXN-014, STD-TXN-002

### DSN-DVM-503 — Flush pipeline
- **Statement:** The flush MUST execute these steps:
  1. pre-lock: validation plans, sheet invariants that do not depend on concurrent state, chunk upload;
  2. enqueue to the branch sequencer;
  3. under the lock: head CAS, base and read-set validation, unique keys, balance invariants, ledger sequences, tree update, commit construction and signature;
  4. one storage transaction writing chunks' descriptors, versions, entries, indexes, projections, commit, outbox rows and idempotency records;
  5. release the lock and publish the new head to caches.

  Only step 4 writes authoritative state (AR-004).
- **Rationale:** Minimal time under lock, one atomic write.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a crash injected at every step boundary, when recovered, then either the complete commit or nothing is visible (FR-TXN-013).
- **Verification:** FAULT, SIM
- **Origin:** FR-TXN-012, FR-TXN-013, STD-TXN-003, STD-TXN-004, AR-004

### DSN-DVM-504 — Commit latency budget
- **Statement:** For a single-object process on ENV-REF-SERVER, the flush budget MUST be allocated as: pre-lock ≤ 2 ms, lock wait ≤ 2 ms (group commit window), under-lock work ≤ 1 ms, storage commit ≤ 5 ms, giving NR-PERF-002. Each step MUST emit a span with its duration.
- **Rationale:** A measurable budget makes regressions visible.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the benchmark, when run, then per-step p95 values meet the budget.
- **Verification:** BENCH
- **Origin:** NR-PERF-002, NR-OBS-001

### DSN-DVM-505 — Empty and read-only processes
- **Statement:** A process with no staged writes MUST complete without a commit. Read-only processes MUST NOT take the sequencer lock.
- **Rationale:** Avoid empty commits and lock pressure.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 1,000 read-only processes, when run, then no commit and no lock acquisition occurs.
- **Verification:** CONF
- **Origin:** FR-VER-013

### DSN-DVM-506 — Multi-branch processes are forbidden
- **Statement:** A process MUST write to exactly one branch. Writes to another branch MUST be done by a separate process linked by a saga (DSN-DVM-564) or by merge.
- **Rationale:** Atomicity stays per branch.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a process that writes to two branches, when flushed, then it fails before any write.
- **Verification:** CONF
- **Origin:** STD-TXN-003

### DSN-DVM-507 — Process limits
- **Statement:** The DVM MUST enforce per-profile limits on staged objects (default 10,000), staged bytes (default 64 MB) and ledger entries (default 50,000) per process. Larger work MUST use bulk processes (DSN-DVM-560).
- **Rationale:** Predictable memory and lock duration.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a process staging 10,001 objects under default limits, when staging, then `LOGIC.LIMIT_EXCEEDED` names the limit.
- **Verification:** CONF
- **Origin:** STD-CTX-004, STD-TXN-070

### DSN-DVM-508 — Crash recovery
- **Statement:** On start, the DVM MUST require no recovery step beyond the storage engine's own: a commit is visible if and only if its storage transaction committed. In-flight processes are lost and MUST be retried by their callers or idempotently re-delivered.
- **Rationale:** Simple, provable recovery (NR-DUR-001).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a process kill (SIGKILL) during 1,000 random flushes, when restarted, then every acknowledged commit is present, and no partial commit exists.
- **Verification:** FAULT
- **Origin:** NR-DUR-001, FR-TXN-013

## Concurrency control

### DSN-DVM-510 — Base validation
- **Statement:** Every staged update MUST name its base version (STD-TXN-020). Under the lock, the flush MUST compare each base with the head version of the object on the branch. Mismatches MUST abort the process with `TXN.HEAD_MOVED` listing all mismatching objects (FR-TXN-024).
- **Rationale:** Object-level compare-and-set (STD-TXN-021).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given two processes updating the same object from the same base, when flushed, then one succeeds, and the other fails naming the object.
- **Verification:** SIM, CONF
- **Origin:** FR-TXN-021, FR-TXN-022, FR-TXN-024, STD-TXN-020, STD-TXN-021

### DSN-DVM-511 — Serializable read-set validation
- **Statement:** Processes running in the serializable mode (STD-TXN-022) MUST record their read set: point reads as `(object, version)` and query reads as `(predicate hash, index ranges, result versions)`. Under the lock, the flush MUST detect writes committed after the snapshot that intersect the read set (by object or by index range) and abort with `TXN.READ_SET_CHANGED`.
- **Rationale:** Serializable business logic where needed (FR-TXN-023).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the write-skew scenario of two on-call doctors, when run serializably, then one process aborts.
  2. Given 10,000 random concurrent histories, when checked by an Elle-style checker, then no serializability anomaly is found.
- **Verification:** SIM, PROP
- **Origin:** FR-TXN-023, STD-TXN-022

### DSN-DVM-512 — Committed read and write summaries
- **Statement:** Each commit MUST store a compact write summary (object IDs and touched index ranges) in `commit_write_summary`, retained for the maximum process age (default 10 minutes), to validate read sets without scanning versions. Read sets of committed processes MUST be stored for lineage when the class requests it (FR-TXN-042).
- **Rationale:** Efficient validation and lineage.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 1,000 commits since a snapshot, when validating a read set of 100 items, then validation takes under 1 ms.
- **Verification:** BENCH
- **Origin:** FR-TXN-042, FR-TXN-043, STD-TXN-022

### DSN-DVM-513 — Automatic retry
- **Statement:** Processes declared re-executable (STD-TXN-023) MUST be retried automatically after `TXN.HEAD_MOVED` or `TXN.READ_SET_CHANGED` with a new snapshot, up to 5 attempts with jittered back-off (5, 10, 20, 40 ms). The journal of each attempt MUST be kept until the process ends. Pre-commit external calls MUST prevent automatic retry unless idempotent.
- **Rationale:** FR-TXN-025.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given 50 concurrent increments of a counter by re-executable processes, when run, then the final value is 50.
- **Verification:** SIM
- **Origin:** FR-TXN-025, STD-TXN-023

### DSN-DVM-514 — Hot-object mitigation
- **Statement:** The DVM MUST expose per-object conflict counters. Objects whose conflict rate exceeds a threshold MUST be reported with a recommendation (use a ledger, a sum-of-deltas field, or a serial queue).
- **Rationale:** Visible contention guides modelling.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a counter object updated by 100 concurrent processes, when observed, then it appears in the hot-object report.
- **Verification:** BENCH
- **Origin:** NR-OBS-002

### DSN-DVM-515 — Concurrency formal model
- **Statement:** Base validation, read-set validation, head CAS and group commit MUST be modelled formally with the properties: linear history per branch; no lost update; serializability for serializable processes; dense `cseq`.
- **Rationale:** Concurrency is verified before implementation (AR-033).
- **Priority:** Must · **Phase:** PH-0 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the model with 3 processes, 2 nodes and 3 objects, when checked, then no property is violated.
- **Verification:** FORM
- **Origin:** STD-TXN-021, STD-TXN-022, AR-033

### DSN-DVM-516 — Deterministic simulation harness
- **Statement:** `ubos-txn` MUST be testable under a deterministic simulator in which the clock, the scheduler, storage latency and crashes are controlled by a seed. Every failing seed MUST be reproducible.
- **Rationale:** Concurrency bugs are found and replayed deterministically.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a seed that exposed a bug, when re-run, then the same interleaving occurs.
- **Verification:** SIM
- **Origin:** NR-MAINT-002, AR-032

## Signing

### DSN-DVM-520 — Commit signing
- **Statement:** Every commit MUST be signed with Ed25519 (STD-PROOF-010) by the author key through the host signing service (CTR-002). For custodial keys, the host MAY cache the unwrapped key in node memory so signing takes under 100 µs; for HSM keys, the sequencer MUST sign outside the lock with a pre-computed commit body and verify the head again before writing.
- **Rationale:** Signed commits without breaking the latency budget.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given every commit in a test run, when verified with the key history, then all signatures are valid.
- **Verification:** CONF, BENCH
- **Origin:** STD-PROOF-010, NR-SEC-006

### DSN-DVM-521 — Key history verification
- **Statement:** Signature verification MUST use the key valid at the commit's transaction time, from the principal's key history (STD-PROOF-011). A signature by a key revoked before the commit's transaction time MUST fail with `PROOF.SIGNATURE_INVALID`.
- **Rationale:** Signatures stay verifiable after rotation.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a key rotated in March, when verifying a February commit, then the old key is used, and verification succeeds.
- **Verification:** CONF
- **Origin:** STD-PROOF-011

### DSN-DVM-522 — Co-signatures
- **Statement:** A commit MAY carry additional signatures (approvers of a change set, parties of a shared BPU). They MUST be over the same commit ID and stored in a separate `commit_signature` table so they can be added after the commit without changing its ID.
- **Rationale:** Multi-party attestations.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a merge commit, when two approvers co-sign, then both signatures verify, and the commit ID is unchanged.
- **Verification:** CONF
- **Origin:** STD-PROOF-010, STD-SYNC-011

## Idempotency, lineage and outbox

### DSN-DVM-530 — Idempotency records
- **Statement:** A process started with an idempotency key MUST record `(tenant, scope, key) → (request hash, outcome, commit ID)` in the flush. A second request with the same key and same hash MUST return the recorded outcome without executing; with a different hash it MUST fail with `TXN.IDEMPOTENCY_MISMATCH` (STD-TXN-040).
- **Rationale:** FR-TXN-031.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given 10 concurrent requests with one key, when processed, then exactly one executes, and all return the same outcome.
- **Verification:** SIM, CONF
- **Origin:** FR-TXN-031, STD-TXN-040, AR-010

### DSN-DVM-531 — Idempotency retention
- **Statement:** Idempotency records MUST be kept for the configured retention (default 7 days, FR-TXN-032) and then removed by a sweep. Requests after expiry MUST be treated as new.
- **Rationale:** Bounded storage.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a record older than retention, when swept, then it is removed.
- **Verification:** CONF
- **Origin:** FR-TXN-032
- **Phase note:** PH-2 step 0b (DEC-087): moved from PH-1 with FR-TXN-032.

### DSN-DVM-532 — Trigger lineage
- **Statement:** Every process MUST record its trigger (user request, event, job, flow step, agent task, parent process) and its causality chain ID (STD-TXN-031). The DVM MUST answer "why did this change happen" by walking triggers and derivations (FR-TXN-043).
- **Rationale:** FR-TXN-041, FR-TXN-043.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a posting created by a flow started by an event from a user commit, when asked why, then the chain back to the user commit is returned.
- **Verification:** CONF
- **Origin:** FR-TXN-041, FR-TXN-043, STD-TXN-031

### DSN-DVM-533 — Derivation records
- **Statement:** Derived writes MUST record their input versions (STD-TXN-030) as rows in `derivation_edge` `(output version, input version, rule binding)`. Inputs MUST be taken from the read set filtered to the reads that the deriving port declared as inputs, or all reads when undeclared.
- **Rationale:** FR-TXN-042, impact analysis (DSN-DVM-321).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a NAV derived from 20 prices, when inspected, then 20 input edges exist.
- **Verification:** CONF
- **Origin:** FR-TXN-042, STD-TXN-030

### DSN-DVM-534 — Outbox
- **Statement:** Events and external effects (notifications, connector calls declared after-commit) MUST be written as rows in `outbox` in the flush transaction (STD-TXN-050). The DVM MUST expose `outbox_claim(n, lease)` and `outbox_ack(ids)` for the host, which releases them at least once (FR-TXN-052).
- **Rationale:** Effects happen if and only if the commit happens.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given an aborted process that emitted events, when inspected, then no outbox row exists.
  2. Given a host crash after claiming, when the lease expires, then the rows are claimed again.
- **Verification:** FAULT, CONF
- **Origin:** FR-TXN-051, FR-TXN-052, STD-TXN-050, NR-DUR-006

### DSN-DVM-535 — Outbox ordering
- **Statement:** Outbox rows MUST carry `(branch, cseq, index within commit)`, and the host MUST release events of one aggregate key in that order.
- **Rationale:** Consumers see events in commit order per key.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given 100 commits on one object, when consumed, then events arrive in commit order.
- **Verification:** SIM
- **Origin:** FR-TXN-052

### DSN-DVM-536 — Pre-commit external calls
- **Statement:** External calls made before commit (`call.connector` in pre-commit mode, STD-TXN-051) MUST be journaled with request and response. If the process aborts, a declared compensation MUST be enqueued through a separate always-committed compensation record.
- **Rationale:** FR-TXN-053.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a pre-commit reservation followed by an abort, when inspected, then a compensation record exists and is released.
- **Verification:** FAULT
- **Origin:** FR-TXN-053, STD-TXN-051

### DSN-DVM-537 — Channel and client attribution
- **Statement:** Every process MUST record the channel (UI, API, SDK, agent, job, sync) and client identity from the context, stored in the commit metadata.
- **Rationale:** FR-TXN-044.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given commits from UI and API, when listed, then each shows its channel and client.
- **Verification:** CONF
- **Origin:** FR-TXN-044

### DSN-DVM-538 — Idempotent handlers
- **Statement:** Event handlers MUST be invoked with an idempotency key derived from `(handler, event ID)`, so re-delivered events do not create duplicate effects (FR-TXN-033).
- **Rationale:** At-least-once delivery with exactly-once effect.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given one event delivered three times, when handled, then one commit results.
- **Verification:** SIM
- **Origin:** FR-TXN-033

## Deferred commits, bulk processes and sagas

### DSN-DVM-550 — Deferred commits
- **Statement:** When a decision returns "requires approval" (STD-TXN-060), the flush MUST, instead of committing to the target, persist the staged changes as a `PendingChange` object on the target branch (in a System-kind area), with the snapshot, the base versions and a preview hash. The process ends in state deferred.
- **Rationale:** Approvals without long-running locks.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a payment above the approval threshold, when submitted, then a pending change exists, and the target state is unchanged.
- **Verification:** CONF
- **Origin:** STD-TXN-060, FR-VER-031

### DSN-DVM-551 — Applying a pending change
- **Statement:** Approval of a pending change MUST start a process that re-validates bases against the current head. If bases are unchanged, it commits the staged changes with the approval recorded. If bases changed, it MUST re-run the original process (when re-executable) and require re-approval when the preview hash differs.
- **Rationale:** What was approved is what is committed.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a pending change whose base object changed, when approved, then re-approval is requested with the new preview.
- **Verification:** CONF, SIM
- **Origin:** STD-TXN-060, STD-VER-021

### DSN-DVM-552 — Pending-change expiry
- **Statement:** Pending changes MUST expire after a configured time (default 30 days) and move to state expired, emitting an event.
- **Rationale:** No indefinitely open work.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a pending change older than its expiry, when the sweep runs, then it is expired, and an event is emitted.
- **Verification:** CONF
- **Origin:** STD-TXN-060

### DSN-DVM-560 — Bulk processes
- **Statement:** A bulk process (STD-TXN-070) MUST iterate a query result in batches (default 1,000 objects), commit one commit per batch with a common bulk ID, and persist a checkpoint (last key processed) in each commit. On restart, it MUST resume from the checkpoint. It MUST be cancellable between batches.
- **Rationale:** FR-TXN-015.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given 1,000,000 objects and a crash after 400 batches, when resumed, then processing continues at batch 401, and no object is processed twice.
- **Verification:** FAULT
- **Origin:** FR-TXN-015, STD-TXN-070

### DSN-DVM-561 — Bulk throughput and fairness
- **Statement:** Bulk processes MUST run in the batch profile with a lower sequencer priority than interactive processes, so interactive p95 commit latency on the same branch rises by no more than 50%.
- **Rationale:** Batch work does not starve users.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a bulk process and interactive load on one branch, when benchmarked, then interactive latency meets the bound.
- **Verification:** BENCH
- **Origin:** NR-PERF-022, STD-CTX-004

### DSN-DVM-564 — Sagas
- **Statement:** A saga (STD-TXN-080) MUST be a System-kind object with steps, each a process on a possibly different branch or VAE, with a compensation per step. The saga coordinator MUST run in the host, persisting step state in the saga object through processes, so it survives restarts.
- **Rationale:** FR-TXN-061, FR-TXN-062.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a 3-step saga that fails at step 3, when run, then compensations for steps 2 and 1 run in reverse order.
- **Verification:** FAULT, CONF
- **Origin:** FR-TXN-061, FR-TXN-062, STD-TXN-080

### DSN-DVM-565 — Saga timeouts and monitoring
- **Statement:** Each saga step MUST have a timeout; on timeout the coordinator MUST compensate or escalate per declaration. Sagas MUST be queryable by state, age and failing step (FR-TXN-064).
- **Rationale:** FR-TXN-063, FR-TXN-064.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a step that never answers, when its timeout passes, then the declared action runs, and the saga appears in the "stuck" list until resolved.
- **Verification:** FAULT
- **Origin:** FR-TXN-063, FR-TXN-064

### DSN-DVM-566 — Saga formal model
- **Statement:** The saga protocol MUST be modelled formally with properties: every started saga ends completed or compensated; compensations run at most once per step.
- **Rationale:** Distributed failure handling is verified before implementation.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the model with 3 steps and crash injection, when checked, then no property is violated.
- **Verification:** FORM
- **Origin:** STD-TXN-080
