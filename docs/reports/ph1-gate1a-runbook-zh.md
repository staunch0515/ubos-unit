# PH-1 第 9 步：gate 1a 验收操作手册（EXIT-1-09 与参考环境复跑）

本手册给出租机后在 ENV-REF-SERVER（PH-1 基准与复跑）和 ENV-REF-SMALL（场景）上要执行的全部命令。
同一次租期先完成 PH-0 结转的基准 W-1（`docs/reports/ph0-step5-runbook-zh.md`，DEC-035），再做本手册。
结果文件放回仓库 `verify/reports/` 的对应目录，由会话归档并更新验收材料
（`docs/50-system/50-phases/PH-1/02-gate-1a-acceptance.md`）。

## 1. 主机（业主）

| 主机 | 规格（同 PH-0，`verify/envs/ENV-REF-SERVER-ph0.md`） |
|---|---|
| 数据库主机 | 32 vCPU、128 GB 内存、本地 NVMe ≥ 20 万 IOPS、可用 ≥ 400 GB、PostgreSQL 16 |
| 应用节点 | 16 vCPU、64 GB 内存、Ubuntu 24.04、Rust 工具链 |
| ENV-REF-SMALL | 一台 8 vCPU、32 GB 的节点，自带 PostgreSQL 16（场景与宿主套件） |
| 共同要求 | 同一可用区、往返 ≤ 0.5 ms；CPU 带 SHA 指令（`grep -c sha_ni /proc/cpuinfo` > 0） |

预计租期两周：W-1 约 4 天（含加载 1,000 万对象约 1 天），PH-1 基准与复跑约 5 天，余量给复测。

## 2. 准备（应用节点）

```sh
git clone https://github.com/staunch0515/ubos-unit && cd ubos-unit && git checkout <验收的提交>
curl https://sh.rustup.rs -sSf | sh -s -- -y && . ~/.cargo/env
cargo build --release --locked -p ubos-dvm --bins
cargo build --release --locked -p ubos-forge -p ubos-node -p ubos-verify
export DB="host=<数据库内网地址> user=bpa password=<密码> dbname=bpa"
mkdir -p results
```

数据库参数按 PH-0 手册第 2 节；另设 `recovery_init_sync_method = syncfs`（故障套件的断电重启）。
业主在数据库主机上需能执行 `pg_ctlcluster 16 main stop -m immediate` 与 `pg_ctl start`（断电模拟），
或把两条命令改为所在系统的等价命令并通过 `--pg-stop/--pg-start` 传入。

## 3. 基准 EXIT-1-09（VER-BENCH-0100，MET-PERF-010…015）

数据集 WL-GENERIC-10M（1,000 万对象、24 个月、种子 20260930）由 W-1 加载后复用；若重新加载：

```sh
./target/release/ubos-bench load --database "$DB" --threads 16 --report results/load.json
./target/release/ubos-bench check --database "$DB" --report results/check.json
```

先测可持续提交吞吐（取 p99 ≤ 60 ms 的最高速率 R_max），背景负载取 R_bg = 0.7 × R_max：

```sh
for r in 300 500 800 1200; do
  ./target/release/ubos-bench run --what commit --rate $r --seconds 120 --database "$DB" --report results/commit-$r.json
done
```

每项各跑三次，取中位数（UBS-VER-01 BENCH 程序）；`--background` 为上一步的 R_bg：

```sh
for i in 1 2 3; do
  ./target/release/ubos-bench run --what read   --rate 2000 --workers 16 --seconds 120 --background $R_bg --database "$DB" --report results/read-$i.json     # MET-PERF-010
  ./target/release/ubos-bench run --what commit --rate 500 --seconds 120 --database "$DB" --report results/commit-500-$i.json                               # MET-PERF-011
  ./target/release/ubos-bench run --what branch --rate 5 --seconds 60 --background $R_bg --database "$DB" --report results/branch-$i.json                   # MET-PERF-012
  ./target/release/ubos-bench run --what diff   --rate 50 --workers 8 --seconds 120 --background $R_bg --database "$DB" --report results/diff-$i.json       # MET-PERF-013
  ./target/release/ubos-bench run --what merge  --rate 0.2 --seconds 120 --background $R_bg --database "$DB" --report results/merge-$i.json                 # MET-PERF-014
  ./target/release/ubos-bench run --what asof   --rate 1000 --workers 16 --seconds 60 --background $R_bg --database "$DB" --report results/asof-$i.json    # MET-PERF-015
done
```

目标：点读 p95 ≤ 5 ms / p99 ≤ 20 ms；提交 500/s 时 p50 ≤ 15 ms / p99 ≤ 60 ms；分支创建 p99 ≤ 10 ms；
10 处变更的 diff p95 ≤ 50 ms；1,000 对象合并预览 p95 ≤ 2 s；asof 读 p95 ≤ 点读的 2 倍。
**发现 1 已处理（9b，2026-10-14）**：`merge_preview` 改为只读取树 diff 指出的改动对象和源分支
提交追加的账本键（STD-VER-031），不再构建完整状态；工作站 2 万对象、1,000 处改动：p50 0.37 s、
p95 0.40 s（改前 36 s），300 对象 0.12 s（改前 0.4 s），耗时随改动量而非对象总数增长。
租机复测时仍按上表运行 `--what merge`。

## 4. 复跑（ENV-REF-SERVER）

```sh
./target/release/forge conform --vectors standard/vectors --backend postgres --database "$DB" --report results/conformance-postgres.json   # EXIT-1-02
./target/release/ubos-store-suite --database "$DB" --report results/storage-suite.json                                                   # EXIT-1-03
./target/release/ubos-fault --database "$DB" --events 10000 --workers 4 --power-loss --verify-every 200 --report results/fault.json       # EXIT-1-06（断电命令见第 2 节）
./target/release/ubos-roundtrip --database "$DB" --vaes 20 --verifier ./target/release/ubos-verify --report results/roundtrip.json        # EXIT-1-12（其中一个 VAE 用 WL-GENERIC-10M：--objects 10000000）
./target/release/ubos-node-suite --database "$DB" --report results/node-suite.json                                                        # SUITE-NOD-HOST
```

## 5. 场景（ENV-REF-SMALL）

```sh
./target/release/forge test verify/scenarios --backend postgres --database "$DB_SMALL" --report results/scenarios.json --junit results/junit.xml   # EXIT-1-13
```

## 6. 交回

把 `results/` 整体交给会话；会话归档为 `verify/reports/exit-1-09/`（基准）与各 EXIT 目录的参考环境报告，
更新 `02-gate-1a-acceptance.md` 的证据表，并提出 DEC-062 的验收决定。外部评审人（DEC-038 第 8 项）
在同期审阅 `docs/reports/ph1-gate1a-acceptance-zh.md`；意见记入 UBS-PHR-1（PH-1 发现与处置）。
