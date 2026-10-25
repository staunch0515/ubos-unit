# BPA 1.0.0-rc10 — Release notes

Release candidate 10 of BPA 1.0 (PH-2 step 4a, DEC-105), the standard for delegation and human
work of the PH-2 kernel. Tag `bpa-v1.0.0-rc10`, signed by the owner (DSN-BPA-001). The manifest
(`release/1.0-rc10/manifest.json`) lists every released file with its SHA-256; its `content_id`
identifies this release. BPA 1.0-rc9 stays frozen as tagged. The standard precedes the code
(AR-033): PH-2 step 4 implements this frozen text and the approval clauses of 1.0-rc8 in parts
4b–4g.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 250 (scope by phase, `scope.json`): the 245 clauses of BPA 1.0-rc9 and 5 written in step 4a |
| MUST clauses with a vector range | 212, 212 covered (`coverage.md`) |
| Vectors | 2,181: those of BPA 1.0-rc9 and 56 of step 4a |

## Changes since BPA 1.0-rc9

- **Delegations** (STD-LGS-060, new): `Delegation` objects with delegator, delegate, roles, scope (`ports`, `classes`, `task_types`, `limit`) and validity; no role beyond the delegator's; `IAM.DELEGATION_EXPIRED` for an ended delegation; approvals and task actions through a delegation record both principals; `delegations` lists them for access reviews.
- **Tasks** (STD-LGS-061, new): `Task` objects with type, subject, queue or assignee, priority, due time and the states `open`, `claimed`, `completed`, `cancelled`; claim, release, reassign, complete and cancel with eligibility, `base` for optimistic concurrency and a log of every change.
- **Work queues, assignment and inbox** (STD-LGS-062, new): `WorkQueue` objects with members, managers and the rules `manual`, `round_robin`, `least_loaded` and `skill`; the inbox of a principal combines tasks, delegated tasks and the pending changes they may decide, with counts.
- **Due dates and escalation** (STD-LGS-063, new): due rules as L1 expressions over business calendars; escalation chains (`notify`, `reassign`, `raise_priority`) applied as the clock advances, recorded on the task and emitted as `task.escalated` effects.
- **Service-level report** (STD-LGS-064, new): attainment per task type and queue for a window, with late and open overdue tasks.
- **Governed reopening** (STD-KIND-025, STD-TXN-060, notes): when a period's class declares a command port `reopen`, its decisions govern `reopen_period`; with `defer` a reopening that requires approval waits as a pending change.
- **Approval evidence** (STD-LGS-043, `DAT-Approval`): the optional member `delegation`.
- **Genesis classes:** `Delegation`, `Task` and `WorkQueue`.
- **Adapter operations** `delegate`, `revoke_delegation`, `delegations`, `task`, `inbox` and `sla_report`; `approve` takes `delegation`, and `reopen_period` takes `defer` and `ctx`.

## Superseded vectors

None. No vector of BPA 1.0-rc9 changed.

## Corrections before the tag

- **Slugs of kernel-written objects** (PH-2 step 4d, DEC-108): the reference model named a delegation or a task by the first eight characters of its ID, so two of them in one vector shared a slug, which STD-OBJ-004 makes unique per class. The slug is now `delegation-<id>` or `task-<id>`; VER-CONF-5911, -5920 and -5955, which read a task, changed accordingly before `bpa-v1.0.0-rc10` was signed.

## Known limits

- **Kernel.** The vectors added in 1.0-rc8, 1.0-rc9 and 1.0-rc10 are not tagged `prototype`. The kernel runs the approval and work vectors as PH-2 step 4 implements them (parts 4b–4g, EXIT-2-01, VER-CONF-8405).
- **Simulated outside world.** Roles come from the adapter's directory (`principal`); escalations are applied when `set_now` advances the clock.
- **Not in this release:**
  - human tasks and approvals inside workflow steps beyond STD-TXN-091 (PH-2 step 6);
  - delivery of escalation notifications to people over channels (PH-2 step 13);
  - delegation chains longer than one principal;
  - membership of queues by role.
