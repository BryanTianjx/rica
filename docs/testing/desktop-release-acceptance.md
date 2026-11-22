---
title: "桌面发布验收清单（Windows 实机）"
document_id: "DESKTOP-ACCEPT-ICA-R1"
document_type: "AcceptanceChecklist"
project: "ICA-RUST"
version: "1.0.0"
status: "BLOCKED — Windows 实机待验（用户已明确接受的环境受限项）"
created: "2026-11-23"
updated: "2026-11-23"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "AUDIT-ICA-ENG-001"
  - "TEST-ICA-DRILL-001"
supersedes: null
---

# AC3 / QA-03：桌面安装·升级·卸载·恢复验收（Windows 实机）

## 0. 结论先行：**BLOCKED（环境受限），不是 PASS**

用户已明确接受此环境受限项：本机是 WSL2，**没有 Windows 实机**，无法安装 NSIS
安装包、无法验证安装/升级/卸载/崩溃恢复的产品行为。本文件**不伪造任何通过**，
也不把 Linux 上的机制级测试冒充 Windows 实机验收。

实测环境证据（真实命令与输出）：

```text
$ uname -a
Linux TIAN 6.18.58.1-microsoft-standard-WSL2 #1 SMP PREEMPT_DYNAMIC Tue Oct 27 00:24:11 UTC 2026 x86_64 GNU/Linux

$ . /etc/os-release; echo "$PRETTY_NAME"
Ubuntu 26.04.1 LTS

$ pkg-config --exists glib-2.0 && echo glib-ok || echo glib-MISSING
glib-MISSING
$ pkg-config --exists webkit2gtk-4.1 && echo webkit-ok || echo webkit2gtk-MISSING
webkit2gtk-MISSING

$ cargo audit --version
error: no such command: `audit`
```

含义：① 无 Windows 主机 → 安装包全链路不可测；② 缺 glib/WebKit → Tauri
host 连 Linux 侧真编译也不可做（只做了 `rustfmt` 语法检查）；③ 无 cargo-audit
→ 依赖 advisory 扫描亦为工具受限。三者均如实标记，不合并为「通过」。

## 1. 待验清单（9 项，逐项给出验收方法；状态全部 = BLOCKED / 待人工）

依据审计报告 §5.10 QA-03 的验收标准：

| # | 场景 | 验收方法（在 Windows 实机执行） | 状态 | 证据归档位置 |
|---|---|---|---|---|
| W1 | Windows 全新安装成功 | 在干净 Win10/11 x64 虚拟机运行 `ICA-Personal-<ver>-x64-setup.exe`，静默+交互各一次，检查安装日志无 error、安装目录完整 | **BLOCKED / 待人工** | 待填：安装包 sha256 + 安装日志 |
| W2 | 首次启动成功 | 安装后立即启动，30s 内出现主窗口，`/health` 等价自检通过，无 crash dialog | **BLOCKED / 待人工** | 待填：启动录屏/日志 |
| W3 | SQLite 数据库自动初始化 | 首次启动后检查用户数据目录 `ica.db` 存在，DDL 对象数与 `crates/ica-infra/schema/ica-baseline.sql`（32 表/121 索引）一致 | **BLOCKED / 待人工** | 待填：真实库 DDL 摘要 |
| W4 | 从上一版本升级成功 | 装旧版→写入业务数据→就地安装新版→启动，确认数据保留、版本manifest 更新 | **BLOCKED / 待人工** | 待填：升级前后行对照 |
| W5 | 数据 Migration 成功 | 旧版库升级后用 `SELECT * FROM schema_migrations` / 行数核对确认迁移幂等、无部分状态 | **BLOCKED / 待人工** | 待填：迁移前后快照 |
| W6 | Backup/Restore Drill 成功（产品路径） | 在实机 UI 走 POST/GET backups 与 confirm restore，核对行数和审计事件 `backup.restored` | **BLOCKED / 待人工** | 机制级证据已由 AC4 提供（见 §2），**不能替代实机** |
| W7 | 异常退出后可重新启动 | 任务管理器 `taskkill /F` 强杀进程→重启→核对探针数据、备份列表、审计轨迹持久化 | **BLOCKED / 待人工** | 待填：强杀前后对照 |
| W8 | 卸载行为符合数据保留策略 | 卸载后检查用户数据目录（ica.db / backups / secrets）是否按策略保留；NSIS installer-hooks 的删除清单逐项核对 | **BLOCKED / 待人工** | 待填：卸载前后目录树 |
| W9 | 安装包版本和构建信息可追溯 | 核对 `tauri.conf.json` version、`version-manifest.json`、`SHA256SUMS`、安装包数字签名与发布 tag `v*` 一致 | **BLOCKED / 待人工** | 待填：三方版本对照表 |

## 2. 本机能提供的**机制级**证据（不是 W1–W9 的替代）

AC4 已在临时库上真实执行 backup→mutate→restore→verify：10/10 断言通过，
32 张表的行数与 DDL 摘要（sha256 `4146a1a8…`）逐表恢复一致。该证据证明
**SQLite 在线备份机制**正确（对应 `crates/ica-infra/src/backup.rs`），但不能
证明 W6 的 Windows 安装包/`installer-hooks.nsh`/UI 路径正确。

执行记录：`docs/testing/test-report-arch-p6.md` §4；
原始证据 `scripts/backup_restore_drill.py --out <path>` 生成的 JSON。

## 3. 解阻条件

需要：① 一台 Windows 10/11 x64 实机或干净虚拟机；② 可构建/获取的 NSIS 安装包
及其 sha256；③ 一名有签字权的人按 §1 逐项执行并填写证据栏。
三者齐备后，本文件状态可由人改为 `Approved`；**Agent 不得自签**。
