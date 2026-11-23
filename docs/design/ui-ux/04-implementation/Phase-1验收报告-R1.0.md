---
title: "Phase 1 验收报告 R1.0"
document_id: "IMPL-ICA-UI2-PHASE1-ACCEPT-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "PassedWithLimitations"
created: "2026-11-24"
owner: "ICA-RUST 项目负责人"
---

# Phase 1 验收报告（2026-11-24）

分支：`baseline/ui-2.0-phase1`（基线 `f94e0db`）。**未提交 / 未推送 / 未合并**。

## 1. 完成的工作包

| WP | 内容 | 状态 |
| --- | --- | --- |
| WP01 | Design Tokens：`src/shared/design-system/tokens/` + `index.css` 顶部 import | 完成 |
| WP02 | 共享组件 barrel `src/components/index.ts` | 完成 |
| WP03 | UI Reuse 基线重冻 + `check:ui-reuse` / `check:ui-reuse:regression` scripts | 完成 |
| WP04 | App Shell（`App.tsx` 外壳改为 `.shell-main` + ErrorBoundary + ToastViewport） | 完成 |
| WP05 | 分组导航 `src/app/navigation/navGroups.ts` | 完成 |
| WP06 | PageHeader / Breadcrumb + `routeMeta.ts` | 完成（页面 h1 保留，待 Phase 2/3 迁移） |
| WP07 | ToastProvider / ToastViewport / useToast + AppErrorBoundary | 完成 |
| WP08 | 404 兜底 `NotFoundPage` + `path="*"` | 完成 |
| WP09 | `PartialNotice` + Candidates/FollowUps 次级数据源提示 | 部分完成（Conversations 未改） |

## 2. 路由口径（验收后）

- 原 10 条 Route 声明全部保留：9 条业务页面 + 1 条 `/index.html` 重定向。
- 新增 1 条 `path="*"` 404 兜底；总声明 11 条。兜底不计入业务页面统计。
- 未新增 `/assistant`、安全审计独立页等空壳路由。

## 3. 验证结果（真实命令与输出）

```text
npm test                          → 29 files / 232 tests passed（基线 25/225 → +4 files/+7 tests）
npm run build                     → 成功（tsc -b && vite build）
npm run lint                      → 0 errors / 3 warnings（与基线相同，均在 check-ui-reuse.mjs）
npm run check:ui-reuse:regression → exit 0，regressions=0（基线 ui-reuse-baseline-r1-20261123.json）
```

新增测试：`AppErrorBoundary.test.tsx`、`PartialNotice.test.tsx`、`ToastViewport.test.tsx`、
`routeMeta.test.ts`、`App.test.tsx` 的 404 用例。

## 4. 已知边界 / 未完成

1. **WP09 只改造了两个页面**：`ConversationsPage` 的消息源失败仍整页 early-return，需后续批次修复。
2. **页面 h1 未上移**：各页面仍自绘 `.page-head`；PageHeader 只提供面包屑，避免重复 heading 破坏现有测试。Phase 2/3 再逐页迁移。
3. **Toast 无业务调用方**：机制与全局入口已就位，页面尚未 push toast；原 inline notice 保留。
4. **UI Reuse 检查仍 2/8 通过**：C1/C3/C5/C6/C7/C8 为历史现状，回归门只拦新增退化，不声称全绿。
5. **Windows 桌面回归未执行**：需负责人构建安装包后按方案 §10 实测。

## 5. 结论

Phase 1 基础设施（WP01–WP08 + WP09 部分）已完成，前端测试/构建/lint/复用回归门全部通过。
按方案 §10 的 Windows Desktop Regression 仍待负责人执行；Git 提交/合并由负责人决定，本分支未自动提交。
