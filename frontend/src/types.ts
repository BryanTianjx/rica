/** TypeScript mirrors of the backend Pydantic schemas. */

export type CandidateStatus =
  | 'New'
  | 'Contacted'
  | 'ReadNoReply'
  | 'Replied'
  | 'Active'
  | 'Interview'
  | 'Rejected'
  | 'Withdrawn'
  | 'Closed';

export type PositionStatus = 'Draft' | 'Open' | 'OnHold' | 'Closed';

export type FollowUpTaskStatus =
  | 'Pending'
  | 'Eligible'
  | 'Drafting'
  | 'Reviewing'
  | 'Rejected'
  | 'Approved'
  | 'Copied'
  | 'AwaitingResult'
  | 'Completed'
  | 'Cancelled'
  | 'Expired'
  | 'Blocked';

export type DraftStatus = 'generated' | 'edited' | 'approved' | 'rejected' | 'copied';

export type ValidationStatus = 'Passed' | 'Warning' | 'Blocked';

export type SenderType = 'Candidate' | 'Recruiter' | 'System';

export type ResultType =
  | 'sent'
  | 'not_sent'
  | 'candidate_replied'
  | 'candidate_no_reply'
  | 'candidate_declined'
  | 'interview'
  | 'other';

export type MessageType =
  | 'greeting'
  | 'reply'
  | 'follow_up_question'
  | 'position_intro'
  | 'salary_talk'
  | 'interview_invite'
  | 'post_interview'
  | 'polite_follow_up'
  | 'decline_ack';

export type Tone = 'formal' | 'friendly' | 'concise' | 'warm';

export interface Page<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
  has_more: boolean;
}

export interface Position {
  id: string;
  name: string;
  description: string | null;
  location: string | null;
  skill_requirements: string | null;
  experience_requirements: string | null;
  education_requirements: string | null;
  salary_min: number | null;
  salary_max: number | null;
  notes: string | null;
  status: PositionStatus;
  archived: boolean;
  created_at: string;
  updated_at: string;
  candidate_count: number;
}

export interface Conversation {
  id: string;
  candidate_id: string;
  channel: string;
  title: string | null;
  is_active: boolean;
  last_message_at: string | null;
  created_at: string;
  updated_at: string;
  message_count: number;
  /** Candidate's current position, resolved server-side (GET and POST agree). */
  position_id: string | null;
  position_name: string | null;
}

export interface Candidate {
  id: string;
  name: string;
  source: string;
  position_id: string | null;
  experience_summary: string | null;
  skill_summary: string | null;
  expected_salary: string | null;
  expected_location: string | null;
  job_intention: string | null;
  notes: string | null;
  status: CandidateStatus;
  archived: boolean;
  created_at: string;
  updated_at: string;
  position_name: string | null;
  message_count: number;
  active_follow_up_count: number;
}

export interface Message {
  id: string;
  candidate_id: string;
  conversation_id: string;
  sender_type: string | null;
  content: string;
  message_time: string | null;
  channel: string;
  source: string;
  created_at: string;
}

export interface TimelineEntry {
  id: string;
  kind: string;
  sender_type: string | null;
  content: string;
  message_time: string | null;
  source: string | null;
  status: string | null;
  created_at: string | null;
}

export interface ImportRecord {
  id: string;
  session_id: string;
  sequence: number;
  sender_type: string | null;
  content: string;
  message_time: string | null;
  status: string;
  duplicate_of_message_id: string | null;
  content_hash: string | null;
  validation_messages: string | null;
}

export interface ImportSession {
  id: string;
  candidate_id: string | null;
  source: string;
  status: string;
  detected_duplicate_count: number;
  confirmed_at: string | null;
  error_message: string | null;
  created_at: string;
  records: ImportRecord[];
}

export interface FollowUpTask {
  id: string;
  candidate_id: string;
  position_id: string | null;
  status: FollowUpTaskStatus;
  objective: string | null;
  reason: string | null;
  block_reason: string | null;
  recommended_at: string | null;
  expires_at: string | null;
  last_follow_up_at: string | null;
  follow_up_count: number;
  completed_at: string | null;
  result_type: string | null;
  result_note: string | null;
  result_recorded_at: string | null;
  created_at: string;
  updated_at: string;
  candidate_name: string | null;
}

export interface FollowUpDecision {
  outcome: 'eligible' | 'wait' | 'blocked';
  eligible: boolean;
  reasons: string[];
  block_reasons: string[];
  recommended_at: string | null;
  suggested_objective: string | null;
  risks: string[];
}

export interface Draft {
  id: string;
  candidate_id: string;
  task_id: string | null;
  message_type: string;
  tone: string;
  status: DraftStatus;
  objective: string | null;
  message_text: string;
  alternatives: string[];
  context_references: string[];
  uncertainties: string[];
  warnings: string[];
  validation_status: ValidationStatus;
  validation_messages: string[];
  quality_status: string | null;
  provider: string | null;
  model: string | null;
  prompt_version: string | null;
  is_edited: boolean;
  approved_at: string | null;
  copied_at: string | null;
  rejected_at: string | null;
  created_at: string;
  updated_at: string;
  // V2.0 AI 增强字段
  draft_group_id: string | null;
  routing_decision_json: string | null;
  routing_tier: string | null;
  routing_reason: string | null;
  cost_cents: number | null;
  prompt_tokens: number | null;
  completion_tokens: number | null;
}

/** V2.0 A/B 对比：一组同上下文、多模型生成的草稿 */
export interface DraftGroup {
  draft_group_id: string;
  created_at: string | null;
  drafts: Draft[];
}

export interface ContextPreview {
  candidate_id: string;
  candidate_summary: Record<string, unknown>;
  position_summary: Record<string, unknown>;
  message_count: number;
  recent_messages: Array<{
    id: string;
    sender: string;
    time: string | null;
    content: string;
  }>;
  notes: string[];
  unknowns: string[];
  conflicts: string[];
  redaction_applied: string[];
}

export interface DashboardCalendar {
  /** 聚合月份（UTC），如 2026-11。 */
  month: string;
  /** 逐日沟通条数（按 message_time ?? created_at 的 UTC 日期）。 */
  messages_by_day: Record<string, number>;
  /** 逐日跟进建议条数（pending follow-up 的 recommended_at）。 */
  followups_by_day: Record<string, number>;
}

export interface Dashboard {
  candidate_total: number;
  candidate_status_counts: Record<string, number>;
  active_follow_up_count: number;
  pending_follow_ups: FollowUpTask[];
  awaiting_result_count: number;
  pending_draft_count: number;
  recent_messages: Message[];
  position_total: number;
  open_position_count: number;
  calendar?: DashboardCalendar | null;
}

export interface FollowUpPolicy {
  id: string;
  name: string;
  first_follow_up_delay_hours: number;
  max_active_follow_ups: number;
  min_interval_hours: number;
  task_validity_days: number;
  is_active: boolean;
}

export interface SettingsInfo {
  app_name: string;
  ai_enabled: boolean;
  ai_provider: string;
  ai_model: string;
  ai_base_url: string;
  database_url: string;
  backup_dir: string;
  backup_keep_count: number;
}

export type AiQualityTier = 'economy' | 'standard' | 'premium';

/** Where a configured model row comes from. */
export type AiModelSource =
  /** 来自 ai_model_configs —— 用户在设置页保存的配置，运行时权威来源。 */
  | 'database'
  /** ai_model_configs 为空时后端从 .env 构造的回落行，不可 PUT/DELETE。 */
  | 'settings';

export interface AiModel {
  model_id: string;
  display_name: string;
  provider_type: string;
  base_url: string;
  api_key_env: string;
  quality_tier: AiQualityTier;
  input_cost_per_1k_tokens: number;
  output_cost_per_1k_tokens: number;
  max_output_tokens: number;
  max_input_tokens: number;
  timeout: number;
  temperature: number;
  enabled: boolean;
  is_default: boolean;
  sort_order: number;
  /** Key 是否已配置；接口永不回显 key 本身。 */
  api_key_configured: boolean;
  source: AiModelSource;
}

export interface AiModelsResponse {
  models: AiModel[];
  routing: Record<string, string>;
}

/**
 * 写模型的公共字段。``model_id`` 是行标识（也是凭据槽位名），不可更新：
 * 改 ID 等于新建配置。字段名必须与后端 ModelCreate/ModelUpdate 一致 ——
 * 后端未设 extra=forbid，拼错的字段会被静默丢弃而不是报错。
 */
export interface AiModelWrite {
  display_name?: string;
  provider_type?: string;
  base_url?: string;
  quality_tier?: AiQualityTier;
  input_cost_per_1k_tokens?: number;
  output_cost_per_1k_tokens?: number;
  max_output_tokens?: number;
  max_input_tokens?: number;
  timeout?: number;
  temperature?: number;
  enabled?: boolean;
  is_default?: boolean;
  sort_order?: number;
}

export interface AiModelCreate extends AiModelWrite {
  model_id: string;
}

export interface AiModelDeleted {
  model_id: string;
  deleted: boolean;
}

export interface BackupRecord {
  name: string;
  path: string;
  size_bytes: number;
  created_at: string;
}

// ── AI key 桌面端配置状态（只含脱敏尾号，永不回显完整密钥） ───────────────

export interface AiKeyStatus {
  ai_enabled: boolean;
  key_masked: string | null;
  /** 本次写入/清除的目标模型槽位；null 或省略表示安装级密钥。 */
  model_id?: string | null;
}

// ── 审计记录（G409-GAP：桌面/前端审计查看入口） ─────────────────────────

export interface AuditEvent {
  id: string;
  event_type: string;
  entity_type: string | null;
  entity_id: string | null;
  actor: string;
  summary: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

// ── V2.0 AI 成本控制（V2-AI-3） ───────────────────────────────────────

export interface AiUsageDay {
  date: string;
  total_cost_cents: number;
  request_count: number;
  by_model: Array<{
    model_id: string;
    cost_cents: number;
    request_count: number;
  }>;
}

export interface AiUsage {
  today: {
    total_cost_cents: number;
    total_prompt_tokens: number;
    total_completion_tokens: number;
    request_count: number;
    daily_budget_cents: number;
    remaining_cents: number;
    budget_utilization: number;
    tier_downgrade_active: boolean;
  };
  history: AiUsageDay[];
  by_model_all_time: Array<{
    model_id: string;
    total_cost_cents: number;
    total_requests: number;
  }>;
}

export interface AiBudget {
  daily_budget_cents: number;
  warning_threshold: number;
  today_spend_cents: number;
  remaining_cents: number;
  status: 'ok' | 'warning' | 'exceeded';
}

export interface AiBudgetPatch {
  daily_budget_cents: number;
  warning_threshold: number;
}

// ── PRIV-07: AI 出网告知与同意 ───────────────────────────────────────────

export interface AiEgressDisclosure {
  enabled: boolean;
  consent_granted: boolean;
  provider: string;
  model: string;
  base_url: string;
  masked_fields: string[];
  include_candidate_name: boolean;
  notice: string;
}

// ── V2.0 阶段 B: 认证 ─────────────────────────────────────────────────────

export interface User {
  id: string;
  email: string;
  display_name: string;
  role: string;
  tenant_id: string;
  is_active: boolean;
}

export interface TokenPair {
  access_token: string;
  token_type: string;
  expires_in: number;
  refresh_token: string;
  user: User;
}

// ── 浏览器子系统（BROWSER-01..05，/api/browser/v1）───────────────────────────
//
// 字段名全部对齐实测响应与后端 schema（docs/plan/02-frontend.md：先跑一遍再定
// 类型）。runtime/permissions/connector/audit 来自 dev 后端真实响应；提取的
// schema 来自 app/schemas/candidate_extraction.py；sessions/navigate/snapshot/
// screenshot/actions/tasks 的响应键来自 app/application/browser_service.py 的
// 响应组装代码（真实 HTTP 路径当前被 bridge 线程缺陷阻断，见交付报告）。

export type BrowserRuntimeState = 'stopped' | 'ready' | 'failed';

export interface BrowserRuntimeBridge {
  pid: number | null;
  port: number | null;
  restarts: number;
}

/** GET /runtime/status 的 capabilities 块（启动后带 browser_version 等）。 */
export interface BrowserRuntimeCapabilities {
  playwright_available: boolean;
  playwright_version?: string | null;
  browser_version?: string | null;
  launched?: boolean;
  vision_fallback?: boolean;
  external_connector?: boolean;
  /** bridge 不可达时的错误码（如 ReadTimeout）。 */
  reason?: string;
}

export interface BrowserSessionRow {
  session_id: string;
  state: string;
  started_at?: string | null;
}

export interface BrowserRuntimeStatus {
  state: BrowserRuntimeState;
  headed: boolean;
  profile_id: string | null;
  uptime_ms: number | null;
  cold_start_ms: number | null;
  bridge: BrowserRuntimeBridge;
  last_error: string | null;
  data_root: string;
  capabilities: BrowserRuntimeCapabilities;
  sessions: BrowserSessionRow[];
  limits: {
    max_sessions: number;
    snapshot_max_bytes: number;
    screenshot_max_bytes: number;
  };
}

export interface BrowserRuntimeStartResult {
  state: BrowserRuntimeState;
  profile_id: string | null;
  already_running: boolean;
  browser_version?: string | null;
  playwright_version?: string | null;
  headed: boolean;
  started_at?: string | null;
  cold_start_ms?: number | null;
  stale_lock_reclaimed?: boolean;
  start_ms?: number | null;
}

export interface BrowserRuntimeStopResult {
  state: BrowserRuntimeState;
  closed_sessions: number;
  already_stopped: boolean;
  forced: boolean;
}

export interface BrowserSessionCreated {
  session_id: string;
  state: string;
  profile_id: string | null;
  created_at: string | null;
}

export interface BrowserSessionClosed {
  session_id: string;
  state: string;
  already_closed: boolean;
  close_reason: string | null;
}

export interface BrowserNavigation {
  session_id: string;
  requested_url: string | null;
  final_url: string | null;
  http_status: number | null;
  title: string | null;
  duration_ms: number | null;
  action_id: string;
  permission_id: string | null;
  blocked_attempts: Array<Record<string, unknown>>;
  new_tabs: Array<Record<string, unknown>>;
}

/** POST /sessions/{id}/snapshot 的响应（service 透传 bridge 快照 + snapshot_id）。 */
export interface BrowserSnapshot {
  snapshot_id: string;
  url?: string;
  title?: string | null;
  [key: string]: unknown;
}

export interface BrowserArtifact {
  artifact_id: string;
  sha256: string | null;
  bytes: number | null;
  width: number | null;
  height: number | null;
  format: string;
  url: string;
  path: string;
  created_at: string | null;
}

export interface BrowserObserved {
  title?: string | null;
  url?: string | null;
  text?: string | null;
  [key: string]: unknown;
}

export interface BrowserVerification {
  passed: boolean;
  kind: string;
  value?: unknown;
  attempts?: number;
  elapsed_ms?: number;
  observed?: BrowserObserved | null;
}

export interface BrowserActionResult {
  action_id: string;
  state: string;
  kind: string;
  risk_level: string;
  target_domain: string | null;
  duration_ms: number | null;
  observed: BrowserObserved | null;
  verification: BrowserVerification | null;
  permission_id: string | null;
  approval_id: string | null;
  blocked_attempts: Array<Record<string, unknown>>;
  new_tabs: Array<Record<string, unknown>>;
}

export interface BrowserPermission {
  permission_id: string;
  scope: string;
  domain: string;
  action_kind: string | null;
  risk_ceiling: string;
  profile_id: string | null;
  expires_at: string | null;
  state: string;
}

export interface BrowserApproval {
  approval_id: string;
  state: string;
  request_type: string;
  risk_level: string;
  summary: string | null;
  action_digest: string | null;
  action_id?: string | null;
  task_id?: string | null;
  expires_at: string | null;
  decided_at?: string | null;
}

export interface BrowserTaskStep {
  step_id: string;
  sequence: number;
  kind: string;
  state: string;
  url?: string | null;
  target?: Record<string, unknown> | null;
  error_code?: string | null;
  approval_id?: string | null;
  [key: string]: unknown;
}

export interface BrowserTask {
  task_id: string;
  session_id: string;
  intent: string;
  state: string;
  risk_level: string;
  plan_sha256: string | null;
  steps_count: number;
  executed_steps: number;
  current_step: number | null;
  cancel_requested_at: string | null;
  error_code: string | null;
  error_message: string | null;
  steps: BrowserTaskStep[];
}

export interface BrowserAuditEvent {
  event_id: string;
  event_type: string;
  entity_type: string;
  entity_id: string | null;
  summary: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface BrowserConnectorInfo {
  running: boolean;
  pid: number | null;
  frame_port: number;
  control_port: number;
  uptime_s: number | null;
}

export interface BrowserConnectorStatus {
  connector: BrowserConnectorInfo;
  live: Record<string, unknown>;
  connectors: Array<Record<string, unknown>>;
  sessions: Array<Record<string, unknown>>;
  tabs: Array<Record<string, unknown>>;
  extension_id: string | null;
}

export interface BrowserExtractionProvenance {
  session_id: string;
  snapshot_action_id: string;
  source_url: string;
  source_domain: string;
  page_title?: string | null;
  snapshot_sha256: string;
  snapshot_path: string;
  snapshot_node_count: number;
  snapshot_masked_fields: number;
}

/** 提取字段的可编辑集合（PATCH 与落档摘要共用同一组键）。 */
export type BrowserExtractionFields = {
  name?: string | null;
  job_intention?: string | null;
  expected_salary?: string | null;
  expected_location?: string | null;
  experience_summary?: string | null;
  skill_summary?: string | null;
  note?: string | null;
};

export interface BrowserExtractionRevision {
  revision_no: number;
  changes: Array<Record<string, unknown>>;
  note: string | null;
  created_at: string;
}

export interface BrowserExtraction {
  id: string;
  status: string;
  fields: Record<string, string | null>;
  field_origins: Record<string, unknown>;
  missing_fields: string[];
  position_hint: string | null;
  company_hint: string | null;
  mask_hits: number;
  provenance: BrowserExtractionProvenance;
  duplicate: { kind: string; candidate_id: string | null; evidence: Record<string, unknown> } | null;
  review_digest: string | null;
  reviewed_at: string | null;
  review_note: string | null;
  import_session_id: string | null;
  candidate_id: string | null;
  confirmed_at: string | null;
  revisions: BrowserExtractionRevision[];
  created_at: string;
  updated_at: string;
}

export interface BrowserExtractionConfirmResult {
  session_id: string;
  extraction_id: string;
  candidate_id: string;
  duplicate_kind: string | null;
  duplicate_override: boolean;
  fields_written: string[];
}

/** 运行时自检单项：pass=通过；missing=缺失（reason 说明）；unverified=系统内无探针。 */
export interface BrowserSelfCheckItem {
  key: 'playwright' | 'chromium' | 'native_host';
  label: string;
  verdict: 'pass' | 'missing' | 'unverified';
  detail: string;
}
