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
- **Notes:** Since 1.0-rc8 (DEC-094):
  - **When it runs.** When a process with successful `now` calls fails (including an attempt that is retried), each call's compensation operation is invoked with the call's request, last call first.
  - **Records.** Each compensation is made under the idempotency key `comp:<process>:<seq>` and recorded in the process record (`compensations`, with `outcome`) and in the exchanges.
  - **At least once.** A compensation that cannot be made now is released as an effect `connector.compensation` for retry.
  - **Exceptions.** An operation declared `compensation: none` is recorded as such. Compensations are not rate-limited.

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
  - **Reopening a period.** Since 1.0-rc10 (DEC-105), `reopen_period` given `defer` and a `process` with its `process_id` stores the reopening as a pending change when the decisions of the period's port `reopen` require steps (STD-KIND-025). The final approval reopens the period with the reason.

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
  delegation: { type: text, required: false, description: "the Delegation object acted through (since 1.0-rc10)" }
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
- **Grammar / Schema:** `DAT-Saga`
- **Conformance vectors:** VER-CONF-2225…2244
- **Satisfies:** FR-TXN-061, FR-TXN-062, FR-TXN-063
- **Notes:** Since 1.0-rc8 (DEC-092):
  - **Objects.** A saga is an object of the genesis class `Saga`. Its author writes `name`, `steps`, `recovery` and `max_attempts`. The state members (`status`, `completed`, `compensated`, `current`, `error`, `log`) are written only by the saga's execution; a change that writes them fails with `FLOW.STATE_DIRECT_WRITE`.
  - **Steps.** A step's process and compensation are port calls `{target, port, params}`. Each runs as its own process and commit, and its process record carries `saga` and `saga_step` (`step`, `attempt`, `phase`).
  - **States.** A saga moves from `pending` to `running`, then ends `completed`; on a failure under backward recovery it moves to `compensating` and ends `compensated`. A failed compensation, or forward recovery that has used `max_attempts` attempts, ends it `failed`. A step without a compensation counts as compensated.
  - **Timeouts.** A step whose process ends by a limit termination (`LOGIC.LIMIT_EXCEEDED`) has timed out.
  - **Commits.** Every state transition is one commit, before and after each step process. On resumption, a step whose process committed is not run again; its recorded outcome is used. The adapter operation `run_saga` runs or resumes a saga; with `interrupt_after: n` it stops after n commits, as a crash would.

```yaml
DAT-Saga:
  name: { type: text, required: true, description: "name" }
  steps: { type: list, required: true, description: "{name, process, compensation, timeout, on_timeout} items; process and compensation are {target, port, params}; timeout is a duration and on_timeout recover | escalate (since 1.0-rc12)" }
  recovery: { type: enum, required: false, description: "backward (default) | forward" }
  max_attempts: { type: integer, required: false, description: "attempts per step under forward recovery (default 1)" }
  operators: { type: list, required: false, description: "principals who may retry, skip or cancel (since 1.0-rc12)" }
  status: { type: enum, required: false, description: "pending | running | stuck | compensating | completed | compensated | failed (written by the execution; stuck since 1.0-rc12)" }
  completed: { type: list, required: false, description: "names of the completed steps (written by the execution)" }
  compensated: { type: list, required: false, description: "names of the compensated steps (written by the execution)" }
  current: { type: composite, required: false, description: "{step, attempt, phase} of the step in progress, with started_at when the step declares a timeout (written by the execution)" }
  skipped: { type: list, required: false, description: "{step, principal, reason} of the skipped steps (written by the execution, since 1.0-rc12)" }
  error: { type: composite, required: false, description: "the failed step and error code (written by the execution)" }
```

### STD-TXN-081 — Saga timeouts, monitoring and intervention
- **Clause:** A saga step MAY declare a `timeout`. A step whose process has not committed when the clock reaches the start of its attempt plus the timeout MUST be treated as timed out (`FLOW.TIMEOUT`). A step that times out, by its timeout or by a limit termination (STD-TXN-080), MUST follow its `on_timeout`:
  - `recover` (the default) applies the saga's recovery as for a failure;
  - `escalate` stops the saga in the state `stuck`, keeping the step in progress.

  Sagas MUST be queryable by state, by the age of the step in progress and by the failing step. A principal listed in the saga's `operators` MUST be able to act on a `stuck` saga, or on a saga `failed` in a forward step:
  - retry the step as a new attempt;
  - skip it with a reason;
  - cancel the saga, which compensates its completed steps.

  Other principals MUST fail with `IAM.DENIED`. Every action MUST be recorded in the saga's `log` and committed like the saga's other state transitions.
- **Grammar / Schema:** `DAT-Saga`
- **Conformance vectors:** VER-CONF-2800…2819
- **Satisfies:** FR-TXN-063, FR-TXN-064
- **Notes:** Since 1.0-rc12 (DEC-119):
  - **Start of an attempt.** When a forward attempt of a step with a `timeout` begins, `current` records `started_at`. `run_saga` resuming such a step whose process has not committed, at or after `started_at` plus the timeout, records the outcome `timeout` with the code `FLOW.TIMEOUT` instead of running the step. Compensations have no timeout.
  - **Escalation.** A `stuck` saga keeps `current` and records `error` `{step, code}`. Its commit releases an effect `SagaStuck` with `{saga, step, code}` to the outbox (STD-TXN-050). `run_saga` on a stuck saga returns its state without commits.
  - **Query.** The adapter operation `sagas {branch, status, older_than, step}` returns `{sagas}`, each `{saga_id, name, status, current, error}`, in object-ID order:
    - `status` keeps the sagas in that state;
    - `older_than`, a duration, keeps the sagas whose step in progress started at or before the clock minus it;
    - `step` keeps the sagas whose step in progress or failed step has that name.
  - **Actions.** The adapter operation `saga_action {branch, saga, action, principal, reason}` acts and then continues the saga as `run_saga` does, returning its result.
    - `retry` sets the saga `running` with the step's next attempt.
    - `skip` adds the step to `completed` and to `skipped` (`{step, principal, reason}`) and sets the saga `running`; it needs a `reason` (`MODEL.DECLARATION_INCOMPLETE`).
    - `cancel` sets the saga `compensating`.
    - An action on a saga in another state fails with `FLOW.TRANSITION_NOT_ALLOWED`.
    - Each action adds `{step, phase: "operator", action, principal, reason}` to the `log`.
  - **Compatibility.** A saga without timeouts, `on_timeout` or `operators` behaves as in 1.0-rc8.

## 8. Workflows

### STD-TXN-090 — Workflow definitions and instances
- **Clause:** A workflow definition MUST be a versioned Definition object (`DAT-WorkflowDefinition`) with typed variables, a start step and steps of the types `action`, `wait_event`, `timer`, `human_task`, `decision`, `parallel` with `join`, `sub_workflow` and `end`. A workflow instance MUST be a System-kind object, pinned to the definition version current when it started, whose state advances only through the workflow operations. Every step MUST be one committed state change, and an action's port call MUST be idempotent by `(instance, step, visit, attempt)`. A `join` MUST fire once, when all branches of its `parallel` step have arrived.
- **Grammar / Schema:** `DAT-WorkflowDefinition`
- **Conformance vectors:** VER-CONF-2245…2264
- **Satisfies:** FR-FLOW-061, FR-FLOW-062
- **Notes:** Since 1.0-rc8 (DEC-092):
  - **Expressions.** Conditions, parameters, correlation keys and sub-workflow inputs are L1 expressions over the variables as `params`.
  - **Steps.**
    - `action` calls a port as a process linked by `workflow` (`instance`, `step`, `visit`, `attempt`) and stores the result in `result`. A failure retries the step up to `retries` times, then follows `on_error`, or else the instance ends `failed`.
    - `decision` follows the first case whose `when` is true, or the branch of a decision table's `output` value; otherwise `default`.
    - `parallel` starts a token per branch; `join` continues once, after the last branch arrives.
    - `sub_workflow` starts a child instance with `inputs` and waits for it to end; `outputs` copy child variables into the parent. A child that fails fails the parent.
    - `end` ends the instance `completed` or `failed`.
  - **Steps of an instance.** A visit counts how often an instance entered a step, so steps in loops keep distinct identities.
  - **Instances.** The adapter operation `start_workflow` creates an instance and advances it until every token waits or it ends. A change that writes a `WorkflowInstance` fails with `LOGIC.FORBIDDEN_TARGET`.

```yaml
DAT-WorkflowDefinition:
  name: { type: text, required: true, description: "name" }
  variables: { type: list, required: false, description: "{name, type, constraints, default} items" }
  start: { type: text, required: true, description: "the first step" }
  steps: { type: composite, required: true, description: "step name -> DAT-WorkflowStep" }
DAT-WorkflowStep:
  type: { type: enum, required: true, description: "action | wait_event | timer | human_task | decision | parallel | join | sub_workflow | end" }
  next: { type: text, required: false, description: "the following step (all types except decision, parallel and end)" }
  target: { type: text, required: false, description: "action: the object URI" }
  port: { type: text, required: false, description: "action: the port" }
  params: { type: composite, required: false, description: "action: parameter -> L1 expression" }
  retries: { type: integer, required: false, description: "action: retries after a failure (default 0)" }
  on_error: { type: text, required: false, description: "action: the step after the last failed attempt" }
  result: { type: text, required: false, description: "the variable that receives the result, event payload or task data" }
  event_type: { type: text, required: false, description: "wait_event: the event type" }
  correlation: { type: text, required: false, description: "wait_event: L1 expression of the correlation key" }
  buffer: { type: duration, required: false, description: "wait_event: how long an earlier event stays deliverable" }
  after: { type: duration, required: false, description: "timer: the delay from entering the step" }
  at: { type: text, required: false, description: "timer: L1 expression of the due instant or date (since 1.0-rc12)" }
  calendars: { type: composite, required: false, description: "timer with at: calendar name -> {weekend, holidays} (since 1.0-rc12)" }
  timeout: { type: duration, required: false, description: "wait_event, human_task: the longest wait (since 1.0-rc12)" }
  on_timeout: { type: text, required: false, description: "wait_event, human_task: the step after a timeout (since 1.0-rc12)" }
  queue: { type: text, required: false, description: "human_task: the work queue of its Task (since 1.0-rc12)" }
  task_type: { type: text, required: false, description: "human_task with a queue: the task type (default: the step name; since 1.0-rc12)" }
  subject: { type: text, required: false, description: "human_task with a queue: the task subject (since 1.0-rc12)" }
  priority: { type: integer, required: false, description: "human_task with a queue: the task priority (since 1.0-rc12)" }
  data: { type: list, required: false, description: "human_task with a queue: the variables copied into the task data (since 1.0-rc12)" }
  on_cancel: { type: text, required: false, description: "human_task with a queue: the step after the task is cancelled (since 1.0-rc12)" }
  defer: { type: boolean, required: false, description: "action: the port call may store a pending change (since 1.0-rc12)" }
  on_reject: { type: text, required: false, description: "action with defer: the step after a rejected, expired or invalidated change (since 1.0-rc12)" }
  assignee: { type: text, required: false, description: "human_task: the principal who completes it" }
  cases: { type: list, required: false, description: "decision: {when, next} items" }
  default: { type: text, required: false, description: "decision: the step when no case applies" }
  table: { type: text, required: false, description: "decision: a decision table (object ID or URI)" }
  inputs: { type: composite, required: false, description: "decision with a table, sub_workflow: name -> L1 expression" }
  output: { type: text, required: false, description: "decision with a table: the output that selects the branch" }
  branches: { type: list, required: false, description: "parallel: the first steps of the branches; decision with a table: output value -> step" }
  join: { type: text, required: false, description: "parallel: the join step" }
  definition: { type: text, required: false, description: "sub_workflow: the child definition (object ID or URI)" }
  outputs: { type: composite, required: false, description: "sub_workflow: parent variable -> child variable" }
  outcome: { type: enum, required: false, description: "end: completed (default) | failed" }
```

### STD-TXN-091 — Workflow events, timers and tasks
- **Clause:** An event MUST be matched to waiting steps by `(event type, correlation key)` and MUST wake at most one of them: the one that has waited longest. An event that matches no waiting step MUST be buffered and delivered to a step that starts waiting within that step's declared `buffer` window. A timer MUST fire when the clock reaches its due time. A human task MUST be completed only by its assignee, and completing the same `(instance, step, visit)` again MUST return the first result. An event delivered again under its event ID MUST return the first result.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2265…2284
- **Satisfies:** FR-FLOW-064
- **Notes:** Since 1.0-rc8 (DEC-092):
  - **Operations.**
    - `deliver_event {event_type, key, payload, event_id}` returns the woken step or `buffered`.
    - `complete_task {instance, step, principal, data}` continues a human task. A step that does not wait fails with `FLOW.TRANSITION_NOT_ALLOWED`, another principal with `IAM.DENIED`, and other data under the same identity with `TXN.IDEMPOTENCY_MISMATCH`.
    - Timers fire when `set_now` advances the clock, in the order of their due times.
  - **Order of waiting.** Steps that wait for the same type and key are ordered by the sequence number of the commit that made them wait, then by instance and token.

### STD-TXN-092 — Workflow migration
- **Clause:** A running instance MUST continue on its pinned definition version after the definition changes. Moving it to another version MUST be an explicit migration that maps every active step (every waiting step and every join not yet fired) to a step of the same type; a migration that leaves a step unmapped MUST fail with `MODEL.MIGRATION_REQUIRED`. The migration MUST be recorded on the instance.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2285…2294
- **Satisfies:** FR-FLOW-062
- **Notes:** Since 1.0-rc8 (DEC-092): the adapter operation `migrate_workflow {instance, version, step_map}` migrates to `version` (default: the definition's head). A step mapped to a missing step or a step of another type fails with `MODEL.INVALID_VALUE`; an instance that has ended fails with `MODEL.CONSTRAINT_VIOLATED`. `migrations` records the versions and the step map.

### STD-TXN-093 — Workflow tasks and approvals `[Profile: Governance]`
- **Clause:** A `human_task` step that names a `queue` MUST, when entered, create a `Task` (STD-LGS-061) linked to the instance and the step. The task is assigned by the queue's rules and gets its due time from the queue's due rule (STD-LGS-062, -063). The step MUST continue when that task is completed, with the task's outcome as the step's result. When the task is cancelled, the wait MUST continue along the step's `on_cancel`, or else the instance fails.

  An `action` step with `defer: true` makes its port call with `defer` (STD-TXN-060). When the call stores a pending change, the step MUST wait for its decision:
  - an applied change continues to `next`, with `{pending_change_id, cseq}` as the step's result;
  - a rejected, expired or invalidated change continues to `on_reject`, or else the instance fails.
- **Grammar / Schema:** `DAT-WorkflowDefinition`
- **Conformance vectors:** VER-CONF-2820…2849
- **Satisfies:** FR-FLOW-062, FR-FLOW-042, FR-FLOW-051
- **Notes:** Since 1.0-rc12 (DEC-119):
  - **The task.** The task ID is the UUID derived from `ubos.workflow.task` with the instance, token, step and visit, as child instances are derived (STD-TXN-090). The task's members:
    - `type` is the step's `task_type`, or the step name;
    - `subject` and `priority` are taken from the step (`priority` defaults to 0);
    - `assignee` is `{principal}` when the step names an `assignee`;
    - `data` maps the variables the step lists in `data` to their values;
    - `created_by` is the instance's initiator;
    - `workflow` is `{instance, step, visit}`.

    The waiting step shows `task_id`.
  - **Completion.** Completing the task with the `task` operation (STD-LGS-061) continues the instance in the same operation. `complete_task` on a step that waits for a task fails with `FLOW.TRANSITION_NOT_ALLOWED` and `details` `{task_id}`.
  - **Cancellation.** A cancelled task logs `cancelled` on the instance and continues to `on_cancel`. Without `on_cancel`, the instance ends `failed` with `error` `{step, task_id, state: "cancelled"}`.
  - **Approvals.** The waiting step shows `pending_change`.
    - The approval that applies or rejects the change continues the instance in the same operation.
    - Expiry is noticed when the clock advances, and invalidation at the next decision on the change.
    - Without `on_reject`, the instance ends `failed` with `error` `{step, pending_change, state}`.
    - A port call that commits without approval continues as an ordinary action.
  - **Declarations.** `queue`, `task_type` and `subject` are text, `priority` is an integer, `data` lists variables, `defer` is a boolean, and `on_cancel` and `on_reject` name steps (`MODEL.INVALID_VALUE`). An unknown queue fails when the step is entered: the instance ends `failed` with `error` `{step, code: "IAM.NOT_FOUND"}`.

### STD-TXN-094 — Workflow timers, business calendars and timeouts
- **Clause:** A `timer` step MUST name exactly one of:
  - `after`, a duration from entering the step;
  - `at`, an L1 expression of the due instant or date.

  The `at` expression is evaluated when the step is entered, over the variables (`params`), with the entry instant as the context's `now`, so that `now()` and `today()` give the entry instant and date. The calendar functions of STD-EXPR-005 use the business calendars the step declares in `calendars`. A `wait_event` or `human_task` step MAY declare `timeout` with `on_timeout`. When the clock reaches the entry plus the timeout before the wait ends, the token MUST continue to `on_timeout`, and a task the step created MUST be cancelled. Due times MUST be resolved when the step is entered. When a migration (STD-TXN-092) moves a waiting token, they MUST be recomputed from the entry instant under the new version.
- **Grammar / Schema:** `DAT-WorkflowDefinition`
- **Conformance vectors:** VER-CONF-2850…2879
- **Satisfies:** FR-FLOW-063, FR-FLOW-082
- **Notes:** Since 1.0-rc12 (DEC-119):
  - **Values of `at`.** An instant is the due time. A date is due at 00:00 UTC of that date.
  - **Errors.** Another type, or an error when the expression is evaluated, ends the instance `failed` with `error` `{step, code}`.
  - **Declarations.** A timer with both or neither of `after` and `at`, or a `timeout` without `on_timeout`, fails with `MODEL.DECLARATION_INCOMPLETE`. `calendars` map names to `{weekend, holidays}`, as the calendars of work queues do (STD-LGS-063).
  - **Firing.** Timers and timeouts fire in the order of their due times when the clock advances (STD-TXN-091).
    - A timeout logs `timeout` on the instance.
    - A task the step created is cancelled with the reason `timeout` by the instance's initiator.
    - An event or a completion that arrives after the timeout finds no waiting step.
  - **Waiting steps** show `due` for a timer and `timeout_at` for a timeout. A token that waits for a step with `at` or `timeout` also records the entry instant.
  - **Migration.** The due time of a timer with `after` keeps its entry instant.
