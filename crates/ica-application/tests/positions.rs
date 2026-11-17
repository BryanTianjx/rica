//! Use-case behaviour, driven through an in-memory repository double.
//!
//! The port trait is the seam: these tests pin what the use case does with
//! the rows a repository hands back (filtering is the adapter's job, paging
//! and not-found mapping are the use case's job).

mod support;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use ica_application::{
    get_position, list_positions, ApplicationError, ListPositions, PositionFilter,
    PositionRepository, RepositoryError,
};
use ica_domain::{Position, PositionStatus};

struct FakeRepo {
    rows: Vec<Position>,
    failure: Option<String>,
}

impl FakeRepo {
    fn with_rows(count: usize) -> Self {
        let rows = (0..count)
            .map(|index| Position {
                id: format!("pos-{index}"),
                name: format!("Position {index}"),
                description: None,
                location: None,
                skill_requirements: None,
                experience_requirements: None,
                education_requirements: None,
                salary_min: None,
                salary_max: None,
                notes: None,
                status: PositionStatus::Open,
                archived: false,
                created_at: Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap(),
                candidate_count: 0,
            })
            .collect();
        Self {
            rows,
            failure: None,
        }
    }

    fn failing(message: &str) -> Self {
        Self {
            rows: Vec::new(),
            failure: Some(message.to_string()),
        }
    }
}

#[async_trait]
impl PositionRepository for FakeRepo {
    async fn list(&self, _filter: PositionFilter) -> Result<Vec<Position>, RepositoryError> {
        if let Some(message) = &self.failure {
            return Err(RepositoryError::unavailable(message.clone()));
        }
        Ok(self.rows.clone())
    }

    async fn find(&self, id: &str) -> Result<Option<Position>, RepositoryError> {
        if let Some(message) = &self.failure {
            return Err(RepositoryError::unavailable(message.clone()));
        }
        Ok(self.rows.iter().find(|row| row.id == id).cloned())
    }
}

fn query(offset: usize, limit: usize) -> ListPositions {
    ListPositions {
        include_archived: false,
        search: None,
        offset,
        limit,
    }
}

#[test]
fn list_returns_the_requested_window_and_the_full_total() {
    let repo = FakeRepo::with_rows(6);

    let page = support::block_on(list_positions(&repo, query(2, 2))).unwrap();

    assert_eq!(page.total, 6);
    assert_eq!(page.offset, 2);
    assert_eq!(page.limit, 2);
    assert_eq!(
        page.items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["pos-2", "pos-3"]
    );
    assert!(page.has_more());
}

#[test]
fn list_with_offset_past_total_returns_no_items_but_keeps_total() {
    let repo = FakeRepo::with_rows(6);

    let page = support::block_on(list_positions(&repo, query(100, 50))).unwrap();

    assert!(page.items.is_empty());
    assert_eq!(page.total, 6);
    assert!(!page.has_more());
}

#[test]
fn list_on_empty_table_is_an_empty_page() {
    let repo = FakeRepo::with_rows(0);

    let page = support::block_on(list_positions(&repo, query(0, 50))).unwrap();

    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert!(!page.has_more());
}

#[test]
fn list_maps_a_repository_failure_to_unavailable() {
    let repo = FakeRepo::failing("database is locked");

    let error = support::block_on(list_positions(&repo, query(0, 50))).unwrap_err();

    match error {
        ApplicationError::Unavailable { message } => {
            assert_eq!(message, "database is locked");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn get_returns_the_position_when_present() {
    let repo = FakeRepo::with_rows(3);

    let position = support::block_on(get_position(&repo, "pos-1")).unwrap();

    assert_eq!(position.id, "pos-1");
    assert_eq!(position.name, "Position 1");
}

#[test]
fn get_missing_position_reports_the_python_not_found_message() {
    let repo = FakeRepo::with_rows(3);

    let error = support::block_on(get_position(&repo, "does-not-exist")).unwrap_err();

    // app/application/common.py::get_or_404 — f"{label} '{entity_id}' was not found"
    assert_eq!(
        error,
        ApplicationError::NotFound {
            message: "Position 'does-not-exist' was not found".to_string()
        }
    );
}

#[test]
fn get_maps_a_repository_failure_to_unavailable() {
    let repo = FakeRepo::failing("disk I/O error");

    let error = support::block_on(get_position(&repo, "pos-0")).unwrap_err();

    assert_eq!(
        error,
        ApplicationError::Unavailable {
            message: "disk I/O error".to_string()
        }
    );
}
