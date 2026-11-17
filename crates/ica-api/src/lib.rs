//! ICA HTTP layer: axum routes, DTOs and error mapping.
//!
//! This crate is the only place that knows about HTTP. It translates a request
//! into an application use case and the result into the exact JSON the Python
//! baseline emits, so an existing frontend cannot tell the two apart.

pub mod body;
pub mod dto;
pub mod error;
pub mod http_parity;
pub mod query;
pub mod routes;
pub mod state;

pub use dto::{
    CandidateDetailRead, CandidateRead, ConversationRead, HealthResponse, MessageRead,
    PageResponse, PositionRead, SendRecordRead, TimelineEntryRead,
};
pub use error::{ApiError, ValidationIssue};
pub use query::{
    RawListCandidatesQuery, RawListConversationsQuery, RawListMessagesQuery, RawListPositionsQuery,
    RawListSendRecordsQuery, RawTimelineQuery, DEFAULT_LIMIT, DEFAULT_MESSAGE_LIMIT, MAX_LIMIT,
    MAX_MESSAGE_LIMIT,
};
pub use routes::router;
pub use state::{AppConfig, AppState};
