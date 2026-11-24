---
title: "ICA-RUST UI 2.0 Phase 2：系统设置重构实施方案"
document_id: "IMPL-ICA-UI2-PHASE2-R1.0"
project: "ICA-RUST"
version: "1.0.0"
status: "ReadyForImplementation"
created: "2026-11-24"
owner: "ICA-RUST 项目负责人"
baseline:
  git_head: "f768662"
  branch: "baseline/ui-phase2"
related_documents:
  - "D01-UI现状审计报告-R1.0"
  - "D02-页面与功能映射表-R1.0"
  - "Phase-1实施方案-R1.0"
---

# Phase 2 实施方案 R1.0 — 系统设置重构

## 0. 依据与边界

- 方案 §7（系统设置重构规范）、§12 Phase 2、D02 设置差距表。
- 硬约束（沿用 Phase 1）：不改 Rust / DB Schema / AI 端点归属；Hermes 零实现，不建空壳；Browser 不改运行架构；不自动 commit/push/merge/release。
- 路由集合不变：`/settings` 仍是唯一设置路由；子分区用 `?section=` query 表达，不新增 Route。
- 测试基线：当前 30 files / 234 tests；任何改动必须保持全绿、build 成功、lint 无新增警告、ui-reuse regression=0。

## 1. 现状差距（D02）

| 方案子页 | 现状 | Phase 2 处理 |
| --- | --- | --- |
| 通用设置 | 主题按钮在侧栏；无独立页 | 新增子页：主题、应用信息、数据位置（只读展示） |
| AI 服务连接 | AI 接入锚点 + AiModelConfigPanel；Key write-only | 分组导航 + Key 清除确认 + 配置来源展示 |
| 模型管理 | 同上（写端点在 Python sidecar） | 不改传输；导航中独立入口复用同一面板 |
| 场景路由 | 后端 9 默认 + override 真实；前端包装器无调用方 | 新增编辑 UI（GET/PATCH `/ai/routing`） |
| 隐私与安全 | 出网开关真实；无 Agent 权限配置页 | 保留现有；不建 Agent 空壳 |
| 数据与备份 | 契约完整；存储位置未渲染；无完整性校验 | 渲染存储位置/保留数；校验标注「后端未提供」 |
| 审计日志 | 列表 + 两类筛选；无详情/导出 | 保留；不扩展无后端能力 |
| Agent 运行时 | 零实现 | **不建**（明确排除） |
| 浏览器管理 | 独立 `/browser` 页 | 设置导航中提供跳转链接，不重复实现 |

## 2. 工作包

| WP | 内容 | 优先级 | 风险 |
| --- | --- | --- | --- |
| WP1 | 设置导航改为两级分组（系统配置 / 运行管理）；`?section=` 单节视图；默认全览保持测试兼容 | P0 | 低 |
| WP2 | 通用设置子页（主题切换、应用名/版本只读、数据库/备份目录只读） | P1 | 低 |
| WP3 | AI 服务连接：Key 清除二次确认（ConfirmBar）；配置来源/尾号展示 | P1 | 低 |
| WP4 | 场景路由编辑 UI：读取 `getRouting`，表单编辑 `updateRouting`，校验后保存 | P1 | 中（后端契约真实） |
| WP5 | 数据与备份：渲染 `database_url`/`backup_dir`/`backup_keep_count`；备份校验标注后端未提供 | P2 | 低 |

## 3. 关键设计

### 3.1 分组导航与 section 参数

```
SETTINGS_GROUPS = [
  { label: '系统配置', items: [general, ai-connections, models, routing, privacy, data-backup] },
  { label: '运行管理', items: [browser(link:/browser), audit] },
]
```

- `SectionKey = 'general'|'ai-connections'|'models'|'routing'|'privacy'|'data-backup'|'audit'`。
- `useSearchParams()` 读取 `section`；**无参数 = 全览**（现有 31 个 SettingsPage 测试直接渲染页面并查询所有分区标题，必须保持）。
- 有参数 = 只渲染对应 section，并给导航项 `aria-current="page"`。
- 未知 section = 全览（不报错，不空白）。

### 3.2 通用设置（WP2）

- 主题：复用 `theme.ts` 的 `initTheme/toggleTheme`，与侧栏按钮同步（同一 localStorage key）。
- 只读信息：应用名、数据库 URL、备份目录、备份保留数（`SettingsInfo` 已有字段）。
- 不提供「修改运行模式」等无后端开关。

### 3.3 Key 清除确认（WP3）

- 现有 `clearAiKey()` 直接执行；改为 `ConfirmBar` 二次确认，文案说明「清除后模型回落到安装级/env 配置，已生成草稿不受影响」。
- 现有测试 `清除密钥调用 clearAiKey 并停用 AI` 点击的是清除按钮；确认后仍需调用一次 → 测试需相应更新（只在测试文件内）。
- 配置来源：`AiModel.source` 已有 `database/settings`；在模型行显示来源徽章（AiModelConfigPanel 已渲染 source，复用不重复）。

### 3.4 场景路由编辑（WP4）

- `GET /ai/routing` 返回 `{ routing: Record<string,string> }`；9 个默认 message type 由后端给出。
- UI：表格列出 scene→tier，下拉选择 `economy/standard/premium`，「保存」调用 `PATCH /ai/routing`。
- 空值/非法值客户端拦截；成功后 reload 显示实际生效值。
- 相关测试：`SettingsPage.test.tsx` 新增 describe。

## 4. 验证命令

```bash
cd O:/ICA-RUST/frontend
npm test                          # >=234 passed
npm run build                     # 成功
npm run lint                      # 0 errors，无新增 warnings
npm run check:ui-reuse:regression  # exit 0，regressions=0
```

## 5. 明确不做

- 不新增 Provider CRUD / 测试连接端点（后端不存在，禁止空壳按钮）。
- 不实现备份完整性校验（后端无 `integrity_check`，只如实标注）。
- 不建 Hermes/Agent Runtime 页面。
- 不在设置页内重复实现浏览器管理（保留 `/browser` 跳转）。
- 不改模型写端点归属（仍 Python sidecar + 桌面宿主转发）。
