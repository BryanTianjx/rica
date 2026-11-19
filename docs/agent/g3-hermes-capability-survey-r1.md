# G3 H-POC-01 交付物 — Hermes Runtime 能力盘点（本机实测）

> G3 卡硬约束 1 原文：「必须记录实际安装版本支持的 RPC、Gateway owner 能力声明、
> 认证方式和 MCP 传输能力。**不能把文档里出现的所有 RPC 名称当作本机版本都支持。**」
> 实测日 2026-11-20（WSL Ubuntu-26.04，ICA-RUST 快照 `67e9d00`）。

## 1. 安装事实（H-POC-01）

| 项 | 实测值 |
| --- | --- |
| 版本 | **Hermes Agent v0.23.1 (2026.9.24) · stable · local `3b4215785882`** |
| 安装方式 | `git`（`/home/bryan/.hermes/hermes-agent`，CLI 于 `~/.local/bin/hermes`） |
| 用户级状态目录 | `~/.hermes/`（auth.json、bin、cache、cron、kanban、woticketflow 等——**ICA 专用 Profile 必须与此隔离**，H-POC-02） |
| 运行中的衍生系统 | woticketflow（任务单台账）、cron、kanban——**同机共存约束**：ICA 的 Runtime POC 不得触碰这些用户级状态 |

## 2. 关键发现：方案假设的 Gateway 与实装不符

《ICA-RUST × Hermes Embedded Agent 方案 R1.0》G3 的建议原文：「首选通过 Hermes
Gateway 的程序化 JSON-RPC 接口管理 Runtime」。**本机版本的实情**：

```
hermes gateway —— 是「消息网关」（Telegram / Discord / WhatsApp / Weixin 桥接），
                  子命令 run/ensure/attach/start/stop/status/install/enroll…
                  其中 attach 提供「WebSocket URL + 单次会话票据」的客户端接入，
                  不是面向 Runtime 管理的程序化 JSON-RPC API。
```

即：**方案假设的「Gateway owner 能力声明 / 程序化 Runtime 管理 RPC」在
v0.23.1 上不存在**。这正是硬约束 1 预警的情形。结论：

- G3 的 H-POC-03/04（Runtime 启停 + Gateway JSON-RPC Session 管理）**不能按
  方案原文的接口实施**——需要一次方案级修订，把集成路径改为以下实测存在的
  表面之一（见 §3）；
- 在该修订（用户裁决）之前，H-POC-03..10 不开工——避免对着不存在的 API 写 POC。

## 3. 实测存在的集成表面（供方案修订选择）

| 表面 | 实测证据 | 适合承接 |
| --- | --- | --- |
| **MCP 客户端** | `hermes mcp add/remove/list/test/configure/catalog/install`——Hermes 作为 MCP **客户端**连接外部 MCP 服务器；文档明示「Running Hermes itself as an MCP server is the mcp-serve plugin」 | ✅ **H-POC-07（ica.ping）路径可行**：ICA 侧起一个 MCP server（暴露 `ica.ping`），`hermes mcp add` 挂接，`hermes mcp test` 验证——传输（stdio/HTTP）与鉴权在 POC 中实测 |
| **CLI 进程面** | `hermes chat/-z/--resume/--continue/--pass-session-id/--in/--safe-mode/--yolo`、`status`、`pause/resume` | 会话提交/恢复/中断的进程级管理（Supervisor 以子进程方式持有 Runtime） |
| **gateway attach** | WebSocket URL + 单次 session ticket | 流式事件（H-POC-05）的候选路径；票据机制 = 短期凭据（与方案「认证方式」调查项对应） |
| **审批面** | `hermes approvals`（审批提示与 allowlist） | H-POC-08（审批事件接收/响应）的现实入口 |
| **降级面** | `hermes pause/resume`（Emergency stop）、`doctor`、`status` | H-POC-09（异常恢复）的可观测/可控面 |

## 4. 对 G3 剩余子任务的影响

| 编号 | 任务 | 结论 |
| --- | --- | --- |
| H-POC-01 | 固定版本 | ✅ 本文档完成 |
| H-POC-02 | 专属 Profile | 可实施（配置目录隔离；需探明 profile 级 flag/目录约定） |
| H-POC-03/04 | Runtime 启停 + Gateway JSON-RPC | ⚠️ **需方案修订**（§2）——建议改为「Supervisor 子进程 + CLI 面」或「gateway attach」路径 |
| H-POC-05 | 流式事件 | 依赖 03/04 的路径裁决 |
| H-POC-06 | Interrupt | `--pass-session-id` + resume 面可探 |
| H-POC-07 | MCP `ica.ping` | ✅ 路径明确（`hermes mcp add` + ICA 侧 MCP server），可独立先行 |
| H-POC-08 | 审批事件 | `approvals` 面可探 |
| H-POC-09 | 异常恢复 | `pause/resume/doctor/status` 面 |
| H-POC-10 | 双平台 | macOS 侧本机不可测——留待有 macOS 环境的会话，结论不得在单平台时给出 |

## 5. 建议的下一步（供 G3 卡修订）

1. 用户裁决集成路径（CLI-Supervisor vs gateway-attach），写入 ADR 或 G3 卡修订节；
2. 可先行项：H-POC-02（Profile 隔离实测）+ H-POC-07（`ica.ping` MCP server 最小实现
   + `hermes mcp add/test`）——两者不依赖被推翻的 Gateway 假设；
3. H-POC-03..06、08、09 在路径裁决后按新表面重写验收判据。
