//! Test harness for the HTTP layer: a throwaway database seeded from the real
//! baseline schema, plus the router under test.
//!
//! Nothing here can reach the baseline database at /mnt/o/ica/backend/ica.db —
//! every test builds its own file inside a `tempfile::TempDir`.

use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use http_body_util::BodyExt;
use ica_application::PositionRepository;
use ica_infra::{connect, SqlitePositionRepository};

/// The real baseline DDL, exported from the Python database.
const BASELINE_SQL: &str = include_str!("../../../ica-infra/schema/ica-baseline.sql");

/// Build the schema in a fresh file by replaying the exported baseline DDL.
pub async fn create_schema(path: &Path) {
    let pool = connect(&format!("sqlite:///{}?mode=rwc", path.display()))
        .await
        .expect("open temp database");
    sqlx::raw_sql(BASELINE_SQL)
        .execute(&pool)
        .await
        .expect("apply baseline schema");
    pool.close().await;
}

/// Four positions (one archived) and three candidates, with distinct
/// `created_at` values so `created_at DESC` is deterministic.
pub async fn seeded_database(path: &Path) {
    create_schema(path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .expect("open temp database");

    let positions = [
        (
            "p-1",
            "Senior Rust Engineer",
            Some("Beijing"),
            "Open",
            0i64,
            "2026-11-10 09:00:00.000000",
            "2026-11-10 09:30:00.000000",
        ),
        (
            "p-2",
            "Frontend Developer",
            Some("Shanghai"),
            "OnHold",
            0,
            "2026-11-11 10:30:00.123456",
            "2026-11-11 10:30:00.123456",
        ),
        (
            "p-3",
            "Data Analyst",
            None,
            "Draft",
            0,
            "2026-11-12 11:45:00.000000",
            "2026-11-12 11:45:00.000000",
        ),
        (
            "p-4",
            "Archived Recruiter",
            Some("beijing"),
            "Closed",
            1,
            "2026-11-13 12:00:00.500000",
            "2026-11-13 12:00:00.500000",
        ),
    ];
    for (id, name, location, status, archived, created, updated) in positions {
        sqlx::query(concat!(
            "INSERT INTO positions (id, name, description, location, status, archived,",
            " created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(id)
        .bind(name)
        .bind(format!("{name} description"))
        .bind(location)
        .bind(status)
        .bind(archived)
        .bind(created)
        .bind(updated)
        .execute(&pool)
        .await
        .expect("insert position");
    }

    for (id, name, position_id) in [
        ("c-1", "Alice", "p-1"),
        ("c-2", "Bob", "p-1"),
        ("c-3", "Carol", "p-4"),
    ] {
        sqlx::query(concat!(
            "INSERT INTO candidates (id, name, source, position_id, status, archived,",
            " created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(id)
        .bind(name)
        .bind("boss")
        .bind(position_id)
        .bind("New")
        .bind(0i64)
        .bind("2026-11-10 09:05:00.000000")
        .bind("2026-11-10 09:05:00.000000")
        .execute(&pool)
        .await
        .expect("insert candidate");
    }

    pool.close().await;
}

/// Build a router over a freshly seeded temporary database.
///
/// Returns the router and the guard that owns the temporary directory: dropping
/// the guard deletes the database, so the caller must keep it alive.
pub async fn seeded_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    seeded_database(&path).await;
    let pool = connect(&format!("sqlite:///{}?mode=ro", path.display()))
        .await
        .expect("open seeded database read-only");
    let repository: Arc<dyn PositionRepository> = Arc::new(SqlitePositionRepository::new(pool));
    let app = ica_api::router(ica_api::AppState::new(
        repository,
        ica_api::AppConfig {
            app_name: "ICA Personal".to_string(),
            ai_enabled: false,
        },
    ));
    (app, dir)
}

/// One HTTP response, decoded for assertions.
pub struct TestResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

/// Issue a GET and decode the JSON body.
pub async fn get(app: Router, uri: &str) -> TestResponse {
    use tower::ServiceExt;

    let request = axum::http::Request::builder()
        .uri(uri)
        .header("accept", "application/json")
        .body(Body::empty())
        .expect("build request");
    let response = app.oneshot(request).await.expect("router responded");
    let status = response.status().as_u16();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("body is JSON")
    };
    TestResponse { status, body }
}
