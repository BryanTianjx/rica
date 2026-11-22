# ADR-0005 — 五级质量门与不可变验收记录

- **状态：** **Accepted**
- **批准人：** Bryan
- **批准时间：** 2026-11-19T16:54Z（CST 2026-11-20 00:54）
- **批准依据：** 用户明确表态「三份 ADR 批」（2026-11-20 00:54 CST 会话裁决）
- **日期：** 2026-11-19
- **依据：** 《ICA-RUST 当前设计总方案：架构评审与改进建议》§2.4
- **关联：** ADR-0003（治理裁决）

---

## 1. 背景

现状看起来矛盾：

```
scripts/check.sh  四关全绿      →  335 passed / 0 failed
三份独立审计       全部 FAIL      →  W4 不可放行
```

这不是逻辑矛盾，而是**质量门的职责没有区分**。
`check.sh` 是自动化质量门，它**不构成独立验收**。

这个区分如果不写清楚，会产生两种误判：

- 「门禁全绿 → 可以放行」（W4 的实际教训）
- 「审计 FAIL → 门禁坏了」（把审计责任推给 CI）

---

## 2. 决策

### 五级质量体系

| 级别 | 名称 | 内容 | 工具 | 失败含义 |
| --- | --- | --- | --- | --- |
| **Q0** | Build Gate | 编译、依赖、静态检查 | `cargo check` / `clippy -D warnings` / `fmt --check` | 代码不能编译或违反静态规则 |
| **Q1** | Unit Gate | 单元测试 | `cargo test --lib` | 单元行为错误 |
| **Q2** | Integration Gate | API / 数据库 / IPC 集成 | `cargo test --test *` | 组件协作错误 |
| **Q3** | Compatibility Gate | Python/Rust 跨语言对拍 | `scripts/contract_verify.sh` | 已覆盖路径上不等价 |
| **Q4** | Independent Acceptance | 独立审计、缺陷关闭、放行 | 独立审计员 | **不可放行** |

**关键规则：**

1. Q0–Q3 全绿**不构成**放行依据。
2. Q4 必须由**非实现方**执行。
3. Q3 的通过率**不是**等价性证明——它只证明被覆盖路径通过。
   对拍必须附带**覆盖边界声明**（哪些路径 0 命中）。
4. 每次 Q4 产出一份**不可变验收记录**。

### 不可变验收记录

```
wave-acceptance/
  W4/
    implementation-commit.txt      Rust 实现的确切 SHA
    baseline-commit.txt            Python 基线的确切 SHA
    test-results.json              测试计数（实数，非「全绿」）
    contract-coverage.json         契约覆盖 + 覆盖边界
    findings-register.json          缺陷台账（编号/严重度/状态/证据）
    mutation-test-results.json      变异测试结果
    independent-review.md           独立审计结论
    acceptance-decision.md          放行或不放行，含理由与签署
```

**验收结果必须关联准确的 Git SHA，不能只依赖 tag 名称**——
tag 可移动，SHA 不可。

---

## 3. 为什么不直接把四关扩成五关

`check.sh` 保持现状（Q0+Q1+Q2 一体），不拆。
原因是拆开会让 CI 配置复杂化，而 Q4 本来就不该在 CI 里跑。

新增的是**概念分层**与**验收记录格式**，不是新的 CI 流水线。

---

## 4. 后果

**正面**

- 「门禁全绿 ≠ 可放行」变成显式规则，不再靠口头共识。
- 验收记录可审计、可复查，且绑定了确切 SHA。
- 覆盖边界成为强制声明项，堵住「220/222 即等价」的推论漏洞。

**负面**

- 每个波次多一份验收记录目录的维护成本。
- Q4 需要独立审计员，是**人力瓶颈**——排期必须预留。

---

## 5. 与现有 W4 流程的衔接

W4 已经有 `docs/audits/w4-independent/`（三份独立审计报告）。
Q4 落地时，该目录扩展为完整的 `wave-acceptance/W4/` 结构，
并补上 `acceptance-decision.md`（当前尚未有放行决定）。

`w4-done` tag **只是 Q4 通过的标记**，不是 Q4 的证据。

---

## 批准前澄清项（已答）

草稿（ADR-BATCH v1「待澄清项汇总」）就 ADR-0005 列的三问，逐条答案与出处如下。ADR 正文未直接写明「仓内 / 仓外」，该问按正文的相对路径加磁盘实测判定，并如实标注。

| 问题 | 答案 | 出处 |
| --- | --- | --- |
| `wave-acceptance/` 目录的确切位置（仓内 vs 仓外）？ | **仓内**：仓库工作树根目录下的 `wave-acceptance/<Wave>/`。本文 §2 的目录树与总方案 §4.7 均按此相对路径书写；磁盘实测 `wave-acceptance/W4/` 已存在（含 `implementation-commit.txt`、`baseline-commit.txt`、`independent-review.md`、`acceptance-decision.md` 等）。 | 本文 §2 L55–65、§5 L96–100；`docs/ICA-RUST 当前设计总方案.md:403–410`；磁盘实测 `wave-acceptance/W4/` |
| Q4 独立审计员的来源与排期（人力瓶颈）？ | 来源必须是**非实现方的独立审计员**：本文规则 2「Q4 必须由非实现方执行」，D-1 追加「独立审计员重新审查，不由修复者自行宣布通过」。排期：Q4 是人力瓶颈，排期必须预留；W4 实测由 `docs/audits/w4-independent/` 三份报告承担两轮独立审计。 | 本文 §2 L44–50、§4 L92；ADR-0003 D-1 L29、L68；实测 `wave-acceptance/W4/acceptance-decision.md` 签署栏 |
| `acceptance-decision.md` 的签署格式？ | 文件名固定为 `acceptance-decision.md`，置于 `wave-acceptance/<Wave>/`；内容含 Decision（放行 / 不放行）、裁决依据（每条绑定确切 SHA 而非 tag）、持续有效的放行条件、签署栏（实现+复验 / 独立审计 / 放行依据）。署名、日期、结论口径三栏齐备，来源为用户本人。 | 本文 §2 L64、§5 L96–102；实测 `wave-acceptance/W4/acceptance-decision.md`；`docs/architecture/browser/b0-deliverables-r1.md:59–60`（裁决署名口径） |
