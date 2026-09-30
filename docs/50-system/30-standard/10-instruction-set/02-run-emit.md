---
id: UBS-STD-10-02
title: BPA Standard — Instructions run.* and emit.*
status: draft
phase: PH-0
depends_on: [UBS-STD-10]
---

# Instructions `run.*` and `emit.*`

## 1. `run.*`

### STD-ISA-050 — `run.port`
- **Clause:** Invokes a port on an object or class, synchronously within the current process (shared unit of work, STD-TXN-002).

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `port: text`, `params: map` |
  | Result | the port's result type |
  | Errors | the port's declared errors, plus `MODEL.PORT_NOT_IMPLEMENTED`, `FLOW.GUARD_FAILED`, `RULE.DECISION_DENIED`, `RULE.DECISION_REQUIRES`, `IAM.DENIED` |
  | Determinism | as the port's implementation |
  | Cost | 20 plus the callee's consumption (shared budget) |
  | Authorization | the port's `permission` |

  Decisions returning `require` MUST raise `RULE.DECISION_REQUIRES` with the list of required steps. The caller may catch it only to report. It MUST NOT be converted into success.

  A port implementation names `<asset>#<function>`; the function sees the target as `this` and its identifier as `self_id` (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3400…3429
- **Satisfies:** FR-MODEL-031, FR-FLOW-022, FR-RULE-051
- **Notes:** Nesting depth is limited by the profile (default 32).

### STD-ISA-051 — `run.super`
- **Clause:** Invokes the next implementation of the current port in the linearization (STD-CLS-020), with the same or refined parameters.

  | Aspect | Value |
  |---|---|
  | Errors | `MODEL.PORT_NOT_IMPLEMENTED` |
  | Cost | 10 plus the callee |
  | Authorization | none additional |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3430…3439
- **Satisfies:** FR-MODEL-033
- **Notes:** —

### STD-ISA-052 — `run.logic`
- **Clause:** Calls a function exported by a verified logic asset (a library), pinned to the version recorded in the caller's imports.

  | Aspect | Value |
  |---|---|
  | Params | `asset: uri`, `function: text`, `args: list` |
  | Errors | `LOGIC.UNVERIFIED_ASSET`, `LOGIC.ASSET_QUARANTINED`, the callee's errors |
  | Determinism | as the callee |
  | Cost | 10 plus the callee |
  | Authorization | `execute` on the asset |

  Imports are `{name, version}` items with the version hash of the library (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3440…3449
- **Satisfies:** FR-LOGIC-043, FR-LOGIC-082
- **Notes:** —

### STD-ISA-053 — `run.decision`
- **Clause:** Evaluates a decision table (FR-RULE-041) with typed inputs and returns its outputs under the table's hit policy. The table version is recorded in the rule binding.

  | Aspect | Value |
  |---|---|
  | Params | `table: uri`, `inputs: map`, `asof: date?` (default: process `asof`) |
  | Errors | `RULE.NO_MATCH` (policy `unique` or `first` without a match), `RULE.MULTIPLE_MATCH` (`unique`) |
  | Determinism | `snapshot` |
  | Cost | 10 + 1 per row evaluated |
  | Authorization | `evaluate` on the table |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3450…3469
- **Satisfies:** FR-RULE-041, FR-RULE-043
- **Notes:** —

### STD-ISA-054 — `run.validate`
- **Clause:** Evaluates validations, invariants and commit-point declarations for proposed changes without committing, and returns all findings (the validation preview).

  | Aspect | Value |
  |---|---|
  | Params | `changes: list<change>` or `target: uri` (for current unit-of-work changes) |
  | Result | `{violations: list<explanation>, warnings: list<explanation>}` |
  | Determinism | `snapshot` |
  | Cost | 20 plus the evaluation work |
  | Authorization | `read` on the targets |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3470…3479
- **Satisfies:** FR-RULE-035, FR-AI-045
- **Notes:** —

### STD-ISA-055 — `run.async`
- **Clause:** Requests a new process (a job) to run a port or logic function after commit, with the current process as its parent. The job is created through the outbox.

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `port: text` or `asset/function`, `params: map`, `not_before: instant?`, `priority: integer?`, `idempotency_key: text?` |
  | Result | `{job_id}` |
  | Determinism | `deferred` |
  | Cost | 20 |
  | Authorization | as for the target port or function |

  The job request is an effect of type `Job` whose payload holds `job_id`, `port`, `params`, `not_before` and `parent_process` (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3480…3489
- **Satisfies:** FR-TXN-014, FR-EVT-031
- **Notes:** —

## 2. `emit.*`

### STD-ISA-070 — `emit.event`
- **Clause:** Emits a declared business event after commit.

  | Aspect | Value |
  |---|---|
  | Params | `type: text` (declared on the class), `subject: uri`, `payload: map` (validated against the declared schema) |
  | Result | `{event_id}` |
  | Errors | `EVT.UNKNOWN_EVENT_TYPE`, `MODEL.CONSTRAINT_VIOLATED` (payload) |
  | Determinism | `deferred` |
  | Cost | 20 |
  | Authorization | `emit` on the subject's class |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3500…3514
- **Satisfies:** FR-EVT-011, FR-EVT-013
- **Notes:** —

### STD-ISA-071 — `emit.notify`
- **Clause:** Requests a notification from a template to recipients (principals, roles or relations) after commit. Rendering happens per recipient with that recipient's permissions (FR-NTF-012).

  | Aspect | Value |
  |---|---|
  | Params | `template: uri`, `subject: uri`, `recipients: list`, `channels: list?`, `priority: enum?`, `action: port-ref?` (for Action Messages) |
  | Result | `{notification_id}` |
  | Determinism | `deferred` |
  | Cost | 20 + 2 per recipient |
  | Authorization | `notify` on the subject |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3515…3524
- **Satisfies:** FR-NTF-011, FR-OFFICE-061
- **Notes:** —
