---
title: "工程审计报告模板(TMP-011 项目适配版)"
document_id: "TMP-011-ICA"
document_type: "Template"
project: "ICA-RUST"
version: "1.0.0"
status: "Active"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
related_documents:
  - "ENG-ICA-SPECS-R1.0"
supersedes: null
---

# 工程审计报告(模板)

> 使用规则:复制本模板到 docs/audits/<audit-id>/ 下;front matter 必填;
> 结论分级按 SPE-GOV-001 §8.3(Critical/Major/Minor/Observation);
> 证据分级:审计类文档统一用 E1(当前证据)/E2(历史对照)/E3(规划,不得认定已实现)。
> 参考实例:docs/audits/mvp-r1.0/(AUDIT-ICA-ENG-001)。

---
title: "<审计标题>"
document_id: "AUDIT-ICA-<序号>"
document_type: "AuditReport"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "<日期>"
updated: "<日期>"
owner: "<负责人>"
related_documents: []
---

# 1. 审计目标
# 2. 审计范围(含明确排除项)
# 3. 基线信息(Git HEAD/提交数/工作区状态/对照基线)
# 4. 审计方法与使用的规范及版本(SPE-GOV-001 §8.4 必填)
# 5. 现状分析(每个结论附文件:行号证据,标注 E1/E2)
# 6. 架构审查
# 7. 代码审查
# 8. 数据及接口审查
# 9. 安全审查
# 10. 测试与质量审查(引用正式测试报告,不重复数字)
# 11. 问题清单(编号 + 等级 + 证据;同步登记 defect-register.md)
# 12. 风险评级
# 13. 整改建议(行动清单,标注决策项/工程项)
# 14. 实施路线
# 15. 验收标准
# 16. 审计结论(含待裁决项,明确标注 AI 生成内容状态)
# 17. 修订记录
