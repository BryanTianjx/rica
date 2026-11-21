# 工程基线报告(baseline-report)— WP-00

**审计编号:** ICA-ENG-AUDIT-001 / WP-00(工程基线冻结)
**基准文档:** 《ICA-RUST MVP 产品化差距审计与实施路线 R1.0》(ICA-RUST.txt)
**核验日期:** 2026-11-21
**证据等级:** 全部结论基于 E1(当前源码/测试/运行),E2 用于历史对照
**使用的规范及版本(SPE-GOV-001 §8.4):** SPE-GOV-001 R1.1(审计记录要素/不符合项分级)、
SPE-BAS-003 R1.1(文档/证据要求)、SPE-BAS-004 R1.1(脚本合规检查项)、SPE-QA-001 R1.1
(测试状态体系/测试报告要求)、SPE-QA-006 R1.1(用例编号与追溯)。对齐结果见
`docs/engineering/ENG-ICA-SPECS-R1.0.md`。

---

## 1. 基线核验结论

### 1.1 R1.0 文档基准 vs 实际(全部复核)

| 项 | R1.0 文档基准 | 实际核验(2026-11-21) | 结论 |
|---|---|---|---|
| Rust HEAD | `4cbb736` | **`ff7b9eb`** | 文档基线过期 11 个提交(43→54) |
| 提交数 | 43 | **54** | 同上 |
| 工作区 | 要求干净 | **不干净** | 5 个已修改文件 + 5 个未跟踪路径(见 §2) |
| Python 基线 | `/mnt/o/ica` @ `9ed173c` | `O:\ica` @ **`9ed173c`** ✅(303 commits,工作区干净) | 一致,HEAD 即 freeze 提交 |
| 迁移契约文档 | 待核 | `docs/rust-migration-contract.md` 存在且与实现**零漂移**(47 = 45+health+1 已记载例外) | 一致性 PASS |

**重要澄清:** R1.0 文档的审计基准 `4cbb736` 已被 11 个提交超越,包括:W4 独立复审闭环
(`66a0534`→`0e4c10c`)、OCR 真实引擎收敛(`fc1c996`→`67e9d00`)、dashboard 日历、
ADR-0004/0005/0006 裁决、G3 Hermes 能力调查、**B0-09 用户签署(B0 进入验收、B1 解冻)**
(`ff7b9eb`)。本文档所有结论均以 `ff7b9eb` + 当前工作区为准。

### 1.2 工作区状态(审计期间快照)

```
M  Cargo.lock / Cargo.toml / crates/ica-desktop/Cargo.toml / src/lib.rs / scripts/contract_capture.py
?? ICA-RUST.txt(本审计的输入文档)
?? crates/ica-desktop/src/browser.rs(565 行,B1 POC 监管者)
?? crates/ica-desktop/src/bin/ica-browser-worker.rs(444 行,CDP worker)
?? crates/ica-desktop/tests/browser_poc.rs(288 行,POC 验收)
?? docs/audits/w4-independent/rerun/{contract-capture,gate-check}.log
```

- 未提交代码**参与工作树编译**(lib.rs 的 `+pub mod browser;` 为未提交修改)。
- **全部 lint 违规集中在未提交 POC**:fmt diff 仅在 `ica-browser-worker.rs`;
  clippy 7 个警告全部属 ica-desktop(POC 文件)。已提交代码 clippy/fmt 干净。
- B0 验收已签署但 B1 POC 代码未提交 — 存在"验收结论与代码入库状态脱节"的风险。

### 1.3 历史数据复核(防止虚假验收)

| 历史主张 | 出处 | 本审计裁定 |
|---|---|---|
| 7 crates / 24,797 行 | bf2bae4 快照 | crate 数正确;行数已过时(现 28,233) |
| 45/68 端点 | 设计总方案 | **精确成立**:47 代码 = 45 声明 + health + 1 已记载例外 |
| 46 IPC 命令 | frontend-architecture-r1(2026-11-19) | 已过时,现 **47**(W7 增 imports_confirm_candidate) |
| 333 / 351 tests passed | bf2bae4 / 设计总方案 | 环境专属(WSL)。Windows 原生实测 **354/353/1**(见 test-baseline.md) |
| fmt 曾未通过 | bf2bae4 快照 | 当前已提交代码 fmt 干净;未提交 POC 违规 |
| W5 15 + W6 8 未完成 | 契约文档 | 成立,已由 ICA-RUST-W5-W6 工作项承接(2026-11-20 批准) |

## 2. 治理状态核对(R1.0 Phase 结构 vs 实际波次治理)

项目实际采用**波次(Wave)+ 门禁治理**(见 `docs/ICA-RUST 当前设计总方案.md`、ADR-0003~0006),
与 R1.0 的 Phase 0-6 / WP-00-10 是两套并行框架。映射关系:

| R1.0 Phase | 对应波次 | 当前状态 |
|---|---|---|
| Phase 0 基线冻结 | WP-00(本审计) | **本次执行** |
| Phase 1 领域与数据闭环 | R0/W1/W2/W3(已完成)+ 领域审计 | 已实现域闭环良好;**Project/Application 实体缺位**(见 §3) |
| Phase 2 API/IPC 契约闭合 | W5(15)+ W6(8) | 已立项未实施(ICA-RUST-W5-W6) |
| Phase 3 AI 产品化 | W5 ai 域 + AI-EXT(ADR 裁决延后) | port/上下文/审核流就绪,真实 provider 延后 |
| Phase 4 GUI 联调 | F0-AUDIT → F1→F4 前端方案(提案态) | **未开工**;前端在姊妹仓且当前接 Python 后端 |
| Phase 5 数据安全与交付 | G6 打包(姊妹仓 NSIS)+ backup/restore | **backup/restore 缺失**;NSIS 在姊妹仓打包 Python sidecar |
| Phase 6 Hermes 集成 | G3 调查完成(网关 API 假设被推翻)| 调查结论:`v0.23.1 无 Gateway API`(d6ddce2) |
| (无对应) | W7 浏览器抽取写路径 | 已完成(confirm-candidate) |
| (无对应) | B0 平台准入 / B1 浏览器 POC | B0-09 已签署;B1 解冻,POC 未提交 |

## 3. 战略级发现(需要用户裁决,非工程可自行决定)

### S-1【最高优先级】MVP 范围与既有契约存在结构性冲突

R1.0 §3.2/§3.3 把 **Project、Application 列为 P0 核心实体**(Application 是"招聘业务的核心
关联实体"),但:

- Python 基线(契约权威,68 端点)**没有** projects/applications 实体;
- Rust 实现(schema 32 表、47 端点、领域模型)**同样没有**;
- 现有业务模型是 Candidate ⇄ Position 直连(position_id 外键),Conversation/Follow-up/Draft
  全部挂在 Candidate 上。

即 R1.0 的 MVP 定义与"Python 契约迁移"是**两个不同的产品范围**。按 §6.3 归类:UNRESOLVED。
可选路径:
- A. 按 R1.0 §3 扩展范围(新增 Application/Project 波次,写 ADR,影响 schema/契约/GUI);
- B. 修订 R1.0 §3,以 68 端点契约 + W5/W6 + GUI 集成作为 MVP 定义(现基线天然满足领域模型);
- C. 混合:先交付 B,Application 作为 MVP 后第一个功能波次。

**本审计不代做裁决;矩阵中 BIZ-01/05/06、GUI-04 标 MISSING,处置取决于此裁决。**

### S-2 GUI↔Rust 后端零运行时连接

前端(姊妹仓 React 19,10 页面,90 个 API 调用点)当前对接 Python sidecar;
ICA-RUST 的 47 个 IPC 命令无 shell host 消费(0 个 `#[tauri::command]`)。
F0-AUDIT(前端接口耦合审计,含前端代码归属裁决)在治理台账上为 READY(parked)。
这是 MVP 产品化路上**工作量最大的缺口**,且其前置(F0 + 跨仓归属)需要用户开工指令。

### S-3 B1 POC 未提交 + 验收状态脱节

B0-09 已签署"B0 进入验收、B1 解冻",但 B1 的全部代码(浏览器模块 + worker + 测试)未入库。
lint 不达标(fmt/clippy)使其无法按 ADR-0005 质量门禁直接提交。
建议:POC 清理 lint 后作为一个独立提交入库(或明确 B1 POC 不入库的决定)。

## 4. 交付物索引

| 文件 | 内容 |
|---|---|
| `module-inventory.md` | 7 crates 结构/行数/架构边界/schema/迁移/事务/导出/未提交归属 |
| `api-inventory.md` | 47 端点全清单、错误/分页/校验/幂等/文档一致性/测试覆盖 |
| `ipc-gui-inventory.md` | 47 IPC 命令、Tauri 缺位、前端 10 页面、三态、打包、集成状态 |
| `test-baseline.md` | Windows 实测 354/353/1、唯一失败根因、lint 基线、门禁映射 |
| `verify-matrix.md` | **63 项 VERIFY → 证据结论转换**(PASS 34 / PARTIAL 19 / FAIL 2 / MISSING 7 / VERIFY 1) |

## 5. 通过条件自检(WP-00)

- [x] 当前工程状态有可复核证据(本目录 5 份文件,全部带文件:行号引用)
- [x] 代码范围确认(7 crates / 28,233 行 / 47 端点 / 47 IPC / 32 表)
- [x] 测试基线确认(Windows 原生实测,非历史转抄)
- [x] Python 基线核验(`9ed173c` = HEAD,干净)
- [x] 迁移契约核验(零漂移)
- [x] 不修改代码、不提交推送(遵守 R1.0 审计边界)
