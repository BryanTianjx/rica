---
title: "桌面发布验收清单（Windows 实机）"
document_id: "DESKTOP-ACCEPT-ICA-R1"
document_type: "AcceptanceChecklist"
project: "ICA-RUST"
version: "1.0.0"
status: "EVIDENCED_PARTIAL — 6/9 项已在 Windows 11 实机执行（W4/W5/W6 未完成），待负责人签署"
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

## 0. 结论先行：**部分实测（6/9），未签署，不是全量 PASS**

归档时（2026-11-23 早间）本机为 WSL2，无 Windows 实机，原结论为 BLOCKED。
随后在本机 Windows 11 x64 实机上完成了构建、静默安装、首启、强杀重启和卸载
共 6 项实测（见 §5）；W4/W5/W6 仍未执行。Agent 不代签，本文件状态需负责人确认。

以下是**归档时**的环境证据，保留作为历史，不覆盖 §5 的实机结果：

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

## 1. 验收清单（9 项；2026-11-23 实机执行结果见 §5）

依据审计报告 §5.10 QA-03 的验收标准。下列状态是**实测记录**，不是负责人签署结论。

| # | 场景 | 验收方法（在 Windows 实机执行） | 状态 | 证据归档位置 |
|---|---|---|---|---|
| W1 | Windows 全新安装成功 | 运行 NSIS 安装包，静默+交互各一次，检查退出码、安装目录完整 | **PASS（静默）/ 交互未执行** | `install-silent.log`：exit 0、4546 文件 |
| W2 | 首次启动成功 | 安装后启动，30s 内出现主窗口，`/health` 等价自检通过 | **PASS** | `launch-first.log`、`host-runtime.log`、`sidecar-runtime.log` |
| W3 | SQLite 数据库自动初始化 | 检查用户数据目录 `ica.db` 与 DDL 对象数（32 表/121 索引） | **PASS** | `sidecar-runtime.log`（migration `501a807efa03`）、§5 表统计 |
| W4 | 从上一版本升级成功 | 装旧版→写入业务数据→就地安装新版→启动，确认数据保留 | **NOT_EXECUTED**（无上一版安装包） | — |
| W5 | 数据 Migration 成功 | 旧版库升级后核对 `schema_migrations` 与行数 | **NOT_EXECUTED**（依赖 W4） | — |
| W6 | Backup/Restore Drill 成功（产品路径） | 在实机 UI 走 backups/restore，核对行数与审计事件 | **BLOCKED**（产品 API 依赖的 `ica-server.exe` 未随包，见 §5 缺陷） | 机制级证据见 §2，不替代实机 |
| W7 | 异常退出后可重新启动 | 强杀进程→重启→核对 DB/配置/日志持久化 | **PASS** | `restart-after-kill.log` |
| W8 | 卸载行为符合数据保留策略 | 卸载后检查安装目录、注册表键与用户数据目录 | **PASS** | `uninstall.log`（前后目录/注册表对照） |
| W9 | 安装包版本和构建信息可追溯 | 核对 tauri.conf.json、version-manifest、SHA256SUMS、签名/tag | **PARTIAL**（版本/哈希一致；未签名、无 tag） | `windows-acceptance-20261123T134723Z.json` |

## 2. 本机能提供的**机制级**证据（不是 W1–W9 的替代）

AC4 已在临时库上真实执行 backup→mutate→restore→verify：10/10 断言通过，
32 张表的行数与 DDL 摘要（sha256 `4146a1a8…`）逐表恢复一致。该证据证明
**SQLite 在线备份机制**正确（对应 `crates/ica-infra/src/backup.rs`），但不能
证明 W6 的 Windows 安装包/`installer-hooks.nsh`/UI 路径正确。

执行记录：`docs/testing/test-report-arch-p6.md` §4；
原始证据 `scripts/backup_restore_drill.py --out <path>` 生成的 JSON。

## 3. 解阻条件

Windows 实机与 NSIS 安装包已具备（见 §4/§5）。剩余条件：
① 上一版本安装包（W4/W5）；② 修复 `ica-server.exe` 未随包的产品缺陷后复验 W6；
③ 一名有签字权的人核对 §5 证据并签署。
三者齐备后，本文件状态可由人改为 `Approved`；**Agent 不得自签**。

## 4. 本底分支推进记录（2026-11-23，Windows 实机）

原 §0 的「无 Windows 实机」是归档时的真实结论；本轮已在 Windows 11 x64
实机上构建出真实 NSIS 安装包并执行了 6 项实测（见 §5）。未执行的 3 项
（W4/W5/W6）与签署要求仍保留，详见 §3。

- 本底分支：`baseline/windows-real-machine-acceptance`（从 `main` @ `d007705` 切出，改动未提交）。
- 新增证据采集器 `scripts/windows_acceptance.ps1`：只采集环境/前置事实并输出
  `PREFLIGHT_NOT_ACCEPTANCE` 报告，不自动安装、升级、卸载，不代签。
  W1–W9 逐项状态为 `READY/BLOCKED/MANUAL`，不存在自动 PASS。
- 实机预检证据：`docs/testing/evidence/windows-acceptance/windows-acceptance-20261123T131547Z.json`
  / `.md`。主机为 `Microsoft Windows 11 家庭版 中文版 10.0.26200`（build 26200，x64）。
- 解除一个 Windows 原生构建阻断：`desktop/tauri-host/src/ica_backend.rs` 的
  `IcaState` 补 `#[derive(Clone)]`（`main.rs` 关闭路径需要克隆后写入 `APP_STATE`）。
  修复后真实命令 `cargo build --manifest-path desktop/tauri-host/Cargo.toml`
  在 Windows 上 **Finished `dev` profile**，退出码 0。原失败为 `E0599: no method named clone`。
- Windows 实机补充门禁（真实输出）：`cargo fmt --all -- --check` 退出码 0；
  `cargo clippy --manifest-path desktop/tauri-host/Cargo.toml -- -D warnings`
  退出码 0；`cargo test --manifest-path desktop/tauri-host/Cargo.toml`
  `11 passed; 0 failed`。这些只证明宿主机可编译/单测通过，**不代表 W1–W9 通过**。
- 本轮已把 ICA 项目的 Windows payload（Python 3.12.16 runtime + site-packages
  + app + UI + Chromium）stage 到 `bundle-resources`，`version-manifest.json`
  记录 `build_commit=9ed173cdc6736f1213fc5697537c7f2e3a433709`；
  用 pinned Tauri CLI `2.12.4` 构建出真实 NSIS 安装包（213.24 MiB），
  sha256 `82f89b5b741a891f1c054918c1e7b096bb565ec3c361672c943e3ba0c1955daf`。

复跑预检（需有 installer 时传入 `-InstallerPath`）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows_acceptance.ps1 `
  -InstallerPath 'C:\path\ICA-Personal-0.2.2-x64-setup.exe' `
  -DataDir 'C:\Temp\ica-accept-data' `
  -OutDir 'docs/testing/evidence/windows-acceptance'
```

## 5. 真实安装验收执行记录（Windows 11 x64 实机，2026-11-23）

安装包：`ICA Personal_0.2.2_x64-setup.exe`，223,602,388 bytes，
sha256 `82f89b5b…55daf`。隔离安装目录 `O:\temp\ica-accept-install`，
隔离数据目录 `O:\temp\ica-accept-data`（`ICA_DATA_DIR` 显式指定），
**未触碰** `%LOCALAPPDATA%\ICA\data` 的真实用户数据。

| 步骤 | 真实命令 / 动作 | 实测结果 |
|---|---|---|
| 构建 | `npx --yes @tauri-apps/cli@2.12.4 build`（`desktop/tauri-host`） | Finished 1 bundle，exit 0 |
| W1 静默安装 | `Start-Process installer -ArgumentList '/S','/D=O:\temp\ica-accept-install' -Wait -PassThru` | `INSTALL_EXIT=0`，安装目录 4546 文件；未做交互安装 |
| 安装完整性 | 检查 exe / python / ui / manifest / SHA256SUMS / uninstall.exe | 全部存在；卸载器名为 `uninstall.exe`（非 `unins000.exe`） |
| 安装钩子 | `reg` 读取 Chrome/Edge `com.ica.connector` 键 | 两键存在，均指向 `O:\temp\ica-accept-data\browser\connector\com.ica.connector.json` |
| W2 首启 | `Start-Process ica-desktop-host.exe`（带 `ICA_DATA_DIR`），等待 8s | PID 5288；`host.log` 有 `ui loaded`；sidecar `127.0.0.1:6553`；`GET /health` → **HTTP 200** `{"status":"ok",...}` |
| W3 初始化 | 查询 `ica.db` 与 `sqlite_master` | `ica.db` 876,544 B；migration `501a807efa03`；**32 表 / 121 索引**，与 `ica-baseline.sql` 一致 |
| W7 强杀重启 | `Stop-Process -Force` 后重新启动 | 新 PID 24692；`ica.db`、`jwt_secret`、日志、connector manifest 均保留；`host.log` 有新的启动记录 |
| W8 卸载 | `uninstall.exe /S` | `UNINSTALL_EXIT=0`；安装目录消失；Chrome/Edge 注册表键均消失；`ica.db`/`jwt_secret`/日志/manifest 仍保留 |
| W9 追溯 | 预检脚本 + manifest 对照 | 安装包名含 `0.2.2`，manifest 版本/构建 commit 一致；**无 Authenticode 签名、无发布 tag**，不判 PASS |

**发现的缺陷（需另立 delta 卡，不在本分支修复）**：`host.log` 在 UI 加载后报
`on-demand ica-server failed: the Rust server binary was not found`（尝试
`ICA_RUST_SERVER_EXE`、`bundle-resources/ica-server.exe`、`bundle-resources/server/ica-server.exe`、
应用根 `ica-server.exe` 均不存在）。因此 Embedded Mode 下前端走 HTTP 的
业务 API（如 `/api/v1/meta`）返回 **404**，W6 的产品路径无法执行。
这是打包清单 `desktop/tauri-host/tauri.conf.json` 未纳入 Rust server 二进制的
真实缺陷，**不得用 sidecar `/health` 200 推断业务 API 可用**。

**证据目录**：`docs/testing/evidence/windows-acceptance/`，其中
`stage-windows-payload.log`、`tauri-nsis-build.log`、`install-silent.log`、
`launch-first.log`、`host-runtime.log`、`sidecar-runtime.log`、
`restart-after-kill.log`、`uninstall.log` 为原始输出；
`windows-acceptance-20261123T134723Z.json/.md` 为带安装包 sha256 的预检报告。

**签署**：未签署。W4/W5 缺上一版本安装包，W6 待上述缺陷修复后复验，
W1 的交互安装亦未执行；负责人核对后再决定是否把本文件改为 `Approved`。
