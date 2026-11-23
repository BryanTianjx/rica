# SEC-04 API 认证矩阵（68 条业务契约）

> 任务单：AUD-P1-04 / SEC-04；本文件由 SEC-04-A 卡（`wt/ica-rust-sec04-a`）增补当前策略与远程暴露门禁。
> 原矩阵源码锚点 `7213f42`（分支 `wt/ica-rust-aud-p1-04`）；契约基线 `d39e321`。
> 生成方式：本文件由 `scripts/endpoint_reconciliation.py --json` 的 68 条契约逐条标注而成，
> handler / use case 锚点从 `crates/ica-api/src/routes.rs` 解析，不手工计数。

## 口径（认证 / 授权 / 资源归属分离）

- **目标认证**：方案 §6.3 的多用户目标分类。当前仓库是 **Local 单用户模式**（`LOCAL_ACTOR=local_user`），
  业务路由**没有认证中间件**（`routes.rs:226-227`；仅有的两个 `.layer` 在 `243/246` 是 HTTP 奇偶中间件），
  因此「目标需认证」≠「当前已实现认证」。不得把本矩阵的「目标」列读成已落实情况。
- **目标授权/归属**：谁可对哪类资源做什么。Local 模式下读写都落在同一 synthetic 用户/租户上，**不视为权限隔离**。
- **可匿名**：该接口本身不携带身份凭据即可完成其语义（注册、登录、健康检查）。
- **需管理员**：多用户目标下的角色门槛；Local 模式下 synthetic 用户 `role=admin`（`auth.rs:119-129`），
  所以「管理员校验通过」在 Local 模式下不构成角色分离的证据。
- **当前 Local 现状**：实测源码行为，见报告 §3 的命令与输出。
- **SEC-04-A 当前策略**：本卡**不实现认证**，只登记当前可验证的启动/路由语义，三档取值为
  `公开端点白名单`（无需身份凭据即可完成自身语义：`/health` + `/auth/register` + `/auth/login`）、
  `Local 放行 / 未来需认证`（其余 66 条业务契约）、`远程暴露阻断`（非回环监听，在 SEC-04 完成前一律禁止）。
  白名单只表示「当前无需凭据即可到达」，**不是安全授权声明**；Local 模式下所有业务路由都没有认证门禁，
  因此白名单之外的端点在回环监听下同样可达，这是单用户本机边界，不是多用户模式的安全边界。

## 汇总

| 目标认证分类 | 条数 |
| --- | ---: |
| 可匿名 | 2 |
| 需管理员 | 11 |
| 需认证 | 52 |
| 需认证 + 需管理员 | 1 |
| 需认证（refresh 凭据） | 2 |
| **合计** | **68** |

### SEC-04-A 当前策略汇总（本卡新增）

| SEC-04-A 当前策略 | 条数 | 含义 |
| --- | ---: | --- |
| 公开端点白名单 | 2 | `/auth/register`、`/auth/login`：无需身份凭据即完成语义（另含非契约 `/health`） |
| Local 放行 / 未来需认证 | 66 | 当前回环监听下无认证门禁直接放行；SEC-04-B/C 落地真实认证后必须加门禁 |
| **合计** | **68** | 与上方目标认证分类合计一致 |

另：`GET /health` 为非契约路由（不计入 68），可匿名、无业务资源；
`POST /api/imports/{}/confirm-candidate` 为 D-2 裁决下的契约外辅助路由，目标同 import session 归属。

## 逐条矩阵

| ID | Method | Path | Module | Handler / Use Case 证据 | 目标认证 | SEC-04-A 当前策略 | 目标授权 / 归属 | 当前 Local 现状 |
| ---: | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | GET | `/ai/models` | `ica-application/ai.rs` | routes.rs:1110 · list_ai_models | 需认证 | Local 放行 / 未来需认证 | AI 配置（租户级） | 匿名可达；无管理员/角色校验 |
| 2 | GET | `/ai/routing` | `ica-application/ai.rs` | routes.rs:1124 · get_ai_routing | 需认证 | Local 放行 / 未来需认证 | AI 配置（租户级） | 匿名可达；无管理员/角色校验 |
| 3 | PATCH | `/ai/routing` | `ica-application/ai.rs` | routes.rs:1130 · update_ai_routing | 需管理员 | Local 放行 / 未来需认证 | AI 配置（租户级） | 匿名可达；无管理员/角色校验 |
| 4 | GET | `/ai/usage` | `ica-application/ai.rs` | routes.rs:1141 · get_ai_usage | 需认证 | Local 放行 / 未来需认证 | AI 配置（租户级） | 匿名可达；无管理员/角色校验 |
| 5 | GET | `/ai/budget` | `ica-application/ai.rs` | routes.rs:1156 · today_spend_cents + budget_read | 需认证 | Local 放行 / 未来需认证 | AI 配置（租户级） | 匿名可达；无管理员/角色校验 |
| 6 | PATCH | `/ai/budget` | `ica-application/ai.rs` | routes.rs:1170 · today_spend_cents + budget_read | 需管理员 | Local 放行 / 未来需认证 | AI 配置（租户级） | 匿名可达；无管理员/角色校验 |
| 7 | POST | `/auth/register` | `ica-application/auth.rs` | routes.rs:1320 · register | 可匿名 | 公开端点白名单 | 无业务资源；首注册建 admin | 匿名可达；首注册自举管理员 |
| 8 | POST | `/auth/login` | `ica-application/auth.rs` | routes.rs:1339 · login | 可匿名 | 公开端点白名单 | 凭据校验（密码验证） | 匿名入口；错误凭据 401；无用户枚举 |
| 9 | POST | `/auth/refresh` | `ica-application/auth.rs` | routes.rs:1362 · refresh | 需认证（refresh 凭据） | Local 放行 / 未来需认证 | 凭据校验（单次轮换） | 匿名入口；refresh token 本身即凭据 |
| 10 | POST | `/auth/logout` | `ica-application/auth.rs` | routes.rs:1378 · logout | 需认证（refresh 凭据） | Local 放行 / 未来需认证 | 凭据校验（幂等吊销） | 匿名入口；未知 token 仍 204 |
| 11 | GET | `/auth/me` | `ica-application/auth.rs` | routes.rs:1389 · local_user | 需认证 | Local 放行 / 未来需认证 | 仅自身信息 | 匿名可达；返回 synthetic local user |
| 12 | POST | `/shares` | `ica-application/shares.rs` | routes.rs:1395 · local_user + create_share | 需认证 | Local 放行 / 未来需认证 | 资源归属：owner + 拒绝 self-share | 匿名可达；Application 拒 self-share（auth.rs:353-355），未校验资源是否属于 actor |
| 13 | GET | `/shares` | `ica-application/shares.rs` | routes.rs:1415 · local_user + list_shares | 需认证 | Local 放行 / 未来需认证 | 仅自身 share 列表 | 匿名可达；按 local_user id 查询（auth.rs:369-377） |
| 14 | DELETE | `/shares/{share_id}` | `ica-application/shares.rs` | routes.rs:1424 · local_user + delete_share | 需认证 | Local 放行 / 未来需认证 | 资源归属：owner 或 admin | 匿名可达；Application owner/admin 校验（auth.rs:395-397），HTTP/IPC 均传 is_admin=true |
| 15 | GET | `/candidates` | `ica-api/routes.rs` + `ica-application/candidates.rs` + `ica-infra/candidates.rs` | routes.rs:324 · candidates::list_candidates | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 16 | POST | `/candidates` | `ica-application/candidates.rs` + `ica-infra/candidates.rs` | routes.rs:334 · candidates::create_candidate | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 17 | GET | `/candidates/{candidate_id}` | `ica-application/candidates.rs` + `ica-infra/candidates.rs` | routes.rs:347 · candidates::get_candidate_detail | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 18 | PATCH | `/candidates/{candidate_id}` | `ica-application/candidates.rs` + `ica-infra/candidates.rs` | routes.rs:361 · candidates::update_candidate | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 19 | DELETE | `/candidates/{candidate_id}` | `ica-application/candidates.rs` + `ica-infra/candidates.rs` | routes.rs:379 · candidates::delete_candidate | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 20 | GET | `/conversations` | `ica-application/conversations.rs` + `ica-infra/conversations.rs` | routes.rs:390 · conversations::list_conversations | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 21 | POST | `/conversations` | `ica-application/conversations.rs` + `ica-infra/conversations.rs` | routes.rs:401 · conversations::create_conversation | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 22 | GET | `/messages` | `ica-application/conversations.rs` + `ica-infra/conversations.rs` | routes.rs:420 · conversations::list_messages | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 23 | POST | `/messages` | `ica-application/conversations.rs` + `ica-infra/conversations.rs` | routes.rs:430 · conversations::create_message | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 24 | GET | `/candidates/{candidate_id}/timeline` | `ica-application/conversations.rs` + `ica-infra/conversations.rs` | routes.rs:447 · conversations::candidate_timeline | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 conversation/message 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 25 | GET | `/dashboard` | `ica-application/dashboard.rs` | routes.rs:632 · dashboard | 需认证 | Local 放行 / 未来需认证 | 聚合数据按租户收敛 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 26 | GET | `/drafts` | `ica-application/drafts.rs` | routes.rs:686 · list_drafts | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 27 | GET | `/drafts/{draft_id}` | `ica-application/drafts.rs` | routes.rs:704 · get_draft | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 28 | POST | `/drafts/generate` | `ica-application/drafts.rs` | routes.rs:727 · generate | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 29 | GET | `/candidates/{candidate_id}/context` | `ica-application/drafts.rs` | routes.rs:757 · preview_context | 需认证 | Local 放行 / 未来需认证 | candidate + 关联 message 归属（上下文含沟通原文） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 30 | PATCH | `/drafts/{draft_id}` | `ica-application/drafts.rs` | routes.rs:772 · update_draft | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 31 | POST | `/drafts/{draft_id}/approve` | `ica-application/drafts.rs` | routes.rs:784 · approve_draft | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 32 | POST | `/drafts/{draft_id}/reject` | `ica-application/drafts.rs` | routes.rs:797 · reject_draft | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 33 | POST | `/drafts/{draft_id}/copy` | `ica-application/drafts.rs` | routes.rs:807 · copy_draft | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 34 | GET | `/draft-groups/{group_id}` | `ica-application/drafts.rs` | routes.rs:817 · get_draft_group | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 35 | POST | `/draft-groups/{group_id}/select` | `ica-application/drafts.rs` | routes.rs:826 · select_draft_from_group | 需认证 | Local 放行 / 未来需认证 | draft / draft-group 归属（含关联 candidate/task） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 36 | POST | `/follow-ups/evaluate` | `ica-application/follow_ups.rs` | routes.rs:502 · evaluate | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 37 | GET | `/follow-ups` | `ica-application/follow_ups.rs` | routes.rs:519 · list_tasks | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 38 | POST | `/follow-ups` | `ica-application/follow_ups.rs` | routes.rs:540 · create_task | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 39 | GET | `/follow-ups/{task_id}` | `ica-application/follow_ups.rs` | routes.rs:557 · get_task | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 40 | PATCH | `/follow-ups/{task_id}` | `ica-application/follow_ups.rs` | routes.rs:567 · update_task | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 41 | POST | `/follow-ups/{task_id}/cancel` | `ica-application/follow_ups.rs` | routes.rs:580 · cancel_task | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 42 | POST | `/follow-ups/{task_id}/complete` | `ica-application/follow_ups.rs` | routes.rs:591 · complete_task | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 43 | POST | `/follow-ups/{task_id}/record-result` | `ica-application/follow_ups.rs` | routes.rs:601 · record_result | 需认证 | Local 放行 / 未来需认证 | follow-up task 归属（含关联 candidate/position） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 44 | GET | `/follow-ups/policy/current` | `ica-application/follow_ups.rs` | routes.rs:613 · ensure_default_policy | 需认证 | Local 放行 / 未来需认证 | 租户级策略读取 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 45 | PATCH | `/follow-ups/policy/current` | `ica-application/follow_ups.rs` | routes.rs:621 · update_policy | 需管理员 | Local 放行 / 未来需认证 | 租户级策略写入 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 46 | POST | `/imports/preview` | `ica-application/imports.rs` | routes.rs:864 · preview_import | 需认证 | Local 放行 / 未来需认证 | import session 归属（含 raw_text PII） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 47 | GET | `/imports/{session_id}` | `ica-application/imports.rs` | routes.rs:875 · （直连 state/repository，无 use case） | 需认证 | Local 放行 / 未来需认证 | import session 归属（含 raw_text PII） | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 48 | POST | `/imports/{session_id}/confirm` | `ica-application/imports.rs` | routes.rs:889 · confirm_import | 需认证 | Local 放行 / 未来需认证 | import session 归属（含 raw_text PII） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 49 | POST | `/imports/{session_id}/reject` | `ica-application/imports.rs` | routes.rs:941 · reject_import | 需认证 | Local 放行 / 未来需认证 | import session 归属（含 raw_text PII） | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 50 | POST | `/export` | `ica-application/sync.rs` | routes.rs:955 · export_data | 需认证 + 需管理员 | Local 放行 / 未来需认证 | 全量快照导出（含 PII） | 匿名可达；无管理员/角色校验 |
| 51 | POST | `/import` | `ica-application/sync.rs` | routes.rs:1001 · import_data | 需管理员 | Local 放行 / 未来需认证 | 全量快照写入（LWW 合并） | 匿名可达；无管理员/角色校验 |
| 52 | POST | `/imports/ocr` | `ica-infra/ocr.rs` + `ica-application/imports.rs` | routes.rs:1020 · ocr_screenshot | 需认证 | Local 放行 / 未来需认证 | import session + 文件目录边界 | 匿名可达；Application 路径守卫先于文件存在性检查 |
| 53 | GET | `/positions` | `ica-api/routes.rs` + `ica-application/positions.rs` + `ica-infra/repository.rs` | routes.rs:267 · list_positions | 需认证 | Local 放行 / 未来需认证 | position 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 54 | POST | `/positions` | `ica-application/positions.rs` + `ica-infra/repository.rs` | routes.rs:277 · positions::create_position | 需认证 | Local 放行 / 未来需认证 | position 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 55 | GET | `/positions/{position_id}` | `ica-api/routes.rs` + `ica-application/positions.rs` + `ica-infra/repository.rs` | routes.rs:288 · get_position | 需认证 | Local 放行 / 未来需认证 | position 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 56 | PATCH | `/positions/{position_id}` | `ica-application/positions.rs` + `ica-infra/repository.rs` | routes.rs:300 · positions::update_position | 需认证 | Local 放行 / 未来需认证 | position 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 57 | DELETE | `/positions/{position_id}` | `ica-application/positions.rs` + `ica-infra/repository.rs` | routes.rs:313 · positions::delete_position | 需认证 | Local 放行 / 未来需认证 | position 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 58 | POST | `/send-records` | `ica-application/send_records.rs` + `ica-infra/send_records.rs` | routes.rs:468 · send_records::record_send | 需认证 | Local 放行 / 未来需认证 | send_record + 关联 candidate 归属 | 匿名可达；写入盖 local owner/tenant 默认值，无请求身份（store.rs:24-28） |
| 59 | GET | `/send-records` | `ica-application/send_records.rs` + `ica-infra/send_records.rs` | routes.rs:481 · send_records::list_send_records | 需认证 | Local 放行 / 未来需认证 | send_record + 关联 candidate 归属 | 匿名可达；Repository 读无 owner/tenant 过滤（repository.rs:17-24） |
| 60 | GET | `/settings` | `ica-application/settings.rs` | routes.rs:1065 · （直连 state/repository，无 use case） | 需认证 | Local 放行 / 未来需认证 | 配置读取（含 database_url/base_url） | 匿名可达；无管理员/角色校验 |
| 61 | PUT | `/settings/ai-key` | `ica-application/settings.rs` | routes.rs:1209 · write_ai_key | 需管理员 | Local 放行 / 未来需认证 | AI 密钥槽写入 | 匿名可达；无管理员/角色校验 |
| 62 | DELETE | `/settings/ai-key` | `ica-application/settings.rs` | routes.rs:1235 · clear_ai_key | 需管理员 | Local 放行 / 未来需认证 | AI 密钥槽清除 | 匿名可达；无管理员/角色校验 |
| 63 | GET | `/settings/ai-egress` | `ica-application/settings.rs` | routes.rs:1082 · read_egress_disclosure | 需认证 | Local 放行 / 未来需认证 | AI egress 披露读取 | 匿名可达；无管理员/角色校验 |
| 64 | POST | `/settings/ai-egress/consent` | `ica-application/settings.rs` | routes.rs:1092 · write_egress_consent | 需管理员 | Local 放行 / 未来需认证 | AI egress 开关写入 | 匿名可达；无管理员/角色校验 |
| 65 | GET | `/audit-events` | `ica-application/settings.rs` | routes.rs:1052 · recent_audit_events | 需管理员 | Local 放行 / 未来需认证 | 审计轨迹读取 | 匿名可达；无管理员/角色校验 |
| 66 | GET | `/backups` | `ica-application/backups.rs` | routes.rs:1254 · list_backups | 需管理员 | Local 放行 / 未来需认证 | 备份列表 | 匿名可达；无管理员/角色校验 |
| 67 | POST | `/backups` | `ica-application/backups.rs` | routes.rs:1263 · create_backup | 需管理员 | Local 放行 / 未来需认证 | 备份创建（数据库级） | 匿名可达；无管理员/角色校验 |
| 68 | POST | `/backups/{name}/restore` | `ica-application/backups.rs` | routes.rs:1273 · list_backups + restore_backup | 需管理员 | Local 放行 / 未来需认证 | 备份恢复（数据库级、破坏性） | 匿名可达；仅 confirm 显式校验（routes.rs:1278-1307），无管理员校验 |

## SEC-04-A 当前策略对照与远程暴露阻断（本卡新增）

### 规则

1. **Local-only 是默认且唯一的可用模式**：`ica-server` 默认 `HOST=127.0.0.1`，`validate_bind_host`
   对任何非回环 host fail-fast，错误文案固定包含 `refusing to bind to non-loopback host` 与
   `no network authentication`（`crates/ica-server/src/main.rs:45`）。
2. **显式开关不是认证**：`ICA_AUTH_REQUIRED=true`（大小写不敏感；仅 `true`/`false` 合法）
   只是「申请远程模式」的策略开关。当前 `NETWORK_AUTHENTICATION_READY=false`
   （`main.rs:65`），所以即使显式开启，非回环监听仍被拒绝，并返回
   `多用户认证尚未实现，不能启用远程模式`（`main.rs:70,94`）。本卡**没有** JWT 中间件、
   用户模型或资源授权，不得把该开关表述为「认证已开启」。
3. **SEC-04 完成前禁止远程暴露**：局域网/公网监听、反向代理转发到非回环地址、
   容器 `-p 0.0.0.0:8000` 映射等均不允许；上述规则由启动阻断 + 发布门禁
   （`docs/architecture/remote-exposure-gate.md`）双重约束。
4. **逐条策略**：68 条契约中 2 条为公开端点白名单，66 条为 Local 放行 / 未来需认证；
   另有非契约 `/health` 属于公开端点但不计入 68。白名单中的端点在远程模式下同样必须受
   传输层与设备边界保护，因为本构建没有任何 HTTP 认证中间件。

### 机器可读的公开端点白名单（源码断言读取此块）

<!-- SEC04-A-PUBLIC-ENDPOINTS-BEGIN
GET /health
POST /api/auth/register
POST /api/auth/login
SEC04-A-PUBLIC-ENDPOINTS-END -->

### 与 `routes.rs` 路由注册对照

| 白名单 Method + Path | `routes.rs` 注册（无认证中间件） | 矩阵行 |
| --- | --- | --- |
| `GET /health` | `.route("/health", get(health))` | 非契约（汇总说明） |
| `POST /api/auth/register` | `.route("/api/auth/register", post(register_endpoint))` | ID 7 |
| `POST /api/auth/login` | `.route("/api/auth/login", post(login_endpoint))` | ID 8 |

`router()` 全文件只有两个 `.layer(...)`：`http_parity::reject_head` 与
`http_parity::redirect_trailing_slash`，均为 HTTP 奇偶中间件，**不是认证**；
其余路由（含 `/api/auth/refresh`、`/api/auth/logout` 等）不在 SEC-04-A 的 3 项公开白名单内，
其凭据校验发生在 use case 而非 HTTP 门禁，仍属于 `Local 放行 / 未来需认证`。
口径提示：SEC-04-B/C 落地后 `/api/auth/refresh`、`/api/auth/logout` 已加入
`crates/ica-api/src/auth.rs::PUBLIC_PATHS`（见下方 B/C 实施映射），由 refresh token 完成
凭据轮换/吊销的自身语义，不再需要 access token；A 卡的历史白名单（2 契约 + `/health`）与
B/C 卡实际 `PUBLIC_PATHS`（5 项）口径不同，不可混用。

### 源码断言测试

- `crates/ica-api/src/routes.rs` 的 `sec04_a_route_manifest_tests`：证明矩阵 68 行的 Path 全部
  对应 `ROUTE_PATTERNS` 中真实注册的路由，白名单 3 项全部在清单内。
- `crates/ica-api/tests/sec04_authorization_http.rs` 的
  `sec04_bc_public_paths_match_auth_public_paths`（B/C 新增）：解析下方 B/C 机器可读块的 5 项与 `auth::PUBLIC_PATHS` 双向逐项相等，
  堵住此前「只比对 A 卡 markers 与 routes.rs、未与 PUBLIC_PATHS 比对」的漂移漏洞。
- `crates/ica-api/tests/sec04_authorization_http.rs` 的
  `sec04_a_public_whitelist_matches_unauthenticated_routes`：证明上表中的白名单与本文件
  markers 块一致、与 `routes.rs` 注册一致、`router()` 中不存在认证中间件，并证明矩阵
  68 行策略列与白名单/非白名单划分一致。
- `crates/ica-server/src/main.rs` 的 `sec04_a_*` 单测：AC1 文案、AC2 显式开关与
  「认证未就绪」拒绝语义。

## 自洽校验

- 契约行数：**68**（`endpoint_reconciliation --json` 的 `contract_total=68`）。
- 本表数据行：**68**（逐 ID 1..68 连续、无重复、无缺号）。
- 当前实现：**68/68 Implemented**，`extra_routes=0`；契约表 68 行与本表 68 行一一对应。
- 认证分类合计：**52 + 11 + 2 + 2 + 1 = 68**。
- SEC-04-A 当前策略合计：**2 + 66 = 68**（公开端点白名单 2；Local 放行 / 未来需认证 66）；
  非契约公开端点 `/health` 单列，不计入 68。
- 公开端点白名单（3 项，含非契约 `/health`）与 `routes.rs` 注册一致，由上述源码断言测试保证。

## 共用证据锚点

| 锚点 | 事实 |
| --- | --- |
| `crates/ica-api/src/routes.rs:226-227` | W6 auth/shares 注释：local 是默认模式，其它路由无中间件门禁 |
| `crates/ica-api/src/routes.rs:243,246` | 仅有的两个 layer：HEAD 拒绝 / 尾斜杠重定向，均非认证 |
| `crates/ica-infra/src/repository.rs:17-24` | 读路径刻意不做 tenant/owner 过滤；等价性止于 local mode |
| `crates/ica-infra/src/store.rs:24-28` | `LOCAL_OWNER_ID` / `DEFAULT_TENANT_ID` / `LOCAL_ACTOR` 常量 |
| `crates/ica-application/src/auth.rs:119-129` | synthetic local user：`role=admin`，id `…0001` |
| `crates/ica-application/src/auth.rs:353-355` | self-share 拒绝（Application 层） |
| `crates/ica-application/src/auth.rs:395-397` | share 删除 owner/admin 校验（Application 层） |
| `crates/ica-server/src/main.rs:45` | `validate_bind_host`：非回环 fail-fast，固定错误文案 |
| `crates/ica-server/src/main.rs:60,74,94` | `ICA_AUTH_REQUIRED` 显式开关、纯解析函数、`validate_bind_policy` 启动阻断 |
| `crates/ica-server/src/main.rs:65,70` | `NETWORK_AUTHENTICATION_READY=false` 与 `多用户认证尚未实现，不能启用远程模式` |
| `crates/ica-server/src/main.rs:127,133` | `HOST` 默认 `127.0.0.1`，启动前执行暴露策略 |

## SEC-04-B/C 实施映射（2026-11-23）

上表「SEC-04-A 当前策略」列是 A 卡的历史事实快照；B/C 卡实施后，
MultiUser 模式的实际门禁由下表定义（Local 模式仍保持原行为）：

| 层 | 实施点 | 68 契约映射 |
| --- | --- | --- |
| 认证 | `crates/ica-api/src/auth.rs::authenticate` + `PUBLIC_PATHS` | `PUBLIC_PATHS` 实际 5 项：`/health`（非契约）+ `/auth/register`、`/auth/login`（首注册/登录入口）+ `/auth/refresh`、`/auth/logout`（bootstrap 链，用 refresh token 完成凭据轮换/吊销的自身语义，不要求 access token）；其余 64 条业务契约必须有合法 HS256 access token |
| 身份 | `RequestContext` 注入 request extensions | `/auth/me` 与 shares 端点从 JWT claims 取真实 actor/tenant/role，不再合成 Local 身份 |
| 权限 | `ica_application::authorize(ctx, Permission)` | 角色门槛（admin/member）与资源归属分离；`forbidden` 403 与 `unauthorized` 401 分列 |
| 资源 | `Scope::from_context` + `PositionFilter.scope` | positions 的 list/get/create/update/delete 强制 tenant 过滤，member 限 owner；跨租户返回 404 |
| 模式 | `ICA_AUTH_MODE=local\|multiuser` | Local 保持单用户边界；MultiUser 拒绝匿名/伪造 token |

### SEC-04-B/C 机器可读的公开端点（源码断言读取此块，与 `auth::PUBLIC_PATHS` 双向一致）

<!-- SEC04-BC-PUBLIC-PATHS-BEGIN
/health
/api/auth/register
/api/auth/login
/api/auth/refresh
/api/auth/logout
SEC04-BC-PUBLIC-PATHS-END -->

`/api/auth/refresh` 与 `/api/auth/logout` 在 MultiUser 下无需 access bearer 即可到达：
前者以请求体的 refresh token 完成单次轮换，后者以 refresh token 幂等吊销；
二者属于 bootstrap 凭据链，不是「需 access token」的业务契约。

三向追溯（契约 → 授权点 → 用例）由
`crates/ica-api/tests/sec04_bc_authorization_http.rs` 的
`contract_authorization_trace_is_complete` 与前述 68 行矩阵共同 pin 住；
负向用例见同文件的 `anonymous_and_forged_bearers_are_rejected_with_baseline_code`、
`cross_tenant_reads_are_invisible`、`write_and_delete_obey_identity_and_ownership`。
