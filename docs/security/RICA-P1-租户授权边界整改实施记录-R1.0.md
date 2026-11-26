# RICA P1 多租户授权边界整改实施记录 R1.0

- 文档编号：RICA-SEC-P1-TENANT-AUTH-IMPL-R1.0
- 审计方案：`docs/security/RICA-P1-多租户授权边界专项审计与整改方案-R1.0.md`（RICA-SEC-P1-TENANT-AUTH-R1.0）
- 方案基准提交：`831f3db1b9a596c9686a795c043f1913eb3cb51b`
- 实施工作区：基于同一基准提交的工作区改动（提交 SHA 在归档时回填）
- 状态：**部分整改完成，专项安全验收尚未通过**

> 本文件按方案第 13 节要求记录“实际做了什么、验证了什么、还有什么没做”。它不声称多租户安全验收通过。

---

## 1. 采用的决策（方案第 12 节推荐默认值）

本次实施按方案列出的推荐默认值执行，尚待产品/安全评审正式冻结：

| 决策 | 采用值 | 说明 |
|---|---|---|
| DEC-AUTH-01 | 租户管理员与平台权限分离 | `platform_admin` 是唯一能授予平台权限的角色；旧 `admin` 只映射为 `tenant_admin` |
| DEC-AUTH-02 | Dashboard 按租户过滤 | 租户端点只返回本租户；平台聚合以契约外路由 `GET /api/dashboard/platform` 暴露，需平台权限 |
| DEC-AUTH-03 | 租户审计与平台审计分离 | 租户审计强制 tenant filter；平台查询需 `AuditReadAll` |
| DEC-AUTH-04 | R1.0 仅平台级全局备份 | Backup/Restore 全部改为平台权限 |
| DEC-AUTH-05 | 平台快照单独授权 | 现有全量 Export/Import 均为平台快照；租户数据包未在本轮实现 |
| DEC-AUTH-06 | 配置分层（仅完成平台层门禁） | 全局 Settings/AI/Secret 改为平台权限；租户/用户三层数据模型未落地 |
| DEC-AUTH-07 | Sidecar 信任委托 | 主机签发短期 HS256 内部断言（iss/aud/exp/jti/ bearer 绑定），Python 验签 + nonce 单次使用；Local 模式保持不变 |
| DEC-AUTH-08 | 权限撤销 | 采用每请求权威状态重载（等价于实时校验，比短 TTL 更严格） |

Local 模式例外：单用户安装的管理员是服务端合成的受信身份，显式持有全部平台权限；它不来自角色字符串比较，因此不改变 MultiUser 下的“租户管理员 ≠ 平台管理员”结论。

---

## 2. 已实施改动

### AUTH-01 强类型主体与权限模型

- `crates/ica-application/src/request_context.rs`
  - 新增 `PrincipalKind`、`TenantRole`、`PlatformPermission`、`AuthMethod`、`DenyReason` 强类型。
  - `RequestContext` 增加 `principal_kind`、`tenant_role`、`platform_permissions`、`authn_method`、`membership_active`；角色字符串只在构造边界映射一次。
  - `authorize` 改为按 `TenantRole` 判定；新增 `authorize_platform`、`require_tenant_scope`。
  - `platform_admin` 是唯一自动获得平台权限的角色；`admin` 不再隐式提升。
  - 拒绝原因码进入错误 `details.reason_code`（`crates/ica-application/src/error.rs` 新增 `details_reason()`）。
- 平台操作统一调用 `authorize_platform(...)`，租户操作继续调用 `authorize(...)`。

### AUTH-02 每请求主体状态/成员关系/角色校验

- `crates/ica-api/src/auth.rs`：JWT 验签后调用 `revalidate_principal()`，从 `AuthRepository` 重载用户，校验 `is_active`、租户归属，并用数据库角色刷新 `tenant_role`/平台权限。
  - 禁用账号 → 401 `AUTH_PRINCIPAL_INACTIVE`
  - 成员关系失效/租户变更 → 403 `AUTH_MEMBERSHIP_INACTIVE`
  - 角色降级 → 原 Token 立即失去对应权限（`AUTH_PERMISSION_DENIED`）

### AUTH-03 Dashboard 租户聚合

- `crates/ica-application/src/dashboard.rs`：`dashboard()` 在 Use Case 内完成门禁并从 `RequestContext` 派生 `Scope`；新增需平台权限的 `platform_dashboard()`。
- `crates/ica-api/src/routes.rs`：新增契约外路由 `GET /api/dashboard/platform`，仅 `platform_admin` 可达（403 + `AUTH_PLATFORM_PERMISSION_REQUIRED`）；登记进 `ROUTE_PATTERNS` 并在反向清单测试中显式豁免，不追加 68 条契约矩阵行。
- `crates/ica-application/src/ports.rs`：`DashboardRepository` 全部 12 个方法接收 `Option<&Scope>`。
- `crates/ica-infra/src/follow_ups.rs`：12 条 SQL 全部显式带 `tenant_id`（非管理员另带 `owner_id`），包括维护清理与日历统计，不再先查全量再过滤。
- HTTP 与 IPC 两个入口都传入各自的 `RequestContext`。

### AUTH-04 审计租户/平台分离

- `AuditTrailRepository::recent_events` 接收 Scope；SQLite 查询强制 `tenant_id`，非管理员额外限定 `actor_id`。
- 新增 `platform_audit_events()`（需 `AuditReadAll`），`recent_audit_events()` 需租户管理员权限。

### AUTH-05/06/07 平台级门禁

- Backup 列表/创建/恢复 → `PlatformPermission::BackupManage`。
- Export/Import 全量快照 → `PlatformPermission::SnapshotManage`。
- Settings、AI Egress、模型目录、路由、Usage、Budget、AI Key 写入/清除 → `PlatformPermission::AiSettingsManage`。
- Follow-up Policy 保持租户管理员门禁（租户级业务策略）。
- 上述 Surface 的租户管理员请求统一返回 403 + `AUTH_PLATFORM_PERMISSION_REQUIRED`。

### AUTH-08 Sidecar 受信身份断言

- `crates/ica-desktop/src/internal_assertion.rs`（新增）：HS256 断言的签发与校验，含 `iss=ica-desktop-host`、`aud=ica-sidecar`、`exp`（60 秒）、`jti` nonce、以及与当次 bearer 的 SHA-256 绑定；另有承载令牌校验（禁止 refresh token 委派）。3 个 Rust 单测覆盖正常/伪造/过期/错受众/错 bearer。
- `crates/ica-contracts/src/proxy_headers.rs`：`x-ica-internal-assertion` 加入请求头白名单；响应白名单不变，断言不会回传 webview。
- `desktop/tauri-host/src/main.rs`：主机启动时生成进程级断言密钥；MultiUser 下对 Python 后端请求先验 bearer，再签发断言并注入转发头；验证失败直接 401。MultiUser 下同时向 Sidecar 注入 `AUTH_MODE=multiuser` 与主机 JWT 密钥，使 Sidecar 能独立验签并重载用户。
- Python Sidecar：`app/config.py` 增加 `sidecar_assertion_secret`；`app/auth/security.py` 增加 `decode_sidecar_assertion` / `bearer_fingerprint`；`app/auth/deps.py` 在 MultiUser 下要求断言、验签、检查 bearer 绑定与 nonce 单次使用，然后仍以 bearer + 数据库记录决定身份。Local 模式行为不变。
- `desktop/tauri-host/bundle-resources/**` 被 `.gitignore` 排除（打包暂存区），因此 Python 改动不会进入本提交；变更的完整源码说明与可执行测试规范见 [RICA-P1-AUTH-08-Sidecar-Python变更说明.md](RICA-P1-AUTH-08-Sidecar-Python变更说明.md)，打包流程必须把该变更同步到权威 Python 基线。

### §10 结构化授权决策日志

- `crates/ica-application/src/ports.rs`：新增 `AuthzDecision` / `AuthzDecisionRecorder` / `NoopAuthzDecisionRecorder`。
- `crates/ica-infra/src/migrations.rs`：canonical v2 迁移创建 `authz_decisions` 旁表（含 request_id、actor_id、tenant_id、action、decision、reason_code、entrypoint、scope_kind、occurred_at）；不修改冻结的 `audit_events` DDL，也不加入 `contract_capture.DB_TABLES`，因此 L2/L4 契约对拍不受影响。
- `crates/ica-infra/src/authz_decisions.rs`：SQLite 持久化实现，仅写入脱敏字段，禁止记录 token/Secret/Cookie/正文。
- `crates/ica-api/src/state.rs`：默认 no-op recorder，composition root 可注入真实实现；`ica-server` 与测试夹具已注入。
- `crates/ica-api/src/auth.rs`：`authenticate` 在 401/403 响应返回前记录 deny 决策（reason_code 从错误 `details` 提取）；对 `/api/dashboard/platform`、备份、导出导入、Settings/AI 等高风险平台操作的 2xx 响应记录 allow 决策。记录失败不改变原始响应。
- 断言：`authorization_denials_are_recorded_as_structured_decisions` 与 `successful_platform_operations_are_recorded_as_allow_decisions` 校验 request_id、actor、reason_code、decision、scope_kind、entrypoint。

### AUTH-09 负向测试

- `crates/ica-api/tests/sec04_bc_authorization_http.rs`
  - 测试夹具新增真实平台管理员身份（MultiUser 中间件要求主体存在于存储中）。
  - `tenant_admin_cannot_reach_platform_surfaces`：覆盖 TA-002/004/005/007/010。
  - `dashboard_and_audit_are_tenant_scoped`：双租户数据验证 TA-001/003。
  - `stale_tokens_lose_access_after_principal_changes`：验证 TA-017/018/019。
  - `admin_operations_reject_member_role` 更新为“租户 admin 不得通过平台门禁、platform_admin 可以通过”。

---

## 3. 验证证据

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace`（`CARGO_TARGET_DIR=/c/ica-target`） | 通过 |
| `cargo test -p ica-api --test sec04_bc_authorization_http` | 15 passed / 0 failed（含平台 Dashboard 与 deny/allow 决策日志用例） |
| `cargo test -p ica-infra --test migrations` | 6 passed / 0 failed（canonical v1+v2） |
| `cargo test -p ica-desktop --lib internal_assertion` | 3 passed / 0 failed |
| `cargo check --manifest-path desktop/tauri-host/Cargo.toml` | 通过（Tauri 主机含断言签发路径） |
| `cargo test --locked --workspace` 门禁等价命令（3 个 DEF-006 POC 用例按 `scripts/gate.py` 的名单 `--skip`） | 51 套件、561 用例全部通过（含 AUTH-08、平台路由与 deny/allow 决策日志后重跑确认） |
| `cargo clippy --locked --all-targets --workspace -- -D warnings` | 通过（含新增断言模块与平台路由；既有的 `insert_conversation_row` 增加了 `#[allow(clippy::too_many_arguments)]`） |

环境说明：O: 盘已满（`target/` 15 GB，剩余约 73 MB），全量编译在 O: 上出现 `no space on device` / `LNK1318 PDB LIMIT`；改用 `CARGO_TARGET_DIR=/c/ica-target` 后正常。这与代码无关，但证据必须记录实际环境与命令。

### 门禁状态

Rust 源码门禁（fmt + check + clippy + 全量测试）与 Tauri 主机 `cargo check` 已在同一工作区执行；`cargo audit`、前端、契约对拍与 Python Sidecar 门禁未在本轮执行。Python 单测文件已新增，但本环境 `python3` 返回退出码 49，无法执行。由于 Schema 扩展、租户数据包与统一门禁仍未关闭，专项验收结论仍为未完成。

---

## 4. 未关闭项（不得据此宣称验收通过）

| 项 | 状态 | 原因 / 后续 |
|---|---|---|
| AUTH-08 Sidecar 签名身份断言 | 已实施（Python 侧未在本环境执行验证） | Rust 签发/校验、主机注入、Python 验签与 nonce 代码已落地；本环境 `python3` 执行被阻断（退出码 49），Python 单测未能实跑，必须在有 Python 的 CI 上执行 `unittest discover -s app/tests`。 |
| AuditEvent 模型扩展 | 阻塞（契约对拍） | 方案要求 `scope_kind`/`request_id`/`outcome` 等字段；直接给 `audit_events` 加列会改变 `SELECT *` 的列集合，破坏 L2/L4 契约对拍，也会让重放基线 DDL 的测试夹具缺少新列。安全做法是新增 Rust 侧 `audit_event_scope` 旁表 + v2 migration，并同步迁移测试与夹具；这属于跨 Python 基线的协同变更，未在本轮强行实施。 |
| Tenant Package 导入导出 | 未实施 | 现有包仍是全量平台快照，已改为平台权限；租户白名单包与 ID 重映射未实现。 |
| AI 三层配置与 Secret 分层 | 部分 | 全部 AI/Settings 表面已收口到平台权限（默认拒绝）；租户级模型/路由/预算/Usage 的 tenant_id 数据列与 Use Case 未实现，同样受契约对拍约束。 |
| `get_platform_dashboard` HTTP 出口 | 已暴露 | `GET /api/dashboard/platform`，契约外路由，仅平台权限；合法平台运营总览仍需产品确认（DEC-AUTH-02）。 |
| 客户端伪造 tenant_id 的全量扫描 | 部分 | 业务资源与本次改造的聚合路径已由服务端 Scope 决定；Import/AI 等请求体字段尚需逐项审计。 |
| 高风险平台操作的业务审计事件 | 部分 | §10 的结构化授权决策（deny 与平台操作 allow）已落入旁表；成功操作尚未写 `audit_events`（会破坏契约捕获的 baseline 行集合），需在 Python 基线侧协同增加事件类型。 |
| `cargo audit`、前端/契约/Python Gate | 未重跑 | 本轮改动集中在 Rust 授权路径；统一门禁需在提交后同一 HEAD 重跑并归档。 |
| 证据绑定提交 SHA | 待回填 | 当前工作区尚未提交；按方案要求证据必须绑定实际 Commit SHA。 |

---

## 5. 结论

本轮关闭了“租户管理员借 `admin` 角色触达平台级聚合/备份/快照/AI 配置”“旧 JWT 在账号禁用、角色降级、成员移除后继续有效”以及“Sidecar 仅凭客户端 bearer 即可访问”的核心 P1 风险；Dashboard/Audit 聚合在 SQL 层租户化，平台聚合有独立受权出口。按照方案第 8.4 节验收门槛，AuditEvent Schema 扩展、租户数据包、AI 三层数据模型、Python 侧实跑验证与统一门禁仍未完成，因此：

**专项状态：整改进行中，安全验收未通过。**
