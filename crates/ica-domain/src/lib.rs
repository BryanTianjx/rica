//! ICA domain layer: entities, enumerations and pure pagination/search rules.
//!
//! This crate intentionally has no dependency on a web framework, a database
//! driver or an async runtime, so domain code cannot accidentally take on an
//! infrastructure concern: the layering rule is enforced by the compiler.

pub mod candidate;
pub mod content_hash;
pub mod conversation;
pub mod pagination;
pub mod position;
pub mod search;
pub mod send_record;

pub use candidate::{Candidate, CandidateStatus};
pub use content_hash::content_hash;
pub use conversation::{Conversation, Message, MessageSource, SenderType, TimelineEntry};
pub use pagination::Page;
pub use position::{Position, PositionStatus};
pub use search::{normalize_search, MAX_SEARCH_LEN};
pub use send_record::{ResultType, SendRecord};
