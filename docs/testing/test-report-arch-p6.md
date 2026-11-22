---
title: "ARCH-P6 测试与执行报告（当前 HEAD）"
document_id: "TEST-ICA-ARCH-P6-R1"
document_type: "TestReport"
project: "ICA-RUST"
version: "1.0.0"
status: "Active"
created: "2026-11-23"
updated: "2026-11-23"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "TEST-ICA-PLAN-R1.0"
  - "ARCH-P6"
  - "TEST-ICA-DRILL-001"
supersedes: null
---

# ARCH-P6 测试与执行报告（绑定具体 HEAD）

## 0. Provenance（先于任何数字）

| 项 | 值 |
|---|---|
| Git HEAD | `40575e1aa78b316cede43104e3e78c9e00956130` |
| Worktree / 分支 | `/mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p6` / `wt/ica-rust-arch-p6` |
| 执行时间（UTC） | 统一门禁 `2026-11-22T19:12:02Z` → `2026-11-22T19:16:06Z`；备份演练 `2026-11-22T19:08:14Z`（0.88s） |
| Rust | `rustc 1.98.0 (88d9e12ae 2026-08-18)` / `cargo 1.98.0 (797e8a9bc 2026-08-05)` |
| Node / npm | `v24.19.0` / `12.0.2` |
| 主机 | `Linux TIAN 6.18.58.1-microsoft-standard-WSL2 x86_64`，Ubuntu 26.04.1 LTS |
| Cargo target | `CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target`（另用全新 `/home/bryan/.cache/ica-rust-target-fresh-p6` 做 ENOENT 排除，见 §5） |
| 执行记录 | `/home/bryan/.hermes/cache/scratch/arch_p6_gate_run2.json`（门禁）；`/home/bryan/.hermes/cache/scratch/arch_p6_backup_drill.json`（演练） |
| 未 git add / commit / push | 是。所有产物留在该 worktree 工作区。 |

> 本报告只描述**这次 HEAD 的这次执行**。历史 `docs/testing/test-report-r1.0.md`
> 中的 248/248、286/286、365、384/385 等数字是不同 HEAD / 不同用例集的历史记录，
> 按 DOC-02 的要求与本报告**分离**，不再作为当前验收基线。

## 1. 逐条真实命令与输出

### 1.1 工作区洁净性（改文档之前的基线）

```text
$ cd /mnt/o/ICA-RUST/.worktrees/ica-rust-arch-p6 && git diff --check
EXIT=0
```

### 1.2 Rust 格式 / clippy / 全量测试

```text
$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo fmt --all -- --check
FMT_EXIT=0

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo clippy --all-targets --workspace -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.13s
CLIPPY_EXIT=0

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target cargo test --workspace -- \
    --skip the_full_poc_happy_path_with_state_records \
    --skip the_worker_env_carries_no_database \
    --skip a_worker_crash_is_contained_and_the_runtime_restarts
...
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
...
tests 汇总（脚本解析 50 条 test result 行）:
  passed=522  failed=0  ignored=0  filtered_out=3
CARGO_TEST_EXIT=0
```

3 条 `filtered out` 是 DEF-006 的 browser_poc POC（看函数名跳过，不是跳 test target）：
实测 `scripts/check.py` 不带 skip 时在本机 `timeout 240` 后退出码 124（挂起），
所以统一门禁通过 `CARGO_TEST_EXTRA_ARGS` 把它们明确记为 Skipped，不计入 Passed。

### 1.3 端点对账（统一门禁内复用既有脚本）

```text
$ python3 scripts/endpoint_reconciliation.py --json   # 摘要
contract_total=68  implemented=68  missing=0  extra_routes=0
auxiliary_routes=1  non_contract_routes=1  matrix_rows=69
coverage: contract=68/68 implementation=68/69 verification=68/69

$ python3 scripts/check_endpoint_map.py --self-test
self-test: 5/5 assertions passed (no-exclusion mismatch, exclusion match,
new-route mismatch, ADR-0008 exact exclusion, ADR-0008 unlisted-route mismatch)
```

基线实机对拍 `endpoint_map_baseline` 本轮为 SKIP：`/mnt/o/ica/backend/.venv`
不存在，无法导入 live FastAPI app；自测已覆盖比较逻辑，但没有伪造基线结果。
`scripts/contract_verify.py` 为 opt-in（需基线 + 两个服务端口），本轮未跑，SKIP。

### 1.4 前端（lint / typecheck+build / vitest）

```text
$ npm ci --offline --prefix frontend
added 168 packages, and audited 169 packages in 30s
found 0 vulnerabilities
NPM_CI_EXIT=0

$ npm run lint          # 3 条 unicorn warning，退出 0，非门禁阻断
LINT_EXIT=0

$ npm run build         # tsc -b && vite build
✓ 62 modules transformed.
dist/assets/index-D02J2ulQ.js  395.20 kB │ gzip: 115.35 kB
✓ built in 762ms
BUILD_EXIT=0

$ npm run test          # vitest run
 RUN  v5.0.5
 Test Files  25 passed (25)
      Tests  225 passed (225)
   Duration  174.72s
VITEST_EXIT=0
```

### 1.5 Sidecar wire contract（既有 Rust 测试，未重造）

```text
$ cargo test -p ica-contracts --test sidecar_contract_test
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 2. 统一门禁执行记录（AC1 的可复跑证据）

```text
$ python3 scripts/gate.py --out /home/bryan/.hermes/cache/scratch/arch_p6_gate_run2.json
ARCH-P6 unified gate | HEAD=40575e1aa78b316cede43104e3e78c9e00956130
...
== GATE SUMMARY ==
HEAD=40575e1...  PASS=7  FAIL=0  SKIP=3  BLOCKED=3
gate_result: PASS_WITH_LIMITATIONS
INDEPENDENT SECURITY VERIFICATION: PENDING (manual) — this gate does NOT self-sign Verified.
WINDOWS DESKTOP ACCEPTANCE: BLOCKED (no Windows host)
GATE_EXIT=0
```

| stage | 状态 | exit | 秒 | 说明 |
|---|---|---|---|---|
| rust_gate | PASS | 0 | 61.09 | 复用 scripts/check.py；522 passed / 0 failed / 3 skipped；contract=68 implemented=68 |
| sidecar_contract | PASS | 0 | 0.49 | 既有 6 条 sidecar wire contract 测试 |
| endpoint_map | PASS | 0 | 0.08 | check_endpoint_map.py --self-test |
| endpoint_map_baseline | SKIP | — | — | 基线 venv 不存在（见 §1.3） |
| contract_verify | SKIP | — | — | opt-in，需冻结基线 + 两服务 |
| frontend_install | SKIP | — | — | node_modules 已存在（此前已 npm ci 实测成功） |
| frontend_lint | PASS | 0 | 0.56 | oxlint |
| frontend_build | PASS | 0 | 4.95 | tsc -b + vite build |
| frontend_test | PASS | 0 | 177.02 | vitest：25 files / 225 tests |
| tauri_syntax | PASS | 0 | 0.12 | rustfmt 解析 host 源码（**不是**编译） |
| tauri_build | **BLOCKED** | — | — | 本机无 glib-2.0 / webkit2gtk-4.1 |
| security_scan | **BLOCKED** | — | — | cargo-audit 未安装；独立安全验证待人工 |
| e2e | **BLOCKED** | — | — | 仓库无 E2E harness（QA-02 / DEF-003） |

**`PASS_WITH_LIMITATIONS` ≠ Verified。** 三个 BLOCKED 与三个 SKIP 都带原因，
门禁不把它们折算成通过；安全验证明确待人工。

## 3. AC4 备份/恢复演练（真实执行，非只读代码）

```text
$ python3 scripts/backup_restore_drill.py --out /home/bryan/.hermes/cache/scratch/arch_p6_backup_drill.json
[C0] build throwaway baseline via scripts/seed_contract_db.py (replays crates/ica-infra/schema/ica-baseline.sql)
seeded .../baseline.db: 32 tables, 7 positions, 6 candidates, 2 conversations, 5 messages, 4 send records
  CHECK PASS: seed script produced a baseline database — exit=0, exists=True
[C1] baseline snapshot captured with a probe row inserted
  CHECK PASS: baseline snapshot contains the probe row — probe_present=True
  CHECK PASS: baseline DDL digest is non-trivial — objects=200, ddl_sha256=4146a1a8546c7385
[C2] backup created through SQLite online-backup API (sqlite3_backup)
  CHECK PASS: backup is byte-distinct and snapshot-equal to baseline — snapshots_equal=True, backup_sha256=993f0020e7156e78
[C3] live database mutated: probe row deleted (proves restore has work to do)
  CHECK PASS: mutation removed the probe and changed row counts — probe_present=False, rows 59 -> 58
  CHECK PASS: mutation did not alter the DDL digest — ddl_sha256=4146a1a8546c7385
[C4] pre-restore safety copy taken through the same online-backup API
  CHECK PASS: pre-restore safety copy preserves the mutated state — safety_snapshot_equals_mutated=True
[C5] restore executed in place from backup through sqlite3_backup + PRAGMA wal_checkpoint(TRUNCATE)
  CHECK PASS: restored DDL digest equals baseline DDL digest — 4146a1a8... == 4146a1a8...
  CHECK PASS: restored per-table row counts equal baseline row counts — all 32 tables compared
  CHECK PASS: restored total rows and probe row equal baseline — rows=59, probe_present=True

BACKUP/RESTORE DRILL: PASS (10/10 checks)
DRILL_EXIT=0
```

机制说明：演练走 `sqlite3.Connection.backup()`，即 `sqlite3_backup`，与
`crates/ica-infra/src/backup.rs`（rusqlite `backup::Backup`）同一路径；用临时库，
**未触碰** `/mnt/o/ica/backend/ica.db`。这是机制级证据，不替代 Windows 实机的
W6 项（见 `docs/testing/desktop-release-acceptance.md`）。

## 4. 关于共享 target 陈旧二进制的 ENOENT 风险（如实说明）

本轮**未遇到** `sec01_t03` ENOENT：共享 target 全量运行中该用例为
`test jwt_key::tests::sec01_t03_no_time_or_pid_in_generators ... ok`。
但确实检出共享 target 中存在烘焙旧 worktree 路径的二进制，例如：

```text
$ strings /home/bryan/.cache/ica-rust-target/debug/deps/positions_http-e672db3773095eb0 | grep -m2 '.worktrees/'
/mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-04
/mnt/o/ICA-RUST/.worktrees/ica-rust-aud-p1-04
```

为排除「代码缺陷」可能，用**全新** CARGO_TARGET_DIR 复跑同一用例：

```text
$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target-fresh-p6 \
    cargo test -p ica-infra --lib jwt_key::tests::sec01_t03_no_time_or_pid_in_generators -- --exact
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out
FRESH_EXIT=0

$ CARGO_TARGET_DIR=/home/bryan/.cache/ica-rust-target \
    cargo test -p ica-infra --lib jwt_key::tests::sec01_t03_no_time_or_pid_in_generators -- --exact
test result: ok. 1 passed; 0 failed; ...
SHARED_EXIT=0
```

结论：该风险本轮未触发，代码无缺陷。陈旧二进制属**环境卫生**，本卡没有删除
共享缓存（避免影响其它 worktree）；按约定应在相关 worktree 收尾后清理并复跑。

## 5. 未验证 / 不得外推的项

1. **AC3 Windows 实机**：BLOCKED（用户已接受的环境受限项），逐项待验清单与验收方法见
   `docs/testing/desktop-release-acceptance.md`；不自签通过。
2. **Tauri 真编译 / 安装包**：本机缺 glib/WebKit，只做了 `rustfmt` 语法检查；不等于构建通过。
3. **独立安全验证**：待人工。门禁的 `security_scan` 为 BLOCKED（无 cargo-audit）；
   DEF-007/008 的 advisory 证据在 `docs/testing/sec09-audit-r1.log`，仅为既有记录。
4. **E2E**：仓库无 harness（DEF-003 Triaged），AC1 只串接入口，未声称 E2E 通过。
5. **基线侧对拍 / 全量 contract_verify**：本轮 SKIP，未产生新的对拍结论。
6. **GitHub 分支保护设置**：需管理员落实，本卡只提供规则文档（`.github/BRANCH_PROTECTION.md`）。
