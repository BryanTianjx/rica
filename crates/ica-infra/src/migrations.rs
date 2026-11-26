//! Versioned schema migrations (DEF-002 / audit DB-02).
//!
//! The baseline database is Alembic-managed on the Python side; this repo
//! ships its frozen DDL as [`crate::schema::ica_baseline`] and, until now, had
//! no way to evolve a database that was already installed. The startup
//! self-check in `ica-server` only verified two tables and refused to run —
//! an upgrade path did not exist.
//!
//! The mechanism is deliberately small:
//!
//! * Rust-owned history table `ica_schema_history` (distinct from Python's
//!   `alembic_version`, so the two tools never fight over one row);
//! * migrations are embedded SQL applied in **declaration order**, each one
//!   in its own transaction together with the history row, so a migration can
//!   never half-apply;
//! * version 0 is implicit: a database that has the baseline tables but no
//!   history table is "baseline vintage" and is stamped, not re-created;
//! * a database whose history names a version this binary does not know is
//!   NEWER than the binary and is refused with a readable error.
//!
//! Read-only connections never migrate: opening the baseline database must
//! leave it byte-identical (see [`crate::db`]), so the runner is only called
//! on a writable handle.

use sqlx::{Connection, SqliteConnection};

/// The Rust-owned history table.
pub const HISTORY_TABLE: &str = "ica_schema_history";

/// One ordered schema migration.
///
/// `sql` may contain multiple statements; it runs verbatim inside one
/// transaction together with the history insert.
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub sql: &'static str,
}

/// The canonical migration list, in application order.
///
/// W5's `/audit-events` filters and sorts on `event_type`/`created_at`; the
/// baseline only indexes `entity_id`/`entity_type`/`created_at`/`actor_id`
/// (see the snapshot header), so the trail scan would read the whole table.
/// This is a performance-only divergence from the frozen DDL: responses and
/// rows are unchanged, which is what the contract parity compares.
pub const CANONICAL_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "ix_audit_events_event_type for the W5 audit trail listing",
        sql: "CREATE INDEX IF NOT EXISTS ix_audit_events_event_type ON audit_events (event_type);",
    },
    Migration {
        version: 2,
        // SEC-P1 §10: structured authorization decisions go to a Rust-owned
        // side table. It is deliberately absent from the frozen baseline DDL
        // and from contract_capture.DB_TABLES, so L2/L4 parity is unaffected.
        description: "authz_decisions side table for structured authorization decisions",
        sql: "CREATE TABLE IF NOT EXISTS authz_decisions (\
              id INTEGER PRIMARY KEY AUTOINCREMENT,\
              request_id VARCHAR(128),\
              actor_id VARCHAR(36),\
              tenant_id VARCHAR(36),\
              action VARCHAR(128) NOT NULL,\
              resource_type VARCHAR(64),\
              decision VARCHAR(16) NOT NULL,\
              reason_code VARCHAR(64),\
              entrypoint VARCHAR(32),\
              scope_kind VARCHAR(16),\
              occurred_at DATETIME NOT NULL);",
    },
];

/// Why the runner refused to proceed.
#[derive(Debug)]
pub enum MigrationError {
    /// sqlx failure with the migration context attached.
    Database {
        version: i64,
        description: &'static str,
        source: sqlx::Error,
    },
    /// The database is newer than this binary: its history names a version
    /// the embedded list does not know.
    DatabaseNewerThanBinary { unknown_versions: Vec<i64> },
    /// The database has no baseline tables at all — `CREATE` is not this
    /// crate's job (the shipped DDL or the Alembic tooling owns that).
    NotABaselineDatabase,
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database {
                version,
                description,
                source,
            } => write!(
                formatter,
                "migration v{version} ({description}) failed: {source}"
            ),
            Self::DatabaseNewerThanBinary { unknown_versions } => write!(
                formatter,
                "the database was migrated by a newer binary: unknown schema \
                 versions {unknown_versions:?}; upgrade this application first"
            ),
            Self::NotABaselineDatabase => write!(
                formatter,
                "the database does not look like an ICA baseline database \
                 (no candidates/positions tables); migrations refuse to run"
            ),
        }
    }
}

impl std::error::Error for MigrationError {}

/// Read the applied versions from the history table, if it exists.
async fn applied_versions(connection: &mut SqliteConnection) -> Result<Vec<i64>, sqlx::Error> {
    let exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(HISTORY_TABLE)
            .fetch_optional(&mut *connection)
            .await?;
    if exists.is_none() {
        return Ok(Vec::new());
    }
    let rows: Vec<(i64,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT version FROM {HISTORY_TABLE} ORDER BY version"
    )))
    .fetch_all(&mut *connection)
    .await?;
    Ok(rows.into_iter().map(|(version,)| version).collect())
}

/// Stamp the baseline vintage and create the history table.
///
/// Runs in one transaction: either the database becomes a tracked one or it
/// stays untouched.
async fn stamp_baseline(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let mut transaction = connection.begin().await?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE IF NOT EXISTS {HISTORY_TABLE} (\
             version INTEGER PRIMARY KEY, \
             description TEXT NOT NULL, \
             applied_at TEXT NOT NULL)"
    )))
    .execute(&mut *transaction)
    .await?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT OR IGNORE INTO {HISTORY_TABLE} (version, description, applied_at) \
         VALUES (0, 'baseline snapshot (ica-baseline.sql)', '{}')",
        crate::store::to_sql_timestamp(&chrono::Utc::now())
    )))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

/// Whether the database carries the baseline's core tables.
async fn has_baseline_tables(connection: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name IN ('candidates', 'positions')",
    )
    .fetch_one(&mut *connection)
    .await?;
    Ok(count == 2)
}

/// Apply `migrations` to `connection`, skipping versions already recorded.
///
/// Each pending migration runs in its own transaction together with its
/// history row; the returned versions are the ones this call applied, in
/// order. Re-running on an up-to-date database is a no-op returning `[]`.
pub async fn apply_migrations(
    connection: &mut SqliteConnection,
    migrations: &[Migration],
) -> Result<Vec<i64>, MigrationError> {
    let has_baseline =
        has_baseline_tables(connection)
            .await
            .map_err(|source| MigrationError::Database {
                version: 0,
                description: "baseline detection",
                source,
            })?;
    if !has_baseline {
        return Err(MigrationError::NotABaselineDatabase);
    }

    let history =
        applied_versions(connection)
            .await
            .map_err(|source| MigrationError::Database {
                version: 0,
                description: "read history",
                source,
            })?;
    if history.is_empty() {
        stamp_baseline(connection)
            .await
            .map_err(|source| MigrationError::Database {
                version: 0,
                description: "stamp baseline vintage",
                source,
            })?;
    }

    let applied =
        applied_versions(connection)
            .await
            .map_err(|source| MigrationError::Database {
                version: 0,
                description: "read history",
                source,
            })?;
    let unknown: Vec<i64> = applied
        .iter()
        .filter(|version| **version != 0 && !migrations.iter().any(|m| m.version == **version))
        .copied()
        .collect();
    if !unknown.is_empty() {
        return Err(MigrationError::DatabaseNewerThanBinary {
            unknown_versions: unknown,
        });
    }

    let mut newly_applied = Vec::new();
    for migration in migrations {
        if applied.contains(&migration.version) {
            continue;
        }
        let mut transaction =
            connection
                .begin()
                .await
                .map_err(|source| MigrationError::Database {
                    version: migration.version,
                    description: migration.description,
                    source,
                })?;
        sqlx::query(migration.sql)
            .execute(&mut *transaction)
            .await
            .map_err(|source| MigrationError::Database {
                version: migration.version,
                description: migration.description,
                source,
            })?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {HISTORY_TABLE} (version, description, applied_at) VALUES (?, ?, ?)"
        )))
        .bind(migration.version)
        .bind(migration.description)
        .bind(crate::store::to_sql_timestamp(&chrono::Utc::now()))
        .execute(&mut *transaction)
        .await
        .map_err(|source| MigrationError::Database {
            version: migration.version,
            description: migration.description,
            source,
        })?;
        transaction
            .commit()
            .await
            .map_err(|source| MigrationError::Database {
                version: migration.version,
                description: migration.description,
                source,
            })?;
        newly_applied.push(migration.version);
    }
    Ok(newly_applied)
}

/// Apply the canonical list ([`CANONICAL_MIGRATIONS`]).
pub async fn apply_pending_migrations(
    connection: &mut SqliteConnection,
) -> Result<Vec<i64>, MigrationError> {
    apply_migrations(connection, CANONICAL_MIGRATIONS).await
}
