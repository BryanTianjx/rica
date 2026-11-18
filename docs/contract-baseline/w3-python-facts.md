# W3 基线实测事实（Python @ `d39e321`，2026-11-18）

实测：种子库副本 + 真实基线服务（无 `AI_API_KEY`），探针覆盖列表/详情/PATCH/approve/
reject/copy/generate 守卫链/context/draft-groups/审计。原始数据 `/tmp/w3probe/facts*.json`。
源码（`app/api/drafts.py`、`app/ai/{service,context}.py`、`app/application/import_pipeline.py`）
只用来解释实测结果。

## 1. 枚举（大小写敏感）

- `DraftStatus`：generated / edited / approved / rejected / copied
- `ValidationStatus`：Passed / Warning / Blocked
- `MessageType`：greeting / reply / follow_up_question / position_intro / salary_talk /
  interview_invite / post_interview / polite_follow_up / decline_ack
- `Tone`：formal / friendly / concise / warm

## 2. 列表与详情

- `GET /drafts`：过滤 `candidate_id` / `task_id` / `status`（alias，enum 校验 422，
  expected `'generated', 'edited', 'approved', 'rejected' or 'copied'`）；`created_at DESC`；
  `total` 为过滤后真实总数。
- 详情 404：`Draft '{id}' was not found`。
- `DraftRead` 字段集（**31 个**，含 V2 成本/路由）：id, candidate_id, task_id, message_type,
  tone, status, objective, message_text, alternatives, context_references, uncertainties,
  warnings, validation_status, validation_messages, quality_status, provider, model,
  prompt_version, is_edited, approved_at, copied_at, rejected_at, draft_group_id,
  cost_cents, prompt_tokens, completion_tokens, routing_tier, routing_reason,
  routing_decision_json, created_at, updated_at。列表字段（alternatives 等）由 `*_json`
  列展开；`routing_decision_json` 是**原样回显的原始 blob 列**，不要漏（漏它就是线格式破坏）。
  核对方式：`/mnt/o/ica/backend/.venv/bin/python -c "from app.schemas.workflow import
  DraftRead; print(len(DraftRead.model_fields))"` → 31。

## 3. PATCH `/drafts/{id}`（实测）

- **状态守卫**：`approved` / `copied` 不可编辑 → 409 `business_rule_violation`
  `"A draft in state '{status}' can no longer be edited"`，details `{"status": …}`；
  `generated` / `edited` / `rejected` 可编辑。
- `model_dump(exclude_unset=True, exclude_none=True)`：**缺省与显式 null 都被忽略**
  （实测 `{"objective": null}` → 200 不变）。
- `message_text`（min_length=1，空串 → 422 `string_too_short`）：写入后 `is_edited=true`，
  且 `generated → edited` 自动推进；`objective` 单独写不改状态。
- 审计 `draft.edited` / `Edited draft text`（metadata NULL）。

## 4. approve / reject / copy（实测矩阵）

| 动作 | 守卫 | 成功后 |
| --- | --- | --- |
| approve | **仅** `rejected` → 409 `"A rejected draft cannot be approved"`（details 空）；其余全部 200（含 approved 自转移、copied 回 approved） | `status=approved`，`approved_at=now`；若有 task：`advance_task → Approved`，reason `The draft was approved by the user`；审计 `draft.approved` / `Approved draft (approval only — no message was sent)` / metadata `{"note": …}`（null 也序列化） |
| reject | **无守卫**——任何状态都 200 → `rejected`（含 approved / copied / rejected 自转移） | `rejected_at=now`；task → `Rejected`，reason `The draft was rejected by the user`；审计 `draft.rejected` / `Rejected draft` |
| copy | 仅 `rejected` → 409 `"A rejected draft cannot be copied"` | `status=copied`，`copied_at=now`；task → `AwaitingResult`，reason `The draft was copied; waiting for the user to record the result`；审计 `draft.copied` / `Copied draft (copy is not a send)`；响应 `DraftCopyResponse{draft_id, message_text, copied_at, note: "Copied to clipboard. This does not mean the message was sent."}` |

`advance_task`：沿状态机最短路径逐状态写入 + **一条**审计
`follow_up.task_advanced` / `Advanced follow-up task {cur} → {tgt}` / metadata `{"path": [...]}`。

## 5. generate（POST `/drafts/generate`，201；无 key 场景）

守卫链顺序（全部实测）：

1. body 校验（pydantic）：candidate_id 必填；message_type/tone 枚举；`max_length ∈ [50,2000]`；
   `user_instruction ≤ 2000`；`comparison_models` 可选列表 → 422。
2. 未知候选人 → 404 `Candidate '{id}' was not found`。
3. egress consent：`app_settings` 无记录时**默认允许**（True）——种子库不拦。
4. `candidate_status.blocks_follow_up`（Rejected/Withdrawn/Closed）→ 409
   `"Candidate status '{X}' does not allow generating a follow-up message"`，details `{"candidate_status": …}`。
5. 链接岗位 Closed → 409 `"The linked position is closed; no message can be generated for it"`。
6. `task_id` 给出时：task 404 → `Follow-up task '{id}' was not found`；Blocked → 409
   `"This task is blocked and cannot be drafted"`（details `{block_reason}`）；终态 → 409
   `"A task in state {X} cannot be drafted"`（details `{current}`）；候选人不匹配 → 409
   `"The follow-up task belongs to a different candidate"`（details `{task_id, task_candidate_id, candidate_id}`）。
7. **无 key**：ModelRegistry 无库行 → 回退 settings 默认模型 `glm-4-flash`；
   `build_provider_for_model` → **503 `ai_provider_error`
   `"No API key found for model 'glm-4-flash' in env var 'AI_API_KEY'"`**（details 空，实测）。

成功路径（需真实 key，属 W5）：provider 调用 → 校验 → 落库 `generated` + `draft.generated`
审计 + task → `Reviewing`（reason `A draft was generated`）。**本卡 Rust 侧实现端口 + 无 key
错误；成功路径由 W5 打通**（端口 trait `DraftGenerator`，测试用 stub）。

## 6. context（GET `/candidates/{id}/context`，200）

`build_context`（`message_limit=30`，无截断，`ai_include_candidate_name=False`）：

- `candidate_summary`：`name`（默认替换为 `[NAME]`）、status、source、position_id +
  五个可选字段（experience_summary / skill_summary / expected_salary / expected_location /
  job_intention）**有值才带**，均经脱敏。
- 脱敏规则（`redact_for_ai`）：手机 `1[3-9]\d{9}`→`[PHONE]`、邮箱→`[EMAIL]`、
  身份证 `\d{17}[\dXx]`→`[ID_NUMBER]`、16–19 位数字→`[CARD_NUMBER]`；
  `姓名/名字/电话/手机/邮箱/微信/身份证` 前缀键值 → `{key}：[REDACTED]`；
  候选人名（≥2 字符）字面替换 → `[NAME]`；`redaction_applied` = 排序去重后的类别。
- unknowns：三个 summary 缺失各一条（`Candidate {experience summary|skill summary|job intention} is unknown.`）、
  无岗位 `The candidate is not linked to a position.`、无消息
  `No communication history is recorded yet.`、岗位缺 skill_requirements
  `Position skill requirements are unknown.`。
- conflicts：岗位 Closed 而候选人仍活跃 →
  `The linked position is closed while the candidate is still active.`；
  expected_salary 有值且候选人消息 >1 条 →
  `The candidate mentioned expectations in several messages; the recorded expected salary may not match all of them.`。
- messages：`message_time ASC NULLS LAST, created_at ASC`，限 30 条，逐条
  `{id, sender: sender_type or "Unknown", time: isoformat or null, content: 脱敏后}`。
- 响应：`message_count = len(messages)`（≤30）、`recent_messages = messages[-10:]`（最新 10 条）、
  notes/unknowns/conflicts/redaction_applied。
- 未知候选人 → 404（get_candidate 先行）。

## 7. draft-groups（实测）

- `GET /draft-groups/{id}`：按 `draft_group_id` 取全部行，`created_at ASC` 排序；
  无行 → 404 `Draft group '{id}' was not found`。
- `select`：draft_id 不在组内 → 409 `"The selected draft does not belong to this draft group"`；
  选中 rejected/approved/copied → 409 `"This draft can no longer be selected"`；
  其余组员 → `rejected` + `rejected_at` + 审计 `draft.rejected` /
  `Draft rejected as superseded by a group member` / metadata `{"superseded_by": …, "reason": "superseded_by_group_member"}`；
  选中者保持原状态，task → `Reviewing`（reason `A draft was selected from a comparison group`）；
  响应整组（排序同上）。重复 select：选中者仍 generated → 200（组员再拒绝一次，审计再写）。

## 8. 无 IMP 候选

实测未发现该登记的缺陷：错误码全部 409/422/404/503 正形；`total` 真实；组排序 created_at ASC
与实现一致；列表与详情 status 同源存库。`generate` 无 key 返回 503 `ai_provider_error`
（**不是** 500）——基线本身正确，无需登记。
