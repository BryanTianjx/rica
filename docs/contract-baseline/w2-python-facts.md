# W2 基线实测事实（Python @ `d39e321`，2026-11-18）

实测方法：种子库副本 + 真实起服务 + 真实发请求（探针脚本：状态机 109 组、evaluate 10 分支、
policy 10 例、过期清扫 5 态、create 5 例、dashboard、audit_events 逐条）。源码只用来解释
实测结果，不用来替代实测。原始数据：`/tmp/w2probe/facts.json`、`/tmp/w2probe2/facts.json`。

## 1. 状态机（12 状态，大小写敏感）

`Pending / Eligible / Drafting / Reviewing / Rejected / Approved / Copied /
AwaitingResult / Completed / Cancelled / Expired / Blocked`

- `is_closed` = {Completed, Cancelled, Expired}
- `means_contacted` = {Copied, AwaitingResult, Completed}（进 max-active 计数）
- 语义红线（docstring 原文保住）：Approved 只表示草稿被批准；Copied 只表示文本被复制；
  AwaitingResult 只表示等用户回填——**任何一个都不代表消息已发出**。
- `can_transition(current, target)`：**current == target 时返回 True**（自转移合法）。

### 1.1 实测转移矩阵（每个 (current, action) 都真实发过请求）

图例：`OK` = 200；`409 Illegal` = `409 business_rule_violation`
`"Illegal follow-up transition {cur} → {tgt}"`；`409 cannot` = 同码的
`"A task in state {cur} cannot be cancelled/completed"`。

| current \ action | PATCH→Pending | PATCH→Eligible | PATCH→Drafting | PATCH→Reviewing | PATCH→Rejected | PATCH→Cancelled | CANCEL | COMPLETE | RESULT(sent) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Pending | OK | OK | 409 Illegal | 409 Illegal | 409 Illegal | OK | OK | 409 cannot | OK |
| Eligible | 409 Illegal | OK | OK | 409 Illegal | 409 Illegal | OK | OK | 409 cannot | OK |
| Drafting | 409 Illegal | 409 Illegal | OK | OK | OK | OK | OK | 409 cannot | OK |
| Reviewing | 409 Illegal | 409 Illegal | OK | OK | OK | OK | OK | 409 cannot | OK |
| Rejected | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | OK | OK | OK | 409 cannot | OK |
| Approved | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | OK | OK | 409 cannot | OK |
| Copied | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | OK | OK | OK | OK |
| AwaitingResult | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | OK | OK | OK | OK |
| Completed | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 cannot | OK | OK |
| Cancelled | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | OK（自转移） | OK（自转移） | 409 cannot | OK |
| Expired | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 cannot | 409 cannot | OK |
| Blocked | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | 409 Illegal | OK | OK | 409 cannot | OK |

要点：

- **PATCH 手工可设集合**（其余直接 409 `"'{X}' cannot be set directly; it is recorded by
  the action it represents"`，details 含 current/requested/settable）：
  {Pending, Eligible, Drafting, Reviewing, Rejected, Cancelled}。Approved/Copied/
  Completed/Expired/Blocked/AwaitingResult 一律不能由 PATCH 直设。
- 自转移全部 200（PATCH 同状态、对 Cancelled 再 cancel、对 Completed 再 complete）。
- **record-result 从任何状态都是 200**（包括终态）——结果照记、计数照加，只是
  RESULT_COMPLETABLE（{Pending, Eligible, Drafting, Reviewing, Approved, Copied,
  AwaitingResult}）之外的状态不再变成 Completed。
- 409 文案逐字：`Illegal follow-up transition {cur} → {tgt}` /
  `A task in state {cur} cannot be cancelled` / `A task in state {cur} cannot be completed`；
  details = {"current": cur}（complete/cancel）或 {"current", "requested"}（patch）。
- PATCH `{"status": "Nonsense"}` → 422 `validation_error`，details.errors[].type = `enum`，
  expected 文案含全部 12 个合法值。

## 2. evaluate（POST /api/follow-ups/evaluate，200）

响应字段：`outcome(eligible|wait|blocked) / eligible / reasons / block_reasons /
recommended_at / suggested_objective / risks`。分支与逐字文案：

| 分支 | outcome | reasons / block_reasons | suggested_objective |
| --- | --- | --- | --- |
| 无消息 | eligible | `No prior communication recorded.` | greeting |
| 最后一条来自候选人 | eligible | `The last message came from the candidate; a reply is expected.` | reply |
| 我方消息，首访（count=0），≥24h | eligible | `{N}h/{D}d{H}h elapsed since the last message, meeting the 24h first-follow-up wait.` | polite_follow_up |
| 我方消息，首访，<24h | wait | `Waiting for the first follow-up delay of 24h.`；`recommended_at` = 消息+24h | null |
| 我方消息，count≥1，≥48h | eligible | `At least 48h since the last follow-up attempt.` | polite_follow_up |
| 我方消息，count≥1，<48h | wait | `Waiting for the 48h minimum interval between follow-ups.`；`recommended_at` = 参考点+48h | null |
| 候选人 Rejected/Withdrawn/Closed | blocked | `Candidate status is {X}; ordinary follow-up is not allowed.` | null |
| 岗位 Closed（显式 override 或候选人链接） | blocked | `Position status is Closed; ordinary follow-up is not allowed.` | null |
| 候选人 Interview | blocked | `Candidate entered the interview stage; use interview follow-up instead.` | null |
| 计数到限 | blocked | `Active follow-up count {n} reached the limit of {max}.` | null |

- **间隔参考点**：`last_follow_up_at` 为 NULL 时回退到该任务的 `updated_at`（实测确认：
  BOB 的 Completed 任务 last_follow_up_at 为 NULL，回退到 updated_at 后仍走 ≥48h 分支）。
- **risks**：outcome=eligible 且候选人无岗位时追加
  `The candidate is not linked to a position, so the message cannot reference position requirements.`
- **时长文案随真实时间流逝变化**（`{D}d{H}h`），两个 capture 之间跨小时边界会翻字——
  因此**对拍用例只取文案稳定的三类**（无消息 / 候选人末条 / 48h 已过 + blocked 全家），
  WAIT 分支与首访已过分支由本轮实测钉住（上面逐字），不进对拍。
- 请求体：`candidate_id` 必填；`position_id` 可覆盖候选人链接；`policy_id` **被忽略**（源码确认）。
- 未知候选人 → 404 `Candidate '{id}' was not found`。

## 3. policy（GET/PATCH /api/follow-ups/policy/current）

- GET 无行时**自动建 default 行**（24/2/48/7，is_active=true）→ 200；不写审计。
- PATCH 字段与边界：`first_follow_up_delay_hours ∈ [0,720]`、`max_active_follow_ups ∈ [1,20]`、
  `min_interval_hours ∈ [0,720]`、`task_validity_days ∈ [1,365]`；越界/非整数 → 422。
- **显式 null 与缺省都被忽略**（`exclude_unset` + `exclude_none`）——与 positions 的
  IMP-2 不同，这里 null 不报错（已实测 `{"max_active_follow_ups": null}` → 200 不变）。
- PATCH 成功写审计 `follow_up.policy_updated` / summary `Updated follow-up policy 'default'` /
  metadata = 实际补丁字段（空补丁时 metadata 为 NULL）。
- 活动行选择：`is_active=true ORDER BY created_at ASC` 第一行。

## 4. 任务过期：存库状态，列表读时清扫

- 过期时刻 = `created_at + task_validity_days` 天，建任务时写入 `expires_at`。
- **GET /api/follow-ups 每次先清扫**：`expires_at < now` 且状态可合法转移到 Expired
  （即 {Pending, Eligible, Drafting, Reviewing, Rejected, Approved, Blocked}）的行
  改为 `Expired` 并写一条审计（**无 entity_type/entity_id**）：
  `follow_up.tasks_expired` / `Expired {n} stale follow-up task(s)`。
- **Copied / AwaitingResult 不会被清扫**（转移矩阵不允许）——用户还欠一个结果。
- 对拍种子任务的 `expires_at` 全为 NULL → 永不过期，清扫对种子无副作用。

## 5. 列表（GET /api/follow-ups）

- 参数：`candidate_id`、`status`（alias，enum 校验 422）、`offset`（ge=0）、`limit`（gt=0, le=500, 默认 50）。
- 排序 `created_at DESC`；`total` 为过滤后真实总数（非截断）。
- 响应字段 18 个：id, candidate_id, position_id, status, objective, reason, block_reason,
  recommended_at, expires_at, last_follow_up_at, follow_up_count, completed_at, result_type,
  result_note, result_recorded_at, created_at, updated_at, candidate_name（mapper 连候选人名）。
- 详情 404：`Follow-up task '{id}' was not found`。

## 6. 写路径与副作用（audit_events 逐字）

| 操作 | event_type | summary | metadata |
| --- | --- | --- | --- |
| POST /follow-ups（新建） | `follow_up.task_created` | `Created follow-up task` | `{"status": …, "outcome": …}` |
| POST /follow-ups（复用） | `follow_up.task_reused` | `Reused the existing open follow-up task` | `{"status": 现有任务状态}` |
| PATCH | `follow_up.task_updated` | `Updated follow-up task ({新状态})` | 无（NULL） |
| cancel | `follow_up.task_cancelled` | `Cancelled follow-up task` | 无 |
| complete | `follow_up.task_completed` | `Completed follow-up task` | 无 |
| record-result | `follow_up.result_recorded` | `Recorded result '{type}'` | `{"result_type": …}` |
| 清扫 | `follow_up.tasks_expired` | `Expired {n} stale follow-up task(s)` | 无；entity_type/entity_id = NULL |
| policy PATCH | `follow_up.policy_updated` | `Updated follow-up policy 'default'` | 补丁字段或 NULL |

- **create 的初始状态由 evaluate 决定**：eligible→`Eligible`、wait→`Pending`、
  blocked→`Blocked`（block_reason = block_reasons 拼接）；`objective` 缺省取
  `suggested_objective`；`reason` 缺省取 reasons/block_reasons 拼接；
  `expires_at = now + task_validity_days`。
- **create 幂等复用**：候选人已有非终态且非 Blocked 的任务时，返回现有行
  （不新建、不改状态），并写 `task_reused` 审计。
- **record-result 幂等**：`result_type` 表明真实接触（sent / candidate_replied /
  candidate_declined / interview）时 `follow_up_count += 1` 且 `last_follow_up_at = now`；
  **重复记录同一 outcome 不再 +1**；改成不同 outcome 是纠错，也不 +1。
  RESULT_COMPLETABLE 内的状态此时变 `Completed` 并写 `completed_at`。
- **record-result 联动 send_records**：首次或换 outcome 时追加一行
  （`task_id` = 任务、`draft_id` = 该任务唯一 sendable（approved/copied）草稿、
  `content_summary` = objective 截 200、纠错时 note = `correction: {note}`），
  同一 outcome 重放不追加。send.recorded 审计由 W1 已实现的 record_send 写入。
- objective 上限 64 字符（pydantic max_length → 422 string_too_long）。

## 7. dashboard（GET /api/dashboard，200）

| 字段 | 口径 |
| --- | --- |
| candidate_total | `archived=false` 的候选人数 |
| candidate_status_counts | 同上按 status 分组；**9 个键恒在**（缺组补 0） |
| active_follow_up_count | 状态 ∈ {Pending, Eligible, Drafting, Reviewing} 的任务数 |
| pending_follow_ups | 同 4 状态的任务，`recommended_at ASC NULLS LAST`，**limit 20**，逐行走 task_to_read |
| awaiting_result_count | 状态 = AwaitingResult 的任务数 |
| pending_draft_count | 草稿 status ∈ {generated, edited} 的行数 |
| recent_messages | 全部消息按 `created_at DESC` 前 **10** 条 |
| position_total / open_position_count | `archived=false` 全量 / 其中 status=Open |

- **读端点的写副作用**：dashboard 每次先跑 `run_maintenance`：
  ① `expire_old_candidates`：硬删除 `archived=true 且 updated_at < now-180d`
  （`data_retention_days` 默认 180）的候选人（FK 级联删子女）；
  ② `prune_audit_events`：删 `created_at < now-180d` 的审计行；
  ③ `prune_stale_import_raw_text`：`import_sessions.created_at < now-7d` 且
  `raw_text != ''` 的行把 `raw_text` 清成空串。
  对新鲜种子库三者皆 no-op；Rust 侧同样实现（行为等价，非可选项）。

## 8. 其它

- `GET /api/follow-ups` 的 `status=0/limit=0/limit=501/offset=-1` 等查询校验与 W1 口径一致
  （greater_than / less_than_equal / enum），422 形状相同。
- `POST /follow-ups`/`PATCH` 的 `recommended_at` 是 pydantic datetime（非法格式 → 422）。
- 基线侧本波次**未发现该登记的 IMP**：错误码全部是 409/422/404 正形，total 是真实总数，
  列表与详情状态一致（Expired 是存库状态）。EXP 影响评估：无。
