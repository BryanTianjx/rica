# ADR-BROWSER-003：Browser Automation Engine

- **状态：** Proposed（原文：需要 POC 验证）
- **来源：** 《ICA Browser Subsystem Architecture R1.0》§8（B0-06 冻结，2026-11-20）
- **关联：** B1 卡（选型证据产出方）、D-10

---

**建议：** Playwright + Chromium，独立 Worker。

备选：

- Playwright + Node.js Worker。
- Rust Browser Automation Library。
- 直接使用 CDP。

POC 验证后决定正式实现方式。方案 §2.3 的告诫仍然有效：**不能在未测试打包体积、
浏览器下载和双平台行为之前认定技术方案已经定型**——B1 的产出之一是选型证据，
不是「按 Playwright 实现」本身。
