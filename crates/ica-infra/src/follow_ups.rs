//! SQLite implementation of the follow-up and dashboard ports.
//!
//! Read/write shapes follow `app/application/follow_up_service.py` and the
//! dashboard API, with every write carrying the audit event the baseline
//! writes for the same operation — in the same transaction. The state-machine
//! checks live in the application/domain layers; this adapter trusts them and
//! only persists.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ica_application::{
    DashboardRepository, Field, FollowUpPolicyPatch, FollowUpPolicyRecord, FollowUpRepository,
    FollowUpResultPatch, FollowUpTaskFilter, FollowUpTaskPatch, NewFollowUpTaskRow, NewSendRecord,
    RepositoryError,
};
use ica_domain::{FollowUpPolicy, FollowUpTask, FollowUpTaskStatus, Message};
use serde_json::json;
use sqlx::{FromRow, SqliteConnection};

use crate::audit::{insert_audit, AuditEntry};
use crate::send_records::insert_send_record;
use crate::store::{
    offset_bind, repo_error, to_sql_timestamp, SqliteStore, DEFAULT_TENANT_ID, LOCAL_OWNER_ID,
};

/// Task columns plus the candidate name the read model carries
/// (`task_to_read` joins `task.candidate.name` in the baseline).
const SELECT_COLUMNS: &str = concat!(
    "SELECT t.id, t.candidate_id, t.position_id, t.status, t.objective, t.reason,",
    " t.block_reason, t.recommended_at, t.expires_at, t.last_follow_up_at,",
    " t.follow_up_count, t.completed_at, t.result_type, t.result_note,",
    " t.result_recorded_at, t.created_at, t.updated_at, c.name AS candidate_name",
    " FROM follow_up_tasks t",
    " LEFT JOIN candidates c ON c.id = t.candidate_id",
);

/// States the sweep may legally expire — `can_transition(status, Expired)` as a
/// literal (`concat!` needs literals; the set is pinned by domain unit tests).
const EXPIRABLE_STATUSES: &str =
    "('Pending', 'Eligible', 'Drafting', 'Reviewing', 'Rejected', 'Approved', 'Blocked')";

/// The contact-counting statuses (`FollowUpTaskStatus::means_contacted`).
const CONTACT_STATUSES: &str = "('Copied', 'AwaitingResult', 'Completed')";

#[derive(Debug, FromRow)]
struct FollowUpTaskRow {
    id: String,
    candidate_id: String,
    position_id: Option<String>,
    status: String,
    objective: Option<String>,
    reason: Option<String>,
    block_reason: Option<String>,
    recommended_at: Option<String>,
    expires_at: Option<String>,
    last_follow_up_at: Option<String>,
    follow_up_count: i64,
    completed_at: Option<String>,
    result_type: Option<String>,
    result_note: Option<String>,
    result_recorded_at: Option<String>,
    created_at: String,
    updated_at: String,
    candidate_name: Option<String>,
}

impl FollowUpTaskRow {
    fn time(
        column: &str,
        id: &str,
        raw: Option<&String>,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        match raw {
            None => Ok(None),
            Some(value) => crate::time::parse_sqlite_datetime(value)
                .map(Some)
                .map_err(|error| {
                    RepositoryError::unavailable(format!(
                        "follow_up_tasks.{column} for id '{id}': {error}"
                    ))
                }),
        }
    }

    fn into_task(self) -> Result<FollowUpTask, RepositoryError> {
        let status = FollowUpTaskStatus::parse(&self.status).ok_or_else(|| {
            RepositoryError::unavailable(format!(
                "follow_up_tasks.status for id '{}' holds an unknown value: '{}'",
                self.id, self.status
            ))
        })?;
        Ok(FollowUpTask {
            id: self.id.clone(),
            candidate_id: self.candidate_id,
            position_id: self.position_id,
            status,
            objective: self.objective,
            reason: self.reason,
            block_reason: self.block_reason,
            recommended_at: Self::time("recommended_at", &self.id, self.recommended_at.as_ref())?,
            expires_at: Self::time("expires_at", &self.id, self.expires_at.as_ref())?,
            last_follow_up_at: Self::time(
                "last_follow_up_at",
                &self.id,
                self.last_follow_up_at.as_ref(),
            )?,
            follow_up_count: self.follow_up_count,
            completed_at: Self::time("completed_at", &self.id, self.completed_at.as_ref())?,
            result_type: self.result_type,
            result_note: self.result_note,
            result_recorded_at: Self::time(
                "result_recorded_at",
                &self.id,
                self.result_recorded_at.as_ref(),
            )?,
            created_at: Self::time("created_at", &self.id, Some(&self.created_at))?.ok_or_else(
                || {
                    RepositoryError::unavailable(format!(
                        "follow_up_tasks.created_at for id '{}' is NULL",
                        self.id
                    ))
                },
            )?,
            updated_at: Self::time("updated_at", &self.id, Some(&self.updated_at))?.ok_or_else(
                || {
                    RepositoryError::unavailable(format!(
                        "follow_up_tasks.updated_at for id '{}' is NULL",
                        self.id
                    ))
                },
            )?,
            candidate_name: self.candidate_name,
        })
    }
}

/// The extra read-model field `task_to_read` adds in the baseline.
#[derive(Debug, FromRow)]
struct MessageRow {
    id: String,
    candidate_id: String,
    conversation_id: String,
    sender_type: Option<String>,
    content: String,
    message_time: Option<String>,
    channel: String,
    source: String,
    created_at: String,
}

impl MessageRow {
    fn into_message(self) -> Result<Message, RepositoryError> {
        let message_time = match self.message_time.as_deref() {
            None => None,
            Some(raw) => Some(
                crate::time::parse_sqlite_datetime(raw).map_err(RepositoryError::unavailable)?,
            ),
        };
        let created_at = crate::time::parse_sqlite_datetime(&self.created_at)
            .map_err(RepositoryError::unavailable)?;
        Ok(Message {
            id: self.id,
            candidate_id: self.candidate_id,
            conversation_id: self.conversation_id,
            sender_type: self.sender_type,
            content: self.content,
            message_time,
            channel: self.channel,
            source: self.source,
            created_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct PolicyRow {
    id: String,
    name: String,
    first_follow_up_delay_hours: i64,
    max_active_follow_ups: i64,
    min_interval_hours: i64,
    task_validity_days: i64,
    is_active: i64,
}

impl PolicyRow {
    fn into_record(self) -> FollowUpPolicyRecord {
        FollowUpPolicyRecord {
            id: self.id,
            name: self.name,
            policy: FollowUpPolicy {
                first_follow_up_delay_hours: self.first_follow_up_delay_hours,
                max_active_follow_ups: self.max_active_follow_ups,
                min_interval_hours: self.min_interval_hours,
                task_validity_days: self.task_validity_days,
            },
            is_active: self.is_active != 0,
        }
    }
}

async fn fetch_one_task(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<FollowUpTask>, RepositoryError> {
    let row: Option<FollowUpTaskRow> = sqlx::query_as(&format!("{SELECT_COLUMNS} WHERE t.id = ?"))
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(repo_error)?;
    row.map(FollowUpTaskRow::into_task).transpose()
}

impl SqliteStore {
    /// The shared WHERE building block: candidate/status filters.
    fn task_filters(filter: &FollowUpTaskFilter) -> (String, Vec<String>) {
        let mut clauses = Vec::new();
        let mut binds = Vec::new();
        if let Some(candidate_id) = &filter.candidate_id {
            clauses.push("t.candidate_id = ?".to_string());
            binds.push(candidate_id.clone());
        }
        if let Some(status) = filter.status {
            clauses.push("t.status = ?".to_string());
            binds.push(status.as_str().to_string());
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        (where_sql, binds)
    }
}

#[async_trait]
impl FollowUpRepository for SqliteStore {
    async fn list(
        &self,
        filter: FollowUpTaskFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<FollowUpTask>, usize), RepositoryError> {
        let (where_sql, binds) = Self::task_filters(&filter);
        let count_sql = format!("SELECT COUNT(*) FROM follow_up_tasks t{where_sql}");
        let list_sql =
            format!("{SELECT_COLUMNS}{where_sql} ORDER BY t.created_at DESC LIMIT ? OFFSET ?");

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value);
        }
        let total: i64 = count_query
            .fetch_one(&mut *connection)
            .await
            .map_err(repo_error)?;

        let mut list_query = sqlx::query_as::<_, FollowUpTaskRow>(&list_sql);
        for value in &binds {
            list_query = list_query.bind(value);
        }
        list_query = list_query.bind(limit as i64).bind(offset_bind(offset));
        let rows = list_query
            .fetch_all(&mut *connection)
            .await
            .map_err(repo_error)?;
        let items = rows
            .into_iter()
            .map(FollowUpTaskRow::into_task)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as usize))
    }

    async fn find(&self, id: &str) -> Result<Option<FollowUpTask>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }

    async fn find_open_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let sql = format!(
            "{SELECT_COLUMNS} WHERE t.candidate_id = ? AND t.status IN (\
             'Pending', 'Eligible', 'Drafting', 'Reviewing', 'Rejected', 'Approved', \
             'Copied', 'AwaitingResult') ORDER BY t.created_at DESC LIMIT 1"
        );
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<FollowUpTaskRow> = sqlx::query_as(&sql)
            .bind(candidate_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(repo_error)?;
        row.map(FollowUpTaskRow::into_task).transpose()
    }

    async fn create(&self, new: NewFollowUpTaskRow) -> Result<FollowUpTask, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();
        let status = new.status.as_str();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO follow_up_tasks (id, candidate_id, position_id, status, objective,",
            " reason, block_reason, recommended_at, expires_at, last_follow_up_at,",
            " follow_up_count, completed_at, result_type, result_note, result_recorded_at,",
            " created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 0, NULL, NULL, NULL, NULL, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&new.candidate_id)
        .bind(&new.position_id)
        .bind(status)
        .bind(&new.objective)
        .bind(&new.reason)
        .bind(&new.block_reason)
        .bind(new.recommended_at.map(|value| to_sql_timestamp(&value)))
        .bind(to_sql_timestamp(&new.expires_at))
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
                "follow_up.task_created",
                "follow_up_task",
                &id,
                "Created follow-up task",
            )
            .with_metadata(json!({
                "status": status,
                "outcome": new.outcome,
            })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, &id)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("inserted follow-up task disappeared"))
    }

    async fn mark_reused(
        &self,
        id: &str,
        status: FollowUpTaskStatus,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM follow_up_tasks WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(repo_error)?;
        if exists.is_none() {
            return Ok(None);
        }
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "follow_up.task_reused",
                "follow_up_task",
                id,
                "Reused the existing open follow-up task",
            )
            .with_metadata(json!({ "status": status.as_str() })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }

    async fn update(
        &self,
        id: &str,
        patch: FollowUpTaskPatch,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let Some(current) = fetch_one_task(&mut tx, id).await? else {
            return Ok(None);
        };

        // Read-modify-write, the same shape the ORM uses. `status = null` never
        // reaches here (the API treats absent and null alike as "no change").
        let objective = match &patch.objective {
            Field::Value(value) => Some(value.clone()),
            Field::Null => None,
            Field::Absent => current.objective.clone(),
        };
        let reason = match &patch.reason {
            Field::Value(value) => Some(value.clone()),
            Field::Null => None,
            Field::Absent => current.reason.clone(),
        };
        let recommended_at = match patch.recommended_at {
            Field::Value(value) => Some(to_sql_timestamp(&value)),
            Field::Null => None,
            Field::Absent => current.recommended_at.map(|value| to_sql_timestamp(&value)),
        };
        let status = match patch.status {
            Field::Value(value) => value,
            _ => current.status,
        };

        sqlx::query(concat!(
            "UPDATE follow_up_tasks SET objective = ?, reason = ?, recommended_at = ?,",
            " status = ?, updated_at = ? WHERE id = ?"
        ))
        .bind(&objective)
        .bind(&reason)
        .bind(&recommended_at)
        .bind(status.as_str())
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "follow_up.task_updated",
                "follow_up_task",
                id,
                format!("Updated follow-up task ({})", status.as_str()),
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }

    async fn cancel(
        &self,
        id: &str,
        reason: Option<String>,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let Some(current) = fetch_one_task(&mut tx, id).await? else {
            return Ok(None);
        };
        // Python: `task.reason = reason or task.reason` — the app only passes
        // `Some` for a non-empty reason, so here it simply replaces.
        let resolved_reason = reason.or(current.reason.clone());
        sqlx::query("UPDATE follow_up_tasks SET status = 'Cancelled', reason = ?, updated_at = ? WHERE id = ?")
            .bind(&resolved_reason)
            .bind(to_sql_timestamp(&Utc::now()))
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "follow_up.task_cancelled",
                "follow_up_task",
                id,
                "Cancelled follow-up task",
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }

    async fn complete(
        &self,
        id: &str,
        completed_at: DateTime<Utc>,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        if fetch_one_task(&mut tx, id).await?.is_none() {
            return Ok(None);
        }
        sqlx::query(
            "UPDATE follow_up_tasks SET status = 'Completed', completed_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(to_sql_timestamp(&completed_at))
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "follow_up.task_completed",
                "follow_up_task",
                id,
                "Completed follow-up task",
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }

    async fn record_result(
        &self,
        id: &str,
        patch: FollowUpResultPatch,
        send_record: Option<NewSendRecord>,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let Some(current) = fetch_one_task(&mut tx, id).await? else {
            return Ok(None);
        };

        // The count advances once per contact-indicating outcome; a correction
        // of an already-counted result does not increment again.
        let follow_up_count = if patch.bump_count {
            current.follow_up_count + 1
        } else {
            current.follow_up_count
        };
        let status = patch.status.unwrap_or(current.status);
        let completed_at = patch
            .completed_at
            .map(|value| to_sql_timestamp(&value))
            .or(current.completed_at.map(|value| to_sql_timestamp(&value)));

        sqlx::query(concat!(
            "UPDATE follow_up_tasks SET result_type = ?, result_note = ?,",
            " result_recorded_at = ?, follow_up_count = ?, last_follow_up_at = ?,",
            " status = ?, completed_at = ?, updated_at = ? WHERE id = ?"
        ))
        .bind(&patch.result_type)
        .bind(&patch.result_note)
        .bind(to_sql_timestamp(&patch.result_recorded_at))
        .bind(follow_up_count)
        .bind(
            patch
                .last_follow_up_at
                .map(|value| to_sql_timestamp(&value)),
        )
        .bind(status.as_str())
        .bind(completed_at)
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "follow_up.result_recorded",
                "follow_up_task",
                id,
                format!("Recorded result '{}'", patch.result_type),
            )
            .with_metadata(json!({ "result_type": patch.result_type })),
        )
        .await
        .map_err(repo_error)?;

        if let Some(send_record) = &send_record {
            let now = to_sql_timestamp(&Utc::now());
            let send_id = crate::store::new_uuid();
            let sent_at = send_record
                .sent_at
                .map(|value| to_sql_timestamp(&value))
                .unwrap_or_else(|| now.clone());
            insert_send_record(&mut tx, &send_id, send_record, &sent_at, &now)
                .await
                .map_err(repo_error)?;
            insert_audit(
                &mut tx,
                AuditEntry::new(
                    "send.recorded",
                    "send_record",
                    &send_id,
                    format!(
                        "Recorded manual send with status '{}'",
                        send_record.status.as_str()
                    ),
                )
                .with_metadata(json!({
                    "candidate_id": send_record.candidate_id,
                    "task_id": send_record.task_id,
                    "draft_id": send_record.draft_id,
                    "channel": send_record.channel,
                    "status": send_record.status.as_str(),
                    "indicates_contact": send_record.status.indicates_contact(),
                })),
            )
            .await
            .map_err(repo_error)?;
        }

        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }

    async fn advance(
        &self,
        id: &str,
        target: ica_domain::FollowUpTaskStatus,
        reason: Option<String>,
    ) -> Result<Option<ica_domain::FollowUpTask>, RepositoryError> {
        self.advance_task_row(id, target, reason).await
    }

    async fn expire_stale(&self, now: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let now_sql = to_sql_timestamp(&now);
        let expired: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT id FROM follow_up_tasks WHERE expires_at IS NOT NULL AND expires_at < ? \
             AND status IN {EXPIRABLE_STATUSES}"
        ))
        .bind(&now_sql)
        .fetch_all(&mut *connection)
        .await
        .map_err(repo_error)?;
        if expired.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        for id in &expired {
            sqlx::query(
                "UPDATE follow_up_tasks SET status = 'Expired', updated_at = ? WHERE id = ?",
            )
            .bind(&now_sql)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
        }
        insert_audit(
            &mut tx,
            AuditEntry::without_entity(
                "follow_up.tasks_expired",
                format!("Expired {} stale follow-up task(s)", expired.len()),
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(expired.len() as u64)
    }

    async fn latest_message_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<Message>, RepositoryError> {
        let sql = concat!(
            "SELECT id, candidate_id, conversation_id, sender_type, content, message_time,",
            " channel, source, created_at FROM messages WHERE candidate_id = ?",
            " ORDER BY (message_time IS NULL), message_time DESC, created_at DESC LIMIT 1"
        );
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<MessageRow> = sqlx::query_as(sql)
            .bind(candidate_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(repo_error)?;
        row.map(MessageRow::into_message).transpose()
    }

    async fn contact_attempt_count(&self, candidate_id: &str) -> Result<i64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM follow_up_tasks WHERE candidate_id = ? AND status IN {CONTACT_STATUSES}"
        ))
        .bind(candidate_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(count)
    }

    async fn latest_contact_task(
        &self,
        candidate_id: &str,
    ) -> Result<Option<FollowUpTask>, RepositoryError> {
        let sql = format!(
            "{SELECT_COLUMNS} WHERE t.candidate_id = ? AND t.status IN {CONTACT_STATUSES} \
             ORDER BY (t.last_follow_up_at IS NULL), t.last_follow_up_at DESC LIMIT 1"
        );
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<FollowUpTaskRow> = sqlx::query_as(&sql)
            .bind(candidate_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(repo_error)?;
        row.map(FollowUpTaskRow::into_task).transpose()
    }

    async fn sole_sendable_draft_id(
        &self,
        task_id: &str,
    ) -> Result<Option<String>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM generated_drafts WHERE task_id = ? AND status IN ('approved', 'copied')",
        )
        .bind(task_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(repo_error)?;
        // Zero or several: pass None rather than guessing provenance.
        Ok(if ids.len() == 1 {
            ids.into_iter().next()
        } else {
            None
        })
    }

    async fn active_policy(&self) -> Result<Option<FollowUpPolicyRecord>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<PolicyRow> = sqlx::query_as(
            "SELECT id, name, first_follow_up_delay_hours, max_active_follow_ups, \
             min_interval_hours, task_validity_days, is_active FROM follow_up_policies \
             WHERE is_active = 1 ORDER BY created_at ASC LIMIT 1",
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(row.map(PolicyRow::into_record))
    }

    async fn policy_by_name(
        &self,
        name: &str,
    ) -> Result<Option<FollowUpPolicyRecord>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<PolicyRow> = sqlx::query_as(
            "SELECT id, name, first_follow_up_delay_hours, max_active_follow_ups, \
             min_interval_hours, task_validity_days, is_active FROM follow_up_policies \
             WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(row.map(PolicyRow::into_record))
    }

    async fn create_default_policy(&self) -> Result<FollowUpPolicyRecord, RepositoryError> {
        let policy = FollowUpPolicy::default();
        let id = crate::store::new_uuid();
        let now = to_sql_timestamp(&Utc::now());
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO follow_up_policies (id, name, first_follow_up_delay_hours,",
            " max_active_follow_ups, min_interval_hours, task_validity_days, is_active,",
            " created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, 'default', ?, ?, ?, ?, 1, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(policy.first_follow_up_delay_hours)
        .bind(policy.max_active_follow_ups)
        .bind(policy.min_interval_hours)
        .bind(policy.task_validity_days)
        .bind(&now)
        .bind(&now)
        .bind(LOCAL_OWNER_ID)
        .bind(DEFAULT_TENANT_ID)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(FollowUpPolicyRecord {
            id,
            name: "default".to_string(),
            policy,
            is_active: true,
        })
    }

    async fn update_policy(
        &self,
        id: &str,
        patch: &FollowUpPolicyPatch,
    ) -> Result<FollowUpPolicyRecord, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "UPDATE follow_up_policies SET",
            " first_follow_up_delay_hours = COALESCE(?, first_follow_up_delay_hours),",
            " max_active_follow_ups = COALESCE(?, max_active_follow_ups),",
            " min_interval_hours = COALESCE(?, min_interval_hours),",
            " task_validity_days = COALESCE(?, task_validity_days),",
            " updated_at = ? WHERE id = ?"
        ))
        .bind(patch.first_follow_up_delay_hours)
        .bind(patch.max_active_follow_ups)
        .bind(patch.min_interval_hours)
        .bind(patch.task_validity_days)
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        // The baseline stores the *applied* fields as metadata; an empty patch
        // stores NULL (`record_audit(metadata={})` — an empty dict is falsy),
        // and fields the request did not carry are omitted, not null.
        let mut entry = AuditEntry::new(
            "follow_up.policy_updated",
            "follow_up_policy",
            id,
            "Updated follow-up policy 'default'",
        );
        if !patch.is_empty() {
            let mut applied = serde_json::Map::new();
            for (key, value) in [
                (
                    "first_follow_up_delay_hours",
                    patch.first_follow_up_delay_hours,
                ),
                ("max_active_follow_ups", patch.max_active_follow_ups),
                ("min_interval_hours", patch.min_interval_hours),
                ("task_validity_days", patch.task_validity_days),
            ] {
                if let Some(value) = value {
                    applied.insert(key.to_string(), json!(value));
                }
            }
            entry = entry.with_metadata(serde_json::Value::Object(applied));
        }

        insert_audit(&mut tx, entry).await.map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<PolicyRow> = sqlx::query_as(
            "SELECT id, name, first_follow_up_delay_hours, max_active_follow_ups, \
             min_interval_hours, task_validity_days, is_active FROM follow_up_policies \
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(repo_error)?;
        row.map(PolicyRow::into_record)
            .ok_or_else(|| RepositoryError::unavailable("updated policy row disappeared"))
    }
}

#[async_trait]
impl DashboardRepository for SqliteStore {
    async fn expire_old_candidates(&self, cutoff: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let result = sqlx::query("DELETE FROM candidates WHERE archived = 1 AND updated_at < ?")
            .bind(to_sql_timestamp(&cutoff))
            .execute(&mut *connection)
            .await
            .map_err(repo_error)?;
        Ok(result.rows_affected())
    }

    async fn prune_audit_events(&self, cutoff: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let result = sqlx::query("DELETE FROM audit_events WHERE created_at < ?")
            .bind(to_sql_timestamp(&cutoff))
            .execute(&mut *connection)
            .await
            .map_err(repo_error)?;
        Ok(result.rows_affected())
    }

    async fn prune_stale_import_raw_text(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let result = sqlx::query(
            "UPDATE import_sessions SET raw_text = '' WHERE created_at < ? AND raw_text != ''",
        )
        .bind(to_sql_timestamp(&cutoff))
        .execute(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(result.rows_affected())
    }

    async fn candidate_total(&self) -> Result<i64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM candidates WHERE archived = 0")
            .fetch_one(&mut *connection)
            .await
            .map_err(repo_error)?;
        Ok(count)
    }

    async fn candidate_status_counts(&self) -> Result<Vec<(String, i64)>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT status, COUNT(*) FROM candidates WHERE archived = 0 GROUP BY status",
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(rows)
    }

    async fn pending_tasks(
        &self,
        statuses: &[FollowUpTaskStatus],
        limit: usize,
    ) -> Result<Vec<FollowUpTask>, RepositoryError> {
        let list = statuses
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "{SELECT_COLUMNS} WHERE t.status IN ({list}) \
             ORDER BY (t.recommended_at IS NULL), t.recommended_at ASC LIMIT ?"
        );
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let rows: Vec<FollowUpTaskRow> = sqlx::query_as(&sql)
            .bind(limit as i64)
            .fetch_all(&mut *connection)
            .await
            .map_err(repo_error)?;
        rows.into_iter().map(FollowUpTaskRow::into_task).collect()
    }

    async fn task_count_in(&self, statuses: &[FollowUpTaskStatus]) -> Result<i64, RepositoryError> {
        let list = statuses
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM follow_up_tasks WHERE status IN ({list})"
        ))
        .fetch_one(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(count)
    }

    async fn draft_count_in(&self, statuses: &[&str]) -> Result<i64, RepositoryError> {
        let list = statuses
            .iter()
            .map(|status| format!("'{status}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM generated_drafts WHERE status IN ({list})"
        ))
        .fetch_one(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(count)
    }

    async fn recent_messages(&self, limit: usize) -> Result<Vec<Message>, RepositoryError> {
        let sql = concat!(
            "SELECT id, candidate_id, conversation_id, sender_type, content, message_time,",
            " channel, source, created_at FROM messages",
            " ORDER BY created_at DESC LIMIT ?"
        );
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let rows: Vec<MessageRow> = sqlx::query_as(sql)
            .bind(limit as i64)
            .fetch_all(&mut *connection)
            .await
            .map_err(repo_error)?;
        rows.into_iter().map(MessageRow::into_message).collect()
    }

    async fn position_total(&self) -> Result<i64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE archived = 0")
            .fetch_one(&mut *connection)
            .await
            .map_err(repo_error)?;
        Ok(count)
    }

    async fn open_position_count(&self) -> Result<i64, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM positions WHERE archived = 0 AND status = 'Open'",
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(repo_error)?;
        Ok(count)
    }
}

/// The draft flows' task advance (`advance_task` in the baseline): walk the
/// shortest legal path, stamp every intermediate status, optionally set the
/// reason, and write ONE `follow_up.task_advanced` audit with the path.
impl SqliteStore {
    pub(crate) async fn advance_task_row(
        &self,
        id: &str,
        target: ica_domain::FollowUpTaskStatus,
        reason: Option<String>,
    ) -> Result<Option<ica_domain::FollowUpTask>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let Some(current) = fetch_one_task(&mut tx, id).await? else {
            return Ok(None);
        };
        if current.status == target {
            return Ok(Some(current));
        }
        let Some(path) = ica_domain::find_transition_path(current.status, target) else {
            return Err(RepositoryError::unavailable(format!(
                "A task in state {} cannot reach {}",
                current.status.as_str(),
                target.as_str()
            )));
        };
        let mut status = current.status;
        for step in &path {
            status = *step;
        }
        let resolved_reason = reason.or(current.reason.clone());
        sqlx::query(
            "UPDATE follow_up_tasks SET status = ?, reason = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(resolved_reason)
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "follow_up.task_advanced",
                "follow_up_task",
                id,
                format!(
                    "Advanced follow-up task {} → {}",
                    current.status.as_str(),
                    target.as_str()
                ),
            )
            .with_metadata(json!({
                "path": path.iter().map(|step| step.as_str()).collect::<Vec<_>>(),
            })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_task(&mut connection, id).await
    }
}
