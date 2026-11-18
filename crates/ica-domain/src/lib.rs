//! ICA domain layer: entities, enumerations and pure pagination/search rules.
//!
//! This crate intentionally has no dependency on a web framework, a database
//! driver or an async runtime, so domain code cannot accidentally take on an
//! infrastructure concern: the layering rule is enforced by the compiler.

pub mod candidate;
pub mod content_hash;
pub mod conversation;
pub mod draft;
pub mod follow_up;
pub mod import;
pub mod import_parser;
pub mod pagination;
pub mod position;
pub mod redact;
pub mod search;
pub mod send_record;

pub use candidate::{Candidate, CandidateStatus};
pub use content_hash::content_hash;
pub use conversation::{Conversation, Message, MessageSource, SenderType, TimelineEntry};
pub use draft::{DraftStatus, GeneratedDraft, MessageType, Tone, ValidationStatus};
pub use follow_up::{
    can_transition, evaluate_follow_up, find_transition_path, is_task_expired, task_expiry,
    DecisionOutcome, EvaluateInput, FollowUpDecision, FollowUpPolicy, FollowUpTask,
    FollowUpTaskStatus,
};
pub use import::{ImportRecord, ImportSession, ImportStatus, RecordStatus};
pub use import_parser::{
    find_duplicates, normalize_text, parse_messages, split_message_blocks, ParseResult,
    ParsedMessage,
};
pub use redact::redact_for_ai;

pub use pagination::Page;
pub use position::{Position, PositionStatus};
pub use search::{normalize_search, MAX_SEARCH_LEN};
pub use send_record::{ResultType, SendRecord};
