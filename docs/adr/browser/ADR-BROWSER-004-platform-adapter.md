# ADR-BROWSER-004：Platform Adapter

- **状态：** Proposed（原文：建议采纳）
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** B3 卡（Platform Adapter 实施方）

---

**决策：** 通用 Browser Controller 与平台专属 Adapter 分离。

理由：

- 避免平台 DOM 变化影响通用层。
- 平台适配可单独测试和升级。
- 多招聘平台扩展时不改变领域层。
- 每个平台可以定义自己的页面身份验证和发送核验规则。
