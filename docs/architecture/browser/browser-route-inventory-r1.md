# Browser Route Inventory R1.0 — 39 条 `/api/browser/v1/*` 路由归属盘点

**状态：** 盘点骨架已建立，归属待 B0 裁决（D-3）
**生成日期：** 2026-11-19
**来源：** `backend/app/api/browser.py` @ `9ed173c`（39 条 `@router` 装饰器，逐条提取）

## 0. 本文档的用途

依据裁决 D-3：这 39 条路由**不能忽略，也不能直接全部迁移**，必须全量盘点、
逐条给出最终归属。D-3 要求的记录项：

```
HTTP Method / Path · Python 实现位置 · 是否被 UI 调用 · 是否被 Agent 或 MCP 调用
数据表依赖 · 是否进入 W7 · 是否进入 Browser Subsystem · 是否保留兼容 · 是否废弃
Rust 对应实现状态
```

**已实测的列**：Method/Path、Python 函数名、UI 调用情况、数据表依赖。
**待 B0 填的列**：Agent/MCP 调用、集合归属、W7 归属、保留兼容/废弃、Rust 实现状态。

## 1. 实测汇总

| 指标 | 值 |
| --- | --- |
| 路由总数 | **39** |
| 有 UI 调用 | **29** |
| 无 UI 调用 | **10** |
| 现有 browser 表 | 12 张 |
| Rust 侧 browser 业务实现 | **无**（仅 `column_defaults.rs` 有表名条目） |

### 关于 D-3 集合 C 的判断修正

裁决提到集合 C 是「没有有效调用方或已经失去产品价值的旧路由」。实测：

```
frontend/src/services/endpoints.ts   含 browser 的行 53 行
                                 实际 API 调用点 27 处
                                 覆盖 29 / 39 条路由
```

**29 / 39 条有前端调用**，集合 C 很可能为空。无 UI 调用的 10 条
几乎全部集中在 **connector 系列**（External Browser 配对，10 条中 9 条）——
这些是浏览器扩展侧调用的，不是 UI 侧调用，所以「前端没引用」不等于「没有调用方」。

**结论：盘点起点应假定「多数会被保留」，而不是「先找废品」。**
connector 系列的调用方需要单独查证（浏览器扩展 / MCP）。

## 2. 逐条路由清单

| # | Method | Path | Python 实现 | UI 调用 | 数据表依赖 | 集合 | W7 | BW0 | 保留 | 废弃 | Rust 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | POST | `/api/browser/v1/runtime/start` | `start_runtime` | ✓ | browser_sessions | B | | | | | 未实现 |
| 2 | POST | `/api/browser/v1/runtime/stop` | `stop_runtime` | ✓ | browser_sessions | B | | | | | 未实现 |
| 3 | GET | `/api/browser/v1/runtime/status` | `runtime_status` | ✓ | browser_sessions | B | | | | | 未实现 |
| 4 | POST | `/api/browser/v1/sessions` | `create_session` | ✓ | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 5 | DELETE | `/api/browser/v1/sessions/{session_id}` | `close_session` | ✓ | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 6 | POST | `/api/browser/v1/sessions/{session_id}/navigate` | `navigate` | ✓ | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 7 | POST | `/api/browser/v1/sessions/{session_id}/snapshot` | `snapshot` | ✓ | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 8 | POST | `/api/browser/v1/sessions/{session_id}/screenshot` | `screenshot` | ✓ | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 9 | POST | `/api/browser/v1/sessions/{session_id}/actions` | `execute_action` | ✓ | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 10 | POST | `/api/browser/v1/actions/{action_id}/verify` | `verify_action` | ✓ | browser_actions | B | | | | | 未实现 |
| 11 | GET | `/api/browser/v1/artifacts/{artifact_id}` | `read_artifact` | ✓ | browser_actions（artifact_path） | B | | | | | 未实现 |
| 12 | POST | `/api/browser/v1/permissions` | `grant_permission` | ✓ | browser_permissions | B | | | | | 未实现 |
| 13 | GET | `/api/browser/v1/permissions` | `list_permissions` | ✓ | browser_permissions | B | | | | | 未实现 |
| 14 | DELETE | `/api/browser/v1/permissions/{permission_id}` | `revoke_permission` | ✓ | browser_permissions | B | | | | | 未实现 |
| 15 | POST | `/api/browser/v1/approvals` | `request_approval` | ✓ | browser_approvals | B | | | | | 未实现 |
| 16 | POST | `/api/browser/v1/approvals/{approval_id}/decide` | `decide_approval` | ✓ | browser_approvals | B | | | | | 未实现 |
| 17 | GET | `/api/browser/v1/approvals/{approval_id}` | `approval_detail` | ✓ | browser_approvals | B | | | | | 未实现 |
| 18 | POST | `/api/browser/v1/tasks` | `create_task` | ✓ | browser_tasks, browser_task_steps | B | | | | | 未实现 |
| 19 | GET | `/api/browser/v1/tasks/{task_id}` | `task_detail` | ✓ | browser_tasks, browser_task_steps | B | | | | | 未实现 |
| 20 | POST | `/api/browser/v1/tasks/{task_id}/run` | `run_task` | ✓ | browser_tasks, browser_task_steps | B | | | | | 未实现 |
| 21 | POST | `/api/browser/v1/tasks/{task_id}/cancel` | `cancel_task` | ✓ | browser_tasks, browser_task_steps | B | | | | | 未实现 |
| 22 | GET | `/api/browser/v1/tasks/{task_id}/audit` | `task_action_audit` | ✓ | browser_tasks, browser_task_steps | B | | | | | 未实现 |
| 23 | POST | `/api/browser/v1/sessions/{session_id}/hybrid/click` | `hybrid_click` | — | browser_sessions, browser_tabs | B | | | | | 未实现 |
| 24 | GET | `/api/browser/v1/audit` | `audit_query` | ✓ | audit_events | B | | | | | 未实现 |
| 25 | POST | `/api/browser/v1/connector/start` | `connector_start` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 26 | POST | `/api/browser/v1/connector/stop` | `connector_stop` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 27 | POST | `/api/browser/v1/connector/pair` | `connector_pair` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 28 | POST | `/api/browser/v1/connector/rotate` | `connector_rotate` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 29 | GET | `/api/browser/v1/connector/status` | `connector_status` | ✓ | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 30 | GET | `/api/browser/v1/connector/tabs` | `connector_seen_tabs` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 31 | POST | `/api/browser/v1/connector/tabs/grant` | `connector_grant_tab` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 32 | POST | `/api/browser/v1/connector/tabs/revoke` | `connector_revoke_tab` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 33 | POST | `/api/browser/v1/connector/act` | `connector_act` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 34 | GET | `/api/browser/v1/connector/audit` | `connector_audit` | — | browser_connectors, browser_connector_sessions, browser_tabs | B | | | | | 未实现 |
| 35 | POST | `/api/browser/v1/candidate-extractions` | `create_candidate_extraction` | ✓ | browser_candidate_extractions, browser_candidate_extraction_revisions | A | | | | | 未实现 |
| 36 | GET | `/api/browser/v1/candidate-extractions` | `list_candidate_extractions` | ✓ | browser_candidate_extractions, browser_candidate_extraction_revisions | A | | | | | 未实现 |
| 37 | GET | `/api/browser/v1/candidate-extractions/{extraction_id}` | `get_candidate_extraction` | ✓ | browser_candidate_extractions, browser_candidate_extraction_revisions | A | | | | | 未实现 |
| 38 | PATCH | `/api/browser/v1/candidate-extractions/{extraction_id}` | `update_candidate_extraction` | ✓ | browser_candidate_extractions, browser_candidate_extraction_revisions | A | | | | | 未实现 |
| 39 | POST | `/api/browser/v1/candidate-extractions/{extraction_id}/review` | `review_candidate_extraction` | ✓ | browser_candidate_extractions, browser_candidate_extraction_revisions | A | | | | | 未实现 |

## 3. 集合 A · W7 迁移范围

| Method | Path | 说明 |
| --- | --- | --- |
| POST | `/api/imports/{session_id}/confirm-candidate` | **不在 browser.py**，在 `api/imports.py:185`。
| | | 是 68 条契约里的 5 条差额之一，属 W7。 |

`/candidate-extractions*` 这 5 条（B0 裁决为集合 B）**是 confirm-candidate 的上游**：
create → update → review（置 approved + review_digest）→ confirm。
没有 review 就没有 approved，没有 approved 就没有可确认的草稿。

**因此 W7 实际依赖这 5 条**。B0 必须裁定：这 5 条随 W7 一起迁移，
还是独立进 BW0 后由 W7 依赖它——两种都可以，但不能留空。

## 4. 集合 B · Browser Subsystem 兼容迁移范围

33 条（B0 审查后决定复用 / 改造 / 替换）：

- **Runtime 生命周期** — 3 条
- **Session 与页面操作** — 8 条
- **Action 验证与 Artifact** — 2 条
- **Permissions** — 3 条
- **Approvals** — 3 条
- **Tasks 与审计** — 5 条
- **Audit 查询** — 1 条
- **Connector（External Browser）** — 10 条
- **Candidate Extractions** — 5 条

## 5. 集合 C · 废弃或合并范围

**当前判定：可能为空。** 见 §1。

B0 须对 10 条无 UI 调用路由逐条查证调用方（浏览器扩展 / MCP / 脚本），
确认其是否真的无调用方，才能判定是否废弃。

## 6. 下一步

1. B0 完成本表的待填列（Agent/MCP 调用、集合归属、W7/BW0 归属、保留/废弃）。
2. B0 输出字段级差异报告（现有 12 张表 vs 提议的 11 张）。
3. B0 结论确定后立 **BW0** 卡（Browser API Compatibility Wave），确定实际迁移范围。
