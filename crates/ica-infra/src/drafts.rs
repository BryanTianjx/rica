//! SQLite implementation of the draft repository port, plus the W3
//! not-configured AI generator (the real provider call belongs to W5).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ica_application::{
    ApplicationError, DraftFilter, DraftGenerator, DraftRepository, RepositoryError,
};
use ica_domain::{GeneratedDraft, Message};
use serde_json::json;
use sqlx::{FromRow, SqliteConnection};

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{repo_error, to_sql_timestamp, SqliteStore};

/// The `ai.egress_consent` setting key (`app/application/ai_consent.py`).
const EGRESS_CONSENT_KEY: &str = "ai.egress_consent";
/// The V1.0 default model (`settings.ai_model`) — the no-key 503 message names
/// it. W5 replaces this whole generator with the real provider call.
const DEFAULT_AI_MODEL: &str = "glm-4-flash";

/// The `ai_provider_error` the baseline raises for a model id it cannot resolve.
///
/// `app/ai/registry.py::get_model` raises `AIProviderError` with the id in
/// `details`, so the envelope carries `{"model_id": …}` — not `{}`.
fn unknown_model(model_id: &str) -> ApplicationError {
    ApplicationError::ProviderUnavailable {
        message: format!("Unknown model '{model_id}'. No matching configuration found."),
        details: serde_json::json!({ "model_id": model_id }),
    }
}

/// Resolve the model a request names, the way the baseline's registry does
/// when `ai_model_configs` is empty.
///
/// The registry builds its catalogue from the `ai_model_configs` table and
/// falls back to the single `settings.ai_model` when that table has no rows
/// (`app/ai/registry.py`). The contract database has zero rows, so exactly one
/// id is known: [`DEFAULT_AI_MODEL`]. Any other requested id is a 503 naming
/// it — the baseline refuses rather than quietly using the default, and a
/// silent fall back would make `model_id`/`comparison_models` look accepted
/// when they were not.
///
/// Resolution order matches `app/ai/service.py`: `comparison_models` wins over
/// `model_id`, and its ids are checked in order so the first unknown one is the
/// one reported.
fn resolve_requested_model(
    request: &ica_application::ai_context::GenerateDraftRequest,
) -> Result<&str, ApplicationError> {
    if !request.comparison_models.is_empty() {
        for model_id in &request.comparison_models {
            if model_id != DEFAULT_AI_MODEL {
                return Err(unknown_model(model_id));
            }
        }
        return Ok(DEFAULT_AI_MODEL);
    }
    if let Some(model_id) = &request.model_id {
        if model_id != DEFAULT_AI_MODEL {
            return Err(unknown_model(model_id));
        }
    }
    Ok(DEFAULT_AI_MODEL)
}

#[derive(Debug, FromRow)]
struct DraftRow {
    id: String,
    candidate_id: String,
    task_id: Option<String>,
    message_type: String,
    tone: String,
    status: String,
    objective: Option<String>,
    message_text: String,
    alternatives_json: String,
    context_references_json: String,
    uncertainties_json: String,
    warnings_json: String,
    validation_status: String,
    validation_messages_json: String,
    quality_status: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    prompt_version: Option<String>,
    is_edited: i64,
    approved_at: Option<String>,
    copied_at: Option<String>,
    rejected_at: Option<String>,
    draft_group_id: Option<String>,
    cost_cents: Option<i64>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    routing_decision_json: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DraftRow {
    fn time(
        &self,
        column: &str,
        raw: Option<&String>,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        match raw {
            None => Ok(None),
            Some(value) => crate::time::parse_sqlite_datetime(value)
                .map(Some)
                .map_err(|error| {
                    RepositoryError::unavailable(format!(
                        "generated_drafts.{column} for id '{}': {error}",
                        self.id
                    ))
                }),
        }
    }

    fn into_draft(self) -> Result<GeneratedDraft, RepositoryError> {
        let status = ica_domain::DraftStatus::parse(&self.status).ok_or_else(|| {
            RepositoryError::unavailable(format!(
                "generated_drafts.status for id '{}' holds an unknown value: '{}'",
                self.id, self.status
            ))
        })?;
        // Bind the timestamp strings before the struct move: `time` borrows
        // `self`, and the struct below moves its fields.
        let approved_at = self.time("approved_at", self.approved_at.as_ref())?;
        let copied_at = self.time("copied_at", self.copied_at.as_ref())?;
        let rejected_at = self.time("rejected_at", self.rejected_at.as_ref())?;
        let created_at = self
            .time("created_at", Some(&self.created_at))?
            .ok_or_else(|| {
                RepositoryError::unavailable(format!(
                    "generated_drafts.created_at for id '{}' is NULL",
                    self.id
                ))
            })?;
        let updated_at = self
            .time("updated_at", Some(&self.updated_at))?
            .ok_or_else(|| {
                RepositoryError::unavailable(format!(
                    "generated_drafts.updated_at for id '{}' is NULL",
                    self.id
                ))
            })?;
        Ok(GeneratedDraft {
            id: self.id.clone(),
            candidate_id: self.candidate_id,
            task_id: self.task_id,
            message_type: self.message_type,
            tone: self.tone,
            status,
            objective: self.objective,
            message_text: self.message_text,
            alternatives_json: self.alternatives_json,
            context_references_json: self.context_references_json,
            uncertainties_json: self.uncertainties_json,
            warnings_json: self.warnings_json,
            validation_status: self.validation_status,
            validation_messages_json: self.validation_messages_json,
            quality_status: self.quality_status,
            provider: self.provider,
            model: self.model,
            prompt_version: self.prompt_version,
            is_edited: self.is_edited != 0,
            approved_at,
            copied_at,
            rejected_at,
            draft_group_id: self.draft_group_id,
            cost_cents: self.cost_cents,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            routing_decision_json: self.routing_decision_json,
            created_at,
            updated_at,
        })
    }
}

const SELECT_COLUMNS: &str = concat!(
    "SELECT id, candidate_id, task_id, message_type, tone, status, objective,",
    " message_text, alternatives_json, context_references_json, uncertainties_json,",
    " warnings_json, validation_status, validation_messages_json, quality_status,",
    " provider, model, prompt_version, is_edited, approved_at, copied_at, rejected_at,",
    " draft_group_id, cost_cents, prompt_tokens, completion_tokens, routing_decision_json,",
    " created_at, updated_at",
    " FROM generated_drafts",
);

async fn fetch_one_draft(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<GeneratedDraft>, RepositoryError> {
    let row: Option<DraftRow> = sqlx::query_as(&format!("{SELECT_COLUMNS} WHERE id = ?"))
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(repo_error)?;
    row.map(DraftRow::into_draft).transpose()
}

/// The W3 AI generator: no provider is configured, so every call answers the
/// baseline's exact not-configured error (503 `ai_provider_error`, measured in
/// `docs/contract-baseline/w3-python-facts.md` §5).
pub struct NotConfiguredDraftGenerator;

#[async_trait]
impl DraftGenerator for NotConfiguredDraftGenerator {
    async fn generate(
        &self,
        request: ica_application::ai_context::GenerateDraftRequest,
    ) -> Result<GeneratedDraft, ApplicationError> {
        // An unresolvable model is reported before the key is even looked for,
        // exactly like the baseline (`registry.get_model` runs first).
        let model = resolve_requested_model(&request)?;
        // The no-key error carries no details, so the envelope's `details` is
        // an empty object — Python serialises it as `{}`, not `null`.
        Err(ApplicationError::ProviderUnavailable {
            message: format!("No API key found for model '{model}' in env var 'AI_API_KEY'"),
            details: serde_json::json!({}),
        })
    }
}

#[async_trait]
impl DraftRepository for SqliteStore {
    async fn list(
        &self,
        filter: DraftFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<GeneratedDraft>, usize), RepositoryError> {
        let mut clauses: Vec<&str> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(candidate_id) = &filter.candidate_id {
            clauses.push("candidate_id = ?");
            binds.push(candidate_id.clone());
        }
        if let Some(task_id) = &filter.task_id {
            clauses.push("task_id = ?");
            binds.push(task_id.clone());
        }
        if let Some(status) = filter.status {
            clauses.push("status = ?");
            binds.push(status.as_str().to_string());
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let count_sql = format!("SELECT COUNT(*) FROM generated_drafts{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value);
        }
        let total: i64 = count_query
            .fetch_one(&mut *connection)
            .await
            .map_err(repo_error)?;

        let list_sql =
            format!("{SELECT_COLUMNS}{where_sql} ORDER BY created_at DESC LIMIT ? OFFSET ?");
        let mut list_query = sqlx::query_as::<_, DraftRow>(&list_sql);
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
            .map(DraftRow::into_draft)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as usize))
    }

    async fn find(&self, id: &str) -> Result<Option<GeneratedDraft>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_draft(&mut connection, id).await
    }

    async fn find_group(&self, group_id: &str) -> Result<Vec<GeneratedDraft>, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let rows: Vec<DraftRow> =
            sqlx::query_as(&format!("{SELECT_COLUMNS} WHERE draft_group_id = ?"))
                .bind(group_id)
                .fetch_all(&mut *connection)
                .await
                .map_err(repo_error)?;
        rows.into_iter().map(DraftRow::into_draft).collect()
    }

    async fn egress_consent(&self) -> Result<bool, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let row: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
                .bind(EGRESS_CONSENT_KEY)
                .fetch_optional(&mut *connection)
                .await
                .map_err(repo_error)?;
        // Defaults to allowed when nothing is recorded (PRIV-07).
        Ok(match row {
            None => true,
            Some(value) => matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
        })
    }

    async fn update(
        &self,
        id: &str,
        message_text: Option<String>,
        objective: Option<String>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let Some(current) = fetch_one_draft(&mut tx, id).await? else {
            return Ok(None);
        };
        // `message_text` present: the wording was touched, `is_edited` marks it
        // and `generated` advances to `edited`; the validator's findings stay
        // visible next to the edited version.
        let (message_text, is_edited, status) = match message_text {
            Some(text) => (
                text,
                true,
                match current.status {
                    ica_domain::DraftStatus::Generated => ica_domain::DraftStatus::Edited,
                    other => other,
                },
            ),
            None => (
                current.message_text.clone(),
                current.is_edited,
                current.status,
            ),
        };
        let objective = objective.or(current.objective.clone());

        sqlx::query(concat!(
            "UPDATE generated_drafts SET message_text = ?, objective = ?, is_edited = ?,",
            " status = ?, updated_at = ? WHERE id = ?"
        ))
        .bind(&message_text)
        .bind(&objective)
        .bind(is_edited as i64)
        .bind(status.as_str())
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new("draft.edited", "draft", id, "Edited draft text"),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_draft(&mut connection, id).await
    }

    async fn approve(
        &self,
        id: &str,
        approved_at: DateTime<Utc>,
        note: Option<String>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        if fetch_one_draft(&mut tx, id).await?.is_none() {
            return Ok(None);
        }
        sqlx::query(concat!(
            "UPDATE generated_drafts SET status = 'approved', approved_at = ?, updated_at = ?",
            " WHERE id = ?"
        ))
        .bind(to_sql_timestamp(&approved_at))
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "draft.approved",
                "draft",
                id,
                "Approved draft (approval only — no message was sent)",
            )
            .with_metadata(json!({ "note": note })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_draft(&mut connection, id).await
    }

    async fn reject(
        &self,
        id: &str,
        rejected_at: DateTime<Utc>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        if fetch_one_draft(&mut tx, id).await?.is_none() {
            return Ok(None);
        }
        sqlx::query(concat!(
            "UPDATE generated_drafts SET status = 'rejected', rejected_at = ?, updated_at = ?",
            " WHERE id = ?"
        ))
        .bind(to_sql_timestamp(&rejected_at))
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new("draft.rejected", "draft", id, "Rejected draft"),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_draft(&mut connection, id).await
    }

    async fn copy(
        &self,
        id: &str,
        copied_at: DateTime<Utc>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        if fetch_one_draft(&mut tx, id).await?.is_none() {
            return Ok(None);
        }
        sqlx::query(concat!(
            "UPDATE generated_drafts SET status = 'copied', copied_at = ?, updated_at = ?",
            " WHERE id = ?"
        ))
        .bind(to_sql_timestamp(&copied_at))
        .bind(to_sql_timestamp(&Utc::now()))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "draft.copied",
                "draft",
                id,
                "Copied draft (copy is not a send)",
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        fetch_one_draft(&mut connection, id).await
    }

    async fn supersede_others(
        &self,
        group_id: &str,
        selected_id: &str,
        rejected_at: DateTime<Utc>,
    ) -> Result<Vec<GeneratedDraft>, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let others: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM generated_drafts WHERE draft_group_id = ? AND id != ?",
        )
        .bind(group_id)
        .bind(selected_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(repo_error)?;
        for id in &others {
            sqlx::query(concat!(
                "UPDATE generated_drafts SET status = 'rejected', rejected_at = ?,",
                " updated_at = ? WHERE id = ?"
            ))
            .bind(to_sql_timestamp(&rejected_at))
            .bind(to_sql_timestamp(&Utc::now()))
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
            insert_audit(
                &mut tx,
                AuditEntry::new(
                    "draft.rejected",
                    "draft",
                    id,
                    "Draft rejected as superseded by a group member",
                )
                .with_metadata(json!({
                    "superseded_by": selected_id,
                    "reason": "superseded_by_group_member",
                })),
            )
            .await
            .map_err(repo_error)?;
        }
        tx.commit().await.map_err(repo_error)?;

        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let rows: Vec<DraftRow> =
            sqlx::query_as(&format!("{SELECT_COLUMNS} WHERE draft_group_id = ?"))
                .bind(group_id)
                .fetch_all(&mut *connection)
                .await
                .map_err(repo_error)?;
        rows.into_iter().map(DraftRow::into_draft).collect()
    }
}

#[async_trait]
impl DraftGenerator for SqliteStore {
    async fn generate(
        &self,
        request: ica_application::ai_context::GenerateDraftRequest,
    ) -> Result<GeneratedDraft, ApplicationError> {
        let model = resolve_requested_model(&request)?;
        Err(ApplicationError::ProviderUnavailable {
            message: format!("No API key found for model '{model}' in env var 'AI_API_KEY'"),
            details: serde_json::json!({}),
        })
    }
}

/// The AI context builder's message read (W3): oldest first, capped.
pub(crate) async fn context_messages(
    pool: &sqlx::SqlitePool,
    candidate_id: &str,
    limit: i64,
) -> Result<Vec<Message>, RepositoryError> {
    let mut connection = pool.acquire().await.map_err(repo_error)?;
    let rows: Vec<MessageRow> = sqlx::query_as(concat!(
        "SELECT id, candidate_id, conversation_id, sender_type, content, message_time,",
        " channel, source, created_at FROM messages WHERE candidate_id = ?",
        " ORDER BY (message_time IS NULL), message_time ASC, created_at ASC LIMIT ?"
    ))
    .bind(candidate_id)
    .bind(limit)
    .fetch_all(&mut *connection)
    .await
    .map_err(repo_error)?;
    rows.into_iter().map(MessageRow::into_message).collect()
}

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

/// The imports the adapter shares with the other SQLite adapters.
use crate::store::offset_bind;
