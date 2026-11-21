---
title: "ICA-RUST 测试计划"
document_id: "TEST-ICA-PLAN-R1.0"
document_type: "TestPlan"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "TEST-ICA-CATALOG-001"
  - "AUDIT-ICA-ENG-001"
  - "ENG-ICA-SPECS-R1.0"
supersedes: null
---

# ICA-RUST 测试计划 R1.0

依据 SPE-QA-001 §3(测试计划至少 12 项)编制。整合进项目实施计划(小型项目条款 §3.1),
不省略关键要求。

## 1. 测试目标

验证 ICA-RUST 已实现域(Position/Candidate/Conversation/Follow-up/Draft/Import/Export/OCR/Dashboard)
满足 Python 基线契约;验证工程门禁(分层/fmt/clippy/build/test)可重复执行。

## 2. 测试范围

**本次验证:** 47 个 HTTP 端点、47 个 IPC 命令、领域规则、事务与审计一致性、路由 parity、
契约对拍(与 Python 基线 248 case)。

**本次不验证:** W5/W6 未实现域(ai/settings/backups/auth/shares,23 端点)、GUI 集成
(前端在姊妹仓,未连接)、打包交付(姊妹仓 NSIS)、真实 AI Provider(AI-EXT 延后)。

## 3. 测试对象与环境

| 对象 | 层级 | 环境要求 |
|---|---|---|
| crates/ica-domain | UNIT | 无外部依赖 |
| crates/ica-contracts | UNIT | 无外部依赖 |
| crates/ica-application | UNIT | 无外部依赖 |
| crates/ica-infra | UNIT/INT | 临时 SQLite(基线 DDL 整库快照) |
| crates/ica-api | INT | 临时 SQLite + axum router 直连 |
| crates/ica-desktop | INT | DesktopState 真实派发;browser_poc 需本机 Chromium(缺失则 NOT_EXECUTED 跳过) |
| 跨语言契约 | Contract | 需 Python 基线 venv(WSL);`bash scripts/contract_verify.sh` |

环境记录要素(QA-001 §6.2):Windows 10.0.26200 x64 / Git Bash / WSL Ubuntu-26.04 /
rust-toolchain.toml 锁定版本 / Python 3.12(仅基线 venv)。见 `docs/audits/mvp-r1.0/test-baseline.md`。

## 4. 测试类型与工具

| 类型 | 工具 | 现状 |
|---|---|---|
| Unit / Integration | `cargo test --workspace` | 354 收集(2026-11-21 实测) |
| Contract | `contract_verify.sh` + parity fixture | 248/248(E2 历史证据,Windows 未复跑) |
| IPC | commands_smoke + w4_imports_commands | 通过(信封与 HTTP 逐字段同形) |
| E2E | 无 | **缺位**(§8.2 十二步闭环无对应物,随 Phase 4/5 建立) |
| Security | 路径守卫/加密导出回归 + 待补 cargo audit | 部分覆盖 |
| Performance | 无 | 未纳入本轮 |

## 5. 通过标准与门禁

1. `cargo test --workspace` 全绿(0 failed);Blocked/Skipped/Not Applicable 不计入 Passed。
2. `cargo fmt --check` 与 `cargo clippy --workspace --all-targets` 零 error 零 warning(已提交代码)。
3. `scripts/check.py` 全部门禁通过。
4. `scripts/check_scope.py` 两条范围门禁通过(68 端点表 + schema 对象集)。
5. 契约对拍零未解释差异(Regression = 差异必须有 INTENTIONAL/BUG 记录)。
6. 无 Critical/Major 未批准遗留缺陷(defect-register.md)。

## 6. 测试执行与证据

- 每次执行记录:commit、环境、时间、结果(审计 test-baseline.md 为首份正式记录)。
- 证据保存:测试输出日志 + 本报告 + defect-register 关联。
- 唯一当前失败:TC 对应 `ocr_inside_the_staging_directory_reports_an_undecodable_image`
  → DEF-001(夹具平台缺陷,修复方案已登记)。

## 7. 回归策略(QA-001 §10)

| 变更 | 范围 |
|---|---|
| 局部代码 | 模块 cargo test + check.py |
| 契约/端点变更 | check_endpoint_map + contract_verify + 全量 |
| schema 变更 | compare_baseline_schema + 全量(注意 DB-02 无迁移机制的约束) |
| 依赖升级 | cargo audit + 全量 |
| 发布 | 完整回归 + 打包验证(姊妹仓) |

## 8. 风险

| 风险 | 缓解 |
|---|---|
| Windows/WSL 平台差异(已发生 DEF-001) | 路径/JSON 构造用平台中性 API;两环境均跑 |
| browser_poc 依赖本机 Chromium | NOT_EXECUTED 语义明确区分,不计入 Passed |
| 契约对拍依赖基线 venv | check.py 刻意不依赖 venv;对拍单独成卡 |
| E2E 缺位 | 登记于缺陷台账 DEF-003,随 Phase 4/5 关闭 |
