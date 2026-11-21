---
title: "ICA-RUST 缺陷台账"
document_id: "DEF-ICA-REGISTER-001"
document_type: "DefectRegister"
project: "ICA-RUST"
version: "1.0.0"
status: "Active"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "TEST-ICA-PLAN-R1.0"
  - "AUDIT-ICA-ENG-001"
supersedes: null
---

# 缺陷台账(Active,随缺陷开闭维护)

依据 SPE-QA-001 §9(缺陷至少含编号/标题/发现版本/影响模块/严重程度/优先级/复现条件/
复现步骤/预期/实际/证据/修复版本/回归验证)。生命周期:New → Triaged → Assigned →
In Progress → Fixed → Ready for Retest → Verified → Closed。

| 编号 | 标题 | 严重度 | 优先级 | 模块 | 发现版本 | 状态 |
|---|---|---|---|---|---|---|
| DEF-001 | OCR 测试夹具 Windows 平台缺陷(非法 JSON 转义) | Minor | P1 | ica-api/tests/w4_imports_http.rs | ff7b9eb(2026-11-21) | **Closed**(2026-11-21 修复,365/0 全绿回归确认) |
| DEF-002 | 数据库零迁移机制,已装库无法升级 | Major | P1 | ica-infra / ica-server | ff7b9eb(2026-11-21) | **Closed**(ica-infra::migrations,6 测试通过;365/0 全绿回归确认) |
| DEF-003 | 核心招聘闭环 E2E 测试缺位 | Major | P2 | 全仓 | ff7b9eb(2026-11-21) | Triaged |
| DEF-004 | 未提交 B1 POC 违反 fmt/clippy 门禁(7 警告+fmt diff) | Minor | P2 | ica-desktop(未提交文件) | ff7b9eb(2026-11-21) | **Closed**(fmt --check 通过 + clippy 全仓 0 警告;365/0 全绿回归确认) |
| DEF-005 | ica-baseline.sql 头注释过时("20 tables/66 indexes",实测 32/121) | Trivial | P3 | ica-infra/schema | ff7b9eb(2026-11-21) | **Closed**(2026-11-21 头注释已重测修正) |
| DEF-006 | browser_poc 在"发现 Chromium 但浏览器不可启动"的机器上无限挂起 | Major | P2 | ica-desktop/tests/browser_poc.rs(未提交) | ff7b9eb(2026-11-21) | Triaged |

## 明细

### DEF-001 OCR 测试夹具 Windows 平台缺陷
- **复现条件:** Windows 原生(Git Bash)执行 `cargo test -p ica-api --test w4_imports_http`。
- **复现步骤:** 1) 在 Windows 工作区跑该测试文件;2) 观察 `ocr_inside_the_staging_directory_reports_an_undecodable_image` 失败。
- **预期:** 503 `ocr_unavailable`(本机无 OCR 模型时的可读错误分支)。
- **实际:** 422 `validation_error` — 请求体 JSON 本身非法(`invalid escape at line 1 column 55`)。
- **根因:** w4_imports_http.rs:337 硬编码 `/tmp/ica-test-ocr`(POSIX);:342-345 用 `format!` 拼接
  Windows 路径未转义反斜杠(`\i`/`\w` 非法 JSON 转义);:370-375 模型检测用 WSL 路径 `/mnt/o/ica/...`。
  产品代码未被触达,纯夹具缺陷。
- **修复方案(已评):** 路径改 `std::env::temp_dir()`;JSON 体改 serde_json 序列化;
  模型路径参数化(env 或 cfg)。环境差异按 SPE-QA-001 兼容性测试条款治理。
- **证据:** docs/audits/mvp-r1.0/test-baseline.md §2。

### DEF-002 数据库零迁移机制
- **复现条件:** 检查 crates/ica-infra 与 ica-server;`grep -r migrate\|refinery`。
- **预期(SPE 要求):** schema 版本化迁移 + 升级路径(AUDIT-ICA-ENG-001 DB-02 FAIL)。
- **实际:** 仅 baseline 快照;测试整库应用;server 只自检 2 张表;alembic_version 表存在但无版本行。
- **影响:** 真实用户数据装库后无法升级 schema — MVP 发布门禁阻断项。
- **证据:** docs/audits/mvp-r1.0/module-inventory.md §3。

### DEF-003 核心招聘闭环 E2E 缺位
- **复现条件:** 对照 R1.0 文档 §8.2 十二步闭环清单与现有测试文件。
- **实际:** 无跨模块 E2E(项目→岗位→候选人→沟通→跟进→AI 草稿→重启验证)。
- **关联:** AUDIT-ICA-ENG-001 行动清单 #6/#7;随 Phase 4/5 建立。

### DEF-004 B1 POC lint 违规
- **复现条件:** 工作树(未提交 browser.rs/bin/browser_poc.rs)执行 `cargo fmt --check` 与 clippy。
- **实际:** fmt diff 仅在 ica-browser-worker.rs;clippy 7 警告全属 ica-desktop POC 文件;
  已提交代码零违规。另 1 个 unused import。
- **处置:** 与 S-3(POC 入库决策)联动:清理后提交或明确不入库。

### DEF-006 browser_poc 环境健壮性缺陷(挂起)
- **复现条件:** Windows 机器上 `discover_chromium` 能找到 Chromium 路径、但浏览器实际无法启动/连接
  (本机现象:3 个 ica-browser-worker 进程存活但 0 CPU,无任何 chrome 进程)。
- **实际:** 3 个 POC 用例无限运行(>10 分钟),无超时看门狗;cargo test 套件被阻塞。
- **预期:** 启动/连接应有超时,超时后按 NOT_EXECUTED(Skipped)语义退出,不计入 Passed。
- **处置建议(属未提交 B1 POC 作者):** supervisor 增加启动/连接超时;
  `discover_chromium` 成功 ≠ 可启动,launch 失败要回到跳过路径。
- **本轮规避:** 全量验证以 `--skip` 显式排除 3 个 POC 用例并记录(Skipped,不计入 Passed)。
- **证据:** 进程列表(19:52:02 三个 worker,0 CPU)+ 测试日志 60 秒超时告警 ×3。

### DEF-005 baseline.sql 头注释过时
- **复现条件:** `grep '^CREATE TABLE' crates/ica-infra/schema/ica-baseline.sql | wc -l` = 32;
  `grep -c 'CREATE INDEX'` = 121;文件头写 "20 tables, 66 indexes"。
- **处置:** 重新生成(脚本导出)或人工修正注释;不触碰 DDL 本身。
