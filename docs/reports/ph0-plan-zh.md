# PH-0 执行计划与进度

> 依据：UBS-PH-0（`docs/50-system/50-phases/PH-0/00-plan.md`）、DEC-028（单仓）、DEC-030（PH-0 执行方式）。
> 业主确认：2026-09-29（"按默认建议"）。

## 1. 目标

在正式开发前，用实测结果排除三大技术风险：
1. PostgreSQL 上的内容寻址、双时态、可分支存储能否达到 PH-1 性能目标的 80%（ASM-003、DEC-020）；
2. 核心语义（提交与 CAS、三方合并含账本追加、时间线代数）用形式化模型证明无违规；
3. 标准可以脱离具体语言来测试：BPA 0.1 发布，至少 300 个一致性向量，由运行器跑原型。

另做 Rhai 沙箱初步验证：100 个逃逸用例全部拦截（ASM-008）。

## 2. 执行方式（DEC-030）

- 由业主借助 Claude 会话执行。每一步一个 PR，业主审核后合并。
- PH-0 退出标准要求的验证工作（模型检查、向量、属性测试、基准测试、逃逸用例）属于业主已要求的范围。除此之外仍然不做测试，也不做安全审查。
- 验收时由外部评审（数据库和形式化方法专家）审阅模型报告和基准报告。
- 基准测试硬件（ENV-REF-SERVER 的数据库主机加一台应用节点）由业主在第 5 步租用。
- 签名发布（例如 BPA 0.1 的标签 `bpa-v0.1.0`）由业主用自己的密钥签名；会话负责准备发布清单和校验值。

## 3. 步骤与进度

| 步骤 | 内容 | 退出标准 | 状态 |
|---|---|---|---|
| 0 | 规格对齐：DEC-030；PH-0 计划去掉 ENV-REF-BOX（DEC-024），路径改为单仓文件夹（DEC-028）；ASM-001 加 phase note | — | 已完成（PR #2） |
| 1 | 单仓骨架：`standard/`、`verify/`、`crates/`（Cargo 工作区，含五个原型 crate），各目录许可证，CI（`regen_all.sh`、fmt/clippy、依赖图检查 AR-031） | 基础 | 已完成（PR #3） |
| 2 | TLA+ 模型 `verify/models/{commit_cas,merge,timeline}.tla`，按 DEC-031 检查（能穷举的穷举，否则最大可完成规模穷举加完整规模 100 万条随机模拟），报告见 `verify/models/reports/README.md` | EXIT-0-02 | 已完成（本 PR）：无违规；发现并修正 1 个设计问题（DSN-DVM-208），提出 OQ-004、OQ-005 |
| 3 | BPA 0.1：Core、Bitemporal、Ledger 条款，JSON Schema，向量格式，至少 300 个向量；覆盖率报告（MUST 条款覆盖至少 80%） | EXIT-0-01 | 已完成（PR #5 与本 PR）：551 个向量（453 个标 `prototype`），MUST 条款覆盖率 94%（68/72），发布清单已生成；待业主签名标签 `bpa-v0.1.0` |
| 4 | 原型 crate、适配器协议、`forge conform` 运行器；`prototype` 向量 100% 通过；属性测试每条至少 100 万例 | EXIT-0-03、EXIT-0-04 | 已完成：453 个 prototype 向量在内存与 PostgreSQL 上全部通过（EXIT-0-03，见 `verify/reports/exit-0-03/`）；三条性质各 100 万例 0 失败（EXIT-0-04，见 `verify/reports/exit-0-04/`） |
| 5 | ENV-REF-SERVER 基准测试（读 p95 ≤ 6.25 ms，提交 p50 ≤ 19 ms，diff p95 ≤ 62.5 ms），给出 ASM-003 结论 | EXIT-0-05 | 准备工作已完成（5a–5c）；正式测试因暂时租不到主机，**转入 PH-1**（DEC-035），租到主机后按 `docs/reports/ph0-step5-runbook-zh.md` 补跑 |
| 6 | Rhai 沙箱初步验证：100 个逃逸用例，给出 ASM-008 结论 | EXIT-0-06 | 已完成：100 个用例全部拦截且重放一致（见 `verify/reports/exit-0-06/`）；ASM-008 对 L2 成立（附条件，DEC-034），提出 OQ-007、OQ-008 |
| 7 | 根据 PH-0 结果重新制定 PH-1 计划；外部评审；用一条 DEC 记录验收结论 | EXIT-0-07 | 已完成：PH-1 已重新制定（DEC-036），发现处置表见 `docs/50-system/50-phases/PH-0/02-findings.md`；PH-0 已验收（DEC-037），外部评审与补跑基准一起进行 |

第 2、3 步在写代码之前完成（AR-033，标准先于代码）。

## 4. 需要业主做的事

| 时间 | 事项 |
|---|---|
| 第 3 步完成后 | 用自己的密钥创建带签名的标签 `bpa-v0.1.0` |
| 第 5 步开始前 | 租用云主机：数据库主机 32 vCPU、128 GB 内存、本地 NVMe（至少 20 万 IOPS），应用节点 16 vCPU、64 GB 内存，约 2 周 |
| 第 7 步 | 指定外部评审人 |

## 5. 签名 BPA 0.1 标签的操作（业主）

第 3b 步的 PR 合并后，在本机执行（需要已配置的 GPG 或 SSH 签名密钥）：

```sh
git fetch origin main
git tag -s bpa-v0.1.0 origin/main -m "BPA 0.1.0 (manifest: standard/release/0.1/manifest.json)"
git push origin bpa-v0.1.0
```

- 使用 SSH 密钥签名时，先执行一次 `git config gpg.format ssh` 和 `git config user.signingkey <公钥文件>`。
- 发布清单中的 `content_id` 就是这个版本内容的标识，可以写进标签说明，便于日后核对。

