# ADR-BROWSER-010：Browser Platform Compliance

- **状态：** Proposed
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** B0-04（平台合规报告）、B3 卡（Adapter 上线准入）

---

**决策：** 平台适配不得绕过登录保护、验证码、访问限制或平台安全机制。
平台规则和可执行范围应成为每个 Adapter 的上线准入条件。
