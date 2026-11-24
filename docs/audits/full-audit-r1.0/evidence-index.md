# RICA 全量审计证据索引 R1.0

审计基线：`c3749018a61132aecf1a2db0a3fcdc47af723493`

## 本轮直接检查的仓库对象

| 路径/对象 | 检查目的 | 证据性质 |
|---|---|---|
| `README.md` | 项目定位、能力状态、Known limitations | 当前文档 |
| `Cargo.toml` | Workspace、依赖与 lint 规则 | 当前配置 |
| `crates/ica-api/src/routes.rs` | 路由注册、Middleware、RequestContext、Scope 使用 | 当前源码 |
| `crates/ica-server/src/main.rs` | 启动配置、监听策略、迁移、AI/SecretStore 初始化 | 当前源码 |
| `crates/ica-api/src/auth.rs` | 认证实现入口 | 当前源码路径已定位；本轮未逐行穷尽全部实现 |
| `crates/ica-infra/src/store.rs` | SQLite Repository 实现 | 当前源码路径已定位；本轮未逐行穷尽全部实现 |
| `scripts/gate.py` | 统一门禁入口 | 当前工程配置 |
| `.github/workflows/quality-gate.yml` | CI 工作流 | 当前工程配置 |
| `frontend/package.json` | 前端测试/构建脚本 | 当前工程配置 |
| `src-tauri/tauri.conf.json` | 桌面配置路径探查 | 路径需与实际 Workspace 结构进一步核对 |
| `docs/testing/test-report-arch-p6.md` | 历史测试数据与未执行项 | 历史验收报告 |
| `docs/testing/desktop-release-acceptance.md` | 桌面验收依据 | 本轮未完整读取，需在 Phase 0 补查 |
| `docs/audits/sec-04-bc-delivery-record.md` | 认证/授权整改状态 | 阶段交付记录 |
| `docs/architecture/api-authentication-matrix.md` | API 认证授权矩阵 | 架构文档 |
| `docs/testing/defect-register.md` | 历史缺陷与依赖安全事项 | 缺陷台账 |
| Commit `c3749018...` | 最新 UI Phase 2 合并内容与验收记录 | 提交元数据及差异 |

## 历史测试证据

### ARCH-P6 报告

报告绑定 HEAD：`40575e1aa78b316cede43104e3e78c9e00956130`。

报告中记录：
- Rust 522 passed、0 failed、3 skipped。
- Frontend 225 passed。
- Build / Clippy：PASS。
- Sidecar 契约测试：6 passed。
- Tauri 实际构建：未在该次 WSL 执行中验证。
- Dependency security scan：BLOCKED。
- E2E：BLOCKED。
- Python baseline parity：未执行。

这些结果仅适用于其绑定的历史 commit。

### UI Phase 2 验收记录

合并提交：`c3749018a61132aecf1a2db0a3fcdc47af723493`。

提交差异中的验收记录声明：
- Vitest：31 files / 239 tests passed。
- Frontend build：成功。
- Lint：0 errors / 3 warnings。
- UI Reuse：5/8 passed，C3/C5/C8 仍 failing，regressions=0。
- Windows Phase 2 桌面回归：未执行。

该数据是提交内记录的结果，本轮未独立重跑。

## 证据限制

- 本次未克隆仓库到本地环境。
- 未执行 Cargo/npm/Windows 构建或测试命令。
- 未执行动态扫描、模糊测试或渗透测试。
- 未逐行审查仓库全部文件；本报告是关键路径审查与现有证据审查。
- 任何未列入直接检查对象的模块均不能据此推定通过。

后续应在 Phase 0 补充实际命令、环境版本、日志路径、退出码和 SHA256，并将报告绑定新的验证 commit。
