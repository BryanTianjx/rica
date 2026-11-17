# ICA-RUST + Tauri 2 架构差异审计（R0.1）

**审计对象**：`/mnt/o/ICA-RUST` @ **`w1.6-done`**（commit `df1c7b7`，已独立审计通过）
**审计方式**：`git archive` 导出干净快照到 `/tmp/audit-r01` 后逐文件读取；全程只读，未修改任何文件、未执行 git 写操作
**审计日期**：2026-11-18
**对照方案**：《ICA-RUST + Tauri 跨平台桌面客户端技术架构分析》

> **为什么基于 `w1.6-done` 而不是工作树**：审计期间 zcode 正在同一工作树做 W2
> （`follow_ups.rs` / `dashboard.rs` / `ports.rs` 等 20 个文件在途）。读工作树 = 审计半成品。
> 这是本项目已写入 skill 的审计纪律。

---

## 0. 最重要的发现：ICA 已经有 Tauri 宿主，而且正是方案批评的「方案 A」

**这一条改变整个审计的语境，必须先说。**

| 事实 | 证据 |
| --- | --- |
| ICA 主仓已存在 Tauri 2 宿主 | `/mnt/o/ica/desktop/tauri-host/`，`name = "ica-desktop-host"`，`version = "0.2.1"`，`Cargo.toml` 依赖 `tauri = { version = "2" }` |
| 它启动的是 **Python sidecar**，不是 Rust 服务 | `src/main.rs:1-18` 头注释：「spawn its bundled python sidecar (PORT=0) → parse the listening event → poll /health → serve the unit's ui/ and forward /api to the sidecar over the `ica` scheme」 |
| 它**没有**任何自定义 IPC 命令 | `grep "invoke_handler\|generate_handler\|tauri::command"` 在 656 行里**零命中** |
| 前端通过 **HTTP** 访问后端 | `main.rs:8`「forward /api to the sidecar over the `ica` scheme (same origin)」；`frontend/src/services/api.ts:18` `const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? '/api'` |
| 宿主跑的是 Python 解释器 | `main.rs:36-39` `SIDECAR_PY = "sidecar/runtime/python/bin/python3"` |

**即：现有实现 = 方案的「方案 A」（Tauri 启动后端、前端走 HTTP），而方案主张改成「方案 B」（Tauri IPC + 独立 Rust Core）。**

同时，**ADR-0001 已 Accepted**（2026-11-12，产品负责人批准，`docs/architecture/adr/ADR-0001-desktop-shell.md`），
正式技术栈明确定为「**React/Vite + FastAPI + SQLite + Tauri 2**」。
`desktop/tauri-host` 就是这份 ADR 的落地物。

**所以这不是「给一个空白项目选架构」，而是「推翻一个已批准 ADR 的宿主内后端实现」。**
这件事本身合法（用户有权改方向），但必须走 ADR 修订，不能当作技术选型建议悄悄落地。

### 0.1 第二重要的发现：契约只共享得了一半

方案的核心诉求是「IPC 与 HTTP 共享同一份契约，禁止双入口重复实现」。审计实测：

| 链路环节 | 位置 | 能否给 IPC 复用 |
| --- | --- | --- |
| 请求校验（1763 行） | `ica-api/body.rs`(952) + `query.rs`(811)，返回 `ApiError` | **不能** |
| 用例（18 个） | `ica-application`，纯 async，无框架类型 | **能**（这是方案最想要的部分，已具备） |
| 响应 DTO（9 个） | `ica-api/dto.rs`，**只有 `Serialize`** | **能**，但需搬出 `ica-api` 才不拖入 axum |

**即「业务逻辑可复用」成立，「契约整体可复用」不成立。** 详见 §4.2 / §4.3。

---

## 1. 五个 crate 的实际职责与依赖（与方案的映射）

### 1.1 实际结构（`/tmp/audit-r01`）

| crate | 职责（文件头注释原文要点） | src 文件 |
| --- | --- | --- |
| `ica-domain` | 「Pure domain model: entities, enums, pagination and search rules」 | `position.rs` `candidate.rs` `conversation.rs` `send_record.rs` `follow_up.rs`* `pagination.rs` `search.rs` `content_hash.rs` |
| `ica-application` | 「Use cases and repository ports」 | `positions.rs` `candidates.rs` `conversations.rs` `send_records.rs` `payload.rs` `ports.rs` `error.rs` |
| `ica-infra` | 「SQLite persistence adapters (sqlx, runtime queries)」 | `db.rs` `store.rs` `repository.rs` `candidates.rs` `conversations.rs` `send_records.rs` `audit.rs` `time.rs` |
| `ica-api` | 「HTTP surface (axum) for the ICA backend: routes, DTOs, error mapping」 | `routes.rs` `dto.rs` `query.rs` `body.rs` `error.rs` `state.rs` `http_parity.rs` |
| `ica-server` | 「ICA backend binary: configuration, tracing, startup self-check, graceful shutdown」 | `main.rs`（唯一文件） |

\* `follow_up.rs` 是 W2 在途新增，不在 `w1.6-done` 快照内。

### 1.2 依赖方向（实测，`crates/*/Cargo.toml`）

```
ica-domain        → chrono, sha2                        （无框架/驱动/运行时）
ica-application   → ica-domain, async-trait, chrono, serde_json
ica-infra         → ica-domain, ica-application, async-trait, sqlx, uuid, serde_json, chrono, tokio, tracing
ica-api           → ica-domain, ica-application, axum, serde, serde_json, chrono, tracing, form_urlencoded, serde_urlencoded
ica-server        → ica-infra, ica-api, axum, tokio, tracing, tracing-subscriber
```

**验证结论**：

- 分层纯度**成立**且是**编译期强制**：`ica-domain` 依赖只有 `chrono` + `sha2`；`ica-application` 无 axum/sqlx/tokio/hyper。
  `scripts/check.py` 用 `cargo metadata --no-deps` 机器断言这一点（第 1 关）。
- **`ica-api` 不依赖 `ica-infra`**（只在 `[dev-dependencies]`），`ica-server` 才是把两者接起来的组合根。
  这正是方案 §四要求的「`ica-api` 和 `ica-desktop` 都依赖 `ica-application`，不相互依赖」——**已经做到了**。
- **没有 circular dependency**。

### 1.3 与方案目标结构的差异

| 方案提的 crate | 现状 | 差异 |
| --- | --- | --- |
| `ica-domain` | 有 | 一致 |
| `ica-application` | 有 | 一致 |
| `ica-infrastructure` | 有，但命名为 **`ica-infra`** | 仅命名差异，不影响 |
| `ica-api` | 有 | 一致 |
| `ica-desktop`（新） | **无**（Rust 仓）；但主仓有 `desktop/tauri-host`（Python sidecar 版） | 需新建，且要与主仓那个宿主对齐或替换 |
| `ica-contracts`（可选） | 无 | 见 §4，**当前不需要** |

**结论：方案 §四「现有 5 个 crate 不需要因为引入 Tauri 而全部重构」——审计实测成立，且比方案预期的更干净（依赖方向已经正确）。**

---

## 2. Application Core 的可复用性（方案的核心假设，逐条验证）

方案的关键主张是「业务能力下沉到独立 Rust Application Core，Tauri 与 Axum 各做一个适配器」。
审计结论：**这个前提在本仓库已经具备，不需要重构。**

### 2.1 用例层已经是纯 Rust，与 HTTP 无关

18 个用例函数定义在 `ica-application`，签名**不含任何 axum 类型**：

```rust
// crates/ica-application/src/positions.rs:32
pub async fn list_positions(
    repository: &dyn PositionRepository,
    query: ListPositions,
) -> Result<Page<Position>, ApplicationError>

// crates/ica-application/src/candidates.rs:27 / :50 / :73 / :89 / :105 / :123
// crates/ica-application/src/conversations.rs:40 / :60 / :73 / :103 / :155
// crates/ica-application/src/send_records.rs:36 / :64
```

而 `ica-api` 的 handler 只是一层薄包装（`routes.rs:118-353`，18 个 `async fn`），
把 `State<AppState>` + `Json`/`ApiQuery` 解出参数、调用例、把结果塞进 DTO。

**这意味着：新增一个 `ica-desktop` crate，只需要重新实现「参数解析 + 结果序列化」这一薄层，
业务逻辑零重复。方案 §十一.1「禁止双入口重复实现」在技术上已经被结构保证。**

> ⚠️ **但这句话只对用例层成立，别读成整条链路都免费。** 请求侧的 1763 行校验
> （`ica-api/body.rs` 952 行 + `query.rs` 811 行）也住在 `ica-api` 且返回 axum 耦合的 `ApiError`，
> 而 DTO 只有 `Serialize` 没有 `Deserialize`。**Tauri 适配器白拿不到那一层**——详见 §4.2。

### 2.2 端口 trait 不含 sqlx 类型

`crates/ica-application/src/ports.rs` 定义 **5 个 Repository trait / 23 个方法**（22 必填 + 1 默认实现）：

| trait | 方法数 | 备注 |
| --- | --- | --- |
| `PositionRepository` | 6（含 1 个默认实现 `page`） | `page()` 默认在内存切片，SQLite adapter 覆盖为 `COUNT(*)+LIMIT/OFFSET` |
| `CandidateRepository` | 6 | 含 `position_of()` 用于 detail 嵌套 |
| `ConversationRepository` | 5 | 含 `create_implicit()`（隐式建会话不写 audit） |
| `MessageRepository` | 2 | |
| `SendRecordRepository` | 5 | 含两个窄读用于跨表校验 |

（W2 在途会再加 `FollowUpRepository` / `DashboardRepository`，但那不在 `w1.6-done` 快照内，不计入本审计。）

全部是 `#[async_trait]`，返回 `Result<T, RepositoryError>`，**不出现任何 `sqlx::` 类型**。
`RepositoryError` 只持有一个 `message: String`（`ports.rs:25-48`），注释明说「logged and surfaced as a generic 500, never echoed」。

**Tauri 侧可以在完全不引入 sqlx 的前提下实现这些 trait**（例如做测试替身，或未来换存储）。
但注意：**`ica-desktop` 若要真连 SQLite，仍需依赖 `ica-infra`**——trait 只是抽象，实现还是那一个。

### 2.3 错误类型刻意不含 HTTP 概念

`crates/ica-application/src/error.rs:1-6` 的头注释是决定性的：

> 「Kept free of HTTP concepts: the mapping to status codes and to the JSON
> error envelope belongs to `ica-api`.」

```rust
pub enum ApplicationError {
    NotFound { message: String },                                  // 注释标 HTTP 404
    Validation { message: String, details: BTreeMap<String,String> }, // 422
    Conflict   { message: String, details: BTreeMap<String,String> }, // 409
    Unavailable{ message: String },                                // 500
}
```

**变体与 HTTP 状态码是一一对应的语义，但类型本身不 import 任何 HTTP crate。**
`ica-api/src/error.rs:24` 才 `use axum::http::StatusCode` 做映射。

**对 Tauri 的意义**：IPC 适配器可以直接把 `ApplicationError` 映射成 `{code, message, details}`，
**不需要伪造 HTTP 状态码**（IPC 没有状态码概念）。这是现有设计最有利于 Tauri 的一点。

### 2.4 `AppState` 是纯 trait object

`crates/ica-api/src/state.rs:31-40`：

```rust
pub struct AppState {
    positions: Arc<dyn PositionRepository>,
    candidates: Arc<dyn CandidateRepository>,
    conversations: Arc<dyn ConversationRepository>,
    messages: Arc<dyn MessageRepository>,
    send_records: Arc<dyn SendRecordRepository>,
    config: AppConfig,
}
```

注释明说「Each port is a trait object so the HTTP layer never sees `sqlx`」。
`ica-server/src/main.rs:76-92` 把**同一个** `SqliteStore` clone 成 5 个 trait view 注入。

**但 `AppState` 住在 `ica-api` crate 里**——这是唯一的结构性问题：
`ica-desktop` 若依赖 `ica-api`（为了复用 `AppState`），就同时引入了 axum（`ica-api` 的硬依赖）。
见 §4 的处理方案。

---

## 3. 数据库层与运行时生命周期（桌面化的实际缺口）

### 3.1 已具备的（可直接复用）

| 能力 | 事实 | 证据 |
| --- | --- | --- |
| 连接池 | `max_connections(5)` | `ica-infra/src/db.rs:158` |
| busy timeout | 5 秒 | `db.rs:143` |
| `foreign_keys=ON` | 显式开启 | `db.rs:142` |
| WAL | 写连接启用；只读连接**不**碰（避免改动基线文件） | `db.rs:145-151` |
| 只读/immutable 模式 | 支持 `?mode=ro` / `?immutable=true` | `db.rs:22-24`, `:153-155` |
| 显式事务 | `pool().begin()` → `tx.commit()`，业务行与 audit 同事务 | `repository.rs:234,271,287,374,383,432` |
| 启动自检 | `table_names()` + `REQUIRED_TABLES`，缺表**启动即失败** | `ica-server/main.rs:19,98-116` |
| 优雅关闭 | Ctrl-C + SIGTERM，`with_graceful_shutdown` | `main.rs:119-144` |

`REQUIRED_TABLES` 只有 2 项（`candidates`、`positions`）——**容错设计**，多出的表不会导致启动失败。
这一点对桌面端升级很友好（新版本加表不影响旧检查）。

### 3.2 缺失的（桌面客户端的实质缺口）

| # | 缺口 | 现状证据 | 影响 |
| --- | --- | --- | --- |
| **G-1** | **无 schema 迁移机制** | `grep "migrate\|user_version\|schema_version" crates/` → 零命中（`db.rs:166` 只是注释里出现 "migrated" 一词） | 桌面端升级时 schema 如何演进**完全没有方案**。当前 `ica-baseline.sql` 是**只读快照**（`crates/ica-infra/schema/ica-baseline.sql:1-9`，注释写「Generated by scripts/export_baseline_schema.py … Do not hand-edit」），**运行时从不执行它** |
| **G-2** | **无数据加密** | `grep "encrypt\|aes\|cipher\|AES" crates/` → 零命中 | 方案 §三.4 明确要求「候选人个人信息，数据库加密…不能仅依赖操作系统文件权限」。当前是**明文 SQLite 文件**。桌面客户端把库放在用户目录下，这是实质风险 |
| **G-3** | **无备份/恢复/导出** | `grep "export\|import\|backup\|restore" crates/ica-api/src/routes.rs` → 零命中 | 属 W4/W5 范围（契约里 `POST /api/export`、`/api/import`、`/api/backups`），**当前未实现**。桌面端「卸载不丢数据」「升级前备份」依赖这个 |
| **G-4** | **配置全部来自环境变量** | `main.rs:46-64`：`DATABASE_URL`、`ICA_APP_NAME`、`AI_API_KEY`、`HOST`、`PORT` | 桌面应用没有「设置环境变量」的入口。Tauri 宿主必须自己决定数据目录（`%LOCALAPPDATA%` / `~/Library/Application Support`）并注入 |
| **G-5** | **`target-dir` 硬编码 WSL 路径且已提交进 git** | `.cargo/config.toml`：`target-dir = "/home/bryan/.cache/ica-rust-target"`；`git ls-files .cargo/` → 已跟踪 | **跨平台阻断项**。Windows 会把它解析成当前盘符下的 `\home\bryan\...`；macOS 上 `/home` 通常不可写。方案 §六.1 已预警这条，审计确认它是**真实存在且会直接失败**的问题 |

### 3.3 关于 G-1 的额外说明（这条最容易低估）

方案 §三.4 的表格里写「Migration → SQLx Migration」。审计要指出：
**当前仓库连 sqlx 的 migrate feature 都没启用**（`Cargo.toml`：`sqlx = { features = ["runtime-tokio", "sqlite", "derive"] }`，无 `migrate`）。

而 `ica-baseline.sql` 是「Python 基线库的 DDL 快照」，用于**证明两边 schema 一致**（`scripts/compare_baseline_schema.py` 逐对象比对），
**不是**迁移脚本，也不在运行时执行。

所以「桌面端 schema 演进」是一个**从零开始的子项目**，不是「配一下」。
可选路线（需单独立项，不在本次审计范围）：
- 引入 `sqlx::migrate!` + 手写 forward-only 迁移（与 Python 基线的 alembic 版本对齐）；
- 或沿用 Python 的 alembic 作为唯一迁移权威，Rust 只做 schema 校验（与当前 `compare_baseline_schema.py` 思路一致）。

---

## 4. DTO 与跨适配器契约

### 4.1 现状：DTO 在 `ica-api`，但形状是纯 serde

`crates/ica-api/src/dto.rs` 的全部 DTO 用**纯 serde 特性**描述形状：

- 时间戳：`#[serde(serialize_with = "serialize_utc")]` / `serialize_utc_opt`（`dto.rs:66,113,166,203,234,268…`）
- 嵌套：`#[serde(flatten)]`（`dto.rs:153`）
- 分页信封：`PageResponse<T> { items, total, offset, limit, has_more }`（`dto.rs:297-303`）
- 错误信封：`{"error": {"code", "message", "details"}}`（`ica-api/src/error.rs:1-19`）

**没有任何 DTO 带 HTTP 特有字段**（无 status code、无 header）。`has_more` 是业务计算值（`dto.rs:305-317`），不是 HTTP 语义。

**结论：这些 DTO 可以直接用于 Tauri IPC 序列化，零改动。**

### 4.2 更严重的缺口：**请求侧契约整个住在 `ica-api`，且返回 axum 耦合的 `ApiError`**

这是本次审计最重要的技术发现，方案的「禁止双入口重复实现」在这一层**不成立**。

**实测事实**：

| 项 | 数值 | 证据 |
| --- | --- | --- |
| 请求体校验 | `crates/ica-api/src/body.rs` **952 行**，7 个 `Raw*` 结构 + 8 个 `pub fn`（`parse` + 7 个 `*_create`/`*_patch`） | 全部返回 `Result<_, ApiError>` |
| 查询串校验 | `crates/ica-api/src/query.rs` **811 行**，6 个 `Raw*Query` + 7 个 `validate*` | 全部返回 `Result<_, ApiError>` |
| 合计 | **1763 行请求侧契约** | 全在 `ica-api` |
| DTO 方向 | `grep -c Deserialize crates/ica-api/src/dto.rs` → **0** | 只有 `Serialize`，**响应专用** |
| 域模型 | `grep derive crates/ica-domain/src/*.rs` → 只有 `Debug/Clone/Copy/PartialEq/Eq/Hash` | **零 serde**，序列化必须经 `dto.rs` 的 `From` |

**这意味着什么**：

- 用例函数的输入结构被文档化为「已校验」——`payload.rs:3-8` 原文：「every invariant the Python pydantic schemas enforce … has already been checked by `ica-api/body.rs` by the time one of these is built」；`positions.rs:9-13` 同样声明校验在 HTTP 层。
- 这些结构**没有校验构造器**，字段全 `pub`。绕过 `body.rs` 直接构造一个越界 `NewPosition` 在类型上是合法的。
- 所以 **Tauri 侧如果自己写 command 参数，就必须重写这 1763 行的长度/枚举/三态 null/边界规则** —— 正是方案 §十一.1 要避免的「双入口重复实现」，也是本仓库 `IMP-1..IMP-4` 与 pydantic 逐字对齐（`bool_parsing`/`int_parsing`/`greater_than` 等文案）的那部分资产。

**结论修正**：§2.1 说的「业务逻辑零重复」**只对用例层成立**。完整链路是：

```
请求字节 → [校验 1763 行 · ica-api · 返回 ApiError] → [用例 18 个 · ica-application · 纯] → [DTO 9 个 · ica-api · Serialize-only]
             ↑ 不可复用                                    ↑ 可复用                        ↑ 可复用但需搬 crate
```

**所以 Tauri 适配器能白拿的只有中间一段。** 两端都要动。

### 4.3 契约层的三种处理方案（含请求侧）

结合 §4.2 的新发现，方案必须同时处理「响应 DTO」与「请求校验」两半：

| 选项 | 做法 | 代价 | 是否满足「禁止双入口重复实现」 |
| --- | --- | --- | --- |
| **A. 复用 `ica-api`** | `ica-desktop` 依赖 `ica-api` | 连带引入 axum/hyper/tower；且 `ApiError` 走 `IntoResponse`，IPC 侧仍要另写错误映射 | **否**——请求侧 command 参数还得自己校验 |
| **B. 只抽 `ica-contracts`（搬 `dto.rs`）** | 响应 DTO 独立 | 一次性重构 ~380 行；`ica-api` 改 `pub use` 不破坏调用方 | **否**——只解决响应半边 |
| **C. 抽 `ica-contracts` + 搬校验 + 拆错误类型** | 搬 `dto.rs` + `Raw*`(13 个) + 15 个校验函数；把 `ApiError` 拆成「传输中立 `ContractError`」+ `impl IntoResponse`（现有 `ica-api/src/error.rs:69-136` 的 6 个变体已是纯数据，`IntoResponse` 在 `:155` 单独 impl，拆分成本低） | 3 处搬动 + 1 次错误类型拆分，约 2000 行移动 | **是**——这是唯一真正达标的形态，`ica-api` 退化成 routes + `From<ContractError>` |

**审计建议：若确定走 IPC，只有 C 才真正满足方案自己的约束。** 且必须排在 W2 之后
（当前工作树有 25 个在途改动，此时搬 `ica-api` 会直接冲突）。

**若维持现有 sidecar 方案（ADR-0001），三项都不需要做** —— HTTP 契约本来就完整。

### 4.4 前端侧的改造量（实测，比方案预估的小）

| 事实 | 证据 |
| --- | --- |
| 前端有**统一薄封装** | `frontend/src/services/api.ts:150-155`：`export const api = { get, post, put, patch, delete }`，全部转发 `request(path, options)` |
| BASE_URL 单点可配 | `api.ts:18` `import.meta.env.VITE_API_BASE_URL ?? '/api'` |
| 调用点 | `grep` 到 **60 处** `api.get/post/...` 调用 |
| 401 刷新逻辑集中 | `api.ts:96-113` 单处处理 |

**即：60 个调用点全部经过 `api.*` 这 5 个方法。** 若改 IPC，理论上只需重写 `api.ts` 里 `request()` 一个函数
（把 `fetch(BASE_URL+path)` 换成 `invoke(<command>, {params})`），**业务组件零改动**。

这与方案 §五.2「前端业务组件不应该直接调用 invoke()」的诉求**天然吻合**——
审计发现现状已经满足，因为 `api.*` 就是那个 `BackendClient` 抽象，只是当前实现是 HTTP。

**但方案 §四/§五 提出的 `src/services/backend/{tauri-client,http-client}.ts` 双实现，审计认为当前不需要**
（当前只有一种传输）。等真出现 Web 管理端再说，避免过度设计。

---

## 5. 与已批准决策的冲突（必须先解决，否则不用谈技术）

### 5.1 三份文档的现状

| 文档 | 状态 | 关键内容 |
| --- | --- | --- |
| `ICA/docs/architecture/adr/ADR-0001-desktop-shell.md` | **Accepted（2026-11-12，用户批准）** | 「正式技术栈定为 React/Vite + **FastAPI** + SQLite + Tauri 2」；Tauri 宿主启动 Python sidecar |
| `ICA-RUST/docs/adr/ADR-0002-rust-backend-migration-reinstated.md` | **Accepted（2026-11-17）** | Rust 替换后端 HTTP/data plane；**「Out of scope: the Tauri 2 desktop shell」** |
| `ICA/docs/architecture/local-admin-cloud-management-architecture-r1.md`（ADM-001） | Proposed | §1.7/§3.2/§20.3 三处禁止 Python→Rust 迁移（已被 ADR-0002 在本仓覆盖） |

### 5.2 方案触发了 ADR-0002 §4 的复审条件

`ADR-0002` §4 列了 5 条「必须重开此 ADR」的条件，其中**第 4 条**：

> 「**The frontend needs changes to talk to the Rust backend.** D3 says the
> contract is compatible; needing frontend edits invalidates that assumption
> and must be re-approved.」

用户方案要求前端从 HTTP 改成 Tauri IPC —— **这正是第 4 条描述的情形**。
而且方案同时改动了 ADR-0002 明确声明「Out of scope」的东西（desktop shell）。

**审计结论：方案在技术上是可行的（见 §6），但它是一个 ADR 级决策变更，不是实现细节。**
按本项目管理规则（`woticketflow`：立卡→审批→执行），它需要：
① 修订 ADR-0001（宿主内后端从 Python sidecar 改为 Rust Core）；
② 修订 ADR-0002（把 desktop shell 从 out-of-scope 移入 in-scope）；
③ 明确 `desktop/tauri-host` 现有 656 行资产怎么处理（替换/并存/废弃）。

**这三件事都没做之前，不应该动任何代码。**

---

## 6. 技术可行性判定（逐条对应方案的技术断言）

| 方案断言 | 审计判定 | 依据 |
| --- | --- | --- |
| 「不建议简单地将现有 Axum HTTP 服务作为 Tauri 的唯一业务入口」 | **成立**，且现状比方案以为的好——业务不在 axum 里 | 18 个用例在 `ica-application`，axum 只是薄适配器 |
| 「业务能力下沉到独立 Rust Application Core，通过不同适配器支持 IPC 和 HTTP」 | **已经就是这个结构** | `ica-domain`/`ica-application` 零框架依赖；`ica-api` 与未来 `ica-desktop` 平级 |
| 「Tokio 统一异步运行时，不要每个 Command 重复创建 Runtime」 | **成立且已满足** | `ica-server` 用 `#[tokio::main]`；Tauri 2 自带 tokio（crates.io 元数据确认 `tokio ^1` 是 `tauri` 的非可选依赖），共用同一 runtime 无冲突 |
| 「不要同时用 SQLx 和 Tauri SQL Plugin」 | **强烈同意**，且有额外理由 | Tauri SQL Plugin 的 SQLite 不支持加密（社区实测，3 年未合并），且无事务 API；ICA 需要事务（业务行+audit 同事务），插件做不到 |
| 「SQLx + SQLite 继续作为唯一数据访问体系」 | **成立** | 已是现状 |
| 「Serde 建立统一 IPC/HTTP 契约，DTO 与领域对象分离」 | **部分成立**——响应侧可复用，**请求侧不可**（DTO 只有 `Serialize`；1763 行校验在 `ica-api` 且返回 `ApiError`） | 见 §4.2、§4.3 选项 C |
| 「`ica-api` 和 `ica-desktop` 都依赖 `ica-application`，不相互依赖」 | **已满足**；`ica-desktop` 应回避依赖 `ica-api`（否则拖入 axum） | `ica-api` 的 `ica-infra` 只在 dev-deps；见 §4.3 |
| 「Rust 2021 完全兼容」 | **成立** | 项目 `edition = "2021"`，`rust-version = "1.98"`；Tauri 2.12 的 MSRV 是 **1.90**、edition **2024**——项目 rustc 1.98 满足，edition 各不相同不影响（同 workspace 可混 edition） |
| 「`target-dir = /home/bryan/.cache/...` 不适合所有平台」 | **成立，且是硬阻断** | 见 §3.2 G-5，且该文件已提交进 git |
| 「WSL 编译成功 ≠ 双平台通过」 | **成立** | 当前无 Windows/macOS 构建记录；`desktop/tauri-host` 在 Windows 有 spike 记录，Rust 侧没有 |

---

## 7. 缺口清单（按「是否阻断 Tauri 集成」排序）

| # | 缺口 | 阻断级别 | 说明 |
| --- | --- | --- | --- |
| 1 | **ADR-0001 / ADR-0002 未修订** | **P0 阻断** | 决策未落，编码即违规 |
| 2 | **`.cargo/config.toml` 硬编码 `/home/bryan/...`** | **P0 阻断** | Windows/macOS 构建直接失败；需改为按平台/环境变量 |
| 3 | **无 schema 迁移机制**（G-1） | **P0**（桌面端升级必需） | 从零开始，非「配置一下」 |
| 4 | 数据目录未定义（G-4） | P1 | 桌面端必须用平台标准目录，不能靠 cwd |
| 5 | **契约层分裂**：响应 DTO 在 `ica-api/dto.rs`（`Serialize` only），请求校验 1763 行在 `ica-api/{body,query}.rs` 且返回 `ApiError` | **P0**（IPC 路线） | 方案要求「IPC 与 HTTP 共享契约」；现状只能共享响应半边。走 IPC 必须做 §4.3 选项 C |
| 6 | 无备份/导出（G-3） | P1 | 属 W4/W5，但桌面发布前必须有 |
| 7 | 无数据加密（G-2） | P1（安全） | 方案自己提了这条；当前明文 |
| 8 | `desktop/tauri-host` 656 行资产的归属 | P1 | 替换/并存/废弃必须明确，否则两套宿主并存 |
| 9 | 后端只完成 18/68 端点 | P2（进度） | W3–W6 未做；桌面端要用全功能必须等，或按波次逐步切 |
| 10 | 无 Windows/macOS 构建与签名链路 | P2 | 属 RUST-DESKTOP-03 |

---

## 8. 最小改造方案（结论性建议，不含代码）

### 8.1 如果采纳方案（Tauri IPC + Rust Core）

分层本身是对的，所以**不需要重写业务**；但「请求侧契约」要搬家，这一步方案低估了。真正要做的是 5 件，按顺序：

1. **决策层**（P0，先做）：
   - 修订 ADR-0001：宿主内后端改为 Rust Core；
   - 修订 ADR-0002：desktop shell 从 out-of-scope 移入 in-scope；
   - 明确 `desktop/tauri-host` 的处置（建议：保留其打包/版本单元/`ica://` 转发机制，把 sidecar 从 Python 换成 Rust binary——656 行资产大部分可复用，规避「一次改太多」）。

2. **抽 `ica-contracts`**（§4.3 选项 C，**这是 IPC 路线的真实成本**）：
   - 搬 `dto.rs`（响应 9 个）+ `Raw*`（请求 13 个）+ 15 个校验函数（**共约 2000 行**）；
   - 把 `ApiError` 拆成传输中立的 `ContractError` + 在 `ica-api` 里 `impl IntoResponse`；
   - `ica-api` 退化为 routes + `From<ContractError>`，契约从此单一真源。

3. **新增 `crates/ica-desktop`**（依赖 `ica-application` + `ica-infra` + `ica-contracts`，**不依赖 `ica-api`**）：
   - Tauri 命令层：每个端点一个 `#[tauri::command]`，调 `ica_contracts` 的校验 → 调 `ica_application::*` → 映射 `ContractError`；
   - 复用现有 5 个 port trait 注入（照搬 `ica-server/main.rs:76-92` 的组装逻辑）。

4. **数据目录与配置**：桌面端不能用环境变量 + cwd（§3.2 G-4），必须用平台标准目录（`%LOCALAPPDATA%` / `~/Library/Application Support`），或复用 `desktop/tauri-host` 已有的 `%LOCALAPPDATA%\ICA\data` 约定。

5. **前端零改业务代码**：只重写 `frontend/src/services/api.ts` 的 `request()`（60 个调用点全部经过 `api.*`）。

**注意顺序**：第 2 步会大范围移动 `ica-api` 文件，必须等 W2 落地后做（当前工作树有 25 个在途改动）。

### 8.2 如果维持现有 ADR-0001（Python sidecar）

则 **Rust 项目完全不需要 Tauri 相关改造**。Rust 后端作为「可替换的后端实现」存在，
将来 W7 切换时把 sidecar 的 Python 换成 Rust binary 即可（`ica://` 转发机制不变，前端零改动）。

**这条路的成本远低于方案，且不需要推翻任何已批准 ADR。**

### 8.3 审计的独立判断

两条路都成立，但**关键区别是「前端走 IPC 还是走 HTTP」**：

| | IPC（方案） | HTTP + 同源转发（现状） |
| --- | --- | --- |
| 安全面 | 无本地端口，无 CSRF | 需管端口、绑定回环、令牌 |
| 前端改动 | 重写 `request()` 一处（60 个调用点零改） | 零 |
| **后端改动** | **搬 ~2000 行契约（含 1763 行校验）+ 拆 `ApiError`** | **零** |
| 与已有资产 | `desktop/tauri-host` 的转发机制要废弃一部分 | 完全复用 |
| 契约测试 | 需重做（IPC 无状态码） | 现有 113 条对拍原样有效 |
| ADR 成本 | 改 2 份 ADR | 零 |

**审计倾向于：IPC 在安全面上确实更优（方案 §三.2 的论证成立），但代价比方估计的大——不是「只重写前端 request()」，
而是要在后端搬 2000 行契约并拆错误类型；同时当前 113 条契约对拍、`ica://` 转发、以及「Rust 逐端点替换 Python」
的整个方法论都是围绕 HTTP 建立的，改 IPC 会让这套已验证的资产贬值。**

**建议：先做 §8.1 的第 1 步（ADR 修订决策），再决定技术路线；不要在决策未落时动代码。**

---

## 9. 审计未覆盖的事项与两个易踩的坑

**未覆盖（诚实声明）**：

- **未验证** Tauri 2 在本项目的实际构建（未安装 Tauri CLI，未在 Windows/macOS 上构建）；§6 关于 MSRV/edition 的结论来自 crates.io 元数据与官方发布说明，非本地实测。
- **未验证** IPC 模式下错误契约的具体形状（未写原型）；`ContractError` 拆分的工作量是估算。
- **未评估** `desktop/tauri-host` 656 行的完整细节（只读了头注释、sidecar 启动、`ica://` 转发、mime 处理，未逐行审）。
- **未审计** W2 在途代码（`follow_ups.rs` / `dashboard.rs` 未提交，按纪律不审工作树）。
- **未评估** macOS WKWebView 与 Windows WebView2 的渲染差异（需真机）。
- 前端改造量基于 `grep` 统计（60 处 `api.*` 调用），未逐个确认是否有绕过 `api.ts` 的直接 `fetch`。

**两个实现时会踩的坑（审计实测）**：

1. **`ica-application` 的根导出不完整** —— `lib.rs:15-27` 只重导出了
   `get_position` / `list_positions` / `ListPositions` / `ListCandidates` / `CandidateDetail` /
   `ListConversations` / `ListMessages` / `ListSendRecords`。
   **`create_position` / `update_position` / `delete_position` 与 candidates 的 6 个用例、send_records 的
   `record_send` 都不在根导出里**（虽然模块是 `pub`，`routes.rs:136` 就走全路径）。
   按根导出清单写 `ica-desktop` 会误以为「用例不全」。→ 要么补全根导出，要么统一走全路径。

2. **`count` 类数字必须自己数** —— 本次审计中，子代理报告的「22 个用例」与「7 个 trait」都与实测不符
   （实测：**18 个用例**、**5 个 trait**）。原因是读到了含 W2 在途代码的工作树而非 `w1.6-done` 快照。
   **这就是「审计已提交里程碑、不审工作树」这条纪律的实际价值** —— 连审计者自己都会串。

---

## 10. 复跑命令

```bash
# 1) 导出审计快照（与本次审计同一基线）
rm -rf /tmp/audit-r01 && mkdir -p /tmp/audit-r01
git -C /mnt/o/ICA-RUST archive w1.6-done | tar -x -C /tmp/audit-r01

# 2) 分层纯度与依赖方向
grep -E "^\[dependencies\]" -A20 /tmp/audit-r01/crates/ica-application/Cargo.toml
grep -rn "sqlx\|axum\|tokio" /tmp/audit-r01/crates/ica-domain/src/ /tmp/audit-r01/crates/ica-application/src/

# 3) 用例层是否 axum 无关（应只在 ica-application 出现 pub async fn）
grep -rn "^pub async fn" /tmp/audit-r01/crates/ica-application/src/

# 4) 主要缺口
grep -rn "encrypt\|aes\|cipher" /tmp/audit-r01/crates/            # 空 = 无加密
grep -rn "migrate\|user_version\|schema_version" /tmp/audit-r01/crates/  # 空 = 无迁移
grep -rn "export\|backup" /tmp/audit-r01/crates/ica-api/src/routes.rs    # 空 = 未实现

# 5) 跨平台阻断项
cat /tmp/audit-r01/.cargo/config.toml
git -C /mnt/o/ICA-RUST ls-files .cargo/                          # 已跟踪 = 会带到所有平台

# 6) 已批准决策
sed -n '1,80p' /mnt/o/ica/docs/architecture/adr/ADR-0001-desktop-shell.md
sed -n '1,120p' /mnt/o/ICA-RUST/docs/adr/ADR-0002-rust-backend-migration-reinstated.md

# 7) 已有 Tauri 宿主（Python sidecar 版）
sed -n '1,60p' /mnt/o/ica/desktop/tauri-host/src/main.rs
cat /mnt/o/ica/desktop/tauri-host/Cargo.toml
grep -n "invoke_handler\|tauri::command" /mnt/o/ica/desktop/tauri-host/src/main.rs   # 空 = 无 IPC 命令
```
