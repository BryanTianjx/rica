//! Response bodies, serialised exactly like the Python schemas.
//!
//! The field set and the timestamp rendering are the contract: a field added
//! here without the Python schema gaining it first is a wire-format break, and
//! a naive timestamp (no `Z`) makes every browser read the value as local time.

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use ica_application::ai_context::AiContext;
use ica_application::payload::DraftCopyOutcome;
use ica_application::FollowUpPolicyRecord;
use ica_domain::{
    Candidate, Conversation, FollowUpDecision, FollowUpTask, GeneratedDraft, Message, Page,
    Position, SendRecord, TimelineEntry,
};
use serde::{Serialize, Serializer};
use serde_json::Value;

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

/// Python's *naive* `datetime.isoformat()` — no `Z`, no offset.
///
/// `DraftGroupRead` is a plain `BaseModel`, not a `UtcModel`, so its
/// `created_at` never passes through `as_utc` and comes out as
/// `2026-11-10T09:00:04`. A port that "helpfully" adds the `Z` here is a
/// value difference, and the contract diff catches it.
fn format_naive(value: &DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        value.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    }
}

/// `null` stays `null`; a present instant is rendered like [`format_naive`].
fn serialize_naive_opt<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(instant) => serializer.serialize_some(&format_naive(instant)),
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
    /// The workbench calendar (BROWSER-05 increment, frozen baseline):
    /// per-day dynamics for the current UTC month.
    pub calendar: DashboardCalendar,
}

/// `DashboardCalendar`: the UTC month prefix plus the two per-day maps the
/// frontend calendar renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DashboardCalendar {
    pub month: String,
    pub messages_by_day: std::collections::BTreeMap<String, i64>,
    pub followups_by_day: std::collections::BTreeMap<String, i64>,
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

// ── Drafts (W3) ────────────────────────────────────────────────────────────

fn expand_json_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// Python's `load_json_dict`: read a JSON object from a text column, returning
/// `None` for NULL/empty, malformed JSON, or any non-object value — a broken
/// blob must never break the row's rendering.
fn load_json_object(raw: Option<&str>) -> Option<serde_json::Map<String, Value>> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// One generated draft (`DraftRead`), field-for-field the Python schema
/// including the V2 cost/routing fields; the JSON-backed list columns are
/// expanded, and `routing_tier`/`routing_reason` are projected out of the
/// `routing_decision_json` blob exactly like `draft_to_read` does.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftRead {
    pub id: String,
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub message_type: String,
    pub tone: String,
    pub status: &'static str,
    pub objective: Option<String>,
    pub message_text: String,
    pub alternatives: Vec<String>,
    pub context_references: Vec<String>,
    pub uncertainties: Vec<String>,
    pub warnings: Vec<String>,
    pub validation_status: String,
    pub validation_messages: Vec<String>,
    pub quality_status: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub is_edited: bool,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub copied_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub rejected_at: Option<DateTime<Utc>>,
    pub draft_group_id: Option<String>,
    pub cost_cents: Option<i64>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub routing_tier: Option<String>,
    pub routing_reason: Option<String>,
    /// The raw routing blob, echoed verbatim like Python's `DraftRead`
    /// (the ORM model carries it as a column, so `model_validate` keeps it).
    pub routing_decision_json: Option<String>,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_utc")]
    pub updated_at: DateTime<Utc>,
}

impl From<GeneratedDraft> for DraftRead {
    fn from(draft: GeneratedDraft) -> Self {
        // Python: `routing = load_json_dict(draft.routing_decision_json)` then
        // `routing.get("actual_tier") or routing.get("requested_tier")` and
        // `routing.get("reason")`. A NULL/empty blob, malformed JSON, or a
        // JSON non-object all degrade to "no routing" (never an error).
        let routing = load_json_object(draft.routing_decision_json.as_deref());
        let routing_tier = routing
            .as_ref()
            .and_then(|routing| {
                routing
                    .get("actual_tier")
                    .filter(|value| !value.is_null())
                    .or_else(|| {
                        routing
                            .get("requested_tier")
                            .filter(|value| !value.is_null())
                    })
            })
            .and_then(Value::as_str)
            .map(str::to_string);
        let routing_reason = routing
            .as_ref()
            .and_then(|routing| routing.get("reason"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Self {
            id: draft.id,
            candidate_id: draft.candidate_id,
            task_id: draft.task_id,
            message_type: draft.message_type,
            tone: draft.tone,
            status: draft.status.as_str(),
            objective: draft.objective,
            message_text: draft.message_text,
            alternatives: expand_json_list(&draft.alternatives_json),
            context_references: expand_json_list(&draft.context_references_json),
            uncertainties: expand_json_list(&draft.uncertainties_json),
            warnings: expand_json_list(&draft.warnings_json),
            validation_status: draft.validation_status,
            validation_messages: expand_json_list(&draft.validation_messages_json),
            quality_status: draft.quality_status,
            provider: draft.provider,
            model: draft.model,
            prompt_version: draft.prompt_version,
            is_edited: draft.is_edited,
            approved_at: draft.approved_at,
            copied_at: draft.copied_at,
            rejected_at: draft.rejected_at,
            draft_group_id: draft.draft_group_id,
            cost_cents: draft.cost_cents,
            prompt_tokens: draft.prompt_tokens,
            completion_tokens: draft.completion_tokens,
            routing_tier,
            routing_reason,
            routing_decision_json: draft.routing_decision_json,
            created_at: draft.created_at,
            updated_at: draft.updated_at,
        }
    }
}

/// One comparison group (`DraftGroupRead`): the shared id, the group's latest
/// `created_at`, and the drafts sorted `created_at ASC`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftGroupRead {
    pub draft_group_id: String,
    /// Naive on purpose — Python's `DraftGroupRead` is a plain `BaseModel`, so
    /// this field keeps `2026-11-10T09:00:04` (the `DraftRead`s inside do get
    /// the `Z`, because they inherit `UtcModel`).
    #[serde(serialize_with = "serialize_naive_opt")]
    pub created_at: Option<DateTime<Utc>>,
    pub drafts: Vec<DraftRead>,
}

/// The transparency surface (`ContextPreviewRead`): exactly what the model
/// would see for this candidate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextPreviewRead {
    pub candidate_id: String,
    pub candidate_summary: Value,
    pub position_summary: Option<Value>,
    pub message_count: usize,
    /// The newest 10 of the oldest-first context messages.
    pub recent_messages: Vec<Value>,
    pub notes: Vec<String>,
    pub unknowns: Vec<String>,
    pub conflicts: Vec<String>,
    pub redaction_applied: Vec<String>,
}

impl From<AiContext> for ContextPreviewRead {
    fn from(context: AiContext) -> Self {
        let message_count = context.messages.len();
        let recent_start = message_count.saturating_sub(10);
        Self {
            candidate_id: context.candidate_id,
            candidate_summary: context.candidate_summary,
            position_summary: context.position_summary,
            message_count,
            recent_messages: context.messages[recent_start..].to_vec(),
            notes: context.notes,
            unknowns: context.unknowns,
            conflicts: context.conflicts,
            redaction_applied: context.redaction_applied,
        }
    }
}

/// The `POST /api/drafts/{id}/copy` response (`DraftCopyResponse`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftCopyResponse {
    pub draft_id: String,
    pub message_text: String,
    #[serde(serialize_with = "serialize_utc")]
    pub copied_at: DateTime<Utc>,
    pub note: &'static str,
}

impl From<DraftCopyOutcome> for DraftCopyResponse {
    fn from(outcome: DraftCopyOutcome) -> Self {
        Self {
            draft_id: outcome.draft_id,
            message_text: outcome.message_text,
            copied_at: outcome.copied_at,
            note: "Copied to clipboard. This does not mean the message was sent.",
        }
    }
}

// ── Import sessions (W4) ────────────────────────────────────────────────────

/// One parsed record inside an import session (`ImportRecordRead`).
///
/// `status` is the parser's per-record verdict (`duplicate` / `valid` /
/// `invalid` / …); it is projected to `&'static str` like every other domain
/// enum in this file, because `ica-domain` carries no serde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportRecordRead {
    pub id: String,
    pub session_id: String,
    pub sequence: i64,
    pub sender_type: Option<String>,
    pub content: String,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub message_time: Option<DateTime<Utc>>,
    pub status: &'static str,
    pub duplicate_of_message_id: Option<String>,
    pub content_hash: Option<String>,
    pub validation_messages: Option<String>,
}

impl From<ica_domain::ImportRecord> for ImportRecordRead {
    fn from(record: ica_domain::ImportRecord) -> Self {
        Self {
            id: record.id,
            session_id: record.session_id,
            sequence: record.sequence,
            sender_type: record.sender_type,
            content: record.content,
            message_time: record.message_time,
            status: record.status.as_str(),
            duplicate_of_message_id: record.duplicate_of_message_id,
            content_hash: record.content_hash,
            validation_messages: record.validation_messages,
        }
    }
}

/// One import session with its parsed records (`ImportSessionRead`).
///
/// Nine fields, no `total` and no pagination: the baseline returns every
/// record of the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportSessionRead {
    pub id: String,
    pub candidate_id: Option<String>,
    pub source: String,
    pub status: &'static str,
    pub detected_duplicate_count: i64,
    #[serde(serialize_with = "serialize_utc_opt")]
    pub confirmed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
    pub records: Vec<ImportRecordRead>,
}

impl From<ica_domain::ImportSession> for ImportSessionRead {
    fn from(session: ica_domain::ImportSession) -> Self {
        Self {
            id: session.id,
            candidate_id: session.candidate_id,
            source: session.source,
            status: session.status.as_str(),
            detected_duplicate_count: session.detected_duplicate_count,
            confirmed_at: session.confirmed_at,
            error_message: session.error_message,
            created_at: session.created_at,
            records: session.records.into_iter().map(Into::into).collect(),
        }
    }
}

/// Result of confirming an import session (`ImportConfirmResult`).
///
/// `duplicate_count` counts records the parser had already flagged as
/// duplicates; those are also counted in `skipped_count`, exactly as the
/// baseline does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportConfirmResult {
    pub session_id: String,
    pub saved_message_ids: Vec<String>,
    pub skipped_count: i64,
    pub duplicate_count: i64,
}

/// Result of a screenshot OCR call (`ImportOcrResult`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportOcrResult {
    pub session_id: String,
    pub text: String,
    pub language: String,
    pub candidate_id: Option<String>,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
}

// ── Data export / import (W4) ───────────────────────────────────────────────

/// A plaintext export snapshot (`ExportResponse`).
///
/// `exported_at` is a **string** in the wire format, not a datetime: the
/// baseline builds it with `datetime.isoformat()` and puts it in a `str` field,
/// so it carries `+00:00` rather than the `Z` that `UtcModel` would produce.
/// `version` is the literal `"0.2.0"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportResponse {
    pub version: String,
    pub exported_at: String,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// The opaque envelope an encrypted export returns (`EncryptedExportResponse`).
///
/// `ciphertext` seals the UTF-8 JSON of the plaintext snapshot, `iv` is a fresh
/// 96-bit GCM nonce and `tag` the 128-bit authentication tag; all three are
/// standard base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EncryptedExportResponse {
    pub alg: String,
    pub iv: String,
    pub tag: String,
    pub ciphertext: String,
}

/// Outcome of a JSON snapshot import (`ImportResult`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportResult {
    pub imported: i64,
    pub updated: i64,
    pub skipped: i64,
    pub conflicts: i64,
}

// ── Audit events (W5) ──────────────────────────────────────────────────────

/// One audit trail entry (`AuditEventRead`), newest-first list element.
#[derive(Debug, Serialize)]
pub struct AuditEventRead {
    pub id: String,
    pub event_type: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub actor: String,
    pub summary: Option<String>,
    /// Parsed from the stored JSON text by `load_json_dict`: a missing or
    /// unparseable blob degrades to `{}`, never hides the event.
    pub metadata: Value,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: DateTime<Utc>,
}

impl From<ica_domain::AuditEvent> for AuditEventRead {
    fn from(event: ica_domain::AuditEvent) -> Self {
        let metadata = event
            .metadata_json
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .filter(|value: &Value| value.is_object())
            .unwrap_or_else(|| Value::Object(Default::default()));
        Self {
            id: event.id,
            event_type: event.event_type,
            entity_type: event.entity_type,
            entity_id: event.entity_id,
            actor: event.actor,
            summary: event.summary,
            metadata,
            created_at: event.created_at,
        }
    }
}

// ── Settings (W5) ──────────────────────────────────────────────────────────

/// `SettingsRead` — the effective configuration, minus any secret
/// (the baseline's `read_settings`).
#[derive(Debug, Serialize)]
pub struct SettingsRead {
    pub app_name: String,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub database_url: String,
    pub backup_dir: String,
    pub backup_keep_count: i64,
    pub data_retention_days: i64,
}

/// `AIEgressRead` — where AI context would go and what is masked before it
/// leaves (`PRIV-07`). Never carries a key.
#[derive(Debug, Serialize)]
pub struct AIEgressRead {
    pub enabled: bool,
    pub consent_granted: bool,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub masked_fields: Vec<String>,
    pub include_candidate_name: bool,
    pub notice: String,
}
impl From<ica_application::EgressDisclosure> for AIEgressRead {
    fn from(disclosure: ica_application::EgressDisclosure) -> Self {
        Self {
            enabled: disclosure.enabled,
            consent_granted: disclosure.consent_granted,
            provider: disclosure.provider,
            model: disclosure.model,
            base_url: disclosure.base_url,
            masked_fields: disclosure.masked_fields,
            include_candidate_name: disclosure.include_candidate_name,
            notice: disclosure.notice,
        }
    }
}

// == AI models / routing / usage / budget (W5) ==

/// `ModelRead` — one configured model, key presence only (never the key).
#[derive(Debug, Serialize)]
pub struct AiModelRead {
    pub model_id: String,
    pub display_name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key_env: String,
    pub quality_tier: String,
    pub input_cost_per_1k_tokens: f64,
    pub output_cost_per_1k_tokens: f64,
    pub max_output_tokens: i64,
    pub max_input_tokens: i64,
    pub timeout: i64,
    pub temperature: f64,
    pub enabled: bool,
    pub is_default: bool,
    pub sort_order: i64,
    pub api_key_configured: bool,
    pub source: String,
}

impl From<ica_application::ai_registry::AiModelRead> for AiModelRead {
    fn from(model: ica_application::ai_registry::AiModelRead) -> Self {
        Self {
            model_id: model.model_id,
            display_name: model.display_name,
            provider_type: model.provider_type,
            base_url: model.base_url,
            api_key_env: model.api_key_env,
            quality_tier: model.quality_tier,
            input_cost_per_1k_tokens: model.input_cost_per_1k_tokens,
            output_cost_per_1k_tokens: model.output_cost_per_1k_tokens,
            max_output_tokens: model.max_output_tokens,
            max_input_tokens: model.max_input_tokens,
            timeout: model.timeout,
            temperature: model.temperature,
            enabled: model.enabled,
            is_default: model.is_default,
            sort_order: model.sort_order,
            api_key_configured: model.api_key_configured,
            source: model.source,
        }
    }
}

/// `ModelsRead` — the settings-page read view.
#[derive(Debug, Serialize)]
pub struct AiModelsRead {
    pub models: Vec<AiModelRead>,
    pub routing: std::collections::BTreeMap<String, String>,
}

/// `GET /ai/routing` — `{"routing": {message_type: tier}}`.
#[derive(Debug, Serialize)]
pub struct AiRoutingRead {
    pub routing: std::collections::BTreeMap<String, String>,
}

/// `UsageDay` — one day's aggregate usage.
#[derive(Debug, Serialize)]
pub struct AiUsageDayRead {
    pub date: String,
    pub total_cost_cents: i64,
    pub request_count: i64,
    pub by_model: Vec<AiUsageByModelRead>,
}

/// One per-model entry inside a day.
#[derive(Debug, Serialize)]
pub struct AiUsageByModelRead {
    pub model_id: String,
    pub cost_cents: i64,
    pub request_count: i64,
}

/// `UsageRead.today` — a fixed-key object.
#[derive(Debug, Serialize)]
pub struct AiUsageTodayRead {
    pub total_cost_cents: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub request_count: i64,
    pub daily_budget_cents: i64,
    pub remaining_cents: i64,
    pub budget_utilization: f64,
    pub tier_downgrade_active: bool,
}

/// One per-model all-time total.
#[derive(Debug, Serialize)]
pub struct AiUsageAllTimeRead {
    pub model_id: String,
    pub total_cost_cents: i64,
    pub total_requests: i64,
}

/// `UsageRead` — usage overview.
#[derive(Debug, Serialize)]
pub struct AiUsageRead {
    pub today: AiUsageTodayRead,
    pub history: Vec<AiUsageDayRead>,
    pub by_model_all_time: Vec<AiUsageAllTimeRead>,
}

impl From<ica_application::ai_registry::UsageOverview> for AiUsageRead {
    fn from(overview: ica_application::ai_registry::UsageOverview) -> Self {
        Self {
            today: AiUsageTodayRead {
                total_cost_cents: overview.today.total_cost_cents,
                total_prompt_tokens: overview.today.total_prompt_tokens,
                total_completion_tokens: overview.today.total_completion_tokens,
                request_count: overview.today.request_count,
                daily_budget_cents: overview.today.daily_budget_cents,
                remaining_cents: overview.today.remaining_cents,
                budget_utilization: overview.today.budget_utilization,
                tier_downgrade_active: overview.today.tier_downgrade_active,
            },
            history: overview
                .history
                .into_iter()
                .map(|day| AiUsageDayRead {
                    date: day.date,
                    total_cost_cents: day.total_cost_cents,
                    request_count: day.request_count,
                    by_model: day
                        .by_model
                        .into_iter()
                        .map(|entry| AiUsageByModelRead {
                            model_id: entry.model_id,
                            cost_cents: entry.cost_cents,
                            request_count: entry.request_count,
                        })
                        .collect(),
                })
                .collect(),
            by_model_all_time: overview
                .by_model_all_time
                .into_iter()
                .map(|entry| AiUsageAllTimeRead {
                    model_id: entry.model_id,
                    total_cost_cents: entry.total_cost_cents,
                    total_requests: entry.total_requests,
                })
                .collect(),
        }
    }
}

/// `BudgetRead` — current budget configuration and spend state.
#[derive(Debug, Serialize)]
pub struct AiBudgetRead {
    pub daily_budget_cents: i64,
    pub warning_threshold: i64,
    pub today_spend_cents: i64,
    pub remaining_cents: i64,
    pub status: String,
}

impl From<ica_application::ai_registry::BudgetRead> for AiBudgetRead {
    fn from(read: ica_application::ai_registry::BudgetRead) -> Self {
        Self {
            daily_budget_cents: read.daily_budget_cents,
            warning_threshold: read.warning_threshold,
            today_spend_cents: read.today_spend_cents,
            remaining_cents: read.remaining_cents,
            status: read.status,
        }
    }
}

// == AI key status / backups (W5) ==

/// `AiKeyStatus` — key-configuration status; `key_masked` never contains the
/// full key.
#[derive(Debug, Serialize)]
pub struct AiKeyStatus {
    pub ai_enabled: bool,
    pub key_masked: Option<String>,
    pub model_id: Option<String>,
}

/// `BackupRead` — one backup file on disk.
#[derive(Debug, Serialize)]
pub struct BackupRead {
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub kind: String,
}

impl From<ica_application::ports::BackupRecord> for BackupRead {
    fn from(record: ica_application::ports::BackupRecord) -> Self {
        Self {
            name: record.name,
            path: record.path,
            size_bytes: record.size_bytes,
            created_at: record.created_at,
            kind: record.kind,
        }
    }
}

/// `RestoreRead` — the outcome of a restore.
#[derive(Debug, Serialize)]
pub struct RestoreRead {
    pub restored_from: String,
    pub database: String,
    pub previous_copy: String,
    pub message: String,
}

// == Auth + shares (W6) ==

/// `UserRead` — the public user representation, never the password hash.
#[derive(Debug, Serialize)]
pub struct UserRead {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub tenant_id: String,
    pub is_active: bool,
}

impl From<&ica_application::auth::UserData> for UserRead {
    fn from(user: &ica_application::auth::UserData) -> Self {
        Self {
            id: user.id.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id.clone(),
            is_active: user.is_active,
        }
    }
}

/// `TokenResponse` — the access/refresh pair plus the user.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub user: UserRead,
}

/// `ShareRead` — one explicit owner -> recipient share.
#[derive(Debug, Serialize)]
pub struct ShareRead {
    pub id: String,
    pub owner_id: String,
    pub recipient_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub permission: String,
    #[serde(serialize_with = "serialize_utc")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&ica_application::auth::ShareData> for ShareRead {
    fn from(share: &ica_application::auth::ShareData) -> Self {
        Self {
            id: share.id.clone(),
            owner_id: share.owner_id.clone(),
            recipient_id: share.recipient_id.clone(),
            resource_type: share.resource_type.clone(),
            resource_id: share.resource_id.clone(),
            permission: share.permission.clone(),
            created_at: share.created_at,
        }
    }
}
