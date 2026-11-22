# ICA-RUST 前端 Backend Client 调用点盘点 — ARCH-P0 / AC4

**基线 HEAD（完整 SHA）：** `7213f424c51f1a20f2b911bc5d6f651dd0244c3f`（短 SHA `7213f42`）
**分支 / worktree：** `wt/ica-rust-arch-p0` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0`
**取数时点：** 2026-11-22（CST） · **性质：** 纯只读盘点。

## 1. 取数命令原文

```bash
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0
grep -Ec 'api\.(get|post|put|patch|delete)\s*<' frontend/src/services/endpoints.ts   # 90
grep -rEo 'backend\.[a-zA-Z]+\.[a-zA-Z]+\(' frontend/src/pages frontend/src/components \
  frontend/src/auth frontend/src/App.tsx --include=*.ts --include=*.tsx | wc -l          # 95
grep -cE '^pub async fn ' crates/ica-desktop/src/commands.rs                              # 70
cat frontend/src/services/backend/index.ts frontend/src/services/backend/client.ts \
    frontend/src/services/backend/http.ts frontend/src/services/backend/ipc.ts
```

## 2. 调用面结构

`frontend/src/services/backend/index.ts` 是唯一入口，导出 `backend: BackendClient`：

```ts
export const backend: BackendClient = isTauri
  ? { ...httpBackend, positions: ipcPositions }   // Tauri 窗口：positions 走 IPC
  : httpBackend;                                     // 其它环境：全量 HTTP
```

- `client.ts`：`BackendClient = HttpBackend`（HTTP 形状是合同，IPC adapter 必须逐方法对齐，缺失即编译错误）。
- `http.ts`：`httpBackend` 由 13 个 `xxxApi` 组装配，共 **90 个 API 调用点**（`endpoints.ts` 中
  `api.get/post/put/patch/delete<...>` 每个一次）。
- `ipc.ts`：`ipcPositions` 5 个方法 → `invokeOrThrow("positions_<op>", …)`，与 AC3 的 5 条注册命令一一对应。
- 页面/组件层另有 **95 处** `backend.<domain>.<method>(...)` 消费点（非新增 API，是上述 90 点的调用方）。

### 按 domain 分组（90 个调用点）

| API 组 | 调用点数 |
|---|---|
| `candidatesApi` | 7 |
| `positionsApi` | 5 |
| `conversationsApi` | 2 |
| `messagesApi` | 2 |
| `importsApi` | 4 |
| `followUpsApi` | 10 |
| `draftsApi` | 8 |
| `draftGroupsApi` | 2 |
| `aiApi` | 7 |
| `dashboardApi` | 1 |
| `authApi` | 5 |
| `settingsApi` | 9 |
| `browserApi` | 28 |

### 传输归属汇总

| 传输 | 后端 | 调用点数 |
|---|---|---|
| IPC（仅 Tauri 窗口内 positions 分支） | Rust DesktopState（进程内） | 5 |
| HTTP | Rust ica-server（68 契约 + 辅助路由） | 54 |
| HTTP | Python sidecar（browser 族 27 + AI-EXT 4） | 31 |
| **合计** | | **90** |

## 3. 90 个调用点逐条映射

列「Rust 入口 / Use Case」取自 `crates/ica-api/src/routes.rs` handler 的 `ica_application::…` 调用；
Python sidecar 目标按 Host 的 `route_backend()` 规则判定（见 AC5 文档）。

| API 组 | TS 方法 | Verb | Path | 传输 | 后端目标 | Rust 入口 / Use Case |
|---|---|---|---|---|---|---|
| candidatesApi | `list` | GET | `/candidates${query(params)}` | HTTP | Rust ica-server | `candidates::list_candidates` |
| candidatesApi | `get` | GET | `/candidates/${id}` | HTTP | Rust ica-server | `candidates::get_candidate_detail` |
| candidatesApi | `create` | POST | `/candidates` | HTTP | Rust ica-server | `candidates::create_candidate` |
| candidatesApi | `update` | PATCH | `/candidates/${id}` | HTTP | Rust ica-server | `candidates::update_candidate` |
| candidatesApi | `remove` | DELETE | `/candidates/${id}` | HTTP | Rust ica-server | `candidates::delete_candidate` |
| candidatesApi | `timeline` | GET | `/candidates/${id}/timeline` | HTTP | Rust ica-server | `conversations::candidate_timeline` |
| candidatesApi | `context` | GET | `/candidates/${id}/context` | HTTP | Rust ica-server | `preview_context` |
| positionsApi | `list` | GET | `/positions${query(params)}` | IPC | Rust DesktopState（进程内） | `list_positions` |
| positionsApi | `get` | GET | `/positions/${id}` | IPC | Rust DesktopState（进程内） | `get_position` |
| positionsApi | `create` | POST | `/positions` | IPC | Rust DesktopState（进程内） | `positions::create_position` |
| positionsApi | `update` | PATCH | `/positions/${id}` | IPC | Rust DesktopState（进程内） | `positions::update_position` |
| positionsApi | `remove` | DELETE | `/positions/${id}` | IPC | Rust DesktopState（进程内） | `positions::delete_position` |
| conversationsApi | `list` | GET | `/conversations${query(params)}` | HTTP | Rust ica-server | `conversations::list_conversations` |
| conversationsApi | `create` | POST | `/conversations` | HTTP | Rust ica-server | `conversations::create_conversation` |
| messagesApi | `list` | GET | `/messages${query(params)}` | HTTP | Rust ica-server | `conversations::list_messages` |
| messagesApi | `create` | POST | `/messages` | HTTP | Rust ica-server | `conversations::create_message` |
| importsApi | `preview` | POST | `/imports/preview` | HTTP | Rust ica-server | `preview_import` |
| importsApi | `get` | GET | `/imports/${id}` | HTTP | Rust ica-server | `(port/view 直连)` |
| importsApi | `confirm` | POST | `/imports/${id}/confirm` | HTTP | Rust ica-server | `confirm_import` |
| importsApi | `reject` | POST | `/imports/${id}/reject` | HTTP | Rust ica-server | `reject_import` |
| followUpsApi | `list` | GET | `/follow-ups${query(params)}` | HTTP | Rust ica-server | `list_tasks` |
| followUpsApi | `get` | GET | `/follow-ups/${id}` | HTTP | Rust ica-server | `get_task` |
| followUpsApi | `evaluate` | POST | `/follow-ups/evaluate` | HTTP | Rust ica-server | `evaluate` |
| followUpsApi | `create` | POST | `/follow-ups` | HTTP | Rust ica-server | `create_task` |
| followUpsApi | `update` | PATCH | `/follow-ups/${id}` | HTTP | Rust ica-server | `update_task` |
| followUpsApi | `cancel` | POST | `/follow-ups/${id}/cancel` | HTTP | Rust ica-server | `cancel_task` |
| followUpsApi | `complete` | POST | `/follow-ups/${id}/complete` | HTTP | Rust ica-server | `complete_task` |
| followUpsApi | `recordResult` | POST | `/follow-ups/${id}/record-result` | HTTP | Rust ica-server | `record_result` |
| followUpsApi | `getPolicy` | GET | `/follow-ups/policy/current` | HTTP | Rust ica-server | `ensure_default_policy` |
| followUpsApi | `updatePolicy` | PATCH | `/follow-ups/policy/current` | HTTP | Rust ica-server | `update_policy` |
| draftsApi | `list` | GET | `/drafts${query(params)}` | HTTP | Rust ica-server | `list_drafts` |
| draftsApi | `get` | GET | `/drafts/${id}` | HTTP | Rust ica-server | `get_draft` |
| draftsApi | `generate` | POST | `/drafts/generate` | HTTP | Rust ica-server | `generate` |
| draftsApi | `generateComparison` | POST | `/drafts/generate` | HTTP | Rust ica-server | `generate` |
| draftsApi | `update` | PATCH | `/drafts/${id}` | HTTP | Rust ica-server | `update_draft` |
| draftsApi | `approve` | POST | `/drafts/${id}/approve` | HTTP | Rust ica-server | `approve_draft` |
| draftsApi | `reject` | POST | `/drafts/${id}/reject` | HTTP | Rust ica-server | `reject_draft` |
| draftsApi | `copy` | POST | `/drafts/${id}/copy` | HTTP | Rust ica-server | `copy_draft` |
| draftGroupsApi | `get` | GET | `/draft-groups/${groupId}` | HTTP | Rust ica-server | `get_draft_group` |
| draftGroupsApi | `select` | POST | `/draft-groups/${groupId}/select` | HTTP | Rust ica-server | `select_draft_from_group` |
| aiApi | `listModels` | GET | `/ai/models` | HTTP | Rust ica-server | `list_ai_models` |
| aiApi | `getRouting` | GET | `/ai/routing` | HTTP | Rust ica-server | `get_ai_routing` |
| aiApi | `updateRouting` | PATCH | `/ai/routing` | HTTP | Rust ica-server | `update_ai_routing` |
| aiApi | `createModel` | POST | `/ai/models` | HTTP | Python sidecar | `(AI-EXT model CRUD，Rust 无实现)` |
| aiApi | `updateModel` | PUT | `/ai/models/${encodeURIComponent(modelId)}` | HTTP | Python sidecar | `(AI-EXT model CRUD，Rust 无实现)` |
| aiApi | `setDefaultModel` | PUT | `/ai/models/${encodeURIComponent(modelId)}/default` | HTTP | Python sidecar | `(AI-EXT model CRUD，Rust 无实现)` |
| aiApi | `deleteModel` | DELETE | `/ai/models/${encodeURIComponent(modelId)}` | HTTP | Python sidecar | `(AI-EXT model CRUD，Rust 无实现)` |
| dashboardApi | `get` | GET | `/dashboard` | HTTP | Rust ica-server | `dashboard` |
| authApi | `me` | GET | `/auth/me` | HTTP | Rust ica-server | `local_user` |
| authApi | `login` | POST | `/auth/login` | HTTP | Rust ica-server | `login` |
| authApi | `register` | POST | `/auth/register` | HTTP | Rust ica-server | `register` |
| authApi | `refresh` | POST | `/auth/refresh` | HTTP | Rust ica-server | `refresh` |
| authApi | `logout` | POST | `/auth/logout` | HTTP | Rust ica-server | `logout` |
| settingsApi | `get` | GET | `/settings` | HTTP | Rust ica-server | `(port/view 直连)` |
| settingsApi | `getAiEgress` | GET | `/settings/ai-egress` | HTTP | Rust ica-server | `read_egress_disclosure` |
| settingsApi | `setAiEgressConsent` | POST | `/settings/ai-egress/consent` | HTTP | Rust ica-server | `write_egress_consent` |
| settingsApi | `setAiKey` | PUT | `/settings/ai-key` | HTTP | Rust ica-server | `write_ai_key` |
| settingsApi | `clearAiKey` | DELETE | `/settings/ai-key` | HTTP | Rust ica-server | `clear_ai_key` |
| settingsApi | `listBackups` | GET | `/backups` | HTTP | Rust ica-server | `list_backups` |
| settingsApi | `createBackup` | POST | `/backups` | HTTP | Rust ica-server | `create_backup` |
| settingsApi | `restore` | POST | `/backups/${encodeURIComponent(name)}/restore` | HTTP | Rust ica-server | `list_backups, restore_backup` |
| settingsApi | `listAuditEvents` | GET | `/audit-events${query(params)}` | HTTP | Rust ica-server | `recent_audit_events` |
| browserApi | `runtimeStatus` | GET | `/browser/v1/runtime/status` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `runtimeStart` | POST | `/browser/v1/runtime/start` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `runtimeStop` | POST | `/browser/v1/runtime/stop` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `createSession` | POST | `/browser/v1/sessions` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `closeSession` | DELETE | `/browser/v1/sessions/${sessionId}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `navigate` | POST | `/browser/v1/sessions/${sessionId}/navigate` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `snapshot` | POST | `/browser/v1/sessions/${sessionId}/snapshot` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `screenshot` | POST | `/browser/v1/sessions/${sessionId}/screenshot` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `executeAction` | POST | `/browser/v1/sessions/${sessionId}/actions` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `verifyAction` | POST | `/browser/v1/actions/${actionId}/verify` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `listPermissions` | GET | `/browser/v1/permissions` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `grantPermission` | POST | `/browser/v1/permissions` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `revokePermission` | DELETE | `/browser/v1/permissions/${permissionId}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `requestApproval` | POST | `/browser/v1/approvals` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `getApproval` | GET | `/browser/v1/approvals/${approvalId}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `decideApproval` | POST | `/browser/v1/approvals/${approvalId}/decide` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `createTask` | POST | `/browser/v1/tasks` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `getTask` | GET | `/browser/v1/tasks/${taskId}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `runTask` | POST | `/browser/v1/tasks/${taskId}/run` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `cancelTask` | POST | `/browser/v1/tasks/${taskId}/cancel` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `taskAudit` | GET | `/browser/v1/tasks/${taskId}/audit` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `audit` | GET | `/browser/v1/audit${query(params)}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `connectorStatus` | GET | `/browser/v1/connector/status` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `listExtractions` | GET | `/browser/v1/candidate-extractions` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `getExtraction` | GET | `/browser/v1/candidate-extractions/${id}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `patchExtraction` | PATCH | `/browser/v1/candidate-extractions/${id}` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `reviewExtraction` | POST | `/browser/v1/candidate-extractions/${id}/review` | HTTP | Python sidecar | `(browser 路由，Rust 无实现)` |
| browserApi | `confirmExtraction` | POST | `/imports/${encodeURIComponent(importSessionId)}/confirm-candidate` | HTTP | Rust ica-server | `confirm_candidate` |

> 注 1：`settingsApi.clearAiKey` 的模板串带 `?model_id=` 可选查询，表中归一为 `/settings/ai-key`。
> 注 2：`draftsApi.generate` 与 `generateComparison` 指向同一 `POST /drafts/generate`（两个前端方法，一个入口）。
> 注 3：`browserApi.confirmExtraction` 走 `/imports/{importSessionId}/confirm-candidate`，
> 虽在 browserApi 组内，但按 Host 路由规则落 Rust（W7 辅助路由），不是 Python sidecar。

**结论：AC4 VERIFIED。** 90 个调用点逐条标注 HTTP/IPC 与业务接口映射，前端唯一入口/传输分流路径已核实，数字绑定 HEAD 7213f424c51f1a20f2b911bc5d6f651dd0244c3f。
