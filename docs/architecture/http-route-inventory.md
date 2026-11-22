# ICA-RUST 全量 HTTP 路由盘点 — ARCH-P0 / AC2

**基线 HEAD（完整 SHA）：** `7213f424c51f1a20f2b911bc5d6f651dd0244c3f`（短 SHA `7213f42`）
**分支 / worktree：** `wt/ica-rust-arch-p0` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0`
**取数时点：** 2026-11-22（CST） · **性质：** 纯只读盘点。

## 1. 取数命令原文与实测数字

```bash
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0
PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH -u PYTHONHOME \
  /mnt/o/ica/backend/.venv/bin/python scripts/endpoint_reconciliation.py --json
PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH -u PYTHONHOME \
  /home/bryan/projects/ica/backend/.venv/bin/python scripts/check_endpoint_map.py \
  --backend /home/bryan/projects/ica/backend
```

**路径偏差说明（如实记录）：** 任务单指定的 `/mnt/o/ica/backend/.venv/bin/python` 在本次现场是不完整镜像
（`.venv` 只有 `lib/python3.12/site-packages/rapidocr`，无 `bin/python`，`/mnt/o/ica/backend/app` 不存在），
原样命令 exit 2。`endpoint_reconciliation.py` 为纯标准库脚本，改用 `/home/bryan/.local/bin/python3.11`
并以**同样的 env guard** 执行，输出不受解释器差异影响。`check_endpoint_map.py` 需导入 Python baseline 的
`app.main`，改用真实基线 `/home/bryan/projects/ica`（HEAD `9ed173cdc6736f1213fc5697537c7f2e3a433709`，
与 S0 报告一致）下带 FastAPI 0.133.0 的 `.venv/bin/python`，并显式 `--backend` 指向该目录。
未对任何基线仓执行写操作（`PYTHONDONTWRITEBYTECODE=1` + `env -u PYTHONPATH -u PYTHONHOME`）。

### endpoint_reconciliation.py --json 原样计数

| 指标 | 值 |
|---|---|
| contract_total | 68 |
| implemented | 68 |
| missing | 0 |
| extra_routes | 0 |
| auxiliary_routes | 1 |
| non_contract_routes | 1 |
| matrix_rows | 69 |
| coverage | contract 68/68；implementation 68/69；verification 68/69 |
| non_contract | `GET /health` |

### check_endpoint_map.py 原样输出

```
imported app.main from: /home/bryan/projects/ica/backend/app/main.py
live /api routes:        112
excluded (out of scope): 39   prefix: /api/browser/
excluded (ADR-0008):     5   contract-external exact routes
compared live routes:    68
documented rows:         68

RESULT: table matches the in-scope live route table exactly
```

**对账等式：** 112 live = 68 契约 + 5 ADR-0008 契约外 + 39 browser 族（另有 Rust 侧 `GET /health`
为 health 辅助路由，不计入 `/api` 契约对比）。

## 2. 68 契约路由 + 1 辅助路由：逐条入口 / Use Case 映射

入口提取自 `crates/ica-api/src/routes.rs` 的 `router()` 路由表（52 条 `.route()`、70 个 method-handler
绑定）；Use Case 提取自各 handler 函数体的 `ica_application::…` 调用。列「模块」来自
endpoint_reconciliation 的 contract 表。

| # | Method | Path (contract) | Rust handler | Application Use Case | 模块 | Wave |
|---|---|---|---|---|---|---|
| 1 | GET | `/ai/models` | `list_ai_models` | `list_ai_models` | ica-application/ai.rs | W5 |
| 2 | GET | `/ai/routing` | `get_ai_routing` | `get_ai_routing` | ica-application/ai.rs | W5 |
| 3 | PATCH | `/ai/routing` | `update_ai_routing` | `update_ai_routing` | ica-application/ai.rs | W5 |
| 4 | GET | `/ai/usage` | `get_ai_usage` | `get_ai_usage` | ica-application/ai.rs | W5 |
| 5 | GET | `/ai/budget` | `get_ai_budget` | `today_spend_cents, budget_read` | ica-application/ai.rs | W5 |
| 6 | PATCH | `/ai/budget` | `update_ai_budget` | `today_spend_cents, budget_read` | ica-application/ai.rs | W5 |
| 7 | POST | `/auth/register` | `register_endpoint` | `register` | ica-application/auth.rs | W6 |
| 8 | POST | `/auth/login` | `login_endpoint` | `login` | ica-application/auth.rs | W6 |
| 9 | POST | `/auth/refresh` | `refresh_endpoint` | `refresh` | ica-application/auth.rs | W6 |
| 10 | POST | `/auth/logout` | `logout_endpoint` | `logout` | ica-application/auth.rs | W6 |
| 11 | GET | `/auth/me` | `me_endpoint` | `local_user` | ica-application/auth.rs | W6 |
| 12 | POST | `/shares` | `create_share_endpoint` | `local_user, create_share` | ica-application/shares.rs | W6 |
| 13 | GET | `/shares` | `list_shares_endpoint` | `local_user, list_shares` | ica-application/shares.rs | W6 |
| 14 | DELETE | `/shares/{share_id}` | `delete_share_endpoint` | `local_user, delete_share` | ica-application/shares.rs | W6 |
| 15 | GET | `/candidates` | `list_candidates` | `candidates::list_candidates` | ica-api/routes.rs + ica-application/candidates.rs + ica-infra/candidates.rs | ? |
| 16 | POST | `/candidates` | `create_candidate` | `candidates::create_candidate` | ica-application/candidates.rs + ica-infra/candidates.rs | ? |
| 17 | GET | `/candidates/{candidate_id}` | `get_candidate` | `candidates::get_candidate_detail` | ica-application/candidates.rs + ica-infra/candidates.rs | ? |
| 18 | PATCH | `/candidates/{candidate_id}` | `update_candidate` | `candidates::update_candidate` | ica-application/candidates.rs + ica-infra/candidates.rs | ? |
| 19 | DELETE | `/candidates/{candidate_id}` | `delete_candidate` | `candidates::delete_candidate` | ica-application/candidates.rs + ica-infra/candidates.rs | ? |
| 20 | GET | `/conversations` | `list_conversations` | `conversations::list_conversations` | ica-application/conversations.rs + ica-infra/conversations.rs | ? |
| 21 | POST | `/conversations` | `create_conversation` | `conversations::create_conversation` | ica-application/conversations.rs + ica-infra/conversations.rs | ? |
| 22 | GET | `/messages` | `list_messages` | `conversations::list_messages` | ica-application/conversations.rs + ica-infra/conversations.rs | ? |
| 23 | POST | `/messages` | `create_message` | `conversations::create_message` | ica-application/conversations.rs + ica-infra/conversations.rs | ? |
| 24 | GET | `/candidates/{candidate_id}/timeline` | `candidate_timeline` | `conversations::candidate_timeline` | ica-application/conversations.rs + ica-infra/conversations.rs | ? |
| 25 | GET | `/dashboard` | `dashboard` | `dashboard` | ica-application/dashboard.rs | ? |
| 26 | GET | `/drafts` | `list_drafts` | `list_drafts` | ica-application/drafts.rs | ? |
| 27 | GET | `/drafts/{draft_id}` | `get_draft` | `get_draft` | ica-application/drafts.rs | ? |
| 28 | POST | `/drafts/generate` | `generate_draft` | `generate` | ica-application/drafts.rs | ? |
| 29 | GET | `/candidates/{candidate_id}/context` | `preview_context` | `preview_context` | ica-application/drafts.rs | ? |
| 30 | PATCH | `/drafts/{draft_id}` | `update_draft` | `update_draft` | ica-application/drafts.rs | ? |
| 31 | POST | `/drafts/{draft_id}/approve` | `approve_draft` | `approve_draft` | ica-application/drafts.rs | ? |
| 32 | POST | `/drafts/{draft_id}/reject` | `reject_draft` | `reject_draft` | ica-application/drafts.rs | ? |
| 33 | POST | `/drafts/{draft_id}/copy` | `copy_draft` | `copy_draft` | ica-application/drafts.rs | ? |
| 34 | GET | `/draft-groups/{group_id}` | `get_draft_group` | `get_draft_group` | ica-application/drafts.rs | ? |
| 35 | POST | `/draft-groups/{group_id}/select` | `select_draft_from_group` | `select_draft_from_group` | ica-application/drafts.rs | ? |
| 36 | POST | `/follow-ups/evaluate` | `follow_up_evaluate` | `evaluate` | ica-application/follow_ups.rs | ? |
| 37 | GET | `/follow-ups` | `list_follow_ups` | `list_tasks` | ica-application/follow_ups.rs | ? |
| 38 | POST | `/follow-ups` | `create_follow_up` | `create_task` | ica-application/follow_ups.rs | ? |
| 39 | GET | `/follow-ups/{task_id}` | `get_follow_up` | `get_task` | ica-application/follow_ups.rs | ? |
| 40 | PATCH | `/follow-ups/{task_id}` | `update_follow_up` | `update_task` | ica-application/follow_ups.rs | ? |
| 41 | POST | `/follow-ups/{task_id}/cancel` | `cancel_follow_up` | `cancel_task` | ica-application/follow_ups.rs | ? |
| 42 | POST | `/follow-ups/{task_id}/complete` | `complete_follow_up` | `complete_task` | ica-application/follow_ups.rs | ? |
| 43 | POST | `/follow-ups/{task_id}/record-result` | `record_follow_up_result` | `record_result` | ica-application/follow_ups.rs | ? |
| 44 | GET | `/follow-ups/policy/current` | `get_follow_up_policy` | `ensure_default_policy` | ica-application/follow_ups.rs | ? |
| 45 | PATCH | `/follow-ups/policy/current` | `update_follow_up_policy` | `update_policy` | ica-application/follow_ups.rs | ? |
| 46 | POST | `/imports/preview` | `preview_import` | `preview_import` | ica-application/imports.rs | ? |
| 47 | GET | `/imports/{session_id}` | `get_import` | `(port/view 直连)` | ica-application/imports.rs | ? |
| 48 | POST | `/imports/{session_id}/confirm` | `confirm_import` | `confirm_import` | ica-application/imports.rs | ? |
| 49 | POST | `/imports/{session_id}/reject` | `reject_import` | `reject_import` | ica-application/imports.rs | ? |
| 50 | POST | `/export` | `export_data` | `export_data` | ica-application/sync.rs | ? |
| 51 | POST | `/import` | `import_data` | `import_data` | ica-application/sync.rs | ? |
| 52 | POST | `/imports/ocr` | `ocr_import` | `ocr_screenshot` | ica-infra/ocr.rs + ica-application/imports.rs | ? |
| 53 | GET | `/positions` | `list_positions` | `list_positions` | ica-api/routes.rs + ica-application/positions.rs + ica-infra/repository.rs | ? |
| 54 | POST | `/positions` | `create_position` | `positions::create_position` | ica-application/positions.rs + ica-infra/repository.rs | ? |
| 55 | GET | `/positions/{position_id}` | `get_position` | `get_position` | ica-api/routes.rs + ica-application/positions.rs + ica-infra/repository.rs | ? |
| 56 | PATCH | `/positions/{position_id}` | `update_position` | `positions::update_position` | ica-application/positions.rs + ica-infra/repository.rs | ? |
| 57 | DELETE | `/positions/{position_id}` | `delete_position` | `positions::delete_position` | ica-application/positions.rs + ica-infra/repository.rs | ? |
| 58 | POST | `/send-records` | `record_send` | `send_records::record_send` | ica-application/send_records.rs + ica-infra/send_records.rs | ? |
| 59 | GET | `/send-records` | `list_send_records` | `send_records::list_send_records` | ica-application/send_records.rs + ica-infra/send_records.rs | ? |
| 60 | GET | `/settings` | `read_settings` | `(port/view 直连)` | ica-application/settings.rs | W5 |
| 61 | PUT | `/settings/ai-key` | `write_ai_key_endpoint` | `write_ai_key` | ica-application/settings.rs | W5 |
| 62 | DELETE | `/settings/ai-key` | `clear_ai_key_endpoint` | `clear_ai_key` | ica-application/settings.rs | W5 |
| 63 | GET | `/settings/ai-egress` | `read_ai_egress` | `read_egress_disclosure` | ica-application/settings.rs | W5 |
| 64 | POST | `/settings/ai-egress/consent` | `write_ai_egress_consent` | `write_egress_consent` | ica-application/settings.rs | W5 |
| 65 | GET | `/audit-events` | `audit_events` | `recent_audit_events` | ica-application/settings.rs | W5 |
| 66 | GET | `/backups` | `list_backups_endpoint` | `list_backups` | ica-application/backups.rs | W5 |
| 67 | POST | `/backups` | `create_backup_endpoint` | `create_backup` | ica-application/backups.rs | W5 |
| 68 | POST | `/backups/{name}/restore` | `restore_backup_endpoint` | `list_backups, restore_backup` | ica-application/backups.rs | W5 |
| — | POST | `/api/imports/{}/confirm-candidate` | `confirm_candidate_import` | `confirm_candidate` | (router only) | W7 |

> 注：`preview_context` / `get_import` / `read_settings` 等少数 handler 直接走 `state.<port>()` getter
> 或 config view，没有独立的 `ica_application::<fn>` 调用点；表中如实标为 `(port/view 直连)`，
> 其业务归属仍与「模块」列一致。

## 3. 契约外差额（5 + 1 + 1）

### 3.1 ADR-0008 契约外精确路由（5 条，Python live，Rust 不实现）

| Method | Path | 归属 |
|---|---|---|
| DELETE | `/api/ai/models/{model_id:path}` | AI-EXT / W7 辅助 |
| POST | `/api/ai/models` | AI-EXT / W7 辅助 |
| POST | `/api/imports/{session_id}/confirm-candidate` | AI-EXT / W7 辅助 |
| PUT | `/api/ai/models/{model_id:path}` | AI-EXT / W7 辅助 |
| PUT | `/api/ai/models/{model_id:path}/default` | AI-EXT / W7 辅助 |

### 3.2 Rust 侧辅助路由（1 条）

`POST /api/imports/{session_id}/confirm-candidate` → handler `confirm_candidate_import` →
`ica_application::confirm_candidate`，scope `Auxiliary (D-2)`，wave W7，verification Unverified，
baseline_sha `9ed173c`。它是 reconciliation 的 `auxiliary_routes=1`，也是 check_endpoint_map 的
ADR-0008 第 5 条——两脚本从不同方向记录同一条路由，不重复计数。

### 3.3 非契约路由（1 条）

`GET /health` → handler `health`，仅返回 `status/app/ai_enabled`，auxiliary/non-contract，不计入 68。

## 4. Browser 族（39 条 `/api/browser/v1/*`，全部 out of scope）

check_endpoint_map 实跑列出 39 条，全部被 `--exclude-prefix /api/browser/` 排除；
与 `docs/architecture/browser/browser-route-inventory-r1.md` 的 39 条终裁集合一致
（源 `backend/app/api/browser.py` @ `9ed173c`，归 BW0，集合 C 为空，W7 命中 0）。

| Method | Path |
|---|---|
| DELETE | `/api/browser/v1/permissions/{permission_id}` |
| DELETE | `/api/browser/v1/sessions/{session_id}` |
| GET | `/api/browser/v1/approvals/{approval_id}` |
| GET | `/api/browser/v1/artifacts/{artifact_id}` |
| GET | `/api/browser/v1/audit` |
| GET | `/api/browser/v1/candidate-extractions` |
| GET | `/api/browser/v1/candidate-extractions/{extraction_id}` |
| GET | `/api/browser/v1/connector/audit` |
| GET | `/api/browser/v1/connector/status` |
| GET | `/api/browser/v1/connector/tabs` |
| GET | `/api/browser/v1/permissions` |
| GET | `/api/browser/v1/runtime/status` |
| GET | `/api/browser/v1/tasks/{task_id}` |
| GET | `/api/browser/v1/tasks/{task_id}/audit` |
| PATCH | `/api/browser/v1/candidate-extractions/{extraction_id}` |
| POST | `/api/browser/v1/actions/{action_id}/verify` |
| POST | `/api/browser/v1/approvals` |
| POST | `/api/browser/v1/approvals/{approval_id}/decide` |
| POST | `/api/browser/v1/candidate-extractions` |
| POST | `/api/browser/v1/candidate-extractions/{extraction_id}/review` |
| POST | `/api/browser/v1/connector/act` |
| POST | `/api/browser/v1/connector/pair` |
| POST | `/api/browser/v1/connector/rotate` |
| POST | `/api/browser/v1/connector/start` |
| POST | `/api/browser/v1/connector/stop` |
| POST | `/api/browser/v1/connector/tabs/grant` |
| POST | `/api/browser/v1/connector/tabs/revoke` |
| POST | `/api/browser/v1/permissions` |
| POST | `/api/browser/v1/runtime/start` |
| POST | `/api/browser/v1/runtime/stop` |
| POST | `/api/browser/v1/sessions` |
| POST | `/api/browser/v1/sessions/{session_id}/actions` |
| POST | `/api/browser/v1/sessions/{session_id}/hybrid/click` |
| POST | `/api/browser/v1/sessions/{session_id}/navigate` |
| POST | `/api/browser/v1/sessions/{session_id}/screenshot` |
| POST | `/api/browser/v1/sessions/{session_id}/snapshot` |
| POST | `/api/browser/v1/tasks` |
| POST | `/api/browser/v1/tasks/{task_id}/cancel` |
| POST | `/api/browser/v1/tasks/{task_id}/run` |

Rust 侧 browser 业务实现 = 0：`crates/ica-api/src/routes.rs` 无 browser 路由（仅 906-907 行注释），
browser 请求由 Host 按 `route_backend()` 转发给 Python sidecar（见 AC5 文档）。

**结论：AC2 VERIFIED。** 68 契约路由全部有 handler 与 Use Case 映射；契约外 5+1+1、browser 39 条均逐条列出，数字绑定 HEAD 7213f424c51f1a20f2b911bc5d6f651dd0244c3f。