# Browser Route Inventory R1.1 — 39 条 `/api/browser/v1/*` 路由归属盘点（BW0 终裁版）

**状态：** 归属已终裁（B0-10 / D-3 / D-11）；39 条全部归 BW0，集合 C 为空
**生成日期：** 2026-11-19（R1.0）／2026-11-22（R1.1 补齐最终值）
**来源：** `backend/app/api/browser.py` @ `9ed173c`（39 条 `@router` 装饰器，逐条提取）
**裁决依据：** 本文件 §7 B0-10 结论；`b0-deliverables-r1.md` §10 / §11；
兼容策略见 `bw0-compatibility-strategy.md`

## 0. 本文档的用途与记录项

依据裁决 D-3：这 39 条路由不能忽略，也不能直接全部迁移，必须全量盘点、
逐条给出最终归属。D-3 要求的记录项，R1.1 已全部逐条落定：

```
HTTP Method / Path · Python 实现位置 · 是否被 UI 调用 · 是否被 Agent 或 MCP 调用
数据表依赖 · 是否进入 W7 · 是否进入 Browser Subsystem · 是否保留兼容 · 是否废弃
Rust 对应实现状态
```

- 实测列（R1.0）：Method/Path、Python 函数名、UI 调用情况、数据表依赖。
- 终裁列（R1.1）：Agent/MCP 调用、集合归属、W7/BW0 归属、保留兼容/废弃、Rust 实现状态。
- 本卡是**盘点裁定卡**，不写 39 条 Rust 实现；实现归后续 BW0 迁移卡。

## 1. 实测与终裁汇总

| 指标 | 值 |
| --- | --- |
| 路由总数 | **39** |
| 有 UI 调用 | **29** |
| 无 UI 调用 | **10**（9 条 connector + 1 条 hybrid/click，均为扩展侧调用，见 §2.1） |
| Agent/MCP 直接 HTTP 调用 | **0**（Agent 只见业务级 MCP 工具，不直连这些 HTTP 路由） |
| 集合 A（W7） | **0**（39 条中无一条进 W7；W7 只消费候选抽取产物） |
| 集合 B（Browser Subsystem / BW0） | **39** |
| 集合 C（废弃或合并） | **0**（为空，维持全部保留） |
| 归 BW0 | **39**（D-3/D-11） |
| 归 W7 | **0** |
| 保留兼容路由 | **8**（§5.1） |
| 改造路由 | **31**（§5.2） |
| 废弃路由 | **0** |
| 现有 browser 表 | **12 张**（字段级零差异，见 §4） |
| Rust 侧 browser 业务实现 | **0**（仅 `column_defaults.rs` / `ica-baseline.sql` 有表定义与默认值条目） |

### 关于 D-3 集合 C 的判断（已终裁：为空）

```
frontend/src/services/endpoints.ts   含 browser 的行 53 行
                                 实际 API 调用点 27 处
                                 覆盖 29 / 39 条路由
```

**29 / 39 条有前端调用。** 无 UI 调用的 10 条几乎全部集中在 connector 系列
（10 条中 9 条）——这些是浏览器扩展（MV3 + native messaging）侧调用的，不是 UI 侧
调用，所以「前端没引用」不等于「没有调用方」。逐条查证见 §2.1。

**结论：集合 C 为空**，全部保留；盘点起点是「多数保留」，不是「先找废品」。

## 2. 逐条路由清单（39 条，最终值）

列义：`集合` = A(W7)/B(Browser Subsystem/BW0)/C(废弃)；`W7` = 是否进入 W7 迁移；
`BW0` = 是否进入 BW0 及其内部批次（`批1` Read → `批2` 写路径 → `批3` Connector →
`批4` Candidate Extractions，见 §7）；`保留/废弃` = 兼容处置；`Rust 状态` = 实现状态。

| # | Method | Path | Python 实现 | UI 调用 | Agent/MCP 调用 | 数据表依赖 | 集合 | W7 | BW0 | 兼容处置 | 废弃 | Rust 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | POST | `/api/browser/v1/runtime/start` | `start_runtime` | ✓ | 否｜UI | browser_sessions | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 2 | POST | `/api/browser/v1/runtime/stop` | `stop_runtime` | ✓ | 否｜UI | browser_sessions | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 3 | GET | `/api/browser/v1/runtime/status` | `runtime_status` | ✓ | 否｜UI | browser_sessions | B | 否 | 是·批1 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 4 | POST | `/api/browser/v1/sessions` | `create_session` | ✓ | 否｜UI | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 5 | DELETE | `/api/browser/v1/sessions/{session_id}` | `close_session` | ✓ | 否｜UI | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 6 | POST | `/api/browser/v1/sessions/{session_id}/navigate` | `navigate` | ✓ | 否｜UI | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 7 | POST | `/api/browser/v1/sessions/{session_id}/snapshot` | `snapshot` | ✓ | 否｜UI | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 8 | POST | `/api/browser/v1/sessions/{session_id}/screenshot` | `screenshot` | ✓ | 否｜UI | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 9 | POST | `/api/browser/v1/sessions/{session_id}/actions` | `execute_action` | ✓ | 否｜UI | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 10 | POST | `/api/browser/v1/actions/{action_id}/verify` | `verify_action` | ✓ | 否｜UI | browser_actions | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 11 | GET | `/api/browser/v1/artifacts/{artifact_id}` | `read_artifact` | ✓ | 否｜UI | browser_actions（artifact_path） | B | 否 | 是·批1 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 12 | POST | `/api/browser/v1/permissions` | `grant_permission` | ✓ | 否｜UI | browser_permissions | B | 否 | 是·批2 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 13 | GET | `/api/browser/v1/permissions` | `list_permissions` | ✓ | 否｜UI | browser_permissions | B | 否 | 是·批1 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 14 | DELETE | `/api/browser/v1/permissions/{permission_id}` | `revoke_permission` | ✓ | 否｜UI | browser_permissions | B | 否 | 是·批2 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 15 | POST | `/api/browser/v1/approvals` | `request_approval` | ✓ | 否｜UI | browser_approvals | B | 否 | 是·批2 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 16 | POST | `/api/browser/v1/approvals/{approval_id}/decide` | `decide_approval` | ✓ | 否｜UI | browser_approvals | B | 否 | 是·批2 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 17 | GET | `/api/browser/v1/approvals/{approval_id}` | `approval_detail` | ✓ | 否｜UI | browser_approvals | B | 否 | 是·批1 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 18 | POST | `/api/browser/v1/tasks` | `create_task` | ✓ | 否｜UI | browser_tasks, browser_task_steps | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 19 | GET | `/api/browser/v1/tasks/{task_id}` | `task_detail` | ✓ | 否｜UI | browser_tasks, browser_task_steps | B | 否 | 是·批1 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 20 | POST | `/api/browser/v1/tasks/{task_id}/run` | `run_task` | ✓ | 否｜UI | browser_tasks, browser_task_steps | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 21 | POST | `/api/browser/v1/tasks/{task_id}/cancel` | `cancel_task` | ✓ | 否｜UI | browser_tasks, browser_task_steps | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 22 | GET | `/api/browser/v1/tasks/{task_id}/audit` | `task_action_audit` | ✓ | 否｜UI | browser_tasks, browser_task_steps | B | 否 | 是·批1 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 23 | POST | `/api/browser/v1/sessions/{session_id}/hybrid/click` | `hybrid_click` | — | 否｜扩展（MV3+native messaging） | browser_sessions, browser_tabs | B | 否 | 是·批2 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 24 | GET | `/api/browser/v1/audit` | `audit_query` | ✓ | 否｜UI | audit_events | B | 否 | 是·批1 | 保留兼容 | 否 | 未实现｜归 BW0 迁移卡 |
| 25 | POST | `/api/browser/v1/connector/start` | `connector_start` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 26 | POST | `/api/browser/v1/connector/stop` | `connector_stop` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 27 | POST | `/api/browser/v1/connector/pair` | `connector_pair` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 28 | POST | `/api/browser/v1/connector/rotate` | `connector_rotate` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 29 | GET | `/api/browser/v1/connector/status` | `connector_status` | ✓ | 否｜UI（扩展系列经原生宿主联调） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 30 | GET | `/api/browser/v1/connector/tabs` | `connector_seen_tabs` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 31 | POST | `/api/browser/v1/connector/tabs/grant` | `connector_grant_tab` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 32 | POST | `/api/browser/v1/connector/tabs/revoke` | `connector_revoke_tab` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 33 | POST | `/api/browser/v1/connector/act` | `connector_act` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 34 | GET | `/api/browser/v1/connector/audit` | `connector_audit` | — | 否｜扩展（MV3+native messaging） | browser_connectors, browser_connector_sessions, browser_tabs | B | 否 | 是·批3 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 35 | POST | `/api/browser/v1/candidate-extractions` | `create_candidate_extraction` | ✓ | 否｜UI；MCP 经业务级工具读产物 | browser_candidate_extractions, browser_candidate_extraction_revisions | B | 否（W7 消费产物） | 是·批4 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 36 | GET | `/api/browser/v1/candidate-extractions` | `list_candidate_extractions` | ✓ | 否｜UI；MCP 经业务级工具读产物 | browser_candidate_extractions, browser_candidate_extraction_revisions | B | 否（W7 消费产物） | 是·批4 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 37 | GET | `/api/browser/v1/candidate-extractions/{extraction_id}` | `get_candidate_extraction` | ✓ | 否｜UI；MCP 经业务级工具读产物 | browser_candidate_extractions, browser_candidate_extraction_revisions | B | 否（W7 消费产物） | 是·批4 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 38 | PATCH | `/api/browser/v1/candidate-extractions/{extraction_id}` | `update_candidate_extraction` | ✓ | 否｜UI；MCP 经业务级工具读产物 | browser_candidate_extractions, browser_candidate_extraction_revisions | B | 否（W7 消费产物） | 是·批4 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |
| 39 | POST | `/api/browser/v1/candidate-extractions/{extraction_id}/review` | `review_candidate_extraction` | ✓ | 否｜UI；MCP 经业务级工具读产物 | browser_candidate_extractions, browser_candidate_extraction_revisions | B | 否（W7 消费产物） | 是·批4 | 改造 | 否 | 未实现｜归 BW0 迁移卡 |

### 2.1 十条例外路由的调用方逐条查证（AC2）

查证口径：UI 侧以 `frontend/src/services/endpoints.ts` 的 `browserApi` 对象为底册
（另核 sidecar UI 打包产物，只有 `connectorStatus`）；扩展侧以 `extension/`
（MV3 `manifest.json` 的 `nativeMessaging` 权限 + `background.js` 的
`chrome.runtime.connectNative("com.ica.connector")`）与
`backend/app/api/browser.py` 模块 docstring / connector 段注释为据；
MCP 侧全仓无直接调用这些 HTTP 路由的工具定义。

| # | 路由 | UI 调用 | 实际调用方（逐条查证） | 证据 | 集合 C 判定 |
| --- | --- | --- | --- | --- | --- |
| 23 | `POST /sessions/{session_id}/hybrid/click` | 无 | 浏览器扩展侧（hybrid 引擎入口） | `browser.py` docstring L18 明确标注 `extension`；`endpoints.ts` 无 hybrid 条目 | 不废弃：BROWSER-02 D05 §5.2 定义的扩展入口 |
| 25 | `POST /connector/start` | 无 | 浏览器扩展（MV3 + native messaging） | `background.js` `connectNative("com.ica.connector")`；`browser.py` L364-368 connector 段注释；前端仅 `connectorStatus` | 不废弃：配对生命周期起点 |
| 26 | `POST /connector/stop` | 无 | 同上 | 同上 | 不废弃：关闭会话 / AC-03-5 |
| 27 | `POST /connector/pair` | 无 | 同上 | 同上；`connector.py` 一次性配对 nonce | 不废弃：配对身份与 nonce |
| 28 | `POST /connector/rotate` | 无 | 同上 | 同上；`key_epoch` / `previous_key_epoch` 字段 | 不废弃：密钥轮换 |
| 30 | `GET /connector/tabs` | 无 | 同上 | 同上；`connector_seen_tabs` 返回扩展可见 tab 列表 | 不废弃：tab 可见性来源 |
| 31 | `POST /connector/tabs/grant` | 无 | 同上 | 同上；`browser_tabs` 授权记录 | 不废弃：逐 tab 授权 |
| 32 | `POST /connector/tabs/revoke` | 无 | 同上 | 同上；`browser_tabs` 撤销记录 | 不废弃：逐 tab 撤销 |
| 33 | `POST /connector/act` | 无 | 同上 | 同上；扩展在已授权 tab 内执行 op_command | 不废弃：动作执行面 |
| 34 | `GET /connector/audit` | 无 | 同上 | 同上；`connector_audit` 读审计轨迹 | 不废弃：可审计性 |

对照：#29 `GET /connector/status` **有** UI 调用（`endpoints.ts:379` /
sidecar UI），不属于这 10 条例外，但仍随 connector 系列归 BW0 批3。

**查证结论**：10 条均有真实调用方（浏览器扩展 / native messaging），
「无 UI 调用」是调用面差异而非产品价值缺失；集合 C 维持为空。

## 3. 集合 A · W7 迁移范围（39 条中为空）

W7 默认范围（`POST /api/imports/{session_id}/confirm-candidate`，
在 `api/imports.py`，是 68 条契约里的 5 条差额之一）**不在 browser.py 39 条内**。

`/candidate-extractions*` 5 条（#35–39）表格初版标为集合 A，B0-10 已终裁改判为
**集合 B**：这 5 条是 confirm-candidate 的上游
（create → update → review（置 approved + review_digest）→ confirm），
W7 默认范围已实现并验收，5 条上游 CRUD + 列表随其余 34 条进 BW0，
避免 W7 承载 B 线的半套基础设施（波次职责不重叠）。详见 §7 与
`bw0-compatibility-strategy.md` §4。因此本表 39 条中集合 A 计数为 0。

## 4. 集合 B · 12 张 browser 表复用确认（AC3）

BW0 依赖的表**全部复用现有 12 张**，不新建平行结构：

`browser_profiles`、`browser_sessions`、`browser_actions`、`browser_tasks`、
`browser_task_steps`、`browser_permissions`、`browser_approvals`、
`browser_connectors`、`browser_connector_sessions`、`browser_tabs`、
`browser_candidate_extractions`、`browser_candidate_extraction_revisions`。

字段级差异报告（Rust `crates/ica-infra/schema/ica-baseline.sql` vs 基线
`backend/app/models/browser.py` @ `9ed173c`，按列名 / 类型含长度 / NOT NULL 逐列比对）：

| 表 | Rust 列数 | 基线列数 | 字段级差异 |
| --- | --- | --- | --- |
| browser_profiles | 14 | 14 | 无 |
| browser_sessions | 15 | 15 | 无 |
| browser_actions | 25 | 25 | 无 |
| browser_tasks | 20 | 20 | 无 |
| browser_task_steps | 17 | 17 | 无 |
| browser_permissions | 17 | 17 | 无 |
| browser_approvals | 20 | 20 | 无 |
| browser_connectors | 21 | 21 | 无 |
| browser_connector_sessions | 20 | 20 | 无 |
| browser_tabs | 21 | 21 | 无 |
| browser_candidate_extractions | 33 | 33 | 无 |
| browser_candidate_extraction_revisions | 9 | 9 | 无 |
| **合计** | **232** | **232** | **0** |

**结论：12 张表 / 232 列逐列一致，字段级差异为零**；`column_defaults.rs` 已含
这些表的 NOT NULL 默认值。方案提议的 `candidate_captures` /
`candidate_matches` 等是同一件事的另一种命名，按 D-9 复用现有 Extraction 表与
duplicate 字段；`platform_accounts` / `message_preparations` / `send_executions`
按 B0-08 defer 到 B3/B6/B7，不在 BW0 建表范围。唯一开放项是
`browser_approvals` 目标身份独立 Hash 列（B0-08），由 B7 字段级迁移报告裁决，
不构成 BW0 建新表的理由。

## 5. 兼容处置分类

### 5.1 保留兼容路由（8 条）

#11 artifact 读取、#12–14 permissions、#15–17 approvals、#24 audit 查询——
Python 行为原样落地，迁移期保留 `/api/browser/v1/*` 路径与 wire 契约作兼容面；
兼容期与移除条件见 `bw0-compatibility-strategy.md` §2。

### 5.2 改造路由（31 条）

#1–10（runtime/session/action）、#18–23（tasks/hybrid）、#25–34（connector）、
#35–39（candidate-extractions）——实现/传输归属变化（Browser Runtime Worker /
MV3 扩展边界 / W7 数据链衔接）；逐组行为差异清单见
`bw0-compatibility-strategy.md` §3。废弃路由 0 条。

## 6. 最终归属与迁移顺序

1. **归属**：39 条全部归 BW0（D-3/D-11），不入 68 条契约统计；每条路由的契约
   以基线 `9ed173c` 实测为准（方法：实测 → 冻结 → 对拍）。
2. **兼容策略**：见 `bw0-compatibility-strategy.md`（兼容期 / 移除条件 /
   行为差异清单 / W7 依赖裁定 / 12 表复用确认 / 门禁独立声明）。
3. **BW0 内部批次**（供迁移卡拆分参考）：
   - 批1 Read（7 条）：#3 #11 #13 #17 #19 #22 #24
   - 批2 写路径（17 条）：#1 #2 #4–#10 #12 #14–#16 #18 #20 #21 #23
   - 批3 Connector（10 条）：#25–#34（扩展侧联调）
   - 批4 Candidate Extractions（5 条）：#35–#39（含修订表写入）
   - 合计 7 + 17 + 10 + 5 = 39。

## 7. B0-10 归属结论（2026-11-20，详见 b0-deliverables-r1.md §10）—— 本 R1.1 已据此落定

- **集合 C 判定：为空（维持现状，全部保留待迁移）。** 10 条无 UI 调用路由
  逐条查证为浏览器扩展（MV3 + native messaging）侧调用，有真实调用方；
  「废弃或合并」不适用（§2.1）。
- **归属：39 条全部归 BW0**（D-3/D-11），按 §6 分组分批迁移；
  不入 68 条契约统计（方案 §1.3），每条路由的契约以基线 `9ed173c` 实测为准。
- **`candidate-extractions` 5 条**：随 BW0（B0 卡 §10 的裁决）——W7 默认范围
  （confirm-candidate 依赖链）已完成，5 条上游 CRUD 与 browser.py 其余路由
  同模块同 schema 族，随 BW0 保持「波次职责不重叠」；W7 只消费其产物，无循环依赖。
- **迁移顺序建议**（BW0 内部，供卡面拆分参考）：Read 类（tasks/permissions/
  approvals/audit 查询）→ 任务/动作写路径 → Connector 10 条（扩展侧联调）→
  Candidate Extractions 5 条（含修订表写入）。
