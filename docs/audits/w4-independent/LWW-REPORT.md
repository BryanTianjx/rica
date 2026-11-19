# ICA-RUST W4 · LWW 导入合并语义 独立复核报告

- 审计角色：独立审计员（非实现者）。所有结论仅来自本人的只读读码 + 实测命令输出；不引用实现者说法。
- 审计快照：`/tmp/audit-w4-2`（只读，未修改；仅新增本报告文件）
- Rust 实现：`crates/ica-infra/src/snapshots.rs`（593 行）、`crates/ica-infra/src/time.rs`、`crates/ica-infra/src/ocr_rapid.rs`
- 基线：`/mnt/o/ica/backend`（只读）
- 实测环境：WSL2，Rust release 二进制 `CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target`（本报告中已**重新构建**，见 §6），Python `/mnt/o/ica/backend/.venv/bin/python`，两侧独立 SQLite 库

---

## 0. 版本核对（并更正一条前提）

### 0.1 快照 = tag = HEAD，工作树干净

```
$ cd /mnt/o/ICA-RUST && git cat-file -t w4-candidate && git rev-parse w4-candidate^{} && git rev-parse HEAD && git status --porcelain | wc -l
tag
304d2dab112302b5db8edecaa81f65e53b483092
304d2dab112302b5db8edecaa81f65e53b483092
0
```

`diff -rq /tmp/audit-w4-2 /mnt/o/ICA-RUST --exclude=.git --exclude=target` 只报 `scripts/__pycache__` 与 `tmp/`（非受版本控制产物）→ **审计快照内容 = HEAD 304d2da = tag w4-candidate**。

### 0.2 我对照的基线是哪个版本：**9ed173c 的工作树**

```
$ cd /mnt/o/ica && git rev-parse HEAD && git status --porcelain | wc -l
9ed173cdc6736f1213fc5697537c7f2e3a433709
0
$ git log -1 --format='%h %ci %s'
9ed173c 2026-11-19 23:18:51 +0800 freeze: Python 基线终态 —— 浏览器随包交付、供应商预设、双主题、工作台日历与后端修复
```

**明确说明**：我的逐条源码对照读的是 `/mnt/o/ica/backend` 的**工作树**，即 **9ed173c**，不是契约锚点 d39e321。这一点必须写进结论，因为两者内容并不完全一样。

### 0.3 LWW 六条规则在 `d39e321..9ed173c` 之间**没有变化**（实测）

```
$ git diff --stat d39e321..HEAD -- backend/app/application/sync_service.py backend/app/application/import_pipeline.py backend/app/api/imports.py
 backend/app/api/imports.py | 32 +++++++++++++++++++++++++++++++-
 1 file changed, 31 insertions(+), 1 deletion(-)

$ git diff d39e321..HEAD -- backend/app/application/sync_service.py
（空输出）

$ git rev-parse d39e321:backend/app/application/sync_service.py HEAD:backend/app/application/sync_service.py
df288cc1f25c2b8135ecf1776a0939ceba6ebb60
df288cc1f25c2b8135ecf1776a0939ceba6ebb60
```

→ `sync_service.py` 在两个 commit 上是**同一个 blob**（工作树文件 sha1 亦等于该 blob 内容 sha1：`ee755ab6...`）。**因此本报告 §2 的 LWW 逐条对照对锚点 d39e321 同样成立**；漂移不影响 LWW 那六条规则本身。

### 0.4 但漂移确实落在 W4 端点面上（实测确认，推翻「对 W4 结论的影响：无」）

`/tmp/audit-w4-2/docs/contract-baseline/w4-python-facts.md` §11.6 断言漂移「没有一行落在 W4 契约面」。按正确口径（`git diff d39e321..HEAD`，而非只比工作树）复测，该断言**不成立**：

```
$ git diff d39e321..HEAD -- backend/app/api/imports.py | head -40
@@ -177,6 +182,31 @@ def confirm_import(
+@router.post("/imports/{session_id}/confirm-candidate", response_model=CandidateConfirmResult)
+def confirm_candidate_import(
...
$ git diff d39e321..HEAD -- backend/app/application/conversation_service.py
+    if session.source == MessageSource.BROWSER_EXTRACT.value:
+        raise BusinessRuleError(
+            "This import session is a browser extraction; confirm it with "
+            "POST /api/imports/{session_id}/confirm-candidate"
+        )
...
+    from app.application.candidate_extraction import mark_rejected_for_session
+    mark_rejected_for_session(db, session)
```

三处行为变化（本人独立核实 Rust 侧均无实现）：

```
$ cd /tmp/audit-w4-2 && grep -rn 'confirm-candidate|confirm_candidate' crates/ --include='*.rs'
（无输出）
$ grep -rn 'browser_extract|mark_rejected_for_session' crates/ --include='*.rs'
（无输出）
$ grep -n '"/api/imports' crates/ica-api/src/routes.rs
75:    "/api/imports/preview",
76:    "/api/imports/ocr",
77:    "/api/imports/{session_id}",
78:    "/api/imports/{session_id}/confirm",
79:    "/api/imports/{session_id}/reject",
```

时间线补充：ICA-RUST tag 提交于 `2026-11-19 23:17:32`，基线 freeze 提交于 `23:18:51` —— **基线在对方交付后 1 分钟才冻结**。因此这是「锚点后移」而非「实现回退」，归属需 PM 裁决（见 §7 P1-2）。

---

## 1. 结论摘要

| 审计项 | 结论 | 依据类型 |
| --- | --- | --- |
| A. 六条 LWW 规则语义 | **6/6 与基线等价**（核心矩阵） | 读码 + 实测 |
| B. 实跑 LWW | 13 个场景中 11 个两侧响应与库内状态完全一致，2 个输入边界不一致（§4 P2-1/P2-2），1 个列覆盖差异（P2-3）、1 个 id 边界差异（P2-4） | 实测 |
| C. 门禁 335 / 对拍 220-222 | **数字属实**；2 条失败确为 dashboard 基线漂移（`calendar` 缺失），但漂移面不止 W2（见 §0.4） | 实测 |
| D. OCR 并发/资源 | 并发不崩、暂存文件全路径删除、无 BLOB 列；存在 2 个「一次失败永久失效」的资源设计缺陷（P3） | 实测 + 读码 |
| 新发现 P1 | 422 `import_validation_error` 的 `details` 信封与基线不等价（实测） | 实测 |

**核心判断：LWW 六条规则本身移植正确，但 `/api/import` 端点仍有 1 个契约可见的 P1 差异，故整体 FAIL。**

---

## 2. A 项：六条规则的逐条对照

基线函数：`/mnt/o/ica/backend/app/application/sync_service.py::_import_row`（774-857）。
Rust 函数：`/tmp/audit-w4-2/crates/ica-infra/src/snapshots.rs::import_tables`（111-228）。

### 2.1 辅助语义先对齐（决定六条规则是否真的等价）

| 辅助语义 | 基线 | Rust | 判断 |
| --- | --- | --- | --- |
| 比较时排除的列 | `id`、`updated_at`（sync_service.py:760-761, 725-726） | `id`、`updated_at`（snapshots.rs:343-345, 539-541） | 等价 |
| datetime 比较方式 | 仅对 **DateTime 列**的字符串值 `_parse_optional_datetime` 解析，然后 `_normalize_value` 把两侧都转成 `_as_utc(dt).isoformat()` 字符串再比（735-748, 765-769） | 两侧都 `parse_sqlite_datetime` 成 `DateTime<Utc>` 后比较瞬时（351-366, time.rs:33-50） | 等价（实测 TZ/DT 两例均一致） |
| 无时区 / 带时区 | naive 视为 UTC，aware 转 UTC（`_as_utc`，sync_service.py:92-101） | `parse_from_rfc3339` 转 UTC；naive 按 UTC（time.rs:38-47） | 等价 |
| 只比 payload 出现的列 | `for key, value in record.items()`（759） | `object.get(*name)`，缺失 continue（346-348） | 等价（列集合见 P2-3 例外） |
| 只写 payload 出现的列 | `_apply_fields_only` 遍历 `record.items()`（724-733） | `update_row` 对 `spec.columns` 逐个 `object.get`，缺失 continue（538-546） | 等价（实测 R6） |

### 2.2 六条规则

| # | 规则 | 基线行 | Rust 行 | 实测 | 判定 |
| --- | --- | --- | --- | --- | --- |
| 1 | 入站 `updated_at` 更旧 + 业务数据不同 → 跳过并计 conflicts | 850-856（`else` 分支：`_business_data_differs` → conflicts，`skipped=1`，不写） | 216-221（`_` 分支：`skipped+=1`，`differs` → `conflicts+=1`） | R1：两侧 `{"conflicts":1,"imported":0,"skipped":1,"updated":0}`，`name` 仍为 `Dave` | **等价** |
| 2 | `updated_at` 相等 + 业务数据不同 → 跳过并计 conflicts | 843-849（`_is_same_instant`；不同 → conflicts+skipped，相同 → 仅 skipped） | 同一 `_` 分支 216-221（`incoming_at > stored_at` 不成立即落入） | R2：两侧 `conflicts:1,skipped:1`，`name` 仍 `Frank`；R2b（数据相同）：两侧 `conflicts:0,skipped:1` | **等价** |
| 3 | 入站无 `updated_at` + 业务数据不同 → 跳过并计 conflicts | 830-837（`new_updated is None`） | 191-205（`incoming` 为 `None` 的 match 臂） | R3：两侧 `conflicts:1,skipped:1`，`name` 仍 `Bob` | **等价** |
| 4 | append-only 表（`send_records`）永不重写，但冲突仍上报 | 74（`_APPEND_ONLY_TABLES`）+ 805-813 | 28（`APPEND_ONLY_TABLES`）+ 183-189 | R4（新 ts + 不同）：两侧 `skipped:1,conflicts:1`，`content_summary` 未变；R4b（新 ts + 相同）：两侧 `skipped:1,conflicts:0`，行未变 | **等价** |
| 5 | 入站更新 → 覆盖（并推进 `updated_at`） | 838-842 | 206-213（`incoming_at > stored_at` → `update_row`） | R5：两侧 `updated:1`，`name` 变 `Eve NEW`；R5b（数据相同但 ts 更新）：两侧仍 `updated:1` 且 ts 推进 | **等价** |
| 6 | 部分 payload 只写出现的列，不清空未提及列 | 724-733（遍历 `record.items()`，只 `setattr` 出现过的键） | 536-552（`object.get` 缺失即 `continue`，`assignments` 只含出现的列） | R6（只给 `name`+ts）：两侧 `updated:1`，`source/status/created_at/archived` 全部保持原值 | **等价**（列集合例外见 P2-3） |

### 2.3 一处必须点明的实现细节（不是缺陷）

两侧对**新行**的处理也一致：基线在 `is_new` 时先 `_apply_fields_only` 再按需写 `updated_at`（818-824）；Rust 的 `insert_row` 只写 payload 提到的列，未提到的列走 `column_defaults` / DDL DEFAULT，`created_at`/`updated_at` 缺省填当前时间（472-494）。实测 N1/N2：两侧都 `imported:1`，生成的 `created_at/updated_at` 均非空、仅时间值不同（必然）。**等价**。

### 2.4 观测项：写入后的 `updated_at` 文本形态不同（不构成契约差异，实测确认）

- 基线把解析后的 `datetime` 交给 SQLAlchemy，SQLite 里落 `2030-01-01 00:00:00.000000`；
- Rust 把 payload 原文直接绑定，落 `2030-01-01T00:00:00+00:00`（带原始偏移，如 `2030-01-01T00:30:00+08:00`）。

实测该差异**不外泄到 API**：对同一批已更新行，两侧 `POST /api/export` 的 `candidates[].updated_at` 逐字符相同（含把 `+08:00` 归一成 `16:30Z`）：

```
$ diff /tmp/audit-lww2/exp_8320.txt /tmp/audit-lww2/exp_8321.txt
（C1..C5 全部相同；仅 C6 因 P2-1 的数据分歧不同）
01 Alice 2026-11-10T09:05:00+00:00
02 Bob PARTIAL 2031-01-01T00:00:00+00:00
03 Carol 2032-01-01T00:00:00+00:00
04 Dave 2026-11-10T09:05:00.000003+00:00
05 Eve NEW 2030-01-01T00:00:00+00:00
```

`GET /api/candidates/{id}` 同样一致（`"updated_at": "2029-12-31T16:30:00Z"` 两侧相同）。故仅记为**观测项**：若两实现共用同一个 SQLite 文件，库内文本格式不一致（Python 侧 SQLAlchemy 仍能读回，已实测 `OK rows: 6`）。

---

## 3. B 项：实跑证据

### 3.1 环境（独立库、独立端口，未共用）

```
$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo build -p ica-server --release
    Finished `release` profile [optimized] target(s) in 2m 01s      # 旧二进制早于 ica-infra 源码，重编译
$ env -u PYTHONPATH -u PYTHONHOME python3 scripts/seed_contract_db.py --out /tmp/audit-lww2/seed.db
seeded /tmp/audit-lww2/seed.db: 32 tables, 7 positions, 6 candidates, 2 conversations, 5 messages, 4 send records
$ cp seed.db python.db && cp seed.db rust.db
```

- Python：`/tmp/audit-py-baseline`（基线的 rsync 副本，排除 `.venv/.env/ica.db`），`env -u PYTHONPATH -u PYTHONHOME /mnt/o/ica/backend/.venv/bin/python -m uvicorn app.main:app --port 8320`，`DATABASE_URL=sqlite:////tmp/audit-lww2/python.db`
- Rust：`/home/bryan/.cache/ica-rust-target/release/ica-server`，`PORT=8321`，`DATABASE_URL=sqlite:////tmp/audit-lww2/rust.db`，`ICA_DATA_DIR=/tmp/audit-lww2/python-data`
- 探针：`/tmp/audit-lww2/probe.py`（自写，同 payload 打两侧，再直查各自库）

### 3.2 关键实测输出（原样）

```
### R1 older updated_at + different data (expect skipped=1 conflicts=1, name unchanged)
  python: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  python row: {"archived": 0, "created_at": "2026-11-10 09:05:00.000003", "name": "Dave", ...}
  rust: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  rust row: {"archived": 0, "created_at": "2026-11-10 09:05:00.000003", "name": "Dave", ...}

### R2 equal updated_at + different data
  python: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  rust:   HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}

### R2b equal updated_at + identical data
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 1, "updated": 0}
  rust:   HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 1, "updated": 0}

### R3 no updated_at + different data
  python: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  rust:   HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}

### R6 partial payload newer (only name) -> other columns keep stored values
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
  python row: {"archived": 0, "created_at": "2026-11-10 09:05:00.000001", "name": "Bob PARTIAL", "notes": null, "source": "boss", "status": "New", "updated_at": "2031-01-01 00:00:00.000000"}
  rust: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
  rust row: {"archived": 0, "created_at": "2026-11-10 09:05:00.000001", "name": "Bob PARTIAL", "notes": null, "source": "boss", "status": "New", "updated_at": "2031-01-01T00:00:00+00:00"}

### R4 append-only send_records newer + different
  python: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  python row: {"content_summary": "summary", "note": null, "sent_at": "2026-11-10 10:00:00.000000", "status": "sent", "updated_at": "2026-11-10 10:00:00.000000"}
  rust: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  rust row: {"content_summary": "summary", "note": null, "sent_at": "2026-11-10 10:00:00.000000", "status": "sent", "updated_at": "2026-11-10 10:00:00.000000"}

### R4b append-only newer + identical data
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 1, "updated": 0}
  rust:   HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 1, "updated": 0}

### R5b newer updated_at + identical data
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
  rust:   HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
```

**时区瞬时比较**（直接验证「不是字符串比」）：

```
### TZ incoming 2026-11-10T17:05:00.000005+08:00 == stored ...09:05:00.000005Z
  python: HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  rust:   HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
### TZ2 newer expressed with +08:00 offset
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}   （库内归一为 2029-12-31 16:30:00）
  rust:   HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}   （库内保留 2030-01-01T00:30:00+08:00）
```

**幂等回灌**（各自 export → 打回自己），用于暴露 `business_data_differs` 的假阳性：

```
python export tables: 13 rows: {... 'candidates': 10, ...}
python re-import own export -> 200 {"conflicts": 0, "imported": 0, "skipped": 70, "updated": 0}
rust export tables: 13 rows: {... 'candidates': 9, ...}
rust re-import own export -> 200 {"conflicts": 0, "imported": 0, "skipped": 81, "updated": 0}
```

→ 两侧都对自身快照零冲突，**没有「每次回灌都报冲突」的假阳性**。

### 3.3 update_row 在 Bool / Int / Float 列上不崩（实测确认）

```
rust positions HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
   row: [(50000, 70000, 1, 'Beijing', '2035-01-02T00:00:00+00:00')]
python insert HTTP 200 {"conflicts": 0, "imported": 1, "skipped": 0, "updated": 0}
rust   insert HTTP 200 {"conflicts": 0, "imported": 1, "skipped": 0, "updated": 0}
rust update HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
   -> [(0.9, 0.25, 0, '2036-01-01T00:00:00+00:00')]        # temperature FLOAT / enabled BOOLEAN
```

BLOB：基线 DDL 里**没有 BLOB 列**（`grep -n 'BLOB' schema/ica-baseline.sql` 无输出；`export_tables.rs` 列类型分布 Text 130 / DateTime 39 / Int 16 / Bool 7 / Float 3），因此「update_row 在 BLOB 列上崩」在当前 schema 下**不存在**（无法验证，也不需要验证）。

---

## 4. 新发现缺陷

### P1-1（阻断）`POST /api/import` 的 422 `import_validation_error` 信封与基线不等价 —— 实测确认

基线把每一项 problem 作为 **JSON 数组元素**放进 `details.errors`；Rust 把整个数组 `serde_json::to_string` 成**字符串**（snapshots.rs:438-442 的 `details.insert("errors", serde_json::to_string(&problems)...)`）。

```
python HTTP 422
{"error": {"code": "import_validation_error", "message": "...", "details": {"errors": [{"table": "positions", "index": 0, "field": "name", "message": "name is required and must be a string", "value": null}]}}}
   type(details.errors): list
rust HTTP 422
{"error": {"code": "import_validation_error", "message": "...", "details": {"errors": "[{\"table\":\"positions\",\"index\":0,\"field\":\"name\",\"message\":\"name is required and must be a string\",\"value\":null}]"}}}
   type(details.errors): str
```

同一族问题在 `validation_error`（body 校验）上也存在，且更细：`input` 被二次编码、`loc` 的数组下标变成字符串：

```
payload: {"data": {"candidates": "oops"}}
  python 422 ... "loc": ["body", "data", "candidates"], "input": "oops"
  rust   422 ... "loc": ["body", "data", "candidates"], "input": "\"oops\""
payload: {"data": {"candidates": [1, 2]}}
  python 422 ... "loc": ["body", "data", "candidates", 0], "input": 1
  rust   422 ... "loc": ["body", "data", "candidates", "0"], "input": "1"
```

契约文档 `docs/contract-baseline/w4-python-facts.md` §12.2 只写了「失败不写任何一行 / 422 import_validation_error」，**未登记该信封为 IMP**；`docs/rust-migration-contract.md` 也没有对应 IMP 条目 → 属未声明差异。对拍未覆盖该路径（见 §5.2），所以 220/222 全绿也没抓到。

### P1-2（阻断项，归属需 PM 裁决）基线 freeze 后 W4 端点面新增 1 端点 + 2 行为，Rust 未实现 —— 实测确认

见 §0.4。基线 9ed173c 新增：`POST /api/imports/{session_id}/confirm-candidate`；`confirm_import` 对 `browser_extract` 来源会话抛 `BusinessRuleError`；`reject_import` 调 `mark_rejected_for_session`。Rust 路由表只有 5 条 `/api/imports*`，无 `confirm-candidate`，grep 无 `browser_extract`。

性质判定：**不是实现回退，是锚点后移**（tag 23:17:32 → freeze 23:18:51）。若 PM 认定 9ed173c 为新锚点，则 W4 交付面缺 1 条端点，属 P1；若锚点仍为 d39e321，则记为漂移待办。

### P2-1 只带日期的 `updated_at`（`"2033-01-01"`）：基线更新，Rust 跳过并报冲突 —— 实测确认

`datetime.fromisoformat("2033-01-01")` 合法（午夜 UTC）；Rust 的 `parse_sqlite_datetime` 明确拒绝裸日期（time.rs:57-60 还专门写了测试 `a_bare_date_is_not_a_timestamp`），于是落入「无时间戳」分支。

```
### DATEONLY updated_at='2033-01-01'
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1}
  python row: {"name": "Frank DATEONLY", "updated_at": "2033-01-01 00:00:00.000000"}
  rust:   HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
  rust row:   {"name": "Frank TZ2", ...}
```

影响：响应计数不同且**基线的更新被丢弃**。可达性低（基线导出恒为带偏移的完整 ISO），但基线接受的输入被拒收，属真实语义差。

### P2-2 布尔列的数值型 payload（`"archived": 0`）：Python 等值，Rust 判冲突 —— 实测确认

Python `0 == False` 为真；Rust 用 `serde_json::Value` 比较 `Number(0)` 与 `Bool(false)` → 不等。

```
### BOOL payload archived=0 for false column
  python: HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 1, "updated": 0}
  rust:   HTTP 200 {"conflicts": 1, "imported": 0, "skipped": 1, "updated": 0}
```

只影响 `conflicts` 计数，不误写数据。Python 导出恒为 JSON bool，故仅在第三方/SQLite dump 形态快照下可达。

### P2-3 `import_sessions.raw_text`：基线写入，Rust 静默忽略 —— 实测确认

基线 `_apply_fields_only` 遍历 `record.items()` + `hasattr`，所以 payload 里的 `raw_text` 会被写；Rust 只遍历 `spec.columns`，而 `raw_text` 被 `_EXPORT_COLUMN_EXCLUDES` 排除在 spec 之外（export_tables.rs 头注释），于是既不比较也不写。

```
before: [('候选人：种子数据里的原文\n我：回复', 'previewed')]
python HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1} raw_text now: [('NEW RAW TEXT FROM SNAPSHOT', '2037-01-01 00:00:00.000000')]
rust   HTTP 200 {"conflicts": 0, "imported": 0, "skipped": 0, "updated": 1} raw_text now: [('候选人：种子数据里的原文\n我：回复', '2037-01-01T00:00:00+00:00')]
```

注意：两侧响应**完全一致**（都 `updated:1`），只有库内结果不同 —— 这正是「rule 6 只写出现的列」在 Rust 侧被 spec 收窄后的漏点。基线导出不含该列，故仅影响由旧版/第三方工具产出的快照。

### P2-4 `"id": ""` 空串主键：基线铸新 UUID，Rust 以空串为主键插入 —— 实测确认

```
python 1st: (200, {'imported': 1, ...})   empty-id rows: []      total: 9
rust   1st: (200, {'imported': 1, ...})   empty-id rows: [('', 'Empty Id')]   total: 9
python 2nd: (200, {'imported': 1, ...})   total: 10
rust   2nd: (200, {'imported': 0, 'skipped': 1, ...})   total: 9
```

基线 `_get_or_create` 用 `if not row_id` → 空串按「无 id」处理并铸 UUID；Rust 用 `and_then(|v| v.as_str())` → `Some("")`，直接当主键插入，留下一条 `id=''` 的行。第二次同 payload：基线再铸一行，Rust 命中该行转 skipped。响应与库内状态都不同。

### P3-1 OCR 引擎初始化失败被 `OnceLock` 永久缓存 —— 读码判断

`ocr_rapid.rs:109-134`：`static ENGINE: OnceLock<Result<Arc<Mutex<RapidOcr>>, String>>`。模型缺失时 503 的 `Err(String)` 被缓存，进程生命周期内**不会重试**：之后补装模型也必须重启服务。实测该 503 本身行为正确（见 §6.2）。

### P3-2 OCR 推理 panic 会永久毒化 Mutex —— 读码判断（未构造出 panic，未能验证触发）

`run_inference` 持 `Mutex` 调 `guard.run_path`（ocr_rapid.rs:218-233）。若 ONNX 推理 panic，锁被毒化，随后每个请求都返回 `OCR engine lock was poisoned`（经 `Unavailable` 变 503），直到重启。基线只锁初始化、推理不共享 `&mut`，无对应失效模式。属健壮性差异，非数据缺陷。

### P3-3 模型目录回退硬编码开发机路径 —— 读码判断

`model_dir()`（ocr_rapid.rs:88-92）在未设 `RAPIDOCR_MODEL_DIR` / `ICA_OCR_MODEL_DIR` / `ICA_DATA_DIR/ocr/models` 时，直接读 `/mnt/o/ica/backend/.venv/lib/python3.12/site-packages/rapidocr/models`。发布二进制会静默依赖这台机器的 Python 基线；换机就退化为 503。属打包耦合，不是 W4 行为缺陷。

---

## 5. C 项：门禁与对拍数字核对

### 5.1 `scripts/check.sh`：**335 passed / 0 failed 属实**

在 `/tmp/audit-w4-2`（非 git 仓）直接跑通，无需 git：

```
$ cd /tmp/audit-w4-2 && CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target bash scripts/check.sh
== 1/4 layering：ok
== 2/4 cargo fmt --check
== 3/4 cargo clippy --all-targets -- -D warnings
== 4/4 cargo test
...
tests: 335 passed / 0 failed

All gates passed.
EXIT=0
```

### 5.2 `scripts/contract_verify.sh`：**220/222 属实，2 条失败确为 dashboard `calendar`**

隔离运行（不污染快照、不与他人抢端口）：`WORK=/tmp/audit-cv OUT_DIR=/tmp/audit-cv-out PY_PORT=8420 RUST_PORT=8421`

```
220/222 cases matched, 2 failure(s)
EXIT=1

FAIL dashboard_after_drafts
  shape differs:
    python={..., "calendar": {"followups_by_day": {}, "messages_by_day": {"2026-11-19": true}, "month": true}, ...}
    rust  ={...}                       # 缺 calendar
FAIL dashboard_default
  （同上）
```

- 2 条失败**都是** Python 多出 `dashboard.calendar`（`followups_by_day/messages_by_day/month`），Rust 没有 → 与「W2 dashboard 基线漂移」的判断**一致**（该项是 9ed173c 新增功能）。
- 但「漂移只影响 W2」的说法不成立（§0.4：W4 端点面也变了）。
- 对拍**已覆盖 6 条 LWW 用例**，且全绿：

```
PASS import_lww_older_differs
PASS import_lww_equal_differs
PASS import_lww_no_timestamp_differs
PASS import_lww_newer_wins
PASS import_lww_no_timestamp_identical
PASS import_lww_append_only_send_record
```

- 对拍**未覆盖** 422 校验信封（P1-1）—— 这就是 220/222 与 P1 缺陷可以并存的原因。

---

## 6. D 项：OCR 并发与资源处理

### 6.1 并发实测（4 个请求同时打同一实例）

```
--- rust: 4 concurrent OCR ---
req1=200 23.552218s
req3=200 42.864886s
req4=200 58.650742s
req2=200 73.885259s
resp1: {"session_id":"43f0ec54-...","text":"Hello OCR\nCandidate: hi there","language":"zh+en",...}
resp2..resp4: 同上（文本逐字符相同）
```

从耗时单调递增（23.5 / 42.9 / 58.7 / 73.9 秒，步长约 15~16 秒）推断请求被 Mutex 串行化，与代码 108-134（OnceLock+Mutex）与 218-221（`engine.lock()`）的锁语义一致；**无 panic、无死锁、无 500**，四个响应文本逐字符相同。`spawn_blocking` 在 multi-thread runtime 下工作正常；`#[tokio::test]` 的 current-thread 分支未单独实测（未验证）。

### 6.2 失败路径与暂存文件（实测确认全部清理）

```
--- rust: bad image ---
{"error":{"code":"ocr_image_undecodable","message":"OCR image could not be decoded","details":{"reason":"The image format could not be determined"}}}
HTTP 409
--- leftover files in staging dir ---
total 0
```

引擎不可用（`RAPIDOCR_MODEL_DIR` 指向空目录，端口 8322）：

```
{"error":{"code":"ocr_unavailable","message":"Local OCR is not available on this machine. ...","details":{"reason":"OCR models are not available: failed to prepare ..."}}}
HTTP 503
--- leftovers ---
total 0
```

代码路径亦确认删除在 `?` 之前：`imports.rs:319-326` 把 `ensure_available` + `read_text` 包在同一个 `async` 块里先求值，再 `remove_file`，最后才 `read?`。

### 6.3 其他

- `decode_image` 整文件 `fs::read` + `load_from_memory`（ocr_rapid.rs:242-247），无大小上限；基线用 Pillow 同样全量解码 → 对齐，非缺陷（读码判断）。
- 先 decode 后 `run_path` 再读一次文件：理论上存在 TOCTOU，但文件在受控暂存目录内、且路径守卫已挡住目录外路径，风险低（读码判断）。
- 模型缓存锁文件：`ModelCache::new(model_dir())` + `ModelDownloadMode::Never`，模型存在时不写缓存目录；未观察到只读目录写入失败（实测用基线 venv 只读路径跑通 4 次推理）。

---

## 7. VERDICT

**VERDICT: FAIL**

阻断项（P1）：

1. **P1-1**（实测）`POST /api/import` 的 422 `details` 信封与基线不等价：`import_validation_error.details.errors` 是 JSON 字符串而非数组；`validation_error.details.errors[].input` 被二次编码、`loc` 索引为字符串。端点契约可见，且未登记 IMP、对拍未覆盖。
2. **P1-2**（实测，归属待 PM 裁决）基线已 freeze 到 9ed173c，W4 端点面新增 `POST /api/imports/{session_id}/confirm-candidate`、`confirm_import` 的 `browser_extract` 拒绝、`reject_import` 的 `mark_rejected_for_session` 副作用，Rust 侧三项均无实现。交付针对的是 d39e321，属锚点后移；若 9ed173c 为新锚点则阻断，否则登记为漂移待办。

非阻断差异（P2，均实测）：日期型 `updated_at`（P2-1）、布尔列数值 payload 的假冲突（P2-2）、`raw_text` 列被忽略（P2-3）、`"id": ""` 空串主键（P2-4）。
健壮性观察（P3，读码判断）：OCR 引擎失败结果被 `OnceLock` 永久缓存（P3-1）、推理 panic 毒化 Mutex（P3-2）、模型目录硬编码开发机路径（P3-3）。

**A 项结论不受以上影响：六条 LWW 规则在 Rust 中确实实现且语义等价（6/6），含「相等/更旧/无时间戳 → 跳过+冲突」「append-only 永不重写但上报冲突」「更新覆盖」「部分 payload 只写出现的列」；datetime 按瞬时比较而非字符串，`business_data_differs` 与 `_apply_fields_only` 排除的列一致（仅 `raw_text` 例外，见 P2-3）。**

---

### 附：本次审计产生的文件

| 路径 | 说明 |
| --- | --- |
| `/tmp/audit-lww2/probe.py` | LWW 主探针（13 场景） |
| `/tmp/audit-lww2/probe-output.txt` | 主探针原始输出 |
| `/tmp/audit-lww2/probe-types2.txt` | Int/Float/Bool 列更新 |
| `/tmp/audit-lww2/probe-rawtext.txt` | `raw_text` 差异 |
| `/tmp/audit-lww2/probe-emptyid.txt` | `id: ""` 差异 |
| `/tmp/audit-lww2/probe-badpayload.txt` | 422 信封差异 |
| `/tmp/audit-lww2/probe-roundtrip.txt` | 各自导出自回灌 |
| `/tmp/audit-lww2/ocr-output.txt` | OCR 并发与清理 |
| `/tmp/audit-lww2/check.log` | 门禁全量日志（335 passed） |
| `/tmp/audit-lww2/contract.log` | 对拍全量日志（220/222） |

未修改 `/tmp/audit-w4-2`（仅新增本文件）、未修改 `/mnt/o/ICA-RUST`、未修改 `/mnt/o/ica/backend`。
