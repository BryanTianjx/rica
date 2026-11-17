//! SQLite implementation of the candidate repository port.
//!
//! The read statement joins the position name and two **grouped** counters in
//! one round trip (OPT-1): `message_count` and `active_follow_up_count` are
//! aggregate subqueries keyed by candidate, not a query per row.
//!
//! `active_follow_up_count` counts only the follow-up states where a contact
//! attempt actually happened (`Copied`, `AwaitingResult`, `Completed`) — the
//! same `FollowUpTaskStatus.means_contacted` set the Python service uses.
//! Drafting or approving a draft is not a contact.

use async_trait::async_trait;
use chrono::Utc;
use ica_application::{
    CandidateFilter, CandidatePatch, CandidateRepository, Field, NewCandidate, RepositoryError,
};
use ica_domain::{Candidate, CandidateStatus, Position};
use sqlx::{FromRow, SqliteConnection};

use crate::audit::{insert_audit, AuditEntry};
use crate::repository::find_position_by_id;
use crate::store::{
    repo_error, to_sql_timestamp, where_clause, SqliteStore, DEFAULT_TENANT_ID, LOCAL_OWNER_ID,
};

/// Statuses that count as a contact attempt (`means_contacted`).
///
/// Inlined as a literal because `concat!` only accepts literals; the set is
/// `FollowUpTaskStatus.means_contacted` in the Python domain.
const SELECT_COLUMNS: &str = concat!(
    "SELECT c.id, c.name, c.source, c.position_id, c.experience_summary, c.skill_summary,",
    " c.expected_salary, c.expected_location, c.job_intention, c.notes, c.status, c.archived,",
    " c.created_at, c.updated_at, p.name AS position_name,",
    " COALESCE(msg.message_count, 0) AS message_count,",
    " COALESCE(fu.active_follow_up_count, 0) AS active_follow_up_count",
    " FROM candidates c",
    " LEFT JOIN positions p ON p.id = c.position_id",
    " LEFT JOIN (SELECT candidate_id, COUNT(*) AS message_count FROM messages",
    "            GROUP BY candidate_id) msg ON msg.candidate_id = c.id",
    " LEFT JOIN (SELECT candidate_id, COUNT(*) AS active_follow_up_count",
    "            FROM follow_up_tasks WHERE status IN ('Copied', 'AwaitingResult', 'Completed')",
    "            GROUP BY candidate_id) fu ON fu.candidate_id = c.id",
);

#[derive(Debug, FromRow)]
struct CandidateRow {
    id: String,
    name: String,
    source: String,
    position_id: Option<String>,
    experience_summary: Option<String>,
    skill_summary: Option<String>,
    expected_salary: Option<String>,
    expected_location: Option<String>,
    job_intention: Option<String>,
    notes: Option<String>,
    status: String,
    archived: i64,
    created_at: String,
    updated_at: String,
    position_name: Option<String>,
    message_count: i64,
    active_follow_up_count: i64,
}

impl CandidateRow {
    fn into_candidate(self) -> Result<Candidate, RepositoryError> {
        let status = CandidateStatus::parse(&self.status).ok_or_else(|| {
            RepositoryError::unavailable(format!(
                "candidates.status for id '{}' holds an unknown value: '{}'",
                self.id, self.status
            ))
        })?;
        let created_at = crate::time::parse_sqlite_datetime(&self.created_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "candidates.created_at for id '{}': {error}",
                self.id
            ))
        })?;
        let updated_at = crate::time::parse_sqlite_datetime(&self.updated_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "candidates.updated_at for id '{}': {error}",
                self.id
            ))
        })?;

        Ok(Candidate {
            id: self.id,
            name: self.name,
            source: self.source,
            position_id: self.position_id,
            experience_summary: self.experience_summary,
            skill_summary: self.skill_summary,
            expected_salary: self.expected_salary,
            expected_location: self.expected_location,
            job_intention: self.job_intention,
            notes: self.notes,
            status,
            archived: self.archived != 0,
            created_at,
            updated_at,
            position_name: self.position_name,
            message_count: self.message_count,
            active_follow_up_count: self.active_follow_up_count,
        })
    }
}

/// `WHERE` fragment plus text binds for the candidate filter.
fn filter_sql(filter: &CandidateFilter) -> (String, Vec<String>) {
    let mut conditions: Vec<&str> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if !filter.include_archived {
        conditions.push("c.archived = 0");
    }
    if let Some(status) = filter.status {
        conditions.push("c.status = ?");
        binds.push(status.as_str().to_string());
    }
    if let Some(position_id) = filter.position_id.as_deref() {
        conditions.push("c.position_id = ?");
        binds.push(position_id.to_string());
    }
    if let Some(search) = filter.search.as_deref() {
        let pattern = format!("%{search}%");
        conditions.push(concat!(
            "(LOWER(c.name) LIKE LOWER(?)",
            " OR LOWER(c.skill_summary) LIKE LOWER(?)",
            " OR LOWER(c.expected_location) LIKE LOWER(?))"
        ));
        binds.push(pattern.clone());
        binds.push(pattern.clone());
        binds.push(pattern);
    }

    (where_clause(&conditions), binds)
}

async fn fetch_candidates(
    connection: &mut SqliteConnection,
    sql: &str,
    binds: &[String],
    limit: Option<(usize, usize)>,
) -> Result<Vec<Candidate>, RepositoryError> {
    let mut query = sqlx::query_as::<_, CandidateRow>(sql);
    for value in binds {
        query = query.bind(value.clone());
    }
    if let Some((limit, offset)) = limit {
        query = query.bind(limit as i64).bind(offset as i64);
    }
    let rows = query
        .fetch_all(&mut *connection)
        .await
        .map_err(repo_error)?;
    rows.into_iter().map(CandidateRow::into_candidate).collect()
}

#[async_trait]
impl CandidateRepository for SqliteStore {
    async fn list(
        &self,
        filter: CandidateFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Candidate>, usize), RepositoryError> {
        let (where_sql, binds) = filter_sql(&filter);

        let count_sql = format!("SELECT COUNT(*) FROM candidates c{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value.clone());
        }
        let total: i64 = count_query
            .fetch_one(self.pool())
            .await
            .map_err(repo_error)?;

        let sql =
            format!("{SELECT_COLUMNS}{where_sql} ORDER BY c.updated_at DESC LIMIT ? OFFSET ?");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let items = fetch_candidates(&mut connection, &sql, &binds, Some((limit, offset))).await?;
        Ok((items, total as usize))
    }

    async fn find(&self, id: &str) -> Result<Option<Candidate>, RepositoryError> {
        let sql = format!("{SELECT_COLUMNS} WHERE c.id = ?");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let mut rows = fetch_candidates(&mut connection, &sql, &[id.to_string()], None).await?;
        Ok(rows.pop())
    }

    async fn position_of(&self, candidate_id: &str) -> Result<Option<Position>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let position_id: Option<Option<String>> =
            sqlx::query_scalar("SELECT position_id FROM candidates WHERE id = ?")
                .bind(candidate_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(repo_error)?;
        let Some(Some(position_id)) = position_id else {
            return Ok(None);
        };
        find_position_by_id(&mut connection, &position_id).await
    }

    async fn create(&self, new: NewCandidate) -> Result<Candidate, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO candidates (id, name, source, position_id, experience_summary,",
            " skill_summary, expected_salary, expected_location, job_intention, status, notes,",
            " archived, created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&new.name)
        .bind(&new.source)
        .bind(&new.position_id)
        .bind(&new.experience_summary)
        .bind(&new.skill_summary)
        .bind(&new.expected_salary)
        .bind(&new.expected_location)
        .bind(&new.job_intention)
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
            AuditEntry::new("candidate.created", "candidate", &id, "Created candidate"),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        self.find(&id)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("inserted candidate disappeared"))
    }

    async fn update(
        &self,
        id: &str,
        patch: CandidatePatch,
    ) -> Result<Option<Candidate>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let existing =
            sqlx::query_as::<_, CandidateRow>(&format!("{SELECT_COLUMNS} WHERE c.id = ?"))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(repo_error)?;
        let Some(row) = existing else {
            tx.rollback().await.map_err(repo_error)?;
            return Ok(None);
        };
        let current = row.into_candidate()?;

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
        let source = match &patch.source {
            Field::Value(value) => value.clone(),
            Field::Absent | Field::Null => current.source.clone(),
        };
        // `position_id = null` detaches the position — a legal PATCH.
        let position_id = apply(&patch.position_id, current.position_id.clone());
        let experience_summary = apply(
            &patch.experience_summary,
            current.experience_summary.clone(),
        );
        let skill_summary = apply(&patch.skill_summary, current.skill_summary.clone());
        let expected_salary = apply(&patch.expected_salary, current.expected_salary.clone());
        let expected_location = apply(&patch.expected_location, current.expected_location.clone());
        let job_intention = apply(&patch.job_intention, current.job_intention.clone());
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
            "UPDATE candidates SET name = ?, source = ?, position_id = ?, experience_summary = ?,",
            " skill_summary = ?, expected_salary = ?, expected_location = ?, job_intention = ?,",
            " status = ?, notes = ?, archived = ?, updated_at = ? WHERE id = ?"
        ))
        .bind(&name)
        .bind(&source)
        .bind(&position_id)
        .bind(&experience_summary)
        .bind(&skill_summary)
        .bind(&expected_salary)
        .bind(&expected_location)
        .bind(&job_intention)
        .bind(status.as_str())
        .bind(&notes)
        .bind(i64::from(archived))
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new("candidate.updated", "candidate", id, "Updated candidate"),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        self.find(id).await
    }

    /// Delete the row; FK cascades remove conversations, messages, send records
    /// and follow-up tasks (the baseline measured all of them at 0 afterwards).
    async fn delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM candidates WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(repo_error)?;
        if exists.is_none() {
            tx.rollback().await.map_err(repo_error)?;
            return Ok(false);
        }

        sqlx::query("DELETE FROM candidates WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "candidate.deleted",
                "candidate",
                id,
                "Deleted candidate and all dependent records",
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(true)
    }
}
