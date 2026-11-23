# D01 — UI 现状审计报告（R1.0 / 2026-11-23）

- 审计对象：`O:/ICA-RUST` 当前 `main`（HEAD `f94e0db`），`frontend/src/` + Rust 边界 + 契约文档。
- 对照基线：`docs/ICA R2.0 UI 重构方案：统一招聘工作台 + Hermes Agent Runtime + Browser Workspace.md`（下称「方案」）。
- 方法：四路只读源码审计（路由/页面、组件/样式、设置/AI 契约、后端边界/测试）+ 本机真实回归命令。
- 定位：**Phase 0 交付物**。审计结论是「可演进的 MVP Shell」，不是方案所描述的 AI-native Workspace。

## 1. 结论先行

| 维度 | 现状 | 方案要求 | 结论 |
| --- | --- | --- | --- |
| 导航/路由 | 扁平 7 项侧栏 + `/settings` 底栏，10 条路由 | 四功能区域、11 个一级页面、二级导航 | 部分满足 |
| App Shell | 侧栏 + `<main>` + 各页自绘标题 | 独立 Page Header、面包屑、通知、Status/Feedback、AI 面板入口 | 明显缺口 |
| 设计系统 | 颜色 + 少量圆角 token 在单一 `index.css` | 独立 tokens（色/字/距/圆角）+ 17 个基础组件 | 明显缺口 |
| 设置中心 | 1 页 5 锚点 | 10 个子页、两级分组、连接测试/状态/来源 | 部分满足 |
| 资源隔离 | 功能可用、Key write-only、OS 凭据库 | 同左 + 清除确认、无明文回显 | 基本满足，有缺口 |
| AI/模型 | 4 条写端点仅在 Python sidecar；Rust 只有 `GET /ai/models` | 模型注册/启用/默认/能力 | 桌面形态满足，dev/纯 HTTP 不满足 |
| 场景路由 | 后端 9 条默认 + DB override + 预算降级真实存在 | UI 展示解析模型/继承/覆盖编辑 | UI 缺 |
| 备份恢复 | 契约完整、两级确认、online-backup + 安全副本真实 | 完整性校验与存储状态呈现 | 缺校验 |
| Hermes | **零代码**，仅设计文档 | 独立 Runtime 集成入口 | 未实现（不得伪装） |
| Browser | 独立 `/browser` + 6 面板 + Python 39 路由 + Playwright | 后续扩展入口 | 真实实现，未在 Windows 实机跑通 API |

## 2. 回归基线（本机实测，2026-11-23）

执行环境：Windows 11 x64；`node_modules` 原为 Linux 原生 binding，已 `npm install` 修复（package-lock 未变化）。

```text
npm test        → 25 files / 225 tests passed（退出码 0）
npm run build   → tsc -b && vite build 成功（退出码 0，Vite configLoader 警告 1 条）
npm run lint    → 0 errors / 3 warnings（均在 scripts/check-ui-reuse.mjs）
```

结论：当前前端存在可用回归基线。任何 Phase 1+ 改动必须保持 `npm test` 225+ 全绿与 `npm run build` 成功。

## 3. 关键审计发现

### 3.1 App Shell 与导航（F1）

- `frontend/src/App.tsx:17-26` 的 `NAV` 是扁平数组（7 项），设置以独立 `NavLink` 置底（`:119-125`）；无分组、无折叠、无 `aria-label`、无键盘快捷键。
- 页面标题由各页自绘 `page-head`（如 `DashboardPage.tsx:20-24`）；无 Shell 级 Page Header、无面包屑；`index.html:7` 标题静态。
- 无全局 Toast/Bell/Status Area；错误仅在页面内 `.notice aria-live`；认证 reason/message 未由 Shell 渲染；无 ErrorBoundary。
- AI 面板入口缺失（方案 §4.1:240、§6.6）；AI 生成能力内嵌于 `CandidateDetailPage.tsx:601+`。
- `<Routes>` 直接置于 `<main>`（`App.tsx:142-156`），无 `Route path="*"` 兜底。

### 3.2 页面与状态（F2）

- **路由统计口径（统一定义）**：`App.tsx:143-155` 共 **10 条 `<Route>` 声明** = **9 条业务页面路由**（`/`、`/candidates`、`/candidates/:id`、`/positions`、`/conversations`、`/follow-ups`、`/drafts`、`/browser`、`/settings`）+ **1 条兼容重定向**（`/index.html` → `/`）。**0 条通配/404 兜底**。页面测试 11 文件 / 132 个 `it()`；完整清单见 D02。
- 方案 §3.1 中「招聘项目」「AI 助手」「安全与审计」三页不存在。招聘项目按方案 §158-162 允许不建；AI 助手与安全审计属明确缺口。
- 页面状态覆盖：大部分具备 Loading/Empty/Error；**Partial（部分数据源失败仍可用）**普遍缺失，典型：
  - `CandidatesPage.tsx:51,195,267` 岗位下拉失败无提示；
  - `FollowUpsPage.tsx:111,200` 策略源无 L/Er；
  - `CandidateDetailPage.tsx` 多个二级数据源错误未渲染（decision/timeline/context/models/egress）。
- `ConversationsPage.tsx:88` 消息源失败整页 early-return，与「局部失败不拖垮整页」目标冲突。

### 3.3 组件与设计系统（F3）

- 公共 UI 集中在 `frontend/src/components/ui.tsx`（7 个导出）+ `ConfirmBar.tsx`；无 `shared/design-system/`，无 barrel/index。
- 方案 §5.6 的 17 个基础组件中，真正有 React 组件的只有 Badge / EmptyState / Loading(=LoadingState) / ErrorBox(=ErrorState)；Button/Input/Select/Checkbox/Card/Table 仅为 CSS 样式；Switch/Radio/Dialog/Drawer/Dropdown/Tooltip/Tabs 完全缺失。
- 原始 HTML 控件大量散落：`<button>` 86、`<input>` 45、`<select>` 17、`<textarea>` 10、`<table>` 26（非测试文件）。
- 颜色 token 完整（深/浅双主题，`index.css:8-114`），但**间距、字号、布局尺寸无 token**：侧栏 `208px`（`:330`，目标 220–240px）、内容 `max-width:1200px`（`:451`，目标 1320px）等硬编码。
- `check-ui-reuse.mjs`（v1.2.0）未挂到 `package.json` scripts / CI；基线 `ui-reuse-baseline-960b6ca.json` 冻结于已不存在的提交，且 8/8 FAIL，**不可作为当前门禁**；重跑 `--json --out` 重新冻结后才能使用。
- 生产源码 `any` 为 0；`StatusBadge` 仍为 `status: string`（`ui.tsx:27`），与复用检查 C3 不一致；无 ErrorBoundary。

### 3.4 设置 / AI / 数据契约（F4）

- 设置页 `SettingsPage.tsx:186-193` 仅 5 个锚点（AI 接入 / 隐私出网 / 跟进策略 / 备份 / 审计），方案 §7.1 的 10 个子页多数不存在。
- **安全项正确**：API Key write-only（输入后清空），不进 localStorage；后端 OS 凭据库持久化、`key_masked` 仅回尾号；清除走 `DELETE /settings/ai-key`。但清除无二次确认（方案 §7.2 要求确认）；Rust `ai_key.rs:7-9` 注释声称剥离旧 `.env` 明文，实现未做（Python 做了）。
- 模型 4 条写端点真实存在于 Python sidecar（`ai.py:319,424,486,499`）并经桌面宿主按 method/path 转发；**Rust HTTP 只注册 `GET /ai/models`**。因此 Vite dev（直连 8000）下模型写操作 404。
- 方案 §7.3 的 `capabilities` / `provider_id` / `model_identifier` 在真实 wire shape 中不存在；`tier` 真实字段名为 `quality_tier`，`configuration_source` 为 `source`。
- 场景路由后端真实（`ai_registry.rs:32-42` 默认 9 条 + DB override + 预算降级），前端 `getRouting/updateRouting` 包装器**无调用方**，UI 只读展示 `models.routing`；无覆盖规则编辑、继承/解析模型呈现。
- 备份恢复真实且有 confirm 409/200 两级（`backup.rs:34-65`）+ pre-restore 安全副本；但**无完整性校验**（全仓无备份 `PRAGMA integrity_check`），存储位置/DB 状态字段在 `types.ts:289-298` 却未渲染。
- Hermes 零实现（`docs/agent/g3-hermes-capability-survey-r1.md` 记载 H-POC-03..10 因上游能力不符停工）；Browser 为真实实现（6 面板 + Python 39 路由 + Playwright bridge），但 Windows 实机验收未发 `/api/browser/v1/*` 探针，不能声称实机通过。

### 3.5 边界与测试（F5）

- Rust HTTP 68/68 契约实现；前端 `endpoints.ts` 90 个包装器 / 13 组：5 positions 可切 IPC，54 走 Rust HTTP，27 走 Python sidecar browser，4 为 AI-EXT。
- IPC 允许表只开 5 条 positions 命令（`ipc_allowlist.rs:17-51`），宿主 `invoke_handler` 同源；新增 IPC 必须三处同改（migration.ts / generate_handler! / ipc_allowlist.rs）。
- 回归命令：`npm test`（225）、`npm run build`、`npm run lint`、`cargo test --workspace --locked -- --skip <DEF-006 三条>`、`scripts/check.py`、`scripts/endpoint_reconciliation.py --json`。
- 工作区存在 staged UI 打包产物（`bundle-resources/ui/index.html` 修改 + `ui/assets/` 未跟踪），来自最近一次安装包构建；UI 重构前须明确归属，避免覆盖。

## 4. 方案 §15.2 待验证事项逐条裁定

| 待验证项 | 审计裁定 |
| --- | --- |
| 当前 React 路由实现 | 10 条扁平路由，无嵌套/无 404 兜底；见 D02 |
| 当前组件库及样式体系 | 4 个真实基础组件；颜色 token 完整，字/距/布局无 token |
| 系统设置数据契约 | 5 锚点单页；子页概念不存在 |
| AI Provider 配置持久化 | OS 凭据库（Win Credential Manager / macOS Keychain / Linux 0600），DB 只存审计与开关 |
| 模型注册与场景路由真实实现 | 写端点在 Python；场景路由后端真实、前端只读 |
| Rust Command 模块边界 | 70 个适配函数、5 条注册，allowlist 双向 pin |
| 当前数据库结构 | 68 契约基线表齐全；本次审计不修改 schema |
| 测试覆盖率 | 前端 225 tests；后端约 549 `#[test]`，实测基线 543 passed |
| Hermes 集成进度 | 零代码，只有 Draft 设计文档与停工记录 |
| Browser 子系统实现进度 | 前端/Python 真实实现；Rust 业务路由 0；实机 API 未验证 |

## 5. Phase 0 结论与 Phase 1 准入

**准入条件已满足**：现状、页面清单、组件清单、API 映射、测试基线均已建立，且回归基线全绿。

**Phase 1 范围建议（先低风险、可回滚）**：
1. 建立 `frontend/src/shared/design-system/tokens/`（spacing/typography/radius），复用现有颜色变量，不改组件行为。
2. 统一基础组件命名：保留 `Loading/ErrorBox/EmptyState` 现有导出，补 barrel `index.ts`，不为对齐方案而重复造同名组件。
3. App Shell 分组导航（工作空间 / AI 工作空间 / 系统管理）+ Page Header 组件，保持 10 条路由不变。
4. 重新冻结 `ui-reuse` 基线并加入 npm scripts，作为防重复门禁。

**Phase 1 明确不做**：不新增业务路由、不改 Rust/DB、不新增第三方组件库、不实现 Hermes/Agent 页面、不改模型写端点归属。

## 6. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 重构破坏现有功能 | 每批次跑 `npm test` + `npm run build`，保持 225+ 基线 |
| 组件重复造轮子 | 先比对 `ui.tsx` 现有导出；用重冻的 ui-reuse 基线做回归门 |
| 设计建议误当现状 | 本审计报告逐条标注「真实/部分/未实现」 |
| staged UI 产物被覆盖 | 打包产物目录保持未入库，不纳入重构 diff |
| 外部 Agent 状态伪装 | 不实现任何无真实后端的 Agent/Browser 展示页面 |
