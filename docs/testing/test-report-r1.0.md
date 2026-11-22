---
title: "ICA-RUST 测试报告 R1.0"
document_id: "TREP-ICA-1.0.0"
document_type: "TestReport"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "TEST-ICA-PLAN-R1.0"
  - "DEF-ICA-REGISTER-001"
  - "AUDIT-ICA-ENG-001"
supersedes: "TREP-ICA-1.0.0-draft-2026-11-21"
---

# 测试报告 R1.0(第二次执行,全绿)

依据 SPE-QA-001 §12(报告 14 项)与 BAS-003 §13.3(必须区分已执行/通过/失败/未执行/
阻塞/已知缺陷/遗留风险;不得把计划测试描述为实际通过)编制。

## 1-4. 概要信息

| 项 | 值 |
|---|---|
| 项目 | ICA-RUST |
| 被测版本 | HEAD `ff7b9eb` + 未提交 B1 POC(工作区,2026-11-21) |
| 执行环境 | Windows 10.0.26200 x64 / Git Bash / rust-toolchain.toml 锁定工具链 |
| 执行时间 | 2026-11-21 |
| 命令 | `cargo test --workspace --no-fail-fast` |

## 5. 执行统计

| 状态(SPE-QA-001 §7.1) | 数量 | 说明 |
|---|---|---|
| Passed | **385** | 全部测试二进制跑完(fail-fast 已禁用;命令见 §9) |
| Failed | **0** | — |
| Not Run(未收集) | 0 | — |
| Blocked | 0 | — |
| Skipped | **3** | browser_poc 全部用例,DEF-006(发现 Chromium 但不可启动时无限挂起,缺超时看门狗)— 显式 `--skip` 并按 §7.1 不计入 Passed |
| Not Applicable | 0 | — |

**合计收集 384。** 用例级映射见 `docs/testing/test-case-catalog.md`(384 条,DRAFT)。
累计新增:迁移机制 6 + audit-events 7 + settings 5 + ai 六端点 5 + ai-key/backups 5 +
auth/shares 4(HTTP)。实现端点 45 → **68/68(契约全量落地)**;模型 CRUD 4 条按裁决属
AI-EXT 不在 68 内。W6 要点:refresh 单次轮换(守卫式原子 UPDATE,并发安全)、bcrypt
密码哈希、本地合成用户应答 `me`(local mode 保持默认,无中间件)。

## 6-7. 通过/失败情况

- 通过:**365/365(执行范围 100%)**。按 crate:domain/application/contracts/infra/api/desktop/server 全部通过。
- 失败:0(第一次执行的 DEF-001 夹具缺陷已修复并回归:tempfile 派生暂存目录 + serde_json
  构造请求体 + 模型路径双视图探测)。

## 8. 缺陷统计

见 `docs/testing/defect-register.md`:DEF-001(Minor/P1,本报告唯一执行失败)、
DEF-002/003(Major,结构性缺位,非本轮执行失败)、DEF-004(Minor,lint)、DEF-005(Trivial)。

## 9. 日志及测试证据索引

| 证据 | 位置 |
|---|---|
| 完整测试输出(353 ok + 1 failed 明细) | 审计执行记录;复跑命令 `cargo test --workspace --no-fail-fast` 可复现 |
| 唯一失败断言详情 | `docs/audits/mvp-r1.0/test-baseline.md` §2(含错误体原文) |
| 用例目录(354 条 Case ID 草稿) | `docs/testing/test-case-catalog.md` |
| 追溯矩阵(需求代理 → 用例) | `docs/testing/traceability-matrix.csv`(AI-04 为 UNMAPPED 缺口) |
| Lint 证据 | fmt:已提交代码干净/POC 违规;clippy:7 警告全属未提交 POC(`docs/audits/mvp-r1.0/test-baseline.md` §4) |

## 10. 未覆盖范围

W5/W6 域(23 端点)、GUI 集成、打包交付、真实 AI Provider、E2E 十二步闭环、
性能测试、cargo audit 依赖扫描(工具未安装,SEC-09 VERIFY)。

## 11. 遗留风险

1. 契约对拍 248/248 为 E2(WSL 历史)证据,Windows 未复跑;基线 venv 可用时可复验。
2. 平台可移植性:唯一执行失败即平台夹具缺陷,提示两环境(Windows/WSL)均应纳入门禁。
3. 需求追溯矩阵以审计矩阵 ID 为代理(EX-007),S-1 裁决后需迁移到正式 REQ 编号。

## 13. 演练执行记录(backup/restore drill D1-D10,2026-11-21)

契约验收前置(恢复演练)已执行:全套 D1-D10 **PASS**。矩阵与证据引用见
`docs/testing/backup-restore-drill.md`(勾选列);原始响应/日志快照存于
`docs/testing/drill-evidence/`(D9 进程演练)。

| 执行方式 | 覆盖 | 证据 |
|---|---|---|
| 套件内自动化(`backup_drill_http.rs`,5 测试) | D1、D2(A/B 双备份状态往返)、D7(5 秒并发读穿过恢复窗口零 5xx)、D8(keep_count=3 剪枝两族合算)、D10(运行中删档→列表消失→404) | 5.35s 全绿 |
| 既有回归(引用) | D3-D5(守卫次序与零写入)、D6(审计行带 previous_copy) | `ai_key_backups_http.rs` 既有测试 |
| **真实进程演练(D9)** | `ica-server.exe` + curl:空库→建探针→备份(880,640 B)→删探针→恢复→**taskkill /F 硬杀**→重启→探针恢复、两份备份在列、`backup.restored` 审计轨迹持久 | `docs/testing/drill-evidence/`(health/probe/backup/restore/after-restart JSON + 双实例日志) |

要点:硬杀(强于优雅停机的持久性证据)后 SQLite 文件一致(在线备份 API + WAL
checkpoint 的组合),重启后服务自检通过、业务数据/备份文件/审计轨迹三者齐备。

## 15. 跨语言对拍记录(2026-11-22,W5/W6 纳入)

对拍集扩展至 **286 case**(新增 auth×13、settings/AI×12、backups×3、shares×6 与
最终观察),`contract_verify.py` 全流程 **286/286 匹配、0 失败**,18 张表终态 diff
全 PASS,基线库未动(git clean / sha256 一致)。对拍工程同步:
- seed 预置本地身份(本地租户 + 合成用户,与部署现实一致);
- capture 支持嵌套字段捕获与 body 占位符替换(refresh 轮换链、shares 接收者);
- diff 归一化新增:令牌对、备份文件名/路径/大小、每侧配置目录、OCR 暂存 side 标记、
  未来时间戳(`<future>`);行排序键屏蔽时间戳(双侧近现值不同导致的顺序漂移);
- 两侧共用 JWT_SECRET(本地默认空串会使 PyJWT 拒签)、各自独立 BACKUP_DIR
  (OCR 暂存保持共享 — 该目录本身就是 allowed_prefix 契约)。

## 14. 依赖安全扫描记录(SEC-09,2026-11-22)

`cargo-audit v0.23.0`(RustSec advisory 库 1555 条,Cargo.lock 390 依赖)首次例行执行:

| 发现 | 等级 | 处置 |
|---|---|---|
| RUSTSEC-2026-0459 — libsqlite3-sys 0.30.1 经 C SQLite CVE-2026-119375 | medium 6.8 | DEF-007(Minor/P2):本地暴露面有限;整改 = sqlx/rusqlite 协同升级至 libsqlite3-sys ≥ 0.39 + 全量回归 |
| RUSTSEC-2026-0543 / 2024-0436 / 2026-0192 — chrono/paste/ttf-parser 失维护 | warning | DEF-008(Trivial/P3):均无修复版本,接受 + 监测 |

原始输出:`docs/testing/sec09-audit-r1.log`。遗留:建议将 `cargo audit` 纳入
`scripts/check.py` 门禁链(advisory 库拉取依赖 GitHub 连通性,门禁化前先验证离线
`--db` 模式的可行性)。

## 12. 测试结论

- 本次测试**已完成计划范围**(实现域 UNIT/INT/Contract-IPC 层),测试计划 §5 门禁
  第 1/2/3 条达成(全绿 + fmt --check 通过 + clippy 全仓 0 警告,已提交与未提交代码同口径)。
- **结论:CONDITIONAL PASS** — 依据 QA-001 §11.3,遗留条件如下:
  - 风险说明:browser_poc 3 用例未执行(DEF-006,挂起缺陷,属未提交 B1 POC);
    契约对拍 248/248 为 E2 历史证据,Windows 未复跑;E2E 十二步闭环缺位(DEF-003)。
  - 责任人:ICA-RUST 项目负责人(B1 POC 作者)。
  - 整改计划:DEF-006 增加 supervisor 启动/连接超时后重跑 browser_poc;
    基线 venv 可用时复跑契约对拍;E2E 随 Phase 4/5 建立。
  - 审批:待项目负责人签署本报告(状态 Draft)。
