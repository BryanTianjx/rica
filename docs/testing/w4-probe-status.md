# W4 回归探针执行状态（2026-11-22 独立复核后续）

> 独立复核把 8 条 AC 标为「无法判定 / NOT_RUN」。本文如实记录它们的后续执行状态：
> 能在当前环境跑的已跑（附真实输出），受环境或方法论限制的保持 NOT_RUN——
> **不把未执行的动态验证伪装成 VERIFIED**。

## 环境事实

- OCR 模型目录 `<repo>/models/` 为空（当前环境无 PP-OCRv6 模型文件）；
  真 OCR 推理需要模型，宽图扫描/缺模型恢复类探针无法在此环境执行。
- 变异测试（临时回退修复→跑用例→断言变红）需要改源码并全量重编（9p 上每次 ~4 分钟），
  属方法论验证，不等于"行为正确性"——已有 2 条 mutation 记录（time.rs bare-date、ocr_rapid.rs detector）。

## 逐条状态

| 卡 | AC | 判据 | 后续状态 | 证据 / 受限原因 |
| --- | --- | --- | --- | --- |
| W4-R1 | AC2 | ≥1000px 多行截图两侧逐字符一致 | 部分执行 | contract_verify 的 `imports_ocr_wide_image`（1000×136）两侧 text 逐字相同；≥4 宽图×3 字号全档扫描受模型缺失限制 |
| W4-R1 | AC3 | 740–1400px 全档无分叉、757 翻面点消失 | 受限 | `the_detector_runs_unrestricted_like_the_baseline` 单测通过（unrestricted 配置已钉住）+ mutation killed=true；多档真图扫描需模型 |
| W4-R1 | AC5 | 缺模型初始化失败后同进程补模型恢复 | 受限 | 源码 `OnceLock<Mutex<Option<Arc<...>>>>` 已确认失败不缓存（ocr_rapid.rs:129-136）；真机探针需模型 |
| W4-R1 | AC6 | 锁中毒 code=ocr_error | 部分执行 | 源码 `.unwrap_or_else(poisoned.into_inner)` + `code:"ocr_error"`（ocr_rapid.rs:143/238）已坐实；注入锁中毒的动态断言未执行 |
| W4-R1 | AC7 | 回归用例在修复前变红 | 部分执行 | mutation-test-results.json：ocr_rapid detector 变异 killed=true；其余逐条变异未 sweep |
| W4-R2 | AC6 | 每条修复配一条修复前会红的用例 | 部分执行 | w4_reaudit_regressions 8 条单测全过、断言具体基线值；逐条变异未 sweep |
| W4-R3 | AC6 | 每条规则配一条修复前会红的用例 | 部分执行 | 同上；`import_positions_name_too_long` 已从189修为201字符并两侧422 |
| W4-R4 | AC4 | F1–F4 逐项结论（F3 比对库内值） | 已补台账 | wave-acceptance/W4/p2-defect-ledger.md 六项逐条裁决（含 F3 库内值） |

## 已真实执行的探针输出（本轮）

```
$ cargo test -p ica-api --test w4_reaudit_regressions
test result: ok. 8 passed; 0 failed
$ cargo test -p ica-infra --lib ocr_rapid
test result: ok. 8 passed; 0 failed; 14 filtered out
$ python3 scripts/contract_verify.sh  (OUT_DIR=/tmp/ica-rust-w4r3-out)
286/286 cases matched, 0 failure(s)
```

## 结论

8 条中：1 条（R4 AC4）已由台账补齐可收口；4 条（R1 AC2/AC6/AC7、R2 AC6、R3 AC6）
有行为级证据但变异/全档方法论未逐条执行；3 条（R1 AC3/AC5 + 宽图全档）受模型缺失限制。
按 woticketflow，方法论未执行与行为已验证须分开陈述——本文即该区分的落盘。

## 用户裁决（2026-11-22）：接受环境限制

用户已明确接受以下环境受限欠账，要求其余卡按计划推进到关单：

| 卡 | 受限项 | 处置 |
|---|---|---|
| W4-R1 | 5 条 OCR 探针（≥1000px 宽图逐字符 / 757px 全档扫描 / 缺模型同进程恢复 / 锁中毒注入 / 逐条变异）NOT_RUN | 保留 REVIEW，resume_state 已记欠账；需真实 PP-OCRv6 模型的环境（Windows 实机）补跑 |
| W4-R3 | AC6 逐条变异未执行 | 保留 REVIEW，行为级单测已绿 |
| G3 | 7 条 Hermes Runtime POC 未实施 | 保留 BLOCKED（此前用户裁决转期） |
| ICA-BROWSER-UI-RELEASE | Windows 端到端实机验收 | 保留 BLOCKED |
| plan:ICA-Browser | AC30 Windows/macOS 双平台部署记录 | 保留 REVIEW |

**这些卡不伪造关单**：在 WSL 环境无真实 OCR 模型 / 无 Windows/macOS 实机的条件下无法产生合规证据。
裁决含义是「接受它们留在非终态作为已知欠账」，不是「把它们标成完成」。
其他任务卡按既定计划推进到关单。
