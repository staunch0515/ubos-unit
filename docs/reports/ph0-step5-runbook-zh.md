# PH-0 第 5 步：基准测试操作手册（VER-BENCH-0001，EXIT-0-05）

## 1. 租用主机（业主）

| 主机 | 规格 |
|---|---|
| 数据库主机 | 32 vCPU、128 GB 内存、本地 NVMe（≥ 20 万 IOPS），可用空间 ≥ 400 GB |
| 应用节点 | 16 vCPU、64 GB 内存 |
| 共同要求 | 同一可用区，往返延迟 ≤ 0.5 ms；x86-64 CPU 带 SHA 指令（`grep -c sha_ni /proc/cpuinfo` 大于 0，例如 AMD EPYC、Intel Ice Lake 及以后）；Ubuntu 24.04 |

大约需要 2 周。详细定义见 `verify/envs/ENV-REF-SERVER-ph0.md`。PH-0 不配同步备库，报告中会注明。

## 2. 数据库主机

```sh
sudo apt-get install -y postgresql-16
sudo -u postgres psql -c "CREATE ROLE bpa LOGIN PASSWORD '<密码>'" -c "CREATE DATABASE bpa OWNER bpa"
```

在 `postgresql.conf` 中设置：`listen_addresses = '*'`、`shared_buffers = 32GB`、
`effective_cache_size = 96GB`、`max_wal_size = 32GB`、`checkpoint_timeout = 15min`、
`wal_compression = on`、`random_page_cost = 1.1`，并保持 `synchronous_commit = on`、`fsync = on`。
在 `pg_hba.conf` 中只放行应用节点的内网地址，然后重启 PostgreSQL。

## 3. 应用节点

```sh
git clone https://github.com/staunch0515/ubos-unit && cd ubos-unit
curl https://sh.rustup.rs -sSf | sh -s -- -y && . ~/.cargo/env
cargo build --release --locked -p ubos-dvm --bin ubos-bench
export DB="host=<数据库内网地址> user=bpa password=<密码> dbname=bpa"
```

先做一次正确性检查（小数据，约 1 分钟）：

```sh
./target/release/ubos-bench check --objects 3000 --months 6 --database "$DB"
```

## 4. 加载 WL-GENERIC-10M（1,000 万对象，24 个月，种子 20260930）

```sh
./target/release/ubos-bench load --database "$DB" --report load.json
```

预计 30 到 90 分钟，数据约 75 GB。

## 5. 测量（每项 30 分钟，共 3 轮）

```sh
for round in 1 2 3; do
  ./target/release/ubos-bench run --database "$DB" --what read   --seconds 1800 --rate 2000 --workers 16 --report read-$round.json
  ./target/release/ubos-bench run --database "$DB" --what commit --seconds 1800 --rate 500 --report commit-$round.json
  ./target/release/ubos-bench run --database "$DB" --what diff   --seconds 1800 --rate 50  --workers 4 --report diff-$round.json
done
```

判定取三轮的中位数：读 p95 ≤ 6.25 ms（MET-PERF-001），提交 p50 ≤ 19 ms（每秒 500 次，MET-PERF-002），10 个变更的 diff p95 ≤ 62.5 ms（MET-PERF-003）。

## 6. 交回

把 `load.json` 和 9 个 `*-N.json` 发给 AI 会话。AI 会把它们放进 `verify/reports/exit-0-05/`，写出基准报告，并起草 EXIT-0-05 的 DEC：达标就确认 ASM-003；不达标就记录 DEC-020 的回退或调整方案。测完后即可退租主机。
