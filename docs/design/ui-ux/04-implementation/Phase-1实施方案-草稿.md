# Phase 1 实施方案 — Design System 与 App Shell（R1.0）

依据：方案 §12 Phase 1、§13.1 实施约束、D01 审计发现 F1/F3。
本方案先于代码改动形成；每批次独立验证，可回滚。

## 1. 目标与边界

目标：建立统一设计令牌与 App Shell 骨架，**不改变任何业务路由、功能、Rust 接口或 DB**。

首批次（本次实施）：
1. 新增 `frontend/src/shared/design-system/tokens/`：spacing / typography / radius / index。
2. 在 `src/index.css` 顶部引入 tokens；只新增变量，不改动现有规则的选择器与取值（行为不变）。
3. 新增 `frontend/src/components/index.ts` barrel，导出 `ui.tsx` 与 `ConfirmBar.tsx` 已有导出，不新建同名组件。
4. 重冻 ui-reuse 基线并挂 npm script，作为 Phase 1+ 的防重复门禁。

不在首批次：App Shell 分组导航、Page Header、404 兜底、Toast/ErrorBoundary、Partial 状态组件。这些留待第二批次，需单独验证。

## 2. 令牌映射（方案 §5.3-5.5 → CSS 变量）

| Token 组 | 变量 | 值 | 说明 |
| --- | --- | --- | --- |
| spacing | `--ds-space-1..12` | 4/8/12/16/20/24/32/40/48px | 4px 基础体系 |
| typography | `--ds-font-title/section/card/body/secondary/caption` | 26/18/16/14/13/12px | 与方案一致 |
| radius | `--ds-radius-button/input/card/dialog/dropdown` | 8/8/12/14/8px | Badge 999px 沿用现有 |
| font-family | `--ds-font-family` | Microsoft YaHei / PingFang SC / Noto Sans CJK SC | 系统字体栈 |

现有 `--radius-s/--radius/--radius-l`、`--bg`、`--text` 等颜色变量保持不动，避免破坏双主题。

## 3. 验证命令（每批次）

```bash
cd O:/ICA-RUST/frontend
npm test                 # 25 files / 225 tests 基线，不得减少
npm run build            # tsc -b && vite build 必须成功
npm run lint             # 0 errors
npm run check:ui-reuse   # 报告模式，记录指标
```

## 4. 回滚

每批次仅新增文件 + `index.css` 顶部一行 `@import` + `package.json` 一行 script；回滚等于删除新增文件并撤销这两行改动。

## 5. 风险

- CSS `@import` 顺序错误可能覆盖主题变量：tokens 只定义 `--ds-*` 命名空间，不碰现有变量。
- barrel 可能触发 ui-reuse C7（共享导出需外部调用）：barrel 是 re-export，现有调用点不变；门禁暂用报告模式，不做 strict 拦截。
- 新增 tokens 不等于页面已使用：本次只建立基础设施，页面迁移属于后续批次，不得声称 §5 视觉验收已通过。
