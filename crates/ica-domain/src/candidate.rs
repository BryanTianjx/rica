//! Candidate entity and pipeline enumeration.

use chrono::{DateTime, Utc};

/// Candidate lifecycle (`app/domain/enums.py::CandidateStatus`).
///
/// Transitions are driven by user actions only; AI output never advances this
/// state machine. The stored values are capitalised except `ReadNoReply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateStatus {
    New,
    Contacted,
    ReadNoReply,
    Replied,
    Active,
    Interview,
    Rejected,
    Withdrawn,
    Closed,
}

impl CandidateStatus {
    /// Value stored in `candidates.status` and returned on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateStatus::New => "New",
            CandidateStatus::Contacted => "Contacted",
            CandidateStatus::ReadNoReply => "ReadNoReply",
            CandidateStatus::Replied => "Replied",
            CandidateStatus::Active => "Active",
            CandidateStatus::Interview => "Interview",
            CandidateStatus::Rejected => "Rejected",
            CandidateStatus::Withdrawn => "Withdrawn",
            CandidateStatus::Closed => "Closed",
        }
    }

    /// Parse a stored or requested value; `None` means "not a known status".
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "New" => Some(CandidateStatus::New),
            "Contacted" => Some(CandidateStatus::Contacted),
            "ReadNoReply" => Some(CandidateStatus::ReadNoReply),
            "Replied" => Some(CandidateStatus::Replied),
            "Active" => Some(CandidateStatus::Active),
            "Interview" => Some(CandidateStatus::Interview),
            "Rejected" => Some(CandidateStatus::Rejected),
            "Withdrawn" => Some(CandidateStatus::Withdrawn),
            "Closed" => Some(CandidateStatus::Closed),
            _ => None,
        }
    }

    /// True when no ordinary follow-up should be suggested.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            CandidateStatus::Rejected | CandidateStatus::Withdrawn | CandidateStatus::Closed
        )
    }
}

/// A candidate, as read from `candidates` plus its derived read fields.
///
/// `position_name`, `message_count` and `active_follow_up_count` are not
/// columns: the read model resolves the position name through the foreign key
/// and counts messages / contact-bearing follow-ups. They live on the entity so
/// every branch that returns a candidate fills them the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
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
    pub status: CandidateStatus,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub position_name: Option<String>,
    pub message_count: i64,
    pub active_follow_up_count: i64,
}
