# SUITE-NOD-EVENTS — events of the node (PH-1 step 15)

`ubos-events-suite --suite events` (DEC-076, step 15e) checks the following against
DSN-NOD-201…-204, -208, -209, -228, NR-DUR-006 and FR-EVT-014, -015, -021…-024, -051, -052:

- the releaser and event log of step 15a;
- the subscriptions and handlers of step 15b;
- the leadership of the leader loop.

It runs on the memory backend and on PostgreSQL, on the development workstation (Intel Xeon
@ 2.10 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1, debug build; PostgreSQL 16 on the same
host). Reports: `events-suite-memory.json`, `events-suite-pg.json` (signed by ephemeral
runner keys).

| Backend | Result | Time |
|---|---|---|
| memory | **8 of 8 cases pass**, the 2 fault cases need PostgreSQL | 6.4 s |
| PostgreSQL | **10 of 10 cases pass** | 43.9 s |

The cases drive a Server in the suite's process through its bindings: UBTP over HTTP, the
REST facade and `/api/v1/admin`. The VAE is a memory VAE, or a fresh tenant of the
PostgreSQL registry. The two fault cases run nodes as child processes (`--serve`) on fresh
tenants, kill them with SIGKILL and restart them. Webhooks, brokers, replay and masking per
recipient are PH-2 (DEC-076).

| Case | Clause | memory / PostgreSQL | ms |
|---|---|---|---|
| release_order | DSN-NOD-201 | pass / pass | 858 / 5459 |
| outbox_claims | DSN-NOD-201 | pass / pass | 526 / 478 |
| subscription_scope | DSN-NOD-203 | pass / pass | 115 / 365 |
| handler_process_once | DSN-NOD-204 | pass / pass | 99 / 334 |
| dead_letters | DSN-NOD-209 | pass / pass | 397 / 1039 |
| became_effective | DSN-NOD-208 | pass / pass | 3547 / 3715 |
| event_log_query_retention | DSN-NOD-202 | pass / pass | 105 / 338 |
| leader_leases | DSN-NOD-228 | pass / pass | 401 / 408 |
| crash_recovery | NR-DUR-006 | — / pass | — / 13703 |
| failover | DSN-NOD-228 | — / pass | — / 17414 |

What the cases show:

- **release_order** (DSN-NOD-201, FR-EVT-014): 30 effects of three objects, committed in
  interleaved order, reach the event log once each, with 30 distinct identifiers. Each
  object's events keep their commit order, and the log's sequence increases.
- **outbox_claims** (DSN-NOD-201): claims are leases.
  - A second holder gets none of the effects the first holds, nor any later effect of an
    object whose earlier effect the first holds (per-object order).
  - The first holder's unacknowledged effects are claimable again after its lease.
  - Acknowledged effects are never handed out again.
- **subscription_scope** (DSN-NOD-203, FR-EVT-021…-023): with one subscription scoped to
  `main` and one to `feature/*`, the listener registry lists only the second for a work
  branch. A closed account on the work branch produces one delivery, for the subscription in
  scope.
- **handler_process_once** (DSN-NOD-204, FR-EVT-024): a delivery runs as one committed
  process of its service principal, with the event as its trigger. A redelivery of the same
  job reports `already_done` and does not run the handler again.
- **dead_letters** (DSN-NOD-209, FR-EVT-051, -052):
  - A handler whose target is missing is dead after its policy's 2 attempts, with its last
    error.
  - Once the target exists, a re-drive runs it once.
  - An operator annotates it. A second dead letter is cancelled.
  - A request to edit the payload has no route.
  - Dead letters and operator actions are in the security log.
- **became_effective** (DSN-NOD-208, NR-PERF-012): a version 2 s in the future fires
  `BecameEffective` once, 180 ms after its valid-from on PostgreSQL (35–40 ms in memory).
- **event_log_query_retention** (DSN-NOD-202, FR-EVT-015):
  - The log answers by object (in order), by type and by time.
  - With a 30-day retention for `AccountClosed`, a sweep at 29 days keeps the events; one at
    31 days removes them, and other types are kept.
  - The log needs an administrator.
- **leader_leases** (DSN-NOD-228): this node leads its VAE. A second holder waits while the
  first holder's lease lives, and takes it after the lease lapses with a larger token. The
  former holder cannot renew.
- **crash_recovery** (NR-DUR-006, FR-TXN-052; PostgreSQL):
  - 60 pings over three accounts, each with a delivery, then SIGKILL of the node while it
    releases and delivers.
  - After the restart, every effect is in the log once, in per-object order, and every
    delivery is done once. That took 1.3 s in this run.
  - When the kill catches a delivery claimed, the restarted node waits for that job's 60 s
    lease (DSN-NOD-221): 59.5 s in an earlier run, within NR-DUR-006's 5 minutes.
- **failover** (DSN-NOD-228; PostgreSQL): with two nodes, exactly one leads the VAE.
  Killing the leader hands the VAE to the other after 14.9 s, with token 2 after 1. The new
  leader fires became-effective events.

## Defects found and fixed

- **Connections grew with every store of every VAE**, and a failover with three nodes ran
  out of PostgreSQL's 100 connections. A node now keeps one pool per tenant. The cursors of
  deleted and disabled schedules are forgotten, so their VAEs no longer count as having work
  when nodes start (see `verify/reports/exit-1-17/nod/`).

## Reproduce

```
cargo build --locked -p ubos-node --bin ubos-events-suite
./target/debug/ubos-events-suite --suite events --report verify/reports/suite-nod-events/events-suite-memory.json
./target/debug/ubos-events-suite --suite events --database "host=127.0.0.1 user=postgres dbname=bpa" \
    --report verify/reports/suite-nod-events/events-suite-pg.json
```
