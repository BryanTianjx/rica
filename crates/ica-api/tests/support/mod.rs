//! Test harness for the HTTP layer: throwaway databases seeded from the real
//! baseline schema, plus the router under test.
//!
//! Each integration test binary compiles this module for itself and uses only
//! part of it, so unused helpers here are expected rather than dead code.
#![allow(dead_code)]
//!
//! Nothing here can reach the baseline database at /mnt/o/ica/backend/ica.db —
//! every test builds its own file inside a `tempfile::TempDir`.
//!
//! Two seeds exist:
//!
//! * [`seeded_database`] — R0's four positions and three candidates, kept
//!   byte-compatible with the R0 contract tests.
//! * [`seeded_w1_database`] — the same fixture plus conversations, messages,
//!   send records, follow-up tasks and drafts, so the W1 write paths and the
//!   derived counters have something to operate on.

use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use http_body_util::BodyExt;
use ica_api::{AppConfig, AppState};
use ica_application::{
    CandidateRepository, ConversationRepository, MessageRepository, PositionRepository,
    SendRecordRepository,
};
use ica_infra::{connect, SqliteStore};

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

/// R0 fixture: four positions (one archived) and three candidates, with
/// distinct `created_at` values so `created_at DESC` is deterministic.
pub async fn insert_r0_fixture(path: &Path) {
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

/// R0 seed: schema + the four-position fixture.
pub async fn seeded_database(path: &Path) {
    create_schema(path).await;
    insert_r0_fixture(path).await;
}

/// W1 seed: the R0 fixture plus conversations, messages, send records,
/// follow-up tasks and drafts.
///
/// Fixed timestamps everywhere so `ORDER BY` is deterministic and the derived
/// `message_count` / `active_follow_up_count` values are knowable in a test.
pub async fn seeded_w1_database(path: &Path) {
    create_schema(path).await;
    insert_r0_fixture(path).await;

    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .expect("open temp database");

    // Alice has an active conversation; Bob's is inactive (so an implicit
    // message must open a new one instead of reusing it); Carol has none.
    for (id, candidate_id, channel, is_active, last_message_at) in [
        (
            "k-1",
            "c-1",
            "boss",
            1i64,
            Some("2026-11-12 18:00:00.000000"),
        ),
        ("k-2", "c-2", "wx", 0, Some("2026-11-11 08:00:00.000000")),
    ] {
        sqlx::query(concat!(
            "INSERT INTO conversations (id, candidate_id, channel, title, is_active,",
            " last_message_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(id)
        .bind(candidate_id)
        .bind(channel)
        .bind(Option::<String>::None)
        .bind(is_active)
        .bind(last_message_at)
        .bind("2026-11-10 09:10:00.000000")
        .bind("2026-11-10 09:10:00.000000")
        .execute(&pool)
        .await
        .expect("insert conversation");
    }

    // Five messages for Alice, one with a NULL time so
    // `message_time ASC NULLS LAST` is exercised.
    for (id, candidate_id, conversation_id, sender, content, message_time) in [
        (
            "m-1",
            "c-1",
            "k-1",
            Some("Candidate"),
            "msg 0",
            Some("2026-11-10 10:00:00.000000"),
        ),
        (
            "m-2",
            "c-1",
            "k-1",
            Some("Recruiter"),
            "msg 1",
            Some("2026-11-10 10:05:00.000000"),
        ),
        (
            "m-3",
            "c-1",
            "k-1",
            Some("Candidate"),
            "msg 2",
            Some("2026-11-11 11:00:00.000000"),
        ),
        (
            "m-4",
            "c-1",
            "k-1",
            Option::<&str>::None,
            "msg 3",
            Option::<&str>::None,
        ),
        (
            "m-5",
            "c-1",
            "k-1",
            Some("Recruiter"),
            "msg 4",
            Some("2026-11-12 18:00:00.000000"),
        ),
    ] {
        sqlx::query(concat!(
            "INSERT INTO messages (id, candidate_id, conversation_id, sender_type, content,",
            " message_time, channel, source, content_hash, created_at, updated_at)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(id)
        .bind(candidate_id)
        .bind(conversation_id)
        .bind(sender)
        .bind(content)
        .bind(message_time)
        .bind("boss")
        .bind("manual")
        .bind(Option::<String>::None)
        .bind("2026-11-10 10:00:00.000000")
        .bind("2026-11-10 10:00:00.000000")
        .execute(&pool)
        .await
        .expect("insert message");
    }

    // One message for Bob so a candidate filter has a different answer.
    sqlx::query(concat!(
        "INSERT INTO messages (id, candidate_id, conversation_id, sender_type, content,",
        " message_time, channel, source, content_hash, created_at, updated_at)",
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    ))
    .bind("m-6")
    .bind("c-2")
    .bind("k-2")
    .bind(Some("Candidate"))
    .bind("bob msg")
    .bind(Some("2026-11-11 08:00:00.000000"))
    .bind("wx")
    .bind("manual")
    .bind(Option::<String>::None)
    .bind("2026-11-11 08:00:00.000000")
    .bind("2026-11-11 08:00:00.000000")
    .execute(&pool)
    .await
    .expect("insert bob message");

    for (id, candidate_id, status, sent_at) in [
        ("s-1", "c-1", "sent", "2026-11-10 10:00:00.000000"),
        (
            "s-2",
            "c-1",
            "candidate_replied",
            "2026-11-11 10:00:00.000000",
        ),
        ("s-3", "c-1", "not_sent", "2026-11-12 10:00:00.000000"),
        ("s-4", "c-2", "sent", "2026-11-13 10:00:00.000000"),
    ] {
        sqlx::query(concat!(
            "INSERT INTO send_records (id, candidate_id, task_id, draft_id, channel, target,",
            " content_summary, status, sent_at, note, actor, created_at, updated_at)",
            " VALUES (?, ?, NULL, NULL, ?, ?, ?, ?, ?, NULL, ?, ?, ?)"
        ))
        .bind(id)
        .bind(candidate_id)
        .bind("boss")
        .bind("target")
        .bind("summary")
        .bind(status)
        .bind(sent_at)
        .bind("local_user")
        .bind(sent_at)
        .bind(sent_at)
        .execute(&pool)
        .await
        .expect("insert send record");
    }

    // Alice: one contact-bearing task (Completed) and one that is not a contact
    // (Pending); Bob: one Completed task.
    for (id, candidate_id, status) in [
        ("t-1", "c-1", "Completed"),
        ("t-2", "c-1", "Pending"),
        ("t-3", "c-2", "Completed"),
    ] {
        sqlx::query(concat!(
            "INSERT INTO follow_up_tasks (id, candidate_id, position_id, status, objective,",
            " reason, block_reason, recommended_at, expires_at, last_follow_up_at,",
            " follow_up_count, completed_at, result_type, result_note, result_recorded_at,",
            " idempotency_key, created_at, updated_at) VALUES (?, ?, NULL, ?, NULL, NULL, NULL,",
            " NULL, NULL, NULL, 0, NULL, NULL, NULL, NULL, NULL, ?, ?)"
        ))
        .bind(id)
        .bind(candidate_id)
        .bind(status)
        .bind("2026-11-10 09:00:00.000000")
        .bind("2026-11-10 09:00:00.000000")
        .execute(&pool)
        .await
        .expect("insert follow-up task");
    }

    // Alice: one approved draft (sendable) and one generated draft (not).
    for (id, candidate_id, status) in [
        ("d-1", "c-1", "approved"),
        ("d-2", "c-1", "generated"),
        ("d-3", "c-2", "approved"),
    ] {
        sqlx::query(concat!(
            "INSERT INTO generated_drafts (id, candidate_id, task_id, message_type, tone,",
            " status, objective, message_text, alternatives_json, context_references_json,",
            " uncertainties_json, warnings_json, validation_status, validation_messages_json,",
            " quality_status, provider, model, prompt_version, is_edited, approved_at,",
            " copied_at, rejected_at, created_at, updated_at) VALUES (?, ?, NULL, 'reply',",
            " 'friendly', ?, NULL, 'draft body', NULL, NULL, NULL, NULL, 'Passed', NULL, NULL,",
            " NULL, NULL, NULL, 0, NULL, NULL, NULL, ?, ?)"
        ))
        .bind(id)
        .bind(candidate_id)
        .bind(status)
        .bind("2026-11-10 09:00:00.000000")
        .bind("2026-11-10 09:00:00.000000")
        .execute(&pool)
        .await
        .expect("insert draft");
    }

    pool.close().await;
}

/// Build a router over a freshly seeded temporary database (R0 fixture).
///
/// Returns the router and the guard that owns the temporary directory: dropping
/// the guard deletes the database, so the caller must keep it alive.
pub async fn seeded_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    seeded_database(&path).await;
    let app = app_for(&path).await;
    (app, dir)
}

/// Build a router over a temporary database with the W1 fixture, opened
/// read-write so write endpoints work.
pub async fn w1_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    seeded_w1_database(&path).await;
    let app = app_for(&path).await;
    (app, dir)
}

/// Wire the router over `path` (read-write).
pub async fn app_for(path: &Path) -> Router {
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .expect("open seeded database");
    let store = SqliteStore::new(pool);
    ica_api::router(AppState::new(
        Arc::new(store.clone()) as Arc<dyn PositionRepository>,
        Arc::new(store.clone()) as Arc<dyn CandidateRepository>,
        Arc::new(store.clone()) as Arc<dyn ConversationRepository>,
        Arc::new(store.clone()) as Arc<dyn MessageRepository>,
        Arc::new(store) as Arc<dyn SendRecordRepository>,
        AppConfig {
            app_name: "ICA Personal".to_string(),
            ai_enabled: false,
        },
    ))
}

/// One HTTP response, decoded for assertions.
pub struct TestResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

async fn send(app: Router, request: axum::http::Request<Body>) -> TestResponse {
    use tower::ServiceExt;

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

/// Issue a GET and decode the JSON body.
pub async fn get(app: Router, uri: &str) -> TestResponse {
    let request = axum::http::Request::builder()
        .uri(uri)
        .header("accept", "application/json")
        .body(Body::empty())
        .expect("build request");
    send(app, request).await
}

/// Issue a request with a JSON body.
async fn with_body(app: Router, method: &str, uri: &str, body: &str) -> TestResponse {
    let request = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("build request");
    send(app, request).await
}

pub async fn post(app: Router, uri: &str, body: &str) -> TestResponse {
    with_body(app, "POST", uri, body).await
}

pub async fn patch(app: Router, uri: &str, body: &str) -> TestResponse {
    with_body(app, "PATCH", uri, body).await
}

pub async fn delete(app: Router, uri: &str) -> TestResponse {
    let request = axum::http::Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("accept", "application/json")
        .body(Body::empty())
        .expect("build request");
    send(app, request).await
}

/// Open the same temporary database the router is using, for assertions on the
/// rows a write left behind.
pub async fn pool_for(dir: &tempfile::TempDir) -> sqlx::SqlitePool {
    connect(&format!(
        "sqlite:///{}",
        dir.path().join("ica.db").display()
    ))
    .await
    .expect("open the test database for assertions")
}

/// Count rows matching `sql` with the given text binds.
pub async fn count(pool: &sqlx::SqlitePool, sql: &str, binds: &[&str]) -> i64 {
    let mut query = sqlx::query_scalar::<_, i64>(sql);
    for value in binds {
        query = query.bind(*value);
    }
    query.fetch_one(pool).await.expect("count query")
}
