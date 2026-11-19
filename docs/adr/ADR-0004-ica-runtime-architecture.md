# ADR-0004 — ICA Runtime 架构边界与统一执行网关

- **状态：** Proposed（待批准）
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

## 5. 待批准前必须澄清

- Execution Gateway 与现有 `ica-application` 的授权设施如何复用？
- correlation id 是否复用现有的 `request_id` / `correlation_id` 字段，
  还是引入新的跨进程标识？
- 首期是否需要 IPC 与 HTTP 各自一个网关入口，还是共用同一实现？
