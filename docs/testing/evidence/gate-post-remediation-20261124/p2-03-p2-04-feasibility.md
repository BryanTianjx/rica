# P2-03 / P2-04 可行性结论（2026-11-24）

## P2-03：3 个 Browser POC 测试跳过

- 测试：`crates/ica-desktop/tests/browser_poc.rs:76`（full POC happy path）、`:211`（worker env carries no database）、`:249`（worker crash contained / runtime restarts）。它们**没有** `#[ignore]` 属性，而是被 `scripts/gate.py:52-60` 按函数名从 `cargo test` 中排除；`scripts/check.py:196-205` 消费该参数。
- 根因（DEF-006，`docs/testing/defect-register.md:40,89-99`）：`discover_chromium` 能“发现”浏览器，但进程无法启动/连接；3 个 worker 存活却 0 CPU，无 Chrome 进程，无 watchdog，曾挂起超过 10 分钟。
- 代码事实：headless 参数已硬编码（`crates/ica-desktop/src/bin/ica-browser-worker.rs:253-265`），`DevToolsActivePort` 等待已有 ~10s 上限（`:270-289`），host IPC 有 30/20/10s 超时（`crates/ica-desktop/src/browser.rs:155-174,371,451,489`）。**仍未设上限**的是 CDP HTTP GET 的 `read_to_end`（`:434-455`）和 WebSocket connect（`:301-303`）；host 超时后也不会杀掉孤儿 worker。
- 环境：本机无 Chrome/Chromium on PATH；`discover_chromium` 会找到 Edge（`C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`），但 DEF-006 正是“发现却不可启动”，所以仅靠 `ICA_BROWSER_CHROMIUM` 指向 Edge 不能安全解除 skip。
- 结论：**保持 3 项跳过**。只有先给 CDP HTTP/WS 连接加超时、host 超时时清理 worker、并把“启动失败”变成 `NOT_EXECUTED` 的可诊断结果，才可能逐项重试；本次不做浏览器环境变更。

## P2-04：核心招聘业务 E2E

- `frontend/package.json` 与 `node_modules` 只有 vitest/jsdom/@testing-library/oxlint；无 Playwright、Puppeteer、Selenium 或 WebDriver。真实浏览器 E2E 需要新驱动，且恰好落在 P2-03 的不可启动区域，不建议在本环境做。
- 可行的替代是**跨进程 HTTP 级 E2E**：Node 脚本用内置 `node:sqlite` 从 `crates/ica-infra/schema/ica-baseline.sql` 建临时库，spawn `ica-server`（`HOST=127.0.0.1`、动态端口、临时 `ICA_DATA_DIR`/`BACKUP_DIR`），从 stdout 读取 `{"listening": true, "port": N}`（`crates/ica-server/src/main.rs:384-389`），再用 `fetch` 走岗位→候选人→沟通→跟进→AI 草稿→审核→发送→重启后持久化的链路。无需新 npm 依赖。
- 可参考的进程内夹具：`crates/ica-api/tests/support/mod.rs:358-374`（`seeded_app`/`w1_app`）、`:378-489`（`app_state_for`/`app_for`）、`:502-658`（请求辅助）。它们是 `Router::oneshot` 助手，适合断言但不是真实 socket E2E。
- `scripts/gate.py` 的 e2e 阶段当前检查 `test:e2e`/`e2e` npm 脚本；引入 harness 后应**显式**新增该阶段，不得把 `BLOCKED` 静默改成 PASS。
- 结论：本轮先留下可复现的方案与证据，不在未验证浏览器/服务启动稳定性的前提下把 E2E 门禁假绿。
