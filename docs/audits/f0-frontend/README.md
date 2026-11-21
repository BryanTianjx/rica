---
title: "F0-AUDIT 前端接口耦合审计(索引)"
document_id: "AUDIT-ICA-F0-001"
document_type: "AuditReport"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
related_documents:
  - "AUDIT-ICA-ENG-001"
  - "ENG-ICA-SPECS-R1.0"
  - "ADR-0003"
  - "ADR-0007-frontend-ownership"
supersedes: null
---

# F0-AUDIT — 前端接口耦合审计(开工令已由用户下达,2026-11-21)

依据《ICA-RUST UI 重构方案》四阶段过渡(F0 冻结 → F1 IPC → F2 按模块切换 → F3 收敛)
与 ADR-0003 的开工裁决:「先对 `ica-desktop`、`ica-api`、`ica-application` 和现有 React
前端做一次接口耦合审计,确认 F0/F1 的实际改造范围,再形成正式任务卡」。

**边界:** 只读审计 — 前端代码(`O:\ica\frontend`,姊妹仓)与 Rust 后端(`O:\ICA-RUST`,
HEAD cc0e64b)的接口耦合,不修改任何一侧代码。

## 交付物

| 文件 | 内容 |
|---|---|
| [coupling-report.md](coupling-report.md) | 主报告:90 个前端调用点 × Rust 68 条契约的覆盖矩阵、线格式兼容性结论、缺口分类(AI-EXT/browser)、集成路径、12 项集成风险 |
| [task-cards.md](task-cards.md) | F0-1..F0-5(冻结与抽象)与 F1-1..F1-4(垂直切片)的正式任务卡草案 |
| [../../adr/ADR-0007-frontend-ownership.md](../../adr/ADR-0007-frontend-ownership.md) | 前端归属裁决 ADR(Proposed — 待项目负责人签署,阻塞 F0 实施) |

## 一句话结论

前端 90 个调用点中 **57 个落在 Rust 已实现的 68 条契约上且线格式兼容**(契约本就逐字段
对齐 Python 基线;auth 单飞刷新、错误信封、分页消费、时序解析全部验证通过),
**4 个 AI-EXT 模型 CRUD** 与 **27 个 browser/v1** 是 Rust 侧按裁决排除的两块缺口;
归属裁决签署后,F0 改造范围为「调用收敛到 Backend Client 抽象层」,F1 垂直切片建议
从 Positions 开始。
