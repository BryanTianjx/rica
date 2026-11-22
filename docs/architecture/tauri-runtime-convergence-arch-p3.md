# ARCH-P3 — Tauri Runtime 职责收敛（交付记录）

**实现基线：** `e70dc8a`（worktree `wt/ica-rust-arch-p3`）→ **已合并 main @ `5ebcc6d`** 并 push origin/main；worktree 已按约定删除。
**实现提交：** `711e071`；合并提交 `5ebcc6d`（--no-ff）；本 provenance 于合并后刷新。
**取数时点：** 2026-11-23（CST）
**依据：** 任务卡 `ARCH-P3.md`、`docs/audits/RICA 总体架构演进设计与 ADR 决策 R1.0 2026-11-22.md` §6.4/§7.4/§11/§14/§19 评审裁决 1、`docs/architecture/runtime-unification-arch-p2.md`（P2 `AppRuntime` 入口）、`docs/architecture/sidecar-lifecycle-inventory.md`（P0 四段定位）、`docs/audits/RICA 全量工程实现审计报告 R1.0 2026-11-22.md` §5.5 REL-01。

**安全验证状态：PENDING_HUMAN（独立安全验证待人工）。** 本记录不签署 Verified；Local Mode 仍是非多用户边界。Tauri Host 在本机因缺 glib/WebKit 不能类型检查，相关结论只到「rustfmt 可解析 + 源码接线断言 + 可测逻辑在 ica-desktop 全绿」， Windows 侧编译与实机行为、真实 Chromium sidecar、独立安全复核见 §7 的四项 gap。

---

## 1. 改前 / 改后

| 维度 | 改前（`e70dc8a`） | 改后（main @ `5ebcc6d`） |
| --- | --- | --- |
| 子进程四段（启动/握手/通信/关闭） | 全在 `desktop/tauri-host/src/main.rs`：两个 `struct { child: Child }` + 两个 `static Mutex<Option<…>>`、`spawn_rust_server()`、`spawn_sidecar()`、`forward_to()`、`poll_health()`、`parse_listening()`、`stop_backends()`（P0 盘点逐行定位） | 四段集中到 `ica-desktop`：`processes.rs`（Sidecar Manager：spawn/握手/健康/stdout+stderr 分文件/关闭/重启）、`proxy.rs`（转发与 `/health` 探测）、`routing.rs`（纯路由判定）；Host 只剩 `ProcessSpec` 构造 + `manager().start(spec)` + `shutdown_all()` 的薄接线 |
| Rust HTTP Server | 启动即常驻 `ica-server` 子进程；`/health` 超时 FATAL | **默认不启动**（评审裁决 1，Embedded Mode）。`RuntimePolicy.http_mode` 默认 `embedded`，首次 Rust 路由 `/api` 才按需启动；`always` 保留 §6.4 场景常驻，`never` 为纯 IPC |
| AppRuntime 生命周期 | Host 直接 `IcaState::connect` 装配但没有显式 init/drain/destroy | P2 `AppRuntime` 仍是唯一装配；`RuntimeLifecycle`（Uninitialized→Ready→Draining→Stopped）在 ica-application，Host 在装配后记录 ready、关闭时先 `begin_drain()`、子进程回收后 `mark_stopped()` |
| 健康检查 / 日志（§11） | `poll_health()` 只在启动时轮询；`log_line()` 只有 `[Nms] 文本`，无统一字段 | `diagnostics.rs` 提供 `HealthKind`(liveness/readiness/dependency) × `HealthState`、`RuntimeHealth`（`is_ready()` 只被 readiness 门控，可选 Provider 不降级整体）、`DiagnosticEvent`（request_id/principal/source/module/operation/duration_ms/result/error_code），`redact()` 统一脱敏；Manager 输出所管进程的真实健康项，SQLite/Provider/Backup/Auth 标记 `NotApplicable` 而不谎报 |
| 优雅关闭（REL-01） | `stop_backends()` 对 Rust server 直接 `child.kill()`，无排空、无超时、无日志 | 固定顺序 `close_stdin → stop_accepting_new_requests → wait_drain → kill_if_timeout → reap → log_result`：drop stdin 触发 EOF → `try_wait()` 轮询至 `graceful_shutdown_timeout_ms` → 超时 `kill()` + `wait()` **必回收** → 逐条结构化日志。`ica-server` 新增 stdin-EOF 作为 `with_graceful_shutdown` 的第三信号（Windows 无 SIGTERM 监听时的可用通道） |
| 重启恢复 | 无；异常退出不检测、不重启 | `LifecycleState` 状态机 + `SidecarManager::restart()` 有界重启（`max_restarts`，默认 2）；握手同时接受测试 shape 与基线 shape `{"event":"listening",…,"port":N}` |
| Host 业务职责 | 已有 thin 包装（P2 之后），但仍有大量进程管理代码 | 源码断言证明 Host 无 `Command::new` / `.kill()` / `BufReader` / `recv_timeout` / `child.stdin` / `spawn_*` / `poll_health` / `forward_to`，也无 `ContractError::` / `ica_application::` / `is_admin` / 业务 `.find(` |

## 2. 文件清单（已提交 `711e071`，合并至 main `5ebcc6d`）

新增（6）：

- `crates/ica-desktop/src/processes.rs` — Sidecar Manager、生命周期状态机、`ProcessSpec`、`parse_listening`、优雅关闭/强制 kill/回收、有界重启、`runtime_health()`。
- `crates/ica-desktop/src/proxy.rs` — 共享 allowlist 的阻塞转发与 `/health` 探测（从 Host 下沉）。
- `crates/ica-desktop/src/routing.rs` — `Backend` + `route_backend()` 纯函数（从 Host 下沉）。
- `crates/ica-desktop/src/runtime_policy.rs` — `HttpServerMode` + `RuntimePolicy`（env/`from_pairs`）。
- `crates/ica-desktop/src/diagnostics.rs` — §11 健康模型、诊断事件、脱敏。
- `crates/ica-desktop/tests/arch_p3_runtime_convergence.rs` — 8 条源码/行为断言（Host 无子进程管理、无业务、Manager 四段存在、Embedded 默认、init/destroy 顺序、关机顺序、健康快照、不自签说明）。

修改（6）：

- `desktop/tauri-host/src/main.rs` — 删除散落的 spawn/握手/转发/健康/关闭代码，改为写 `ProcessSpec` + 调 Manager；policy/manager/host_log/boot/state 用 `OnceLock`；按需启动 `ensure_rust_backend()` 接入 `ica://` 的 Rust 分支；`stop_backends()` 改为 drain→shutdown_all→stopped 并记录结构化诊断；握手测试改为共享解析器。
- `crates/ica-application/src/runtime.rs` — 新增 `RuntimePhase` / `RuntimeLifecycle`，`AppRuntime` 持有共享 lifecycle，`new()` 末尾 `mark_ready()`；新增状态机测试。
- `crates/ica-application/src/lib.rs` — re-export `RuntimeLifecycle` / `RuntimePhase`。
- `crates/ica-desktop/src/lib.rs` — 注册 5 个新模块。
- `crates/ica-server/src/main.rs` — `shutdown_signal()` 增加 stdin-EOF 分支（与 Ctrl-C / SIGTERM 并列），使 Host 关闭 stdin 即触发 axum graceful shutdown。
- `crates/ica-contracts/src/proxy_headers.rs` — F1-4 漂移守卫随分层调整：改为断言 Host 调 `ica_desktop::proxy::forward` 且 ica-desktop 内仍调用共享 allowlist 三个函数。
- 本文档（新增）。

## 3. 五条 AC 落地

- **AC1 抽离 Sidecar Manager**：四段全部下沉；Host 13 个「禁止出现」源码串逐条断言为 0（见测试 `host_owns_no_child_process_lifecycle`）；Manager 侧 8 个「必须存在」串逐条断言命中（四段 + 有界重启）。启动/握手/健康/关闭/重启的纯逻辑与真实子进程行为在 ica-desktop 测试。
- **AC2 Desktop Embedded Mode**：`RuntimePolicy::default().http_mode == Embedded`，`starts_at_boot() == false`、`starts_on_demand() == true`；Host 启动路径以 `policy.starts_http_at_boot()` 门控，默认日志明确 `local HTTP server NOT started at boot`；`ica://` 的 Rust 分支调用 `ensure_rust_backend()` 才首次启动；`always`/`never` 为显式 opt-in；仍能采纳 `ICA_RUST_SERVER_PORT` 外部进程。
- **AC3 统一健康检查与日志 + AppRuntime 初始化/销毁**：§11 的 liveness/readiness/dependency 与 DiagnosticEvent 八字段有单一实现与脱敏测试；Host 记录 Manager 的真实健康行，Manager 对未探测项如实标 `NotApplicable`；`AppRuntime::new` → phase Ready，退出时 `begin_drain()` → `shutdown_all()` → `mark_stopped()`，源码断言三者顺序且状态在 clone 间共享。与 P2 的 `runtime()` 装配保持兼容（未改 `AppState`/`DesktopState` 的薄包装契约）。
- **AC4 优雅关闭 + 超时兜底（REL-01）**：`shutdown_plan()` 固定六步；Manager 的 `drain()` drop stdin → `try_wait()` 轮询到超时 → `kill()` + `wait()` 回收；Linux 真子进程测试证明「顽固子进程被 kill 后在 `/proc/<pid>` 不存在」（无孤儿），以及「响应 stdin EOF 的子进程不触发 kill」。每次关闭逐条结构化日志，超时强制 kill 单独记一行。
- **AC5 重启/异常可恢复、Host 无业务**：`LifecycleState` 状态机与 `restart()` 有界重启已测；握手兼容基线 shape（P0 遗留项 4）；Host 无业务源码断言与既有 `ALLOWED_COMMANDS`/`adapter_contract_parity` 测试同时通过；**独立安全验证待人工（§7 gap 4）**，本交付不自签 Verified。

## 4. 测试与真实验证输出

```text
$ cd /mnt/o/ICA-RUST
$ git diff --check
（无输出，exit 0）

$ cargo fmt --all -- --check
（无输出，exit 0）

$ rustfmt --edition 2021 --check desktop/tauri-host/src/main.rs
（无输出，exit 0；Host 已格式化，语法可解析。注：基线原文件本就有 4 处 rustfmt 差异）

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace -- --skip the_full_poc_happy_path --skip the_worker_env_carries_no_database --skip a_worker_crash_is_contained
47 个 test result 行：合计 485 passed; 0 failed; 0 ignored; 3 filtered out（DEF-006 POC 三条按指令 skip，已验证安全）
... test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # arch_p3_runtime_convergence
... test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out  # browser_poc
注：一次运行在建 `commands_smoke` 时遇瞬时 `Cannot allocate memory (os error 12)`（include_str 大 SQL + 并发编译，非代码失败）；原命令原样重跑即 exit 0，上面的数字来自重跑日志。

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.51s   (exit 0)
```

`bash scripts/check.sh` 未作为门禁：它不带 DEF-006 的 `--skip`，会执行会挂起的 browser POC；按任务单给出的显式命令验证并单独取 clippy/fmt 结果，未把 check.sh 的挂起写成通过。分层门禁相关事实：本轮未新增跨层依赖，ica-application 仍只用 std 锁实现 lifecycle。

## 5. 兼容性

- **前端零改动**：仍走 `ica://` 同源；P1 双后端路由判定逐条不变（ica-desktop `routing.rs` 的测试复制了 Host 原有 browser/AI-EXT/68 路由表）。
- **行为变化（有意为之）**：默认启动不再常驻 `ica-server`；第一个 Rust 路由 `/api` 会触发一次按需启动（握手 + `/health`），之后常驻到退出。`ICA_HTTP_SERVER_MODE=always` 恢复旧行为，`=never` 时 Rust 路由 `/api` 返回 502（消息含 backend name 与原因）。
- **环境变量**：新增 `ICA_HTTP_SERVER_MODE`、`ICA_HANDSHAKE_TIMEOUT_MS`、`ICA_HEALTH_TIMEOUT_MS`、`ICA_GRACEFUL_SHUTDOWN_TIMEOUT_MS`、`ICA_MAX_RESTARTS`、`ICA_DIAGNOSTICS`；全部有默认值，不配置即 Embedded。既有 `ICA_UNIT_DIR`/`ICA_DATA_DIR`/`ICA_SIDECAR_PORT`/`ICA_RUST_SERVER_PORT`/`ICA_RUST_SERVER_EXE` 语义保留。
- **外部采纳进程**：`ICA_SIDECAR_PORT`/`ICA_RUST_SERVER_PORT` 采纳时不持有 child，关闭阶段不会误杀（Manager 只回收自己 spawn 的进程）。
- **安全**：SEC-02 请求/响应 allowlist 未改，只是调用点从 Host 移到 `ica-desktop::proxy`；SEC-01 JWT key 仍走 `JwtKeyProvider`，且同一把 key 现在同时注入 AppRuntime 配置与可选 HTTP server，避免 IPC/HTTP token 语义分叉；日志统一经 `redact()`，凭据不落 host.log。
- **已知边界**：`ica-server` 的 stdin-EOF 关闭在独立终端运行时，若 stdin 被关闭会触发优雅退出（这是 Host 关闭所依赖的机制）；真机行为待 Windows 验证。

## 6. 回滚

1. 回滚本交付：在 main 上 `git revert -m 1 5ebcc6d`（--no-ff merge）；实现基线 `e70dc8a` 不受影响。
2. 若要保留部分能力但恢复旧行为：设 `ICA_HTTP_SERVER_MODE=always` 恢复启动即常驻；把 `RuntimePolicy.graceful_shutdown_timeout_ms` 调小可更快进入 force-kill 分支；`=never` 可完全禁用本地 HTTP。
3. 回滚 `ica-server` 的 stdin-EOF 分支时，必须同时把 Manager 的关闭策略改回 drop-stdin 之后立即 kill，否则 Python sidecar 的 EOF 优雅退出仍有效而 Rust server 只能靠超时兜底。
4. 本记录与架构文档同目录，回滚代码时一并删除/标注，避免下一轮把未验证结论当基线。

## 7. 四项 gap（无法在本机验证，如实记录）

1. **Tauri Host 类型检查 / Windows 实机**：本机缺 glib-2.0/WebKit，`desktop/tauri-host` 不能编译。已做 rustfmt 解析检查 + Host 源码接线断言 + `ipc_allowlist`/SEC-01/SEC-02 既有断言；它们只证明语法与接线一致，**不证明 Tauri 2 API 使用正确或 Windows 安装包行为**。待 Windows 侧 `cargo build`/实机验证。
2. **真实 Chromium sidecar 协议与异常恢复**：DEF-006 POC 三条按编排方指令 skip（已验证安全）；握手 shape、健康探测、崩溃重启只由纯函数测试 + `sh`/`cat` 真子进程测试覆盖，没有真实 `run_sidecar.py` + Playwright/Chromium 的 Windows 对拍。待 Phase 4 Sidecar Contract Test。
3. **Windows 优雅关闭与孤儿进程实机证明**：`kill + wait` 回收与 `/proc/<pid>` 消失在 Linux 测过；Windows 无 SIGTERM 监听，stdin-EOF  graceful shutdown 路径未在 Windows 实机跑过，`WindowEvent::Destroyed` 触发顺序也未实机测。待 QA-03 桌面交付验收。
4. **独立安全验证**：ADR-004 的「Host 无业务/Policy Engine 边界」、多用户授权（当前仍是 `RequestContext::local()` 合成身份、`correlation_id = "local-mode"`）、日志脱敏覆盖面的独立复核均**待人工**。本交付不自签 Verified，不把这些当作已实现的安全边界。

## 8. 明确未做

未改基线仓；未接前端；未新增 crate；未打 tag；未运行 `scripts/check.sh`（原因见 §4）。提交 `711e071` / 合并 `5ebcc6d` / 已 push origin/main。
