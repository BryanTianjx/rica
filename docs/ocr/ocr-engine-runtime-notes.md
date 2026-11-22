# OCR 引擎运行时备注（OCR-01-01 / 01-09 交付）

> 依据 W4 复审后 OCR-01 卡的收敛项：依赖来源 / 版本 / 许可记录完整；
> ONNX Runtime 分发与平台兼容有结论。数据为 2026-11-20 实测。

## 依赖与许可（OCR-01-01）

| 依赖 | 版本 | 许可 | 来源 |
| --- | --- | --- | --- |
| `rapidocr-core` | 0.3.0 | **Apache-2.0**（crate `Cargo.toml` `license` 字段原文） | crates.io（`index.crates.io-…` 注册表缓存） |
| ONNX Runtime | `ort` crate 所绑定版本（`ort.pyke.io` 预编译分发） | MIT（ORT 项目） | `ort.pyke.io` 下载缓存，按目标三元组分发 |
| 模型文件 | PP-OCRv6 small 套件 | 与基线同一套文件（SHA256 逐个核对一致，见 `docs/contract-baseline/w4-python-facts.md` §13） | 随桌面宿主分发 / 基线 venv 同源 |

### crate 依赖精确版本与许可（桌面端分发结论）

| 依赖（crate） | 精确版本 | 许可证（crate `Cargo.toml` `license` 字段原文） | 桌面端分发结论 |
| --- | --- | --- | --- |
| `rapidocr-core` | 0.3.0 | Apache-2.0 | **可随桌面端分发**（Apache-2.0 允许再分发，随分发物保留版权与许可声明） |
| `ort` | 2.0.0-rc.15 | MIT OR Apache-2.0 | **可随桌面端分发**（MIT / Apache-2.0 二选一，随分发物附对应许可文本） |
| `image` | 0.25.10 | MIT OR Apache-2.0 | **可随桌面端分发**（MIT / Apache-2.0 二选一，随分发物附对应许可文本） |

上表版本取自 `Cargo.lock`，许可证取自 `index.crates.io-…` 注册表缓存中各 crate 的
`Cargo.toml` `license` 字段原文（2026-11-20 实测）。三者均为宽松许可，
**可随桌面端分发**，分发时随附对应许可与版权声明即可；与上文「随桌面宿主分发」的口径一致。

模型套件（四个文件，31 MB）：

```
PP-OCRv6_det_small.onnx
PP-OCRv6_rec_small.onnx
ch_ppocr_mobile_v2.0_cls_mobile.onnx
ppocrv6_dict.txt
```

## 体积与分发（OCR-01-09）

| 项 | 实测值 | 说明 |
| --- | --- | --- |
| release `ica-server` 二进制 | 48 MB | 静态链接 ONNX Runtime |
| ONNX Runtime 预编译缓存 | 107 MB（按三元组） | `ort.pyke.io` 首次构建时下载，不入仓库 |
| debug target 全量 | ~44 GB（ext4） | `CARGO_TARGET_DIR=$HOME/.cache/ica-rust-target`；**仓库卷 `/mnt/o` 不放 target**（20 GB 盘曾被打满——教训：临时 `cargo build` 必须带该环境变量，`check.py` 与 `contract_verify.py` 已内置） |
| 模型目录 | 31 MB | 运行时解析顺序见 `ocr_rapid.rs::model_dir` 文档注释 |

## 平台兼容

- 引擎为 CPU 推理（无 CUDA 依赖）；`ort` 预编译覆盖 `x86_64-unknown-linux-gnu`（本机），
  Windows/macOS 由 `ort.pyke.io` 按三元组提供。
- 推理被 `Mutex` 串行化（`RapidOcr` 需要 `&mut self`）；4 并发实测完全串行
  （23.1 s vs Python 无锁并发 13.0 s）——**已知且合理的取舍**（桌面端拖多图会排队），
  产品上需可接受；阻塞线程占用与并发上限在 G3/B 线的运行时设计中评估。
- 内存：RSS 平台期 ~703 MB（ONNX arena + 图像缓冲），无泄漏（fd 恒定）；
  为 Python 基线的 ~2.7 倍——体积优化不在本卡范围。

## 运行时解析与恢复（本轮收敛后）

- **OCR-01-04 已关闭**：`model_dir()` 不再回退到基线 Python venv；
  解析顺序 `RAPIDOCR_MODEL_DIR` → `ICA_OCR_MODEL_DIR` → `ICA_DATA_DIR/ocr/models`
  → 相对 `models`（缺模型 = 可读 503 `ocr_unavailable`，非 panic）。
  `contract_verify.py` 与测试各自显式设置模型目录。
- **初始化失败可重试**（LWW P3-1 关闭）：引擎槽位为 `Option`，启动失败不再被
  `OnceLock` 缓存成终态——下一个请求重试，装好模型无需重启进程。
- **锁中毒恢复**（LWW P3-2 / OCR F6 关闭）：引擎互斥锁中毒时 `into_inner()` 恢复
  而非永久 500；真正损坏的引擎走 `Local OCR failed` → **500 `ocr_error`**
  （基线 `ICAError` 默认状态，OCR F2 关闭：原 409）。
- **`details.reason` 文案**（OCR F3 关闭）：不可识别文件 → Pillow 原文
  `cannot identify image file '<path>'`；截断文件 → `image file is truncated`。
