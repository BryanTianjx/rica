# NSIS 安装包重建与安装记录（2026-11-26 / 2026-11-28 更新）

## 第二次重建（会话面板修复，2026-11-28 安装验证）

- 源码基线：`9bb131e29cc34c323483e6fd5cc48f989facad1e`
  （`fix(ui): 浏览器运行时启动后会话面板仍显示未启动`）
- 安装包：`C:\ica-target\release\bundle\nsis\ICA Personal_0.2.2_x64-setup.exe`
- SHA256：`6e32b01fa1f2695ad1e58d808db76d88953f37f3eb848b99b67090a3ba90c3f7`（223 MB）
- 构建说明：Rust 侧复用缓存（1m18s）；makensis 报 `os error 32`（文件被占用）但安装包
  已完整产出并验证可用——安装后 UI 引用新 bundle、manifest 为新 commit。
- UI 组件更新：`ui/assets/index-CIWrFsan.js`
  （`99a0fc1395db8f9f191150b2e90487b41e4724c63c4571ad29b498dc8216a35b`）、
  `ui/index.html`（`f067ee8581ab3bc55e708bd3658ff43066e42ba27d5cf656bc429ecf6a18109b`）；
  `version-manifest.json` 的 ui 组件哈希按「排序后各文件 SHA256 十六进制串拼接」重新计算
  （`254a80b68f790a70241f4ed4d4a31e737f1a60bf1c8c8005fdf0dd139b211040`，bytes 430044）。
  注意：原 tree-hash 算法未知，旧值无法复现，本次以该确定性口径重算并在此记录。
- 安装：`/S` 静默安装退出码 0；安装后 `ui/index.html` 引用 `index-CIWrFsan.js`，
  `version-manifest.json` 的 `build_commit=9bb131e…`。
- 应用验证（2026-11-28）：
  - 启动已安装应用，浏览器页在运行时停止状态下正确显示「运行时未启动」；
  - 点击「启动运行时」→ sidecar `runtime/status` 变为 `ready`（cold_start 8.0s）；
  - 会话面板的轮询取到 ready 后解除阻塞（该行为由 vitest 回归测试
    `enables session creation after the runtime starts elsewhere` 钉住）；
  - 在同一运行时上 `POST /api/browser/v1/sessions` → 201 `ses_452ea482801b4f8c987f8d56`，
    随后 `DELETE` 关闭（200）。测试会话已清理。
- GUI 自动化限制：运行时的 headed Chromium 窗口出现后，CUA 辅助进程观察超时并重启，
  「新建会话」按钮的最终点击未能在 GUI 层留影；以上以「新 JS 已被加载 + 轮询逻辑有
  单元回归测试 + 运行时/会话 API 端到端 201」作为替代证据。

## 第一次重建（API Key 修复，2026-11-26）

- 安装包 SHA256：`b2703cf2e656c93210c534d14b6af2a560cfe88777b8748522d4c0af07b22bd5`（已被第二次构建覆盖）
- 源码基线：`0052e5eeb807c7ff9c056d5dad2bae5cc7b7b86c`
  （`fix(ai): 修复桌面端配置 API Key 后生成草稿仍提示不可用`）
- 静默安装：`Start-Process ... -ArgumentList '/S' -Wait`，退出码 0
- 安装目录：`O:\temp`（沿用现有 per-user 安装位置）

### 组件哈希

| 组件 | SHA256 |
|---|---|
| `ica-server.exe`（新构建） | `d8892a6dfc7cbf1cfdb4955442c2628f1a5fadf9729c874ed1b5a0e63aafe813` |
| `sidecar/app/app/auth/deps.py` | `6cd3462ca06c53656e5ac85b35a3632f3f855d55f667eef03ef3e0ea974820b2` |
| `sidecar/app/app/auth/security.py` | `64810d8a3e061dc30cdbf6150131bf3076b8677fe09a2aad502e03ea62260264` |
| `sidecar/app/app/config.py` | `a812b6175945253821cb3c02c8e72b1bba2f333ae5611f69c3fbb68bde526157` |
| `sidecar/app/tests/test_internal_assertion.py` | `a090f732c1c18c8c373cc041474004738889b8f5d104fc931b5385043f554e55` |

`SHA256SUMS` 与 `version-manifest.json`（`build_commit` / `built_at`）已同步更新；
安装后的 `O:\temp\bundle-resources\ica-server.exe` 哈希与上表一致。

### 重建步骤

1. 释放 O: 盘空间：删除过期的仓库根 `target/` 构建缓存（15 GB，全部新构建使用 `CARGO_TARGET_DIR=/c/ica-target`）。
2. `CARGO_TARGET_DIR=/c/ica-target cargo build --release --locked -p ica-server`（5m50s，退出码 0）。
3. 复制新 `ica-server.exe` 到 `desktop/tauri-host/bundle-resources/`，更新 `SHA256SUMS` 与 `version-manifest.json`。
4. `cd desktop/tauri-host && CARGO_TARGET_DIR=/c/ica-target tauri build`（约 13m，退出码 0，NSIS 222.91 MiB）。
5. 用 `/S` 静默安装到 `O:\temp`，退出码 0。

### 修复验证（安装后的二进制）

用真实数据副本启动**已安装**的 `ica-server.exe`：

| 步骤 | 结果 |
|---|---|
| `PUT /api/settings/ai-key`（dummy key） | 200，`ai_enabled=true`，返回脱敏尾号 |
| `GET /api/settings` | `ai_enabled=true`（GUI 指示不再误报未配置） |
| `GET /api/ai/models` | `api_key_configured=true` |
| Windows 凭据管理器 | 出现 `ICA_ai_api_key` 条目（`LegacyGeneric:target=ICA_ai_api_key.ICA`） |
| `POST /api/drafts/generate` | 越过 key 解析，达到 provider（dummy key 返回 401，503 `ai_provider_error`） |
| `DELETE /api/settings/ai-key` | 200，`ai_enabled=false`，凭据条目被移除，`api_key_configured=false` |

验证用 dummy 凭据已清除，本地验证服务器已停止。随后启动已安装桌面应用：sidecar 与
按需启动的 Rust 服务器健康检查正常，UI 正常加载，无崩溃。

## 限制

- 安装包**未做 Authenticode 签名**（本机无代码签名证书），也没有 release tag，不能作为正式发布制品。
- `desktop/tauri-host/bundle-resources/**` 被 `.gitignore` 排除；本记录是该暂存区在本地构建状态的证据。
- Sidecar Python 侧（AUTH-08 断言）改动位于同一被忽略目录，按
  `docs/security/RICA-P1-AUTH-08-Sidecar-Python变更说明.md` 同步，仍需 CI 实跑。
- 交互式安装/升级、W4/W5 实机回归与 `docs/testing/desktop-release-acceptance.md` 人工 sign-off 尚未执行。
