//! ICA application layer: use cases and repository ports.
//!
//! Dependency rule (enforced by this crate's manifest): the domain plus the
//! ``async-trait`` proc macro. No axum, no sqlx, no tokio, no hyper — the
//! use cases talk to a port trait that ``ica-infra`` implements.

pub mod ai_context;
pub mod candidates;
pub mod conversations;
pub mod dashboard;
pub mod drafts;
pub mod error;
pub mod follow_ups;
pub mod imports;
pub mod payload;
pub mod ports;
pub mod positions;
pub mod send_records;
pub mod sync;

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
pub use follow_ups::{
    cancel_task, complete_task, create_task, ensure_default_policy, evaluate, get_task, list_tasks,
    record_result, update_policy, update_task,
};
pub use payload::{
    CandidateFilter, CandidatePatch, ConversationFilter, DraftCopyOutcome, DraftFilter, DraftPatch,
    Field, FollowUpPolicyPatch, FollowUpResultPatch, FollowUpTaskFilter, FollowUpTaskPatch,
    ImportCorrection, MessageFilter, NewCandidate, NewConversation, NewDraftGeneration,
    NewDraftGroupSelection, NewFollowUpEvaluation, NewFollowUpResult, NewFollowUpTask,
    NewImportConfirm, NewImportOcr, NewImportPreview, NewMessage, NewPosition, NewSendRecord,
    PositionPatch, SendRecordFilter,
};
pub use ports::{
    CandidateRepository, ConversationRepository, DashboardRepository, DataSnapshotRepository,
    DraftGenerator, DraftRepository, ExportSealer, FollowUpPolicyRecord, FollowUpRepository,
    ImportConfirmOutcome, ImportRecordCorrection, ImportRepository, MessageRepository,
    NewFollowUpTaskRow, NewImportRecordRow, NewImportSessionRow, OcrEngine, PositionFilter,
    PositionRepository, RepositoryError, SendRecordRepository, SnapshotOutcome,
};
pub use positions::{
    create_position, delete_position, get_position, list_positions, update_position, ListPositions,
};
pub use send_records::{list_send_records, record_send, ListSendRecords};
