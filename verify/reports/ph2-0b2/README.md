# PH-2 step 0b-2 — file versions and idempotency retention (DEC-087)

| Item | Value |
|---|---|
| Scope | The two acceptance tests DEC-087 moved to PH-2 step 0b — FR-FILE-011, and FR-TXN-032 with the retention sweep of DSN-DVM-531 |
| Result | Both met on PostgreSQL in SUITE-NOD-EVENTS: cases `file_versions` and `idempotency_retention` (15 of 15 cases pass) |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, release build, 2026-10-19; `ubos-events-suite --suite events --database …` |

## FR-FILE-011 — a replaced attachment, both files by version

The file service already stored files as content-addressed chunks and linked them to object
versions (PH-1 step 17a, DEC-078); the acceptance test was missing.

The case `file_versions`:
1. uploads two different files;
2. creates a `Statement` whose `scan` field refers to the first;
3. replaces the attachment with the second, which makes a new version of the object.

It then asks the file service for a download link at three coordinates:
- the branch head: the second file;
- the commit of the first version: the first file;
- the commit of the second version: the second file.

Each download is compared byte for byte with the content uploaded.

A link at a commit names the branch as well (`?branch=…&commit=…`). Without it, the link API
adds the default branch to the read, and a commit of another branch is refused
`ADDR.CONFLICTING_SELECTORS`. This is the selector rule of STD-ADDR-002, not a defect.

## FR-TXN-032 and DSN-DVM-531 — idempotency retention

**Change.**
- **Expiry.** An idempotency record now carries its expiry: the commit's time plus the VAE's
  retention. The retention is 7 days by default; it is set per VAE and is never less than
  7 days.
- **After expiry.** Within the retention, a repeated request returns the first result.
  After it, the record no longer counts: the request is new, and its commit replaces the
  record. In memory the flush checks the expiry; on PostgreSQL the insert of an existing key
  replaces the row only when it has expired.
- **Sweep.** The sweep removes expired records. The leader runs it for each VAE at the
  interval of the event-log sweep, on the VAE's clock (the test clock in the dev preset).
- **Operator API** (an administrator's token):
  - `GET` and `PUT /api/v1/admin/idempotency/retention` `{days}`;
  - `POST /api/v1/admin/idempotency/sweep` `{now?}`.
- **Storage.** The expiry uses the `expires_at` column that `idempotency_record` already had
  (migration 0003), so no migration is needed. Records written before this part have no
  expiry and are kept.

**Test.** The case `idempotency_retention`, on PostgreSQL, uses two node processes in the dev
preset, which have a test clock:

| Step | Check |
|---|---|
| clock set to 2026-11-02 | the retention reads 7 days; 3 days is refused |
| request with key `retry-1` | it commits |
| the node process is killed; a second process opens the VAE; clock +6 days | the same request returns the first commit (FR-TXN-032 acceptance: a retry 6 days later after a failover is deduplicated) |
| another payload under the key | `TXN.IDEMPOTENCY_MISMATCH` |
| clock +8 days | the other payload commits: the key is new (DSN-DVM-531) |
| sweep | the expired record is removed (DSN-DVM-531 acceptance) |
| retention set to 30 days | read back as 30 |

## Verification

| Suite | Result |
|---|---|
| SUITE-NOD-EVENTS on PostgreSQL | 15 of 15 (with `file_versions` and `idempotency_retention`) |
| SUITE-NOD-JOBS on PostgreSQL | 13 of 13 |
| Conformance, in memory and on PostgreSQL | 0 failures |
| Replay corpus | 10,000 of 10,000 outcomes and 1,000 of 1,000 replays identical |
| Simulation; properties | 2,000 seeds, 0 violations; 10,000 cases each, 0 failures |
| L2 escape; storage; fault | 382 of 382 blocked; 20 of 20; 40 SIGKILL events, 0 findings |
| Scenarios | 28 of 28 in memory, on PostgreSQL and over UBTP (WebSocket and HTTP) |
| Node | node suite 12 of 12; `cargo test -p ubos-node` passed; SUITE-NOD-ISO 544 of 544 |
| `cargo test` of forge, dvm, store and version on PostgreSQL | passed |
| fmt, clippy, `check_deps.py`, `regen_all.sh` | clean |

## Note

`crash_recovery` of SUITE-NOD-EVENTS failed twice in a debug build with `TXN.HEAD_MOVED` on a
port call. In that case the subscription's handler commits to the same accounts while the
port calls run, and the slower debug build makes the retries collide more often. The release
build that CI uses passes it. The case's port calls could retry on `TXN.HEAD_MOVED` as a
client does. That would be a separate change to the suite; it is not made here.
