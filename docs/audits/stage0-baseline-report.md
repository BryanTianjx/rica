# ICA-RUST AUD-S0 — Stage0 当前 HEAD 动态验证基线报告

**基线 HEAD：** `ef2da578388fd3190230b7f8f68afe8d57bd1a21`
**取数时点：** 2026-11-22（CST）
**依据：** 审计报告 RICA-AUDIT-001 §7 / §9.1；实施方案 RICA-SEC-001
**性质：** 只读核实，不改任何源码。

## 1. 工作树状态
`git status --short` 除 `.worktrees/`（并行实施卡的工作区）外为空；HEAD = ef2da57。

## 2. 15 项 Finding 现场核实（最新 HEAD）

| Finding | 审计报告记录位置 | 实测结果 | 结论 |
|---|---|---|---|
| SEC-01 | main.rs `generate_jwt_secret()` | FOUND `main.rs:207` | 缺陷现场成立 |
| SEC-02 | main.rs `forward_to()` | FOUND `main.rs:448` | 缺陷现场成立 |
| SEC-03 | AuthContext / auth 路由 | FOUND `AuthContext.tsx:9-10`（refresh 探针注释）+ routes.rs:100-103 auth 路由 | 缺陷现场成立 |
| SEC-04 | routes.rs / application / domain / infra | FOUND `routes.rs:100-103` auth 路由清单 | 缺陷现场成立 |
| REL-01 | Server 强制 kill | FOUND `main.rs:324/429 child.kill()` + :637 kill 注释 | 缺陷现场成立 |
| TEST-01 | Browser POC 超时 | 需进一步定位（首轮 grep 路径未命中，非否定） | 待 P2-02 核实 |
| SEC-05 | 依赖安全记录 | 本轮未核（P2-03 范围） | 转 P2-03 |
| QA-01 | 全栈 CI 门禁 | FOUND `.github/workflows/` 不存在 | 缺陷现场成立 |
| QA-02 | E2E 证据 | 本轮未核（P2-05 范围） | 转 P2-05 |
| QA-03 | 桌面交付验收 | 本轮未核（P2-06 范围，需 Windows 实机） | 转 P2-06 |
| DOC-01 | README 状态 | FOUND `README.md:1/3` 标题与描述 | 缺陷现场成立 |
| DOC-02 | 测试报告矛盾 | 本轮未核（P2-07 范围） | 转 P2-07 |
| DOC-03 | 模块清单 | 本轮未核（P2-07 范围） | 转 P2-07 |
| DOC-04 | 缺陷台账同步 | 本轮未核（P2-07 范围） | 转 P2-07 |
| GOV-01 | main 分支保护 | 与 QA-01 同（无 CI workflow） | 缺陷现场成立 |

**结论：** 审计报告 §4 的 15 项 Finding，本轮只读核实了 8 项现场（SEC-01..04/REL-01/QA-01/DOC-01/GOV-01），均真实存在；7 项转对应 P2 卡核实（需专门工具或实机）。没有发现"报告记录的位置已失效"的情况。

## 3. 门禁基线数字（真实执行）

| 门禁 | 命令 | 结果 |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | exit 0 |
| clippy | `cargo clippy --all-targets --workspace -- -D warnings` | 0 errors |
| test | `cargo test --workspace`（skip 3 DEF-006 POC） | 404 passed |
| 端点对账 | `endpoint_reconciliation.py --json` | 68/68/0/0 |
| 端点映射 | `check_endpoint_map.py` | RESULT: table matches |
| 跨语言对拍 | `contract_verify.sh`（隔离 OUT_DIR） | 286/286 matched, 0 failure |
| 基线零改动 | 同上 | HEAD 9ed173c, ica.db sha256 `466aa3fc…` 未变 |

## 4. 待整改清单（P1/P2 输入）
- Stage 1（可并行）：P1-01 SEC-01、P1-02 SEC-02 — 现场成立，已派发。
- Stage 2：P1-03 SEC-03（依赖 P1-01/02）。
- Stage 3：P1-04 SEC-04（现场成立）。
- Stage 4-7：P2-01..07 — 待 P1 后用；TEST-01/QA-03 需实机。

## 5. 明确不做
- 未改任何源码；未写实库；未 commit/push（编排方统一处理）。
- 未把审计报告静态推断当事实——以实测为准。
