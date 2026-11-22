# ICA-RUST ADR-ALIGN — ADR-0004/0005/0006 对齐评估

- 任务单：`wi_7fce38198f8d4320` / `ICA-RUST-ADR-ALIGN`（IN_PROGRESS）
- 承接：ADR-BATCH（`wi_08408b4cdc8647a0`）的 **AC4 缺口**（该 AC 当前状态 `FAILED`）
- 核对对象：`docs/adr/ADR-0004-ica-runtime-architecture.md`、`docs/adr/ADR-0005-five-tier-quality-gates.md`、`docs/adr/ADR-0006-baseline-layers-and-change-handling.md`（三份均 **Accepted**，本卡不改其状态）
- 评估环境：worktree `/mnt/o/ICA-RUST/.worktrees/ica-rust-adr-align` @ `6a7c0f2`，分支 `wt/ica-rust-adr-align`
- 数据源：`/home/bryan/.hermes/woticketflow/state.db`（**只读连接** `file:…?mode=ro`）；本卡对该库的净写入 = 0（会话期内该库被**并发会话**改动过，归因实测见 §6.2）
- Python 基线（只读参照）：`/home/bryan/projects/ica` @ `9ed173c`（工作树干净）；`/mnt/o/ica` 在本机**不是 git 仓库**（无 `.git`），故以 `/home/bryan/projects/ica` 为基线核对源
- 方法：纯评估 + 仅新增本文档；**未改任何卡的草稿/审批、未改 `state.db`、未改三份 ADR、未 `git add/commit/push`**

---

## 0. 一句话结论

五张卡**全部**草稿（10 个版本）对 ADR-0004/0005/0006 的**字面引用为零**（任务单指定的 grep 全部 `exit 1`）；但**概念实质依赖集中在 G0 与 W4-A 两张已 COMPLETED 卡**——它们独立地推导出了 ADR-0005 / ADR-0006 的核心结论（Q4 独立验收、覆盖边界、变异测试；重锚、覆盖边界声明、锚点 sha 留痕），只是**没有引用 ADR 编号**；**ADR-0004 在五张卡中无任何实质依赖**。**在途卡仅 G3 一张**（BW0 已于 2026-11-22 收口为 COMPLETED，ADR-BATCH 核对文档记载的「IN_PROGRESS」已过期），且 G3 的 7 条 AC 全部 `FAILED`、卡已 `BLOCKED`——**当前不存在一张「草稿确有实质冲突且仍在途」的卡**。

---

## 1. AC1 — 五卡逐卡评估：草稿对 ADR-0004/0005/0006 的实质依赖

### 1.1 取数命令原文（可复跑）

```bash
# 0) 定位 state.db 与全部草稿版本（只读）
python3 - <<'EOF'
import sqlite3
con = sqlite3.connect("file:/home/bryan/.hermes/woticketflow/state.db?mode=ro", uri=True)
cur = con.cursor()
CARDS = [("B0","wi_0217e2addf5641d2"),("G3","wi_54c59a45766045bd"),
         ("BW0","wi_5e8fe402c22a4e5e"),("G0","wi_2ee2b6dc860b4cdf"),
         ("W4-A","wi_8362423d9e5743a1")]
for label, wi in CARDS:
    print("==", label, wi)
    for v, dg, ca, cb, content in cur.execute(
        "SELECT version,draft_digest,created_at,created_by,draft_content "
        "FROM work_item_versions WHERE work_item_id=? ORDER BY version", (wi,)):
        print(f"  v{v} digest={dg} created_at={ca} created_by={cb} lines={len(content.splitlines())}")
EOF

# 1) 把全部版本正文导出到仓外工作区（只读，不写 state.db）
#    → /home/bryan/.hermes/cache/scratch/adr-align/drafts/{B0,G3,BW0,G0,W4A}-v*.md

# 2) 任务单指定的引用检查（AC1 原文的 grep pattern）
cd /home/bryan/.hermes/cache/scratch/adr-align/drafts
grep -nE 'ADR-0004|ADR-0005|ADR-0006|执行网关|五级质量门|基线四层|Policy Engine|correlation' \
     B0-v1.md B0-v2.md G3-v1.md BW0-v1.md G0-v1.md G0-v2.md G0-v3.md \
     W4A-v1.md W4A-v2.md W4A-v3.md
echo "EXIT=$?"

# 3) 对照组：同一 pattern 打 ADR-BATCH v1（应当命中，用来证明 pattern 本身有效）
grep -nE 'ADR-0004|ADR-0005|ADR-0006|执行网关|五级质量门|基线四层|Policy Engine|correlation' \
     ADRBATCH-v1.md
echo "EXIT=$?"

# 4) 概念级探针（绕过字面关键词，检出实质依赖）
grep -nEi '网关|Gateway|审批凭证|幂等|受控工具|绕过.*ica-application|权限|correlation|Policy' \
     B0-v2.md G3-v1.md BW0-v1.md G0-v3.md W4A-v3.md
grep -nEi '质量门|独立验收|独立审计|wave-acceptance|acceptance-decision|覆盖边界|不可变|Q[0-4]|不可放行|变异测试|门禁' \
     B0-v2.md G3-v1.md BW0-v1.md G0-v3.md W4A-v3.md
grep -nEi '四层|Baseline Anchor|Contract Baseline|Test Fixture|Implementation Baseline|Diff Report|Accept|Reject|Deferred|Out of Scope|锚点|冻结契约|基线漂移|重锚' \
     B0-v2.md G3-v1.md BW0-v1.md G0-v3.md W4A-v3.md
```

### 1.2 字面引用检查结果：零命中

```text
$ cd /home/bryan/.hermes/cache/scratch/adr-align/drafts
$ grep -nE 'ADR-0004|ADR-0005|ADR-0006|执行网关|五级质量门|基线四层|Policy Engine|correlation' \
       B0-v1.md B0-v2.md G3-v1.md BW0-v1.md G0-v1.md G0-v2.md G0-v3.md \
       W4A-v1.md W4A-v2.md W4A-v3.md
EXIT=1                    # ← 零命中（grep 无匹配即 exit 1）

$ # 对照组 ADR-BATCH v1（同 pattern）
ADRBATCH-v1.md:2:# ICA-RUST ADR-BATCH — ADR-0004 / 0005 / 0006 的批准与落地
ADRBATCH-v1.md:7:ADR-0004  ICA Runtime 架构边界与统一执行网关（Execution Gateway）
ADRBATCH-v1.md:8:ADR-0005  五级质量门与不可变验收记录
ADRBATCH-v1.md:9:ADR-0006  基线的四层概念与变更处置流程
ADRBATCH-v1.md:19:| ADR-0004 | B0（执行边界）、G3（Agent 权限）、BW0（Browser 执行） |
ADRBATCH-v1.md:20:| ADR-0005 | 全部波次的验收判据（尤其 W4-A） |
ADRBATCH-v1.md:21:| ADR-0006 | G0（基线治理）、W4-A（放行口径） |
ADRBATCH-v1.md:33:### ADR-0004
ADRBATCH-v1.md:36:- correlation id 是否复用现有 `request_id` / `correlation_id`，还是新引入？
ADRBATCH-v1.md:39:### ADR-0005
ADRBATCH-v1.md:45:### ADR-0006
EXIT=0                    # ← pattern 本身有效
```

**逐卡命中数（`work_item_versions.draft_content` 正则扫描，DB 内直接计数）**：

| 卡 | work_item_id | 版本 | 各版本 ADR-keyword 命中 | 合计 |
| --- | --- | --- | --- | --- |
| B0 | `wi_0217e2addf5641d2` | v1, v2 | 0, 0 | **0** |
| G3 | `wi_54c59a45766045bd` | v1 | 0 | **0** |
| BW0 | `wi_5e8fe402c22a4e5e` | v1 | 0 | **0** |
| G0 | `wi_2ee2b6dc860b4cdf` | v1, v2, v3 | 0, 0, 0 | **0** |
| W4-A | `wi_8362423d9e5743a1` | v1, v2, v3 | 0, 0, 0 | **0** |
| ADR-BATCH（对照） | `wi_08408b4cdc8647a0` | v1 | 10 | 10 |

**结论**：五张卡的草稿正文**从未引用** ADR-0004/0005/0006 的编号或专有名词。这与 ADR-BATCH 核对文档 §AC4 的记载一致（「均无 ADR-0004 执行边界 / 统一网关口径」等）。

### 1.3 概念级实质依赖（逐卡、逐条、给出出处）

字面零引用 ≠ 零依赖。下表逐条给出**草稿实际写了什么**、**它对应哪条 ADR 概念**、**出处（work_item_id + 版本号 + 行号 + 原句）**。行号取自 `work_item_versions.draft_content` 的 1-based 行号。

#### B0 — `wi_0217e2addf5641d2` v2（249 行）

| 草稿写了什么 | 对应 ADR 概念 | 出处（v:行） |
| --- | --- | --- |
| 「没有平台准入结论，不进入真实招聘平台的自动化开发」 | ADR-0004 §2 原则 4 的**门禁形态**：Browser Runtime 能力必须先有准入结论才能启用 | v2:L6、v2:L103 |
| 「不新增第八个 crate（方案 §3 明确「不新增第八个 crate」）」 | ADR-0004 §2「**首期不单独部署、不新增 crate**——先作为 `ica-application` 内部的应用服务实现」 | v2:L104 |
| 「D-7 · Hermes 发送权限 —— Hermes 不获得直接发送工具或执行权限」 | ADR-0004 §2 原则 3「Hermes 只能通过 ICA 提供的受控工具访问业务能力」 | v2:L189 |
| 「`Unknown` 必须进入人工核验，不能自动重试」 | ADR-0004 §2 Execution Gateway「校验一次性审批凭证（未过期、未撤销、已绑定内容）」 | v2:L198 |
| 「但必须先有字段级差异报告，再决定扩展还是新建」 | ADR-0006 §2「每份 Baseline Diff Report 至少记录…裁决人与日期」 | v2:L159 |
| 「W7 与 B4 不得各自维护 Candidate Capture 的状态机、去重规则和持久化结构」 | ADR-0004 §2 原则 1「ICA Application 是唯一业务规则执行主体」 | v2:L242 |
| 「不把 Browser 测试数计入 68 条 API 对拍统计」 | ADR-0006 §2「Contract Baseline…**人工裁决**」（测试统计不自动跟随范围变化） | v2:L105 |
| 「B0-06 冻结 ADR-BROWSER-001…010」 | **不是** ADR-0004/0005/0006——是另一组浏览器 ADR（另立卡面），本卡三份不在其列 | v2:L33 |

**B0 判定**：与 ADR-0004 §2 的四条原则有**部分同构但不冲突**的表述（B0 门禁、第八 crate 禁令、Hermes 无发送工具）。但**均来自《ICA Browser Subsystem Architecture R1.0》方案原文与 ADR-0003 裁决 D-6/D-7/D-9/D-11**，不是从 ADR-0004 推导。**无实质冲突**；**无实质依赖**（ADR-0004 未定义 Browser 的准入门禁细节，B0 的门禁不是 ADR-0004 的产物）。

#### G3 — `wi_54c59a45766045bd` v1（85 行）

| 草稿写了什么 | 对应 ADR 概念 | 出处（v:行） |
| --- | --- | --- |
| 「H-POC-04 \| Gateway / JSON-RPC」 | **术语碰撞**：此「Gateway」指 **Hermes 自身的 messaging/JSON-RPC 网关**，与 ADR-0004 的 **ICA Execution Gateway** 是两个不同东西 | v1:L38、v1:L50 |
| 「必须记录实际安装版本支持的 RPC、Gateway owner 能力声明、认证方式和 MCP 传输能力」 | ADR-0004 §2 原则 2「每次执行都重新判，不信任上游声明」的**同向精神**（以实测能力为准，不信任文档声明） | v1:L48 |
| 「H-POC-08 \| 审批事件 \| 能接收审批请求并响应」 | ADR-0004 §2 Execution Gateway「校验一次性审批凭证」的**上游事件面** | v1:L42 |
| 「不开放除 `ica.ping` 以外的任何 MCP 工具」 | ADR-0004 §2 原则 3「MCP 工具白名单**不是**安全边界，只是能力入口」——G3 的口径**恰好落在白名单层**，未进入授权层 | v1:L57 |
| 「H-POC-09 \| 异常恢复 \| Hermes 崩溃后 ICA 核心业务不受影响」 | ADR-0004 §2「不让 Browser Runtime 拥有 SQLite 凭据」类**故障隔离**原则的同向表述 | v1:L43 |
| 「不让 Agent 集成反向改变迁移验收口径」 | ADR-0005 §2 规则 1「Q0–Q3 全绿**不构成**放行依据」同向 | v1:L84 |
| 「/mnt/o/ica HEAD \| `20b252d`（**非** `d39e321`）…」 | ADR-0006 §1 背景「2026-11-19 一天内 Python 基线 HEAD 动了两次」——**但草稿记录的是 ADR-0006 成文之前的一次中间态** | v1:L76 |

**G3 判定**：**最接近 ADR-0004 的一张卡，但依赖方向相反**——ADR-0004 把 Agent 侧放在「受控工具 + 授权」框架下，G3 v1 只做**运行时能力 POC**（`ica.ping` 单工具、无业务数据）。ADR-0004 §5.1/§5.2 实测结论明确「Gateway 的 Policy Engine 无设施可复用」「correlation id 不存在、需新建」——**G3 的草稿口径与该结论相容，无冲突**。**但 G3 草稿未提及 ADR-0004 的任何结论**，一旦 ADR-0004 落地，G3 恢复实施时必须遵守它。

#### BW0 — `wi_5e8fe402c22a4e5e` v1（55 行）

| 草稿写了什么 | 对应 ADR 概念 | 出处（v:行） |
| --- | --- | --- |
| 「\| B \| 现有会话、任务、连接器、标签页、**权限**等 API」 | ADR-0004 §2 原则 2 的**对象**（权限）出现在 BW0 迁移面内 | v1:L27 |
| 「不新建平行的数据模型（复用现有 12 张 browser 表）」 | ADR-0004 §2 原则 1「ICA Application 是唯一业务规则执行主体」+ ADR-0006 §2「Contract Baseline 人工裁决」的**同一诉求**（不平行建模） | v1:L53 |
| 「不在 B0 盘点结论出来前动手（范围未定）」 | ADR-0006 §2「**Python 基线更新不自动改变冻结契约**」的同向（范围先裁决后动手） | v1:L54 |
| 「`/candidate-extractions*` 这 5 条是 `confirm-candidate` 的**上游**」 | ADR-0006 §2「Baseline Anchor 变化 → 产生 Baseline Diff Report → 人工裁决」的**归属裁决形态** | v1:L46 |
| 「不把任何一条塞进原 68 条契约」 | ADR-0006 §2「Contract Baseline…**人工裁决**」 | v1:L52 |

**BW0 判定**：全部口径来自 **ADR-0003 裁决 D-3 / D-11 / D-9**，与 ADR-0004/0005/0006 **无冲突**。BW0 的迁移面确实包含「权限」路由，而 ADR-0004 要求 Browser 侧执行受网关管控——这是**后继工作义务**，不是草稿冲突。

#### G0 — `wi_2ee2b6dc860b4cdf` v3（172 行）★ ADR-0006 概念最密集

| 草稿写了什么 | 对应 ADR 概念 | 出处（v:行） |
| --- | --- | --- |
| 「**D-1 基线重锚**：继续用 `d39e321`，还是把新的 Python 行为纳入下一版契约？」 | ADR-0006 §2 四层「Baseline Anchor（谁决定它变）→ Contract Baseline（人工裁决）」 | v3:L40 |
| 「选择 **B：重锚到 `9ed173c`，三处差异另立波次承接。**」 | ADR-0006 §3「D-1 已裁决『重锚到 `9ed173c`』，这属于**已做出的 Accept 决定**」 | v3:L132 |
| 「三处行为差异的**实现**不在 G0：转入新卡 `ICA-RUST-W7`」 | ADR-0006 §2 裁决分类「**Deferred**（记入待办台账，不影响当前波次放行）」 | v3:L162 |
| 「G0-03（端点范围 ADR）仍需就 4 条 `ai/models` 独立裁决」 | ADR-0006 §2「Contract Baseline…**人工裁决**」+ §2 裁决四分类 | v3:L163 |
| 「不擅自把 `/mnt/o/ica` 当前工作树当作新的对拍基线（见 G0-02：那是用户决策）」 | ADR-0006 §2 核心规则「**Python 基线更新不自动改变冻结契约**」 | v3:L74 |
| 「**结论：换成新锚点后仍是 220/222、仍是那 2 条 dashboard。**」 | ADR-0006 §2「Baseline Anchor 变化 → 产生 Baseline Diff Report → 人工裁决」 | v3:L154 |
| 「「三处差异」并没有被现有用例检验到并失败——它们是**用例集之外的空白**，不是已知失败」 | ADR-0006 §1 的**错误推论反例**：「重锚后对拍仍是 220/222 → 三处差异不影响契约」（**错**） | v3:L155 |
| 「即 **220/222 是真实数字，但它的保证范围小于「W4 契约面完全等价」**」 | ADR-0005 §2 规则 3「Q3 的通过率**不是**等价性证明——它只证明被覆盖路径通过」 | v3:L120 |
| 「「终态」是基线方的自我声明，不是机制保证。G0 的迁移快照必须记录锚点 sha」 | ADR-0006 §1「『终态』是基线方的自我声明，**不是机制保证**」+ §4「把当前锚点 SHA 收敛到一处」 | v3:L169 |
| 「后续任何波次的验收结论都要注明它是对哪个 sha 测的——否则同名的「220/222」会指向两个不同的基线」 | ADR-0006 §4 缓解措施「单一权威位置，其余位置引用它而不复制」 | v3:L171 |
| 「G0-05 \| 把 W4 独立审计结论纳入基线记录」 | ADR-0005 §2 规则 4「每次 Q4 产出一份**不可变验收记录**」 | v3:L31 |
| 「`w4-done` 应在独立审计通过后创建，不能先补 tag 再补证据」 | ADR-0005 §5「`w4-done` tag **只是 Q4 通过的标记**，不是 Q4 的证据」 | v3:L50 |

**G0 判定（关键）**：**ADR-0006 的全部核心结论，G0 v3 已经独立地写出来了**——重锚裁决、冻结契约不自动跟随、覆盖边界声明、锚点 sha 单一权威、差异归属分类（Deferred）。这些内容写于 2026-11-19 15:53Z，**早于 ADR-0006 批准 16:54Z 约 1 小时**。**因此 G0 的草稿与 ADR-0006 是「同一结论的两条独立来源」，不是冲突**——**对齐缺口是「引用」缺口，不是「语义」缺口**。

#### W4-A — `wi_8362423d9e5743a1` v3（171 行）★ ADR-0005 概念最密集

| 草稿 wrote | 对应 ADR 概念 | 出处（v:行） |
| --- | --- | --- |
| 「对 `w4-candidate` 进行独立审计，不能仅凭自报测试数字补发 `w4-done`」 | ADR-0005 §2 规则 2「Q4 必须由**非实现方**执行」+ 规则 1「Q0–Q3 全绿不构成放行依据」 | v3:L16 |
| 「`w4-done` 应在独立审计通过后创建，不能先补 Tag 再补证据」 | ADR-0005 §5「`w4-done` tag 只是 Q4 通过的标记，不是 Q4 的证据」 | v3:L17 |
| 「复跑跨语言对拍与质量门，**取真实数字**」 | ADR-0005 §2 Q0–Q3 的分级定义（`check.sh` = Q0+Q1+Q2，对拍 = Q3） | v3:L26 |
| 「7. **测试样本、Python 基线 commit、Rust commit 全部记录**」 | ADR-0005 §2「验收结果必须关联准确的 Git SHA，不能只依赖 tag 名称——tag 可移动，SHA 不可」 | v3:L144 |
| 「8. **独立审计员重新审查**，不由修复者自行宣布通过」 | ADR-0005 §2 规则 2「Q4 必须由非实现方执行」 | v3:L145 |
| 「**220/222 不能继续作为 W4 等价性证明。** 它只能证明原来被覆盖的那些路径通过，不能证明未覆盖路径不存在差异」 | ADR-0005 §2 规则 3「Q3 的通过率**不是**等价性证明」+「对拍必须附带**覆盖边界声明**」 | v3:L149 |
| 「「全绿」在这里是覆盖不足的表现，不是等价性的证明」 | ADR-0005 §1 背景「『门禁全绿 → 可以放行』（W4 的实际教训）」 | v3:L163 |
| 「**因此复审必须包含变异测试**：把修复改回去，用例必须变红」 | ADR-0005 §2 目录树 `mutation-test-results.json`（变异测试是 Q4 记录的一部分） | v3:L170 |
| 「AC6 仍写「preview 零写入」—— 实测基线 `preview_import` 确实写…」 | ADR-0005 §2 规则 3 的配套：判据与实测冲突时按实测口径判 | v3:L36 |
| 「AC7 要求 `/mnt/o/ica` HEAD 为 `d39e321` 且工作树为空」 | ADR-0006 §1 背景（基线一天内动两次） | v3:L40 |

**W4-A 判定（关键）**：**ADR-0005 的全部四条关键规则，W4-A v3 已独立写齐**——Q0–Q3 不等于放行、Q4 非实现方、SHA 绑定（不可依赖 tag）、覆盖边界声明、变异测试。**语义上完全对齐**，缺口同样只在「引用」层面。

### 1.4 AC1 小结

| 卡 | ADR-0004（Runtime 边界/执行网关） | ADR-0005（五级质量门） | ADR-0006（基线四层） | 实质依赖 | 是否冲突 |
| --- | --- | --- | --- | --- | --- |
| B0 | 部分同构（门禁/不新增 crate/Hermes 无发送工具） | 无 | 弱（字段级差异报告） | **无**（来源是方案原文+D-3/D-4/D-6/D-7/D-9/D-11） | **否** |
| G3 | 术语碰撞（Hermes Gateway ≠ Execution Gateway）；概念相邻 | 弱（不让 Agent 反改验收口径） | 弱（基线 HEAD 中间态） | **无** | **否** |
| BW0 | 弱（迁移面含「权限」路由） | 无 | 弱（不平行建模/范围先裁决） | **无** | **否** |
| G0 | 无 | 弱（Q4 独立审计归档） | **强**（重锚/冻结契约不自动跟随/覆盖边界/锚点单一权威/差异分类） | **强（语义已对齐，引用缺失）** | **否** |
| W4-A | 无 | **强**（Q0–Q3≠放行/Q4 非实现方/SHA 绑定/覆盖边界/变异测试） | 弱（基线漂移登记） | **强（语义已对齐，引用缺失）** | **否** |

**AC1 判定：PASS** — 五卡逐卡评估完成，每条都给出「引用了哪些 ADR 概念 + 出处（版本号/行号）」；**结论是「零字面引用、零实质冲突、两张卡有强语义同源（G0↔ADR-0006、W4-A↔ADR-0005）」**。

---

## 2. AC2 — 在途卡的需出新草稿条目与重批影响（只评估，不擅改）

### 2.1 在途状态核定（以 `work_items.state` 为准，非 ADR-BATCH 旧文档）

```bash
python3 $HOME/.hermes/skills/software-development/woticketflow/scripts/woticketflow_cli.py work-item show <id>
```

| 卡 | work_item_id | 当前 state | 判定 |
| --- | --- | --- | --- |
| B0 | `wi_0217e2addf5641d2` | `COMPLETED`（2026-11-22T13:58:33Z 收口） | 已终态 |
| **G3** | `wi_54c59a45766045bd` | **`BLOCKED`**（2026-11-22T13:29:35Z；`resume_state=REVIEW`） | **唯一在途卡** |
| BW0 | `wi_5e8fe402c22a4e5e` | `COMPLETED`（2026-11-22T14:44:36Z 收口） | 已终态 |
| G0 | `wi_2ee2b6dc860b4cdf` | `COMPLETED`（2026-11-22T13:58:33Z 收口） | 已终态 |
| W4-A | `wi_8362423d9e5743a1` | `COMPLETED`（2026-11-22T13:58:32Z 收口） | 已终态 |

> **ADR-BATCH 核对文档的过期陈述**：`docs/adr/adr-batch-acceptance-check.md:57` 记 BW0 为「IN_PROGRESS」——该记载已被 2026-11-22T14:44:36Z 的 COMPLETED 取代。这与该文档 `:8` 自己声明的「过期陈述…交编排方处理」同类，属**留痕更正**，本卡据实更新。

### 2.2 G3 需出新草稿的具体条目

G3 v1 草稿与 ADR-0004 的**语义无冲突**（§1.3），但**缺三处 ADR-0004 的硬结论**，在 G3 恢复实施（BLOCKED → READY）时必须补上。三条均出自 ADR-0004 §5.1/§5.2/§6（2026-11-20 实测），是**已固化的工程事实**，补进去不改变 G3 的范围与验收标准：

| # | 需在新草稿中补的条目 | ADR 出处 | G3 v1 现状 | 补入后的净效果 |
| --- | --- | --- | --- | --- |
| G3-N1 | 「本 POC 的 `ica.ping` 属**受控工具入口**，不构成安全边界；真实权限判定由 Execution Gateway 的 Policy Engine 承担，且其完整实现硬依赖 W6」 | ADR-0004 §2 原则 3 + §5.1（L146–169） | v1:L57 只写「不开放除 `ica.ping` 以外的任何 MCP 工具」，停在白名单层 | **范围不变、验收标准不变**；补一条「不得以本 POC 证明权限隔离」的说明 |
| G3-N2 | 「correlation id 当前**不存在**，需新建为独立一等字段，不得与 `entity_id` 混同；存放位置随 W6 后的 schema 变更决定」 | ADR-0004 §5.2（L171–184） | 草稿**完全未提** correlation id | 纯事实登记，无判据变化 |
| G3-N3 | 「W6 完成前 Gateway 只能以单用户降级形态存在，**不得在任何验收结论中被引用为『权限隔离已实现』**」 | ADR-0004 §6（L210–225） | 草稿**完全未提** | 纯约束登记；直接约束 G3 的交付说明措辞 |

**此外需在草稿中澄清一处术语**（非 ADR 引入，是 G3 自身歧义）：v1:L38 / L50 的「Gateway」指 **Hermes 自身的 JSON-RPC 网关**，与 ADR-0004 的 **ICA Execution Gateway** 无关。新草稿应改称「Hermes Gateway（产品名）」以免与 ADR-0004 的「Execution Gateway」混用。

### 2.3 重批影响（实测，不靠推断）

**实测方法**：复制 `state.db`（含 `-wal`/`-shm`）到仓外 scratch 副本，在**副本**上跑 `Runtime.create_draft` / `approve` / `transition`；**真实 `state.db` 全程只读**（sha256 见 §6）。

```text
=== BEFORE（scratch 副本）===
  [B0/COMPLETED]    state=COMPLETED  acceptable=True   cond7_authorized=True
  [BW0/COMPLETED]   state=COMPLETED  acceptable=True   cond7_authorized=True
  [G0/COMPLETED]    state=COMPLETED  acceptable=True   cond7_authorized=True
  [W4A/COMPLETED]   state=COMPLETED  acceptable=True   cond7_authorized=True
  [G3/BLOCKED]      state=BLOCKED    acceptable=False  cond7_authorized=True
      gaps={"2_no_blocking_result":"7 条标准最新结果为阻塞","3_required_evidence_valid":"9 项必需证据缺失"}

=== EXPERIMENT：给每张卡出一条新草稿版本（仅副本）===
  B0/COMPLETED:    create_draft OK -> v3
      state=COMPLETED  acceptable=False  cond7_authorized=False
      gaps={"7_authorized":"无绑定当前草稿的 APPROVED 记录"}
  BW0/COMPLETED:   create_draft OK -> v2
      state=COMPLETED  acceptable=False  cond7_authorized=False
      gaps={"7_authorized":"无绑定当前草稿的 APPROVED 记录"}
  G0/COMPLETED:    create_draft OK -> v4
      state=COMPLETED  acceptable=False  cond7_authorized=False
      gaps={"7_authorized":"无绑定当前草稿的 APPROVED 记录"}
  W4A/COMPLETED:   create_draft OK -> v4
      state=COMPLETED  acceptable=False  cond7_authorized=False
      gaps={"7_authorized":"无绑定当前草稿的 APPROVED 记录"}
  G3/BLOCKED:      create_draft OK -> v2
      state=BLOCKED    acceptable=False  cond7_authorized=False
      gaps={...,"7_authorized":"无绑定当前草稿的 APPROVED 记录"}

=== 能否用新草稿重新审批？===
  B0: approve REFUSED -> RuntimeError_: 审批前置条件不满足：Work Item 状态为 COMPLETED，需先进入 APPROVAL_PENDING（DRAFTED → APPROVAL_PENDING）
  G3: approve REFUSED -> RuntimeError_: 审批前置条件不满足：Work Item 状态为 BLOCKED，需先进入 APPROVAL_PENDING（DRAFTED → APPROVAL_PENDING）

=== 能否迁移到 APPROVAL_PENDING？===
  B0: transition REFUSED -> TransitionError: 非法迁移 COMPLETED → APPROVAL_PENDING；合法目标：['REOPEN_REQUESTED']
  G3: transition REFUSED -> TransitionError: 非法迁移 BLOCKED → APPROVAL_PENDING；合法目标：['CANCELLED','FAILED','IN_PROGRESS','READY']
```

**重批影响的机器结论**：

1. **出草稿本身可做**（`create_draft` 对任何状态的卡都能写）——所以「改草稿会使旧审批自动失效」不是自动发生的，而是**在出草稿那一刻就发生**。
2. **出草稿即导致条件 7 翻红**：验收硬条件 7「有人工批准，绑定当前草稿且未过期」判 `False`，gaps 报「无绑定当前草稿的 APPROVED 记录」。**四张 COMPLETED 卡的可重开性在账上直接消失**（`acceptable` 从 `True` 翻成 `False`），**即使它们早已收口**。
3. **新草稿无法就地重批**：`approve()` 要求状态为 `APPROVAL_PENDING`；`COMPLETED → APPROVAL_PENDING` 是非法迁移（合法目标只有 `REOPEN_REQUESTED`），`BLOCKED → APPROVAL_PENDING` 亦非法（合法目标 `CANCELLED/FAILED/IN_PROGRESS/READY`）。
4. **状态机可达性实测**（`ALLOWED_TRANSITIONS` 闭包）：从 `COMPLETED` / `BLOCKED` 出发，`DRAFTED` 与 `APPROVAL_PENDING` **均不可达**。→ **在途卡与已终态卡都不能靠「原卡出草稿再重批」完成对齐**。

**因此 G3 的合规路径只有一条**：**后继 Work Item 承接**——新建一张继承卡，在其草稿中写入 G3-N1/N2/N3 与术语澄清，重新走 capture → 草稿 → 审批；G3 原卡保留（`BLOCKED`），不重开、不改其 v1 草稿。

**若坚持在原卡上出草稿**（本卡不建议，且违反 ADR-BATCH 的既定处置）：必须先走 `COMPLETED → REOPEN_REQUESTED → REOPENED → READY → IN_PROGRESS → REVIEW` 的 reopen 协议（需人工授权），或 `BLOCKED → CANCELLED` 后再建继承卡；两者都**回退已批准的审批**，与本卡「审批边不可回退」的约束直接冲突。

**AC2 判定：PASS** — 在途卡唯一（G3）；列出需出新草稿的 3 条具体条目 + 1 处术语澄清；重批影响由 scratch 副本实验实测给出（出草稿即令条件 7 翻红、无法就地重批、状态机不可达）；**本卡未改任何草稿**。

---

## 3. AC3 — 已 COMPLETED 卡的处置说明（不改草稿、不回退审批）

### 3.1 处置口径（统一适用于 B0 / BW0 / G0 / W4-A）

**处置：草稿保持原样、审批保持有效、不回退、不重开。ADR 的约束由「后继工作遵守」承接。**

依据与理由：

| # | 依据 | 说明 |
| --- | --- | --- |
| 1 | woticketflow「审批边不可回退」 | `READY` 之后无出边回 `DRAFTED`/`APPROVAL_PENDING`；`COMPLETED` 唯一出边是 `REOPEN_REQUESTED`（需人工授权）。改草稿 = 旧审批失效 = 回退审批 |
| 2 | 实测（§2.3） | 出新草稿那一刻条件 7 即判 `False`，四张卡 `acceptable` 从 `True` 翻 `False`——**代价是让已收口的卡在账上不可验收** |
| 3 | ADR-BATCH 既有处置 | `docs/adr/adr-batch-acceptance-check.md:64`：「审批边不可回退，按 woticketflow 需后继 Work Item 承接或取得用户显式裁决，**不得直接改草稿冒充对齐**」 |
| 4 | 本卡任务单约束 | 「不改已 COMPLETED 卡的草稿/审批」「不改 state.db 里其它卡的草稿版本」 |
| 5 | 语义已对齐（AC1 §1.3） | G0 与 ADR-0006、W4-A 与 ADR-0005 的结论**本就一致**；B0 与 BW0 与三份 ADR **无冲突**。**没有任何一张卡因未引用 ADR 而产生了错误交付** |

### 3.2 逐卡处置说明

#### B0 — `wi_0217e2addf5641d2`（COMPLETED）

- **不改草稿**：v2 保持不动；`approvals.apr_d6d73a2c314841ad`（绑 v2 digest `sha256:9fb6b1db…`）保持有效，`acceptance verify` 实测 `acceptable=true`、7 条件全绿。
- **后继工作遵守 ADR-0004 的义务**（B0 已定方向为浏览器方向，B1/B2/B3… 才是执行者）：
  - Browser Runtime 的任何新增执行路径必须经 `ica-application` 的 port，不得直连业务库（B0 v2:L242「W7 与 B4 不得各自维护…」已定此口径，与 ADR-0004 §2 原则 1/4 同向）；
  - 不得新增第八个 crate（B0 v2:L104 与 ADR-0004 §2「首期不拆 crate」一致，无需改动）；
  - 发送链路须遵守 ADR-0004 的审批凭证校验语义（`action_digest`/`consumed_at`/`expires_at` 已在 B0-08 对账结论中覆盖，见 `docs/architecture/browser/b0-deliverables-r1.md:105`）。
- **验证证据**：B1 的浏览器 worker **已机械地**落实 ADR-0004 原则 4——`crates/ica-desktop/src/browser.rs:196–205` 的 `worker_env()` 只给最小环境、**不注入 `DATABASE_URL` / `ICA_DATA_DIR` / 任何 DSN**，注释明写「B1 AC "Browser Worker 无法直接访问 ICA SQLite", made mechanical」。

#### BW0 — `wi_5e8fe402c22a4e5e`（COMPLETED）

- **不改草稿**：v1 保持不动；`apr_428643f35bfb4200` 保持有效；`acceptance verify` 实测 `acceptable=true`。
- **后继工作遵守 ADR 的义务**：BW0 迁移面含 `/permissions`、`/approvals`（`docs/architecture/browser/bw0-compatibility-strategy.md` §2 的 8 条保留兼容路由）。这些路由落进 Rust 后，**其执行必须接入 ADR-0004 的统一网关语义**（授权判定每次重做、审批凭证校验、审计与 correlation id）。当前 `crates/` 内 `grep -rni 'execution.gateway|execution_gateway'` = **0 命中**、`grep -rn 'correlation_id|request_id' crates/ --include='*.rs'` = **0 命中**——**Gateway 与 correlation id 尚未实现**，故 BW0 后继的 browser 执行路径**不得声称已受网关管控**。
- **诚实标注**：BW0 的 8 条保留兼容路由目前是**路由级 1:1 移植**（wire 与表结构对齐），**尚未涉及执行网关**；这不构成 BW0 的缺陷（BW0 的 AC 未包含网关口径），但**构成后继波次的义务**。

#### G0 — `wi_2ee2b6dc860b4cdf`（COMPLETED）

- **不改草稿**：v3 保持不动；`apr_ce9e6f96895a4810` 保持有效；`acceptance verify` 实测 `acceptable=true`。
- **语义已对齐**：G0 v3 的重锚裁决、冻结契约不自动跟随、覆盖边界声明、锚点 sha 单一权威、差异分类（Deferred）**与 ADR-0006 逐条同构**（§1.3）。**ADR-0006 是对 G0 已做之事的追认，不是对 G0 的修正。**
- **后继工作遵守 ADR-0006 的义务**：
  1. **锚点单一权威尚未收敛**——ADR-0006 §3 第 3 项要求「契约文档第 6 行与 §1.1 的改写」。实测：`docs/rust-migration-contract.md:6` 仍写 `d39e321`，`:66`（§1.1）仍写 `d39e321`；而 `docs/migration/snapshot-r1.md:14` 已记 `9ed173c`。**该不一致仍然存在，是 ADR-0006 §3 的未闭合项**（总方案 §6.1 亦标为未闭合）。**后续波次引用锚点时必须同时注明两个值，或引用迁移快照并声明契约文档待改写**——不得只引用其中一个。
  2. 后续基线再动时，**必须**产出 Baseline Diff Report 并按 Accept / Reject / Deferred / Out of Scope 逐项留痕（ADR-0006 §2 六项字段）。
  3. 「终态是自我声明，不是机制保证」的口径继续有效。

#### W4-A — `wi_8362423d9e5743a1`（COMPLETED）

- **不改草稿**：v3 保持不动；`apr_d5f2b25f3bec4d28` 保持有效；`acceptance verify` 实测 `acceptable=true`。
- **语义已对齐**：W4-A v3 的独立审计、独立审计员复审、SHA 记录、覆盖边界声明、变异测试**与 ADR-0005 逐条同构**（§1.3）。
- **后继工作遵守 ADR-0005 的义务**：
  1. **不可变验收记录已按 ADR-0005 §2 落盘**——实测 `wave-acceptance/W4/` 下 §2 目录树要求的 **8 个文件全部存在**（`implementation-commit.txt` / `baseline-commit.txt` / `test-results.json` / `contract-coverage.json` / `findings-register.json` / `mutation-test-results.json` / `independent-review.md` / `acceptance-decision.md`），另有 1 个额外项 `p2-defect-ledger.md`。`implementation-commit.txt` = `66a053450f02383766163af06a8de10b456d982f`、`baseline-commit.txt` = `9ed173cdc6736f1213fc5697537c7f2e3a433709`——**绑 SHA 而非 tag**，符合 ADR-0005 §2。
  2. **后续每个波次**放行时必须产出同构的 `wave-acceptance/<Wave>/` 记录（**当前 `wave-acceptance/` 下只有 `W4/` 一个目录**，其余波次尚未建立）。
  3. **Q0–Q3 全绿不得作为放行结论**；放行必须由非实现方执行并附覆盖边界声明。

### 3.3 一条必须诚实标注的残留缺口（不构成回退理由）

**ADR-0004 §5.1 的硬依赖已被 W6 部分解除，但 Policy Engine 仍无实现。**

- ADR-0004 §5.1 实测（2026-11-20）：`crates/ica-application/src/` 下**无** auth/access/permission 模块；`grep 'fn authorize|fn require_access|AccessScope' crates/` → **0 命中**。
- 现状实测（本 worktree @ `6a7c0f2`）：`crates/ica-application/src/auth.rs` **已存在**（W6 交付，`cc0e64b` 2026-11-21），提供 `register`/`login`/`refresh`/`logout`/shares 与 `role` 字段、`LOCAL_USER_ID`/`LOCAL_TENANT_ID` 常量——**即 ADR-0004 §5.1 所要求的「真实用户与权限模型」已有雏形**。
- **但**：`fn authorize` / `fn require_access` / `AccessScope` / `fn can_access` / `fn check_permission` / `fn enforce` 在 `crates/` 下**仍全部 0 命中**——**Policy Engine 的判定设施仍未实现**，且 ADR-0004 §5.1 描述的 `LOCAL_OWNER_ID` / `LOCAL_ACTOR` / `DEFAULT_TENANT_ID` 三个常量名与 `auth.rs` 实际的 `LOCAL_USER_ID` / `LOCAL_TENANT_ID` **不一致**（属命名漂移，不影响语义）。
- **因此 ADR-0004 §6 的实施顺序约束依然有效**：`W6 → Policy Engine → Execution Gateway → B5/B6 + Agent 受控工具`；**在 Policy Engine 实现前，任何验收结论都不得引用「权限隔离已实现」**。这条约束**直接适用于 G3 的后继实施**。

**AC3 判定：PASS** — 四张已 COMPLETED 卡（B0 / BW0 / G0 / W4-A）均给出「不改草稿、后继工作遵守 ADR」的处置说明；**未回退任何审批、未改任何草稿**（`acceptance verify` 实测仍 `acceptable=true`）。

---

## 4. AC4 — 五卡逐条对齐状态表 + 处置 + 依据

| 卡 | work_item_id | state | 草稿版本 / 批准 | ADR-0004 | ADR-0005 | ADR-0006 | 对齐状态 | 处置 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **B0** | `wi_0217e2addf5641d2` | COMPLETED | v2 @16:16:34Z / 16:19:26Z | 弱同构（无冲突） | 无 | 弱 | **概念已对齐 / 引用缺失 / 无冲突** | 不改草稿；后继（Browser 执行路径）遵守 ADR-0004 §2 原则 1/4 + §5 网关语义 |
| **G3** | `wi_54c59a45766045bd` | **BLOCKED** | v1 @15:30:01Z / 16:20:12Z | 术语碰撞；缺 §5.1/§5.2/§6 三条 | 弱 | 弱 | **待补 3 条 + 术语澄清；无冲突** | **需后继卡承接**（出 v2 的 3 条具体条目见 §2.2）；原卡 BLOCKED 保留 |
| **BW0** | `wi_5e8fe402c22a4e5e` | COMPLETED | v1 @16:16:34Z / 16:19:26Z | 弱（迁移面含权限/审批路由） | 无 | 弱 | **概念已对齐 / 引用缺失 / 无冲突** | 不改草稿；后继 browser 执行必须接网关，**当前 Gateway 与 correlation id 均未实现，不得声称已管控** |
| **G0** | `wi_2ee2b6dc860b4cdf` | COMPLETED | v3 @15:53:24Z / 16:20:12Z | 无 | 弱 | **强（逐条同构）** | **语义已对齐 / 引用缺失 / 无冲突** | 不改草稿；后继遵守 ADR-0006：**锚点单一权威仍未收敛**（契约文档 `:6`/`:66` = `d39e321` vs 快照 `9ed173c`），引用时须双注 |
| **W4-A** | `wi_8362423d9e5743a1` | COMPLETED | v3 @16:16:34Z / 16:19:26Z | 无 | **强（逐条同构）** | 弱 | **语义已对齐 / 引用缺失 / 无冲突** | 不改草稿；后继遵守 ADR-0005：每波次放行须产 `wave-acceptance/<Wave>/` 记录（**当前仅 `W4/` 存在**）；Q0–Q3 全绿≠放行 |
| ADR-BATCH（对照） | `wi_08408b4cdc8647a0` | BLOCKED | v1 @16:46:35Z / 16:55:01Z | 字面引用 10 处 | | | AC4 = `FAILED` | 见 §5 |

**草稿与 ADR 批准的时间差**（全部早于批准 → 无一张卡为对齐已 Accepted ADR 而新建草稿）：

| 卡 | 最新草稿创建 | 早于 ADR 批准 | 批准时间 | 早于 ADR 批准 |
| --- | --- | --- | --- | --- |
| B0 | 2026-11-19T16:16:34Z | 0:37:26 | 2026-11-19T16:19:26Z | 0:34:34 |
| G3 | 2026-11-19T15:30:01Z | 1:23:59 | 2026-11-19T16:20:12Z | 0:33:48 |
| BW0 | 2026-11-19T16:16:34Z | 0:37:26 | 2026-11-19T16:19:26Z | 0:34:34 |
| G0 | 2026-11-19T15:53:24Z | 1:00:36 | 2026-11-19T16:20:12Z | 0:33:48 |
| W4-A | 2026-11-19T16:16:34Z | 0:37:26 | 2026-11-19T16:19:26Z | 0:34:34 |
| ADR-BATCH | 2026-11-19T16:46:35Z | 0:07:25 | 2026-11-19T16:55:01Z | −0:01:01（**晚于** ADR 批准 1 分钟） |

**AC4 判定：PASS** — 五卡逐条对齐状态表 + 处置 + 依据（含 sqlite 取数命令原文与实测输出）。

---

## 5. AC5 — ADR-BATCH AC4 缺口的承接说明

### 5.1 缺口陈述

`ICA-RUST-ADR-BATCH`（`wi_08408b4cdc8647a0`）的 **AC4**（「受影响卡片的草稿已相应更新：ADR 变更导致草稿实质变化的卡片已出新版草稿，旧审批按规则失效并重新走审批」）当前 **`FAILED`**，`acceptance verify` 实测：

```text
acceptable = False
conditions = {"1_all_mandatory_have_result": true, "2_no_blocking_result": false,
              "3_required_evidence_valid": true, "4_implementation_completeness": true,
              "5_independent_review": true, "6_no_unresolved_critical_conflict": true,
              "7_authorized": true}
gaps = {"2_no_blocking_result": "1 条标准最新结果为阻塞"}
```

ADR-BATCH 的 `block_reason` 已明写承接关系：

> `AC4(5卡草稿对齐ADR)需重开已批准卡草稿，违反审批边不可回退；后继卡 ICA-RUST-ADR-ALIGN wi_7fce38198f8d4320 承接评估；AC1/2/3/5 已 VERIFIED`

### 5.2 本卡的承接声明

**本卡 `ICA-RUST-ADR-ALIGN`（`wi_7fce38198f8d4320`）承接 ADR-BATCH AC4 缺口的「评估与后继处置」部分。**

具体承接内容（对应 ADR-BATCH AC4 的四项要求，逐项对照）：

| ADR-BATCH AC4 的要求 | 本卡的承接结论 | 落点 |
| --- | --- | --- |
| 「ADR 变更导致草稿**实质变化**的卡片」 | **零张**。五卡 10 个版本对 ADR-0004/0005/0006 的字面引用均为 0；概念级评估显示 G0↔ADR-0006、W4-A↔ADR-0005 **语义已对齐**（独立推导出的同构结论），其余三卡与三份 ADR **无冲突** | §1.2 / §1.3 / §1.4 |
| 「已出**新版草稿**」 | **未出，且本卡不出**。在途唯一卡 G3 需补 3 条 + 术语澄清（§2.2）；四张 COMPLETED 卡**不得出**（出草稿即令条件 7 翻红，§2.3 实测） | §2.2 / §2.3 |
| 「旧审批按规则**失效**并重新走审批」 | **不适用于本卡的处置**。旧审批对各自草稿**仍然有效**（`approval.draft_digest` == 各卡最新草稿摘要，实测一致），**不得失效**——失效即回退审批 | §3.1 依据 1–4 |
| 缺口的**去向** | ADR-BATCH 的 AC4 由「**出草稿对齐**」这一条路无法在不回退审批的前提下完成；本卡给出的替代路径是：**语义对齐已成立（无需改草稿）+ 后继卡承接 G3 的三条补充 + 后继工作遵守 ADR**。ADR-BATCH 的 AC4 **应当以本卡结论为依据改判措辞**（属「已批准的判据被实测推翻」情形：判据原文要求出草稿，但实测证明该路径与「审批边不可回退」直接冲突） | §2.3 / §3 |

### 5.3 给 ADR-BATCH 后继处理的三条可执行交接项

1. **本卡 ID 与结论的可引用性**：`ICA-RUST-ADR-ALIGN` / `wi_7fce38198f8d4320`，产出 `docs/adr/adr-alignment-assessment.md`（本文件，worktree `wt/ica-rust-adr-align` @ `6a7c0f2`）。
2. **ADR-BATCH AC4 的处置建议**：该 AC 的**字面要求**（出新版草稿 + 旧审批失效重批）与本卡的**实测约束**（出草稿即令 4 张已完成卡条件 7 翻红、状态机不可达）**不可兼得**。建议 ADR-BATCH 的后继处理按 woticketflow「已批准的判据被实测推翻」路径：**在 `audit_events` 留更正声明 + 改判据措辞为「评估与后继处置」**，而非强行出草稿。本卡**未执行**该动作（超出本卡授权：本卡只做评估与文档）。
3. **G3 后继卡的建卡要素已在本卡备齐**：需补 3 条（§2.2 的 G3-N1/N2/N3）+ 1 处术语澄清 + 出处行号，可直接作为新卡草稿正文。

### 5.4 AC5 判定

**PASS** — ADR-BATCH AC4 缺口在本卡有明确承接说明，引用本卡 ID（`ICA-RUST-ADR-ALIGN` / `wi_7fce38198f8d4320`）与产出文件；并逐项对照 ADR-BATCH AC4 的四项要求给出承接结论与可执行交接项。

---

## 6. 验证

### 6.1 工作树检查

```bash
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-adr-align
$ git diff --check
EXIT=0                       # 无空白 / 行尾错误

$ git status --short
?? docs/adr/adr-alignment-assessment.md      # 仅新增本文档，无其它改动

$ git log --oneline -1
6a7c0f2 merge: ARCH-P0 architecture baseline inventories

$ grep -c $'\r' docs/adr/adr-alignment-assessment.md
0                            # LF 行尾
```

### 6.2 `state.db` 未被本卡修改（只读连接 + 归因实测）

**先说事实**：作业期间 `state.db` 三件套的哈希**确实变了**，因此不能笼统写「哈希前后一致」。逐项归因如下。

```bash
$ sha256sum ~/.hermes/woticketflow/state.db{,-wal,-shm}    # 作业开始前（本卡第一条命令记录）
f96b99b005000111c3fd0ce44289c78644c30a441615ffd1c2397c1be98c81e4  state.db
cc8f3d7b9d475dd06fac38ca23a2017ffde3effd59ff89013dee0fef9d983117  state.db-wal
1d76ab5ade13f83cd896dbf0eccd0fd1406501e394508ed2a94c59440da2306e  state.db-shm

$ sha256sum ~/.hermes/woticketflow/state.db{,-wal,-shm}    # 作业结束后
005f595922049aa733a3627c653050b65080a373d2212134e8225619d71c7744  state.db
5c190d9f17ae61ae39f84ced99da326e193e3f65da26e00c9be4286147cd24d5  state.db-wal
e2249bd2f23c0a32ae221096bfed020b826c27aa7b51b5bc94f26d1925e17e35  state.db-shm
```

**归因实测（两步）**：

**第 1 步 — 会话开始快照 vs 现在，逐表逐行对比。** 作业开始时把 live 库（含 `-wal`/`-shm`）复制为仓外 scratch 快照 `state-exp.db`；对比结果：

- **本卡（`ICA-RUST-ADR-ALIGN` / `wi_7fce38198f8d4320`）与五张被评估卡（B0 / G3 / BW0 / G0 / W4-A）的全部 8 张相关表内容哈希逐行相同**：`work_item_versions` / `approvals` / `acceptance_criteria` / `criterion_verifications` / `lifecycle_events` / `execution_runs` / `evidence_records` / `work_items` —— 全部 `same`。
- live 库中 `created_by='experiment'` 的草稿行数 = **0**（5 条实验行只存在于 scratch 副本）；live 库 `work_item_versions` = 65 行，作业开始快照 = 70 行（= 65 + 5 实验行）。
- 快照之后 live 库新增的事件**全部属于另外三张卡**，且 actor 均为 `system:woticketflow`：

  | work_item_id | source_id | 事件 | 时刻 |
  | --- | --- | --- | --- |
  | `wi_859ac3d0903740d4` | `ICA-RUST-AUD-P1-03` | `IN_PROGRESS → REVIEW → ACCEPTANCE → COMPLETED` + `EXECUTION_CYCLE_CLOSED` | 2026-11-22T16:22:47Z |
  | `wi_fdc4ea545618476b` | `ICA-RUST-AUD-P1-04` | `IN_PROGRESS → REVIEW → ACCEPTANCE → COMPLETED` + `EXECUTION_CYCLE_CLOSED` | 2026-11-22T16:22:50Z |
  | `wi_2850de77b00d498c` | `ICA-RUST-ARCH-P2` | `READY → IN_PROGRESS`（reason `user-authorized-auto-advance`） | 2026-11-22T16:23:26Z |

  **本卡与五张被评估卡在该窗口内的新增 `lifecycle_events` 均为 0 条、新增草稿版本均为 0 条。**

**第 2 步 — 对照实验：同样的只读命令跑一遍，哈希不动。**

```bash
$ # H0（读之前）
005f595922049aa733a3627c653050b65080a373d2212134e8225619d71c7744  state.db
$ # 逐张跑本卡用过的全部只读命令（work-item show / acceptance criteria list / acceptance verify × 7 卡，
$ #   外加 sqlite3 mode=ro 全版本正文读取）
$ # H1（读之后）
005f595922049aa733a3627c653050b65080a373d2212134e8225619d71c7744  state.db
$ # 三件套哈希与 mtime 完全不变
```

**结论**：哈希变化来自**并发会话对另外三张卡（AUD-P1-03 / AUD-P1-04 / ARCH-P2）的正常推进**，与本卡无关——本卡全部 `state.db` 访问均为 `file:…?mode=ro` 只读连接，或 `woticketflow_cli.py` 的只读子命令（`work-item show` / `acceptance criteria list` / `acceptance verify`，三者内部均为 `readonly=True` 或 `mode=ro`），对照实验证明这些调用**不改变三件套的哈希与 mtime**。§2.3 的实验全程在**仓外 scratch 副本** `state-exp.db` 上进行（`Runtime(SCRATCH)`，真实库全程只读），实验产生的 5 条草稿行**只存在于副本**，live 库中为 0。

**本卡自身的净写入 = 0**：`state.db` 中本卡的 `work_item_versions` / `approvals` / `acceptance_criteria` / `lifecycle_events` / `execution_runs` / `evidence_records` 与作业开始时**逐行一致**；本卡新增的 1 条 `lifecycle_events`（`READY → IN_PROGRESS`，`user-authorized-auto-advance`）**发生在作业开始之前**（2026-11-22T16:01:59Z，rowid 557），不是本卡执行期产生的。

### 6.3 三份 ADR 状态未改

```bash
$ grep -n '状态：' docs/adr/ADR-0004-ica-runtime-architecture.md \
                docs/adr/ADR-0005-five-tier-quality-gates.md \
                docs/adr/ADR-0006-baseline-layers-and-change-handling.md
docs/adr/ADR-0004-ica-runtime-architecture.md:3:- **状态：** **Accepted**
docs/adr/ADR-0005-five-tier-quality-gates.md:3:- **状态：** **Accepted**
docs/adr/ADR-0006-baseline-layers-and-change-handling.md:3:- **状态：** **Accepted**
```

### 6.4 未提交、未推送

本卡**未执行** `git add` / `git commit` / `git push`。`git status --short` 只有本文档一条未跟踪记录；分支 `wt/ica-rust-adr-align` 仍停在 `6a7c0f2`。

### 6.5 改动文件清单

| 文件 | 类型 | 说明 |
| --- | --- | --- |
| `docs/adr/adr-alignment-assessment.md` | 新增 | 本评估文档（唯一产出） |

**未改动**：任何卡的草稿版本与审批记录；`state.db`；三份 ADR；基线仓 `/home/bryan/projects/ica`（@ `9ed173c`，工作树干净）与 `/mnt/o/ica`（本机非 git 仓，仅只读参照）。

---

## 7. 遗留与未覆盖

| 项 | 性质 | 说明 |
| --- | --- | --- |
| ADR-0006 §3 第 3 项（契约文档第 6 行 / §1.1 改写） | **未闭合** | 实测 `docs/rust-migration-contract.md:6`、`:66` 仍写 `d39e321`，快照已记 `9ed173c`。本卡只评估不修（超出授权：改契约文档需另立授权） |
| Policy Engine 实现 | **不存在** | ADR-0004 §5.1 的授权设施由 W6 部分供给（`auth.rs` 有 role 与 shares），但 `authorize`/`require_access`/`AccessScope` 类判定设施仍 0 命中。ADR-0004 §6 顺序约束继续有效 |
| Execution Gateway / correlation id | **不存在** | `crates/` 下两个模式各 0 命中。ADR-0004 §5.2 结论：需新建 |
| `wave-acceptance/` 只覆盖 W4 | 未铺开 | ADR-0005 §2 要求每波次放行产出不可变验收记录；当前仅 `wave-acceptance/W4/` |
| ADR-BATCH AC4 的**改判**动作 | **未执行** | 本卡只评估不写 `state.db`；改判据属破坏性写入，需用户授权后由 ADR-BATCH 后继处理 |
| G3 的 7 条 AC | **全 FAILED** | `acceptance verify` 实测 `acceptable=false`（条件 2、3 不满足）。这是 G3 自身的实施缺口，与 ADR 对齐无关，但**它是 G3 恢复实施的真正前置** |
| ADR-0003 D-12 并行口径 | 适用 | 「只读调查与架构审计可并行，实现不可并行」——本卡属纯评估 + 文档，符合；后继 G3 实施须先过 D-12 判据（是否改 `crates/`、是否改冻结契约） |
