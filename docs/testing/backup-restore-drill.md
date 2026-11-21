---
title: "备份/恢复演练(backup/restore drill)"
document_id: "TEST-ICA-DRILL-001"
document_type: "TestPlan"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "TEST-ICA-PLAN-R1.0"
  - "DEF-ICA-REGISTER-001"
supersedes: null
---

# 备份/恢复演练 R1.0

契约验收前置(SPE-BAS-003 引用 / 契约文档 §3 W5 行:"backup/restore drill documented
before implementation")。本文档是实现 `/api/backups` 三件套的验收前置物,也是 QA-001
Recovery Test 类别的操作手册。

## 1. 机制(与 Python 基线逐条对齐)

| 环节 | 机制 | 打开句柄的处理 |
|---|---|---|
| 创建备份 | SQLite **在线备份 API**(`sqlite3_backup`,经 rusqlite)读源库写目标文件 | 与活库并存,WAL 未 checkpoint 的事务不丢失 |
| 恢复前安全点 | 同上,活库 → `pre-restore-*.db` | 同上(基线明确记录:裸文件复制会丢 WAL 数据,故必须走备份 API) |
| 恢复 | 备份文件 → 活库,同样**通过备份 API 原地写入**(同 inode,页级替换) | 打开的连接在新读事务中看到恢复后的内容;恢复后执行 `PRAGMA wal_checkpoint(TRUNCATE)` 强制落盘一致 |
| 保留策略 | 两族(`ica-backup-*` / `pre-restore-*`)按名排序,超出 `backup_keep_count` 删最旧;恢复只产生第二族,同样受剪 | — |
| 权限 | POSIX 0600(cfg(unix));Windows 依赖用户级目录 ACL(与基线 DPAPI 用户域一致) | — |

## 2. 演练矩阵(实现后逐项执行并记录)

| # | 场景 | 预期 | 状态 |
|---|---|---|---|
| D1 | 空库 POST /api/backups → 得到 ica-backup-*.db,GET 列表含它,kind=backup | 201 + 记录字段完整(size_bytes/created_at) | ☐ |
| D2 | 写入业务数据(POST /api/positions)→ 再备份 | 新备份含该数据;恢复旧备份后数据消失;再恢复新备份数据回来 | ☐ |
| D3 | restore 无 confirm → 409 business_rule_violation(零写入) | 台账不变、磁盘不变 | ☐ |
| D4 | restore 带路径分隔符/`..`/空名 → 422(名称而非路径) | 同左 | ☐ |
| D5 | restore 不存在的名字 → 404 not_found | 同左 | ☐ |
| D6 | restore 成功 → 响应含 restored_from/database/previous_copy;audit_events 有 `backup.restored`(metadata.previous_copy) | 同左 | ☐ |
| D7 | 恢复过程中并发读(恢复后立刻 GET /api/positions) | 新读事务看到恢复后数据,无 WAL 错误 | ☐ |
| D8 | keep_count 剪枝:连续创建 N+1 份,最旧的被删 | 两族合计不超上限 | ☐ |
| D9 | 重启服务 → 数据为恢复后的状态 | 持久性 | ☐ |
| D10 | 服务运行中删除备份目录文件 → GET 列表不再含它;restore 它 → 404 | 同左 | ☐ |

## 3. 边界与已知偏差

1. **多用户守卫不可达**:基线的 `admin_required`/`restore_not_tenant_scoped` 只在
   multiuser 模式触发;Rust 无 auth(W6),恒为 local 语义 — 守卫分支随 W6 落地。
2. **进程内连接回收**:基线 restore 后 `engine.dispose()`;Rust 经备份 API 原地写入 +
   checkpoint 达到同等效果(sqlx 池按操作取连接,无跨请求事务),偏差记录于实现模块注释。
3. **`.db-wal`/`.db-shm` 孤儿清理**:随 create/restore 各执行一次(基线 `_remove_orphan_sidecars`)。

## 4. 记录

每次演练按 QA-001 §7.2 记录(commit/环境/时间/结果/证据),登记到 test-report。
