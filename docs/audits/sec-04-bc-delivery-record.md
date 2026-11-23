---
title: "SEC-04-B/C 交付记录"
document_id: "DELIVERY-ICA-SEC-04-BC-R1"
document_type: "DeliveryRecord"
project: "ICA-RUST"
version: "1.0.0"
status: "DeliveredWithLimitations"
created: "2026-11-23"
updated: "2026-11-23"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "RICA-AUDIT-002-R1.2"
  - "DESKTOP-ACCEPT-ICA-R1"
supersedes: null
---

# SEC-04-B/C 交付记录（2026-11-23）

## 0. 边界

- Worktree：`.worktrees/sec-04-bc`；分支 `baseline/sec-04-bc`。
- 依据：R1.2 §14.3 第 3 项负责人裁决（建设完整多用户认证授权）。
- B 卡 `wi_72c04ffc112a406d` 5 条 AC、C 卡 `wi_d85d8b37858f4a22` 4 条 AC。
- 本记录是**实现交付**，不是独立安全验证；验证者必须与实现者分离。

## 1. 实现内容

### B 卡
- **AC1 真实用户/租户 + 密码 hash + JWT**：基线 schema 已有 `tenants`、`users`
  （bcrypt `password_hash`、`role`、`is_active`）、`refresh_tokens`（SHA-256 单轮换）
  表与 `AuthCrypto` 实现；本卡补齐此前缺失的 **access token 验签**
  （`crates/ica-api/src/auth.rs::verify_access_token`），使 JWT 真正成为身份凭据。
- **AC2 68 契约认证映射**：`PUBLIC_PATHS` 明确 2 条公开契约 + `/health`；
  其余 66 条在 MultiUser 模式要求合法 bearer。`docs/architecture/api-authentication-matrix.md`
  增补 B/C 实施映射；源码断言测试 pin 住白名单与路由表一致性。
- **AC3 资源归属与 scope**：`Scope::from_context` 从真实身份派生租户/owner；
  `PositionFilter.scope` 驱动 SQLite 强制 `tenant_id`（member 另加 `owner_id`）过滤；
  positions 的 list/get/create/update/delete 全部走 scoped 路径。
- **AC4 authorize 生效**：`Permission` 增加五类契约动作；`authorize` 不再忽略
  permission，按角色判定，缺失权限返回 403 `forbidden`（新增共享错误码常量）。
- **AC5 前端登录 / local 降级**：前端已是真实 HTTP 登录/刷新/登出链路；
  `ICA_AUTH_MODE=local`（默认）下后端 `/auth/me` 仍返回合成 Local 用户，
  前端无需伪造身份；`multiuser` 下返回 JWT 中的真实 actor/tenant/role。

### C 卡
- 新增 `crates/ica-api/tests/sec04_bc_authorization_http.rs`：匿名/伪造 bearer 401、
  跨租户读不可见（404）、写/删受身份与 owner 约束、`/auth/me` 真实身份、
  契约→授权点→用例三向追溯源码断言。
- 错误码与基线一致：401 `unauthorized`、403 `forbidden`；
  out-of-scope 行按 404 处理，不提供 IDOR 探针。

## 2. 运行方式

```bash
# Local（默认，单用户开发边界）
cargo run -p ica-server

# MultiUser（真实认证必需；需同时配置 SEC-01 的 JWT_SECRET）
ICA_AUTH_MODE=multiuser JWT_SECRET=<256-bit secret> cargo run -p ica-server
```

测试用 `router_with_mode(state, AuthMode::MultiUser)` 避免进程级环境变量竞争。

## 3. 验证结果

全部命令在 Windows 11 x64、分支 `baseline/sec-04-bc`、HEAD `f8b9284` 上实跑：

- `cargo test --workspace --locked -- --skip <DEF-006 三条 browser_poc>` →
  **543 passed / 0 failed**，退出码 0（3 条 skip 记为 Skipped，不计 Passed）。
- `cargo fmt --all -- --check` → 退出码 0。
- `cargo clippy --workspace --all-targets --locked -- -D warnings` → 退出码 0。
- `cargo test -p ica-api --test sec04_authorization_http --test sec04_bc_authorization_http`
  → `7 passed` + `5 passed`，退出码 0。

日志归档：合并前的执行输出保存在本机临时日志；关键断言与错误码由上述
两个测试文件源码 pin 住。独立安全验证仍未执行。

## 4. 已知边界 / 不外推

1. Repository scope 当前**仅覆盖 positions**；candidates、conversations、messages、
   drafts、browser_* 等实体的读取仍依赖 Local 默认归属，尚未逐条移植 Python
   `scope_*` 语义。不得据此宣称 68 契约全部完成资源隔离。
2. MultiUser 模式下 `NETWORK_AUTHENTICATION_READY` 仍为 `false`，
   `validate_bind_policy` 继续阻断非回环监听；本卡不开放远程部署。
3. `AdminOperation` 的角色门槛已在 Application 单测验证，尚无独立的管理员端点
   HTTP 负向用例（基线报告 T13 仍为 NOT EXECUTED）。
4. 未做独立安全验证；不得由实现方自签 Verified。
