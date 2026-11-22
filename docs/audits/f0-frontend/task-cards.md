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
| **F0-2 Backend Client 抽象** | ✅ **已实现(2026-11-22)**:`services/backend/index.ts` 统一客户端(13 域,每端点一方法;`BackendClient` 类型 = F1 IPC 契约;ApiError 统一重导出);35 个页面/组件/测试文件机械重写。验收:全仓 grep endpoints/api 直连导入 **双零**(仅 backend/index.ts 持有);tsc 构建 + vitest 184 测试全绿 |
| **F0-3 行为冻结验证** | 现有 vitest + 手工冒烟(对照 Python 后端):登录/本地模式、七个导航页、错误与空状态 | UI 行为与迁前一致;diff 仅限抽象层 |
| **F0-4 已知项顺手修** | 下拉第一页截断(candidates/positions `limit:500`);错误呈现统一到 `code: message` | 两处修复 + 对应测试 |
| **F0-5 对拍资产化** | FollowUpsPage 状态机镜像表纳入与 `ica_domain::follow_up` 的对拍(防漂移) | 对拍脚本或测试落地 |

## F1 Rust IPC 垂直切片

| 卡 | 内容 | 验收 |
|---|---|---|
| **F1-1 Host 集成 Rust server** | ✅ **已实现并实机联调(2026-11-22)**:宿主迁入 `desktop/tauri-host`(ADR-0007),spawn/采纳双后端(`ICA_RUST_SERVER_EXE`/`ICA_RUST_SERVER_PORT`/`ICA_SIDECAR_PORT`),P1 分流为纯函数 `route_backend` + 4 组单测。**实机联调通过**:真实 Tauri 窗口渲染迁移后的 dist,UI 创建岗位 → ica:// → 宿主分流 → Rust 8141 → SQLite 落库 → UI 列表回显;**双后端一致性验证** — 同一行数据 Rust 8141 与 Python 8140 同时可见。实测发现并记录:WAL 数据库跨 /mnt 9p 边界(Windows 后端 + WSL sidecar 共享一个库文件)会**分裂脑**(各侧只写不见),开发形态解法 = 双后端同侧(WSL)共存 + 宿主端口采纳;生产形态(双后端同 OS 原生)不受影响 |
| **F1-2 Positions IPC 切片** | ✅ **已实现(2026-11-22)**:前端 `services/backend/ipc.ts` — positions 五方法经 `invoke()`(`@tauri-apps/api/core`),IpcError 信封 → ApiError 单点转换;宿主注册 5 个 `#[tauri::command]`(ica_backend.rs,manage IcaState = DesktopState 同库)+ capabilities/default.json;`BackendClient["positions"]` 编译期钳制 IPC 形状(缺方法=编译错)。isTauri 检测选择分支,浏览器/vitest 走 HTTP 不变 | 实机对拍(见 F1-1 联调):IPC 创建 → HTTP 查询同行可见;HTTP 种子 → IPC 列表同见;错误信封同形 |
| **F1-3 事件通道** | Tauri Events 定义(后端事件 → 前端),替换"手动 reload 链"的首个场景 | 一处 reload 链改为事件驱动 |
| **F1-4 IPC 权限白名单** | 命令白名单机制落地(方案 §3 要求) | 白名单外 invoke 被拒并有测试 |

## 决策依赖

| 决策 | 阻塞 | 归属 |
|---|---|---|
| ADR-0007 前端归属 | F0-1..F0-5 全部 | 项目负责人 |
| P1 双后端分流(Python sidecar 保留范围与时限) | F1-1 | 项目负责人 |
| AI-EXT 是否永不进 Rust(维持 ADR-0003) | F1-1 分流规则 | 项目负责人 |
