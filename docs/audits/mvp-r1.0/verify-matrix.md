# 差距矩阵:VERIFY → 证据结论(verify-matrix)

**审计编号:** ICA-ENG-AUDIT-001 / WP-00
**核验日期:** 2026-11-21
**基准:** HEAD `ff7b9eb` + 当前工作区;证据均为 E1(另有标注 E2 者)
**统计:** 63 项 = **PASS 34 / PARTIAL 19 / FAIL 2 / MISSING 7 / VERIFY(未执行)1**
**状态定义:** 沿用 R1.0 §5(PASS/PARTIAL/FAIL/MISSING/VERIFY)

> 使用说明:本矩阵不把"未实现"当缺陷 — W5/W6(23 端点)、AI-EXT、F0 前端方案
> 均有治理归属;真正需要行动的项在文末"行动清单"。

---

## 5.1 核心业务功能(12 项)

| ID | 审计项目 | 状态 | 证据与说明 |
|---|---|---|---|
| BIZ-01 | Project CRUD | **MISSING** | 无 projects 表/端点/领域实体;Python 契约亦无 → 关联 S-1 范围裁决 |
| BIZ-02 | Position CRUD | **PASS** | 5 端点(api-inventory §2);28+17 HTTP 测试;IPC 5 命令;删除语义与基线一致(IMP-2) |
| BIZ-03 | Candidate CRUD | **PASS** | 7 端点含 timeline;级联删除语义与基线一致;21+ 测试 |
| BIZ-04 | Candidate Search | **PASS** | search/status/position_id/include_archived;domain search 模块;筛选测试通过 |
| BIZ-05 | Application 创建 | **MISSING** | 无 applications 实体 → 关联 S-1 |
| BIZ-06 | Application Lifecycle | **MISSING** | 同上 |
| BIZ-07 | Status History | **PARTIAL** | Application 状态史随实体缺位不存在;但现有域有完整替代物:audit_events 业务写同事务留痕(33 处)+ candidate timeline 端点 + follow-up 状态机校验 |
| BIZ-08 | Conversation | **PASS** | conversations/messages CRUD、content_hash、message_count、`last_message_at DESC NULLS LAST`,19 测试 |
| BIZ-09 | Follow-up | **PASS** | 10 端点全生命周期(create/patch/cancel/complete/record-result/policy/evaluate);状态机 + 幂等 + 过期清扫;45 个 infra 测试行数最大模块 |
| BIZ-10 | Reminder | **PASS** | evaluate 端点(不落库决策)+ 列表前过期清扫 + policy 阈值(首用自动建默认行) |
| BIZ-11 | Dashboard | **PASS** | `/api/dashboard` 由真实业务数据聚合(9 状态桶补零 + 日历;67e9d00 实现日历);IPC dashboard 同源 |
| BIZ-12 | AI Draft | **PASS**(工作流) | generate/approve/reject/copy + draft-groups + candidate_context;approve≠发送、copy≠发送(基线语义);真实 provider 延后至 AI-EXT(ADR-0003 补充裁决),见 AI-01 |

## 5.2 API 契约(10 项)

| ID | 审计项目 | 状态 | 证据与说明 |
|---|---|---|---|
| API-01 | REST 路由完整性 | **PARTIAL** | 已实现部分与契约**零漂移**(47 = 45+health+1 已记载例外);45/68,余 23 条 = W5+W6 已立项未实施 |
| API-02 | Request Schema | **PASS** | 手写校验器(body.rs 2,004 行/16 单测;query.rs 885 行/21 单测);pydantic 兼容 issue;HTTP/IPC 共享 |
| API-03 | Response Schema | **PASS** | DTO 单一真源 ica-contracts;parity fixture(positions 20 case、w1 214 case,E2 对拍 248/248) |
| API-04 | Error Contract | **PASS** | 统一信封 `{error:{code,message,details}}`;ContractError 传输无关;HTTP/IPC 信封逐字段同形(commands_smoke.rs:78-96);500 固定脱敏 |
| API-05 | Pagination | **PASS** | 统一 offset/limit(50/500;消息 200/1000)+ PageResponse{…has_more};timeline 例外与基线一致 |
| API-06 | Filtering | **PASS** | 各域过滤器测试覆盖(candidates 四过滤、send-records 四过滤等);重复 key 取最后一个(Python 语义) |
| API-07 | Validation | **PASS** | 边界:limit clamp 1..=500、超 u64 offset 422(IMP-4)、Python int() 语法、RFC3339/naive 多格式、坏 JSON 422 |
| API-08 | Transaction | **PASS** | 33 处业务写+审计同事务;PRAGMA foreign_keys 连接级开启(db.rs:142);测试断言 |
| API-09 | Idempotency | **PARTIAL** | 无 Idempotency-Key 头;领域级幂等 3 类(follow-up 复用、record-result、导入 upsert LWW + send_records append-only);与 Python 基线语义一致(COMPATIBLE),但未达 R1.0 重试控制要求 |
| API-10 | API Documentation | **PASS** | rust-migration-contract.md 与实现零漂移(本审计实测核对);无 OpenAPI 文件(风险:手维护文档,靠 47 端点核对脚本守护) |

## 5.3 数据层(10 项)

| ID | 审计项目 | 状态 | 证据与说明 |
|---|---|---|---|
| DB-01 | Schema | **PASS** | 32 表/121 索引/34 FK,由脚本从 Python 基线库只读导出("Do not hand-edit");文件头注释过时(20/66)为文档小缺陷 |
| DB-02 | Migration | **FAIL** | **零迁移机制**:无 refinery/sqlx-migrate/migrations 目录;测试整库快照;server 仅自检 2 张表。已装库无法升级 schema — 违反 §4.3"迁移版本管理与回滚"。MVP 前必须补 |
| DB-03 | Foreign Keys | **PASS** | 34 FK + 连接级 PRAGMA 开启 + 测试断言(database.rs:95-112)+ 脏数据特例回归 |
| DB-04 | Indexes | **PASS** | 121 索引随基线导出;列表端点排序字段均有对应索引(继承 Python 生产库) |
| DB-05 | Transactions | **PASS** | 33 处 begin;"业务写与审计事件共享事务"成文于模块注释并被测试守护(audit.rs:12) |
| DB-06 | Soft Delete | **PASS** | archived 语义(IMP-2:显式 null 422)+ 删除级联语义与基线逐条一致(positions 删岗保留候选人行;candidates 删人级联会话/消息/发送记录) |
| DB-07 | Backup | **PARTIAL** | 无 db 文件级备份/自动备份;数据级 export(AES-256-GCM 封缄)可作备份载体但非产品化备份(无调度、无校验、无保留策略) |
| DB-08 | Restore | **PARTIAL** | import_data(幂等 upsert + LWW + send_records 拒改写)可恢复数据;缺:恢复前安全快照、失败中断保护、完整性验证 |
| DB-09 | Integrity | **PARTIAL** | FK/事务/审计顺序有测试;无 PRAGMA integrity_check 例行检查或校验命令 |
| DB-10 | Export | **PASS** | 明文/A256GCM 双信封;字节级审计顺序回归(3 测试);导出隐藏 raw_text |

## 5.4 AI 能力(9 项)

| ID | 审计项目 | 状态 | 证据与说明 |
|---|---|---|---|
| AI-01 | Provider 配置 | **PARTIAL** | `AI_API_KEY` env 决定 ai_enabled(/health 暴露);**真实 provider 不存在** — ica-server 装配 `NotConfiguredDraftGenerator`(main.rs:83),按 ADR 划入 AI-EXT 延后 |
| AI-02 | Adapter | **PARTIAL** | `DraftGenerator` port 就绪(ports.rs:809)+ 503 ai_provider_error 兜底(generate_model_resolution 3 测试);缺真实 adapter 实现(属 AI-EXT) |
| AI-03 | Prompt 管理 | **PARTIAL** | prompt 随 draft 行持久化(generated_drafts.prompt);无模板管理机制 |
| AI-04 | Context 构建 | **PASS** | ai_context.rs(232 行):候选人 scoped 查询 + 最小化(redact_for_ai)+ redaction_applied 清单;`/api/candidates/{id}/context` 预览端点 + IPC candidate_context |
| AI-05 | Draft 生成 | **PASS**(工作流) | 守卫链 → AI port → 草稿组 → 人工 select;无 provider 时 503 而非静默失败 |
| AI-06 | Summary | **MISSING** | 无摘要端点/模块(若属需求,应归入 W5 ai 域范围核对) |
| AI-07 | 错误处理 | **PARTIAL** | 未配置→503 可读信封(有测试);真实 provider 的超时/重试随 AI-EXT 缺位 |
| AI-08 | Human Review | **PASS** | approve/reject/copy 三态 + "approve 不是发送、copy 不是发送"基线语义;唯一标记 sent 路径是 follow_ups_record_result |
| AI-09 | Privacy | **PASS** | redact_for_ai 应用于模型上下文(NAME_PLACEHOLDER,redaction_applied 可审计);redact 模块 303 行;导出时 raw_text 置空 |

## 5.5 GUI 与桌面集成(12 项)

> 前端代码在姊妹仓 `O:\ica\frontend`(E2 证据);ICA-RUST 无 Tauri 运行时(E1)。
> 页面功能"完整"指其在 Python 后端上成立;对 Rust 后端**全部未连接**。

| ID | 审计项目 | 状态 | 证据与说明 |
|---|---|---|---|
| GUI-01 | Dashboard | **PARTIAL** | 页面存在(DashboardPage,日历/统计)但接 Python 后端;Rust /api/dashboard 已实现未消费 |
| GUI-02 | Candidate | **PARTIAL** | 列表+详情页存在;Rust 7 端点未消费 |
| GUI-03 | Position | **PARTIAL** | 页面存在;Rust 5 端点未消费 |
| GUI-04 | Application | **MISSING** | 无实体无页面 → 关联 S-1 |
| GUI-05 | Conversation | **PARTIAL** | 时间线页存在;Rust 未消费 |
| GUI-06 | Follow-up | **PARTIAL** | 待办页存在;Rust 10 端点未消费 |
| GUI-07 | AI Assistant | **PARTIAL** | 草稿审核页存在(/drafts,非对话页 — 与基线定位一致);Rust 未消费;无独立 AI 页为设计使然 |
| GUI-08 | Loading | **PASS** | 全局 Loading 组件 + useAsync(role=status aria-live;AbortController 防竞态);10/10 页面采用 |
| GUI-09 | Error State | **PASS** | ErrorBox(role=alert)+ 统一错误信封单点解析;useMutation.clearError |
| GUI-10 | Empty State | **PASS** | EmptyState 区分"加载完零行"与"请求在途" |
| GUI-11 | IPC | **PARTIAL** | 47 命令与 HTTP 1:1、信封同形、smoke 测试通过;**但 0 个 `#[tauri::command]`,无 shell host 消费** — IPC 契约未被真实 GUI 行使 |
| GUI-12 | Lifecycle | **FAIL** | ICA-RUST 无宿主生命周期(无 spawn/退出清理);姊妹宿主管理的是 Python sidecar(stdin EOF 协议)。Rust server 作为 sidecar 的生命周期管理不存在 |

## 5.6 安全与可靠性(10 项)

| ID | 审计项目 | 状态 | 证据与说明 |
|---|---|---|---|
| SEC-01 | Local Access | **PASS** | 默认绑定 `127.0.0.1:8000`(main.rs:53-56);HOST/PORT 可配(留有暴露面,文档应注明);未知路径/方法 404/405 信封 |
| SEC-02 | Authentication | **MISSING** | Rust 后端无 auth 中间件(基线 401/refresh 流在前端对接 Python);users/refresh_tokens/tenants 表在 schema 中但未被 Rust 使用;属 W6 范围(已立项) |
| SEC-03 | Authorization | **MISSING** | 单租户本地语义,无权限边界实现;同 W6 |
| SEC-04 | Secrets | **PASS** | AI key 仅经 env,不入库不入审计(.gitignore 护 .env);导出密钥经请求头即时封缄、审计只记元数据;无硬编码密钥 |
| SEC-05 | Encryption | **PARTIAL** | 导出 AES-256-GCM(associated data 绑定版本);DB 落盘未加密(与 Python 基线一致);无 SQLCipher/静态加密 |
| SEC-06 | Input Validation | **PASS** | 见 API-07;OCR 路径守卫先于 I/O(两条拒绝路径有测试) |
| SEC-07 | File Access | **PASS** | OCR staging 白名单:越界路径拒绝(ica-api + ica-desktop 双侧测试);undecodable 可读错误 |
| SEC-08 | Audit | **PASS** | audit_events 与业务写同事务(33 处);"审计事件永不描述已回滚的变更"有注释与测试;确认操作字节级顺序回归 |
| SEC-09 | Dependency | **VERIFY** | cargo audit 本机未安装,未执行。待补跑(需 `cargo install cargo-audit`,本审计不擅自安装) |
| SEC-10 | Recovery | **PARTIAL** | 优雅停机(Ctrl-C/SIGTERM)+ 启动自检(缺表拒绝启动)+ WAL;缺崩溃恢复/迁移失败/备份中断等异常场景测试 |

---

## 行动清单(按优先级,均需用户裁决后动工)

| # | 行动 | 来源 | 性质 |
|---|---|---|---|
| 1 | **S-1 范围裁决**:MVP = 68 端点契约 还是 扩展 Project/Application?写 ADR | S-1;BIZ-01/05/06、GUI-04 | 决策 |
| 2 | **DB-02 迁移机制**:为 SQLite 建版本化迁移(含已装库升级 + 回滚策略) | DB-02 FAIL | 工程缺口,MVP 硬门禁 |
| 3 | **w4_imports_http OCR 夹具修复**(env::temp_dir + serde_json 序列化 + 模型路径参数化) | test-baseline §2 | 测试缺陷,1 项,Windows 全绿的前提 |
| 4 | **B1 POC 入库决策**:清理 fmt/clippy(7 警告+fmt diff 全在 POC)后提交,或明确不入库 | S-3 | 决策+小工程 |
| 5 | **F0-AUDIT 开工指令**:前端接口耦合审计 + 前端归属裁决 → 解锁 Phase 4 | S-2;GUI-01~07、11、12 | 决策 |
| 6 | **backup/restore 产品化**(含恢复前快照、中断保护、integrity check) | DB-07/08/09、SEC-10 | 工程缺口,Phase 5 主体 |
| 7 | **W5/W6 实施**(23 端点:ai/settings/backups/auth/shares) | API-01、SEC-02/03 | 已立项,按波次门禁执行 |
| 8 | **SEC-09 依赖漏洞扫描**(cargo audit 或同等) | SEC-09 | 例行门禁 |
| 9 | baseline.sql 头注释修正(20 tables/66 indexes → 实测 32/121) | DB-01 附注 | 文档微修 |

## 与发布门禁(§8.3)的当前距离

| 门禁 | 现状 |
|---|---|
| P0 功能全部验收 | 阻塞于 S-1 范围裁决 + GUI 集成(S-2) |
| 阻塞级缺陷关闭 | 当前无产品代码阻塞缺陷;1 测试夹具缺陷 + DB-02 结构缺口 |
| 核心 E2E 通过 | **无 E2E 存在**(12 步闭环 §8.2 无对应物) — Phase 4/5 需建 |
| 备份恢复通过 | PARTIAL,行动 #6 |
| Windows 安装包 | 姊妹仓 NSIS 打包的是 Python sidecar;Rust 侧打包不存在 |
| 依赖安全检查 | 未执行(行动 #8) |
| 无未批准契约变化 | **满足**(47 端点零漂移实测) |
