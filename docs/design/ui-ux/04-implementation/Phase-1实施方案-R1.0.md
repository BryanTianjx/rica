---
title: "ICA-RUST UI 2.0 Phase 1：Design System 与 App Shell 重构实施方案"
document_id: "IMPL-ICA-UI2-PHASE1-R1.0"
document_type: "ImplementationPlan"
project: "ICA-RUST"
version: "1.0.0"
status: "ApprovedForImplementation"
created: "2026-11-23"
updated: "2026-11-23"
owner: "ICA-RUST 项目负责人"
baseline:
  git_head: "f94e0db"
  branch: "baseline/ui-2.0-phase1"
  frontend_tests: 225
related_documents:
  - "D01-UI现状审计报告-R1.0"
  - "D02-页面与功能映射表-R1.0"
---

# Phase 1 实施方案 R1.0

## 0. 定位

本阶段**只做基础设施**：Design Tokens、基础组件整理、复用门禁、App Shell 与分组导航、页面外壳反馈、Partial 状态机制。
不改变任何业务能力、后端接口、数据库或路由集合。

- 基线：`f94e0db`；独立分支 `baseline/ui-2.0-phase1`（已建立，未提交）。
- 实施约束见 §1；工作包见 §2；文件级清单见 §6。
- D01 / D02 为只读审计基线，后续发现的问题进入新审计版本或整改记录，不回写覆盖。

## 1. 工程约束（硬门槛）

| 约束 | 要求 |
| --- | --- |
| Git 基线 | `f94e0db`；独立 UI 重构分支 |
| Git 操作 | 不自动 commit / push / merge / release（由负责人逐批决定） |
| 原有路由 | **10 条 `<Route>` 声明保持不变**：9 条业务页面 + 1 条 `/index.html` 重定向；登录是认证态渲染，不占 Route；无通配/404 兜底（WP08 仅新增兜底，不删原路由） |
| Rust 后端 | 不修改（含 IPC allowlist、宿主命令注册） |
| 数据库 | 不修改 Schema |
| AI API | 不改变端点归属（模型写操作仍在 Python sidecar，经桌面宿主转发） |
| Hermes | 不新增空壳实现（当前零代码） |
| Browser | 不改变现有运行架构；保留 `/browser` 入口 |
| 前端测试 | `npm test` ≥225 全部通过 |
| Build | `npm run build` 必须成功 |
| Lint | `npm run lint` 不新增警告/错误（当前基线 3 warnings 均在脚本，不新增） |
| 工作区产物 | `desktop/tauri-host/bundle-resources/ui/` 为安装包 staged 产物，非源码；本次重构不覆盖、不清理、不入库 |

## 2. 工作包（WP01–WP09）

| WP | 内容 | 优先级 | 依赖 |
| --- | --- | --- | --- |
| WP01 | Design Tokens（spacing / typography / radius；颜色沿用现有变量） | P0 | — |
| WP02 | 基础 UI 组件整理（保留现有导出 + barrel，不重复造组件） | P0 | WP01 |
| WP03 | UI Reuse 检查门禁（重冻基线 + npm script + CI 接线） | P0 | WP02 |
| WP04 | App Shell 重构（统一布局容器、Page Header、通知/反馈区、ErrorBoundary） | P1 | WP01–03 |
| WP05 | 分组导航（工作空间 / AI 工作空间 / 系统管理） | P1 | WP04 |
| WP06 | Page Header 与 Breadcrumb（由路由元数据驱动，不逐页硬编码） | P1 | WP04/WP05 |
| WP07 | 全局 Toast 与 ErrorBoundary（替换页面内散落 notice） | P1 | WP04 |
| WP08 | 404 路由兜底（新增 `path="*"`，不删除原 10 条） | P1 | WP05 |
| WP09 | Partial 状态规范（局部数据源失败仍可用 + 明确反馈） | P1 | WP04/WP07 |

推荐顺序：WP01 → WP02 → WP03 → WP04（含 Navigation / Page Header / Breadcrumb / Toast / ErrorBoundary）→ WP09 → WP08 → 验收。
WP05/WP06/WP07 作为 WP04 的子工作包并行设计、串行合入，避免同一文件多批次冲突。

## 3. 重点决策一：ui.tsx 组件的保留、拆分与扩展

现状（D01 F3）：真实可复用组件只有 `Badge / StatusBadge / Loading / ErrorBox / EmptyState / Stat / LocalTime`，另有 `ConfirmBar`；无 `shared/design-system/`；无 barrel。

决策：
1. **保留现有导出名与语义**：`Loading` 不改名 `LoadingState`、`ErrorBox` 不改名 `ErrorState`；方案文档中的名称视为概念别名。
2. **不拆分到多文件**：单一 `ui.tsx` 保持稳定，避免大规模 import 路径变更；新增 `components/index.ts` 作为 barrel 统一出口（已实施，见 §7）。
3. **扩展门槛**：只有当一个组件被 ≥2 个 Feature 使用、且无业务耦合时，才提升为共享组件；否则留在 Feature 内。
4. **命名冲突防线**：新增任何基础组件前先比对 `ui.tsx` 现有导出；`StatusBadge` 保持 `status: string`（当前 C3 已知缺口），待状态联合类型迁移完成后再改，不在 Phase 1 引入 breaking change。
5. **禁止**：不为凑齐方案 §5.6 的 17 个组件而创建无调用点的空组件（会触发 reuse 检查 C7 死导出）。

## 4. 重点决策二：Design Tokens 目录、命名与 CSS 变量

已实施（WP01，未提交）：

```
frontend/src/shared/design-system/tokens/
├── spacing.css      # --ds-space-1..12 : 4/8/12/16/20/24/32/40/48px
├── typography.css   # --ds-font-family + --ds-font-title/section/card/body/secondary/caption
├── radius.css       # --ds-radius-button/input/card/dialog/dropdown/badge
└── index.css        # 汇出上述三个
```

`src/index.css` 顶部新增 `@import "./shared/design-system/tokens/index.css";`，**只新增变量，不改动任何现有选择器与取值**。

设计规则：
- 命名空间 `--ds-*`，与旧 `--bg/--text/--radius*` 隔离，避免主题切换被覆盖。
- 颜色不新建 token：直接复用 `:root` 与 `[data-theme='light']` 现有变量（深/浅双主题已可用）。
- 迁移策略：新组件/新样式优先用 `--ds-*`；老页面在各自重构批次迁移，不全局替换。
- 方案尺寸对齐：一级导航目标 220–240px、AI 面板 320–400px、内容 max-width 1320px；这些是**迁移目标**，Phase 1 不改 `index.css:330` 等现有硬编码值。

## 5. 重点决策三：App.tsx 导航与路由重构

现状：`NAV` 扁平 7 项（`App.tsx:17-26`）+ 底部设置链接（`:119-125`）；`<Routes>` 直接置于 `<main>`（`:143-155`）。

目标结构（不增加业务路由）：

```
<AppShell>                         // WP04 新增，承载全局能力
  <Sidebar groups={NAV_GROUPS} />  // WP05：三组 + 折叠 + aria-label
  <div className="shell-content">
    <PageHeader title breadcrumb actions />  // WP06
    <main><Routes>…原 10 条 + path="*" 404…</Routes></main>
    <ToastViewport />              // WP07
  </div>
  <ErrorBoundary fallback={<ErrorState/>}>…</ErrorBoundary>  // WP07
</AppShell>
```

- `NAV_GROUPS` 由原 `NAV` 重排为三组：工作空间（`/`、`/candidates`、`/positions`、`/conversations`、`/follow-ups`）、AI 工作空间（`/drafts`、`/browser`；**AI 助手无真实后端，不加入**）、系统管理（`/settings`，含 `#sec-ai`/`#sec-backup`/`#sec-audit` 锚点）。
- 不新增 `/assistant`、不新增「安全与审计」独立路由（保持 10 条 Route 口径）。
- 页面标题从各页 `page-head` 逐步上移到 `PageHeader`；迁移前保留页面原样，避免功能回归。
- WP08 新增 `<Route path="*" element={<NotFoundPage/>}/>`；这是**第 11 条声明**，但属于兜底而非业务路由，D01/D02 口径需注明「10 条原业务/兼容路由 + 1 条兜底」。

## 6. 重点决策四：新增组件接口

| 组件 | 文件（拟） | 接口 | 说明 |
| --- | --- | --- | --- |
| PageHeader | `app/layout/PageHeader.tsx` | `{ title: string; description?: string; breadcrumb?: Crumb[]; actions?: ReactNode }` | 面包屑由路由元数据 `ROUTE_META` 生成，不在页面手写 |
| Breadcrumb | `app/navigation/Breadcrumb.tsx` | `{ items: Crumb[] }`（`Crumb = { label; to? }`） | 末项不可点；支持键盘 |
| ToastViewport | `shared/components/ToastViewport.tsx` | `{ toasts: Toast[]; onDismiss(id) }` + `useToast()` hook | 统一成功/失败/信息反馈；不替代表单 inline error |
| ErrorBoundary | `app/layout/AppErrorBoundary.tsx` | class component，`fallbackRender` | 捕获渲染错误，提供「重试/返回工作台」；不吞掉错误日志 |

所有新组件必须有测试 import（reuse 检查 C6）且至少一个外部调用点（C7），否则不合入。

## 7. 重点决策五：Partial 状态统一机制

问题（D01 F2）：页面主资源成功但次级数据源失败时，当前无统一反馈（如候选人页岗位下拉失败、跟进页策略源失败）。

机制：
1. `useAsync` 扩展为**命名数据源**模型：`sources: Record<string, AsyncState<T>>`，每个源独立 L/E/Er。
2. 新增 `PartialNotice` 组件（基于 `ErrorBox`），在页面顶部/相关区块渲染「部分数据不可用」及具体源名与重试入口。
3. 禁止整页 early-return 因单个次级源失败（如 `ConversationsPage.tsx:88` 需在 WP09 修复）。
4. 每个改造页面补一条测试：主资源成功 + 次级源失败 → 主内容可见 + `role="alert"` 局部提示。

## 8. 重点决策六：UI Reuse 门禁重建与 CI

现状：`check-ui-reuse.mjs` v1.2.0 未挂 scripts/CI；旧基线 `ui-reuse-baseline-960b6ca.json` 已失效（commit 不存在，8/8 FAIL）。

已实施（WP03，未提交）：
- 重冻当前基线 `frontend/ui-reuse-baseline-r1-20261123.json`（scope 43 源/25 测试，C2/C4 通过，C1/C3/C5/C6/C7/C8 仍 fail 但为已知现状）。
- `package.json` 新增：
  - `check:ui-reuse`（报告模式，exit 0）
  - `check:ui-reuse:regression`（`--baseline … --fail-on-regression`，exit 0，0 regressions）

CI 接线（待负责人批准）：在 `.github/workflows/quality-gate.yml` 前端阶段加入 `npm run check:ui-reuse:regression` 作为**回归门**（只拦新增退化，不要求全绿），避免一次性强制 8/8 全绿阻塞交付。

## 9. 重点决策七：文件级修改清单与验收（按 WP）

| WP | 预期文件级改动 | 测试 | 验收标准 |
| --- | --- | --- | --- |
| WP01 | 新增 `src/shared/design-system/tokens/{spacing,typography,radius,index}.css`；`src/index.css` +1 `@import` | 无新增测试；build 验证 CSS 解析 | 现有 225 tests 全绿；build 成功；变量只在 `--ds-*` 命名空间 |
| WP02 | 新增 `src/components/index.ts` | 无新增测试（barrel 为 re-export） | import 路径兼容；`npm test` 225+ |
| WP03 | `frontend/package.json` +2 scripts；新增 `ui-reuse-baseline-r1-20261123.json` | `npm run check:ui-reuse:regression` | exit 0，regressions=0 |
| WP04 | 新增 `src/app/AppShell.tsx`、`src/app/layout/AppErrorBoundary.tsx`；改 `App.tsx` 外壳结构（路由内容不动） | 新增 `AppShell.test.tsx`、`AppErrorBoundary.test.tsx` | 原 10 路由可达；Shell 能力不重复实现 |
| WP05 | 新增 `src/app/navigation/navGroups.ts`；改 `App.tsx` 的 `NAV` → 分组 | 改 `App.test.tsx` | 分组名称/顺序正确；无新增业务入口 |
| WP06 | 新增 `PageHeader.tsx`、`Breadcrumb.tsx`、`src/app/navigation/routeMeta.ts` | 新增组件测试 | 标题/面包屑与路由一致；页面无硬编码重复 |
| WP07 | 新增 `ToastViewport.tsx` + `useToast.ts`；改 `App.tsx` 挂载点 | 新增 toast 测试 | 错误/成功反馈统一；无页面 notice 回归 |
| WP08 | 新增 `NotFoundPage.tsx`；`App.tsx` +1 条 `path="*"` | 改 `App.test.tsx` | 未知路径显示 404；原 10 条不变 |
| WP09 | 改 `useAsync.ts`；新增 `PartialNotice.tsx`；改 `CandidatesPage`/`FollowUpsPage`/`ConversationsPage` 局部 | 每页 +1 次级源失败测试 | 主内容仍可见；局部 `role="alert"` 明确 |

## 10. 验收流程（Phase 1 Exit）

```bash
cd O:/ICA-RUST/frontend
npm test                          # ≥225 passed
npm run build                     # 成功
npm run lint                      # 0 errors，无新增 warnings
npm run check:ui-reuse:regression  # exit 0，regressions=0
# Windows Desktop Regression（负责人执行）：
#   构建 Tauri 安装包 → 静默安装 → 首启 → UI 导航/设置/备份入口回归 → 卸载
```

未通过任一门槛不得宣布 Phase 1 完成。Git 提交/合并由负责人另行决定，本方案不自动执行。

## 11. 已实施但未提交的工作（截至本版）

WP01 / WP02 / WP03 的**基础设施部分**已在分支 `baseline/ui-2.0-phase1` 工作区完成（未 commit）：tokens 四文件、`index.css` 顶部 import、`components/index.ts` barrel、`package.json` 两条 script、重冻基线 JSON。
WP04–WP09 尚未开始。下一步按 §2 顺序进入 WP04，并在每批次后跑 §10 全部门槛。
