# B0 交付物包 — 浏览器子系统架构基线与平台准入 R1.0

> ICA-RUST B0 卡（v2）的交付物：B0-01…B0-09、B0-11 逐项产出；
> B0-10 的路由归属结论见 §10 与 `browser-route-inventory-r1.md` 的结论节。
> 实测日 2026-11-20；基线锚点 `9ed173c`；Rust 侧快照 `1fefda6+`（见
> `docs/migration/snapshot-r1.md`）。
>
> **状态声明**：本包是 D-4 批准的「准入工作」产出。B0 门禁原文：
> 「没有平台准入结论，不进入真实招聘平台的自动化开发」——本包不触碰
> 真实平台，不写浏览器自动化代码（B1/B2 受 D-5 约束暂不实施）。
>
> **2026-11-20 更新：B0-09 已签署（见 §B0-09 签署栏，签署依据为用户原话）。**
> 按签署口径：B0 进入验收阶段；**B1 解冻**（用户裁决加速了 D-5 的原始口径
> 「B0 验收通过后恢复可实施」——以签署时点为准）。B2 仍按 D-5 等待
> B0 验收 + B1 POC 验收两道前置。B1 开工时其判据不变，另受 D-10 技术方向
> 与「选型证据非定型」两条约束。

---

## B0-01 Browser Architecture Baseline（crate 边界审查）

**七个 crate 的依赖边界（实测，编译期强制）：**

```
ica-server → ica-api + ica-infra → ica-application → ica-domain
ica-desktop（Tauri IPC 适配）→ ica-application + ica-contracts
ica-contracts（wire 契约）→ ica-application（payload 类型）
ica-domain：无 axum/sqlx/tokio/hyper 依赖（Cargo.toml 编译期强制）
```

- `scripts/check.py` 第 1 关（layering）持续守卫：内层 crate 依赖中不得出现
  web 框架 / DB 驱动 / 异步运行时 / 适配器 crate。
- **浏览器能力落位（按方案 §3）**：进程管理与窗口归 Tauri Host；浏览器控制归
  Browser Runtime（独立进程，B1 POC 定形态）；策略/授权/审批/审计归
  ica-application（新增 ports + use cases，**不新增第八个 crate**）；持久化复用
  ica-infra。
- **边界结论**：现有七 crate 结构足以承载浏览器子系统；Browser Runtime 作为
  独立进程（ADR-BROWSER-001/006），通过 ica-application 的 port 与 ICA 通信；
  ica-domain 新增 `candidate_extraction`（W7 已落）证明该层可承载浏览器域的纯规则。

## B0-02 Desktop Integration Report（Tauri IPC 接入点）

- 现有 IPC 命令：**47 个**（`grep -c '^pub async fn' crates/ica-desktop/src/commands.rs`），
  覆盖全部 45 条契约端点 + health + confirm-candidate。
- 命令层模式：`parse_body → ica_contracts 校验 → ica_application use case → DTO`，
  与 ica-api 共享同一契约层（Tauri-audit R0.1 §4.3 方案 C：无双重实现）。
- 浏览器接入点预留：Browser Runtime POC（B1）将以独立 Worker 进程 +
  Worker IPC 接入 Tauri Host，**不经过 ica-desktop 的业务命令层**；
  `imports_confirm_candidate` 的加入未改变该边界。

## B0-03 Platform Scope Decision（首个平台与目标页面）

- **建议值（待用户签署确认）**：首个招聘平台 = **BOSS 直聘**（zhipin.com）——
  ICA Personal 的既有业务域（种子数据、解析器标签别名、浏览器抽取源头均为此平台）。
- 目标页面族：沟通列表页 → 候选人详情页 → 简历/附件页（与 W7 已迁移的
  candidate-extraction 数据模型对齐）。
- **门禁**：本结论 + B0-04 合规报告 + B0-05 威胁模型齐备并经用户确认后，
  B3（Platform Adapter）方可开工。

## B0-04 Platform Compliance Report（平台规则与自动化边界）

- 平台条款要点（BOSS 直聘用户协议与robots/反自动化实践）：禁止未经授权的
  自动化访问、验证码与人机验证必须人工完成、频率限制与账号风控存在。
- **ICA 的边界（与 ADR-BROWSER-010 一致）**：
  1. 不绕过登录保护、验证码、访问限制或任何安全机制；
  2. 自动化仅操作用户已人工登录的 ICA 专用 Profile 会话；
  3. 采集频率与范围受 ICA 侧节流（B3 实施时落到 Controller 层）；
  4. 发送必须逐条人工审批（ADR-BROWSER-005），无批量路径；
  5. 平台规则的「可执行范围」作为 Adapter 上线准入条件（每平台一份）。
- 本报告为**原则层**结论；每个 Adapter 的具体准入清单在 B3 逐平台产出。

## B0-05 Browser Threat Model

资产：候选人 PII（姓名/联系方式/经历）、平台登录态（Cookie）、发送能力、执行证据。

| 威胁 | 缓解（设计层约束） |
| --- | --- |
| 采集内容泄漏出本机 | 本地优先（local-first）；OCR/浏览器全程无外呼（W4 审计实测：模型本地、无网络下载 `ModelDownloadMode::Never`） |
| 登录态被复用/导出 | Profile 隔离（ADR-BROWSER-002/007）；ICA 不读取用户日常浏览器；登录态仅存于专用 Profile |
| Agent 越权发送 | D-7：Hermes 不获得发送工具；ADR-BROWSER-005/008：逐条审批 + MCP 只见业务级工具 |
| 发送不可控 | 每消息一审批；`action_digest` 绑定消息内容；一次性 `consumed_at`；`expires_at` 失效；Unknown 状态进人工核验不自动重试 |
| 页面内容注入（提示注入/恶意页面） | 浏览器 Runtime 不访问业务库；采集文本经既有 privacy filter（mask_hits）进入抽取；Agent 只见业务级工具输出 |
| 执行证据缺失 | 每步 BrowserTask/BrowserAction 状态记录（复用既有四表）；审计事件全链路（ADR-BROWSER-009） |
| 进程崩溃波及核心 | Browser Runtime 独立进程（方案 §2.2），崩溃不连带 ICA Server（B1 AC） |

## B0-07 Browser Contract R1.0（Schema，骨架）

浏览器 API 契约**不进 68 条迁移契约**（方案 §1.3），独立成族：
39 条路由的分组、请求/响应 Schema 与权限要求，以
`docs/architecture/browser/browser-route-inventory-r1.md` 的逐条盘点为底册；
正式 Schema 文件随 B0-10 的归属结论 + BW0 波次产出（每条路由一对
Request/Response 契约，写法与 `ica-contracts` 现有 body/query 模式一致）。
**本包不定义新 Schema 字段**——基线 `9ed173c` 已实现这些路由，契约以基线
实测为准（与迁移工程同一方法：实测 → 冻结 → 对拍）。

## B0-08 Table Reconciliation（表结构对账 + 字段级差异报告）

**对账原则（D-4 裁决原文）：「复用现有表不代表现有表一定足够」——**
逐领域先审查现有表，字段级差异报告先行，再决定扩展或新建。

| 方案提议（11 张） | 现有对应表 | 对账结论 |
| --- | --- | --- |
| browser_profiles | `browser_profiles` ✅已存在 | 复用 |
| browser_sessions | `browser_sessions` + `browser_connector_sessions` ✅ | 复用；connector 会话职责在 B0-02 边界内独立 |
| browser_tasks / browser_actions | `browser_tasks` / `browser_task_steps` / `browser_actions` ✅ | 复用（B1 AC 的状态记录落在这些表） |
| platform_accounts | 无直接对应（`platform_accounts` 未建） | **Deferred 到 B3**：首个平台确认（B0-03）后再定列，避免为未定平台建表 |
| candidate_captures | `browser_candidate_extractions`（33 列）✅ | **同一件事两种命名 → 复用现有表**（D-9：W7 已以其为迁移基线） |
| candidate_matches | `duplicate_kind` / `duplicate_of_candidate_id` / `duplicate_evidence_json` 三列 | **复用现有去重字段**，不并行建结构（B4 裁决见 §11） |
| message_preparations | 无 | **Deferred 到 B6** |
| send_approvals | `browser_approvals`（20 列）✅ | 复用，见下方字段级差异 |
| send_executions | 审计事件 + `browser_actions` | **Deferred 到 B7**：先审查现有执行记录能力 |
| browser_audit_events | `audit_events`（统一审计表，`browser.*` 事件类型） | 复用统一审计（与迁移工程同一张表，事件类型区分） |

**`browser_approvals` 字段级差异报告**（D-4 点名的四个判据）：

| 判据 | 现有字段 | 结论 |
| --- | --- | --- |
| 一次性消息审批 | `state` + `consumed_at` | ✅ 已覆盖 |
| 消息 Hash | `action_digest`（String(64)） | ✅ 已覆盖 |
| 目标身份 Hash | 无独立列（`action_digest`/`summary` 兼载） | ⚠️ **唯一开放项**：目标身份（候选人/账号/会话三元）是否需要独立 Hash 列。B7 开工前由字段级迁移报告裁决；基线已有 one-shot pairing nonce hash（`browser_connectors`）可参照 |
| 过期时间 | `expires_at`（NOT NULL） | ✅ 已覆盖 |

## B0-09 Product Change Record V2.0（待用户签署）

以下记录**内容已由 D-6/D-7 裁决给出**，本节把它们落成正式变更记录。
**签署栏留空——由用户本人签署后生效**（B0 卡原文：不得由执行 Agent 自己写结论）。

---

### ICA 产品边界变更记录 V2.0（草案，待签署）

**变更内容**：ICA V1.x 的「消息发送由用户在平台手动完成」边界，变更为
**「允许用户逐条确认后的 ICA 受控发送」（D-6）**；Hermes（Agent）**不获得
直接发送工具或执行权限**（D-7）。

**Agent 与 ICA 的能力分离（裁决原文九行表精神）**：

| 能力 | Agent (Hermes) | ICA Browser/App |
| --- | --- | --- |
| 候选人分析/匹配 | ✅（业务级工具） | ✅ |
| 页面采集 | 经业务级 MCP 工具发起 | ✅ 执行 |
| 草稿起草 | ✅（AI 起草） | ✅ |
| 发送审批 | ❌ 只能接收审批事件并提示用户 | ✅ 呈现审批 UI |
| 消息发送 | ❌ 无发送工具 | ✅ 用户逐条确认后执行 |
| 批量自主发送 | ❌ 无 | ❌ 无（明确不开放） |
| 业务库直接访问 | ❌ | ✅ |
| CDP/DOM/任意 JS | ❌ | ✅（Runtime 内部） |
| 执行证据 | 只读引用 | ✅ 写入 |

**固定执行流程（七步）**：起草 → 用户审批（绑定消息 Hash + 目标 + 有效期）→
Approved → 执行（Browser Runtime）→ 核验 → Sent 记录 → 证据归档。
状态模型：`Draft → Pending Approval → Approved → Executing → Sent / Failed / Unknown`；
**Unknown 必须进入人工核验，不得自动重试**。

**生效条件**：Browser Runtime（B1/B2）与发送审批链路（B7）就绪后启用；
在 B7 实施前不得对外宣称可用。

**签署**：

| 项 | 值 |
| --- | --- |
| 签署人 | **Bryan**（项目所有者；本行由执行 Agent 代录——结论口径为用户本人原话，见下） |
| 日期 | **2026-11-20**（CST，会话裁决时刻） |
| 结论口径 | **按上表批准 V2.0 产品边界**（D-6/D-7 定稿内容照录，未由 Agent 增删）；B0 进入验收；B1 解冻 |

**签署依据（用户原话，2026-11-20 会话指令）：**

> 「签署 B0-09（b0-deliverables-r1.md §B0-09 签署栏）——内容已按 D-6/D-7 定稿，
> 只差你签字；签署后 B0 可进入验收、B1 解冻」

代录说明：B0 卡要求「不得由执行 Agent 自己写结论」——本记录的结论内容
全部来自 D-6/D-7 既有裁决与上面引用的用户原话，Agent 仅负责誊录与归档，
未新增、未修改任何边界条款。

---

## §10 B0-10 路由归属结论（39 条 + W7 的 5 条上游）

- 完整逐条盘点底册：`docs/architecture/browser/browser-route-inventory-r1.md`。
- 实测修正（D-3 前提）：29/39 条有 UI 调用方；无 UI 调用的 10 条几乎全是
  connector 系列（浏览器扩展侧调用）——**集合 C（无主废弃路由）大概率为空**，
  盘点起点为「多数保留」。
- **W7 §5 的 5 条 `candidate-extractions` 路由的归属裁决**（卡面要求不能留空）：
  **随 BW0 迁移，不并入 W7 默认范围。** 理由：(a) W7 默认范围（confirm-candidate
  依赖链）已实现并验收；(b) 这 5 条是 create/update/review 的完整 CRUD + 列表，
  与其余 34 条 browser 路由共享 `browser.py` 路由模块、schema 族与任务/动作
  基础设施——拆出来并入 W7 会让 W7 承载 B 线的半套基础设施，违反
  「波次职责不得重叠」纪律；(c) BW0 已由 D-3/D-11 立卡承接 browser API 兼容，
  范围以本盘点为准。W7 卡 §5 的「需单独裁决」项就此关闭。

## §11 B0-11 W7 与 B4 的职责边界（D-9 成文）

> **D-9 裁决原文**：W7 是现有 Browser Candidate Extraction 数据与写入链路的
> 迁移波次。Browser Subsystem 的 B4 不重新建立平行的 Candidate Capture 数据模型，
> 必须通过既有 Extraction 模型及 Application Service 完成新页面采集结果的接入。

| 能力 | 归属 |
| --- | --- |
| 既有抽取记录迁移 | W7（✅ 已实现） |
| `browser_candidate_extractions` 写入 | W7（✅ 已实现） |
| Extraction Revisions | W7（表已就绪；修订写入随 BW0 的 review 端点迁移） |
| `confirm-candidate` | W7（✅ 已实现） |
| duplicate 字段迁移 | W7（✅ 读路径已实现；写路径随 BW0） |
| 新浏览器页面解析 | B3 |
| 新解析结果进入现有 Extraction 流程 | B4（调整后） |
| Candidate 标准化、去重业务增强 | 基于现有模型扩展 |
| Candidate 分析和岗位匹配 | Agent / Application 领域能力 |

**W7 与 B4 不得各自维护 Candidate Capture 的状态机、去重规则和持久化结构。**

## 明确不做（本包）

- 不写浏览器自动化代码（B1/B2 受 D-5 约束）；
- 不进入真实招聘平台自动化（B0 门禁）；
- 不新增第八个 crate；
- 不把 Browser 测试数计入 68 条 API 对拍统计；
- 不替用户签署 B0-09（签署栏留空）。
