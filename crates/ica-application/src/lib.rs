//! ICA application layer: use cases and repository ports.
//!
//! Dependency rule (enforced by this crate's manifest): the domain plus the
//! ``async-trait`` proc macro. No axum, no sqlx, no tokio, no hyper — the
//! use cases talk to a port trait that ``ica-infra`` implements.

pub mod candidates;
pub mod conversations;
pub mod error;
pub mod payload;
pub mod ports;
pub mod positions;
pub mod send_records;

pub use candidates::{CandidateDetail, ListCandidates};
pub use conversations::{ListConversations, ListMessages};
pub use error::ApplicationError;
pub use payload::{
    CandidateFilter, CandidatePatch, ConversationFilter, Field, MessageFilter, NewCandidate,
    NewConversation, NewMessage, NewPosition, NewSendRecord, PositionPatch, SendRecordFilter,
};
pub use ports::{
    CandidateRepository, ConversationRepository, MessageRepository, PositionFilter,
    PositionRepository, RepositoryError, SendRecordRepository,
};
pub use positions::{get_position, list_positions, ListPositions};
pub use send_records::ListSendRecords;
