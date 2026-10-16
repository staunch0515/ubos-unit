# EXIT-1-17 — workstation runs of PH-1 step 15e (MET-PERF-017, NR-PERF-012, NR-DUR-006)

Runs of `ubos-events-suite --bench` (DEC-076, part 15e) on the development workstation
(Intel Xeon @ 2.10 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1, release build; PostgreSQL 16
on the same machine, default settings). They are the NOD part of VER-BENCH-2000 on the
workstation; the reference-environment runs under WL-MIXED-OLTP on ENV-REF-SERVER follow at
step 19. Reports: `bench-memory.json` and `bench-pg.json` (signed by ephemeral runner keys).

## Results

| Metric | Target | Memory | PostgreSQL | Met |
|---|---|---|---|---|
| MET-PERF-017 commit to handler, under load | p95 ≤ 500 ms | p95 **140.5 ms** (p50 91.7, p99 160.2) | p95 **432.6 ms** (p50 303.3, p99 557.9) | yes / yes |
| MET-PERF-017 commit to handler, idle | — | p95 9.9 ms (p50 6.9) | p95 87.5 ms (p50 73.7) | — |
| NR-PERF-012 became-effective after valid-from | p99 ≤ 5 s | p99 **0.39 s** (p50 0.20, max 0.41) | p99 **0.51 s** (p50 0.27, max 0.55) | yes / yes |
| NR-DUR-006 pending effects released after recovery | 100% within 5 min | — (needs PostgreSQL) | **1,000 of 1,000 in 1.3 s**, each once | yes |

## How each is measured

- **MET-PERF-017.** A command port `ping` commits on an account and emits `Pinged`. A
  subscription's port handler `mark` commits on the same account, run by the Server's
  releaser and job worker as a process of a service principal. The latency is from the
  ping's commit to the handler's commit, both by the kernel's commit times (`tx_time`). The
  handler's commit bounds its start from above. The samples run one at a time: 100 idle,
  then 300 under background load. The suite waits on the delivery job in the job store, so
  measuring adds no load to the VAE.
  - The load is four clients calling a command port on their own accounts. It is paced to
    70% of the capacity measured just before: four unpaced clients for 5 s, 169.6 calls/s in
    memory and 26.6 calls/s on PostgreSQL. This follows the 70% convention of WL-MIXED-OLTP.
  - The clients did not reach the rate they aimed for: 11.5 of 18.6 calls/s on PostgreSQL
    and 39.4 of 118.7 calls/s in memory. The measured flow itself (each ping: port call,
    release, delivery job, handler, job state) occupies the VAE's one actor, so both loaded
    figures are for a saturated VAE.
- **NR-PERF-012.** 1,000 versions of a `Rate` class with instant valid time and
  `emit_became_effective` are committed in ten commits on `main`. Their valid-from instants
  are spread evenly over 45 s starting 15 s after the commits. The lag is the event's
  `released_at` in the event log minus its valid-from. No event was released early, and all
  1,000 fired once.
- **NR-DUR-006.** The crash between commit and release is simulated: 1,000 effects are
  committed on a fresh tenant with no node running (the VAE in server mode, nothing claims
  its outbox). Then a node starts as a child process. The time runs from the start of the
  node to the last `Pinged` event in its event log; all 1,000 effect identifiers are
  distinct. SUITE-NOD-EVENTS' `crash_recovery` covers the SIGKILL of a node in the middle
  of releasing and delivering.

## Defects found and fixed

- **The sweeper grew with the history of jobs.** Every 30 s it read each `Job` object of
  `main` on the VAE's actor (reading all 300 delivery jobs of the first benchmark took up to
  4.7 s). Became-effective events waited behind it, and NR-PERF-012 failed on PostgreSQL
  (p99 8.3 s). The sweeper now reads only the objects without a row in the `job` table
  (`JobStore::ids`), normally none.
- **A tick read the branch state once per due timer.** `Engine::tick` built a view of the
  branch for each due timer. It now builds it once per branch per tick: on PostgreSQL the
  p99 went from 3.7 s to 0.42 s (0.51 s in the final run).
- **An operator's port job had no principal.** A port job enqueued through
  `POST /api/v1/admin/jobs` has no parent process, so the worker found no principal and the
  job went straight to `dead`. It now runs as the operator who enqueued it.
- **Connections grew with every store of every VAE.** Each VAE a node opened used about
  five pools of up to 4 connections (VAE services, events, jobs, schedules), plus identity
  and secrets per tenant. Three nodes on the workstation reached 93 of PostgreSQL's 100
  connections, and a failover case timed out on the pool. A node now keeps one pool per
  tenant (`TENANT_POOL`, 8 connections), and the kernel store keeps its own.
- **Schedule cursors outlived their schedules.** Nodes open at start every VAE with a schedule
  cursor (step 15d, DSN-NOD-226), and the cursors of deleted or disabled schedules stayed, so
  each test run added VAEs that every node opened (about 12 connections each). The leader now
  forgets the cursors of schedules that are deleted or disabled when the VAE's schedules
  change, and the jobs suite disables its schedules at the end.

## Observations for step 19

- On this workstation's PostgreSQL a VAE answers about 17–27 command-port calls per second,
  because its actor serialises the commits. The reference environment and WL-MIXED-OLTP
  will show whether that leaves room for the 70% load at the stated latency, or whether
  group commit across a VAE's sessions is needed.
- Jobs claimed by a killed node wait for their lease (60 s for jobs, 30 s for outbox
  claims) before another holder takes them. So `crash_recovery` completes within about 1 s
  of the restart, or about 60 s when the kill catches a delivery claimed: within NR-DUR-006,
  and by design (DSN-NOD-221).
- A node's connections grow with the VAEs it has open: each VAE's kernel store keeps a pool
  of 4, and each tenant a pool of 8. Three nodes on one PostgreSQL with default settings
  (100 connections) are close to the limit on this workstation. The reference environment
  needs a connection budget per node, or a pooler, in front of PostgreSQL.

## Reproduce

```
cargo build --release --locked -p ubos-node --bin ubos-events-suite
./target/release/ubos-events-suite --bench --report verify/reports/exit-1-17/nod/bench-memory.json
./target/release/ubos-events-suite --bench --database "host=127.0.0.1 user=postgres dbname=bpa" \
    --report verify/reports/exit-1-17/nod/bench-pg.json
```
