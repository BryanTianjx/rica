---
title: "ICA-RUST UI 2.0 Phase 6：整体质量优化实施方案"
document_id: "IMPL-ICA-UI2-PHASE6-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "ReadyForImplementation"
created: "2026-11-24"
baseline: "80bb8b6 (Phase 5 本地提交)"
branch: "baseline/ui-phase6"
---

# Phase 6 实施方案 R1.0 — 整体质量优化

## 0. 依据

方案 §12 Phase 6：性能优化、键盘操作、无障碍优化、窗口适配、异常处理、自动化测试、Windows 安装包验收。

## 1. 审计发现与工作包

| 缺口 | 处理 | 验收 |
| --- | --- | --- |
| 无全局快捷键（方案 §9.4） | Ctrl+K 命令面板（仅导航，不写数据）；Ctrl+, 设置；Esc 关闭 | CommandPalette 测试 |
| 无 skip link | App Shell 加「跳到主内容」，键盘可达 | focus 样式 |
| 窄屏无适配（仅 1080px 断点） | 新增 720px 断点：单列 + 侧栏横排 | CSS 构建 |
| ErrorBoundary 已有 | 保留（Phase 1） | 测试已覆盖 |
| 自动化测试 | 247 tests（Phase 6 新增命令面板测试） | npm test 全绿 |
| Windows 安装包验收 | 重建安装包 → 静默安装 → 启动 → API 探针 → UI 特性标记 | 安装日志 + 探针 |

## 2. 明确不做

- 不改 Rust 后端 / DB / 契约。
- 不引入 UI 组件库或大规模依赖变更（性能风险）。
- 不建无真实后端的能力展示。
- 不自动 commit / push / merge（留负责人决定）。
