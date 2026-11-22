# ICA-RUST ADR-BATCH — 5 条 AC 核对结论

- 任务单：`wi_08408b4cdc8647a0` / `ICA-RUST-ADR-BATCH`（APPROVED → IN_PROGRESS）
- 核对对象：`docs/adr/ADR-0004-ica-runtime-architecture.md`、`docs/adr/ADR-0005-five-tier-quality-gates.md`、`docs/adr/ADR-0006-baseline-layers-and-change-handling.md`
- 批准基线：commit `c8740b6`（2026-11-20 00:57:34 +0800），三份 ADR 由 Proposed 改为 Accepted
- 核对环境：worktree `/mnt/o/ICA-RUST/.worktrees/ica-rust-adr-batch` @ `31a7ed1`，分支 `wt/ica-rust-adr-batch`
- 方法：只读核对 + 仅补 `docs/adr/` 文档；未改 `state.db`、未改基线仓 `/mnt/o/ica`、未 `git add/commit/push`
- 过期陈述：草稿（ADR-BATCH v1）写「三份 ADR 已起草，**均为 Proposed**」——磁盘与 `c8740b6` 均为 Accepted，该句已过期，交编排方处理（见 AC4）。三份 ADR 的 Accepted 状态本次保持不变。

## 逐条 AC 结论

| AC | 结论 | 一句话依据 |
| --- | --- | --- |
| AC1 三份 ADR 状态与结论一致 | **PASS** | 三份头部 L3 均为 Accepted，批准人 / 时间 / 依据口径完全一致 |
| AC2 三组待澄清项逐条有答案 | **PASS**（本次补齐） | 九问全部可定位出处；0004 原有 §5.1–5.3，0005/0006 本次新增「批准前澄清项（已答）」 |
| AC3 批准记录合规 | **PASS** | 署名 Bryan / 日期 2026-11-19T16:54Z / 结论口径「三份 ADR 批」齐备，来源用户本人 |
| AC4 受影响卡草稿对齐 | **FAIL（只报告，不修 state.db）** | 五张受影响卡的最新草稿均早于 ADR 批准且未出新版本对齐；其中 ADR-BATCH v1 明写「均为 Proposed」 |
| AC5 与总方案 §4.7/4.8/4.9 一致 | **PASS** | 质量门级别 / 基线四层 / 网关职责逐项同构，无冲突 |

### AC1 — 三份 ADR 状态与结论一致（Accepted）：PASS

- `docs/adr/ADR-0004-ica-runtime-architecture.md:3` `**状态：** **Accepted**`；批准人 / 时间 / 依据见 `:4`–`:6`
- `docs/adr/ADR-0005-five-tier-quality-gates.md:3` 同上；批准记录 `:4`–`:6`
- `docs/adr/ADR-0006-baseline-layers-and-change-handling.md:3` 同上；批准记录 `:4`–`:6`
- `git show c8740b6 -- docs/adr/ADR-0005-five-tier-quality-gates.md`：diff `Proposed（待批准）` → `**Accepted**`，0004/0005/0006 同批修改
- `docs/ICA-RUST 当前设计总方案.md:726`–`:728` 权威文档地图将三份均记为 Accepted 2026-11-20
- 结论口径一致：批准人 Bryan、批准时间 2026-11-19T16:54Z（CST 11-20 00:54）、批准依据「用户明确表态『三份 ADR 批』」。

### AC2 — 三组待澄清项逐条有答案：PASS（本次补齐）

草稿出处（只读）：`woticketflow/state.db` → `work_item_versions`，`wi_08408b4cdc8647a0` v1「待澄清项汇总」；共 9 问。

- ADR-0004 三问：答案原已在 `docs/adr/ADR-0004-ica-runtime-architecture.md:144`–`:196`（§5.1–5.3）；本次在 `:198` 新增 §5.4 速查表（`:200`–`:204`），逐条列问题 / 答案 / 出处。
- ADR-0005 三问：本次新增 `docs/adr/ADR-0005-five-tier-quality-gates.md:106`「## 批准前澄清项（已答）」（表 `:108`–`:114`）。
- ADR-0006 三问：本次新增 `docs/adr/ADR-0006-baseline-layers-and-change-handling.md:109`「## 批准前澄清项（已答）」（表 `:111`–`:117`）。
- 九问逐条答案见下节表格。答案出处落在 ADR 正文、ADR-0003 D-1…D-12 裁决或仓库实测文件中。
- 诚实标注：ADR-0005 正文未直接写「仓内 / 仓外」，该问按正文相对路径 + 磁盘实测判定为仓内工作树根目录，已在 ADR 表格中显式说明；ADR-0006 的锚点问题存在「快照已记 `9ed173c`、契约文档仍 `d39e321`」的待办遗留（总方案 §6.1，`:745`–`:754`），已标注为 G0-02 未闭合而非答案冲突。

### AC3 — 批准记录合规：PASS

- 署名：三份 ADR 头 `批准人：Bryan`（0004:4 / 0005:4 / 0006:4）
- 日期：三份 `批准时间：2026-11-19T16:54Z（CST 2026-11-20 00:54）`（各 `:5`）
- 结论口径：三份 `批准依据：用户明确表态「三份 ADR 批」（2026-11-20 00:54 CST 会话裁决）`（各 `:6`）
- 来源：`git show c8740b6` 正文 `on the user's explicit ruling (2026-11-20 00:54 CST)`；提交时间 2026-11-20 00:57:34 +0800
- Work Item 审批（只读）：`approvals` `apr_c83e10b23e024a50`，`decision=APPROVED`、`approved_by=Bryan`、`approved_at=2026-11-19T16:55:01+00:00`，绑定本卡 v1 草稿摘要；`lifecycle_events` rowid 308 `APPROVAL_RECORDED`（actor `Bryan`），`approved_scope` 明列「三份 ADR 批准与落地」
- 口径说明：Work Item 审批绑定的是本卡草稿（`CREATE_TICKET`），三份 ADR 本身的批准留痕在各自头部 + `c8740b6`；两者署名 / 时间 / 来源一致，均指向用户本人裁决，无代签。

### AC4 — 受影响卡草稿对齐：FAIL（只报告，不修 state.db）

共同事实：五张受影响卡的最新草稿创建 / 批准时间均在 2026-11-19T15:30–16:20Z，**早于 ADR 批准时间 16:54Z**；`approvals.draft_digest` 与各自最新草稿摘要一致（即旧审批对其自身草稿仍有效），但**没有一张出现为对齐已 Accepted ADR 而新建的草稿版本**。

| 卡（source_id / state） | 最新草稿 v / 创建时间 / 批准时间 | 对齐结论 |
| --- | --- | --- |
| ADR-BATCH（`wi_08408b4cdc8647a0`，IN_PROGRESS） | v1 / 16:46:35Z / 16:55:01Z | 草稿 L3「三份 ADR 已起草，**均为 Proposed**」已过期；L「本卡要做什么」第 3、4 步（改状态、更新受影响卡草稿）均未反映到草稿正文。自身即需新版本。 |
| B0（`wi_0217e2addf5641d2`，COMPLETED） | v2 / 16:16:34Z / 16:19:26Z | 无 ADR-0004 执行边界 / 统一网关口径；草稿 L33 仍记「ADR-BROWSER-001…010（待批准）」（另一组浏览器 ADR，不在本卡三份范围，另行核对）。 |
| G3（`wi_54c59a45766045bd`，BLOCKED） | v1 / 15:30:01Z / 16:20:12Z | 无 ADR-0004 的 Agent 受控工具 / 授权边界口径；H-POC 范围仍是通用 POC 描述。 |
| BW0（`wi_5e8fe402c22a4e5e`，IN_PROGRESS） | v1 / 16:16:34Z / 16:19:26Z | 无 ADR-0004 的 Browser Runtime 执行边界与「不得绕过 ica-application」口径。 |
| G0（`wi_2ee2b6dc860b4cdf`，COMPLETED） | v3 / 15:53:24Z / 16:20:12Z | 无 ADR-0006 四层基线概念 / 默认裁决人口径；草稿 L35–43「四项前置决策（全部需要用户批准）」把 D-1 写成未决，虽被 L128 起的 v3 修订覆盖，但未标注该历史段失效。 |
| W4-A（`wi_8362423d9e5743a1`，COMPLETED） | v3 / 16:16:34Z / 16:19:26Z | 无 ADR-0005 五级质量门 / Q4 独立验收口径引用；沿用批准前的验收表述。 |

处置建议（交编排方，本卡不执行）：

1. 确需对齐时，必须先出各卡的**新草稿版本**，再以新版本重新审批；改草稿会使绑定旧摘要的审批自动失效。
2. B0 / G0 / W4-A 已 COMPLETED，G3 已 IN_PROGRESS/BLOCKED，BW0 已 IN_PROGRESS——审批边不可回退，按 woticketflow 需后继 Work Item 承接或取得用户显式裁决，不得直接改草稿冒充对齐。
3. 本卡只读核对，未改 `state.db`，未改上述任何卡的草稿或审批记录。

### AC5 — 与总方案 §4.7/4.8/4.9 一致性：PASS

| 概念 | ADR 侧 | 总方案侧 | 结论 |
| --- | --- | --- | --- |
| 质量门级别 | `ADR-0005:36`–`:50`：Q0 Build / Q1 Unit / Q2 Integration / Q3 Compatibility / Q4 Independent Acceptance；Q0–Q3 全绿不构成放行，Q4 必须非实现方执行 | `docs/ICA-RUST 当前设计总方案.md:392`–`:401`：同五级、同规则 | 无冲突 |
| 基线四层 | `ADR-0006:40`–`:47`：Baseline Anchor / Contract Baseline / Test Fixture / Implementation；「Python 基线更新不自动改变冻结契约」；`:54`–`:59` Accept/Reject/Deferred/Out of Scope | `docs/ICA-RUST 当前设计总方案.md:421`–`:437`：同四层、同核心规则、同四类裁决 | 无冲突 |
| 网关职责 | `ADR-0004:73`–`:87`：统一受控执行入口，校验审批凭证 / 摘要比对 / 幂等 / 审计与 correlation id；首期不拆 crate，作为 `ica-application` 内部服务 | `docs/ICA-RUST 当前设计总方案.md:488`–`:493`：同职责、同部署边界 | 无冲突 |

非阻塞观察（不构成概念冲突，留痕）：

- `ADR-0005:18` 的测试快照数 `335 passed` 与总方案 `:385` 的 `344 passed` 是不同时点的实测数字，非质量门定义差异。
- ADR-0004 §5.1 / §6 新增的「W6 → Policy Engine → Execution Gateway」硬依赖已同步进总方案 §6.9（`:798`–`:816`），不是单方新增、无冲突。

## 三组澄清项答案表（9 条）

| # | ADR | 问题 | 答案（摘要） | 出处 |
| --- | --- | --- | --- | --- |
| 1 | 0004 | Gateway 与既有 `ica-application` 授权设施如何复用 | 无设施可复用；依赖 W6 建立真实权限模型，W6 前降级形态不构成安全边界 | `ADR-0004:146`–`:169`（§5.1）；总方案 `:803`–`:816` |
| 2 | 0004 | correlation id 是否复用 `request_id` / `correlation_id` | 不复用；Rust 与 Python 基线均无该字段，新建独立一等字段，不与 `entity_id` 混同 | `ADR-0004:171`–`:184`（§5.2） |
| 3 | 0004 | IPC 与 HTTP 是否共用同一网关 | 共用 `ica-application` 内同一实现；入口分离，授权 / 审批 / 幂等 / 审计只做一次 | `ADR-0004:186`–`:196`（§5.3） |
| 4 | 0005 | `wave-acceptance/` 目录位置（仓内 vs 仓外） | 仓内：工作树根目录 `wave-acceptance/<Wave>/`，实测 `wave-acceptance/W4/` 已存在 | `ADR-0005:55`–`:65`、`:99`–`:100`；总方案 `:403`–`:410`；磁盘实测 |
| 5 | 0005 | Q4 独立审计员来源与排期 | 非实现方独立审计员；D-1 要求复审不由修复者自宣；人力瓶颈，排期必须预留 | `ADR-0005:47`、`:92`；`ADR-0003:29`、`:68`；`wave-acceptance/W4/acceptance-decision.md` 签署栏 |
| 6 | 0005 | `acceptance-decision.md` 签署格式 | `wave-acceptance/<Wave>/acceptance-decision.md`；含 Decision / 绑定 SHA 的裁决依据 / 放行条件 / 签署栏，署名+日期+结论口径齐备且来源用户本人 | `ADR-0005:64`、`:100`–`:102`；实测 `wave-acceptance/W4/acceptance-decision.md` |
| 7 | 0006 | 锚点 SHA 的单一权威位置 | 契约文档顶部 + 迁移快照两处收敛，其余引用不复制；快照记 `9ed173c`，契约文档仍 `d39e321`（G0-02 未闭合） | `ADR-0006:102`–`:105`；`docs/migration/snapshot-r1.md:14`；总方案 `:745`–`:754` |
| 8 | 0006 | Baseline Diff Report 模板与存放位置 | `docs/migration/`；实测 `baseline-diff-report-r1.md`；六项留痕字段 + Accept/Reject/Deferred/Out of Scope 分类 | `ADR-0006:49`–`:69`、`:81`–`:84`；`docs/migration/baseline-diff-report-r1.md:1`–`:5` |
| 9 | 0006 | 基线变更默认裁决人 | 用户本人（项目所有者 Bryan / BryanTianjx）；D-12 解释权归用户、fail-closed | `ADR-0006:42`–`:47`、`:61`–`:69`、`:94`；`ADR-0003:420`–`:421`；`ADR-0008:51` |

## 验证

工作树内执行，均通过：

```text
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-adr-batch && git diff --check
EXIT=0                                      # 无空白 / 行尾错误

$ grep -n '状态：' docs/adr/ADR-000{4,5,6}-*.md
docs/adr/ADR-0004-ica-runtime-architecture.md:3:- **状态：** **Accepted**
docs/adr/ADR-0005-five-tier-quality-gates.md:3:- **状态：** **Accepted**
docs/adr/ADR-0006-baseline-layers-and-change-handling.md:3:- **状态：** **Accepted**
EXIT=0
```

改动文件（仅 `docs/adr/` 下，LF 行尾，未提交）：三份 ADR + 本核对文档。三份 ADR 的 Accepted 状态未改动。
