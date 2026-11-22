# SEC-01 JWT 密钥安全治理 — 安全验证记录

- 任务单：AUD-P1-01（SEC-01，P1，Stage 1）
- 依据：审计报告 §5.1 / RICA-SEC-001 §3（含 §3.4 生成、§3.5 持久化、§3.6 轮换兼容、§3.7 测试）
- Worktree：`/mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-01`，分支 `wt/ica-rust-aud-p1-01`，起点 `ef2da57`
- 状态：**实现完成，待独立安全审查**。按任务约束，实现方不代签 Verified；verifier ≠ implementer 审查通过后缺陷台账 SEC-01 方可转 Verified。
- 凭据约定：任何真实/测试密钥值全程以 `[REDACTED]` 表示；本文只记录密钥槽名、权限位、字节长度等非值信息，不记录任何具体凭据值。

## 1. 改前 / 改后行为

| 项目 | 改前（HEAD @ ef2da57） | 改后 |
|---|---|---|
| 生成入口 | `desktop/tauri-host/src/main.rs:207` `generate_jwt_secret()` | 同签名函数（现 `main.rs:211`）仅做委托 |
| 随机源 | `SystemTime::now()` 纳秒 + `std::process::id()` 异或，格式化为 32 hex 字符 | `rand::rngs::OsRng::fill_bytes`（OS CSPRNG），32 字节 / 256 bit（`ica-infra/src/jwt_key.rs`） |
| 可预测性 | 时间与 PID 均属可预测输入；无密码学保证 | 无可预测输入；无 SystemTime、无 PID、无固定默认 |
| 持久化 | 无，每次启动重新生成（per-launch） | OS 凭据存储优先（Windows Credential Manager / macOS Keychain，走 `keyring`，复用 `secret_store.rs` 的 `ICA` / `ICA_` 命名）；Linux 开发主机回退到 `<data root>/secrets/JWT_SIGNING_KEY.secret`，创建即 `0600` |
| 重启行为 | 密钥变化，历史 token 全部失效 | 槽中密钥有效时复用（会话连续性）；槽缺失/损坏才重新生成 |
| 轮换 | 无显式轮换 | `JwtKeyProvider::rotate()` 生成并覆盖单一活动槽，旧密钥 `Drop` 时挥发性清零（volatile wipe） |
| 失败策略 | 无失败路径 | 存储不可用 → 返回 `KeyError::Unavailable` 并 fail-fast（host `expect`），绝不静默退回固定密钥 |
| 日志/配置 | 值不经日志，但生成逻辑散落在 host | `JwtKey` 的 `Debug`/`Display` 均渲染 `[REDACTED]`；存储错误只含槽名与系统错误；密钥仅通过进程环境 `JWT_SECRET` 交给签名方 |

改前现场核实命令与输出（行号）：

```
$ git show HEAD:desktop/tauri-host/src/main.rs | nl -ba | sed -n '204,214p'
   204  /// A per-launch JWT secret (32 hex chars) for the Rust server's token
   205  /// signing. stdlib-only: time + pid are enough for a per-run secret that
   206  /// never persists.
   207  fn generate_jwt_secret() -> String {
   208      let seed = std::time::SystemTime::now()
   209          .duration_since(std::time::UNIX_EPOCH)
   210          .map(|d| d.as_nanos())
   211          .unwrap_or(0)
   212          ^ ((std::process::id() as u128) << 64);
   213      format!("{seed:032x}")
   214  }
```

改后同一位置：

```
$ nl -ba desktop/tauri-host/src/main.rs | sed -n '211,217p'
   211  fn generate_jwt_secret() -> String {
   212      let provider = ica_desktop::jwt_key::JwtKeyProvider::new(resolve_data_root());
   213      let key = provider
   214          .load_or_create()
   215          .expect("JWT signing key store unavailable: refusing to start with a predictable secret");
   216      key.to_hex()
   217  }
```

## 2. 文件清单

| 文件 | 变更 |
|---|---|
| `crates/ica-infra/src/jwt_key.rs` | 新增。CSPRNG 生成、`JwtKey`（Debug/Display 输出 `[REDACTED]` + volatile wipe）、`JwtKeyStorage` 存储后端（OS keyring / 0600 文件 / 测试内存）、`JwtKeyProvider::load_or_create` / `rotate`、9 个 SEC-01 单测 |
| `crates/ica-infra/src/lib.rs` | 增加 `pub mod jwt_key;` |
| `crates/ica-desktop/src/lib.rs` | 增加 `pub use ica_infra::jwt_key;`（host 通过此单一入口取密钥） |
| `desktop/tauri-host/src/main.rs` | 替换 `generate_jwt_secret()` 实现；更新模块头注释；无其他逻辑改动 |
| `docs/audits/sec-01-jwt-key-report.md` | 新增（本文） |

未改动：任何 `Cargo.toml` / `Cargo.lock`（全部复用现有 `rand 0.8`、`keyring 3`，无新依赖）；招聘业务领域模型；local 单用户模式；`ica-server` 的 `JWT_SECRET` 读取逻辑。

## 3. 密钥生命周期

1. **生成**：`generate_jwt_secret() -> [u8; 32]`，`OsRng.fill_bytes`。32 字节 = 256 bit（SEC01-T01）。
2. **持久化**：`load_or_create()` 先读单一活动槽 `JWT_SIGNING_KEY`。值为合法 64 位 hex → 直接复用；缺失/损坏 → 生成新密钥并写回。写文件使用 `OpenOptions::mode(0o600)` 创建并对已存在文件再次 `set_permissions(0o600)`；目录位于数据根，不在项目目录、不在明文配置。
3. **轮换**：`rotate()` 生成新密钥覆盖活动槽；旧 `JwtKey` 离开作用域时被挥发性清零。仅保留一个活动密钥，无历史验证密钥槽。
4. **销毁**：`Drop` 以 `ptr::write_volatile` 逐字节写 0，防止优化器消除清零；文件/凭据槽的删除沿用 `secret_store.rs` 的 `delete` 语义（本任务未改动该模块）。
5. **不泄露**：`JwtKey` 无暴露字节的 `Display`/`Debug`；`to_hex()` 是唯一取值入口，仅用于 `JWT_SECRET` 进程环境传递，调用方不记录该字符串。

## 4. 兼容性（RICA-SEC-001 §3.6）

- 现有签名方 `ica-infra/src/auth_store.rs` 使用 `jsonwebtoken::EncodingKey::from_secret(self.jwt_secret.as_bytes())`。`JwtKey::as_bytes()` 提供同一字节接口；host 侧仍以 hex 字符串经 `JWT_SECRET` 注入，签名协议与 Claims 不变。
- 当前实现只有 **当前签名密钥**一个槽，没有历史验证密钥，也没有 `kid`。轮换后旧密钥立即停止验证 —— 即旧 token **立即失效**（强制失效机制），这是明确的策略选择，不是静默降级。
- Token 最大有效期沿用现有配置：access token 15 分钟、refresh token 7 天（`ica-desktop/src/state.rs` 默认值）。轮换窗口内的 in-flight 请求需在 15 分钟内完成，否则需重新认证；本地单用户桌面场景下可接受。
- Python sidecar 仍自行创建其密钥（本次不在范围）；host 生成的密钥只交给 Rust `ica-server`，跨后端 token 互认语义未改变。
- 不引入 Token 自报密钥地址或外部 `kid` 解析，避免 §3.6 的可信标识问题。

## 5. 日志泄露检查（AC4）

检查命令（在 worktree 内）：

```
grep -rn -i "jwt_secret\|jwt_key\|jwt signing key\|generate_jwt" --include="*.rs" --include="*.toml" --include="*.py" . | grep -v '^./target/'
grep -rn "log_line\|tracing::\|println!\|eprintln!\|log::" --include="*.rs" desktop/tauri-host/src crates/ica-infra/src/jwt_key.rs | grep -i "secret\|jwt\|key"
```

结果：

- 第二条的匹配数为 **0**：没有任何日志语句引用 JWT 密钥/密钥值。
- 第一条命中的均为槽名、类型字段、环境变量名、测试夹具常量或非日志代码；唯一的参数传递点是 `desktop/tauri-host/src/main.rs:268` 的 `.env("JWT_SECRET", generate_jwt_secret())`，不经日志。
- `KeyError::Unavailable` 的文本只包含槽名与系统错误；`KeyError::InvalidStoredKey` 只说明槽内容非法，均不含值。
- 自动化测试 `sec01_t06_key_never_renders_in_logs` 断言 `Debug`/`Display` 输出包含 `[REDACTED]` 且不含 hex 值（值全程 [REDACTED]，不写入任何证据）。

## 6. 测试与实测证据

命令（worktree 内，`CARGO_TARGET_DIR` 指向本机空闲盘，因 `/mnt/o` 已满）：

```
cargo test -p ica-infra --offline -j 2
exit=0
test result: ok. 31 passed; 0 failed   # lib（含 9 项 SEC-01）
test result: ok. 13 passed; 0 failed   # database
test result: ok.  6 passed; 0 failed   # migrations
test result: ok. 11 passed; 0 failed   # positions_repository
```

SEC-01 专项（`cargo test -p ica-infra --offline -j 2 jwt_key`）：

| 测试 | 覆盖 | 结果 |
|---|---|---|
| `sec01_t01_secret_is_256_bit_hex` | SEC01-T01：32 字节 / 64 hex / 往返解析 | ok |
| `sec01_t02_independent_generations_do_not_repeat` | SEC01-T02：独立生成不复用固定值 | ok |
| `sec01_t03_no_time_or_pid_in_generators` | SEC01-T03：源码级断言 host 与生成器函数体无 SystemTime / UNIX_EPOCH / process::id，且含 OsRng + fill_bytes | ok |
| `sec01_t04_restart_reuses_persisted_key` | SEC01-T04：模拟重启复用持久化槽 | ok |
| `sec01_t05_unavailable_store_fails_closed` | SEC01-T05：存储故障 fail-closed，无固定密钥回退 | ok |
| `sec01_t06_key_never_renders_in_logs` | SEC01-T06：Debug/Display 红acted | ok |
| `sec01_t07_rotation_replaces_the_active_key` | SEC01-T07：轮换换槽、重启后新键生效 | ok |
| `sec01_t08_invalid_stored_value_is_regenerated` | SEC01-T08：非法槽值重新生成 | ok |
| `file_fallback_creates_0600_key_outside_project_dir` | 0600 权限 + 数据根路径（unix） | ok |

`git diff --check`：退出码 0，无 whitespace 错误。

格式检查：

```
rustfmt --edition 2021 --check crates/ica-infra/src/jwt_key.rs crates/ica-infra/src/lib.rs crates/ica-desktop/src/lib.rs   -> exit 0
rustfmt --edition 2021 --check <仅 SEC-01 修改的 main.rs 函数片段>                                                   -> exit 0
rustfmt --edition 2021 --check desktop/tauri-host/src/main.rs                                                        -> exit 1
```

整份 `main.rs` 的 rustfmt 不通过源于 4 处**改动前即存在**的格式漂移（`main.rs:228` `spawn_rust_server` 签名、`:342` `spawn_sidecar` 签名、`:682`/`:691` `ICA_RUST_SERVER_PORT` 分支），与 SEC-01 改动无关；为避免范围外 diff，本次未重排这些代码。SEC-01 修改的片段单独 `rustfmt --check` 通过。

类型检查限制（如实记录）：本机无 `glib-2.0` / `webkit2gtk-4.1`（`pkg-config` 均 MISSING），`desktop/tauri-host` 无法完成类型检查：

```
$ cd desktop/tauri-host && cargo check --offline -j 2
error: failed to run custom build command for `glib-sys v0.18.1`
  pkg-config exited with status code 1
  > PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1 pkg-config --libs --cflags glib-2.0 'glib-2.0 >= 2.70'
  Package 'glib-2.0' not found
  The system library `glib-2.0` required by crate `glib-sys` was not found.
```

替代验证：核心逻辑下沉到可编译的 `ica-infra` 并由 9 项真实单测覆盖；`cargo check -p ica-desktop --offline` 退出码 0，验证 `pub use ica_infra::jwt_key` 导出链类型正确；host 侧 `generate_jwt_secret()` 由 `sec01_t03` 做源码级一致性断言（委托 provider、无时间/PID）。**host 二进制未经本机类型检查**，需在具备 Tauri 原生依赖的 CI/主机上补做。

## 7. 回滚

本次未 `git add` / `commit` / `push`，改动仅在工作区，回滚不产生历史记录：

```
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-01
git checkout -- crates/ica-infra/src/lib.rs crates/ica-desktop/src/lib.rs desktop/tauri-host/src/main.rs
rm crates/ica-infra/src/jwt_key.rs docs/audits/sec-01-jwt-key-report.md
```

回滚影响：恢复为时间+PID 生成的可预测密钥（SEC-01 重新打开），仅应在整体回退该整改时执行。已有持久化槽 `JWT_SIGNING_KEY`（值 [REDACTED]）在回滚后不再被读取，可依运维策略从 OS 凭据存储 / `<data root>/secrets/` 清除。

## 8. 待办（不构成实现缺陷）

- 在具备 glib-2.0 / webkit2gtk-4.1 的环境执行 `cargo check`/`cargo test -p ica-desktop-host`，补足 host 侧类型检查证据。
- 独立安全审查（verifier ≠ implementer）复核本记录后，方可将缺陷台账 SEC-01 转 Verified。
- 由编排方统一处理 `git add` / `commit` / `push`；本次实现方未执行上述任一操作。
