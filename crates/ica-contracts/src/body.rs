//! Request bodies: deserialisation, the omitted/null distinction, and the
//! field rules the Python pydantic schemas enforce.
//!
//! Two decisions are structural, not cosmetic:
//!
//! * **`null` and "absent" are different.** [`Incoming`] has three states and is
//!   used as the field type directly. `Option<Incoming<T>>` would *not* work:
//!   serde short-circuits a JSON `null` to `None` before the inner type is ever
//!   asked, which would make `PATCH {"name": null}` indistinguishable from `{}`
//!   and IMP-2 impossible to implement.
//! * **`Json<T>` is not used.** axum's extractor answers with its own plain-text
//!   body and a status that depends on the failure mode; the project needs one
//!   JSON envelope for every 422, so the bytes are parsed here.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use ica_application::{
    CandidatePatch, Field, FollowUpPolicyPatch, FollowUpTaskPatch, NewCandidate, NewConversation,
    NewFollowUpEvaluation, NewFollowUpResult, NewFollowUpTask, NewMessage, NewPosition,
    NewSendRecord, PositionPatch,
};
use ica_domain::{
    CandidateStatus, FollowUpTaskStatus, MessageSource, PositionStatus, ResultType, SenderType,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::error::{ContractError, ValidationIssue};

/// One field of a JSON object: absent, explicitly `null`, or a value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Incoming<T> {
    /// The key was not in the body.
    #[default]
    Absent,
    /// The key was present with `null`.
    Null,
    /// The key was present with a value.
    Value(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Incoming<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Only called when the key is present; `[serde(default)]` on the struct
        // supplies `Absent` otherwise.
        match Option::<T>::deserialize(deserializer)? {
            Some(value) => Ok(Incoming::Value(value)),
            None => Ok(Incoming::Null),
        }
    }
}

/// Collapse the wire shape into the application's three-state field.
fn field<T>(incoming: Incoming<T>) -> Field<T> {
    match incoming {
        Incoming::Absent => Field::Absent,
        Incoming::Null => Field::Null,
        Incoming::Value(value) => Field::Value(value),
    }
}

/// Parse a request body, answering 422 (not axum's plain text) on bad JSON.
pub fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ContractError> {
    serde_json::from_slice(bytes).map_err(|error| {
        ContractError::validation(ValidationIssue::new(
            "json_invalid",
            vec!["body"],
            &format!("Input should be a valid JSON object: {error}"),
            "",
        ))
    })
}

fn issue(kind: &str, field: &str, message: &str, input: &str) -> ValidationIssue {
    ValidationIssue::new(kind, vec!["body", field], message, input)
}

fn check_min_chars(issues: &mut Vec<ValidationIssue>, field: &str, value: &str, min: usize) {
    if value.chars().count() < min {
        issues.push(issue(
            "string_too_short",
            field,
            &format!("String should have at least {min} character"),
            value,
        ));
    }
}

fn check_max_chars(issues: &mut Vec<ValidationIssue>, field: &str, value: &str, max: usize) {
    if value.chars().count() > max {
        issues.push(issue(
            "string_too_long",
            field,
            &format!("String should have at most {max} characters"),
            value,
        ));
    }
}

fn check_non_negative(issues: &mut Vec<ValidationIssue>, field: &str, value: Option<i64>) {
    if let Some(value) = value {
        if value < 0 {
            issues.push(issue(
                "value_error",
                field,
                "Value error, salary must not be negative",
                &value.to_string(),
            ));
        }
    }
}

fn failure<T>(issues: Vec<ValidationIssue>) -> Result<T, ContractError> {
    Err(ContractError::validation_issues(issues))
}

/// Parse an instant the way pydantic does: RFC 3339, `YYYY-MM-DD HH:MM:SS[.f]`,
/// or a bare date at midnight. A value without an offset is taken as UTC, which
/// is how the baseline stores every timestamp.
fn parse_datetime(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if let Ok(aware) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(aware.with_timezone(&Utc));
    }
    for format in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(naive.and_utc());
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Utc
            .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
            .single();
    }
    None
}

fn take_datetime(
    issues: &mut Vec<ValidationIssue>,
    field_name: &str,
    value: Incoming<String>,
) -> Option<DateTime<Utc>> {
    match value {
        Incoming::Absent | Incoming::Null => None,
        Incoming::Value(raw) => match parse_datetime(&raw) {
            Some(parsed) => Some(parsed),
            None => {
                issues.push(issue(
                    "datetime_parsing",
                    field_name,
                    "Input should be a valid datetime",
                    &raw,
                ));
                None
            }
        },
    }
}

fn take_enum<T>(
    issues: &mut Vec<ValidationIssue>,
    field_name: &str,
    value: Incoming<String>,
    expected: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    match value {
        Incoming::Absent | Incoming::Null => None,
        Incoming::Value(raw) => {
            let parsed = parse(&raw);
            if parsed.is_none() {
                issues.push(issue(
                    "enum",
                    field_name,
                    &format!("Input should be {expected}"),
                    &raw,
                ));
            }
            parsed
        }
    }
}

/// Take a required string that may not be `null` or absent.
fn take_required_string(
    issues: &mut Vec<ValidationIssue>,
    field_name: &str,
    value: Incoming<String>,
    max: Option<usize>,
) -> String {
    let value = match value {
        Incoming::Value(value) => value,
        Incoming::Null => {
            issues.push(issue(
                "string_type",
                field_name,
                "Input should be a valid string",
                "null",
            ));
            return String::new();
        }
        Incoming::Absent => {
            issues.push(issue("missing", field_name, "Field required", ""));
            return String::new();
        }
    };
    if let Some(max) = max {
        check_max_chars(issues, field_name, &value, max);
    }
    value
}

/// An optional string field: `null`/absent both mean "no value".
fn take_optional_string(value: Incoming<String>) -> Option<String> {
    match value {
        Incoming::Absent | Incoming::Null => None,
        Incoming::Value(value) => Some(value),
    }
}

// ── Positions ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawPositionCreate {
    pub name: Incoming<String>,
    pub description: Incoming<String>,
    pub location: Incoming<String>,
    pub skill_requirements: Incoming<String>,
    pub experience_requirements: Incoming<String>,
    pub education_requirements: Incoming<String>,
    pub salary_min: Incoming<i64>,
    pub salary_max: Incoming<i64>,
    pub notes: Incoming<String>,
    pub status: Incoming<String>,
    pub archived: Incoming<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawPositionPatch {
    pub name: Incoming<String>,
    pub description: Incoming<String>,
    pub location: Incoming<String>,
    pub skill_requirements: Incoming<String>,
    pub experience_requirements: Incoming<String>,
    pub education_requirements: Incoming<String>,
    pub salary_min: Incoming<i64>,
    pub salary_max: Incoming<i64>,
    pub notes: Incoming<String>,
    pub status: Incoming<String>,
    pub archived: Incoming<bool>,
}

fn take_salary(
    issues: &mut Vec<ValidationIssue>,
    field_name: &str,
    value: Incoming<i64>,
) -> Option<i64> {
    let parsed = match value {
        Incoming::Absent | Incoming::Null => None,
        Incoming::Value(value) => Some(value),
    };
    check_non_negative(issues, field_name, parsed);
    parsed
}

/// Validate a `POST /api/positions` body.
pub fn position_create(raw: RawPositionCreate) -> Result<NewPosition, ContractError> {
    let mut issues = Vec::new();

    let name = take_required_string(&mut issues, "name", raw.name, Some(200));
    if name.is_empty() && issues.iter().all(|entry| entry.loc[1] != "name") {
        issues.push(issue(
            "string_too_short",
            "name",
            "String should have at least 1 character",
            &name,
        ));
    }
    let location = take_optional_string(raw.location);
    if let Some(location) = location.as_deref() {
        check_max_chars(&mut issues, "location", location, 200);
    }
    let education_requirements = take_optional_string(raw.education_requirements);
    if let Some(value) = education_requirements.as_deref() {
        check_max_chars(&mut issues, "education_requirements", value, 200);
    }
    let salary_min = take_salary(&mut issues, "salary_min", raw.salary_min);
    let salary_max = take_salary(&mut issues, "salary_max", raw.salary_max);
    let status = match take_enum(
        &mut issues,
        "status",
        raw.status,
        "'Draft', 'Open', 'OnHold' or 'Closed'",
        PositionStatus::parse,
    ) {
        Some(status) => status,
        None => PositionStatus::Open,
    };
    // `archived` is not part of PositionCreate in the baseline schema: an extra
    // key is ignored (pydantic's default) and a new row is always unarchived.

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewPosition {
        name,
        description: take_optional_string(raw.description),
        location,
        skill_requirements: take_optional_string(raw.skill_requirements),
        experience_requirements: take_optional_string(raw.experience_requirements),
        education_requirements,
        salary_min,
        salary_max,
        notes: take_optional_string(raw.notes),
        status,
    })
}

/// Validate a `PATCH /api/positions` body.
///
/// IMP-2 lives here: an explicit `null` on a NOT NULL column (`name`,
/// `archived`) is a 422, not the 500 the baseline produces when SQLAlchemy tries
/// to store NULL. `status = null` is still accepted and ignored, because the
/// baseline's `if payload.status is not None` skips it.
pub fn position_patch(raw: RawPositionPatch) -> Result<PositionPatch, ContractError> {
    let mut issues = Vec::new();

    let name = match raw.name {
        Incoming::Null => {
            issues.push(issue(
                "string_type",
                "name",
                "Input should be a valid string",
                "null",
            ));
            Field::Absent
        }
        Incoming::Value(value) => {
            check_min_chars(&mut issues, "name", &value, 1);
            check_max_chars(&mut issues, "name", &value, 200);
            Field::Value(value)
        }
        Incoming::Absent => Field::Absent,
    };

    let location = match raw.location {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "location", &value, 200);
            Field::Value(value)
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };
    let education_requirements = match raw.education_requirements {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "education_requirements", &value, 200);
            Field::Value(value)
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };

    let salary_min = match raw.salary_min {
        Incoming::Value(value) => {
            check_non_negative(&mut issues, "salary_min", Some(value));
            Field::Value(value)
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };
    let salary_max = match raw.salary_max {
        Incoming::Value(value) => {
            check_non_negative(&mut issues, "salary_max", Some(value));
            Field::Value(value)
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };

    // `status = null` is ignored by the baseline (`if payload.status is not None`).
    let status = match raw.status {
        Incoming::Value(value) => match PositionStatus::parse(&value) {
            Some(parsed) => Field::Value(parsed),
            None => {
                issues.push(issue(
                    "enum",
                    "status",
                    "Input should be 'Draft', 'Open', 'OnHold' or 'Closed'",
                    &value,
                ));
                Field::Absent
            }
        },
        Incoming::Null | Incoming::Absent => Field::Absent,
    };

    // IMP-2: `archived` is NOT NULL; an explicit null is a rejected request.
    let archived = match raw.archived {
        Incoming::Value(value) => Field::Value(value),
        Incoming::Null => {
            issues.push(issue(
                "bool_type",
                "archived",
                "Input should be a valid boolean",
                "null",
            ));
            Field::Absent
        }
        Incoming::Absent => Field::Absent,
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(PositionPatch {
        name,
        description: field(raw.description),
        location,
        skill_requirements: field(raw.skill_requirements),
        experience_requirements: field(raw.experience_requirements),
        education_requirements,
        salary_min,
        salary_max,
        status,
        archived,
        notes: field(raw.notes),
    })
}

// ── Candidates ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawCandidateCreate {
    pub name: Incoming<String>,
    pub source: Incoming<String>,
    pub position_id: Incoming<String>,
    pub experience_summary: Incoming<String>,
    pub skill_summary: Incoming<String>,
    pub expected_salary: Incoming<String>,
    pub expected_location: Incoming<String>,
    pub job_intention: Incoming<String>,
    pub notes: Incoming<String>,
    pub status: Incoming<String>,
    pub archived: Incoming<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawCandidatePatch {
    pub name: Incoming<String>,
    pub source: Incoming<String>,
    pub position_id: Incoming<String>,
    pub experience_summary: Incoming<String>,
    pub skill_summary: Incoming<String>,
    pub expected_salary: Incoming<String>,
    pub expected_location: Incoming<String>,
    pub job_intention: Incoming<String>,
    pub notes: Incoming<String>,
    pub status: Incoming<String>,
    pub archived: Incoming<bool>,
}

/// Trim a display name and reject one that is only whitespace.
///
/// `min_length=1` runs before trimming in pydantic, so `"   "` would otherwise
/// pass validation and store a candidate with a blank name.
fn clean_candidate_name(issues: &mut Vec<ValidationIssue>, value: String) -> String {
    let cleaned = value.trim().to_string();
    if cleaned.is_empty() {
        issues.push(issue(
            "value_error",
            "name",
            "Value error, name must not be blank",
            &value,
        ));
    }
    cleaned
}

/// Validate a `POST /api/candidates` body.
pub fn candidate_create(raw: RawCandidateCreate) -> Result<NewCandidate, ContractError> {
    let mut issues = Vec::new();

    let raw_name = take_required_string(&mut issues, "name", raw.name, Some(200));
    let name = clean_candidate_name(&mut issues, raw_name);

    let source = match raw.source {
        Incoming::Null => {
            issues.push(issue(
                "string_type",
                "source",
                "Input should be a valid string",
                "null",
            ));
            "boss".to_string()
        }
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "source", &value, 64);
            value
        }
        Incoming::Absent => "boss".to_string(),
    };

    let expected_salary = take_optional_string(raw.expected_salary);
    if let Some(value) = expected_salary.as_deref() {
        check_max_chars(&mut issues, "expected_salary", value, 100);
    }
    let expected_location = take_optional_string(raw.expected_location);
    if let Some(value) = expected_location.as_deref() {
        check_max_chars(&mut issues, "expected_location", value, 200);
    }

    let status = match take_enum(
        &mut issues,
        "status",
        raw.status,
        "'New', 'Contacted', 'ReadNoReply', 'Replied', 'Active', 'Interview', 'Rejected', 'Withdrawn' or 'Closed'",
        CandidateStatus::parse,
    ) {
        Some(status) => status,
        None => CandidateStatus::New,
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewCandidate {
        name,
        source,
        position_id: take_optional_string(raw.position_id),
        experience_summary: take_optional_string(raw.experience_summary),
        skill_summary: take_optional_string(raw.skill_summary),
        expected_salary,
        expected_location,
        job_intention: take_optional_string(raw.job_intention),
        notes: take_optional_string(raw.notes),
        status,
    })
}

/// Validate a `PATCH /api/candidates` body.
///
/// `name`, `source`, `status` and `archived` are NOT NULL: an explicit `null`
/// is a 422 (not an IntegrityError), and an omitted field stays untouched.
pub fn candidate_patch(raw: RawCandidatePatch) -> Result<CandidatePatch, ContractError> {
    let mut issues = Vec::new();

    let name = match raw.name {
        Incoming::Null => {
            issues.push(issue(
                "value_error",
                "name",
                "Value error, name must not be null",
                "null",
            ));
            Field::Absent
        }
        Incoming::Value(value) => {
            check_min_chars(&mut issues, "name", &value, 1);
            check_max_chars(&mut issues, "name", &value, 200);
            Field::Value(clean_candidate_name(&mut issues, value))
        }
        Incoming::Absent => Field::Absent,
    };

    let source = match raw.source {
        Incoming::Null => {
            issues.push(issue(
                "value_error",
                "source",
                "Value error, field must not be null",
                "null",
            ));
            Field::Absent
        }
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "source", &value, 64);
            Field::Value(value)
        }
        Incoming::Absent => Field::Absent,
    };

    let expected_salary = match raw.expected_salary {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "expected_salary", &value, 100);
            Field::Value(value)
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };
    let expected_location = match raw.expected_location {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "expected_location", &value, 200);
            Field::Value(value)
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };

    let status = match raw.status {
        Incoming::Null => {
            issues.push(issue(
                "value_error",
                "status",
                "Value error, field must not be null",
                "null",
            ));
            Field::Absent
        }
        Incoming::Value(value) => match CandidateStatus::parse(&value) {
            Some(parsed) => Field::Value(parsed),
            None => {
                issues.push(issue(
                    "enum",
                    "status",
                    "Input should be a valid candidate status",
                    &value,
                ));
                Field::Absent
            }
        },
        Incoming::Absent => Field::Absent,
    };

    let archived = match raw.archived {
        Incoming::Value(value) => Field::Value(value),
        Incoming::Null => {
            issues.push(issue(
                "value_error",
                "archived",
                "Value error, field must not be null",
                "null",
            ));
            Field::Absent
        }
        Incoming::Absent => Field::Absent,
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(CandidatePatch {
        name,
        source,
        position_id: field(raw.position_id),
        experience_summary: field(raw.experience_summary),
        skill_summary: field(raw.skill_summary),
        expected_salary,
        expected_location,
        job_intention: field(raw.job_intention),
        status,
        archived,
        notes: field(raw.notes),
    })
}

// ── Conversations and messages ─────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawConversationCreate {
    pub candidate_id: Incoming<String>,
    pub channel: Incoming<String>,
    pub title: Incoming<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawMessageCreate {
    pub candidate_id: Incoming<String>,
    pub conversation_id: Incoming<String>,
    pub sender_type: Incoming<String>,
    pub content: Incoming<String>,
    pub message_time: Incoming<String>,
    pub channel: Incoming<String>,
    pub source: Incoming<String>,
}

/// Validate a `POST /api/conversations` body.
pub fn conversation_create(raw: RawConversationCreate) -> Result<NewConversation, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);
    let channel = match raw.channel {
        Incoming::Null => {
            issues.push(issue(
                "string_type",
                "channel",
                "Input should be a valid string",
                "null",
            ));
            "boss".to_string()
        }
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "channel", &value, 32);
            value
        }
        Incoming::Absent => "boss".to_string(),
    };
    let title = take_optional_string(raw.title);
    if let Some(value) = title.as_deref() {
        check_max_chars(&mut issues, "title", value, 200);
    }
    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewConversation {
        candidate_id,
        channel,
        title,
    })
}

/// Validate a `POST /api/messages` body.
pub fn message_create(raw: RawMessageCreate) -> Result<NewMessage, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);
    let content = take_required_string(&mut issues, "content", raw.content, None);
    check_min_chars(&mut issues, "content", &content, 1);

    let channel = match raw.channel {
        Incoming::Null => {
            issues.push(issue(
                "string_type",
                "channel",
                "Input should be a valid string",
                "null",
            ));
            "boss".to_string()
        }
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "channel", &value, 32);
            value
        }
        Incoming::Absent => "boss".to_string(),
    };
    let sender_type: Option<SenderType> = take_enum(
        &mut issues,
        "sender_type",
        raw.sender_type,
        "'Candidate', 'Recruiter' or 'System'",
        SenderType::parse,
    );
    let source = match take_enum(
        &mut issues,
        "source",
        raw.source,
        "'manual', 'paste_import' or 'screenshot_ocr'",
        MessageSource::parse,
    ) {
        Some(source) => source,
        None => MessageSource::Manual,
    };
    let message_time = take_datetime(&mut issues, "message_time", raw.message_time);

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewMessage {
        candidate_id,
        conversation_id: take_optional_string(raw.conversation_id),
        sender_type,
        content,
        message_time,
        channel,
        source,
    })
}

// ── Send records ───────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawSendRecordCreate {
    pub candidate_id: Incoming<String>,
    pub task_id: Incoming<String>,
    pub draft_id: Incoming<String>,
    pub channel: Incoming<String>,
    pub target: Incoming<String>,
    pub content_summary: Incoming<String>,
    pub status: Incoming<String>,
    pub sent_at: Incoming<String>,
    pub note: Incoming<String>,
}

/// Validate a `POST /api/send-records` body.
pub fn send_record_create(raw: RawSendRecordCreate) -> Result<NewSendRecord, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);

    let channel = match raw.channel {
        Incoming::Null => {
            issues.push(issue(
                "string_type",
                "channel",
                "Input should be a valid string",
                "null",
            ));
            "boss".to_string()
        }
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "channel", &value, 32);
            value
        }
        Incoming::Absent => "boss".to_string(),
    };
    let target = take_optional_string(raw.target);
    if let Some(value) = target.as_deref() {
        check_max_chars(&mut issues, "target", value, 200);
    }

    let status = match take_enum(
        &mut issues,
        "status",
        raw.status,
        "'sent', 'not_sent', 'candidate_replied', 'candidate_no_reply', 'candidate_declined', 'interview' or 'other'",
        ResultType::parse,
    ) {
        Some(status) => status,
        None => {
            if issues.iter().all(|entry| entry.loc[1] != "status") {
                issues.push(issue("missing", "status", "Field required", ""));
            }
            ResultType::Sent
        }
    };
    let sent_at = take_datetime(&mut issues, "sent_at", raw.sent_at);

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewSendRecord {
        candidate_id,
        task_id: take_optional_string(raw.task_id),
        draft_id: take_optional_string(raw.draft_id),
        channel,
        target,
        content_summary: take_optional_string(raw.content_summary),
        status,
        sent_at,
        note: take_optional_string(raw.note),
    })
}

// ── Follow-ups (W2) ────────────────────────────────────────────────────────

/// A validated `POST /api/follow-ups` body.
#[derive(Deserialize, Default)]
pub struct RawFollowUpCreate {
    #[serde(default)]
    pub candidate_id: Incoming<String>,
    #[serde(default)]
    pub position_id: Incoming<String>,
    #[serde(default)]
    pub objective: Incoming<String>,
    #[serde(default)]
    pub reason: Incoming<String>,
    #[serde(default)]
    pub recommended_at: Incoming<String>,
}

/// Validate a `POST /api/follow-ups` body (`objective` caps at 64 characters).
pub fn follow_up_create(raw: RawFollowUpCreate) -> Result<NewFollowUpTask, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);
    let position_id = take_optional_string(raw.position_id);
    let objective = match &raw.objective {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "objective", value, 64);
            Some(value.clone())
        }
        Incoming::Null | Incoming::Absent => None,
    };
    let reason = take_optional_string(raw.reason);
    let recommended_at = take_datetime(&mut issues, "recommended_at", raw.recommended_at);

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewFollowUpTask {
        candidate_id,
        position_id,
        objective,
        reason,
        recommended_at,
    })
}

/// A validated `PATCH /api/follow-ups/{id}` body.
///
/// The three-state distinction matters for `recommended_at`: an explicit null
/// clears the field (`model_dump(exclude_unset=True)` keeps it), while an
/// absent key leaves it alone. `status` collapses null into absent — the
/// baseline checks `payload.status is not None` after the pydantic round trip.
#[derive(Deserialize, Default)]
pub struct RawFollowUpPatch {
    #[serde(default)]
    pub objective: Incoming<String>,
    #[serde(default)]
    pub reason: Incoming<String>,
    #[serde(default)]
    pub recommended_at: Incoming<String>,
    #[serde(default)]
    pub status: Incoming<String>,
}

const FOLLOW_UP_STATUS_EXPECTED: &str = "'Pending', 'Eligible', 'Drafting', 'Reviewing', \
     'Rejected', 'Approved', 'Copied', 'AwaitingResult', 'Completed', 'Cancelled', \
     'Expired' or 'Blocked'";

/// Validate a `PATCH /api/follow-ups/{id}` body.
pub fn follow_up_patch(raw: RawFollowUpPatch) -> Result<FollowUpTaskPatch, ContractError> {
    let mut issues = Vec::new();

    let objective = match &raw.objective {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "objective", value, 64);
            Field::Value(value.clone())
        }
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };
    let reason = field(raw.reason);
    let recommended_at = match raw.recommended_at {
        Incoming::Value(raw) => match parse_datetime(&raw) {
            Some(value) => Field::Value(value),
            None => {
                issues.push(issue(
                    "datetime_parsing",
                    "recommended_at",
                    "Input should be a valid datetime",
                    &raw,
                ));
                Field::Absent
            }
        },
        Incoming::Null => Field::Null,
        Incoming::Absent => Field::Absent,
    };
    let status = match raw.status {
        Incoming::Value(raw) => match FollowUpTaskStatus::parse(&raw) {
            Some(status) => Field::Value(status),
            None => {
                issues.push(issue(
                    "enum",
                    "status",
                    &format!("Input should be {FOLLOW_UP_STATUS_EXPECTED}"),
                    &raw,
                ));
                Field::Absent
            }
        },
        // Absent and null mean the same thing for a status: no change.
        Incoming::Null | Incoming::Absent => Field::Absent,
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(FollowUpTaskPatch {
        objective,
        reason,
        recommended_at,
        status,
    })
}

/// A validated `POST /api/follow-ups/{id}/record-result` body.
#[derive(Deserialize, Default)]
pub struct RawFollowUpResult {
    #[serde(default)]
    pub result_type: Incoming<String>,
    #[serde(default)]
    pub note: Incoming<String>,
}

/// Validate a `POST /api/follow-ups/{id}/record-result` body.
pub fn follow_up_result(raw: RawFollowUpResult) -> Result<NewFollowUpResult, ContractError> {
    let mut issues = Vec::new();
    let result_type = match take_enum(
        &mut issues,
        "result_type",
        raw.result_type,
        "'sent', 'not_sent', 'candidate_replied', 'candidate_no_reply', 'candidate_declined', 'interview' or 'other'",
        ResultType::parse,
    ) {
        Some(result_type) => result_type,
        None => {
            if issues.iter().all(|entry| entry.loc[1] != "result_type") {
                issues.push(issue("missing", "result_type", "Field required", ""));
            }
            ResultType::Sent
        }
    };
    let note = take_optional_string(raw.note);

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewFollowUpResult { result_type, note })
}

/// A validated `POST /api/follow-ups/evaluate` body (`policy_id` is accepted
/// and ignored, so it is not even read here).
#[derive(Deserialize, Default)]
pub struct RawFollowUpEvaluate {
    #[serde(default)]
    pub candidate_id: Incoming<String>,
    #[serde(default)]
    pub position_id: Incoming<String>,
}

/// Validate a `POST /api/follow-ups/evaluate` body.
pub fn follow_up_evaluate(
    raw: RawFollowUpEvaluate,
) -> Result<NewFollowUpEvaluation, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);
    let position_id = take_optional_string(raw.position_id);

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewFollowUpEvaluation {
        candidate_id,
        position_id,
    })
}

/// A validated `PATCH /api/follow-ups/policy/current` body. Every field is
/// `int | None` with a range; `null` and absent collapse into "left alone".
#[derive(Deserialize, Default)]
pub struct RawFollowUpPolicyPatch {
    #[serde(default)]
    pub first_follow_up_delay_hours: Incoming<i64>,
    #[serde(default)]
    pub max_active_follow_ups: Incoming<i64>,
    #[serde(default)]
    pub min_interval_hours: Incoming<i64>,
    #[serde(default)]
    pub task_validity_days: Incoming<i64>,
}

/// Clamp-check one policy integer against its pydantic range.
fn take_policy_int(
    issues: &mut Vec<ValidationIssue>,
    field_name: &str,
    value: Incoming<i64>,
    min: i64,
    max: i64,
) -> Option<i64> {
    match value {
        Incoming::Absent | Incoming::Null => None,
        Incoming::Value(value) => {
            if value < min {
                issues.push(issue(
                    "greater_than_equal",
                    field_name,
                    &format!("Input should be greater than or equal to {min}"),
                    &value.to_string(),
                ));
                None
            } else if value > max {
                issues.push(issue(
                    "less_than_equal",
                    field_name,
                    &format!("Input should be less than or equal to {max}"),
                    &value.to_string(),
                ));
                None
            } else {
                Some(value)
            }
        }
    }
}

/// Validate a `PATCH /api/follow-ups/policy/current` body.
pub fn follow_up_policy_patch(
    raw: RawFollowUpPolicyPatch,
) -> Result<FollowUpPolicyPatch, ContractError> {
    let mut issues = Vec::new();
    let first_follow_up_delay_hours = take_policy_int(
        &mut issues,
        "first_follow_up_delay_hours",
        raw.first_follow_up_delay_hours,
        0,
        720,
    );
    let max_active_follow_ups = take_policy_int(
        &mut issues,
        "max_active_follow_ups",
        raw.max_active_follow_ups,
        1,
        20,
    );
    let min_interval_hours = take_policy_int(
        &mut issues,
        "min_interval_hours",
        raw.min_interval_hours,
        0,
        720,
    );
    let task_validity_days = take_policy_int(
        &mut issues,
        "task_validity_days",
        raw.task_validity_days,
        1,
        365,
    );

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(FollowUpPolicyPatch {
        first_follow_up_delay_hours,
        max_active_follow_ups,
        min_interval_hours,
        task_validity_days,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_null_name_is_rejected_on_patch() {
        let raw: RawPositionPatch = parse(br#"{"name": null}"#).unwrap();
        let error = position_patch(raw).unwrap_err();
        let ContractError::ValidationIssues { issues } = error else {
            panic!("expected a 422");
        };
        assert_eq!(issues[0].loc, ["body", "name"]);
    }

    #[test]
    fn an_explicit_null_archived_is_rejected_on_patch() {
        let raw: RawPositionPatch = parse(br#"{"archived": null}"#).unwrap();
        assert!(position_patch(raw).is_err());
    }

    #[test]
    fn a_null_status_is_ignored_like_the_baseline() {
        let raw: RawPositionPatch = parse(br#"{"status": null}"#).unwrap();
        let patch = position_patch(raw).unwrap();
        assert_eq!(patch.status, Field::Absent);
    }

    #[test]
    fn a_null_on_a_nullable_column_clears_it() {
        let raw: RawPositionPatch = parse(br#"{"location": null}"#).unwrap();
        let patch = position_patch(raw).unwrap();
        assert_eq!(patch.location, Field::Null);
        assert!(patch.description.is_absent());
    }

    #[test]
    fn a_whitespace_candidate_name_is_blank() {
        let raw: RawCandidateCreate = parse(br#"{"name": "   "}"#).unwrap();
        assert!(candidate_create(raw).is_err());
    }

    #[test]
    fn a_whitespace_position_name_is_accepted_like_pydantic() {
        // PositionBase has no strip validator: "  " is a legal (if useless) name.
        let raw: RawPositionCreate = parse(br#"{"name": "  "}"#).unwrap();
        assert_eq!(position_create(raw).unwrap().name, "  ");
    }

    #[test]
    fn a_negative_salary_is_rejected_on_create_and_update() {
        let raw: RawPositionCreate = parse(br#"{"name": "x", "salary_min": -1}"#).unwrap();
        assert!(position_create(raw).is_err());
        let raw: RawPositionPatch = parse(br#"{"salary_max": -5}"#).unwrap();
        assert!(position_patch(raw).is_err());
    }

    #[test]
    fn message_content_must_not_be_empty() {
        let raw: RawMessageCreate = parse(br#"{"candidate_id": "c-1", "content": ""}"#).unwrap();
        assert!(message_create(raw).is_err());
    }

    #[test]
    fn an_unknown_message_source_is_rejected() {
        let raw: RawMessageCreate =
            parse(br#"{"candidate_id": "c-1", "content": "hi", "source": "browser"}"#).unwrap();
        assert!(message_create(raw).is_err());
    }

    #[test]
    fn a_datetime_without_an_offset_is_utc() {
        assert_eq!(
            parse_datetime("2026-11-01T09:30:00").unwrap().to_rfc3339(),
            "2026-11-01T09:30:00+00:00"
        );
        assert_eq!(
            parse_datetime("2026-11-01 09:30:00").unwrap().to_rfc3339(),
            "2026-11-01T09:30:00+00:00"
        );
        assert_eq!(
            parse_datetime("2026-11-01T17:30:00+08:00")
                .unwrap()
                .to_rfc3339(),
            "2026-11-01T09:30:00+00:00"
        );
    }

    #[test]
    fn a_send_record_without_a_status_is_rejected() {
        let raw: RawSendRecordCreate = parse(br#"{"candidate_id": "c-1"}"#).unwrap();
        assert!(send_record_create(raw).is_err());
    }

    #[test]
    fn an_absent_key_stays_absent_even_next_to_a_null_one() {
        let raw: RawCandidatePatch = parse(br#"{"notes": null}"#).unwrap();
        let patch = candidate_patch(raw).unwrap();
        assert_eq!(patch.notes, Field::Null);
        assert_eq!(patch.name, Field::Absent);
        assert_eq!(patch.position_id, Field::Absent);
    }
}

// ── Drafts (W3) ────────────────────────────────────────────────────────────

use ica_application::{
    DraftPatch, ImportCorrection, NewDraftGeneration, NewDraftGroupSelection, NewImportConfirm,
    NewImportOcr, NewImportPreview,
};
use ica_domain::{MessageType, Tone};

/// A validated `POST /api/drafts/generate` body.
#[derive(Deserialize, Default)]
pub struct RawGenerateRequest {
    #[serde(default)]
    pub candidate_id: Incoming<String>,
    #[serde(default)]
    pub task_id: Incoming<String>,
    #[serde(default)]
    pub message_type: Incoming<String>,
    #[serde(default)]
    pub tone: Incoming<String>,
    #[serde(default)]
    pub user_instruction: Incoming<String>,
    #[serde(default)]
    pub include_alternatives: Incoming<bool>,
    #[serde(default)]
    pub max_length: Incoming<i64>,
    #[serde(default)]
    pub comparison_models: Incoming<Vec<String>>,
    #[serde(default)]
    pub model_id: Incoming<String>,
}

/// Validate a `POST /api/drafts/generate` body.
pub fn generate_request(raw: RawGenerateRequest) -> Result<NewDraftGeneration, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);
    let task_id = take_optional_string(raw.task_id);
    let message_type = match take_enum(
        &mut issues,
        "message_type",
        raw.message_type,
        MessageType::EXPECTED_LIST,
        MessageType::parse,
    ) {
        Some(value) => value,
        None => MessageType::Reply,
    };
    let tone = match take_enum(
        &mut issues,
        "tone",
        raw.tone,
        Tone::EXPECTED_LIST,
        Tone::parse,
    ) {
        Some(value) => value,
        None => Tone::Friendly,
    };
    let user_instruction = match &raw.user_instruction {
        Incoming::Value(value) => {
            check_max_chars(&mut issues, "user_instruction", value, 2000);
            Some(value.clone())
        }
        Incoming::Null | Incoming::Absent => None,
    };
    let include_alternatives = match raw.include_alternatives {
        Incoming::Value(value) => value,
        _ => true,
    };
    let max_length = match raw.max_length {
        Incoming::Absent | Incoming::Null => None,
        Incoming::Value(value) => {
            if value < 50 {
                issues.push(issue(
                    "greater_than_equal",
                    "max_length",
                    "Input should be greater than or equal to 50",
                    &value.to_string(),
                ));
                None
            } else if value > 2000 {
                issues.push(issue(
                    "less_than_equal",
                    "max_length",
                    "Input should be less than or equal to 2000",
                    &value.to_string(),
                ));
                None
            } else {
                Some(value)
            }
        }
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewDraftGeneration {
        candidate_id,
        task_id,
        message_type,
        tone,
        user_instruction,
        include_alternatives,
        max_length,
        comparison_models: match raw.comparison_models {
            Incoming::Value(models) => models,
            _ => Vec::new(),
        },
        model_id: take_optional_string(raw.model_id),
    })
}

/// A validated `PATCH /api/drafts/{id}` body. `message_text` has
/// `min_length=1`; both fields ignore absent and null alike
/// (`exclude_unset + exclude_none`).
#[derive(Deserialize, Default)]
pub struct RawDraftPatch {
    #[serde(default)]
    pub message_text: Incoming<String>,
    #[serde(default)]
    pub objective: Incoming<String>,
}

/// Validate a `PATCH /api/drafts/{id}` body.
pub fn draft_patch(raw: RawDraftPatch) -> Result<DraftPatch, ContractError> {
    let mut issues = Vec::new();
    let message_text = match &raw.message_text {
        Incoming::Value(value) => {
            check_min_chars(&mut issues, "message_text", value, 1);
            Some(value.clone())
        }
        Incoming::Null | Incoming::Absent => None,
    };
    let objective = match &raw.objective {
        Incoming::Value(value) => Some(value.clone()),
        Incoming::Null | Incoming::Absent => None,
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(DraftPatch {
        message_text,
        objective,
    })
}

/// A validated `POST /api/drafts/{id}/approve` body (`DraftApproveRequest`:
/// one optional `note`, echoed into the audit metadata).
#[derive(Deserialize, Default)]
pub struct RawDraftApprove {
    #[serde(default)]
    pub note: Incoming<String>,
}

/// Validate a `POST /api/drafts/{id}/approve` body: absent and null are both
/// "no note" (`note: str | None = None`).
pub fn draft_approve(raw: RawDraftApprove) -> Result<Option<String>, ContractError> {
    Ok(match raw.note {
        Incoming::Value(value) => Some(value),
        Incoming::Null | Incoming::Absent => None,
    })
}

/// A validated `POST /api/draft-groups/{id}/select` body.
#[derive(Deserialize, Default)]
pub struct RawDraftGroupSelect {
    #[serde(default)]
    pub draft_id: Incoming<String>,
}

/// Validate a `POST /api/draft-groups/{id}/select` body.
pub fn draft_group_select(
    raw: RawDraftGroupSelect,
) -> Result<NewDraftGroupSelection, ContractError> {
    let mut issues = Vec::new();
    let draft_id = take_required_string(&mut issues, "draft_id", raw.draft_id, None);

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewDraftGroupSelection { draft_id })
}

// ── Import sessions (W4) ────────────────────────────────────────────────────

/// Raw `POST /api/imports/preview` body (`ImportPreviewRequest`).
#[derive(Deserialize, Default)]
pub struct RawImportPreview {
    #[serde(default)]
    pub candidate_id: Incoming<String>,
    #[serde(default)]
    pub raw_text: Incoming<String>,
    #[serde(default)]
    pub source: Incoming<String>,
}

/// Validate a `POST /api/imports/preview` body.
///
/// `raw_text` is `Field(min_length=1)` in the baseline, so an empty or
/// whitespace-free-but-empty value is a 422; `source` is an enum whose values
/// are the `MessageSource` strings.
pub fn import_preview(raw: RawImportPreview) -> Result<NewImportPreview, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_optional_string(raw.candidate_id);
    let raw_text = take_required_string(&mut issues, "raw_text", raw.raw_text, None);
    if raw_text.is_empty() {
        issues.push(issue(
            "string_too_short",
            "raw_text",
            "String should have at least 1 character",
            "",
        ));
    }
    let source = match raw.source {
        Incoming::Absent | Incoming::Null => ica_domain::MessageSource::PasteImport,
        Incoming::Value(value) => match ica_domain::MessageSource::parse(&value) {
            Some(parsed) => parsed,
            None => {
                issues.push(issue(
                    "enum",
                    "source",
                    "Input should be 'manual', 'paste_import' or 'screenshot_ocr'",
                    &value,
                ));
                ica_domain::MessageSource::PasteImport
            }
        },
    };

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewImportPreview {
        candidate_id,
        raw_text,
        source,
    })
}

/// Raw per-record correction inside `ImportConfirmRequest.records`.
#[derive(Deserialize, Default)]
pub struct RawImportCorrection {
    #[serde(default)]
    pub id: Incoming<String>,
    #[serde(default)]
    pub sender_type: Incoming<String>,
    #[serde(default)]
    pub content: Incoming<String>,
    #[serde(default)]
    pub message_time: Incoming<String>,
    #[serde(default)]
    pub skip: Incoming<bool>,
}

/// Raw `POST /api/imports/{id}/confirm` body (`ImportConfirmRequest`).
#[derive(Deserialize, Default)]
pub struct RawImportConfirm {
    #[serde(default)]
    pub candidate_id: Incoming<String>,
    #[serde(default)]
    pub conversation_id: Incoming<String>,
    #[serde(default)]
    pub records: Incoming<Vec<RawImportCorrection>>,
}

/// Validate a `POST /api/imports/{id}/confirm` body.
pub fn import_confirm(raw: RawImportConfirm) -> Result<NewImportConfirm, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_required_string(&mut issues, "candidate_id", raw.candidate_id, None);
    let conversation_id = take_optional_string(raw.conversation_id);

    let mut corrections = Vec::new();
    if let Incoming::Value(items) = raw.records {
        for (index, item) in items.into_iter().enumerate() {
            let prefix = format!("records.{index}");
            let id = take_required_string(&mut issues, &format!("{prefix}.id"), item.id, None);
            let sender_type = match item.sender_type {
                Incoming::Absent | Incoming::Null => None,
                Incoming::Value(value) => match ica_domain::SenderType::parse(&value) {
                    Some(parsed) => Some(parsed),
                    None => {
                        issues.push(issue(
                            "enum",
                            &format!("{prefix}.sender_type"),
                            "Input should be 'Candidate', 'Recruiter' or 'System'",
                            &value,
                        ));
                        None
                    }
                },
            };
            let content = take_optional_string(item.content);
            let message_time = take_datetime(
                &mut issues,
                &format!("{prefix}.message_time"),
                item.message_time,
            );
            let skip = match item.skip {
                Incoming::Value(value) => value,
                Incoming::Absent | Incoming::Null => false,
            };
            corrections.push(ImportCorrection {
                id,
                sender_type,
                content,
                message_time,
                skip,
            });
        }
    }

    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewImportConfirm {
        candidate_id,
        conversation_id,
        corrections,
    })
}

/// Raw `POST /api/imports/ocr` body (`ImportOcrRequest`).
#[derive(Deserialize, Default)]
pub struct RawImportOcr {
    #[serde(default)]
    pub candidate_id: Incoming<String>,
    #[serde(default)]
    pub image_path: Incoming<String>,
}

/// Validate a `POST /api/imports/ocr` body (`image_path` is `min_length=1`).
pub fn import_ocr(raw: RawImportOcr) -> Result<NewImportOcr, ContractError> {
    let mut issues = Vec::new();
    let candidate_id = take_optional_string(raw.candidate_id);
    let image_path = take_required_string(&mut issues, "image_path", raw.image_path, None);
    if image_path.is_empty() {
        issues.push(issue(
            "string_too_short",
            "image_path",
            "String should have at least 1 character",
            "",
        ));
    }
    if !issues.is_empty() {
        return failure(issues);
    }
    Ok(NewImportOcr {
        candidate_id,
        image_path,
    })
}

/// Raw `POST /api/import` body (`ImportPayload`): `{data: {<table>: [...]}}`.
#[derive(Deserialize, Default)]
pub struct RawImportPayload {
    #[serde(default)]
    pub data: Incoming<serde_json::Map<String, serde_json::Value>>,
}

/// Validate a `POST /api/import` body. `data` defaults to an empty map.
pub fn import_payload(
    raw: RawImportPayload,
) -> Result<serde_json::Map<String, serde_json::Value>, ContractError> {
    Ok(match raw.data {
        Incoming::Value(map) => map,
        Incoming::Absent | Incoming::Null => serde_json::Map::new(),
    })
}

#[cfg(test)]
mod draft_tests {
    use super::*;

    #[test]
    fn generate_defaults_match_the_python_signature() {
        let raw: RawGenerateRequest = parse(br#"{"candidate_id": "c-1"}"#).unwrap();
        let request = generate_request(raw).unwrap();
        assert_eq!(request.message_type, MessageType::Reply);
        assert_eq!(request.tone, Tone::Friendly);
        assert!(request.include_alternatives);
        assert_eq!(request.max_length, None);
    }

    #[test]
    fn max_length_is_range_checked() {
        // Two bodies of different lengths, so the array elements must be
        // slices rather than fixed-size byte arrays.
        let out_of_range: [&[u8]; 2] = [
            br#"{"candidate_id": "c-1", "max_length": 49}"#,
            br#"{"candidate_id": "c-1", "max_length": 2001}"#,
        ];
        for body in out_of_range {
            let raw: RawGenerateRequest = parse(body).unwrap();
            assert!(
                generate_request(raw).is_err(),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
        let raw: RawGenerateRequest =
            parse(br#"{"candidate_id": "c-1", "max_length": 2000}"#).unwrap();
        assert_eq!(generate_request(raw).unwrap().max_length, Some(2000));
    }

    #[test]
    fn a_draft_patch_ignores_null_and_absent_alike() {
        let raw: RawDraftPatch = parse(br#"{"message_text": null}"#).unwrap();
        let patch = draft_patch(raw).unwrap();
        assert_eq!(patch.message_text, None);
        let raw: RawDraftPatch = parse(br#"{}"#).unwrap();
        let patch = draft_patch(raw).unwrap();
        assert_eq!(patch.message_text, None);
    }

    #[test]
    fn an_empty_message_text_is_a_422() {
        let raw: RawDraftPatch = parse(br#"{"message_text": ""}"#).unwrap();
        let error = draft_patch(raw).unwrap_err();
        let ContractError::ValidationIssues { issues } = error else {
            panic!("expected a 422");
        };
        assert_eq!(issues[0].kind, "string_too_short");
    }
}
