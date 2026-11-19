# ICA-RUST W4 复审报告（15 项发现的独立复核）

- 审计对象：`/tmp/audit-w4-2`（tag `w4-candidate` 导出快照，非 git 仓）
- 复验时间：2026-11-19 23:20 – 23:55 CST
- 审计员立场：独立实测；不采信实现者说法、不采信测试的"绿"
- 运行时：Python 基线 8110 / Rust 8111 / Rust（无 OCR 模型）8112，两侧各自独立 SQLite
- 证据目录：`/tmp/w4v-work/`（`python.json`、`rust.json`、`python-browser.json`、`rust-browser.json`、三份服务日志）
- 探针脚本：`/tmp/w4v-probe.py`（15 项 + 已知修复点）、`/tmp/w4v-browser-probe.py`（基线漂移三处）

## 0. 结论摘要

**VERDICT: FAIL**

15 项里 **13 项已真正关闭**（P0-1/2/3/5、P1-1..8、P2-2）；
**P0-4 只关闭了原报告举的三个例子，规则集仍缺 3 条、错误信封形状不符**；
P2-1 的"桩"已换成真引擎，但同一张图的识别文本比基线少一个空格。

此外本次实测发现 **3 个新缺陷**（`id:null` → 500、`id:""` → 空主键、内容修正后 `content_hash` 不重算）和 **1 组基线漂移缺口**（`browser_extract` / `confirm-candidate` / reject 副作用，Rust 全部未实现，且对拍集从未覆盖）。

## 1. 复核环境与可追溯性

| 项 | 实测值 |
| --- | --- |
| Python 基线 | `/mnt/o/ica` HEAD **9ed173c** `freeze: Python 基线终态`，`git status --porcelain` 为空 |
| Rust 被测源码 | 快照与 `/mnt/o/ICA-RUST` 工作树逐字节一致（`rsync -an` 无差异） |
| Rust 二进制 | 由快照重建：`sha256 a217a695…f93ab`，与现成 release 产物**哈希相同** |
| 数据库 | `seed.db` 播种后各自复制；Python 侧先清 `-wal/-shm`（首轮曾因残留 WAL 复现旧状态，已作废重跑） |
| 测试 | `cargo test -p ica-api --test w4_imports_http` = 12 passed；`--test w4_audit_regressions` = 3 passed |

```text
$ git -C /mnt/o/ica log --oneline -1
9ed173c freeze: Python 基线终态 —— 浏览器随包交付、供应商预设、双主题、工作台日历与后端修复
$ sha256sum …/ica-rust-target-w4v/release/ica-server …/ica-rust-target/release/ica-server
a217a695746a78f01f7a5c24d38444fe4919645a90e4adc2948870cf6afb93ab  (两个路径相同)
```

## 2. 逐项复核表

| 编号 | 原判定 | 修复声称 | 我的独立验证方法 | 实测结果（PY / RS） | 结论 |
| --- | --- | --- | --- | --- | --- |
| P0-1 | 部分列快照被 NULL 覆盖，insert/update 均 500 | 只写 payload 出现的列，缺列用 ORM/DDL 默认 | 发 `{"candidates":[{"id":…,"name":"Partial Only"}]}`；再对 Dave 发 `name+updated_at`；直查库 | 插入：两侧 `200 imported:1`，落库 `source=boss/status=New/archived=0`；更新：两侧 `200 updated:1`，`source/status/created_at` 均保留 | **已关闭** |
| P0-2 | LWW 用 `>=`，等时戳本应跳过 | 改严格 `>`，等时戳/旧时戳走 skipped+conflicts | `POST /export` 后原样 `POST /import` | 两侧均 `{"imported":0,"updated":0,"skipped":44,"conflicts":0}` | **已关闭** |
| P0-3 | 无 `id` 的行被丢弃 | 无 `id` 时铸 UUID 并 insert | 发 `{"candidates":[{"name":"No Id Row",…}]}` | 两侧 `200 imported:1`，库中各出现 1 行，id 为新 UUID | **已关闭**（但见新缺陷 N-1/N-2） |
| P0-4 | 无 payload 校验：422 变 500 或 200 | 补 `_validate_payload` 等价实现 | 3 个原例 + 5 个 `_position_problems` 补充用例 | 原 3 例状态码/错误码已对齐；**但** `details.errors` 为字符串而非数组；`status/location/archived` 三条规则缺失（RS 200 写入，PY 422） | **仍存在（部分）** |
| P0-5 | 不认 append-only `send_records` | 已存行永不重写，差异计 conflicts | 对已有 send_record 发送 `status:"bounced"` + 2099 时戳 | 两侧 `{"skipped":1,"conflicts":1}`，库中仍为 `status=sent`，`updated_at` 未动 | **已关闭** |
| P1-1 | conflicts 计数条件不同，6 场景少计 | 按 `_business_data_differs` 判定 | 4 个场景：旧时戳差异/等时戳差异/无时戳差异/无时戳相同 | 依次 `1/1`、`1/1`、`1/1`、`1/0`（skipped/conflicts），两侧完全一致 | **已关闭** |
| P1-2 | confirm 未知 conversation 返回 409 | 改 `not_found` | `conversation_id:"does-not-exist"` | 两侧 `404 {"code":"not_found","message":"Conversation 'does-not-exist' was not found"}`；会话仍 `previewed`、`raw_text` 保留 | **已关闭** |
| P1-3 | 跨 candidate 的 conversation 返回 409 | 改 `Validation`(422) + 两个 id | 用 Alice 的会话 confirm Bob 的会话 | 两侧 `422 validation_error`、同 message、`details` 同值 | **已关闭**（仅 `details` 键序不同，见 N-4） |
| P1-4 | `messages.source` 硬编码 `paste_import` | 取 `session.source` | preview 用 `source:"screenshot_ocr"` 后 confirm | 两侧新增消息 `source='screenshot_ocr'` | **已关闭** |
| P1-5 | `messages.channel` 硬编码 `boss` | 取 `conversation.channel` | 先把会话 channel 改为 `linkedin` 再 confirm | 两侧 `channel='linkedin'` | **已关闭** |
| P1-6 | 不重算 `conversations.last_message_at` | confirm 尾部 `MAX(message_time)` 回写 | 修正 3 条消息时间为 10:00/10:05/10:07 后 confirm | 两侧 `2026-11-18 10:07:00.000000` | **已关闭** |
| P1-7 | 不回写 `import_sessions.candidate_id` | confirm 时写入 candidate | preview 不带 candidate，confirm 给 Bob | 两侧 `candidate_id=…0002`，状态 `confirmed` | **已关闭** |
| P1-8 | OCR 引擎不可用时暂存文件不删 | 把删除移到"无论如何执行" | ①非图片文件；②`RAPIDOCR_MODEL_DIR=/nonexistent` 强制 503 | ①两侧 409 `ocr_image_undecodable`，`file_exists_after=false`；②RS 503 `ocr_unavailable`，`file exists: NO` | **已关闭**（但无测试断言，见 §5） |
| P2-1 | OCR 永远 503 桩；与基线错误码不同 | 接真 RapidOCR 引擎 | 一张真实 PNG（"HELLO 123 / CANDIDATE OK"）与非图片 | 两侧均 `200`；文本 `"HELLO 123\nCANDIDATE OK"` vs `"HELLO 123\nCANDIDATEOK"`；错误码两侧一致 | **已关闭（有残余）**：引擎可用，识别文本差 1 个空格 |
| P2-2 | 加密导出明文未 sort_keys | 改 `python_json_sorted_compact` | 用 AES-256-GCM 解密两侧信封，比对键序与字节 | 两侧顶层 `["data","exported_at","version"]`、data 表名、行内键均字母序、紧凑无空格 | **已关闭** |

### 关键证据片段

P0-1（部分行不再 500，且默认值与基线一致）：

```text
PY {"imported":1,...}  db_row {"name":"Partial Only","source":"boss","status":"New","archived":0}
RS {"imported":1,...}  db_row {"name":"Partial Only","source":"boss","status":"New","archived":0}
PY {"imported":0,"updated":1,...} db_row {"name":"Dave Partial Rename","source":"boss","status":"New","created_at":"2026-11-10 09:05:00.000003"}
RS {"imported":0,"updated":1,...} db_row {"name":"Dave Partial Rename","source":"boss","status":"New","created_at":"2026-11-10 09:05:00.000003"}
```

P0-2（导出后原样导回，幂等）：

```text
PY {"imported":0,"updated":0,"skipped":44,"conflicts":0}
RS {"imported":0,"updated":0,"skipped":44,"conflicts":0}
```

P0-4（原三例状态码对齐，但 `details.errors` 形状不同；三条规则缺失）：

```text
空 name
PY details {"errors":[{"table":"positions","index":0,"field":"name",...}]}
RS details {"errors":"[{\"table\":\"positions\",\"index\":0,...}]"}        ← 字符串

status="bogus"
PY 422 {"code":"import_validation_error","details":{"errors":[{"field":"status",...}]}}
RS 200 {"imported":1,"updated":0,"skipped":0,"conflicts":0}

location=123 / archived="yes" 同上：PY 422，RS 200 imported:1
```

P0-5：

```text
PY {"skipped":1,"conflicts":1}  db_row {"status":"sent","updated_at":"2026-11-10 10:00:00.000000"}
RS {"skipped":1,"conflicts":1}  db_row {"status":"sent","updated_at":"2026-11-10 10:00:00.000000"}
```

P1-2 / P1-3：

```text
P1-2 PY 404 not_found "Conversation 'does-not-exist' was not found"
     RS 404 not_found "Conversation 'does-not-exist' was not found"
P1-3 PY 422 validation_error details {"conversation_id":..., "candidate_id":...}
     RS 422 validation_error details {"candidate_id":..., "conversation_id":...}
```

P1-4/5/6/7（一次 confirm 同时验证四件事）：

```text
session: {"status":"confirmed","raw_text":"","candidate_id":"…0001"}
messages: {"content":"再一条","source":"screenshot_ocr","channel":"linkedin","message_time":"2026-11-18 10:07:00.000000"}
conversation.last_message_at: "2026-11-18 10:07:00.000000"
（PY / RS 上述四项完全一致）
```

P1-8（503 路径也不留文件）：

```text
$ curl -X POST :8112/api/imports/ocr -d '{"image_path":"…/ocr/probe.png"}'
{"error":{"code":"ocr_unavailable",…}}   HTTP 503
file exists: NO
```

P2-2（解密后的明文键序）：

```text
PY top_level_keys ["data","exported_at","version"]  first_row_keys ["archived","created_at","expected_location",…]
RS top_level_keys ["data","exported_at","version"]  first_row_keys ["archived","created_at","expected_location",…]
```

## 3. 额外「已知修复点」验证（派单中点名，非 15 项）

| 断言 | 实测 | 结论 |
| --- | --- | --- |
| confirm 必须清空 `raw_text` | 两侧 confirm 后 `raw_text=""`；并有回归测试 `confirming_blanks_the_raw_text` | 成立 |
| `import.confirmed` metadata = `{saved, skipped, duplicates}` | 两侧原始 TEXT 均为 `{"saved": 3, "skipped": 0, "duplicates": 0}`，字节一致 | 成立 |
| `data.exported` / `data.imported` 的 `entity_id` 必须 NULL | 两侧 `entity_type=export/import`、`entity_id=None` | 成立 |
| 加密导出也要记录表行数（封包前计数） | 加密导出的审计含 `"encrypted": true` 且 `tables` 完整（13 张表、声明序） | 成立 |
| 审计 metadata：Python 默认分隔符 + 插入序 | 相同计数组合的 metadata 字符串**多重集完全一致**（含 `", "`/`": "`、键序、tables 已排序） | 成立 |
| 加密导出明文：紧凑 + sort_keys | 解密后 compact、键字母序（P2-2 同证） | 成立 |
| 显式 `"data": null` → 422 | 两侧 422 `dict_type`；但 `input` 渲染 PY `null` / RS `"null"`（字符串） | 状态成立，细节不同 |

数据导入审计（同计数比对，逐字节）：

```text
PY  {"imported": 0, "updated": 0, "skipped": 44, "conflicts": 0, "tables": ["ai_model_configs", … "send_records"], "outcome": "ok"}
RS  {"imported": 0, "updated": 0, "skipped": 44, "conflicts": 0, "tables": ["ai_model_configs", … "send_records"], "outcome": "ok"}
```

## 4. 基线漂移（按更正后的口径）

我读基线时它处于 **9ed173c**（工作树干净）。相对锚点 `d39e321`：

```text
backend/app/api/imports.py                      | 32 ++-
backend/app/application/conversation_service.py | 13 ++
backend/app/schemas/{browser,candidate_extraction,connector,workflow}.py | +489
```

三处行为差异我都对**冻结后的基线**做了实时请求，Rust 侧均未实现：

| # | 基线行为（9ed173c 实测） | Rust 行为（实测） |
| --- | --- | --- |
| 1 | `POST /api/imports/{id}/confirm-candidate` **存在**：非 browser 会话返回 `404 {"code":"extraction_not_found", "message":"This import session is not a browser extraction draft", details:{session_id}}` | `404 {"code":"http_error","message":"Not Found"}` —— **路由不存在** |
| 2 | `preview` 接受 `source:"browser_extract"` → `200`，会话/记录正常落库；对该会话 `confirm` → `409 business_rule_violation`「…confirm it with POST /api/imports/{session_id}/confirm-candidate」 | `preview` → `422 enum`「Input should be 'manual', 'paste_import' or 'screenshot_ocr'」；手工造一条 browser_extract 会话后 `confirm` → **200** `{"saved_message_ids":[]}`（静默成功） |
| 3 | `reject` 对 browser 抽取行有副作用：`browser_candidate_extractions.status` → `rejected`、`reviewed_at` 置位，并写 `browser.candidate_extraction.reviewed` 审计 | 抽取行仍为 `draft`、`reviewed_at=NULL`、无审计事件（只写 `import.rejected`） |

**对 15 项判定的影响**：不改变任何一项的结论——15 项全部在 `paste_import` / `screenshot_ocr` / 非 browser 路径上复测，这些路径的基线行为在 `d39e321 → 9ed173c` 之间没有变化。但需要重新表述两点：

1. 上一轮报告里"基线漂移未触及 W4 契约面"的结论**在 9ed173c 上不成立**；W4 的 import 面新增了 1 条路由和 2 个分支。
2. P1-2/P1-3 的 confirm 语义现在多了一条前置守卫（browser_extract → 409）。Rust 的实现没有这条守卫，因此在 browser 会话上 P1-2/P1-3 的"已关闭"不覆盖该来源。
3. 本仓 `docs/rust-migration-scope-20261118.md §5` 把 `confirm-candidate` 登记为"范围外且不在 `/api/browser/` 前缀下的已知锐边"，理由是 BROWSER-04 **未提交**。该前提在 9ed173c 已失效（代码已冻结进基线），范围豁免需要重锚，不能继续沿用。

## 5. 覆盖边界（220/222 用例集到底覆盖了什么）

对拍用例集来自 `scripts/contract_capture.py`，本快照里定义了 **228** 个场景（`grep -c '{"name":'`）。静态与实测两条线：

| 检查 | 结果 |
| --- | --- |
| `browser_extract` 出现次数（capture + Rust 全部源码） | **0** |
| `confirm-candidate` 出现次数（capture + Rust 全部源码） | **0** |
| import 场景的 payload | 只有 `{}`、`{"data":{}}`、`{"data":null}`、6 条完整 candidate LWW、1 条完整 send_record；**没有任何 positions 校验用例、没有部分列行、没有无 id 行** |
| `audit_events.metadata_json` 的比较方式 | `JSON_COLUMNS` 声明为**解析后比较**，字节级分隔符/键序对拍看不见 |
| OCR 暂存文件是否删除 | 对拍只比 HTTP 响应 + 11 张表，**没有文件系统断言** |

**结论**：这组用例**不覆盖** `browser_extract` 来源的会话，也**不覆盖** `confirm-candidate` 端点；三处基线差异从未被任何对拍检验过。同理，P0-4 的校验路径与 P1-8 的文件删除也从未被对拍检验——它们"看起来对"只来自本次审计的临时探针。

## 6. 假绿与新问题

### 6.1 假绿（测试通过 ≠ 行为被断言）

1. **P1-8 的测试自己删文件**，服务端泄漏也照样绿：

```rust
let response = post(app, "/api/imports/ocr", &body).await;
std::fs::remove_file(&image).ok();      // ← 测试先删掉，再断言状态码
let code = json(&response.body, "error")["code"]…
```

2. **P0-4 的修复没有任何测试引用**：`grep -rn import_validation_error crates/ --include=*.rs` 只命中源码注释与实现，测试里 0 次；也没有任何测试发送 positions 导入载荷。缺的 3 条规则、字符串化的 `details.errors` 全部穿过测试与对拍。
3. **导入测试只做"导出再导入"**（`import_is_an_idempotent_upsert` 直接回灌 export 的完整行），P0-1 的部分列、P0-3 的无 id 行都不在断言范围内。
4. 对拍把 `metadata_json` 解析后比较，所以 P2-2/审计字节这类差异只能靠新加的 3 条回归测试兜住——这 3 条是有效的：前两条对原始 `metadata_json` 做**整串相等**断言，第三条 `the_encrypted_export_audit_keeps_the_declaration_order` 断言的是声明序（`candidates` 在 `ai_model_configs` 之前）与 `"encrypted": true`，不是整串字节。

### 6.2 新缺陷（本轮实测新发现）

**N-1 `"id": null` 的行 → 500（基线 200 且铸 UUID）**

```text
POST {"data":{"candidates":[{"id":null,"name":"Null Id Row"}]}}
PY 200 {"imported":1,…}   库中 id=26948440-…（新 UUID）
RS 500 {"error":{"code":"internal_error",…}}
rs log: NOT NULL constraint failed: candidates.id
```

原因：`id` 只在"键缺失"时铸 UUID；键存在但为 null 时，`insert_row` 又把 `id` 作为普通列绑了 NULL，随后再追加一次 `id`。

**N-2 `"id": ""` 的行 → 主键为空串（基线铸 UUID）**

```text
PY 200 imported:1  库中 id=67ab751a-…（新 UUID）
RS 200 imported:1  库中 id=''        ← 空主键行
```

**N-3 内容修正后 `content_hash` 不重算（基线按修正后内容重算）**

```text
preview 原文「原文内容」，confirm 时 corrections.content="改写后的内容"
PY content="改写后的内容" content_hash=sha256("改写后的内容")=9c9365…
RS content="改写后的内容" content_hash=sha256("原文内容")=c5e085…
```

影响：`messages.content_hash` 是 preview 去重（`message_hashes`）的依据，留下的是与内容不符的哈希——同一段文本下一次 preview 不会被判重。这是**静默的数据不一致**，且不属于 15 项中的任何一项，是修 P1-4/5 时未覆盖的旁路。

**N-4（轻微，字节级）**
- P1-3 的 `details` 键序：PY `conversation_id,candidate_id` / RS `candidate_id,conversation_id`（BTreeMap 排序）。
- 校验错误里的 `input`：PY 保留原 JSON 值（对象/数字/null），RS 统一转成字符串；`loc` 的数组下标 RS 是 `"0"` 而非 `0`。
- OCR `details.reason` 文案不同：PY `cannot identify image file '<path>'` / RS `The image format could not be determined`。
- 导入落库的时间戳文本格式：PY `2099-01-01 00:00:00.000000`，RS `2099-01-01T00:00:00+00:00`。经 `/api/export` 重新渲染后两侧一致（已比对），但库里原始文本不同，混格式下的 `ORDER BY/MAX` 文本排序存在风险。

## 7. 阻断项（FAIL 的理由）

1. **P0-4 未真正关闭**：`_position_problems` 的 `status` 枚举、`location/education_requirements` 类型/长度、`archived` 布尔三条规则缺失，非法行被静默写入（PY 422 / RS 200）；`details.errors` 以 JSON 字符串返回，客户端无法按数组解析。
2. **N-1/N-2**：`id:null` 触发 500、`id:""` 写入空主键——都发生在本次"已修"的导入路径上。
3. **N-3**：内容修正后 `content_hash` 不重算，破坏去重语义。
4. **基线漂移缺口**：冻结基线（9ed173c）的 `confirm-candidate` 路由、`browser_extract` 预览/确认守卫、reject 的抽取行副作用，Rust 一个都没有；且 228 条对拍用例对这三处覆盖为 **0**。范围豁免的前提（BROWSER-04 未提交）已失效。

## 8. 复跑方式

```bash
# 两侧（各自独立库）
env -u PYTHONPATH -u PYTHONHOME /mnt/o/ica/backend/.venv/bin/python /tmp/w4v-probe.py \
  --base-url http://127.0.0.1:8110 --db /tmp/w4v-work/python.db \
  --ocr-dir /tmp/w4v-work/python-data/ocr --label python --out /tmp/w4v-work/python.json
env -u PYTHONPATH -u PYTHONHOME /mnt/o/ica/backend/.venv/bin/python /tmp/w4v-probe.py \
  --base-url http://127.0.0.1:8111 --db /tmp/w4v-work/rust.db \
  --ocr-dir /tmp/w4v-work/python-data/ocr --label rust --out /tmp/w4v-work/rust.json

# 基线漂移三处
env -u PYTHONPATH -u PYTHONHOME /mnt/o/ica/backend/.venv/bin/python /tmp/w4v-browser-probe.py \
  --base-url http://127.0.0.1:8110 --db /tmp/w4v-work/python.db --out /tmp/w4v-work/python-browser.json
```

未修改 `/tmp/audit-w4-2` 的源码、未修改 `/mnt/o/ica`（`git status` 干净）、未修改 `/mnt/o/ICA-RUST`；全部写入只发生在 `/tmp/w4v-*` 与 `/tmp/w4v-work/`。
