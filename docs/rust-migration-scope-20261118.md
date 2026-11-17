# 范围与基线漂移分析（2026-11-18）

**性质**：计划增补（plan addendum）。不改 W1/W1.5 已验收内容，不改契约文档 §7.1。
本文是 [`rust-migration-contract.md`](rust-migration-contract.md) §1.1「Scope and
exclusions under baseline drift」的完整依据；W1.6 卡（G4）把它落进本仓。

**一句话结论**：ICA 主仓在 `d39e321` 之后新增了**浏览器子系统**（6 个提交、34 条
`/api/browser/v1` 路由、10 张 `browser_*` 表），但**既有的 68 条业务路由集合逐条未变**
——已用 live `app.routes` 双向集合比对证明。因此 Rust 迁移的波次计划（W2–W6 的
11/10/7/15/8 条）**仍然成立**，而浏览器子系统按 ICA 自身架构文档**明确不属于 Rust
迁移范围**；门禁已按本文件 §5 加固（W1.6 落地）。

---

## 1. 三个检出的真实关系（先看这张表）

| 检出 | HEAD | 相对关系 | 工作树 | `/api` 路由（pairs） | 模型表 |
| --- | --- | --- | --- | --- | --- |
| `/mnt/o/ica`（契约权威，只读） | `d39e321`（2026-11-16 21:32） | = `origin/main`，**落后本地 6 个提交** | 干净，`ica.db` sha256 `466aa3fc…18e1` | **68** | 20（DB 对象：19 业务/身份表 + `alembic_version`） |
| `/home/bryan/ica`（主检出，ext4） | `d96bb05`（2026-11-17 23:06） | `ahead 6`，**未 push** | **脏**（BROWSER-04 未提交） | **102**（= 68 + 34 browser） | metadata **29** = 19 + 10 `browser_*` |
| 同上工作树（含未提交改动） | `d96bb05` + 未提交 | BROWSER-04 进行中 | 9 改 + 7 新增 | **108**（+6） | metadata **31** = 19 + 12 `browser_*` |

**纪律**：`/mnt/o/ica` 是**冻结的契约镜像**，不是「跟着 main 走的工作副本」。
一旦它被 `git pull` / rsync 更新到含浏览器子系统的提交，本仓所有门禁的语义都会变
（见 §5）。本仓不得修改它，只能读并记录 sha。重锚必须走契约文档 §1.1 的四步程序。

## 1.1 数据库对象算术（全部实测，2026-11-18，W1.6）

| 口径 | 表 | 索引 | 说明 |
| --- | --- | --- | --- |
| 基线 DDL 导出 `ica-baseline.sql` | **20** | **66** | 19 张业务/身份表 + `alembic_version`；与冻结基线库比对 → IDENTICAL |
| `d96bb05` alembic head（副本实测） | **30**（`browser_*` 10） | **107**（browser 上 41） | 排除 `browser_*` 后 20 × 66 → IDENTICAL |
| `d96bb05` SQLAlchemy metadata | **29**（`browser_*` 10，其它 19） | — | 与模型注册一致（`app.models`） |
| 工作树 alembic head（含未提交 `501a807efa03`） | **32**（`browser_*` 12） | **121**（browser 上 55） | 排除后 20 × 66 → IDENTICAL |
| 工作树 SQLAlchemy metadata | **31**（`browser_*` 12，其它 19） | — | `browser_candidate_extractions` + `_revisions` 两张 |

实测方法：`git archive d96bb05` 导出 + 主检出 venv 跑 `alembic upgrade head` 到
`/tmp` 副本，`sqlite_master` 计数；metadata 用 `Base.metadata.tables`。ICA 两个检出
全程只读。注意：主检出的开发库 `ica.db` 是**旧迁移状态**（20 表），不能代表 HEAD 的
schema，一律以「副本上 `alembic upgrade head`」为准。

---

## 2. ICA 主仓这 6 个提交到底加了什么

时间线（全部 2026-11-17 晚间，BROWSER-00 → BROWSER-03 一条线）：

| 提交 | 时间 | 内容 |
| --- | --- | --- |
| `c4a1c6c` | 22:47 | BROWSER-00 架构基线：`docs/architecture/browser-{architecture,api-contract,security-model,data-model,implementation-breakdown}-r1.md` |
| `e7a8ede` | 22:48 | 进程外浏览器运行时（`backend/browser_bridge/`，Playwright 惰性 import）+ MV3 扩展（`extension/`） |
| `a52d269` | 22:48 | 内置浏览器 + 权限/策略 + AI Agent 应用层（`application/browser_{service,policy,planner,hybrid,tasks}.py`） |
| `6e1f86e` | 22:49 | 外部浏览器连接器 + Native Messaging（`browser_connector/`、`browser_native_host/`） |
| `d6d15f3` | 22:49 | 测试套件 + fixtures + 证据脚本（16 个 `test_browser_*.py`） |
| `d96bb05` | 23:06 | 连接器测试与 Chrome/Edge 真机 e2e 证据脚本（BROWSER-03 收口） |

规模：76 个文件、+20 023 行（其中测试约 2 800 行、文档约 2 000 行）。

**数据库**：3 个已提交迁移（`6265c1a4a6ef_browser_r1_schema`、
`e74d896da5cb_browser_r2_policy_schema`、`fdf23d777122_browser_r3_external_connector_schema`），
新增 10 张表：`browser_profiles` / `browser_sessions` / `browser_tabs` / `browser_actions` /
`browser_permissions` / `browser_approvals` / `browser_tasks` / `browser_task_steps` /
`browser_connectors` / `browser_connector_sessions`。

**配置增量**（`backend/app/config.py`）：`browser_enabled`（默认 **True**）、
`browser_data_dir`、`browser_max_sessions=1`、artifact 限额。注意：关闭时端点返回
**404 `feature_disabled`**（路由仍在，只是拒绝服务），所以开关不改变路由计数。

**未提交的 BROWSER-04（招聘业务集成，进行中）**：

| 项 | 事实 |
| --- | --- |
| 新增路由 | `POST/GET /api/browser/v1/candidate-extractions`、`GET/PATCH /{id}`、`POST /{id}/review`（5 条）+ `POST /api/imports/{session_id}/confirm-candidate`（1 条） |
| 新增表 | `browser_candidate_extractions`、`browser_candidate_extraction_revisions`（迁移 `501a807efa03`） |
| 状态 | 未提交（9 改 + 7 未跟踪），**不得当作契约权威**；等它进 `origin/main` 再谈 |

---

## 3. 对既有 68 条契约的影响：**零**（有证据）

方法：把 `d39e321` 与 `d96bb05` 各自 `git archive` 到独立目录，用同一 venv 导入 live
`app.main`，取 `/api` 的 `(method, path)` 集合双向比对：

```
old /api: 68  new /api: 102
added: 34  removed: 0
non-browser old: 68  new: 68  identical: True
```

34 条新增全部是 `/api/browser/v1/…`。非浏览器路由集合**逐条相同**，且
`backend/app/{api,application,domain,schemas}` 里除 `browser*` 新增文件外**没有改动**
（唯一被改的既有文件是 `api/__init__.py` 挂路由器、`models/__init__.py` 注册模型、
`config.py` 加浏览器配置，都不触碰既有 handler）。

**推论**：

- W1 已验收的 18 条、对拍、测试结论**不失效**；W1.5 的缺陷收尾不受影响；
- W2–W6 卡面的端点数 11 / 10 / 7 / 15 / 8 = 51 条**继续有效**（68 − 17 = 51 的算术未变）；
- 新增的 34 (+6) 条不属于这 68 条里的任何一条，因此**不能**混进 W2–W6 的验收。

---

## 4. 范围登记：浏览器子系统为什么不在迁移范围

不是执行方的判断，是 ICA 自己的架构文档写死的（行号已用 `sed -n` 复核，2026-11-18）：

| 出处 | 原话 |
| --- | --- |
| `docs/architecture/browser-architecture-r1.md:229` | 「不迁移 Rust（PR #13 §非目标已排除；`migration/rust-backend` 已删，保全 tag 保留）。」 |
| `docs/architecture/browser-implementation-breakdown-r1.md:28` | 「**不越界**：D 项不得修改 V1.0 冻结范围、不得提出 Rust 迁移、不得替代 `Software Engineer` 写业务实现」 |
| `docs/architecture/browser-implementation-breakdown-r1.md:194` | 「不提出 Rust 迁移（PR #13 §非目标已排除）。」 |
| `docs/architecture/local-admin-cloud-management-architecture-r1.md` §20.3（第 688 行） | 「继续采用 Tauri 2 + React/TypeScript + Python/FastAPI + SQLite 本地核心。云端管理服务是未来新增部署面，不要求替换桌面后端，也**不构成 Python → Rust 迁移理由**。」 |

**范围口径（本仓执行）**：

| 范围 | 路由数 | 归属 |
| --- | --- | --- |
| R0 + W1..W6（业务核心） | **68** | 迁移范围，按现有卡推进 |
| `/api/browser/v1/*`（BROWSER-00..03，已提交） | 34 | **明确排除**（引用上表），不实现、不对拍、不写进契约表 |
| `/api/browser/v1/candidate-extractions*` + `imports/confirm-candidate`（BROWSER-04，未提交） | 6 | 同上，且未进 main 前连基线都不算 |
| 云端管理面 / IAM / Managed AI（R1.0 架构与 R1.1 商业文档） | 0（文档无端点） | 不实现；其「建议契约」文档自己写明「不代表当前代码已存在同名类」 |

**技术理由（供 W7 之后的决策用，不是现在的承诺）**：浏览器子系统的执行面是独立的
Python 进程（`browser_bridge` / `browser_connector` / native host）＋ Playwright，
Rust 侧即便接管 HTTP 面，也要保留这些 Python 侧车；那是「新写一个 BFF」，不是
「移植 68 条 CRUD」。收益与风险都不在当前量级。

**待决策 D-1**：浏览器是否在 W7 之后单独立项（新契约 + 新授权），默认**不立**。

---

## 5. 门禁加固（W1.6 已落地）与一个已知锐边

W1.6 之前：`check_endpoint_map.py` 在全量 `/api` 上做双向集合比对、
`compare_baseline_schema.py` 与 20 对象的 DDL 绑定——指向主检出就
`live 102 / documented 68 → MISMATCH`，且多出的 browser 表与「DDL 写错」在
同一条输出里分不开。**当时的绿是因为镜像没动，不是因为门禁能区分「范围外」和「漏做」。**

W1.6 之后（全部带 `--self-test`）：

| 工具 | 变化 | 默认排除 |
| --- | --- | --- |
| `scripts/check_endpoint_map.py` | `--exclude-prefix`（可重复）+ 排除清单打印 + `--expect-excluded N` 断言 | `/api/browser/` |
| `scripts/compare_baseline_schema.py` | `--exclude-table`（fnmatch，可重复）+ 索引按**属主表**连带排除 + `--expect-excluded-tables N` 断言 | `browser_*` |
| `scripts/check_scope.sh`（新） | 一条命令跑两个门禁 + 两个 self-test；只依赖冻结镜像；主检出的探测是记录性输出，不参与退出判定 | — |

**实测锚点（复跑用）**：冻结镜像 `68 / excluded 0 / documented 68` → exit 0；
`d96bb05` 导出快照 `102 / excluded 34 / documented 68` → exit 0；
`--expect-excluded 33` → exit 1（断言真的在拦）；不带排除指向主检出 → MISMATCH exit 1。

**已知锐边（不要「修」它）**：BROWSER-04 的
`POST /api/imports/{session_id}/confirm-candidate` **在范围外却不在 `/api/browser/`
前缀下**，门禁会把它报成 MISSING。这是正确行为：把前缀放宽到 `/api/imports/` 会连
W4 真正要做的 import 路由一起藏掉。它的归宿是重锚决策（契约文档 §1.1 第四步），
不是扩大排除规则。

**一个反直觉的实测**：`feature_disabled` 只影响响应（404），**不影响路由计数**——
所以「关闭 browser_enabled」不是范围问题的解法，路由仍在 live 表里。

---

## 6. 波次计划更新

```
W1 ✅ 6484794 → W1.5 ✅ b178e39（w1.5-candidate）→ W1.6（本卡）→ W2 → W3
→ [前端冒烟] → W4 → [压测] → W5 → W6 → W7
```

| 波次 | 内容 | 端点数 | 与本次增补的关系 |
| --- | --- | --- | --- |
| W1.5 | 基线/端口缺陷 6 项 | 0 | 不变（已完成，2026-11-18） |
| **W1.6（本卡）** | 门禁范围加固 | 0 | 见 §5 |
| W2 | follow-ups + dashboard | 11 | 不变（Dashboard R2.1 是提案，第 192 行明写「不假定具体 API 端点、字段名或响应结构」，不得据此改 W2 契约） |
| W3 | drafts + draft-groups | 10 | 不变 |
| W4 | imports + export/import + OCR | 7 | 注意：`/api/imports/...confirm-candidate` 属 BROWSER-04，**不在** W4 的 7 条内 |
| W5 | ai + settings + audit + backups | 15 | 不变；Managed AI / Credits（商业 R1.1、IAM §10）**不实现**，schema 只增不改 |
| W6 | auth + shares | 8 | 不变；IAM R1.0 的三角色域 / RBAC+ABAC 属**目标设计**，不是当前代码；W6 仍以 `d39e321` 的 8 条为准，不得自行新增云端端点 |
| W7 | 双跑 → 切换 | 0 | 出口前补一条：与 ICA 当时 main 的路由差集必须**已知且登记**（现在就是那 34/+6 条 browser） |

---

## 7. 决策记录

| # | 决策 | 状态 |
| --- | --- | --- |
| D-1 | 浏览器子系统是否在 W7 之后单独立项迁移 | 默认**不立**；维持排除 |
| D-2 | `/home/bryan/ica` 的 6 个浏览器提交何时 push，以及 `/mnt/o/ica` 何时重锚 | 保持现状（镜像 `d39e321`），W7 前再定（C8） |
| D-3 | W1.6 是否插到 W2 之前 | **已决：插**（小卡，防门禁误判） |
| D-4 | 契约文档 §1 何时并入本文件结论 | **已落地**：§1.1 已写入（W1.6） |
| D-5 | 尾斜杠行为 | **已裁决（2026-11-18）：方案 B —— 307 重定向，与 Starlette 逐值一致，不登记 IMP-5**。理由：与基线逐值一致就是最优。已同步三处：W1.5 卡 A6、`ica-rust-workbook` SKILL.md 分叉表、契约文档 §7.1（无 IMP-5）。执行易错点两条：①重定向必须先于 405 判定（`HEAD /api/positions/` → 307）；②`Location` 保留 query 且为绝对 URL——均有集成测试守住（`router_parity_http.rs`） |

## 7.1 D-5 的 12 行基线实测（2026-11-18，端口 8097，`curl --max-redirs 0`）

```
GET /api/positions/            → 307  location: …/api/positions
GET /api/positions/?limit=2&search=abc → 307  location: …/api/positions?limit=2&search=abc
GET /api/positions/abc123/     → 307  location: …/api/positions/abc123
GET /api/positions//           → 307  location: …/api/positions
POST /api/positions/           → 307
DELETE /api/positions/        → 307     ← 方法不符也照样重定向
HEAD /api/positions/           → 307     ← 重定向优先于 405
GET /api/nonexistent/          → 404
GET /api/                       → 404
GET /                           → 200
HEAD /                          → 405
HEAD /api/nonexistent          → 404
```

Rust 侧逐行一致（10/12）；`GET /` 与 `HEAD /` 两行落在契约文档 §1 既有的
「`/` 不实现」偏离内，非 W1.5 引入。Starlette 依据：`routing.py:694-710`，
`Match.PARTIAL` 也算命中（方法不符也重定向），`rstrip("/")` 剥全部尾斜杠。

---

## 8. 复跑清单（任何人接手先跑这几条）

```bash
# 1) 三个检出的身份（只读）
git -C /mnt/o/ica rev-parse --short HEAD && git -C /mnt/o/ica status --porcelain -uall
git -C /home/bryan/ica log --oneline -1 && git -C /home/bryan/ica status -sb | head -1

# 2) 两个门禁 + self-test + 冻结镜像比对（一条命令）
bash scripts/check_scope.sh

# 3) 分方向细看（AC 级证据）
scripts/check_endpoint_map.py --self-test
scripts/compare_baseline_schema.py --self-test

# 4) 本仓门禁
bash scripts/check.sh

# 5) 基线未被写
sha256sum /mnt/o/ica/backend/ica.db    # 466aa3fc…18e1
```
