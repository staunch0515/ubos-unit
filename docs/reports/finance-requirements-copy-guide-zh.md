# 财务需求复制到其他仓库的操作说明

> 适用版本：财务需求 1.0（标签 `fin-req-v1.0`，DEC-029）。

## 1. 原则

- `docs/finance-requirements/` 是一个自包含的文件夹：不提 UBOS，不提 Java，也不引用文件夹之外的任何路径，生成和校验工具在它自己的 `tools/` 里。
- 权威来源只在 `staunch0515/ubos-unit`。别的仓库只放**某个标签版本的原样副本**，在那里不修改。
- 需求要改时，只在 `ubos-unit` 里改，发新版本（例如 `fin-req-v1.1`），再按下面的步骤重新复制一次。

## 2. 第一次复制（在 Java 仓库里做）

前提：本机装有 git 和 Python 3.8 或以上版本（校验工具只用标准库）。

```sh
# 1) 取出冻结版本（只取这个标签，不带历史）
git clone --depth 1 --branch fin-req-v1.0 https://github.com/staunch0515/ubos-unit.git /tmp/fin-req-v1.0

# 2) 进入 Java 仓库，复制整个文件夹（目标路径可以自己定，这里用 docs/finance-requirements）
cd <Java 仓库目录>
mkdir -p docs
cp -r /tmp/fin-req-v1.0/docs/finance-requirements docs/

# 3) 校验副本完整：必须输出 errors: 0，且 git 看不到任何文件被改动
git add docs/finance-requirements
( cd docs/finance-requirements && sh tools/regen.sh )
git status --short docs/finance-requirements     # 应当没有输出

# 4) 提交
git commit -m "Add finance requirements fin-req-v1.0 (copy, do not edit)"
```

不想用命令行的话，也可以在 GitHub 网页上打开 `ubos-unit` → Tags → `fin-req-v1.0` → 下载 zip，解压后把 `docs/finance-requirements` 整个文件夹放进 Java 仓库，再执行上面第 3、4 步。

如果 Java 仓库也由 Claude 维护，建议在它的 `CLAUDE.md` 里加一句：
`docs/finance-requirements/ is an unchanged copy of fin-req-v1.0 from staunch0515/ubos-unit; never edit it.`

## 3. 升级到新版本

```sh
git clone --depth 1 --branch fin-req-v1.1 https://github.com/staunch0515/ubos-unit.git /tmp/fin-req-v1.1
cd <Java 仓库目录>
rm -rf docs/finance-requirements
cp -r /tmp/fin-req-v1.1/docs/finance-requirements docs/
( cd docs/finance-requirements && sh tools/regen.sh )
git add -A docs/finance-requirements
git commit -m "Update finance requirements to fin-req-v1.1"
```

变更内容见新版本 `99-change-log.md`。两边按约定日期同时切换，切换之后的工作记录填新的 `req_version`。

## 4. 开始记录工作项（两边相同）

记录规则写在 `40-comparison-protocol.md` 第 2 节，要点如下：

1. 把 `templates/work-items.csv` 和 `templates/defects.csv` 复制到自己仓库里的一个记录文件夹（例如 `finance-work/`），不要放进需求文件夹。至少每周提交一次。
2. 系统代码：UBOS 填 `U`，Java 填 `J`。第一行的 `req_version` 填 `1.0`。
3. 每人、每个工作项、每天记一行工时，按 0.25 小时为单位，当天记。工作项就是需求编号（FIN-GL-003 等）、场景编号、变更请求（CR-A…CR-F），或者 `TASK-名称`。
4. 每次提交的说明以工作项编号开头（例如 `FIN-GL-003: posting rules`）。代码规模事后从 git 历史里统计，不用手填。
5. 发现缺陷就在 `defects.csv` 记一行；修复缺陷的工时记为 `defect-fix`。
6. 已记的行不删除。记错了就补一行负工时，并写明原因。
