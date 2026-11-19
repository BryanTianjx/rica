# AI-EXT 起草稿 — AI Models Extension Contract R1.0（4 条 ai/models CRUD）

> ICA-RUST AI-EXT 卡（状态 ANALYZED）的起草文档。依据 ADR-0003 D-2 补充裁决：
> 这 4 条路由**不入 68 条迁移契约**，作为 AI 域的独立扩展契约，归属 AI-EXT 波次。
> 本文是给 woticketflow 起草用的底稿——卡面由此生成后按常规审批流推进。
> 实测日 2026-11-20；基线 `9ed173c`；Rust 快照 `67e9d00`。

## 1. 范围：精确的 4 条路由

对账口径（`check_endpoint_map.py` 与迁移契约 §2 逐行核对）：契约表第 1 行已含
`GET /ai/models`；live 业务路由中的 **POST /ai/models、
PUT /ai/models/{model_id}/default、PUT /ai/models/{model_id}、
DELETE /ai/models/{model_id}** 共 4 条不在契约内——这就是 D-2 说的
「4 条 ai/models 差额」。

| # | Method | Path | 基线位置 | 语义 |
| --- | --- | --- | --- | --- |
| 1 | POST | `/api/ai/models` | `api/ai.py:319`（201） | 新增模型配置（body **无 key 字段**——key 走独立安全通道） |
| 2 | PUT | `/api/ai/models/{model_id}/default` | `api/ai.py:424` | 设默认模型（原子切换，写 `ai_model_routing`） |
| 3 | PUT | `/api/ai/models/{model_id}` | `api/ai.py:486` | 更新配置（omitted 字段不动） |
| 4 | DELETE | `/api/ai/models/{model_id}` | `api/ai.py:499` | 删除配置（`ModelDeleted` 响应） |

依赖域：`ai_model_configs` / `ai_model_routing` / `ai_usage_daily` 三表 +
模型注册表（`registry.list_all_models()`）——**与 W5 的 AI 域同仓同表**。

## 2. 波次与顺序

- **AI-EXT 依赖 W5**：W5 实现契约内 6 条 AI 端点时建出 `ai_model_configs`
  的仓储、注册表与路由骨架；AI-EXT 在其上加 4 条 CRUD——同一能力域
  一套状态机与持久化（通用纪律 3）。
- 实现形态与 `confirm-candidate` 相同：**辅助契约**（reconciliation 的
  auxiliary 登记），不算入 68 条统计；`AUXILIARY_ROUTES` 白名单届时追加 4 行。
- 验收：4 条路由逐条进 `contract_capture.py`（正路径 + 删除不存在模型的
  404 + PUT default 的原子性），质量门 + 跨语言对拍全绿。

## 3. 明确不做

- 不动 68 条契约表（D-2 结论不变：GET /ai/models 之外的 4 条仍留在 68 之外）；
- 不改基线行为（PUT 的「omitted 不动」、POST 无 key 字段、DELETE 的响应形状
  全部照抄基线）；
- 不实现 key 的加密存储变更（基线已有的存储方式照抄，另行评估属 W5/AI-EXT
  后续）；
- 不把 W5 的验收口径改成包含这 4 条（ADR-0003 边界）。

## 4. 验收判据（起草用）

1. 4 条端点全实现且契约对拍通过（新增用例 ≥ 8 条）；
2. `check.sh` 五关全绿，端点对账 auxiliary=5；
3. 跨语言对拍 N/N（在 W5 完成后的对拍基数上 0 failure）；
4. 基线零改动（sha256 锚点照旧）；
5. 桌面 IPC 命令同步补齐 4 条。

## 5. 风险

- `PUT /{id}` 的路径参数是 `{model_id:path}`（模型名可含斜杠）——axum 的
  path 通配需要 `{*model_id}` 语法，路由匹配语义要与 Starlette 对拍验证；
- `ai_model_routing` 的「默认模型」切换与 W5 的 routing 端点共享状态——
  两处写路径的原子性要同一事务；
- POST 不收 key：创建后模型的 key 状态（`has_key` 类字段）要对拍基线读响应。
