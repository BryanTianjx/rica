# P1-04 契约对拍证据（2026-11-24）

## 本地基线断言（修正审计记录中的“无 Python baseline”）

审计证据索引称本机无 Python baseline；实测该结论已过时：

- WSL 发行版：`Ubuntu-26.04`（`wsl -l -q` 可见）
- 真实基线仓：`/home/bryan/projects/ica`（WSL），`git status --porcelain` 为空
- 基线解释器：`/home/bryan/projects/ica/backend/.venv/bin/python` → Python 3.12.14、FastAPI 0.133.0、uvicorn 0.34.0
- `/mnt/o/ica` 只有 `backend/.venv/site-packages`，无源码、无 `.git`、无 `ica.db`；因此 `contract_verify.py` 的默认路径（`PY_BASELINE=/mnt/o/ica/backend`）不可用，必须显式指定 `PY_BASELINE_REPO=/home/bryan/projects/ica`。

注意：该基线仓的 `refs/heads/main` 写有 `6b303212…`（在本仓中是有效提交，但在基线仓中不是有效对象），而 commit-graph 把 HEAD 解析为 `9ed173c…`。验收记录必须同时写 ref 值与对象/树哈希，不能只取 `git rev-parse --short HEAD`。

## 基线锚点矛盾已裁决

- `d39e321` → `9ed173c` 的重锚由 **ADR-0006 D-1** 决定；`docs/migration/snapshot-r1.md:14` 与 `wave-acceptance/W4/baseline-commit.txt` 均记录 `9ed173cdc6736f1213fc5697537c7f2e3a433709`。
- 本次已把 `docs/rust-migration-contract.md` 顶部、§1.1、§3.1 与历史程序段落的过时引用改为“权威=9ed173c，d39e321 仅为历史 diff 锚点”，并同步 `README.md`；历史 ADR 与评估报告保持原样，不改写历史记录。
- `git diff --stat d39e321 9ed173c`（基线仓内）= 152 files, +32563/−717，详见 `docs/migration/baseline-diff-report-r1.md`。

## 已执行的离线契约对拍

命令：

```
cd /mnt/o/ICA-RUST
PYTHONDONTWRITEBYTECODE=1 /home/bryan/projects/ica/backend/.venv/bin/python -B \
  scripts/contract_diff.py \
  --python docs/contract-baseline/w1-python.json \
  --rust docs/contract-baseline/w1-rust.json
```

结果：**286/286 cases matched, 0 failures, exit=0**，日志 `contract-diff-offline.log`。

边界：该结果绑定 2026-11-22 的冻结 capture，不证明当前未提交工作树的全部 68 契约；输出同时列出 364 个未被边界探针覆盖的路径（`docs/contract-baseline/contract-coverage.json`）。`check_endpoint_map.py --self-test` 的 5/5 断言同样只验证比较逻辑。

## 仍待完成

用显式环境变量跑一次完整的 `contract_verify.py`（双服务 + 当前 Rust 工作树 + 新 capture），需要可用端口 8010/8011 与工作目录 `/tmp/ica-contract/...`；这一步会写 capture，须在最终提交后以新 HEAD 执行并绑定 SHA。
