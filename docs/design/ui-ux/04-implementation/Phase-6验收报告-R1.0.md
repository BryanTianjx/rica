---
title: "Phase 6 验收报告 R1.0"
document_id: "IMPL-ICA-UI2-PHASE6-ACCEPT-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "PassedWithLimitations"
created: "2026-11-24"
branch: "baseline/ui-phase6"
---

# Phase 6 验收报告（2026-11-24）

## 1. 完成内容

| 质量项 | 结果 |
| --- | --- |
| 键盘操作 | Ctrl+K 命令面板（仅导航，不写数据）、Ctrl+, 设置、Esc 关闭；CommandPalette 测试覆盖 |
| 无障碍 | skip link「跳到主内容」+ `:focus-visible` 焦点环（已有） |
| 窗口适配 | 新增 720px 断点：单列布局 + 侧栏横排 |
| 异常处理 | AppErrorBoundary（Phase 1）+ 各页 ErrorBox/PartialNotice；本阶段保持 |
| 自动化测试 | 35 files / 247 tests passed |
| Windows 安装包验收 | 重建 NSIS → 静默安装 → 启动 → API 探针 → UI 特性标记，全部通过 |

## 2. 路由口径（最终）

- 11 条业务/兼容 Route（原 10 + `/ai-workspace`）+ 1 条 404 兜底 = 12 条声明。
- 新增 `/ai-workspace` 是 Phase 4 真实契约聚合页，不是空壳。

## 3. 验证

```text
npm test                          → 35 files / 247 tests passed
npm run build                     → 成功
npm run lint                      → 0 errors / 3 warnings（历史）
npm run check:ui-reuse:regression → 5/8 passed，regressions=0，gate PASS（基线 r4）
```

## 4. Windows 实机（真实证据）

- 安装：`INSTALL_EXIT=0`；`LATEST_UI_OK`（`index-DDE_bnUM.js`）。
- 启动：ica-desktop-host 19928 + ica-server 28368；host.log `ui loaded (http://ica.localhost/)`、
  `ica request uri=ica://localhost/assets/index-DDE_bnUM.js`、`ica-server listening on 127.0.0.1:12290`。
- API：`/health` /api/dashboard /api/positions /api/auth/me 全部 HTTP 200。
- UI 特性标记（安装包 JS）：全局命令 / 跳到主内容 / AI 工作空间 / 操作审计 / 场景路由 全部命中。

## 5. 已知边界

1. IAB 浏览器不支持 Tauri WebView 的 `ica://` 协议，未做 GUI 自动化截图；改用日志 + 静态标记 + API 探针。
2. UI Reuse C3/C5/C8 仍 failing（历史现状），回归门不退化。
3. Browser 的 `/api/browser/v1/*` 未在 Windows 实机做专门探针（仅验证了前端面板契约与构建）。
4. Phase 3/4/5/6 均为本地提交/工作区改动，未合并 main / push，留负责人决定。
