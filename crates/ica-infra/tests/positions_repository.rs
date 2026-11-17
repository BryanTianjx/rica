//! Adapter behaviour against a throwaway SQLite database built from the
//! exported baseline schema.
//!
//! These tests pin the parts the use case delegates to the adapter:
//! archive filtering, case-insensitive search over name/location, the
//! ``created_at DESC`` order, the per-position candidate count and the
//! SQLite timestamp decoding.

mod support;

use ica_application::{PositionFilter, PositionRepository};
use ica_domain::PositionStatus;
use ica_infra::{connect, SqlitePositionRepository};

fn filter(include_archived: bool, search: Option<&str>) -> PositionFilter {
    PositionFilter {
        include_archived,
        search: search.map(str::to_string),
    }
}

#[tokio::test]
async fn list_hides_archived_rows_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository.list(filter(false, None)).await.unwrap();

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["p-3", "p-2", "p-1"]
    );
    assert!(rows.iter().all(|row| !row.archived));
}

#[tokio::test]
async fn list_include_archived_returns_every_row_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository.list(filter(true, None)).await.unwrap();

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["p-4", "p-3", "p-2", "p-1"]
    );
    assert!(rows[0].archived);
}

#[tokio::test]
async fn search_matches_the_name_case_insensitively() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository.list(filter(false, Some("RUST"))).await.unwrap();

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["p-1"]
    );
}

#[tokio::test]
async fn search_matches_the_location_case_insensitively() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository
        .list(filter(false, Some("beijing")))
        .await
        .unwrap();
    // "Beijing" (p-1) matches; the archived "beijing" (p-4) is filtered out.
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["p-1"]
    );

    let with_archived = repository
        .list(filter(true, Some("beijing")))
        .await
        .unwrap();
    assert_eq!(
        with_archived
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["p-4", "p-1"]
    );
}

#[tokio::test]
async fn search_that_matches_nothing_returns_no_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository
        .list(filter(true, Some("zzz-nothing")))
        .await
        .unwrap();

    assert!(rows.is_empty());
}

#[tokio::test]
async fn search_survives_a_null_location_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    // p-3 has a NULL location; a name search on it must still work, and a
    // search that would only match locations must not error on the NULL row.
    let rows = repository.list(filter(true, Some("Data"))).await.unwrap();
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["p-3"]
    );
    assert_eq!(rows[0].location, None);
}

#[tokio::test]
async fn candidate_count_comes_from_the_candidates_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository.list(filter(true, None)).await.unwrap();
    let counts: Vec<(&str, i64)> = rows
        .iter()
        .map(|row| (row.id.as_str(), row.candidate_count))
        .collect();

    assert_eq!(counts, vec![("p-4", 1), ("p-3", 0), ("p-2", 0), ("p-1", 2)]);
}

#[tokio::test]
async fn list_decodes_status_archived_and_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let rows = repository.list(filter(true, None)).await.unwrap();
    let second = &rows[1];

    assert_eq!(second.id, "p-3");
    assert_eq!(second.status, PositionStatus::Draft);
    assert!(!second.archived);
    assert_eq!(second.created_at.to_rfc3339(), "2026-11-12T11:45:00+00:00");

    // Fractional seconds from the DATETIME text survive the decode.
    let p2 = &rows[2];
    assert_eq!(p2.id, "p-2");
    assert_eq!(p2.status, PositionStatus::OnHold);
    assert_eq!(p2.created_at.timestamp_subsec_micros(), 123_456);
}

#[tokio::test]
async fn find_returns_none_for_an_unknown_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    assert_eq!(repository.find("does-not-exist").await.unwrap(), None);
}

#[tokio::test]
async fn find_returns_the_row_with_its_candidate_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let row = repository.find("p-1").await.unwrap().expect("p-1 exists");

    assert_eq!(row.name, "Senior Rust Engineer");
    assert_eq!(row.location.as_deref(), Some("Beijing"));
    assert_eq!(row.salary_min, None);
    assert_eq!(row.status, PositionStatus::Open);
    assert_eq!(row.candidate_count, 2);
}

#[tokio::test]
async fn find_returns_an_archived_row_too() {
    // The Python detail endpoint does not filter on `archived`; only the list
    // endpoint does. A detail read must therefore still return the row.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ica.db");
    support::seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();
    let repository = SqlitePositionRepository::new(pool);

    let row = repository.find("p-4").await.unwrap().expect("p-4 exists");

    assert!(row.archived);
    assert_eq!(row.status, PositionStatus::Closed);
    assert_eq!(row.candidate_count, 1);
}
