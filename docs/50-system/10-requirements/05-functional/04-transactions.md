---
id: UBS-REQ-05-04
title: Functional Requirements — TXN (Processes, Atomicity, Concurrency, Provenance)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-02]
---

# Functional Requirements — TXN

## CAP-TXN-01 — Process and atomic commit

### FR-TXN-011 — Every mutation runs in a process
- **Statement:** Every state change MUST execute inside a process. A process MUST record:
  - `process_id`, `kind` (action, schedule, job, event handler, merge, import, agent run, kernel service), `initiator` (principal and on-behalf-of) and `channel`;
  - the triggering reference and the start and end times;
  - the status (`running`, `committed`, `aborted`, `failed`) and the resulting commits.
- **Rationale:** The process is the unit of audit, atomicity, revert and replay (L40:SPEC-15).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor, PER-KernelEngineer
- **Acceptance:**
  1. Given any commit, when its process is read, then every listed member is present.
- **Verification:** CONF
- **Origin:** L40:SPEC-15, EXT-TRI

### FR-TXN-012 — Unit of work and single flush
- **Statement:** Within a process, changes MUST accumulate in a unit of work that is invisible to other processes. At completion they MUST be flushed in one storage transaction, together with the commit record, rule binding, outbox and projections in `sync` mode.
- **Rationale:** All-or-nothing business changes.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a process in progress, when another reader queries, then none of its changes are visible until the flush completes.
- **Verification:** CONF, SIM
- **Origin:** L40:SPEC-15

### FR-TXN-013 — Crash atomicity
- **Statement:** A crash or power loss at any point MUST leave either the complete commit (with all its records) or no trace of it in authoritative state. Recovery MUST need no manual repair.
- **Rationale:** Integrity under failure.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given crash injection at every write step of the flush in a fault matrix, when recovered, then invariants hold, and no partial commit exists.
- **Verification:** FAULT, SIM, FORM
- **Origin:** L40:SPEC-15

### FR-TXN-014 — Nested invocations share the unit of work
- **Statement:** Ports, logic and rules invoked synchronously within a process MUST share its unit of work, and they MUST observe the process's own uncommitted changes. Starting a separate process MUST be explicit (an asynchronous job or a new process instruction).
- **Rationale:** Predictable composition of logic.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given action A that creates an object and calls action B that reads it, when run, then B sees the object, and one commit results.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

### FR-TXN-015 — Bulk processes
- **Statement:** A process MUST respect a maximum change count per commit, configurable per profile with a standard default of at least 100,000. Larger operations MUST run as a bulk process: an ordered series of commits linked by a parent bulk ID, resumable, with progress and an all-or-nothing option that stages the commits on a branch and merges once.
- **Rationale:** Migrations and agent runs can exceed a single transaction's practical size.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a 2,000,000-object import with all-or-nothing, when interrupted at 60%, then `main` is unchanged, and the bulk process resumes on its staging branch.
- **Verification:** SCN, FAULT
- **Origin:** NEW — scale of migration and agent workloads.

## CAP-TXN-02 — Optimistic concurrency

### FR-TXN-021 — Every write names its base
- **Statement:** Every change to an existing Definition object MUST name the base version it was derived from. If the object's current version on the branch differs, the commit MUST fail with `TXN.HEAD_MOVED`, stating the current version and the changed fields.
- **Rationale:** No lost updates (SCN-005). This is compare-and-set on object heads (EXT-RFC 3.1.1).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser, PER-LogicDeveloper
- **Acceptance:**
  1. Given SCN-005, when run with 100 concurrent writers, then exactly one write per base version succeeds.
- **Verification:** SIM, PROP, CONF
- **Origin:** EXT-RFC, L40:SPEC-15

### FR-TXN-022 — Object-level conflict scope
- **Statement:** A commit MUST NOT fail only because the branch head moved. It MUST fail only if an object it writes changed since its base, or if a declared read-set check fails (FR-TXN-023).
- **Rationale:** Unrelated concurrent work must not serialise throughput.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given two processes writing disjoint objects concurrently, when both commit, then both succeed.
- **Verification:** SIM, BENCH
- **Origin:** NEW — resolves a throughput limit of branch-level CAS.

### FR-TXN-023 — Serializable read-set validation
- **Statement:** A process MAY declare serializable mode. The DVM MUST then validate at commit that no object read by the process changed since it was read, and fail with `TXN.READ_SET_CHANGED` otherwise.
- **Rationale:** Calculations that depend on several inputs need protection from write skew.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a serializable process reading a limit and writing usage, when the limit changes concurrently, then the process fails with `TXN.READ_SET_CHANGED`.
- **Verification:** SIM, PROP
- **Origin:** NEW

### FR-TXN-024 — All conflicts reported at once
- **Statement:** When a commit fails for concurrency, the error MUST list every conflicting object, not only the first.
- **Rationale:** Clients resolve in one round trip.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given 3 conflicting objects, when the commit fails, then the error lists 3 entries.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

### FR-TXN-025 — Automatic retry for re-executable processes
- **Statement:** Processes whose logic is declared `retryable` (deterministic and free of external effects before commit) MUST be retried automatically by the kernel on `TXN.HEAD_MOVED`, up to a configured count, with backoff.
- **Rationale:** Most concurrency conflicts in server-side logic are resolved by re-running the logic.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a retryable counter increment under contention, when 50 run concurrently, then all 50 succeed within the retry budget.
- **Verification:** SIM, CONF
- **Origin:** NEW

## CAP-TXN-03 — Idempotency

### FR-TXN-031 — Idempotency keys
- **Statement:** Every mutating request MUST accept an idempotency key. A repeated request with the same key and the same payload hash MUST return the original result without a new effect. The same key with a different payload MUST fail with `TXN.IDEMPOTENCY_MISMATCH`.
- **Rationale:** Networks retry, so business effects must not repeat.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a subscription submitted twice with one key, when both complete, then one order exists, and both responses are identical.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-15

### FR-TXN-032 — Idempotency retention
- **Statement:** Idempotency records MUST be retained for at least 7 days, configurable per VAE, and MUST survive node restarts and failover.
- **Rationale:** Late retries from queues and mobile devices.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a retry 6 days later after a failover, when received, then it is deduplicated.
- **Verification:** FAULT
- **Origin:** NEW
- **Phase note:** PH-2 step 0b (DEC-087): moved from PH-1 with the retention sweep of DSN-DVM-531 and its tests (retention, failover); PH-1 keeps idempotency records in the store without a sweep.

### FR-TXN-033 — Idempotent handlers
- **Statement:** Event handlers and jobs MUST receive a stable delivery ID and MUST use it as the idempotency key of the process they start.
- **Rationale:** At-least-once delivery requires idempotent effects.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an event delivered twice, when handled, then one handler process commits.
- **Verification:** CONF, SIM
- **Origin:** L40:SPEC-20

## CAP-TXN-04 — Provenance and causality

### FR-TXN-041 — Trigger lineage
- **Statement:** Every process MUST record what triggered it: a user action, an event (event ID and emitting process), a schedule firing, a job, a merge or an external request. The resulting links MUST form a queryable causality graph.
- **Rationale:** Cause and effect across asynchronous flows (EXT-TRI Book II, "the nervous system").
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor, PER-LogicDeveloper
- **Acceptance:**
  1. Given user action → event → handler → job → commit, when the causality of the final commit is requested, then the whole chain is returned.
- **Verification:** CONF
- **Origin:** EXT-TRI, L40:SPEC-15

### FR-TXN-042 — Input dependencies of derived writes
- **Statement:** For objects written by logic, the process MUST record the input object versions read (the dependency set), bounded by a configurable size with overflow summarised per class and version range.
- **Rationale:** Lineage and retroactive impact analysis (FR-TIME-042).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Acceptance:**
  1. Given a NAV computation reading 800 prices, when inspected, then the dependency set lists 800 price versions.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

### FR-TXN-043 — Causality queries
- **Statement:** The platform MUST answer ancestor and descendant queries on the causality graph with depth and time limits.
- **Rationale:** Investigation and revert of dependants (FR-VER-075).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a process, when descendants to depth 5 are requested, then all triggered processes up to depth 5 are returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

### FR-TXN-044 — Channel and client attribution
- **Statement:** Every process MUST record its channel (`workspace`, `studio`, `api`, `sdk`, `agent`, `integration`, `schedule`, `kernel`) and the client application and version.
- **Rationale:** Audit and incident analysis.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a commit from the mobile client, when inspected, then channel `workspace` and client `ubos-mobile/<version>` are recorded.
- **Verification:** CONF
- **Origin:** NEW

## CAP-TXN-05 — Post-commit effects

### FR-TXN-051 — Outbox
- **Statement:** Events, job enqueues, notifications and non-transactional connector calls requested during a process MUST be stored in the commit's outbox and released only after the commit succeeds.
- **Rationale:** No side effect for a change that never happened.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a process that emits an event and then aborts, when observed, then no subscriber receives the event.
- **Verification:** CONF, FAULT
- **Origin:** L40:SPEC-15

### FR-TXN-052 — At-least-once release
- **Statement:** Released effects MUST be delivered at least once, with a stable effect ID for deduplication, surviving crashes and runtime-store loss.
- **Rationale:** Reliable integration.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a crash after commit and before release, when recovered, then the outbox releases the pending effects.
- **Verification:** FAULT, SIM
- **Origin:** L40:SPEC-20

### FR-TXN-053 — Pre-commit external interactions are recorded and compensable
- **Statement:** A process that must call an external system before commit (for example to reserve or validate) MUST do so through a connector that records request and response. It MUST declare a compensation action that the platform triggers if the process then aborts.
- **Rationale:** Consistency with systems outside the commit.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, BRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a reservation call followed by an abort, when the process ends, then the compensation call is issued and recorded.
- **Verification:** CONF, FAULT
- **Origin:** NEW

## CAP-TXN-06 — Long-running processes

### FR-TXN-061 — Sagas
- **Statement:** The platform MUST support long-running processes (sagas). A saga is a declared sequence of steps, each executed as its own process and commit, with a compensation per step. Its state MUST be persisted so that it resumes after crashes.
- **Rationale:** Multi-day business processes cannot be single transactions.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a 4-step saga with a crash after step 2, when the node restarts, then step 3 runs next. Nothing is repeated, and nothing is skipped.
- **Verification:** FAULT, SIM
- **Origin:** L40:SPEC-19

### FR-TXN-062 — Saga linkage
- **Statement:** All processes of a saga MUST carry the saga ID. The saga object MUST list its steps, their processes and their status.
- **Rationale:** Visibility and audit of multi-step work.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a completed saga, when inspected, then each step links to its commit.
- **Verification:** CONF
- **Origin:** L40:SPEC-19

### FR-TXN-063 — Timeouts and compensation
- **Statement:** A saga step MAY declare a timeout. When a step fails or times out, the platform MUST run the compensations of completed steps in reverse order, unless the saga declares forward recovery.
- **Rationale:** Deterministic failure handling.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a failure at step 3, when handled, then compensations 2 and 1 run in that order and are recorded.
- **Verification:** CONF, SIM
- **Origin:** L40:SPEC-19

### FR-TXN-064 — Saga monitoring
- **Statement:** Operators and owners MUST be able to list running, stuck and failed sagas, and retry, skip (with reason and permission) or cancel a step.
- **Rationale:** Operational control of long-running work.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, WSP, CTL
- **Personas:** PER-PlatformOperator, PER-FundOperationsManager
- **Acceptance:**
  1. Given a stuck step, when an authorised user retries it, then the retry is a new attempt linked to the saga.
- **Verification:** SCN
- **Origin:** NEW
