//! ICA application layer: use cases and repository ports.
//!
//! Dependency rule (enforced by this crate's manifest): the domain plus the
//! ``async-trait`` proc macro. No axum, no sqlx, no tokio, no hyper — the
//! use cases talk to a port trait that ``ica-infra`` implements.

pub mod ai_context;
pub mod ai_key;
pub mod ai_registry;
pub mod audit_trail;
pub mod auth;
pub mod auth_recovery;
pub mod backup;
pub mod candidates;
pub mod conversations;
pub mod dashboard;
pub mod drafts;
pub mod error;
pub mod extractions;
pub mod follow_ups;
pub mod imports;
pub mod payload;
pub mod ports;
pub mod positions;
pub mod prompts;
pub mod request_context;
pub mod runtime;
pub mod send_records;
pub mod settings;
pub mod sync;
pub mod validators;

pub use ai_key::{
    api_key_resolved, clear_ai_key, installation_key_present, mask_key, resolve_model_api_key,
    slot_for, write_ai_key, AI_KEY_SLOT, AI_KEY_SLOT_PREFIX,
};
pub use ai_registry::{
    budget_read, budget_status, estimate_cost_cents, get_ai_routing, get_ai_usage,
    get_default_model, get_model, get_model_by_tier, list_ai_models, routing_table, select_model,
    today_spend_cents, update_ai_routing, AiBudgetSettings, AiModelData, AiModelRead,
    AiModelsOverview, AiSettingsView, AiUsageAllTime, AiUsageByModel, AiUsageRow, BudgetRead,
    RoutingDecision, TodayUsage, UsageDay, UsageOverview, DEFAULT_ROUTING, QUALITY_TIERS,
};
pub use audit_trail::{platform_audit_events, recent_audit_events};
pub use auth::{
    create_share, delete_share, list_shares, local_user, login, logout, me, refresh, register,
    IssuedTokens, ShareData, UserRead,
};
pub use auth_recovery::{
    transition as auth_transition, AuthEvent, AuthPhase, AuthSessionState, SessionEndReason,
    ALL_AUTH_PHASES,
};
pub use backup::{create_backup, list_backups, restore_backup, restore_backup_checked};
pub use candidates::{
    create_candidate, delete_candidate, get_candidate, get_candidate_detail, list_candidates,
    update_candidate, CandidateDetail, ListCandidates,
};
pub use conversations::{
    candidate_timeline, candidate_timeline_scoped, create_conversation, create_conversation_scoped,
    create_message, create_message_scoped, list_conversations, list_conversations_scoped,
    list_messages, list_messages_scoped, ListConversations, ListMessages,
};
pub use dashboard::{dashboard, platform_dashboard, DashboardSnapshot};
pub use drafts::{
    approve_draft, approve_draft_scoped, copy_draft, copy_draft_scoped, generate, generate_scoped,
    get_draft, get_draft_group, get_draft_group_scoped, get_draft_scoped, list_drafts,
    list_drafts_scoped, preview_context, preview_context_scoped, reject_draft, reject_draft_scoped,
    select_draft_from_group, select_draft_from_group_scoped, update_draft, update_draft_scoped,
};
pub use error::ApplicationError;
pub use extractions::{confirm_candidate, confirm_candidate_scoped};
pub use follow_ups::{
    cancel_task, cancel_task_scoped, complete_task, complete_task_scoped, create_task,
    create_task_scoped, ensure_default_policy, evaluate, evaluate_scoped, get_task,
    get_task_scoped, list_tasks, list_tasks_scoped, record_result, record_result_scoped,
    update_policy, update_task, update_task_scoped,
};
pub use imports::{
    confirm_import, confirm_import_scoped, get_import, get_import_scoped, ocr_screenshot,
    ocr_screenshot_scoped, preview_import, preview_import_scoped, reject_import,
    reject_import_scoped, OcrOutcome,
};
pub use payload::{
    CandidateFilter, CandidatePatch, ConversationFilter, DraftCopyOutcome, DraftFilter, DraftPatch,
    Field, FollowUpPolicyPatch, FollowUpResultPatch, FollowUpTaskFilter, FollowUpTaskPatch,
    ImportCorrection, MessageFilter, NewCandidate, NewCandidateConfirm, NewConversation,
    NewDraftGeneration, NewDraftGroupSelection, NewFollowUpEvaluation, NewFollowUpResult,
    NewFollowUpTask, NewImportConfirm, NewImportOcr, NewImportPreview, NewMessage, NewPosition,
    NewSendRecord, PositionPatch, SendRecordFilter,
};
pub use ports::{
    AiModelRepository, AiUsageRepository, AppSettingsRepository, AuditTrailRepository,
    AuthRepository, AuthSecurity, BackupRecord, BackupStore, BrowserExtractionRepository,
    CandidateConfirm, CandidateConfirmOutcome, CandidateRepository, ConversationRepository,
    DashboardRepository, DataSnapshotRepository, DraftGeneration, DraftGenerator, DraftRepository,
    ExportSealer, ExtractionDraft, FollowUpPolicyRecord, FollowUpRepository, ImportConfirmOutcome,
    ImportRecordCorrection, ImportRepository, MessageRepository, NewFollowUpTaskRow,
    NewImportRecordRow, NewImportSessionRow, OcrEngine, PositionFilter, PositionRepository,
    RepositoryError, RestoreOutcome, SecretStore, SendRecordRepository, SnapshotOutcome,
};
pub use positions::{
    create_position, create_position_scoped, delete_position, delete_position_scoped, get_position,
    get_position_scoped, list_positions, list_positions_scoped, update_position,
    update_position_scoped, ListPositions,
};
pub use request_context::{
    authorize, authorize_platform, require_tenant_scope, AuthMethod, DenyReason, Permission,
    PlatformPermission, PrincipalKind, RequestContext, Scope, TenantRole,
};
pub use runtime::{AppRuntime, RuntimeConfig, RuntimeLifecycle, RuntimePhase};
pub use send_records::{
    list_send_records, list_send_records_scoped, record_send, record_send_scoped, ListSendRecords,
};
pub use settings::{
    read_egress_disclosure, write_egress_consent, EgressDisclosure, SettingsConfigView,
    SettingsSnapshot, AI_EGRESS_CONSENT_KEY, EGRESS_GRANTED_EVENT, EGRESS_NOTICE,
    EGRESS_REVOKED_EVENT, MASKED_CATEGORIES,
};
pub use sync::{export_data, import_data, ExportOutcome, ExportSnapshot, SealedSnapshot};
