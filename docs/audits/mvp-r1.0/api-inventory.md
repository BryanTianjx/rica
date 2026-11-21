# HTTP API 契约盘点(api-inventory)

**审计编号:** ICA-ENG-AUDIT-001 / WP-00
**证据等级:** E1(当前工作区源码,HEAD `ff7b9eb` + 未提交改动)
**核验日期:** 2026-11-21
**结论先行:** 代码 47 个方法级端点 = 契约文档声明 45(已完成)+ `/health` + 1 个已记载例外
(`confirm-candidate`,契约文档 §1.1 有记载)→ **无未记载漂移**。

---

## 1. 框架与入口

- **框架:** axum。HTTP 适配层仅住 `ica-api`;组合根 `ica-server/src/main.rs`。
- **监听:** env `HOST`/`PORT`,默认 `127.0.0.1:8000`;`DATABASE_URL` 默认 `sqlite:///./ica.db`;
  `AI_API_KEY` 决定 `ai_enabled`;`ICA_DATA_DIR` 决定 OCR 暂存目录(main.rs:46-98)。
- **启动:** `TcpListener::bind` → `axum::serve` + graceful shutdown(main.rs:105-110);
  自检要求 `candidates`/`positions` 表存在,缺表拒绝启动(main.rs:19, 117-134)。
- **路由注册中心:** `ica-api/src/routes.rs:86-186` 的 `router(state)`;`ROUTE_PATTERNS`(routes.rs:47-83)
  常量列全 34 条路径供 parity 中间件使用。
- **路由级 parity:** HEAD→405、尾斜杠→307、方法不允许→405 信封、未知路径→404 信封
  (`http_parity.rs:32-91`,`router_parity_http.rs` 16 个测试专门守护)。

## 2. 端点清单(47 个方法级 / 34 个路径)

### health(1)
| METHOD | 路径 | 处理函数(routes.rs) |
|---|---|---|
| GET | `/health` | `health`(90/189),返回 `{status, app, ai_enabled}` |

### positions(5)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| GET | `/api/positions` | `list_positions`(91/203) | 分页,`created_at DESC` |
| POST | `/api/positions` | `create_position`(91/213) | 201 |
| GET | `/api/positions/{id}` | `get_position`(93/224) | |
| PATCH | `/api/positions/{id}` | `update_position`(95/236) | |
| DELETE | `/api/positions/{id}` | `delete_position`(96/249) | 204,关联候选人保留行、失去链接 |

### candidates(7)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| GET | `/api/candidates` | `list_candidates`(100/260) | 分页,`updated_at DESC`,`search/status/position_id/include_archived` |
| POST | `/api/candidates` | `create_candidate`(100/270) | 201;name trim,空白拒绝 |
| GET | `/api/candidates/{id}` | `get_candidate`(104/283) | 含嵌套 position 与两个计数器 |
| PATCH | `/api/candidates/{id}` | `update_candidate`(105/297) | `position_id: null` 解除关联 |
| DELETE | `/api/candidates/{id}` | `delete_candidate`(106/315) | 204;conversations/messages/send-records 级联 |
| GET | `/api/candidates/{id}/timeline` | `candidate_timeline`(109/383) | 裸数组(非 Page),仅 `limit` |
| GET | `/api/candidates/{id}/context` | `preview_context`(153/675) | AI 上下文预览 |

### conversations / messages(4)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| GET | `/api/conversations` | `list_conversations`(114/326) | `last_message_at DESC NULLS LAST`,含 `message_count` |
| POST | `/api/conversations` | `create_conversation`(114/337) | 201,审计 `conversation.created` |
| GET | `/api/messages` | `list_messages`(116/356) | 时间序;`total` 为真实计数(IMP-1/3);200/1000 |
| POST | `/api/messages` | `create_message`(116/366) | 复用/开启候选人活跃会话;`content_hash` |

### send-records(2)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| GET | `/api/send-records` | `list_send_records`(118/417) | 四过滤器,最新发送在前 |
| POST | `/api/send-records` | `record_send`(119/404) | 201,审计 `send.recorded` |

### follow-ups(10)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| POST | `/api/follow-ups/evaluate` | `follow_up_evaluate`(121/438) | 200(不创建资源) |
| GET | `/api/follow-ups/policy/current` | `get_follow_up_policy`(123/549) | 首用建默认行 |
| PATCH | `/api/follow-ups/policy/current` | `update_follow_up_policy`(124/557) | |
| GET | `/api/follow-ups` | `list_follow_ups`(128/455) | 分页;先清扫过期任务 |
| POST | `/api/follow-ups` | `create_follow_up`(128/476) | 201;对开放任务幂等复用 |
| GET | `/api/follow-ups/{task_id}` | `get_follow_up`(131/493) | |
| PATCH | `/api/follow-ups/{task_id}` | `update_follow_up`(132/503) | 状态机校验 |
| POST | `/api/follow-ups/{task_id}/cancel` | `cancel_follow_up`(134/516) | 终端态 409 |
| POST | `/api/follow-ups/{task_id}/complete` | `complete_follow_up`(136/527) | 200/409 |
| POST | `/api/follow-ups/{task_id}/record-result` | `record_follow_up_result`(140/537) | 幂等;追加发送日志 |

### dashboard(1)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| GET | `/api/dashboard` | `dashboard`(143/568) | 聚合工作台,维护 tick 后 |

### drafts / draft-groups(9)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| GET | `/api/drafts` | `list_drafts`(144/622) | 分页,`created_at DESC` |
| POST | `/api/drafts/generate` | `generate_draft`(147/652) | 201;无 AI provider 时 503 |
| GET | `/api/drafts/{id}` | `get_draft`(148/640) | |
| PATCH | `/api/drafts/{id}` | `update_draft`(148/690) | |
| POST | `/api/drafts/{id}/approve` | `approve_draft`(149/702) | |
| POST | `/api/drafts/{id}/reject` | `reject_draft`(150/715) | |
| POST | `/api/drafts/{id}/copy` | `copy_draft`(151/725) | |
| GET | `/api/draft-groups/{group_id}` | `get_draft_group`(156/735) | |
| POST | `/api/draft-groups/{group_id}/select` | `select_draft_from_group`(170/744) | |

### imports / export / ocr(W4,6+2)
| METHOD | 路径 | 处理函数 | 备注 |
|---|---|---|---|
| POST | `/api/imports/preview` | `preview_import`(157/782) | 200,粘贴文本解析 |
| POST | `/api/imports/ocr` | `ocr_import`(160/938) | 路径守卫先于 I/O;进程内 RapidOCR |
| GET | `/api/imports/{session_id}` | `get_import`(161/793) | |
| POST | `/api/imports/{session_id}/confirm` | `confirm_import`(162/807) | 幂等复用;重复 confirm 409 |
| POST | `/api/imports/{session_id}/confirm-candidate` | `confirm_candidate_import`(163/829) | **契约外增量**(BROWSER-04),文档 §1.1 已记载 |
| POST | `/api/imports/{session_id}/reject` | `reject_import`(167/859) | |
| POST | `/api/export` | `export_data`(168/873) | `X-Encrypt-Aes-Key` 切换明文/A256GCM 信封 |
| POST | `/api/import` | `import_data`(169/919) | 幂等 upsert,逐行 LWW |

### 兜底(非业务)
`fallback`→404 信封(routes.rs:174/429)、`method_not_allowed_fallback`→405(routes.rs:175)、
HEAD 一律 405、尾斜杠 307(`http_parity.rs`)。

**未实现域:** ai / settings / audit / backups / auth / shares / browser(W5 15 条 + W6 8 条,
由已批准的 ICA-RUST-W5-W6 工作项承接;browser 整体排除在迁移外,仅 confirm-candidate 落点例外)。

## 3. 错误契约 — PASS(基础)

- 统一信封 `{"error": {"code", "message", "details"}}`:`ica-api/src/error.rs:8-10, 143-149`。
- 分层:`ContractError`(传输无关,`ica-contracts/src/error.rs:85-125`,desktop 共用)→ HTTP 绑定
  由 newtype `ApiError` 的 `IntoResponse` 完成(`error.rs:75-151`)。
- 状态码映射:404 `not_found` / 422 `validation_error`(pydantic 风格 `details.errors`)/
  409 `business_rule_violation` / 503 `ai_provider_error` / 500 `internal_error`(消息固定脱敏,真实原因仅入日志)。
- 可扩展错误码:`ValidationWithCode/StatusWithCode/ConflictWithCode`(如 `invalid_encryption_key`、
  `ocr_unavailable`、`ocr_path_not_allowed`);`ValidationIssue{type,loc,msg,input}` 逐字段对齐 pydantic。

## 4. 分页契约 — PASS

- 统一 `offset/limit`(DEFAULT_LIMIT=50、MAX_LIMIT=500;消息类 200/1000)— `ica-contracts/src/query.rs:27-33`。
- 响应信封 `PageResponse<T>{items,total,offset,limit,has_more}`(`dto.rs:466-472`);
  `has_more` 唯一实现于领域层 `Page::has_more`(`pagination.rs:41-45`,对应 Python `Page.has_more`)。
- 7 个列表端点统一;timeline 例外返回裸数组(与 Python 基线一致)。
- 查询解析刻意模仿 Python:重复 key 取最后一个(`extract.rs:61-70`)、Python `int()` 语法、超 u64 offset 422(IMP-4)。

## 5. 校验方式 — PASS(手写,pydantic 兼容)

无 validator/garde/utoipa crate。模式:serde_json 原始解析 + `ica-contracts` 手写校验器
(`body.rs` 2,004 行/16 单测;`query.rs` 885 行/21 单测)。坏 JSON→422 `json_invalid`;
`check_min_chars/check_max_chars/check_non_negative` 产出 pydantic 风格 issue;
RFC3339/naive 多格式日期解析(body.rs:144-165)。

## 6. 幂等性 — PARTIAL

- **无 `Idempotency-Key`/`X-Idempotency` 请求头处理**(全仓库无命中)。
- 领域级幂等存在:follow-up 创建对开放任务复用(`follow_ups.rs:200`)、record-result 幂等
  (`follow_ups_http.rs:201-205` 有集成测试)、快照导入幂等 upsert(`sync.rs:116`、`snapshots.rs:9`)、
  confirm 重复 409 且零写入。
- `send_records.idempotency_key` 列仅存在于导出 schema 镜像与测试种子,API 层无对应头语义。

## 7. 文档一致性 — PASS

- 权威契约:`docs/rust-migration-contract.md` §2(68 个 /api 路由-方法条目);
  §1.1 记录基线漂移与 browser 排除、confirm-candidate "sharp edge"。
- 声明 45 个已完成 vs 代码 47 = health + 45 + 1(confirm-candidate)→ **完全吻合**。
- 无独立 api-contract.md / OpenAPI 文件;`docs/contract-baseline/` 为 parity 快照 fixture
  (positions 20 case、w1 214 case)。
- 调和工具:`scripts/endpoint_reconciliation.py`(设计总方案引用)。

## 8. 测试覆盖

`ica-api/tests` 13 个集成测试文件、169 个测试函数:positions_http 28、candidates_http 21、
conversations_http 19、positions_write_http 17、router_parity_http 16、send_records_http 15、
follow_ups_http 11、write_side_effects 10、w4_imports_http 12、w4_reaudit_regressions 8、
w4_audit_regressions 3、draft_unreachable_transition 6、generate_model_resolution 3。
夹具:真实基线 DDL 临时库(include_str)+ 直连 `ica_api::router()` 发 axum 请求。

---

## 关键事实速览

| 项 | 事实 |
|---|---|
| 框架 | axum;入口 binary `ica-server`,默认 127.0.0.1:8000 |
| 端点总数 | **47 方法级 / 34 路径**(含 `/health`) |
| 契约一致性 | 47 = 45(声明)+ health + 1 已记载例外 → **无漂移** |
| 未实现域 | ai 15 / auth+shares 8(W5-W6 已立项);browser 排除 |
| 错误契约 | 统一信封 + 错误码体系 + 500 脱敏 |
| 分页 | 统一 offset/limit + PageResponse;timeline 例外 |
| 幂等 | 无请求头幂等;领域级幂等 3 处 |
| API 集成测试 | 13 文件 / 169 测试 |
