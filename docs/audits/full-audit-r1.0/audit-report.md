# RICA 全量工程实现审计报告 R1.0

## 1. 审计摘要

| 项目 | 内容 |
|---|---|
| 仓库 | `BryanTianjx/rica` |
| 审计基线 | `c3749018a61132aecf1a2db0a3fcdc47af723493` |
| 基线提交 | `merge: UI Phase 2 系统设置分组与子页` |
| 基线时间 | 2026-11-24 00:53:35 UTC |
| 审计范围 | Rust Workspace、API、认证授权、数据访问、前端、Tauri/桌面、AI/Browser、测试门禁、工程文档与产品化证据 |
| 审计方式 | GitHub 远程源码/配置读取、提交记录与差异检查、已有测试报告审查 |
| 未执行事项 | 本地完整克隆构建、当前 HEAD 全量测试、Windows 实机安装回归、动态安全测试、渗透测试 |

**总体结论：有条件通过工程实现审查；当前证据不足以支持“正式发布验收通过”。**

项目已形成较清晰的 Rust 分层架构、统一 API 契约、SQLite 本地数据层、AI 接入、Tauri 桌面适配及前端质量门禁。当前主要风险集中在多用户资源级授权闭环、最新 HEAD 的全栈验证证据、Python/Rust 契约对拍、E2E 覆盖和桌面交付验证。

本报告是基于可读取证据的工程审查，不是逐文件动态审计。未读取或未执行的项目不应被推断为通过或失败。

## 2. 审计原则与判定口径

- **实现存在**：源码中可定位到相应实现。
- **测试通过**：有明确绑定审计基线的测试命令、结果和日志/报告。
- **产品验收通过**：有目标操作系统、安装包、版本、环境和验收记录。
- **待验证**：缺少当前基线的执行证据；不得标记为 PASS。
- **历史证据**：只适用于其报告明确绑定的 commit，不自动覆盖后续提交。

风险等级：
- P0：可导致重大数据泄露、数据损坏或系统性安全失陷，且已具备直接证据。
- P1：核心安全/业务正确性/发布能力存在实质风险，必须在对应能力正式开放前关闭。
- P2：工程质量、维护性、覆盖率或产品体验问题，应按计划治理。
- INFO：观察项或需要补充验证的事项。

## 3. 项目与版本基线

本轮读取到的仓库最新提交为：

- Commit：`c3749018a61132aecf1a2db0a3fcdc47af723493`
- Message：`merge: UI Phase 2 系统设置分组与子页`
- 时间：2026-11-24 00:53:35 UTC

该提交包含 UI Phase 2 设置中心分组、通用设置、场景路由编辑等变更。提交所附验收记录声明：

- Vitest：31 files / 239 tests passed。
- Frontend build：成功。
- Lint：0 errors / 3 historical warnings。
- UI Reuse：5/8 passed，regression=0。
- Windows Phase 2 桌面回归：未执行。

注意：以上是提交中记录的验收声明，并非本次在当前环境重新执行所得结果。

历史 ARCH-P6 测试报告绑定的 HEAD 为 `40575e1aa78b316cede43104e3e78c9e00956130`，早于本次审计基线。因此该报告不能作为 `c3749018` 当前完整全栈验收证明。

## 4. 架构审查

### 4.1 Rust Workspace 与分层架构

当前可见的架构方向：

```text
ica-server
  ├── ica-api
  ├── ica-application
  │     └── ica-domain
  └── ica-infra

共享契约：ica-contracts
桌面适配：desktop/tauri-host / ica-desktop
```

正向评价：
- Domain 与 Web、数据库、运行时基础设施隔离。
- Application 通过 Port/Repository 抽象隔离 Infrastructure。
- HTTP 与桌面适配有共享契约和应用层复用的设计方向。
- SQLite Adapter 作为基础设施实现集中管理数据访问。

审查风险：
- 需要通过依赖图/架构测试确认所有模块均遵守依赖方向。
- 需要检查 Tauri IPC 是否只承担适配职责，避免形成第二套业务规则。
- Workspace 仍允许部分严格 lint 规则为 allow，错误处理与公开 API 文档债务需要分阶段治理。

结论：架构方向良好；“所有模块均符合边界”尚未由本轮完整依赖扫描证明。

### 4.2 API 与契约

当前 `crates/ica-api/src/routes.rs` 已注册认证、设置、AI、备份、分享等路由，并通过认证 Middleware 注入 RequestContext。路由实现中可见部分业务查询使用 Scope。

历史项目文档记录 68 个契约接口的实现对账，但当前 HEAD 下完整 Python/Rust 契约对拍未获得重新执行证据。

整改要求：
- 固定 Python 基线 commit 与 Rust 审计 commit。
- 对全部契约逐项比对方法、路径、请求、响应、状态码、错误体、分页、排序和边界行为。
- 将对拍结果输出为机器可读报告并绑定 commit SHA。
- 对新增/删除路由进行 allowlist 差异检查。

结论：接口实现覆盖面较广；当前版本兼容性仍需重新对拍。

### 4.3 身份认证与资源授权

源码可见认证 Middleware：

```rust
.layer(middleware::from_fn_with_state(
    state.clone(),
    auth::authenticate,
))
.layer(Extension(mode))
```

Server 启动代码仍保留 `NETWORK_AUTHENTICATION_READY = false`，并限制非 loopback 监听。这一默认拒绝策略是合理的安全保护，不应为了方便测试而绕过。

当前证据支持的判断：
- JWT/认证模式及 RequestContext 注入已有实现。
- Positions 存在 Scope 查询路径。
- 认证实现存在不等于所有业务实体的资源级授权已完成。
- 安全整改文档提示资源归属过滤仍需扩展到其他业务资源。

需要逐项验证 Candidates、Conversations、Messages、Drafts、Follow-ups、Browser 数据、Shares 等实体的读取、创建、更新、删除、关联访问是否强制校验 Tenant/Owner/Share 权限。

**P1：在全实体授权矩阵和跨租户负向测试完成前，不应将 MultiUser 宣称为完整安全能力，也不应开放远程监听。**

### 4.4 数据库与持久化

本轮确认项目使用 SQLite/SQLx 方向，并在 Server 启动路径执行数据库自检与迁移；只读/immutable URL 跳过迁移。备份目录、保留数量和密钥存储配置在 Server 初始化路径中可见。

仍需验证：
- Migration 从所有受支持历史版本升级的兼容性。
- Migration 中断、磁盘满、只读文件和并发启动行为。
- 备份是否采用一致性快照，恢复是否有完整性校验。
- Windows/macOS Keyring 与 Linux fallback 的权限、异常和恢复行为。
- 数据库路径、备份路径和导入/导出路径的规范化及路径穿越防护。

本轮没有执行数据库故障注入，故不对上述运行时行为作通过判断。

### 4.5 AI 子系统

Server 启动路径可见 AI Draft Generator、SecretStore、预算配置、模型路由等真实组件初始化，不应再将 AI 子系统描述为“完全未接入”。

仍需补充验证：
- Provider 请求超时、重试、取消与错误归一化。
- Key 写入/清除/回退优先级及日志脱敏。
- 预算并发竞争、跨日重置和异常响应计费处理。
- AI 请求数据出网授权、用户撤回同意后的行为。
- Provider 返回畸形内容时的输出校验与事务回滚。

### 4.6 Browser 子系统

历史测试报告记录 3 个 Browser POC 测试跳过，不能计入通过。需要核验浏览器进程生命周期、超时清理、崩溃恢复、配置隔离、用户数据目录和自动化操作权限。

在真实端到端证据形成前，Browser 稳定性应标记为待验证。

### 4.7 前端与 UI

UI Phase 2 已记录设置中心两级分组、section query、通用设置、场景路由编辑、AI Key 清除二次确认等实现。

提交附带验收数据：
- 31 个测试文件 / 239 tests passed。
- 构建成功。
- Lint 0 errors / 3 warnings。
- UI Reuse 5/8 passed，C3/C5/C8 仍失败；回归门显示 0 regressions。
- Phase 2 Windows 桌面回归未执行。

注意：回归门通过只说明未发现相对冻结基线的新增退化，不等于全部 UI 质量指标通过。

### 4.8 Tauri 与桌面交付

当前存在 Tauri Host / Sidecar 的桌面架构。最新 UI Phase 2 验收记录明确尚未执行 Windows 桌面回归。

需要形成 Windows 验收矩阵：
- 全新安装、首次启动、升级安装。
- Sidecar 启动/退出/异常重启。
- 端口占用、进程残留、权限不足。
- 数据目录迁移、备份恢复、卸载保留策略。
- 安装包签名、版本信息、升级回滚。
- 杀毒软件/SmartScreen 环境下的启动体验。

### 4.9 测试与工程门禁

历史 ARCH-P6 报告记录：
- Rust：522 passed、0 failed、3 skipped。
- Frontend：225 passed。
- Build：PASS。
- Clippy：PASS。
- Sidecar contract tests：6 passed。
- Tauri 实际构建：该次 WSL 执行未验证。
- Dependency security scan：BLOCKED。
- E2E：BLOCKED。
- Python baseline parity：未执行。

这些数字属于历史基线，不可直接代表当前 HEAD。最新 Phase 2 前端数据单独记录于其提交验收文档。

必须在当前 HEAD 重新执行完整门禁并生成唯一报告。

## 5. 缺陷与风险摘要

详见 [findings-register.csv](./findings-register.csv)。

重点风险：
1. P1：多租户资源授权覆盖不完整/缺少全实体负向测试证据。
2. P1：当前 HEAD 未获得完整 Rust、契约、桌面和安全扫描综合验收证据。
3. P1：Python/Rust 全量契约对拍缺少当前基线执行结果。
4. P1：Windows Phase 2 回归未执行。
5. P2：README 与分阶段实现状态存在漂移。
6. P2：Rust missing_docs / unwrap_used / expect_used 等规则仍较宽松。
7. P2：Browser POC 测试跳过及 E2E 缺口。
8. P2：UI Reuse C3/C5/C8 指标未通过。
9. INFO：历史依赖安全报告中的 SQLite advisory/版本需在当前 Cargo.lock 上重新核对，不能直接断言当前版本仍受影响。

## 6. 发布判断

当前不建议将审计基线标记为“正式发布验收通过”。

发布前最低退出条件：
- 全实体授权矩阵完成，跨租户正反向测试通过。
- 当前 HEAD 的 Rust/Frontend/Build/Lint/契约对拍/依赖扫描通过。
- 核心招聘业务 E2E 通过。
- Windows 当前版本安装、升级、备份恢复回归通过。
- 所有 P1 缺陷关闭，P2 有明确接受/延期记录。
- 发布文档、版本号、制品校验和签名信息齐备。

## 7. 审计限制

本次通过 GitHub 远程读取关键源码、配置、提交记录和验收文档；没有在本次执行环境中克隆仓库并运行编译、测试或桌面安装，也未执行动态扫描和渗透测试。报告中凡缺少直接源码或执行证据的事项均标记为待验证。

本报告不自动修改业务源码，不代表所有文件均已逐行检查，不构成第三方独立安全认证。
