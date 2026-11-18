//! Database bootstrap: URL parsing, connection pragmas, read-only opening and
//! the baseline schema snapshot.

mod support;

use chrono::{TimeZone, Utc};
use ica_infra::{connect, parse_sqlite_datetime, table_names, DatabaseUrl};

const EXPECTED_TABLES: [&str; 32] = [
    "ai_model_configs",
    "ai_model_routing",
    "ai_usage_daily",
    "alembic_version",
    "app_settings",
    "audit_events",
    "browser_actions",
    "browser_approvals",
    "browser_candidate_extraction_revisions",
    "browser_candidate_extractions",
    "browser_connector_sessions",
    "browser_connectors",
    "browser_permissions",
    "browser_profiles",
    "browser_sessions",
    "browser_tabs",
    "browser_task_steps",
    "browser_tasks",
    "candidates",
    "conversations",
    "follow_up_policies",
    "follow_up_tasks",
    "generated_drafts",
    "import_records",
    "import_sessions",
    "messages",
    "positions",
    "refresh_tokens",
    "send_records",
    "shares",
    "tenants",
    "users",
];

#[test]
fn parses_python_style_absolute_url() {
    let url = DatabaseUrl::parse("sqlite:////tmp/ica-contract/seed.db").unwrap();

    assert_eq!(url.path, "/tmp/ica-contract/seed.db");
    assert!(!url.read_only);
    assert!(!url.immutable);
}

#[test]
fn parses_relative_url_like_the_python_default() {
    let url = DatabaseUrl::parse("sqlite:///./ica.db").unwrap();

    assert_eq!(url.path, "./ica.db");
}

#[test]
fn parses_mode_ro_and_immutable_query_parameters() {
    let url =
        DatabaseUrl::parse("sqlite:////mnt/o/ica/backend/ica.db?mode=ro&immutable=true").unwrap();

    assert_eq!(url.path, "/mnt/o/ica/backend/ica.db");
    assert!(url.read_only);
    assert!(url.immutable);
}

#[test]
fn rejects_urls_that_are_not_sqlite() {
    let error = DatabaseUrl::parse("postgresql://localhost/ica").unwrap_err();

    assert!(error.to_string().contains("sqlite"), "{error}");
}

#[tokio::test]
async fn baseline_snapshot_rebuilds_every_baseline_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema.db");
    support::create_schema(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();

    let names = table_names(&pool).await.unwrap();

    assert_eq!(names.len(), EXPECTED_TABLES.len());
    for expected in EXPECTED_TABLES {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
}

#[tokio::test]
async fn read_write_connection_turns_on_foreign_keys_and_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rw.db");
    support::create_schema(&path).await;
    let pool = connect(&format!("sqlite:///{}", path.display()))
        .await
        .unwrap();

    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode.to_lowercase(), "wal");
}

#[tokio::test]
async fn read_only_connection_can_query_but_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ro.db");
    support::seeded_database(&path).await;

    let url = format!("sqlite:///{}?mode=ro", path.display());
    let pool = connect(&url).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM positions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 4);

    let write = sqlx::query("DELETE FROM positions WHERE id = 'p-1'")
        .execute(&pool)
        .await;
    assert!(write.is_err(), "a read-only pool must reject writes");
}

#[tokio::test]
async fn immutable_read_only_connection_never_creates_sidecar_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("immutable.db");
    support::seeded_database(&path).await;
    let wal = path.with_file_name("immutable.db-wal");
    let shm = path.with_file_name("immutable.db-shm");
    let wal_before = wal.exists();
    let shm_before = shm.exists();

    let url = format!("sqlite:///{}?mode=ro&immutable=true", path.display());
    let pool = connect(&url).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM positions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 4);

    assert_eq!(wal.exists(), wal_before);
    assert_eq!(shm.exists(), shm_before);
}

#[tokio::test]
async fn truncated_schema_reports_a_missing_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.db");
    // A database with no schema at all: the startup self-check must be able to
    // tell this apart from a healthy database.
    let pool = connect(&format!("sqlite:///{}?mode=rwc", path.display()))
        .await
        .unwrap();

    let names = table_names(&pool).await.unwrap();

    assert!(names.is_empty());
}

#[test]
fn decodes_sqlite_datetime_without_fraction() {
    let parsed = parse_sqlite_datetime("2026-11-10 09:00:00").unwrap();

    assert_eq!(parsed, Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap());
}

#[test]
fn decodes_sqlite_datetime_with_microseconds() {
    let parsed = parse_sqlite_datetime("2026-11-11 10:30:00.123456").unwrap();

    assert_eq!(parsed.timestamp_subsec_micros(), 123_456);
    assert_eq!(
        parsed,
        Utc.with_ymd_and_hms(2026, 11, 11, 10, 30, 0).unwrap()
            + chrono::Duration::microseconds(123_456)
    );
}

#[test]
fn decodes_iso_datetime_with_utc_offset() {
    let parsed = parse_sqlite_datetime("2026-11-11T10:30:00+00:00").unwrap();

    assert_eq!(
        parsed,
        Utc.with_ymd_and_hms(2026, 11, 11, 10, 30, 0).unwrap()
    );
}

#[test]
fn rejects_a_value_that_is_not_a_timestamp() {
    let error = parse_sqlite_datetime("not-a-timestamp").unwrap_err();

    assert!(error.to_string().contains("not-a-timestamp"), "{error}");
}
