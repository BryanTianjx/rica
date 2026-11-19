# ADR-BROWSER-002：Managed Browser 为默认模式

- **状态：** Proposed（原文：已由产品决策确认方向）
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** ADR-0003（D-6/D-7）、B0 卡、B2 卡（双模式实现）

---

**决策：** 默认由 ICA 管理独立 Chromium Profile，External Browser 作为可选连接方式。

理由：

- 浏览器会话生命周期可控。
- 登录数据与日常浏览器隔离。
- 双平台更容易建立一致的运行模型。
- 可以在 ICA 中显示浏览器运行状态。
