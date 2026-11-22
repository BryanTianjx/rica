# SEC-02 Tauri HTTP Proxy Header 修复 — 安全验证记录

**任务单：** AUD-P1-02（Finding SEC-02，P1）
**worktree：** `/mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-02` @ `ef2da57`，分支 `wt/ica-rust-aud-p1-02`
**范围：** `desktop/tauri-host/src/main.rs::forward_to` 的 Header 转发策略
**状态：** 实现完成 + 本机可验证部分全绿；**独立安全验证未执行，SEC-02 不得标 Verified**（verifier ≠ 实现方）

---

## 1. 现场核实（AC1）

```
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-02 && grep -n "forward_to" desktop/tauri-host/src/main.rs
448:fn forward_to(port: u16, method: &str, path: &str, body: &[u8]) -> Result<(u16, Vec<u8>), String> {
485:        if let Ok((200, _)) = forward_to(port, "GET", "health", b"") {
564:        return match forward_to(port, &method, &api_target, &body) {
```

修复前 `forward_to` 只接收 `(port, method, path, body)`，请求头由本地 `format!` 重建：

```
"{} /{} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n"
```

`Authorization`、`X-Encrypt-Aes-Key`、`Accept`、`X-Request-ID` 等全部丢失；响应头也被整体丢弃（`handle_ica` 硬编码 `content-type`）。与审计报告 §5.2 一致。

---

## 2. Header 策略表（AC2 / AC3）

策略真源下沉到 `crates/ica-contracts/src/proxy_headers.rs`（纯 std，无 axum/sqlx/tokio/hyper），沿用项目 F1-3/F1-4 的下沉模式：host 只做 socket 接线，所有判定与测试在下层 crate。

### 2.1 请求方向（浏览器 → 本机 Backend）

| Header | 策略 | 依据 |
|---|---|---|
| `Authorization` | **允许**（原值透传） | SEC02-T01；Bearer token 必须到达 Server |
| `X-Encrypt-Aes-Key` | **允许** | SEC02-T02；加密导出信封密钥 |
| `Content-Type` | **允许** | 业务 JSON body |
| `Accept` | **允许** | 内容协商 |
| `X-Request-ID` | **允许** | 链路关联 ID |
| `Content-Length` | **由 Proxy 按真实 body 计算**，不信任输入 | §4.4：不能信任不一致的用户输入；渲染函数忽略任何同名入参 |
| `Host` | **Proxy 生成** `127.0.0.1:<port>` | §4.4 禁转；防任意目标主机 |
| `Connection` | **Proxy 生成** `close` | hop-by-hop |
| `Connection` 中声明的 token | **全部禁转** | §4.4「对 Connection Header 中声明的其他 hop-by-hop Header 也应正确处理」 |
| `Proxy-Authenticate` / `Proxy-Authorization` / `TE` / `Trailer` / `Transfer-Encoding` / `Upgrade` / `Keep-Alive` | **禁转** | §4.4 hop-by-hop 清单（RFC 9110 §7.6.1） |
| `Cookie` / `User-Agent` / `Sec-Fetch-*` 等浏览器头 | **禁转** | 不在 allowlist；不无差别转发 |
| 名称含 CR/LF 或非 token 字符 | **丢弃**（非 allowlist）/ 渲染期报错 | CRLF 注入防护 |
| 允许头的值含 CR/LF/NUL/控制字符 | **拒绝**（`UnsafeValue`） | CRLF 注入 / 请求走私 |
| 单头 > 8 KiB / 头总长 > 16 KiB | **拒绝**（明确错误） | §4.5.3 限制 Header 总大小 |

### 2.2 响应方向（Backend → 浏览器，**独立 allowlist**）

| Header | 策略 | 依据 |
|---|---|---|
| `Content-Type` | 允许（缺失时 host 保留 `application/json` 默认） | 与既有前端行为兼容 |
| `Content-Length` | 允许 | 原值透传 |
| `Cache-Control` / `ETag` / `Last-Modified` | 允许 | 标准缓存语义 |
| `Location` | 允许 | 基线尾斜杠 307 必须带绝对 URL + query |
| `WWW-Authenticate` | 允许 | SEC02-T09：401 challenge |
| `X-Request-ID` | 允许 | 链路关联 |
| `Connection` / `Transfer-Encoding` / `Keep-Alive` 等 hop-by-hop | **禁转** | 代理自己用 `Connection: close` 分帧 |
| Backend 内部诊断头（如 `X-Secret-Internal`） | **禁转** | §4.6：请求侧允许不代表响应侧允许 |

`Proxy 不承担业务授权`（SEC-04 职责）：本修复只保证认证/加密头不丢、恶意头不进，不新增任何业务授权判断，也不改变业务请求语义。

---

## 3. 实现落地（AC2 / AC3 / AC4）

| 文件 | 改动 |
|---|---|
| `crates/ica-contracts/src/proxy_headers.rs`（新增） | 请求/响应 allowlist、hop-by-hop 黑名单 + `Connection` token 解析、名称/值/请求行校验、请求渲染、响应解析、错误类型（**只带头名不带值**）、6 个测试 |
| `crates/ica-contracts/src/lib.rs` | `pub mod proxy_headers;` |
| `desktop/tauri-host/src/main.rs` | `forward_to` 第 4 参改为 `&tauri::http::HeaderMap`，返回 `ForwardResponse`；`handle_ica` 在 `into_body()` 前 `request.headers().clone()`，用响应 allowlist 逐项写回 builder；`poll_health` 适配新签名 |

`forward_to` 现在只是：取头 → `select_request_headers` → `render_request`（生成 Host/Connection/Content-Length）→ 写 socket → `parse_response`（响应 allowlist）。不再有任何从浏览器输入手拼请求头的路径。

---

## 4. 测试矩阵（AC4）

| 编号 | 场景 | 覆盖用例 | 结果 |
|---|---|---|---|
| T01 | Authorization 到达 backend | `business_headers_reach_backend_and_browser_junk_does_not` | PASS |
| T02 | X-Encrypt-Aes-Key 到达 backend | 同上 | PASS |
| T03 | 自定义头按白名单处理 | 同上（`accept`/`x-request-id` 保留；`user-agent`/`cookie`/`sec-fetch-*` 被丢） | PASS |
| T04 | hop-by-hop 不转发 | `connection_declared_hop_headers_are_dropped_and_content_length_is_computed` | PASS |
| T05 | 非法头拒绝/过滤 | `malicious_headers_are_rejected`（CRLF 注入、超大值、非法请求行/头名） | PASS |
| T06 | 超大 Header 明确错误 | 同上（`HeaderTooLarge`） | PASS |
| T09/T10 | 401/500 状态与响应头 | `response_headers_are_filtered_with_a_separate_allowlist`（401 + body） | PASS |
| T11 | Proxy 与直连一致 | `proxy_matches_direct_http_for_business_headers`（同一 backend 两次连接逐头比对） | PASS |
| — | host↔策略源码漂移守卫 | `host_wires_the_shared_policy`（`include_str!` 解析 host 源码） | PASS |

未覆盖（明确标注）：T07 超大 Body（项目尚无 body 上限策略，属独立决策）、T08 任意端口访问（端口只来自 `route_backend` 的两个 `AtomicU16`，无用户输入）、T12 日志泄露（host 日志不写 header 值，仅人工核对）。实机 Windows/Webview 行为与 `cargo check -p ica-desktop-host` 因缺 glib-2.0/WebKit 未执行。

---

## 5. 实测输出（AC5 证据）

```console
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-02 && git diff --check
EXIT=0

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo fmt --all -- --check
EXIT=0

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test -p ica-contracts
test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target bash scripts/check.sh
== 1/4 layering ... ok
== 2/4 cargo fmt --check ... ok
== 3/4 cargo clippy --all-targets -- -D warnings ... ok（无 warning/error）
== 4/4 cargo test ... 3 条 browser_poc 超时失败（见下）
```

`check.sh` 第 4 关失败的三条：`a_worker_crash_is_contained_and_the_runtime_restarts`、`the_full_poc_happy_path_with_state_records`、`the_worker_env_carries_no_database`，均为 `worker reply timeout (30s)`，位于未改动的 `crates/ica-desktop/tests/browser_poc.rs`（Chromium/Playwright 环境相关，与 SEC-02 无关）。按项目惯例显式跳过这三个函数名后：

```console
$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace \
    -- --skip a_worker_crash_is_contained_and_the_runtime_restarts \
       --skip the_full_poc_happy_path_with_state_records \
       --skip the_worker_env_carries_no_database
TEST_EXIT=0
汇总：44 个 test target，410 passed / 0 failed / 3 filtered out
```

host 源码语法检查（不要求系统库）：

```console
$ rustfmt --edition 2021 --check desktop/tauri-host/src/main.rs desktop/tauri-host/src/ica_backend.rs
Diff in .../main.rs:225 / :339 / :708 / :717   # 4 处均为 HEAD 既有格式差异
```

基线对照（同一 rustfmt，把 HEAD 的两个文件复制到 scratch 后检查）报 `Diff in :224 / :338 / :678 / :687` —— 同一批既有差异，行号随本次插入平移。**本次 SEC-02 改动未引入任何新的 rustfmt 差异**；未对这 4 处做无关格式化以保持 diff 最小。

---

## 6. 兼容性

- 前端 `api.ts` 实际只发 `Content-Type`（有 body 时）和 `Authorization`（有 token 时），两者均在 allowlist → 桌面行为不变。
- `X-Encrypt-Aes-Key` 加密导出链路恢复，与直连 HTTP 行为一致。
- 响应侧保留 `Location`，尾斜杠 307 在桌面端不会被降级；保留 `WWW-Authenticate`，401 与直连一致。
- 无 content-type 的后端响应仍回填 `application/json`，保持既有默认。
- 未改契约、未改端点、未改业务授权语义；Proxy 仍只访问 `127.0.0.1` 上由 host 自己启动/采纳的两个端口。

**已知限制：** 后端若用 `transfer-encoding: chunked`（本次两个后端在 `Connection: close` 下均回 `Content-Length`），代理会把 chunked 分帧字节当 body 透传；该 hop-by-hop 头不会转发给前端。如需支持应在后续卡中做反分帧，不属本次范围。

---

## 7. 回滚

未做 git add/commit/push；回滚只需在 worktree 内还原三个文件：

```bash
cd /mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-02
git checkout -- crates/ica-contracts/src/lib.rs desktop/tauri-host/src/main.rs
rm crates/ica-contracts/src/proxy_headers.rs
```

回滚后恢复旧的 `forward_to(port, method, path, body)`（丢 Header）行为，前端认证/加密头丢失缺陷重新出现。不建议在无替代方案时回滚。

---

## 8. 签署

- 实现方自验：AC1–AC4 完成，本机可编译部分 AC5 证据如上。
- **独立验证：未执行。** 按任务单 AC5，SEC-02 转 Verified 必须由 verifier ≠ 实现方复核本报告并重跑上述命令；在独立验证记录落库前，状态保持 `Implemented / Pending Independent Verification`。
