# 项目决策记录 — ICA-RUST 架构与迁移治理裁决

**状态：** 已批准（用户裁决，2026-11-19）
**适用范围：** `/mnt/o/ICA-RUST`
**依据：** 三份独立审计报告（`/tmp/audit-w4-2/`）+ 仓库实测

---

## 0. 总体原则（裁决原文的要点）

> 先恢复 W4 的工程可信度，再开放 Browser Subsystem 的实施；浏览器方向可以批准
> 架构准入，但不能绕过产品边界、数据模型和迁移范围裁决。

> 这次审计暴露的不只是几个代码缺陷，而是三个治理问题：契约覆盖不完整、
> 历史基线发生漂移、不同波次之间存在职责重叠。**不能再以现有对拍通过率或
> 卡片状态代替工程验收。**

因此确立三条工程纪律：

1. **对拍通过率不是等价性证明。** 它只证明「被覆盖的那些路径通过」，
   不证明未覆盖路径无差异。放行结论必须显式列出覆盖边界。
2. **基线锚点必须记录 sha。** 同名的「220/222」在不同锚点指向不同事实。
3. **波次职责不得重叠。** 同一能力域只能有一套状态机、去重规则与持久化结构。

---

## D-1 · W4 审计 FAIL 后的处理

**结论：** W4 暂不放行。拆为四张返工卡并行，完成后**由独立审计员复审**，
未通过不放行。`w4-done` 不得创建。

| 卡 | 范围 | 优先级 |
| --- | --- | --- |
| `ICA-RUST-W4-R1` | OCR 等价性（检测器输入限制） | P1 阻断 |
| `ICA-RUST-W4-R2` | HTTP 错误契约等价性（422 信封） | P1 阻断 |
| `ICA-RUST-W4-R3` | 历史校验规则补齐（status/location/archived） | P0/P1 阻断 |
| `ICA-RUST-W4-R4` | P2 缺陷与契约覆盖清理 | P2 |

放行必须等待 R1、R2、R3 完成；R4 中**经确认影响契约**的项也须在放行前关闭。

**返工项清单（裁决版）**

| 问题 | 优先级 | 裁决 | 放行要求 |
| --- | --- | --- | --- |
| OCR F1：检测器输入限制致文本分叉 | P1 | 必须修复 | 扩大边界测试，Python/Rust 输出**及最终落库文本**一致 |
| 422 `details.errors` 类型不等价 | P1 | 必须修复 | 数组保持数组，不允许二次 JSON 编码 |
| `validation_error` 的 input/loc 差异 | P1 | 必须修复 | 字段类型、数组下标、错误结构一致 |
| FINDINGS P0-4 残项 | 阻断项 | 必须关闭 | status/location/archived 校验行为与 Python 一致 |
| 其他已确认 P1 差异 | P1 | 必须修复 | 逐条复测并形成证据 |
| OCR 初始化永久缓存失败 | P2 | 建议本轮修复 | 初始化失败后具备合理恢复能力 |
| OCR 锁中毒错误分类 | P2 | 建议本轮修复 | 错误映射符合 OCR 错误契约 |
| 日期型 / 布尔 / `raw_text` / 空串 ID | P2 | 纳入缺陷台账 | 逐项判断是否影响契约，**不允许无记录遗留** |

**裁决特别强调的一条**：

> 不能只修复 `details.errors`，还要补上之前没有进入契约测试的错误响应路径。
> 否则即使本次 422 修复通过，仍然无法证明错误响应契约完整。

**W4 重新放行条件（在 W4-A 判据基础上追加）**

1. AC2 的七端点复核中，明确增加 OCR F1 边界复核。
2. AC3 的 15 项逐条确认；原来标记关闭但仍存在的项目**重新打开**。
3. 422 错误响应**进入正式契约测试**。
4. P0-4 三项校验规则全部有 Python/Rust 对照证据。
5. OCR 结果比较**不止比较 API 响应，还要比较最终落库内容**。
6. 不允许为了通过测试而修改冻结 Python 基线。
7. 测试样本、Python 基线 commit、Rust commit **全部记录**。
8. **独立审计员重新审查**，不由修复者自行宣布通过。

> **220/222 不能继续作为 W4 等价性证明。** 它只能证明原来被覆盖的那些路径通过，
> 不能证明未覆盖路径不存在差异。

---

## D-2 · 4 条 `ai/models` 是否进入原 68 条

**结论：不纳入原 68 条**，独立建立 **AI Models Extension Contract**。

**理由：** 原 68 条的冻结基线是 `d39e321`。这 4 条属于之后新增的接口，
不能为了追求路由总数一致而回写原契约。

**三个范围**

| 范围 | 处理 |
| --- | --- |
| 原冻结基线 68 条 | 保持不变，继续作为 R0 + W1–W6 对拍基准 |
| 新增 4 条 `ai/models` | 独立建立 AI Models Extension Contract |
| 后续 Rust AI Models 能力 | 单独评估是否实施、是否需要新波次 |

### ⚠ 实测发现：裁决中的一处前提需要修正

裁决原文说「如果只是 Python 工作树中的新增实验接口，可以暂缓」。
**实测显示这条前提不成立** —— 前端已在调用这 5 个端点：

```
frontend/src/services/endpoints.ts
  208: listModels:   api.get<AiModelsResponse>('/ai/models')
  217: createModel:  api.post<AiModel>('/ai/models', body)
  221: updateModel:  api.put<AiModel>(`/ai/models/${id}`, body)
  225: setDefault:   api.put<AiModel>(`/ai/models/${id}/default`)
  229: deleteModel:  api.delete<AiModelDeleted>(`/ai/models/${id}`)
```

**即它们有真实产品依赖，不是实验接口。** 因此：

- 「不进入原 68 条」这个**结论不变**（口径正确）；
- 但「可以暂缓」这个**推论要改** —— 需要进入 Rust 产品交付路线，
  否则 Rust 前端将失去 AI 模型管理能力。
- 裁决要求的核实事项「这 4 条接口是否已被 UI、Agent Adapter 或其他调用方依赖」，
  **已核实：前端 5 个方法全部在用**。

**待裁决补充**：AI Models Extension Contract 是本波次实施还是单独立波次，
以及 Rust 前端的过渡方案（继续走 Python 后端，还是等 Rust 实现）。

---

## D-3 · 39 条 `/api/browser/*` 路由

**结论：全量盘点，W7 与 BW0 分开管理。**

**不能忽略，也不能直接全部迁移。** 分三个集合：

| 集合 | 内容 | 归属 |
| --- | --- | --- |
| A | `confirm-candidate` 及其关联的既有 Candidate Extraction 写入链路 | W7 |
| B | 现有会话、任务、连接器、标签页、权限等 API | Browser Subsystem 兼容迁移（B0 审查后决定复用/改造/替换） |
| C | 与新 Browser Runtime 重叠、无有效调用方或已失去产品价值的旧路由 | 废弃或合并 |

**新增路由盘点文档** `docs/architecture/browser/browser-route-inventory-r1.md`，
每条路由至少记录：

```
HTTP Method / Path
Python 实现位置
是否被 UI 调用
是否被 Agent 或 MCP 调用
数据表依赖
是否进入 W7
是否进入 Browser Subsystem
是否保留兼容
是否废弃
Rust 对应实现状态
```

**39 条路由必须全部有最终归属**，但不要求全部按原样迁移。

**新建独立的 Browser API Compatibility Wave（暂称 BW0）**，在 B0 完成路由盘点后
确定实际迁移范围。**不塞入 68 条**，也**不让它无归属地留在 Python 系统中**。

### ⚠ 实测发现：集合 C 目前可能为空

裁决提到「没有有效调用方或已经失去产品价值的旧路由」可以废弃。实测：

```
frontend/src/services/endpoints.ts   含 "browser" 的调用点 33 处
```

前端有 **33 处 browser 路由调用**（`/browser/v1/runtime/status`、`/sessions`、
`/navigate`、`/snapshot`、`/screenshot`、`/actions` 等）。

**结论：这些路由有真实产品依赖，集合 C 很可能是空的。** 实际归属更可能是
「大部分进集合 B」。这不改变「必须全量盘点」的裁决，但盘点时的起点应假定
「多数会被保留」，而不是「先找废品」。

---

## D-4 · B0 是否批准

**结论：批准 B0 准入工作**，并增加强制裁决条件。

B0-08（表结构对账）与 B0-09（产品边界 V2.0 记录）**是必要的，不是额外负担**。

### B0-08 的强制处理原则（裁决原文）

> 复用现有表不代表现有表一定足够。

| 领域 | 处理原则 |
| --- | --- |
| Browser Profiles | 优先检查现有 `browser_profiles` |
| Browser Sessions | 检查 `browser_sessions` 与 connector sessions 的职责 |
| Browser Tasks | 检查 `browser_tasks`、`browser_task_steps` |
| Permissions | 复用 `browser_permissions` |
| Approvals | 审查 `browser_approvals` 是否覆盖发送审批需求 |
| Candidate Extraction | 以现有 33 列表为迁移基线 |
| Extraction Revisions | 复用现有 9 列修订表 |
| Candidate Matching | 先审查现有去重字段，不并行建新结构 |
| Send Executions | 审查现有执行记录能力，不预设新表 |
| Audit Events | 检查现有审计结构和事件关联方式 |

若现有 `browser_approvals` 无法表达一次性消息审批、消息 Hash、目标身份 Hash
和过期时间，可提出受控迁移或新增字段。**但必须先有字段级差异报告**，
再决定扩展还是新建。

### ⚠ 实测：现有 `browser_approvals` 已具备所需机制

```
browser_approvals (20 列)
  session_id, task_id, action_id, request_type, risk_level,
  action_digest, summary, state, requested_by, requested_at,
  decided_by, decided_at, decision_note, expires_at, consumed_at,
  id, owner_id, tenant_id, created_at, updated_at
```

逐项对照裁决要求的四项：

| 裁决要求 | 现有字段 | 结论 |
| --- | --- | --- |
| 一次性消息审批 | `state` + `consumed_at` | ✅ 已有 |
| 消息 Hash | `action_digest`（String(64)） | ✅ 已有（语义为动作摘要，可承载消息摘要） |
| 目标身份 Hash | `action_digest` / `summary` | ⚠ 需确认是否需独立字段 |
| 过期时间 | `expires_at`（NOT NULL） | ✅ 已有 |

且基线已有 **one-shot pairing nonce hash** 机制（`browser_connectors.key_epoch`
+ nonce hash，见 `backend/app/models/browser.py:400-425`）。

**这支持裁决的结论**：`browser_approvals` 很可能**无需新建**即可承载发送审批，
B0-08 的字段级差异报告重点应放在「目标身份 Hash 是否需要独立字段」。

---

## D-5 · B1 / B2 是否批准

**结论：暂不批准进入实施。**

- B1：等待 B0 验收
- B2：等待 B1 POC 验收

三张卡不能视为一个整体批准。

---

## D-6 · 浏览器发送能力

**结论：ICA V2.0 允许浏览器辅助消息发送**，但**不允许 Hermes Agent 获得直接发送权限**。

两个方案的表面矛盾，通过区分「Agent 能力」与「ICA 执行能力」解决：

| 能力 | Hermes Agent | ICA Application | Browser Worker |
| --- | --- | --- | --- |
| 搜索候选人 | 可请求 | 授权和控制 | 执行 |
| 采集候选人 | 可请求 | 校验和入库 | 执行 |
| 分析候选人 | 可请求 | 管理数据 | 不负责 |
| 生成消息草稿 | 可请求 | 保存和管理 | 不负责 |
| 请求发送审批 | 可请求 | 创建审批 | 不负责 |
| **批准发送** | **不允许** | 用户通过 ICA UI 操作 | 不允许 |
| **执行已批准发送** | **不允许直接调用** | 授权、调度和核验 | 执行指定动作 |
| **绕过审批发送** | **禁止** | **禁止** | **禁止** |

**固定流程**

```
Hermes Agent      → 生成消息草稿、请求审批
ICA Application   → 绑定候选人、账号、会话、消息 Hash
ICA 用户逐条确认  → 显示完整消息、目标身份和平台账号
ICA Application   → 校验一次性审批凭据及消息内容
Browser Worker    → 执行单条发送并返回证据
ICA Application   → 核验结果、更新状态、记录审计
```

**B7 作为 V2.0 规划能力保留，但在 B0 阶段不批准实施。**

---

## D-7 · Hermes 发送权限

**结论：Hermes 不获得直接发送工具或执行权限。**

### Embedded Agent 文档措辞修订（**修订，不是删除安全约束**）

原文：

> 永远不向 Agent 暴露招聘平台消息发送能力。

修订为：

> 永远不向 Agent 暴露**可直接执行**招聘平台消息发送的**工具或权限**。
> 消息发送只能由 ICA Application 在用户逐条审批后，通过受控 Browser Runtime 执行。

**执行状态模型**（扩展原 `Approved / Copied / Sent`）：

```
Draft → Pending Approval → Approved → Executing → Sent / Failed / Unknown
```

**`Unknown` 必须进入人工核验，不能自动重试。**

---

## D-8 · Candidate Extraction

**结论：复用现有表和写入链路，禁止平行建模。**

## D-9 · W7 与 B4

**结论：W7 迁移；B4 改为既有结构上的集成增强。**

| 能力 | 归属 |
| --- | --- |
| 既有抽取记录迁移 | W7 |
| `browser_candidate_extractions` 写入 | W7 |
| Extraction Revisions | W7 |
| `confirm-candidate` | W7 |
| duplicate 字段迁移 | W7 |
| 新浏览器页面解析 | B3 |
| 新解析结果进入现有 Extraction 流程 | B4（调整后） |
| Candidate 标准化、去重业务增强 | 基于现有模型扩展 |
| Candidate 分析和岗位匹配 | Agent / Application 领域能力 |

**W7 与 B4 不得各自维护 Candidate Capture 的状态机、去重规则和持久化结构。**

B0 裁决记录须写明：

> W7 是现有 Browser Candidate Extraction 数据与写入链路的迁移波次。
> Browser Subsystem 的 B4 不重新建立平行的 Candidate Capture 数据模型，
> 必须通过既有 Extraction 模型及 Application Service 完成新页面采集结果的接入。

---

## D-10 · Browser Engine

**结论：** B1 POC 验证 Playwright + Chromium + 独立 Worker。

**B1 暂不批准实施**（见 D-5），但技术方向已定。

---

## D-11 · 旧 Browser Routes

**结论：** 独立 **BW0** 兼容迁移评估，在 B0 路由盘点后确定范围。


---

## D-12 · D-1 的并行口径（补充裁决，2026-11-20 00:54 CST）

**背景**：D-1 原文「先恢复 W4 工程可信度，再开放任何新产品线」与 §12
「G3、F0-AUDIT 可立即开始」之间存在歧义。架构评审 §2.6 指出这处治理歧义。

**裁决**（按总方案 §4.15 建议口径）：

> **允许**不触碰 W4 实现、不改变迁移契约的**调查与架构审计**并行开展；
> **暂不允许** Browser Runtime、Agent 业务功能及前端通信改造**进入正式实施**。

### 逐项授权表

| 工作 | 可并行 | 理由 |
| --- | --- | --- |
| G3（Hermes POC，仅 `ica.ping`） | ✅ | 只读验证运行时能力，不碰业务数据与契约 |
| F0-AUDIT（前端接口耦合审计） | ✅ | 只读审计，不改代码 |
| B0（架构准入与裁决） | ✅ | 产出决策与文档，不写实现 |
| AI-EXT（契约文档） | ✅ | 只产出契约文档 |
| W4-R1～R4 | ✅ | 本身就是要先做的 |
| ADR-BATCH（ADR 落地） | ✅ | 文档与裁决记录 |
| QA-L5（对拍框架） | ✅ | 测试框架建设，不碰业务实现 |
| BW0（Browser 路由迁移） | ❌ | 等 Phase 2/3 门槛 |
| B1 / B2（Browser 实现） | ❌ | 等 B0 验收（见 D-5） |
| 前端 F0（实现） | ❌ | 等 F0-AUDIT 结论 + 前端归属裁决 |
| W5/W6 实现 | ❌ | 等 Phase 1 闭环 |

### 判据

一项工作是否可并行，只看两条：

1. **是否修改 `crates/` 下的实现代码**（W4 返工除外——那就是本阶段的工作）。
2. **是否改变冻结契约**（`docs/rust-migration-contract.md` 的 68 条与波次划分）。

两条都不触犯 → 可并行；任一触犯 → 不得进入实施。

> **这条规则的解释权归用户**。执行方判断不准时按「不可并行」处理（fail-closed），
> 不要用「我认为这不算实现」来扩大授权。

---

## 附：文档勘误

《ICA Browser Subsystem Architecture R1.0》§1.3：

> 原文：当前 ICA-RUST 的 68 条 API 迁移契约、**W1–W7 波次**和跨语言对拍体系保持独立。

**更正为：**

> R0 + W1–W6：原始 68 条 Python API 迁移路线。

W7 是独立的 Browser Candidate Extraction 迁移波次，**不属于原 68 条**。
同理，B0–B8 与 BW0 均在 68 条之外。

---

## 附：实测基线（2026-11-19）

| 事实 | 值 |
| --- | --- |
| 基线 HEAD | `9ed173c`（契约锚点 `d39e321`） |
| 库内总表数 | 32 |
| 已有 browser 表 | **12 张** |
| `browser*` 未迁移路由 | **39 条**（前端有 33 处调用） |
| Rust 侧 browser 业务实现 | **无**（仅 `column_defaults.rs` 有表名条目） |
| 迁移契约 | 68 条（R0 + W1…W6），已实现 45 |
| 现有 browser 表清单 | `browser_profiles` / `browser_sessions` / `browser_tasks` / `browser_task_steps` / `browser_actions` / `browser_permissions` / `browser_approvals` / `browser_connectors` / `browser_connector_sessions` / `browser_tabs` / `browser_candidate_extractions`(33 列) / `browser_candidate_extraction_revisions`(9 列) |
