# ADR-0006 — 基线的四层概念与变更处置流程

- **状态：** **Accepted**
- **批准人：** Bryan
- **批准时间：** 2026-11-19T16:54Z（CST 2026-11-20 00:54）
- **批准依据：** 用户明确表态「三份 ADR 批」（2026-11-20 00:54 CST 会话裁决）
- **日期：** 2026-11-19
- **依据：** 《ICA-RUST 当前设计总方案：架构评审与改进建议》§2.3
- **关联：** ADR-0003（裁决 D-1：重锚到 `9ed173c`）

---

## 1. 背景

2026-11-19 一天内，Python 基线 HEAD 动了两次：

```
d39e321  →  20b252d  →  9ed173c "freeze: Python 基线终态"
```

「终态」是基线方的自我声明，**不是机制保证**。

裁决 D-1 定了「重锚到 `9ed173c`」，但重锚之后契约文档第 6 行仍写 `d39e321`
（G0-02 未完成）。这暴露了一个概念混淆：**基线**这个词在当前工程里
同时指四样不同的东西。

如果不分清，会出现两类错误推论：

- 「重锚后对拍仍是 220/222 → 三处差异不影响契约」
  （错：只能说明当前样本没产生新差异）
- 「基线变了 → 契约自动跟着变」
  （错：契约是人工裁决的产物，不自动跟随）

---

## 2. 决策

### 四层基线概念

| 概念 | 含义 | 谁决定它变 | 变了的处理 |
| --- | --- | --- | --- |
| **Baseline Anchor** | Python 源码事实锚点（git SHA） | 基线仓库的自然演进 | 产生 Baseline Diff Report → 人工裁决 |
| **Contract Baseline** | 冻结的业务契约（68 条端点 + 波次 + declared improvements） | **人工裁决** | 走 Accept / Reject / Deferred / Out of Scope |
| **Test Fixture Baseline** | 对拍样本与测试数据 | 种子脚本 + 契约用例 | 随契约变更更新 |
| **Implementation Baseline** | Rust 当前实现版本（git SHA） | 每次提交 | 自动，绑定验收记录 |

**核心规则：Python 基线更新不自动改变冻结契约。**

### 基线变更的处置流程

每次基线 HEAD 变化时，产生一份 **Baseline Diff Report**，
由人工按下表逐项裁决：

| 裁决 | 含义 | 后果 |
| --- | --- | --- |
| **Accept** | 接受基线的行为变化 | 更新 Contract Baseline + Test Fixture + 迁移计划 |
| **Reject** | 不接受基线的行为变化 | 登记为 known divergence；Rust 保持冻结契约行为 |
| **Deferred** | 暂缓处理 | 记入待办台账，不影响当前波次放行 |
| **Out of Scope** | 明确不属于迁移范围 | 排除出契约表并记录理由 |

### 裁决记录必须留痕

每份 Baseline Diff Report 至少记录：

- 基线旧 SHA 与新 SHA。
- 受影响的契约端点编号。
- 每项差异的分类（Accept / Reject / Deferred / Out of Scope）。
- 裁决人与日期。
- 对 Rust 实现的影响面。

**报告是审计对象**——已落库的结论不因后续发现错误而改写，
而是另发更正声明。

---

## 3. 与 D-1 的衔接

D-1 已裁决「重锚到 `9ed173c`」，这属于**已做出的 Accept 决定**，
但配套的文档改写尚未完成（契约文档 §1 与第 6 行仍写 `d39e321`）。

因此 G0-02 的产出是：
1. 一份 `d39e321 → 9ed173c` 的 Baseline Diff Report；
2. 三处差异的裁决记录（W7 承接）；
3. 契约文档第 6 行与 §1.1 的改写。

---

## 4. 后果

**正面**

- 基线漂移变成**受控流程**，不再是无声的变化。
- 「重锚」这类动作有明确记录，不再只是改一行文档。
- 契约的变更权明确在人，不由 git 自动决定。

**负面**

- 基线每次变动都增加一道人工裁决，成本上升。
- 需要维护「基线当前 SHA」的单一权威位置，否则又会出现
  文档各处写的 SHA 不一致。

**缓解**

- 把当前锚点 SHA 收敛到一处（建议契约文档顶部 + 迁移快照文件），
  其余位置引用它而不复制。

---

## 批准前澄清项（已答）

草稿（ADR-BATCH v1「待澄清项汇总」）就 ADR-0006 列的三问，逐条答案与出处如下。

| 问题 | 答案 | 出处 |
| --- | --- | --- |
| 当前锚点 SHA 的单一权威位置定在哪？ | 收敛到一处：**契约文档顶部**（`docs/rust-migration-contract.md` 的 Contract authority）+ **迁移快照**（`docs/migration/snapshot-r1.md`），其余位置只引用不复制。快照已记录 `9ed173c`；契约文档顶部与 §1.1 仍写 `d39e321`，总方案 §6.1 标为**未闭合**（G0-02 待改写）——这是待办遗留，不是两份 ADR 之间的答案冲突。 | 本文 §4「缓解」L102–105；`docs/migration/snapshot-r1.md:14`；`docs/ICA-RUST 当前设计总方案.md:745–754`；ADR-0003 D-1（L27–30） |
| Baseline Diff Report 的模板与存放位置？ | 存放于 `docs/migration/`；实测实例 `docs/migration/baseline-diff-report-r1.md`（G0-02 交付物）。模板字段即本文 §2「裁决记录必须留痕」六项：旧 / 新 SHA、受影响端点编号、逐项分类（Accept / Reject / Deferred / Out of Scope）、裁决人与日期、对 Rust 实现的影响面；D-1 的 `d39e321 → 9ed173c` 三处差异裁决记录在该报告内。 | 本文 §2 L49–69、§3 L81–84；实测 `docs/migration/baseline-diff-report-r1.md:1–5` |
| 谁是基线变更的默认裁决人？ | **用户本人**（项目所有者，署名 Bryan / BryanTianjx）。本文规定 Contract Baseline 由人工裁决、每份报告记裁决人与日期、契约变更权明确在人；D-12 进一步规定该规则的解释权归用户，判断不准时 fail-closed，执行方不得自行扩大授权。 | 本文 §2 L42–47、L61–69、§4 L94；ADR-0003 D-12 L420–421；实测同族裁决署名 `docs/adr/ADR-0008-endpoint-scope-and-noncontract-deltas.md:51` |
