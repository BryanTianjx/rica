//! Response bodies, serialised exactly like the Python schemas.
//!
//! The field set and the timestamp rendering are the contract: a field added
//! here without the Python schema gaining it first is a wire-format break, and
//! a naive timestamp (no `Z`) makes every browser read the value as local time.

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use ica_application::FollowUpPolicyRecord;
use ica_domain::{
    Candidate, Conversation, FollowUpDecision, FollowUpTask, Message, Page, Position, SendRecord,
    TimelineEntry,
};
use serde::{Serialize, Serializer};

/// Render a UTC instant the way the Python baseline does.
///
/// `app/schemas/common.py::as_utc` attaches UTC to a naive value and pydantic
/// then serialises it as RFC 3339 with a `Z` suffix. Microseconds are printed
/// only when they are non-zero, and always as six digits, because that is what
/// pydantic emits for `...T12:00:00.500000Z`.
pub fn format_utc(value: &DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.to_rfc3339_opts(SecondsFormat::Secs, true)
    } else {
        value.to_rfc3339_opts(SecondsFormat::Micros, true)
    }
}

fn serialize_utc<S>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format_utc(value))
}

/// `null` stays `null`; a present instant is rendered like [`format_utc`].
fn serialize_utc_opt<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(instant) => serializer.serialize_some(&format_utc(instant)),
        None => serializer.serialize_none(),
    }
}

/// `GET /health` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub app: String,
    pub ai_enabled: bool,
}

/// One position, field-for-field identical to `PositionRead`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PositionRead {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub skill_requirements: Option<String>,
    pub experience_requirements: Option<String>,
    pub education_requirements: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub notes: Option<String>,
    pub status: &'static str,
    pub archived: bool,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_utc")]
    pub updated_at: DateTime<Utc>,
    pub candidate_count: i64,
}

impl From<Position> for PositionRead {
    fn from(position: Position) -> Self {
        Self {
            id: position.id,
            name: position.name,
            description: position.description,
            location: position.location,
            skill_requirements: position.skill_requirements,
            experience_requirements: position.experience_requirements,
            education_requirements: position.education_requirements,
            salary_min: position.salary_min,
            salary_max: position.salary_max,
            notes: position.notes,
            status: position.status.as_str(),
            archived: position.archived,
            created_at: position.created_at,
            updated_at: position.updated_at,
            candidate_count: position.candidate_count,
        }
    }
}

/// One candidate, field-for-field identical to `CandidateRead`.
///
/// `position_name`, `message_count` and `active_follow_up_count` are derived at
/// read time; the wire shape is the Python schema's, not the table's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateRead {
    pub id: String,
    pub name: String,
    pub source: String,
    pub position_id: Option<String>,
    pub experience_summary: Option<String>,
    pub skill_summary: Option<String>,
    pub expected_salary: Option<String>,
    pub expected_location: Option<String>,
    pub job_intention: Option<String>,
    pub notes: Option<String>,
    pub status: &'static str,
    pub archived: bool,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_utc")]
    pub updated_at: DateTime<Utc>,
    pub position_name: Option<String>,
    pub message_count: i64,
    pub active_follow_up_count: i64,
}

impl From<Candidate> for CandidateRead {
    fn from(candidate: Candidate) -> Self {
        Self {
            id: candidate.id,
            name: candidate.name,
            source: candidate.source,
            position_id: candidate.position_id,
            experience_summary: candidate.experience_summary,
            skill_summary: candidate.skill_summary,
            expected_salary: candidate.expected_salary,
            expected_location: candidate.expected_location,
            job_intention: candidate.job_intention,
            notes: candidate.notes,
            status: candidate.status.as_str(),
            archived: candidate.archived,
            created_at: candidate.created_at,
            updated_at: candidate.updated_at,
            position_name: candidate.position_name,
            message_count: candidate.message_count,
            active_follow_up_count: candidate.active_follow_up_count,
        }
    }
}

/// `CandidateDetail`: the candidate plus its position object.
///
/// The position is mapped through `PositionRead`, so its `candidate_count` is
/// the real count (mapping the ORM object directly is what left it at 0 in the
/// baseline's first draft).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateDetailRead {
    #[serde(flatten)]
    pub candidate: CandidateRead,
    pub position: Option<PositionRead>,
}

/// One conversation (`ConversationRead`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConversationRead {
    pub id: String,
    pub candidate_id: String,
    pub channel: String,
    pub title: Option<String>,
    pub is_active: bool,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub last_message_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_utc")]
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
    pub position_id: Option<String>,
    pub position_name: Option<String>,
}

impl From<Conversation> for ConversationRead {
    fn from(conversation: Conversation) -> Self {
        Self {
            id: conversation.id,
            candidate_id: conversation.candidate_id,
            channel: conversation.channel,
            title: conversation.title,
            is_active: conversation.is_active,
            last_message_at: conversation.last_message_at,
            created_at: conversation.created_at,
            updated_at: conversation.updated_at,
            message_count: conversation.message_count,
            position_id: conversation.position_id,
            position_name: conversation.position_name,
        }
    }
}

/// One message (`MessageRead`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MessageRead {
    pub id: String,
    pub candidate_id: String,
    pub conversation_id: String,
    pub sender_type: Option<String>,
    pub content: String,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub message_time: Option<DateTime<Utc>>,
    pub channel: String,
    pub source: String,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
}

impl From<Message> for MessageRead {
    fn from(message: Message) -> Self {
        Self {
            id: message.id,
            candidate_id: message.candidate_id,
            conversation_id: message.conversation_id,
            sender_type: message.sender_type,
            content: message.content,
            message_time: message.message_time,
            channel: message.channel,
            source: message.source,
            created_at: message.created_at,
        }
    }
}

/// One timeline row (`TimelineEntry`): a message tagged with its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimelineEntryRead {
    pub id: String,
    pub kind: &'static str,
    pub sender_type: Option<String>,
    pub content: String,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub message_time: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub status: Option<String>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub created_at: Option<DateTime<Utc>>,
}

impl From<TimelineEntry> for TimelineEntryRead {
    fn from(entry: TimelineEntry) -> Self {
        Self {
            id: entry.id,
            kind: entry.kind,
            sender_type: entry.sender_type,
            content: entry.content,
            message_time: entry.message_time,
            source: entry.source,
            status: entry.status,
            created_at: entry.created_at,
        }
    }
}

/// One manual send record (`SendRecordRead`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SendRecordRead {
    pub id: String,
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub channel: String,
    pub target: Option<String>,
    pub content_summary: Option<String>,
    pub status: String,
    #[serde(serialize_with = "serialize_utc")]
    pub sent_at: DateTime<Utc>,
    pub note: Option<String>,
    pub actor: String,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
}

impl From<SendRecord> for SendRecordRead {
    fn from(record: SendRecord) -> Self {
        Self {
            id: record.id,
            candidate_id: record.candidate_id,
            task_id: record.task_id,
            draft_id: record.draft_id,
            channel: record.channel,
            target: record.target,
            content_summary: record.content_summary,
            status: record.status,
            sent_at: record.sent_at,
            note: record.note,
            actor: record.actor,
            created_at: record.created_at,
        }
    }
}

/// One follow-up task (`FollowUpTaskRead`), field-for-field identical to the
/// Python schema — including the mapper-added `candidate_name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FollowUpTaskRead {
    pub id: String,
    pub candidate_id: String,
    pub position_id: Option<String>,
    pub status: &'static str,
    pub objective: Option<String>,
    pub reason: Option<String>,
    pub block_reason: Option<String>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub recommended_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub last_follow_up_at: Option<DateTime<Utc>>,
    pub follow_up_count: i64,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub completed_at: Option<DateTime<Utc>>,
    pub result_type: Option<String>,
    pub result_note: Option<String>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub result_recorded_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_utc")]
    pub updated_at: DateTime<Utc>,
    pub candidate_name: Option<String>,
}

impl From<FollowUpTask> for FollowUpTaskRead {
    fn from(task: FollowUpTask) -> Self {
        Self {
            id: task.id,
            candidate_id: task.candidate_id,
            position_id: task.position_id,
            status: task.status.as_str(),
            objective: task.objective,
            reason: task.reason,
            block_reason: task.block_reason,
            recommended_at: task.recommended_at,
            expires_at: task.expires_at,
            last_follow_up_at: task.last_follow_up_at,
            follow_up_count: task.follow_up_count,
            completed_at: task.completed_at,
            result_type: task.result_type,
            result_note: task.result_note,
            result_recorded_at: task.result_recorded_at,
            created_at: task.created_at,
            updated_at: task.updated_at,
            candidate_name: task.candidate_name,
        }
    }
}

/// The decision engine's answer (`FollowUpDecisionRead`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FollowUpDecisionRead {
    pub outcome: String,
    pub eligible: bool,
    pub reasons: Vec<String>,
    pub block_reasons: Vec<String>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub recommended_at: Option<DateTime<Utc>>,
    pub suggested_objective: Option<String>,
    pub risks: Vec<String>,
}

impl From<FollowUpDecision> for FollowUpDecisionRead {
    fn from(decision: FollowUpDecision) -> Self {
        Self {
            outcome: decision.outcome.as_str().to_string(),
            eligible: decision.eligible(),
            reasons: decision.reasons,
            block_reasons: decision.block_reasons,
            recommended_at: decision.recommended_at,
            suggested_objective: decision.suggested_objective,
            risks: decision.risks,
        }
    }
}

/// The active follow-up policy (`FollowUpPolicyRead`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FollowUpPolicyRead {
    pub id: String,
    pub name: String,
    pub first_follow_up_delay_hours: i64,
    pub max_active_follow_ups: i64,
    pub min_interval_hours: i64,
    pub task_validity_days: i64,
    pub is_active: bool,
}

impl From<FollowUpPolicyRecord> for FollowUpPolicyRead {
    fn from(record: FollowUpPolicyRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            first_follow_up_delay_hours: record.policy.first_follow_up_delay_hours,
            max_active_follow_ups: record.policy.max_active_follow_ups,
            min_interval_hours: record.policy.min_interval_hours,
            task_validity_days: record.policy.task_validity_days,
            is_active: record.is_active,
        }
    }
}

/// The aggregated workbench view (`DashboardRead`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DashboardRead {
    pub candidate_total: i64,
    pub candidate_status_counts: BTreeMap<String, i64>,
    pub active_follow_up_count: i64,
    pub pending_follow_ups: Vec<FollowUpTaskRead>,
    pub awaiting_result_count: i64,
    pub pending_draft_count: i64,
    pub recent_messages: Vec<MessageRead>,
    pub position_total: i64,
    pub open_position_count: i64,
}

/// The `Page<T>` envelope: rows plus the metadata the UI pages with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

impl<T> PageResponse<T> {
    /// Wrap a domain page, mapping its items to the wire shape.
    ///
    /// `has_more` comes from [`Page::has_more`], the single implementation of
    /// that arithmetic: counting the rows actually returned, not `limit`, so a
    /// short last page reports `false` — the same result as the Python
    /// `Page.has_more` computed field. Recomputing it here would be a second
    /// truth to keep in sync (R0 audit finding F-07).
    pub fn from_page<U, F: FnMut(U) -> T>(page: Page<U>, map: F) -> Self {
        let has_more = page.has_more();
        let Page {
            items,
            total,
            offset,
            limit,
        } = page;
        Self {
            items: items.into_iter().map(map).collect(),
            total,
            offset,
            limit,
            has_more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn renders_a_whole_second_without_a_fraction() {
        let value = Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap();
        assert_eq!(format_utc(&value), "2026-11-10T09:00:00Z");
    }

    #[test]
    fn renders_microseconds_with_six_digits() {
        let value = Utc.with_ymd_and_hms(2026, 11, 13, 12, 0, 0).unwrap()
            + chrono::Duration::microseconds(500_000);
        assert_eq!(format_utc(&value), "2026-11-13T12:00:00.500000Z");
    }

    #[test]
    fn a_short_page_reports_no_more_rows() {
        let page: PageResponse<u8> =
            PageResponse::from_page(Page::from_items(vec![1, 2], 0, 50), |value| value);
        assert!(!page.has_more);
    }

    #[test]
    fn a_full_page_reports_more_rows() {
        // A full window of a *larger* match set: `from_items` cannot express
        // this (its `total` is the slice length), so build the `Page` directly.
        let page: PageResponse<u8> = PageResponse::from_page(
            Page {
                items: vec![1, 2],
                total: 4,
                offset: 0,
                limit: 2,
            },
            |value| value,
        );
        assert!(page.has_more);
    }

    #[test]
    fn the_last_partial_page_reports_no_more_rows() {
        // offset + returned < total would be true if `limit` were used here,
        // but the page itself is exhausted.
        let page: PageResponse<u8> =
            PageResponse::from_page(Page::from_items(vec![1, 2, 3], 2, 2), |value| value);
        assert!(!page.has_more);
    }
}
