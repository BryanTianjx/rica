# ADR-0004 — ICA Runtime 架构边界与统一执行网关

- **状态：** **Accepted**
- **批准人：** Bryan
- **批准时间：** 2026-11-19T16:54Z（CST 2026-11-20 00:54）
- **批准依据：** 用户明确表态「三份 ADR 批」（2026-11-20 00:54 CST 会话裁决）
- **日期：** 2026-11-19
- **依据：** 《ICA-RUST 当前设计总方案：架构评审与改进建议》§3.1
- **关联：** ADR-0002（Rust 迁移）、ADR-0003（治理裁决 D-1…D-11）

---

## 1. 背景

ICA-RUST 已经形成三个能力域：业务核心（Rust + SQLite + Tauri）、
自动化执行（Browser Runtime）、智能体协作（Hermes Runtime + MCP）。

三者的**控制流、数据流、权限流**此前只画成了组件关系图，没有明确表达。
风险是：Browser 与 Hermes 在演进中逐渐变成第二套 ICA 业务后端。

已识别的具体风险：

- Browser Runtime 直接写业务 SQLite，绕过 `ica-application`。
- Hermes 通过 MCP 拿到「足够通用」的工具，等于拿到业务写权限。
- 三套独立日志（ICA / Browser / Hermes）无法拼出一次完整操作的因果链。

---

## 2. 决策

引入 **ICA Runtime Architecture**，明确四条不可跨越的边界。

```
                  ICA Desktop
                 Tauri 2 + React
                       │
                 BackendClient
                       │
              IPC Adapter / HTTP Adapter
                       │
                ICA API Gateway
                       │
                ICA Application
                 /           \
                /             \
          Domain Services   Policy Engine
                |             |
           Repository     Approval Service
                |             |
             SQLite      Execution Gateway
                              │
                    ┌─────────┴─────────┐
                    │                   │
               Hermes Runtime      Browser Runtime
                    │                   │
                 MCP Client        Playwright
                    │                   │
              ICA MCP Server       Chromium
                    │
             Controlled Tools
```

### 四条原则

1. **ICA Application 是唯一业务规则执行主体。**
   Hermes 与 Browser Runtime 都不得实现业务规则。
2. **Policy Engine 负责每次操作的权限与业务约束判断**——
   每次执行都重新判，不信任上游声明。
3. **Hermes 只能通过 ICA 提供的受控工具访问业务能力。**
   MCP 工具白名单**不是**安全边界，只是能力入口。
4. **Browser Runtime 不能绕过 ICA Application 直接修改业务数据库。**

### Execution Gateway（执行网关）

Agent 与 Browser 之间的**统一受控执行入口**。

它负责：

- 校验一次性审批凭证（未过期、未撤销、已绑定内容）。
- 重新计算操作摘要并与审批时比对（防止「审批后改内容」）。
- 幂等键去重（防重放）。
- 记录审计事件与 correlation id。
- 调度 Browser Runtime 执行并回传证据。

**首期不单独部署、不新增 crate**——先作为 `ica-application` 内部的应用服务实现。
这符合现阶段 Local-first、Single-user 的约束：不需要为 Agent 引入
PostgreSQL、Redis、消息队列或分布式基础设施。

### 不做什么

- 不把 Execution Gateway 做成微服务。
- 不让 MCP 直接触达 Repository。
- 不让 Browser Runtime 拥有 SQLite 凭据。

---

## 3. 理由

### 为什么是统一网关而不是两个独立通道

Agent 触发的浏览器操作与用户直接触发的浏览器操作，**在安全上必须同构**。
若 Hermes 走一条「更宽松」的通道，就等于开了一个绕过审批的后门。

### 为什么首期不拆 crate

执行网关的职责边界（授权、审批、审计、幂等）本质上是**应用层职责**，
放在 `ica-application` 内可以复用既有的授权与审计设施。
等它需要独立部署或独立依赖治理时再评估拆分。

### 为什么 correlation id 是必须的

一次 Agent 任务的完整因果链是：

```
Agent Task → MCP Tool Call → Approval → Browser Execution → Business Update
```

跨越 ICA / Browser / Hermes 三个进程与三套日志。没有统一标识，
排查「Agent 为什么改了这条候选人记录」就得人工拼接三份日志。

---

## 4. 后果

**正面**

- Agent 与 Browser 的安全性质同构，无法出现「更宽松的旁路」。
- 权限判断集中在 Policy Engine，不散落在各调用方。
- 审计链天然可追溯。

**负面 / 代价**

- 所有执行类操作都要过一次网关，**首期会引入额外延迟**。
- 网关成为单点——需要谨慎设计其并发与失败语义。
- 多一层抽象，调试链路变长。

**缓解**

- 网关首期只做校验与记账，不做重试与调度，避免变成新的复杂点。
- 网关内的操作必须幂等，重复调用不产生重复副作用。

---

## 5. 澄清项（批准后实测答复，2026-11-20）

### 5.1 与既有授权设施如何复用 —— **前置依赖 W6，当前无设施可复用**

实测（2026-11-20）：

```
crates/ica-application/src/ 下无 auth/access/permission 模块
grep 'fn authorize|fn require_access|AccessScope' crates/  →  0 命中
```

当前是 **local 单用户模式**，由常量标识身份：

```rust
pub const LOCAL_OWNER_ID: &str = "00000000-0000-0000-0000-000000000001";
pub const LOCAL_ACTOR: &str = "local_user";
pub const DEFAULT_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";
```

**结论**：Execution Gateway 的 Policy Engine **没有现成的授权设施可复用**。
它依赖 W6（Auth 5 条 + Shares 3 条）先建立真实的用户与权限模型。

**因此本 ADR 的实施顺序约束是**：Policy Engine 的完整实现
**必须在 W6 之后**，或至少与 W6 同步设计。在 W6 之前，
Gateway 只能做「单用户 + 无授权判断」的降级形态——
那种形态**不构成安全边界**，不得被当作 Agent 隔离的证据。

### 5.2 correlation id —— 当前不存在，需新建

实测：

```
grep -rn 'correlation_id|request_id' crates/ --include='*.rs'  →  0 命中
```

Rust 侧目前**没有任何请求级关联标识**。Python 基线的审计表也没有该列
（审计事件只有 `entity_type` / `entity_id`）。

**结论**：需要新建。建议作为**独立的一等字段**引入，不复用 `entity_id`
（`entity_id` 是业务对象标识，correlation id 是一次操作链路的标识，
语义不同）。存放位置与是否加表列，由 W6 之后的 schema 变更一并决定。

### 5.3 IPC 与 HTTP 是否共用同一网关 —— **共用，但入口分离**

`ica-desktop` 的 46 个 IPC 命令与 `ica-api` 的 45 条 HTTP 路由，
目前都直接调用 `ica-application` 的用例函数，**没有额外中间层**。

**结论**：Execution Gateway 作为 `ica-application` 内的服务，
**IPC 与 HTTP 共用同一实现**——这符合 ADR-0002 R1 把 wire 契约上移
`ica-contracts` 的方向（两侧共用 DTO，也应共用业务逻辑）。

「入口分离」指的是：IPC 侧与 HTTP 侧各自做协议适配与参数校验，
但**授权、审批校验、幂等、审计这四件事只在 Gateway 内做一次**。

---

## 6. 实施顺序约束（批准后追加）

本 ADR 的完整落地有硬依赖：

```
W6（Auth + Shares，23 条剩余端点的一部分）
   ↓  提供真实用户与权限模型
Policy Engine 完整实现
   ↓
Execution Gateway 完整实现
   ↓
B5/B6（Browser 审批与执行）+ Agent 受控工具
```

**在 W6 完成前**，Gateway 只能以单用户降级形态存在，
且**不得在任何验收结论中被引用为「权限隔离已实现」**。
