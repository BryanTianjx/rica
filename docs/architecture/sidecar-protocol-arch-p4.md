# ARCH-P4 — Sidecar 协议化（交付记录）

**实现基线：** `785f3e5`（worktree `wt/ica-rust-arch-p4` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p4`）。
**性质：** 实现 + 可编译层的真实测试；未改基线仓、未 git add/commit/push、未打 tag、未接前端、未新增 crate。
**依据：** 任务卡 `ARCH-P4.md`（WI `wi_b9f02d28f1644c54`）、架构设计 §7.3/§7.4/§7.5、ADR-005、评审裁决 3、
P0 盘点 `sidecar-lifecycle-inventory.md`、P2 `runtime-unification-arch-p2.md`、P3 `tauri-runtime-convergence-arch-p3.md`。
**取数时点：** 2026-11-23（CST）。

**安全验证状态：PENDING_HUMAN（独立安全验证待人工）。** 本记录不签署 Verified。本机缺 glib/WebKit，
`desktop/tauri-host` 不能类型检查；相关结论只到「rustfmt 可解析 + 源码接线/边界断言 + 可测协议层全绿」。
Windows 侧编译、真实 Chromium sidecar、实机优雅关闭、独立安全复核见 §6 四项 gap。

---

## 1. 五条 AC 落地

| AC | 落地 | 证据 |
|---|---|---|
| AC1 版本化协议 + DTO（§7.3） | 新增 `ica-contracts::sidecar_protocol`：`PROTOCOL_VERSION = "1.0"`、`SidecarRequest` / `SidecarResponse` / `SidecarError` / `CapabilityGrant` / `PathGrant`；JSON Line 编解码、`negotiate_version()`、请求与响应校验；只依赖 serde/serde_json（传输中立）。Rust Core 与 Sidecar 共享同一 DTO 类型，不 import 任何 Python 模块。 | `ica-contracts` 协议单测 10 条 + `tests/sidecar_contract_test.rs` 6 条；`ac1_protocol_is_transport_neutral_and_not_duplicated_in_the_host` |
| AC2 超时/取消/健康/崩溃/有限重启（§7.4） | 新增 `ica-desktop::sidecar`：`PendingRequests` 有界请求表（per-request deadline、最大并发、幂等 cancel）、`cancel_request()` / `health_check_request()`、`is_crash()` / `crash_decision()`。**不重造轮子**：进程四段与 `restart()` 仍是 P3 `SidecarManager`，关闭链直接取 P3 `shutdown_plan()`，健康传输仍走 P3 `proxy::probe_health()`。新增两个 host 注入配置 `ICA_SIDECAR_REQUEST_TIMEOUT_MS`（默认 30s）/`ICA_SIDECAR_MAX_CONCURRENT_REQUESTS`（默认 16）。 | `sidecar` 单测 9 条 + `runtime_policy` 新增 1 条；`ac2_timeout_cancel_and_crash_reuse_p3_instead_of_rebuilding_it` |
| AC3 权限与文件访问控制（§7.5） | 请求必须携带显式 `CapabilityGrant`；方法不在 `method::ALL` 或无覆盖 grant 一律拒绝；`PathGrant::is_safe()` 拒绝相对路径、`..`、DB/backend/凭据类路径；`database_url`/`dsn`/`api_key` 等参数键递归拒绝；凭据由 `redact()` 统一脱敏，**错误 DTO 在 encode 时强制 sanitize**，不回显。`ica-desktop::diagnostics::redact` 改为复用同一脱敏器，日志与协议不再各有一套 marker。 | `unsafe_path_grants_are_rejected`、`database_paths_and_credentials_are_not_allowed_in_params`、`error_dto_redacts_credentials_in_message_and_details`、`ac3_executor_gets_no_database_and_no_undeclared_capability`（含 `worker_env()` 无 DB 变量断言） |
| AC4 Sidecar 限定外部能力执行器（ADR-005 / 评审裁决 3） | 协议方法白名单只含 browser 能力 + health/cancel；`BUSINESS_PREFIXES` 显式拒绝 draft/position/candidate/conversation/message/follow_up/import/share/auth/backup/task/audit 前缀，即使调用方伪造 grant 也拒绝。源码断言证明 worker、协议层与 host 均不含用例/事务/授权实现。招聘业务、状态机、授权全留 Rust。 | `business_methods_are_rejected_even_when_granted`、`ac4_sidecar_is_an_external_capability_executor_only`（对 worker/host/supervisor 三处逐串断言 + 方法集扫描） |
| AC5 Sidecar Contract Test + 独立验收 | `crates/ica-contracts/tests/sidecar_contract_test.rs`：golden wire shape、全能力方法向量、版本协商、7 类错误码路径、脱敏、畸形行拒绝。Rust Core 五个 crate 全量源码扫描无 `run_sidecar.py`/`playwright`/`pyo3`/`from app.`/`Python::` 耦合；host 无协议编解码。独立验收**待人工**，不自签。 | `ac5_rust_core_never_imports_python_implementation_details`、`independence_is_documented_not_self_signed` |

## 2. 改动文件清单

新增（5）：
- `crates/ica-contracts/src/sidecar_protocol.rs` — 版本化协议、DTO、编解码/校验/协商/脱敏（§7.3/§7.5）。
- `crates/ica-contracts/tests/sidecar_contract_test.rs` — Sidecar Contract Test（AC5）。
- `crates/ica-desktop/src/sidecar.rs` — 请求超时/取消/并发/崩溃决策 + 与 P3 的衔接 + legacy worker 协议桥（AC2）。
- `crates/ica-desktop/tests/arch_p4_sidecar_protocol.rs` — 源码断言/全量核心扫描/不自签说明（AC4/AC5）。
- `docs/architecture/sidecar-protocol-arch-p4.md` — 本文档。

修改（5）：
- `crates/ica-contracts/src/lib.rs` — 注册并 re-export 协议类型。
- `crates/ica-desktop/src/lib.rs` — 注册 `sidecar` 模块。
- `crates/ica-desktop/src/runtime_policy.rs` — 两个 host 注入的协议超时/并发配置 + 测试。
- `crates/ica-desktop/src/diagnostics.rs` — `redact()` 委托给共享脱敏器（单一实现）。
- `desktop/tauri-host/src/main.rs` — **未改动**（host 保持 P3 薄接线；协议逻辑全在下层）。

## 3. 与 P3 的衔接（不重复造轮子）

- 崩溃检测只产出 `CrashDecision`，真正重启调用 P3 `SidecarManager::restart()`（有界 `max_restarts`）。
- 优雅关闭直接取 P3 `shutdown_plan(policy)` 的六步，P3 manager 执行 drop-stdin → try_wait → kill → reap → log。
- 健康传输仍由 P3 `proxy::probe_health()` 负责；本协议新增的是 `health.check` 这种 executor 自答的 DTO。
- stdout/stderr 分离、握手兼容 baseline shape 全部沿用 P3，无第二份实现。

## 4. 真实验证输出

```text
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p4
$ git diff --check
（无输出，exit 0）

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo fmt --all -- --check
（无输出，exit 0）

$ rustfmt --edition 2021 --check desktop/tauri-host/src/main.rs
（无输出，exit 0；Host 语法可解析，不等于类型检查通过）

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace -- --skip the_full_poc_happy_path --skip the_worker_env_carries_no_database --skip a_worker_crash_is_contained
EXIT=0
49 个 test result 行：合计 517 passed; 0 failed; 0 ignored; 3 filtered out（DEF-006 POC 三条按指令 skip，已验证安全）
... test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # arch_p4_sidecar_protocol
... test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # sidecar_contract_test
... test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out  # browser_poc

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.78s   (exit 0)
```

`bash scripts/check.sh` 未作为门禁：它不带 DEF-006 的 `--skip`，会执行会挂起的 browser POC；
按任务单给出的显式命令验证。本轮未新增跨层依赖：ica-contracts 只加了 serde/serde_json 类型，
ica-desktop 的 sidecar 模块只依赖已存在的 ica-contracts / crate 内 P3 模块，未引入 axum/sqlx/tokio 新用法。

## 5. 兼容性

- 前端零改动：host 仍走 P3 的 `ica://` 同源双后端路由；本轮未改路由判定。
- 协议是加性能力：现有 worker 的 `seq`/`cmd` 线协议由 `to_legacy_command` / `from_legacy_reply` 桥接，
  不要求立即重写 Python sidecar；`protocol_version` 与 grant 留在 Rust 侧，不泄漏给 worker。
- 新环境变量都有默认值，不配置即 30s 请求超时 / 16 并发上限；P3 的环境变量语义不变。

## 6. 四项 gap（本机无法验证，如实记录）

1. **Tauri Host 类型检查 / Windows 实机**：本机缺 glib-2.0/WebKit，`desktop/tauri-host` 不能编译。
   已做 rustfmt 解析检查 + Host 源码接线断言 + 可测层测试；**不证明 Tauri 2 API 正确或 Windows 安装包行为**。
2. **真实 Chromium sidecar 协议与异常恢复**：DEF-006 POC 三条按编排方指令 skip（已验证安全）；
   协议编解码/协商/错误路径只由纯函数与 Contract Test 覆盖，没有真实 `run_sidecar.py` + Playwright/Chromium 的
   Windows 对拍。握手 shape 统一（P0 遗留项 4）仍需实机确认。
3. **Windows 优雅关闭与孤儿进程实机证明**：P3 的 `kill + wait` 回收与 `/proc/<pid>` 消失在 Linux 测过；
   Windows 无 SIGTERM，stdin-EOF graceful shutdown 与 `WindowEvent::Destroyed` 顺序未实机跑过。
4. **独立安全验证**：ADR-004「Host 无业务 / Policy Engine 边界」、多用户授权（当前仍是
   `RequestContext::local()` 合成身份、`correlation_id = "local-mode"`）、脱敏覆盖面、Sidecar 进程权限最小化
   均**待人工**独立复核。本交付不自签 Verified，不把这些当作已实现的安全边界。

## 7. 独立验收确认

**状态：PENDING_HUMAN。** 本轮由实现方自测并给出真实输出，不构成第四方独立验收；由未参与实现的人工复核方
按上述 AC 与 gap 逐条确认后再决定是否签署。本记录不自签 Verified。
