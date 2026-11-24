---
title: "ICA-RUST UI 2.0 Phase 3：招聘业务页面重构实施方案"
document_id: "IMPL-ICA-UI2-PHASE3-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "ReadyForImplementation"
created: "2026-11-24"
baseline: "c374901"
branch: "baseline/ui-phase3"
---

# Phase 3 实施方案 R1.0 — 招聘业务页面

## 0. 依据与边界

- 方案 §6（核心页面 UI 设计）、§12 Phase 3；D01/D02 现状基线。
- 硬约束沿用：不改 Rust / DB / AI 端点；页面功能与数据必须来自真实后端；不新增空壳。
- 测试基线 31 files / 239 tests；任何改动必须保持全绿、build 成功、lint 无新增警告、reuse gate 不退化。

## 1. 范围判断

审计发现五个业务页面（工作台/候选人/岗位/沟通/跟进）已具备完整功能与真实数据，
Phase 1 已统一 `PageHeader`/`Loading`/`ErrorBox`/`EmptyState`/`PartialNotice`。
因此 Phase 3 不做视觉重写，集中解决「交互统一」的真实工程债：

1. 状态词表重复：Candidate/Position/FollowUp 状态数组在多个页面重复声明（C8 52 行）。
2. 状态色调映射分散：`StatusBadge` 内嵌词表，页面另有映射，易漂移（C3）。
3. 页面级状态呈现已统一，仅保留确认。

## 2. 工作包

| WP | 内容 | 验收 |
| --- | --- | --- |
| WP1 | 新建 `src/shared/statusVocabulary.ts`：候选/岗位/跟进/草稿/校验状态有序列表、终态/下一态/结果选项、统一 `toneForStatus` | 共享模块测试覆盖 |
| WP2 | CandidatesPage / CandidateDetailPage / FollowUpsPage 删除页面内重复数组，改用共享词表 | 页面测试全绿 |
| WP3 | StatusBadge 保留静态 tone 分支（满足 C3 静态分析），语义与共享映射一致 | C3 toned_values 不退化 |
| WP4 | 重冻 reuse 基线 r3；回归门只拦新增退化 | gate PASS |

## 3. 明确不做

- 不重绘页面视觉、不改布局结构、不新增页面/路由。
- 不改 AI 分析呈现边界（CandidateDetailPage 已区分原始/推断/确认）。
- 不改业务状态机（后端为准）。
- 不做 Windows 安装包回归（Phase 1 已执行；Phase 3 改动为前端内部重构，下一轮统一实测）。
