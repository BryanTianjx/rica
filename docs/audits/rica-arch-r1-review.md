# RICA 架构演进设计与 ADR 决策 R1.0 — 评审记录

**评审对象：** `docs/audits/RICA 总体架构演进设计与 ADR 决策 R1.0 2026-11-22.md`（RICA-ARCH-001）
**评审日期：** 2026-11-22（CST）
**评审基线：** main @ `8304ef05e619376bbe701ca5c6ba13a5254117f1`
**评审人：** 用户本人（BryanTianjx）就三个待确认问题作出裁决；编排方整理记录
**记录性质：** 架构评审裁决留痕。**本记录不修改 RICA-ARCH-001 原文档的状态字段**（该文档仍为 Proposed，ADR 的 Accepted 仍标"目标决策"）。

## 1. 评审范围与结论

对 19 章、8 条 ADR、7 个 Phase 的完整评审，结论：**接受 RICA-ARCH-001 作为 RICA 下一阶段架构演进的目标基线**，并按本文第 3 节的裁决澄清三个待确认问题。

架构主张（模块化单体 + Rust 唯一业务核心 + HTTP/IPC 双入口 + 可插拔 Sidecar）与当前 main 实现及已落地的 F1-3 事件通道、F1-4 IPC 白名单、W5-W6 契约方向一致，评审无异议。

## 2. 与项目现状的交叉核验（评审时实测）

| 核验项 | 结果 | 锚点 |
|---|---|---|
| 文档 §2.2「已知实现事实」 | 与当前 main 吻合（Tauri 管 Server+Sidecar、IPC 命令覆盖 Positions、前端有 Backend Client 抽象） | S0 报告 + routes.rs / AuthContext.tsx 只读核实 |
| §9.1 / 审计报告 §9.1 的「先冻结基线」要求 | **已满足** | Stage0 Baseline Report 已落盘并合并 main（`docs/audits/stage0-baseline-report.md`，main @ 8304ef0） |
| Phase 1（安全整改） | **已启动并部分完成** | AUD-S0 COMPLETED；AUD-P1-01/02/03/04 IN_PROGRESS |
| 文档作者声明「实施前须以当前 HEAD 重新核对」 | 已由 S0 完成（8 项 Finding 现场核实成立，门禁 404/clippy0/68-68/286-286） | S0 报告 |

## 3. 三个待确认问题的裁决（用户本人，2026-11-22）

### 问题 1：是否采用 Desktop Embedded Mode 作为默认运行模式？
**裁决：采用。** 桌面端直接装配 Rust Application Runtime，HTTP Server 按需启动，减少不必要的本地服务进程。
- 影响：Phase 3（Tauri Runtime 职责收敛）按 Embedded Mode 推进；§6.4 的"HTTP Server 按需启用"落地为默认不启动、场景触发才启动。

### 问题 2：是否长期保留 HTTP + IPC 双通道？
**裁决：保留双通道，严格统一到 Application Core。** 与现有 F1-3/F1-4 方向一致；不让 HTTP 和 IPC 各自形成一套业务逻辑。
- 影响：ADR-001/002 确认为目标基线；§5.2「双通道边界」与 §5.5「传输协议一致性」为强约束（相同输入→相同结果、相同权限→相同授权、相同错误码、相同事务与审计）。
- 配套：需要 §5.5 建议的 HTTP/IPC Contract Test（已列入架构 Phase 1 之后的 P1 任务，见文档 §15「HTTP/IPC Contract Test」）。

### 问题 3：Python Sidecar 是否限定为外部能力执行器？
**裁决：限定。** 招聘业务规则、数据事务、状态机和授权统一留在 Rust；Python 主要负责 Browser 等生态适配能力。
- 影响：ADR-005 确认为目标基线；Phase 4（Sidecar 协议化）按此边界设计，Sidecar 不承担第二套业务实现。

## 4. 8 条 ADR 的状态澄清

RICA-ARCH-001 中 8 条 ADR 均标 `Accepted（目标决策）`，而文档整体为 `Proposed`。评审裁决澄清如下：

| ADR | 标题 | 评审结论 |
|---|---|---|
| ADR-001 | HTTP + IPC 双通道策略 | 确认为目标基线（问题 2 裁决） |
| ADR-002 | Rust Core 作为唯一业务核心 | 确认（与问题 3「业务留 Rust」一致） |
| ADR-003 | 模块化单体，不采用微服务 | 确认 |
| ADR-004 | Tauri Host 作为 Desktop Runtime | 确认（与问题 1 Embedded Mode 一致） |
| ADR-005 | Python Sidecar 可插拔化 | 确认（问题 3 裁决） |
| ADR-006 | Local Mode 与 Multi-user Mode 安全边界 | 确认为目标方向；具体授权矩阵由审计 SEC-04 卡产出 |
| ADR-007 | SQLite 作为 Local Mode 默认数据库 | 确认 |
| ADR-008 | 统一 Contracts 与错误模型 | 确认 |

说明：上述"确认"是**架构目标基线的确认**，不构成对具体实现的验收。每条 ADR 的最终 Verified 仍需对应实施卡（Phase 任务 + 审计整改卡）的独立验收证据。

## 5. 与审计整改卡的关系（职责边界）

- RICA-ARCH-001 的 **Phase 1（安全基线整改）= 审计 SEC-01..04**，已建 AUD-P1-01..04 卡并在推进，不重复建卡。
- ARCH 的 **Phase 0（基线冻结）** 由 AUD-S0 部分覆盖（Finding 现场 + 门禁数字）；文档另要求的「Architecture Dependency Map / 全量 HTTP 路由盘点 / 全量 IPC 命令盘点 / 前端调用点盘点」**尚未完成**，作为后续架构卡的输入。
- ARCH 的 **Phase 2-6**（Runtime 统一、Tauri 收敛、Sidecar 协议化、通信迁移、门禁发布）是**比安全整改更大的架构工程**，当前无对应卡。本评审不自行建这些卡——按用户治理纪律，需另行立卡+审批。

## 6. 明确不做（治理纪律）

- 未修改 RICA-ARCH-001 原文档的状态字段（仍 Proposed，ADR 仍"目标决策"）——签字/状态变更是文档作者与用户的动作，编排方不代签。
- 未自行把任何 ADR 标为已批准或已 Verified。
- 未建 Phase 2-6 的架构卡（超出本次"评审+记录"范围）。
- 未改动任何源码、未触碰实库、未做 git commit/push（本记录文件交由用户决定提交时机）。

## 7. 后续建议（待用户决定）

1. 若接受本评审，考虑将 RICA-ARCH-001 文档状态从 Proposed 更新为 Accepted，并把 8 条 ADR 的"Accepted（目标决策）"统一为"Accepted（已评审确认的目标基线）"——此动作需用户明确授权或自行执行。
2. Phase 0 补齐项（依赖图 / 全量入口盘点）可并入下一张架构卡。
3. Phase 2-6 按"先审计后设计再分阶段实施"建卡，与在跑的 P1 安全整改解耦。
