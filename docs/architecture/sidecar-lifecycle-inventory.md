# ICA-RUST Sidecar 生命周期盘点 — ARCH-P0 / AC5（Phase 4 输入）

**基线 HEAD（完整 SHA）：** `7213f424c51f1a20f2b911bc5d6f651dd0244c3f`（短 SHA `7213f42`）
**分支 / worktree：** `wt/ica-rust-arch-p0` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0`
**取数时点：** 2026-11-22（CST） · **性质：** 纯只读盘点，不改业务行为。

## 1. 取数命令原文

```bash
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0
grep -nE 'sidecar|Sidecar|Command::new|child|kill|spawn|/health|poll_health|stop_backends|AI-EXT|Browser|PORT' \
  desktop/tauri-host/src/main.rs
sed -n '1,110p;221,232p;346,448p;456,510p;570,600p;665,685p;700,795p;1060,1100p' \
  desktop/tauri-host/src/main.rs
grep -nE 'stdin|listening|PORT' /home/bryan/projects/ica/backend/run_sidecar.py    # 只读交叉核对
```

Sidecar 生命周期全部在 `desktop/tauri-host/src/main.rs`（Tauri 2 Host，约 1200 行）；Python 对端为
baseline `run_sidecar.py`（只读交叉核对，未修改）。

## 2. 生命周期总览

| 阶段 | 代码锚点（main.rs） | 行为 |
|---|---|---|
| 版本单元解析 | `resolve_unit()`，:139-163 | 顺序：`ICA_UNIT_DIR` → `<exe>/current.json` → `<exe>/bundle-resources` → `<exe>/ui`（判据 `ui/` 或 `sidecar/` 目录存在） |
| 解释器定位 | `SIDECAR_PY`，:58-62 | Windows `sidecar\runtime\python\python.exe`；非 Windows `sidecar/runtime/python/bin/python3` |
| 启动 | `spawn_sidecar()`，:346-384 | `<unit>/sidecar/app/run_sidecar.py`；`Command::new` + `-S -s`；`current_dir(unit)`；Windows `CREATE_NO_WINDOW` |
| 环境变量 | :366-371 | `ICA_DATA_DIR=<data_root>`、`PORT=0`（临时端口）、`PYTHONDONTWRITEBYTECODE=1`、`PLAYWRIGHT_BROWSERS_PATH=<unit>/browsers`（BROWSER-05：只用内置 Chromium） |
| 握手 | :385-448 | stdout 行含 `"listening"` 且带 `port` → `parse_listening()`（:221-232）取端口；60s 超时 kill 并报错；stderr + stdout 全量追加到 `<data>/logs/sidecar.log` |
| 存活探测 | `poll_health()`，:496-510 | 200ms 间隔、20,000ms 上限，经 `forward_to(port,"GET","health",…)` 轮询 `/health`；超时只降级不致命 |
| 端口登记 | :773-784 | `PY_PORT AtomicU16`；health 失败则写回 0，browser/AI-EXT 路由随之返回 502 |
| 通信 | `forward_to()`，:459-494 | `ica-contracts::proxy_headers` 双向允许列表过滤 + `render_request`；`TcpStream` 连 `127.0.0.1:<port>`，30s 读超时，按响应允许列表重建 header |
| 双后端路由 | `route_backend()`，:83-110；分派 :577-590 | `Backend::Python` → `PY_PORT`；`Backend::Rust` → `RUST_PORT`；webview 侧 `/api` 经 `ica://` 同源转发，前端零改动 |
| 关闭 | `stop_backends()`，:668-680 | `drop(sc.child.stdin.take())` → EOF 触发 sidecar 优雅退出；Rust server 用 `child.kill()` |
| 关闭触发点 | :886 / :897 | 主窗口创建失败时调用并 exit(2)；`WindowEvent::Destroyed` 时调用 |

Python 对端确认：`run_sidecar.py:299-305` `_watch_stdin()` 在 `stdin.read(1)` 返回空时关闭 server；
`:352-369` 读 `PORT`、打印含 `"listening"` 的端口行、非 tty 时启动 stdin 监听线程。
Host 与 Python 的关闭契约（stdin EOF = graceful exit）双向闭合。

## 3. Browser / AI-EXT 依赖的路由判定（纯函数 `route_backend`）

| 请求 | 判定 | 后端 |
|---|---|---|
| `api/browser/v1/...`（含裸前缀 `api/browser/v1`） | `path.starts_with("api/browser/v1")` | Python sidecar（39 条，BW0） |
| `POST /api/ai/models` | 精确 path + method | Python sidecar（AI-EXT） |
| `PUT`/`DELETE /api/ai/models/<built-in function id>` | `starts_with("api/ai/models/")` + PUT/DELETE | Python sidecar（AI-EXT） |
| `PUT /api/ai/models/<built-in function id>/default` | 同上（含斜杠 id） | Python sidecar（AI-EXT） |
| `GET /api/ai/models`、`GET/PATCH /api/ai/routing`、`GET /api/ai/usage`、`GET/PATCH /api/ai/budget` | 落入默认分支 | Rust ica-server（W5 已实现） |
| `POST /api/imports/<built-in function id>/confirm-candidate` | 落入默认分支 | Rust（W7 辅助路由） |
| 其它一切 `/api` | 默认分支 | Rust ica-server（68 契约） |

AI-EXT 精确集合（4 条）与 `check_endpoint_map.py` 的 ADR-0008 前 4 条一致；第 5 条
confirm-candidate 归 Rust——Host 单测 `ai_ext_model_crud_routes_to_python()` /
`implemented_w5_routes_stay_on_rust()` / `the_68_route_contract_stays_on_rust()` 逐条钉住该判定。

## 4. 降级与开发态分支

- Rust ica-server 是主后端：spawn 失败或 `/health` 超时 → `exit(1)`（FATAL）。
- Sidecar 是次后端：三种降级路径均不阻断启动——无内置 Python（:353-360）、spawn 失败（:765-769）、
  `/health` 超时（:779-783）。降级后 `PY_PORT=0`，browser/v1 + AI-EXT 请求走 502（带可读原因）。
- `ICA_SIDECAR_PORT` / `ICA_RUST_SERVER_PORT` 环境变量可采纳外部已启动进程（F1-1 WSL drill；
  规避 `/mnt` 9p 上 SQLite WAL 的 split-brain）。采纳 sidecar 时 Host 不持有 child，关闭阶段不 kill。

## 5. Phase 4 输入（待办，不在本卡实施）

1. **B 系列迁移边界**：39 条 browser 路由仍 100% 依赖 Python sidecar；BW0 迁移卡需逐批把
   `route_backend()` 判定改为 Rust，并同步 frontend `browserApi`（28 个调用点，其中 27 条 browser/v1）。
2. **AI-EXT 归属**：4 条 model CRUD 按 ADR-0003 D-2 留在 sidecar；AI-EXT 卡落地时需同时改
   `route_backend()`、`NONCONTRACT_ROUTES`（check_endpoint_map.py）、ADR-0008 与前端 `aiApi` 4 个写方法。
3. **优雅关闭验证**：Windows 实机验证 `WindowEvent::Destroyed` + stdin EOF 后 sidecar 进程确实退出
   （QA-03 桌面交付验收依赖此项）；当前仅有源码契约与 Python 侧 `_watch_stdin` 实现。
4. **握手兼容性**：Host 测试用的是 `{"listening": true, "port": N}`，baseline 实发
   `{"event": "listening", "host": ..., "port": N}`；两者都满足「行含 `"listening"` + 有 `port` 字段」，
   但建议在 Phase 4 统一为单一 shape 并补一条对拍测试，避免以后误改。
5. **降级可观测性**：browser/AI-EXT 502 目前只写 `host.log`；建议在 Phase 4 补前端可读提示与日志结构化字段。
6. **端口采纳语义**：`ICA_SIDECAR_PORT` 采纳外部进程时不持有 child，属已知设计；Phase 4 若引入
   「外部进程也由 Host 关闭」语义，需显式定义所有权，避免误杀。

**结论：AC5 VERIFIED。** Sidecar 启动/握手/通信/关闭四段逻辑与 Browser(39)+AI-EXT(4) 依赖已逐段定位到代码行，
Phase 4 输入 6 项已产出；未改任何业务行为，数字绑定 HEAD 7213f424c51f1a20f2b911bc5d6f651dd0244c3f。
