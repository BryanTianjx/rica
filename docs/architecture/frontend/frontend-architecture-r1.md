# ICA Frontend Transition Architecture R1.0

**状态：** 提案（待 ADR 正式批准）
**依据：** 用户裁决 2026-11-19（前端过渡方案原文）
**适用工程：** `/mnt/o/ICA-RUST`（后端）+ `/mnt/o/ica/frontend`（前端）
**关联：** `docs/adr/ADR-0003-governance-decisions-20261119.md`

---

## 0. 决策摘要

**保留 React + TypeScript + Vite 作为长期 UI 技术栈，以 Rust Application Core 为业务核心，采用 Backend Client 双通道适配，逐步从 HTTP API 过渡到 Tauri IPC。**

不建议同步把前端迁移成 Rust，也不建议在 Rust 后端迁移期间重写 GUI。

> **迁移后端，不迁移前端；调整通信边界，不推倒 UI。**

React 不是临时过渡技术，而是 ICA 长期前端技术栈。Rust 负责业务和系统能力，React 负责交互与展示。

| 层次 | 技术选择 | 定位 |
| --- | --- | --- |
| UI | React 19 | 长期保留 |
| 开发语言 | TypeScript | 长期保留 |
| 构建工具 | Vite | 长期保留 |
| 桌面宿主 | Tauri 2 | Rust Host |
| 业务核心 | Rust Application Core | 唯一业务规则权威 |
| 桌面通信 | Tauri IPC | 默认通信通道 |
| HTTP API | Axum | 可选兼容及外部接口 |
| 数据访问 | Rust Infra + SQLx | 统一数据访问 |
| 数据库 | SQLite | 本地业务数据 |

---

## 1. 目标架构

```
ICA Frontend                    React 19 + TypeScript + Vite
                                Pages · Components · State · View Models
                                      ↓ 依赖接口，不依赖协议
Frontend Backend Client         统一业务接口
                                ↓
              ┌─────────────────┴─────────────────┐
Tauri IPC Adapter (桌面默认)              HTTP Adapter (可选)
invoke / events                            Axum REST API
              └─────────────────┬─────────────────┘
                                ↓
Rust Application Core            Use Cases · Domain Rules · Authorization · Transactions
Rust Infrastructure              SQLx · SQLite · File System · AI Adapter · Browser Runtime
```

**两个 Adapter 不能各自实现业务逻辑。它们必须调用同一套 Rust Application Service。**

这样未来即使增加 Web 管理端，也只需要增加 HTTP 客户端，不需要重新设计业务逻辑。

---

## 2. 为什么不做 Rust UI 迁移

| 维度 | React + TypeScript | Rust UI（Dioxus / Leptos） |
| --- | --- | --- |
| 桌面 UI | 成熟 | 可行 |
| Tauri 2 集成 | 成熟 | 需要额外集成 |
| 组件生态 | 丰富 | 相对有限 |
| 复杂交互 | 成熟 | 需要验证 |
| 前端开发效率 | 较高 | 团队经验影响较大 |
| Rust 代码统一 | 否 | 是 |
| 现有 UI 复用 | 直接复用 | 通常需要重写 |

ICA 的主要复杂度在招聘业务、候选人管理、沟通工作流、Agent 和 Browser 子系统，而不是前端语言。把 React 改成 Rust 不会自动解决业务架构问题，反而会同时引入 UI 迁移和后端迁移两种风险。

**结论：不开展 Rust UI 迁移，继续 React。**

---

## 3. Backend Client 抽象

页面既不直接调用 `invoke("get_candidates")`，也不直接依赖 `fetch("/api/candidates")`，而是依赖业务接口：

```typescript
export interface BackendClient {
  candidates: {
    list(query: CandidateQuery): Promise<CandidatePage>;
    get(id: string): Promise<Candidate>;
    create(input: CreateCandidateInput): Promise<Candidate>;
    update(id: string, input: UpdateCandidateInput): Promise<Candidate>;
  };
  positions: {
    list(query: PositionQuery): Promise<PositionPage>;
  };
  communications: {
    list(candidateId: string): Promise<Communication[]>;
  };
}
```

目标目录结构：

```
src/
├── app/
├── pages/
├── components/
├── features/
├── services/
│   ├── backend-client.ts
│   ├── contracts/
│   ├── errors/
│   └── events/
└── infrastructure/
    └── backend/
        ├── http/
        │   └── http-backend-client.ts
        ├── ipc/
        │   └── ipc-backend-client.ts
        └── backend-factory.ts
```

**页面只依赖 `BackendClient`。切换通信方式时，只替换 Adapter，不修改页面业务代码。**

### 通信方式选择

| 场景 | 使用方式 |
| --- | --- |
| 桌面端查询和写入 | Tauri IPC |
| 桌面端实时任务进度 | Tauri Events |
| Browser Worker 通信 | 独立进程 IPC |
| Hermes MCP | ICA MCP Server |
| 自动化测试 | 可选择 HTTP 或测试专用接口 |
| 未来 Web 管理端 | Axum HTTP API |

> **Hermes MCP 不应直接复用前端 IPC 权限。** MCP 必须独立执行身份、能力和业务范围授权。

---

## 4. 四阶段过渡

> **实施进度(2026-11-22):** ADR-0007 已签署(前端迁入本仓 frontend/,F0-1 完成);
> F1-1 的 P1 双后端分流已在 `desktop/tauri-host` 落地 —— 宿主 spawn Rust ica-server
> (主)+ Python sidecar(次),`/api/browser/v1/*` 与 AI-EXT 模型 CRUD 走 sidecar,
> 其余 68 条契约走 Rust;sidecar 缺席降级 502 而不阻断启动。本节以下的文本为
> 签署时的方案原文。

### F0：前端现状冻结与通信抽象

**目标：不改变现有页面和业务流程。**

- 盘点现有 React 页面、API 调用和状态管理。
- 定义 Backend Client 接口。
- 将页面直接调用 HTTP 的代码收敛到 Adapter。
- 统一 DTO、错误码、分页和事件模型。
- 建立前端接口与 Rust Contracts 的映射关系。

**验收：** UI 行为不变，原有 HTTP 功能全部正常。

### F1：Rust IPC Adapter 建设

**目标：建立 React 到 Rust Core 的正式通信通道。**

- 在 `ica-desktop` 注册 Tauri Commands。
- 将 Command 映射到 Rust Application Service。
- 统一 IPC DTO 和错误转换。
- 实现 Tauri Events。
- 建立 IPC 权限白名单。

**验收：** 至少一个完整业务垂直切片通过 IPC 完成读写。

### F2：按业务模块逐步切换

**目标：逐模块由 HTTP 切换到 IPC。**

建议顺序：Settings → Positions → Candidates → Communications → Follow-ups → AI → Browser → Agent Workspace。

每个模块独立验证，不要求一次性切换。

**验收：** HTTP 与 IPC 对同一业务操作的结果、错误语义和持久化行为一致。

### F3：通信架构收敛

- 桌面正式默认 IPC。
- Axum 保留为独立 HTTP 适配层。
- 清理不再使用的前端 HTTP 调用。
- 保留统一 Backend Client。
- 建立 IPC 与 HTTP 的兼容性测试。

**验收：** 页面不感知底层通信方式，Rust Application Core 是唯一业务实现。

---

## 5. 与七个 crate 的对应关系

| crate | 前端过渡相关职责 |
| --- | --- |
| `ica-domain` | 业务实体与规则，不感知 UI |
| `ica-application` | 统一 Use Cases |
| `ica-contracts` | IPC/HTTP DTO、事件与错误契约 |
| `ica-infra` | 数据库、外部服务、文件系统 |
| `ica-api` | Axum REST API |
| `ica-server` | HTTP 服务装配及生命周期 |
| `ica-desktop` | Tauri Commands、Events、Desktop Host |

**硬约束：`ica-desktop` 不应该重新实现 `ica-application` 中的业务逻辑。**

Tauri Command 只负责：接收前端参数 → 参数转换和基础校验 → 调用 Application Service → 返回结构化结果 → 转换错误类型。

IPC 与 HTTP 不应各自维护一份业务 DTO。优先共享 `ica-contracts` 中的类型，并明确 JSON 序列化兼容要求。

---

## 6. 与 W4 / W7 的范围隔离

当前 ICA-RUST 仍在处理 W4 审计失败、OCR 等价性和 Browser Candidate Extraction 迁移问题。

| 工作 | 当前处理 |
| --- | --- |
| React 页面重写 | 不做 |
| Rust UI 框架迁移 | 不做 |
| Backend Client 抽象 | 可以进入设计 |
| IPC Adapter 架构 | 可以进入设计 |
| IPC 最小 POC | B0/W4 稳定后安排 |
| 全模块 HTTP → IPC | 暂缓 |
| Browser UI 接入 | 等 Browser B0 完成 |
| Hermes Agent Workspace | 等 Agent Runtime/MCP 契约稳定 |

**接口治理约束：** 68 条原始迁移契约不能因为增加 IPC 就重新计算成另一套端点数量。

HTTP API、Tauri IPC 和 MCP 是不同的传输/调用契约，应**分别维护测试与版本记录**。

---

## 7. 决策清单

| 编号 | 决策内容 |
| --- | --- |
| FE-001 | React + TypeScript 作为长期前端技术栈 |
| FE-002 | 不迁移 Dioxus / Leptos |
| FE-003 | 前端通过 Backend Client 访问后端 |
| FE-004 | Tauri IPC 是桌面默认通信方式 |
| FE-005 | Axum HTTP 保留为可选适配器 |
| FE-006 | IPC 与 HTTP 共用 Rust Application Core |
| FE-007 | 按业务模块渐进迁移，不重写 GUI |
| FE-008 | Agent MCP 独立于前端 IPC 权限体系 |

**最终建议：ICA-RUST 采用 React + Rust Core + Tauri IPC 的长期架构。** 先做 F0 和 F1 的架构及最小垂直切片验证，不立即全面切换全部页面。

---

## 8. 实施前的实测基线（2026-11-19）

| 事实 | 值 | 取数 |
| --- | --- | --- |
| React 页面数 | 20 | `ls /mnt/o/ica/frontend/src/pages/` |
| 前端 API 调用点 | 90 | `grep -c 'api\.\(get\|post\|put\|patch\|delete\)' endpoints.ts` |
| Rust IPC 命令数 | 46 | `grep -c '^pub async fn' crates/ica-desktop/src/commands.rs` |
| 前端代码位置 | `/mnt/o/ica/frontend` | **在基线仓，不在 ICA-RUST 仓** |

### ⚠ 一个必须先解决的结构问题

前端代码在 **`/mnt/o/ica/frontend`**——那是**基线仓**（Python 后端所在）。
而 Rust 后端在 `/mnt/o/ICA-RUST`，是**独立 git 库**。

这意味着前端过渡方案的实施会**同时跨两个仓库**，而两者的约束相反：

- `/mnt/o/ica` 在本工程中**只读**（基线仓，契约来源）。
- `/mnt/o/ICA-RUST` 是开发树。

即：前端过渡要改前端代码，而前端代码在只读仓里。

**这必须在 F0 动工前解决**——前端代码该迁到哪个仓？是 ICA-RUST 新增 frontend/
子目录（与 Rust 同仓），还是前端独立成仓？这个决策不定，F0 无法开工。

裁决原文也确认了这一点：

> 如果要进入实施，我建议下一步先对 `/mnt/o/ICA-RUST` 的 `ica-desktop`、`ica-api`、
> `ica-application` 和现有 React 前端做一次**接口耦合审计**，确认 F0/F1 的实际改造范围，
> 再形成正式任务卡。

---

## 9. 建议的正式文档结构

```
docs/
├── architecture/
│   └── frontend/
│       ├── frontend-architecture-r1.md          ← 本文档
│       ├── frontend-backend-client-r1.md
│       ├── frontend-ipc-contract-r1.md
│       ├── frontend-http-compatibility-r1.md
│       └── frontend-migration-plan-r1.md
└── adr/
    ├── ADR-FRONTEND-001-react-typescript-retention.md
    ├── ADR-FRONTEND-002-backend-client-abstraction.md
    ├── ADR-FRONTEND-003-tauri-ipc-default.md
    └── ADR-FRONTEND-004-axum-http-compatibility.md
```
