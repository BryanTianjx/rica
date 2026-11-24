---
title: "ICA-RUST UI 2.0 Phase 4：AI 工作空间实施方案"
document_id: "IMPL-ICA-UI2-PHASE4-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "ReadyForImplementation"
created: "2026-11-24"
baseline: "1ace0d5 (Phase 3 本地提交)"
branch: "baseline/ui-phase4"
---

# Phase 4 实施方案 R1.0 — AI 工作空间

## 0. 依据与边界

- 方案 §6.6（AI 助手）、§6.7（草稿审核）、§12 Phase 4；D01/D02。
- **关键裁决**：仓库没有通用聊天/工具调用端点；真实 AI 契约只有 `/drafts/generate`、
  `/drafts/generateComparison`、`/ai/models`、`/ai/routing`、`/settings/ai-egress`。
  因此本阶段**不建聊天空壳**，而是做基于真实数据的「AI 工作空间」聚合页。

## 1. 工作包

| WP | 内容 | 验收 |
| --- | --- | --- |
| WP1 | 新页 `/ai-workspace`：模型状态、出网边界、待审核草稿、状态链、路由概览 | 全部数据来自真实 API |
| WP2 | 导航「AI 工作空间」入口 + routeMeta 面包屑 | 导航可达 |
| WP3 | 人工边界文案：Approved ≠ 已发送；AI 服务失败时局部降级 | 测试断言 |
| WP4 | 重冻 reuse 基线 r4；回归门不退化 | gate PASS |

## 2. 明确不做

- 不建自然语言聊天面板（无后端支撑）。
- 不模拟工具调用/执行状态（无 Hermes 实现）。
- 不改草稿生成/审核既有逻辑（CandidateDetailPage / DraftsPage 已实现）。
- 不在本页重复实现系统设置或草稿审核编辑（用跳转链接）。
