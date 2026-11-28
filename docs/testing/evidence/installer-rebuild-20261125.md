# NSIS 安装包重建记录（2026-11-25）

## 结果

- 安装包：`O:\ICA-RUST\desktop\tauri-host\target\release\bundle\nsis\ICA Personal_0.2.2_x64-setup.exe`
- 大小：233,719,997 bytes（222.89 MiB）
- SHA256：`04b7c800f604c41e08397e9c2a689ac3f770b439976df403187f0a3956e54690`
- 构建时间：2026-11-25 08:35:11 +0800
- 源码基线：`831f3db1b9a596c9686a795c043f1913eb3cb51b`（main，含 R1.0 审计整改与 SQLite 升级）

## 构建步骤

1. 安装 Tauri CLI：`npm install -g @tauri-apps/cli@2` → tauri-cli 2.12.6。
2. 从当前 main 重建服务器二进制：`cargo build --release --locked -p ica-server`（7m40s），产物 41,176,064 bytes，SHA256 `9eb28b32290f7d7d8723a8b71363be23fed78210f06cc7bafb60fc8a2a9aa473`。
3. 将服务器二进制复制到 `desktop/tauri-host/bundle-resources/ica-server.exe`，并更新：
   - `SHA256SUMS`：移除旧 UI 资产条目，写入当前 UI 三文件与 server 的哈希；
   - `version-manifest.json`：`build_commit` 改为当前 Rust HEAD（原先错误地指向 Python 基线 `9ed173cd…`），`built_at` 更新，`components.ui` 更新为当前 UI 树哈希。
4. 在 `desktop/tauri-host/` 执行 `tauri build`（11m19s 编译 + NSIS 压缩），退出码 0。

## 组件哈希（当前工作区实测）

| 组件 | SHA256 |
|---|---|
| `ui/index.html` | `b5d8b2b11128bc6bef416ec64b5b70e55419dc4ab2637a1eb5c7b77c450b9b8b` |
| `ui/assets/index-C2OzubNE.css` | `2167e41ecd20a55dddc359c46fb093713669e073d8a6e4fef0ac85ad4dd01547` |
| `ui/assets/index-DDE_bnUM.js` | `9385e98280a4b3d2df0da96f09fd87c9e8d8c0a7502560436f6e650e9352cb8b` |
| UI 目录树哈希（`ica-tree-v1`） | `291cd20fe685f95d8182d3fd336f5624cf11cc71fcd5383ce88b03aa799813c7`（3 files / 431,250 bytes） |
| `ica-server.exe` | `9eb28b32290f7d7d8723a8b71363be23fed78210f06cc7bafb60fc8a2a9aa473` |

## 限制

- 安装包**未做 Authenticode 签名**（本机无代码签名证书），也没有 release tag；不能作为正式发布制品。
- 尚未执行实机安装回归（W1 交互式安装、W4/W5 升级与迁移）；`docs/testing/desktop-release-acceptance.md` 的人工 sign-off 仍需负责人完成。
- `desktop/tauri-host/bundle-resources/*` 被 `.gitignore` 忽略，因此 server exe、SHA256SUMS 与 version-manifest 的更新不会随 git 提交传播；本记录是它们在本地构建时的证据。
- 构建使 `desktop/tauri-host/Cargo.lock` 发生依赖重解析变更（未提交）。
