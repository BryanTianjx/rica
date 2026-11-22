# RICA 总体架构演进设计与 ADR 决策 R1.0

**文档编号：** RICA-ARCH-001  
**版本：** R1.0  
**状态：** Accepted（用户已评审确认，2026-11-22）  
**基准日期：** 2026-11-22  
**适用项目：** RICA  
**目标平台：** Windows 优先，兼容 macOS  
**核心技术栈：** Rust、Tauri 2、React、TypeScript、SQLite、Python Sidecar

---

## 1. 文档目的

本文定义 RICA 当前架构的目标形态、模块边界、运行时结构、通信机制、安全边界及架构决策记录（ADR），为后续工程演进提供统一依据。

本设计重点解决以下问题：

1. HTTP 与 Tauri IPC 双通道并存，但缺少统一的业务执行边界。
2. Tauri Host 承担过多运行时职责，桌面生命周期与业务服务耦合。
3. Rust Server、Application、Domain 和 Infrastructure 的复用边界需要进一步明确。
4. Python Sidecar 缺少统一的进程生命周期、通信协议和错误处理规范。
5. 认证、授权、资源归属与本地桌面安全边界需要进一步收敛。
6. 架构演进需要兼顾当前实现、迁移成本和长期可维护性。

本文遵循以下原则：

- 不为架构整洁而推倒重写。
- 不将模块化单体过早拆分成微服务。
- 不让 Tauri IPC 成为新的业务实现层。
- 不让 HTTP Handler 或 IPC Command 承载核心业务逻辑。
- 不让 Python Sidecar 成为 Rust 核心业务的第二套实现。
- 不自动执行 Git Commit、Push、Merge 或 Release。

## 2. 现状与问题基线

### 2.1 当前技术架构

当前工程采用 Rust Workspace，主要由以下 Crates 组成：

| Crate             | 当前职责                   | 目标职责                  |
| ----------------- | ---------------------- | --------------------- |
| `ica-domain`      | 领域实体、规则和状态             | 核心领域模型与业务规则           |
| `ica-application` | 应用服务、业务用例              | 用例编排、事务边界和授权上下文       |
| `ica-contracts`   | 共享 DTO 和校验             | HTTP/IPC/内部协议契约       |
| `ica-infra`       | SQLx、SQLite、AI、OCR、备份等 | 基础设施适配器               |
| `ica-api`         | Axum API               | HTTP 入站适配器            |
| `ica-desktop`     | 桌面相关能力                 | 桌面应用组合与运行时适配          |
| `ica-server`      | 服务启动与组合                | HTTP Server 组合根及运行时入口 |

桌面端还包含：

- React + TypeScript 前端。
- Tauri 2 Host。
- Rust Server 子进程。
- Python Sidecar。
- HTTP Backend Client。
- IPC Backend Client。

### 2.2 已知实现事实

此前只读审查观察到：

1. 当前 Tauri Host 同时管理 Rust Server 与 Python Sidecar。
2. Tauri 前端同时使用 HTTP 和 IPC。
3. 已注册的 Tauri IPC 命令主要覆盖 Positions CRUD，共 5 个命令。
4. 其他业务模块仍主要通过 HTTP 访问。
5. 前端已经存在 HTTP Backend / IPC Backend 抽象，为渐进式迁移提供基础。
6. Browser 子系统依赖 Python Sidecar。
7. 部分 AI-EXT 能力也依赖 Python Sidecar。

以上属于此前审查基线。后续实施前必须重新核对当前分支、HEAD 和源码，不应直接把历史审查结果当成当前实现状态。

### 2.3 核心架构问题

| 编号     | 问题                       | 影响                  | 优先级 |
| ------ | ------------------------ | ------------------- | --- |
| ARC-01 | HTTP 与 IPC 双通道缺少统一业务执行边界 | 业务行为和错误处理可能不一致      | P1  |
| ARC-02 | Tauri Host 职责过多          | 生命周期管理和测试复杂度增加      | P1  |
| ARC-03 | Rust Core 的运行时复用边界不明确    | 桌面与 Server 可能形成重复装配 | P1  |
| ARC-04 | Python Sidecar 缺少统一运行时抽象 | 进程异常、超时和取消难以统一处理    | P1  |
| ARC-05 | 认证与资源授权链路不完整             | 可能出现未授权访问和资源越权      | P1  |
| ARC-06 | 契约版本和错误模型需要统一            | 前后端兼容性难以保障          | P2  |
| ARC-07 | 端到端测试覆盖不足                | 架构迁移容易引入回归          | P1  |
| ARC-08 | 运行时健康检查和诊断能力不足           | 故障定位和恢复成本较高         | P2  |

## 3. 总体架构目标

### 3.1 架构定位

RICA 采用：

**模块化单体 + Rust Core + 多入口适配器 + 可插拔 Sidecar Runtime**

这意味着：

- 业务核心保持在一个 Rust Workspace 中。
- Domain 和 Application 不依赖 Tauri、Axum 或 Python。
- HTTP 和 IPC 是两种入站适配方式。
- 两种入口最终调用相同的 Application Use Case。
- SQLite、AI、OCR、备份和 Sidecar 都通过 Infrastructure Port/Adapter 接入。
- 桌面运行时负责启动、关闭、健康检查和进程协调，不负责重新实现业务。

### 3.2 目标逻辑架构

```
flowchart TB
    UI["React / TypeScript UI"]
    BC["Frontend Backend Client"]
    HTTP["HTTP Adapter"]
    IPC["Tauri IPC Adapter"]

    subgraph CORE["Rust Core"]
        APP["Application Use Cases"]
        DOMAIN["Domain Model"]
        CONTRACT["Contracts / DTO"]
        PORTS["Repository & Service Ports"]
    end

    subgraph INFRA["Infrastructure Adapters"]
        DB["SQLite / SQLx"]
        AI["AI Provider Adapter"]
        SEC["Secrets / Crypto"]
        BACKUP["Backup / Restore"]
        OCR["OCR Adapter"]
    end

    subgraph RUNTIME["Desktop Runtime"]
        HOST["Tauri Host"]
        SERVER["Rust HTTP Runtime"]
        SIDE["Python Sidecar Runtime"]
    end

    UI --> BC
    BC --> HTTP
    BC --> IPC
    HTTP --> APP
    IPC --> APP
    APP --> DOMAIN
    APP --> PORTS
    APP --> CONTRACT
    PORTS --> DB
    PORTS --> AI
    PORTS --> SEC
    PORTS --> BACKUP
    PORTS --> OCR
    HOST --> SERVER
    HOST --> SIDE
```

### 3.3 架构分层约束

| 层               | 可以依赖                     | 禁止依赖                   |
| --------------- | ------------------------ | ---------------------- |
| Presentation    | Backend Client、UI State  | SQLx、Domain 内部存储实现     |
| Inbound Adapter | Application、Contracts    | 直接实现业务规则               |
| Application     | Domain、Ports、Contracts   | Axum、Tauri、SQLx 具体实现   |
| Domain          | 标准库及必要领域基础依赖             | Tauri、Axum、SQLx、Python |
| Infrastructure  | Domain、Application Ports | 反向控制 Application       |
| Runtime         | 具体 Adapter 和组合根          | 在 Host 内复制业务逻辑         |

依赖方向应保持：

```
Presentation
    |
    v
Inbound Adapters
    |
    v
Application
    |
    v
Domain + Ports
    ^
    |
Infrastructure Adapters
```

Infrastructure 通过实现 Application 定义的 Port 接入，而不是让 Application 依赖基础设施实现。

## 4. Rust Core 目标设计

### 4.1 Rust Core 的职责

Rust Core 是 RICA 唯一的业务执行核心，负责：

- 业务用例编排。
- 领域规则执行。
- 数据访问抽象。
- 事务边界管理。
- 身份与授权上下文传递。
- 业务错误定义。
- 业务事件和审计记录。
- 外部能力调用编排。

Rust Core 不负责：

- Tauri 窗口管理。
- HTTP 路由注册。
- 前端状态管理。
- Python 进程直接启动。
- 桌面安装包和自动更新。
- 操作系统 UI 交互。

### 4.2 是否新增独立 Rust Core Crate

建议采用渐进方案，而不是立即拆出新的大 Crate。

**阶段 A：保留现有 Crate。**

继续使用：

- `ica-domain`
- `ica-application`
- `ica-contracts`
- `ica-infra`

优先明确各 Crate 的接口边界和依赖约束。

**阶段 B：引入统一 Application Runtime / AppState。**

建立统一装配入口：

```
pub struct AppRuntime {
    pub application: ApplicationServices,
    pub identity: IdentityServices,
    pub diagnostics: DiagnosticsServices,
}
```

此处仅为目标接口示意，具体类型应按当前依赖关系设计。

HTTP 与 IPC 都从同一运行时获取 Application Services。

**阶段 C：评估独立 Core Facade Crate。**

只有当以下条件成立时，才考虑新增独立 Crate：

- HTTP 和 IPC 需要稳定复用同一组服务。
- 组合根和服务装配出现重复。
- 当前 Crate 边界无法清晰表达 Runtime/Core 的关系。
- 新 Crate 能够减少依赖，而不是只增加转发代码。

不建议仅为命名统一而新增 `ica-core`。

### 4.3 Application Service 设计

每个业务模块使用明确的 Use Case 接口。

示意：

```
pub trait PositionUseCases {
    async fn list(
        &self,
        context: &RequestContext,
        query: PositionQuery,
    ) -> Result<Page<PositionDto>, AppError>;

    async fn get(
        &self,
        context: &RequestContext,
        id: PositionId,
    ) -> Result<PositionDto, AppError>;

    async fn create(
        &self,
        context: &RequestContext,
        command: CreatePosition,
    ) -> Result<PositionDto, AppError>;
}
```

设计约束：

1. Use Case 不感知 HTTP Request。
2. Use Case 不感知 Tauri Command。
3. Use Case 接收明确的业务 Command / Query。
4. Use Case 接收可信的 RequestContext。
5. 授权在 Application 层执行，不能只依赖前端隐藏按钮。
6. Repository 查询应包含资源归属条件。
7. 写操作应定义事务边界和审计要求。

### 4.4 RequestContext

建议建立统一请求上下文：

```
pub struct RequestContext {
    pub principal: Principal,
    pub request_id: RequestId,
    pub source: RequestSource,
    pub permissions: PermissionSet,
}
```

其中：

- `principal`：经过认证的主体。
- `request_id`：贯穿日志和调用链的请求标识。
- `source`：HTTP、IPC 或内部任务。
- `permissions`：可信授权信息，不能由前端任意构造。

对于 Local Mode，可使用受控的本地用户主体；对于 Multi-user Mode，必须由认证组件建立主体身份。

## 5. HTTP 与 IPC 通信架构

### 5.1 最终决策

**ADR-001：保留 HTTP + IPC 双通道，采用统一 Application Core。**

不选择一次性全面 IPC 化，也不要求所有桌面业务永久使用 HTTP。

原因：

- IPC 适合桌面本地高频调用和类型化请求。
- HTTP 适合外部客户端、独立服务和标准 API 集成。
- 两者都可以调用同一 Application Use Case。
- 现有前端 Backend Client 抽象可以支持渐进迁移。
- 业务能力不应被某一种传输方式绑定。

### 5.2 双通道的边界

```
React UI
   |
Backend Client Interface
   |
   +---------------- HTTP Adapter
   |                     |
   |                     v
   |                Application Core
   |
   +---------------- IPC Adapter
                         |
                         v
                    Application Core
```

HTTP 与 IPC 只负责：

- 参数解析。
- DTO 校验。
- 身份上下文建立。
- Application 调用。
- 结果和错误映射。
- 传输层日志。

不得分别实现业务状态机、权限规则或数据访问逻辑。

### 5.3 Backend Client 接口

前端统一使用业务接口，不让业务组件直接依赖 `fetch` 或 `invoke`。

示意：

```
export interface PositionClient {
  list(query: PositionQuery): Promise<Page<Position>>;
  get(id: string): Promise<Position>;
  create(input: CreatePositionInput): Promise<Position>;
  update(
    id: string,
    input: UpdatePositionInput
  ): Promise<Position>;
  delete(id: string): Promise<void>;
}
```

具体实现：

- `HttpPositionClient`
- `IpcPositionClient`

选择机制由应用启动配置决定，不应让页面组件自行判断传输方式。

### 5.4 迁移原则

不按技术偏好迁移，而按业务模块和验收能力迁移。

| 业务能力             | 建议通信方式                        | 决策依据              |
| ---------------- | ----------------------------- | ----------------- |
| Positions        | IPC 优先，HTTP 保留                | 已有 IPC 基础         |
| Candidates       | 可按现有调用保留 HTTP，逐步迁移            | 先确保统一 Use Case    |
| Conversations    | 按模块边界迁移                       | 涉及状态和授权           |
| FollowUps        | 统一业务状态机后再迁移                   | 状态流转正确性优先         |
| AI 调用            | 由 Application Service 统一编排    | Provider 不应暴露给 UI |
| Browser 能力       | 经 Rust Application 调用 Sidecar | Python 运行时隔离      |
| Backup / Restore | 由统一应用服务控制                     | 高风险写操作            |
| Diagnostics      | 本地 IPC 与诊断 API 按需提供           | 桌面运维需求            |

### 5.5 传输协议一致性

HTTP 与 IPC 必须满足：

- 相同业务输入产生相同业务结果。
- 相同权限条件产生相同授权结论。
- 相同业务错误映射到一致的错误码。
- 相同写操作执行相同事务和审计规则。
- 传输层差异不得改变领域状态机。

建议建立 Contract Test，对同一组输入分别调用 HTTP 和 IPC Adapter，比较结果。

## 6. Tauri Desktop Runtime 设计

### 6.1 Tauri Host 定位

Tauri Host 应是桌面应用的运行时协调器，而不是业务 Server 的替代品。

职责：

1. 创建和管理桌面窗口。
2. 初始化桌面安全配置。
3. 创建 AppRuntime。
4. 注册 IPC Adapter。
5. 管理外部 Sidecar 生命周期。
6. 提供应用退出与资源清理流程。
7. 提供桌面诊断信息。
8. 管理必要的本地运行时资源。

不建议 Host 直接实现复杂业务规则、数据库查询或 AI 业务流程。

### 6.2 运行模式

建议支持两个清晰的运行模式：

**Desktop Embedded Mode**

```
RICA Desktop
  ├── Tauri Host
  ├── React UI
  ├── Rust Application Runtime
  ├── SQLite
  └── Python Sidecar（按需启动）
```

特点：

- 默认面向个人本地使用。
- Rust Core 由桌面进程直接装配。
- 业务调用优先使用 IPC。
- 不需要为了本机 UI 调用而强制启动 HTTP Server。
- 需要外部 API 或其他客户端时再启动 HTTP 服务。

**Server Mode**

```
RICA Server
  ├── Rust HTTP Runtime
  ├── Application Runtime
  ├── Infrastructure
  ├── Database
  └── Optional Sidecar Runtime
```

特点：

- 通过 HTTP 对外提供 API。
- 认证与资源授权必须完整启用。
- 不依赖 Tauri。
- 不要求运行桌面窗口。
- 适合未来独立服务部署和多客户端访问。

### 6.3 两种模式共用代码

两种模式应共用：

- Domain。
- Application。
- Contracts。
- Infrastructure Adapter。
- 业务状态机。
- 授权策略。
- 数据访问规则。

差异仅存在于：

- 组合根。
- 入站 Adapter。
- 运行时生命周期。
- 配置和部署方式。

### 6.4 是否保留 Desktop HTTP Server

建议采用按需启用，而不是直接删除。

以下场景仍然需要 HTTP：

- 第三方工具访问。
- 本地 Web 客户端。
- 自动化测试。
- 开发调试。
- 未来远程服务模式。

如果 Desktop Embedded Mode 仅通过 IPC 使用业务能力，则不必为了历史架构强制启动本地 HTTP Server。

## 7. Python Sidecar Runtime 设计

### 7.1 定位

Python Sidecar 是外部能力执行器，不是第二套 RICA 业务核心。

适合承载：

- Browser Automation。
- 浏览器驱动与浏览器环境适配。
- 特定 Python 生态依赖。
- 暂时不适合迁移到 Rust 的外部工具能力。

不应承载：

- RICA 核心领域模型。
- Candidate、Position、Conversation 等核心业务规则。
- FollowUp 状态机。
- 数据库业务事务。
- 独立于 Rust 的授权判断。

### 7.2 Sidecar 抽象

建议在 Rust Infrastructure 层定义：

```
#[async_trait]
pub trait ExternalCapability {
    async fn execute(
        &self,
        request: CapabilityRequest,
    ) -> Result<CapabilityResponse, CapabilityError>;

    async fn health_check(
        &self,
    ) -> Result<CapabilityHealth, CapabilityError>;
}
```

具体适配器：

- `PythonBrowserAdapter`
- `PythonAiExtensionAdapter`（仅在现有能力确有必要时保留）
- 其他未来外部能力适配器

### 7.3 通信协议

优先采用本机标准输入输出（stdin/stdout）上的 JSON-RPC 风格协议，或经过明确版本化的 JSON Line 协议。

建议协议结构：

```
{
  "protocol_version": "1.0",
  "request_id": "uuid",
  "method": "browser.open",
  "params": {
    "url": "https://example.com"
  }
}
```

响应：

```
{
  "protocol_version": "1.0",
  "request_id": "uuid",
  "result": {
    "status": "ok"
  },
  "error": null
}
```

规范：

- stdout 只输出协议消息。
- stderr 用于诊断日志。
- 每个请求具有唯一 request_id。
- 协议必须有版本号。
- 每个请求必须有超时。
- 支持取消和任务清理。
- 子进程退出时清理关联资源。
- 不能把任意异常堆栈直接返回给 UI。
- 协议不允许 Sidecar 自行访问核心数据库。

### 7.4 生命周期管理

建议状态机：

```
Stopped
   |
   v
Starting
   |
   v
Ready
   |
   +------> Degraded
   |            |
   v            v
Stopping <--- Restarting
   |
   v
Stopped
```

至少实现：

- 启动超时。
- 健康检查。
- 请求超时。
- 最大并发请求数。
- 取消任务。
- 进程异常退出检测。
- 有限次数重启。
- 优雅关闭。
- 强制终止兜底。
- 标准输出与错误输出分离。

### 7.5 安全边界

- Sidecar 使用最小权限运行。
- 不向 Sidecar 传递不必要的密钥。
- IPC/本地通信不得暴露为无认证的 LAN 服务。
- 浏览器访问目标需有 URL 和网络访问策略。
- 对本地文件、Cookie、浏览器 Profile 的访问必须受控。
- Sidecar 不得自行执行未授权的核心业务写操作。

RICA V1 的业务人工操作边界保持不变：不因 Sidecar 接入而默认增加招聘平台自动发送或未经确认的浏览器操作。

## 8. 身份认证与授权架构

### 8.1 安全模型

建议将以下概念明确分离：

| 概念                 | 作用            |
| ------------------ | ------------- |
| Authentication     | 确认调用者身份       |
| Authorization      | 判断调用者能否执行操作   |
| Resource Ownership | 判断调用者是否拥有目标资源 |
| Session            | 管理登录会话生命周期    |
| Secret Management  | 管理密钥和敏感凭证     |
| Audit              | 记录关键安全操作      |

不能将“拥有有效 JWT”视为“有权访问所有业务资源”。

### 8.2 Local Mode 与 Multi-user Mode

**Local Mode**

- 默认面向单机本地用户。
- 本地进程和本机 IPC 仍需有安全边界。
- 不应默认把本地 HTTP API 暴露到局域网。
- 敏感数据和密钥应使用系统安全存储。
- 数据库访问仍应经过 Application 层。

**Multi-user Mode**

- 必须有明确的用户身份。
- 每个资源必须定义归属或访问范围。
- Repository 查询必须包含授权范围。
- 所有 HTTP 和 IPC 入口必须执行统一授权规则。
- 需要跨用户隔离测试和安全审计。
- 未完成上述能力前，不应宣称具备安全多用户支持。

### 8.3 JWT 密钥管理

当前已知风险是使用时间戳和 PID 构造 JWT Secret。该方式不能满足密码学安全随机数要求。

目标方案：

1. 使用操作系统 CSPRNG 生成高熵密钥。
2. 明确密钥持久化位置和访问权限。
3. 不将密钥硬编码到仓库。
4. 不在普通日志中记录密钥。
5. 明确密钥轮换和会话失效策略。
6. 明确桌面重启后 Access Token 与 Refresh Token 的恢复机制。

密钥持久化应根据目标平台选择系统凭据存储或受保护的本地密钥文件。不能仅为保持 JWT 长期有效而牺牲密钥保护。

### 8.4 Tauri HTTP Proxy 安全

HTTP Proxy 只能转发明确允许的 Header。

建议：

- 使用请求 Header Allowlist。
- 过滤 hop-by-hop Header。
- 明确 Authorization 的传递策略。
- 对加密密钥类 Header 设置专门规则。
- 禁止任意目标端口转发。
- 限制请求体大小。
- 限制允许的路径和 HTTP Method。
- 明确代理错误和超时行为。

不能简单复制所有前端 Header 到后端。

### 8.5 认证恢复状态机

前端启动时不能仅依赖 localStorage 中的 Access Token。

建议状态：

```
Initializing
    |
    v
CheckingSession
    |
    +------ Valid ------> Authenticated
    |
    +------ Expired ----> Refreshing
                              |
                              +---- Success ----> Authenticated
                              |
                              +---- Failure ----> Unauthenticated
    |
    +------ Invalid ----> Unauthenticated
```

要求：

- Refresh 请求支持并发合并。
- Refresh Token 轮换应防止重放。
- Refresh 失败时清理无效会话。
- `/auth/me` 与业务 API 的行为应一致。
- 应定义桌面重启、应用升级和密钥轮换时的恢复策略。

## 9. Contracts 与错误模型

### 9.1 契约统一

`ica-contracts` 负责共享：

- Request DTO。
- Response DTO。
- Error DTO。
- Enum 和状态编码。
- 分页结构。
- 协议版本。
- 输入校验规则。

不要把 SQLx Model、Domain Entity 直接暴露给前端。

### 9.2 统一错误模型

建议建立稳定的错误分类：

| 错误类型           | 示例     | HTTP 映射 | IPC 映射               |
| -------------- | ------ | ------- | -------------------- |
| Validation     | 参数非法   | 400     | Validation Error     |
| Authentication | 未登录    | 401     | Authentication Error |
| Authorization  | 无权操作   | 403     | Authorization Error  |
| NotFound       | 资源不存在  | 404     | NotFound             |
| Conflict       | 状态冲突   | 409     | Conflict             |
| RateLimited    | 请求过多   | 429     | RateLimited          |
| External       | 外部服务异常 | 502/503 | External Error       |
| Internal       | 内部异常   | 500     | Internal Error       |

IPC 不一定使用 HTTP 状态码，但应共享业务错误码。

错误响应不得泄露：

- SQL 语句。
- 文件系统敏感路径。
- Token 和 Secret。
- 内部堆栈。
- 数据库连接信息。

### 9.3 版本策略

- 内部 Rust 类型优先通过编译期约束维护。
- IPC Command 参数变化应有兼容性策略。
- HTTP API 应维护明确的版本策略。
- Sidecar 协议必须携带 protocol_version。
- 破坏性变更必须有迁移说明和兼容测试。

## 10. 数据访问与持久化架构

### 10.1 SQLite 定位

SQLite 继续作为 Local Mode 的默认数据库。

约束：

- Application 通过 Repository Port 访问。
- SQLx 具体实现位于 Infrastructure。
- Migration 是唯一正式 Schema 演进路径。
- 事务由 Application Use Case 定义边界。
- 数据库备份和恢复由受控服务执行。
- Sidecar 不直接写入核心数据库。

### 10.2 事务边界

对以下操作明确事务要求：

- 创建或更新招聘业务实体。
- Conversation 与消息记录更新。
- FollowUp 状态迁移。
- 用户确认和审核记录。
- 重要配置变更。
- 备份元数据和恢复记录。

不能在多个 Adapter 中分别拼装同一个业务事务。

### 10.3 未来数据库扩展

不因为未来可能支持 PostgreSQL 而提前抽象所有 SQL。

当前优先保证：

- Repository Port 具有清晰业务语义。
- 不在 Domain 中暴露数据库类型。
- SQL Migration 可追踪。
- SQLite 约束和并发行为有测试。
- 只有明确出现服务端部署需求时，再评估 PostgreSQL 适配器。

## 11. 可观测性与运行时诊断

### 11.1 统一诊断模型

建议为每次业务调用记录：

- request_id。
- principal_id（适用时）。
- source（HTTP/IPC/Internal）。
- module。
- operation。
- duration_ms。
- result。
- error_code。

敏感数据必须脱敏。

### 11.2 运行时健康状态

建议提供：

| 组件                       | 健康指标              |
| ------------------------ | ----------------- |
| Rust Application Runtime | 初始化完成、服务可用        |
| SQLite                   | 连接、Migration、读写检查 |
| HTTP Server              | 监听状态、请求处理         |
| Python Sidecar           | 进程、协议、健康检查        |
| AI Adapter               | Provider 配置、连通性   |
| Backup                   | 最近备份状态、可恢复性       |
| Authentication           | Session 和密钥状态     |

健康检查分为：

- Liveness：进程是否存活。
- Readiness：是否可以处理请求。
- Dependency Health：外部依赖是否正常。

不能把某个可选 Provider 不可用等同于整个应用不可用。

## 12. 测试架构

### 12.1 测试分层

| 层级               | 测试内容                        |
| ---------------- | --------------------------- |
| Unit             | Domain 规则、状态机、校验            |
| Application      | Use Case、授权、事务              |
| Adapter          | HTTP、IPC DTO 和错误映射          |
| Infrastructure   | SQLite、Migration、备份、Secrets |
| Sidecar Contract | 协议、超时、取消、异常退出               |
| Integration      | Rust Core 与数据库、外部能力         |
| E2E              | 真实桌面操作与招聘业务流程               |
| Security         | 认证、授权、资源隔离、代理安全             |
| Release          | Windows 安装、升级、恢复            |

### 12.2 HTTP / IPC 一致性测试

同一业务场景分别走 HTTP 和 IPC：

1. 相同输入。
2. 相同主体身份。
3. 相同资源状态。
4. 相同授权条件。
5. 比较业务结果。
6. 比较错误码。
7. 比较数据库最终状态。

这类测试是双通道架构长期存在的必要门禁。

### 12.3 Sidecar 测试

必须覆盖：

- 正常启动。
- 启动失败。
- 协议版本不匹配。
- 非法 JSON。
- 超时。
- 用户取消。
- 子进程异常退出。
- stdout 噪声。
- stderr 大量输出。
- 连续崩溃和重启上限。
- 应用退出时进程回收。

### 12.4 安全测试

必须覆盖：

- 无 Token 访问受保护资源。
- 无效 Token。
- 过期 Token。
- Refresh 失败。
- 用户 A 访问用户 B 资源。
- IPC 越权调用。
- HTTP 与 IPC 授权差异。
- Proxy Header 注入。
- 任意端口代理。
- 本地文件和 Sidecar 权限边界。

## 13. ADR 决策记录

### ADR-001：HTTP + IPC 双通道策略

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**上下文：** 当前桌面端同时存在 HTTP 和 IPC，部分业务已使用 IPC。

**决策：** 保留两种通信方式，通过统一 Application Core 共享业务能力。

**备选方案：**

- A：全面 IPC 化。
- B：全面 HTTP 化。
- C：HTTP + IPC 双通道共存，统一 Application Core。

**选择：** C。

**原因：**

- 桌面本地调用适合 IPC。
- 对外 API 适合 HTTP。
- 业务逻辑不应依赖传输协议。
- 可以渐进迁移，降低重构风险。

**代价：**

- 需要维护两个 Adapter。
- 需要双通道契约测试。
- 必须防止两种 Adapter 出现业务逻辑分叉。

**重新评估条件：**

- 桌面端完全不再需要 HTTP。
- 或服务端部署需求发生根本变化。

### ADR-002：Rust Core 作为唯一业务核心

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** Rust Application/Domain 作为唯一业务核心，HTTP 和 IPC 共享 Use Case。

**不选择：** 在 Tauri Host、HTTP Handler 和前端分别实现业务逻辑。

**理由：**

- 避免业务规则重复。
- 统一授权和事务边界。
- 提升单元测试与集成测试能力。
- 为未来独立 Server 模式提供基础。

**实施约束：**

- 不立即强制新增 `ica-core` Crate。
- 先建立统一 Runtime 和装配边界。
- 根据真实依赖再决定是否拆 Crate。

### ADR-003：模块化单体，不采用微服务

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** 继续使用 Rust Workspace 模块化单体。

**理由：**

- 当前业务规模不需要分布式部署复杂度。
- SQLite 本地优先与单体运行模式更匹配。
- 模块化单体更容易保证事务一致性。
- 减少服务发现、网络失败和部署运维成本。

**重新评估条件：**

- 出现独立扩缩容需求。
- 出现明确的团队自治和部署边界。
- 模块间通信成本已超过进程内调用的优势。

### ADR-004：Tauri Host 作为 Desktop Runtime

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** Tauri Host 负责桌面生命周期和运行时协调，不承载核心业务逻辑。

**理由：**

- 降低 Host 复杂度。
- 保持业务层可测试。
- 让 Desktop 和 Server 共享 Application Runtime。

**实施要求：**

- 将 Sidecar 生命周期管理独立封装。
- 将 IPC Command 降为薄适配层。
- 明确关闭顺序、超时和资源清理。

### ADR-005：Python Sidecar 可插拔化

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** Python 继续承载适合 Python 生态的外部能力，但必须经过统一 Sidecar Runtime 和版本化协议。

**理由：**

- 避免不必要地重写成熟 Python 能力。
- 避免 Python 直接承担核心业务。
- 支持未来替换或增加其他外部执行器。

**实施要求：**

- 统一启动、停止和健康检查。
- 统一超时、取消和错误模型。
- 限制权限和文件访问。
- 禁止 Sidecar 直接写核心业务数据库。

### ADR-006：Local Mode 与 Multi-user Mode 安全边界

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** 明确区分单机本地模式和多用户服务模式。

**理由：**

- 本地应用与服务端部署具有不同威胁模型。
- 有效认证不等于资源授权。
- 多用户隔离需要从 API 到 Repository 的完整链路。

**实施要求：**

- Local Mode 不默认暴露 LAN 服务。
- Multi-user Mode 必须执行资源级授权。
- 未完成安全测试前不得宣称支持安全多用户模式。

### ADR-007：SQLite 作为 Local Mode 默认数据库

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** SQLite 继续作为本地模式默认数据库，Repository Port 为未来其他数据库适配保留边界。

**理由：**

- 与本地优先架构一致。
- 部署成本低。
- 单机备份和恢复路径简单。

**实施要求：**

- Migration 统一管理。
- 事务边界明确。
- 备份恢复可验证。
- 不提前引入无实际需求的多数据库抽象。

### ADR-008：统一 Contracts 与错误模型

**状态：** Accepted（用户评审确认的目标基线，2026-11-22）

**决策：** HTTP、IPC 和 Sidecar 使用明确的契约边界，统一业务错误分类。

**理由：**

- 降低前后端耦合。
- 保证双通道行为一致。
- 支持协议兼容和测试。

**实施要求：**

- DTO 不暴露 Domain Entity 或数据库 Model。
- 错误码稳定。
- Sidecar 协议版本化。
- 破坏性变更需要兼容策略。

## 14. 架构演进路线

### Phase 0：基线冻结与架构验证

目标：建立真实现状基线。

任务：

- 记录当前分支和完整 HEAD SHA。
- 确认 Rust Workspace 依赖关系。
- 盘点全部 HTTP Routes。
- 盘点全部 IPC Commands。
- 盘点前端 Backend Client 调用点。
- 盘点 Sidecar 启动、通信和关闭逻辑。
- 形成 Architecture Dependency Map。
- 运行当前已有测试并记录基线。

验收：

- 当前实现与文档差异可追踪。
- 每个业务模块的入口和 Use Case 有明确映射。
- 不修改业务行为。

### Phase 1：安全基线整改

目标：先消除高风险安全问题。

任务：

- SEC-01：JWT Secret 改为 CSPRNG。
- SEC-02：HTTP Proxy Header Allowlist。
- SEC-03：会话恢复和 Refresh 流程。
- SEC-04：业务 API 认证和资源授权审计。
- 建立 Local / Multi-user 安全边界。
- 增加跨用户资源访问测试。

验收：

- JWT 密钥生成和持久化策略明确。
- HTTP Proxy 不丢失必要头，也不接受任意头。
- 桌面重启后会话行为可预测。
- 业务 API 的授权链路可验证。
- 无未授权跨用户访问。

### Phase 2：统一 Rust Application Runtime

目标：HTTP 与 IPC 共用同一套 Application Services。

任务：

- 定义统一 Application Runtime 装配入口。
- 整理 Application Services。
- 明确 RequestContext。
- 清理 HTTP Handler 中的业务逻辑。
- 清理 IPC Command 中的业务逻辑。
- 统一错误模型。
- 建立 Adapter Contract Test。

验收：

- HTTP 和 IPC 不包含重复业务规则。
- 两个 Adapter 使用相同 Use Case。
- 相同场景结果一致。
- 授权在 Application 层执行。

### Phase 3：Tauri Runtime 职责收敛

目标：简化 Host，明确桌面运行时边界。

任务：

- 抽离 Sidecar Manager。
- 明确 AppRuntime 初始化和销毁流程。
- 明确 Rust HTTP Runtime 是否按需启动。
- 统一健康检查。
- 统一日志与诊断信息。
- 实现优雅关闭和超时兜底。

验收：

- Host 不直接实现业务逻辑。
- 应用退出后无孤儿子进程。
- HTTP Server 可按运行模式启停。
- 桌面重启和异常退出可恢复。

### Phase 4：Python Sidecar 协议化

目标：形成稳定外部能力边界。

任务：

- 定义 Sidecar Protocol。
- 定义请求、响应和错误 DTO。
- 定义 protocol_version。
- 实现请求超时和取消。
- 实现健康检查。
- 实现崩溃检测和有限重启。
- 加入权限与文件访问控制。
- 完成 Sidecar Contract Test。

验收：

- Sidecar 协议可独立测试。
- Sidecar 不直接访问核心业务数据库。
- 超时、取消和异常退出可恢复。
- Rust Core 不依赖 Python 具体实现细节。

### Phase 5：通信方式渐进迁移

目标：在不改变业务行为的前提下优化桌面调用。

任务：

- 建立统一 Frontend Backend Client。
- 为业务模块选择 HTTP 或 IPC Adapter。
- 优先整理 Positions 已有 IPC 实现。
- 逐模块迁移，不一次性替换所有 API。
- 增加 HTTP/IPC 一致性测试。
- 保留必要的 HTTP Server 能力。

验收：

- UI 组件不直接依赖传输实现。
- 双通道结果和权限一致。
- 迁移前后业务行为一致。
- 失败时可回退到原有调用方式。

### Phase 6：质量门禁和发布验收

目标：把架构约束固化为持续质量门禁。

任务：

- Rust Format / Clippy / Test。
- Frontend Lint / Typecheck / Test / Build。
- Tauri Build。
- Sidecar Contract Test。
- Security Scan。
- E2E。
- Windows 安装、升级、卸载和恢复测试。
- 文档和版本状态同步。

验收：

- CI 可重复执行。
- 主分支保护规则明确。
- 发布包经过 Windows 实机验收。
- 备份恢复经过实际验证。
- 报告中的测试数据可追溯到具体 HEAD 和执行记录。

## 15. 任务优先级与依赖

| 任务                     | 优先级 | 依赖              | 交付物                  |
| ---------------------- | --- | --------------- | -------------------- |
| 当前 HEAD 与源码基线          | P0  | 无               | Baseline Report      |
| JWT Secret 安全整改        | P1  | 基线              | 安全实现与测试              |
| Proxy Header 安全整改      | P1  | 基线              | Proxy Policy         |
| 认证恢复状态机                | P1  | JWT 策略          | Session Design/Test  |
| 业务 API 授权审计            | P1  | 身份模型            | Authorization Matrix |
| Application Runtime 统一 | P1  | 基线              | Runtime Facade       |
| HTTP/IPC Contract Test | P1  | Runtime 统一      | Adapter Test Suite   |
| Sidecar Manager 抽离     | P1  | 基线              | Runtime Manager      |
| Sidecar Protocol       | P1  | Sidecar Manager | Protocol Spec        |
| Tauri Host 收敛          | P2  | Runtime 统一      | Desktop Runtime      |
| IPC 渐进迁移               | P2  | Contract Test   | 模块迁移记录               |
| CI 和质量门禁               | P1  | 测试基线            | CI Workflow          |
| Windows 发布验收           | P1  | Runtime 稳定      | Acceptance Report    |

## 16. Hermes 多 Agent 实施分工

建议按任务边界分配，避免多个 Agent 同时修改同一核心模块。

| Agent                  | 职责                  | 主要交付物                    | 修改边界                   |
| ---------------------- | ------------------- | ------------------------ | ---------------------- |
| Architect              | 总体架构和 ADR           | 架构决策、依赖图                 | 文档和评审                  |
| Security Engineer      | SEC-01～SEC-04       | 安全实现与测试                  | Auth、Proxy、授权          |
| Rust Core Engineer     | Application Runtime | Runtime Facade、Use Case  | Application、Contracts  |
| Desktop Engineer       | Tauri Host          | 生命周期、IPC Adapter         | Desktop Host           |
| Sidecar Engineer       | Python Runtime      | Manager、Protocol         | Sidecar 与 Rust Adapter |
| QA Engineer            | 自动化测试               | Contract、Integration、E2E | 测试目录与 CI               |
| Documentation Engineer | 文档一致性               | README、ADR、验收记录          | 文档                     |

协作规则：

1. Architect 先输出任务依赖图。
2. Security Engineer 优先完成安全边界评审。
3. Rust Core Engineer 与 Desktop Engineer 通过接口契约协作。
4. Sidecar Engineer 不得直接修改 Rust Domain。
5. QA Engineer 在接口冻结后补充测试。
6. 所有代码修改必须有明确文件范围和验收标准。
7. 不允许 Agent 自动 Commit、Push、Merge 或 Release。
8. 每个任务结束后提交变更摘要、测试记录和未解决问题。

## 17. 风险与回滚策略

### 17.1 主要风险

| 风险                | 影响     | 控制措施                      |
| ----------------- | ------ | ------------------------- |
| 统一 Runtime 造成过度抽象 | 增加复杂度  | 先复用现有 Application Service |
| 双通道长期维护成本         | 测试成本上升 | Contract Test 门禁          |
| Sidecar 协议变化      | 兼容性破坏  | 版本化协议                     |
| JWT 密钥迁移          | 旧会话失效  | 明确迁移与重新登录策略               |
| IPC 迁移造成回归        | 桌面功能异常 | 按模块迁移和回退                  |
| Host 重构影响启动       | 桌面无法启动 | 启动链路回归测试                  |
| 授权整改影响既有数据访问      | 合法操作失败 | 资源归属矩阵和兼容测试               |

### 17.2 回滚原则

- 每个 Phase 独立验收。
- 不把多个高风险改造合并成一个不可拆分的大变更。
- 保留 HTTP Adapter 作为迁移期间的可用入口。
- Sidecar 协议升级应明确兼容策略。
- 数据库 Migration 必须提供前向修复或明确恢复方案。
- 安全整改不能通过关闭认证、授权或代理校验回滚。
- 任何数据结构变更都应先验证备份恢复。

## 18. 最终架构决策摘要

| 决策项         | 最终方向                                           |
| ----------- | ---------------------------------------------- |
| 总体架构        | 模块化单体                                          |
| 核心语言        | Rust                                           |
| 业务核心        | 统一 Rust Application Core                       |
| HTTP        | 保留，主要承担标准 API 和外部接入                            |
| IPC         | 保留，承担桌面本地调用                                    |
| 前端通信        | 统一 Backend Client                              |
| Tauri Host  | Desktop Runtime / 组合根                          |
| Rust Server | 可独立运行的 HTTP Runtime                            |
| Python      | 可插拔 Sidecar                                    |
| Sidecar 通信  | 版本化本机协议                                        |
| 数据库         | Local Mode 默认 SQLite                           |
| 认证          | 独立身份认证和会话管理                                    |
| 授权          | Application 层执行资源级授权                           |
| 测试          | Unit + Contract + Integration + E2E + Security |
| 部署          | Desktop Embedded Mode + Server Mode            |
| 微服务         | 当前阶段不采用                                        |
| 全量 IPC 化    | 不采用                                            |
| 自动 Git 操作   | 禁止，必须人工授权                                      |

## 19. 评审结论与下一步

本文件已由用户评审确认为 RICA 下一阶段架构演进的 Accepted 目标基线（2026-11-22）。

评审通过后，按以下顺序推进：

1. 冻结当前 HEAD 和完整源码基线。
2. 完成 SEC-01～SEC-04 安全整改。
3. 建立统一 Application Runtime。
4. 抽离 Tauri Sidecar 生命周期管理。
5. 完成 Sidecar 协议化。
6. 建立 HTTP/IPC 一致性测试。
7. 逐模块推进通信迁移。
8. 完成 CI、Windows 安装包和备份恢复验收。

**最终目标：**

RICA 应形成一个以 Rust 为唯一业务核心、以 Tauri 为桌面运行时、以 HTTP/IPC 为多入口适配器、以 Python Sidecar 为可插拔外部能力执行器的模块化单体架构。

该架构既保留当前实现的有效投资，也为后续桌面产品化、独立 Server 部署和外部 Agent 集成提供稳定边界。

---

**文档状态：** Accepted  
**下一评审项：** ADR-001～ADR-008  
**实施约束：** 先审计、后设计、再分阶段实施；未经明确授权不得执行 Git 提交、推送、合并或发布。

## **架构评审时需要重点确认的三个问题**

虽然文档给出了 8 项 ADR 建议，但我建议正式冻结前重点审查以下三项：

1 **是否采用 Desktop Embedded Mode 作为默认运行模式？**  
我的建议是采用。桌面端直接装配 Rust Application Runtime，HTTP Server 按需启动，减少不必要的本地服务进程。

2 **是否长期保留 HTTP + IPC 双通道？**  
我的建议是保留，但严格统一到 Application Core。不要让 HTTP 和 IPC 各自形成一套业务逻辑。

3 **Python Sidecar 是否限定为外部能力执行器？**  
我的建议是限定。招聘业务规则、数据事务、状态机和授权统一留在 Rust，Python 主要负责 Browser 等生态适配能力。

**状态说明：** 用户已于 2026-11-22 评审确认三个待确认问题（Embedded Mode 采用 / 双通道统一 Application Core / Sidecar 限定外部能力执行器）并接受 8 条 ADR 为目标基线，文档状态由 Proposed 升为 Accepted。评审记录见 `docs/audits/rica-arch-r1-review.md`。ADR 的 Accepted 表示架构目标基线的确认，不等于具体实现的验收 Verified。实施前仍须以 RICA 当前仓库 HEAD 重新核对代码和依赖关系。

下一步建议先进行一次架构决策评审，冻结 ADR-001～ADR-008，再形成按 Crate、模块、文件和测试用例拆分的实施任务清单。