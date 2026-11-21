---
title: "F0-AUDIT 耦合审计报告"
document_id: "AUDIT-ICA-F0-001-R"
document_type: "AuditReport"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
related_documents:
  - "AUDIT-ICA-F0-001"
  - "AUDIT-ICA-ENG-001"
supersedes: null
---

# F0-AUDIT 耦合审计报告

**前端:** `O:\ica\frontend`(React 19.3 + Vite 8 + react-router 7,54 个 TS/TSX 文件,
12,330 行;无 store 库,无 OpenAPI 生成,types.ts 767 行全手写镜像 Pydantic)。
**后端:** `O:\ICA-RUST`(HEAD `cc0e64b`,68/68 契约端点)。
**证据等级:** E1(两侧当前源码逐行核对,文件:行号可复核)。

---

## 1. 调用面总量与分布

前端 `endpoints.ts` 共 **91 个成员 = 90 个 HTTP 调用包装 + 1 个 URL builder**:

| 域 | 调用数 | Rust 契约覆盖 | 缺口 |
|---|---|---|---|
| candidates | 7 | ✅ 68 条内 | — |
| positions | 5 | ✅ | — |
| conversations | 2 | ✅ | — |
| messages | 2 | ✅ | — |
| imports | 4 | ✅ | — |
| followUps | 10 | ✅ | — |
| drafts | 8 | ✅ | — |
| draftGroups | 2 | ✅ | — |
| dashboard | 1 | ✅ | — |
| auth | 5 | ✅(register/refresh 为死包装,刷新走 api.ts 裸 fetch — Rust 已实现该端点) | — |
| settings | 9 | ✅(含 backups×3、audit-events×1) | — |
| ai | 7 | **1 ✅**(listModels)**+ 4 ❌ AI-EXT**(create/update/setDefault/delete 模型)+ 2 死包装(routing 读写) | **AI-EXT×4** |
| browser | 28 | **1 ✅**(confirmExtraction → `/imports/{id}/confirm-candidate`,Rust 已实现)+ 27 ❌ `/browser/v1/*` | **browser×27** |
| (Rust 有、前端未消费) | — | ai usage/budget ×4、shares ×3、AI routing ×2 等 | UI 缺位(反向缺口) |

**结论:90 个调用中 57 个落在 Rust 68 条契约上;4 个 AI-EXT 与 27 个 browser/v1 是
Rust 侧按裁决排除的两块;12 个死包装无 UI 调用。**

## 2. 线格式兼容性(前端切换到 Rust 后端是否需要改代码)

| 假设 | 前端证据 | Rust 现状 | 结论 |
|---|---|---|---|
| 错误信封 `{error:{code,message,details}}` | api.ts:20-40;details 仅扁平读取(approval_required 分支) | 统一信封 + `details.errors` 数组(422) | **兼容** — 前端不假定 errors 数组,422 详情多出的数组字段无害 |
| 分页 `Page{items,…}` | types.ts:58-64;UI 只读 `items` | `PageResponse{items,total,offset,limit,has_more}` | **兼容** |
| auth 刷新流程 | api.ts:48-112 单飞刷新,裸 fetch `POST /auth/refresh`,localStorage 键 `ica.access_token/ica.refresh_token` | `/auth/refresh` 单次轮换已实现;`/auth/` 前缀不触发刷新逻辑在后端无依赖 | **兼容** — 轮换是单次语义,前端"重放一次"假设成立 |
| 本地模式判定 | `/auth/me` 返回 email 为空 → 侧栏显示"本地模式"、LoginPage 不渲染 | `/auth/me` 返回合成用户(email:"") | **兼容** |
| 时序解析 | ui.tsx:96 `new Date(value)`,假定 UTC ISO `T` 串 | RFC3339 `Z` 后缀(Python 为 `+00:00`;既有已归一化偏差) | **兼容** — 两者 `Date.parse` 等价 |
| 分页上限 | conversations 显式 limit 200;其余不传 | limit clamp 1..=500 | **兼容** |
| AI-EXT 模型 CRUD 409 `ai_model_exists`、`admin_required` 映射 | AiModelConfigPanel.tsx:210-227 | Rust 无这些路由 | **缺口**(AI-EXT) |

**总判定:除 AI-EXT 与 browser/v1 两块外,前端指向 Rust 后端不需要改动任何调用代码 —
这正是"契约逐字段对齐 Python"的设计结果,也是本审计最重要的可迁移性证据。**

## 3. 缺口处置路径(两块,三条路)

| 路径 | 内容 | 影响 |
|---|---|---|
| **P1(推荐)** | 桌面宿主按路径分流:`/api/browser/v1/*` 与 AI-EXT 4 条转发 Python sidecar,其余转发 Rust server。前端零改动,双后端共存窗口期 | 宿主转发规则改造(ica:// 协议 handler);Python sidecar 保留至 B 系列迁移完成 |
| P2 | 把 AI-EXT 4 条纳入 Rust 实现(改 ADR-0003 裁决) | 范围变更需新裁决;browser/v1 36 条不在契约内,不可行 |
| P3 | Rust-only 构建下隐藏 BrowserPage 与 AiModelConfigPanel 写操作(feature flag) | 功能缺失直到 B 系列/AI-EXT 落地;改动前端 |

P1 与 F2 的"逐模块切换"天然契合:Python sidecar 只服务 browser+AI-EXT,
其余域由 Rust 承担,F2 完成后 sidecar 退役。

## 4. 集成风险清单(审计发现,按影响排序 — 详见盘点证据)

1. **Approvals/Tasks 列表靠审计事件反推**(browser 面板):`GET /audit?limit=200` 截断静默丢条目 + N+1 拉取 — browser 域问题,随 B 系列处置。
2. **无轮询/推送,面板间状态互不过期**(browser 域)。
3. **FollowUpsPage 硬编码状态机镜像**(FollowUpsPage.tsx:41-87):后端状态机变更即漂移 — F0 应将该表纳入与 Rust `ica_domain::follow_up` 的对拍资产。
4. **候选人/岗位下拉只取第一页**(默认 limit 50):超过一页显示"未知候选人" — 前端既有缺陷,F0 顺手修(与后端无关)。
5. **restore 备份后仅 reload backups**,全库已回滚而 UI 陈旧 — F0/F2 期间需整页刷新提示(Rust restore 行为与基线一致)。
6. **fetch 无超时**;AbortController 未接线 — 前端既有,F0 通信抽象时统一补。
7. **Token 存 localStorage**(XSS 面)— Tauri CSP 已收紧;容器版无 CSP。记录为已知接受项。
8. **12 个死包装**:后端行为变更不被前端感知 — F0 收敛时明确保留/删除。
9. **错误呈现不一致**(裸 message vs `code: message` vs 中文映射)— F0 统一 Backend Client 时收敛。
10. **前端对 Rust 新增能力的 UI 缺位**(反向缺口):ai usage/budget(有类型无包装)、AI routing 编辑、shares 全域 — 不阻塞 F0,列 F2 后增量。

## 5. F0 改造范围(确认 — 供任务卡)

1. **Backend Client 抽象层**(`services/backend/`):把 api.ts 的 90 个调用点收敛到
   一层接口(每个端点一个方法),页面/组件不再直接 import endpoints.ts —
   这是 F1 切 IPC 的唯一改造面(90 个调用点 → 1 层)。
2. **DTO/错误码/分页统一**:types.ts 保持手写但增加与 `ica-contracts` DTO 的对拍
   注释;错误码表与分页形状已在 Rust 侧验证兼容(§2),无需改动。
3. **行为冻结验证**:UI 行为不变 — 以 Python 后端为参照跑通全部页面(现有 vitest
   套件 + 手工冒烟)。
4. **不做的**:不改业务流程、不修 §4 中的产品缺陷(除 #4 低风险项可顺手)、不接
   Tauri IPC(F1 的事)。

## 6. F1 垂直切片建议

**Positions**(最小耦合面:5 个端点、单页面、无跨页 reload 链):
Tauri Commands 注册 `positions_*`(ica-desktop 已有 5 个对应命令)+ 前端
Backend Client 的 positions 分支切 `invoke()` + 结果/错误/持久化对拍(HTTP vs IPC
同操作同结果)— 即《UI 重构方案》F1 验收判据的最小实现。

## 7. 审计结论

1. 前端与 Rust 后端的接口耦合面**已完全可映射**(90 调用点全量清单化,零未鉴调用)。
2. 线格式兼容性:**57/90 调用即插即用**,两块缺口均有明确归属(AI-EXT 裁决排除、
   browser B 系列)与处置路径(P1 双后端分流推荐)。
3. F0 改造范围确认为"一层抽象 + 行为冻结验证",无后端改动。
4. **前置阻塞:** 前端归属裁决(ADR-0007,Proposed)未签署 — 签署后 F0 任务卡
   (task-cards.md)即可开工。
