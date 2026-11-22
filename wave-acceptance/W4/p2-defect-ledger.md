# W4-R4 P2 缺陷台账（六项逐条裁决）

- 归属卡：`ICA-RUST-W4-R4`（判据 6 条，见 `docs/ICA-RUST 当前设计总方案.md` §3.2：「P2 缺陷台账（六项逐条裁决，禁止无记录遗留）」）。
- 锚点：Python 基线 `9ed173c`（D-1 重锚）；实现提交 `66a0534`。均见 `wave-acceptance/W4/contract-coverage.json`。
- 本文件为**补建**：独立复核发现原 W4-R4 台账缺失（AC1/AC5 真缺口）。不修改 `wave-acceptance/W4/findings-register.json`——那是 P0/N/LWW-P/OCR-F 另一套编号体系，两者并存。
- 结论取值限定为 `修复` / `接受差异` / `延期`。两侧实测值全部引用仓内已有文档，未新增实测、未编造数字。

## 1. 编号映射依据

| 本台账编号 | 缺陷 | 编号来源 / 映射依据 |
| --- | --- | --- |
| F1 | OCR 检测器输入被钳制，文本分叉 | **照录**：`docs/audits/w4-independent/OCR-REPORT.md` §6 缺陷清单原编号 `F1`（P1 阻断）；`findings-register.json` 中作 `OCR-F1`。 |
| F2 | 推理失败 409 vs 基线 500 | **照录**：同上 §6 原编号 `F2`（P3）；register 作 `OCR-F2`。 |
| F3 | 不可解码图 `details.reason` 文案不同 | **照录**：同上 §6 原编号 `F3`（P3）；register 作 `OCR-F3`。 |
| F4 | 503 `message` 常量不同 | **照录**：同上 §6 原编号 `F4`（P3）；register 作 `OCR-F4`。 |
| E1 | `id:null` → 500 | **映射**：仓内无「E1」缺陷编号（E1/E2 在审计模板中另指证据等级：当前证据/历史对照，见 `docs/templates/engineering-audit-template.md`）。按任务单给定对应关系 + 两侧实测值逐字一致，对应 `FINDINGS-REPORT.md` §6.2 / `findings-register.json` 的 **N-1**。 |
| E2 | 内容修正后 `content_hash` 不重算 | **映射**：同上，对应 **N-3**。依据：任务单对应关系 + `FINDINGS-REPORT.md` §6.2 N-3 与 `w4-python-facts.md` §12.1「`messages.content_hash = content_hash(content)`，按修正后的 content 重算」一致。 |

## 2. 六项逐条裁决（AC1）

| 编号 | 差异描述 | 两侧实测值 | 契约影响判定 | 结论 | 依据 |
| --- | --- | --- | --- | --- | --- |
| **F1** | 检测器输入被 `DetInputLimits::default()` 钳制（`max_side_len=4096` / `max_pixels=2048²=4194304` / overflow=`Downscale`），基线无上限 → 同一张图行切分不同 → OCR `text` 中 `\n` ↔ 空格互换，落库 `import_records.content` 随之成为另一份文本。 | `english.png` 1000×136：PY `'Hello OCR\nCandidate: hi\nthere'` / RS `'Hello OCR\nCandidate: hi there'`（DIFF @char 23：U+000A vs U+0020，均 29 字符，3/3 复跑一致）；宽度扫描 740 px 一致、760 px 起分叉，翻面点 `4096×136/736=757`；下游落库 PY `content='Hello OCR\nCandidate: hi\nthere'` / RS `content='Hello OCR\nCandidate: hi there'`（均 1 message / 2 warnings）。修复后 1400×240 三行截图 PARITY OK（逐字符一致）。 | **P1 阻断**：直接改变落库 `import_records.content` 与消息切分，破坏 OCR→import 文本契约；属 W4-R1 必关项。 | **修复** | `OCR-REPORT.md` §3.1–3.4 / §6；`findings-register.json` OCR-F1 `CLOSED by 66a0534`（`the_detector_runs_unrestricted_like_the_baseline`，变异验证）；`acceptance-decision.md` 宽图 PARITY OK；`independent-review.md`。 |
| **F2** | 推理失败映射为 `409 ocr_error`；基线 `raise ICAError(..., code="ocr_error")` 的 `status_code` 默认 **500**（`errors.py:17`，`main.py:127-129` 直回 `exc.status_code`）。 | **读码判断，未能实测**：PY 500 / RS 409 `ocr_error`。非图片、0 字节、截断、24 MPx、空图五条路径全部在解码阶段被拦下，未能构造「解码成功但推理抛错」的触发图。 | **P3 低**：错误状态码分叉，但触发路径不可达；属 OCR 引擎错误映射/生命周期范畴，非 W4 import 契约面，不阻塞放行。 | **延期 → OCR-01** | `OCR-REPORT.md` §6 F2（读码判断，需注入测试）；`findings-register.json` OCR-F2 `OPEN, ASSIGNED to OCR-01 backlog (needs injection test)`；`acceptance-decision.md` §3 开放项 F2；`contract-coverage.json` residual `OCR-F2/F5/F6` owner = OCR-01 backlog。 |
| **F3** | `ocr_image_undecodable` 的 `details.reason` 文案不同：非图片 PY `cannot identify image file '…'` / RS `The image format could not be determined`；截断图 PY `image file is truncated` / RS `unexpected end of file`。 | C6 非图片：两侧均 409 `ocr_image_undecodable`，reason 如上；C7 0 字节：409 同 code；C8 截断 PNG（前 120 字节）：409 同 code，PY reason `image file is truncated` / RS `unexpected end of file`。13 条路径守卫用例的状态码与 code 全同。 | **P3 低**：仅人读字段 `details.reason` 差异；HTTP 状态与错误码等价，不影响契约行为。 | **延期 → OCR-01** | `OCR-REPORT.md` §4 C6/C7/C8（实测确认）、§6 F3；`findings-register.json` OCR-F3 `OPEN, ASSIGNED to OCR-01 backlog`（evidence: probe C6/C7/C8）；`acceptance-decision.md` §3 开放项 F3。 |
| **F4** | 503 `ocr_unavailable` 的 `message` 常量与基线不同：基线 ``Local OCR is not available on this machine. Install the optional OCR stack with `uv pip install -e '.[ocr]'` (RapidOCR + ONNX Runtime) to use screenshot import.``，Rust 少中间半句。 | **读码判断，未能实测**：本机基线 `OCR_AVAILABLE=True`，该分支不可达；基线 `ocr_service.py:88-96` 常量 vs Rust `ocr_rapid.rs:141-143`。 | **P3 低**：仅文案常量，错误码/状态一致；修复成本极低，不另设卡。 | **修复** | `OCR-REPORT.md` §6 F4；`findings-register.json` OCR-F4 `CLOSED by 66a0534`（`baseline constant copied byte-for-byte`）。 |
| **E1** | `POST /api/import` 中 `"id": null` 的候选行：Rust 抛 500 `internal_error`（`NOT NULL constraint failed: candidates.id`），基线 200 并铸新 UUID。 | `{"data":{"candidates":[{"id":null,"name":"Null Id Row"}]}}`：PY `200 {"imported":1,…}`，库中 `id=26948440-…`（新 UUID）；RS `500 {"error":{"code":"internal_error",…}}`，rs 日志 `NOT NULL constraint failed: candidates.id`。 | **P0（新缺陷）**：发生在本次「已修」的导入路径上，500 破坏 payload 契约等价，必须关闭。 | **修复** | 编号映射见 §1（N-1）；`FINDINGS-REPORT.md` §6.2 N-1；`findings-register.json` N-1 `CLOSED by 66a0534`（`an_id_of_null_mints_a_uuid_instead_of_a_500`）；`contract-coverage.json` 回归 `import_row_null_id_mints_uuid` PASS。 |
| **E2** | confirm 时 `corrections.content` 改写正文后 `content_hash` 未重算：Rust 仍用 preview 原文的哈希，基线按修正后内容重算。 | preview「原文内容」→ confirm `corrections.content="改写后的内容"`：PY `content="改写后的内容"`, `content_hash=sha256("改写后的内容")=9c9365…`；RS `content="改写后的内容"`, `content_hash=sha256("原文内容")=c5e085…`。 | **P1（新缺陷）**：`content_hash` 是 preview 去重（`message_hashes`）判据；残留与内容不符的哈希使同一文本下次 preview 不被判重，属静默数据不一致。 | **修复** | 编号映射见 §1（N-3）；`FINDINGS-REPORT.md` §6.2 N-3；`w4-python-facts.md` §12.1；`findings-register.json` N-3 `CLOSED by 66a0534`（`a_corrected_content_gets_a_hash_that_matches_its_text`）。 |

裁决统计：**修复 4**（F1/F4/E1/E2）、**接受差异 0**、**延期 2**（F2/F3）。四项修复均有具名回归测试（见 `findings-register.json` / `contract-coverage.json`）。

## 3. 延期项的依据与去向（AC5）

| 编号 | 延期依据 | 去向卡 | 关闭判据 / 复验要求 |
| --- | --- | --- | --- |
| F2 | 触发器不可构造：五条候选路径均在解码阶段被拦下，只有读码判断无实测；`findings-register.json` 明确标 `needs injection test`，且 `acceptance-decision.md` §3 将其列为不阻塞放行的 OCR-01 开放项。 | **OCR-01**（backlog，OCR 引擎生命周期/错误映射所有权） | 补一条「解码成功但推理抛错」的注入测试，确认 Rust 与基线同为 500 `ocr_error`；与 OCR-01 的 LWW P3-2 / F6 同批复验。 |
| F3 | P3 文案差异，状态码与 code 已等价；`findings-register.json` 标 `OPEN, ASSIGNED to OCR-01 backlog`，`contract-coverage.json` residual 明列 owner OCR-01 backlog；`acceptance-decision.md` §3 列为不阻塞放行的开放项。 | **OCR-01**（backlog） | C6/C8 的 `details.reason` 与 Pillow 原文逐字一致（`cannot identify image file '…'` / `image file is truncated`），并保留 409 `ocr_image_undecodable` 状态码。 |

**W7 去向说明**：本六项中**无** W7 归属项。`findings-register.json` 的 `LWW-P1-2`（冻结基线 9ed173c 的 `confirm-candidate` 路由、`browser_extract` 预览/确认守卫、reject 抽取行副作用三处漂移）才是 W7 归属（`ADR-0008` #5、`acceptance-decision.md` 放行条件 1、`independent-review.md`），它不属于 F1–F4/E1/E2 六项，另行跟踪。

## 4. 后续状态注记（不回写 §2 裁决）

`docs/ocr/ocr-engine-runtime-notes.md`（OCR-01-01 / 01-09 交付，2026-11-20 实测）记录：初始化失败可重试（LWW P3-1 关闭）、锁中毒恢复（LWW P3-2 / OCR F6 关闭）、推理失败改为 500 `ocr_error`（原 409，OCR F2 关闭）、`details.reason` 对齐 Pillow 原文（OCR F3 关闭）、`model_dir()` 不再回退基线 venv（OCR-01-04 关闭）。这是上表两项「延期 → OCR-01」在 OCR-01 卡内的后续交付结果；W4-R4 层面的裁决仍保持「延期」，不追认为 W4 修复。

## 5. 证据来源

- `docs/audits/w4-independent/OCR-REPORT.md`（§0/§3/§4/§6/§7）
- `docs/audits/w4-independent/FINDINGS-REPORT.md`（§6.2 N-1/N-3、§7）
- `docs/contract-baseline/w4-python-facts.md`（§12.1、§13）
- `wave-acceptance/W4/findings-register.json`、`contract-coverage.json`、`acceptance-decision.md`、`independent-review.md`
- `docs/ocr/ocr-engine-runtime-notes.md`、`docs/adr/ADR-0008-endpoint-scope-and-noncontract-deltas.md`
