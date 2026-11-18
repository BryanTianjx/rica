# W4 基线事实（实测，非推断）

基线：`/mnt/o/ica` @ `d39e321`（只读）。全部数字来自一次真实调用序列：
`scripts/seed_contract_db.py` 造种子库 → 起 Python 服务 → 逐个打端点 → 直查库。
原始输出留档 `/tmp/w4-facts/facts.json`。

> **先读这条**：任务单 `12-W4-imports-export-ocr.md` 把 `POST /imports/preview` 称作
> 「零写入」。**实测与源码都不支持这个说法**：preview 会写 `import_sessions`、
> `import_records` 和一条 `import.previewed` 审计。它不写的是**业务表**
> （`candidates` / `messages`）。把预览当成「一个字节都不落库」会写出一版
> 与基线不等价的实现，也会让验收把忠实移植判成失败。

## 1. preview 写哪些表（实测前后对照）

| 表 | 调用前 | 调用后 | 增量 | 判定 |
| --- | --- | --- | --- | --- |
| `import_sessions` | 0 | 1 | **+1** | 写 |
| `import_records` | 0 | 1 | **+1** | 写（每个解析出的消息一条） |
| `audit_events` | 0 | 1 | **+1** | 写（`import.previewed`） |
| `candidates` | 6 | 6 | 0 | **不写** |
| `messages` | 5 | 5 | 0 | **不写** ← 这就是「零写入」真正指的东西 |

- 响应 `status` = `previewed`；解析不出消息时是 `failed`，且 `error_message` 带上解析告警。
- 响应体 9 个字段：`id` / `candidate_id` / `source` / `status` / `records` /
  `detected_duplicate_count` / `error_message` / `created_at` / `confirmed_at`。
- **没有 `total` 字段**（任务单问过「`total` 口径」，基线里不存在这个东西）。

## 2. `GET /imports/{session_id}`

同样的 9 个字段，`records` 每条 10 个字段：
`id` / `session_id` / `sequence` / `sender_type` / `content` / `message_time` /
`status` / `duplicate_of_message_id` / `content_hash` / `validation_messages`。
**无分页、无 `total`**：`records` 全量返回。

## 3. `confirm` 的原子性

- 单事务。用不存在的 `conversation_id` 触发中途失败：
  **404** `not_found` `Conversation '…' was not found`，`messages` 增量 **0**，
  五张表前后行数完全不变 → 失败不留半批。
- 成功：`{session_id, saved_message_ids, skipped_count, duplicate_count}`。
  duplicate 记录默认跳过并计入 `duplicate_count`，同时计入 `skipped_count`。
- 再次 confirm：**409** `business_rule_violation` `This import session was already confirmed`。
- 支持逐条 `corrections`（`skip` / 覆盖 `sender_type` / `content` / `message_time`）。

## 4. `reject` 之后的状态 —— **基线有缺陷，注意**

- **200**，`status` → `rejected`，且 `raw_text` 被清空为 `""`（`clear_import_raw_text`）。
- **但 reject 之后再 confirm 仍然返回 200 并真的保存了消息**：`confirm_import`
  只拦 `CONFIRMED`，**不拦 `REJECTED`**，而 `import_records` 还留着，
  于是「已拒绝」的会话仍能把消息写进会话时间线。
  源码注释写着 "Rejecting is final"，行为却不是。
- **因此**：Rust 若补上「rejected 不可 confirm」的拒绝，属**行为修复 = IMP**，
  必须登记到契约文档 §7 并写清基线的实际最终库状态；照抄基线则不加判定。
- 另一个不对称：`reject_import` 调 `get_import_session(db, session_id)`
  **没有传 `current_user`**，`confirm_import` 传了 —— 即 reject 少一层访问控制。

## 5. duplicate 判定

判据是 `content_hash`（归一化正文的哈希），与候选人已有消息的 `content_hash` 比对。
同一段文本第二次 preview：`detected_duplicate_count` = 1，该记录 `status` = `duplicate`，
`duplicate_of_message_id` 指向已有消息。事件类型 `import.previewed` 的
`metadata` 记 `{"records": N, "duplicates": M}`。

## 6. `POST /export`

- **JSON 响应，不是文件下载**：`content-type: application/json`，
  **没有 `Content-Disposition`**。
- 顶层键：`data` / `exported_at` / `version`；`version` 固定 `"0.2.0"`。
- `data` 是 13 张表的 map：
  `ai_model_configs` / `app_settings` / `audit_events` / `candidates` / `conversations` /
  `follow_up_policies` / `follow_up_tasks` / `generated_drafts` / `import_records` /
  `import_sessions` / `messages` / `positions` / `send_records`。
- 带 `X-Encrypt-Aes-Key`（base64 的 32 字节）时返回 AES-256-GCM 信封
  `{alg: "A256GCM", iv, tag, ciphertext}`；密钥不合法 → **422** `invalid_encryption_key`
  `X-Encrypt-Aes-Key must be base64-encoded 32-byte AES-256 key material`。
- 审计 `data.exported` 在**密钥校验与封装成功之后**才写，所以被拒的导出不留成功事件。

## 7. `POST /import`（与 `imports/*` 不是一回事）

- 输入 `{"data": {<表名>: [<行>...]}}`，语义是**幂等 upsert**：
  已知 UUID 按 `updated_at` 后写覆盖，未知 UUID 插入。
- 返回 `{imported, updated, skipped, conflicts}`。
- 实测（用已存在的 candidate id 只给 `name`）：`{imported:0, updated:0, skipped:1, conflicts:1}`
  —— 缺必填字段的行算 `skipped` 并计入 `conflicts`。
- 单事务；跨 tenant / owner / 引用不合法会在写入前整批拒绝。

## 8. `POST /imports/ocr`

- 输入是 **`image_path`（服务器本地路径）**，不是 base64、不是 multipart：
  `{"candidate_id": ..., "image_path": "..."}`。
- 顺序是**先路径守卫后存在性检查**：
  - 路径落在允许目录（`<ICA_DATA_DIR>/ocr`）之外 → **409** `ocr_path_not_allowed`
    `OCR image path outside allowed directory: …`，`details.allowed_prefix` 给出允许前缀。
    越界的相对路径（`../../etc/passwd`）同样 409。
  - 路径合法但文件不存在 → `BusinessRuleError("OCR image not found: …")`。
  - 图片无法解码 → `BusinessRuleError("OCR image could not be decoded", code="ocr_image_undecodable", details={"reason": …})`。
- **无 OCR 能力时用 `require_ocr()` 抛可读错误，不是 500，也不 panic。**
- 暂存文件在 `finally` 里删除，成功失败都删。
- 成功后解析成 import session（仍走 preview → confirm 两步），返回
  `{session_id, text, language: "zh+en", candidate_id, created_at}`。

## 9. 每个写操作的审计事件（实测汇总）

| event_type | entity_type | summary |
| --- | --- | --- |
| `import.previewed` | `import_session` | `Prepared import preview with N record(s)` |
| `import.confirmed` | `import_session` | `Confirmed import with N message(s)` |
| `import.rejected` | `import_session` | `Rejected import session` |
| `data.exported` | `export` | `Exported a data snapshot` |
| `data.imported` | `import` | `Imported a data snapshot (N new, M updated, K skipped)` |

`record_audit` 在 local 模式下 `actor_id` 为 `None`。

## 10. 枚举（`app/domain/enums.py`，行号已核）

```
ImportStatus  (179)：pending / previewed / confirmed / rejected / failed
RecordStatus  (193)：pending / valid / invalid / duplicate / unknown
MessageSource  (65)：manual / paste_import / screenshot_ocr
```

## 11. 跨语言对拍实测（`scripts/contract_verify.sh`，214/214）

这一节记的是**两边同时跑、逐字段 diff** 得到的事实，与上面的「源码推读」分开。
原始证据：`docs/contract-baseline/w1-python.json` / `w1-rust.json`（222 条用例）。

### 11.1 对拍暴露的七处真实差异（都已修）

| 差异 | 基线 | 修之前 | 性质 |
| --- | --- | --- | --- |
| confirm 后的 `raw_text` | 清空 | 保留 | **隐私**：原文是最富集的 PII 载荷，确认后无人再读，基线在同一事务里删 |
| `import.confirmed` 的 metadata | `{"saved":N,"skipped":N,"duplicates":N}` | `null` | 漏写审计元数据 |
| `data.exported` / `data.imported` 的 `entity_id` | `NULL` | `"export"` / `"import"` | 多写 |
| **加密**导出的 metadata.tables | 有完整表计数 | `{}` | 基线在封包**前**就取了计数，加密导出照样报 |
| 导入 payload 里的未知表 | 完全忽略（`skipped` 不变） | 计入 `skipped: 1` | 基线只遍历 `_BUSINESS_TABLES` |
| confirm 时未知 candidate | **404** `not_found` | 409 | 状态码错 |
| `"data": null` | 422 `dict_type` | 200 | 校验漏 |

### 11.2 `metadata_json` 是 TEXT 列，字节即契约

Python 的 `json.dumps` 在这个代码库里有**两种形态**，不可互换：

| 用途 | 参数 | 形态 |
| --- | --- | --- |
| 审计 `metadata_json` | `json.dumps(metadata, ensure_ascii=False)` | **默认分隔符**（`", "` / `": "`）+ **插入序**（不排序） |
| 加密导出的明文 | `json.dumps(..., separators=(",",":"), sort_keys=True)` | **紧凑** + **键排序** |

Rust 侧因此需要两个函数（`crates/ica-infra/src/json_fmt.rs`）：
`python_json()` 与 `python_json_sorted_compact()`；serde_json 开启 `preserve_order`
才能保住插入序。

> 由此推出一条通用规则：**凡是把 JSON 存进 TEXT 列的地方，格式都是契约**。
> 反过来，凡是走 HTTP JSON 响应的，键序不是契约（前端按名取值）。

### 11.3 `entity_id` 为 NULL 是常态

`record_audit` 的 `entity_id` 是可选参数。`data.exported` / `data.imported`
在基线里**不传** `entity_id`，所以列是 NULL。Rust 侧曾传 `"export"` 占位，
那会在库里留下一条基线没有的字符串。

### 11.4 schema 快照曾缺 12 张表

`crates/ica-infra/schema/ica-baseline.sql` 第一版是从**从未跑过浏览器子系统**的库导出的，
缺 12 张 `browser_*` 表。基线 `reject_import` 会查 `browser_candidate_extractions` ——
对着缺表的 schema，**基线自己**返 500（`no such table: browser_candidate_extractions`）。
这是夹具缺陷，不是契约差异，但会让忠实移植被判成不一致。

### 11.5 diff 工具本身的两个结构缺陷

1. **UUID 占位图是两侧各自生成的** → 同一个种子 id 在两侧拿到不同编号，
   每行带 `owner_id` 的记录都被读成差异（**122 处伪差异**）。
   改为两侧共用一张图（对两侧 UUID 的并集排序编号）。
2. **运行期生成的随机 id 无法按值比较** → 新建候选、确认的消息，两侧必然不同。
   改为：只在一侧出现的 UUID 按各自首次出现顺序编号成 `<new-uuid:N>`。

另外，加密导出的 `iv` / `ciphertext` **每次都不同（这是 GCM 的安全要求，
复用 nonce 是真实破解）**，只能比形状；而 `alg` 与 `tag` 的长度是契约。

> W3 的 189 条用例没暴露这两点，是因为它们恰好让两侧的 UUID 出现顺序一致。
> 用例一多、场景一改，伪差异就会淹没真差异。

### 11.6 对拍时基线已漂移（记录，不自行处置）

2026-11-19 跑对拍时，`/mnt/o/ica` 的 HEAD 已从 `d39e321` 变成 `20b252d`，
工作树另有 24 个文件未提交（AI provider / browser bridge / frontend / packaging）。
**这些改动不是本工程做的**——对拍脚本只从基线**读**（rsync 复制到 `/tmp` 后在副本上跑）。

对 W4 结论的影响：**无**。漂移没有一行落在 W4 契约面：

```
git diff --stat -- backend/app/api/imports.py   backend/app/application/conversation_service.py   backend/app/application/sync_service.py   backend/app/infrastructure/export_crypto.py   backend/app/application/import_pipeline.py backend/app/schemas/
→ 空
```

但端点账变了：`scripts/check_endpoint_map.py` 现在报
`live /api 112 → 排除 39 条 browser → 73 条业务路由`，
而契约文档 §2 记的是 **68** 条，差额 **5** 条：

| 端点 | 来源 |
| --- | --- |
| `POST /api/ai/models` | 外部会话新增 |
| `PUT /api/ai/models/{model_id:path}` | 外部会话新增 |
| `PUT /api/ai/models/{model_id:path}/default` | 外部会话新增 |
| `DELETE /api/ai/models/{model_id:path}` | 外部会话新增 |
| `POST /api/imports/{session_id}/confirm-candidate` | 基线本来就有，W4 清单只列 7 条 |

**处置：记下来交 PM 定归属，不自行改契约表。** 按既定口径，
「基线何时重锚、重锚到哪个 commit、是否纳入新增端点」是**用户决策**。

> 提醒后续会话：对拍复制的是基线**工作树**（含未提交改动），
> 所以每次交付都要写清「对的是哪个版本的基线」。
