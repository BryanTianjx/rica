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
    CandidateRepository, ConversationRepository, DashboardRepository, DataSnapshotRepository,
    DraftGenerator, DraftRepository, ExportSealer, FollowUpRepository, ImportRepository,
    MessageRepository, OcrEngine, PositionRepository, SendRecordRepository,
};
use ica_infra::drafts::NotConfiguredDraftGenerator;
use ica_infra::export_crypto::AesGcmSealer;
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
        Arc::new(store.clone()) as Arc<dyn SendRecordRepository>,
        Arc::new(store.clone()) as Arc<dyn FollowUpRepository>,
        Arc::new(store.clone()) as Arc<dyn DashboardRepository>,
        Arc::new(store.clone()) as Arc<dyn DraftRepository>,
        Arc::new(NotConfiguredDraftGenerator) as Arc<dyn DraftGenerator>,
        Arc::new(store.clone()) as Arc<dyn ImportRepository>,
        Arc::new(store.clone()) as Arc<dyn ica_application::BrowserExtractionRepository>,
        Arc::new(store.clone()) as Arc<dyn DataSnapshotRepository>,
        Arc::new(store.clone()) as Arc<dyn ica_application::AuditTrailRepository>,
        Arc::new(store.clone()) as Arc<dyn ica_application::AppSettingsRepository>,
        Arc::new(store.clone()) as Arc<dyn ica_application::AiModelRepository>,
        Arc::new(store.clone()) as Arc<dyn ica_application::AiUsageRepository>,
        Arc::new(ica_infra::secret_store::MemorySecretStore::new())
            as Arc<dyn ica_application::SecretStore>,
        Arc::new(ica_infra::backup::BackupManager::new(
            path.parent()
                .unwrap_or(std::path::Path::new("."))
                .join("ica.db"),
            path.parent()
                .unwrap_or(std::path::Path::new("."))
                .join("backups"),
            10,
        )) as Arc<dyn ica_application::BackupStore>,
        Arc::new(store.clone()) as Arc<dyn ica_application::AuthRepository>,
        Arc::new(ica_infra::auth_store::AuthCrypto {
            jwt_secret: String::new(),
            access_token_minutes: 15,
            refresh_token_days: 7,
            bcrypt_cost: 4,
        }) as Arc<dyn ica_application::AuthSecurity>,
        Arc::new(std::sync::RwLock::new(false)),
        Arc::new(std::sync::RwLock::new(ica_application::AiBudgetSettings {
            daily_budget_cents: 0,
            warning_threshold: 90,
        })),
        Arc::new(AesGcmSealer) as Arc<dyn ExportSealer>,
        Arc::new(ica_infra::ocr_rapid::RapidOcrEngine) as Arc<dyn OcrEngine>,
        AppConfig {
            app_name: "ICA Personal".to_string(),
            ai_enabled: false,
            // The OCR staging area lives inside the same per-test temporary
            // directory as the database — never a hardcoded POSIX path, so
            // the fixture works on Windows and WSL alike (DEF-001).
            ocr_upload_dir: path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("ocr-uploads"),
            ai_provider: "openai_compatible".to_string(),
            ai_model: "glm-4-flash".to_string(),
            ai_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            ai_include_candidate_name: false,
            database_url: format!("sqlite:///{}", path.display()),
            backup_dir: path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("backups")
                .display()
                .to_string(),
            backup_keep_count: 10,
            data_retention_days: 180,
            ai_max_tokens: 1500,
            ai_timeout: 60,
            ai_temperature: 0.4,
            ai_daily_budget_cents: 0,
            ai_budget_warning_threshold: 90,
            database_path: format!("{}", path.display()),
            jwt_secret: String::new(),
            jwt_access_token_expire_minutes: 15,
            jwt_refresh_token_expire_days: 7,
            bcrypt_cost: 4,
        },
    ))
}

/// Install a tracing subscriber once per test binary, so a 500's real cause
/// is visible when a gate fails (the handler logs it, never echoes it).
pub fn init_test_logging() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt().try_init();
    });
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

/// Issue a POST with extra headers (the encrypted-export path needs
/// `X-Encrypt-Aes-Key`).
pub async fn post_with_headers(
    app: Router,
    uri: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> TestResponse {
    let mut builder = axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("accept", "application/json")
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let request = builder
        .body(Body::from(body.to_string()))
        .expect("build request");
    send(app, request).await
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

/// `PUT` with a JSON body — the ai-key endpoints' verb.
pub async fn put(app: Router, uri: &str, body: &str) -> TestResponse {
    let request = axum::http::Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("build request");
    send(app, request).await
}

/// Provision the local identity (default tenant + synthetic user), mirroring
/// a deployed local-mode installation: the sharing endpoints' owner FK needs
/// a `users` row for the synthetic local actor, and a fresh database does
/// not have one (the baseline's empty local DB fails the same way).
pub async fn seed_local_identity(pool: &sqlx::SqlitePool) {
    let now = "2026-11-21 00:00:00.000000";
    sqlx::query(concat!(
        "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at)",
        " VALUES ('00000000-0000-0000-0000-000000000000', 'Local', ?, ?)"
    ))
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed tenant");
    sqlx::query(concat!(
        "INSERT OR IGNORE INTO users (id, tenant_id, email, password_hash, display_name,",
        " role, is_active, created_at, updated_at)",
        " VALUES ('00000000-0000-0000-0000-000000000001',",
        " '00000000-0000-0000-0000-000000000000', '', '', 'Local User', 'admin', 1, ?, ?)"
    ))
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed local user");
}
