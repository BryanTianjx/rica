# BW0 Compatibility Strategy — Browser API 兼容迁移策略

**状态：** BW0 盘点裁定配套策略（AC5 兼容期/差异清单、AC4 W7 依赖、AC6 门禁独立）
**配套底册：** `browser-route-inventory-r1.md`（39 条逐条终裁）
**基线锚点：** `9ed173c`（Python `backend/app/api/browser.py`，只读参照）
**门禁口径：** 浏览器 API 契约不进 68 条迁移契约（方案 §1.3 / B0-07）

## 0. 范围与原则

- 39 条 `/api/browser/v1/*` 路由全部归 BW0（D-3/D-11），**全部保留**，废弃 0 条。
- 兼容面 = 路径 + wire 契约 + 落库表，三者均以基线 `9ed173c` 实测为准
  （实测 → 冻结 → 对拍），不在本卡定义新字段。
- 12 张 browser 表全部复用，字段级零差异（见 §5）；不新建平行数据模型。
- 兼容处置分两类：**保留兼容路由 8 条**（§2）、**改造路由 31 条**（§3）。

## 1. 总兼容期

| 项 | 声明 |
| --- | --- |
| 起点 | BW0 首个迁移批次（批1 Read）在 Rust 路径上线并通过该批对拍 |
| 覆盖 | 整个 BW0 迁移期（批1→批4）+ 一个完整发布周期 |
| 期间不变 | `/api/browser/v1/*` 的 path / method / 请求响应形状 / 12 张表结构保持不变 |
| 回退 | 迁移期内 Python sidecar 保留为回退路径；Rust 侧按批接管 |
| 终止 | §2.2 四个移除条件全部满足后，下线 Python 路由注册（Rust 路径与原路径保留） |

兼容期长短是工程排期参数，具体值由 BW0 迁移卡批准；本卡只声明兼容期的
起止判据与移除条件，不替迁移卡定日期。

## 2. 保留兼容路由（8 条）— 兼容期与移除条件

路由：#11 `GET /artifacts/{id}`、#12 `POST /permissions`、#13 `GET /permissions`、
#14 `DELETE /permissions/{id}`、#15 `POST /approvals`、
#16 `POST /approvals/{id}/decide`、#17 `GET /approvals/{id}`、#24 `GET /audit`。

### 2.1 兼容承诺

- 这 8 条 1:1 移植，wire 行为与基线 `9ed173c` 逐值对齐；
  `browser_permissions` / `browser_approvals` / `audit_events` 表结构不变。
- 审批的绑定语义（action_digest、expires_at NOT NULL、consumed_at 一次性）沿用
  现有 `browser_approvals` 列，不引入新列（目标身份独立 Hash 列是 B7 开放项，
  见 B0-08，不属 BW0）。
- 审计查询继续使用统一 `audit_events` 表，以 `browser.*` 事件类型切片，
  不建 `browser_audit_events` 平行表。

### 2.2 移除条件（需全部满足）

1. **逐条对拍通过**：8 条各自在 Rust 路径上对基线 `9ed173c` 契约对拍通过
   （分母/锚点/取数时点随报告记录，见 §6）。
2. **调用方切换完成**：前端 UI（endpoints.ts 对应 8 个调用点）与审批界面
   已切到 Rust 路径并验证；审批事件提示链（Agent 只读接收）未受影响。
3. **无回退调用**：连续一个完整发布周期，Python 回退路径无实际调用
   （以审计事件 / 访问日志确认），且无待迁移的存量客户端。
4. **边界未回滚**：B0-09 产品边界（逐条确认发送、Agent 无发送工具）未发生
   需要回滚的变更。

任一条件不满足则继续保留兼容面，不得单独下线其中一条。

## 3. 改造路由（31 条）— 行为差异清单

差异分层说明：**落库表与字段零差异**（§5）；下列差异集中在
**实现/传输归属**与**衔接 W7 数据链**，不是 wire 字段的形状变更——
每条 wire 契约仍以 `9ed173c` 实测冻结，移植时逐值对拍。

| 组 | 路由 | 相对基线的差异 | 不变项 |
| --- | --- | --- | --- |
| Runtime / Session / Action（#1–10，含 hybrid #23） | POST runtime/start|stop、GET status、sessions CRUD、navigate/snapshot/screenshot/actions、verify、hybrid/click | ① 承载从 Python sidecar（Playwright）改为 Rust Browser Runtime 独立 Worker 进程 + Worker IPC 接入 Tauri Host，不经过 ica-desktop 业务命令层（B0-02）；② `hybrid/click` 的 op_command 执行改由 Worker 驱动扩展（原 docstring L18 标注 extension）；③ 会话/动作状态由 Rust use case 写入 | `browser_sessions`/`browser_actions`/`browser_task_steps` 列与状态枚举不变；artifact 仍落 `browser_actions.artifact_path` |
| Tasks（#18–22） | tasks create/get/run/cancel/audit | ① 任务编排由 Rust use case 承载；② `advance_task` 类写路径按基线单事务语义落地（审批拒绝时不得留脏状态） | `browser_tasks`/`browser_task_steps` 复用，字段与状态枚举不变 |
| Connector（#25–34） | start/stop/pair/rotate/status/tabs/grant/revoke/act/audit | ① connector 协议由 Rust Worker 接管；② 配对 nonce / `key_epoch` 轮换 / tab 授权与撤销由 Rust use case 写入；③ 调用方不变（MV3 扩展 + native messaging `com.ica.connector`），前端 `status` 端点保留 | 调用链仍是「扩展 → native messaging → `/connector/*`」；`browser_connectors`/`browser_connector_sessions`/`browser_tabs` 列不变 |
| Candidate Extractions（#35–39） | create/list/get/patch/review | ① `review` 端点写 `browser_candidate_extraction_revisions`（§11：修订写入随 BW0 的 review 端点迁移）；② duplicate 字段**写路径**随 BW0（读路径已在 W7，§11）；③ 这 5 条是 W7 `confirm-candidate` 的上游，BW0 产出 approved + review_digest 供 W7 消费 | `browser_candidate_extractions`（33 列）与修订表（9 列）结构不变；duplicate 读路径沿用 W7 已落地实现 |

### 3.1 不在 BW0 引入的变更

- 任意 JavaScript 执行工具不对 Agent 开放（B0-04）；无批量自主发送路径。
- 不新建 `candidate_captures` / `candidate_matches` / `browser_audit_events` 等
  平行结构；`platform_accounts` / `message_preparations` / `send_executions`
  按 B0-08 defer 到 B3/B6/B7。
- 不把任何一条 browser 路由塞进 68 条契约（见 §6）。

## 4. W7 依赖裁定（AC4 — `/candidate-extractions*` 五条）

**裁决**：#35–39 五条随 BW0 迁移，**不并入 W7 默认范围**
（`b0-deliverables-r1.md` §10）：

- (a) W7 默认范围（`confirm-candidate` 依赖链）已实现并验收；
- (b) 这 5 条是 create → update → review（approved + review_digest）→ confirm
  的完整 CRUD + 列表，与其余 34 条共享 `browser.py` 路由模块、schema 族与
  任务/动作基础设施——拆进 W7 会让 W7 承载 B 线半套基础设施，违反
  「波次职责不得重叠」；
- (c) BW0 已由 D-3/D-11 立卡承接 browser API 兼容，范围以盘点底册为准。
  W7 卡 §5 的「需单独裁决」项就此关闭。

**依赖方向（单向，无循环）**：

```
BW0 #35–39 (create/update/review → approved + review_digest)
        │  写 browser_candidate_extractions + revisions
        ▼
W7 confirm-candidate（已实现）：读 approved 产物 → 确认草稿
```

W7 是**已交付的下游消费者**，不再反向要求 BW0 提供新能力；BW0 只读同一张
`browser_candidate_extractions` 表并复用 W7 已落地的 duplicate 读路径。
依赖图无环，职责无重叠。

**职责边界（D-9 / §11）**：

| 能力 | 归属 |
| --- | --- |
| 既有抽取记录迁移、`browser_candidate_extractions` 写入、confirm-candidate、duplicate 读路径 | W7（已实现） |
| Extraction Revisions 写入、duplicate 写路径、5 条 CRUD/review | BW0 |
| 新浏览器页面解析 / 解析结果接入现有 Extraction 流程 | B3 / B4（不建平行模型） |

W7 与 B4 不得各自维护 Candidate Capture 的状态机、去重规则和持久化结构。

## 5. 12 表复用确认（AC3）

字段级差异报告（Rust `crates/ica-infra/schema/ica-baseline.sql` vs 基线
`backend/app/models/browser.py` @ `9ed173c`，按列名 / 类型含长度 / NOT NULL 逐列比对）：

| 表 | 列数（Rust / 基线） | 差异 |
| --- | --- | --- |
| browser_profiles | 14 / 14 | 无 |
| browser_sessions | 15 / 15 | 无 |
| browser_actions | 25 / 25 | 无 |
| browser_tasks | 20 / 20 | 无 |
| browser_task_steps | 17 / 17 | 无 |
| browser_permissions | 17 / 17 | 无 |
| browser_approvals | 20 / 20 | 无 |
| browser_connectors | 21 / 21 | 无 |
| browser_connector_sessions | 20 / 20 | 无 |
| browser_tabs | 21 / 21 | 无 |
| browser_candidate_extractions | 33 / 33 | 无 |
| browser_candidate_extraction_revisions | 9 / 9 | 无 |
| **合计** | **232 / 232** | **0** |

`column_defaults.rs` 已含这些表的 NOT NULL 默认值（如 `browser_approvals.state='pending'`、
`browser_candidate_extractions.status='draft'`），BW0 写入路径直接复用。
**12 表全部复用、零新建、字段级差异为零**；唯一开放项
`browser_approvals` 目标身份独立 Hash 列由 B7 字段级迁移报告裁决，
不在 BW0 建表范围。

## 6. 门禁独立声明（AC6）

**BW0 测试与门禁独立于 68 条对拍统计，禁止把 Browser 测试数加进 68 条统计。**

- 68 条对拍统计的分母固定为 68 条迁移契约
  （`R0 2 + W1 15 + W2 11 + W3 10 + W4 7 + W5 15 + W6 8`），
  与浏览器子系统无关；端点对账门禁（`endpoint_reconciliation`）继续按
  `implemented + missing == 68` 且 `extra_routes == 0` 判定。
- 浏览器独立成族（方案 §1.3 / B0-07 / `b0-deliverables-r1.md` §明确不做）：
  39 条路由对的契约用例、B1 POC 的 `browser_poc` 测试、connector 端到端测试
  （`test_browser_connector*.py`）**独立统计、独立门禁**，其通过数不计入
  68 条的分子或分母。
- 任何通过率报告必须同时带三件：**分母 + 锚点 sha + 取数时点**
  （例：「在 `9ed173c` 实测契约上 N/N matched」）；仅写「全绿」不构成
  可复核断言。
- 本卡是盘点/策略卡，不写 39 条实现，也不新增任何浏览器测试到 68 条 CASES
  清单；BW0 迁移卡交付时按上述独立门禁出具自己的统计。

## 7. AC 对照

| AC | 落地位置 |
| --- | --- |
| AC1 路由清单完整 | `browser-route-inventory-r1.md` §2（39 条逐条终裁，无留空） |
| AC2 集合 C 判定有据 | 同文件 §2.1（10 条例外路由逐条查证，扩展侧调用） |
| AC3 复用现有表 | 同文件 §4 + 本文档 §5（12 表 232 列零差异） |
| AC4 与 W7 依赖已裁定 | 本文档 §4（5 条随 BW0，单向无环） |
| AC5 兼容策略明确 | 本文档 §1–§3（兼容期/移除条件/行为差异清单） |
| AC6 门禁独立 | 本文档 §6（BW0 测试不进 68 条统计） |
