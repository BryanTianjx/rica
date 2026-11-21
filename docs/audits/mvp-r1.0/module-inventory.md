# 模块与数据库层盘点(module-inventory)

**审计编号:** ICA-ENG-AUDIT-001 / WP-00
**证据等级:** E1(当前工作区源码,HEAD `ff7b9eb` + 未提交改动)
**核验日期:** 2026-11-21
**范围:** crates 全部 7 个 crate、数据库 schema、迁移机制、事务、备份导出、未提交代码归属

---

## 1. Crate 结构(7 个,workspace 根 `Cargo.toml`)

| Crate | 层级职责 | 关键证据 |
|---|---|---|
| ica-domain | L1 纯领域模型:实体、枚举、分页/搜索、candidate 提取、redact、content_hash | 仅依赖 `chrono, sha2, regex, unicode-normalization`;Cargo.toml 注释:"No web framework, no database driver, no async runtime: the layering rule is enforced by this dependency list" |
| ica-application | L2 用例 + 仓储端口(`ports.rs` 定义全部 port trait) | 依赖 `ica-domain, async-trait, chrono, serde_json`,无 axum/sqlx/tokio |
| ica-contracts | L2.5 传输无关 wire contract:请求校验(body/query)、响应 DTO、错误形状 | 依赖 `ica-application, ica-domain, serde` 等;边界由 `scripts/check.py` 强制 |
| ica-infra | SQLite 持久化适配器(sqlx)+ RapidOCR + AES-256-GCM 导出加密 | 依赖 `ica-domain, ica-application, sqlx, rapidocr-core, aes-gcm` |
| ica-api | HTTP 表面(axum):routes/state/error binding,纯协议转换 | sqlx 仅在 dev-dependencies;自身无 SQL |
| ica-desktop | Tauri IPC command 层(无 tauri 依赖,command 为普通 async fn)+ 浏览器 POC | 未提交追加 `sha2, tokio-tungstenite, futures-util, sqlx` 及 `[[bin]] ica-browser-worker` |
| ica-server | 组合根二进制:配置、tracing、启动自检、优雅停机 | 仅 `ica-infra, ica-api, axum, tokio, tracing`;不直接依赖 domain/application |

**架构边界结论:** Domain 不依赖 HTTP 框架 ✅、Application 无运行时依赖 ✅、API 只做协议转换 ✅、
边界有脚本强制(`scripts/check.py`)+ workspace 依赖注释 ✅。对应架构验收要求(§4.3)前四条成立。

## 2. 模块树与行数

全仓库 Rust 总行数 **28,233**(src 21,591 + tests 6,642,排除 target/)。历史快照(24,797)已过时。

| Crate | src 行数 | tests 行数 | 最大模块 |
|---|---|---|---|
| ica-domain | 2,681 | 331 | follow_up 821 |
| ica-application | 4,033 | 220 | ports 924 |
| ica-contracts | 4,142 | 0 | body 2,004 |
| ica-infra | 6,825 | 596 | follow_ups 1,101 |
| ica-api | 1,577 | 4,827 | routes 959 |
| ica-desktop | 2,171 | 668 | commands 928 |
| ica-server | 162 | 0 | main 162(唯一文件) |

## 3. 数据库 Schema 与迁移机制

- **Schema 单一来源:** `crates/ica-infra/schema/ica-baseline.sql`(986 行),由
  `scripts/export_baseline_schema.py` 从 Python 基线库只读导出,头注释 "Do not hand-edit: regenerate"。
- **实测规模:** 32 张表、121 个索引、34 条 FOREIGN KEY 约束。
  ⚠️ 文件头注释写 "20 tables, 66 indexes" — **已过时**,建议重新生成或修正注释(文档缺陷,非代码缺陷)。
- **迁移机制:不存在。** 全仓库无 refinery / `sqlx::migrate!` / migrations 目录。
  - 测试: `sqlx::raw_sql(include_str!(ica-baseline.sql))` 整库应用(`ica-infra/tests/support/mod.rs:13` 等)。
  - 生产: `ica-server/src/main.rs` 仅做启动自检(`REQUIRED_TABLES = ["candidates", "positions"]`),缺表拒绝启动,不建表不迁移。
- **schema 版本号仓库内不可得:** `alembic_version` 表存在但快照无 INSERT 行,Rust 不读写。
- **表清单(32 张):**
  - 业务核心:`positions, candidates, conversations, messages, follow_up_tasks, follow_up_policies, generated_drafts, send_records`
  - 导入:`import_sessions, import_records`
  - 审计/系统:`audit_events, app_settings, ai_model_configs, ai_model_routing, ai_usage_daily, tenants, users, refresh_tokens, shares, alembic_version`
  - 浏览器(12 张):`browser_actions, browser_approvals, browser_candidate_extraction_revisions, browser_candidate_extractions, browser_connector_sessions, browser_connectors, browser_permissions, browser_profiles, browser_sessions, browser_tabs, browser_task_steps, browser_tasks`
  - **不存在:`projects`、`applications`(任何 project/application 表均无)**

## 4. 事务与外键

- **事务:** 33 个 `pool().begin()` 调用点,全部在 `ica-infra/src`,统一模式为
  "业务写 + audit 行共享同一事务"(`store.rs` 头注释:"a business write and the audit event it produces share a transaction";`audit.rs:12`:"an audit event can never describe a change that rolled back")。
  分布:follow_ups×10、drafts×5、repository×3、candidates×3、conversations×3、imports×3、snapshots×3、extractions×2、send_records×1。
- **外键:** schema 内 34 条 FK;运行时 `PRAGMA foreign_keys` 连接级开启
  (`ica-infra/src/db.rs:142`,写连接同时开 WAL + 5s busy_timeout;只读连接不动 journal mode 保证基线库字节不变)。
  测试断言 `ica-infra/tests/database.rs:95-112`;特例回归 `ica-api/tests/draft_unreachable_transition.rs:210/231`。

## 5. 备份 / 恢复 / 导出

| 能力 | 状态 | 证据 |
|---|---|---|
| backup | **不存在** | 全仓库 crates/*.rs 零命中 |
| restore | **不存在** | 同上 |
| export(业务快照) | 存在 | `POST /api/export`(`routes.rs:168`),`ica-application/src/sync.rs::export_data`,版本 "0.2.0",支持 `X-Encrypt-Aes-Key` 明文/AES-256-GCM 信封两种(`export_crypto.rs`,associated data `ica:export:0.2.0`) |
| import(业务快照) | 存在 | `POST /api/import`(`routes.rs:169`),幂等 upsert、`updated_at` last-write-wins、`send_records` append-only 拒改写(`snapshots.rs`) |
| 内容导入(贴文本/OCR) | 存在 | `POST /api/imports/preview|ocr|{id}|confirm|reject|confirm-candidate`(`routes.rs:157-167`),`imports.rs` + `ocr_rapid.rs`(RapidOCR 进程内引擎,reqwest 仅模型下载) |
| 桌面入口 | 存在 | `ica-desktop/src/commands.rs:777` 起 "W4: imports, export, import, OCR" |

## 6. 未提交代码归属(工作区 vs HEAD 编译行为)

| 未跟踪文件 | 行数 | 是否参与编译 | 消费者 |
|---|---|---|---|
| `ica-desktop/src/browser.rs` | 565 | **是**(`lib.rs` 未提交修改 `+pub mod browser;`) | `tests/browser_poc.rs` |
| `ica-desktop/src/bin/ica-browser-worker.rs` | 444 | **是**(自动发现 + 未提交 `[[bin]]` 声明) | `browser.rs:304` 经 `tokio::process::Command` 启动;依赖树新增 tokio-tungstenite/futures-util/sqlx/sha2 |
| `ica-desktop/tests/browser_poc.rs` | 288 | 是(B1 POC 验收测试) | — |

**结论:** B1 浏览器 POC 整体(模块 + worker 二进制 + 验收测试 + Cargo.toml/lib.rs 修改)处于未提交状态。
工作树与 HEAD 的编译产物不同;stash/checkout 到 HEAD 会让以上三者全部消失。
`docs/audits/w4-independent/rerun/` 下另有未跟踪的 `contract-capture.log`、`gate-check.log` 两份日志。

---

## 关键事实速览

| 指标 | 值 |
|---|---|
| Crate 数 | 7 |
| Rust 总行数 | 28,233(历史快照 24,797 已过时) |
| 迁移文件数 | **0**(仅 1 个 baseline 快照 SQL) |
| 表数 | 32(实测;baseline.sql 头注释 "20" 过时) |
| 索引数 | 121(实测;头注释 "66" 过时) |
| FK 约束 | 34 |
| 事务使用点 | 33(全在 ica-infra,业务写与审计同事务) |
| backup / restore | **不存在**(仅 export/import) |
| projects / applications 表 | **不存在** |
| B1 POC 提交状态 | **整体未提交**(参与工作树编译) |
