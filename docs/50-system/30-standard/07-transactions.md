---
id: UBS-STD-07
title: BPA Standard — Processes and Commit Semantics (TXN)
status: draft
phase: PH-0
depends_on: [UBS-STD-05, UBS-STD-06]
---

# BPA Standard — Processes and Commit Semantics (TXN)

## 1. Processes

### STD-TXN-001 — Process record
- **Clause:** Every state change MUST occur inside a process described by `DAT-Process`. A process has the states `running`, `committed`, `aborted` and `failed`, and ends in exactly one of the last three.
- **Grammar / Schema:** `DAT-Process`
- **Conformance vectors:** VER-CONF-2000…2014
- **Satisfies:** FR-TXN-011, FR-TXN-044
- **Notes:** —

```yaml
DAT-Process:
  process_id: { type: text, required: true, description: "UUID" }
  kind: { type: enum, required: true, description: "action | schedule | job | handler | merge | import | agent_run | sync | kernel" }
  initiator: { type: text, required: true, description: "principal DID" }
  on_behalf_of: { type: text, required: false, description: "principal DID" }
  channel: { type: enum, required: true, description: "workspace | studio | api | sdk | agent | integration | schedule | kernel" }
  client: { type: text, required: false, description: "client application and version" }
  trigger: { type: composite, required: true, description: "{type: user|event|schedule|job|merge|external, ref: text}" }
  parent_process: { type: text, required: false, description: "triggering process id" }
  saga: { type: text, required: false, description: "saga id" }
  bulk: { type: text, required: false, description: "bulk process id" }
  started_at: { type: instant, required: true, description: "host time at start" }
  ended_at: { type: instant, required: false, description: "host time at end" }
  state: { type: enum, required: true, description: "running | committed | aborted | failed" }
  commits: { type: list, required: false, description: "commit ids produced" }
  journal: { type: text, required: false, description: "content-id of the execution journal" }
  idempotency_key: { type: text, required: false, description: "client key" }
  message: { type: text, required: false, description: "commit message; required by audit_level strict (STD-LGS-011); since 1.0-rc2" }
  reason: { type: text, required: false, description: "reason code; required by audit_level strict (STD-LGS-011); since 1.0-rc2" }
```

### STD-TXN-002 — Snapshot and unit of work
- **Clause:** A process MUST read from a snapshot fixed at its first read of each branch. It MUST observe its own uncommitted changes (read-your-writes). Its changes MUST be invisible to other processes until its commit becomes visible.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2015…2029
- **Satisfies:** FR-TXN-012, FR-TXN-014, FR-VER-103
- **Notes:** —

### STD-TXN-003 — Atomic visibility
- **Clause:** All changes of a process on one branch MUST become visible together in one commit, or not at all. A process that writes several branches produces one commit per branch. These commits are individually atomic, and the process MUST NOT report `committed` until all have been written. If a later branch write fails, the earlier commits MUST be compensated by revert within the same process, and the process ends `failed`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2030…2044
- **Satisfies:** FR-TXN-013, FR-VER-013
- **Notes:** Multi-branch processes are rare (for example agent feedback plus audit). Implementations SHOULD avoid them.

### STD-TXN-004 — Flush content
- **Clause:** The flush that makes a commit visible MUST durably write, in one storage transaction:
  - new chunks and versions;
  - entries with their sequence numbers;
  - the commit record and branch head;
  - the kernel indexes and Merkle nodes;
  - the rule binding, the outbox, `sync` projections, the idempotency record and the process state.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2045…2054
- **Satisfies:** FR-TXN-012, FR-TXN-051
- **Notes:** Chunks already stored in the object store before the transaction are allowed. They are content-addressed and harmless if unreferenced.

## 2. Concurrency

### STD-TXN-020 — Every update names its base
- **Clause:** Every `update`, `end`, `move` and `hide` change MUST carry the version hash it was derived from (`base`). `create` MUST fail if the object already exists on the branch (`TXN.ALREADY_EXISTS`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2060…2069
- **Satisfies:** FR-TXN-021
- **Notes:** —

### STD-TXN-021 — Object-level compare-and-set
- **Clause:** At flush, for each change with a base, the implementation MUST verify that the object's current version on the branch equals the base. If not, the commit MUST fail with `TXN.HEAD_MOVED`, listing every object whose base is stale, with its current version and changed field paths. The branch head moving for unrelated objects MUST NOT cause failure.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2070…2089
- **Satisfies:** FR-TXN-021, FR-TXN-022, FR-TXN-024
- **Notes:** Commits on one branch are serialised by the branch sequencer. Only object bases are compared.

### STD-TXN-022 — Serializable processes
- **Clause:** A process declared `serializable` MUST additionally record the version of every object it read (its read set). At flush it MUST fail with `TXN.READ_SET_CHANGED` if any of them changed on the branch since its snapshot.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2090…2099
- **Satisfies:** FR-TXN-023
- **Notes:** —

### STD-TXN-023 — Automatic retry
- **Clause:** Processes of logic declared `retryable` MAY be re-executed from the start by the implementation after `TXN.HEAD_MOVED` or `TXN.READ_SET_CHANGED`. Each attempt uses a fresh snapshot and the same idempotency key, up to the configured maximum (default 5). Non-deterministic journal values MUST be re-drawn per attempt, and only the successful attempt's journal is kept.

  `retryable` is declared on the exported function. `max_attempts` (default 5) counts all attempts. Each attempt draws its entropy from SHA-256 of the seed and the attempt number (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2100…2109
- **Satisfies:** FR-TXN-025
- **Notes:** —

### STD-TXN-024 — Ledger appends and concurrency `[Profile: Ledger]`
- **Clause:** Appends carry no base and never fail with `TXN.HEAD_MOVED`. Invariants over `sync` balance projections MUST be evaluated at flush against the serialised state including all earlier commits of the branch.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2110…2119
- **Satisfies:** FR-LEDG-034, FR-LEDG-042
- **Notes:** —

## 3. Provenance

### STD-TXN-030 — Derivation records
- **Clause:** For every version or entry written by logic (not by direct field input), the process MUST record a `DAT-Derivation`: the producing logic versions, and the input versions and entries read, as `(object, version, asof)` triples. The record MUST be referenced from the output's `provenance_ref`. When the input set exceeds the configured maximum (default 10,000), it MUST be summarised per class as version ranges plus a Merkle commitment to the full list, which is stored as a chunk.

  In detail (DEC-041): inputs are deduplicated and sorted by object, `asof` and version. The record's content ID (`ubos.chunk.v1`) is the `provenance_ref`, which is part of the version envelope (STD-OBJ-002) and of the entry hash (STD-KIND-020). `mode` defaults to `as_effective`, and `logic` is not empty. `version_range` holds the lowest and highest commit sequence numbers of the input versions of the class, and `inputs_commitment` is the content ID of the full sorted list. An input that names no version of its object fails with `MODEL.REFERENCE_INVALID`.
- **Grammar / Schema:** `DAT-Derivation`
- **Conformance vectors:** VER-CONF-2120…2134
- **Satisfies:** FR-TXN-042, FR-TIME-062, FR-AUD-021
- **Notes:** —

```yaml
DAT-Derivation:
  process_id: { type: text, required: true, description: "producing process" }
  logic: { type: list, required: true, description: "content-ids of logic assets, tables and sheets used" }
  inputs: { type: list, required: false, description: "{object, version, asof} triples" }
  inputs_summary: { type: list, required: false, description: "{class, version_range, count} when truncated" }
  inputs_commitment: { type: text, required: false, description: "content-id of the full input list" }
  mode: { type: enum, required: true, description: "as_originally | as_effective | current (STD-TIME-041)" }
```

### STD-TXN-031 — Trigger lineage
- **Clause:** Every process MUST record its trigger, and its `parent_process` when triggered by an event, job or process. The implementation MUST answer ancestor and descendant queries over these links. A process with trigger type `event` or `job` and no parent fails with `MODEL.DECLARATION_INCOMPLETE`, and a named parent must exist (`IAM.NOT_FOUND`). Ancestors are listed from the nearest parent; descendants are the transitive closure (DEC-041).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2135…2144
- **Satisfies:** FR-TXN-041, FR-TXN-043
- **Notes:** —

## 4. Idempotency

### STD-TXN-040 — Idempotency keys
- **Clause:** An idempotency key MUST be scoped by (tenant, VAE, initiator principal, key). The implementation MUST store the payload hash and the result reference with the commit.
  - A request with a known key and an equal payload hash MUST return the stored result without effect.
  - A request with a known key and a different payload hash MUST fail with `TXN.IDEMPOTENCY_MISMATCH`.
  - Records MUST be kept at least 7 days.

  The payload hash is the content ID of the request (branch and changes). Only committed requests store a record, and a replay writes no effects (DEC-041).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2150…2164
- **Satisfies:** FR-TXN-031, FR-TXN-032, FR-TXN-033
- **Notes:** —

## 5. Outbox

### STD-TXN-050 — Effects after commit
- **Clause:** Effects (events, job enqueues, notifications, deferred connector calls) requested by a process MUST be written to the outbox in the flush, with stable effect IDs. They are released only after the commit is durable, delivered at least once, and ordered per object by commit order. Aborted processes MUST release nothing. Effect IDs are `<process_id>/<n>`, numbered from 1 in request order within the process; delivery follows commit order, and an effect is delivered again until it is acknowledged (DEC-041).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2165…2179
- **Satisfies:** FR-TXN-051, FR-TXN-052, FR-EVT-014
- **Notes:** —

### STD-TXN-051 — Pre-commit external calls
- **Clause:** A connector call made before commit MUST be journaled. If its connector declares a compensation operation and the process then aborts or fails, the implementation MUST invoke the compensation (at least once, idempotently) and record it.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2180…2189
- **Satisfies:** FR-TXN-053
- **Notes:** —

## 6. Deferred commits (approvals)

### STD-TXN-060 — Deferred commit semantics `[Profile: Governance]`
- **Clause:** When a decision returns `require: approval`, the implementation MUST store the process's intended changes as a pending change (`DAT-PendingChange`) with their bases and the preview hash, and end the process `committed` without effect on the target. Final approval MUST start a new process that re-validates the changes against the current branch state:
  - if the bases are unchanged and validation passes, the changes are committed with the approvals referenced;
  - otherwise the pending change becomes `invalidated`, and approvers are notified (FR-FLOW-045).

  Rejection or expiry discards the pending change.
- **Grammar / Schema:** `DAT-PendingChange`
- **Conformance vectors:** VER-CONF-2190…2214
- **Satisfies:** FR-FLOW-042, FR-FLOW-044
- **Notes:** Since 1.0-rc8 (DEC-091):
  - **Request.** A port call made with `defer` stores a pending change when its decisions require steps; without `defer` it fails with `RULE.DECISION_REQUIRES` as before.
  - **Identity.** The pending change is identified by the process that requested it, and `preview` is the content ID of `{branch, changes}`. The requirements are those of STD-LGS-041. `expires_at` is the request time plus the shortest `expires_in`.
  - **States.** At each read or approval, an expired pending change becomes `expired`, and one whose bases are no longer the heads becomes `invalidated`, listing the changed objects with their approved and current bodies (`changed_since`) and keeping the voided approvals.
  - **Application.** The final approval commits the stored changes in a new process with the requester as initiator. A failure of that commit invalidates the pending change with the error code (`invalid_reason`).
  - **Ending.** A rejection ends it as `rejected`; a decision on a pending change that is no longer `pending` returns its state.

```yaml
DAT-PendingChange:
  target_branch: { type: text, required: true, description: "branch name" }
  changes: { type: list, required: true, description: "DAT-Change items with bases (versions stored as chunks)" }
  preview: { type: text, required: true, description: "content-id of validation and diff at request time" }
  requirements: { type: list, required: true, description: "approval requirements from decisions" }
  approvals: { type: list, required: false, description: "DAT-Approval items" }
  state: { type: enum, required: true, description: "pending | applied | rejected | expired | invalidated" }
  expires_at: { type: instant, required: false, description: "expiry" }
DAT-Approval:
  approver: { type: text, required: true, description: "principal DID" }
  on_behalf_of: { type: text, required: false, description: "delegator DID" }
  decision: { type: enum, required: true, description: "approve | reject" }
  preview: { type: text, required: true, description: "content-id of the preview approved" }
  at: { type: instant, required: true, description: "time" }
  auth_level: { type: text, required: true, description: "authentication assurance at decision" }
  comment: { type: text, required: false, description: "comment" }
  signature: { type: composite, required: false, description: "DAT-Signature when strong approval is required" }
```

## 7. Bulk processes and sagas

### STD-TXN-070 — Bulk processes
- **Clause:** A bulk process MUST divide its work into child processes of at most the configured change count (default 100,000), linked by `bulk`. In `all_or_nothing` mode, children commit to a staging branch, and completion merges the staging branch once. Resumption MUST continue after the last committed child. Children are `job` processes triggered by the bulk process (`parent_process` and `bulk`). In `all_or_nothing` mode the staging branch is created from the target at the start. A failing child stops the bulk process in state `failed`; resumption divides the work again and starts with the first child that is not committed (DEC-041).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2215…2224
- **Satisfies:** FR-TXN-015, FR-MIG-032
- **Notes:** —

### STD-TXN-080 — Sagas
- **Clause:** A saga MUST be a Definition object that lists steps (a process spec and an optional compensation) and the saga state. Each step is a process linked by `saga`. On step failure or timeout, compensations of completed steps MUST run in reverse order, unless the saga declares forward recovery. Saga state transitions MUST be committed, so that execution resumes after crashes without repeating completed steps.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2225…2244
- **Satisfies:** FR-TXN-061, FR-TXN-062, FR-TXN-063
- **Notes:** —
