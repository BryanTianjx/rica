//! The ICA wire contract, transport-neutral.
//!
//! One source of truth for everything an adapter must honour, whatever
//! transport carries it: request validation (`body`, `query`), response
//! shapes (`dto`) and the error vocabulary (`error`). `ica-api` binds these
//! to axum/HTTP; `ica-desktop` binds them to Tauri IPC. Neither adapter owns
//! contract logic — that is the whole point of this crate (Tauri-audit R0.1
//! §4.3, option C: "no dual-entry duplicate implementation").
//!
//! Dependency rule (enforced by `scripts/check.py`): this crate must not
//! depend on axum, sqlx, tokio or hyper, and must not depend on an adapter
//! crate. It MAY depend on `ica-application` — the validators construct the
//! application layer's already-validated payloads.

pub mod body;
pub mod dto;
pub mod error;
pub mod query;

pub use body::{
    follow_up_create, follow_up_evaluate, follow_up_patch, follow_up_policy_patch,
    follow_up_result, parse, RawFollowUpCreate, RawFollowUpEvaluate, RawFollowUpPatch,
    RawFollowUpPolicyPatch, RawFollowUpResult, RawMessageCreate, RawSendRecordCreate,
};
pub use dto::{
    CandidateDetailRead, CandidateRead, ConversationRead, DashboardRead, FollowUpDecisionRead,
    FollowUpPolicyRead, FollowUpTaskRead, HealthResponse, MessageRead, PageResponse, PositionRead,
    SendRecordRead, TimelineEntryRead,
};
pub use error::{ContractError, ValidationIssue};
pub use query::{
    typed_limit, typed_offset, validate, validate_candidates, validate_conversations,
    validate_follow_ups, validate_messages, validate_send_records, validate_timeline,
    RawFollowUpCancelQuery, RawListCandidatesQuery, RawListConversationsQuery,
    RawListFollowUpsQuery, RawListMessagesQuery, RawListPositionsQuery, RawListSendRecordsQuery,
    RawTimelineQuery, DEFAULT_LIMIT, DEFAULT_MESSAGE_LIMIT, MAX_LIMIT, MAX_MESSAGE_LIMIT,
};
