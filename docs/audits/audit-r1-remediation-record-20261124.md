# RICA 全量审计 R1.0 整改执行记录（2026-11-24）

- 审计来源：远端分支 `origin/audit/full-audit-r1.0`，审计基线 `c3749018`（UI Phase 2）
- 整改执行工作区：main @ `6b303212865adf13deecb3d3bfef5d52ae380381`（UI Phase 6）+ 未提交整改改动
- 性质：按审计报告与整改路线图执行的**首轮工程整改**；本记录不是发布验收结论
- 重要说明：整改期间未创建 commit。HEAD 仍为审计时的 `6b30321`，所有验证证据绑定的是**当前工作区状态**，后续提交不得自动继承本记录

## 1. 已完成的整改项

| 缺陷 | 措施 | 代码/证据 |
|---|---|---|
| P1-02 管理员权限负向测试不足 | 在 HTTP 适配层为 9 个高风险接口（AI 预算/路由/Key、AI 出网同意、导入、导出、备份创建/恢复）接入 `Permission::AdminOperation`；member 统一 403 `forbidden` | `crates/ica-api/src/routes.rs`；测试 `crates/ica-api/tests/sec04_bc_authorization_http.rs::admin_operations_reject_member_role` |
| P1-01（部分）多租户资源授权覆盖不足 | 候选人实体下沉 Scope：Filter/Repository/UseCase/HTTP 四层新增 scoped 路径，SQL 强制 `tenant_id`（member 额外 `owner_id`），关联 position 按同域校验，越权返回 404 | `crates/ica-application/src/{payload,ports,candidates}.rs`、`crates/ica-infra/src/candidates.rs`、`crates/ica-api/src/routes.rs`；测试 `candidate_rows_are_isolated_by_tenant_and_owner` |
| P2-01 README 文档漂移 | 重写 Known limitations，删除 W5/W6 未实现、无多租户、无桌面交付等过时表述，改为当前证据支持的限制清单 | `README.md` |
| P2-05 UI Reuse C3/C5/C8 | C3：33/33 union 值全部有 tone 分支，`status` prop 改为 union 类型；C8：页面状态字面量归拢到 `statusVocabulary`；C5：抽取共享页面原语（Card/Notice/FormRow/Actions） | `frontend/src/components/ui.tsx`、`frontend/src/shared/statusVocabulary.ts`、`frontend/src/shared/components/PagePrimitives.tsx`、相关页面 |
| P2-07 依赖安全扫描 | 修复门禁脚本的 `cargo audit --locked` 版本兼容与 Windows `npm` 解析；实际执行 cargo audit 并归档结果 | `scripts/gate.py`；`docs/testing/evidence/gate-6b30321-20261124/cargo-audit-current.log`、`tmp/cargo-audit-after.log` |
| INFO-01 / 新发现的 url 漏洞 | `url 2.5.8 → 2.5.9`（含 `form_urlencoded 1.2.2 → 1.2.3`），漏洞数由 2 降为 1 | `Cargo.lock` |
| P2-07 / INFO-01 libsqlite3-sys（2026-11-24 续做） | `rusqlite 0.32→0.43`、`sqlx 0.8→0.9.1`，共享解析到 `libsqlite3-sys 0.40.0`；适配 sqlx 0.9 的 `SqlSafeStr`（50 处动态 SQL 经审计后包装 `AssertSqlSafe`，值全部走 bind）；`SqliteArguments<'a>` 去掉生命周期 | `Cargo.toml`、`crates/ica-infra/Cargo.toml`、`crates/ica-infra/src/*.rs`、`crates/ica-api/tests/support/mod.rs`；`cargo-audit-sqlite-upgrade.log`、`sqlite-upgrade-check.log`、`sqlite-upgrade-tests.log` |

## 2. 验证结果

- Rust 全 workspace 测试：`cargo test --locked --workspace`（跳过 3 个 DEF-006 Browser POC）→ **51 个测试套件全部 ok，0 failed**，含两个新增授权测试。已清理 25.6GiB 可再生构建缓存后以干净缓存重跑，日志 `rust-workspace-tests.log`（exit=0）。
- UI Reuse：`node scripts/check-ui-reuse.mjs` → **C1~C8 全部 pass**（整改前 C3/C5/C8 为 fail）。C5 最差页面对由 44 行/13.4% 降到 **26 行/8.6%**；`check:ui-reuse:regression` 8/8、0 regressions。前端 `npm run build` 通过，`npm run test` 35 files / 247 tests 通过，`npm run lint` 0 errors（3 项既有 warnings）。
- 依赖扫描：升级前 `cargo audit` → 2 vulnerabilities；升级 `url` 后 → 1；升级 rusqlite/sqlx 后 → **`cargo audit` exit=0，0 vulnerabilities**，仅剩 3 项 unmaintained 警告（chrono/paste/ttf-parser，均无可用修复版本）。证据：`cargo-audit-sqlite-upgrade.log`。
- SQLite 升级后 Rust 全 workspace 测试重跑：**51 个套件全部 ok，0 failed**（`sqlite-upgrade-tests.log`，exit=0）；`cargo fmt --check` 通过。
- Phase 0 原始门禁：`docs/testing/evidence/gate-6b30321-20261124/`（整改前，gate_result=FAIL，含门禁工具自身两个环境 bug）。
- 整改后门禁（SQLite 升级后最终一次）：`docs/testing/evidence/gate-post-remediation-20261124/gate-run-after-sqlite-upgrade.json`。结果 **PASS=8 / FAIL=0 / SKIP=3 / BLOCKED=2**，gate_result=**PASS_WITH_LIMITATIONS**。
  - PASS：rust_gate、sidecar_contract、endpoint_map、frontend_lint、frontend_build、frontend_test、tauri_syntax、security_scan（漏洞已清零）。
  - 历史：升级依赖前的最后一次门禁为 PASS=7 / FAIL=1（`gate-run-final.json`），唯一 FAIL 是 `libsqlite3-sys 0.30.1` 的真实漏洞 RUSTSEC-2026-0459。
  - 中间一次运行曾因构建缓存膨胀到 25.6GiB 触发 `rustc-LLVM ERROR: IO failure on output stream: no space on device` 及 advisory DB 拉取失败；清理可再生缓存后重跑已消除这两项环境问题（`gate-run.json` 与 `gate-console.log` 保留了该过程）。
  - SKIP：endpoint_map_baseline、contract_verify、frontend_install；BLOCKED：tauri_build（无 WebKit2GTK）、e2e（无 harness）。

## 3. 未关闭项与原因（需负责人决策）

| 缺陷 | 状态 | 原因 / 下一步 |
|---|---|---|
| P1-01 其余实体（conversations/messages/drafts/follow-ups/imports/audit/settings/AI/backups/browser） | 未整改 | 本轮只覆盖 candidates；需按同一模式分批推进，并为每类实体补跨租户负向测试 |
| P1-03 当前 HEAD 全栈门禁 | 部分完成 | 已在整改工作区重跑门禁；但 HEAD 未变、证据不绑定新 commit。需在改动提交后用新 commit 重跑并归档 |
| P1-04 Python/Rust 契约对拍 | 部分完成 | 发现 WSL 中的真实基线 `/home/bryan/projects/ica` @ `9ed173c`；离线 `contract_diff` 286/286 通过并归档 (`contract-parity-20261124.md`)；`d39e321`/`9ed173c` 矛盾按 ADR-0006 D-1 裁决为 9ed173c 并回写契约文档。待完成：用当前 HEAD 跑完整 `contract_verify.py` 并绑定 SHA |
| P1-05 Windows 桌面回归 | 阻塞（已补本地证据） | Phase 6 UI bundle 哈希与清单不匹配（`ui-integrity-20261124.md`）；已有 2026-11-23/24 原始日志与 7/9 验收记录，但 NSIS 安装包本体不存在、W4/W5 未执行、无签名/无 tag，需人工实机验收 |
| P2-02 Rust lint 债务 | 未整改 | `missing_docs/unwrap_used/expect_used` 仍为 allow；约 661 处 unwrap/expect，收紧需分 crate 清理 |
| P2-03 Browser POC 跳过 | 未整改 | 3 项测试仍按函数名被门禁排除，需可启动 Chromium 的隔离环境排查挂起 |
| P2-04 核心业务 E2E | 未整改 | 仓库无 E2E harness，门禁仍 BLOCKED；需新建测试工程与 CI 数据夹具 |
| P2-06 数据库故障注入 | 未整改 | 备份恢复演练有历史证据，但无 DB 锁/约束故障注入用例 |
| P2-08 安装包签名与追溯 | 阻塞 | 需签名证书、发布 tag、制品清单的人工流程 |
| 审计分支文档合入 | 未执行 | `origin/audit/full-audit-r1.0` 的报告尚未合入 main，需负责人决定是否合并及如何纳入 `docs/index.md` |

## 4. 风险接受要求

- libsqlite3-sys 漏洞已通过共享依赖升级关闭（`cargo audit` 0 vulnerabilities）；但 P1-04 契约对拍、P1-05 Windows 验收、P2-08 签名等仍未关闭，因此当前版本仍不应标记为“正式发布验收通过”。
- `AssertSqlSafe` 是对内部常量型动态 SQL 的人工审计标记，不提供任何输入清洗；新增 SQL 时必须继续用 bind 参数传递请求数据（约定见 `crates/ica-infra/src/lib.rs` 模块文档）。
- 在 MultiUser 全实体授权矩阵完成前，保持 `NETWORK_AUTHENTICATION_READY = false` 与 loopback-only 监听，不得为测试便利绕过。
- 本记录只证明所列改动与验证已执行，不构成独立安全认证或第三方渗透测试结论。
