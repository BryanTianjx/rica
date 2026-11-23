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
//!
//! Writes keep the row and its audit event in one transaction and stamp the
//! local-mode ownership defaults (`app/application/recruiting.py::_owner_tenant`).
//!
//! There is deliberately no tenant/owner filtering on any read: this adapter
//! mirrors the Python baseline's **local mode**, where
//! `app/application/access.py::scope_position_query` adds no filter when
//! `current_user.is_local` — so "no scoping here" is behaviour parity, not an
//! omission. That equivalence ends at cloud mode: when auth arrives (W6), every
//! read path must be scoped by tenant/owner/share like the Python `scope_*`
//! helpers, or it is a cross-tenant read. Treat that as a hard requirement for
//! the W6 card, not a refactor.

use async_trait::async_trait;
use chrono::Utc;
use ica_application::{
    Field, NewPosition, PositionFilter, PositionPatch, PositionRepository, RepositoryError, Scope,
};
use ica_domain::{Position, PositionStatus};
use sqlx::{FromRow, SqliteConnection};

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{
    offset_bind, repo_error, to_sql_timestamp, where_clause, SqliteStore, DEFAULT_TENANT_ID,
    LOCAL_OWNER_ID,
};

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
        let created_at = crate::time::parse_sqlite_datetime(&self.created_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "positions.created_at for id '{}': {error}",
                self.id
            ))
        })?;
        let updated_at = crate::time::parse_sqlite_datetime(&self.updated_at).map_err(|error| {
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

/// `WHERE` fragment plus its text binds for a position filter.
pub(crate) fn filter_sql(filter: &PositionFilter) -> (String, Vec<String>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if !filter.include_archived {
        conditions.push("p.archived = 0".to_string());
    }

    // SEC-04-B/C: an authenticated identity must never read another tenant's
    // rows. Admins see everything in their tenant; members only own rows.
    // `None` is the documented Local mode and adds no predicate.
    if let Some(scope) = &filter.scope {
        conditions.push("p.tenant_id = ?".to_string());
        binds.push(scope.tenant_id.clone());
        if !scope.is_admin {
            conditions.push("p.owner_id = ?".to_string());
            binds.push(scope.actor_id.clone());
        }
    }

    // `search` arrives normalised by the domain layer (trimmed, non-empty).
    // The pattern keeps SQLite's `%`/`_` semantics, exactly like the f-string
    // in the Python service: a user-supplied `%` stays a wildcard.
    if let Some(search) = filter.search.as_deref() {
        let pattern = format!("%{search}%");
        conditions
            .push("(LOWER(p.name) LIKE LOWER(?) OR LOWER(p.location) LIKE LOWER(?))".to_string());
        binds.push(pattern.clone());
        binds.push(pattern);
    }

    let refs: Vec<&str> = conditions.iter().map(String::as_str).collect();
    (where_clause(&refs), binds)
}

/// Run one position query on a pool or a transaction.
async fn fetch_positions(
    connection: &mut SqliteConnection,
    sql: &str,
    binds: &[String],
) -> Result<Vec<Position>, RepositoryError> {
    let mut query = sqlx::query_as::<_, PositionRow>(sql);
    for value in binds {
        query = query.bind(value.clone());
    }
    let rows = query
        .fetch_all(&mut *connection)
        .await
        .map_err(repo_error)?;
    rows.into_iter().map(PositionRow::into_position).collect()
}

/// Load one position by id on any connection.
///
/// Shared with the candidate adapter, which has to return the *mapped* position
/// for the detail response (mapping the raw row is what keeps
/// `candidate_count` real).
pub(crate) async fn find_position_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<Position>, RepositoryError> {
    let sql = format!("{SELECT_COLUMNS} WHERE p.id = ?");
    let mut rows = fetch_positions(connection, &sql, &[id.to_string()]).await?;
    Ok(rows.pop())
}

/// SEC-04-B/C: true only when the row exists inside the caller's scope. An
/// out-of-scope row is treated as missing by scoped mutations as well as reads.
async fn scoped_position_exists(
    pool: &sqlx::SqlitePool,
    id: &str,
    scope: &Scope,
) -> Result<bool, RepositoryError> {
    let sql = if scope.is_admin {
        "SELECT 1 FROM positions WHERE id = ? AND tenant_id = ?".to_string()
    } else {
        "SELECT 1 FROM positions WHERE id = ? AND tenant_id = ? AND owner_id = ?".to_string()
    };
    let mut query = sqlx::query_scalar::<_, i64>(&sql)
        .bind(id)
        .bind(&scope.tenant_id);
    if !scope.is_admin {
        query = query.bind(&scope.actor_id);
    }
    Ok(query
        .fetch_optional(pool)
        .await
        .map_err(repo_error)?
        .is_some())
}

#[async_trait]
impl PositionRepository for SqliteStore {
    async fn list(&self, filter: PositionFilter) -> Result<Vec<Position>, RepositoryError> {
        let (where_sql, binds) = filter_sql(&filter);
        let sql = format!("{SELECT_COLUMNS}{where_sql} ORDER BY p.created_at DESC");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_positions(&mut connection, &sql, &binds).await
    }

    /// OPT-2: `COUNT(*)` for the total and `LIMIT/OFFSET` for the window, so a
    /// large table is never read into memory to serve page 1. Same filter, same
    /// `ORDER BY`, so `items`, `total` and `has_more` are identical to the
    /// in-memory slice the baseline performs.
    async fn page(
        &self,
        filter: PositionFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Position>, usize), RepositoryError> {
        let (where_sql, binds) = filter_sql(&filter);

        let count_sql = format!("SELECT COUNT(*) FROM positions p{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value.clone());
        }
        let total: i64 = count_query
            .fetch_one(self.pool())
            .await
            .map_err(repo_error)?;

        let sql =
            format!("{SELECT_COLUMNS}{where_sql} ORDER BY p.created_at DESC LIMIT ? OFFSET ?");
        let mut page_query = sqlx::query_as::<_, PositionRow>(&sql);
        for value in &binds {
            page_query = page_query.bind(value.clone());
        }
        let rows = page_query
            .bind(limit as i64)
            .bind(offset_bind(offset))
            .fetch_all(self.pool())
            .await
            .map_err(repo_error)?;

        let items = rows
            .into_iter()
            .map(PositionRow::into_position)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as usize))
    }

    async fn find(&self, id: &str) -> Result<Option<Position>, RepositoryError> {
        let sql = format!("{SELECT_COLUMNS} WHERE p.id = ?");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let mut rows = fetch_positions(&mut connection, &sql, &[id.to_string()]).await?;
        Ok(rows.pop())
    }

    async fn find_scoped(
        &self,
        id: &str,
        scope: &Scope,
    ) -> Result<Option<Position>, RepositoryError> {
        let mut sql = format!("{SELECT_COLUMNS} WHERE p.id = ? AND p.tenant_id = ?");
        let mut binds = vec![id.to_string(), scope.tenant_id.clone()];
        if !scope.is_admin {
            sql.push_str(" AND p.owner_id = ?");
            binds.push(scope.actor_id.clone());
        }
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let mut rows = fetch_positions(&mut connection, &sql, &binds).await?;
        Ok(rows.pop())
    }

    async fn create(&self, new: NewPosition) -> Result<Position, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO positions (id, name, description, location, skill_requirements,",
            " experience_requirements, education_requirements, salary_min, salary_max,",
            " status, archived, notes, created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&new.name)
        .bind(&new.description)
        .bind(&new.location)
        .bind(&new.skill_requirements)
        .bind(&new.experience_requirements)
        .bind(&new.education_requirements)
        .bind(new.salary_min)
        .bind(new.salary_max)
        .bind(new.status.as_str())
        .bind(&new.notes)
        .bind(&now)
        .bind(&now)
        .bind(LOCAL_OWNER_ID)
        .bind(DEFAULT_TENANT_ID)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "position.created",
                "position",
                &id,
                format!("Created position '{}'", new.name),
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        self.find(&id)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("inserted position disappeared"))
    }

    async fn create_scoped(
        &self,
        new: NewPosition,
        scope: &Scope,
    ) -> Result<Position, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO positions (id, name, description, location, skill_requirements,",
            " experience_requirements, education_requirements, salary_min, salary_max,",
            " status, archived, notes, created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&new.name)
        .bind(&new.description)
        .bind(&new.location)
        .bind(&new.skill_requirements)
        .bind(&new.experience_requirements)
        .bind(&new.education_requirements)
        .bind(new.salary_min)
        .bind(new.salary_max)
        .bind(new.status.as_str())
        .bind(&new.notes)
        .bind(&now)
        .bind(&now)
        .bind(&scope.actor_id)
        .bind(&scope.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "position.created",
                "position",
                &id,
                format!("Created position '{}'", new.name),
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        self.find_scoped(&id, scope)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("inserted position disappeared"))
    }

    /// Read-modify-write, the same shape the ORM uses: load the row, apply the
    /// patch, write every column back. `status = null` is ignored (the baseline
    /// only assigns when the value is present); `archived = null` never reaches
    /// here because the API rejects it with 422 first (IMP-2).
    async fn update(
        &self,
        id: &str,
        patch: PositionPatch,
    ) -> Result<Option<Position>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;

        let existing =
            sqlx::query_as::<_, PositionRow>(&format!("{SELECT_COLUMNS} WHERE p.id = ?"))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(repo_error)?;
        let Some(row) = existing else {
            tx.rollback().await.map_err(repo_error)?;
            return Ok(None);
        };
        let current = row.into_position()?;

        fn apply<T: Clone>(field: &Field<T>, current: Option<T>) -> Option<T> {
            match field {
                Field::Absent => current,
                Field::Null => None,
                Field::Value(value) => Some(value.clone()),
            }
        }

        let name = match &patch.name {
            Field::Value(value) => value.clone(),
            Field::Absent | Field::Null => current.name.clone(),
        };
        let description = apply(&patch.description, current.description.clone());
        let location = apply(&patch.location, current.location.clone());
        let skill_requirements = apply(
            &patch.skill_requirements,
            current.skill_requirements.clone(),
        );
        let experience_requirements = apply(
            &patch.experience_requirements,
            current.experience_requirements.clone(),
        );
        let education_requirements = apply(
            &patch.education_requirements,
            current.education_requirements.clone(),
        );
        let salary_min = apply(&patch.salary_min, current.salary_min);
        let salary_max = apply(&patch.salary_max, current.salary_max);
        let notes = apply(&patch.notes, current.notes.clone());
        let status = match &patch.status {
            Field::Value(value) => *value,
            Field::Absent | Field::Null => current.status,
        };
        let archived = match &patch.archived {
            Field::Value(value) => *value,
            Field::Absent | Field::Null => current.archived,
        };
        let now = to_sql_timestamp(&Utc::now());

        sqlx::query(concat!(
            "UPDATE positions SET name = ?, description = ?, location = ?,",
            " skill_requirements = ?, experience_requirements = ?, education_requirements = ?,",
            " salary_min = ?, salary_max = ?, status = ?, archived = ?, notes = ?, updated_at = ?",
            " WHERE id = ?"
        ))
        .bind(&name)
        .bind(&description)
        .bind(&location)
        .bind(&skill_requirements)
        .bind(&experience_requirements)
        .bind(&education_requirements)
        .bind(salary_min)
        .bind(salary_max)
        .bind(status.as_str())
        .bind(i64::from(archived))
        .bind(&notes)
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "position.updated",
                "position",
                id,
                format!("Updated position '{name}'"),
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        self.find(id).await
    }

    /// Delete a row and audit it. The audit summary names the row, so the value
    /// is read before the delete and the event is written in the same
    /// transaction — a candidate that pointed here keeps its row (FK SET NULL).
    async fn delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let name: Option<String> = sqlx::query_scalar("SELECT name FROM positions WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(repo_error)?;
        let Some(name) = name else {
            tx.rollback().await.map_err(repo_error)?;
            return Ok(false);
        };

        // Python's ORM nulls the children through the relationship, which fires
        // the `updated_at` onupdate hook once per row. That gives the affected
        // children *distinct, increasing* timestamps, and therefore a
        // deterministic order in the next `updated_at DESC` listing; the bare FK
        // action would leave `updated_at` untouched and make that ordering a tie.
        let affected: Vec<String> =
            sqlx::query_scalar("SELECT id FROM candidates WHERE position_id = ? ORDER BY rowid")
                .bind(id)
                .fetch_all(&mut *tx)
                .await
                .map_err(repo_error)?;
        let base = Utc::now();
        for (index, candidate_id) in affected.iter().enumerate() {
            let stamp = to_sql_timestamp(&(base + chrono::Duration::microseconds(index as i64)));
            sqlx::query("UPDATE candidates SET position_id = NULL, updated_at = ? WHERE id = ?")
                .bind(&stamp)
                .bind(candidate_id)
                .execute(&mut *tx)
                .await
                .map_err(repo_error)?;
        }

        sqlx::query("DELETE FROM positions WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "position.deleted",
                "position",
                id,
                format!("Deleted position '{name}'"),
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(true)
    }

    async fn update_scoped(
        &self,
        id: &str,
        patch: PositionPatch,
        scope: &Scope,
    ) -> Result<Option<Position>, RepositoryError> {
        if !scoped_position_exists(self.pool(), id, scope).await? {
            return Ok(None);
        }
        self.update(id, patch).await
    }

    async fn delete_scoped(&self, id: &str, scope: &Scope) -> Result<bool, RepositoryError> {
        if !scoped_position_exists(self.pool(), id, scope).await? {
            return Ok(false);
        }
        self.delete(id).await
    }
}
