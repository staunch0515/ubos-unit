# PH-0 验收材料（DEC-037 提案）

供业主确认验收，并供外部评审人（数据库与形式化方法专家）审阅。依据的规格文件：PH-0 计划 UBS-PH-0（`docs/50-system/50-phases/PH-0/00-plan.md`）、发现处置表 UBS-PHR-0（`docs/50-system/50-phases/PH-0/02-findings.md`）、决定 DEC-030 至 DEC-037（`docs/50-system/90-decisions/000-index.md`）。

## 1. 结论（提案）

建议有条件验收 PH-0：7 条退出标准中 6 条已达成；EXIT-0-05（存储基准）因暂时租不到主机，转入 PH-1（DEC-035）。外部评审与补跑基准一起进行。业主确认后，DEC-037 的状态改为 Accepted，PH-1 即可开始。

## 2. 退出标准与证据

| 退出标准 | 内容 | 状态 | 证据 |
|---|---|---|---|
| EXIT-0-01 | BPA 0.1 发布并签名，≥ 300 个向量 | 达成：551 个向量，MUST 条款覆盖率 94%，业主已签名标签 `bpa-v0.1.0` | `standard/release/0.1/`，DEC-032 |
| EXIT-0-02 | 三个形式化模型检查无违规 | 达成：按 DEC-031 的规模检查；发现的 1 个设计问题已修正（DSN-DVM-208） | `verify/models/reports/` |
| EXIT-0-03 | 原型在 PostgreSQL 上通过全部 `prototype` 向量 | 达成：453/453，有签名报告 | `verify/reports/exit-0-03/` |
| EXIT-0-04 | 根哈希与历史无关，≥ 100 万例 | 达成：三条性质各 100 万例，0 失败；另加的树增量性质 10 万例，0 失败 | `verify/reports/exit-0-04/`，`verify/reports/exit-0-05/tree-property.json` |
| EXIT-0-05 | 基准达到 MET-PERF-001～003 | **转入 PH-1**（DEC-035）；工具、环境定义和操作手册已备好；本机试跑不作为证据 | `verify/reports/exit-0-05/`，`docs/reports/ph0-step5-runbook-zh.md` |
| EXIT-0-06 | Rhai 沙箱拦截 100 个初始逃逸用例 | 达成：100/100；ASM-008 对 L2 成立（附条件，DEC-034） | `verify/reports/exit-0-06/` |
| EXIT-0-07 | 根据 PH-0 结果重新制定 PH-1 | 达成：DEC-036，UBS-PHR-0，更新后的 UBS-PH-1 | 本 PR |

## 3. 主要发现与处置（详见 UBS-PHR-0）

- 形式化模型发现 1 个真实设计问题（F-1：定序器用了过期的视图），已在写代码之前修正。
- 沙箱：Rhai 默认包里的 `sleep` 能绕过时间限制；引擎自身不限制总内存。两者都已写进 DSN-DVM-612，PH-1 继续做每个 worker 的内存计量。
- 存储：提交速度受 CPU 限制，树节点的 SHA-256 哈希是主要开销（本机约每秒 57 次，目标每秒 500 次）。RSK-001 上调为高，在 PH-1 以 RSK-016 继续跟踪；另新增 RSK-017（哈希吞吐）。能否达标由补跑的基准决定。
- 未决问题：OQ-007 决定把 L2 大小上限写进 STD-CTX-004，从 BPA 1.0-rc1 起生效；OQ-008 决定 L2 使用内核的小数类型。
- RSK-004（Rhai 不用浮点就无法确定性）没有发生，已关闭。ASM-008 的 WASM 部分放到 PH-3 验证（DEC-036 更正了 DEC-034 的说法）。

## 4. 转入 PH-1 的事项

| 事项 | 负责 | 期限 |
|---|---|---|
| W-1：在 ENV-REF-SERVER 上补跑 VER-BENCH-0001，并用 DEC 给出 ASM-003 结论 | 业主（主机），AI 会话（运行与报告） | PH-1 存储布局定型之前，最迟 gate 1a |
| 外部评审（含 RSK-005 的外部实现者评审） | 业主指定评审人 | 与 W-1 同时 |
| W-2 至 W-12（见 UBS-PHR-0 §3） | AI 会话 | PH-1 计划内 |

## 5. 请外部评审人重点看的内容

1. `verify/models/`：模型是否抓住了提交 CAS、合并和时间线的关键性质，检查规模是否足够（DEC-031）。
2. `verify/reports/exit-0-05/`：存储布局、group commit，以及基准方法（不遗漏排队时间、记录环境哈希）。
3. `standard/`：向量格式是否依赖某个具体实现（RSK-005）。

## 6. 需要业主做的

1. 回复确认 DEC-037（有条件验收 PH-0）。
2. 租到主机时，按操作手册补跑基准，并在那时指定外部评审人。
