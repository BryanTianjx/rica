---
title: "F0/F1 任务卡草案"
document_id: "AUDIT-ICA-F0-001-T"
document_type: "TaskCards"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
related_documents:
  - "AUDIT-ICA-F0-001"
supersedes: null
---

# F0/F1 正式任务卡(草案 — 归属裁决签署后生效)

> 前置:ADR-0007(前端归属)签署。所有卡在迁入后的前端代码上执行;若裁决为
> 独立仓,仓库地址替换为该仓。

## F0 冻结与抽象(行为不变)

| 卡 | 内容 | 验收 |
|---|---|---|
| **F0-1 前端迁仓** | 按 ADR-0007 裁决迁移 `O:\ica\frontend` → 目标位置(含 vitest 套件、check-ui-reuse 基线 json、package-lock);`O:\ica` 打包脚本冻结在最后发布态 | 迁后 `npm run build` + `vitest run` 全绿;UI 复用门禁基线不变 |
| **F0-2 Backend Client 抽象** | 新建 `services/backend/`(每端点一方法);页面/组件全部改为经此层调用;endpoints.ts 退役为 backend 层的 HTTP 实现 | grep 全仓无页面直连 endpoints/api;90 调用点全部经抽象层 |
| **F0-3 行为冻结验证** | 现有 vitest + 手工冒烟(对照 Python 后端):登录/本地模式、七个导航页、错误与空状态 | UI 行为与迁前一致;diff 仅限抽象层 |
| **F0-4 已知项顺手修** | 下拉第一页截断(candidates/positions `limit:500`);错误呈现统一到 `code: message` | 两处修复 + 对应测试 |
| **F0-5 对拍资产化** | FollowUpsPage 状态机镜像表纳入与 `ica_domain::follow_up` 的对拍(防漂移) | 对拍脚本或测试落地 |

## F1 Rust IPC 垂直切片

| 卡 | 内容 | 验收 |
|---|---|---|
| **F1-1 Host 集成 Rust server** | Tauri 宿主 spawn `ica-server`(替代/并存 Python sidecar;P1 分流规则:`/api/browser/v1/*`+AI-EXT → sidecar,其余 → Rust) | 宿主启动即用 Rust 后端;health 探测通过 |
| **F1-2 Positions IPC 切片** | `positions_*` 五命令(ica-desktop 已有)接 Backend Client positions 分支(`invoke()`);HTTP 与 IPC 双实现对拍 | 同操作同结果同错误语义同落库(方案 F1 验收原文) |
| **F1-3 事件通道** | Tauri Events 定义(后端事件 → 前端),替换"手动 reload 链"的首个场景 | 一处 reload 链改为事件驱动 |
| **F1-4 IPC 权限白名单** | 命令白名单机制落地(方案 §3 要求) | 白名单外 invoke 被拒并有测试 |

## 决策依赖

| 决策 | 阻塞 | 归属 |
|---|---|---|
| ADR-0007 前端归属 | F0-1..F0-5 全部 | 项目负责人 |
| P1 双后端分流(Python sidecar 保留范围与时限) | F1-1 | 项目负责人 |
| AI-EXT 是否永不进 Rust(维持 ADR-0003) | F1-1 分流规则 | 项目负责人 |
