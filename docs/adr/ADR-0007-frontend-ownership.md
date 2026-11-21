---
title: "ADR-0007: 前端代码归属"
document_id: "ADR-0007"
document_type: "ArchitectureDecisionRecord"
project: "ICA-RUST"
version: "1.0.0"
status: "Proposed"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
related_documents:
  - "AUDIT-ICA-F0-001"
  - "ADR-0003"
  - "ADR-0006"
supersedes: null
---

# ADR-0007: 前端代码归属

## Status
Proposed(待项目负责人签署 —《UI 重构方案》§8 明言"这个决策不定,F0 无法开工")

## Date
2026-11-21

## Context

前端(React 19,54 文件 / 12,330 行,含 vitest 套件与 UI 复用门禁基线)目前住在
姊妹仓 `O:\ica\frontend`。而 ADR-0006 把该仓定性为**只读基线仓**(Python 契约权威),
F0 起的前端改造必须修改这些代码 — 两仓约束相反,改造无处落地。Tauri 宿主(亦在
`O:\ica\desktop\tauri-host`)打包时把 `frontend/dist` 拷入 `bundle-resources/ui`,
打包链同样受影响。

F0-AUDIT(2026-11-21)确认:前端与 Rust 后端的线格式兼容性为 57/90 调用即插即用,
改造面收敛于一层 Backend Client 抽象 — 前端迁仓是纯位置移动,不改变任何行为。

## Problem

前端代码的开发权归属哪个仓库;迁移对基线仓冻结语义与打包链的影响。

## Alternatives

| 方案 | 内容 | 优点 | 代价 |
|---|---|---|---|
| **A. 迁入 ICA-RUST/frontend/**(推荐) | 前端整体迁入本仓,与 Rust 同仓 | 与 R1.0 架构图一致(ICA Desktop 单仓);一个 PR 叙事;Tauri 打包链可整体入仓;issue/CI 单点 | 一次性迁移(git 历史留在基线仓,以 F0-1 迁移提交为准);基线仓打包脚本冻结在最后发布态 |
| B. 独立前端仓 | 新建 `ica-frontend` 仓 | 前后端彻底解耦 | 多一套 CI/门禁/PR 流程;桌面产品跨三仓;规模不匹配(单维护者) |
| C. 留在基线仓并就地开发 | 违反 ADR-0006 只读语义 | 零迁移成本 | 契约权威仓被开发污染;两仓约束冲突永久化 — **不可取** |

## Decision(待签署)

采用方案 A:前端整体迁入 `O:\ICA-RUST\frontend\`。

## Rationale

1. R1.0 §4.1 的目标架构本就把 React+TS 画在 ICA Desktop 之内 — 归属裁决是把
   架构图落成仓结构,不是新设计。
2. F0-AUDIT 证明改造面(一层抽象)与两侧代码的耦合是**前后端契约**,同仓让
   契约变更(ica-contracts ↔ types.ts 对拍)在同一 PR 可见。
3. 打包链(`stage_bundle.sh` 的 ui 拷贝 → Tauri `frontendDist`)可整体迁入,
   版本单元模型不变。
4. 单维护者现实下,独立仓的流程成本大于解耦收益。

## Consequences

- 正面:单仓开发/CI;契约对拍同 PR;打包链完整入仓。
- 负面/约束:基线仓从此前端部分为**冻结归档**(历史追溯走旧仓 git);
  `O:\ica` 的打包脚本不再服务后续版本(在仓内标注 frozen)。
- 中性:Python sidecar 与 browser 子系统仍在基线仓(运行时引用,非代码归属),
  至 B 系列迁移完成。

## Validation

F0-1 迁移卡执行后:`npm run build` + `vitest run` 全绿、UI 复用门禁基线不变、
桌面打包产物 sha256 与迁移前对拍。

## References

F0-AUDIT(2026-11-21)coupling-report.md §3/§6;《ICA-RUST UI 重构方案》§8;
ADR-0003(波次治理);ADR-0006(基线分层)。
