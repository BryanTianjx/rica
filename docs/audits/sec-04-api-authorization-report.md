# SEC-04 业务 API 授权审计报告（AUD-P1-04）

| 项 | 值 |
| --- | --- |
| Finding | SEC-04（P1）— 多用户业务 API 授权边界未证明 |
| 任务单 | `/home/bryan/.hermes/cache/scratch/ica-rust-cards/AUD-P1-04.md` |
| Worktree | `/mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-04`，分支 `wt/ica-rust-aud-p1-04` @ `7213f42` |
| 契约基线 | `d39e321`（68 条业务契约），未改动 `/mnt/o/ica` |
| 依据 | 审计报告 §5.4 / §6.1；整改实施方案 §6.2–§6.9 |
| 状态 | **证据已交付，待独立安全验证（人工）**；本报告不自签 `Verified` |

## 0. 结论摘要

- 已建立 **68 条业务契约的逐条认证矩阵**（`docs/architecture/api-authentication-matrix.md`），行的数量与 `endpoint_reconciliation` 的 `contract_total=68` 自洽。
- 已建立资源授权矩阵与 IDOR/越权测试矩阵（§3、§4），并新增 5 条可执行测试，实测 **5/5 通过**；连同全量回归 `cargo test -p ica-api -p ica-application` 共 **224 passed / 0 failed**（25 个测试目标）。
- **当前 Local 单用户模式下没有可证实的 P0**：服务默认仅绑定 `127.0.0.1`，只有 synthetic 用户 `local_user`，不存在跨用户资源面。
- **但必须如实记录**：业务路由**没有任何认证中间件**；认证矩阵中标注为「需认证 / 需管理员 / 需资源归属」的目标要求，在当前实现中**均未落实**。这不是被隐藏的问题——§4 的 T01 用可执行测试明确钉住了「伪造 Bearer token 与无 token 的响应完全相同」。
- **升级 P0 的触发条件（不得降级/掩盖）**：一旦 `HOST` 被设置为非回环地址，或启用多用户/远程访问，T01、T04–T08、T13 即构成「未经认证即可访问敏感数据 / 跨用户读写」，按审计报告 §6.1 必须**升级 P0 并暂停该模式发布**。在此之前不得宣称「权限隔离已实现」。
- 本轮**没有把 Local 单用户模式改造成多租户**，没有修改任何产品行为代码，只增加矩阵文档与测试。

## 1. AC1 — 现场核实（命令 + 输出 + 行号）

### 1.1 认证/授权设施检索

```
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-04
$ grep -rn 'fn authorize\|fn require_access\|AccessScope\|require_auth\|auth_middleware\|Claims\|verify_access_token\|decode_token\|authorize' crates/ --include=*.rs
（无 authorize / AccessScope / require_auth 命中；仅 auth.rs 中 401 构造与 proxy_headers.rs 的 401 测试样例）
```
结论：**没有统一的授权抽象**（无 `AuthContext`、无 `authorize()`、无资源归属校验器）。

```
$ grep -rn 'middleware::from_fn\|layer(' crates/ica-api/src/routes.rs
crates/ica-api/src/routes.rs:243:        .layer(middleware::from_fn(move |request, next| {
crates/ica-api/src/routes.rs:246:        .layer(middleware::from_fn(move |request, next| {
```
结论：仅有的两个 layer 是 `reject_head` 与 `redirect_trailing_slash`（HTTP 奇偶中间件），**不是认证门禁**。`routes.rs:226-227` 的原文亦明确：
`// W6 auth + shares (local mode is the default: no middleware gates the other routes, me answers the synthetic local user).`

### 1.2 Local 身份常量

```
$ grep -rn 'LOCAL_ACTOR\|LOCAL_OWNER_ID\|DEFAULT_TENANT_ID' crates/ica-infra/src/store.rs
24:pub const LOCAL_OWNER_ID:   = "0000…0001"
26:pub const DEFAULT_TENANT_ID: = "0000…0000"
28:pub const LOCAL_ACTOR:       = "local_user"

$ grep -rn 'LOCAL_USER_ID\|LOCAL_TENANT_ID\|pub fn local_user' crates/ica-application/src/auth.rs
21:pub const LOCAL_USER_ID:    = "0000…0001"
23:pub const LOCAL_TENANT_ID:  = "0000…0000"
119:pub fn local_user() -> UserData        // role = "admin", is_active = true
```
（UUID 值按任务约束以省略号表示，凭据相关值 `[REDACTED]`。）

### 1.3 仓储层读范围

```
$ sed -n '17,24p' crates/ica-infra/src/repository.rs
There is deliberately no tenant/owner filtering on any read: this adapter
mirrors the Python baseline's local mode, where
`app/application/access.py::scope_position_query` adds no filter when
`current_user.is_local` — so "no scoping here" is behaviour parity, not an
omission. That equivalence ends at cloud mode: when auth arrives (W6), every
read path must be scoped by tenant/owner/share like the Python `scope_*`
helpers, or it is a cross-tenant read. Treat that as a hard requirement for
the W6 card, not a refactor.
```
结论：源码作者已明确把「读无 owner/tenant 过滤」限定为 **local mode 的行为等价**，并要求云模式下必须补范围约束。

### 1.4 监听边界

```
$ grep -n 'env_or("HOST"' crates/ica-server/src/main.rs
53:    let host = env_or("HOST", "127.0.0.1");
```
默认仅本机可访问；这是本轮不升级 P0 的关键事实，同时也是不能被误读为「已实现网络安全边界」的事实。

### 1.5 Application 层现存校验

```
$ grep -n 'recipient_id == actor_id\|owner_id != actor_id\|is_admin\|pub fn local_user' crates/ica-application/src/auth.rs
119:pub fn local_user() -> UserData
353:    if recipient_id == actor_id {
354:        return Err(validation("Cannot share with yourself"));
384:    is_admin: bool,
395:    if share.owner_id != actor_id && !is_admin {
396:        return Err(unauthorized("Only the owner can revoke this share"));
```
跨资源存在性校验（非归属校验）见 `candidates.rs:54/115/127`、`conversations.rs:115`、`follow_ups.rs:95/212` 的 `not_found`。

## 2. AC2 — API 认证矩阵

产物：`docs/architecture/api-authentication-matrix.md`（**123 行**，逐条数据行 **68**）。

| 目标认证分类 | 条数 |
| --- | ---: |
| 可匿名 | 2 |
| 需认证 | 52 |
| 需管理员 | 11 |
| 需认证 + 需管理员 | 1 |
| 需认证（refresh 凭据） | 2 |
| **合计** | **68** |

自洽校验（真实输出）：

```
$ python3 scripts/endpoint_reconciliation.py --json
counts: contract_total=68 implemented=68 missing=0 extra_routes=0
        auxiliary_routes=1 non_contract_routes=1 matrix_rows=69

$ awk '/^\| [0-9]+ \|/{n++} END{print n}' docs/architecture/api-authentication-matrix.md
68
```

口径：`目标` 列是实施方案 §6.3 的多用户目标分类，**不是当前实现状态**；`当前 Local 现状` 列才是源码实测。例如 68 条中 66 条目标为「需认证/需管理员」，但当前 Local 模式下全部匿名可达。`GET /health` 不属于 68 条契约，单列为可匿名、无业务资源；`confirm-candidate` 为 D-2 裁决下的契约外辅助路由。

## 3. AC3 — 资源授权矩阵与 Application 层校验

### 3.1 资源归属矩阵（按实体）

| 实体 | 归属列（schema） | 写路径 | 读路径 | 归属校验位置 | Local 边界 | 多用户目标 |
| --- | --- | --- | --- | --- | --- | --- |
| positions | `owner_id`, `tenant_id` | 盖 local 常量（`repository.rs:233-234`） | 无过滤（`repository.rs:46-50,167`） | 无 | 单 synthetic 用户 | Use Case + Repository 范围约束 |
| candidates | `owner_id`, `tenant_id` | 盖 local 常量（`candidates.rs:233-234`） | 无过滤，仅有存在性 404 | 无 | 同上 | 同 position + 关联 position 归属 |
| conversations / messages | `owner_id`, `tenant_id` | 盖 local 常量（`conversations.rs:239-240,383-384`） | 无过滤 | 无 | 同上 | candidate/conversation 归属 |
| follow-ups | `owner_id`, `tenant_id` | 盖 local 常量（`follow_ups.rs:322-323,780-781`） | 无过滤 | 无 | 同上 | task + 关联 candidate/position 归属 |
| drafts / draft-groups | `owner_id`, `tenant_id` | 盖 local 常量 | 无过滤 | 无 | 同上 | draft + 关联 candidate/task 归属 |
| imports / OCR | `owner_id`, `tenant_id`, `raw_text` | 盖 local 常量（`imports.rs:185`） | 无过滤 | OCR 路径守卫（先判目录、后判存在） | 同上 | session 归属，不泄漏越界路径存在性 |
| send_records | `owner_id`, `tenant_id` | 盖 local 常量 | 无过滤 | 无 | 同上 | record + candidate 归属 |
| shares | `owner_id`, `recipient_id` | actor 来自 `local_user()` | 按 `local_user().id` 过滤 | **Application**：self-share 拒绝（`auth.rs:353-355`）、owner/admin 删除（`auth.rs:395-397`） | 唯一已有归属校验，但 synthetic 用户恒为 admin，**不构成角色分离证据** | 可信 actor + 资源归属双重校验 |
| app_settings / audit / backups | `tenant_id`, `owner_id`（部分表） | — | 无过滤 | 无管理权限门槛 | 匿名可达 | 管理员角色 |

**明确边界**：SQLite 表里的 `owner_id` / `tenant_id` 列和索引（基线 schema 121 个索引中包含各表的 owner/tenant 索引）在 Local 模式下只是**数据标签**——所有写入使用同一常量、所有读取不做过滤，**不视为权限隔离机制**。

### 3.2 Application 层是否可信后端

- 现存的三类校验（self-share、share owner/admin、OCR 路径守卫/跨资源存在性）**都在 Application/Infra 的可信后端执行**，不依赖前端隐藏页面/按钮；§4 的 T12 通过直接 HTTP 调用证明 self-share 会被后端 422 拒绝。
- **但通用业务资源归属校验尚不存在**：`grep` 无 `authorize` / `AccessScope` / `AuthContext`，Repository 读无范围约束。因此 AC3 只能判为「现有校验在可信后端；多用户资源归属未实现」，不能判为「已完成资源归属校验」。

### 3.3 HTTP 与 IPC 一致性（源码断言）

新增测试 `http_and_ipc_share_and_auth_paths_use_the_same_use_cases`（`crates/ica-api/tests/sec04_authorization_http.rs`）用 `include_str!` 读取两个适配层源码并断言：

- HTTP（`routes.rs`）与 IPC（`commands.rs`）都调用同一批 use case：`create_share` / `list_shares` / `delete_share` / `register` / `login` / `refresh` / `logout` / `local_user`；
- 两侧 share 调用都传 `&local.id`（各 3 处），即 actor 来自可信 synthetic 身份，不来自请求参数。

实测：`test http_and_ipc_share_and_auth_paths_use_the_same_use_cases ... ok`。

补充事实：当前 Tauri host 的 `invoke_handler` 仅注册 5 条 positions 命令（`desktop/tauri-host/src/main.rs:853-859`），与 `ipc_allowlist.rs:17-24` 的 `ALLOWED_COMMANDS` 一一对应；`commands.rs` 中虽存在 auth/share 命令函数，但**未注册到 host**，其越权面目前不可通过桌面壳触达。该一致性由 `ipc_allowlist.rs:97-119` 的源码断言测试守护（本机缺 glib/WebKit 不编译 host，采用源码断言而非类型检查）。

## 4. AC4 — IDOR / 越权测试矩阵

### 4.1 计划 §6.8 的 14 项逐条结论

| 编号 | 场景 | 多用户预期 | Local 可测性 | 实测结论 | 证据 |
| --- | --- | --- | --- | --- | --- |
| T01 | 无 Token 访问受保护 API | 401 | 可测 | **实测 200**；伪造 Bearer 与无 header 响应逐字节相同 → Local 边界事实，多用户硬阻塞 | `local_mode_does_not_gate_business_routes_on_a_bearer_token` |
| T02 | 无效 Token | 401 | 可测（凭据消费端点） | refresh `not-a-token` → **401 unauthorized**；业务路由仍忽略 token | `an_invalid_token_is_rejected_with_401` |
| T03 | 过期 Token | 按刷新策略 | 部分 | 已吊销 refresh 重放 → 401 `Refresh token expired or revoked`；精确过期时间边界未单测 | `auth_shares_http.rs:163-174` |
| T04 | 用户 A 读用户 B 候选人 | 403 / 404 | **不可测** | `NOT EXECUTED`：Local 无两个真实用户；不得因无测试而判 PASS | §5 边界 |
| T05 | 用户 A 改用户 B 职位 | 拒绝 | **不可测** | `NOT EXECUTED`（同上） | §5 边界 |
| T06 | 用户 A 读用户 B 草稿 | 拒绝 | **不可测** | `NOT EXECUTED`（同上） | §5 边界 |
| T07 | 用户 A 读用户 B 跟进记录 | 拒绝 | **不可测** | `NOT EXECUTED`（同上） | §5 边界 |
| T08 | 用户 A 读用户 B 沟通记录 | 拒绝 | **不可测** | `NOT EXECUTED`（同上） | §5 边界 |
| T09 | 修改 URL 中资源 ID | 不越权（不存在→404） | 可测 | candidate / position / share 的不存在 ID 均 **404**，无泄漏 | `missing_resources_return_404` |
| T10 | 修改 Body 中 `owner_id` | 不得越权 | 部分 | 写请求模型无 `owner_id` / `user_id` 字段（仅注册可选 `tenant_id`，`body.rs:2255/2264`），写库由 infra 盖常量 → 客户端不能指定归属；但这**不能证明**多用户下不可越权 | `store.rs:24-28`、`body.rs:2255` |
| T11 | IPC 越权调用 | 拒绝 | 可测（源码断言） | host 仅注册 5 条 positions 命令；allowlist `reject_if_not_allowed` 单测通过；auth/share 命令未注册 | `ipc_allowlist.rs:52-78,97-119` |
| T12 | 直接调用 API 绕过前端 | 仍执行授权 | 可测 | 直接 HTTP 以 local actor 自身为 recipient → **422 `Cannot share with yourself`**，证明校验在后端 | `self_share_is_rejected_by_the_application_layer` |
| T13 | 备份/恢复权限检查 | 仅授权主体 | 部分 | 无管理员门槛；仅有 `confirm=true` 与路径守卫（`routes.rs:1278-1307`）→ 多用户硬阻塞 | 矩阵 ID 66–68 |
| T14 | 跨资源关联访问 | 必须校验关联归属 | 部分 | 存在性校验有（`candidates.rs:54/115/127` 等），**归属校验无**（Local 单用户） | §3.1 |

### 4.2 任务单指定的三项最小可测项

| 项 | 测试 | 实测输出 |
| --- | --- | --- |
| self-share 拒绝 | `self_share_is_rejected_by_the_application_layer` | 422，`error.message = "Cannot share with yourself"` |
| 不存在资源 404 | `missing_resources_return_404` | 3 个断言全部 404 |
| 非法 token 401 | `an_invalid_token_is_rejected_with_401` | refresh → 401 `unauthorized` / `Invalid refresh token` |

### 4.3 实测输出（真实粘贴）

```
$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test -p ica-api --test sec04_authorization_http
running 5 tests
test http_and_ipc_share_and_auth_paths_use_the_same_use_cases ... ok
test self_share_is_rejected_by_the_application_layer ... ok
test an_invalid_token_is_rejected_with_401 ... ok
test local_mode_does_not_gate_business_routes_on_a_bearer_token ... ok
test missing_resources_return_404 ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`local_mode_does_not_gate_business_routes_on_a_bearer_token` 是**特征化测试**（characterization test）：它记录当前事实并在将来加入真实认证门禁时期望失败/被替换；它不是「这样做是安全的」的批准。

## 5. Local Mode 安全边界与 P0 判定

1. **身份**：`LOCAL_ACTOR=local_user`、`LOCAL_OWNER_ID=…0001`、`DEFAULT_TENANT_ID=…0000`（`store.rs:24-28`）；`local_user()` 的 `role=admin`（`auth.rs:119-129`）。
2. **网络**：`HOST` 默认 `127.0.0.1`（`main.rs:53`），不向局域网默认暴露；未引入 CORS/CSRF 远程面。
3. **文件/数据**：本地数据目录依赖 OS 用户权限；SQLite 无用户隔离能力（见 §3.1）；IPC 仅应用内部可信调用 + 5 条命令 allowlist。
4. **不宣称**：不把 Local Mode 宣称为强隔离多用户模型；不把「IPC 与 HTTP 共用 use case」读成「已实现授权」（共用保证规则一致，但当前规则本身是空的）。
5. **P0 判定**：默认配置下无可证实的未认证敏感访问或跨用户读写 → **本轮不升级 P0**。任何使服务离开回环或引入第二个真实用户的变更，必须在发布前按 §4 的 T01/T04–T08/T13 重新评估；若届时仍无认证与归属约束，立即升级 P0 并暂停该模式发布。审计报告 §6.1 的「本轮无 P0」不构成永久豁免。

## 6. 兼容性

- **产品行为零变更**：未改动 `crates/` 下任何产品代码；`routes.rs`、Application/Infra/Domain 保持原样；68/68 契约实现与 `extra_routes=0` 不变；基线仓 `/mnt/o/ica` 未触碰。
- **凭据**：报告与矩阵中不含任何凭据值（按约束 `[REDACTED]`）；测试使用临时数据库（support 的 `tempfile::TempDir`），不读写生产/基线库。
- **回归**：`git diff --check` exit 0；`cargo test -p ica-api -p ica-application` 共 25 个测试目标、224 passed / 0 failed / 0 ignored。
- **新增面**：仅 1 个测试文件 + 2 份文档，不影响编译产物与运行时。

## 7. 回滚

由于没有产品代码变更，回滚是纯文件级操作（本轮未 `git add`/`commit`/`push`，改动全部留在本 worktree 工作区）：

```
rm -f docs/architecture/api-authentication-matrix.md \
      docs/audits/sec-04-api-authorization-report.md \
      crates/ica-api/tests/sec04_authorization_http.rs
```

若后续在真实认证门禁实现中发现正常访问被破坏，按实施方案 §10 的 SEC-04 条目处理：**修正授权规则，而不是关闭全部鉴权**。

## 8. 独立安全验证 — 待人工（未自签 Verified）

- 本报告由执行方输出证据矩阵，**不是独立安全验证结论**。按实施方案 §9.4，需由未参与实现的安全验证人手工执行：多用户 fixture 下的 T04–T08、真实网络暴露下的 T01/T03、host 侧真实 IPC 调用（本机缺 glib/WebKit，本轮只做源码断言）、以及配置/备份管理权限复核（T13）。
- 在上述人工验证完成前，SEC-04 状态保持「证据已交付，待验证」，**不转为 `Verified`**；本卡不自签、不改审计 tag、不宣称跨用户隔离成立。

## 9. 改动文件清单

| 文件 | 类型 | 作用 |
| --- | --- | --- |
| `docs/architecture/api-authentication-matrix.md` | 新增 | 68 条契约逐条认证/授权矩阵 + 自洽校验 |
| `docs/audits/sec-04-api-authorization-report.md` | 新增 | 本报告（AC1–AC5、P0 判定、边界、回滚） |
| `crates/ica-api/tests/sec04_authorization_http.rs` | 新增 | 5 条可执行测试：三项最小可测项 + HTTP/IPC 同 use case 源码断言 + Local 无门禁特征化 |
