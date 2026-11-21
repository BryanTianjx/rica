//! Test fixtures: a throwaway database built from the exported baseline
//! schema, plus a seeded variant.
//!
//! Each integration test binary compiles this module for itself and uses only
//! part of it, so unused helpers here are expected rather than dead code.
#![allow(dead_code)]
//!
//! Every test gets its own ``tempfile::TempDir``; nothing here can reach the
//! baseline database at /mnt/o/ica/backend/ica.db.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

/// The real baseline DDL, exported from the Python database.
pub const BASELINE_SQL: &str = include_str!("../../schema/ica-baseline.sql");

/// Create a database at ``path`` by replaying the baseline schema snapshot.
pub async fn create_schema(path: &Path) {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .expect("valid sqlite path")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open temp database");
    for statement in BASELINE_SQL.split(";\n") {
        let trimmed = statement.trim();
        if trimmed.is_empty() || !trimmed.contains("CREATE ") {
            continue;
        }
        if let Some(start) = trimmed.find("CREATE ") {
            let ddl = &trimmed[start..];
            sqlx::raw_sql(ddl)
                .execute(&pool)
                .await
                .unwrap_or_else(|error| panic!("apply DDL failed: {error}\n{ddl}"));
        }
    }
    sqlx::raw_sql("PRAGMA journal_mode=DELETE;")
        .execute(&pool)
        .await
        .expect("checkpoint journal mode");
    pool.close().await;
}

/// Create + seed: 4 positions (one archived), candidates for counts.
///
/// Ordering matters: ``created_at`` is distinct per row so the DESC ordering
/// assertion is deterministic.
pub async fn seeded_database(path: &Path) {
    create_schema(path).await;
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .expect("valid sqlite path")
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open temp database");

    let positions = [
        (
            "p-1",
            "Senior Rust Engineer",
            "Beijing",
            "Open",
            0,
            "2026-11-10 09:00:00.000000",
            "2026-11-10 09:30:00.000000",
        ),
        (
            "p-2",
            "Frontend Developer",
            "Shanghai",
            "OnHold",
            0,
            "2026-11-11 10:30:00.123456",
            "2026-11-11 10:30:00.123456",
        ),
        (
            "p-3",
            "Data Analyst",
            // NULL location: the search filter must not choke on it.
            "",
            "Draft",
            0,
            "2026-11-12 11:45:00.000000",
            "2026-11-12 11:45:00.000000",
        ),
        (
            "p-4",
            "Archived Recruiter",
            "beijing",
            "Closed",
            1,
            "2026-11-13 12:00:00.500000",
            "2026-11-13 12:00:00.500000",
        ),
    ];
    for (id, name, location, status, archived, created, updated) in positions {
        let location: Option<&str> = if location.is_empty() {
            None
        } else {
            Some(location)
        };
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

    // p-1 -> 2 candidates, p-4 -> 1 candidate, others 0.
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
        .bind(0)
        .bind("2026-11-10 09:05:00.000000")
        .bind("2026-11-10 09:05:00.000000")
        .execute(&pool)
        .await
        .expect("insert candidate");
    }

    pool.close().await;
}
