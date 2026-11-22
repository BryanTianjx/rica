# ICA-RUST Architecture Dependency Map — ARCH-P0 / AC1

**基线 HEAD（完整 SHA）：** `7213f424c51f1a20f2b911bc5d6f651dd0244c3f`（短 SHA `7213f42`）
**分支 / worktree：** `wt/ica-rust-arch-p0` @ `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0`
**取数时点：** 2026-11-22（CST） · **性质：** 纯只读盘点（不改源码/配置，不 git add/commit/push）
**git 状态：** 取数时 `git status --porcelain` 为空；本卡仅新增 `docs/architecture/` 文档。

## 1. 取数命令原文（全部在 HEAD 7213f424c51f1a20f2b911bc5d6f651dd0244c3f 上执行）

```bash
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p0
git rev-parse HEAD                                   # 7213f424c51f1a20f2b911bc5d6f651dd0244c3f
git rev-parse --abbrev-ref HEAD                      # wt/ica-rust-arch-p0
git status --porcelain=v1                            # 空
cargo metadata --no-deps --format-version 1          # exit 0，workspace_members = 7
cargo tree -p ica-domain --prefix none                # 无 axum/sqlx/tokio/hyper
cargo tree -p ica-application --prefix none           # 无 axum/sqlx/tokio/hyper
PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH -u PYTHONHOME python3 -c \
  "import sys; sys.path.insert(0,'scripts'); import check; print(check.layering_violations())"
# 输出：[]  → PASS (0 violations)
```

`cargo metadata --no-deps --format-version 1` 原始输出已留存（32,619 字节）；下表为其 `packages[].dependencies` 的直接抽取，非人工估算。

## 2. Crate 依赖图（7 个 workspace member）

| Crate | 层 | 内部依赖（workspace） | 外部依赖（normal） | 禁止框架命中 |
|---|---|---|---|---|
| `ica-api` | 适配器/组合根 | `ica-application`, `ica-contracts`, `ica-domain` | `axum`, `chrono`, `form_urlencoded`, `serde`, `serde_json`, `serde_urlencoded`, `tracing` | axum |
| `ica-application` | 内层（guarded） | `ica-domain` | `async-trait`, `chrono`, `regex`, `serde_json` | 0 |
| `ica-contracts` | 内层（guarded） | `ica-application`, `ica-domain` | `chrono`, `form_urlencoded`, `serde`, `serde_json`, `serde_urlencoded` | 0 |
| `ica-desktop` | 适配器/组合根 | `ica-application`, `ica-contracts`, `ica-domain`, `ica-infra` | `chrono`, `futures-util`, `serde`, `serde_json`, `sha2`, `sqlx`, `tokio`, `tokio-tungstenite`, `tracing` | sqlx, tokio |
| `ica-domain` | 内层（guarded） | — | `chrono`, `regex`, `sha2`, `unicode-normalization` | 0 |
| `ica-infra` | 适配器/组合根 | `ica-application`, `ica-domain` | `aes-gcm`, `async-trait`, `base64`, `bcrypt`, `chrono`, `image`, `jsonwebtoken`, `keyring`, `rand`, `rapidocr-core`, `regex`, `reqwest`, `rusqlite`, `serde_json`, `sha2`, `sqlx`, `tokio`, `tracing`, `uuid` | sqlx, tokio |
| `ica-server` | 适配器/组合根 | `ica-api`, `ica-application`, `ica-infra` | `axum`, `tokio`, `tracing`, `tracing-subscriber` | axum, tokio |

依赖方向（仅列 workspace 内边）：

```
ica-domain  ← ica-application ← ica-contracts ← ica-api ← ica-server
                    ↑                ↑             ↑
                    ├── ica-infra ───┘             │
                    └── ica-desktop ───────────────┘
```

- `ica-infra` 依赖 `ica-application` + `ica-domain`（适配器 → 内层，方向正确）。
- `ica-desktop` 依赖 `ica-application` + `ica-contracts` + `ica-domain` + `ica-infra`。
- `ica-api` 依赖 `ica-application` + `ica-contracts` + `ica-domain`（`ica-infra` 仅 dev 依赖）。
- `ica-server` 是组合根，依赖 `ica-api` + `ica-application` + `ica-infra`。

## 3. 分层约束（禁止边）核验

约束出处：`docs/adr/ADR-0002-rust-backend-migration-reinstated.md` §3 Consequences/Positive
（「`ica-domain` 和 `ica-application` 不能依赖 axum/sqlx/tokio/hyper」）与 §5 Revision 1
（「`ica-contracts` joins the compile-enforced inner layer (no axum/sqlx/tokio)」）。
任务单写作「§5 分层约束」，实际禁止边条文分布在 §3 与 §5，二者均已核对。

机器可执行闸门：`scripts/check.py` 定义
`FORBIDDEN_IN_INNER_LAYERS = {"axum","sqlx","tokio","hyper"}`、
`ADAPTER_CRATES = {"ica-infra","ica-api","ica-server","ica-desktop"}`、
`GUARDED_CRATES = {"ica-domain","ica-application","ica-contracts"}`。
闸门不只查框架名，也禁止内层直接依赖任何适配器 crate（防 `ica-application → ica-infra` 把 sqlx 夹带进来）。

| 受保护 crate | 直接依赖中的禁止框架/适配器 | 传递闭包（cargo tree）命中 | 结论 |
|---|---|---|---|
| `ica-domain` | 0 | 0 | PASS |
| `ica-application` | 0 | 0 | PASS |
| `ica-contracts` | 0 | 未单独展开（直接依赖仅 chrono / form_urlencoded / serde / serde_json / serde_urlencoded + 两个内层 crate） | PASS |

闸门实跑：`check.layering_violations()` 返回 `[]` → **0 violation**。

**结论：AC1 VERIFIED。** 禁止边计数 = 0；7 个 crate 的依赖图与 ADR-0002 §3/§5 一致，内层保持框架无关。

## 4. 明确不做
未改任何源码/配置；未重跑 fmt/clippy/test（S0 已做门禁数字，见 `docs/audits/stage0-baseline-report.md`）；未 git add/commit/push。
