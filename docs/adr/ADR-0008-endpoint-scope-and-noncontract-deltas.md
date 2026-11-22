# ADR-0008：迁移端点范围与 5 条契约外差额的归属

- **状态：** Accepted（用户 2026-11-22 签署；据 ADR-0003 D-2/D-11 起草）
- **来源：** G0 AC3（独立复核 2026-11-22 判 FAIL：端点范围 ADR 不存在）；ADR-0003 D-1/D-2 补充裁决/D-3/D-11
- **关联：** ADR-0002（Rust 迁移）、ADR-0003（治理裁决）、ICA-RUST-W5-W6 / AI-EXT / W7
- **注意：** 冻结契约恒为 68 条（`d39e321` 基线），本 ADR 不回写、不扩缩该集合。

---

## 背景

`check_endpoint_map.py` 报「73 live / 68 documented，差 5 条」。G0 AC3 要求对这 5 条差额
逐条给出「迁移 / 不迁移 / 待定」结论并附用户批准记录。此前 ADR-0003 已就其中 4 条给出 D-2
补充裁决，confirm-candidate 由 D-1 重锚归入 W7，但一直没有一份独立的端点范围 ADR 汇总，
故本 ADR 补位。

## 决策（据 ADR-0003 既有裁决汇总，非新增裁决）

冻结契约集合恒为 **68 条**，以下 5 条差额均为**契约外增量**，不进入 68 条、不回写基线：

| # | 方法 | 路径 | 归属 | 结论 | 依据 |
| --- | --- | --- | --- | --- | --- |
| 1 | POST | `/api/ai/models` | AI-EXT | 契约外增量，单独立波次 | ADR-0003 D-2 补充裁决（2026-11-20） |
| 2 | PUT | `/api/ai/models/{model_id:path}` | AI-EXT | 契约外增量，单独立波次 | D-2 |
| 3 | PUT | `/api/ai/models/{model_id:path}/default` | AI-EXT | 契约外增量，单独立波次 | D-2 |
| 4 | DELETE | `/api/ai/models/{model_id:path}` | AI-EXT | 契约外增量，单独立波次 | D-2 |
| 5 | POST | `/api/imports/{session_id}/confirm-candidate` | W7 | 契约外增量，随 D-1 重锚归 W7 | ADR-0003 D-1 / D-2 补充裁决（「第 5 条是 confirm-candidate」） |

### 算术自洽（实测 2026-11-20）

```
68（冻结契约，d39e321）
+ 4（ai/models CRUD，契约外，AI-EXT）
+ 1（confirm-candidate，契约外，W7）
= 73（live 业务路由）✓ 与 check_endpoint_map.py 的 73 一致
```

### 剩余 39 条 `/api/browser/*`

归 BW0 兼容迁移评估，在 B0 路由盘点后确定范围（ADR-0003 D-11）。本节不对其逐条裁决。

## 后果

- `scripts/check_endpoint_map.py` 的「73 live / 68 documented」差异是**预期且已裁决**的，
  MISMATCH 不代表迁移缺口；脚本的例外规则需表达本 ADR（G0 AC7 真缺口，另行修复）。
- `ICA-RUST-W5-W6` 范围恒为 23 条（含 `GET /ai/models` 一条 read 端点），不存在「19 条」口径。
- 68 条契约的完整实现为 68/68（`scripts/endpoint_reconciliation.py --json` 实测），本 ADR 不改变该计数。

## 签署

- 裁决人：**BryanTianjx**（项目所有者，用户本人；用户 2026-11-22 指令「批准…ADR-0008 签署后的门禁例外修复」，署名采用确认的默认值）
- 签署日期：**2026-11-22**（CST）
- 结论口径：**Approved**（批准端点范围与 5 条差额归属，无附加条件）
- 授权来源：用户 2026-11-22 明确指令；内容据 ADR-0003 D-2/D-11 起草，非执行方自创裁决。

> 本 ADR 已于 2026-11-22 由用户签署生效（状态 Accepted）；此前为 Proposed 草案。
