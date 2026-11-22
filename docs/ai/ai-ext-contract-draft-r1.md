# AI-EXT 起草稿 — AI Models Extension Contract R1.0（4 条 ai/models CRUD）

> ICA-RUST AI-EXT 卡（状态 ANALYZED）的起草文档。依据 ADR-0003 D-2 补充裁决：
> 这 4 条路由**不入 68 条迁移契约**，作为 AI 域的独立扩展契约，归属 AI-EXT 波次。
> 本文是给 woticketflow 起草用的底稿——卡面由此生成后按常规审批流推进。
> 实测日 2026-11-20；基线 `9ed173c`；Rust 快照 `67e9d00`。
>
> **状态：draft（起草稿，标题与 draft 状态保留）；契约已补全，待独立复核。**
> 本轮按 AI-EXT AC1/AC2 补齐 §6（请求/响应 Schema、错误码表、状态转换与副作用）
> 与 §7（调用方查证）；新增内容全部标注源码路径，不改动任何实现。

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

## 6. 请求 / 响应 Schema、错误码、状态转换与副作用（AC1）

> 来源与口径：Python 基线 `9ed173c` 的 `backend/app/api/ai.py`（4 条路由 + Pydantic 模型）、
> `backend/app/errors.py`（错误包络与默认状态码）、`backend/app/main.py:126-144`（异常处理器）、
> `backend/app/api/settings.py:36,70-83`（管理员门禁）、
> `backend/app/ai/providers/base.py:225-227` 与 `backend/app/ai/registry.py:29-35`（取值集合）。
> Rust 侧 W5 快照 `67e9d00` 仅实现契约内的 `GET /ai/models`
> （`crates/ica-api/src/routes.rs:209,1109`），4 条写路由尚未落地；
> 因此以下字段一律以基线源码为准，Rust 实现须逐字段对拍。凡源码未给出细节者，
> 明确写「以基线 `9ed173c` 为准」并给出来源路径，不做推测。

### 6.1 请求 Schema

**POST `/api/ai/models` — `ModelCreate`（ai.py:133-150）**

| 字段 | 类型 | 必填 / 默认 | 约束（源码位置） |
| --- | --- | --- | --- |
| `model_id` | `str` | 必填 | `min_length=1, max_length=128`，pattern `^[^\s]{1,128}$`（`MODEL_ID_PATTERN`，ai.py:128,136） |
| `display_name` | `str` | 默认 `""` | `max_length=200`（ai.py:137）；空白串在创建时被替换为 `model_id`（ai.py:335-336） |
| `provider_type` | `str` | 默认 `"openai_compatible"` | `max_length=64`（ai.py:138）；取值须属 `SUPPORTED_PROVIDER_TYPES`（ai.py:125,183-192）＝ `{openai_compatible, z.ai, minimax-cn, opencode}`（`app/ai/providers/base.py:225-227`） |
| `base_url` | `str` | 必填 | `min_length=1, max_length=500`（ai.py:139）；且须以 `http://` 或 `https://` 开头（ai.py:193-199） |
| `api_key_env` | `str` | 默认 `"AI_API_KEY"` | `max_length=128`，pattern `^[A-Za-z_][A-Za-z0-9_]*$`（`ENV_NAME_PATTERN`，ai.py:130,140） |
| `quality_tier` | `str` | 默认 `QualityTier.STANDARD.value`（`standard`） | `max_length=32`（ai.py:141）；须属 `QualityTier` 枚举 `{economy, standard, premium}`（`app/ai/registry.py:29-35`；校验 ai.py:200-207） |
| `input_cost_per_1k_tokens` | `float` | 默认 `0.0` | `ge=0.0`（ai.py:142） |
| `output_cost_per_1k_tokens` | `float` | 默认 `0.0` | `ge=0.0`（ai.py:143） |
| `max_output_tokens` | `int` | 默认 `1500` | `ge=1, le=200000`（ai.py:144） |
| `max_input_tokens` | `int` | 默认 `8000` | `ge=1, le=2000000`（ai.py:145） |
| `timeout` | `int` | 默认 `60` | `ge=1, le=600`（ai.py:146） |
| `temperature` | `float` | 默认 `0.4` | `ge=0.0, le=2.0`（ai.py:147） |
| `enabled` | `bool` | 默认 `True` | ai.py:148；`is_default=true` 时必须为 `true`（ai.py:329-334） |
| `is_default` | `bool` | 默认 `False` | ai.py:149 |
| `sort_order` | `int` | 默认 `0` | `ge=0, le=9999`（ai.py:150） |

**无 key 字段**：请求体不含任何 key 值字段（ai.py:7-11、133-134）；key 走独立凭据通道
`PUT /settings/ai-key`（ai.py:9-11）。

**PUT `/api/ai/models/{model_id:path}` — `ModelUpdate`（ai.py:153-173）**

全部字段为 `T | None`，默认 `None`；语义为「omitted 字段不动」，且 `model_id` 是行标识
（也是凭据槽位键），不可更新——改 ID 等于新建配置（ai.py:154-158）。
handler 以 `payload.model_dump(exclude_unset=True, exclude_none=True)` 取更新集
（ai.py:495），故显式传 `null` 与省略等价。

| 字段 | 类型 | 约束（与 `ModelCreate` 同名者一致） |
| --- | --- | --- |
| `display_name` | `str \| None` | `min_length=1, max_length=200`（ai.py:160） |
| `provider_type` | `str \| None` | `max_length=64`，取值校验同上（ai.py:161） |
| `base_url` | `str \| None` | `min_length=1, max_length=500`，http(s) 校验同上（ai.py:162） |
| `api_key_env` | `str \| None` | `max_length=128`，`ENV_NAME_PATTERN`（ai.py:163） |
| `quality_tier` | `str \| None` | `max_length=32`，`QualityTier` 校验同上（ai.py:164） |
| `input_cost_per_1k_tokens` | `float \| None` | `ge=0.0`（ai.py:165） |
| `output_cost_per_1k_tokens` | `float \| None` | `ge=0.0`（ai.py:166） |
| `max_output_tokens` | `int \| None` | `ge=1, le=200000`（ai.py:167） |
| `max_input_tokens` | `int \| None` | `ge=1, le=2000000`（ai.py:168） |
| `timeout` | `int \| None` | `ge=1, le=600`（ai.py:169） |
| `temperature` | `float \| None` | `ge=0.0, le=2.0`（ai.py:170） |
| `enabled` | `bool \| None` | ai.py:171 |
| `is_default` | `bool \| None` | ai.py:172；结果态为默认且禁用时拒绝（ai.py:385-397） |
| `sort_order` | `int \| None` | `ge=0, le=9999`（ai.py:173） |

**PUT `/api/ai/models/{model_id:path}/default` — 请求体可选 `ModelUpdate | None`**（ai.py:429）：
无 body 亦可；若带 body 且 `{model_id}/default` 这个更长 id 的行存在，body 会应用到该长 id 上
（ai.py:447-456，歧义处理见 ai.py:436-444）。路径参数 `model_id` 为 `str`。

**DELETE `/api/ai/models/{model_id:path}` — 无请求体**：仅路径参数 `model_id: str`（ai.py:499-504）。
（`ModelDeleted` 在 ai.py:176-180 被注释为「Body of DELETE」，但源码实际把它用作
`response_model`，见 ai.py:499、525。）

### 6.2 响应 Schema

统一错误包络：`{"error": {"code": …, "message": …, "details": …}}`
（`ICAError.to_payload`，`backend/app/errors.py:32-41`；注册于 `backend/app/main.py:127-144`）。

| 路由 | 成功状态 | `response_model` | 字段 |
| --- | --- | --- | --- |
| POST | `201 Created`（ai.py:319 显式 `status_code`） | `ModelRead` | 见下表 17 字段 |
| PUT（通用） | `200`（FastAPI 默认；ai.py:486 未显式声明 status_code） | `ModelRead` | 同上 |
| PUT `/default` | `200`（同上，ai.py:424） | `ModelRead` | 同上 |
| DELETE | `200`（同上，ai.py:499） | `ModelDeleted`（ai.py:176-180,525） | `model_id: str`；`deleted: bool = True` |

`ModelRead`（ai.py:41-62；经 `_read_model` → `ModelRegistry.list_all_models()` →
`_read_config` 的生产读路径生成，ai.py:283-316）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `model_id` / `display_name` / `provider_type` / `base_url` / `api_key_env` | `str` | 基础配置（ai.py:44-48） |
| `quality_tier` | `str` | 枚举值字符串（ai.py:49） |
| `input_cost_per_1k_tokens` / `output_cost_per_1k_tokens` | `float` | ai.py:50-51 |
| `max_output_tokens` / `max_input_tokens` / `timeout` | `int` | ai.py:52-54 |
| `temperature` | `float` | ai.py:55 |
| `enabled` / `is_default` | `bool` | ai.py:56-57 |
| `sort_order` | `int` | ai.py:58 |
| `api_key_configured` | `bool` | `bool(cfg.api_key)`，只表示是否配置了 key，**从不返回 key 值**（ai.py:59、101-103） |
| `source` | `str`，默认 `"database"` | `"database"`＝来自 `ai_model_configs`；`"settings"`＝表空时的 V1.0 兜底（ai.py:60-62） |

`ModelRead` 继承 `ORMModel`（`app/schemas/common.py:46-49`，仅 `from_attributes=True`，
无额外字段）；该基类再继承 `UtcModel`——其字段集合以基线 `9ed173c` 为准，
来源 `backend/app/schemas/common.py`。

### 6.3 错误码表

| HTTP | `error.code` | 触发条件 | 来源 |
| --- | --- | --- | --- |
| `403` | `admin_required` | 多用户模式下非管理员调用任一写路由；本地单人模式放行 | `app/api/settings.py:36,70-83`；四处 `_require_admin`：ai.py:326,445,494,511 |
| `422` | `unsupported_provider_type` | `provider_type` 不在支持集合 | ai.py:183-192 |
| `422` | `invalid_base_url` | `base_url` 不以 `http://`/`https://` 开头 | ai.py:193-199 |
| `422` | `invalid_quality_tier` | `quality_tier` 不在 `QualityTier` 枚举 | ai.py:200-207 |
| `422` | `model_disabled` | POST：`is_default` 且 `!enabled`；PUT：结果态默认且禁用；PUT `/default`：目标行已禁用 | ai.py:329-334,386-397,459-464 |
| `422` | `validation_error` | Pydantic 请求校验失败（字段约束 / JSON 形状），由 `RequestValidationError` 处理器统一返回 | `app/main.py:131-144` |
| `404` | `not_found` | 目标 `model_id` 行不存在（含写后读回失败） | ai.py:221-228,306-316 |
| `409` | `ai_model_exists` | POST 的 `model_id` 已存在，**不覆盖** | ai.py:338-346 |

说明：`ValidationError` / `NotFoundError` / `ConflictError` 的类默认码分别是
`validation_error` / `not_found` / `conflict`（`app/errors.py:49-62`），但上表中
ai.py 抛出的实例均显式传了 `code=` 覆盖；`details` 至少含 `model_id` 等上下文字段，
完整字段集合以基线 `9ed173c` 为准，来源 `backend/app/api/ai.py` 对应 raise 处。

### 6.4 状态转换与副作用

状态用行是否存在 × `enabled` × `is_default` 描述；不变量：
**同一时刻至多一行 `is_default`**，由应用层 `_clear_other_defaults` 保证
（ai.py:231-247），不靠唯一索引（表结构冻结 14 列，以基线 `9ed173c` 为准，
来源 `backend/app/models/__init__.py` 的 `AIModelConfig`）；且默认模型必须 `enabled`。

| 起始态 | 操作 | 结果态 | 副作用（源码） |
| --- | --- | --- | --- |
| 行不存在 | POST（`is_default=false`） | 存在·按 `enabled` | 校验 → 空 `display_name` 回填 `model_id` → 查重 → 插入 → `db.flush()` → 审计 `ai.model_created` → `db.commit()` → 读路径回读（ai.py:326-364） |
| 行不存在 | POST（`is_default=true`） | 存在·启用·默认 | 同上，且先 `_clear_other_defaults(keep=新 id)` 再插入（ai.py:348-349）；非启用态拒绝 422 |
| 任意存在态 | PUT（部分字段） | 字段按 body 更新的存在态 | 404 校验 → 值校验 → 默认/启用矛盾校验 → 若 `is_default=true` 先清其他默认 → 仅写实际变更字段 → **仅当 `changed` 非空**审计 `ai.model_updated` → commit → 回读（ai.py:367-421,486-496）；`model_id` 不可改 |
| 存在·启用·非默认 | PUT `/default` | 存在·启用·默认 | 管理员校验 → `_clear_other_defaults(keep=id)` → `row.is_default=true` → 审计 `ai.model_default_set`（metadata 含 `previous_defaults`、`provider_type`、去 query 的 `base_url`）→ commit → 回读（ai.py:424-483）；对已是默认的行重复调用仍写审计 |
| 存在·禁用 | PUT `/default` | 不变 | 拒绝 422 `model_disabled`（ai.py:458-464） |
| 路径 id 以 `/default` 结尾且该长 id 行存在 | PUT `/default` | 长 id 行被局部更新 | body 应用到 `f"{model_id}/default"`，短 id 行不动（ai.py:436-456） |
| 任意存在态 | DELETE | 行不存在 | 404 校验 → `db.delete(row)` → 审计 `ai.model_deleted` → commit → 返回 `ModelDeleted`（ai.py:499-525）；**不删除该模型的凭据槽位、不自动补一个新默认**（ai.py:505-510；UI 侧同注 `endpoints.ts:227`） |

其他副作用口径：
- 请求与响应均不带 key；审计 metadata 只含环境变量**名**（`api_key_env`）与脱敏 URL
  （`_audit_url` 去 query/fragment，ai.py:250-280）；审计事件的落表字段以基线 `9ed173c`
  为准，来源 `backend/app/application/common.py:record_audit`。
- 这 4 条与 W5 的 AI 路由同仓同表 `ai_model_configs` / `ai_model_routing` / `ai_usage_daily`
  （§1）；「默认模型」切换与 W5 routing 写路径须同一事务保证原子性（§5），
  Rust 侧状态机实现以 W5 为准，来源 `crates/ica-application/src/ai_registry.rs`。

## 7. 调用方查证（AC2）

### 7.1 UI 侧：5 方法（已核实）

来源 `frontend/src/services/endpoints.ts:207-230`（`aiApi`）：

| 方法 | HTTP | 路径 | 位置 |
| --- | --- | --- | --- |
| `listModels` | GET | `/ai/models` | endpoints.ts:208 |
| `createModel` | POST | `/ai/models` | endpoints.ts:217 |
| `updateModel` | PUT | `/ai/models/${encodeURIComponent(modelId)}` | endpoints.ts:220-221 |
| `setDefaultModel` | PUT | `/ai/models/${encodeURIComponent(modelId)}/default` | endpoints.ts:224-225 |
| `deleteModel` | DELETE | `/ai/models/${encodeURIComponent(modelId)}` | endpoints.ts:228-229 |

`model_id` 常含 `/` 与 `:`（如 `nvidia/nemotron-3.5-super-vl:free`），后端以
`{model_id:path}` 接收，故 URL 必须百分号编码，否则路径被截断（endpoints.ts:214-215）。
ADR-0003 要求新增路由逐条记录「是否被 UI 调用 / 是否被 Agent 或 MCP 调用」
（`docs/adr/ADR-0003-governance-decisions-20261119.md:183-191`）。

### 7.2 Agent / MCP 侧：可复现命令与真实结论

查证命令（在 `/mnt/o/ICA-RUST` 执行）：

```bash
grep -rn 'ai/models\|listModels' crates/ --include=*.rs | grep -i hermes
```

**真实结论：输出为空，`grep` 退出码 1（0 命中）。**
即 `crates/` 下 `.rs` 文件中没有任何同时涉及 `ai/models`/`listModels` 与
hermes（或 MCP）的代码——**当前不存在调用这 4 条路由（也不存在调用
`GET /ai/models`）的 Agent/MCP 调用方**。AI 相关 handler 一律调用
`ica_application`（例：`crates/ica-api/src/routes.rs:1109-1119` 的 `list_ai_models`）。

旁证（同环境实测）：

```bash
grep -rniE 'hermes|mcp' crates/ --include=*.rs | wc -l   # → 0
grep -rn 'ai/models\|listModels' crates/ --include=*.rs # → 7 处
```

7 处命中全部是 W5 的只读 GET 路径与测试，不是 Agent/MCP 调用方：
`crates/ica-api/src/routes.rs:92,209,1109`、
`crates/ica-api/tests/ai_http.rs:1,16`、
`crates/ica-api/tests/ai_key_backups_http.rs:35`、
`crates/ica-application/src/ai_registry.rs:155`。

MCP 工具面：`ica.ping` 是**唯一** MCP 工具——ADR-0003 逐项授权表 G3 行写明
「Hermes POC，仅 `ica.ping`」，只读验证运行时能力、不碰业务数据与契约
（`docs/adr/ADR-0003-governance-decisions-20261119.md:399`）。
除该工具之外的任何 Agent/MCP 调用面均未在仓库中登记，以基线/ADR-0003 为准，
来源同上（:399 与 :187）。本结论是当前代码检索快照；若将来新增 Agent/MCP 调用方，
须重跑本节命令并更新本表。
