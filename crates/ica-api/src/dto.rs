//! Response bodies, serialised exactly like the Python schemas.
//!
//! The field set and the timestamp rendering are the contract: a field added
//! here without the Python schema gaining it first is a wire-format break, and
//! a naive timestamp (no `Z`) makes every browser read the value as local time.

use chrono::{DateTime, SecondsFormat, Utc};
use ica_domain::Position;
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
    /// Wrap an already-sliced page.
    ///
    /// `has_more` counts the rows actually returned, not `limit`, so a short
    /// last page reports `false` — the same arithmetic as the Python
    /// `Page.has_more` computed field.
    pub fn new(items: Vec<T>, total: usize, offset: usize, limit: usize) -> Self {
        let has_more = offset + items.len() < total;
        Self {
            items,
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
        let page: PageResponse<u8> = PageResponse::new(vec![1, 2], 2, 0, 50);
        assert!(!page.has_more);
    }

    #[test]
    fn a_full_page_reports_more_rows() {
        let page: PageResponse<u8> = PageResponse::new(vec![1, 2], 4, 0, 2);
        assert!(page.has_more);
    }

    #[test]
    fn the_last_partial_page_reports_no_more_rows() {
        // offset + returned < total would be true if `limit` were used here,
        // but the page itself is exhausted.
        let page: PageResponse<u8> = PageResponse::new(vec![3], 3, 2, 2);
        assert!(!page.has_more);
    }
}
