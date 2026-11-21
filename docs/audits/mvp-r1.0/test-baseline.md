# 测试基线(test-baseline)

**审计编号:** ICA-ENG-AUDIT-001 / WP-00
**证据等级:** E1(本机实际执行)
**执行环境:** Windows 10.0.26200 x64 / Git Bash / 当前工作区(HEAD `ff7b9eb` + 未提交 B1 POC)
**执行日期:** 2026-11-21
**命令:** `cargo test --workspace --no-fail-fast`

---

## 1. 结果总览

| 指标 | 值 |
|---|---|
| 收集测试 | **354** |
| 通过 | **353** |
| 失败 | **1** |
| 忽略 | 0 |
| 失败二进制 | `ica-api` → `w4_imports_http` |

按 crate 分布(全部测试二进制跑完,fail-fast 已禁用):
ica-domain、ica-application、ica-contracts、ica-infra、ica-api(13 文件/169 测试)、
ica-desktop(3 文件:commands_smoke 4、w4_imports_commands 10、browser_poc 未提交)、
ica-server。除下述 1 个失败外全部通过。

## 2. 唯一失败 — 已定位为测试夹具平台缺陷(非产品代码缺陷)

```
test ocr_inside_the_staging_directory_reports_an_undecodable_image ... FAILED
crates/ica-api/tests/w4_imports_http.rs:364:  left: 422, right: 503
错误体: {"error":{"code":"validation_error",...,"msg":"Input should be a valid JSON
object: invalid escape at line 1 column 55","input":""}}
```

**根因链(三个平台假设叠加,全部是 WSL/POSIX 假设):**

1. `w4_imports_http.rs:337` — 暂存目录硬编码 `/tmp/ica-test-ocr`(POSIX)。
2. `w4_imports_http.rs:342-345` — JSON 用 `format!` 直接拼接 `image.display()`,
   Windows 路径 `O:\tmp\ica-test-ocr\w4-probe.png` 中的 `\i`/`\w` 是非法 JSON 转义 →
   **请求体本身解析失败 422,OCR 引擎根本未被调用**(期望的 503 `ocr_unavailable` 永远到不了)。
3. `w4_imports_http.rs:370-375` — 模型可用性检测硬编码 WSL 路径
   `/mnt/o/ica/backend/.venv/lib/python3.12/site-packages/rapidocr/models`,原生 Windows 恒为 false。

**定性:**
- 不是产品缺陷:`ocr_import` 的路径守卫、staging 校验、错误信封均未被本测试实际触达。
- 是测试夹具可移植性缺陷:同一文件还有 `preview` 等测试用 `tempfile`,不受影响;
  `ica-desktop/tests/w4_imports_commands.rs` 的同名 OCR 测试在本机通过,佐证夹具写法差异是关键。
- 归类(按 §6.3 差异分类):**BUG**(Rust 测试实现缺陷,平台假设未声明)。

**修复方向(留给后续任务包,本审计不改代码):**
路径改用 `std::env::temp_dir()`,JSON 体改用 serde_json 序列化而非 format! 拼接,
模型检测路径参数化(env 或 cfg)。

## 3. 与历史记录的对照

| 记录 | 数值 | 环境 | 本审计结论 |
|---|---|---|---|
| 设计总方案 §1(当前) | 351 passed / 0 failed | 推定 WSL(文档路径均为 `/mnt/o/...`) | 与本机 353/1 不矛盾:差异含 3 个未提交 browser_poc 测试与 1 个平台夹具失败 |
| R1.0 审计文档引用 | 333 passed / 0 failed | `bf2bae4` 历史快照 | 已过时,不可作为当前结论 |
| 本审计 E1 | **354 收集 / 353 通过 / 1 失败** | Windows 原生 | 当前工作区唯一有效基线 |

**结论:** 历史零失败结论是**环境专属**的。原生 Windows 下套件当前非绿,
但距全绿仅差 1 个夹具修复。测试数量与历史主张同量级,无系统性回归迹象。

## 4. Lint 基线

- `cargo fmt --check`:结果见本文件执行记录(历史快照"曾未通过";本次结果随审计报告更新)。
- `cargo clippy --workspace --all-targets`:结果随审计报告更新。
- 工作树已知编译警告:1 个 `unused import: std::time::Duration`(未提交的 browser POC 代码内)。

## 5. 门禁结论(对 §8.1 测试矩阵的当前映射)

| 测试类型 | 现状 |
|---|---|
| Unit Test | ica-domain/application/contracts/infra 单测齐全,通过 |
| Integration Test | ica-api 169 测试通过(除 1 个平台夹具失败) |
| Contract Test | parity fixture(positions 20 case、w1 214 case)+ 248/248 对拍(历史 E2,未在本机复跑) |
| IPC Test | commands_smoke + w4_imports_commands 通过 |
| Migration Test | **无迁移机制,无从测试**(对应 DB-02 缺口) |
| Recovery Test | **无 backup/restore,无从测试**(对应 DB-07/08 缺口) |
| Security Test | 无专门安全测试;路径守卫/加密导出有回归测试覆盖 |
| E2E Test | 无跨模块 E2E(12 步招聘闭环 §8.2 无对应物) |
| Build Test | Windows 本机 build + 测试通过(clippy 待记录) |
| Lint | 见 §4 |
