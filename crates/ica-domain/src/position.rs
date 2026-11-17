//! Position entity and lifecycle enumeration.

use chrono::{DateTime, Utc};

/// Lifecycle of a job position.
///
/// The wire/DB strings come from ``app/domain/enums.py::PositionStatus`` and
/// are capitalised; only the exact values are accepted when reading a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PositionStatus {
    Draft,
    Open,
    OnHold,
    Closed,
}

impl PositionStatus {
    /// The value stored in the ``positions.status`` column and returned by the
    /// API.
    pub fn as_str(self) -> &'static str {
        match self {
            PositionStatus::Draft => "Draft",
            PositionStatus::Open => "Open",
            PositionStatus::OnHold => "OnHold",
            PositionStatus::Closed => "Closed",
        }
    }

    /// Parse a stored value. ``None`` means the row holds a value the domain
    /// does not know, which is a data defect rather than a new state.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "Draft" => Some(PositionStatus::Draft),
            "Open" => Some(PositionStatus::Open),
            "OnHold" => Some(PositionStatus::OnHold),
            "Closed" => Some(PositionStatus::Closed),
            _ => None,
        }
    }

    /// Closed positions stop follow-up suggestions.
    pub fn blocks_follow_up(self) -> bool {
        matches!(self, PositionStatus::Closed)
    }
}

/// A job position, as read from the ``positions`` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
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
    pub status: PositionStatus,
    pub archived: bool,
    /// Always UTC: SQLite stores no offset, and the API renders an explicit
    /// ``Z`` so a browser cannot read the value as local time.
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Number of ``candidates`` rows pointing at this position.
    pub candidate_count: i64,
}
