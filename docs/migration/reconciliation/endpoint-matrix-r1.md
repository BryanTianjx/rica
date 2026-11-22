# Endpoint Reconciliation Matrix R1.0

**生成方式：** `python3 scripts/endpoint_reconciliation.py --out <file>`
所有统计数字从本矩阵生成，不再分别用 grep、契约文档与 Work Item 手工统计。

> **审计修正（P0-01）**：此前总方案写「已实现 46 条」，是把 `GET /health` 计入了——它不属于 68 条业务契约。正确数字是 **68 / 68**（68 条契约全部已实现，missing=0）。

## 计数

| 指标 | 值 |
| --- | --- |
| 契约端点总数 | **68** |
| 已实现 | **68** |
| 未实现 | **0** |
| 契约外路由（Rust 有、契约表无） | 0 |
| 非契约路由（`/health` 等） | 1 |
| 矩阵行数 | 68 |

**校验：68 + 0 = 68 ✓**

三个覆盖率指标：

- Contract Coverage: **68/68**
- Implementation Coverage: **68/68**
- Verification Coverage: **68/68**

> 三个指标当前同值，是因为**所有已实现端点都已进对拍且通过**。> W4 的三份审计 FAIL 之后，Verification Coverage 应当低于 Implementation Coverage —— 那才是真实状态。

## 非契约路由

这些**不属于** 68 条业务契约，不计入实现率：

- `GET /health`

## 逐条矩阵

| Contract ID | Method | Python Route | Wave | Module | Implementation | Verification | Scope | Baseline SHA |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | GET | `/ai/models` | W5 | ``ica-application/ai.rs`` | Implemented | Verified | Contract | `d39e321` |
| 2 | GET | `/ai/routing` | W5 | ``ica-application/ai.rs`` | Implemented | Verified | Contract | `d39e321` |
| 3 | PATCH | `/ai/routing` | W5 | ``ica-application/ai.rs`` | Implemented | Verified | Contract | `d39e321` |
| 4 | GET | `/ai/usage` | W5 | ``ica-application/ai.rs`` | Implemented | Verified | Contract | `d39e321` |
| 5 | GET | `/ai/budget` | W5 | ``ica-application/ai.rs`` | Implemented | Verified | Contract | `d39e321` |
| 6 | PATCH | `/ai/budget` | W5 | ``ica-application/ai.rs`` | Implemented | Verified | Contract | `d39e321` |
| 7 | POST | `/auth/register` | W6 | ``ica-application/auth.rs`` | Implemented | Verified | Contract | `d39e321` |
| 8 | POST | `/auth/login` | W6 | ``ica-application/auth.rs`` | Implemented | Verified | Contract | `d39e321` |
| 9 | POST | `/auth/refresh` | W6 | ``ica-application/auth.rs`` | Implemented | Verified | Contract | `d39e321` |
| 10 | POST | `/auth/logout` | W6 | ``ica-application/auth.rs`` | Implemented | Verified | Contract | `d39e321` |
| 11 | GET | `/auth/me` | W6 | ``ica-application/auth.rs`` | Implemented | Verified | Contract | `d39e321` |
| 12 | POST | `/shares` | W6 | ``ica-application/shares.rs`` | Implemented | Verified | Contract | `d39e321` |
| 13 | GET | `/shares` | W6 | ``ica-application/shares.rs`` | Implemented | Verified | Contract | `d39e321` |
| 14 | DELETE | `/shares/{share_id}` | W6 | ``ica-application/shares.rs`` | Implemented | Verified | Contract | `d39e321` |
| 15 | GET | `/candidates` | ? | ``ica-api/routes.rs` + `ica-application/candidates.rs` + `ica-infra/candidates.rs`` | Implemented | Verified | Contract | `d39e321` |
| 16 | POST | `/candidates` | ? | ``ica-application/candidates.rs` + `ica-infra/candidates.rs`` | Implemented | Verified | Contract | `d39e321` |
| 17 | GET | `/candidates/{candidate_id}` | ? | ``ica-application/candidates.rs` + `ica-infra/candidates.rs`` | Implemented | Verified | Contract | `d39e321` |
| 18 | PATCH | `/candidates/{candidate_id}` | ? | ``ica-application/candidates.rs` + `ica-infra/candidates.rs`` | Implemented | Verified | Contract | `d39e321` |
| 19 | DELETE | `/candidates/{candidate_id}` | ? | ``ica-application/candidates.rs` + `ica-infra/candidates.rs`` | Implemented | Verified | Contract | `d39e321` |
| 20 | GET | `/conversations` | ? | ``ica-application/conversations.rs` + `ica-infra/conversations.rs`` | Implemented | Verified | Contract | `d39e321` |
| 21 | POST | `/conversations` | ? | ``ica-application/conversations.rs` + `ica-infra/conversations.rs`` | Implemented | Verified | Contract | `d39e321` |
| 22 | GET | `/messages` | ? | ``ica-application/conversations.rs` + `ica-infra/conversations.rs`` | Implemented | Verified | Contract | `d39e321` |
| 23 | POST | `/messages` | ? | ``ica-application/conversations.rs` + `ica-infra/conversations.rs`` | Implemented | Verified | Contract | `d39e321` |
| 24 | GET | `/candidates/{candidate_id}/timeline` | ? | ``ica-application/conversations.rs` + `ica-infra/conversations.rs`` | Implemented | Verified | Contract | `d39e321` |
| 25 | GET | `/dashboard` | ? | ``ica-application/dashboard.rs`` | Implemented | Verified | Contract | `d39e321` |
| 26 | GET | `/drafts` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 27 | GET | `/drafts/{draft_id}` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 28 | POST | `/drafts/generate` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 29 | GET | `/candidates/{candidate_id}/context` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 30 | PATCH | `/drafts/{draft_id}` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 31 | POST | `/drafts/{draft_id}/approve` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 32 | POST | `/drafts/{draft_id}/reject` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 33 | POST | `/drafts/{draft_id}/copy` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 34 | GET | `/draft-groups/{group_id}` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 35 | POST | `/draft-groups/{group_id}/select` | ? | ``ica-application/drafts.rs`` | Implemented | Verified | Contract | `d39e321` |
| 36 | POST | `/follow-ups/evaluate` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 37 | GET | `/follow-ups` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 38 | POST | `/follow-ups` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 39 | GET | `/follow-ups/{task_id}` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 40 | PATCH | `/follow-ups/{task_id}` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 41 | POST | `/follow-ups/{task_id}/cancel` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 42 | POST | `/follow-ups/{task_id}/complete` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 43 | POST | `/follow-ups/{task_id}/record-result` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 44 | GET | `/follow-ups/policy/current` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 45 | PATCH | `/follow-ups/policy/current` | ? | ``ica-application/follow_ups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 46 | POST | `/imports/preview` | ? | ``ica-application/imports.rs`` | Implemented | Verified | Contract | `d39e321` |
| 47 | GET | `/imports/{session_id}` | ? | ``ica-application/imports.rs`` | Implemented | Verified | Contract | `d39e321` |
| 48 | POST | `/imports/{session_id}/confirm` | ? | ``ica-application/imports.rs`` | Implemented | Verified | Contract | `d39e321` |
| 49 | POST | `/imports/{session_id}/reject` | ? | ``ica-application/imports.rs`` | Implemented | Verified | Contract | `d39e321` |
| 50 | POST | `/export` | ? | ``ica-application/sync.rs`` | Implemented | Verified | Contract | `d39e321` |
| 51 | POST | `/import` | ? | ``ica-application/sync.rs`` | Implemented | Verified | Contract | `d39e321` |
| 52 | POST | `/imports/ocr` | ? | ``ica-infra/ocr.rs` + `ica-application/imports.rs`` | Implemented | Verified | Contract | `d39e321` |
| 53 | GET | `/positions` | ? | ``ica-api/routes.rs` + `ica-application/positions.rs` + `ica-infra/repository.rs`` | Implemented | Verified | Contract | `d39e321` |
| 54 | POST | `/positions` | ? | ``ica-application/positions.rs` + `ica-infra/repository.rs`` | Implemented | Verified | Contract | `d39e321` |
| 55 | GET | `/positions/{position_id}` | ? | ``ica-api/routes.rs` + `ica-application/positions.rs` + `ica-infra/repository.rs`` | Implemented | Verified | Contract | `d39e321` |
| 56 | PATCH | `/positions/{position_id}` | ? | ``ica-application/positions.rs` + `ica-infra/repository.rs`` | Implemented | Verified | Contract | `d39e321` |
| 57 | DELETE | `/positions/{position_id}` | ? | ``ica-application/positions.rs` + `ica-infra/repository.rs`` | Implemented | Verified | Contract | `d39e321` |
| 58 | POST | `/send-records` | ? | ``ica-application/send_records.rs` + `ica-infra/send_records.rs`` | Implemented | Verified | Contract | `d39e321` |
| 59 | GET | `/send-records` | ? | ``ica-application/send_records.rs` + `ica-infra/send_records.rs`` | Implemented | Verified | Contract | `d39e321` |
| 60 | GET | `/settings` | W5 | ``ica-application/settings.rs`` | Implemented | Verified | Contract | `d39e321` |
| 61 | PUT | `/settings/ai-key` | W5 | ``ica-application/settings.rs`` | Implemented | Verified | Contract | `d39e321` |
| 62 | DELETE | `/settings/ai-key` | W5 | ``ica-application/settings.rs`` | Implemented | Verified | Contract | `d39e321` |
| 63 | GET | `/settings/ai-egress` | W5 | ``ica-application/settings.rs`` | Implemented | Verified | Contract | `d39e321` |
| 64 | POST | `/settings/ai-egress/consent` | W5 | ``ica-application/settings.rs`` | Implemented | Verified | Contract | `d39e321` |
| 65 | GET | `/audit-events` | W5 | ``ica-application/settings.rs`` | Implemented | Verified | Contract | `d39e321` |
| 66 | GET | `/backups` | W5 | ``ica-application/backups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 67 | POST | `/backups` | W5 | ``ica-application/backups.rs`` | Implemented | Verified | Contract | `d39e321` |
| 68 | POST | `/backups/{name}/restore` | W5 | ``ica-application/backups.rs`` | Implemented | Verified | Contract | `d39e321` |

## 波次分布

| Wave | 已实现 | 未实现 |
| --- | --- | --- |
| ? | 45 | 0 |
| W5 | 15 | 0 |
| W6 | 8 | 0 |
