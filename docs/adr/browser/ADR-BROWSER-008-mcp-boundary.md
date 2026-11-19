# ADR-BROWSER-008：Browser MCP Boundary

- **状态：** Proposed
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** G3 卡（Hermes Runtime POC）、方案 §5（MCP 契约设计）

---

**决策：** Hermes 只访问招聘业务级 Browser MCP Tools，不访问 CDP、任意 DOM 和任意 JavaScript。
