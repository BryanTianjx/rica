//! Draft domain: entities, enumerations and state rules — the port of the
//! draft-related parts of `app/domain/enums.py` plus the `GeneratedDraft`
//! model's wire-relevant fields.
//!
//! Semantic red line (preserved from the baseline docstrings): `approved` only
//! means the wording was accepted, `copied` only means the text reached the
//! clipboard — **neither ever implies the message was sent**.

use chrono::{DateTime, Utc};

/// The draft lifecycle (case-sensitive lower-case strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftStatus {
    Generated,
    Edited,
    Approved,
    Rejected,
    Copied,
}

impl DraftStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Edited => "edited",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Copied => "copied",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "generated" => Self::Generated,
            "edited" => Self::Edited,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "copied" => Self::Copied,
            _ => return None,
        })
    }

    /// The draft validator's verdict (case-sensitive capitalised strings).
    pub fn validation_status_str() -> &'static str {
        "Passed"
    }
}

/// The validator's verdict values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStatus {
    Passed,
    Warning,
    Blocked,
}

impl ValidationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "Passed",
            Self::Warning => "Warning",
            Self::Blocked => "Blocked",
        }
    }
}

/// The communication goal of a generated draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Greeting,
    Reply,
    FollowUpQuestion,
    PositionIntro,
    SalaryTalk,
    InterviewInvite,
    PostInterview,
    PoliteFollowUp,
    DeclineAck,
}

impl MessageType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Greeting => "greeting",
            Self::Reply => "reply",
            Self::FollowUpQuestion => "follow_up_question",
            Self::PositionIntro => "position_intro",
            Self::SalaryTalk => "salary_talk",
            Self::InterviewInvite => "interview_invite",
            Self::PostInterview => "post_interview",
            Self::PoliteFollowUp => "polite_follow_up",
            Self::DeclineAck => "decline_ack",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "greeting" => Self::Greeting,
            "reply" => Self::Reply,
            "follow_up_question" => Self::FollowUpQuestion,
            "position_intro" => Self::PositionIntro,
            "salary_talk" => Self::SalaryTalk,
            "interview_invite" => Self::InterviewInvite,
            "post_interview" => Self::PostInterview,
            "polite_follow_up" => Self::PoliteFollowUp,
            "decline_ack" => Self::DeclineAck,
            _ => return None,
        })
    }

    /// The expected list in pydantic's enum error, verbatim.
    pub const EXPECTED_LIST: &'static str = "'greeting', 'reply', 'follow_up_question', \
         'position_intro', 'salary_talk', 'interview_invite', 'post_interview', \
         'polite_follow_up' or 'decline_ack'";
}

/// The requested writing style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Formal,
    Friendly,
    Concise,
    Warm,
}

impl Tone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Formal => "formal",
            Self::Friendly => "friendly",
            Self::Concise => "concise",
            Self::Warm => "warm",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "formal" => Self::Formal,
            "friendly" => Self::Friendly,
            "concise" => Self::Concise,
            "warm" => Self::Warm,
            _ => return None,
        })
    }

    /// The expected list in pydantic's enum error, verbatim.
    pub const EXPECTED_LIST: &'static str = "'formal', 'friendly', 'concise' or 'warm'";
}

/// One generated draft — the `generated_drafts` table's wire-relevant fields.
/// The JSON-backed list columns are carried as strings; the DTO expands them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedDraft {
    pub id: String,
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub message_type: String,
    pub tone: String,
    pub status: DraftStatus,
    pub objective: Option<String>,
    pub message_text: String,
    pub alternatives_json: String,
    pub context_references_json: String,
    pub uncertainties_json: String,
    pub warnings_json: String,
    pub validation_status: String,
    pub validation_messages_json: String,
    pub quality_status: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub is_edited: bool,
    pub approved_at: Option<DateTime<Utc>>,
    pub copied_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub draft_group_id: Option<String>,
    pub cost_cents: Option<i64>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    /// The routing decision blob (`routing_decision_json`), echoed verbatim in
    /// the API response and projected into `routing_tier`/`routing_reason`.
    pub routing_decision_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
