# SEC-04-A 远程暴露门禁（发布前规则与检查方法）

> 依据：已批准审计报告 R1.2 §11.1（SEC-04-A/B/C）、§13.2、§14.2（局域网/公网在 SEC-04 完成前不允许开放）、§14.3 第 3 项（负责人裁决建设完整多用户认证授权）。
> 本文件是**规则 + 可复核检查方法**，不是「已配置生效」的声明。SEC-04-A 只实现启动策略与文档门禁；真实多用户认证属 SEC-04-B/C 卡，**未在本卡实现**。

## 1. 硬性规则

| # | 规则 | 适用范围 | 违反判定 |
|---|---|---|---|
| R1 | `ica-server` 只允许回环监听：`HOST` 取 `127.0.0.1` / `localhost` / `::1`；默认 `127.0.0.1` | 所有本地运行、开发、演示 | `validate_bind_host` fail-fast |
| R2 | SEC-04 完成前禁止局域网/公网暴露：不得绑定 `0.0.0.0`、`::`、网卡 IP 或域名；不得用反向代理、SSH 隧道、容器端口映射把回环服务暴露到外部 | 部署、Docker、Agent 运行、桌面壳 | 发布/部署评审直接拒绝 |
| R3 | `ICA_AUTH_REQUIRED=true` 只是「申请远程模式」的显式策略开关，**不是认证**；当前 `NETWORK_AUTHENTICATION_READY=false`，开启后非回环仍被拒绝并提示 `多用户认证尚未实现，不能启用远程模式` | 所有尝试远程监听的进程 | 不得把该开关写成「认证已启用」 |
| R4 | Local 单用户模式下所有业务路由都没有 HTTP 认证中间件，synthetic `local_user`/admin 只在本机信任边界内有效；不得跨边界复用默认管理员身份或白名单端点 | API、IPC、分享、备份/导出 | 任何跨用户/跨边界授权声明均无效 |
| R5 | 发布/合并必须同时通过：源码断言测试、矩阵一致性、CI Required Gate；门禁结果为 BLOCKED/SKIP 时不得当作 PASS | PR、Release、Tag | 缺失证据按未通过处理 |

## 2. 检查方法（逐条可复核）

在 worktree 根目录执行，`CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target`。

| 检查 ID | 命令 / 动作 | 通过标准 |
|---|---|---|
| G1 | `cargo test -p ica-server --bin ica-server` | 3 条 `bas002_*` 通过；`sec04_a_*` 全部通过；非回环被拒绝且错误含 `refusing to bind to non-loopback host`、`no network authentication`；显式开关但认证未就绪时含 `多用户认证尚未实现，不能启用远程模式` |
| G2 | `cargo test -p ica-api --test sec04_authorization_http` | `sec04_a_public_whitelist_matches_unauthenticated_routes` 与 `sec04_a_matrix_policy_column_matches_whitelist` 通过；白名单 = `/health` + `/auth/register` + `/auth/login`，且在 `routes.rs` 中真实注册；`router()` 只有 2 个 HTTP 奇偶 layer，无认证中间件 |
| G3 | `cargo test -p ica-api --lib sec04_a_route_manifest_tests` | 矩阵 68 行的 Path 全部对应 `ROUTE_PATTERNS`；白名单 3 项都在清单内 |
| G4 | `HOST=0.0.0.0 cargo run -p ica-server` | 进程启动失败（ExitCode FAILURE），日志出现 `refusing to bind to non-loopback host`，且**未**打印 `{"listening": true, ...}` |
| G5 | `ICA_AUTH_REQUIRED=true HOST=0.0.0.0 cargo run -p ica-server` | 仍然启动失败，日志含 `多用户认证尚未实现，不能启用远程模式`；不得出现任何「认证已启用/远程模式可用」的信息 |
| G6 | `python3 scripts/gate.py --ci` 与 `Quality Gate / gate` Required Check | 状态为 PASS / PASS_WITH_LIMITATIONS；BLOCKED/SKIP 不得被解释为通过；CI 未放行则禁止合并 |
| G7 | 人工核对 `docs/architecture/api-authentication-matrix.md` 的「SEC-04-A 当前策略」列与 `docs/audits/RICA 全量工程实现复审报告 R1.2 2026-11-23.md` §14.2 | 68 行策略分明（2 公开 / 66 Local 放行 / 未来需认证），并明确 SEC-04 完成前禁止远程暴露 |
| G8 | 部署配置审计（发布评审时人工执行） | 无 `-p 0.0.0.0:8000`、无 `HOST=0.0.0.0`、无指向本服务的公网反代/隧道配置；有则视为阻断 |

## 3. 证据与诚实状态

- G1–G3 是可自动化的源码/单测检查，SEC-04-A 卡内执行；G4/G5 需要实际启动进程验证，G6 依赖 CI 与仓库管理员配置，G7/G8 为人工发布评审。
- `.github/BRANCH_PROTECTION.md` 状态为 **Proposed — 需仓库管理员在 GitHub 设置中落实**，分支保护/Ruleset **尚未生效**；本文件不替代其管理员验收步骤。
- SEC-04-B/C（真实多用户认证、资源归属、IDOR/越权测试）完成后，才允许重新评估 R1–R5；届时必须由非实现者做独立安全验证，不得由实现方自签。
- 当前状态：Local-only 启动策略与远程暴露阻断**已实现**；真实多用户认证**未实现**；独立安全验证**未完成**。
