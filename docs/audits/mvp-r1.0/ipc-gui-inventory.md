# Tauri IPC 与 GUI 集成盘点(ipc-gui-inventory)

**审计编号:** ICA-ENG-AUDIT-001 / WP-00
**证据等级:** E1(ICA-RUST 工作区源码)+ E2(姊妹仓 `O:\ica` 只读核对)
**核验日期:** 2026-11-21
**结论先行:** ICA-RUST 仓内**无 Tauri 运行时**;47 个 IPC 命令是"pre-Tauri"命令层;
真正宿主与前端在姊妹仓,且宿主当前 spawn **Python sidecar**。前端与 Rust 后端
**尚无任何运行时连接** — 这是 MVP 路线图上最大的结构性缺口。

---

## 1. 结构性事实(最重要)

| 项 | 事实 | 证据 |
|---|---|---|
| tauri.conf.json | ICA-RUST **不存在** | 全仓 find 零命中 |
| Tauri 依赖 | **无**(Cargo.lock 无 tauri 条目) | `ica-desktop/Cargo.toml:9-13` 头注释:"this crate stays tauri-free so it builds (and tests) on any machine" |
| `#[tauri::command]` | **0 个** | grep 全仓零命中;仅 lib.rs:7、commands.rs:14 注释说明"shell host 会包一层" |
| main.rs / 进程管理 | ica-desktop 是 library crate,无 main、无 spawn、无退出清理 | `ica-desktop/src/lib.rs`(21 行) |
| 真正宿主 | 姊妹仓 `O:\ica\desktop\tauri-host\`,Tauri 2(v0.2.1) | `O:\ica\desktop\tauri-host\Cargo.toml:8-12` |
| 宿主当前后端 | **Python sidecar**(`run_sidecar.py`,PORT=0,stdout JSON 握手,/health 轮询) | `tauri-host/src/main.rs:137-230` |
| 宿主自定义 IPC 命令 | **零**(661 行无 invoke_handler) | tauri-architecture-audit-r0.1.md §0 |
| 前端 | `O:\ica\frontend\`,React 19.3.1 + Vite 8 + react-router 7,**10 页面** | `O:\ica\frontend\package.json`、`src/App.tsx:143-155` |
| 前端调用方式 | 纯 HTTP fetch(`api.ts:89`,`BASE_URL = VITE_API_BASE_URL ?? '/api'`),**零 `@tauri-apps/api`** | `src/services/api.ts:18,89,150-156` |
| 打包 | 姊妹仓 NSIS-only(`targets:["nsis"]` + installer-hooks),`build_windows_desktop.ps1` + `update.sh` 版本单元指针模型;ICA-RUST 无打包配置 | `tauri.conf.json:15-31` |
| GUI↔Rust 集成 | **未集成**。F0(前端接口耦合审计)在治理台账上为 READY(parked) | `docs/ICA-RUST 当前设计总方案.md:123,191` |

跨仓结构问题:`docs/architecture/frontend/frontend-architecture-r1.md:248` 明记
"前端代码位置 /mnt/o/ica/frontend —— 在基线仓,不在 ICA-RUST 仓",并指出这是 F0 开工前必须裁决的结构问题。

## 2. IPC 命令清单(47 个,与 HTTP 47 操作 1:1)

全部位于 `ica-desktop/src/commands.rs`(提交态),模式统一:
`payload → ica_contracts::body::parse → ica_application 用例 → ica_contracts::dto`。
**与 HTTP 侧共享同一校验与 DTO(契约单一真源在 ica-contracts;审计 R0.1 的"选项 C"重构已落地)。**

| 域 | 命令 | 行号 |
|---|---|---|
| health | health | 54 |
| positions(5) | positions_list/get/create/patch/delete | 74,101,107,118,131 |
| candidates(6) | candidates_list/get/create/patch/delete、candidate_timeline | 151,182,194,206,219,226 |
| conversations/messages(4) | conversations_list/create、messages_list/create | 257,285,307,335 |
| send_records(2) | send_records_list/create | 365,399 |
| follow-ups(10) | follow_ups_evaluate/list/create/get/patch/cancel/complete/record_result/policy_get/policy_patch | 413,439,475,492,501,513,523,532,544,552 |
| dashboard(1) | dashboard | 565 |
| drafts/ai(10) | drafts_list/get/generate、candidate_context、drafts_patch/approve/reject/copy、draft_groups_get/select | 624,650,656,679,694,706,719,728,737,746 |
| imports/export/ocr(8) | imports_preview/get/confirm/confirm_candidate/reject、export_data、import_data、imports_ocr | 783,794,803,821,851,864,888,907 |

**未覆盖域**(前端有、Rust HTTP 与 IPC 均无):auth、settings/ai-key、ai/models、backups、audit-events、browser/v1(约 36 端点)。
前端 `endpoints.ts` 共 90 个调用点,分 13 组 — 其中 6 组只有 Python 基线后端实现。

**IPC 与 HTTP 重复实现?不重复:**
- 校验/DTO/错误全在 ica-contracts(`ica-contracts/src/lib.rs:1-13`:"option C: no dual-entry duplicate implementation")。
- `IpcError` 序列化为 `{"error":{code,message,details}}`,与 HTTP 信封**逐字段同形**
  (`commands_smoke.rs:78-96` 断言相等)。
- 差异仅两处:IPC 无状态码概念;IPC 查询参数用 typed args(`typed_offset/typed_limit`),HTTP 用 query-string 校验器。

## 3. GUI 页面覆盖(前端在姊妹仓)

| 页面 | 状态 | 文件 |
|---|---|---|
| Dashboard | 有("工作台") | pages/DashboardPage.tsx |
| Candidates | 有 + 详情页 | pages/CandidatesPage.tsx、CandidateDetailPage.tsx |
| Positions | 有 | pages/PositionsPage.tsx |
| Conversations | 有 | pages/ConversationsPage.tsx |
| Follow-ups | 有 | pages/FollowUpsPage.tsx |
| AI | **无独立 AI 页**;`/drafts` 是"AI 草稿审核"页,AI 模型配置在 Settings 内 | pages/DraftsPage.tsx、components/AiModelConfigPanel.tsx |
| Settings | 有 | pages/SettingsPage.tsx |
| Browser | 有(6 个子面板) | pages/BrowserPage.tsx |
| Login | 有 | pages/LoginPage.tsx |
| **Applications** | **无独立页面**(寄送记录/导入并入候选人详情与 FollowUps/Drafts 流) | — |

**GUI 三态:成体系。** 全局 `components/ui.tsx`(Loading:45 / ErrorBox:53 / EmptyState:69,
区分"加载完但零行"与"请求在途")+ `hooks/useAsync.ts`(useAsync:18 带 AbortController 防竞态、
useMutation:63)。10 个页面全部采用(抽查 SettingsPage 12 处、ConversationsPage 11 处、CandidateDetailPage 12 处)。

## 4. ica-desktop 测试(3 文件)

| 文件 | 内容 |
|---|---|
| commands_smoke.rs(提交) | 真实 DesktopState 派发:health 注入配置;positions 回路;未知 id IPC 错误信封=HTTP 信封;policy 默认值 |
| w4_imports_commands.rs(提交) | W4 命令面与 HTTP 相同 JSON 体/DTO;导入回路、重复 confirm 冲突、export 封缄、import 幂等、OCR 路径守卫等 10 测试 |
| browser_poc.rs(**未提交**) | 真实 Chromium B1 POC 验收 10 卡项,3 测试:全链路 happy path、worker 无 DATABASE_URL、worker 崩溃被遏制可重启;无 Chromium 时 NOT_EXECUTED 跳过 |

配套未提交产物:`src/browser.rs`(565 行监管者)、`src/bin/ica-browser-worker.rs`(444 行,CDP over WebSocket)。

---

## 关键事实速览

| 项 | 值 |
|---|---|
| Tauri | ICA-RUST **零 Tauri**;姊妹仓宿主 Tauri 2 spawn Python sidecar |
| IPC 命令 | **47**(pre-Tauri async fn;与 HTTP 47 操作 1:1) |
| 契约重复 | 无(单一真源 ica-contracts;错误信封同形) |
| 前端 | 姊妹仓 React 19,10 页面,纯 HTTP fetch |
| GUI 三态 | 全局组件+hooks,成体系,10/10 页面采用 |
| GUI↔Rust 集成 | **未连接**(F0-AUDIT parked;跨两仓结构未裁决) |
| 打包 | 姊妹仓 NSIS;ICA-RUST 无打包配置 |
