# ADR-BROWSER-006：Browser Runtime 生命周期

- **状态：** Proposed
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** B1 卡（POC 验证生命周期）、B2 卡（状态机）

---

**决策：** Tauri Host 负责进程生命周期，ICA-RUST Application 负责业务任务与授权。
