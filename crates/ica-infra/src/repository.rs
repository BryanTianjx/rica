//! SQLite implementation of the position repository port.
//!
//! Queries are built at runtime (`sqlx::query_as` + `FromRow`, decision D7) so
//! `cargo build` never needs a live database. The SQL is written to match what
//! SQLAlchemy emits for the Python baseline, because the wire contract is
//! defined by the observable rows, not by the ORM:
//!
//! * `archived = 0` unless archived rows were requested;
//! * `LOWER(name) LIKE LOWER(?)` on name *or* location (SQLAlchemy's `ilike`);
//! * `ORDER BY created_at DESC`;
//! * the candidate count is a correlated `COUNT(*)` over `candidates`, so it is
//!   never a constant and never includes rows of another position.

use async_trait::async_trait;
use ica_application::{PositionFilter, PositionRepository, RepositoryError};
use ica_domain::{Position, PositionStatus};
use sqlx::{FromRow, SqlitePool};

use crate::time::parse_sqlite_datetime;

/// Columns the adapter reads, plus the derived candidate count.
///
/// `concat!` rather than `\`-continuations: a backslash continuation eats the
/// newline *and* the following indentation, which silently glues the last
/// column onto `FROM` and yields a syntax error at runtime.
const SELECT_COLUMNS: &str = concat!(
    "SELECT p.id, p.name, p.description, p.location, p.skill_requirements,",
    " p.experience_requirements, p.education_requirements, p.salary_min, p.salary_max,",
    " p.notes, p.status, p.archived, p.created_at, p.updated_at,",
    " (SELECT COUNT(*) FROM candidates c WHERE c.position_id = p.id) AS candidate_count",
    " FROM positions p",
);

/// Raw row shape: SQLite hands back text timestamps and an integer boolean.
#[derive(Debug, FromRow)]
struct PositionRow {
    id: String,
    name: String,
    description: Option<String>,
    location: Option<String>,
    skill_requirements: Option<String>,
    experience_requirements: Option<String>,
    education_requirements: Option<String>,
    salary_min: Option<i64>,
    salary_max: Option<i64>,
    notes: Option<String>,
    status: String,
    archived: i64,
    created_at: String,
    updated_at: String,
    candidate_count: i64,
}

impl PositionRow {
    /// Convert the raw row, reporting corrupt values as adapter errors.
    ///
    /// An unknown `status` or unparseable timestamp is a data defect the
    /// baseline would also fail on; surfacing it as an error keeps the API from
    /// inventing a status the domain does not know.
    fn into_position(self) -> Result<Position, RepositoryError> {
        let status = PositionStatus::parse(&self.status).ok_or_else(|| {
            RepositoryError::unavailable(format!(
                "positions.status for id '{}' holds an unknown value: '{}'",
                self.id, self.status
            ))
        })?;
        let created_at = parse_sqlite_datetime(&self.created_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "positions.created_at for id '{}': {error}",
                self.id
            ))
        })?;
        let updated_at = parse_sqlite_datetime(&self.updated_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "positions.updated_at for id '{}': {error}",
                self.id
            ))
        })?;

        Ok(Position {
            id: self.id,
            name: self.name,
            description: self.description,
            location: self.location,
            skill_requirements: self.skill_requirements,
            experience_requirements: self.experience_requirements,
            education_requirements: self.education_requirements,
            salary_min: self.salary_min,
            salary_max: self.salary_max,
            notes: self.notes,
            status,
            archived: self.archived != 0,
            created_at,
            updated_at,
            candidate_count: self.candidate_count,
        })
    }
}

/// Read access to `positions` over a `sqlx` SQLite pool.
#[derive(Debug, Clone)]
pub struct SqlitePositionRepository {
    pool: SqlitePool,
}

impl SqlitePositionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The pool this repository reads through.
    ///
    /// Exposed so the server's startup self-check can reuse one connection
    /// pool instead of opening a second handle to the same file.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn run(&self, sql: &str, binds: &[String]) -> Result<Vec<Position>, RepositoryError> {
        let mut query = sqlx::query_as::<_, PositionRow>(sql);
        for value in binds {
            query = query.bind(value);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| RepositoryError::unavailable(error.to_string()))?;
        rows.into_iter().map(PositionRow::into_position).collect()
    }
}

#[async_trait]
impl PositionRepository for SqlitePositionRepository {
    async fn list(&self, filter: PositionFilter) -> Result<Vec<Position>, RepositoryError> {
        let mut conditions: Vec<&str> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if !filter.include_archived {
            conditions.push("p.archived = 0");
        }

        // `search` arrives normalised by the domain layer (trimmed, non-empty).
        // The pattern keeps SQLite's `%`/`_` semantics, exactly like the
        // f-string in the Python service: a user-supplied `%` stays a wildcard.
        if let Some(search) = filter.search.as_deref() {
            let pattern = format!("%{search}%");
            conditions.push("(LOWER(p.name) LIKE LOWER(?) OR LOWER(p.location) LIKE LOWER(?))");
            binds.push(pattern.clone());
            binds.push(pattern);
        }

        let mut sql = String::from(SELECT_COLUMNS);
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY p.created_at DESC");

        self.run(&sql, &binds).await
    }

    async fn find(&self, id: &str) -> Result<Option<Position>, RepositoryError> {
        let sql = format!("{SELECT_COLUMNS} WHERE p.id = ?");
        let mut rows = self.run(&sql, &[id.to_string()]).await?;
        Ok(rows.pop())
    }
}
