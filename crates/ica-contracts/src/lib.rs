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

pub mod auth_session;
pub mod body;
pub mod dto;
pub mod error;
pub mod events;
pub mod proxy_headers;
pub mod query;
pub mod sidecar_protocol;

pub use auth_session::{
    auth_transition, AuthEvent, AuthPhase, AuthSessionState, SessionEndReason, ALL_AUTH_PHASES,
    AUTH_PHASE_NAMES,
};
pub use body::{
    candidate_confirm, draft_approve, draft_group_select, draft_patch, follow_up_create,
    follow_up_evaluate, follow_up_patch, follow_up_policy_patch, follow_up_result,
    generate_request, import_confirm, import_ocr, import_payload, import_preview, parse,
    RawCandidateConfirm, RawDraftApprove, RawDraftGroupSelect, RawDraftPatch, RawFollowUpCreate,
    RawFollowUpEvaluate, RawFollowUpPatch, RawFollowUpPolicyPatch, RawFollowUpResult,
    RawGenerateRequest, RawImportConfirm, RawImportOcr, RawImportPayload, RawImportPreview,
    RawMessageCreate, RawSendRecordCreate,
};
pub use dto::{
    CandidateDetailRead, CandidateRead, ContextPreviewRead, ConversationRead, DashboardRead,
    DraftCopyResponse, DraftGroupRead, DraftRead, EncryptedExportResponse, ExportResponse,
    FollowUpDecisionRead, FollowUpPolicyRead, FollowUpTaskRead, HealthResponse,
    ImportConfirmResult, ImportOcrResult, ImportRecordRead, ImportResult, ImportSessionRead,
    MessageRead, PageResponse, PositionRead, SendRecordRead, TimelineEntryRead,
};
pub use error::{code, sidecar, ContractError, ErrorEnvelope, ValidationIssue};
pub use query::{
    typed_limit, typed_offset, validate, validate_candidates, validate_conversations,
    validate_drafts, validate_follow_ups, validate_messages, validate_send_records,
    validate_timeline, ListDrafts, RawFollowUpCancelQuery, RawListCandidatesQuery,
    RawListConversationsQuery, RawListDraftsQuery, RawListFollowUpsQuery, RawListMessagesQuery,
    RawListPositionsQuery, RawListSendRecordsQuery, RawTimelineQuery, DEFAULT_LIMIT,
    DEFAULT_MESSAGE_LIMIT, MAX_LIMIT, MAX_MESSAGE_LIMIT,
};
pub use sidecar_protocol::{
    decode_request, decode_response, encode_request, encode_response, negotiate_version,
    new_request_id, redact, validate_request, validate_response, CapabilityGrant, PathAccess,
    PathGrant, SidecarError, SidecarProtocolError, SidecarRequest, SidecarResponse,
    PROTOCOL_VERSION, SUPPORTED_VERSIONS,
};
