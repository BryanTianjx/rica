# **ICA-RUST × Hermes Embedded Agent 具体实施方案 R1.0**



方案基线：用户提供的 ICA-RUST 实测状态

实施前需完成基线与 OCR 决策



## **一、先明确总体方向：Agent 集成必须以 ICA-RUST 为目标后端**

这次方案需要调整此前《ICA Embedded Agent 架构设计 R1.0》的一个重要前提。

此前方案以 Python/FastAPI 为 ICA 业务后端，建议 Hermes 通过 Python MCP Server 调用 ICA Application Service。但现在已经存在独立的 `ICA-RUST` 迁移工程，并且明确以跨语言对拍作为等价性验收标准。因此，不能再把 Python MCP Server 作为长期目标架构，否则会形成双后端、双业务入口和重复维护。

**建议将最终架构确定为：ICA-RUST 是唯一业务后端，Hermes 是独立 Agent Runtime，ICA MCP Server 是 Rust 后端提供的受控业务工具接口。**

Python 继续保留为迁移期间的行为基线和对拍对象；迁移完成后，Python 不再承担正式产品运行时职责。Hermes 不应直接访问 SQLite，也不应绕过 ICA-RUST 的 Application 层。

### **1.1 三个系统的职责边界**

| 系统           | 长期职责                              | 不应承担的职责             |
| ------------ | --------------------------------- | ------------------- |
| ICA-RUST     | 招聘领域业务、数据、权限、审批、审计、API            | Agent 推理和模型编排       |
| Hermes Agent | 推理、任务规划、Skills、工具选择、Agent Session | 直接操作 ICA 数据库、决定业务授权 |
| ICA Desktop  | 用户交互、Runtime 生命周期、审批展示、事件桥接       | 复制后端业务规则            |

迁移期间增加一个临时角色：

| 系统        | 迁移期间职责                   |
| --------- | ------------------------ |
| Python 基线 | 提供原有行为、响应与落库结果，作为跨语言对拍参照 |

### **1.2 目标架构**

**ICA Desktop**

Tauri 2 · React · TypeScript

Agent Workspace · Approval UI · Runtime Supervisor

**ICA-RUST Backend**

ica-domain

ica-application

ica-contracts

ica-infra

ica-api / ica-server

Agent Policy / MCP Tools

**Hermes Runtime**

独立进程

Agent Engine

Skills / Session

MCP Client

Model Provider

Hermes MCP Client → ICA MCP Server

**ICA Local Data**

SQLite · Domain Records · Agent Approvals · Audit Evidence

逻辑目标架构。Agent 工具接口的具体部署方式需通过 MCP POC 决定。

**关键原则：** Hermes 可以提出业务操作请求，但最终是否允许执行，必须由 ICA-RUST 决定。

## **二、ICA-RUST 当前状态对实施顺序的影响**

按照你提供的实测数据，当前项目不适合直接进入完整 Agent 功能开发。需要先处理迁移工程本身的基线、质量门和未完成工作。

### **2.1 当前迁移状态**

| 项目        | 当前状态                      | 对 Agent 集成的影响           |
| --------- | ------------------------- | ----------------------- |
| Rust 后端   | 45/68 端点                  | 业务接口仍有缺口                |
| W5        | 15 条未完成                   | AI、设置、审计、备份能力尚未完成       |
| W6        | 8 条未完成                    | 认证和分享能力尚未完成             |
| W4        | 已提交至 `w4-candidate`，未独立放行 | 不能当作最终验收基线              |
| 桌面 IPC    | 46 个命令                    | 需要确认后续 Agent IPC 的扩展方式  |
| HEAD 快照测试 | 333 passed                | 快照质量门有较好基础              |
| 工作树测试     | 336 passed / 1 failed     | OCR 在途改动存在运行时问题         |
| fmt       | 当前失败                      | 不能认为工作树质量门通过            |
| 跨语言对拍     | 220/222                   | 两个 Dashboard 差异属于基线漂移问题 |
| 端点映射      | 112 live / 68 contract    | 基线契约与当前 Python 工作树不一致   |
| OCR       | 真引擎在途                     | 不应混入 W4 已验收结论           |

### **2.2 实施前必须冻结的事实**

建议将以下四项设为正式实施前置条件。

1. **Python 对拍基线重锚决策。** 明确继续使用 `d39e321`，还是将新的 Python 行为纳入下一版迁移契约。不能把当前 Python 工作树自动当作新基线。
2. **五条端点差异决策。** 明确四条 `ai/models` 和一条 `imports/{id}/confirm-candidate` 是否进入迁移范围。
3. **W4 独立验收。** 对 `w4-candidate` 进行独立审计，不能仅凭自报测试数字补发 `w4-done`。
4. **OCR 工作流隔离。** 决定真 OCR 引擎是否作为 W4 补充交付，或建立独立 OCR 改进波次。

这四项未完成前，不建议启动 Agent 业务工具开发。可以并行做 Hermes Runtime 的无业务数据 POC。

## **三、ICA-RUST 内部模块设计**

当前 7-crate 分层已经有明确的依赖方向和 `scripts/check.py` 机器门禁。Agent 能力应顺着现有分层扩展，不应为了接入 Hermes 破坏已有分层纯度。

### **3.1 目标模块归属**

| 现有 crate          | Agent 相关职责                                 | 是否需要扩展        |
| ----------------- | ------------------------------------------ | ------------- |
| `ica-domain`      | Agent Task、Approval、Permission 等领域状态及规则    | 需要，按实际领域复杂度决定 |
| `ica-application` | Agent 用例、授权检查、审批流程、业务工具调用编排                | 需要            |
| `ica-contracts`   | Agent API DTO、MCP Tool 输入输出契约、错误结构         | 需要            |
| `ica-infra`       | Agent 审计持久化、Runtime 配置存储、必要的模型网关适配         | 需要            |
| `ica-api`         | REST API、MCP Protocol Adapter、Agent API 路由 | 需要            |
| `ica-server`      | HTTP 服务组合根、MCP endpoint 注册、启动配置            | 需要            |
| `ica-desktop`     | Hermes 进程监督、Tauri IPC、Agent 事件桥接           | 需要            |

不建议首期增加一个独立的 `ica-agent` 业务 crate。原因是 Agent 的核心业务规则属于 ICA Application，而不是一个平行于招聘业务的独立领域。等 Agent 子系统复杂到需要独立部署或独立依赖治理时，再评估是否拆分。

### **3.2 目标依赖关系**

```
ica-domain
    ↑
ica-application
    ↑
ica-contracts

ica-infra ─────→ ica-application / ica-domain
    ↑
ica-api ───────→ ica-application / ica-contracts
    ↑
ica-server

ica-desktop ───→ Tauri IPC / Runtime Supervisor
```

Agent 的工具调用链应当是：

```
Hermes Agent
    │
    │ MCP Tool Call
    ▼
ICA MCP Adapter (ica-api)
    │
    ▼
Agent Application Use Case
    │
    ├── Permission Policy
    ├── Candidate / Position Use Case
    ├── Communication / Draft Use Case
    ├── Approval Use Case
    └── Audit Use Case
    │
    ▼
Repositories / SQLite (ica-infra)
```

禁止以下依赖：

- `ica-domain` 依赖 Hermes、MCP SDK、Tauri。
- `ica-application` 直接依赖 MCP Transport 或 Hermes SDK。
- MCP Tool Handler 直接操作 SQLx/ORM/SQLite。
- Hermes 直接读取 ICA 数据库文件。
- `ica-desktop` 实现候选人、沟通、草稿等业务规则。

### **3.3 MCP Server 部署方式建议**

这里需要结合 ICA-RUST 现有进程模型重新选择，而不是机械沿用此前 Python 方案中的 stdio。



建议优先验证

**ICA-RUST 内嵌 MCP HTTP Endpoint**



```
ICA Desktop
  │
  ├── ICA Server
  │     ├── REST API
  │     ├── Application Services
  │     ├── MCP HTTP Endpoint
  │     └── SQLite
  │
  └── Hermes Runtime
        └── MCP Client
              └── http://127.0.0.1:<port>/mcp
```

优点：

1. MCP Handler 与 REST API 使用同一组 Application Service。
2. 不需要额外启动一个 MCP Server 子进程。
3. 不会因为两个进程分别初始化数据库而产生连接、迁移和事务管理问题。
4. 业务授权、审计和数据访问仍由 ICA-RUST 统一控制。
5. MCP 与 REST API 可以共享领域用例，但保持协议适配层独立。

需要重点验证：当前 Hermes 版本支持的 MCP HTTP 传输方式、认证配置、工具发现和超时行为。

**stdio 作为备选方案：** 如果 HTTP MCP 的认证或兼容性无法满足要求，再评估独立 `ica-mcp` 进程。该进程只能通过 ICA-RUST 的受控内部 API 调用 Application Service，不得自行连接 SQLite。

因此建议建立一个明确的技术决策门：

| 验证项目   | HTTP MCP         | stdio MCP        |
| ------ | ---------------- | ---------------- |
| 业务服务复用 | 直接复用同一进程         | 需要内部 API 或共享服务组合 |
| 额外进程   | 不需要              | 需要               |
| 本地网络暴露 | 有本地监听面           | 无额外 HTTP 监听      |
| 认证方式   | 短期 Token / 本地授权  | 进程身份 + 短期凭据      |
| 双平台部署  | 相对简单             | 需要额外进程管理         |
| 故障隔离   | 与 ICA Server 同进程 | MCP 进程可独立故障      |
| 首期建议   | 优先 POC           | 兼容性不满足时采用        |

无论最终选择哪种方式，MCP 都只是协议适配层，不是权限边界。权限边界必须位于 `ica-application`。

## **四、Agent 领域模型与数据设计**

Agent 数据要纳入 ICA 自己的数据库和迁移体系，但不能与 Hermes 内部 Session/Memory 混在一起。

### **4.1 建议的 ICA 数据实体**

| 实体                  | 关键字段                                                   | 作用               |
| ------------------- | ------------------------------------------------------ | ---------------- |
| `agent_runtime`     | id、runtime_type、version、status                         | 记录 Runtime 注册与状态 |
| `agent_session`     | id、runtime_id、external_session_id、user_id              | ICA 会话索引         |
| `agent_task`        | id、session_id、task_type、status、context_ref             | Agent 任务         |
| `agent_execution`   | id、task_id、started_at、ended_at、result_status           | 一次执行记录           |
| `agent_tool_call`   | id、execution_id、tool_name、arguments_hash、result_status | 工具调用证据           |
| `agent_permission`  | id、principal、scope、capability、expires_at               | Agent 授权         |
| `agent_approval`    | id、task_id、operation_hash、status、approved_by           | 人工审批             |
| `agent_audit_event` | id、actor、action、resource、timestamp                     | 安全审计             |

以上是目标模型，不代表现有迁移仓已经具备这些表。正式建模时应先检查现有用户、权限、审计和业务实体，复用已有结构，避免重复建设。

### **4.2 关键数据关系**

```
User
 ├── AgentPermission
 └── AgentSession
       └── AgentTask
             ├── AgentExecution
             │     └── AgentToolCall
             └── AgentApproval

AgentAuditEvent
 ├── User
 ├── AgentSession
 ├── AgentTask
 └── AgentToolCall
```

### **4.3 数据所有权规则**

- 候选人、岗位、沟通历史、草稿：ICA-RUST。
- Agent 任务、授权、审批、工具调用证据：ICA-RUST。
- Hermes 对话历史、Memory、Skills、Runtime 配置：Hermes Profile。
- Hermes Session ID 与 ICA Session ID 之间的映射：ICA-RUST。
- 模型 API Key：由 ICA 安全配置或系统密钥管理设施管理，不能写入审计记录。
- Agent 对候选人的分析结果：如果需要作为业务记录保存，必须经过 ICA 明确的保存流程；不能仅因为 Hermes 生成了文本就自动写入候选人档案。

### **4.4 Agent 权限模型**

建议采用 Capability + Scope + Approval 三层控制。

| 控制层        | 约束内容           | 示例                            |
| ---------- | -------------- | ----------------------------- |
| Capability | Agent 可以做什么    | read_candidate、generate_draft |
| Scope      | Agent 可以访问哪些对象 | 当前候选人、当前岗位                    |
| Approval   | 哪些操作需要人工批准     | 保存跟进建议、更新业务记录                 |

初期只开放以下能力：

- 查询当前授权候选人。
- 查询当前授权岗位。
- 查询候选人沟通历史。
- 生成沟通草稿。
- 查询跟进事项。
- 提交待审批跟进建议。

**永远不向 Agent 暴露招聘平台消息发送能力。**

此外，必须保留以下状态差异：

```
Draft Generated
      ↓
Draft Reviewed
      ↓
Approved
      ↓
Copied
      ↓
User Sent Manually
```

`Approved` 不代表 `Copied`，`Copied` 不代表 `Sent`。Agent 不得将用户尚未确认的动作伪装为已完成。

## **五、MCP Tool 契约设计**

首期建议将 MCP Tool 分成只读、生成和审批写入三类。

### **5.1 首期工具列表**

| Tool                            | 分类             | 权限                | 首期状态 |
| ------------------------------- | -------------- | ----------------- | ---- |
| `ica.ping`                      | System         | Runtime           | POC  |
| `ica.get_current_context`       | Read           | 当前工作区授权           | 首期   |
| `ica.search_candidates`         | Read           | Candidate Scope   | 首期   |
| `ica.get_candidate_context`     | Read           | Candidate Scope   | 首期   |
| `ica.get_communication_history` | Read           | Candidate Scope   | 首期   |
| `ica.get_position_context`      | Read           | Position Scope    | 首期   |
| `ica.list_follow_up_tasks`      | Read           | Candidate Scope   | 首期   |
| `ica.generate_message_draft`    | Generate       | Draft Capability  | 首期   |
| `ica.get_draft_details`         | Read           | Draft Scope       | 首期   |
| `ica.create_follow_up_proposal` | Write Proposal | Approval Required | 后续   |

建议按垂直切片逐个增加，不一次性把所有招聘领域工具都暴露给 Hermes。

### **5.2 统一 Tool 请求上下文**

每次工具调用都必须有服务端可验证的执行上下文：

```
{
  "request_id": "req_xxx",
  "session_id": "session_xxx",
  "task_id": "task_xxx",
  "candidate_id": "candidate_xxx",
  "arguments": {}
}
```

注意：上面只是逻辑示例。`user_id`、Runtime 身份、权限范围和审批状态不能由模型自由填写后直接信任，必须由 ICA 服务端从已认证的 Runtime Session 中解析。

### **5.3 统一响应结构**

```
{
  "ok": true,
  "data": {},
  "error": null,
  "meta": {
    "request_id": "req_xxx",
    "contract_version": "1.0"
  }
}
```

错误至少区分：

- `UNAUTHENTICATED`
- `PERMISSION_DENIED`
- `SCOPE_VIOLATION`
- `RESOURCE_NOT_FOUND`
- `APPROVAL_REQUIRED`
- `APPROVAL_EXPIRED`
- `CONTRACT_VERSION_MISMATCH`
- `RATE_LIMITED`
- `INTERNAL_ERROR`

所有工具输入输出都应有 JSON Schema，并进入 Rust 单元测试和 MCP 契约测试。

### **5.4 草稿生成流程**

```
Hermes
  │
  ├── get_candidate_context
  │
  ├── get_communication_history
  │
  ├── get_position_context
  │
  └── generate_message_draft
          │
          ▼
    ICA Application
          │
          ├── Validate Candidate Scope
          ├── Validate Draft Policy
          ├── Persist Draft
          └── Audit Tool Call
                  │
                  ▼
            ICA Agent Workspace
                  │
                  ▼
             User Review
                  │
                  ▼
            Copy / Manual Send
```

### **5.5 写操作的审批机制**

任何 Agent 写入业务数据的操作都必须先生成提案，而不是直接修改。

```
Agent Proposal
    │
    ▼
ICA Approval Manager
    │
    ├── Validate User Permission
    ├── Validate Resource Scope
    ├── Calculate Operation Hash
    ├── Persist Pending Approval
    └── Return Approval ID
           │
           ▼
       User Approval
           │
           ▼
    Revalidate Operation Hash
           │
           ▼
    Execute Application Use Case
           │
           ▼
       Audit Result
```

审批应绑定具体参数和资源版本。审批后如果候选人、草稿或目标数据发生变化，必须重新校验，不能拿旧审批执行新参数。

## **六、分阶段实施计划**

整体采用两个并行轨道：

- **轨道 A：ICA-RUST 后端迁移收敛。** 目标是完成 68 条 API 契约迁移、跨语言对拍、质量门和独立验收。
- **轨道 B：Hermes Runtime 集成预研。** 目标是验证 Runtime、MCP 和桌面进程管理，不提前依赖尚未完成的业务接口。

两个轨道在后端迁移达到稳定门禁后汇合，进入 Agent 业务集成。

## **整体实施路线**

1. **G0：迁移基线治理**

   必须先完成

   重锚 Python 对拍基线、处理端点差异、核验 W4、隔离 OCR 在途工作。
2. **G1：W4 验收与 OCR 专项收敛**

   分别处理 W4 独立审计和 OCR 引擎的测试、格式、依赖及模型资源问题。
3. **G2：W5 + W6 后端迁移**

   完成剩余 23 条端点，补齐跨语言对拍、桌面 IPC 和文档。
4. **G3：Hermes Runtime POC**

   并行验证独立 Profile、Gateway、Session、事件流、MCP 连接和进程恢复。
5. **G4：ICA Agent 基础设施**

   建立 Agent Domain、Application Use Cases、授权、审计和 MCP Adapter。
6. **G5：首个招聘业务垂直切片**

   完成候选人上下文读取、分析、草稿生成、用户审核闭环。
7. **G6：Agent Workspace 与 Skills**

   集成对话 UI、任务状态、工具调用事件、审批卡片和招聘领域 Skills。
8. **G7：Hybrid AI 与产品交付**

   完成本地/云端模型路径、出网控制、双平台打包、升级和故障恢复验证。

### **G0：迁移基线治理**

P0 · 阻断正式迁移验收

| 任务    | 具体工作                                          | 交付物                       | 验收条件       |
| ----- | --------------------------------------------- | ------------------------- | ---------- |
| G0-01 | 固定 ICA-RUST HEAD、工作树、Cargo.lock、tag 清单        | Migration Snapshot        | 基线可复现      |
| G0-02 | 核对 `d39e321` 与当前 Python HEAD 的差异              | Python Baseline Report    | 每项差异有归属    |
| G0-03 | 对四条 `ai/models` 和一条 `confirm-candidate` 做范围决策 | Endpoint Scope ADR        | 明确迁移/不迁移   |
| G0-04 | 修正 Dashboard 对拍参照，确认 calendar 是否属于契约          | Dashboard Parity Decision | 不因错误基线扩大契约 |
| G0-05 | 独立复跑 W4 对拍和质量门                                | W4 Audit Report           | 结果有日志和证据   |
| G0-06 | 补齐 W4 文档与 declared improvements               | W4 Documentation Update   | 文档与代码一致    |
| G0-07 | 更新 `check_endpoint_map.py` 所依赖的权威映射           | Endpoint Mapping Baseline | 映射差异有明确解释  |

特别注意：

- 不要直接把 `/mnt/o/ica` 当前工作树当成新的对拍基线。
- 不要为了让 Dashboard 220/222 变成 222/222，擅自把 `calendar` 加入 Rust。
- 五条端点的范围决策必须由项目负责人明确，不应由执行 Agent 自行推断。
- `w4-done` 应在独立审计通过后创建，不能先补 Tag 再补证据。

**G0 完成标准：** 形成一份不可歧义的迁移基线记录，明确每条契约的来源、归属、当前状态和验收方式。

### **G1：W4 验收与 OCR 专项收敛**

建议把当前真 OCR 工作从 W4 原始契约验收中分离，作为一个独立补充交付。

原因是原 W4 范围明确没有真 OCR 引擎，而当前新增的 RapidOCR 已经引入 ONNX Runtime、169 个新增依赖包、模型目录和异步运行时问题。它不仅是一个普通测试修复，而是一个新的工程能力。

建议拆成两个工作包：

**W4-A：原始 W4 契约验收**

- 对 7 条 W4 端点做独立代码审计。
- 检查导入、导出、OCR 服务调用路径是否符合冻结基线。
- 复跑跨语言对拍。
- 处理文档中 W4 七行状态和 §8 统计陈旧问题。
- 补充 W4 对应的 §7.1 declared improvements。
- 通过后再建立 `w4-done`。

**OCR-01：RapidOCR Engine 专项**

| 子任务       | 具体内容                         | 验收标准                               |
| --------- | ---------------------------- | ---------------------------------- |
| OCR-01-01 | 确认 RapidOCR crate/API 版本及许可证 | 依赖来源、版本和许可证记录完整                    |
| OCR-01-02 | 修复 Tokio Runtime Drop panic  | staging 目录用例稳定通过                   |
| OCR-01-03 | 修复 `ocr_rapid.rs` 格式         | `cargo fmt --check` 通过             |
| OCR-01-04 | 模型目录配置                       | 不依赖基线 Python venv 的隐式路径            |
| OCR-01-05 | 模型缓存锁文件处理                    | `.rapidocr-model-cache.lock` 不进入仓库 |
| OCR-01-06 | 模型缺失和初始化失败处理                 | 错误可诊断，不 panic                      |
| OCR-01-07 | OCR 图像测试                     | 真图、非图、损坏图、模型缺失用例通过                 |
| OCR-01-08 | 跨语言 OCR 对拍                   | 明确 OCR 结果、错误类型和落库行为                |
| OCR-01-09 | 依赖和体积评估                      | ONNX Runtime 分发及平台兼容有结论            |

关于当前 panic，建议专项定位：

- 是否在 Tokio 异步上下文中创建或销毁独立 Runtime。
- 是否在异步任务中调用了会阻塞的同步接口。
- OCR Engine 是否持有一个独立 Runtime，并在异步上下文中被 Drop。
- 是否存在 `block_on` 或同步桥接引起的嵌套 Runtime 生命周期问题。

不建议只用 `spawn_blocking` 包裹整个 OCR 调用来掩盖生命周期错误。应先明确 Engine 的 Runtime 所有权和执行模型，再决定修复方式。

模型路径方面，**不能把基线 Python venv 中的&#x20;**`rapidocr/models`**&#x20;作为最终交付依赖**。建议由 ICA-RUST 自己定义模型资源目录、版本校验、下载/安装策略和 SHA-256 校验。离线安装包是否内置模型，需要单独做体积与许可评估。

缓存锁文件应由 OCR 运行时创建在应用数据目录，而不是源码目录，例如：

```
ICA User Data/
└── models/
    └── rapidocr/
        ├── manifest.json
        ├── models/
        └── .cache.lock
```

不要把运行时缓存文件放在 `crates/ica-api/models/`。

### **G2：完成 W5 + W6 后端迁移**

P0 · 核心迁移工作

剩余 23 条端点是当前迁移收敛的主要工作量。

| 波次       | 模块                         | 数量     | 建议子任务                      |
| -------- | -------------------------- | ------ | -------------------------- |
| W5-AI    | AI                         | 6      | Provider、模型列表、配置、调用契约和错误行为 |
| W5-OPS   | Settings / Audit / Backups | 9      | 设置读写、审计查询、备份与恢复            |
| W6-AUTH  | Auth                       | 5      | 身份、登录、会话和授权相关行为            |
| W6-SHARE | Shares                     | 3      | 分享资源、访问控制和失效行为             |
| **合计**   |                            | **23** |                            |

具体 API 级任务必须以 `rust-migration-contract.md` 的 68 条表格为准，不建议仅根据模块名称推测端点行为。

#### **W5 实施要求**

1. 对每条 API 建立 Python 行为基线。
2. 实现 Rust Handler 和 Application Use Case。
3. 复用已有 Domain 和 Infrastructure，不重复写业务逻辑。
4. 更新 ica-desktop 需要同步的 IPC 命令。
5. 补充跨语言对拍用例。
6. 对有副作用的接口同时比较最终数据库状态。
7. 更新契约表和波次文档。

W5 中的 AI 接口需要特别注意：它们是 ICA 原有 AI 能力迁移，不等于 Hermes Runtime 集成。不能把 Hermes 的 Agent 对话 API 混进原有 AI Provider API，也不能让 Hermes 接管原有草稿审核流程。

#### **W6 实施要求**

Auth 与 Shares 涉及安全边界，建议不要简单以 HTTP 200 和响应 JSON 相同作为验收。

还要验证：

- 未授权访问是否一致。
- Token 过期和失效是否一致。
- 跨用户数据是否被拒绝。
- 分享资源撤销后是否立即失效。
- 权限变化后是否能正确拒绝旧 Session。
- 错误响应是否泄露敏感信息。

#### **W5/W6 完成门禁**

- 每条端点均有对应测试。
- 68 条迁移契约完成或有明确批准的范围例外。
- 跨语言对拍通过。
- 数据库最终状态对拍通过。
- `scripts/check.sh` 四关通过。
- `scripts/check_endpoint_map.py` 不再出现未解释的 MISMATCH。
- 文档、Tag、看板与代码状态一致。

### **G3：Hermes Runtime POC**

可与 G0–G2 并行

这个阶段不依赖候选人数据，不开放真实招聘业务工具，只验证运行时集成能力。

#### **POC 架构**

```
ICA Desktop / Tauri
    │
    ├── AgentRuntimeSupervisor
    │       ├── Start
    │       ├── Health Check
    │       ├── Stop
    │       └── Crash Recovery
    │
    └── Hermes Runtime
            ├── Dedicated Profile
            ├── Session
            ├── Event Stream
            └── MCP Client
                    │
                    ▼
               ICA MCP POC
                    └── ica.ping
```

#### **POC 子任务**

| 编号       | 任务                       | 验收标准                     |
| -------- | ------------------------ | ------------------------ |
| H-POC-01 | 固定 Hermes 版本             | 记录 CLI 版本、安装方式和配置        |
| H-POC-02 | 建立 ICA 专属 Hermes Profile | 不复用默认 Profile 的用户级状态     |
| H-POC-03 | 验证 Runtime 启动和停止         | 可重复启动、优雅退出               |
| H-POC-04 | 验证 Gateway/JSON-RPC      | Session 创建、恢复、消息提交可用     |
| H-POC-05 | 验证流式事件                   | UI 可接收消息和执行事件            |
| H-POC-06 | 验证 Interrupt             | 能取消正在运行的任务               |
| H-POC-07 | 验证 MCP                   | Hermes 可发现并调用 `ica.ping` |
| H-POC-08 | 验证审批事件                   | 能接收审批请求并响应               |
| H-POC-09 | 验证异常恢复                   | Hermes 崩溃后 ICA 核心业务不受影响  |
| H-POC-10 | 验证双平台                    | Windows、macOS 生命周期与打包可行  |

建议首选通过 Hermes Gateway 的程序化 JSON-RPC 接口管理 Runtime，而不是让 React UI 直接操作 Hermes CLI。Tauri Host 负责进程管理和 Gateway 连接，React 只接收 ICA 定义的 Agent 事件。

POC 必须记录实际 Hermes 版本支持的 RPC、Gateway owner 能力声明、认证方式和 MCP 传输能力。不能直接把文档中出现的所有 RPC 名称视为用户当前安装版本均支持。

**G3 交付物：**

- Hermes Runtime POC 报告。
- Runtime Adapter 接口定义。
- Hermes 版本兼容矩阵。
- Gateway/MCP 通信方案决策。
- Windows/macOS 进程生命周期测试结果。

### **G4：ICA Agent 基础设施**

该阶段在 W5/W6 迁移达到稳定状态后正式启动。

建议按以下顺序实施：

| 编号    | 模块                  | 实施内容                               | 主要验收       |
| ----- | ------------------- | ---------------------------------- | ---------- |
| AG-01 | Agent Domain        | Task、Session、Execution、Approval 状态 | 状态转换测试     |
| AG-02 | Agent Application   | 创建任务、运行任务、终止任务、审批用例                | 用例测试       |
| AG-03 | Permission Policy   | Capability、Scope、授权校验              | 越权测试       |
| AG-04 | Audit               | Runtime、Task、Tool Call、Approval 审计 | 可追溯测试      |
| AG-05 | Agent Contracts     | DTO、JSON Schema、错误码                | 契约测试       |
| AG-06 | MCP Adapter         | Tool 注册、输入输出验证、调用分发                | MCP 集成测试   |
| AG-07 | Runtime Integration | Hermes Session 与 ICA Session 映射    | 会话恢复测试     |
| AG-08 | Persistence         | Agent 相关数据表和迁移                     | 数据库升级/回滚测试 |

#### **Runtime Adapter 接口建议**

桌面侧的抽象接口可以先定义为：

```
pub trait AgentRuntimeAdapter {
    async fn install_check(&self) -> Result<RuntimeInfo>;
    async fn start(&self) -> Result<RuntimeInfo>;
    async fn health_check(&self) -> Result<RuntimeHealth>;
    async fn create_session(&self) -> Result<RuntimeSession>;
    async fn resume_session(&self, id: &str) -> Result<RuntimeSession>;
    async fn submit_prompt(&self, request: PromptRequest) -> Result<()>;
    async fn interrupt(&self, session_id: &str) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}
```

以上是接口设计示例，最终类型和异步 Runtime 应遵循 `ica-desktop` 当前实现方式，不能在尚未检查现有 Tauri 命令结构时直接复制落地。

Agent Runtime 与 ICA 业务后端之间应通过事件桥接传递状态，例如：

```
agent.runtime.started
agent.runtime.stopped
agent.session.created
agent.session.message.delta
agent.task.started
agent.task.completed
agent.task.failed
agent.tool_call.started
agent.tool_call.completed
agent.approval.required
agent.approval.resolved
```

这些事件要有稳定的 `event_id`、`session_id`、`task_id` 和时间戳，以便断线重连后恢复状态。

### **G5：首个业务垂直切片**

首期 Agent MVP

首期不要建设一个可以随意操作招聘系统的通用 Agent。应以候选人分析与沟通草稿为唯一主场景。

**用户流程：**

1. 用户打开 ICA 中某个候选人。
2. 用户打开 Agent Workspace。
3. ICA 将当前候选人和岗位的授权上下文传递给 Agent。
4. Hermes 调用 MCP 读取候选人资料和沟通历史。
5. Hermes 生成分析结论。
6. Hermes 请求 ICA 生成沟通草稿。
7. ICA 保存草稿，返回 Draft ID。
8. 用户在 ICA 中审核、编辑和批准。
9. 用户复制内容，在招聘平台手动发送。
10. ICA 根据用户实际操作记录沟通结果。

首期仅开放五个核心工具：

- `ica.get_candidate_context`
- `ica.get_communication_history`
- `ica.get_position_context`
- `ica.generate_message_draft`
- `ica.get_draft_details`

`ica.search_candidates` 可以作为后续扩展，因为首个垂直切片已经处于候选人详情页，不需要让 Agent 自行搜索整个候选人库。

#### **垂直切片验收**

| 验收项        | 通过条件                     |
| ---------- | ------------------------ |
| 上下文隔离      | Agent 不能读取当前授权范围以外的候选人   |
| 信息依据       | 生成结果能追溯到候选人资料和沟通记录       |
| 草稿生成       | 生成草稿进入 ICA 原有 Draft 流程   |
| 重复调用       | 重复请求不会产生不可控的重复业务数据       |
| 用户审核       | Agent 不可绕过用户审核           |
| 手动发送       | 不存在招聘消息自动发送工具            |
| 审计         | 可以还原一次完整 Agent 执行链       |
| Runtime 故障 | Hermes 不可用时，原有 ICA 业务仍可用 |

### **G6：Agent Workspace 与 Skills**

建议 UI 采用现有 ICA 工作台的嵌入式区域，不要首期单独开发复杂的 Agent 管理中心。

主要 UI 模块：

| UI 模块        | 功能                    |
| ------------ | --------------------- |
| Agent 对话区    | 用户与 Hermes 交互         |
| 当前上下文栏       | 显示当前候选人、岗位和授权范围       |
| Task 状态区     | 显示运行中、完成、失败、已取消       |
| Tool Call 记录 | 显示 Agent 使用了哪些 ICA 工具 |
| Approval 卡片  | 展示待审批操作及具体影响          |
| 结果区          | 显示候选人分析和沟通草稿          |
| Runtime 状态   | 显示 Hermes 可用性及错误      |

首期 Skills 建议：

1. `ica-candidate-analysis`
2. `ica-communication-assistant`
3. `ica-follow-up-manager`
4. `ica-recruitment-report`

Skills 应由 ICA 负责版本管理、签名/校验和安装策略。Hermes 负责加载和执行，但不能通过修改 Skill 文件扩大自身权限。

### **G7：Hybrid AI 与产品交付**

这一阶段应建立在 G5 的本地 Agent 流程已经稳定之后。

目标是让 Hermes Runtime 可以使用两种推理路径：

```
Hermes Agent Runtime
       │
       ▼
ICA Model Routing Policy
       │
       ├── Local Model Provider
       │
       └── Managed Cloud Model
                │
                ▼
         ICA Model Gateway
                │
                ├── User Consent
                ├── PII Redaction
                ├── Provider Routing
                ├── Budget / Usage
                └── Audit
```

重要约束：

- Hermes 不能通过自有 Provider 配置绕开 ICA 的云端出网策略。
- 用户未授权时，不得把候选人资料发送到云端。
- 本地模型不可用时，不能静默切换到云端。
- 模型 Key 不写入 SQLite 明文或日志。
- 用户可以明确知道当前使用的是本地模型还是云端模型。
- 云端模型调用应与原有 ICA AI Provider Adapter 的调用策略协调，不重复建设两套无关的凭据和预算系统。

双平台交付至少需要覆盖：

- Hermes Runtime 安装与版本检查。
- 专属 Profile 初始化。
- 模型配置导入与密钥保护。
- Runtime 升级和回滚。
- 进程异常退出恢复。
- 日志和诊断信息。
- 用户卸载后的 Agent 数据处理。
- 离线状态下 ICA 核心业务正常工作。

## **七、任务依赖与并行分工**

为了避免多个 Agent 同时修改同一模块，建议把实施任务划分成独立工作包。

### **7.1 依赖关系**

\#chatgpt-mermaid-\_r_1tm\_{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;fill:rgb(237, 237, 237);}@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}#chatgpt-mermaid-\_r_1tm\_ .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_1tm\_ .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}#chatgpt-mermaid-\_r_1tm\_ .error-icon{fill:rgb(48, 48, 48);}#chatgpt-mermaid-\_r_1tm\_ .error-text{fill:rgb(237, 237, 237);stroke:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ .edge-thickness-normal{stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .edge-thickness-thick{stroke-width:3.5px;}#chatgpt-mermaid-\_r_1tm\_ .edge-pattern-solid{stroke-dasharray:0;}#chatgpt-mermaid-\_r_1tm\_ .edge-thickness-invisible{stroke-width:0;fill:none;}#chatgpt-mermaid-\_r_1tm\_ .edge-pattern-dashed{stroke-dasharray:3;}#chatgpt-mermaid-\_r_1tm\_ .edge-pattern-dotted{stroke-dasharray:2;}#chatgpt-mermaid-\_r_1tm\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_1tm\_ .marker.cross{stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_1tm\_ svg{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:16px;}#chatgpt-mermaid-\_r_1tm\_ p{margin:0;}#chatgpt-mermaid-\_r_1tm\_ .label{font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ .cluster-label text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ .cluster-label span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ .cluster-label span p{background-color:transparent;}#chatgpt-mermaid-\_r_1tm\_ .label text,#chatgpt-mermaid-\_r_1tm\_ span{fill:rgb(237, 237, 237);color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ .node rect,#chatgpt-mermaid-\_r_1tm\_ .node circle,#chatgpt-mermaid-\_r_1tm\_ .node ellipse,#chatgpt-mermaid-\_r_1tm\_ .node polygon,#chatgpt-mermaid-\_r_1tm\_ .node path{fill:rgb(9, 23, 44);stroke:rgb(31, 78, 148);stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .rough-node .label text,#chatgpt-mermaid-\_r_1tm\_ .node .label text,#chatgpt-mermaid-\_r_1tm\_ .image-shape .label,#chatgpt-mermaid-\_r_1tm\_ .icon-shape .label{text-anchor:middle;}#chatgpt-mermaid-\_r_1tm\_ .node .katex path{fill:#000;stroke:#000;stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .rough-node .label,#chatgpt-mermaid-\_r_1tm\_ .node .label,#chatgpt-mermaid-\_r_1tm\_ .image-shape .label,#chatgpt-mermaid-\_r_1tm\_ .icon-shape .label{text-align:center;}#chatgpt-mermaid-\_r_1tm\_ .node.clickable{cursor:pointer;}#chatgpt-mermaid-\_r_1tm\_ .root .anchor path{fill:rgb(175, 175, 175)!important;stroke-width:0;stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_1tm\_ .arrowheadPath{fill:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_1tm\_ .edgePath .path{stroke:rgb(175, 175, 175);stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .flowchart-link{stroke:rgb(175, 175, 175);fill:none;}#chatgpt-mermaid-\_r_1tm\_ .edgeLabel{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_1tm\_ .edgeLabel p{background-color:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_1tm\_ .edgeLabel rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_1tm\_ .labelBkg{background-color:rgba(0, 0, 0, 0.5);}#chatgpt-mermaid-\_r_1tm\_ .cluster rect{fill:rgb(48, 48, 48);stroke:rgba(255, 255, 255, 0.15);stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .cluster text{fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ .cluster span{color:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ div.mermaidTooltip{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";font-size:12px;background:rgb(48, 48, 48);border:1px solid rgba(255, 255, 255, 0.15);border-radius:2px;pointer-events:none;z-index:100;}#chatgpt-mermaid-\_r_1tm\_ .flowchartTitleText{text-anchor:middle;font-size:18px;fill:rgb(237, 237, 237);}#chatgpt-mermaid-\_r_1tm\_ rect.text{fill:none;stroke-width:0;}#chatgpt-mermaid-\_r_1tm\_ .icon-shape,#chatgpt-mermaid-\_r_1tm\_ .image-shape{background-color:rgb(0, 0, 0);text-align:center;}#chatgpt-mermaid-\_r_1tm\_ .icon-shape p,#chatgpt-mermaid-\_r_1tm\_ .image-shape p{background-color:rgb(0, 0, 0);padding:2px;}#chatgpt-mermaid-\_r_1tm\_ .icon-shape .label rect,#chatgpt-mermaid-\_r_1tm\_ .image-shape .label rect{opacity:0.5;background-color:rgb(0, 0, 0);fill:rgb(0, 0, 0);}#chatgpt-mermaid-\_r_1tm\_ .label-icon{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}#chatgpt-mermaid-\_r_1tm\_ .node .label-icon path{fill:currentColor;stroke:revert;stroke-width:revert;}#chatgpt-mermaid-\_r_1tm\_ .node .neo-node{stroke:rgb(31, 78, 148);}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node rect,#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].cluster rect,#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node polygon{stroke:url(#chatgpt-mermaid-\_r_1tm\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].swimlane.cluster rect{filter:none;}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node path{stroke:url(#chatgpt-mermaid-\_r_1tm\_-gradient);stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node .outer-path{filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node .neo-line path{stroke:rgb(31, 78, 148);filter:none;}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node circle{stroke:url(#chatgpt-mermaid-\_r_1tm\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].node circle .state-start{fill:#000000;}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].icon-shape .icon{fill:url(#chatgpt-mermaid-\_r_1tm\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_1tm\_ [data-look="neo"].icon-shape .icon-neo path{stroke:url(#chatgpt-mermaid-\_r_1tm\_-gradient);filter:drop-shadow( 1px 2px 2px rgba(185,185,185,1));}#chatgpt-mermaid-\_r_1tm\_ .node text{font-size:14px;font-weight:600;letter-spacing:normal;fill:rgb(153, 206, 255);}#chatgpt-mermaid-\_r_1tm\_ .edgeLabels text{font-size:13px;font-weight:600;letter-spacing:-0.08px;fill:rgb(153, 206, 255);}#chatgpt-mermaid-\_r_1tm\_ .node tspan[font-weight="normal"],#chatgpt-mermaid-\_r_1tm\_ .edgeLabels tspan[font-weight="normal"]{font-weight:600;}#chatgpt-mermaid-\_r_1tm\_ .edgeLabel .label rect{opacity:1;rx:13px;ry:13px;fill:rgb(0, 14, 26);stroke:rgb(26, 62, 95);stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .node rect,#chatgpt-mermaid-\_r_1tm\_ .node circle,#chatgpt-mermaid-\_r_1tm\_ .node ellipse,#chatgpt-mermaid-\_r_1tm\_ .node polygon,#chatgpt-mermaid-\_r_1tm\_ .node path{fill:rgb(0, 40, 77);stroke:rgba(255, 255, 255, 0.1);stroke-width:1px;}#chatgpt-mermaid-\_r_1tm\_ .node rect{rx:16px;ry:16px;}#chatgpt-mermaid-\_r_1tm\_ .mermaid-decision .label-container{fill:rgb(0, 14, 26);stroke:rgb(26, 62, 95);stroke-dasharray:2,2;}#chatgpt-mermaid-\_r_1tm\_ .edgePaths .flowchart-link{stroke:rgb(175, 175, 175);stroke-width:1px;stroke-linecap:round;stroke-linejoin:round;}#chatgpt-mermaid-\_r_1tm\_ .marker{fill:rgb(175, 175, 175);stroke:rgb(175, 175, 175);}#chatgpt-mermaid-\_r_1tm\_ :root{--mermaid-font-family:-apple-system-body,ui-sans-serif,-apple-system,system-ui,"Segoe UI",Helvetica,"Apple Color Emoji",Arial,sans-serif,"Segoe UI Emoji","Segoe UI Symbol";}G0 基线治理G1 W4 验收与 OCR 专项G2 W5/W6 迁移G4 Agent 基础设施G3 Hermes Runtime POCG5 招聘业务垂直切片G6 Workspace 与 SkillsG7 Hybrid AI 与交付

G3 可以在 G0–G2 期间并行执行，因为它不需要真实业务数据，也不需要改动 ICA 业务 API。

### **7.2 建议多 Agent 分工**

| 执行角色             | 负责范围                  | 主要交付物               | 不允许越界            |
| ---------------- | --------------------- | ------------------- | ---------------- |
| Engineering Lead | 基线、架构、ADR、任务协调        | 决策记录、集成方案           | 不替代独立审计          |
| Migration Agent  | W5/W6 端点迁移            | Rust API、测试、对拍      | 不改变冻结契约          |
| OCR Agent        | RapidOCR 专项           | Engine、模型管理、OCR 测试  | 不修改 W5/W6        |
| Audit Agent      | W4/W5/W6 独立审计         | 证据、差异和阻断项           | 不直接修改被审计实现       |
| Runtime Agent    | Hermes POC、Tauri 进程管理 | Runtime Adapter POC | 不接触真实候选人数据       |
| MCP Agent        | MCP Tool 契约与 Adapter  | MCP 接口、契约测试         | 不直接访问数据库         |
| Security Agent   | 权限、审批、注入防护            | 威胁模型、安全测试           | 不以 Skill 替代服务端授权 |
| UI Agent         | Agent Workspace       | React/Tauri UI      | 不实现业务规则          |
| Integration Lead | 集成验证和回归               | 集成报告                | 不擅自合并或发布         |

### **7.3 分支和工作区建议**

当前 `ICA-RUST` 是独立 Git 仓库，`main` 无 remote、未 push。建议继续保持这一治理方式。

建议建立以下工作分支：

```
main
│
├── audit/w4-independent
├── fix/ocr-rapid-engine
├── migration/w5
├── migration/w6
├── poc/hermes-runtime
│
└── feature/embedded-agent
      ├── feature/agent-domain
      ├── feature/agent-mcp
      ├── feature/agent-security
      └── feature/agent-workspace
```

这只是建议的分支规划。是否创建、何时合并，应由你明确授权。

尤其需要注意：

- W5/W6 不应与 OCR 共用一个开发分支。
- Hermes POC 不应修改迁移契约。
- Agent 新增端点必须走新的 Agent 契约，不得混入原来的 68 条迁移清单。
- 不能因为 Agent 新增 MCP 接口，就把原有 `/api` 端点数量和迁移进度混为一谈。
- 不自动 commit、push、merge 或 release。

## **八、工程质量门与最终验收标准**

建议把 ICA-RUST 迁移质量门和 Agent 集成质量门分开管理，避免 Agent 功能完成后掩盖原有后端质量问题。

### **8.1 ICA-RUST 迁移质量门**

| Gate   | 检查项          | 通过标准                          |
| ------ | ------------ | ----------------------------- |
| MIG-01 | 分层约束         | `scripts/check.py` 通过         |
| MIG-02 | 格式检查         | `cargo fmt --check` 通过        |
| MIG-03 | Clippy       | `cargo clippy -D warnings` 通过 |
| MIG-04 | 单元与集成测试      | `cargo test` 全部通过             |
| MIG-05 | Endpoint Map | 契约映射无未解释差异                    |
| MIG-06 | 跨语言对拍        | 所有纳入范围的用例通过                   |
| MIG-07 | 数据库状态对拍      | 最终落库行符合基线                     |
| MIG-08 | 波次文档         | 代码、契约、Tag、看板一致                |
| MIG-09 | 独立审计         | 审计人员确认无阻断问题                   |

`cargo test` 不能只看总通过数量，还要记录测试总数、失败用例、忽略用例和测试环境。独立审计必须使用干净快照，避免把工作树在途改动混入已验收结果。

### **8.2 Agent 集成质量门**

| Gate     | 检查项              | 通过标准                               |
| -------- | ---------------- | ---------------------------------- |
| AGENT-01 | Runtime          | 启动、停止、异常恢复稳定                       |
| AGENT-02 | Session          | 创建、恢复、取消和事件重连正常                    |
| AGENT-03 | MCP              | Tool Schema 和协议兼容测试通过              |
| AGENT-04 | Authorization    | 无授权调用全部拒绝                          |
| AGENT-05 | Resource Scope   | 不允许跨候选人、岗位越权                       |
| AGENT-06 | Approval         | 写入必须经过人工审批                         |
| AGENT-07 | Audit            | Runtime、Session、Task、Tool Call 可追溯 |
| AGENT-08 | Injection        | 不可信候选人文本不能覆盖系统策略                   |
| AGENT-09 | Privacy          | 云端出网授权和脱敏有效                        |
| AGENT-10 | Product Boundary | 不存在自动发送招聘消息能力                      |
| AGENT-11 | Resilience       | Hermes 故障不影响 ICA 核心业务              |
| AGENT-12 | Packaging        | Windows/macOS 安装、升级和卸载通过           |

### **8.3 关键端到端验收场景**

至少准备以下测试：

1. 未启动 Hermes 时，ICA 候选人管理功能正常。
2. 首次打开 Agent Workspace，Runtime 自动启动。
3. Hermes 可以创建 Session 并正常返回消息。
4. Agent 读取当前候选人上下文成功。
5. Agent 尝试读取其他候选人，服务端拒绝。
6. 候选人沟通历史中的恶意指令不能改变 Agent 权限。
7. Agent 生成草稿，ICA 正常保存。
8. Agent 尝试未经授权修改候选人，服务端拒绝。
9. Agent 创建跟进提案，用户批准后才执行。
10. 用户中断 Agent，任务状态正确变更。
11. Hermes 崩溃，ICA 业务仍可继续使用。
12. 云端模型未授权时，候选人资料不出网。
13. Runtime 重启后，ICA 可以恢复会话关联信息。
14. 用户卸载 Agent Runtime，不影响 ICA 业务数据。

## **九、实施周期与里程碑**

以下是工程规划估算，不是对当前仓库剩余工时的实测。

| 阶段                  | 预计工作日 | 主要交付                    |
| ------------------- | ----- | ----------------------- |
| G0 基线治理             | 2–4   | 基线决策、W4 审计、范围确认         |
| G1 W4/OCR 收敛        | 3–7   | W4 放行、OCR 专项报告          |
| G2 W5/W6            | 8–15  | 23 条端点、对拍、桌面同步          |
| G3 Hermes POC       | 3–5   | Runtime、Gateway、MCP POC |
| G4 Agent 基础设施       | 5–8   | Agent Domain、权限、审计、MCP  |
| G5 业务垂直切片           | 4–7   | 候选人分析和草稿闭环              |
| G6 Workspace/Skills | 5–8   | Agent UI 和四个首期 Skill    |
| G7 Hybrid AI/交付     | 7–12  | 模型路由、双平台交付              |

粗略合计：37–66 个工程工作日。G3 可与 G0–G2 并行，因此日历周期会短于各阶段工作日简单相加。OCR 实际复杂度、W5/W6 测试缺口和双平台打包情况可能显著改变估算。

### **建议里程碑**

1. **M1：Rust Migration Baseline Frozen**
   - Python 基线已确认。
   - 68 条迁移范围已确认。
   - W4 已独立审计。
   - OCR 独立工作包已确定。
2. **M2：ICA-RUST Backend Complete**
   - W5/W6 完成。
   - 迁移质量门通过。
   - 68 条契约全部完成或有正式批准的范围例外。
3. **M3：Hermes Runtime Integrated**
   - Tauri 可以管理 Hermes。
   - Session 和事件流正常。
   - MCP 工具连接成功。
   - 异常退出不影响 ICA。
4. **M4：Agent Business MVP**
   - 候选人上下文读取。
   - 沟通历史分析。
   - 草稿生成。
   - 用户审核和手动发送。
5. **M5：ICA Embedded Agent Release Candidate**
   - 权限、审计、安全测试通过。
   - Skills 可管理。
   - Local/Cloud Hybrid 路径可用。
   - Windows/macOS 打包验收通过。

## **十、正式设计文档与仓库落地结构**

建议把 ICA-RUST 的迁移文档和 Agent 新架构文档放在同一仓库，但明确区分两套工程目标。

原有 `rust-migration-contract.md` 继续作为 Python → Rust 行为迁移契约，不把 Hermes 新增接口塞进 68 条迁移清单。

建议新增：

```
ICA-RUST/
├── docs/
│   ├── rust-migration-contract.md
│   ├── contract-baseline/
│   │
│   ├── architecture/
│   │   ├── agent/
│   │   │   ├── agent-architecture-r1.md
│   │   │   ├── agent-domain-model-r1.md
│   │   │   ├── agent-runtime-integration-r1.md
│   │   │   ├── agent-mcp-contract-r1.md
│   │   │   ├── agent-security-model-r1.md
│   │   │   ├── agent-data-model-r1.md
│   │   │   └── agent-hybrid-ai-r1.md
│   │   │
│   │   └── adr/
│   │       ├── ADR-AGENT-001-embedded-agent.md
│   │       ├── ADR-AGENT-002-runtime-integration.md
│   │       ├── ADR-AGENT-003-mcp-transport.md
│   │       ├── ADR-AGENT-004-permission-and-approval.md
│   │       └── ADR-AGENT-005-agent-data-ownership.md
│   │
│   ├── implementation/
│   │   ├── agent-implementation-plan-r1.md
│   │   ├── agent-task-breakdown-r1.md
│   │   └── agent-acceptance-matrix-r1.md
│   │
│   ├── audit/
│   │   ├── w4-independent-audit.md
│   │   ├── ocr-rapid-engine-audit.md
│   │   └── agent-security-audit.md
│   │
│   └── reports/
│       ├── hermes-runtime-poc-report.md
│       └── agent-integration-test-report.md
│
├── crates/
│   ├── ica-domain/
│   ├── ica-application/
│   ├── ica-contracts/
│   ├── ica-infra/
│   ├── ica-api/
│   ├── ica-server/
│   └── ica-desktop/
│
└── scripts/
    ├── check.py
    ├── check.sh
    ├── contract_verify.py
    └── check_endpoint_map.py
```

ADR 编号是 Agent 专项编号示例，正式落地前需要检查仓库当前 ADR 编号和命名规则，避免与现有 `ADR-0002` 冲突。

## **十一、当前需要冻结的架构决策**

以下是我建议的默认决策，尚不能视为已经批准的项目决策。

| 决策            | 建议                                 | 原因                     |
| ------------- | ---------------------------------- | ---------------------- |
| ICA 正式业务后端    | ICA-RUST                           | 与迁移目标一致                |
| Python 后端     | 迁移对拍基线，完成后退出产品运行时                  | 避免长期双后端                |
| Agent Runtime | Hermes 独立进程                        | 隔离故障与生命周期              |
| Runtime 管理    | Tauri Host                         | 与现有桌面宿主结构一致            |
| MCP Server    | 优先 ICA Server 内的 MCP HTTP Endpoint | 复用 Application Service |
| MCP 备选        | 独立 stdio MCP 进程                    | HTTP 方案无法满足兼容性时采用      |
| Agent 业务规则    | `ica-application`                  | 统一业务规则入口               |
| 权限与审批         | ICA-RUST 服务端                       | 不信任模型或 Skill           |
| Agent Session | ICA/Hermes 双侧关联                    | 分离业务和推理状态              |
| 首个业务场景        | 候选人分析与沟通草稿                         | 可形成完整业务闭环              |
| 自动发送          | 永久禁止                               | 保留人工发送边界               |
| 模型模式          | Local + Managed Cloud              | 保留本地优先和出网控制            |

### **11.1 三个需要特别注意的工程问题**

**第一，不能让 Hermes 成为第二个 ICA 后端。**

Agent 只能通过 MCP 使用 ICA-RUST 已公开的领域用例。不要在 Hermes Skill 里自行实现候选人状态管理、草稿保存或审批规则。

**第二，不能把 MCP 工具调用等同于权限授权。**

MCP 工具白名单只是能力暴露控制。真正的授权必须在 Application Service 每次执行时重新校验。即使 Hermes 被 Prompt Injection 操纵，也不能越权操作。

**第三，不能让 Agent 集成反向改变迁移验收口径。**

68 条 API 的迁移验收和新 Agent 功能验收是两个独立质量域。Agent 可以复用迁移完成的业务用例，但不能为了接入 Hermes 修改 Python 冻结基线、放宽对拍条件或把新端点计入迁移完成数。

## **十二、建议的实际执行顺序**

我建议现在不要立即开始 G4 的 Agent 业务代码开发，而是按照下面的顺序推进：

1. **先完成 G0。** 把 Python 基线、五条端点范围、Dashboard calendar 差异和 W4 审计口径冻结。
2. **将 OCR 拆成独立工作包。** 修复 Tokio Runtime 生命周期问题、格式问题、模型路径和缓存文件问题，避免污染 W4 验收。
3. **完成 W5/W6。** 先让 ICA-RUST 成为一个业务接口完整、契约稳定的后端。
4. **并行完成 Hermes POC。** 仅使用 `ica.ping`，不接入候选人和真实招聘数据。
5. **完成 Agent 基础设施。** 先实现权限、审计、Session 映射和 MCP Adapter。
6. **完成首个招聘业务闭环。** 从候选人上下文读取到草稿生成，再到人工审核和复制。
7. **最后建设 Skills、Hybrid AI 和双平台交付。**

**我建议把 G2 和 G3 设为汇合门槛：** ICA-RUST 后端契约稳定，同时 Hermes Runtime 的会话、事件和 MCP POC 验证通过后，才允许进入 G4。

这样可以避免同时调试 Rust 迁移、Hermes Runtime、MCP 协议和招聘业务逻辑，降低问题定位成本。

**本次方案依据：** 你提供的 ICA-RUST HEAD、端点进度、质量门、对拍结果、基线漂移和 OCR 在途工作信息，以及已上传的《ICA Embedded Agent 架构设计 R1.0》。当前运行环境无法访问 `/mnt/o/ICA-RUST`，因此本方案没有声称对仓库做过二次实测；具体文件、API 和 crate 细节仍需在实施前以该仓库实际内容复核。

本轮只完成了架构与任务设计，没有修改代码、创建分支、提交或推送。

**下一步建议：** 先把 G0、G1 和 G3 拆成可以直接分配给 Hermes 多 Agent 的独立任务包，优先解决迁移基线、W4 审计、OCR 和 Runtime POC 四个当前最关键的工作。