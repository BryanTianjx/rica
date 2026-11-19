# ICA-RUST 当前设计总方案

**状态：** 现状快照 + 待办总账
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
| 测试 | **335 passed / 0 failed / 0 ignored** | `cargo test --workspace` |
| 已实现端点 | **46** 条路由 | `grep -cE '^\s*"/(api\|health)' crates/ica-api/src/routes.rs` |
| 契约端点总数 | **68** 条（`R0 + W1…W6`） | `docs/rust-migration-contract.md` §2 |
| 跨语言对拍 | **220/222 cases matched**，2 条 dashboard 失败 | `bash scripts/contract_verify.sh` |
| 桌面 IPC 命令 | 46 个 | `grep -c '^pub async fn' crates/ica-desktop/src/commands.rs` |
| 完成波次 | R0 / W1 / W1.5 / W1.6 / W2 / W2.5 / W3 | `git tag --list` |
| 未完成端点 | 23 条（W5 15 + W6 8） | 契约文档 §1 |
| 卡片台账 | 23 张（3 完成 / 14 已批准未开工 / 1 READY / 1 待批 / 1 BLOCKED / 3 取消） | `woticketflow_cli.py work-item list` |
| 质量门 | 4 关全绿 | `bash scripts/check.sh` |

**当前处境一句话**：后端迁移的**主体已建成且可验证**（46/68 端点、335 测试、对拍 220/222），
但**W4 未通过独立验收**，且新增的 Browser / Agent / Frontend 三条产品线**尚未开工**——
14 张卡已批准但一张都没派发。

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

**W5 + W6 共 23 条端点尚未立项**——契约表已划定归属，但**没有对应的 Work Item**。
这是台账上的一个真实缺口。

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

## 5. 权威文档地图

| 文档 | 作用 |
| --- | --- |
| `docs/rust-migration-contract.md` | **契约主文档**：68 条端点表、波次划分、分层规则、declared improvements |
| `docs/adr/ADR-0002-rust-backend-migration-reinstated.md` | Rust 迁移决议（含 R1：wire 契约上移 `ica-contracts`） |
| `docs/adr/ADR-0003-governance-decisions-20261119.md` | **治理裁决 D-1…D-11**（2026-11-19） |
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

### 6.7 三份审计报告已归档

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

裁决原文的顺序 + 我的补充：

```
第一批  W4-R1 / R2 / R3（并行） → W4-R4 → 独立复审 → w4-done
        ↑ 阻断一切。W4 未可信之前，不开新产品线。

第二批  G0（基线治理，含契约文档锚点改写）
        OCR-01（模型目录脱钩）
        W7（浏览器抽取写路径）
        AI-EXT（契约文档先行，实施另立波次）
        ← 补建 W5 / W6 的 Work Item

第三批  B0（准入）→ 路由盘点 → BW0 立项
        ← B1 / B2 在 B0 验收后才可实施（D-5）

第四批  G3（Hermes POC，可与前两批并行）
        F0-AUDIT → 前端归属裁决 → F0

第五批  B1 → B2 → B3 → B4 → B5 → B6 → B7
        B7 只作为 V2.0 规划能力，不进近期实施（D-6）

        另轨  UI R2.0 阶段 1（Shell + Design System）
              ← 纯前端工作，与后端迁移无冲突，可与第一~四批并行；
                但阶段 0 已在 B0 / F0-AUDIT 里，不要重复做
```

**并行性**：G3 与 F0-AUDIT 不依赖 W4 返工，可立即开始。
但它们**尚未建 Kanban 卡**——按「等所有方案确定」的指令，14 张已批准的卡
一张都没派发。

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
