---
title: "RICA 全量工程实现复审报告 R1.2"
document_id: "RICA-AUDIT-002"
document_type: "AuditReport"
project: "RICA"
version: "1.2.0"
status: "Approved"
created: "2026-11-23"
updated: "2026-11-23"
owner: "RICA 项目负责人"
reviewers:
  - "abc"
related_documents:
  - "RICA-AUDIT-001"
  - "RICA-ARCH-001"
  - "AUD-P1-01"
  - "AUD-P1-02"
  - "AUD-P1-03"
  - "AUD-P1-04"
  - "TEST-ICA-ARCH-P6-R1"
  - "DEF-ICA-REGISTER-001"
  - "ENG-ICA-SPECS-R1.0"（R1.2 基线校正依据）
supersedes: "RICA-AUDIT-001"
---

# RICA 全量工程实现复审报告 R1.2

> **文档性质：** 当前实现复审与历史审计基线校正。  
> **审计方式：** GitHub 远程仓库只读源码审查、工程文档审查、历史测试证据复核。  
> **重要限制：** 本报告不是本次 HEAD 的动态测试报告、独立安全验证报告或 Windows 产品验收报告。  
> **AI 辅助声明：** 本报告由 AI 根据指定仓库可访问内容辅助编制，状态为 Draft；必须由项目负责人复核证据、等级和整改决策后方可批准。

---

## 1. 审计目标

本次复审针对 RICA 仓库在 2026-11-23 的最新 main 基线，重新确认 R1.0 审计发现的整改状态，识别 ARCH-P6 之后仍然存在的实现、验证、质量门禁和产品交付风险。

目标如下：

1. 建立当前代码基线，区分当前实现与历史审计结论。
2. 复核 SEC-01 至 SEC-04 安全问题的整改状态，明确“实现完成”与“独立验证通过”的区别。
3. 审查 Rust Workspace、HTTP/IPC 双通道、SQLite、Tauri Host、Python Sidecar 与前端的实现边界。
4. 复核测试报告、统一质量门禁、CI 和 Windows 桌面验收证据。
5. 形成可执行的问题清单、风险评级、整改路线和验收标准。
6. 为后续缺陷台账同步及 P1/P2 实施任务提供依据。

## 2. 审计范围与排除项

### 2.1 审计范围

| 范围 | 审查内容 |
|---|---|
| 仓库与版本基线 | main HEAD、目录结构、关键提交与文档版本 |
| Rust Workspace | Crate 职责、依赖方向、Application 与 Adapter 边界 |
| API 与认证 | Axum Router、JWT、Refresh、业务 API 认证和资源授权 |
| 桌面运行时 | Tauri Host、HTTP Proxy、IPC 命令、Rust Server 生命周期 |
| 前端 | Backend Client、AuthContext、Token Store、HTTP/IPC 迁移 |
| 数据层 | SQLite、Repository、Migration、Backup/Restore 证据 |
| Agent / Sidecar | Python Sidecar、Browser 子系统及进程隔离边界 |
| 测试与质量 | Rust、Vitest、契约测试、统一 Gate、CI |
| 产品交付 | Windows 构建、安装、升级、卸载和恢复验收 |
| 工程治理 | 缺陷台账、审计证据、分支保护与人工发布规则 |

### 2.2 明确排除项

本次未执行以下操作，因此不对其作通过性结论：

- 未在本次审计环境重新克隆并构建完整仓库。
- 未重新执行 `cargo test`、`cargo clippy`、`cargo fmt --check`。
- 未重新执行前端 lint、typecheck、build、Vitest。
- 未运行 Tauri Windows 原生构建或安装包。
- 未进行动态渗透测试、真实多用户 IDOR 测试或独立安全验证。
- 未在 Windows 实机执行安装、升级、卸载和恢复测试。
- 未验证 GitHub Settings 中的 Branch Protection / Rulesets 是否已经在审计时点发生变化；本报告依据仓库治理文件记录其此前未执行状态。
- 未修改产品代码、配置、缺陷台账或发布设置。

## 3. 基线信息

### 3.1 Git 基线

| 项 | 值 |
|---|---|
| Repository | `BryanTianjx/rica` |
| 审计分支 | `main` |
| 审计 HEAD（R1.2 校正） | `ccf7408af9411bba58bf750b0d05acc70c0c5985` |
| HEAD 短 SHA | `ccf7408` |
| HEAD 提交时间 | 2026-11-23（规范化整改合并后） |
| HEAD 提交说明 | `merge: SPEC-QA test/case norms`（4 卡规范化整改合并） |
| 对照基线 | R1.0/R1.1 审计报告、ARCH-P6 交付记录、SEC-01～SEC-04 专项记录、R1.2 规范化整改（SPEC-GOV/BAS002/SCRIPTS/QA 四卡） |
| 工作区状态 | 远程仓库审查；未建立本地工作区，无法判断本地 dirty state |

### 3.2 技术架构基线

当前仓库采用 Rust Workspace + Tauri 2 + React + TypeScript + SQLite 的桌面应用结构，并包含 Python Sidecar 能力。

主要 Rust Crate：

| Crate | 主要职责 |
|---|---|
| `ica-domain` | 领域实体、值对象与领域规则 |
| `ica-application` | Application Use Case、应用服务与业务编排 |
| `ica-contracts` | DTO、跨传输契约与共享协议 |
| `ica-infra` | SQLite、Repository、Secrets、AI/OCR 等基础设施适配 |
| `ica-api` | Axum HTTP 入站适配器 |
| `ica-desktop` | 桌面适配、IPC、Proxy 与运行时支撑 |
| `ica-server` | Rust HTTP Server 组合根 |

桌面侧还包括 React/TypeScript 前端、Tauri Host、Rust Server 子进程和 Python Sidecar。

### 3.3 证据分级

本报告按仓库工程审计模板使用以下证据等级：

| 等级 | 定义 | 使用规则 |
|---|---|---|
| E1 | 当前审计基线可直接读取的源码、配置或当前文档证据 | 可用于描述当前仓库可观察事实 |
| E2 | 历史提交、历史测试报告、专项整改报告 | 只能证明对应历史基线或执行时点，不得直接外推到当前 HEAD |
| E3 | 设计目标、ADR、整改方案或计划 | 只能证明目标/决策，不得认定为已实现 |

证据引用以仓库相对路径、函数/章节为主。对于未在本轮动态执行的行为，必须标记为待验证。

## 4. 审计方法与规范依据

### 4.1 方法

1. 检查仓库树和 main 当前 HEAD。
2. 阅读 R1.0 审计报告、总体架构演进 ADR、ARCH-P6 交付记录。
3. 阅读 SEC-01、SEC-02、SEC-03、SEC-04 专项审计/整改记录。
4. 阅读测试报告、缺陷台账、桌面验收清单和 Branch Protection 规则。
5. 对照当前源码检查关键安全和运行时边界。
6. 将结论标记为当前实现事实、历史执行证据、风险推断或待验证事项。

### 4.2 规范依据

本报告遵循仓库内以下工程文件及模板：

- `docs/templates/engineering-audit-template.md`（TMP-011-ICA）：审计报告结构、E1/E2/E3 证据分级、结论与修订记录。
- `docs/templates/test-report-template.md`（TMP-010-ICA）：测试状态统计和证据边界；Blocked / Skipped / Not Applicable 不得计入 Passed。
- `docs/audits/RICA 总体架构演进设计与 ADR 决策 R1.0 2026-11-22.md`：模块化单体、Rust Core、HTTP/IPC、Sidecar 与安全边界目标。
- `docs/testing/defect-register.md`：缺陷生命周期与缺陷字段要求。
- `docs/audits/arch-p6-delivery-record.md`：ARCH-P6 交付边界和未完成项。

> 规范版本说明：模板要求在审计方法中记录规范及版本。当前远程仓库可见模板明确标识 TMP-011-ICA v1.0.0；若 SPE-GOV-001、SPE-QA-001 的正式全文或版本号未在本次读取范围内取得，本报告不臆造其条款编号或版本，相关条款引用以专项记录中的引用为历史依据，批准前应由文档负责人核对正式规范版本。

## 5. 现状分析

### 5.1 总体判断

当前实现已超出 R1.0 审计时点。SEC-01、SEC-02、SEC-03 已有对应实现整改记录；ARCH-P6 已增加统一门禁、CI 工作流、备份恢复演练和桌面验收清单。

因此，R1.0 中将 SEC-01～SEC-03 描述为“待整改”的结论已经过时。正确状态应为：

- 实现整改：已有专项报告证明完成。
- 独立验证：专项报告明确仍待 verifier 完成，不得标记 Verified。
- SEC-04：Local 单用户模式边界已被专项测试和矩阵说明，但多用户认证与资源授权尚未实现。
- ARCH-P6：交付为 DeliveredWithLimitations，不等同于产品发布验收通过。

**R1.2 基线校正（2026-11-23）：** 本报告 R1.1 版基于 `266d26e`。其后 main 合并了 4 张规范化整改卡（SPEC-GOV / SPEC-BAS002 / SPEC-SCRIPTS / SPEC-QA，commit `ccf7408`），已解决 R1.1 中的部分 P1 工程项。本版已按当前 HEAD 重核；凡状态变更处均标注「R1.2 校正」并附当前证据，未变更项保持原判。校正只涉及基线事实与已解决问题状态，不改变 E1/E2/E3 证据分级规则与发布判断。

### 5.2 SEC-01：JWT 密钥安全

**当前结论：实现整改已完成；独立安全验证待执行。**

证据：

- `crates/ica-infra/src/jwt_key.rs`：使用 `OsRng` 生成 32 字节密钥；提供 `load_or_create` 和 `rotate`。
- `desktop/tauri-host/src/main.rs`：Host 通过 `JwtKeyProvider` 读取/创建密钥，并将签名密钥传给 Rust Server。
- `docs/audits/sec-01-jwt-key-report.md`：记录 OS Credential Store、Linux 0600 文件回退、脱敏、轮换和测试证据。

当前仍需验证的设计点：

1. 当前仅保留一个活动密钥槽，无历史验证密钥和 `kid`。
2. 轮换后旧 Token 立即失效，需确认这是产品接受的会话策略。
3. Windows Credential Manager、macOS Keychain 和 Linux 回退路径需在目标平台独立验证。
4. Secret 的创建、损坏恢复、权限、轮换和日志脱敏需由非实现者复核。

不得将“实现完成”写成“安全验证通过”。

### 5.3 SEC-02：Tauri HTTP Proxy Header

**当前结论：Allowlist 策略及共享契约已实现；独立安全验证与 Windows 实机链路仍待执行。**

证据：

- `crates/ica-contracts/src/proxy_headers.rs`：共享请求/响应 Header 策略。
- `crates/ica-desktop/src/proxy.rs`：Proxy 策略适配。
- `desktop/tauri-host/src/main.rs`：Host 接入共享策略。
- `docs/audits/sec-02-proxy-header-report.md`：记录 Header Allowlist、Hop-by-hop 过滤、CRLF 和相关测试。

后续验证重点：

- Authorization 和业务 Header 在 WebView → Host → Rust Server 全链路一致。
- 非法 Header、重复 Header、超长 Header、CRLF 注入和响应头边界。
- Proxy 不允许用户输入控制任意目标地址或端口。
- Windows WebView2 实际行为与 Linux 测试结果一致。

### 5.4 SEC-03：认证状态恢复与 Token 生命周期

**当前结论：认证恢复状态机已有实现；独立安全验证待执行。**

证据：

- `frontend/src/auth/AuthContext.tsx`
- `frontend/src/auth/auth-state.ts`
- `frontend/src/services/api.ts`
- `frontend/src/auth/tokenStore.ts`
- `crates/ica-application/src/auth_recovery.rs`
- `docs/audits/sec-03-auth-lifecycle-report.md`

专项记录显示已建立认证状态机、启动恢复、Refresh 单次轮换、并发 single-flight 和失败后会话清理。

剩余关注：

1. 前端 Token Store 的持久化位置及 WebView 脚本注入威胁模型需要复核。
2. Refresh Token 重放、并发刷新、撤销、退出登录和应用重启需要组合测试。
3. Rust 与前端状态机的一致性需要继续由自动化测试守护。
4. 认证状态恢复不等同于业务 API 已执行认证授权；该边界由 SEC-04 单独审查。

### 5.5 SEC-04：业务 API 认证与资源授权

**当前结论：Local 单用户模式有明确实现边界；多用户/远程模式的业务认证与资源授权未完成。**

证据：

- `crates/ica-api/src/routes.rs`：路由层存在 `reject_head` 和尾斜杠处理等 HTTP 中间件；专项报告指出这些不是业务认证中间件。
- `crates/ica-server/src/main.rs`：默认 Host 为 `127.0.0.1`，可通过环境变量覆盖。
- `crates/ica-application/src/auth.rs`：存在 Local synthetic user 及部分分享操作校验。
- `crates/ica-infra/src/repository.rs`：Local 模式按既有行为不执行 tenant/owner 读过滤。
- `docs/architecture/api-authentication-matrix.md`：列出 68 条业务契约的目标认证分类和当前 Local 现状。
- `docs/audits/sec-04-api-authorization-report.md`：明确业务路由当前没有统一认证门禁，矩阵中的多用户认证/管理员/资源归属要求尚未落实。

风险解释：

- 当前默认回环监听 + Local 单用户模型，不应未经证据将其直接定性为已发生的公网越权事件。
- 但回环监听只是部署边界，不是 API 授权实现。
- 如果通过 `HOST` 将服务暴露到非回环地址，或启用远程/多用户模式，现有 Local 信任模型不再成立。
- 在认证中间件、AuthContext、资源级授权、Repository 范围约束和 IDOR 测试完成前，不得开放非回环监听或宣称多用户隔离已实现。

**发布约束：** 远程/多用户部署必须保持禁用；如业务需要开放该模式，应先完成 SEC-04 专项整改和独立验证。若在缺少认证授权的情况下实际开放远程业务 API，应立即升级为 Critical/P0 事件并暂停相关部署。

### 5.6 Rust Workspace 与分层架构

**当前结论：总体分层可保留，不建议大规模重构。**

当前 Crate 划分与 ADR 目标一致：Domain 表达领域规则，Application 编排用例，Contracts 提供共享契约，Infrastructure 实现外部依赖，API/IPC 作为入站适配器，Server/Host 负责组合和运行时生命周期。

建议继续执行以下约束：

- Domain 不依赖 Axum、Tauri、SQLx 和 Python 运行时。
- Application 不依赖具体 HTTP/IPC 框架及数据库实现。
- HTTP Handler 和 IPC Command 只做参数转换、身份上下文传递和错误映射，不复制业务规则。
- Infrastructure 通过 Application Port 接入，不反向控制 Application。
- 不为抽象而抽象，不为模块数量而拆 Crate。

当前静态复审没有发现足以支持推翻 Rust Workspace 的证据。该结论是架构方向判断，不代表每个 Crate 的依赖图已通过自动化约束测试。

### 5.7 HTTP / IPC 双通道

**当前结论：渐进式迁移路径合理，但业务模块迁移尚未完成。**

证据：

- `frontend/src/services/backend/client.ts`
- `frontend/src/services/backend/http.ts`
- `frontend/src/services/backend/ipc.ts`
- `frontend/src/services/backend/migration.ts`
- `crates/ica-desktop/src/commands.rs`
- `docs/audits/RICA 总体架构演进设计与 ADR 决策 R1.0 2026-11-22.md`

当前前端 Backend Client 已提供 HTTP/IPC 访问抽象，IPC 仍处于渐进迁移阶段，不能将“存在 IPC Adapter”描述为“全业务已 IPC 化”。

工程要求：

1. React 页面只能通过统一 Backend Client 访问业务，不得散落直接 `fetch` / `invoke`。
2. HTTP 与 IPC 调用同一 Application Use Case。
3. 两种 Adapter 的 DTO、校验、错误码、认证上下文和事务语义必须一致。
4. 每次迁移必须提供契约一致性测试与回归证据。
5. 不以 IPC 迁移比例作为目标，按实际安全、性能和部署需求决定是否迁移。

### 5.8 SQLite、Migration 与 Backup/Restore

**当前结论：已有数据库工程基础和机制级恢复证据；桌面产品路径仍待验收。**

证据：

- `crates/ica-infra/src/repository.rs`
- `crates/ica-infra/schema/ica-baseline.sql`
- `scripts/backup_restore_drill.py`
- `docs/testing/backup-restore-drill.md`
- `docs/testing/drill-evidence/`
- `docs/audits/arch-p6-delivery-record.md`

ARCH-P6 记录了临时数据库备份恢复演练，核对 DDL 摘要、逐表行数及探针数据。该证据证明机制级恢复路径在对应执行环境中通过，不证明 Windows 安装包数据目录、升级过程或用户真实数据恢复已通过。

仍需关注：

- Migration 从历史版本升级的连续性。
- 备份文件校验、损坏处理和版本兼容。
- Restore 前安全副本、WAL checkpoint 与失败回滚。
- Windows/macOS 用户数据目录和权限。
- 安装升级、卸载和重新安装对用户数据的影响。

### 5.9 Agent / Python Sidecar / Browser

**当前结论：Sidecar 架构已纳入工程，但 Browser POC 不能等同于生产级 Browser Runtime。**

证据：

- `crates/ica-desktop/src/sidecar.rs`
- `crates/ica-desktop/src/browser.rs`
- `crates/ica-desktop/tests/browser_poc.rs`
- `docs/testing/defect-register.md` 中 DEF-006
- `docs/audits/arch-p6-delivery-record.md`

历史测试记录指出，特定机器上 Chromium 路径可被发现但浏览器不能成功启动，导致 Browser POC 测试挂起；ARCH-P6 通过显式跳过 3 条 POC 测试保证常规 Gate 可继续执行。

该处理提高了常规门禁可用性，但未修复 Browser POC 的启动超时和进程回收根因。

必须区分：

- Sidecar Wire Contract 测试通过。
- Browser Worker 生命周期测试通过。
- Chromium 在目标操作系统可启动。
- 浏览器会话、权限、超时、取消和崩溃恢复通过。

以上是不同验收层级，不能互相替代。

### 5.10 测试与质量门禁

**当前结论：自动化基础已建立，门禁语义和未覆盖项仍限制其作为发布阻断门。**

权威历史执行证据：

- `docs/testing/test-report-arch-p6.md`
- `docs/audits/arch-p6-delivery-record.md`

该测试报告明确绑定历史受验 HEAD `40575e1aa78b316cede43104e3e78c9e00956130` 和对应执行时间。它不是当前审计 HEAD `266d26e` 的重新执行报告。

ARCH-P6 交付记录将结果标为 `PASS_WITH_LIMITATIONS`，并明确记录 Tauri Build、Security Scan、Windows 实机验收及 E2E 等限制。

质量门禁的主要问题：

1. Gate 允许在存在 BLOCKED 项时输出 `PASS_WITH_LIMITATIONS` 且退出码为 0。
2. 如果 CI 将退出码 0 作为唯一 Required Status Check，可能把环境未验证误读为门禁通过。
3. Tauri Host 编译、cargo-audit、Windows 安装验收及核心业务 E2E 尚未形成完整跨平台阻断链。
4. Browser POC 测试被显式 Skip，必须持续显示为未通过/未执行，而不是计入 Passed。
5. 需要区分开发质量门禁与正式发布门禁。

建议建立两层 Gate：

- **PR Required Gate：** 代码格式、静态检查、可执行单测/集成测试、前端构建、契约对账等可在 CI 稳定执行的项目；失败必须非零退出。
- **Release Acceptance Gate：** Windows 构建、安装/升级/卸载/恢复、独立安全验证、核心业务 E2E、发布依赖扫描等；任何 Required 项 BLOCKED 均不得判定为 Release PASS。

### 5.11 测试证据与 Git Provenance

**当前结论：ARCH-P6 报告明确记录历史受验 HEAD；后续报告必须进一步绑定工作树状态和产物来源。**

`docs/testing/test-report-arch-p6.md` 将执行绑定到 `40575e1`；`docs/audits/arch-p6-delivery-record.md` 记录相关实现文件在 worktree 中执行，后续由提交 `06a9de1` 入库并经 `05b4f14` 合并到 main，当前仓库 HEAD 后续又由文档提交更新到 `266d26e`。

因此不能简单地把“测试报告 HEAD 与当前 HEAD 不同”判定为报告错误：历史测试报告本来就应绑定历史受验基线。

真正需要加强的是：当测试在含未提交修改的 worktree 中执行时，报告必须同时记录：

- Git HEAD。
- `git status --porcelain`。
- `git diff --binary` 或等效补丁摘要 SHA-256。
- 未跟踪文件清单及内容摘要。
- 执行脚本的版本/摘要。
- 最终提交 SHA 与测试执行内容之间的对应关系。

缺少这些信息时，无法仅凭 HEAD SHA 复现 worktree 中实际执行的全部代码。该问题属于测试证据可追溯性不足，不应错误描述为“历史报告数字必然无效”。

### 5.12 Windows 桌面交付

**当前结论：产品交付验收未完成。**

证据：

- `docs/testing/desktop-release-acceptance.md`
- `docs/audits/arch-p6-delivery-record.md`

ARCH-P6 已建立 Windows 9 项验收清单，但交付记录明确标注由于执行环境为 WSL2、无 Windows 实机，安装、升级、卸载和恢复实测为 BLOCKED。

不得将 Linux/WSL 机制测试代替 Windows 安装包验收；不得由 Agent 代签实机验收。

### 5.13 工程治理与发布保护

**当前结论：规则文档已建立，仓库设置是否落实仍需管理员证据。**

证据：

- `.github/BRANCH_PROTECTION.md`
- `.github/workflows/quality-gate.yml`
- `docs/audits/arch-p6-delivery-record.md`

Branch Protection 文档明确说明其本身不是已生效的 GitHub 设置；需要管理员配置 main 分支保护、Required Status Check、PR Review 和 Tag Ruleset，并执行文档中的验证步骤。

CI Workflow 的存在不能证明 GitHub 分支保护已经启用。

---

## 6. 架构审查结论

| 架构领域 | 结论 | 证据等级 | 后续要求 |
|---|---|---|---|
| Rust 模块化单体 | 方向合理，保留 | E1/E3 | 继续约束依赖方向 |
| Domain / Application 边界 | 基本符合目标架构 | E1 | 增加架构依赖自动检查 |
| HTTP / IPC 双 Adapter | 统一 Client 已建立，迁移未完成 | E1 | 保持契约和错误语义一致 |
| SQLite / Repository | 基础机制已建立 | E1/E2 | 补充升级与桌面恢复验证 |
| Tauri Host | 承担运行时装配与生命周期职责 | E1 | 避免业务逻辑下沉到 Host |
| Python Sidecar | 独立能力适配方向合理 | E1/E2 | 补齐超时、取消、崩溃恢复 |
| Browser 子系统 | POC 证据不足以认定生产就绪 | E1/E2 | 独立验收 Browser Runtime |
| 认证与授权 | Local 模式与多用户模式边界必须明确 | E1/E3 | SEC-04 未完成前禁止远程开放 |

**架构决策：不建议整体重构或拆分微服务。** 近期工程资源应投入 SEC-04 安全边界、Gate 语义、Browser 稳定性、E2E 和 Windows 产品验收。

## 7. 代码与实现审查问题清单

本节等级使用工程审计模板的 Critical / Major / Minor / Observation；优先级使用项目 P0 / P1 / P2 / P3。状态使用 New / Triaged / In Progress / Fixed / Ready for Retest / Verified / Closed 等缺陷生命周期语义。

| ID | 问题 | 等级 | 优先级 | 当前状态判断 | 证据 |
|---|---|---|---|---|---|
| SEC-01 | JWT 密钥安全实现需独立复核 | Major | P1 | Fixed / 待独立验证 | E1/E2 |
| SEC-02 | Proxy Header 策略需独立复核及平台验证 | Major | P1 | Fixed / 待独立验证 | E1/E2 |
| SEC-03 | Token 生命周期和恢复链路需独立复核 | Major | P1 | Fixed / 待独立验证 | E1/E2 |
| SEC-04 | 多用户业务 API 认证与资源授权未实现 | Major | P1 | Triaged / 未关闭（**R1.2 校正：** SEC-04-A 非回环 fail-fast 已实现；B/C 多用户授权与 IDOR 仍未实现） | E1 |
| QA-GATE-01 | BLOCKED 项可能随退出码 0 被 CI 接受 | Major | P1 | **R1.2 校正：Fixed / 待独立验证**（`scripts/gate.py` 缺 cargo-audit 由 BLOCKED 升 FAIL、实测退出码 1；CI `Enforce security_scan PASS (merge blocker)`） | E1（当前源码） |
| TEST-01 | Browser POC 启动失败可导致测试挂起 | Major | P2 | Triaged | E2 |
| QA-02 | 招聘核心业务 E2E 不足 | Minor | P2 | Triaged | E2 |
| QA-03 | Windows 安装/升级/卸载/恢复未实机验收 | Major | P2 | Blocked / 待人工 | E2 |
| SEC-05 | SQLite/传递依赖 advisory 需持续复核 | Minor | P2 | **R1.2 校正：** 工具链已接入（`deny.toml` + CI cargo deny/audit）；advisories 实跑因本机无法 clone RustSec DB 未执行，仍待网络环境验证 | E1/E2 |
| GOV-01 | Branch Protection / Ruleset 尚需管理员落实 | Minor | P2 | Proposed / NOT_EXECUTED（历史记录） | E2 |
| DOC-05 | Worktree 测试证据需绑定 Dirty State 与补丁摘要 | Minor | P2 | New | E1/E2 |
| ARC-01 | HTTP/IPC 迁移仍未覆盖全部业务模块 | Observation | P3 | In Progress / 渐进迁移 | E1 |
| SEC-06 | 前端 Token Store 威胁模型需复核 | Observation | P2 | New / 待安全评估 | E1/E2 |
| SEC-04-A | 非回环启动 fail-fast（R1.2 新增跟踪） | Major | P1 | **R1.2 校正：Fixed / 待独立验证**（`ica-server::validate_bind_host` 拒绝非 loopback，3 条单测） | E1 |

> 注意：上表是本轮复审提出的建议状态，不自动覆盖 `docs/testing/defect-register.md` 中既有编号和状态。SEC-01～SEC-04 等编号必须与现有缺陷台账及专项任务编号核对后再同步，避免重复建单或错误关闭。

## 8. 风险评级

### 8.1 当前风险

| 风险 | 影响 | 触发条件 | 控制要求 |
|---|---|---|---|
| Local API 被错误暴露 | 未认证访问业务数据/操作 | 非回环监听、端口转发、远程部署 | SEC-04 完成前禁止远程开放 |
| 安全整改未独立验证 | 实现缺陷可能残留 | 将实现报告误作安全验证 | verifier 与 implementer 分离 |
| Gate 对 BLOCKED 宽松 | 未验证能力进入合并/发布 | CI 仅看退出码 0 | Required Gate / Release Gate 分层 |
| Browser Worker 挂起 | 测试流水线阻塞、进程残留 | Chromium 可发现但不可启动 | 启动/连接超时与进程回收 |
| E2E 缺失 | 跨模块业务回归难发现 | 多模块改动、升级 | 招聘核心闭环 E2E |
| Windows 验收缺失 | 安装和数据迁移风险 | 发布 Windows 安装包 | Windows 实机验收 |
| Provenance 不完整 | 历史测试难以复现 | Worktree 有未提交改动 | 记录 Dirty State 和 Patch Hash |

### 8.2 P0 判断

本次静态复审没有确认当前 main 基线已经发生 P0 级安全事件。

但存在明确的条件性升级规则：

**如果在 SEC-04 未完成前，将业务 Server 暴露到非回环网络或启用远程/多用户模式，应按 Critical/P0 事件处理，立即关闭相关暴露面并暂停该模式发布。**

该判断是风险触发规则，不代表本次已经观察到公网暴露或实际数据泄漏。

## 9. 测试与质量审查

### 9.1 权威测试证据

当前可引用的最近完整历史测试报告为：

`docs/testing/test-report-arch-p6.md`

该报告绑定其自身的历史 HEAD 和执行时间。其测试统计、环境、Skip/Blocked 说明均应从该报告读取，本报告不将其数字复制为当前 HEAD 的测试结果。

### 9.2 当前审计未执行的测试

| 测试项 | 本次状态 | 是否可判 PASS |
|---|---|---|
| Rust Workspace 测试 | NOT EXECUTED | 否 |
| Rust fmt / clippy | NOT EXECUTED | 否 |
| Frontend lint / build / Vitest | NOT EXECUTED | 否 |
| Tauri Windows Build | NOT EXECUTED | 否 |
| Browser POC 全量测试 | NOT EXECUTED | 否 |
| 真实多用户授权/IDOR | NOT EXECUTED | 否 |
| 独立安全验证 | NOT EXECUTED | 否 |
| Windows 安装/升级/卸载/恢复 | BLOCKED / 待人工 | 否 |

### 9.3 Gate 改进要求

- PR Required Gate 必须将真正的 Required Check 失败映射为非零退出码。
- BLOCKED 不得计入 Passed。
- 若某项是合并必须满足的 Required Check，BLOCKED 必须阻止合并，或由对应平台 Runner 完成。
- Release Gate 必须覆盖 Windows 安装验收、核心业务 E2E 和独立安全验证。
- Gate 输出应保留每个阶段的命令、退出码、耗时、环境和证据路径。
- CI 不得自动 commit、push、merge、tag 或 release。

## 10. 问题清单与缺陷台账同步要求

### 10.1 本次报告新增/重述问题

本轮建议确认以下任务：

1. QA-GATE-01：统一门禁 Required/Optional/Blocked 语义及退出码。（**R1.2 校正：** 已由 SPEC-SCRIPTS 卡修复，gate.py 安全缺失→FAIL/exit 1，CI 有 merge blocker；待独立验证后关闭）
2. DOC-05：补充 Worktree Dirty State、Patch Hash、执行脚本摘要和最终 Commit 映射。
3. SEC-06：复核 Tauri WebView 环境下 Token Store 的安全边界。
4. SEC-04：将 Local-only 部署约束转化为可测试的启动策略和发布门禁。（**R1.2 校正：** SEC-04-A 非回环 fail-fast 已实现；多用户认证授权 SEC-04-B/C 仍未实现）

### 10.2 台账同步规则

本次只新增审计报告，不直接修改既有缺陷台账，避免在未核对原始 DEF/SEC 编号、责任人、发现版本和状态证据前覆盖台账。

报告批准后，缺陷负责人应按 `docs/testing/defect-register.md` 的字段要求同步：

- 缺陷编号与标题。
- 发现版本/HEAD。
- 影响模块。
- 严重度与优先级。
- 复现条件、步骤、预期与实际。
- 证据位置。
- 修复版本。
- 独立回归与验证结果。
- 当前生命周期状态。

**Fixed ≠ Verified；Verified ≠ Closed，除非项目流程明确规定并具备对应证据。**

## 11. 整改建议与行动清单

### 11.1 P1：安全与质量门禁

| 任务 | 类型 | 主要工作 | 完成条件 |
|---|---|---|---|
| SEC-04-A | 工程项 | 明确 Local-only 启动策略；限制非回环监听；避免默认管理员身份跨边界使用 | **R1.2：已实现** `validate_bind_host` + 3 单测（loopback 接受、非 loopback 拒绝、real bind）；待独立验证 |
| SEC-04-B | 工程项 | 设计 AuthContext、认证中间件、资源归属与 Repository scope | 68 条契约逐条映射到真实认证/授权执行点 |
| SEC-04-C | 工程项 | 建立 IDOR、跨用户读写、管理员权限和匿名访问测试 | 未授权访问被拒绝；跨用户资源访问不可成功 |
| SEC-01/02/03-V | 验证项 | 由非实现者执行专项安全复核 | 形成独立验证记录；不得由实现方自签 |
| QA-GATE-01 | 工程项 | 修正 BLOCKED 与退出码/CI 状态映射 | **R1.2：已实现** gate.py 安全缺失→FAIL/exit 1 + CI Enforce security_scan PASS；待独立验证 |

### 11.2 P2：稳定性与产品验收

| 任务 | 类型 | 主要工作 | 完成条件 |
|---|---|---|---|
| TEST-01 | 工程项 | Browser Worker 启动/连接超时、取消、子进程回收、失败重试 | 不可启动 Chromium 时在限定时间退出，不遗留 worker |
| QA-02 | 工程项 | 建立招聘核心业务 E2E Harness | 核心闭环、重启持久化和失败路径通过 |
| QA-03 | 验证项 | Windows 实机安装/升级/卸载/恢复 | 9 项清单逐项有原始证据与人工签署 |
| SEC-05 | 验证项 | 重新执行依赖安全扫描，复核 advisory | 输出绑定当前锁文件 SHA 与扫描时间 |
| GOV-01 | 管理项 | GitHub Branch Protection / Rulesets 实际配置 | Required Review/Status Check/Tag 保护均有仓库设置证据 |
| DOC-05 | 工程项 | 强化测试 Provenance | 报告可重建受验代码状态与执行输入 |

### 11.3 P3：持续架构治理

- 按业务模块渐进式迁移 IPC，不追求无收益的全面迁移。
- 增加 Crate 依赖方向和架构边界自动检查。
- 建立统一运行时诊断、Sidecar 健康状态和故障关联 ID。
- 完善 API 契约、测试用例、实现文件和验收证据之间的追溯矩阵。

## 12. 实施路线

| 阶段 | 目标 | 交付物 | 退出条件 |
|---|---|---|---|
| Stage 0 | 基线与任务确认 | 已批准审计报告、去重后的缺陷台账 | 每项问题有唯一 ID、责任人和状态 |
| Stage 1 | 安全边界收敛 | SEC-04 Local/Remote 策略、认证授权设计与实现任务 | 远程暴露风险被阻断 |
| Stage 2 | Gate 加固 | Required Gate、Release Gate、CI 状态映射 | BLOCKED 不会误报发布通过 |
| Stage 3 | Browser 与 E2E | Browser 生命周期修复、招聘闭环 E2E | 关键业务回归可重复执行 |
| Stage 4 | Windows 产品验收 | 安装/升级/卸载/恢复原始证据 | Windows 验收清单完成 |
| Stage 5 | 独立验证与发布评审 | SEC-01～04 验证报告、发布评审记录 | 所有发布阻断项关闭或经负责人书面接受 |

## 13. 验收标准

本报告及后续整改至少满足以下条件：

### 13.1 审计报告验收

- [x] 审计基线 HEAD 与 GitHub main 一致（**R1.2 校正：** `ccf7408`，与 GitHub main 及本地 main 一致，ahead=0）。
- [x] 所有结论标注 E1/E2/E3 证据等级。
- [x] 当前实现、历史执行记录和规划目标严格区分。
- [x] 每项问题有唯一编号、等级、优先级和验收标准。
- [ ] 与既有缺陷台账完成去重和状态映射。
- [x] 项目负责人完成评审（评审人 abc / 2026-11-23 / 同意），文档状态由 Draft 更新为 Approved。

### 13.2 P1 验收

- [ ] SEC-04 未完成前，非回环/远程业务 API 不可在无认证授权条件下开放。
- [ ] 多用户 API 认证矩阵与实际中间件、资源授权代码一致。
- [ ] IDOR 和越权测试覆盖读、写、删除、分享、管理员操作。
- [ ] SEC-01/02/03 由独立验证者完成复核。
- [ ] Required Gate 对 BLOCKED/FAIL 采取正确阻断行为。

### 13.3 P2 验收

- [ ] Browser Worker 启动、连接、超时、取消和崩溃恢复测试通过。
- [ ] 招聘核心 E2E 闭环通过。
- [ ] Windows 实机安装/升级/卸载/恢复有完整证据。
- [ ] 依赖扫描绑定当前 Cargo.lock。
- [ ] GitHub 分支保护与 Tag Ruleset 已真实生效。
- [ ] 测试报告包含 HEAD、Dirty State、Patch Hash 和执行脚本摘要。

## 14. 审计结论

### 14.1 总体结论

**RICA 当前架构基础可以保留，近期不建议进行大规模架构重写；项目仍不具备完整产品发布验收条件。**

当前最重要的工程工作不是继续拆分 Crate，而是：

> **R1.2 基线校正说明：** 下列 5 项中，第 2 项（Gate BLOCKED 语义）已在 `ccf7408` 由 SPEC-SCRIPTS 卡修复（gate.py 缺 cargo-audit→FAIL/exit 1 + CI merge blocker），第 1 项的非回环 fail-fast 部分已由 SPEC-BAS002 卡实现；其余（SEC-04 多用户授权、Browser/E2E、Windows 实机、独立安全验证）仍未完成，结论不变。

1. 收敛 Local-only 与未来多用户/远程模式的安全边界。
2. 将 Gate 的 BLOCKED 语义落实为真正有效的 CI/发布阻断机制。
3. 修复 Browser Worker 生命周期问题并建立招聘核心 E2E。
4. 完成 Windows 实机交付验收。
5. 保证安全整改由独立验证者复核，不以实现报告替代安全验证。

### 14.2 发布判断

| 发布模式 | 当前判断 |
|---|---|
| 本机 Local 单用户开发/验证 | 可继续，需保持回环监听和本机信任边界 |
| 局域网访问 | 不允许在 SEC-04 完成前开放 |
| 公网/远程访问 | 不允许在 SEC-04 完成前开放 |
| 多用户/租户模式 | 不具备发布条件（负责人已裁决建设完整多用户认证授权，实现仍待立卡；见 §14.3 第 3 项） |
| Windows 正式安装包 | 缺少实机验收证据，暂不批准正式发布 |

### 14.3 待裁决项与负责人裁决记录

原始待裁决项（5 项）与负责人裁决状态：

1. RICA 当前产品阶段是否正式限定为 Local-only 单用户桌面应用。—— **待负责人明确**（本次未裁决）。
2. 多用户/远程模式是否属于近期产品范围，还是后续独立阶段。—— **待负责人明确**（本次未裁决）。
3. SEC-04 的目标是永久保持 Local-only，还是建设完整多用户认证授权。—— **已裁决：建设完整多用户认证授权**（负责人原话：“建完整多用户认证”）。
4. Release Gate 中 Windows 实机、独立安全验证和 E2E 是否全部设为硬性发布门禁。—— **裁决推迟**（负责人原话：“Windows 实机 / 独立安全验证 / E2E 是否设为硬性发布门禁 推迟”）；当前不作硬性门禁设定，待后续明确。
5. 是否接受 JWT 单密钥轮换导致旧 Token 立即失效的策略。—— **已裁决：接受**（负责人原话：“JWT 单密钥轮换导致旧 Token 立即失效接受”）。

> 裁决记录说明：以上第 3、4、5 项为负责人于 2026-11-23 明确给出的方向，按原话录入，不代为推导未裁决的第 1、2 项。
> 第 3 项裁决确立了 SEC-04-B/C（多用户认证授权、IDOR 测试）的产品方向，但其具体实现仍须按工程流程立卡、审批、验收，
> 本记录不等于“已实现”或“已验证”。第 4 项推迟意味着 Windows 实机 / 独立安全验证 / E2E 暂不作为硬性发布门禁，
> 相关缺口仍如实保留在 §7 与 §11 中。第 5 项接受意味着单密钥轮换后旧 Token 失效为已接受的会话策略，无需再作为待裁决项。
> 本报告已于 2026-11-23 由负责人 abc 评审并结论“同意”（见 §14.4），状态为 Approved。第 1、2 项裁决仍待明确，按 §14.3 记录保留。

### 14.4 负责人评审记录

| 项 | 值 |
|---|---|
| 评审人 | abc |
| 评审日期 | 2026-11-23 |
| 结论 | 同意 |
| 评审对象 | RICA 全量工程实现复审报告 R1.2（基线 `ccf7408`） |

> 说明：署名、日期与结论由负责人明确给出，按原值录入；结论“同意”照原文记录，不代为扩展为其它口径。

### 14.5 AI 辅助内容状态

本报告由 AI 辅助编制，已经负责人 abc 于 2026-11-23 评审并结论“同意”（见 §14.4），状态为 Approved。源码事实以指定 HEAD 的仓库文件为准；历史测试数字以原测试报告为准。本报告是工程审计报告，不是独立安全验证报告或正式安全认证结论；SEC-01～04 等安全项的 Verified 仍需按 §11 由非实现者独立完成。

## 15. 修订记录

| 版本 | 日期 | 修订内容 | 状态 |
|---|---|---|---|
| R1.0 | 2026-11-22 | 初始全量工程实现审计，建立 SEC/QA/REL/DOC/GOV 缺陷基线 | Superseded |
| R1.1 | 2026-11-23 | 基于 main `266d26e` 复核 ARCH-P6 与 SEC-01～04 整改状态；区分实现完成、独立验证和产品验收；补充 Gate、Provenance、Windows 交付风险 | Superseded |
| R1.2 | 2026-11-23 | 基线校正至 `ccf7408`：重标 QA-GATE-01（已修复）、SEC-04-A（非回环 fail-fast 已实现）、SEC-05（工具链已接入）三项；其余缺口不变；证据分级与发布判断不变 | Approved |
| R1.2 批准 | 2026-11-23 | 负责人 abc 评审，结论“同意”；录入 §14.3 三项裁决与 §14.4 评审记录 | Approved |
| R1.2 补记 | 2026-11-23 | §14.3 录入负责人三项裁决：建设完整多用户认证授权（第 3 项）、JWT 单密钥轮换旧 Token 失效接受（第 5 项）、Windows 实机/独立安全验证/E2E 硬性发布门禁裁决推迟（第 4 项）；第 1、2 项待明确；状态仍 Draft | Draft — 待项目负责人评审 |
