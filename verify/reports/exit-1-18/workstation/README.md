# EXIT-1-18 — workstation run of SUITE-NOD-OPS (PH-1 step 16f, DEC-077)

Run of `crates/ubos-node/ops/ops_suite.py` (SUITE-NOD-OPS, PH-1 part; VER-FAULT-2000) on the
development workstation (Intel Xeon @ 2.10 GHz, 4 vCPUs, 15 GiB, Linux x86-64; PostgreSQL
16.13), installing the package `ubos-server-0.0.0-x86_64-linux.tar.gz` built by
`crates/ubos-node/packaging/build.sh` (static musl binary, release build). Report:
`suite-nod-ops.json`. The run on ENV-REF-SMALL follows with the reference runs of step 19
(DEC-077 item 7), as for EXIT-1-15 and EXIT-1-17.

## What the suite does

It follows `crates/ubos-node/packaging/INSTALL.md` on a PostgreSQL cluster of its own:

1. **Install** (DSN-NOD-601, FR-OPS-011): the package's checksum against `SHA256SUMS`,
   the binary, the guide and the systemd units in the package, the example configuration
   checked.
2. **PostgreSQL** (guide section 3): WAL archiving through `ubos-server backup wal-push`, the
   role `ubos` that is not a superuser (`REPLICATION`, member of `ubos_app`, owner of the
   database, `pg_create_restore_point` granted in the `ubos` database).
3. **Invalid configurations** (NR-OPER-003, VER-FAULT-2000 step 2): 214 files, every one
   rejected by `check-config` with the errors named.
4. **Bootstrap** (SCN-001): interrupted after the genesis step (exit 86), rerun to
   completion; the invitation link sets the administrator's password; the genesis Buks
   `system` and `ontology` recorded once each.
5. **Backups under load** (DSN-NOD-620, -623): commits of objects carrying a canary value
   run without pause while a base backup and 10 named restore points are taken 2 s apart;
   the WAL is archived through the last point; no canary in the sealed files; another key
   cannot open them.
6. **Diagnostic bundle** (DSN-NOD-615, NR-OPER-005) under the same load.
7. **Point-in-time restore** (DSN-NOD-620 acceptance 1, SCN-017): each of the 10 points
   restored into a new cluster, started, and verified by `ubos-server verify`: the branch
   heads equal those recorded at the point, the DVM root verifier and the index rebuild
   check pass over the last 1,000 commits of every VAE. A copy of the archive with a WAL
   segment removed refuses a restore past the gap and names the last consistent point
   (SCN-017 A1).
8. **Upgrade** (DSN-NOD-633, NR-OPER-002): `ubos-server upgrade` in one command with its
   report; a new binary whose verification fails is replaced by the previous one; the node
   starts ready afterwards.

## Results

| Measure | Result | Criterion |
|---|---|---|
| Checks | 35 of 35 passed | all |
| Installation from the package (binary, configuration) | 0.24 s; PostgreSQL prepared in 1.0 s | — |
| Invalid configurations rejected | 214 of 214 | 100% of ≥ 200 |
| Bootstrap, interrupted and completed | 4.6 s | idempotent completion |
| Commits under load during the backups | 570, none failed | — |
| Restores verified | 10 of 10 (3.1 … 12.0 s each, growing with the WAL replayed) | 10 of 10 |
| Diagnostic bundle | 2.0 s | ≤ 5 minutes |
| Upgrade in one command | 3.4 s, report written; rollback on a failed verification | one command with a report |
| Whole suite | 115 s | — |

## Reading

1. **Restore points under load.** A point is created inside one transaction that holds
   `branch_ref` in share mode: a commit moving a head waits a few milliseconds, every
   commit that moved one before has ended, so the heads recorded with the point are
   exactly the heads of a restore to it. The first design, reading the heads before and
   after the point and keeping them when equal, never settled under continuous commits;
   the suite found it.
2. **The guide.** The suite runs the guide's PostgreSQL preparation as written and found
   that `GRANT EXECUTE ON FUNCTION pg_create_restore_point` must run in the `ubos` database
   (function privileges are per database); the guide says so now.
3. **Not yet measured here.** The 7-day window of VER-FAULT-2000 step 3 is compressed to
   minutes on the workstation; the installation from the guide on a clean ENV-REF-SMALL host,
   with the time recorded, is the step-19 run. The OCI image, the Compose file and the Helm
   chart are built and checked by the `package` workflow (`.github/workflows/package.yml`),
   which also runs this suite on the x86-64 package and builds the ARM64 binary.
