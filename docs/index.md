---
title: "ICA-RUST 文档索引与权威来源"
document_id: "DOC-ICA-INDEX-001"
document_type: "Index"
project: "ICA-RUST"
version: "1.0.0"
status: "Active"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "ENG-ICA-SPECS-R1.0"
  - "AUDIT-ICA-ENG-001"
supersedes: null
---

# ICA-RUST 文档索引与权威来源

依据 SPE-BAS-003 §4(十文档域)与 §6(Single Source of Truth)建立。
存量文档保留原路径(豁免 EX-001/EX-002),本索引提供十域映射、生命周期状态与权威来源声明。

## 1. 权威来源声明(BAS-003 §6)

| 信息 | 权威来源 |
|---|---|
| 项目定位/产品范围 | `ICA-RUST 当前设计总方案.md`(§1-§3)待 S-1 裁决后修订 |
| 端点契约(68 条) | `rust-migration-contract.md` §2 + `scripts/check_endpoint_map.py` 自动校验 |
| 实现现状(端点/IPC/表) | `docs/audits/mvp-r1.0/`(2026-11-21 实测)| 
| 当前架构 | `adr/ADR-0004-ica-runtime-architecture.md` + 分层由 `scripts/check.py` 强制 |
| 技术决策 | `docs/adr/`(ADR-0002~0006、ADR-BROWSER-001~010) |
| 数据库结构 | `crates/ica-infra/schema/ica-baseline.sql`("Do not hand-edit") |
| 实际实现 | 当前代码仓库(HEAD + 工作区) |
| 测试结果 | `docs/testing/test-report-r1.0.md`(正式报告)/ `docs/audits/mvp-r1.0/test-baseline.md`(审计证据) |
| 缺陷 | `docs/testing/defect-register.md` |
| 工程规范配置 | `docs/engineering/ENG-ICA-SPECS-R1.0.md` |

## 2. 十域映射(BAS-003 §4)

### DOC-01 项目管理
| 文档 | 状态 | 说明 |
|---|---|---|
| `ICA-RUST 当前设计总方案.md` | Active | 治理台账/波次/进度权威 |
| `ICA-RUST.txt`(仓库根) | Draft | R1.0 审计输入文档;是否入库待定 |

### DOC-02 产品需求
| 文档 | 状态 |
|---|---|
| `ICA R2.0 UI 重构方案….md` | Draft(提案态) |
| (PRD 缺位:MVP 范围随 S-1 裁决后建立) | TBD |

### DOC-03 系统架构
| 文档 | 状态 |
|---|---|
| `adr/ADR-0004-ica-runtime-architecture.md` | Approved |
| `ICA Browser Subsystem Architecture R1.0.md` | Active |
| `architecture/browser/b0-deliverables-r1.md` | Active(B0-09 已签署) |
| `architecture/browser/browser-route-inventory-r1.md` | Active |
| `architecture/frontend/frontend-architecture-r1.md` | Draft(F0 未开工) |
| `ICA-RUST × Hermes Embedded Agent 具体实施方案 R1.0.md` | Draft |
| `agent/g3-hermes-capability-survey-r1.md` | Active(调查结论:网关 API 假设不成立) |
| `ai/ai-ext-contract-draft-r1.md` | Draft |

### DOC-04 技术设计
| 文档 | 状态 |
|---|---|
| `rust-migration-contract.md` | Active(契约权威) |
| `rust-migration-scope-20261118.md` | Active(范围冻结) |
| `contract-baseline/w2|w3|w4-python-facts.md` | Active(基线事实记录) |
| `migration/baseline-diff-report-r1.md`、`migration/reconciliation/endpoint-matrix-r1.md`、`migration/snapshot-r1.md` | Active |
| `ocr/ocr-engine-runtime-notes.md` | Active |

### DOC-05 开发工程
| 文档 | 状态 |
|---|---|
| `engineering/ENG-ICA-SPECS-R1.0.md` | Draft(待批准) |
| `scripts/README.md` | Active |
| `README.md`(根) | Active(2026-11-23 ARCH-P6 状态纠偏:68/68) |
| `.github/workflows/quality-gate.yml` | Active(ARCH-P6 统一门禁 CI,无发布动作) |
| `.github/BRANCH_PROTECTION.md` | Proposed(GOV-01 规则待管理员在 GitHub 设置落实) |
| `.gitignore`、`rust-toolchain.toml`、`.cargo/` | Active |

### DOC-06 测试质量
| 文档 | 状态 |
|---|---|
| `testing/test-plan.md` | Draft(待批准) |
| `testing/test-case-catalog.md` | Draft(脚本生成,待登记确认) |
| `testing/traceability-matrix.csv` | Draft(同上) |
| `testing/test-report-r1.0.md` | Draft(待批准) |
| `testing/test-report-arch-p6.md` | Active(ARCH-P6 当前 HEAD 执行报告,绑定 SHA) |
| `testing/desktop-release-acceptance.md` | **EVIDENCED_PARTIAL**(Windows 实机 7/9 项已执行,待负责人签署) |
| `scripts/stage_rust_server.ps1` | Active(ica-server 随包 stage,Embedded Mode 必需) |
| `testing/evidence/windows-acceptance/` | Preflight only(本机预检证据,非验收通过) |
| `scripts/windows_acceptance.ps1` | Active(实机预检采集器,不代签) |
| `testing/defect-register.md` | Active(台账随缺陷开闭维护) |
| `templates/` | Active(项目模板) |

### DOC-07 部署运维
| 文档 | 状态 |
|---|---|
| (现状:打包/更新机制在姊妹仓 `O:\ica\desktop\`、`scripts/packaging/`) | 引用,不在本仓 |
| (ica-server 运行配置:见 `docs/audits/mvp-r1.0/api-inventory.md` §1) | 引用 |

### DOC-08 用户交付
| 文档 | 状态 |
|---|---|
| (缺位:安装/使用文档随 WP-07 交付建立) | TBD |

### DOC-09 工程审查
| 文档 | 状态 |
|---|---|
| `audits/tauri-architecture-audit-r0.1.md` | Archived(历史) |
| `audits/w4-independent/README.md` + FINDINGS/LWW/OCR + rerun/ | Archived(验收记录,不可变) |
| `audits/mvp-r1.0/`(6 份:README/baseline/module/api/ipc-gui/test-baseline/verify-matrix) | Draft(待项目负责人确认) |
| `audits/arch-p6-delivery-record.md` | Active(ARCH-P6 交付记录,绑定 HEAD 与执行证据) |
| `audits/sec-04-bc-delivery-record.md` | DeliveredWithLimitations(B/C 实现交付,待独立验证) |

### DOC-10 项目知识
| 文档 | 状态 |
|---|---|
| `adr/`(治理裁决 ADR-0002/0003/0005/0006) | Approved |
| `adr/browser/ADR-BROWSER-001~010` | Approved(B0 包冻结) |

## 3. 新文档规则(生效即执行)

1. 新正式文档必须带 YAML front matter(BAS-003 §7):title/document_id/document_type/
   project/version/status/created/updated/owner 必填。
2. 命名:一次性成果用 `<类型>-<项目>-<主题>-<版本>.md`;长期维护文档用 kebab-case
   (BAS-003 §9.1)。document_id 一经分配不变。
3. 生命周期状态必须标注;AI 生成的文档不自动获得 Approved/Active。
4. 跨文档引用用 document_id,不依赖文件路径。
