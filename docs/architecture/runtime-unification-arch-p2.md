# ARCH-P2 — Unified Rust Application Runtime (交付记录)

**Worktree / branch:** `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p2` @ `wt/ica-rust-arch-p2` @ `3df2012`
**性质:** 实现 + 本地可编译 crate 的测试；未 git add / commit / push，未改基线仓。
**依据:** 任务卡 `ARCH-P2.md`，P0 盘点 `docs/architecture/architecture-dependency-map.md`，ADR-001/002/008，§4.2 阶段 B / §4.4 / §5.2 / §5.5 / §9.2。

## 1. 交付结论

在不新增 `ica-core` crate 的前提下（§4.2 阶段 C 四条件未成立），以阶段 B 的形态完成统一：
`ica-application::AppRuntime` 是唯一的 Application Services 装配类型；HTTP 的 `AppState` 与
IPC 的 `DesktopState` 都退化为它的薄包装（`Deref<Target = AppRuntime>`），各自仅做协议适配。
`AppConfig` / `DesktopConfig` 合并为同一个 `RuntimeConfig` 别名。

**安全验证状态：PENDING_HUMAN（独立安全验证待人工）。** 本记录不签署 Verified；Local Mode
仍是非多用户边界，详见 §4 缺口。

## 2. 五条 AC 的落地与证据

### AC1 统一 AppRuntime / AppState（阶段 B）
- 新增 `crates/ica-application/src/runtime.rs`：`RuntimeConfig` + `AppRuntime`（端口对象、进程内锁、
  注入式配置，全部来自宿主；`Send + Sync + Clone`，无 axum/sqlx/tokio/hyper）。
- `ica-api/src/state.rs` 与 `ica-desktop/src/state.rs` 均只保留 `runtime: AppRuntime` 字段、
  `new`/`connect`/`from_store` 装配、`runtime()` 与 `Deref`。原 HTTP 与 IPC 各自约 300 行的
  端口装配代码删除（`452 insertions / 682 deletions` 的净减来自这里）。
- 测试：`runtime::tests::both_adapters_wrap_the_same_application_runtime` 用源码断言证明两侧
  都调用 `AppRuntime::new` 且暴露 `runtime()`；`the_runtime_is_send_sync_and_cloneable` 类型级证明。
- 分层门禁：`check.layering_violations() == []`；新增模块只依赖既有内层依赖。

### AC2 Application Services + RequestContext + Application 层授权
- 新增 `crates/ica-application/src/request_context.rs`：`RequestContext`（actor / tenant / role /
  active / correlation_id）、`Permission`、`authorize()`。身份由传输层构建，授权在用例内执行。
- `auth.rs` 三个 share 用例改为接收 `&RequestContext`：先 `authorize(ctx, …)`，再用
  `ctx.actor_id` / `ctx.can_administer(&share.owner_id)`。HTTP 与 IPC 都不再硬编码
  `&local.id` 与 `is_admin = true`。
- 单元测试：`request_context::tests::*`（3 条：本地上下文、匿名/禁用/无租户被拒 401
  `unauthorized`、owner/admin 分离）。SEC-04 源码断言测试同步更新（`sec04_authorization_http.rs`）。

### AC3 Handler / Command 业务下沉（源码断言）
- `backup` 恢复的 confirm 守卫、文件名守卫、存在性 404 预检查从两个适配器移入
  `ica_application::backup::restore_backup_checked`；三道守卫顺序保持基线语义。
  **顺带修掉一个跨传输分叉**：IPC 原先缺少存在性预检查，未知备份名在 IPC 上是 500/其它错误，
  HTTP 是 404；现在两侧一致。
- HTTP `get_import` 原先在 handler 里直接 `state.imports().find(...)` 并手写 404，现改为调用
  已有用例 `ica_application::get_import`（IPC 一直如此）。
- 新增 `crates/ica-api/tests/adapter_contract_parity.rs::adapter_bodies_only_parse_validate_and_call_use_cases`：
  对 10 个 HTTP 端点与 10 个 IPC 命令做带字符串跳过的花括号函数体提取，断言体内没有
  `ContractError::` / `.find(` / `"was not found"` / `if ` / `is_admin`，且必须调用
  `ica_application::`。该测试通过。

### AC4 统一错误模型（§9.2）
- `ica-contracts::error` 新增 `code` 常量模块、`ErrorEnvelope {code,message,details}`、
  `ContractError::http_status()` 与 `ContractError::envelope()`；`ApplicationError` 增加
  `code()` / `http_status()`。
- HTTP 的 `ApiError::into_response` 与 IPC 的 `IpcError::from_contract` 均调用同一个
  `envelope()`；`IpcErrorBody` 直接复用 `ErrorEnvelope` 类型。500 的通用文案与不回显根因保持不变。
- Sidecar（browser worker）的失败回复新增 `code` 字段，由
  `ica_contracts::error::sidecar::code_for_error_message` 从同一码表取值；旧 `error` 文本字段保留（加性变更）。
- 测试：`application_and_contract_error_vocabularies_agree`（9 种错误逐条比对 code/status）、
  `the_envelope_has_exactly_the_shared_error_shape`、`the_sidecar_classifier_only_emits_shared_codes`、
  `the_sidecar_uses_the_same_error_code_vocabulary`、HTTP/IPC 错误信封逐值等价（AC5 测试）。

### AC5 Adapter Contract Test（§5.5）
- 新增 `crates/ica-api/tests/adapter_contract_parity.rs`（5 条）：同一 R0 种子库分别装到 axum
  router 与 `DesktopState`，对同一输入比较完整 JSON：health、positions list/get/create/patch/delete、
  4 类错误信封、backup restore 三道守卫、shares list/delete。生成型 id/时间戳只在 create 比较中
  显式剥离（两侧都必须存在），其余字段逐值比对。
- 授权在 Application 层执行：shares 两个适配器的 `RequestContext::local()` + 用例内 `authorize`；
  parity 测试覆盖 shares_list 与 shares_delete(missing)。
- 结果：`cargo test -p ica-api --test adapter_contract_parity` → **5 passed; 0 failed**。

## 3. 真实验证输出

```text
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p2
$ git diff --check
（无输出，exit 0）

$ cargo fmt --all -- --check
（无输出，exit 0）

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.79s    (exit 0)

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace -- --skip the_full_poc_happy_path --skip the_worker_env_carries_no_database --skip a_worker_crash_is_contained
（46 个 test result 行；合计 454 passed; 0 failed; 0 ignored；browser_poc 3 filtered out）
test adapter_bodies_only_parse_validate_and_call_use_cases ... ok
test error_envelopes_are_identical_across_transports ... ok
test shared_application_guards_render_identically_over_both_transports ... ok
test health_and_positions_reads_produce_identical_results ... ok
test a_write_and_its_generated_fields_are_equivalent ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH -u PYTHONHOME python3 -c "import sys; sys.path.insert(0,'scripts'); import check; print(check.layering_violations())"
layering_violations: []
```

`bash scripts/check.sh` 在本机无法跑到收尾：它没有 DEF-006 的 `--skip` 参数，会执行会挂起的
browser POC 用例（编排方已验证带 skip 的显式命令安全）。因此按任务单给出的显式命令验证，
并单独跑分层门禁函数；未把 check.sh 的挂起写成通过。

## 4. 未完成项 / 缺口（如实记录）

1. **IPC 命令覆盖面**：`ALLOWED_COMMANDS` 仍是 5 条 positions 命令（F1-2 切片），相对 68 条
   HTTP 契约只覆盖了 5 条；Adapter Contract Test 的可执行对拍只能覆盖 health/positions +
   两个共享用例守卫（backup restore、shares）。其余界面的“两侧一致”只有源码级断言（同一用例
   调用、无适配器业务规则），不能冒充实机对拍。建议后续单独立卡扩充 host 注册与 parity 用例。
2. **多用户授权未完成**：`RequestContext::local()` 目前是合成本地身份；业务路由没有 JWT 门禁，
   `correlation_id` 是 `"local-mode"` 常量（未接 per-request 生成）。SEC-04 的
   `local_mode_does_not_gate_business_routes_on_a_bearer_token` 仍通过并明确记录该边界。
   真实身份提取、租户隔离与端到端越权测试属独立安全验证，**待人工**，不自签 Verified。
3. **Tauri host 未类型检查**：本机缺 glib/WebKit，`desktop/tauri-host` 不编译；只依赖已有的
   `ipc_allowlist` 源码级一致性测试与本次对 worker 源码的断言。Windows 侧实机验证待人工。
4. **Sidecar 协议**：`code` 是加性字段，supervisor 仍读旧 `error` 文本；带真实 Chromium 的
   worker 行为未验证（DEF-006 POC 三条按编排方指令 skip）。
5. **未做**：未 git add/commit/push；未改基线仓 `d39e321`；未接前端；未新增 crate。
