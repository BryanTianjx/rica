//! Validated write payloads and filters.
//!
//! These types are the application layer's input: every invariant the Python
//! pydantic schemas enforce (lengths, blank names, enum membership) has already
//! been checked by `ica-api/body.rs` by the time one of these is built. What
//! stays here is the *shape* of a patch, including the difference between an
//! omitted field and an explicit `null` — the distinction PATCH semantics and
//! IMP-2 depend on.

use chrono::{DateTime, Utc};
use ica_domain::{CandidateStatus, MessageSource, PositionStatus, ResultType, SenderType};

/// A field in a PATCH body: the three states pydantic's `model_dump(exclude_unset=True)`
/// can distinguish.
///
/// * `Absent` — the key was not in the body: leave the column alone.
/// * `Null` — the key was present with `null`: write NULL (or reject, for a
///   NOT NULL column; that rejection happens in the API layer so the request
///   fails with 422 before any write).
/// * `Value` — the key was present with a value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Field<T> {
    #[default]
    Absent,
    Null,
    Value(T),
}

impl<T> Field<T> {
    /// True when the key was absent from the body.
    pub fn is_absent(&self) -> bool {
        matches!(self, Field::Absent)
    }

    /// Borrow the value when the field carries one.
    pub fn value(&self) -> Option<&T> {
        match self {
            Field::Value(value) => Some(value),
            Field::Absent | Field::Null => None,
        }
    }
}

// ── Positions ──────────────────────────────────────────────────────────────

/// A validated `POST /api/positions` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPosition {
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub skill_requirements: Option<String>,
    pub experience_requirements: Option<String>,
    pub education_requirements: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub notes: Option<String>,
    pub status: PositionStatus,
}

/// A validated `PATCH /api/positions/{id}` body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PositionPatch {
    pub name: Field<String>,
    pub description: Field<String>,
    pub location: Field<String>,
    pub skill_requirements: Field<String>,
    pub experience_requirements: Field<String>,
    pub education_requirements: Field<String>,
    pub salary_min: Field<i64>,
    pub salary_max: Field<i64>,
    pub status: Field<PositionStatus>,
    pub archived: Field<bool>,
    pub notes: Field<String>,
}

// ── Candidates ─────────────────────────────────────────────────────────────

/// A validated `POST /api/candidates` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCandidate {
    pub name: String,
    pub source: String,
    pub position_id: Option<String>,
    pub experience_summary: Option<String>,
    pub skill_summary: Option<String>,
    pub expected_salary: Option<String>,
    pub expected_location: Option<String>,
    pub job_intention: Option<String>,
    pub notes: Option<String>,
    pub status: CandidateStatus,
}

/// A validated `PATCH /api/candidates/{id}` body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CandidatePatch {
    pub name: Field<String>,
    pub source: Field<String>,
    pub position_id: Field<String>,
    pub experience_summary: Field<String>,
    pub skill_summary: Field<String>,
    pub expected_salary: Field<String>,
    pub expected_location: Field<String>,
    pub job_intention: Field<String>,
    pub status: Field<CandidateStatus>,
    pub archived: Field<bool>,
    pub notes: Field<String>,
}

// ── Conversations and messages ─────────────────────────────────────────────

/// A validated `POST /api/conversations` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    pub candidate_id: String,
    pub channel: String,
    pub title: Option<String>,
}

/// A validated `POST /api/messages` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    pub candidate_id: String,
    pub conversation_id: Option<String>,
    pub sender_type: Option<SenderType>,
    pub content: String,
    pub message_time: Option<DateTime<Utc>>,
    pub channel: String,
    pub source: MessageSource,
}

// ── Send records ───────────────────────────────────────────────────────────

/// A validated `POST /api/send-records` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSendRecord {
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub channel: String,
    pub target: Option<String>,
    pub content_summary: Option<String>,
    pub status: ResultType,
    pub sent_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
}

// ── Filters ────────────────────────────────────────────────────────────────

/// Filter for `GET /api/candidates`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateFilter {
    pub include_archived: bool,
    /// Already normalised by the domain (`normalize_search`).
    pub search: Option<String>,
    pub status: Option<CandidateStatus>,
    pub position_id: Option<String>,
}

/// Filter for `GET /api/conversations`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConversationFilter {
    pub candidate_id: Option<String>,
}

/// Filter for `GET /api/messages`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageFilter {
    pub candidate_id: Option<String>,
    pub conversation_id: Option<String>,
}

/// Filter for `GET /api/send-records`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendRecordFilter {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub status: Option<ResultType>,
}
