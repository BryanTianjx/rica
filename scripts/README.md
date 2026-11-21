# scripts/ — 辅助脚本清单

依据 SPE-BAS-004 §13(软件项目辅助脚本工程规范 R1.1)建立。
每个正式脚本至少说明:用途、运行环境、依赖工具、输入参数、输出结果、执行示例、退出码、是否修改文件、风险等级。

## 运行环境

- Python 脚本:Python 3.11+(契约/审计脚本在 Windows 与 WSL 双环境可用)。
- 依赖 Python 基线 venv 的脚本(标注 ⚠️)需先激活 `O:\ica\backend\.venv`
  (WSL:`/mnt/o/ica/backend/.venv`),其余脚本不依赖任何 Python 环境。
- Bash 脚本为薄包装,实现均在同名 `.py` 中,保持既有调用方式不变。
- 统一退出码约定:`0` 成功;`1` 通用失败;`2` 参数错误;`5` 文件/数据错误;
  `10` 验证失败(对拍不一致/清单不符)。各脚本内如无特殊说明,遵循本约定。

## 质量门禁(check 系列,L1 只读检查)

| 脚本 | 类型 | 用途 | 修改文件 | 风险 |
|---|---|---|---|---|
| `check.py` / `check.sh` | 质量 | 一条命令跑全部门禁:依赖分层→fmt→clippy→build→test | 否 | L1 |
| `check_scope.py` / `check_scope.sh` | 项目检查 | 两条范围门禁:68 条契约端点表 + 基线 schema 对象集(对照冻结镜像) | 否 | L1 |

## 契约对拍(contract 系列,L2 本地生成文件)

| 脚本 | 类型 | 用途 | 修改文件 | 风险 |
|---|---|---|---|---|
| `contract_capture.py` | 数据处理 | 对运行中的服务(基线或 Rust)回放固定场景,捕获响应+落库行,输出一份 JSON | 写指定输出 JSON | L2 |
| `contract_diff.py` | 数据处理 | 结构化 diff 两份 capture(归一化动态字段,显式声明差异) | 否 | L1 |
| `contract_verify.py` / `contract_verify.sh` | 测试管理 | 一条命令:种子库→起两个服务→capture→diff | /tmp 下的临时副本 | L2 |
| `seed_contract_db.py` | 测试数据 | 生成契约对拍种子数据库 | 写指定 .db | L2 |
| `peek_seed.py` | 开发工具 | 检视种子库内容 | 否 | L1 |

## 基线与端点盘点(baseline 系列,L1/L2)

| 脚本 | 类型 | 用途 | 修改文件 | 风险 |
|---|---|---|---|---|
| `endpoint_reconciliation.py` | 项目检查 | 端点调和矩阵(端点计数的唯一权威来源,P0-01 修正) | 否 | L1 |
| `check_endpoint_map.py` | 项目检查 | 校验契约文档端点表与真实路由一致(读基线 router,非正则) | 否 | L1 |
| `export_baseline_schema.py` ⚠️ | 数据处理 | 从基线库只读导出 `sqlite_master` DDL → `ica-baseline.sql` | 写 schema SQL | L2 |
| `compare_baseline_schema.py` ⚠️ | 项目检查 | 只读比对导出 DDL 与真实库的表/索引集合(AC2 验收) | 否 | L1 |
| `gen_column_defaults.py` ⚠️ | 开发工具 | 从 SQLAlchemy 模型生成 Rust 列默认值表(import 部分快照必需) | stdout 输出 Rust 代码 | L2 |

⚠️ = 需要 Python 基线 venv(依赖 `app.models` 导入)。

## 测试资产生成(test 系列,QA-006 自动化)

| 脚本 | 类型 | 用途 | 修改文件 | 风险 |
|---|---|---|---|---|
| `generate_test_catalog.py` | 测试管理 | 扫描 Rust 测试函数生成用例目录与追溯矩阵草稿(SPE-QA-006) | 写 docs/testing/ 下生成的目录文件 | L2 |

## 约定

1. 所有脚本默认只读或写入明确指定的输出;破坏性操作不存在(GOV/004 §8)。
2. 脚本不自动执行 `git commit/push/merge`(SPE-BAS-004 §8.4)。
3. 新增脚本必须:标准文件头 + `main()->int` 入口 + argparse + pathlib + 本清单登记。
4. `__pycache__/` 为运行产物,不入库(.gitignore 已覆盖)。
