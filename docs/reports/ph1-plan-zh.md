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
| 5 | 运行时：L1 表达式、L2 生产化（内存计量、加权计量、内核小数）、指令集、journal、重放；逃逸用例扩到 300 个以上 | EXIT-1-10 | 进行中（DEC-055，分 5a–5d 四个 PR）：5a 已完成（新 crate `ubos-expr`：L1 词法、语法、静态类型检查、树遍历求值器（空值三值语义、操作预算与条目上限）、ABI 1.0 函数库；自定义值类型经注册表钩子做 normalize/validate（STD-FND-026）；适配器操作 `check_expr`、`eval_expr`、带 `value_types` 的 `validate_value`，以及条件查询 `query`（STD-EXPR-010/011，暂置于 `ubos-dvm`，PH-1b 建 `ubos-query` 时迁出）；1,227 个向量中 1,009 个在内存与 PostgreSQL 上通过且一致；字节码与编译缓存（DSN-DVM-610）留到 5d 的性能工作）；5b 已完成（DEC-056：ABI 1.0 指令目录、错误注册表与 profile 表由标准条文生成并嵌入内核（`gen_isa_json.py`，与参考实现同源）；`ubos-runtime` 实现网关（STD-CTX-003）、执行上下文（STD-CTX-001/002）、预算、计量与确定性熵（STD-CTX-004…006、-010）及宿主层程序解释器（STD-ISA-007）；各指令的存储侧由引擎以 `Host` 特征提供（AR-031）；`execute` 按 STD-TXN-023 自动重试，journal、metering、metering_total、instructions、profile_limits 操作；1,227 个向量中 1,127 个在内存与 PostgreSQL 上通过且一致；剩余 100 个属 5c（验证门、重放、BPU，43 个）与第 6 步（57 个））；5c 已完成（DEC-057：验证门 `verify_asset`——静态分析、类型检查、能力声明、测试、发布者签名（STD-CTX-023/024），记录由节点签名，`ubos-crypto` 加最小 Ed25519 节点密钥；journal 与计量记录以内容 ID 存为 chunk 并由 `process_record` 引用（迁移 0005）；`replay` 按 journal 重放并报告首个分歧（STD-CTX-012）；`bpu_descriptor`、`bpu_state`、`verify_bpu`（STD-BPU-001/002/011）；`node_did`；1,227 个向量中 1,165 个在内存与 PostgreSQL 上通过且一致；剩余 62 个属第 6 步） |
| 6 | 证明（签名、包含证明）、便携导出/导入、校验库 | EXIT-1-12 | 未开始 |
| 7 | 进程内宿主服务、Forge 的 check/types/test/conform、1a 的 10 个场景 | EXIT-1-02、EXIT-1-13 | 未开始 |
| 8 | 验证收口：属性、模拟（连续 30 晚）、故障注入、三平台重放、覆盖率与变异分数 | EXIT-1-04、05、06、08、11 | 未开始 |
| 9 | Gate 1a 验收：ENV-REF-SERVER 上的基准、外部评审、验收 DEC | EXIT-1-09 | 未开始 |

W-1（从 PH-0 结转的存储基准，DEC-035）：业主租到主机时随时进行，须在第 3 步存储布局定型之前完成。

## 2. 需要业主做的事

| 时间 | 事项 |
|---|---|
| 第 1f 步合并后 | 用自己的密钥创建带签名的标签 `bpa-v1.0.0-rc1`（见第 3 节）；同时补签 `bpa-v0.1.0`（远端目前没有这个标签） |
| 尽早 | 租用 ENV-REF-SERVER，按 `docs/reports/ph0-step5-runbook-zh.md` 补跑 W-1，同时请外部评审人评审 PH-0 |
| 第 8 步前 | 确认 GitHub Actions 额度够用（每晚任务、ARM64 Linux 和 macOS runner）；不够时由 AI 报告，业主决定加额度或自托管 |
| 第 9 步 | 再次租用 ENV-REF-SERVER 做 gate 1a 基准；外部评审人参加验收 |
| gate 1b | 租用 ENV-REF-SMALL |

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
