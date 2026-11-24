//! Validated write payloads and filters.
//!
//! These types are the application layer's input: every invariant the Python
//! pydantic schemas enforce (lengths, blank names, enum membership) has already
//! been checked by `ica-api/body.rs` by the time one of these is built. What
//! stays here is the *shape* of a patch, including the difference between an
//! omitted field and an explicit `null` — the distinction PATCH semantics and
//! IMP-2 depend on.

use chrono::{DateTime, Utc};
use ica_domain::{
    CandidateStatus, FollowUpTaskStatus, MessageSource, PositionStatus, ResultType, SenderType,
};

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
    /// SEC-04 remediation: when present, the SQLite adapter MUST restrict rows
    /// to this tenant and (for a non-admin actor) to rows they own. `None`
    /// preserves the documented single-user Local mode.
    pub scope: Option<crate::Scope>,
}

/// Filter for `GET /api/conversations`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConversationFilter {
    pub candidate_id: Option<String>,
    /// SEC-04: when present, the SQLite adapter restricts rows to this tenant
    /// (and, for a member, to rows they own). `None` is Local mode.
    pub scope: Option<crate::Scope>,
}

/// Filter for `GET /api/messages`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageFilter {
    pub candidate_id: Option<String>,
    pub conversation_id: Option<String>,
    /// SEC-04: repository-level tenant/owner predicate. `None` is Local mode.
    pub scope: Option<crate::Scope>,
}

/// Filter for `GET /api/send-records`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendRecordFilter {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub status: Option<ResultType>,
    /// SEC-04: repository-level tenant/owner predicate. `None` is Local mode.
    pub scope: Option<crate::Scope>,
}

// ── Follow-ups (W2) ────────────────────────────────────────────────────────

/// A validated `POST /api/follow-ups` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFollowUpTask {
    pub candidate_id: String,
    pub position_id: Option<String>,
    pub objective: Option<String>,
    pub reason: Option<String>,
    pub recommended_at: Option<DateTime<Utc>>,
}

/// A validated `PATCH /api/follow-ups/{id}` body. `status` carries the
/// three-state distinction too: absent and null both mean "no status change"
/// (the baseline checks `payload.status is not None` after pydantic).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FollowUpTaskPatch {
    pub objective: Field<String>,
    pub reason: Field<String>,
    pub recommended_at: Field<DateTime<Utc>>,
    pub status: Field<FollowUpTaskStatus>,
}

/// A validated `POST /api/follow-ups/{id}/record-result` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFollowUpResult {
    pub result_type: ResultType,
    pub note: Option<String>,
}

/// A validated `POST /api/follow-ups/evaluate` body. `policy_id` is accepted
/// but ignored, exactly like the baseline (the service never reads it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFollowUpEvaluation {
    pub candidate_id: String,
    pub position_id: Option<String>,
}

/// A validated `PATCH /api/follow-ups/policy/current` body. A `null` value and
/// an absent key behave identically (`exclude_unset` + `exclude_none`), so
/// both collapse into `None` — the field is simply left alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FollowUpPolicyPatch {
    pub first_follow_up_delay_hours: Option<i64>,
    pub max_active_follow_ups: Option<i64>,
    pub min_interval_hours: Option<i64>,
    pub task_validity_days: Option<i64>,
}

impl FollowUpPolicyPatch {
    /// Whether the body carried no field at all (the audit metadata is NULL
    /// for an empty patch, matching the baseline's empty-dict handling).
    pub fn is_empty(&self) -> bool {
        self.first_follow_up_delay_hours.is_none()
            && self.max_active_follow_ups.is_none()
            && self.min_interval_hours.is_none()
            && self.task_validity_days.is_none()
    }
}

/// Filter for `GET /api/follow-ups`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FollowUpTaskFilter {
    pub candidate_id: Option<String>,
    pub status: Option<FollowUpTaskStatus>,
}

/// The fields a result recording writes onto the task row. The repository
/// applies them together with the follow-up audit and the optional send
/// record, in one transaction like the baseline's `record_result`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpResultPatch {
    pub result_type: String,
    pub result_note: Option<String>,
    pub result_recorded_at: DateTime<Utc>,
    /// `Some(Completed)` when the current state may legally be closed by a
    /// recorded result; `None` keeps the status (e.g. on a terminal task).
    pub status: Option<FollowUpTaskStatus>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Whether this outcome increments `follow_up_count` (a contact-indicating
    /// result that was not already counted).
    pub bump_count: bool,
    /// `last_follow_up_at = now` for contact-indicating results.
    pub last_follow_up_at: Option<DateTime<Utc>>,
}

// ── Drafts (W3) ────────────────────────────────────────────────────────────

use ica_domain::{DraftStatus, MessageType, Tone};

/// A validated `POST /api/drafts/generate` body (the AI-port request).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDraftGeneration {
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub message_type: MessageType,
    pub tone: Tone,
    pub user_instruction: Option<String>,
    pub include_alternatives: bool,
    pub max_length: Option<i64>,
    /// The A/B comparison list; accepted and stored on the request, the
    /// per-model generation itself belongs to the provider (W5).
    pub comparison_models: Vec<String>,
    pub model_id: Option<String>,
}

/// A validated `PATCH /api/drafts/{id}` body. `exclude_unset + exclude_none`
/// means absent and null behave identically: both leave the field alone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DraftPatch {
    pub message_text: Option<String>,
    pub objective: Option<String>,
}

/// A validated `POST /api/draft-groups/{id}/select` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDraftGroupSelection {
    pub draft_id: String,
}

/// The `POST /api/drafts/{id}/copy` response body (`DraftCopyResponse`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftCopyOutcome {
    pub draft_id: String,
    pub message_text: String,
    pub copied_at: chrono::DateTime<chrono::Utc>,
}

/// Filter for `GET /api/drafts`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftFilter {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub status: Option<DraftStatus>,
}

// ── Import sessions (W4) ────────────────────────────────────────────────────

/// A validated `POST /api/imports/preview` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportPreview {
    pub candidate_id: Option<String>,
    pub raw_text: String,
    /// Defaults to `paste_import`, the baseline's default for this endpoint.
    pub source: ica_domain::MessageSource,
}

/// One validated per-record correction from `ImportConfirmRequest.records`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCorrection {
    pub id: String,
    pub sender_type: Option<ica_domain::SenderType>,
    pub content: Option<String>,
    pub message_time: Option<chrono::DateTime<chrono::Utc>>,
    pub skip: bool,
}

/// A validated `POST /api/imports/{id}/confirm` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportConfirm {
    /// Required at confirm time: an import must never be filed against the
    /// wrong candidate.
    pub candidate_id: String,
    pub conversation_id: Option<String>,
    pub corrections: Vec<ImportCorrection>,
}

/// A validated `POST /api/imports/ocr` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportOcr {
    pub candidate_id: Option<String>,
    /// A server-side path, guarded against escaping the staging directory.
    pub image_path: String,
}

/// The application-layer confirm request for a browser extraction draft
/// (`CandidateConfirmRequest`), already validated by `ica-contracts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCandidateConfirm {
    pub position_id: Option<String>,
    pub status: ica_domain::CandidateStatus,
    pub confirm_duplicate: bool,
    pub note: Option<String>,
}
