---
title: "GOV-01 主分支保护与人工发布规则"
document_id: "GOV-ICA-BRANCH-001"
document_type: "GovernanceConfig"
project: "ICA-RUST"
version: "1.0.0"
status: "Proposed — 需仓库管理员在 GitHub 设置中落实"
created: "2026-11-23"
updated: "2026-11-23"
owner: "ICA-RUST 项目负责人"
reviewers: []
related_documents:
  - "AUDIT-ICA-ENG-001"
  - "ARCH-P6"
supersedes: null
---

# GOV-01：main 分支保护与人工发布机制

> **诚实状态**：本 worktree 只提供**配置规则文档**。分支保护、Ruleset、Tag 保护是
> GitHub **仓库设置**，不在文件树里，必须管理员在 `BryanTianjx/rica` 的
> Settings 中手工落实。本卡**没有**仓库管理员权限去应用它们，也没有 push；
> 因此本文件是「应配置的规则 + 验收方法」，不是「已生效」的声明。

审计依据：`docs/audits/RICA 全量工程实现审计报告 R1.0 2026-11-22.md` §5.15，
现状为「main 未启用保护 / 无 CI workflow / 最新提交未签名」。QA-01 的 CI
部分已由 `.github/workflows/quality-gate.yml` 提供；本文件覆盖分支保护与发布门。

## 1. 必须配置的主分支保护规则（main）

| # | 设置项 | 要求值 | 为什么 |
|---|---|---|---|
| B1 | 合并方式 | 仅 Allow merge commits 或 Squash；**禁止直接 push 到 main** | 所有变更经 PR |
| B2 | Required pull request reviews | 至少 1 名非作者审批；勾选 Dismiss stale approvals when new commits are pushed | 变更有审查记录 |
| B3 | Required status checks | `Quality Gate / gate`（来自 `.github/workflows/quality-gate.yml` 的 job id `gate`） | 未过门禁不能合并 |
| B4 | Require branches to be up to date before merging | 开启 | 避免用过期基线合并 |
| B5 | Include administrators | 开启 | 管理员不能绕过门禁 |
| B6 | Restrict who can push to matching branches | 仅仓库管理员 | 减少直推面 |
| B7 | Require signed commits | **评估项**，本卡不擅自开启 | 审计原文只要求「评估」，签名不等于代码安全 |
| B8 | 保护 Tag | `v*` 与 `w*-done` 纳入 Ruleset，仅管理员可创建/删除 | 防止验收锚点被移动 |
| B9 | 自动流程 | 仓库内**不存在**自动 commit / push / merge / release / 依赖自动合并 | 审计明确禁止 |

### 1.1 管理员验收方法（逐条可复核）

1. `Settings → Branches → Branch protection rules` 截图/导出规则，对照 B1–B6 逐条核。
2. 开一个测试 PR，故意让 CI 红（例如注入一个 fmt diff），确认 GitHub 上
   `Quality Gate / gate` 为红且 **Merge 按钮被禁用** → 证明 B3 生效。
3. 让非管理员尝试直推 main，确认被拒 → 证明 B1/B6 生效。
4. `Settings → Tags` / Rulesets 确认 `v*`、`w*-done` 受保护 → 证明 B8 生效。
5. 全仓搜索 workflow 中的 `gh release` / `git push` / `contents: write`，
   必须为空 → 证明 B9 生效。

> 上述 1–5 项**尚未执行**（需要 GitHub 管理员权限与一次真实 PR），状态记为
> `NOT_EXECUTED`，不写成 PASS。

## 2. 人工发布机制（保留，且不自动化）

发布是一条**人工、可审计、可回滚**的流程；CI 只产出门禁结果，不触发任何发布动作。

1. **触发**：人决定发布，创建 release 分支 / PR；CI 必须 `Quality Gate / gate` 全绿。
2. **版本**：`desktop/tauri-host/tauri.conf.json` 的 `version` 与
   `bundle-resources/version-manifest.json`、`SHA256SUMS` 三者一致；版本号人工确认。
3. **门禁证据**：把该 HEAD 的 `gate-run.json`、`docs/testing/test-report-arch-p6.md`
   与 `docs/testing/desktop-release-acceptance.md` 作为发布附件归档。
4. **Windows 实机验收**：`docs/testing/desktop-release-acceptance.md` 的 9 项必须
   由人在 Windows 实机执行并签字；本机 WSL 环境该项为 BLOCKED，**不得代签**。
5. **独立安全验证**：人工执行，结果独立于实现方；本仓门禁中
   `security_scan` 即使跑通也只是依赖 advisory 扫描，**不等于安全验证通过**。
6. **发布动作**：人工打 `v*` annotated tag（受 B8 保护）→ 人工构建安装包 →
   人工上传 → 人工写 Release Notes。此链路中**没有** workflow 自动步骤。
7. **回滚**：保留上一版本标签与安装包；回滚由人执行并记录。

### 2.1 明确不授权的动作

- CI 自动 commit / push / merge / release / tag。
- Agent 自签 `Verified`、自签安全验证、自签 Windows 实机验收。
- 在缺少 Windows 实机证据时把 AC3 改判为通过。
