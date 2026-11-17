//! SQLite implementation of the manual send-record port.
//!
//! The table is append-only: every call inserts one row, and the
//! `send.recorded` audit event is written in the same transaction so the trail
//! can answer who / when / what / result without reading the send table.

use async_trait::async_trait;
use chrono::Utc;
use ica_application::{NewSendRecord, RepositoryError, SendRecordFilter, SendRecordRepository};
use ica_domain::SendRecord;
use serde_json::json;
use sqlx::FromRow;

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{
    offset_bind, repo_error, to_sql_timestamp, where_clause, SqliteStore, DEFAULT_TENANT_ID,
    LOCAL_ACTOR, LOCAL_OWNER_ID,
};

const SELECT_COLUMNS: &str = concat!(
    "SELECT id, candidate_id, task_id, draft_id, channel, target, content_summary, status,",
    " sent_at, note, actor, created_at FROM send_records"
);

/// `sent_at DESC, created_at DESC` — newest send first, with the insertion
/// order as the tiebreaker.
const ORDER_BY: &str = " ORDER BY sent_at DESC, created_at DESC";

#[derive(Debug, FromRow)]
struct SendRecordRow {
    id: String,
    candidate_id: String,
    task_id: Option<String>,
    draft_id: Option<String>,
    channel: String,
    target: Option<String>,
    content_summary: Option<String>,
    status: String,
    sent_at: String,
    note: Option<String>,
    actor: String,
    created_at: String,
}

impl SendRecordRow {
    fn into_record(self) -> Result<SendRecord, RepositoryError> {
        let sent_at = crate::time::parse_sqlite_datetime(&self.sent_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "send_records.sent_at for id '{}': {error}",
                self.id
            ))
        })?;
        let created_at = crate::time::parse_sqlite_datetime(&self.created_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "send_records.created_at for id '{}': {error}",
                self.id
            ))
        })?;
        Ok(SendRecord {
            id: self.id,
            candidate_id: self.candidate_id,
            task_id: self.task_id,
            draft_id: self.draft_id,
            channel: self.channel,
            target: self.target,
            content_summary: self.content_summary,
            status: self.status,
            sent_at,
            note: self.note,
            actor: self.actor,
            created_at,
        })
    }
}

#[async_trait]
impl SendRecordRepository for SqliteStore {
    async fn list(
        &self,
        filter: SendRecordFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SendRecord>, usize), RepositoryError> {
        let mut conditions: Vec<&str> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(value) = filter.candidate_id.as_deref() {
            conditions.push("candidate_id = ?");
            binds.push(value.to_string());
        }
        if let Some(value) = filter.task_id.as_deref() {
            conditions.push("task_id = ?");
            binds.push(value.to_string());
        }
        if let Some(value) = filter.draft_id.as_deref() {
            conditions.push("draft_id = ?");
            binds.push(value.to_string());
        }
        if let Some(status) = filter.status {
            conditions.push("status = ?");
            binds.push(status.as_str().to_string());
        }
        let where_sql = where_clause(&conditions);

        let count_sql = format!("SELECT COUNT(*) FROM send_records{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value.clone());
        }
        let total: i64 = count_query
            .fetch_one(self.pool())
            .await
            .map_err(repo_error)?;

        let sql = format!("{SELECT_COLUMNS}{where_sql}{ORDER_BY} LIMIT ? OFFSET ?");
        let mut query = sqlx::query_as::<_, SendRecordRow>(&sql);
        for value in &binds {
            query = query.bind(value.clone());
        }
        let rows = query
            .bind(limit as i64)
            .bind(offset_bind(offset))
            .fetch_all(self.pool())
            .await
            .map_err(repo_error)?;

        let items = rows
            .into_iter()
            .map(SendRecordRow::into_record)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as usize))
    }

    async fn create(&self, new: NewSendRecord) -> Result<SendRecord, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();
        let sent_at = new
            .sent_at
            .map(|value| to_sql_timestamp(&value))
            .unwrap_or_else(|| now.clone());
        let status = new.status.as_str();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO send_records (id, candidate_id, task_id, draft_id, channel, target,",
            " content_summary, status, sent_at, note, actor, created_at, updated_at, owner_id,",
            " tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&new.candidate_id)
        .bind(&new.task_id)
        .bind(&new.draft_id)
        .bind(&new.channel)
        .bind(&new.target)
        .bind(&new.content_summary)
        .bind(status)
        .bind(&sent_at)
        .bind(&new.note)
        .bind(LOCAL_ACTOR)
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
                "send.recorded",
                "send_record",
                &id,
                format!("Recorded manual send with status '{status}'"),
            )
            .with_metadata(json!({
                "candidate_id": new.candidate_id,
                "task_id": new.task_id,
                "draft_id": new.draft_id,
                "channel": new.channel,
                "status": status,
                "indicates_contact": new.status.indicates_contact(),
            })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        Ok(SendRecord {
            id,
            candidate_id: new.candidate_id,
            task_id: new.task_id,
            draft_id: new.draft_id,
            channel: new.channel,
            target: new.target,
            content_summary: new.content_summary,
            status: status.to_string(),
            sent_at: crate::time::parse_sqlite_datetime(&sent_at)
                .map_err(RepositoryError::unavailable)?,
            note: new.note,
            actor: LOCAL_ACTOR.to_string(),
            created_at: crate::time::parse_sqlite_datetime(&now)
                .map_err(RepositoryError::unavailable)?,
        })
    }

    async fn follow_up_candidate(&self, task_id: &str) -> Result<Option<String>, RepositoryError> {
        let mut connection: sqlx::pool::PoolConnection<sqlx::Sqlite> =
            self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<String> =
            sqlx::query_scalar("SELECT candidate_id FROM follow_up_tasks WHERE id = ?")
                .bind(task_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(repo_error)?;
        Ok(row)
    }

    async fn draft_candidate_status(
        &self,
        draft_id: &str,
    ) -> Result<Option<(String, String)>, RepositoryError> {
        let mut connection: sqlx::pool::PoolConnection<sqlx::Sqlite> =
            self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT candidate_id, status FROM generated_drafts WHERE id = ?")
                .bind(draft_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(repo_error)?;
        Ok(row)
    }
}
