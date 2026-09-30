# ENV-REF-SERVER — PH-0 benchmark set-up (VER-BENCH-0001)

ENV-REF-SERVER (UBS-REQ-06 §0) is a 3-node Server cluster: application nodes of 16 vCPU
and 64 GB RAM; PostgreSQL 16 on a dedicated 32 vCPU, 128 GB RAM host with local NVMe
(at least 200k IOPS) and one synchronous standby; 10 Gbps with 0.5 ms RTT. For PH-0 the
owner rents one application node and the database host (DEC-030, UBS-PH-0 §6).

| Item | PH-0 value |
|---|---|
| Database host | 32 vCPU, 128 GB RAM, local NVMe ≥ 200k IOPS, ≥ 400 GB free (the 10M dataset takes about 75 GB of tables before indexes and WAL) |
| Application node | 16 vCPU, 64 GB RAM (the loader keeps about 10 GB of tree and head data in memory) |
| CPU | x86-64 with SHA extensions (`sha_ni` in `/proc/cpuinfo`, for example AMD EPYC or Intel Xeon Ice Lake and later); tree hashing dominates commit CPU (`verify/reports/exit-0-05/` T-1, C-1) |
| Network | same zone, RTT ≤ 0.5 ms between the two hosts |
| PostgreSQL | 16, `synchronous_commit = on`, `fsync = on`; `shared_buffers = 32GB`, `effective_cache_size = 96GB`, `max_wal_size = 32GB`, `checkpoint_timeout = 15min`, `wal_compression = on`, `random_page_cost = 1.1` |
| Deviation | no synchronous standby in PH-0: commit latency excludes the standby round trip; the difference is recorded in the report and revisited in PH-1 |
| Build | `cargo build --release --locked -p ubos-dvm --bin ubos-bench` on the application node, at the commit named in the report |

Every report of `ubos-bench run` records the CPU model, core count, SHA extensions,
memory, kernel and PostgreSQL settings, and a content identifier of these facts
(environment hash).
