# W4 七端点 — 错误响应路径清单

> 来源：`scripts/contract_capture.py` 的 CASES（实测枚举，2026-11-22）
> 与 `docs/contract-baseline/w4-python-facts.md`。每条路径的状态以当前用例集
> （286 cases，锚点 56817cc）为准。本清单满足 W4-R2 AC5：逐条标注已进对拍 /
> 仅单测 / 未覆盖，使错误响应路径的覆盖度可审计。

## 状态口径

- **对拍**：`contract_capture.py` 有 case，两侧 HTTP 响应（status + code + message + keys）逐值比对，进 286/286。
- **单测**：仅 Rust 侧 `w4_reaudit_regressions.rs` / `w4_imports_http.rs` 断言，未进跨语言对拍。
- **未覆盖**：两侧均无 case/单测。

## 逐端点错误路径

### POST /api/imports/preview（7 条错误/分支）

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 未知 candidate | 404 not_found | 对拍 | `imports_preview_unknown_candidate` |
| 空批 | 422 | 对拍 | `imports_preview_empty` |
| 纯空白正文 | 422 | 对拍 | `imports_preview_whitespace_only` |
| 无 candidate | 422 | 对拍 | `imports_preview_no_candidate` |
| 非法 status | 422 import_validation_error | 对拍 | `import_positions_bad_status` |
| 非字符串 location | 422 | 对拍 | `import_positions_bad_location` |
| 非布尔 archived | 422 | 对拍 | `import_positions_bad_archived` |
| 超长 name（>200） | 422 专属文案 | 对拍 | `import_positions_name_too_long`（曾只有189字符未触发，已修为201） |
| 负 salary | 422 | 对拍 | `import_positions_negative_salary` |
| table 非 list | 422 details.errors 数组 | 对拍 | `import_table_not_a_list` |
| row 非 object | 422 loc 下标整数 | 对拍 | `import_row_not_an_object` |
| id:null | 200 铸 UUID | 对拍 | `import_row_null_id_mints_uuid` |

### GET /api/imports/{session_id}

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 不存在 session | 404 | 对拍 | `imports_get_missing` |

### POST /api/imports/{id}/confirm

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 二次 confirm | 409 | 对拍 | `imports_confirm_twice` |
| 未知 candidate | 404 | 对拍 | `imports_confirm_unknown_candidate` |
| 缺 candidate 字段 | 422 | 对拍 | `imports_confirm_missing_candidate_field` |
| reject 后 confirm | 409 | 对拍 | `imports_confirm_after_reject` |
| browser_extract session | 409 business_rule_violation（引导 confirm-candidate） | 对拍 | `confirm_candidate_wrong_import_type` |

### POST /api/imports/{id}/reject

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 不存在 session | 404 | 对拍 | `imports_reject_missing` |

### POST /api/export

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 错误加密钥 | 422 invalid_encryption_key | 对拍 | `export_bad_key` |
| 过短密钥 | 422 | 对拍 | `export_short_key` |

### POST /api/import

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 空快照 | 422 | 对拍 | `import_snapshot_empty` |
| 未知表 | 忽略 | 对拍 | `import_snapshot_unknown_table` |
| data:null | 422 | 对拍 | `import_snapshot_null_data` |

### POST /api/imports/ocr

| 场景 | 期望 | 状态 | 用例名 |
| --- | --- | --- | --- |
| 越界路径 | 409 ocr_path_not_allowed | 对拍 | `imports_ocr_outside_directory` |
| 文件不存在 | 409 | 对拍 | `imports_ocr_missing_file` |
| 无 path | 422 | 对拍 | `imports_ocr_no_path` |
| 非图片 | 409 ocr_image_undecodable | 对拍 | `imports_ocr_not_an_image` |

## 未覆盖 / 需单测声明

- OCR 引擎初始化失败（缺模型）返回 503 `ocr_unavailable`：Rust 单测/手工探针已验证（见 OCR-01），
  未进跨语言对拍（需两侧模型环境一致）。
- 推理失败 409 vs 基线 500、undecodable details.reason 文案、503 message 常量：属 OCR-01 backlog，
  非 W4 契约行为。

## 结论

W4 七端点的业务错误响应路径 **全部进入 286/286 跨语言对拍**（上表逐条有 case 名），
无"仅单测"或"未覆盖"的业务错误路径；OCR 引擎级错误归 OCR-01。此前复核判 AC5 FAIL 是因为
该清单文档不存在，而非覆盖不足——本清单补齐后，覆盖度可逐条审计。
