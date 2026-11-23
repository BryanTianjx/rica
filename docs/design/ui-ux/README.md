# ICA Personal UI/UX 2.0 设计文档集

本目录承载 `docs/ICA R2.0 UI 重构方案：统一招聘工作台 + Hermes Agent Runtime + Browser Workspace.md`
所定义的设计基线与 Phase 0–6 实施产物。

**D01 / D02 为只读审计基线**：后续代码变化不回写覆盖；新问题进入新审计版本或整改记录。

## 目录结构

```
docs/design/ui-ux/
├── README.md
├── 01-architecture/     # 总体规范索引、信息架构、App Shell 架构
├── 02-audit/            # D01 / D02 / D03（只读审计基线）
├── 03-design-system/    # Tokens / 组件 / 交互规范
├── 04-implementation/   # Phase 1+ 实施方案、任务分解、验收报告
└── 05-testing/          # UI 测试规范与回归基线
```

## 文档索引

| 编号 | 文档 | 阶段 | 状态 |
| --- | --- | --- | --- |
| D01 | [`UI 现状审计报告`](./02-audit/D01-UI现状审计报告-R1.0.md) | P0 | 完成（2026-11-23，只读基线） |
| D02 | [`UI 页面与功能映射表`](./02-audit/D02-页面与功能映射表-R1.0.md) | P0 | 完成（2026-11-23，只读基线） |
| — | [`Phase 1 实施方案 R1.0`](./04-implementation/Phase-1实施方案-R1.0.md) | P1 | 已形成，准备实施 |
| — | [`Phase 1 实施方案（早期草稿）`](./04-implementation/Phase-1实施方案-草稿.md) | P1 | 被 R1.0 取代，保留追溯 |

## 统计口径（统一）

- 实际 `<Route>` 声明：**10 条** = 9 条业务页面路由 + 1 条 `/index.html` 兼容重定向。
- 登录：认证态渲染，**不是** Route。
- 招聘项目 / AI 助手 / 安全与审计：未声明 Route。
- WP08 将新增 1 条 `path="*"` 404 兜底：属兜底路由，不增加到「业务页面路由」统计。

## 基线原则（来自方案 §13.1 与 Phase 1 评审）

1. 不自动提交 / 推送 / 合并 / 发布。
2. 不删除现有功能、不改 DB Schema、不改 Rust 业务接口、不改 AI 端点归属。
3. Hermes 零实现，不建空壳状态页；Browser 保留现有入口，不改运行架构。
4. 结构性调整先形成实施方案，每阶段独立验证，变更可回滚。
5. 设计建议 ≠ 现有实现；所有「待验证事项」以仓库实测为准。

## 主规范

[ICA R2.0 UI 重构方案（根目录）](../../ICA%20R2.0%20UI%20重构方案：统一招聘工作台 + Hermes%20Agent%20Runtime +%20Browser%20Workspace.md)
