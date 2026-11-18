//! SQLite implementation of the import-session port.
//!
//! Two transaction boundaries carry contract weight:
//!
//! * `create_preview` writes the session, its records and the
//!   `import.previewed` audit in one transaction. Preview is *not* read-only —
//!   it persists the parser's reading — but it must never write `messages`.
//! * `confirm` writes every message, stamps the session and records
//!   `import.confirmed` in one transaction, so a failure part-way leaves no
//!   partial conversation behind (measured: an unknown `conversation_id` gives
//!   404 with a zero message delta).

use async_trait::async_trait;
use chrono::Utc;
use ica_application::{
    ImportConfirmOutcome, ImportRecordCorrection, ImportRepository, NewImportSessionRow,
    RepositoryError,
};
use ica_domain::{ImportRecord, ImportSession, ImportStatus, RecordStatus};
use serde_json::json;
use sqlx::{FromRow, SqliteConnection};

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{
    new_uuid, repo_error, to_sql_timestamp, SqliteStore, DEFAULT_TENANT_ID, LOCAL_OWNER_ID,
};

#[derive(Debug, FromRow)]
struct SessionRow {
    id: String,
    candidate_id: Option<String>,
    source: String,
    status: String,
    detected_duplicate_count: i64,
    confirmed_at: Option<String>,
    error_message: Option<String>,
    created_at: String,
}

#[derive(Debug, FromRow)]
struct RecordRow {
    id: String,
    session_id: String,
    sequence: i64,
    sender_type: Option<String>,
    content: String,
    message_time: Option<String>,
    status: String,
    duplicate_of_message_id: Option<String>,
    content_hash: Option<String>,
    validation_messages: Option<String>,
}

const SESSION_COLUMNS: &str = "id, candidate_id, source, status, detected_duplicate_count, \
 confirmed_at, error_message, created_at";

const RECORD_COLUMNS: &str = "id, session_id, sequence, sender_type, content, message_time, \
 status, duplicate_of_message_id, content_hash, validation_messages";

fn parse_timestamp(raw: &str) -> chrono::DateTime<Utc> {
    crate::time::parse_sqlite_datetime(raw).unwrap_or_else(|_| Utc::now())
}

fn optional_timestamp(raw: Option<&String>) -> Option<chrono::DateTime<Utc>> {
    raw.map(|value| parse_timestamp(value))
}

fn into_session(row: SessionRow, records: Vec<RecordRow>) -> ImportSession {
    ImportSession {
        status: ImportStatus::parse(&row.status).unwrap_or(ImportStatus::Pending),
        id: row.id,
        candidate_id: row.candidate_id,
        source: row.source,
        detected_duplicate_count: row.detected_duplicate_count,
        confirmed_at: optional_timestamp(row.confirmed_at.as_ref()),
        error_message: row.error_message,
        created_at: parse_timestamp(&row.created_at),
        records: records
            .into_iter()
            .map(|record| ImportRecord {
                status: RecordStatus::parse(&record.status).unwrap_or(RecordStatus::Pending),
                id: record.id,
                session_id: record.session_id,
                sequence: record.sequence,
                sender_type: record.sender_type,
                content: record.content,
                message_time: optional_timestamp(record.message_time.as_ref()),
                duplicate_of_message_id: record.duplicate_of_message_id,
                content_hash: record.content_hash,
                validation_messages: record.validation_messages,
            })
            .collect(),
    }
}

async fn fetch_session(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<ImportSession>, RepositoryError> {
    let row: Option<SessionRow> = sqlx::query_as(&format!(
        "SELECT {SESSION_COLUMNS} FROM import_sessions WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(repo_error)?;

    let Some(row) = row else { return Ok(None) };

    let records: Vec<RecordRow> = sqlx::query_as(&format!(
        "SELECT {RECORD_COLUMNS} FROM import_records WHERE session_id = ? ORDER BY sequence ASC"
    ))
    .bind(id)
    .fetch_all(&mut *connection)
    .await
    .map_err(repo_error)?;

    Ok(Some(into_session(row, records)))
}

/// The candidate's active conversation, creating one on demand
/// (`ensure_conversation`). An explicit id that does not exist is a 404; one
/// that belongs to another candidate is a 422.
async fn ensure_conversation(
    connection: &mut SqliteConnection,
    candidate_id: &str,
    conversation_id: Option<&str>,
) -> Result<String, RepositoryError> {
    if let Some(id) = conversation_id {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT candidate_id FROM conversations WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(repo_error)?;
        match owner {
            None => {
                return Err(RepositoryError::conflict(
                    format!("Conversation '{id}' was not found"),
                    std::collections::BTreeMap::new(),
                ));
            }
            Some(owner) if owner != candidate_id => {
                return Err(RepositoryError::conflict(
                    "Conversation does not belong to the given candidate".to_string(),
                    [
                        ("conversation_id".to_string(), id.to_string()),
                        ("candidate_id".to_string(), candidate_id.to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ));
            }
            Some(_) => return Ok(id.to_string()),
        }
    }

    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM conversations WHERE candidate_id = ? AND is_active = 1 \
         ORDER BY last_message_at IS NULL, last_message_at DESC LIMIT 1",
    )
    .bind(candidate_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(repo_error)?;
    if let Some(id) = existing {
        return Ok(id);
    }

    let id = new_uuid();
    let now = to_sql_timestamp(&Utc::now());
    sqlx::query(
        "INSERT INTO conversations (id, candidate_id, channel, title, is_active, \
         last_message_at, created_at, updated_at, owner_id, tenant_id) \
         VALUES (?, ?, 'boss', NULL, 1, NULL, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(candidate_id)
    .bind(&now)
    .bind(&now)
    .bind(LOCAL_OWNER_ID)
    .bind(DEFAULT_TENANT_ID)
    .execute(&mut *connection)
    .await
    .map_err(repo_error)?;
    Ok(id)
}

#[async_trait]
impl ImportRepository for SqliteStore {
    async fn candidate_exists(&self, id: &str) -> Result<bool, RepositoryError> {
        let found: Option<String> = sqlx::query_scalar("SELECT id FROM candidates WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(repo_error)?;
        Ok(found.is_some())
    }

    async fn message_hashes(
        &self,
        candidate_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, RepositoryError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT content_hash, id FROM messages WHERE candidate_id = ? \
             AND content_hash IS NOT NULL",
        )
        .bind(candidate_id)
        .fetch_all(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(rows.into_iter().collect())
    }

    async fn create_preview(
        &self,
        new: NewImportSessionRow,
    ) -> Result<ImportSession, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let session_id = new_uuid();
        let now = to_sql_timestamp(&Utc::now());

        sqlx::query(
            "INSERT INTO import_sessions (id, candidate_id, source, raw_text, status, \
             detected_duplicate_count, confirmed_at, error_message, created_at, updated_at, \
             owner_id, tenant_id) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        )
        .bind(&session_id)
        .bind(&new.candidate_id)
        .bind(&new.source)
        .bind(&new.raw_text)
        .bind(new.status.as_str())
        .bind(new.detected_duplicate_count)
        .bind(&new.error_message)
        .bind(&now)
        .bind(&now)
        .bind(LOCAL_OWNER_ID)
        .bind(DEFAULT_TENANT_ID)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        for record in &new.records {
            sqlx::query(
                "INSERT INTO import_records (id, session_id, sequence, sender_type, content, \
                 message_time, status, duplicate_of_message_id, content_hash, \
                 validation_messages, created_at, updated_at, owner_id, tenant_id) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_uuid())
            .bind(&session_id)
            .bind(record.sequence)
            .bind(&record.sender_type)
            .bind(&record.content)
            .bind(record.message_time.map(|t| to_sql_timestamp(&t)))
            .bind(record.status.as_str())
            .bind(&record.duplicate_of_message_id)
            .bind(&record.content_hash)
            .bind(&record.validation_messages)
            .bind(&now)
            .bind(&now)
            .bind(LOCAL_OWNER_ID)
            .bind(DEFAULT_TENANT_ID)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
        }

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "import.previewed",
                "import_session",
                &session_id,
                format!(
                    "Prepared import preview with {} record(s)",
                    new.records.len()
                ),
            )
            .with_metadata(json!({
                "records": new.records.len(),
                "duplicates": new.detected_duplicate_count,
            })),
        )
        .await
        .map_err(repo_error)?;

        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_session(&mut connection, &session_id)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("preview vanished after insert"))
    }

    async fn find(&self, id: &str) -> Result<Option<ImportSession>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_session(&mut connection, id).await
    }

    async fn confirm(
        &self,
        session_id: &str,
        candidate_id: &str,
        conversation_id: Option<String>,
        corrections: Vec<ImportRecordCorrection>,
    ) -> Result<ImportConfirmOutcome, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;

        // The candidate must exist; the baseline's get_candidate_accessible
        // raises NotFound first.
        let candidate: Option<String> =
            sqlx::query_scalar("SELECT id FROM candidates WHERE id = ?")
                .bind(candidate_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(repo_error)?;
        if candidate.is_none() {
            // The baseline calls `get_candidate_accessible`, which raises
            // `NotFoundError` — a 404 with this exact message. A conflict
            // (409) would be the wrong envelope for "no such candidate".
            return Err(RepositoryError::not_found(format!(
                "Candidate '{candidate_id}' was not found"
            )));
        }

        let conversation =
            ensure_conversation(&mut tx, candidate_id, conversation_id.as_deref()).await?;

        let rows: Vec<RecordRow> = sqlx::query_as(&format!(
            "SELECT {RECORD_COLUMNS} FROM import_records WHERE session_id = ? \
             ORDER BY sequence ASC"
        ))
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(repo_error)?;

        let by_id: std::collections::HashMap<&str, &ImportRecordCorrection> =
            corrections.iter().map(|c| (c.id.as_str(), c)).collect();

        let mut saved: Vec<String> = Vec::new();
        let mut skipped = 0i64;
        let mut duplicates = 0i64;
        let now = to_sql_timestamp(&Utc::now());

        for row in &rows {
            let correction = by_id.get(row.id.as_str());
            if let Some(correction) = correction {
                if correction.skip {
                    skipped += 1;
                    continue;
                }
            }

            let mut sender_type = row.sender_type.clone();
            let mut content = row.content.clone();
            let mut message_time = row.message_time.clone();

            if let Some(correction) = correction {
                if let Some(value) = correction.sender_type {
                    sender_type = Some(value.as_str().to_string());
                }
                if let Some(value) = &correction.content {
                    content = value.clone();
                }
                if let Some(value) = correction.message_time {
                    message_time = Some(to_sql_timestamp(&value));
                }
            }

            // An unflagged duplicate is skipped and counted twice (duplicates
            // folds into skipped), exactly like the baseline.
            if row.status == RecordStatus::Duplicate.as_str() && correction.is_none() {
                duplicates += 1;
                skipped += 1;
                continue;
            }

            let message_id = new_uuid();
            sqlx::query(
                "INSERT INTO messages (id, candidate_id, conversation_id, sender_type, content, \
                 message_time, channel, source, content_hash, import_record_id, created_at, \
                 updated_at, owner_id, tenant_id) \
                 VALUES (?, ?, ?, ?, ?, ?, 'boss', 'paste_import', ?, ?, ?, ?, ?, ?)",
            )
            .bind(&message_id)
            .bind(candidate_id)
            .bind(&conversation)
            .bind(&sender_type)
            .bind(&content)
            .bind(&message_time)
            .bind(&row.content_hash)
            .bind(&row.id)
            .bind(&now)
            .bind(&now)
            .bind(LOCAL_OWNER_ID)
            .bind(DEFAULT_TENANT_ID)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
            saved.push(message_id);
        }

        // Confirming blanks `raw_text`: the verbatim page is the richest PII
        // payload in the database and nothing reads it after confirmation —
        // the saved messages come from the parsed records. The baseline drops
        // it in the SAME transaction that commits the import, so a crash
        // cannot leave it behind.
        sqlx::query(
            "UPDATE import_sessions SET status = 'confirmed', confirmed_at = ?, raw_text = '', updated_at = ? \
             WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "import.confirmed",
                "import_session",
                session_id,
                format!("Confirmed import with {} message(s)", saved.len()),
            )
            // The baseline's metadata keys, in its insertion order: Python's
            // `json.dumps` does not sort them.
            .with_metadata(json!({
                "saved": saved.len(),
                "skipped": skipped,
                "duplicates": duplicates,
            })),
        )
        .await
        .map_err(repo_error)?;

        tx.commit().await.map_err(repo_error)?;

        Ok(ImportConfirmOutcome {
            session_id: session_id.to_string(),
            saved_message_ids: saved,
            skipped_count: skipped,
            duplicate_count: duplicates,
        })
    }

    async fn reject(&self, id: &str) -> Result<Option<ImportSession>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let now = to_sql_timestamp(&Utc::now());
        // Rejecting clears the raw text: nothing will ever be parsed from this
        // page again. The records stay, which is why a rejected session can
        // still be confirmed (the baseline behaves the same way).
        let affected = sqlx::query(
            "UPDATE import_sessions SET status = 'rejected', raw_text = '', updated_at = ? \
             WHERE id = ?",
        )
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?
        .rows_affected();
        if affected == 0 {
            return Ok(None);
        }
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "import.rejected",
                "import_session",
                id,
                "Rejected import session",
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_session(&mut connection, id).await
    }
}
