---
title: "ICA-RUST UI 2.0 Phase 5：Agent 与浏览器工作区实施方案"
document_id: "IMPL-ICA-UI2-PHASE5-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "ReadyForImplementation"
created: "2026-11-24"
baseline: "74778b6 (Phase 4 本地提交)"
branch: "baseline/ui-phase5"
---

# Phase 5 实施方案 R1.0 — Agent 与浏览器工作区

## 0. 核心裁决

方案 §12 Phase 5 明确要求「以实际后端能力为前提，不得仅实现无实际功能的展示页面」。
审计结果：

| 子系统 | 真实实现 | 决策 |
| --- | --- | --- |
| Hermes Agent Runtime | **零代码**（仅 Draft 设计文档；H-POC-03..10 因上游能力不符停工） | **不建任何 Agent 状态/任务/权限页面** |
| Browser Workspace | 真实实现：Python sidecar 39 端点 + 6 面板 + Playwright bridge + DB 表 | 增强真实能力入口 |

## 1. 工作包

| WP | 内容 | 验收 |
| --- | --- | --- |
| WP1 | 新增 `AuditPanel`：直接消费 `/browser/v1/audit`，支持任务/会话筛选 | 真实事件渲染 + 失败降级 |
| WP2 | BrowserPage 增加「操作审计」独立分区（原审计只在审批/任务面板间接可见） | 7 个面板 |
| WP3 | 测试覆盖审计面板（真实契约 + 失败路径） | 测试全绿 |

## 2. 明确不做

- 不建 Agent Runtime 页面 / Hermes 集成状态 / Agent 任务展示 / 工具权限展示（零实现，禁止空壳）。
- 不改 Browser 运行架构（Python sidecar 39 路由 + 桌面宿主转发）。
- 不新增浏览器能力（会话/权限/审批/任务/提取已由 6 面板覆盖）。
