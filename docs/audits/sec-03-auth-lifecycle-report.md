# SEC-03 认证恢复与 Token 生命周期 — 安全验证记录

- 任务单：AUD-P1-03（SEC-03，P1，Stage 2）
- 依据：审计报告 §5.3 / RICA-SEC-001；P1 整改方案 §5（§5.2 状态机、§5.4 启动恢复、§5.6 Refresh 并发、§5.7 测试矩阵）
- Worktree：`/mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-03`，分支 `wt/ica-rust-aud-p1-03`，起点 `7213f42`
- 前置：P1-01（`ica-infra/jwt_key.rs` 持久化 CSPRNG 密钥）、P1-02（proxy header 白名单）均已在本分支基线上。
- 状态：**实现完成；独立安全验证待 verifier ≠ implementer 完成，不自签 Verified**。SEC-03 缺陷台账在独立复核通过前不得转 Verified。
- 凭据约定：任何真实/测试凭据值均不写入本文或日志，统一以 `[REDACTED]` 表示；本文只记录槽名、接口与状态名。

## 1. AC1 — 现场核实（只读，命令 + 行号）

起点 `7213f42` 的现场（命令均在 worktree 内执行）：

```
$ git show HEAD:frontend/src/auth/AuthContext.tsx | nl -ba | sed -n '1,12p;53,68p'
     1  /**
     2   * V2.0 阶段 B: authentication state.
     4   * On mount the provider probes `GET /api/auth/me`:
     6   *    synthetic local user) → the app renders.
     7   *  - 401 → multiuser and signed out → the login page renders.
     9   * The probe carries no refresh attempt (auth paths never refresh), so a
    10   * missing token surfaces as `unauthenticated`, never a refresh loop.
    53    useEffect(() => {
    54      let active = true;
    55      backend.auth
    56        .me()
    57        .then((current) => { ... setStatus('authenticated') })
    62        .catch(() => { ... setStatus('unauthenticated') });
```

结论：启动探测只有 200/401 两分支；401 直接 `unauthenticated`，**没有 refresh → retry**，三态 `checking/authenticated/unauthenticated` 散落为 `useState`。

```
$ git show HEAD:frontend/src/services/api.ts | nl -ba | sed -n '58,82p;110,122p'
    58  /** Single-flight refresh: concurrent 401s share one rotation. */
    59  let refreshInFlight: Promise<string | null> | null = null;
    61  async function refreshAccessToken(): Promise<string | null> {
    65      const response = await fetch(`${BASE_URL}/auth/refresh`, { ... });
    79  function isAuthPath(path: string): boolean {
    80      return path.startsWith('/auth/');
   110          refreshInFlight = null;
   113      const newToken = await refreshInFlight;
   114      if (newToken) { return request<T>(path, options, true); }
   120      notifySessionExpired();
   121      clearTokens();
```

结论：single-flight 已存在，但只服务业务请求；`/auth/*` 被 `isAuthPath` 排除，`/auth/me` 的 401 不进该链路；refresh 函数是模块私有，AuthContext 无法显式调用。

```
$ grep -n '"/api/auth/' crates/ica-api/src/routes.rs
101:    "/api/auth/register",   102:    "/api/auth/login",   103:    "/api/auth/refresh",
104:    "/api/auth/logout",     105:    "/api/auth/me",
228:        .route("/api/auth/register", post(register_endpoint))
229:        .route("/api/auth/login", post(login_endpoint))
230:        .route("/api/auth/refresh", post(refresh_endpoint))
231:        .route("/api/auth/logout", post(logout_endpoint))
232:        .route("/api/auth/me", get(me_endpoint))
$ grep -n 'revoke_refresh_token_if_active\|single-use' crates/ica-application/src/auth.rs
11://! * rotation is **single-use**: the presented token is revoked atomically
244:/// `POST /auth/refresh` — single-use rotation.
274:        .revoke_refresh_token_if_active(&token_hash, now)
```

结论：后端五条 auth 路由齐备；refresh 是带守卫的单次轮换，并发重复刷新只有一个赢家。

```
$ grep -n 'OsRng\|fn load_or_create\|fn rotate' crates/ica-infra/src/jwt_key.rs
118:    OsRng.fill_bytes(&mut secret);
262:    pub fn load_or_create(&self) -> Result<JwtKey, KeyError> {
276:    pub fn rotate(&self) -> Result<JwtKey, KeyError> {
$ grep -n 'generate_jwt_secret\|JwtKeyProvider\|load_or_create' desktop/tauri-host/src/main.rs
213:fn generate_jwt_secret() -> String {
214:    let provider = ica_desktop::jwt_key::JwtKeyProvider::new(resolve_data_root());
216:        .load_or_create()
270:        .env("JWT_SECRET", generate_jwt_secret())
```

结论：审计里「Host 重启生成新 Secret」的前提已被 P1-01 消除——重启复用持久化密钥；本卡修复的是其下游：前端在 401 时的集中恢复与受控失败。

## 2. AC2 — 认证状态机集中化

六态在两处、单一真源：Rust `ica-application::auth_recovery`（纯函数，无 I/O），`ica-contracts::auth_session` 再导出给适配器；webview 侧 `frontend/src/auth/auth-state.ts` 为镜像 reducer。`ica-contracts` 的源码断言 `sec03_c01` 在编译期 `include_str!` 前端文件并逐个核对六个名字，`sec03_c02` 扫描 `frontend/src` 确认除 `api.ts`/`endpoints.ts` 外没有任何模块直接调用 `/auth/refresh` —— 两份实现漂移即红灯。

| 状态 | 进入 | 离开 |
|---|---|---|
| Unauthenticated | 初始、SIGN_OUT、DISMISS_ERROR | RESTORE_START / LOGIN_START → Authenticating |
| Authenticating | 启动探测/登录中 | PROBE_SUCCEEDED → Authenticated；PROBE_UNAUTHORIZED → Refreshing；PROBE_FAILED → AuthenticationError |
| Authenticated | 探测/登录成功 | PROBE_UNAUTHORIZED → Refreshing；SESSION_EXPIRED → SessionExpired；SIGN_OUT → Unauthenticated |
| Refreshing | 一次 401 后 | REFRESH_SUCCEEDED → Authenticating（置 `retried`）；REFRESH_FAILED → SessionExpired；重复 401 被忽略 |
| SessionExpired | refresh 失败/二次 401/撤销 | LOGIN_START → Authenticating（可重登） |
| AuthenticationError | 非 401 探测失败/登录失败 | DISMISS_ERROR → Unauthenticated 后重试 |

`App.tsx` 仍消费三态 `status`，由 `legacyStatus(phase)` 推导，UI 无改动；新代码应读 `phase`。

## 3. AC3 — 启动恢复链路（改前/改后）

改前：`/auth/me` 401 → 直接 `unauthenticated`，用户被踢到登录页，即使 refresh token 仍有效。

改后（`frontend/src/auth/AuthContext.tsx` 集中处理）：

```
probe /auth/me
  ├─ 200 → Authenticated
  └─ 401 → 有 refresh token？
       ├─ 否 → clearTokens → SessionExpired（要求重新登录）
       └─ 是 → Refreshing → refreshSession()（api.ts single-flight）
              ├─ 成功 → REFRESH_SUCCEEDED → 再 probe 一次 → Authenticated
              │          └─ 仍 401（retried 已置位）→ SessionExpired(invalid_token)，不再刷新
              └─ 失败 → clearTokens → SessionExpired(reason)
```

关键代码行（改后实测）：

```
$ grep -n "refreshSession\|PROBE_UNAUTHORIZED\|REFRESH_SUCCEEDED\|backend.auth.me" frontend/src/auth/AuthContext.tsx
25:import { ApiError, refreshSession, type RefreshOutcome } from '../services/api';
91:    const probe = () => backend.auth.me();
109:      if (!getRefreshToken()) {
114:      dispatch({ type: 'PROBE_UNAUTHORIZED' });
115:      const outcome = await refreshSession();
122:      dispatch({ type: 'REFRESH_SUCCEEDED' });
132:          dispatch({ type: 'PROBE_UNAUTHORIZED' });
```

`refreshSession()` 从 `api.ts` 导出：同一个 in-flight Promise 供业务请求 401 与启动恢复共用；rotate 用裸 `fetch`，不经过 `request()`，杜绝 refresh 响应再次触发 refresh。P1-01 的持久化密钥保证「正常重启」时旧 token 依旧可验证；显式 `rotate()` 或凭据槽损坏导致的密钥变更仍按 SEC03-T11 安全失败（旧 token 不会被恢复成已登录）。

## 4. AC4 — Token 生命周期与并发测试

- Rust：`auth_recovery.rs` 12 条（SEC03-T01/T02/T03/T04/T05/T06/T07/T08/T09/T11 + 错误重试 + 六态名单）；`auth_session.rs` 4 条跨层源码断言。
- Webview：`auth-state.test.ts` 12 条纯 reducer；`AuthContext.test.tsx` 12 条 Provider/生命周期（含 restart、expiry、refresh 失败、二次 401、多次重启、密钥变更、主动退出）；`api.refresh.test.ts` 4 条真实 API client 的 single-flight / 500 / auth path / 无 refresh token。
- 并发 refresh：`api.refresh.test.ts` 用 barrier 卡住第一次 refresh，同时发两个 401 请求，断言 `/auth/refresh` **恰好 1 次**、两个请求均成功、pair 已轮换；状态层的 `retried` 守卫保证二次 401 不再刷新。

方案 §5.7 矩阵落点：

| 编号 | 场景 | 测试 |
|---|---|---|
| T01 | 正常登录 | auth-state、AuthContext |
| T02 | 未过期恢复 | auth-state |
| T03 | Access 过期自动刷新 | auth-state、AuthContext |
| T04 | 正常重启恢复 | AuthContext（unmount/remount 保留 localStorage） |
| T05/T06 | Refresh 过期/撤销 | auth-state、AuthContext、api.refresh |
| T07 | 并发刷新 | api.refresh（single-flight）、auth-state |
| T08 | Refresh 500 | api.refresh、auth-state |
| T09 | 主动退出 | auth-state、AuthContext |
| T10 | 多次重启一致 | AuthContext |
| T11 | 密钥不一致旧 token 不接受 | AuthContext（rotateKey）、auth-state |
| T12 | 数据库 Refresh 状态异常 | `ica-application::auth.rs` 既有单次轮换/撤销语义 + 会话层 refresh 失败路径 |

## 5. AC5 — 完整桌面生命周期与独立验证边界

`AuthContext.test.tsx` 用内存假后端 + localStorage 模拟桌面生命周期：登录 → 关闭（unmount）→ 重启（remount）→ 恢复 → Access 过期 → refresh → 二次 401/刷新失败 → 退出。密钥持久化由 P1-01 的 `JwtKeyProvider::load_or_create` 保证，宿主仍是薄接线（源码断言 `sec03_c03`）。

**本机缺 glib/WebKit，tauri-host 无法类型检查/实机运行**：宿主源码只做 `rustfmt --edition 2021 --emit stdout` 语法解析 + 源码断言；类型检查和真实桌面行为明确列为「待 Windows 侧 verifier 执行」。上面所有 jsdom 生命周期用例是**模拟**，不是 Tauri 实机 E2E，两者不可互相替代。

**独立安全验证待 verifier ≠ implementer 完成**：本记录是实现方自述 + 可复跑命令输出，不构成审计签署；实现方不自签 Verified，也不改缺陷台账状态。复核方需按审计报告 §5.3 验收清单在 Windows 桌面侧重跑全生命周期，并在与实现方隔离的环境下确认后再决定 SEC-03 是否转 Verified。

## 6. 改动文件清单（均在独立 worktree，未 add/commit/push）

| 文件 | 变更 |
|---|---|
| `crates/ica-application/src/auth_recovery.rs` | 新增：六态状态机纯策略 + 12 条单测 |
| `crates/ica-application/src/lib.rs` | 增加 `pub mod auth_recovery` 与再导出 |
| `crates/ica-contracts/src/auth_session.rs` | 新增：再导出 + 4 条跨层源码断言（前端名字、恢复集中化、宿主薄接线、生命周期矩阵） |
| `crates/ica-contracts/src/lib.rs` | 增加 `pub mod auth_session` 与再导出 |
| `frontend/src/auth/auth-state.ts` | 新增：六态 union + 纯 reducer + `legacyStatus` |
| `frontend/src/auth/auth-state.test.ts` | 新增：12 条 reducer 测试 |
| `frontend/src/auth/AuthContext.tsx` | 改为 reducer 驱动；新增 401→refresh→retry、SessionExpired、reason/phase 暴露；保留三态 `status` |
| `frontend/src/auth/AuthContext.test.tsx` | 重写为生命周期测试（12 条） |
| `frontend/src/services/api.ts` | refresh 导出为 `refreshSession()`，带 typed outcome；保留 single-flight 与 auth path 豁免 |
| `frontend/src/services/api.refresh.test.ts` | 新增：4 条真实客户端并发/失败测试 |
| `docs/audits/sec-03-auth-lifecycle-report.md` | 新增（本文） |

未改：任何 `Cargo.toml` / `Cargo.lock` / `package-lock.json`；后端 `/auth/*` 路由及 `ica-infra` 密钥实现；桌面 host 源码；三态 UI 分支。

## 7. 兼容性

- `AuthContext` 仍导出 `AuthStatus = 'checking' | 'authenticated' | 'unauthenticated'`，`App.tsx` 与 `LoginPage` 无需改动。
- localStorage 键（`ica.access_token` / `ica.refresh_token`）与 token pair wire 形状不变；刷新仍为单次轮换（后端守卫未改）。
- 本卡未给 `/auth/me` 增加 Bearer 校验：当前本地模式沿用「无 token 返回 synthetic local user」的既有契约，故真实 401 由已启用 token 校验的部署/密钥轮换路径驱动；jsdom 用例通过模拟 401 覆盖链路。是否需要把本地模式改成强制校验属于契约变更，需单独授权，不在本卡范围。
- Refresh token 仍存 localStorage（既有策略），XSS 风险沿用原设计并由桌面 WebView 边界承担；移入 Tauri 原生安全存储是后续独立决策，本卡不改存储格式。

## 8. 回滚

纯增量改动，按文件整体回退即可：`git checkout -- <上述已跟踪文件>` 并删除两个新增 Rust 文件、三个新增前端文件及本报告；无数据库迁移、无配置项、无密钥轮换，不需要数据回滚。回滚后启动恢复退回「401 直接登出」的旧行为（即审计发现的缺陷）。

## 9. 验证输出（实测）

（下列命令均在 worktree 内执行；本节记录本次实测的真实汇总。）

```
$ git diff --check
# 无输出，exit 0；11 个改动/新增文件经脚本复核：无 CRLF、无行尾空白、均以 LF 结尾。

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test -p ica-application -p ica-contracts
ica_application lib:        test result: ok. 19 passed; 0 failed
ica_application positions:  test result: ok.  7 passed; 0 failed
ica_contracts lib:          test result: ok. 58 passed; 0 failed
Doc-tests: 0 passed / 0 failed（两个 crate）
# 合计 84 passed / 0 failed，exit 0

$ rustfmt --edition 2021 --check crates/ica-application/src/auth_recovery.rs crates/ica-contracts/src/auth_session.rs
# 无输出，exit 0（两个新增 Rust 文件符合 rustfmt）

$ rustfmt --edition 2021 --emit stdout desktop/tauri-host/src/main.rs > /dev/null
# exit 0：宿主源码可解析；未改动的宿主有基线既存格式漂移，故只做语法解析 + 源码断言，不做格式等价断言

$ cd frontend && npx tsc -b --pretty false
# 无输出，exit 0

$ cd frontend && npx vitest run
 Test Files  24 passed (24)
      Tests  215 passed (215)
# exit 0
```
