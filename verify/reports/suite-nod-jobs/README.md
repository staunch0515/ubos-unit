# SUITE-NOD-JOBS — jobs and schedules of the node (PH-1 step 15)

`ubos-events-suite --suite jobs` (DEC-076, step 15e) checks the jobs of step 15c and the
schedules of step 15d against DSN-NOD-220…-226, -229, -230 and FR-EVT-031, -032, -041…-044.
It runs on the memory backend and on PostgreSQL, on the development workstation (Intel Xeon
@ 2.10 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1, debug build; PostgreSQL 16 on the same
host). Reports: `jobs-suite-memory.json`, `jobs-suite-pg.json` (signed by ephemeral runner
keys).

| Backend | Result | Time |
|---|---|---|
| memory | **13 of 13 cases pass** | 47.0 s |
| PostgreSQL | **13 of 13 cases pass** | 60.7 s |

How the cases run:

- Most cases drive a Server in the suite's process through its bindings: UBTP over HTTP,
  the REST facade and `/api/v1/admin`. The VAE is a memory VAE, or a fresh tenant of the
  PostgreSQL registry.
- The store cases call the job store directly, with a job kind the Server's worker does
  not run.
- The schedule cases evaluate on a simulated clock against the schedule store (memory, or
  `schedule_slot` on PostgreSQL).
- `schedule_through_server` waits for a real minute boundary, up to 75 s, so the suite's
  time depends mostly on when it starts within a minute.

| Case | Clause | memory / PostgreSQL | ms |
|---|---|---|---|
| jobs_are_objects | DSN-NOD-220 | pass / pass | 95 / 317 |
| lease_fencing | DSN-NOD-221 | pass / pass | 450 / 467 |
| retries_and_poison | DSN-NOD-223 | pass / pass | 0 / 24 |
| priorities_and_limits | DSN-NOD-229 | pass / pass | 0 / 18 |
| sweeper_restores | FR-EVT-032 | pass / pass | 69 / 221 |
| bulk_cancel | DSN-NOD-230 | pass / pass | 979 / 4952 |
| idempotent_execution | DSN-NOD-221 | pass / pass | 18 / 283 |
| schedule_rules | FR-EVT-041 | pass / pass | 0 / 0 |
| schedule_exactly_once | DSN-NOD-224 | pass / pass | 17 / 170 |
| schedule_missed_slots | DSN-NOD-226 | pass / pass | 0 / 67 |
| schedule_pause_resume | DSN-NOD-225 | pass / pass | 1 / 58 |
| schedule_hot_change | FR-EVT-042 | pass / pass | 27 / 142 |
| schedule_through_server | DSN-NOD-224 | pass / pass | 44873 / 53245 |

What the cases show:

- **jobs_are_objects** (FR-EVT-031): a `run.async` port job is a System-kind `Job` object on `main`. The object reads `succeeded` once the job ran, and an ordinary commit that tries to change it is refused (`LOGIC.FORBIDDEN_TARGET`).
- **lease_fencing** (FR-EVT-033 acceptance 1):
  - A claimed job's lease is heartbeated, and no other worker can claim it while the lease lives.
  - After the lease lapses, a second worker claims the job with a larger token.
  - The first worker's heartbeat and result are then refused, and the second worker's result is kept.
- **retries_and_poison** (FR-EVT-051, -053):
  - The default policy is 5 attempts with backoff from 1 s, doubling: 1 s, 2 s, 4 s, 8 s.
  - The same error three times is poison, and the job is dead at once.
  - Different errors retry until the policy is spent, then the job is dead without poison.
- **priorities_and_limits** (DSN-NOD-229, FR-EVT-034):
  - A paused queue gives no job. Claims take the highest priority first.
  - Queue concurrency 1 and the tenant limit each hold back a second claim.
  - A re-prioritised job goes first.
- **sweeper_restores** (FR-EVT-032): the pending `Job` object of an operator's port job in a paused queue survives the loss of its row. The sweeper rebuilds the row from the object, leaves an existing row alone, and the job runs once the queue resumes.
- **bulk_cancel** (DSN-NOD-230): an operator's bulk job of 200 batches is cancelled while it runs. It stops between batches: after 35 batches in memory and 9 on PostgreSQL.
- **idempotent_execution** (DSN-NOD-221 with STD-TXN-040): two runs of the same job's process with the same idempotency key commit once. The second run replays the first commit, and the head does not move.
- **schedule_rules** (FR-EVT-041 acceptance 1):
  - "18:00 America/New_York on business days" keeps 18:00 local across the DST change (22:00 UTC, then 23:00 UTC). A holiday has no slot.
  - The nth business day, and cron steps, ranges and weekdays.
  - No slot in the spring-forward gap, and one slot at the earlier instant in the fall-back overlap. A bad rule is refused.
- **schedule_exactly_once** (DSN-NOD-224 acceptance 1, FR-EVT-043): two evaluators, as two nodes during a lease handover, evaluate the same schedule at once over 20 slots. They fire 20 times with 20 distinct job identifiers, each derived from its slot.
- **schedule_missed_slots** (DSN-NOD-226 acceptance 1): after a 3-hour outage over an hourly schedule, `fire_all` runs three jobs in slot order, `fire_once` runs the latest slot and `skip` runs none. Every missed slot is recorded.
- **schedule_pause_resume** (DSN-NOD-225 acceptance 1, FR-EVT-044 acceptance 1):
  - A pause over 3 slots records them paused and fires nothing.
  - On resume, `fire_once` runs one catch-up job and records the others skipped; `fire_all` runs three.
  - A second evaluation adds nothing.
- **schedule_hot_change** (FR-EVT-042 acceptance 1):
  - A daily schedule changed to hourly fires within the next hour.
  - Through the Server, the committed change shows in `GET /api/v1/admin/schedules` at once, without a restart.
- **schedule_through_server** (DSN-NOD-224, FR-EVT-044):
  - An every-minute schedule on `main` fires its next slot once, as its service principal, with the slot passed to its target (observed 0.2–0.8 s after the slot).
  - The slot is recorded `fired` with its job.
  - An operator pauses the schedule, and the pause is listed and audited.
  - The suite disables its schedules at the end, so their cursors are forgotten.

## Defects found and fixed

- **An operator's port job had no principal** and was dead at its first attempt. It now runs as the operator who enqueued it.
- **The sweeper read every `Job` object** of `main` every 30 s. It now reads only the objects without a row (see `verify/reports/exit-1-17/nod/`).

## Reproduce

```
cargo build --locked -p ubos-node --bin ubos-events-suite
./target/debug/ubos-events-suite --suite jobs --report verify/reports/suite-nod-jobs/jobs-suite-memory.json
./target/debug/ubos-events-suite --suite jobs --database "host=127.0.0.1 user=postgres dbname=bpa" \
    --report verify/reports/suite-nod-jobs/jobs-suite-pg.json
```
