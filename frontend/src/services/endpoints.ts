/** Typed wrappers over the backend endpoints. */

import { api, query } from './api';
import type {
  AiEgressDisclosure,
  AiModel,
  AiModelCreate,
  AiModelDeleted,
  AiModelsResponse,
  AiModelWrite,
  AiKeyStatus,
  AuditEvent,
  BackupRecord,
  BrowserApproval,
  BrowserArtifact,
  BrowserActionResult,
  BrowserAuditEvent,
  BrowserConnectorStatus,
  BrowserExtraction,
  BrowserExtractionConfirmResult,
  BrowserExtractionFields,
  BrowserNavigation,
  BrowserPermission,
  BrowserRuntimeStartResult,
  BrowserRuntimeStatus,
  BrowserRuntimeStopResult,
  BrowserSessionClosed,
  BrowserSessionCreated,
  BrowserSnapshot,
  BrowserTask,
  BrowserVerification,
  Candidate,
  ContextPreview,
  Dashboard,
  Draft,
  DraftGroup,
  FollowUpDecision,
  FollowUpPolicy,
  FollowUpTask,
  ImportSession,
  Message,
  MessageType,
  Page,
  Position,
  ResultType,
  SettingsInfo,
  TimelineEntry,
  Tone,
  Conversation,
  TokenPair,
  User,
} from '../types';

export const candidatesApi = {
  list: (params: {
    search?: string;
    status?: string;
    position_id?: string;
    include_archived?: boolean;
    offset?: number;
    limit?: number;
  } = {}) => api.get<Page<Candidate>>(`/candidates${query(params)}`),

  get: (id: string) => api.get<Candidate>(`/candidates/${id}`),

  create: (body: Partial<Candidate> & { name: string }) =>
    api.post<Candidate>('/candidates', body),

  update: (id: string, body: Partial<Candidate>) =>
    api.patch<Candidate>(`/candidates/${id}`, body),

  remove: (id: string) => api.delete<void>(`/candidates/${id}`),

  timeline: (id: string) => api.get<TimelineEntry[]>(`/candidates/${id}/timeline`),

  context: (id: string) => api.get<ContextPreview>(`/candidates/${id}/context`),
};

export const positionsApi = {
  list: (params: { search?: string; include_archived?: boolean; limit?: number } = {}) =>
    api.get<Page<Position>>(`/positions${query(params)}`),

  get: (id: string) => api.get<Position>(`/positions/${id}`),

  create: (body: Partial<Position> & { name: string }) =>
    api.post<Position>('/positions', body),

  update: (id: string, body: Partial<Position>) => api.patch<Position>(`/positions/${id}`, body),

  remove: (id: string) => api.delete<void>(`/positions/${id}`),
};

export const conversationsApi = {
  list: (params: { candidate_id?: string; limit?: number } = {}) =>
    api.get<Page<Conversation>>(`/conversations${query(params)}`),
  create: (body: { candidate_id: string; channel?: string; title?: string }) =>
    api.post<Conversation>('/conversations', body),
};

export const messagesApi = {
  list: (params: { candidate_id?: string; conversation_id?: string; limit?: number } = {}) =>
    api.get<Page<Message>>(`/messages${query(params)}`),

  create: (body: {
    candidate_id: string;
    conversation_id?: string;
    sender_type?: string;
    content: string;
    channel?: string;
    message_time?: string;
    source?: string;
  }) => api.post<Message>('/messages', body),
};

export const importsApi = {
  preview: (body: { candidate_id?: string; raw_text: string }) =>
    api.post<ImportSession>('/imports/preview', body),

  get: (id: string) => api.get<ImportSession>(`/imports/${id}`),

  confirm: (id: string, body: { candidate_id: string }) =>
    api.post<{ session_id: string; saved_message_ids: string[]; skipped_count: number; duplicate_count: number }>(
      `/imports/${id}/confirm`,
      body,
    ),

  reject: (id: string) => api.post<ImportSession>(`/imports/${id}/reject`),
};

export const followUpsApi = {
  list: (params: { candidate_id?: string; status?: string; offset?: number; limit?: number } = {}) =>
    api.get<Page<FollowUpTask>>(`/follow-ups${query(params)}`),

  get: (id: string) => api.get<FollowUpTask>(`/follow-ups/${id}`),

  evaluate: (candidateId: string) =>
    api.post<FollowUpDecision>('/follow-ups/evaluate', { candidate_id: candidateId }),

  create: (body: { candidate_id: string; position_id?: string; objective?: string }) =>
    api.post<FollowUpTask>('/follow-ups', body),

  update: (id: string, body: Partial<FollowUpTask>) =>
    api.patch<FollowUpTask>(`/follow-ups/${id}`, body),

  cancel: (id: string) => api.post<FollowUpTask>(`/follow-ups/${id}/cancel`),

  complete: (id: string) => api.post<FollowUpTask>(`/follow-ups/${id}/complete`),

  recordResult: (id: string, body: { result_type: ResultType; note?: string }) =>
    api.post<FollowUpTask>(`/follow-ups/${id}/record-result`, body),

  getPolicy: () => api.get<FollowUpPolicy>('/follow-ups/policy/current'),

  updatePolicy: (body: Partial<FollowUpPolicy>) =>
    api.patch<FollowUpPolicy>('/follow-ups/policy/current', body),
};

export const draftsApi = {
  list: (params: { candidate_id?: string; task_id?: string; status?: string } = {}) =>
    api.get<Page<Draft>>(`/drafts${query(params)}`),

  get: (id: string) => api.get<Draft>(`/drafts/${id}`),

  generate: (body: {
    candidate_id: string;
    task_id?: string;
    message_type?: MessageType;
    tone?: Tone;
    user_instruction?: string;
    include_alternatives?: boolean;
    max_length?: number;
  }) => api.post<Draft>('/drafts/generate', body),

  generateComparison: (body: {
    candidate_id: string;
    task_id?: string;
    message_type?: MessageType;
    tone?: Tone;
    user_instruction?: string;
    include_alternatives?: boolean;
    max_length?: number;
    comparison_models: string[];
  }) => api.post<Draft[]>('/drafts/generate', body),

  update: (id: string, body: { message_text?: string; objective?: string }) =>
    api.patch<Draft>(`/drafts/${id}`, body),

  approve: (id: string) => api.post<Draft>(`/drafts/${id}/approve`, {}),

  reject: (id: string) => api.post<Draft>(`/drafts/${id}/reject`),

  copy: (id: string) =>
    api.post<{ draft_id: string; message_text: string; copied_at: string; note: string }>(
      `/drafts/${id}/copy`,
    ),
};

/** V2.0 A/B 对比：草稿组 */
export const draftGroupsApi = {
  get: (groupId: string) => api.get<DraftGroup>(`/draft-groups/${groupId}`),

  /** 选中组内某一份草稿，其余自动置为 rejected */
  select: (groupId: string, draftId: string) =>
    api.post<DraftGroup>(`/draft-groups/${groupId}/select`, { draft_id: draftId }),
};

export const aiApi = {
  listModels: () => api.get<AiModelsResponse>('/ai/models'),
  getRouting: () => api.get<{ routing: Record<string, string> }>('/ai/routing'),
  updateRouting: (routing: Record<string, string>) =>
    api.patch<{ routing: Record<string, string> }>('/ai/routing', { routing }),

  // ── 模型配置写端点（admin；本地单人模式无感） ────────────────────────
  // model_id 常含 `/` 与 `:`（如 nvidia/nemotron-3.5-super-vl:free），后端用
  // {model_id:path} 接收，但 URL 里必须百分号编码，否则路径会被截断。
  /** 新建一条模型配置。重复的 model_id 返回 409（ai_model_exists），不覆盖。 */
  createModel: (body: AiModelCreate) => api.post<AiModel>('/ai/models', body),

  /** 局部更新；`model_id` 不可改（改 ID 等于新建配置）。 */
  updateModel: (modelId: string, body: AiModelWrite) =>
    api.put<AiModel>(`/ai/models/${encodeURIComponent(modelId)}`, body),

  /** 设为运行时默认模型；后端保证同一时刻只有一行 is_default。 */
  setDefaultModel: (modelId: string) =>
    api.put<AiModel>(`/ai/models/${encodeURIComponent(modelId)}/default`),

  /** 删除配置。后端不会自动补一个新的默认模型，也不会删除该模型的凭据槽位。 */
  deleteModel: (modelId: string) =>
    api.delete<AiModelDeleted>(`/ai/models/${encodeURIComponent(modelId)}`),
};

// 成本控制（usage/budget）的 UI 已暂缓——留待未来的后台管理面；
// 后端 /ai/usage、/ai/budget 端点保持不变，前端包装届时随管理界面一并回归。

export const dashboardApi = {
  get: () => api.get<Dashboard>('/dashboard'),
};

export const authApi = {
  me: () => api.get<User>('/auth/me'),
  login: (body: { email: string; password: string }) =>
    api.post<TokenPair>('/auth/login', body),
  register: (body: { email: string; password: string; display_name?: string }) =>
    api.post<User>('/auth/register', body),
  refresh: (refreshToken: string) =>
    api.post<TokenPair>('/auth/refresh', { refresh_token: refreshToken }),
  logout: (refreshToken: string) =>
    api.post<void>('/auth/logout', { refresh_token: refreshToken }),
};

export const settingsApi = {
  get: () => api.get<SettingsInfo>('/settings'),
  getAiEgress: () => api.get<AiEgressDisclosure>('/settings/ai-egress'),
  setAiEgressConsent: (consentGranted: boolean) =>
    api.post<AiEgressDisclosure>('/settings/ai-egress/consent', {
      consent_granted: consentGranted,
    }),
  /**
   * 写入 AI API Key —— 走 OS 凭据库路径，永不回显明文。
   *
   * 不传 `modelId` 时写安装级槽位（所有模型共用，并剥离 .env 明文）；
   * 传 `modelId` 时只写该模型的独立槽位。响应只有脱敏尾号。
   */
  setAiKey: (key: string, modelId?: string) =>
    api.put<AiKeyStatus>(
      '/settings/ai-key',
      modelId ? { ai_api_key: key, model_id: modelId } : { ai_api_key: key },
    ),
  /** 清除密钥；不传 `modelId` 时清安装级槽位。 */
  clearAiKey: (modelId?: string) =>
    api.delete<AiKeyStatus>(
      `/settings/ai-key${modelId ? `?model_id=${encodeURIComponent(modelId)}` : ''}`,
    ),
  listBackups: () => api.get<BackupRecord[]>('/backups'),
  createBackup: () => api.post<BackupRecord>('/backups'),
  restore: (name: string) =>
    api.post<{ restored_from: string; database: string; previous_copy: string; message: string }>(
      `/backups/${encodeURIComponent(name)}/restore`,
      { confirm: true },
    ),
  // 审计记录（G409-GAP）：后端按 event_type/entity_type/entity_id 过滤、最新在前。
  listAuditEvents: (
    params: {
      event_type?: string;
      entity_type?: string;
      entity_id?: string;
      limit?: number;
    } = {},
  ) => api.get<AuditEvent[]>(`/audit-events${query(params)}`),
};

// ── 浏览器子系统（BROWSER-01..05，全局前缀 /api 下再套 /browser/v1）──────────
// 全部 36 个端点都是已实现的薄适配层；这里只包面板用到的端点。
// 请求体字段名对齐 app/schemas/browser.py（risk_level 不在任何请求体里：
// 服务端自行分级，请求里带 risk_level 会 422）。
export const browserApi = {
  // 运行时
  runtimeStatus: () =>
    api.get<{ runtime: BrowserRuntimeStatus }>('/browser/v1/runtime/status'),
  runtimeStart: () =>
    api.post<{ runtime: BrowserRuntimeStartResult }>('/browser/v1/runtime/start', {}),
  runtimeStop: (force = false) =>
    api.post<{ runtime: BrowserRuntimeStopResult }>('/browser/v1/runtime/stop', { force }),

  // 会话
  createSession: () => api.post<{ session: BrowserSessionCreated }>('/browser/v1/sessions', {}),
  closeSession: (sessionId: string) =>
    api.delete<{ session: BrowserSessionClosed }>(`/browser/v1/sessions/${sessionId}`),
  navigate: (
    sessionId: string,
    body: { url: string; wait_until?: 'load' | 'domcontentloaded' | 'networkidle'; timeout_ms?: number },
  ) => api.post<{ navigation: BrowserNavigation }>(`/browser/v1/sessions/${sessionId}/navigate`, body),
  snapshot: (
    sessionId: string,
    body: { max_bytes?: number; include_text?: boolean; mask_sensitive?: boolean } = {},
  ) => api.post<{ snapshot: BrowserSnapshot }>(`/browser/v1/sessions/${sessionId}/snapshot`, body),
  screenshot: (sessionId: string, body: { full_page?: boolean } = {}) =>
    api.post<{ artifact: BrowserArtifact }>(`/browser/v1/sessions/${sessionId}/screenshot`, body),
  executeAction: (
    sessionId: string,
    body: {
      kind: 'click' | 'fill' | 'select' | 'press' | 'scroll';
      target: { by: 'role' | 'label' | 'text' | 'css' | 'ref'; role?: string; name?: string; label?: string; text?: string; css?: string; ref?: string };
      value?: string;
      approval_id?: string;
      timeout_ms?: number;
    },
  ) => api.post<{ action: BrowserActionResult }>(`/browser/v1/sessions/${sessionId}/actions`, body),
  verifyAction: (
    actionId: string,
    expect: {
      kind: 'url_contains' | 'url_equals' | 'element_visible' | 'element_hidden' | 'text_contains' | 'value_equals' | 'http_status';
      value?: unknown;
      target?: { by: 'role' | 'label' | 'text' | 'css' | 'ref'; css?: string; ref?: string; text?: string; role?: string; name?: string; label?: string };
    },
  ) => api.post<{ verification: BrowserVerification }>(`/browser/v1/actions/${actionId}/verify`, { expect }),

  // 产物
  artifactUrl: (artifactId: string) => `/api/browser/v1/artifacts/${artifactId}`,

  // 权限
  listPermissions: () => api.get<{ permissions: BrowserPermission[] }>('/browser/v1/permissions'),
  grantPermission: (body: {
    domain: string;
    risk_ceiling?: 'L1' | 'L2' | 'L3';
    expires_in_minutes?: number;
    note?: string;
  }) => api.post<{ permission: BrowserPermission }>('/browser/v1/permissions', body),
  revokePermission: (permissionId: string) =>
    api.delete<void>(`/browser/v1/permissions/${permissionId}`),

  // 审批（无列表端点：待审批列表从 /audit 的 browser.approval.requested 事件取 id）
  requestApproval: (body: { session_id: string; request_type?: 'action' | 'task' | 'data_write'; summary?: string; ttl_seconds?: number }) =>
    api.post<{ approval: BrowserApproval }>('/browser/v1/approvals', body),
  getApproval: (approvalId: string) =>
    api.get<{ approval: BrowserApproval }>(`/browser/v1/approvals/${approvalId}`),
  decideApproval: (approvalId: string, decision: 'approve' | 'reject', note?: string) =>
    api.post<{ approval: BrowserApproval }>(`/browser/v1/approvals/${approvalId}/decide`, {
      decision,
      note,
    }),

  // 任务
  createTask: (body: { session_id: string; intent: string; auto_run?: boolean; require_verification?: boolean }) =>
    api.post<{ task: BrowserTask }>('/browser/v1/tasks', body),
  getTask: (taskId: string) => api.get<{ task: BrowserTask }>(`/browser/v1/tasks/${taskId}`),
  runTask: (taskId: string, approvals: Record<string, string> = {}) =>
    api.post<{ task: BrowserTask }>(`/browser/v1/tasks/${taskId}/run`, { approvals }),
  cancelTask: (taskId: string, reason: 'user' | 'timeout' | 'policy' = 'user') =>
    api.post<{ task: BrowserTask }>(`/browser/v1/tasks/${taskId}/cancel`, { reason }),
  taskAudit: (taskId: string) =>
    api.get<{ events: BrowserAuditEvent[] }>(`/browser/v1/tasks/${taskId}/audit`),

  // 审计（browser.* 切片，最新在前）
  audit: (params: { limit?: number; task_id?: string; session_id?: string } = {}) =>
    api.get<{ events: BrowserAuditEvent[] }>(`/browser/v1/audit${query(params)}`),

  // 外部连接器
  connectorStatus: () => api.get<BrowserConnectorStatus>('/browser/v1/connector/status'),

  // 候选人提取
  listExtractions: () =>
    api.get<{ extractions: BrowserExtraction[] }>('/browser/v1/candidate-extractions'),
  getExtraction: (id: string) =>
    api.get<{ extraction: BrowserExtraction }>(`/browser/v1/candidate-extractions/${id}`),
  patchExtraction: (id: string, fields: BrowserExtractionFields) =>
    api.patch<{ extraction: BrowserExtraction }>(`/browser/v1/candidate-extractions/${id}`, fields),
  reviewExtraction: (id: string, decision: 'approve' | 'reject', note?: string) =>
    api.post<{ extraction: BrowserExtraction }>(`/browser/v1/candidate-extractions/${id}/review`, {
      decision,
      note,
    }),
  confirmExtraction: (
    importSessionId: string,
    body: { position_id?: string; confirm_duplicate?: boolean; note?: string },
  ) =>
    api.post<BrowserExtractionConfirmResult>(
      `/imports/${encodeURIComponent(importSessionId)}/confirm-candidate`,
      body,
    ),
};
