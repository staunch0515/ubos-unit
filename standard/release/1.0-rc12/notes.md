# BPA 1.0.0-rc12 — Release notes

Release candidate 12 of BPA 1.0 (PH-2 step 6a, DEC-119), the standard for the durable work of
the PH-2 kernel. Tag `bpa-v1.0.0-rc12`, signed by the owner (DSN-BPA-001). The manifest
(`release/1.0-rc12/manifest.json`) lists every released file with its SHA-256; its `content_id`
identifies this release. BPA 1.0-rc11 stays frozen as tagged. The standard precedes the code
(AR-033): PH-2 step 6 implements this frozen text and the workflow, saga and quarantine clauses
of 1.0-rc8 in parts 6b–6g.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 259 (scope by phase, `scope.json`): the 255 clauses of BPA 1.0-rc11 and 4 written in step 6a |
| MUST clauses with a vector range | 221, 221 covered (`coverage.md`) |
| Vectors | 2,275: those of BPA 1.0-rc11 except one superseded, and 68 of step 6a |

## Changes since BPA 1.0-rc11

- **Saga timeouts, monitoring and intervention** (STD-TXN-081, new): a saga step may declare a `timeout`; a step whose process has not committed by then times out (`FLOW.TIMEOUT`) and, like a limit termination, follows `on_timeout` — `recover` as a failure, or `escalate` to the state `stuck` with the effect `SagaStuck`. The operation `sagas` lists sagas by state, age of the step in progress and step; `saga_action` lets the saga's `operators` retry or skip the step of a stuck or forward-failed saga, or cancel it.
- **Workflow tasks and approvals** (STD-TXN-093, new): a `human_task` step with a `queue` creates a `Task` (STD-LGS-061) that the queue assigns and dates; completing the task continues the instance, cancelling it follows `on_cancel`. An `action` step with `defer` waits for the decision on its pending change and continues to `next` or `on_reject`.
- **Workflow timers, business calendars and timeouts** (STD-TXN-094, new): a `timer` is due `after` a duration or `at` an expression evaluated on entry with the step's `calendars`; `wait_event` and `human_task` steps may declare `timeout` and `on_timeout`; a migration recomputes due times from the entry instant.
- **`ask.input`** (STD-ISA-101, new): a process may ask its caller for typed input; it then ends `needs_input` without committing, and the repeated call with `answers` continues it before the request expires (`FLOW.INPUT_EXPIRED`).
- **Error codes:** `FLOW.TIMEOUT`, `FLOW.INPUT_EXPIRED`.
- **Members:** `Saga` gains `operators`, `skipped` and the state `stuck`; `Task` gains `workflow`; workflow steps gain `at`, `calendars`, `timeout`, `on_timeout`, `queue`, `task_type`, `subject`, `priority`, `data`, `on_cancel`, `defer` and `on_reject`.
- **Adapter operations** `sagas` and `saga_action`; `execute` takes `answers` and may return `needs_input`.

## Superseded vectors

- VER-CONF-3004 asserts the instruction catalogue of 1.0-rc9…rc11; `ask.input` joins the catalogue, and VER-CONF-3005 asserts the catalogue of this release.

## Known limits

- **Kernel.** The kernel runs the workflow, saga, quarantine and durable-work vectors as PH-2 step 6 implements them (parts 6b–6g, EXIT-2-01, VER-CONF-8405).
- **Not in this release:**
  - pre-commit connector calls in the kernel (STD-TXN-051, STD-ISA-090; PH-2 step 12);
  - event replay to a subscription and projection backfill, which are node features outside the standard (DSN-NOD-206, -207; PH-2 step 6f);
  - dialogues answered by agents (FR-FLOW-072, PH-3).
