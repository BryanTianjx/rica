//! Manual send-record entity and its outcome enumeration.
//!
//! ICA never sends a message itself: the user sends in the external channel and
//! reports back here. The table is append-only — a second attempt is a second
//! row — and `content_summary` is a summary, never the full body.

use chrono::{DateTime, Utc};

/// Manually recorded communication outcome (`ResultType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultType {
    Sent,
    NotSent,
    CandidateReplied,
    CandidateNoReply,
    CandidateDeclined,
    Interview,
    Other,
}

impl ResultType {
    pub fn as_str(self) -> &'static str {
        match self {
            ResultType::Sent => "sent",
            ResultType::NotSent => "not_sent",
            ResultType::CandidateReplied => "candidate_replied",
            ResultType::CandidateNoReply => "candidate_no_reply",
            ResultType::CandidateDeclined => "candidate_declined",
            ResultType::Interview => "interview",
            ResultType::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "sent" => Some(ResultType::Sent),
            "not_sent" => Some(ResultType::NotSent),
            "candidate_replied" => Some(ResultType::CandidateReplied),
            "candidate_no_reply" => Some(ResultType::CandidateNoReply),
            "candidate_declined" => Some(ResultType::CandidateDeclined),
            "interview" => Some(ResultType::Interview),
            "other" => Some(ResultType::Other),
            _ => None,
        }
    }

    /// Outcomes that mean a message actually reached the candidate.
    pub fn indicates_contact(self) -> bool {
        matches!(
            self,
            ResultType::Sent
                | ResultType::CandidateReplied
                | ResultType::CandidateDeclined
                | ResultType::Interview
        )
    }
}

/// One manually performed send attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendRecord {
    pub id: String,
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub channel: String,
    pub target: Option<String>,
    pub content_summary: Option<String>,
    pub status: String,
    pub sent_at: DateTime<Utc>,
    pub note: Option<String>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}
