# Phase 0 验证基线记录（整改前）

- 执行时间：2026-11-24 12:05 UTC（20:05 +0800）
- HEAD：`6b303212865adf13deecb3d3bfef5d52ae380381`（`merge: UI Phase 6 整体质量优化`）
- 工作区：存在未提交的 UI bundle 产物（`bundle-resources/ui/index.html` 与 `assets/`）以及 R2.0 设计文档变更；本记录只覆盖已提交代码，未包含这些工作区文件。
- 门禁命令：`python scripts/gate.py --out docs/testing/evidence/gate-6b30321-20261124/gate-run.json`
- 机器可读结果：[gate-run.json](./gate-run.json)
- 控制台输出：[gate-console.log](./gate-console.log)
- 依赖扫描（独立执行，因门禁脚本当时使用了不在本机 cargo-audit 0.23.0 中支持的 `--locked`）：[cargo-audit-current.log](./cargo-audit-current.log)

## 结果摘要

`gate_result = FAIL`，PASS=4 / FAIL=4 / SKIP=3 / BLOCKED=2。

| 阶段 | 结果 | 说明 |
|---|---|---|
| rust_gate | PASS | layering / fmt / clippy -D warnings / workspace tests / endpoint reconciliation；683s |
| sidecar_contract | PASS | ica-contracts sidecar wire contract |
| endpoint_map | PASS | 契约表自测 |
| endpoint_map_baseline | SKIP | 本机无 Python baseline venv（`/mnt/o/ica/backend`） |
| contract_verify | SKIP | opt-in，需双服务 + Python baseline |
| frontend_install | SKIP | node_modules 已存在 |
| frontend_lint / build / test | FAIL | 门禁脚本在 Windows 上传入裸 `npm` 触发 WinError 2，非项目代码失败 |
| tauri_syntax | PASS | 仅语法检查 |
| tauri_build | BLOCKED | 本机无 glib-2.0 / WebKit2GTK |
| security_scan | FAIL | 门禁使用 `cargo audit --locked`，本机 cargo-audit 0.23.0 不支持该参数 |
| e2e | BLOCKED | 仓库无 E2E harness |

## 依赖扫描实际结果（cargo audit 0.23.0）

- RUSTSEC-2026-0459：`libsqlite3-sys 0.30.1`，中危 6.8，修复版本 >=0.39.0。
- RUSTSEC-2026-0592：`url 2.5.8`，修复版本 >=2.5.9。
- 3 项 unmaintained 警告：chrono 0.4.45、paste 1.0.15、ttf-parser 0.25.1。

## 后续

整改后的门禁结果另行归档，不在本记录内。本记录是审计 P1-03「当前 HEAD 缺少全栈重新验收证据」的原始证据，整改后必须绑定新的 HEAD 并重新执行，不得自动继承。
