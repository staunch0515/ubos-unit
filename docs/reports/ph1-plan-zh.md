# PH-1 执行计划（DEC-038）

PH-1 分两个门（DEC-014）：gate 1a 引擎核心（237 项需求，EXIT-1-01 至 EXIT-1-13），gate 1b 治理与服务（101 项需求，EXIT-1-14 至 EXIT-1-20）。PH-1 由业主和 AI 会话执行，一步一个 PR，业主合并；目前只细化 gate 1a，gate 1b 在 1a 验收时再细化。规格见 `docs/50-system/50-phases/PH-1/00-plan.md` §11。

## 1. Gate 1a 步骤

| 步骤 | 内容 | 退出标准 | 状态 |
|---|---|---|---|
| 0 | 对齐：DEC-037 改为 Accepted，新增 DEC-038，UBS-PH-1 §11 写入步骤，建立本表 | — | 已完成（PR #14） |
| 1 | BPA 1.0-rc1：补全 Core、Bitemporal、Ledger、Proof（子集）和执行（CTX）条款与向量，MUST 覆盖 100%；先标准后代码，分 1a–1f 六个 PR（DEC-039） | EXIT-1-01 | 已完成（1f 发布，DEC-045；待业主签名标签 `bpa-v1.0.0-rc1`）：1a–1e（范围规则与工具；对象与类模型 84 个向量，DEC-040；版本、时间、事务 187 个向量，DEC-041；证明 66 个向量，DEC-042；L1 表达式、查询、地址、自定义值类型 174 个向量，DEC-043；执行 160 个向量，DEC-044），rc1 MUST 覆盖 146/146（100%），发布说明见 `standard/release/1.0-rc1/notes.md` |
| 2 | 扩展形式化模型：group commit、账本序列 | EXIT-1-07 | 已完成（DEC-046）：模型 `verify/models/group_commit.tla`（两段式刷写：并行准备、串行排序与树更新）；8 条性质 0 违规（2 次提交穷举，4 次提交规模 100 万条随机模拟，均无违规）；7 个变异均被发现；报告见 `verify/models/reports/README.md` 第 5 节 |
| 3 | 存储内核：StoreTx、sqlx、RLS 与分区、其余索引、分块/压缩/加密、校验器、存储套件；布局等 W-1 结论后定型 | EXIT-1-03 | 已完成（DEC-047，分 3a–3c 三个 PR）：3a 已完成（`StoreTx`/`StoreRead` 异步接口、sqlx PostgreSQL 适配器、按 DVM-04 的迁移脚本（含过渡列）、分支锁下的两段式组提交与账本计数器；453 个 prototype 向量在新适配器上通过）；3b 已完成（DEC-048：5 张大表按租户 64 个哈希分区，所有表启用行级安全并以应用角色 `ubos_app` 访问，账本条目不可修改，zstd + XChaCha20-Poly1305 存储格式及读时校验，块 ≤ 8 KiB 存库内、更大的存本地 blob 目录，`ubos-crypto` 包与本地密钥提供者）；3c 已完成（DEC-049：索引、序列、树根三个校验器，存储套件 20 个用例在 PostgreSQL 16 上全部通过，含 10,000 次追加 5% 注入失败、200 次并发追加、注入损坏被发现；接口检查记录；证据见 `verify/reports/exit-1-03/`）。EXIT-1-03 在本机和 CI 达成，ENV-REF-SERVER 上的一次在第 9 步验收时跑 |
| 4 | 模型、版本、时间、账本、事务生产化：完整类注册（C3、收窄）、租户和 IAM 基础 | EXIT-1-02（部分） | 进行中（DEC-050，分 4a–4d 四个 PR）：4a 已完成（完整类注册表：C3 线性化与特征、收窄、端口覆盖与分派、类型选择器、类扩展；提交时的引用完整性、关系 on_end 规则、基数、扩展检查；CLS 176、OBJ 10、ERR 1 个向量全部通过，内存与 PostgreSQL 结果一致；STD-FND-026 的 9 个向量需 L1 求值器，移到第 5 步）；4b 已完成（DEC-051：提交记录带 commit_id 与 HLC 事务时间、分支元数据与生命周期、父线视图与隐藏标记、带解决记录的合并与校验、合并预览、变更集、标签与发布、revert、进程记录；VER 122/122 通过，内存与 PostgreSQL 一致）；4c 已完成（DEC-052：每次读取带 `evaluated_at`、`follow` 按源对象坐标解析引用、范围查询与历史、派生记录与影响集、幂等请求、outbox 与 BecameEffective 事件、批量进程、AEAD 解密；1,227 个向量中 841 个在内存与 PostgreSQL 上通过且结果一致；剩余 386 个属第 5、6 步）；4d-1 已完成（DEC-053：迁移 0003 替换过渡布局——提交按内容 ID 键入、UUID 标识、timestamptz 有效时间与粒度列、类名加声明内容 ID、带区间的 `head_index` 取代 `object_state`、提交记录携带写集并由校验器重建索引；841 个向量在 PostgreSQL 上与内存一致，存储套件 20/20）；4d-2 已完成（DEC-054：迁移 0004 让分支元数据、标签、进程、变更集、派生记录、outbox、幂等记录与定时器各有其表；效果、幂等记录、派生记录与定时器随提交在刷写事务内写入；VAE 权威名与主体成为引擎作用域；`parse_uri`、`resolve`、`parse_did`、`key_did`；PostgreSQL 上的阶段性签名运行器报告 `verify/reports/exit-1-02/`；1,227 个向量中 869 个在内存与 PostgreSQL 上通过且一致）。第 4 步完成 |
| 5 | 运行时：L1 表达式、L2 生产化（内存计量、加权计量、内核小数）、指令集、journal、重放；逃逸用例扩到 300 个以上 | EXIT-1-10 | 已完成（DEC-055，分 5a–5d 四个 PR）：5a 已完成（新 crate `ubos-expr`：L1 词法、语法、静态类型检查、树遍历求值器（空值三值语义、操作预算与条目上限）、ABI 1.0 函数库；自定义值类型经注册表钩子做 normalize/validate（STD-FND-026）；适配器操作 `check_expr`、`eval_expr`、带 `value_types` 的 `validate_value`，以及条件查询 `query`（STD-EXPR-010/011，暂置于 `ubos-dvm`，PH-1b 建 `ubos-query` 时迁出）；1,227 个向量中 1,009 个在内存与 PostgreSQL 上通过且一致；字节码与编译缓存（DSN-DVM-610）留到 5d 的性能工作）；5b 已完成（DEC-056：ABI 1.0 指令目录、错误注册表与 profile 表由标准条文生成并嵌入内核（`gen_isa_json.py`，与参考实现同源）；`ubos-runtime` 实现网关（STD-CTX-003）、执行上下文（STD-CTX-001/002）、预算、计量与确定性熵（STD-CTX-004…006、-010）及宿主层程序解释器（STD-ISA-007）；各指令的存储侧由引擎以 `Host` 特征提供（AR-031）；`execute` 按 STD-TXN-023 自动重试，journal、metering、metering_total、instructions、profile_limits 操作；1,227 个向量中 1,127 个在内存与 PostgreSQL 上通过且一致；剩余 100 个属 5c（验证门、重放、BPU，43 个）与第 6 步（57 个））；5c 已完成（DEC-057：验证门 `verify_asset`——静态分析、类型检查、能力声明、测试、发布者签名（STD-CTX-023/024），记录由节点签名，`ubos-crypto` 加最小 Ed25519 节点密钥；journal 与计量记录以内容 ID 存为 chunk 并由 `process_record` 引用（迁移 0005）；`replay` 按 journal 重放并报告首个分歧（STD-CTX-012）；`bpu_descriptor`、`bpu_state`、`verify_bpu`（STD-BPU-001/002/011）；`node_did`；1,227 个向量中 1,165 个在内存与 PostgreSQL 上通过且一致；剩余 62 个属第 6 步）；5d-1 已完成（DEC-058：L2 宿主生产化——`ubos-runtime::l2` 取代 PH-0 spike，每个嵌套层一个工作线程上的 Rhai 引擎，指令经 5b 网关以 `sys.<family>.<op>` 调用，只读 `ctx`、类型化句柄、内核小数与货币作为脚本数值类型（W-5），按操作数、线程 CPU 时间、墙钟与每次执行内存（计数分配器）计量（W-2/W-3），编译脚本按资产版本缓存；一致性适配器默认关闭 L2，`forge conform --l2` 开启；一致性结果不变 1,165/1,227）；5d-2 已完成（逃逸套件扩到 382 个用例、运行器 `ubos-l2-suite`，每例两次运行全部拦截，报告 `verify/reports/exit-1-10/`，EXIT-1-10 达成；W-4 实测后解决：内存计量下关闭引擎尺寸检查、`pad` 按调用限界，65,536 次 push 由 2.4 s 降到 45.27 ms；NR-PERF-010 在工作站上 L1 5.53 µs、缓存后 1.28 µs、L2 热调用 p95 848.8 µs；W-2…W-5、W-12 关闭）。第 5 步完成 |
| 6 | 证明（签名、包含证明）、便携导出/导入、校验库 | EXIT-1-12 | 已完成（DEC-059，分 6a–6c 三个 PR）：6a 已完成（每条提交记录在 flush 时由版本层用节点密钥签名，节点记录移入存储层 `ubos_store::node`；`ubos-crypto` 加 DAT-Signature 校验（按 DID 文档密钥历史、半开有效期）；新 crate `ubos-proof`（只依赖 `ubos-tree`、`ubos-crypto`，不读存储）提供 `inclusion_proof`、`verify_inclusion`、`commit_check`、`verify_consistency`；`rotate_node_key` 以最新 tx_time 之后的时钟读数切换密钥，`did_document`、`commit_record`；`Store::branch_names`；1,227 个向量中 1,203 个在内存与 PostgreSQL 上通过且一致，剩余 24 个属 6b）；6b 已完成（`prove`、`consistency_proof`、`tenant_root` 读存储；`export_archive`/`import_archive`（STD-PROOF-070，`ubos.archive.v1`）在 `ubos-version`，纯校验 `verify_archive` 在 `ubos-proof`，导入按父序经内核 flush 写入并重算每个状态根与头根；1,227 个向量全部在内存与 PostgreSQL 上通过且一致）；6c 已完成（独立校验器 `ubos-verify` 在 `verify/ubos-verify/`，Apache-2.0，只依赖 `ubos-proof`，命令行与库：`inclusion`、`consistency`、`signature`、`archive`，输出逐项结构化报告；`ubos-roundtrip` 工具在同一 PostgreSQL 库的三个 VAE 范围间导出、校验、导入、比对（各分支状态与信息、租户根、标签、账簿、每个对象的读取）、再导出再导入；报告 `verify/reports/exit-1-12/`：3 个 VAE 全部一致，归档全部通过校验，EXIT-1-12 达成；CI 每次推送跑小数据集）。第 6 步完成 |
| 7 | 进程内宿主服务、Forge 的 check/types/test/conform、1a 的 10 个场景 | EXIT-1-02、EXIT-1-13 | 已完成（DEC-060，分 7a–7c 三个 PR）：7a 已完成（`ubos-dvm::host`：CTR-002 宿主服务特征与 CTR-001 宿主 API `Vae`；进程种子来自宿主熵并随进程记录，`call.connector`/`ask.model` 先入 journal 再使用、重放不碰宿主；类声明随 VAE 持久化、`open_existing`。新 crate `ubos-node`：租户注册、密钥层级（根 KEK → 包裹的租户 KEK → HKDF 派生的数据密钥与节点签名密钥，私钥不落库）、节点签名器挂到存储、blob 表、按句柄的密钥、连接器白名单、事件日志、带租约与 poison 的作业、脱敏遥测；迁移 0006；SUITE-NOD-HOST 12/12 并入 CI；一致性不变 1,227/1,227）；7b 已完成（Forge 项目模型 `forge.toml` + `model/`、`logic/`、`seed/`、`tests/` 的 YAML 源与 `forge init` 骨架；`forge check` 流水线每个阶段一次全报（注册表、验证门分析器、端口实现、种子装载）；`forge types` 生成 TypeScript 与 Rust 类型并带分支与模型哈希；`forge test` 在确定性服务下的内存 VAE 上跑 scenario/unit/port/property 四类测试，输出 JSON 与 JUnit XML、可复现的根与覆盖率；每个资产的验证记录由校验者密钥签名；`--json` 与退出码；证据 `verify/reports/forge-7b/`，并入 CI）；7c 已完成（十个 gate-1a 场景写成 Forge 项目 `verify/scenarios/`，经宿主 API 与确定性服务在内存和 PostgreSQL 上运行，22 个测试（主流程与备选流程）全部通过，报告 `verify/reports/exit-1-13/`，EXIT-1-13 达成；`forge test` 加每测试独立夹具、模拟连接器、`released_effects` 与 `concurrent` 步骤；`revert` 接受 `resolutions` 并连同批处理子提交一起回退；最终签名的 EXIT-1-02 报告：1,227/1,227 在 PostgreSQL 与内存上通过，`verify/reports/exit-1-02/`；SCN-003 第 3 步、SCN-002 后置条件 2、SCN-018 A1 属 1b 并已在报告中标注）。第 7 步完成 |
| 8 | 验证收口：属性、模拟（连续 30 晚）、故障注入、三平台重放、覆盖率与变异分数 | EXIT-1-04、05、06、08、11 | 已完成，夜晚条件累计中（DEC-061，分 8a–8c 三个 PR）：8a 已完成（VER-PROP-0100 五个新属性加 PH-0 四个属性各 100 万例 0 失败，`verify/reports/exit-1-04/`，EXIT-1-04 达成；向量、场景、属性样本作为集成测试成为变异测试载体；核心 11 个 crate 行覆盖率 90.1%，抽样变异分数 89.9% on a workstation sample of 237 viable mutants (213 detected)，`verify/reports/exit-1-11/`，完整分数由 8c 的 nightly 分片累计）；8b 已完成（确定性模拟器 `ubos-sim`：2–4 节点共享存储、1–3 分支、争用对象与账本键、可串行化事务、时钟跳变、刷写中止与刷写后崩溃，检查线性历史、无丢失已确认提交、无缝序列与 Elle 式可串行化历史，1,000,000 个种子 0 违规，`verify/reports/exit-1-05/`，EXIT-1-05 的百万种子条件达成，30 个清洁夜晚由 8c 的 nightly 累计；故障注入套件 `ubos-fault`：PostgreSQL 上 4 个工作进程带 fsync 确认日志，SIGKILL 与断电（服务器立即停机后重启）交替，每次恢复后核对确认日志、cseq 稠密与内核校验器，10,000 次事件（5,000 次 SIGKILL、5,000 次断电），31,732 个已确认提交，丢失 0、部分提交 0、校验器发现 0，`verify/reports/exit-1-06/`，EXIT-1-06 在工作站达成）；8c 已完成（`ubos-replay` 按种子生成 10,000 个流程——宿主程序与 L2 脚本，覆盖精确十进制与舍入、货币分摊、日历与时区、Unicode 大小写与 NFC、排序、规范格式、种子熵与时钟——记录结果哈希 `verify/replay/expected.json`；Linux x86-64 上 10,000/10,000 一致、1,000/1,000 日志重放一致，`verify/reports/exit-1-08/`；夜间工作流 `nightly.yml`：每晚 10,000 模拟种子、属性 10 万例、每晚一个变异分片（32 晚轮完）、语料在 Linux x86-64/ARM64 与 macOS ARM64 上检查，夜晚记录 `verify/reports/exit-1-05/nights.md` 在分支 `nightly-log` 累计）。第 8 步除夜晚累计条件（30 个清洁夜晚、三平台）外完成 |
| 9 | Gate 1a 验收：ENV-REF-SERVER 上的基准、外部评审、验收 DEC | EXIT-1-09 | 进行中（DEC-062，分 9a–9c）：9a 已完成（验收记录 UBS-PHA-1 与中文验收包、参考环境操作手册 `ph1-gate1a-runbook-zh.md`；`ubos-bench` 补齐分支创建、合并预览、asof 读与背景负载；发现 1：合并预览随对象数线性增长，租机前改为树 diff）；9b（合并预览修正、租机运行 W-1 与本手册、外部评审、验收决定）、9c 已完成（gate 1b 分步计划 DEC-063，见下表）；9b 后半（租机运行 W-1 与本手册、外部评审、验收决定）待业主租机 |

W-1（从 PH-0 结转的存储基准，DEC-035）：业主租到主机时随时进行，须在第 3 步存储布局定型之前完成。

## 1b. Gate 1b 步骤（DEC-063，2026-10-14 确认）

编号接 gate 1a；每步按各自的 DEC 再拆成 a/b/c 等 PR。顺序：标准先行 → 内核能力（进程内绑定上用向量与场景验证）→ Server 与服务 → 客户端与场景 → 收口与验收。第 9 步后半（租机复跑、外审、DEC-062 决定）与第 10 步并行。

| 步骤 | 内容 | 退出标准 | 状态 |
|---|---|---|---|
| 10 | BPA 1.0-rc2：Governance（LGS）与 Protocol（PROTO）条款与向量，MUST 覆盖 100%，标准先于代码；发布范围规则扩到 gate 1b；用单独的 DEC 选定 Cedar 兼容策略库（DEC-021） | EXIT-1-14（标准部分） | 已完成（DEC-064，分 10a–10e）：10a 已完成（rc2 按规则开启：197 条条款，新增 26 条——LGS 10、PROTO 7、Core 9；`regen_all.sh` 改为构建 rc2，rc1 保持冻结；覆盖率 146/165；策略库选型 DEC-065：`ubos-authz` 嵌入 `cedar-policy` crate）；10b 已完成（DEC-066：8 条 LGS 条款澄清，参考模型 `bpa/sheets.py`，创世类 `GovernanceSheet`/`PropertyDecl`，操作 `parse_sheet`/`check_sheet`/`specificity`/`cascade`/`explain_cascade`，错误码 `RULE.PARSE_ERROR`，157 条向量 VER-CONF-4800…5031，覆盖率 153/165）；10c 已完成（DEC-067：`validations`/`invariants`/`decisions` 内核属性，提交期强制、校验预览 `validate`、决策 `decide` 与端口前求值，端口声明与覆盖策略检查，查询文本形式 `parse_query`，指令 `read.explain`/`run.validate`/`search.query`；rc1 两条向量按 STD-CONF-030 的替代机制 `superseded.txt` 被 rc2 取代；116 条向量，覆盖率 159/165）；10d 已完成（DEC-068：进程内绑定操作 `ubtp`、信封规则与 `INT.PROTOCOL_ERROR`、握手与进程内凭证、mutate 的幂等/base/预览/pending、query 各类与坐标、订阅游标与 emit、保留窗口与确认窗口、cancel/ping；88 条向量，覆盖率 165/165）；10e 已完成（DEC-069：发布说明定稿并列入 manifest——197 条条款、MUST 165/165、向量 1,586 条、38 个 Schema，rc1 两条被取代向量排除在外；VER-INSP-9004 记录覆盖率检查；合并后由您签标签 `bpa-v1.0.0-rc2`，签后冻结）。第 10 步完成；EXIT-1-14 标准部分随标签达成，内核部分随第 11–14 步 |
| 11 | 治理表与规则：表解析/编译、级联与解释、校验与不变量、带否决的策略决策（RULE-01/02/03/05）；NR-PERF-009 | EXIT-1-14（内核部分） | 已完成（DEC-070，分 11a–11d）：11a 已完成（crate `ubos-rules`：文法解析与规范打印、属性注册表、作者期检查、特异度、级联秩与重叠分析；操作 `parse_sheet`/`check_sheet`/`specificity`；76 条纯操作向量通过并纳入一致性测试）；11b 已完成（创世类 `GovernanceSheet`/`PropertyDecl`、提交期派生 AST 与表集检查、按 asof 选表、匹配与级联、操作 `cascade`/`explain_cascade`、指令 `read.explain`/`read.effective declarations`；一致性测试声明 STD-LGS-001…023，157 条向量通过）；11c 已完成（提交强制 `read_only`/`field_required`/`audit_level`/`write_policy`/`field_default`，类、扩展与级联的校验和按作用域求值的不变量，一次报告全部违规并受 `max_violations` 限制，预览 `validate`/`run.validate`，命令端口前的决策 `decide` 与日志记录，`read.explain` 的 validation/decision 类，STD-CLS-003/CLS-031/BPU-003，上下文 `role`；一致性测试声明 rc2 已实现条款共 1,475 条向量，PostgreSQL 上全部通过）；11d 已完成（编译表缓存与按类分桶的选择器索引、归一化声明缓存、重叠分析按类对记忆化、索引路径只按类读表、提交证据只读 commit_id、内存存储按类索引；`ubos-bench sheets`/`invariants`；工作站 10,000 个块 20 个候选：warm p95 0.41 ms/对象，5,000 对象时 0.592 ms，目标 ≤ 1 ms 达标；200 条不变量只求值触及的作用域）。第 11 步完成 |
| 12 | 流程：生命周期、端口、守卫、流程对象（FLOW-01…03） | — | 进行中（DEC-071，分 12a–12c）：12a 已完成（开启 BPA 1.0-rc3；新条款 STD-CLS-004 生命周期声明与创世类 `Lifecycle`、STD-ISA-036 `read.actions` 与适配器操作 `actions`；澄清 STD-ISA-017/050、STD-TIME-040、STD-BPU-011；参考模型 `bpa/flow.py`，48 条向量）；12b 已完成（crate `ubos-flow` 纯引擎 + 内核：创世类 `Lifecycle`、版本信封中的状态、`commit.transition`（守卫、退出/进入效果、必填字段）、`run.port` 可用性与守卫、`read.actions`/`actions`、rule binding 的守卫与生命周期版本、BPU 生命周期检查；一致性测试声明 12a 条款，内存 1,531 条通过，PostgreSQL 全部通过，仅余第 13 步的 9 条查询向量）；12c 待做（流程对象状态机、rc3 发布与 DEC-072、签标签 `bpa-v1.0.0-rc3`） |
| 13 | 授权与查询：策略引擎（IAM-04）、条件查询、历史查询、一致分页、视图载荷与确定性、覆盖层、审计轨迹（QRY-01/05/07、TEN-03、AUD-01） | EXIT-1-17（MET-PERF-016） | 待做 |
| 14 | 单节点 Server 与本地身份：请求管线、WebSocket/HTTP/进程内三种绑定、握手、mutate、query 与分页、订阅、断线续传、背压（INT-01）；本地凭证、会话、主体与密钥、API key、安全事件日志、密钥库（IAM-01）；SUITE-NOD-PROTO、-AUTH | EXIT-1-15（NOD 部分） | 待做 |
| 15 | 事件、作业、调度：outbox 释放、事件日志、订阅、处理器即流程、生效调度、死信、租约、重试、计划（EVT-01…05）；SUITE-NOD-JOBS、-EVENTS | EXIT-1-17（MET-PERF-017、NR-PERF-012）、NR-DUR-006 | 待做 |
| 16 | 运维与配置：安装包与安装指南、bootstrap、版本化配置（热重载、≥200 个非法配置拒绝）、OpenTelemetry、健康与自检、诊断包、降级模式、备份与 PITR、租户注册与隔离强制、内嵌 genesis Buks（OPS-04/05/09、TEN）；SUITE-NOD-OPS 在 ENV-REF-SMALL 上验证 | EXIT-1-18 | 待做 |
| 17 | Rust 与 TypeScript SDK 0.2（DEV-06）、`forge dev`、工程 UI 原型（仅供验收）；SUITE-SDK-PROTO、-NET | EXIT-1-15（SDK 部分） | 待做 |
| 18 | 13 个 PH-1 场景经 UBTP 在单节点 Server 上运行（SCN-001、009、017 新增，10 个 1a 场景复跑），场景运行报告 | EXIT-1-19 | 待做 |
| 19 | 安全与验证收口：隔离套件、日志与追踪的密钥扫描、安全事件覆盖、隐私与分类（NR-PRIV、CR-GDPR）、边缘基准（SUITE-NOD-BENCH）、追溯报告；安全报告 | EXIT-1-16、-17、-20 | 待做 |
| 20 | Gate 1b 验收：场景、安全、运维报告，验收委员会加运维负责人，验收 DEC | 全部 | 待做 |

## 2. 需要业主做的事

| 时间 | 事项 |
|---|---|
| 第 1f 步合并后 | 用自己的密钥创建带签名的标签 `bpa-v1.0.0-rc1`（见第 3 节）；同时补签 `bpa-v0.1.0`（远端目前没有这个标签） |
| 尽早 | 租用 ENV-REF-SERVER，按 `docs/reports/ph0-step5-runbook-zh.md` 补跑 W-1，同时请外部评审人评审 PH-0 |
| 第 8 步前 | 确认 GitHub Actions 额度够用（每晚任务、ARM64 Linux 和 macOS runner）；不够时由 AI 报告，业主决定加额度或自托管 |
| 第 9 步 | 再次租用 ENV-REF-SERVER 做 gate 1a 基准；外部评审人参加验收 |
| gate 1b 第 16 步前 | 租用 ENV-REF-SMALL（第 16、18、19 步使用）；第 19 步的边缘基准仍用 ENV-REF-SERVER |
| 第 10e 步合并后 | 用自己的密钥创建带签名的标签 `bpa-v1.0.0-rc2`（做法同第 3 节，manifest 的 `content_id` 写入标签说明） |

性能需求是 Must，不能结转（UBS-PH-00 §3.7）。如果到 gate 1a 仍租不到主机，只能用 DEC 重新分期。

## 3. 签名 BPA 1.0-rc1 标签的操作（业主）

第 1f 步合并后，在本地仓库执行：

```sh
git fetch origin main
CID=$(git show origin/main:standard/release/1.0-rc1/manifest.json | python3 -c "import json,sys;print(json.load(sys.stdin)['content_id'])")
git tag -s bpa-v1.0.0-rc1 origin/main -m "BPA 1.0.0-rc1 (manifest: standard/release/1.0-rc1/manifest.json, content_id $CID)"
git push origin bpa-v1.0.0-rc1
```

- 标签要打在第 1f 步的合并提交上；如果合并后 `main` 又有新提交，把 `origin/main` 换成那个合并提交的哈希。
- BPA 0.1 的标签 `bpa-v0.1.0` 在远端还没有。与 0.1 发布清单逐文件一致的提交是 `8b2e4e0`（PR #6 的合并提交），可以这样补签：

```sh
git tag -s bpa-v0.1.0 8b2e4e0 -m "BPA 0.1.0 (manifest: standard/release/0.1/manifest.json, content_id sha256:6bd7ec59db8815e54c7b802d570727f047d27e9e5e072d1026bb7fcb57d3f0f4)"
git push origin bpa-v0.1.0
```

- 使用 SSH 密钥签名时，先执行一次 `git config gpg.format ssh` 和 `git config user.signingkey <公钥文件>`。
