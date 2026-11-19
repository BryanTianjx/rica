# Migration Snapshot R1（G0-01 交付物）

> G0 卡（迁移基线治理）§范围 G0-01：固定 ICA-RUST HEAD、工作树、`Cargo.lock`、tag 清单。
> 本快照为不可变记录——后续任何波次的验收结论引用它时以 SHA 为准（ADR-0005 / ADR-0006）。

## 快照事实（2026-11-20 实测）

| 项 | 值 | 取数命令 |
| --- | --- | --- |
| ICA-RUST HEAD | `1fefda6171abdfddd29fa71dfe6ad96f94ffec50` | `git rev-parse HEAD` |
| HEAD 摘要 | `feat(W7): browser extraction confirm-candidate — the only page-to-candidates path` | `git log --oneline -1` |
| 工作树 | 干净（提交后） | `git status --porcelain` |
| `Cargo.lock` sha256 | `542ac5ed35d27377…`（完整值见下） | `sha256sum Cargo.lock` |
| 基线锚点 | `9ed173cdc6736f1213fc5697537c7f2e3a433709`（Python 基线终态，D-1 重锚） | `git -C /mnt/o/ica rev-parse HEAD` |
| 基线 `ica.db` sha256 | `466aa3fccf96e98d…`（未变） | `sha256sum /mnt/o/ica/backend/ica.db` |

完整 `Cargo.lock` 哈希：

```
542ac5ed35d27377 (sha256sum Cargo.lock，前 16 位；提交内文件为准)
```

## Tag 清单（波次里程碑）

```
w0-done  w1-done  w1.5-candidate  w1.5-done  w1.6-candidate  w1.6-done
w2-candidate  w2-done  w2.5-contracts-candidate  w2.5-done
w3-candidate  w3-done  w3-done-r2
w4-candidate  w4-done
```

- `w4-done` 建立于 `0e4c10c`（验收记录提交），实现绑定 `66a0534`；
  判据见 `wave-acceptance/W4/`（ADR-0005 不可变验收记录）。
- W7 的默认范围已实现（`1fefda6`），**尚未打 `w7-candidate`/`w7-done`**：
  W7 卡 §5 的 5 条 browser 路由归属由 B0-10 裁决（本卡 v2 已记录
  「两者都可，但不能留空」），且 W7 的独立验收（Q4）未执行——
  按 ADR-0005，Q0–Q3 全绿不构成放行论据。

## 本快照覆盖的提交窗口

```
c8740b6  docs(adr): accept ADR-0004/0005/0006 and rule D-12
0c66b4c  docs(adr): rule that the four ai/models routes belong to AI-EXT
66a0534  fix(W4): close every blocker the second independent audit found
0e4c10c  docs(W4): the immutable acceptance record, bound to 66a0534
1fefda6  feat(W7): browser extraction confirm-candidate
```

## 门禁实测（快照时刻）

| 门 | 值 |
| --- | --- |
| `bash scripts/check.sh` | 5/5 关通过（layering / fmt / clippy / tests / 端点对账） |
| 测试 | 348 passed / 0 failed（另 OCR 批次后见最新验收记录） |
| 端点对账 | contract=68 implemented=45 missing=23 extra=0 auxiliary=1（D-2） |
| 跨语言对拍 | 234/236 cases matched；2 条失败 = dashboard `calendar`（G0-04 处理中，见 baseline-diff-report） |
