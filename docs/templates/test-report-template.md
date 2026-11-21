---
title: "测试报告模板(TMP-010 项目适配版)"
document_id: "TMP-010-ICA"
document_type: "Template"
project: "ICA-RUST"
version: "1.0.0"
status: "Active"
created: "2026-11-21"
updated: "2026-11-21"
owner: "ICA-RUST 项目负责人"
related_documents:
  - "TEST-ICA-PLAN-R1.0"
supersedes: null
---

# 测试报告(模板)

> 使用规则:每轮正式执行复制一份;状态数字必须来自实际执行,
> Blocked/Skipped/Not Applicable 不得计入 Passed(SPE-QA-001 §7.1/§11.3);
> 报告结论统一 PASS / CONDITIONAL PASS / FAIL。
> 参考实例:docs/testing/test-report-r1.0.md(TREP-ICA-1.0.0)。

---
title: "<测试报告标题>"
document_id: "TREP-ICA-<版本>"
document_type: "TestReport"
project: "ICA-RUST"
version: "1.0.0"
status: "Draft"
created: "<日期>"
updated: "<日期>"
owner: "<负责人>"
related_documents: ["TEST-ICA-PLAN-R1.0", "DEF-ICA-REGISTER-001"]
---

# 1. 测试概要
# 2. 软件版本(Git commit + 工作区状态)
# 3. 测试环境(OS/工具链/数据库/外部依赖;browser_poc 的 Chromium 依赖)
# 4. 执行时间
# 5. 测试用例统计(引用 test-case-catalog 对应 Case ID 范围)
# 6. 通过与失败情况(按 Passed/Failed/Blocked/Skipped/Not Applicable 分列)
# 7. 失败原因(逐项关联 DEF 编号)
# 8. 缺陷清单(引用 defect-register.md)
# 9. 日志及测试证据(可复现命令 + 输出保存位置)
# 10. 未覆盖范围(明确"本次不验证"清单)
# 11. 遗留风险
# 12. 测试结论(PASS / CONDITIONAL PASS(附风险说明+责任人+整改计划+审批) / FAIL)
