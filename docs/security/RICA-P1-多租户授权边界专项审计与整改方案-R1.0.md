# RICA P1 多租户授权边界专项审计与整改方案 R1.0

- 文档编号：RICA-SEC-P1-TENANT-AUTH-R1.0
- 状态：整改方案 / 待评审
- 基准仓库：`BryanTianjx/rica`
- 基准提交：`831f3db1b9a596c9686a795c043f1913eb3cb51b`
- 基准日期：2026-11-25
- 适用范围：Rust API、Application、Infrastructure、Tauri/HTTP 路由、Python Browser Sidecar
- 文档性质：安全专项审计与实施设计；不代表整改已实施或验收通过

---

## 1. 执行摘要

本方案处理 RICA 当前 P1 多租户授权边界问题。上一轮已完成部分资源级 Scope 改造，候选人、会话、消息等业务资源已有 scoped Use Case 和 HTTP 授权测试。本轮不推翻这些成果，重点解决**租户管理员权限与平台级权限混用、跨租户聚合查询、全局数据操作、AI 配置边界，以及 Browser Python Sidecar 未纳入统一授权闭环**的问题。

核心原则：

1. **默认拒绝（deny by default）**：没有明确授权规则的操作不得执行。
2. **租户边界由服务端上下文确定**：不得信任请求体、Query、Header 或前端传入的 `tenant_id` 作为授权依据。
3. **角色与资源 Scope 双重校验**：角色允许不代表资源归属允许；Scope 允许不代表角色具备操作权限。
4. **平台权限与租户权限分离**：租户管理员不能因角色字符串为 `admin` 而获得实例级权限。
5. **所有入口统一策略**：HTTP、Tauri IPC、Python Sidecar 和后台任务必须使用一致的授权语义。
6. **可审计**：拒绝事件、平台级操作、备份恢复、导入导出和 AI 凭据变更必须留下结构化审计记录。
7. **不以隐藏 UI 代替后端授权**：所有安全约束必须在可信服务端执行。

### 1.1 当前审计判断

| 领域 | 当前观察 | 本方案处理 |
|---|---|---|
| 普通业务资源 Scope | Candidate、Conversation、Message 等已有 scoped 调用与部分负向测试 | 保持并补齐覆盖 |
| Dashboard | 已有跨租户聚合查询路径 | 改为租户范围聚合；平台聚合独立授权 |
| Audit | 当前审计事件查询存在全局查询语义 | 增加 tenant filter 与平台审计权限 |
| Backup / Restore | 当前备份操作面向 installation-wide 数据 | 明确平台级操作；规划租户级备份边界 |
| Export / Import | 当前存在全量快照导入导出路径 | 分离平台快照与租户数据包 |
| AI 配置 | 全局模型/路由/预算/凭据与租户业务配置边界待明确 | 分层配置与 Secret 权限 |
| Browser Sidecar | `/api/browser/v1/*` 由 Python Backend 处理，Rust JWT Middleware 不自动覆盖 | 建立受信身份委托和资源授权 |
| Token 生命周期 | JWT Claims 构建后账号状态、角色变更即时生效机制需核实 | 定义撤销/版本校验策略 |

本方案中的“当前观察”来自基准提交的远程静态审查和仓库归档证据；未执行最新 HEAD 的本地动态测试。所有整改项必须经代码与测试验证后才能关闭。

---

## 2. 安全目标与威胁模型

### 2.1 保护对象

- Tenant、Tenant Membership、User、Role / Permission
- Candidate、Position、Conversation、Message、Draft、Follow-up、Send Record
- Import Session、OCR / Extraction Result、Share
- Dashboard 指标、Audit Event、AI Usage / Budget
- AI Provider、Model、Routing、API Key / Secret
- Backup、Restore、Export Package、Import Package
- Browser Session、Browser Task、Browser Permission、Browser Extraction
- Sidecar 与 Rust Backend 之间的内部身份凭据

### 2.2 主要威胁场景

| 威胁编号 | 场景 | 安全影响 |
|---|---|---|
| TH-01 | Tenant A 管理员调用全局 Dashboard | 获取 Tenant B 业务统计或活动信息 |
| TH-02 | Tenant A 管理员读取全局 Audit | 暴露其他租户用户行为和敏感元数据 |
| TH-03 | 租户管理员创建/下载/恢复全局备份 | 跨租户数据泄露或破坏 |
| TH-04 | 租户管理员导出全量快照 | 批量泄露其他租户数据 |
| TH-05 | 租户管理员导入全局快照 | 覆盖或污染其他租户数据 |
| TH-06 | 普通成员读取/修改全局 AI 配置或 Secret | 凭据泄露、路由篡改、费用风险 |
| TH-07 | Browser Sidecar 接受伪造 tenant/user 参数 | 跨租户 Browser Session / Extraction 访问 |
| TH-08 | 账号禁用或角色降级后旧 JWT 仍有效 | 权限撤销延迟 |
| TH-09 | 直接访问内部 Sidecar 端口绕过 Rust Gateway | 绕过统一认证和授权 |
| TH-10 | 后台任务使用无租户约束的系统身份 | 任务越权处理其他租户数据 |

### 2.3 不在本方案范围内

- 不重新设计整个登录系统或替换 JWT 算法。
- 不引入新的身份供应商。
- 不改变 RICA 的 Local-first / Offline-first 产品定位。
- 不将 Browser Sidecar 改写为 Rust。
- 不在本轮实施中自动执行代码修改、数据库迁移或发布。
- 不默认支持任意跨租户共享；已有 Share 能力应继续执行显式授权策略。

---

## 3. 统一身份与权限模型

### 3.1 权限主体

建议将当前可能混用的 `admin/member` 角色提升为明确的权限主体：

| 主体 | 定义 | 权限边界 |
|---|---|---|
| `platform_admin` | 安装实例级管理员 | 可执行明确列出的实例级管理操作 |
| `tenant_admin` | 单一租户管理员 | 仅管理当前授权 Tenant |
| `tenant_member` | 单一租户普通成员 | 按资源 Owner / Tenant Scope 执行业务操作 |
| `service_identity` | 后台任务或服务间身份 | 仅具备显式声明的服务权限和租户上下文 |
| `anonymous` | 未认证请求 | 仅允许公开健康检查等白名单接口 |

兼容迁移要求：

- 不允许仅根据旧字段 `role == "admin"` 推断 `platform_admin`。
- 旧 `admin` 的迁移映射必须由部署策略明确决定；默认映射为 `tenant_admin`，不得隐式提升平台权限。
- `platform_admin` 必须来自可信数据库记录或签名可信的权限声明。
- Tenant membership 必须处于有效状态。

### 3.2 权限判定模型

一次请求必须同时满足：

```text
Authenticated
  AND PrincipalActive
  AND PermissionGranted
  AND TenantMembershipValid (tenant operation)
  AND ResourceTenantMatches
  AND ResourceOwnerPolicySatisfied (when applicable)
  AND ResourceStateAllowsOperation
```

平台级操作判定：

```text
Authenticated
  AND PrincipalActive
  AND PlatformPermissionGranted
  AND ExplicitPlatformOperation
  AND AuditRequired
```

任何一项缺失均拒绝访问。授权失败统一返回稳定错误语义，避免通过错误差异泄露跨租户资源是否存在。

### 3.3 RequestContext 建议

建议在现有 `RequestContext` 基础上明确以下字段或等价强类型：

| 字段 | 类型建议 | 说明 |
|---|---|---|
| `actor_id` | UserId / ServiceId | 可信身份主体 |
| `principal_kind` | enum | User / Service |
| `tenant_id` | Option<TenantId> | 当前有效租户上下文 |
| `tenant_role` | Option<TenantRole> | 当前租户角色 |
| `platform_permissions` | PermissionSet | 平台权限集合 |
| `membership_version` | version / epoch | 权限变更或撤销版本 |
| `authn_method` | enum | JWT / Desktop Session / Internal Service |
| `request_id` | RequestId | 审计关联 ID |

不要继续用可任意比较的字符串角色承担全部授权语义。推荐将角色映射为权限集合，再由 policy 函数判定。

---

## 4. 权限矩阵

### 4.1 主体权限总表

符号：
- **允许**：在当前主体的授权范围内允许。
- **拒绝**：默认拒绝。
- **条件允许**：必须满足本表后的资源 Scope / 二次确认 / 审计要求。
- **仅平台**：仅 `platform_admin` 或专用平台服务身份可执行。

| 操作域 | tenant_member | tenant_admin | platform_admin | service_identity |
|---|---|---|---|---|
| 本租户 Candidate / Position 读取 | 条件允许 | 允许 | 条件允许 | 按任务 Scope |
| 本租户业务资源写入 | 条件允许 | 允许 | 条件允许 | 按任务 Scope |
| 其他租户业务资源 | 拒绝 | 拒绝 | 条件允许 | 默认拒绝 |
| 本租户 Dashboard | 条件允许 | 允许 | 条件允许 | 按任务 Scope |
| 全局 Dashboard | 拒绝 | 拒绝 | 允许 | 专用聚合权限 |
| 本租户 Audit | 按策略只读自身/业务事件 | 允许 | 条件允许 | 按任务 Scope |
| 全局 Audit | 拒绝 | 拒绝 | 允许 | 专用审计导出权限 |
| 全局 Backup / Restore | 拒绝 | 拒绝 | 允许 | 专用备份身份 |
| 租户级 Backup / Restore | 拒绝或受限 | 条件允许 | 条件允许 | 专用备份身份 |
| 本租户 Export | 条件允许 | 允许 | 条件允许 | 按任务 Scope |
| 全局 Export / Import | 拒绝 | 拒绝 | 允许 | 专用迁移身份 |
| 租户级 Import | 拒绝或受限 | 条件允许 | 条件允许 | 专用导入身份 |
| 全局 AI Provider / Secret | 拒绝 | 拒绝 | 允许 | Secret 管理专用身份 |
| 本租户 AI 配置 | 条件允许 | 条件允许 | 条件允许 | 按任务 Scope |
| AI Usage / Budget | 本租户只读自身 | 本租户管理 | 全局管理 | 专用计量身份 |
| Browser Session | 自身 Session | 本租户授权范围 | 按审计/运维权限 | 绑定任务身份 |
| Browser Permission | 自身受限能力 | 本租户策略范围 | 平台策略管理 | 最小能力授权 |
| Browser Extraction | 资源 Owner 范围 | 本租户范围 | 特殊审计权限 | 绑定 Candidate / Tenant |

### 4.2 关键规则

1. Tenant Admin 不是 Platform Admin。
2. `platform_admin` 也不能绕过审计和高风险操作确认。
3. 对 Candidate、Conversation、Draft 等业务对象继续执行 tenant + owner 双重 Scope。
4. 仅有 tenant_id 不足以访问资源，必须校验 membership 和 resource ownership。
5. 跨租户 Share 必须使用显式 Share Grant，不得扩大默认 Scope。
6. Service Identity 不得使用通用超级管理员上下文。
7. Browser Worker 不得自行决定或提升调用者 tenant / role。

---

## 5. 六大领域专项整改

## 5.1 Dashboard：租户聚合与平台聚合分离

### 当前问题

基准提交中 Dashboard 路径存在跨租户聚合语义，HTTP 处理器执行 `AdminOperation` 后调用 dashboard snapshot。若 `admin` 仅表示租户管理员，则可能形成跨租户统计越权。

### 目标设计

提供两个明确接口或等价 Use Case：

- `get_tenant_dashboard(ctx)`：只聚合 `ctx.tenant_id` 范围内数据。
- `get_platform_dashboard(ctx)`：要求 `PlatformPermission::DashboardReadAll`。

租户 Dashboard 的所有 Repository 查询必须显式接收 Tenant Scope。不能先查询全量数据，再在 API 层过滤。

### 建议修改位置

| 路径 | 修改内容 |
|---|---|
| `crates/ica-api/src/routes.rs` | 拆分 tenant / platform Dashboard Handler；明确权限检查 |
| `crates/ica-application/src/dashboard.rs` | 新增 TenantDashboardQuery / PlatformDashboardQuery |
| `crates/ica-application/src/request_context.rs` | 提供强类型 tenant / platform 权限访问器 |
| `crates/ica-infra/src/*dashboard*` 或实际 Dashboard Repository | 所有 SQL 增加 tenant_id 条件 |
| `crates/ica-api/tests/sec04_bc_authorization_http.rs` | 增加跨租户聚合负向测试 |

### 负向测试

- Tenant A Admin 请求 tenant Dashboard，不得包含 Tenant B 的数量、名称、状态或活动信息。
- Tenant A Admin 请求 platform Dashboard，返回 403。
- Member 请求平台聚合接口，返回 403。
- 缺少 tenant context 的 tenant Dashboard 请求，返回 401/403，不允许回退到全局查询。
- 使用伪造 Query/Header tenant_id 请求 Tenant B，仍然只返回 Token / Session 确定的 Tenant A 数据。
- Platform Admin 可以读取全局聚合，但操作必须记录 Audit Event。

### 验收标准

- tenant Dashboard 每条 SQL 查询均具备 tenant predicate 或经过可证明的 tenant-scoped Repository。
- 集成测试使用两个租户的独立数据集验证返回集合。
- 全局接口只允许显式平台权限。
- 不存在“授权失败后 fallback 到全局 snapshot”的代码路径。

---

## 5.2 Audit：租户审计与平台审计分离

### 当前问题

当前 Audit Events 查询路径存在全局审计记录查询语义。审计记录可能包含其他租户用户、资源标识、操作类型和时间信息。

### 目标设计

Audit Event 必须包含：

- `tenant_id`（平台级事件可为空，但必须标记 scope）
- `actor_id`
- `action`
- `resource_type`
- `resource_id`
- `scope_kind`（tenant / platform）
- `request_id`
- `occurred_at`
- `outcome`

提供：

- `list_tenant_audit_events(ctx, filter)`
- `list_platform_audit_events(ctx, filter)`

租户管理员只可查看本租户事件。普通成员是否可查看自身审计记录应作为独立产品策略，默认只允许自身相关且脱敏的事件。

### 建议修改位置

| 路径 | 修改内容 |
|---|---|
| `crates/ica-api/src/routes.rs` | 拆分租户审计与平台审计 Handler |
| `crates/ica-application/src/audit_trail.rs` | 增加 tenant / platform 查询 Use Case |
| `crates/ica-infra/src/audit.rs` 或实际 Audit Repository | 查询强制 tenant filter；平台查询显式授权 |
| Audit Event Domain Model / Migration | 增加 scope_kind、tenant_id、request_id 等缺失字段 |
| `crates/ica-api/tests/sec04_bc_authorization_http.rs` | 增加审计跨租户隔离和平台访问测试 |

### 负向测试

- Tenant A Admin 无法读取 Tenant B Audit Event。
- 通过 resource_id、actor_id、时间范围或分页参数均无法绕过 tenant filter。
- Member 无权读取本租户全量管理审计。
- Tenant A Admin 请求平台审计接口返回 403。
- Platform Admin 的全局审计查询记录自身访问行为。
- 审计接口不得返回 API Key、Authorization Header、Cookie、Secret 或敏感正文。

### 验收标准

- 所有租户审计查询在 Repository 层强制 tenant_id。
- 平台事件与租户事件在数据模型中可区分。
- 不能由客户端传入 scope_kind 决定查询范围。
- 敏感字段脱敏测试通过。
- 审计查询失败和授权拒绝也可关联 request_id。

---

## 5.3 Backup / Restore：明确平台级与租户级边界

### 当前问题

当前备份、恢复路径面向 installation-wide 数据库。仅使用 Tenant Admin 的 `AdminOperation` 角色门禁不足以保护全局备份和恢复。

### 目标设计

R1.0 推荐先采用**平台级备份策略**：

- 全局 Backup / Restore 仅 `platform_admin`。
- Tenant Admin 不可列出、下载、创建或恢复全局备份。
- 备份文件下载采用短时、一次性或受控授权。
- Restore 需要二次确认、审计记录和恢复前快照。
- 恢复操作必须禁止普通业务请求并发写入，或具备一致性快照机制。

租户级备份作为后续能力单独设计。不能简单把全局 SQLite 文件复制后称为 tenant backup，因为数据库文件可能包含多个租户和系统级 Secret。

### 建议修改位置

| 路径 | 修改内容 |
|---|---|
| `crates/ica-api/src/routes.rs` | Backup / Restore Handler 改为 Platform Permission |
| `crates/ica-application/src/backup.rs` | 引入 BackupCreate / BackupRead / Restore 权限检查和二次确认 |
| `crates/ica-infra/src/snapshots.rs` | 校验备份元数据、完整性、版本和加密状态 |
| `crates/ica-domain` 备份模型 | 定义 backup scope、creator、checksum、created_at、schema version |
| `crates/ica-api/tests` | 增加租户管理员拒绝与 Restore 安全测试 |

### 负向测试

- Tenant Admin 访问 Backup List / Create / Download / Restore 全部拒绝。
- Member 访问上述接口全部拒绝。
- 伪造 backup_id 不得读取其他备份。
- Restore 请求缺少二次确认凭证或操作确认，不得执行。
- 恢复失败不得破坏当前数据库。
- 非法、损坏、版本不兼容或校验和不匹配的备份必须拒绝恢复。
- 恢复操作必须产生不可由同一业务接口删除的审计记录。

### 验收标准

- 全局 Backup / Restore 全部要求平台级权限。
- 备份元数据包含 scope、版本、校验和、加密状态、创建主体。
- Restore 具备恢复前快照、原子性/失败回滚策略和运行维护状态。
- Tenant Admin 无法通过其他下载、文件路径或 Tauri IPC 入口取得全局备份。

---

## 5.4 Export / Import：拆分全局迁移与租户数据包

### 当前问题

当前存在全量数据快照导出与导入路径。若租户管理员可以调用，则可能造成跨租户数据泄露或全局数据库污染。

### 目标设计

区分两种不同产品能力：

**A. Platform Snapshot**
- 包含安装实例全量数据。
- 仅平台级管理员或专用迁移服务身份。
- 不允许通过 Tenant Admin 的通用 Import / Export 权限访问。

**B. Tenant Data Package**
- 只包含单一 Tenant 的白名单业务实体。
- 不包含其他 Tenant、平台配置、全局 Secret、系统认证数据。
- 导入时强制绑定目标 Tenant。
- 外部 ID 映射为本地新 ID，防止覆盖其他租户资源。
- 所有导入对象必须经过 schema validation 和授权检查。

### 建议修改位置

| 路径 | 修改内容 |
|---|---|
| `crates/ica-api/src/routes.rs` | 区分 platform snapshot 与 tenant package endpoints |
| `crates/ica-application` 导入导出 Use Case | 增加 ExportScope / ImportScope 强类型参数 |
| `crates/ica-infra` Snapshot / Import Repository | 查询和写入强制 tenant filter |
| `crates/ica-contracts` | 定义 TenantPackage Schema、版本和白名单实体 |
| `crates/ica-api/tests` | 增加跨租户导入导出负向测试 |

### 负向测试

- Tenant A Admin 导出数据包不得包含 Tenant B 的任何记录。
- Tenant A Admin 无权导出 Platform Snapshot。
- Tenant A 的导入包中伪造 Tenant B ID，导入必须拒绝或按明确映射策略重新生成 ID。
- 导入包携带 platform settings、AI Secret、用户密码摘要等字段必须拒绝。
- Tenant A Import 不得修改 Tenant B 的记录。
- 重放同一 Import Package 不得重复创建或覆盖非授权资源。
- 未授权用户不得确认、取消或读取其他租户 Import Session。

### 验收标准

- Tenant Package 的实体白名单、字段白名单和关系约束有版本化契约。
- 导入导出在 Repository 层实施 tenant scope。
- Platform Snapshot 单独授权、单独审计。
- 双租户隔离测试覆盖导出内容、导入结果和失败回滚。
- 对导出文件实施安全存储、过期和删除策略。

---

## 5.5 AI 配置：全局 Provider、租户配置与 Secret 分层

### 当前问题

AI 配置包含多个安全等级不同的对象：Provider、Model Catalog、Routing、Budget、Usage、API Key / Secret。不能把它们统一视为普通 Settings。

### 目标设计

配置划分为三层：

| 层级 | 内容 | 管理权限 |
|---|---|---|
| Platform AI Settings | Provider 接入、全局 Model Catalog、全局 Egress Policy、Secret | Platform Admin |
| Tenant AI Settings | 租户允许的模型、默认路由、预算上限、功能开关 | Tenant Admin，受平台策略约束 |
| User AI Preferences | 用户默认模型、非敏感展示偏好 | User 自身 |

安全约束：

- Secret 只能写入受保护的 Secret Store，不应在普通 Settings API 返回明文。
- 读取 Secret 状态仅返回 `configured=true/false`、版本或脱敏标识。
- Tenant Admin 不得修改平台 Provider Secret 或全局 Egress Policy。
- Tenant 配置不能扩大平台允许的模型、域名、数据出站策略。
- AI Usage / Budget 查询必须 tenant scoped。
- 费用归属必须使用服务端确定的 tenant_id，不信任客户端提交值。

### 建议修改位置

| 路径 | 修改内容 |
|---|---|
| `crates/ica-api/src/routes.rs` | 分离 Platform AI / Tenant AI / User Preferences Handler |
| `crates/ica-application/src/settings.rs` | 拆分设置 Use Case 和权限策略 |
| `crates/ica-application` AI routing / usage / budget 模块 | 强制 Tenant Scope 与 Platform Policy |
| `crates/ica-infra` AI Settings / Secret Repository | Secret 加密存储、禁止明文回读 |
| `crates/ica-contracts` | 分离配置 DTO，禁止跨层复用全局 Settings DTO |
| `crates/ica-api/tests` | 增加 Secret、路由和预算跨租户测试 |

### 负向测试

- Tenant Admin 调用 Platform Provider / Secret 更新接口返回 403。
- Member 不得读取或更新 Tenant AI Settings。
- 任意角色读取 Secret API 均不返回明文。
- Tenant A 修改模型路由不得影响 Tenant B。
- Tenant A 的预算查询不得包含 Tenant B Usage。
- Tenant A 不能选择平台禁用模型或违反平台 Egress Policy 的目标。
- 请求体伪造 tenant_id 不得修改其他租户配置或费用归属。
- 平台 Secret 轮换后，旧 Secret 不再被新请求使用。

### 验收标准

- Platform / Tenant / User 三层配置在 Domain、Application、Repository 和 DTO 中可区分。
- Secret 不经过普通 Settings Read API。
- AI Usage、Budget、Routing 具备双租户隔离测试。
- 平台策略对租户配置具有明确的上限约束。
- Secret 访问、变更、轮换和失败事件均有审计记录。

---

## 5.6 Browser Python Sidecar：统一身份委托与资源授权

### 当前问题

`/api/browser/v1/*` 路由由 Python Backend 处理，Rust HTTP JWT Middleware 不会自动覆盖 Sidecar 请求。因此需要证明 Python Sidecar 自己完成认证、授权和 Scope 检查，并确保内部端口不可被不可信客户端直接访问。

### 目标架构

```text
Client (React / Tauri)
        |
        | User Access Token
        v
Rust Gateway / Trusted Router
        |
        | 1. Verify User Token
        | 2. Resolve active user + tenant membership
        | 3. Authorize Browser capability
        v
Internal Sidecar Channel
        |
        | Signed / mTLS / Unix Socket identity assertion
        | actor_id, tenant_id, permissions, request_id, expiry
        v
Python Browser Sidecar
        |
        | Verify trusted assertion
        | Bind Session to actor + tenant
        | Resource-level authorization
        v
Browser Runtime / CDP / Extraction
```

重要要求：

- Sidecar 不得直接信任客户端传入的 `actor_id`、`tenant_id`、`role`。
- 内部身份断言必须具备签名或等价的受信通道保护、过期时间、受众和防重放机制。
- Python Sidecar 监听地址不得暴露到不可信网络；优先 loopback / Unix Domain Socket / 受控内部通道。
- 即使绕过 React UI 直接调用 Sidecar，也必须执行认证。
- Browser Session、Task、Permission、Extraction 都必须绑定 actor 与 tenant。
- CDP Worker 不应获得数据库连接或不必要的业务权限。
- 任务执行期间 tenant membership 被撤销时，应定义取消或拒绝后续操作的策略。

### 建议修改位置

| 路径 | 修改内容 |
|---|---|
| `crates/ica-desktop/src/routing.rs` | 审查并收敛 Browser 路由代理、内部端口和请求头传递 |
| `crates/ica-api/src/auth.rs` | 复用统一身份验证与上下文解析，不向 Sidecar 传递原始不可信 Claims |
| `crates/ica-application` Browser Use Cases / Ports | Browser 操作前执行 capability + resource scope 授权 |
| Python Sidecar API Router / Auth Middleware（实际路径待代码定位） | 验证内部身份断言，拒绝直连匿名请求 |
| Python Browser Session / Permission / Extraction Repository（实际路径待代码定位） | 所有查询和更新强制 actor / tenant 约束 |
| Sidecar Contract | 定义内部身份断言 Schema、签名、受众、过期和 request_id |
| `crates/ica-api/tests` 与 Python Sidecar tests | 增加跨服务授权和直连绕过测试 |

注：本轮未取得足够的 Python Sidecar 全量源码证据，因此 Python 文件路径属于待实施阶段定位项，不应根据本表虚构具体模块名。实施前必须先读取仓库当前目录树并确定真实路径。

### 负向测试

1. 不带内部身份断言直接请求 Sidecar，返回 401。
2. 伪造或篡改签名的断言，返回 401。
3. 过期断言、错误 audience、错误 issuer、重复 nonce 按策略拒绝。
4. Tenant A 的身份访问 Tenant B Browser Session，返回 403/404。
5. Tenant A 修改请求体中的 tenant_id 为 Tenant B，仍不能跨租户访问。
6. 普通 Member 不能修改超出授权范围的 Browser Permission。
7. Extraction Result 不能关联到其他 Tenant Candidate。
8. 直接访问 Sidecar 内部端口不得绕过 Rust Gateway。
9. Worker 崩溃、重启或超时后，不得遗留可被其他用户复用的 Session。
10. 审计日志能够关联 Rust request_id、Sidecar request_id、Browser Session 和 actor。

### 验收标准

- Rust 与 Python 使用同一套主体、租户、权限和资源归属语义。
- Sidecar 的所有外部入口均强制认证。
- 内部身份断言不可由客户端伪造或重放。
- Browser Session / Task / Extraction 的跨租户负向测试全部通过。
- Sidecar 无权直接访问不属于当前任务的租户数据。
- Browser Worker 生命周期、超时和清理测试通过。

---

## 6. JWT 与权限撤销补充要求

本节作为六大领域的共同基础。

### 6.1 当前需要核实的风险

现有 JWT 验证路径在验证 Token 后构建 RequestContext。需确认每次请求是否能够反映账号禁用、Tenant Membership 撤销和角色降级。

### 6.2 推荐方案

按复杂度从低到高选择：

1. 短 TTL Access Token + Refresh Token 撤销。
2. User / Membership 的 `authz_version` 或 `token_version`，每次请求或缓存窗口内校验。
3. 对高风险操作强制实时读取主体状态和平台权限。
4. 账号禁用、移除租户成员或角色降级时递增版本并撤销相关会话。

### 6.3 必测场景

- 用户禁用后旧 Access Token 不得继续访问。
- Tenant Membership 移除后旧 Token 不得访问原 Tenant。
- Tenant Admin 降级为 Member 后旧 Token 不得继续执行管理操作。
- 平台管理员撤销平台权限后旧 Token 不得继续执行平台级操作。
- Refresh Token 轮换后旧 Refresh Token 不得重复使用。

---

## 7. 代码实施任务清单

建议按以下任务拆分 PR。所有任务均以本方案评审通过后的最新主干为实施基线。

| Task | 优先级 | 修改范围 | 交付物 |
|---|---|---|---|
| AUTH-01 | P1 | Domain / RequestContext / Permission | 强类型主体和权限模型 |
| AUTH-02 | P1 | Auth Middleware / JWT | membership、账号状态和权限撤销 |
| AUTH-03 | P1 | Dashboard | Tenant / Platform 查询拆分 |
| AUTH-04 | P1 | Audit | tenant / platform 审计查询拆分 |
| AUTH-05 | P1 | Backup / Restore | 平台级权限与恢复保护 |
| AUTH-06 | P1 | Export / Import | Platform Snapshot 与 Tenant Package 分离 |
| AUTH-07 | P1 | AI Settings / Routing / Usage | 三层配置和 Secret 保护 |
| AUTH-08 | P1 | Rust Gateway / Python Sidecar | 受信身份委托和 Sidecar 授权 |
| AUTH-09 | P1 | HTTP / IPC / Sidecar Tests | 双租户负向测试矩阵 |
| AUTH-10 | P1 | Security Gate / Docs | 权限矩阵、测试证据和门禁更新 |

实施顺序建议：

```text
AUTH-01
  ├── AUTH-02
  ├── AUTH-03
  ├── AUTH-04
  ├── AUTH-05
  ├── AUTH-06
  ├── AUTH-07
  └── AUTH-08
          |
          v
       AUTH-09
          |
          v
       AUTH-10
```

---

## 8. 测试设计与验收矩阵

### 8.1 测试数据基线

所有跨租户测试至少创建：

- Tenant A / Tenant B
- Tenant A Admin / Member
- Tenant B Admin / Member
- Platform Admin
- A、B 各自独立的 Candidate、Conversation、Message、Audit Event、AI Usage、Browser Session
- 至少一个全局配置、全局备份和全局快照

测试不得只使用同一租户下不同 Owner 的数据替代跨租户测试。

### 8.2 必须覆盖的负向测试

| 编号 | 测试场景 | 预期 |
|---|---|---|
| TA-001 | Tenant A Admin 读取 Tenant B Dashboard | 403 或仅 A 范围 |
| TA-002 | Tenant A Admin 读取全局 Dashboard | 403 |
| TA-003 | Tenant A Admin 读取 Tenant B Audit | 403/404 |
| TA-004 | Tenant A Admin 读取平台 Audit | 403 |
| TA-005 | Tenant A Admin 创建/下载全局 Backup | 403 |
| TA-006 | Tenant A Admin Restore 全局 Backup | 403 |
| TA-007 | Tenant A Admin 导出 Platform Snapshot | 403 |
| TA-008 | Tenant A Admin 导出 Tenant Package | 仅 A 数据 |
| TA-009 | Tenant A Import 修改 Tenant B 记录 | 拒绝且无副作用 |
| TA-010 | Tenant Admin 修改 Platform AI Secret | 403 |
| TA-011 | Member 读取 Tenant AI 配置 | 按策略拒绝 |
| TA-012 | Tenant A 读取 Tenant B AI Usage | 403/空结果，不泄露存在性 |
| TA-013 | 匿名直连 Browser Sidecar | 401 |
| TA-014 | 伪造 Sidecar 身份断言 | 401 |
| TA-015 | Tenant A 访问 Tenant B Browser Session | 403/404 |
| TA-016 | 伪造 tenant_id 请求 | 服务端 Scope 不变 |
| TA-017 | 禁用账号使用旧 JWT | 401 |
| TA-018 | 降级角色使用旧 JWT 执行管理操作 | 403 |
| TA-019 | 移除 Membership 后访问原租户 | 403 |
| TA-020 | Platform Admin 执行全局操作 | 允许且产生 Audit Event |

### 8.3 自动化层级

| 层级 | 主要验证 | 必须执行 |
|---|---|---|
| Domain Unit | Permission / Policy 判定 | 是 |
| Application Unit | Tenant / Platform Use Case | 是 |
| Repository Integration | SQL tenant filter | 是 |
| HTTP Integration | JWT + Handler + Scope | 是 |
| Sidecar Integration | 内部断言 + Python 授权 | 是 |
| IPC Integration | Tauri Command 授权一致性 | 是 |
| E2E | 登录、租户切换、核心业务、审计 | 是 |
| Security Regression | 参数篡改、越权、重放、直连 | 是 |

### 8.4 统一验收门槛

以下全部满足才能关闭本专项：

- [ ] 权限矩阵经产品与安全评审确认。
- [ ] Tenant Admin 与 Platform Admin 在代码和数据模型中明确区分。
- [ ] 六大领域均完成授权检查与资源 Scope。
- [ ] HTTP、IPC、Sidecar 不存在授权绕过入口。
- [ ] TA-001 至 TA-020 全部通过。
- [ ] 关键 Repository 查询具备 tenant filter 证据。
- [ ] 所有高风险平台操作产生结构化审计记录。
- [ ] Secret 不会通过普通配置 API 明文返回。
- [ ] JWT 权限撤销测试通过。
- [ ] Rust、Frontend、Python、Contract 和 E2E Gate 在同一 HEAD 执行。
- [ ] 测试报告记录 commit SHA、环境、命令、退出码和日志路径。
- [ ] 无 P1/P0 未关闭项。

**禁止使用**“单元测试通过”“HTTP 接口返回 403”或“前端隐藏按钮”单独判定多租户安全验收通过。

---

## 9. 数据库迁移与兼容性要求

如需调整 Tenant、Membership、Audit、AI Settings 或 Backup Schema：

1. 先提供 Migration Plan 和回滚策略。
2. Migration 必须可重复执行或具备明确版本保护。
3. 旧数据中无法确认 Tenant 归属的记录不得默认归入任意租户。
4. 旧 `admin` 角色不得自动升级为 `platform_admin`。
5. 备份恢复必须兼容 Schema Version 校验。
6. 迁移前后均执行双租户隔离回归测试。
7. SQLite Migration 不得依赖生产环境中不可用的外部服务。
8. 迁移失败必须保持原数据可恢复。

---

## 10. 安全日志与可观测性

所有授权决策建议记录：

- request_id / trace_id
- actor_id / principal_kind
- tenant_id（若有）
- permission / action
- resource_type / resource_id（脱敏）
- decision（allow / deny）
- reason_code
- entrypoint（HTTP / IPC / Sidecar / Job）
- occurred_at

禁止记录：

- JWT 原文
- API Key / Provider Secret
- Cookie / Authorization Header
- 未脱敏的个人简历、消息正文或敏感业务字段

建议统一拒绝原因码：

| Reason Code | 含义 |
|---|---|
| AUTH_UNAUTHENTICATED | 未认证 |
| AUTH_PRINCIPAL_INACTIVE | 主体已禁用 |
| AUTH_MEMBERSHIP_INACTIVE | 租户成员关系失效 |
| AUTH_PERMISSION_DENIED | 权限不足 |
| AUTH_TENANT_SCOPE_MISMATCH | 租户 Scope 不匹配 |
| AUTH_RESOURCE_OWNER_MISMATCH | 资源 Owner 不匹配 |
| AUTH_PLATFORM_PERMISSION_REQUIRED | 需要平台级权限 |
| AUTH_ASSERTION_INVALID | Sidecar 身份断言无效 |
| AUTH_ASSERTION_EXPIRED | Sidecar 身份断言过期 |

对外 HTTP 错误响应不得暴露资源属于哪个其他 Tenant。

---

## 11. 实施阶段与退出条件

| 阶段 | 内容 | 退出条件 |
|---|---|---|
| S0 设计评审 | 确认主体模型、角色迁移和权限矩阵 | 产品/安全/架构评审通过 |
| S1 基础授权模型 | RequestContext、Permission、Membership、Token Revocation | Domain / Application 测试通过 |
| S2 Rust 资源授权 | Dashboard、Audit、Backup、Import/Export、AI | Repository + HTTP 测试通过 |
| S3 Sidecar 授权 | 内部身份委托、Session / Task / Extraction Scope | Sidecar 集成测试通过 |
| S4 全入口回归 | HTTP、IPC、Sidecar、后台任务 | TA-001 至 TA-020 通过 |
| S5 安全验收 | 全量 Gate、证据归档、文档更新 | 同一 HEAD 全部证据通过 |

任何阶段发现跨租户读取、写入、导出或恢复成功，均应阻断后续安全验收并按 P1/P0 处理。

---

## 12. 风险与设计决策待确认

正式实施前需要评审并冻结以下决策：

| ADR / 决策 | 推荐默认值 | 待确认问题 |
|---|---|---|
| DEC-AUTH-01 角色模型 | Tenant Admin 与 Platform Admin 分离 | 当前安装实例是否只有单平台管理员？ |
| DEC-AUTH-02 Dashboard | Tenant Dashboard 默认按租户过滤 | 是否需要平台运营总览？ |
| DEC-AUTH-03 Audit | Tenant Audit 与 Platform Audit 分离 | Tenant Member 是否可看自身事件？ |
| DEC-AUTH-04 Backup | R1.0 仅平台级全局备份 | 是否需要租户级可移植备份？ |
| DEC-AUTH-05 Import/Export | 平台快照与租户数据包分离 | Tenant Package 的实体白名单？ |
| DEC-AUTH-06 AI Config | Platform / Tenant / User 三层 | Provider Secret 是否只允许平台管理？ |
| DEC-AUTH-07 Sidecar Trust | Rust Gateway 签名身份断言 + 内部受信通道 | 当前 Sidecar 部署方式和端口暴露情况？ |
| DEC-AUTH-08 Token Revocation | 短 TTL + authz_version | 是否接受每请求查询或短缓存？ |

未确认的产品策略不得由实现人员自行推断为更宽松的权限。

---

## 13. 本方案验收声明

本文件是基于基准提交 `831f3db1b9a596c9686a795c043f1913eb3cb51b` 的专项审计与整改设计。它不表示：

- 本方案所列代码已经修改；
- 所有权限缺陷已经修复；
- Python Sidecar 已经完成安全审计；
- TA-001 至 TA-020 已经执行；
- 项目已经通过多租户安全验收。

实施完成后，应新增独立的整改实施记录和安全验收报告，绑定实际完成的 Git Commit SHA、测试结果及证据文件。

## 14. 相关代码与证据入口

以下为本方案基准审查时定位的主要入口；实施时必须重新核对最新路径和行号：

- `crates/ica-api/src/auth.rs` — JWT 验证与身份上下文构建
- `crates/ica-api/src/routes.rs` — Dashboard、Audit、Backup、Import/Export、AI 等 HTTP Handler
- `crates/ica-application/src/request_context.rs` — RequestContext
- `crates/ica-application/src/dashboard.rs` — Dashboard Use Case
- `crates/ica-application/src/audit_trail.rs` — Audit Use Case
- `crates/ica-application/src/backup.rs` — Backup Use Case
- `crates/ica-application/src/settings.rs` — Settings Use Case
- `crates/ica-infra/src/snapshots.rs` — Snapshot / Backup Infrastructure
- `crates/ica-infra/src/audit.rs` — Audit Infrastructure
- `crates/ica-desktop/src/routing.rs` — Browser Sidecar 路由代理
- `crates/ica-api/tests/sec04_bc_authorization_http.rs` — 当前 HTTP 授权集成测试
- `docs/architecture/api-authentication-matrix.md` — 现有认证/授权矩阵（需要同步更新）
- `docs/testing/evidence/gate-post-remediation-20261124/` — 上一轮整改门禁证据

---

**文档结束**
