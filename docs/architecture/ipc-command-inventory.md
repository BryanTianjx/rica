# ICA-RUST 全量 IPC 命令盘点 — ARCH-P0 / AC3

**基线 HEAD（完整 SHA）：** `7213f424c51f1a20f2b911bc5d6f651dd0244c3f`（短 SHA `7213f42`）
**分支 / worktree：** `wt/ica-rust-arch-p0` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0`
**取数时点：** 2026-11-22（CST） · **性质：** 纯只读盘点。

## 1. 取数命令原文

```bash
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0
grep -n -A40 'generate_handler' desktop/tauri-host/src/main.rs
cat crates/ica-desktop/src/ipc_allowlist.rs
grep -nE '^pub async fn ' crates/ica-desktop/src/commands.rs
grep -c '#\[tauri::command\]' desktop/tauri-host/src/ica_backend.rs     # 5（第 1 处为文件注释）
cat desktop/tauri-host/capabilities/default.json
```

## 2. 已注册命令（5 条）——白名单与 Host 逐条一致

`desktop/tauri-host/src/main.rs:853-859` 的 `tauri::generate_handler![...]`：

```
ica_backend::positions_list
ica_backend::positions_get
ica_backend::positions_create
ica_backend::positions_patch
ica_backend::positions_delete
```

`crates/ica-desktop/src/ipc_allowlist.rs` 的 `ALLOWED_COMMANDS`：

```
"positions_list", "positions_get", "positions_create", "positions_patch", "positions_delete"
```

两侧集合**完全相等（5 = 5，无多余、无遗漏）**。`ipc_allowlist.rs` 内还有单测
`host_invoke_handler_registers_exactly_the_allowlist()`，用 `include_str!` 解析 Host 源码中的
`generate_handler![...]` 块并与 `ALLOWED_COMMANDS` 排序后逐项比对——这是源码级的同步闸门
（Host 因缺 glib/WebKit 系统库无法在本机编译）。

| 命令 | Host wrapper（#[tauri::command]） | adapter（ica_desktop::commands） | Application Use Case | 写事件 |
|---|---|---|---|---|
| `positions_list` | `ica_backend.rs:65` | `commands.rs:74` | `list_positions` | — |
| `positions_get` | `ica_backend.rs:73` | `commands.rs:101` | `get_position` | — |
| `positions_create` | `ica_backend.rs:81` | `commands.rs:107` | `create_position` | emit POSITION_CHANGED |
| `positions_patch` | `ica_backend.rs:92` | `commands.rs:118` | `positions::update_position` | emit POSITION_CHANGED |
| `positions_delete` | `ica_backend.rs:104` | `commands.rs:131` | `delete_position` | emit POSITION_CHANGED |

- Host wrapper 全部位于 `desktop/tauri-host/src/ica_backend.rs`，是薄桥接层，无合同逻辑。
- 三个写命令在 wrapper 中调用 `announce()` → `app.emit(POSITION_CHANGED, …)`，payload 是
  `ica-contracts` 的 transport-neutral `PositionChanged`；emit 结果被丢弃（通知语义）。
- `capabilities/default.json` 仅含 `core:default`；IPC 可调用性由 `invoke_handler` 注册控制，不依赖额外权限。

## 3. 潜在新增（已实现但未注册）

`crates/ica-desktop/src/commands.rs` 共 **70** 个 `pub async fn`（含 `health`），其中只有上表 5 个
在 Host 注册；**65 个已实现但未注册，当前不可 invoke**。按前缀分组：

| 前缀 | 未注册命令数 | 命令名 |
|---|---|---|
| `ai*` | 10 | `ai_egress_read`, `ai_egress_consent`, `ai_models_list`, `ai_routing_get`, `ai_routing_patch`, `ai_usage`, `ai_budget_get`, `ai_budget_patch`, `ai_key_write`, `ai_key_clear` |
| `audit*` | 1 | `audit_events_list` |
| `auth*` | 5 | `auth_register`, `auth_login`, `auth_refresh`, `auth_logout`, `auth_me` |
| `backups*` | 3 | `backups_list`, `backups_create`, `backups_restore` |
| `candidate*` | 2 | `candidate_timeline`, `candidate_context` |
| `candidates*` | 5 | `candidates_list`, `candidates_get`, `candidates_create`, `candidates_patch`, `candidates_delete` |
| `conversations*` | 2 | `conversations_list`, `conversations_create` |
| `dashboard*` | 1 | `dashboard` |
| `draft*` | 2 | `draft_groups_get`, `draft_groups_select` |
| `drafts*` | 7 | `drafts_list`, `drafts_get`, `drafts_generate`, `drafts_patch`, `drafts_approve`, `drafts_reject`, `drafts_copy` |
| `export*` | 1 | `export_data` |
| `follow*` | 10 | `follow_ups_evaluate`, `follow_ups_list`, `follow_ups_create`, `follow_ups_get`, `follow_ups_patch`, `follow_ups_cancel`, `follow_ups_complete`, `follow_ups_record_result`, `follow_ups_policy_get`, `follow_ups_policy_patch` |
| `health*` | 1 | `health` |
| `import*` | 1 | `import_data` |
| `imports*` | 6 | `imports_preview`, `imports_get`, `imports_confirm`, `imports_confirm_candidate`, `imports_reject`, `imports_ocr` |
| `messages*` | 2 | `messages_list`, `messages_create` |
| `send*` | 2 | `send_records_list`, `send_records_create` |
| `settings*` | 1 | `settings_read` |
| `shares*` | 3 | `shares_create`, `shares_list`, `shares_delete` |

这些命令共用同一套 `ica_application` Use Case 与 `ica-contracts` DTO，F2 后续卡只需在
Host 的 `generate_handler!` 与 `ALLOWED_COMMANDS` 两处同步增加即可开闸（源码级单测会强制两处一致）。
本次盘点**只登记，不注册**——不改业务行为。

**结论：AC3 VERIFIED。** 已注册 5 条 = 白名单 5 条，逐条有 Use Case；潜在新增 65 条已登记但未开闸，数字绑定 HEAD 7213f424c51f1a20f2b911bc5d6f651dd0244c3f。
