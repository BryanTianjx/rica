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
    api_key_resolved, clear_ai_key, mask_key, resolve_model_api_key, slot_for, write_ai_key,
    AI_KEY_SLOT, AI_KEY_SLOT_PREFIX,
};
pub use ai_registry::{
    budget_read, budget_status, estimate_cost_cents, get_ai_routing, get_ai_usage,
    get_default_model, get_model, get_model_by_tier, list_ai_models, routing_table, select_model,
    today_spend_cents, update_ai_routing, AiBudgetSettings, AiModelData, AiModelRead,
    AiModelsOverview, AiSettingsView, AiUsageAllTime, AiUsageByModel, AiUsageRow, BudgetRead,
    RoutingDecision, TodayUsage, UsageDay, UsageOverview, DEFAULT_ROUTING, QUALITY_TIERS,
};
pub use audit_trail::recent_audit_events;
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
    candidate_timeline, create_conversation, create_message, list_conversations, list_messages,
    ListConversations, ListMessages,
};
pub use dashboard::{dashboard, DashboardSnapshot};
pub use drafts::{
    approve_draft, copy_draft, generate, get_draft, get_draft_group, list_drafts, preview_context,
    reject_draft, select_draft_from_group, update_draft,
};
pub use error::ApplicationError;
pub use extractions::confirm_candidate;
pub use follow_ups::{
    cancel_task, complete_task, create_task, ensure_default_policy, evaluate, get_task, list_tasks,
    record_result, update_policy, update_task,
};
pub use imports::{
    confirm_import, get_import, ocr_screenshot, preview_import, reject_import, OcrOutcome,
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
pub use request_context::{authorize, Permission, RequestContext, Scope};
pub use runtime::{AppRuntime, RuntimeConfig, RuntimeLifecycle, RuntimePhase};
pub use send_records::{list_send_records, record_send, ListSendRecords};
pub use settings::{
    read_egress_disclosure, write_egress_consent, EgressDisclosure, SettingsConfigView,
    SettingsSnapshot, AI_EGRESS_CONSENT_KEY, EGRESS_GRANTED_EVENT, EGRESS_NOTICE,
    EGRESS_REVOKED_EVENT, MASKED_CATEGORIES,
};
pub use sync::{export_data, import_data, ExportOutcome, ExportSnapshot, SealedSnapshot};
