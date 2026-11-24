# P1-05 / P2-08 桌面产物完整性与阻塞项（2026-11-24）

## 本地可计算的 UI bundle 完整性

工作区（已在本分支的单独批次提交）中的 `desktop/tauri-host/bundle-resources/ui/` 共 3 个文件、431,224 字节：

| 文件 | 字节 | SHA256 |
|---|---:|---|
| `index.html` | 528 | `d6d8138ada56c85826861ef8d3d9bb7ac8c3f1db9eb7b4997f53c6b010fd4af9` |
| `assets/index-C2OzubNE.css` | 19,143 | `432771ccf5fec6b39b96fab5379865d211fe111460050d29bf6e11d9c9092c1f` |
| `assets/index-DDE_bnUM.js` | 411,553 | `51df3a7b5568f0696767cab4dd2b500f19856a6c6e2ca1c21f3c3b00b21f1e5e` |

这些哈希是在本地工作区实测的，可作为 Phase 6 UI 产物的完整性记录。

## 与已发布清单不一致（重要）

`desktop/tauri-host/bundle-resources/version-manifest.json` 记录的 UI 仍是旧产物：

- `ui.bytes = 404812`（当前 431224）
- `ui.sha256 = 934c04262bf71c39683479cb73a45312e3460045fef7f13a61154d463136118d`
- 资产名为 `index-BmqP3wF0.js` / `index-CY80JeYa.css`（当前为 `index-DDE_bnUM.js` / `index-C2OzubNE.css`）
- `index.html` 记录 `2d91e440…`，当前 `d6d8138a…`

因此当前 UI 产物**不能**被当作已签名/已发布的制品：清单与产物不匹配。必须先重建清单并重新做验收，而不是仅替换文件。

## 已存在的历史验收证据（只读归档）

`docs/testing/evidence/windows-acceptance/` 下已有原始日志/JSON（2026-11-23/24）：`install-silent.log`、`uninstall.log`、`restart-after-kill.log`、`launch-first.log`、`host-runtime.log`、`sidecar-runtime.log`、`verify-embedded-server-fix.log`、`stage-rust-server.log`、`tauri-nsis-build*.log`、`windows-acceptance-20261123T141500Z.json`、若干 API JSON。`docs/testing/desktop-release-acceptance.md` 为 `EVIDENCED_PARTIAL 7/9`；细分为 W1 仅静默安装通过、W2/W3/W6/W7/W8 PASS、W4/W5 NOT_EXECUTED、W9 PARTIAL（无 Authenticode 签名、无 release tag）。

`O:\temp\ica-ui6-install` 等隔离安装目录保留 `ica-server.exe`（40,230,400 B）、`ica-desktop-host.exe`（44,715,520 B）、`uninstall.exe`（117,866 B）及各自 `version-manifest.json` / `SHA256SUMS`。这些是构建输出，不是可追溯的签名制品。

## 严格阻塞项（需要人或外部条件）

- 仓库内与 `O:\` 下均**没有 NSIS installer 二进制本体**；构建日志中的 `ICA Personal_0.2.2_x64-setup.exe`（222.58 MiB，sha256 `82f89b5b…` / 重建 `ec5ccb3b…`）无法再次哈希或签名。
- `signtool.exe` 存在（Windows SDK），但本机**没有代码签名证书/私钥**（无 `.pfx`/`.p12`，证书存储中只有 Adobe/测试/设备证书）。
- 没有 0.2.2 的 release tag；仅有 `w0-done` … `w4-done` 波次 tag。
- `desktop-release-acceptance.md` §3 明确禁止 agent 自我签核；W1 交互式安装与 W4/W5 升级/迁移验收、W9 签名和人工 sign-off 均需负责人执行。
