//! DEF-002: the migration runner's observable rules, verified against a
//! throwaway baseline database (same fixture style as `database.rs`).

mod support;

use ica_infra::migrations::{
    apply_migrations, apply_pending_migrations, Migration, MigrationError, HISTORY_TABLE,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

/// One real migration for the tests that need actual DDL to run, plus a
/// deliberately broken one to prove atomicity.
fn test_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create a probe table",
            sql: "CREATE TABLE migration_probe (id INTEGER PRIMARY KEY);",
        },
        Migration {
            version: 2,
            description: "a second statement runs in the same transaction",
            sql: "INSERT INTO migration_probe (id) VALUES (1);",
        },
    ]
}

async fn open(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .expect("valid sqlite path");
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open temp database")
}

async fn history_versions(pool: &SqlitePool) -> Vec<i64> {
    let rows: Vec<(i64,)> = sqlx::query_as(&format!(
        "SELECT version FROM {HISTORY_TABLE} ORDER BY version"
    ))
    .fetch_all(pool)
    .await
    .expect("read history");
    rows.into_iter().map(|(version,)| version).collect()
}

#[tokio::test]
async fn a_baseline_vintage_database_is_stamped_then_migrated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    support::create_schema(&path).await;
    let pool = open(&path).await;

    let mut connection = pool.acquire().await.expect("connection");
    let applied = apply_migrations(&mut connection, &test_migrations())
        .await
        .expect("migrations applied");
    assert_eq!(applied, vec![1, 2], "both pending versions ran in order");
    drop(connection);

    assert_eq!(
        history_versions(&pool).await,
        vec![0, 1, 2],
        "the baseline vintage is stamped 0, then the pending versions"
    );
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM migration_probe")
        .fetch_one(&pool)
        .await
        .expect("probe table exists");
    assert_eq!(count, 1, "the migration's DDL and DML both ran");
}

#[tokio::test]
async fn re_applying_is_a_no_op() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    support::create_schema(&path).await;
    let pool = open(&path).await;

    let mut connection = pool.acquire().await.expect("connection");
    apply_migrations(&mut connection, &test_migrations())
        .await
        .expect("first run");
    let second = apply_migrations(&mut connection, &test_migrations())
        .await
        .expect("second run");
    assert!(second.is_empty(), "nothing left to apply");
    drop(connection);
    assert_eq!(history_versions(&pool).await, vec![0, 1, 2]);
}

#[tokio::test]
async fn a_failing_migration_leaves_no_partial_state() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    support::create_schema(&path).await;
    let pool = open(&path).await;

    let broken = vec![
        Migration {
            version: 1,
            description: "valid first step",
            sql: "CREATE TABLE migration_probe (id INTEGER PRIMARY KEY);",
        },
        Migration {
            version: 2,
            description: "breaks halfway",
            sql: "INSERT INTO migration_probe (id) VALUES (1); \
                  INSERT INTO missing_table (id) VALUES (2);",
        },
    ];
    let mut connection = pool.acquire().await.expect("connection");
    let error = apply_migrations(&mut connection, &broken)
        .await
        .expect_err("the broken migration is refused");
    assert!(
        matches!(error, MigrationError::Database { version: 2, .. }),
        "the error names the failing version: {error}"
    );
    drop(connection);

    // v1 committed (each migration is its own transaction), v2 left nothing
    // behind: neither its DML nor a history row.
    assert_eq!(history_versions(&pool).await, vec![0, 1]);
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM migration_probe")
        .fetch_one(&pool)
        .await
        .expect("probe table");
    assert_eq!(count, 0, "v2's INSERT rolled back with its transaction");
}

#[tokio::test]
async fn a_database_newer_than_the_binary_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    support::create_schema(&path).await;
    let pool = open(&path).await;

    sqlx::query(&format!(
        "CREATE TABLE {HISTORY_TABLE} (version INTEGER PRIMARY KEY, \
         description TEXT NOT NULL, applied_at TEXT NOT NULL)"
    ))
    .execute(&pool)
    .await
    .expect("create history");
    sqlx::query(&format!(
        "INSERT INTO {HISTORY_TABLE} (version, description, applied_at) VALUES (99, 'from the future', 'x')"
    ))
    .execute(&pool)
    .await
    .expect("insert future version");

    let mut connection = pool.acquire().await.expect("connection");
    let error = apply_migrations(&mut connection, &test_migrations())
        .await
        .expect_err("a newer database is refused");
    assert!(
        matches!(error, MigrationError::DatabaseNewerThanBinary { .. }),
        "the error is the newer-database guard: {error}"
    );
}

#[tokio::test]
async fn a_database_without_the_baseline_tables_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("empty.db");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
                .expect("valid path")
                .create_if_missing(true),
        )
        .await
        .expect("open empty database");

    let mut connection = pool.acquire().await.expect("connection");
    let error = apply_migrations(&mut connection, &test_migrations())
        .await
        .expect_err("a non-baseline database is refused");
    assert!(
        matches!(error, MigrationError::NotABaselineDatabase),
        "the error is the baseline guard: {error}"
    );
}

#[tokio::test]
async fn the_canonical_list_applies_cleanly_on_the_baseline() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    support::create_schema(&path).await;
    let pool = open(&path).await;

    let mut connection = pool.acquire().await.expect("connection");
    let applied = apply_pending_migrations(&mut connection)
        .await
        .expect("canonical migrations apply");
    assert_eq!(applied, vec![1], "the canonical v1 index migration ran");
    drop(connection);

    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'index' AND name = 'ix_audit_events_event_type'",
    )
    .fetch_one(&pool)
    .await
    .expect("query index");
    assert_eq!(count, 1, "the audit-trail index exists");
}
