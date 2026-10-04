# EXIT-1-05 — Concurrency and crash simulation (SUITE-DVM-SIM, VER-SIM-0001)

| Item | Value |
|---|---|
| Criterion | EXIT-1-05: 1,000,000 simulation seeds with 0 violations, including 30 consecutive clean nights |
| Result | **1,000,000 seeds, 0 violations** on the workstation (`simulation.json`); the 30 consecutive nights accumulate from the nightly workflow of step 8c, logged in `nights.md` |
| Build | PH-1 step 8b (`cargo build --release --locked -p ubos-dvm --bin ubos-sim`), DEC-061 |
| Simulator | `ubos-sim` (DSN-DVM-516): per seed 2–4 nodes over one shared in-memory store, 1–3 branches, six contended objects, two contended ledger keys, 30–120 scheduled steps; a failing seed is reproduced with `--only SEED` |
| Workstation | 4 vCPU, Linux x86-64, Rust 1.94, 4 threads, 24,849.2 s |

## 1. What the seeds exercised

| Quantity | Total over 1,000,000 seeds |
|---|---|
| scheduled steps | 74,495,354 |
| acknowledged commits | 54,305,916 |
| `TXN.HEAD_MOVED` seen by nodes | 50,513,548 |
| `TXN.READ_SET_CHANGED` seen by nodes (serializable mode) | 40,588,129 |
| retries (DSN-DVM-513, at most five per transaction) | 79,324,143 |
| aborted flushes (storage transaction rolled back) | 6,274,491 |
| crashes after apply (commit durable, acknowledgement lost) | 3,137,413 |
| node restarts with empty caches | 9,411,904 |
| clock jumps on one node (forwards and backwards) | 7,453,874 |

## 2. Checks (after a sample of steps and at the end of every seed)

1. **Linear history** per branch: `cseq` dense from the fork point to the head, every commit's
   parents include the previous commit, the head is the last commit (DSN-DVM-006, DSN-DVM-208).
2. **No lost acknowledged commit**: every acknowledged commit is in the store at its `cseq`
   with its identity, its versions and its entries (NR-DUR-001, DSN-DVM-508).
3. **Unacknowledged commits** equal the crashes after apply: a commit without acknowledgement
   is one whose acknowledgement a crash lost, never a partial one (FR-TXN-013).
4. **Gap-free ledger sequences** per key and branch (DSN-DVM-402).
5. **Serializable history** in the manner of Elle: per branch, the write sets of the commits in
   `cseq` order form one version chain per object (every update's base, and every version's
   predecessor, is the version the previous write produced), and a serializable
   transaction's read set was the head when it committed (DSN-DVM-510, DSN-DVM-511,
   FR-TXN-023).

## 3. Sensitivity

`ubos-sim --inject lost_ack` seeds the defect NR-DUR-001 forbids — the store acknowledges a
flush it did not apply (every seventh `cseq`). `sensitivity-lost-ack.json`: 20 violations
reported over 1,000 seeds (the runner stops a thread after ten), as lost acknowledged
commits and acknowledged identities the store does not hold.

## 4. Reproduce

```sh
cargo build --release --locked -p ubos-dvm --bin ubos-sim
./target/release/ubos-sim --seeds 1000000 --threads 4 --report simulation.json
./target/release/ubos-sim --seeds 1000 --inject lost_ack --report sensitivity-lost-ack.json
./target/release/ubos-sim --only 12345          # one seed, verbose
```

CI runs 2,000 seeds on every push; the nightly workflow (step 8c) runs 10,000 seeds a night.
