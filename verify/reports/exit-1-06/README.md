# EXIT-1-06 — Durability under real crashes on PostgreSQL (SUITE-DVM-FAULT, VER-FAULT-0001)

| Item | Value |
|---|---|
| Criterion | EXIT-1-06: 0 lost acknowledged commits in ≥ 10,000 injected crashes on PostgreSQL (5,000 SIGKILL and 5,000 power-loss events; 0 partial commits; 0 verifier findings) |
| Result | **31,732 acknowledged commits, 0 lost, 0 partial, 0 verifier findings** over 10,000 events: 5,000 node crashes (SIGKILL of every worker process) and 5,000 power losses of the server (`fault.json`, signed by an ephemeral runner key) |
| Build | PH-1 step 8b (`cargo build --release --locked -p ubos-dvm --bin ubos-fault`), DEC-061 |
| Database | PostgreSQL 16.13 (Ubuntu 16.13-0ubuntu0.24.04.1) on the development workstation (4 vCPU, Linux x86-64), `recovery_init_sync_method = syncfs`; the ENV-REF-SERVER run is made at gate-1a acceptance (step 9) |
| Workload | WL-MIXED-OLTP (80% commits of 1–2 updates against the versions read, half with a ledger append; 20% reads); 4 worker processes, each logging every acknowledged commit to its own file with `fsync` |
| Duration | 7,492 s, of which 3,358 s server restarts |

## 1. Events and recovery checks

- **SIGKILL**: every worker process at a random moment — the node crashes; the workers are
  restarted after the check. A worker killed between its commit and its log line leaves a
  commit the store holds without an acknowledgement: 1,268 such commits in the run,
  each the consequence of one crash, none partial.
- **Power loss**: `pg_ctlcluster 16 main stop -m immediate` kills every server process
  without a checkpoint — the server's equivalent of pulling the plug — then `pg_ctl start`
  runs crash recovery; the workers lose their connections and are restarted.
- **After every event** the recovery check opens the VAE afresh on the quiet store (the
  workers are dead until it ends): every acknowledged commit is at its `cseq` with its
  identity and its versions, `cseq` is dense from the fork point, and `verify_roots` runs on
  the commits added since the last check (DSN-DVM-092, STD-PROOF-001).
- **Every 200 events and at the end** `verify_indexes` and `verify_sequences` rebuild the
  whole branches (DSN-DVM-092, DSN-DVM-402); every acknowledged ledger entry is present and
  the sequences are gap-free.

Samples of the run (every thousandth event):

| Event | Kind | Acknowledged so far | Without acknowledgement | Commits checked | Check ms |
|---|---|---|---|---|---|
| 1000 | power_loss | 4,696 | 167 | 11 | 510 |
| 2000 | power_loss | 8,632 | 325 | 6 | 775 |
| 3000 | power_loss | 12,340 | 464 | 7 | 914 |
| 4000 | power_loss | 15,712 | 608 | 4 | 1057 |
| 5000 | power_loss | 18,792 | 740 | 2 | 1090 |
| 6000 | power_loss | 21,812 | 855 | 4 | 1446 |
| 7000 | power_loss | 24,585 | 970 | 2 | 1338 |
| 8000 | power_loss | 27,211 | 1073 | 2 | 1637 |
| 9000 | power_loss | 29,466 | 1170 | 3 | 2326 |
| 10000 | power_loss | 31,732 | 1268 | 3 | 1950 |

## 2. Reproduce

```sh
cargo build --release --locked -p ubos-dvm --bin ubos-fault
# SIGKILL only (CI)
./target/release/ubos-fault --database "host=127.0.0.1 user=postgres dbname=bpa" --events 40 --workers 3
# with power loss (needs control of the server; the commands are configurable)
./target/release/ubos-fault --database "host=127.0.0.1 user=postgres dbname=bpa" --events 10000 --workers 4 \
    --power-loss --verify-every 200 --report fault.json
```
