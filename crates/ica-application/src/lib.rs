//! ICA application layer: use cases and repository ports.
//!
//! Dependency rule (enforced by this crate's manifest): the domain plus the
//! ``async-trait`` proc macro. No axum, no sqlx, no tokio, no hyper — the
//! use cases talk to a port trait that ``ica-infra`` implements.

pub mod candidates;
pub mod conversations;
pub mod dashboard;
pub mod error;
pub mod follow_ups;
pub mod payload;
pub mod ports;
pub mod positions;
pub mod send_records;

pub use candidates::{
    create_candidate, delete_candidate, get_candidate, get_candidate_detail, list_candidates,
    update_candidate, CandidateDetail, ListCandidates,
};
pub use conversations::{
    candidate_timeline, create_conversation, create_message, list_conversations, list_messages,
    ListConversations, ListMessages,
};
pub use dashboard::{dashboard, DashboardSnapshot};
pub use error::ApplicationError;
pub use follow_ups::{
    cancel_task, complete_task, create_task, ensure_default_policy, evaluate, get_task, list_tasks,
    record_result, update_policy, update_task,
};
pub use payload::{
    CandidateFilter, CandidatePatch, ConversationFilter, Field, FollowUpPolicyPatch,
    FollowUpResultPatch, FollowUpTaskFilter, FollowUpTaskPatch, MessageFilter, NewCandidate,
    NewConversation, NewFollowUpEvaluation, NewFollowUpResult, NewFollowUpTask, NewMessage,
    NewPosition, NewSendRecord, PositionPatch, SendRecordFilter,
};
pub use ports::{
    CandidateRepository, ConversationRepository, DashboardRepository, FollowUpPolicyRecord,
    FollowUpRepository, MessageRepository, NewFollowUpTaskRow, PositionFilter, PositionRepository,
    RepositoryError, SendRecordRepository,
};
pub use positions::{
    create_position, delete_position, get_position, list_positions, update_position, ListPositions,
};
pub use send_records::{list_send_records, record_send, ListSendRecords};
