# Python Baseline Diff Report R1（G0-02 交付物）

> G0 卡 §范围 G0-02：核对契约锚点 `d39e321` 与当前 Python HEAD 的差异，逐项归属。
> 依据 ADR-0006（基线四层概念）：**基线源码锚点的后移不静默改变冻结契约**——
> 每一份基线差异报告都必须得到显式的 Accept / Reject / Deferred / Out-of-Scope 裁决。

## 锚点历史（2026-11-19 当天的两次后移）

```
d39e321  （原契约锚点，W0–W4 交付所针对）
   ↓ 外部会话
20b252d  + 24 个未提交文件（01:58–07:50 时间戳）
   ↓ 外部会话 freeze
9ed173c  「freeze: Python 基线终态 —— 浏览器随包交付、供应商预设、双主题、
          工作台日历与后端修复」（2026-11-19 23:18:51，tag 于 23:17:32 之后 1 分钟冻结）
```

D-1（用户，2026-11-19）：**重锚到 `9ed173c`，三处行为差异另立波次承接。**
依据：freeze 提交自声明为「基线终态」。

## 差异全量（`git diff --stat d39e321..9ed173c`：152 文件，+32,563 / −717）

契约面（W4/BROWSER-04 相关）之外的大头是前端与桌面发布工程（React 前端、
双主题、Windows 安装器脚本、供应商预设），它们随包交付但**不属于 Rust 迁移
范围**（D3：只重写后端；前端沿用）。下表只列**契约可见**的差异，逐项归属：

## 契约可见差异逐项归属

| # | 差异 | 位置 | 归属 | 状态 |
| --- | --- | --- | --- | --- |
| 1 | 新端点 `POST /api/imports/{session_id}/confirm-candidate` | `backend/app/api/imports.py:185` | **W7**（D-1：独立承接） | ✅ 已实现（`1fefda6`），对拍 14 用例全绿；按 D-2 登记 auxiliary，不入 68 条 |
| 2 | `confirm_import` 拒绝 `browser_extract` 来源会话 | `conversation_service.py:262` | **W7** | ✅ 已实现（同上，契约用例 `confirm_message_guard_browser_session`） |
| 3 | `reject_import` 连带把抽取草稿置 `rejected` | `conversation_service.py:393` | **W7** | ✅ 已实现（契约用例 `reject_browser_session_marks_draft`） |
| 4 | Dashboard `calendar` 块（`DashboardCalendar`：本月逐日消息/跟进计数） | `api/dashboard.py:178`、`schemas/workflow.py:245` | **G0-04 / W2 对齐**（重锚后 `calendar` 属于冻结契约） | ✅ 已实现（本提交） |
| 5 | `browser_extract` 成为 `MessageSource` 合法值 | `domain/enums.py` | **W7**（preview/confirm 路径需要） | ✅ 已实现 |
| 6 | browser 表结构（12 张，含抽取 33 列 + 修订 9 列） | `models/browser.py` | **B0-08 对账 / BW0**（D-9：W7 沿用既有抽取模型，B4 不新建平行结构） | 📋 B0 卡范围 |
| 7 | 39 条 `browser*` 路由 | `api/browser.py` 等 | **BW0**（D-3/D-11：独立波次，B0-10 盘点后定范围） | 📋 BW0 卡（待 B0 盘点） |
| 8 | 供应商预设 / 双主题 / 安装器 / 前端 | 前端与发布工程 | **Out of scope**（D3：只重写后端；前端沿用 React） | 记录在案 |
| 9 | `ai/models` 4 条 CRUD | `api/ai.py` | **AI-EXT**（D-2：4 条不入 68 条契约，独立扩展契约） | 📋 AI-EXT 卡（ANALYZED） |

## 「calendar 是否属于契约」（G0-04）的裁决口径

G0 卡 v1 的方案原文提醒「不要为了让 Dashboard 220/222 变成 222/222 而擅自把
calendar 加进 Rust」。该提醒写于重锚**之前**——当时 calendar 是工作树里的未提交
增量。D-1 重锚后，`9ed173c` 的 dashboard 响应**本身包含** `calendar`，
契约权威（Python wins）随之覆盖它：实现 calendar 是**对齐冻结契约**，
不是为通过对拍而扩范围。本报告按此口径处理差异 #4，并在
`docs/migration/snapshot-r1.md` 记录锚点 sha——同名的「222/222」必须指向
明确的基线版本。

## 遗留提醒（G0 v3 原文，仍然有效）

基线在 2026-11-19 当天从 `d39e321` → `20b252d` → `9ed173c` 动了两次。
「终态」是基线方的自我声明，不是机制保证。后续任何波次的验收结论都必须
注明它对哪个 sha 测的——本报告与 Migration Snapshot R1 即为此而立。
