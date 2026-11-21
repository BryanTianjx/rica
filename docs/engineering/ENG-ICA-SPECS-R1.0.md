---
title: "ICA-RUST 项目工程规范配置"
document_id: "ENG-ICA-SPECS-R1.0"
document_type: "Engineering"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "SPE-GOV-001"
  - "SPE-BAS-003"
  - "SPE-BAS-004"
  - "SPE-QA-001"
  - "SPE-QA-006"
  - "AUDIT-ICA-ENG-001"
supersedes: null
---

# ICA-RUST 项目工程规范配置 R1.0

## 1. 文档目的

依据 SPE-GOV-001 §5.3 建立本项目的规范配置:项目工程规范清单、规范适用性矩阵、
项目特殊约束、规范豁免记录、工程审计结果索引。本文件是项目级工程治理的单一入口。

## 2. 适用范围

本仓库 O:\ICA-RUST(Rust 后端 + 桌面命令层)。姊妹仓 O:\ica(Python 基线 + 前端 +
Tauri 宿主)仅作只读参照,不在本配置治理范围内。

## 3. 项目工程规范清单

| 规范编号 | 规范名称 | 版本 | 适用度 | 项目落地物 |
|---|---|---|---|---|
| SPE-GOV-001 | 软件项目工程规范体系 | R1.1 | 核心 | 本文件及其 §5-§8 |
| SPE-BAS-003 | 软件项目文档工程规范 | R1.1 | 核心 | `docs/index.md`、新文档元数据、模板 |
| SPE-BAS-004 | 软件项目辅助脚本工程规范 | R1.1 | 核心 | `scripts/README.md`、脚本整改 |
| SPE-QA-001 | 软件项目测试工程规范 | R1.1 | 核心 | `docs/testing/test-plan.md`、测试报告、缺陷台账 |
| SPE-QA-006 | 软件项目测试用例设计与管理规范 | R1.1 | 核心 | `docs/testing/test-case-catalog.md`、追溯矩阵 |
| SPE-BAS-001 | C/C++ 软件项目代码工程规范 | R1.1 | 不适用 | 本仓库无 C/C++ 代码(见 §5) |
| SPE-QA-002 | 协议一致性验证工程规范 | R1.1 | 不适用 | 非协议类项目(见 §5);跨语言契约对拍归 SPE-QA-001 Contract 类型 |
| SPE-BAS-002 | Rust 软件项目代码工程规范 | — | 待评估 | 规范尚未建立(P1);暂以 cargo fmt/clippy/check.py 门禁替代(见 §7) |

## 4. 规范适用性矩阵(ICA-RUST)

依据 SPE-GOV-001 §5.2 初始矩阵,经本项目技术栈确认:

| 规范领域 | 初始矩阵(ICA) | 本项目确认 | 确认依据 |
|---|---|---|---|
| C/C++ 代码 | 条件 | **不适用** | 全仓无 C/C++ 源码 |
| Rust 代码 | 核心 | **核心(规范待建)** | 7 crates 纯 Rust;SPE-BAS-002 未建立,现行门禁替代 |
| 文档工程 | 核心 | **核心** | 本文件与 docs/index.md 生效起执行 |
| 辅助脚本 | 核心 | **核心** | scripts/README.md 已建立,存量整改分阶段 |
| 架构设计 | 核心 | **核心** | 分层边界由 scripts/check.py 自动强制;架构文档见 docs/index.md DOC-03 |
| 协议一致性验证 | 不适用 | **不适用** | 非通信协议项目;与 Python 基线的契约对拍属 QA-001 契约测试 |
| 自动化测试 | 核心 | **核心** | cargo test + 契约对拍;用例体系建立中(QA-006) |
| 安全工程 | 核心 | **核心(规范待建)** | SPE-SEC-001 待建设;现行:审计矩阵 SEC-01~10 + 门禁 |
| 跨平台交付 | 核心 | **核心(条件落地)** | Windows 优先;打包在姊妹仓 NSIS(现状见 AUDIT-ICA-ENG-001) |
| AI Agent 工程 | 核心 | **核心(规范待建)** | SPE-AI-001 待建设;Hermes 集成为 Phase 6/WP-08 |

## 5. 不适用项评估记录

| 规范 | 结论 | 评估理由 | 复核条件 |
|---|---|---|---|
| SPE-BAS-001(C/C++) | 不适用 | 本仓库无任何 C/C++ 源码、无 C/C++ 构建目标;RapidOCR 为 Python 运行时,非 C/C++ 工程 | 引入 C/C++ 依赖或原生扩展时 |
| SPE-QA-002(协议一致性) | 不适用 | 规范适用范围为"具有正式协议标准、消息格式和编码规则的通信软件"(3GPP/IEEE/ITU);本项目为招聘业务工作台。与 Python 基线的跨语言行为对拍(248/248)属于 SPE-QA-001 §2.2 Contract 测试类型,管理要求由 QA-001/QA-006 承担 | 无(业务性质不变) |

## 6. 项目特殊约束

1. **契约权威链:** Python 基线(`O:\ica` @ `9ed173c`)是业务行为契约权威;Rust 与基线不一致时,
   基线优先,除非有 ADR 明确变更(见 docs/adr/)。契约文档 `docs/rust-migration-contract.md`
   为 68 端点的权威登记,与代码的一致性由 `scripts/check_endpoint_map.py` 自动校验。
2. **波次治理:** 工程活动按波次门禁推进(见 docs/ICA-RUST 当前设计总方案.md、ADR-0003~0006);
   本规范配置不替代波次治理,是其工程基线的补充。
3. **验收记录不可变:** `docs/audits/w4-independent/` 等 acceptance 记录按 ADR-0006 冻结,
   其路径被提交历史与验收判据绑定 — 目录重命名/移动会使验收记录失效。
4. **跨仓结构:** 前端与 Tauri 宿主在姊妹仓(见 AUDIT-ICA-ENG-001 ipc-gui-inventory);
   本仓库文档涉及前端时引用姊妹仓路径,不复制内容。
5. **AI 生成内容边界:** AI 生成文档不自动获得 Approved/Active 状态;工程结论须区分
   Verified / Decision / Planned / Assumption / TBD(SPE-BAS-003 §15.2)。审计类文档以
   E1(当前证据)/E2(历史对照)标注证据等级。

## 7. 规范豁免记录

依据 SPE-GOV-001 §4.3,以下偏离经项目负责人确认前为**待批准豁免草案**(批准后转正式):

| 编号 | 偏离条款 | 偏离内容 | 原因 | 风险与替代控制 | 有效期 |
|---|---|---|---|---|---|
| EX-001 | BAS-003 §7(文档元数据) | 存量 docs/ 文档未带 YAML front matter、未按 `<类型>-<项目>-<主题>-<版本>` 命名 | 存量路径被波次验收记录与提交历史绑定(特殊约束 3);批量重命名破坏治理链 | 新文档全部带元数据;docs/index.md 建立存量映射与状态;不影响文档内容正确性 | 至下一文档工程验收 |
| EX-002 | BAS-003 §16(标准文档目录) | 未迁移到 docs/01-…10- 编号目录结构 | 同 EX-001;采用 docs/index.md 十域映射等效实现目录语义 | index.md 为导航权威;新增文档遵循现有域目录 | 至下一文档工程验收 |
| EX-003 | BAS-004 §3.1(脚本子目录) | scripts/ 平铺,未按 env/build/test/… 分子目录 | §3.2 允许"脚本数量较少时简化目录,但保留清晰职责划分";16 个脚本职责由 README 清单划分 | scripts/README.md 为职责权威;移动脚本需同步引用,列为分阶段任务 | 长期(按 §3.2 简化条款) |
| EX-004 | BAS-004 §5.3(标准文件头) | 存量脚本以 docstring(Purpose/Usage)替代 Copyright/Project/Module 头 | 13 个存量脚本已稳定运行且被文档引用;头部改造属 §14.2 专项审查范围,churn 大于收益 | scripts/README.md 承担 Project/Module/Purpose 登记;新脚本必须带标准头 | 至下次存量脚本专项审查 |
| EX-005 | BAS-004 §5.5(argparse) | check.py、check_scope.py、contract_verify.py、peek_seed.py 无参数解析(零参固定行为) | 四个脚本为固定流程工具,无参数即接口;改造无行为收益 | 无参数即无输入风险;行为由 README 登记 | 长期(接口不变即有效) |
| EX-006 | QA-006 §3(测试用例编号) | 存量 354 个 Rust 测试无 Case ID | 编号体系随本轮建立;对存量测试逐个回溯标注工作量大且不改测试有效性 | 用 `scripts/generate_test_catalog.py` 生成草稿目录建立映射基线;**新测试必须先登记 Case ID**;存量按波次分批登记 | 至用例目录正式验收 |
| EX-007 | QA-006 §8(需求追溯) | 需求层(REQ-xxx)尚未建立正式编号 | MVP 范围裁决(S-1)未定,需求基线冻结前建立 REQ 编号会随裁决返工 | 追溯矩阵以审计矩阵 BIZ/API/DB/AI/GUI/SEC ID 为需求代理 ID;S-1 裁决后迁移 | 至 MVP 范围裁决 |
| EX-008 | QA-001 §6(环境分类) | 无独立测试环境分类;测试在开发机临时库执行 | 桌面本地应用无部署环境梯度;SQLite 临时库即隔离环境 | 每测试自建临时库(include_str 基线 DDL),环境随 AUDIT 记录(OS/工具链) | 长期(单机产品形态) |

## 8. 工程审计结果索引

| 审计 | 日期 | 范围 | 状态 |
|---|---|---|---|
| tauri-architecture-audit-r0.1 | 2026-11-19 前 | Tauri 架构 | 归档(docs/audits/) |
| w4-independent(FINDINGS/LWW/OCR) | 2026-11-19 | W4 独立审计 | 判 FAIL 后整改闭环,验收记录冻结 |
| **AUDIT-ICA-ENG-001(mvp-r1.0/)** | **2026-11-21** | WP-00 全仓基线:63 项矩阵 PASS 34/PARTIAL 19/FAIL 2/MISSING 7/VERIFY 1;3 项战略发现 S-1~S-3 | Draft(待项目负责人确认) |
| 本文件建立(规范对齐) | 2026-11-21 | GOV/BAS/QA 7 份规范对标落地 | Draft(待项目负责人批准) |

## 9. 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| 1.0.0 | 2026-11-21 | 初始建立:清单、矩阵、不适用评估、特殊约束、8 项豁免草案、审计索引 |
