# ARCH-P5 — 通信方式渐进迁移（交付记录）

**任务单：** `ARCH-P5.md`（WI `wi_78a205445fed46e4`）
**实现基线：** `173362c872d9aee661734df3093d52b6b7da1fa3`（短 `173362c`；worktree `wt/ica-rust-arch-p5` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p5`）
**依据：** RICA 架构设计 §5.3/§5.4/§5.5 + §14 Phase 5 + §19 评审裁决 2（双通道保留、严格统一 Application Core）；
前置 P0 盘点 `frontend-backend-client-inventory.md`、P2 `runtime-unification-arch-p2.md`、
P2 可执行对拍 `crates/ica-api/tests/adapter_contract_parity.rs`、
P4 `sidecar-protocol-arch-p4.md`。
**取数时点：** 2026-11-23（CST）。**性质：** 实现 + 可编译层真实测试。
**安全验证状态：PENDING_HUMAN（独立安全验证待人工）。** 本记录不签署 Verified。
本机缺 glib-2.0/WebKit，`desktop/tauri-host` 不做类型检查；实机 Windows 编译、Tauri 运行时、
多用户权限隔离验证均为未完成项（见 §5）。

---

## 1. 五条 AC 落地

| AC | 落地 | 证据 |
|---|---|---|
| **AC1 统一 Frontend Backend Client（§5.3）** | `frontend/src/services/backend/index.ts` 是 UI 唯一入口：导出 `backend`、`ApiError`、`describeApiError`、会话刷新，页面不再从 `services/api.ts`（HTTP 实现）取任何东西。三处越界 import（`AuthContext.tsx`、`CandidatesPage.tsx`、`PositionsPage.tsx`）已改为从 facade 导入。传输选择在 `config.ts` 启动时一次性解析；`createBackend()` 按 manifest 组装，页面组件不判断 `fetch`/`invoke`。 | `ui_components_do_not_depend_on_transport_implementations`（Rust，扫描 27 个 UI 源文件，禁串 `fetch(`/`invoke(`/`@tauri-apps/api`/`services/api`/`services/backend/{ipc,http,config,migration}` 全部零命中）；`npm run build` 通过 |
| **AC2 Positions 优先、其余按业务边界渐进（§5.4）** | 已迁移：`positions`（复用 P0/P2 已有的 5 条 IPC 命令与 `ipcPositions` 适配器，本次整理为 manifest 中的唯一 IPC 分支）。未迁移 12 个 domain 全部保持原 HTTP 调用；host `generate_handler!` 仍只注册 5 条 positions 命令，未扩大 IPC 面。清单见 §2。 | `host_registers_only_the_migrated_positions_commands`；前端 `moves_only_migrated_domains_and_keeps_every_other_domain_on_http`（逐 domain 断言对象同一性） |
| **AC3 HTTP/IPC 一致性（§5.5）** | **复用不重造**：后端等价性由 P2 `crates/ica-api/tests/adapter_contract_parity.rs` 5 条提供（相同输入的结果、404/422/409 错误码、backup restore 三道守卫、shares 授权、共享事务守卫），本次新增 Rust 测试只断言该套件仍完整并被交付记录引用；前端补 `transport.test.ts` 10 条 transport-agnostic 用例：同一 Client 接口走 HTTP（stub fetch）与 IPC（stub invoke）比较成功 DTO、`code/message/details`、渲染后的错误串，含 `not_found` / `validation_error` / `forbidden` / Page 与单实体。HTTP 状态按 P2 形态只作旁证不参与值比较。 | `p2_adapter_contract_parity_is_reused_not_rebuilt`；全量 cargo test 中 P2 套件 `5 passed`；前端 `transport.test.ts` `10 passed` |
| **AC4 行为一致 + 可回退** | 迁移前后 domain 方法签名不变（`BackendClient` 仍派生自 `HttpBackend`，IPC 适配器缺失方法即编译错误）；回退开关三级：`window.__ICA_BACKEND_CONFIG__.transport` > `localStorage['ica.backend.transport']` > `VITE_ICA_BACKEND_TRANSPORT` > 启动时宿主探测。设 `'http'` 即恢复原调用图，无需改页面、无需重编译（localStorage/global 两级）。兼容兜底仍按 Tauri 窗口走 IPC、浏览器走 HTTP。 | 前端 5 条 config/rollback 用例（优先级、localStorage 回退、非法值忽略、manifest 全覆盖）；`rollback_flag_and_migration_manifest_are_wired` |
| **AC5 UI 不依赖传输、保留 HTTP Server、独立验收** | 源码断言（Rust 测试，机器执行）证明 27 个 UI 源文件无 fetch/invoke/传输层 import；`api.ts` 与 `http.ts` 保留，HTTP 路由 health/positions/candidates/auth/settings/imports 与 sidecar `browser/v1` 路由均仍在。独立安全验证**待人工**，不自签 Verified。 | `ui_components_do_not_depend_on_transport_implementations`、`http_server_capability_is_still_registered`、`p2_...` 三条 + 交付记录 §5 gap 清单 |

## 2. 已迁移 / 未迁移模块清单

判断口径：`transportForDomain(domain, 'ipc') === 'ipc'` 且 host 已注册对应命令。未迁移 domain 的对象引用与 `httpBackend` 完全相同（前端测试逐条 `toBe` 断言）。

### 已迁移（1）
| domain | 传输 | 后端 | 依据 |
|---|---|---|---|
| `positions` | IPC（Tauri 窗口；HTTP 保留） | `ica-desktop` 进程内 `DesktopState`，5 条命令 `positions_list/get/create/patch/delete` | P2 已有 IPC 基础 + `adapter_contract_parity.rs` 覆盖结果/错误/授权/审计 |

### 未迁移（12，保持原 HTTP 调用）
| domain | 保持 HTTP 的理由 |
|---|---|
| `candidates` | HTTP 已满足；先统一 Use Case，待 host 注册命令后迁移 |
| `conversations` | 涉及状态与授权，按模块边界单独迁移 |
| `messages` | 与 conversations 同边界，待授权一致性确认 |
| `imports` | 写路径含事务与审计，沿用 HTTP 直到 IPC parity 覆盖 |
| `followUps` | 业务状态机正确性优先，统一后再迁移 |
| `drafts` | AI 编排经 Application Service，暂不暴露第二个入口 |
| `draftGroups` | 与 drafts 共用状态机，同批迁移 |
| `ai` | 模型配置写端点仍由 Python sidecar 提供 |
| `dashboard` | 只读聚合，无高频 IPC 收益 |
| `auth` | 会话刷新与 token 轮换是 HTTP 行为 |
| `settings` | AI 密钥写入 OS 凭据库，保留 HTTP Server 单一入口 |
| `browser` | Python sidecar 的协议化 HTTP 边界（ARCH-P4），不本地化 |

## 3. 改动文件清单

新增（5）：
- `frontend/src/services/backend/config.ts` — 启动配置解析与回退优先级（§5.3/AC4）。
- `frontend/src/services/backend/migration.ts` — 逐模块迁移 manifest 与 `transportForDomain`（§5.4）。
- `frontend/src/services/backend/transport.test.ts` — 前端 transport-agnostic 一致性与回退测试（10 条，AC3/AC4）。
- `crates/ica-api/tests/arch_p5_transport_migration.rs` — 源码边界断言（5 条，AC1/AC2/AC3/AC5）。
- `docs/architecture/frontend-backend-client-arch-p5.md` — 本文档。

修改（4）：
- `frontend/src/services/backend/index.ts` — `createBackend`/`activeTransport`/启动解析；facade 统一 re-export；去掉页面侧的 `isTauri` 分支。
- `frontend/src/auth/AuthContext.tsx` — `ApiError`/`refreshSession`/`backend` 改从 facade 导入。
- `frontend/src/pages/CandidatesPage.tsx` — `describeApiError` 改从 facade 导入。
- `frontend/src/pages/PositionsPage.tsx` — `describeApiError` 改从 facade 导入（events 订阅保持 transport-neutral 通道）。

未改：`frontend/package-lock.json`（hash 前后均为 `5aa045ca…7e733be`，`git status --short` 空）、`api.ts`/`http.ts`/`ipc.ts` 传输实现本体、`desktop/tauri-host/src/main.rs`、任何 Rust 业务 crate 源码。未 git add / commit / push。

## 4. 真实验证输出

```text
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p5
$ git status --short frontend/package-lock.json        # install 前后各一次
（两次均为空）
$ sha256sum frontend/package-lock.json
5aa045ca29498a81b892dca115e8de4dc8c4dc0a8adfeb5296fc6b30f7e733be  frontend/package-lock.json

$ cd frontend && npm install --no-save --include=optional
added 168 packages, and audited 169 packages in 29s
found 0 vulnerabilities

$ git diff --check
（无输出，exit 0）

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo fmt --all -- --check
（无输出，exit 0）

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.56s   (exit 0)

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace -- \
    --skip the_full_poc_happy_path --skip the_worker_env_carries_no_database --skip a_worker_crash_is_contained
TEST_EXIT=0
50 个 test result 行：522 passed; 0 failed; 0 ignored; 3 filtered out
     Running tests/adapter_contract_parity.rs ...
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/arch_p5_transport_migration.rs ...
test p2_adapter_contract_parity_is_reused_not_rebuilt ... ok
test host_registers_only_the_migrated_positions_commands ... ok
test rollback_flag_and_migration_manifest_are_wired ... ok
test http_server_capability_is_still_registered ... ok
test ui_components_do_not_depend_on_transport_implementations ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/browser_poc.rs ...
running 0 tests        # DEF-006 三条按指令 skip

$ cd frontend && npm run build
✓ 62 modules transformed.
dist/assets/index-D02J2ulQ.js   395.20 kB │ gzip: 115.35 kB
✓ built in 984ms                 (exit 0)

$ npm test -- --reporter=dot
Test Files  25 passed (25)
     Tests  225 passed (225)     (基线 24 files / 215 tests → +1 file / +10 tests)
```

`npm run lint`（oxlint）也跑过：exit 0，仅 3 条既有 warning（tokenStore / events / check-ui-reuse），
无本次新增文件的告警。`scripts/check.sh` 未作为门禁：它不带 DEF-006 的 `--skip`，会执行会挂起的
browser POC；按任务单给出的显式命令验证。

## 5. 未完成项 / 缺口（如实记录，不签署 Verified）

1. **独立安全验证待人工**：多用户权限隔离、跨用户越权、真实 JWT/凭据链路不在本机验证范围；
   P2 已记录 `RequestContext::local()` 为合成本地身份、`correlation_id = "local-mode"`，
   本轮未改变该边界，不得据本记录宣称「权限隔离已实现」。
2. **tauri-host 未类型检查**：本机缺 glib-2.0/WebKit，Rust workspace 测试不含该独立 workspace；
   host 的 `generate_handler!` 只做了源码级解析断言，Windows 侧编译、真实 webview 中
   `invoke` 往返与启动链仍待人工验收。
3. **IPC 覆盖面**：host 仅注册 5 条 positions 命令（§2），其余 12 个 domain 的可执行 HTTP/IPC
   对拍没有 IPC 侧可调用目标；前端 transport-agnostic 用例证明 adapter 等价形态，
   不能冒充后端共享 Application Core 的实机一致性（后者仍以 P2 套件为限）。
4. **事务/审计一致性**：沿用 P2 的共享 use case 与 parity 断言，本轮未新增前端可见的写路径；
   `browser` 族仍走 Python sidecar HTTP（ARCH-P4 协议），不纳入 IPC 对拍。
5. **回滚开关的实机验证**：localStorage/host 注入两级开关在 vitest jsdom 中验证；
   真实 Tauri 窗口注入与线上配置下发方式待 Windows 侧确认。
6. **worktree 状态**：按要求未 git add/commit/push；合并后需刷新本文档头部 provenance
   与改动文件状态（参见 ICA-RUST workbook 的 worktree 交付约定）。
