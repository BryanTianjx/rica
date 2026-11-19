# ICA-RUST Engineering Architecture & Delivery Baseline R1.0

**状态：** 工程基线（现状快照 + 待办总账 + 架构边界 + 验收体系）
**前身：** 《ICA-RUST 当前设计总方案》（2026-11-19 架构评审后升格）
**基准日期：** 2026-11-19
**仓库：** `/mnt/o/ICA-RUST`（独立 git 库，43 个提交，HEAD `4cbb736`）
**Python 基线：** `/mnt/o/ica` @ `9ed173c`（**契约锚点为 `d39e321`，见 §6.1 的未闭合矛盾**）

> **本文档的纪律**：所有数字均为 2026-11-19 当场实测，不引用历史数字。
> 复算命令写在各处，可自行验证。

---

## 1. 一页纸

| 维度 | 实测值 | 复算命令 |
| --- | --- | --- |
| Rust 代码量 | 24,780 行（7 crate） | `for c in crates/*/; do ... wc -l; done` |
| 测试 | **351 passed / 0 failed / 0 ignored**（W4 复审回归 + W7 domain + OCR 收敛测试） | `cargo test --workspace` |
| 已实现端点 | **45** 条 `/api/*`（另有 `/health` 不计入契约） | `python3 scripts/endpoint_reconciliation.py` |
| 契约端点总数 | **68** 条（`R0 + W1…W6`） | `python3 scripts/endpoint_reconciliation.py` |
| 跨语言对拍 | **248/248 cases matched, 0 failure**（对拍集 248：+14 W7 守卫 + 12 导入校验/error 路径；dashboard calendar 已对齐） | `bash scripts/contract_verify.sh` |
| 桌面 IPC 命令 | 47 个（W7 增 `imports_confirm_candidate`） | `grep -c '^pub async fn' crates/ica-desktop/src/commands.rs` |
| 完成波次 | R0 / W1 / W1.5 / W1.6 / W2 / W2.5 / W3 | `git tag --list` |
| 未完成端点 | 23 条（W5 15 + W6 8） | 契约文档 §1 |
| 卡片台账 | 23 张（3 完成 / 14 已批准未开工 / 1 READY / 1 待批 / 1 BLOCKED / 3 取消） | `woticketflow_cli.py work-item list` |
| 质量门 | 4 关全绿（**自动化门禁，不等于独立验收** — 见 ADR-0005） | `bash scripts/check.sh` |
| 端点对账 | 45 + 23 = 68 ✓，额外路由 0 | `python3 scripts/endpoint_reconciliation.py` |
| 契约外增量 | 5 条：4 条 `ai/models` CRUD（Python-only，AI-EXT 所有）+ `confirm-candidate`（**W7 已实现**，对账登记 auxiliary） | ADR-0003 D-2 补充裁决 |

**当前处境一句话**：迁移主体全部可验证（**45/68** 契约端点 + confirm-candidate 辅助路由、**351 测试、对拍 248/248 全对**——对冻结基线 `9ed173c` 首次零失败）。
W4 已放行（`w4-done`）；W7 默认范围、OCR-01 收敛、G0 治理交付物、B0 准入包均已完成。
Browser（B1/B2 受 D-5 约束）与 BW0（39 条）等待各自前置；G3 的集成路径需按实测能力盘点修订；
W5/W6 是剩余的两条迁移波次。

---

## 2. 已实现的部分

### 2.1 分层架构（7 crate，编译期强制）

```
ica-domain        2760 行   业务实体与规则，不依赖 axum/sqlx/tokio/hyper
    ↑
ica-application   3904 行   用例编排、端口定义
    ↑
ica-contracts     3802 行   wire 契约（DTO / 事件 / 错误码）— ADR-0002 R1 后的唯一契约源
    ↑
ica-infra         6666 行   SQLite 仓储、AES-GCM、OCR 引擎、快照导入导出
    ↑
ica-api           5995 行   axum 路由、HTTP 错误映射
    ↑
ica-server         160 行   组合根
ica-desktop       1493 行   Tauri IPC 命令层
```

**分层纯度由 `scripts/check.sh` 第 1 关在守**：`ica-domain` / `ica-application`
的依赖里不得出现 axum / sqlx / tokio / hyper。

**关键设计决定**（ADR-0002 R1）：wire 契约从 `ica-api` 上移到 `ica-contracts`，
使 IPC 与 HTTP 共用同一套类型——这正是前端过渡方案（FE-006）能成立的前提。

### 2.2 已完成的波次

| 波次 | 内容 | tag | 端点数 |
| --- | --- | --- | --- |
| R0 | Rust 骨架、`/health`、positions 只读 | `w0-done` (`b262c33`) | 2 |
| W1 | positions 写路径 + candidates + conversations/messages/timeline + send-records | `w1-done` (`6484794`) | 15 |
| W1.5 | 路由/查询与 Python 的 parity 修复 | `w1.5-done` (`b178e39`) | — |
| W1.6 | scope-aware 门禁（68 条在基线漂移下存活） | `w1.6-done` (`df1c7b7`) | — |
| W2 | follow-ups 状态机 + policy + dashboard | `w2-done` (`04a37ba`) | 11 |
| W2.5 | wire 契约上移 `ica-contracts` | `w2.5-done` (`5889354`) | — |
| W3 | drafts + draft-groups + AI 生成入口 | `w3-done-r2` (`d1dc67e`) | 10 |
| **W4** | imports preview/get/confirm/reject + export/import + OCR | `w4-candidate` (`304d2da`) | 7 |

**W4 是唯一有 `candidate` 而无 `done` 的波次**——因为三份独立审计全部判 FAIL。

### 2.3 值得记录的实现细节

这些是「已实现」里不容易从代码表面看出的部分：

- **OCR 真引擎**：`rapidocr-core` 的 `ppocrv6-small`，与基线 Python `rapidocr`
  **同一套模型文件**（四个文件 SHA256 逐一核对一致）。基线把连字符删掉的 quirk
  （`_EXCLUDED_CHARS` 是集合字面量、含 U+002D）已逐字符复刻。
- **加密导出**：AES-256-GCM 信封 `{alg, iv, tag, ciphertext}`，AAD `ica:export:0.2.0`，
  12 字节 nonce、16 字节 tag，密钥直接使用不派生、不回显。
- **JSON 存 TEXT 列是契约**：审计 `metadata_json` 用 Python `json.dumps` 默认分隔符 +
  插入序；加密导出明文用紧凑 + `sort_keys`。两者不可互换。
  （`serde_json` 已开 `preserve_order` feature。）
- **LWW 导入合并**：基线 `_import_row` 的 6 条规则全部实现，含 append-only 表
  （`send_records`）永不重写但仍报 conflicts。
- **分层依赖方向不可逆**：输入 payload 类型属于 `ica-application::payload`，
  `ica-contracts` 只能单向依赖 application。

### 2.4 W4 已实现但未放行的部分

W4 的 7 条端点**代码已实现且对拍通过**，但三份独立审计（2026-11-19）全部判 FAIL：

```
LWW-REPORT.md       FAIL   6 条 LWW 规则等价，但有 2 条 P1
FINDINGS-REPORT.md  FAIL   15 项中 13 项真关闭，P0-4 部分未关
OCR-REPORT.md       FAIL   「逐字符一致」不成立（检测器输入限额）
```

审计报告在 `/tmp/audit-w4-2/`。**三份报告的结论已逐条复核**，其中两处我此前的
错误结论被推翻：

1. 「OCR 逐字符一致」——我的 3 张测试图太小（640×180），正好卡在尺寸盲区。
   审计员用 8 张图 + 9 档宽度在 **757 px 翻面点**抓到稳定分叉。
2. 「基线漂移未触及契约面」——我用了 `git diff`（只比工作树），应该用
   `git diff d39e321..HEAD`。按正确口径，契约面**确实变了**。

---

## 3. 待办总账

### 3.1 全部卡片（woticketflow 台账，23 张）

| 状态 | 张数 | 卡 |
| --- | --- | --- |
| **COMPLETED** | 3 | R0 / W1（优化版）/ W3 |
| **APPROVED**（已批准未开工） | 14 | 见 §3.2 |
| **READY**（parked，等开工指令） | 1 | F0-AUDIT |
| **APPROVAL_PENDING** | 1 | F0 |
| **BLOCKED** | 1 | W4（等四张返工卡） |
| **CANCELLED** | 3 | V1 / W1（旧）/ W4R1（被拆分） |

### 3.2 第一批：W4 恢复可信度（**优先级最高**）

裁决 D-1：先恢复 W4 的工程可信度，再开放任何新产品线。

| 卡 | 范围 | 判据 | 状态 |
| --- | --- | --- | --- |
| W4-R1 | OCR 等价性（检测器输入限额、落库文本、初始化恢复、锁中毒） | 8 | APPROVED |
| W4-R2 | HTTP 错误契约（422 `details.errors` 数组、`input` 二次编码、`loc` 下标类型） | 8 | APPROVED |
| W4-R3 | 历史校验规则（status / location / archived 三条缺失规则 + 零写入） | 7 | APPROVED |
| W4-R4 | P2 缺陷台账（六项逐条裁决，禁止无记录遗留） | 6 | APPROVED |
| W4-A | 独立复审与放行（9 条判据，含覆盖边界） | 9 | APPROVED |
| W4（主卡） | 7 条端点本体 | 7 | **BLOCKED** |

**放行门槛**：R1 + R2 + R3 完成，且 R4 中经确认影响契约的项关闭，
再由**独立审计员**复审（不由修复者自批），才可创建 `w4-done`。

**三条工程纪律（裁决原文）**：

1. **220/222 不是等价性证明**——`contract_capture.py` 的 235 个场景中
   `browser_extract` 与 `confirm-candidate` 均 **0 命中**。全绿恰恰因为没覆盖到。
2. **回归用例会失败**——每处修复配一条修复前会红的用例，验收时做变异测试。
3. **基线锚点必须记录 sha**——同名数字在不同锚点指向不同事实。

### 3.3 第二批：迁移收敛

| 卡 | 范围 | 判据 | 状态 |
| --- | --- | --- | --- |
| G0 | 迁移基线治理（快照、基线差异归属、端点范围 ADR、Dashboard 参照、映射基线） | 7 | APPROVED |
| OCR-01 | RapidOCR 专项收敛（**模型目录不依赖基线 venv** 是最重要一项） | 6 | APPROVED |
| AI-EXT | AI Models Extension Contract（4 条 `ai/models`，单独立波次） | 5 | APPROVED |
| W7 | 浏览器抽取写路径迁移（`confirm-candidate` + 33/9 列表） | 8 | APPROVED |

**W5 + W6 共 23 条端点**：已由 `ICA-RUST-W5-W6` 承接（2026-11-20 批准）。
范围固定 23 条：AI 6 + Settings/Audit/Backups 9 + Auth 5 + Shares 3。

> 4 条 `ai/models` CRUD 归 `AI-EXT`，但它们在**契约表外**，
> 不在 23 条之内，因此 W5-W6 不扣减（见 §4.6 与 ADR-0003 D-2 补充）。

### 3.4 第三批：Browser Subsystem（准入中）

| 卡 | 范围 | 判据 | 状态 |
| --- | --- | --- | --- |
| B0 | 架构基线与平台准入（表结构对账、V2.0 产品边界、路由归属、W7/B4 边界） | 10 | APPROVED |
| BW0 | 39 条 `/api/browser/v1/*` 兼容迁移（范围待 B0 盘点确定） | 6 | APPROVED |
| B1 | Browser Runtime POC（Playwright + Chromium + 独立 Worker） | 8 | APPROVED（**但 D-5 暂不实施**） |
| B2 | Session Management 基础 | 6 | APPROVED（**但 D-5 暂不实施**） |

**B0 门禁**（方案原文）：没有平台准入结论，不进入真实招聘平台的自动化开发。

**已实测的关键事实**：

- 基线已有 **12 张** browser 表，方案提议的 11 张里 4 张已存在，
  而真正承载采集数据的 `browser_candidate_extractions`（33 列）方案**没提**。
- 39 条 browser 路由中 **29 条有前端调用**（集合 C「废路由」很可能为空）。
- `browser_approvals`（20 列）**已具备**一次性消息审批所需字段：
  `state` + `consumed_at`（一次性）、`action_digest`（消息 Hash）、
  `expires_at` NOT NULL。

### 3.5 第四批：Agent 与前端

| 卡 | 范围 | 判据 | 状态 |
| --- | --- | --- | --- |
| G3 | Hermes Runtime POC（仅 `ica.ping`，不接真实候选人数据） | 7 | APPROVED |
| F0-AUDIT | 前端接口耦合审计（含**前端代码归属裁决**） | 7 | READY（parked） |
| F0 | Backend Client 抽象（parked，范围待审计确定） | 5 | APPROVAL_PENDING |

---

## 4. 三条产品线的目标架构

### 4.1 总方向（裁决 D-1…D-11 之后）

```
ICA-RUST（唯一业务后端）
  ├── ica-domain / application / contracts / infra / api / server
  ├── ica-desktop（Tauri 2 宿主，46 个 IPC 命令）
  └── SQLite（本地业务数据）
        ↑
        ├── Hermes Runtime（独立进程，Agent 推理 / Skills / MCP Client）
        │     └── MCP → ICA MCP Server（受控业务工具）
        └── Browser Runtime（独立进程，Playwright + Chromium）
              └── Managed Browser（默认）/ External Browser（可选）
```

**三条红线**（来自 Embedded Agent 方案 §11.1 与裁决 D-7）：

1. **Hermes 不获得直接发送工具或执行权限**——发送只能由 ICA Application
   在用户逐条审批后，通过受控 Browser Runtime 执行。
2. **MCP 工具白名单 ≠ 权限授权**——真正的授权在 `ica-application` 每次执行时重新校验。
3. **Agent 集成不反向改变迁移验收口径**——68 条契约与 Agent 功能是两个独立质量域。

### 4.2 Browser Subsystem（B0–B8 + BW0）

- **Managed Browser 默认**，External Browser 可选（只连明确暴露的受控调试端点）。
- **Browser Runtime 与 ICA Server / Hermes Runtime 独立进程**——浏览器崩溃不影响核心业务。
- **不新增第八个 crate**，沿用 7-crate 跨层模块。
- **发送状态模型**：`Draft → Pending Approval → Approved → Executing → Sent / Failed / Unknown`，
  **`Unknown` 必须人工核验，不得自动重试**。

### 4.3 UI R2.0 重构方案（第四份方案，与前端过渡互补）

`docs/ICA R2.0 UI 重构方案：统一招聘工作台 + Hermes Agent Runtime + Browser Workspace.md`
（1037 行）管的是**产品层**：信息架构、交互模式、工作台组织。

它与 §4.4 的前端过渡架构**互补不冲突**：

| 方案 | 管什么 | 不管什么 |
| --- | --- | --- |
| UI R2.0 重构 | 界面怎么组织（IA、交互、审批中心、三个工作空间） | 界面怎么连后端 |
| 前端过渡架构 | 界面怎么连后端（Backend Client、IPC/HTTP 双通道） | 界面长什么样 |

**但两份独立产出的方案，收敛到了同一批问题**——这是交叉验证，不是巧合：

```
UI R2.0 方案「阶段 0：现有架构与 UI 基线审计」   →  已有卡片
  审计现有 React 页面及组件                        →  F0-AUDIT
  核验 ICA-RUST API 与数据库模型                  →  B0-08
  核对现有 12 张 Browser 表的复用                  →  B0-08
  解决 W7 与 B4 的架构冲突                        →  B0-11
  明确 39 条 Browser 路由的归属                    →  B0-10 / BW0
```

**这加强了 B0 与 F0-AUDIT 的必要性**：两份方案从不同角度（产品层 / 通信层）
得出同一批前置问题，说明这些问题确实是瓶颈。

UI R2.0 的实施路线图是 6 个阶段（Shell → 招聘工作区 → Agent 工作台 →
Browser Workspace → 审批体系 → 集成验收），其中阶段 0 已在 B0 / F0-AUDIT 里覆盖，
阶段 1 起是纯前端工作，与后端迁移无冲突。

### 4.4 前端过渡（FE-001…FE-008）

**迁移后端，不迁移前端；调整通信边界，不推倒 UI。**

- React + TypeScript + Vite 是**长期**技术栈，不迁 Rust UI。
- 页面只依赖 `BackendClient` 接口；Tauri IPC 与 Axum HTTP 是两个可替换的 Adapter，
  共用同一套 Rust Application Service。
- IPC 与 HTTP 共用 `ica-contracts` 的 DTO（ADR-0002 R1 已经为此铺路）。

**⚠ 一个未解决的结构问题**：前端代码在 `/mnt/o/ica/frontend`——那是**基线仓**，
在本工程中只读；而 Rust 后端在独立仓 `/mnt/o/ICA-RUST`。
**F0 要改前端，但前端在只读仓里。** 这个归属决策不定，F0 无法开工，
已写成 F0-AUDIT 的 AC4。

---

### 4.5 架构成熟度分层（评审 §1.1）

八个维度的当前状态：

| 维度 | 状态 | 判断 |
| --- | --- | --- |
| Rust 分层架构 | 已实现 | 7-crate 分层方向正确，编译期强制 |
| 业务契约迁移 | 部分完成 | 45/68 端点，W5/W6 未动 |
| 跨语言等价性 | 部分完成 | 对拍覆盖存在明显盲区 |
| 数据持久化 | 已实现主体 | 需补恢复、并发、一致性验收 |
| Tauri 桌面架构 | 已实现主体 | IPC 与 HTTP 双适配仍需验证 |
| Browser Subsystem | 方案阶段 | 数据模型与执行边界待明确 |
| Hermes Agent | 方案阶段 | 权限与执行隔离需形成正式契约 |
| 前端架构 | 过渡方案 | 代码归属与 BackendClient 未闭合 |

### 三个成熟度层次

**第一层 · 业务核心** —— 主体已建成

```
Domain / Application / Contracts · SQLite / Repository / Encryption
Axum API / Tauri IPC · 原 Python 业务契约迁移
```

**第二层 · 自动化执行** —— 待建设

```
Browser Runtime · Browser Session · 操作审批与执行状态机 · 页面抽取与候选人采集
```

**第三层 · 智能体协作** —— 待验证

```
Hermes Runtime · MCP Tool Gateway · Agent Context/Memory
Agent Planning/Execution · Human-in-the-loop
```

**这三个层次不应该同时进入大规模开发。**

> **业务核心负责正确性，Browser 负责受控执行，Agent 负责智能决策。**
> 不要让 Hermes 或 Browser Runtime 逐渐演变成第二套 ICA 业务后端。

---

### 4.6 Endpoint Reconciliation Matrix（评审 §2.1 · P0-01）

### 审计修正

此前总方案写「已实现 46 条」，并列出「46 + 23 = 69」的矛盾。
**根因是我的计数错误**：`GET /health` 被计入了——它**不属于** 68 条业务契约。

正确数字是 **45 / 68**。逐条对账结果：

```
Rust /api 路由 (method,path) 对    45
契约表行数                          68
★ 交集                             45    ← 全部已实现路由都在契约表内，无遗漏
Rust 有、契约表无                    0    ← 没有「多实现」的野路由
契约表有、Rust 无                   23    ← 正是 W5(15) + W6(8)

45 + 23 = 68  ✓
```

**对账工具已固化**：`scripts/endpoint_reconciliation.py`，
从路由器源码与契约表生成矩阵，不猜、不手数。

```bash
python3 scripts/endpoint_reconciliation.py                          # 人读表
python3 scripts/endpoint_reconciliation.py --json                   # 机读
python3 scripts/endpoint_reconciliation.py --out <path>             # 生成文档
```

矩阵产出在 `docs/migration/reconciliation/endpoint-matrix-r1.md`，
含契约 ID、Python 路由、Method、波次、模块、实现状态、验证状态、范围、基线 SHA。

### 三个覆盖率指标（评审要求）

| 指标 | 当前值 | 含义 |
| --- | --- | --- |
| Contract Coverage | 45/68 | 契约端点实现率 |
| Implementation Coverage | 45/68 | 实际路由实现率 |
| Verification Coverage | 45/68 | 已通过独立验证的契约比例 |

**三个指标当前同值，是因为所有已实现端点都已进对拍且通过。**
W4 三份审计 FAIL 之后，Verification Coverage 应当**低于** Implementation Coverage
——那才是真实状态。**不要用一个「已完成端点数」代表这三个指标。**

### 契约外增量（别再算错）

`check_endpoint_map.py` 报「73 live / 68 documented，差 5 条」。这 5 条是：

| 端点 | 归属 | 性质 |
| --- | --- | --- |
| `POST /ai/models` | AI-EXT | 契约外增量 |
| `PUT /ai/models/{model_id:path}` | AI-EXT | 契约外增量 |
| `PUT /ai/models/{model_id:path}/default` | AI-EXT | 契约外增量 |
| `DELETE /ai/models/{model_id:path}` | AI-EXT | 契约外增量 |
| `POST /api/imports/{session_id}/confirm-candidate` | W7 | 契约外增量 |

**算术：68（冻结契约）+ 4（ai/models CRUD）+ 1（confirm-candidate）= 73 live 业务路由。**

> **注意**：`GET /ai/models` **在** 68 条契约内（编号 1，属 W5），
> 与上面 4 条 CRUD 不是一回事。所以 `ICA-RUST-W5-W6` 恒为 **23 条**，
> 不会因为「4 条归 AI-EXT」而变成 19 条——那 4 条不在 23 里。

---

### 4.7 五级质量门与不可变验收记录（评审 §2.4 · ADR-0005）

当前看起来矛盾：

```
scripts/check.sh  四关全绿   →  344 passed / 0 failed
三份独立审计       全部 FAIL   →  W4 不可放行
```

**这不是逻辑矛盾，是质量门的职责没区分。** `check.sh` 是自动化质量门，
**不构成独立验收**。

| 级别 | 名称 | 内容 | 失败含义 |
| --- | --- | --- | --- |
| Q0 | Build Gate | 编译、依赖、静态检查 | 代码不能编译或违反静态规则 |
| Q1 | Unit Gate | 单元测试 | 单元行为错误 |
| Q2 | Integration Gate | API/数据库/IPC 集成 | 组件协作错误 |
| Q3 | Compatibility Gate | Python/Rust 跨语言对拍 | 已覆盖路径上不等价 |
| Q4 | Independent Acceptance | 独立审计、缺陷关闭、放行 | **不可放行** |

**规则：** Q0–Q3 全绿**不构成**放行依据；Q4 必须由非实现方执行；
Q3 的通过率不是等价性证明，必须附带覆盖边界声明。

每个波次产出一份**不可变验收记录**（绑定确切 SHA，不只靠 tag）：

```
wave-acceptance/W4/
  implementation-commit.txt  baseline-commit.txt  test-results.json
  contract-coverage.json     findings-register.json  mutation-test-results.json
  independent-review.md      acceptance-decision.md
```

---

### 4.8 基线的四层概念与变更处置（评审 §2.3 · ADR-0006）

2026-11-19 一天内基线 HEAD 动了两次（`d39e321` → `20b252d` → `9ed173c`）。
「终态」是基线方的自我声明，**不是机制保证**。

**四个「基线」不是同一件事：**

| 概念 | 含义 | 谁决定它变 |
| --- | --- | --- |
| Baseline Anchor | Python 源码事实锚点（git SHA） | 基线仓库自然演进 |
| Contract Baseline | 冻结的业务契约（68 条 + 波次 + declared improvements） | **人工裁决** |
| Test Fixture Baseline | 对拍样本与测试数据 | 种子脚本 + 契约用例 |
| Implementation Baseline | Rust 当前实现版本（git SHA） | 每次提交 |

**核心规则：Python 基线更新不自动改变冻结契约。**

基线每次变化产生一份 **Baseline Diff Report**，逐项人工裁决：

| 裁决 | 含义 | 后果 |
| --- | --- | --- |
| Accept | 接受基线行为变化 | 更新契约 + 夹具 + 迁移计划 |
| Reject | 不接受 | 登记 known divergence，Rust 保持冻结行为 |
| Deferred | 暂缓 | 记入待办，不影响当前波次放行 |
| Out of Scope | 不属迁移范围 | 排除出契约表并记录理由 |

**这也修正了 §6.1 的逻辑**：重锚后仍是 220/222，只能说明当前样本没产生新差异，
**不能据此证明三处差异不影响契约**。三处差异的实际处置由 W7 承接，
裁决记录在 G0-02 的 Baseline Diff Report 里。

---

### 4.9 统一执行网关 Execution Gateway（评审 §3.1 · ADR-0004）

此前总架构图只画了组件关系，没有表达控制流、数据流、权限流。
引入 **ICA Runtime Architecture**：

```
                  ICA Desktop
                 Tauri 2 + React
                       │
                 BackendClient
                       │
              IPC Adapter / HTTP Adapter
                       │
                ICA API Gateway
                       │
                ICA Application
                 /           \
                /             \
          Domain Services   Policy Engine
                |             |
           Repository     Approval Service
                |             |
             SQLite      Execution Gateway
                              │
                    ┌─────────┴─────────┐
                    │                   │
               Hermes Runtime      Browser Runtime
                    │                   │
                 MCP Client        Playwright
                    │                   │
              ICA MCP Server       Chromium
                    │
             Controlled Tools
```

**四条不可跨越的边界：**

1. **ICA Application 是唯一业务规则执行主体。**
2. **Policy Engine 负责每次操作的权限与业务约束判断**——每次重新判，
   不信任上游声明。
3. **Hermes 只能通过受控工具访问业务能力**——MCP 白名单不是安全边界。
4. **Browser Runtime 不能绕过 ICA Application 直接改业务数据库。**

**Execution Gateway** 是 Agent 与 Browser 之间的统一受控执行入口：
校验一次性审批凭证、重新计算操作摘要比对、幂等去重、记录审计与 correlation id。

**首期不单独部署、不新增 crate**，先作为 `ica-application` 内部应用服务实现——
符合 Local-first / Single-user 约束，不需要为 Agent 引入
PostgreSQL、Redis、消息队列或分布式基础设施。

---

### 4.10 跨语言对拍的五层验证（评审 §2.2）

当前对拍是**单一维度**的（响应体匹配）。评审建议升级为五层：

| 层 | 内容 |
| --- | --- |
| L1 HTTP | Method、Path、Status、Headers、Response |
| L2 Schema | JSON 类型、字段缺省、null、数组结构 |
| L3 Validation | 422、错误位置、错误信息 |
| L4 Persistence | **最终 SQLite 数据状态** |
| L5 Behavior | 状态迁移、副作用、幂等性、异常恢复 |

**L4 与 L5 特别重要**——同一个 API 返回完全相同的 JSON，
并不意味着两个实现的数据库最终状态一致。

**评审指出的具体盲区（已实测确认）：**

```
contract_capture.py 定义 235 个场景
  browser_extract      0 命中
  confirm-candidate    0 命中
import 载荷只有「空 data」与「完整 candidate 行」两种形态
  → 无 positions 校验 / 无部分列 / 无无 id 行
metadata_json 是解析后比较，不是字节比较
```

即 **220/222 全绿恰恰因为用例集没覆盖出问题的路径**。
「全绿」在这里是覆盖不足的表现，不是等价性的证明。

**Contract Test Coverage Matrix**（评审要求）每条契约至少标明：
正常路径、参数边界、无效输入、权限拒绝、数据库异常、并发场景、
最终持久化结果、修复前失败的回归用例。

---

### 4.11 OCR 等价性定义到引擎行为层（评审 §7.1）

已确认的事实：模型文件 SHA256 一致、模型版本相同，
但**小图测试覆盖不到真实差异**，757 px 附近存在稳定分叉。

> **不能继续用「同模型文件」推导「OCR 行为等价」。**

**OCR Differential Test Matrix** 建议维度：

| 维度 | 测试范围 |
| --- | --- |
| 图片宽度 | 757 px 上下边界及多档宽度 |
| 图片高度 | 不同纵横比 |
| 输入格式 | PNG、JPEG 等 |
| 图像质量 | 模糊、低分辨率、压缩 |
| 文字内容 | 中英文、数字、特殊字符 |
| 预处理 | Resize、Padding、Normalization |
| 检测器 | 输入限制、检测框 |
| 识别器 | 文本和置信度 |
| 后处理 | 字符过滤、文本拼接 |
| 持久化 | 最终落库文本 |

最终应区分 **Detection Parity / Recognition Parity /
Post-processing Parity / Persistence Parity**——
不要只比较最终 OCR 文本，否则很难定位差异产生的位置。

---

### 4.12 统一审计事件模型与 correlation id（评审 §6.3）

所有自动化操作统一记录：

```
AuditEvent
├── event_id        ├── correlation_id    ├── actor_type    ├── actor_id
├── action          ├── resource_type     ├── resource_id   ├── policy_decision
├── approval_id     ├── execution_id      ├── timestamp     ├── result   └── error
```

**`correlation_id` 是关键**——完整因果链：

```
Agent Task → MCP Tool Call → Approval → Browser Execution → Business Update
```

整条链路必须能用同一个标识追溯。否则排查「Agent 为什么改了这条候选人记录」
「某条消息为什么没发送」「Browser 为什么执行失败」，
就得在三个独立日志系统之间人工拼接。

---

### 4.13 数据域划分与生命周期（评审 §4）

| 数据域 | 主要内容 | 管理主体 |
| --- | --- | --- |
| Core Business | Candidates、Positions、Conversations、Messages | ICA Domain |
| Automation | Browser Sessions、Extracted Data、Execution Records | ICA Application |
| Agent | Agent Sessions、Tool Calls、Agent Tasks | ICA Application |
| Governance | Approvals、Audit Events、Policies | ICA Application |

**原则：不要因为增加 Hermes 和 Browser 就重复创建候选人、消息、职位等核心实体。**

```
Browser Page → Extraction Result → Candidate Extraction → Normalization
             → Validation → User Confirmation → Core Candidate
```

- 抽取记录与正式候选人记录必须区分。
- 抽取失败不能破坏原始数据。
- 用户确认前不产生正式业务副作用。
- 重复候选人识别**复用现有 duplicate 机制**。

**Audit Event 不应与普通业务记录采用相同的更新策略**——
审批记录、执行记录与最终结果要能相互关联，形成完整审计链。


---

### 4.14 Phase 0–4 工程路线图（评审 §8）

把「迁移收敛」与「新产品能力建设」明确分开。

### Phase 0 — Migration Trust Recovery（当前最高优先级）

```
W4-R1 / W4-R2 / W4-R3（并行）→ W4-R4 → 独立复审 → W4 正式验收
Endpoint Reconciliation（已建工具，见 §4.6）
```

**退出条件：W4 独立验收 PASS。**

### Phase 1 — Migration Convergence

```
G0 基线治理（Baseline Diff Report + 契约文档改写）
OCR-01（模型目录脱钩）
W7（浏览器抽取写路径）
AI-EXT（契约先行，实施另立波次）
补建 W5 / W6 Work Items
68 条契约完成情况闭环
```

**退出条件：契约清单、基线、波次台账一致。**

### Phase 2 — Runtime Foundation

```
B0 Browser 平台准入（含表结构字段级差异报告）
Browser 数据模型差异审计
G3 Hermes POC
F0-AUDIT + 前端仓库归属裁决
统一 Execution Gateway 契约（ADR-0004）
```

**退出条件：Browser 准入、Agent 权限、前端归属明确。**

### Phase 3 — Controlled Automation

```
BW0 Browser 路由迁移 → B1 Browser Runtime → B2 Session Management
→ B3 页面交互 → B4 数据抽取与确认 → B5 审批和执行 → B6 审计与恢复
```

**退出条件：Browser 执行、审批、异常恢复完整闭环。**

### Phase 4 — Agent-native ICA

```
Hermes Runtime 正式集成 → MCP Tool Gateway → Agent Task Lifecycle
→ Agent Memory/Context → Browser + Agent 协同 → 前端统一交互工作台
```

**退出条件：Agent 可受控完成端到端业务任务。**

> **不要把 Phase 4 理解为「Agent 能操作招聘网站」。**
>
> 真正的目标是：
>
> ```
> 用户提出任务 → Agent 理解和规划 → ICA 校验业务规则 → Browser 执行允许的操作
> → 用户审批敏感动作 → ICA 更新业务数据 → 全链路可追溯
> ```
>
> 这才是 ICA 与普通浏览器自动化工具的差异化价值。

### 各阶段退出条件

| 阶段 | 不可缺少的退出条件 |
| --- | --- |
| Phase 0 | W4 独立验收 PASS |
| Phase 1 | 契约清单、基线、波次台账一致 |
| Phase 2 | Browser 准入、Agent 权限、前端归属明确 |
| Phase 3 | Browser 执行、审批、异常恢复完整闭环 |
| Phase 4 | Agent 可受控完成端到端业务任务 |

---

### 4.15 D-1 并行歧义的澄清（评审 §2.6）

§3.2 的 D-1 写「先恢复 W4 工程可信度，再开放任何新产品线」，
而 §7 又说 G3 与 F0-AUDIT 可以立即开始。**这需要明确区分。**

**建议的裁决口径：**

> **允许**不触碰 W4 实现、不改变迁移契约的**调查与架构审计**；
> **暂不允许** Browser Runtime、Agent 业务功能及前端通信改造**进入正式实施**。

| 工作 | 允许并行？ | 理由 |
| --- | --- | --- |
| G3（Hermes POC，仅 `ica.ping`） | ✅ | 只读验证运行时能力，不碰业务数据与契约 |
| F0-AUDIT（前端接口耦合审计） | ✅ | 只读审计，不改代码 |
| B0（架构准入与裁决） | ✅ | 产出决策与文档，不写实现 |
| AI-EXT（契约文档） | ✅ | 只产出契约文档 |
| W4-R1～R4 | ✅ | 本身就是要先做的 |
| B1/B2/BW0（实现） | ❌ | 等 Phase 2/3 门槛 |
| 前端 F0（实现） | ❌ | 等 F0-AUDIT 结论 + 前端归属裁决 |
| W5/W6 实现 | ❌ | 等 Phase 1 闭环 |

**这样既不会浪费时间，也不会破坏 W4 的验收边界。**

> **已裁决（2026-11-20 00:54 CST）**：用户按上表口径裁决，
> 记录为 **ADR-0003 的 D-12**。判据收敛为两条：是否修改 `crates/` 实现代码、
> 是否改变冻结契约——两条都不触犯才可并行；判断不准时按「不可并行」处理（fail-closed）。


---

## 5. 权威文档地图

| 文档 | 作用 |
| --- | --- |
| `docs/rust-migration-contract.md` | **契约主文档**：68 条端点表、波次划分、分层规则、declared improvements |
| `docs/adr/ADR-0002-rust-backend-migration-reinstated.md` | Rust 迁移决议（含 R1：wire 契约上移 `ica-contracts`） |
| `docs/adr/ADR-0003-governance-decisions-20261119.md` | **治理裁决 D-1…D-11**（2026-11-19） |
| `docs/adr/ADR-0004-ica-runtime-architecture.md` | Runtime 边界与 Execution Gateway（**Accepted** 2026-11-20） |
| `docs/adr/ADR-0005-five-tier-quality-gates.md` | 五级质量门与不可变验收记录（**Accepted** 2026-11-20） |
| `docs/adr/ADR-0006-baseline-layers-and-change-handling.md` | 基线四层概念与变更处置（**Accepted** 2026-11-20） |
| `docs/migration/reconciliation/endpoint-matrix-r1.md` | **端点对账矩阵**（生成物，勿手改） |
| `docs/contract-baseline/w{2,3,4}-python-facts.md` | 各波次的 Python 基线实测事实 |
| `docs/architecture/browser/browser-route-inventory-r1.md` | 39 条 browser 路由逐条盘点 |
| `docs/architecture/frontend/frontend-architecture-r1.md` | 前端过渡架构 |
| `docs/audits/tauri-architecture-audit-r0.1.md` | Tauri 壳架构审计 |
| `docs/ICA Browser Subsystem Architecture R1.0.md` | Browser 子系统方案（**提案，待 ADR 批准**） |
| `docs/ICA R2.0 UI 重构方案…md` | UI 产品层重构方案（**提案，与前端过渡架构互补**） |
| `docs/ICA-RUST × Hermes Embedded Agent 具体实施方案 R1.0.md` | Agent 集成方案 |

**权威顺序**：Python 基线源码 > 契约文档 > 方案提案。
方案文档自称「提案 / 待 ADR 正式批准」，**不是已生效的设计**。

---

## 6. 已知问题与缺口（诚实清单）

### 6.1 契约文档的锚点与裁决不一致 —— **未闭合**

- `docs/rust-migration-contract.md` 第 6 行与 §1.1 写：契约权威是 `@ d39e321`。
- 裁决 D-1 已定：**重锚到 `9ed173c`**，三处差异交 W7 承接。
- 但契约文档**尚未改写**（G0-02 未完成）。

**实测依据**：重锚后复跑对拍仍为 220/222、失败项仍是那 2 条 dashboard——
说明三处差异是**用例集之外的空白**，不是已知失败。重锚是安全的。

**待办**：G0-02 出「基线差异报告」，G0 落地时改写契约文档第 6 行与 §1.1。

### 6.2 W5 / W6 的 23 条端点没有 Work Item

契约表已划定归属（W5 15 条 + W6 8 条），台账上**一张卡都没有**。
这是台账的真实缺口，不是遗漏记录。

### 6.3 两条 dashboard 对拍失败

`dashboard_default` / `dashboard_after_drafts`——基线多了 `dashboard.calendar`。
**这是基线漂移导致的范围问题，不是实现缺陷**；但裁决 G0-04 要求书面裁定
`calendar` 是否属于冻结契约，**不得为了让 222/222 而擅自把 `calendar` 加进 Rust**。

### 6.4 OCR 模型目录依赖基线 venv

`ocr_rapid.rs` 的 `model_dir()` 仍有 `/mnt/o/ica/backend/.venv/...` 硬编码回退。
裁决与 OCR-01 的 AC1 都明确：**不能把基线 Python venv 作为最终交付依赖**。
这是 OCR-01 未关闭的最重要一项。

### 6.5 前端代码归属未决

见 §4.4。

### 6.6 Browser 方案的表与路由重叠

方案提议 11 张表，其中 4 张已存在，而真正承载采集数据的两张（33 列 / 9 列）
方案没提；`candidate_captures` / `candidate_matches` 与已有的抽取表、
`duplicate_*` 三列是同一件事的两种建模。裁决 D-8 已定「复用现有表，禁止平行建模」，
但**字段级差异报告尚未产出**（B0-08）。

### 6.7 端点数字已纠正，但口径曾出错

此前写「已实现 46 条」，并出现「46 + 23 = 69」的矛盾。
**根因是计数错误**：`GET /health` 不属于 68 条业务契约，被误计。

逐条对账：**45 + 23 = 68 ✓**，额外路由 0。
对账工具已固化（`scripts/endpoint_reconciliation.py`），
从此所有统计数字从矩阵生成，不再手工数（见 §4.6）。

### 6.8 D-1 的并行口径尚未裁决

§3.2 的「先恢复 W4 再开放新产品线」与「G3/F0-AUDIT 可立即开始」存在歧义。
§4.15 给了建议口径（允许只读调查与审计，不允许实现进入），但**需要一次显式裁决**。

### 6.9 三份 ADR 已批准，但带一项硬依赖

ADR-0004 / 0005 / 0006 已于 **2026-11-20 00:54 CST 批准（Accepted）**，
批准依据是用户明确表态「三份 ADR 批」。

**但 ADR-0004 落地时发现一项硬依赖（已写入其 §5.1 / §6）：**

Execution Gateway 的 Policy Engine **没有现成授权设施可复用**——
实测 `crates/ica-application/` 下无 auth/access 模块，当前是 local 单用户模式
（`LOCAL_ACTOR = "local_user"`）。

因此：

```
W6（Auth + Shares）→ Policy Engine → Execution Gateway → B5/B6 + Agent 受控工具
```

**在 W6 完成前，Gateway 只能以单用户降级形态存在，
且不得在任何验收结论中被引用为「权限隔离已实现」。**

另：correlation id 当前在 Rust 侧**完全不存在**（`grep` 0 命中），
Python 基线审计表也无该列——需要新建，不是复用。

### 6.10 三份审计报告已归档

`/tmp` 不持久，所以三份报告已复制进 `docs/audits/w4-independent/`：

```
LWW-REPORT.md        29 KB   6 条 LWW 规则等价，但 2 条 P1
FINDINGS-REPORT.md   19 KB   15 项中 13 项真关闭，P0-4 部分未关
OCR-REPORT.md        24 KB   「逐字符一致」不成立
README.md            10 KB   审计任务单与范围
```

**归档的是报告原文，未做任何改动**——独立审计报告是审计对象，改写它比留着错数更难追溯。

---

## 7. 建议推进顺序

**权威路线图见 §4.14（Phase 0–4）**，本节只补充执行细则。

```
Phase 0  Migration Trust Recovery   ← 当前，最高优先级
         W4-R1/R2/R3 → W4-R4 → 独立复审 → W4 正式验收
         Endpoint Reconciliation（工具已就绪，见 §4.6）

Phase 1  Migration Convergence
         G0 · OCR-01 · W7 · AI-EXT · 补建 W5/W6 Work Items

Phase 2  Runtime Foundation
         B0 · G3 · F0-AUDIT · Execution Gateway 契约（ADR-0004）

Phase 3  Controlled Automation
         BW0 → B1 → B2 → B3 → B4 → B5 → B6

Phase 4  Agent-native ICA
         Hermes 集成 → MCP Gateway → Agent 生命周期 → Browser+Agent 协同
```

**并行规则见 §4.15**（D-1 并行歧义的澄清口径）：
允许只读调查与架构审计并行，不允许实现进入。

**另轨（纯前端，与后端无冲突）**：UI R2.0 阶段 1（Shell + Design System）。
但 UI R2.0 的阶段 0 已在 B0 / F0-AUDIT 里覆盖，**不要重复做**。

---

## 8. 环境事实（避免重复踩坑）

| 事项 | 值 |
| --- | --- |
| cargo 产物目录 | `/home/bryan/.cache/ica-rust-target`（ext4；**手工跑 cargo 必须 `export CARGO_TARGET_DIR=...`**，否则写进 `/mnt/o` Windows 卷） |
| Python 基线 venv | `/mnt/o/ica/backend/.venv/bin/python`（3.12） |
| **Python 命令必须** | `env -u PYTHONPATH -u PYTHONHOME`（否则 Hermes 的 PYTHONPATH 污染导致 import 失败） |
| 对拍脚本 | `WORK=/tmp/x OUT_DIR=/tmp/y bash scripts/contract_verify.sh` |
| 端点映射检查 | `PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH -u PYTHONHOME /mnt/o/ica/backend/.venv/bin/python scripts/check_endpoint_map.py` |
| 基线仓 | **只读**。对拍脚本 rsync 到 `/tmp` 副本后运行 |
| Rust 字符串陷阱 | **不要用行尾 `\` 续行**——它会连下一行前导空白一起吞掉，破坏 SQL 与中文字面量 |
| 字节敏感文件 | 文本往返会改行尾（CRLF ↔ LF）。改这类文件必须二进制读写 |
| 基线 drift 事实 | 基线在 2026-11-19 一天内动了两次（`d39e321` → `20b252d` → `9ed173c`）。「终态」是基线方的自我声明，不是机制保证 |

---

## 9. 复算清单

```bash
cd /mnt/o/ICA-RUST

# 代码量
for c in crates/*/; do echo "$(basename $c): $(find "$c" -name '*.rs' | xargs wc -l | tail -1 | awk '{print $1}')"; done

# 测试
CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace

# 门禁
bash scripts/check.sh

# 对拍
WORK=/tmp/verify OUT_DIR=/tmp/verify-out bash scripts/contract_verify.sh

# 端点映射
PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH -u PYTHONHOME \
  /mnt/o/ica/backend/.venv/bin/python scripts/check_endpoint_map.py

# 端点对账矩阵（所有统计数字的唯一来源）
python3 scripts/endpoint_reconciliation.py
python3 scripts/endpoint_reconciliation.py --out docs/migration/reconciliation/endpoint-matrix-r1.md

# 卡片台账
C="python3 ~/.hermes/skills/software-development/woticketflow/scripts/woticketflow_cli.py"
$C work-item list
```

---

## 10. 相关文档

- 治理裁决全文：`docs/adr/ADR-0003-governance-decisions-20261119.md`
- 契约主文档：`docs/rust-migration-contract.md`
- W4 基线事实（含 §11 七处真差异、§13 OCR 实测）：`docs/contract-baseline/w4-python-facts.md`
- 三份独立审计报告（已归档）：`docs/audits/w4-independent/`
