# **ICA R2.0 UI 重构方案：统一招聘工作台 + Hermes Agent Runtime + Browser Workspace**

结合这五张现有界面，以及 ICA 最近确定的架构方向，我建议不要仅仅对现有页面进行美化，而是进行一次**信息架构（IA）、交互模式和工作流层面的整体重构**。

核心变化是：

- 从传统招聘信息管理系统，转向 AI-native 招聘工作台。
- 从分散的候选人、岗位、沟通、跟进页面，转向围绕招聘任务组织工作。
- 将 Hermes Agent Runtime 作为可调度的智能执行层。
- 将 Browser Subsystem 作为独立的受控浏览器执行环境。
- 将人工审批、权限控制和执行审计提升为一级交互能力。

最重要的架构原则是：

**ICA 是业务系统，Hermes 是 Agent Runtime，Browser 是执行环境。三者协同，但不能互相取代。**

这也意味着 UI 不应该把 Hermes 的终端、Agent 会话或浏览器控制台直接搬进 ICA，而应该通过统一的工作台向用户呈现业务任务、AI 分析、浏览器执行和审批过程。

## **一、现有 UI 评估**

### **1. 总体评价**

当前界面采用传统 CRUD 管理后台模式，整体风格比较统一，信息结构也比较清晰，作为 ICA 早期版本是合适的。

但当加入 Hermes Agent Runtime 和 Browser Subsystem 后，现有设计会出现明显的扩展瓶颈。

**现有 UI 的主要问题**

需要结构调整

| 页面      | 现有问题                 | 改进方向                 |
| ------- | -------------------- | -------------------- |
| 工作台     | 日历占据大量空间，缺少工作流概览     | 改为招聘任务驾驶舱            |
| 候选人     | 纯表格展示，操作分散           | 增加候选人工作区             |
| 候选人详情   | 档案、沟通、跟进纵向排列，空间利用率低  | 多面板工作空间              |
| 岗位管理    | 基础 CRUD，缺少岗位与候选人关联视图 | 岗位 Pipeline          |
| 沟通记录    | 左侧列表、右侧空白，信息密度低      | 沟通时间线与 AI 辅助工作区      |
| AI 草稿审核 | 独立于业务流程              | 嵌入候选人沟通与审批流程         |
| 浏览器自动化  | 只有导航入口，没有统一工作区       | 独立 Browser Workspace |

### **2. 五张截图逐页分析**

**01. 工作台**

截图中的核心问题：

- 统计卡片仅展示数量，没有告诉招聘者下一步应该做什么。
- 月历占据了主要视觉区域，但实际招聘任务更需要按优先级展示。
- 待跟进候选人只是简单列表。
- 没有 Agent 工作状态、浏览器会话、待审批操作的入口。

建议将月历降级为辅助组件，工作台以待办任务、候选人动态和 Agent 工作状态为主。

**02. 候选人列表**

当前表格的优点是简单直接，但候选人管理缺少招聘上下文。

建议增加：

- 当前招聘岗位。
- 候选人阶段。
- 最近一次沟通时间。
- 下一步跟进时间。
- AI 分析状态。
- 浏览器来源及关联状态。
- 待处理事项。

不建议将所有字段都塞进表格。列表负责快速筛选，详情负责完整信息和操作。

**03. 候选人详情**

这是最值得重点改造的页面。

目前存在三个相对独立的区域：

1. 候选人档案。
2. 跟进判断。
3. 手动导入沟通记录。

它们实际上属于同一条招聘业务链路。

建议整合成：

- 左侧：候选人档案及招聘上下文。
- 中间：沟通时间线和业务活动。
- 右侧：AI 助手、跟进建议和审批操作。

这样不需要频繁切换页面。

**04. 岗位管理**

当前岗位列表缺少招聘进度信息。

建议增加岗位工作台，包括：

- 岗位基本信息。
- 招聘需求与技能要求。
- 关联候选人。
- 各招聘阶段人数。
- 当前招聘任务。
- 岗位状态与归档。

岗位是招聘业务的组织维度，不应该仅仅是一个普通数据表。

**05. 沟通记录**

当前页面的问题最明显：

- 左侧候选人列表占用大量空间。
- 右侧没有选中会话时几乎没有有效信息。
- 沟通记录缺少与 AI 分析、跟进任务的关联。

建议改为双栏沟通工作区，支持沟通记录、AI 摘要、待回复事项和历史招聘上下文。

## **二、重新设计整体信息架构**

我建议将 ICA 的一级导航从当前七个入口，调整为六个业务工作区。

## **ICA Personal · 新版导航结构**

**01 工作台**

个人招聘驾驶舱

**02 招聘管理**

候选人 · 岗位 · 沟通记录 · 跟进任务

**03 AI 工作台**

Hermes Runtime

Agent 会话 · AI 分析 · 话术草稿 · 任务执行 · 运行记录

**04 浏览器工作区**

Browser Subsystem

浏览器 · 页面预览 · 会话管理 · 自动化任务 · 执行状态

**05 审批中心**

消息审批 · 浏览器操作审批 · 权限申请 · 审计记录

**06 系统设置**

Agent 配置 · 浏览器配置 · 权限策略 · 数据管理 · 运行诊断

这里有一个重要调整：

原来的「AI 草稿审核」不再作为独立的业务导航，而是将草稿审批统一纳入审批中心，同时在候选人详情和 AI 工作台保留上下文操作入口。

「跟进任务」也不再孤立存在，而是成为工作台和候选人详情中的任务视图。

### **三、整体 UI 布局设计**

建议采用统一的三层结构。

**顶部全局栏**

高度约 60px

ICA Personal　/　当前工作区

ICA Core

Hermes Online

**左侧导航**

约 220–240px

**工作台**

招聘管理

AI 工作台

浏览器工作区

审批中心

系统设置

**中央业务区**

**页面标题与主要操作**

搜索、筛选、创建、切换视图

业务数据

列表、时间线、详情

任务上下文

状态、建议、审批

底部运行状态

Agent / Browser / 数据库 / 后端连接

**布局原则：**

- 左侧导航保持稳定，不随业务页面变化。
- 中间区域始终展示 ICA 业务内容。
- 右侧智能面板根据当前页面上下文切换。
- 底部运行状态显示系统健康信息，不占据主要工作空间。
- Agent 面板支持收起、展开，不强制占据所有页面。

## **四、核心页面重新设计**

### **1. 新工作台：招聘驾驶舱**

工作台应该回答四个问题：

1. 当前有哪些招聘工作？
2. 哪些候选人需要处理？
3. Agent 正在做什么？
4. 哪些操作需要我批准？

[Interview Management Platform - Case Study | Chromosis](https://images.openai.com/static-rsc-4/VeopxR7jAeapHZoo2OKzv4iLjezh0nEyt3k4FWsdplTmFxs5MT18IP_LM8DqChAv3iE8CzOS8JUvtkz5P_lciCJT69kLjboOwOvHeY_8UgePnHfUXh7EzuFA4LfYOeHjcYfA_y9RtbHFJq08ds68EBLVeLIj9Ee5wSE_tU3W3zY?purpose=inline)

[chromosis.com](https://chromosis.com/case-study/custom-web-application/interview-automation?utm_source=chatgpt.com)

推荐布局：

**工作台**

示意布局

# **12**

候选人

# **4**

待跟进

# **2**

待审批

# **1**

Agent 任务

**今日待办**

李四 · 首次沟通

王五 · 跟进提醒

赵六 · 面试反馈

**待审批**

2 项待确认

候选人沟通草稿

浏览器页面操作

**Agent 运行情况**

Candidate Analysis

运行中

正在分析候选人经历，等待 ICA Core 返回结果

Browser Session

空闲

**ICA AI 助手**

可以询问候选人情况、生成沟通建议或创建招聘任务……

原来的月历保留，但移动到「任务日历」视图，默认不再占据工作台主要区域。

### **2. 候选人管理：从表格升级为候选人工作区**

建议提供三种视图：

- 列表视图：适合查找、筛选、批量浏览。
- Pipeline 视图：适合按招聘阶段查看候选人。
- 归档视图：适合历史候选人管理。

[Devlumiq ATS - Full-Stack Recruitment Platform | Codester](https://images.openai.com/static-rsc-4/FeGkirPy-JGz8pDGpELd8dF2XkK6feTJoGWwsjOHusYpwb-GawCs82uUkAGns8DYGispkeiN2Oq0I2xgC47Ua8EQ6xZoUpY9bf-A7UHnQnVainuJIA_bZNpb9yiHotxNg1WwH4mQ3Ih9qlSDconpvjK3_JqBuJKZ2MgnJAQJhh0?purpose=inline)

[codester.com](https://www.codester.com/items/65323/devlumiq-ats-full-stack-recruitment-platform?utm_source=chatgpt.com)

建议列表字段：

| 字段    | 说明                            |
| ----- | ----------------------------- |
| 候选人   | 姓名及基本信息                       |
| 关联岗位  | 当前招聘岗位                        |
| 招聘阶段  | New / Contacted / Interview 等 |
| 最近沟通  | 最近一次有效沟通时间                    |
| 下一步   | 跟进动作                          |
| AI 状态 | 未分析、分析中、已完成                   |
| 操作    | 查看、编辑、归档                      |

状态建议在 UI 中统一中文显示，内部枚举可以保留英文，例如：

- New → 新建
- Contacted → 已联系
- Interview → 面试中
- Rejected → 已淘汰
- Blocked → 已阻塞
- Archived → 已归档

不要让 Agent 自动修改候选人的业务状态。状态仍然由用户确认后变更。

### **3. 候选人详情：采用三栏工作空间**

这是整个 ICA 最重要的业务页面。

[Building a Fake Job Candidate Detection System in 4 Hours: How Sardine Can Fight AI-Powered Job Fraud](https://images.openai.com/static-rsc-4/GvBGXoOxtOpG8PNJOTuIm3Pz0pUkydyFVK2BNZOO6REAJJkL4SYplqRdNcMwo0UZGmQ08AqRucf94ruyxxIfNK8FScTl7XuhjOMftGXqTwm_1umLC_fL5I5HB7nTwGr1TKFrB29l8GECY-ayYkbatBDYTQ8eOrvDQiIclBKcGbU?purpose=inline)

[sardine.ai](https://www.sardine.ai/engineering-blogs/fake-job-candidate-detection-system?utm_source=chatgpt.com)

建议重新组织为以下结构：

**李四 · 候选人详情**

已联系

React 工程师 · BOSS 来源 · 关联岗位 Recruiting Partner

**候选人档案**

## **李四**

5 年工作经验

React / TypeScript

期望薪资：25k

期望地点：杭州

**招聘信息**

关联岗位

Recruiting Partner

**沟通时间线**

11 月 19 日 · Recruiter

你好，看到您投递了后端工程师岗位，方便聊聊吗？

11 月 19 日 · Candidate

您好，我目前在职，主要做 Python 后端。

录入沟通记录

粘贴聊天记录，解析后预览并确认写入

**AI 助手**

Hermes 可用

**分析摘要**

候选人技能与岗位存在一定匹配度，建议进一步确认后端技术栈。

**下一步建议**

确认工作经历和求职意向

**沟通草稿**

由 AI 生成，等待人工审核

待确认

真实实现中，三栏应支持独立滚动和宽度调整。

候选人详情增加顶部 Tab：

- 概览
- 沟通记录
- 跟进任务
- AI 分析
- 活动审计

这样既保留三栏高效工作模式，也能容纳更多详细信息。

### **4. 岗位管理：引入招聘 Pipeline**

岗位页面建议从简单数据表改成岗位工作区。

[Recruitment CRM and ATS with built in AI | itris CRM Software](https://images.openai.com/static-rsc-4/pOwbjPhESnTHlYh3gEnusQyMc5k-3RsfTjVXRMKmj3yHqsQOmhlNnnRIFdcqD1WA4GvZ4hTG8aWfZd2aiH6DG1DPTEJYIQWl7cfsyAzBHSviv0JZdcifl_mnUhWnfj1bHs0gqDTVEnikiCgMNozIt332-vhlMqDGEyJ1AVnBw9E?purpose=inline)

[itris.co.uk](https://www.itris.co.uk/?utm_source=chatgpt.com)

岗位详情应包含：

| 区域    | 内容            |
| ----- | ------------- |
| 岗位信息  | 名称、地点、薪资、职位描述 |
| 招聘要求  | 技能、经验、学历等     |
| 候选人   | 关联候选人及招聘阶段    |
| 任务    | 当前跟进及待处理工作    |
| AI 分析 | 岗位要求结构化及匹配建议  |

特别注意：AI 匹配结果只能作为辅助判断，不能直接替代招聘者的决策，也不能未经确认修改候选人档案。

### **5. 沟通工作区：统一人工沟通和 AI 辅助**

建议采用左右双栏：

[Intercom | The only helpdesk designed for the AI Agent era](https://images.openai.com/static-rsc-4/fqRZKSkfAC5d8jPY0oIpApBFViIDPfZobTlJqbR-kvLI7LYIrCGzsMDXodBTyvCJmpOAHKxZUwXRxCwXNS92TMooeE4V8mwfxoBFBmjJ7b6GpvYweS9TvLpV9PmLJTlCYil4lp77mRwi5YtEkEpLOesxFASOTHsAmpbtynruYaE?purpose=inline)

[intercom.com](https://www.intercom.com/?utm_source=chatgpt.com)

- 左栏：候选人会话列表、岗位筛选、未读状态。
- 右栏：完整沟通时间线、沟通记录录入、AI 摘要、话术建议。
- 右侧可选面板：候选人档案和跟进任务。

核心操作：

1. 导入或粘贴聊天文本。
2. AI 解析沟通内容。
3. 显示解析预览。
4. 用户确认。
5. ICA Core 写入沟通记录。
6. Agent 根据授权上下文提供后续建议。

首期仍保留人工复制、粘贴和发送，不将 BOSS 等平台的自动发送作为默认能力。

## **五、Hermes Agent Runtime：新增 AI 工作台**

这一部分是 ICA 这次升级的核心。

我不建议简单地把 Hermes 对话框放在界面右侧，而是建设一个独立的 Agent 工作台。

它需要管理 Agent 的任务、运行状态、执行过程和结果。

### **1. AI 工作台布局**

[AI & Automation | Kinso Labs](https://images.openai.com/static-rsc-4/ypoVwUQkck-wIUr1J1h_0u0TEhjaZzE2ryxGzCshgxbyHhePZ4rjATfseEFgwTIINyjfn7phSBdcEREaquaftLg-ZWGZRQih_qeC5eXfVQ3CTL_r9C-uXoigQAMqf8U85gE9f0mzDgwRT_G4_sd9HOIM5y_1NW3_cN9ZSi5qa8Q?purpose=inline)

[kinsolabs.com](https://kinsolabs.com/services/ai?utm_source=chatgpt.com)

建议采用三栏布局：

| 区域 | 功能                 |
| -- | ------------------ |
| 左栏 | Agent 会话、任务历史、任务分类 |
| 中栏 | AI 对话、任务执行过程、结果展示  |
| 右栏 | 当前候选人、岗位、上下文、审批信息  |

### **2. Agent 任务类型**

建议先定义有限的招聘业务 Agent 能力：

| Agent 能力                | 职责      | 权限      |
| ----------------------- | ------- | ------- |
| Candidate Analysis      | 候选人资料分析 | 只读      |
| Job Matching            | 岗位匹配建议  | 只读      |
| Communication Assistant | 沟通摘要与话术 | 生成草稿    |
| Follow-up Assistant     | 跟进建议    | 创建待确认建议 |
| Browser Assistant       | 浏览器辅助操作 | 受控执行    |
| Recruitment Assistant   | 招聘工作流协调 | 受限调度    |

不建议一开始就为每个功能创建一个独立 Hermes Agent。首期可以通过一个招聘主 Agent 配合不同 Skill 和受控工具实现，后续根据实际负载再拆分。

### **3. Agent 执行状态**

统一展示以下状态：

## **Agent 任务生命周期**

1. **Created · 已创建**

   用户发起任务，ICA 记录任务上下文
2. **Queued · 排队中**

   等待 Runtime 调度
3. **Running · 执行中**

   Hermes 执行分析或调用授权工具
4. **Waiting Approval · 等待审批**

   涉及需要人工确认的操作
5. **Completed · 已完成**

   返回结果，由 ICA 持久化业务结果

其他终态：Failed、Cancelled、Expired。暂停和恢复作为运行控制状态处理。

UI 应显示：

- 当前任务。
- Agent 名称。
- 运行时间。
- 当前步骤。
- 工具调用记录。
- 等待审批的原因。
- 执行结果。
- 错误与重试信息。

这里有一个非常重要的设计区别：

**Hermes 的运行状态与 ICA 的业务状态必须分开。**

例如：

- Hermes：候选人分析任务 Completed。
- ICA：候选人状态仍然是 New。

AI 完成分析，并不意味着候选人的招聘流程自动推进。

## **六、Browser Subsystem：独立浏览器工作区**

浏览器子系统不能只是一个嵌入式网页窗口。

它需要承担浏览器会话、页面观察、受控交互、任务执行和操作审批等职责。

结合已经确认的方向：

- Managed Browser 为默认模式。
- External Browser 为可选连接方式。
- 默认使用内置 Chromium + Playwright。
- 外部 Chrome / Edge 通过可选连接能力接入。
- DOM 优先，视觉和坐标操作作为兜底。
- 每次重要操作后进行结果验证。
- 发送消息必须由用户逐条确认。
- Hermes 不允许绕过 ICA 审批直接发送消息。

### **1. 浏览器工作区布局**

这是我建议重点建设的新页面。

[GPT-5.4 Computer Use Tutorial: Build a Live News Dashboard | DataCamp](https://images.openai.com/static-rsc-4/yxUSspPLqFOkkDEOTtrGim4jcu7l8QYI2yhedBJqWiTfQfLdHMu4eeXkyn75MltCZ0gT2MNlXPdI1qyGmd-ysJchfWALid30mUDsi7Uy6TPbygBAjCe9xUNP8oxZ_V95QOhfEin1DCU72aIAUFB2rezfvBO2VRQ5nIRHK7qtfNg?purpose=inline)

[datacamp.com](https://www.datacamp.com/tutorial/gpt-5-4-computer-use-tutorial?utm_source=chatgpt.com)

推荐结构：

**Browser Workspace**

Managed Browser

[https://www.zhipin.com/](https://www.zhipin.com/?utm_source=chatgpt.com)...

Session 01

**Browser View**

真实浏览器页面渲染区域

用户可查看和直接操作

**Agent Panel**

Runtime Online

当前任务：查看候选人资料

**页面分析**

发现候选人基本信息及技能描述

**操作建议**

提取候选人信息，等待用户确认导入

**Candidate Queue**

候选人待处理队列

李四 · 前端工程师

待确认导入

王五 · 后端工程师

待处理

**Execution Timeline**

10:20 页面打开 → 10:21 DOM 分析 → 10:22 信息提取 → 等待用户确认

### **2. 浏览器工作区模块**

建议使用此前已经规划的七个组成部分：

| 组件               | 作用              |
| ---------------- | --------------- |
| Browser Toolbar  | 地址、导航、刷新、会话控制   |
| Browser View     | 实际网页展示          |
| Candidate Queue  | 候选人待处理队列        |
| Candidate Detail | 候选人信息预览         |
| Agent Analysis   | Agent 页面分析和操作建议 |
| Message Composer | 沟通内容草稿          |
| Approval Panel   | 操作审批与确认         |

其中 Candidate Queue 和 Candidate Detail 需要通过明确的导入预览流程与 ICA 招聘域关联，不应直接将浏览器页面数据写入业务数据库。

### **3. 浏览器独立管理页面**

Browser Workspace 是日常操作界面，浏览器管理则用于配置和运维。

建议包含：

- Browser Overview：运行概况。
- Profiles：浏览器配置档案。
- Sessions：浏览器会话。
- Permissions：权限配置。
- Automation Tasks：自动化任务。
- Approval Center：浏览器操作审批。

Managed Browser 和 External Browser 应在会话创建时选择，而不是将两种模式混合在一个难以区分的操作界面中。

## **七、统一审批中心：整个系统的安全控制入口**

这是原有界面中缺失，但新架构必须具备的一级模块。

不能让用户分别在 AI 工作台、浏览器工作区和候选人详情中寻找所有待确认操作。

建议建设统一审批中心。

[Best Workflow Approval Software for Teams in 2026 | UI Bakery Blog](https://images.openai.com/static-rsc-4/BryfjZOSfR1R4EbT0C4FAyplqnqXQhEEl-vPCJ_FiU1iKDBP7GvLR-6qPJpJkItVzQaCBI5UliMve2utIzLlat_25iROGf8LtQfPAJxa9L3Jq0Bs2v5Ii9skt9nQjB1J6phJdKmxZOiD46fvROzoqsoadfOKsvdRA82xQTAnhR0?purpose=inline)

[uibakery.io](https://uibakery.io/blog/best-workflow-approval-software?utm_source=chatgpt.com)

### **审批分类**

| 类型    | 示例           | 审批方式  |
| ----- | ------------ | ----- |
| AI 草稿 | 招聘沟通话术       | 查看后确认 |
| 消息发送  | BOSS 候选人消息   | 逐条确认  |
| 候选人导入 | 浏览器提取的候选人资料  | 预览后确认 |
| 业务变更  | 修改候选人状态      | 人工确认  |
| 浏览器操作 | 高影响页面操作      | 执行前确认 |
| 权限申请  | Agent 请求额外能力 | 授权或拒绝 |

### **审批交互**

每条审批需要展示：

1. 发起者：用户或 Agent。
2. 业务对象：候选人、岗位或浏览器会话。
3. 计划执行的操作。
4. 操作影响范围。
5. 原始内容或参数。
6. 执行前后的预期变化。
7. 确认、拒绝、取消。

尤其是消息发送：

**待发送消息审批**

等待确认

候选人：李四 · 平台：BOSS

您好，看到您的项目经历比较符合我们目前的岗位要求，希望进一步了解您的技术经验，方便沟通一下吗？

Agent 生成

未经发送

修改草稿

**确认发送**

确认后由 ICA 授权浏览器执行，并返回执行结果；不能由 Hermes 绕过审批直接发送。

审批中心还需要支持审批超时、取消、执行失败、重复提交防护和审计追踪。

## **八、ICA、Hermes、Browser 的系统架构与 UI 对应关系**

这里需要特别强调架构边界。

根据已经确认的 ICA 集成方向：

- ICA-RUST 是唯一正式业务后端。
- Hermes 是独立进程，通过 Tauri Host / Runtime Adapter 集成。
- Hermes 不直接访问 ICA 数据库。
- ICA-RUST 负责权限、审批、审计。
- Browser Subsystem 作为受控执行环境。

建议形成如下结构：

**ICA React UI**

招聘工作台 · AI 工作台 · Browser Workspace · 审批中心

Tauri IPC / Backend Client

## **Tauri Host / Runtime Adapter**

**ICA Core Client**

业务 API 与事件

**Hermes Adapter**

Runtime 连接、任务协调

**Browser Adapter**

浏览器会话、操作接口

**Event Adapter**

状态事件、执行结果

## **ICA-RUST**

唯一正式业务后端

招聘领域服务

业务数据与 SQLite

权限与审批

审计与业务事件

## **Hermes Runtime**

独立 Agent 执行进程

Agent 调度

Skills / Tools

任务执行

AI 分析与建议

## **Browser Subsystem**

独立浏览器执行环境 · Managed Browser 默认 · External Browser 可选

Chromium

Playwright

DOM Inspection

Visual Fallback

Session Manager

### **三个关键设计约束**

**1. UI 不直接依赖 Hermes 内部接口**

React 只调用统一的前端 API。Runtime Adapter 负责 Hermes 进程通信和协议适配，避免未来替换 Agent Runtime 时影响整个 UI。

**2. Hermes 不直接访问业务数据库**

Agent 获取候选人、岗位和沟通记录时，必须经过 ICA-RUST 授权的业务 API 或受控工具。

**3. 浏览器操作必须受 ICA 权限控制**

Agent 可以提出操作请求，但高影响操作必须先经过 ICA-RUST 的审批流程。浏览器执行器只能执行经过授权的请求。

还应明确一个实现细节：审批凭证必须绑定具体操作、业务对象和参数，不能只依靠一个通用的 `approved=true` 标志。发送前还应验证审批没有过期、没有被撤销，且内容未发生变化。

## **九、设计系统与视觉规范**

现有 UI 不需要彻底推翻视觉风格，可以保留浅色、蓝色主色调和卡片式设计，但需要提高信息层次和交互密度。

建议设计规范：

| 项目   | 建议           |
| ---- | ------------ |
| 主色   | 蓝色 `#2563EB` |
| 背景   | `#F5F7FA`    |
| 卡片   | `#FFFFFF`    |
| 正文   | `#1F2937`    |
| 辅助文字 | `#64748B`    |
| 成功   | `#16A34A`    |
| 警告   | `#D97706`    |
| 危险   | `#DC2626`    |
| 圆角   | 8–12px       |
| 左侧导航 | 220–240px    |
| 顶部栏  | 56–64px      |
| 页面间距 | 16–24px      |

### **组件统一**

建议建设 ICA Design System：

- AppShell
- Sidebar
- TopBar
- PageHeader
- DataTable
- CandidateCard
- CandidateProfile
- ActivityTimeline
- TaskCard
- AgentPanel
- AgentRunStatus
- BrowserToolbar
- BrowserView
- ApprovalCard
- PermissionDialog
- AuditTimeline

尤其建议将 AgentPanel、ApprovalCard、ActivityTimeline 做成全局可复用组件，而不是在每个页面重新实现。

### **状态展示统一**

业务状态、Agent 状态、浏览器状态必须采用不同的状态体系。

| 对象       | 状态示例                |
| -------- | ------------------- |
| 候选人      | 新建、已联系、面试中、已淘汰      |
| Agent    | 排队、运行、等待审批、完成、失败    |
| Browser  | 已连接、空闲、执行中、暂停、异常    |
| Approval | 待审批、已批准、已拒绝、已撤销、已过期 |

避免全部使用 New、Running、Completed 这样的通用英文标签，造成用户无法分辨状态属于哪个系统。

## **十、分阶段实施计划**

不建议一次性同时重做所有页面、Agent Runtime 和 Browser Subsystem。应该先统一业务模型与 UI 契约，再逐步接入执行能力。

## **ICA UI R2.0 实施路线图**

1. **阶段 0：现有架构与 UI 基线审计**

   前置阶段
   - 审计最新 main 分支。
   - 核验 ICA-RUST API 与数据库模型。
   - 核验现有 React 页面及组件。
   - 梳理 Browser V2.0 边界记录。
   - 核对现有 12 张 Browser 表的复用情况。
   - 解决 W7 与 B4 的架构冲突。
   - 明确 39 条 Browser 路由的归属。

   交付：UI 差异审计、模块边界、API 契约、迁移清单。
2. **阶段 1：统一 UI Shell 与 Design System**
   - 重构 Sidebar、TopBar、PageLayout。
   - 统一色彩、字体、间距和组件。
   - 实现全局状态栏。
   - 建立右侧上下文面板。
   - 实现响应式布局。

   交付：ICA 新版 UI 框架。
3. **阶段 2：招聘业务工作区**
   - 工作台重构。
   - 候选人列表及详情重构。
   - 岗位管理重构。
   - 沟通记录工作区。
   - 跟进任务统一管理。

   交付：完整的招聘业务工作台。
4. **阶段 3：Hermes Agent 工作台**
   - Runtime Adapter。
   - Agent 会话管理。
   - 任务生命周期管理。
   - 执行事件展示。
   - AI 结果与业务对象关联。
   - Agent 运行诊断。

   交付：可观测、可控制的 Agent 工作台。
5. **阶段 4：Browser Workspace**
   - Managed Browser。
   - External Browser 连接。
   - 浏览器会话管理。
   - 页面预览和操作。
   - Candidate Queue。
   - Agent Browser 协同。

   交付：统一浏览器工作区。
6. **阶段 5：审批与安全体系**
   - 统一审批中心。
   - 消息逐条审批。
   - 浏览器操作审批。
   - 权限策略。
   - 执行审计。
   - 失败恢复与操作追踪。

   交付：完整的人机协同闭环。
7. **阶段 6：集成验收**
   - 招聘业务回归测试。
   - Hermes Runtime 集成测试。
   - Browser 自动化测试。
   - 权限绕过测试。
   - 审批与审计一致性测试。
   - Windows / macOS 桌面验收。

   交付：ICA R2.0 可交付版本。

### **建议的模块优先级**

| 模块                | 优先级 | 原因           |
| ----------------- | --- | ------------ |
| UI Shell          | P0  | 全部页面基础       |
| 招聘工作区             | P0  | ICA 核心业务     |
| Agent 工作台         | P1  | 核心差异化能力      |
| 审批中心              | P1  | Agent 安全执行前提 |
| Browser Workspace | P1  | 浏览器协同能力      |
| Agent 高级编排        | P2  | 后续扩展         |
| 高级自动化流程           | P2  | 业务闭环成熟后再建设   |

这里有一个实施顺序上的细节：Browser 的架构审计属于阶段 0，但完整 Browser Workspace 的 UI 和执行能力属于后续阶段。不能因为界面暂时不开发，就推迟解决数据库和路由边界问题。

## **十一、最终建议：ICA 应形成三个工作空间**

我认为新版 ICA 最有价值的设计不是增加多少菜单，而是将产品重新组织成三个相互协作的工作空间。

## **01 · Recruitment Workspace**

业务核心

[Kula - The Modern All-In-One ATS](https://images.openai.com/static-rsc-4/grcs9K3S1IeTkqQB8cpk1XurpqW1CXBln8XOlqdVcn98tw-q3R9AoUVl8jDjiSvxJbM0E_NuXwMNYE5l5Kk9QtFsNrDJpl0LOCy1f66zxkYS5FH0pPRH7GIlleoq2GCOScRZ7l_z2mT3KfPrh_BAgbvQYRjelRXEM2R9_8bFmy8?purpose=inline)

[kula.ai](https://www.kula.ai/applicant-tracking-system?utm_source=chatgpt.com)

**解决招聘者的日常业务问题**

- 候选人管理
- 岗位管理
- 沟通记录
- 跟进任务
- 招聘进度

## **02 · Agent Workspace**

智能核心

[Gemini 3.5 Flash API for Multimodal Reasoning and Agentic AI | Kie.ai](https://images.openai.com/static-rsc-4/-HxbQ7FRgO1R21A0cKhtJe2UJVutQP6uDeOw-CyODFYgjlTBi_p7xpMIAmkCxXB7ZA-mwrChd8HKZ5PPgSMGJQZtL1yqrZRoHNXhCqQmsDPViH6g4dnjzs21Kvwabc4nCqwgGsZQe_p8sECrBY_i1HUZuzS3PH4eKVYY3CPtPiI?purpose=inline)

[kie.ai](https://kie.ai/gemini-3-5-flash?utm_source=chatgpt.com)

**解决 AI 分析与任务执行问题**

- Hermes Runtime
- Agent 会话
- AI 分析
- Skill / Tool 调用
- 执行监控

## **03 · Browser Workspace**

执行核心

[Microsoft Edge browser gains Copilot Mode: All-seeing AI that researches, assists, and soon shops for you - Notebookcheck News](https://images.openai.com/static-rsc-4/XD_8d3qzrPASkopKEbQUcAsnLOSFJPPGoHOdwNTFFexFCV2_w8nikF80Oj20-ll8zEtFcoLAaMs0Y4lmifdNA-R4Gwgr6MPOXknwcg78d6wTYUy0OjwoxD26fVQG5vyRlZGUQ20ub_gGpn7rX6UathM3eITJ2pT73sN-ixqIDus?purpose=inline)

[notebookcheck.net](https://www.notebookcheck.net/Microsoft-Edge-browser-gains-Copilot-Mode-All-seeing-AI-that-researches-assists-and-soon-shops-for-you.1071576.0.html?utm_source=chatgpt.com)

**解决外部招聘平台交互问题**

- 内置浏览器
- 外部浏览器
- 页面解析
- 受控自动化
- 用户确认与执行

三个工作空间通过统一的审批中心、权限体系和业务数据模型协作，而不是各自维护一套候选人、任务和操作记录。

## **十二、我建议最终确定的 UI 产品方向**

# **ICA Personal R2.0**

## AI-Native Recruitment Workspace

面向个人招聘者的本地优先、AI 驱动、人机协同招聘工作台。

**业务层**

# **ICA Core**

候选人、岗位、沟通、跟进

**智能层**

# **Hermes**

Agent、Skills、任务执行

**执行层**

# **Browser**

页面观察、浏览器操作

**治理层**

# **Approval**

权限、人工审批、审计

**核心产品原则**

- Local-first：数据本地优先。
- AI-native：AI 深度融入业务工作流。
- Human-in-the-loop：关键操作人工确认。
- Modular Runtime：Agent 和 Browser 独立演进。
- Security-first：权限、审批和审计由 ICA 控制。

### **最终判断**

我建议保留现有 React 技术栈和 Tauri 桌面外壳，不需要为了 UI 重构而再次迁移前端技术栈。

这次真正需要改变的是：

1. 从七个分散功能入口转为三个核心工作空间。
2. 从传统 CRUD 页面转为招聘业务工作流。
3. 从简单 AI 对话框转为 Hermes Agent 工作台。
4. 从浏览器自动化入口转为完整 Browser Workspace。
5. 从分散的人工确认转为统一审批中心。
6. 从页面级状态管理转为跨 ICA、Hermes、Browser 的统一运行状态展示。

**一个重要取舍：** 不要让 ICA 变成 Hermes 的 GUI，也不要让 Browser 变成整个产品的中心。ICA 的核心价值仍然是招聘业务数据、上下文、工作流和决策管理；Hermes 与 Browser 是增强这一核心价值的两个执行能力层。

### **下一步建议**

建议先形成正式的 **《ICA R2.0 UI/UX Architecture & Design Specification》**，然后基于最新 main 进行 UI 与架构差异审计，再输出可直接交给 Hermes 多 Agent 团队执行的任务包。

这份规范应包含：

- 完整页面信息架构和导航树。
- 关键页面高保真设计。
- React 组件及 Design System 规范。
- ICA-RUST API / Tauri IPC 接口映射。
- Hermes Runtime 交互契约。
- Browser Workspace 交互契约。
- 权限、审批与审计交互规范。
- 分阶段实施及验收标准。

实施时继续遵循不自动 commit、push、merge 和 release 的工程约束。