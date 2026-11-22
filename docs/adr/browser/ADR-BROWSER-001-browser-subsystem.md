# ADR-BROWSER-001：Browser Subsystem 独立子系统

- **状态：** Accepted（原文：建议采纳）
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** ADR-0003（D-4/D-6/D-7/D-9）、B0 卡
- **注意：** 冻结 = 把提案文字提取为正式 ADR 记录；批准以用户签署为准（B0 门禁）。

---

**决策：** 浏览器自动化作为 ICA 一级子系统，不直接嵌入 Hermes Runtime。

备选方案：

- A：Hermes 自己管理浏览器。
- B：ICA Browser Subsystem 独立管理浏览器。
- C：ICA 和 Hermes 分别管理自己的浏览器。

**选择 B。**

理由：

- 统一权限与审计。
- 避免 Runtime 供应商绑定。
- 统一候选人采集和平台适配。
- 统一发送审批和执行证据。

---

## 批准记录

- **状态：** Accepted
- **批准人：** BryanTianjx
- **批准日期：** 2026-11-22
- **结论口径：** Accepted（批准，无附加条件）
- **授权来源：** 用户 2026-11-22 明确指令「批准全部 10 份 ADR-BROWSER」；署名/日期/结论口径采用用户确认的默认值。
