//! The AI registry/usage adapters (W5): `ai_model_configs`,
//! `ai_model_routing` and `ai_usage_daily`.

use sqlx::FromRow;

use crate::store::{new_uuid, repo_error, to_sql_timestamp, SqliteStore};
use ica_application::ai_registry::{AiModelData, AiUsageRow};
use ica_application::ports::{AiModelRepository, AiUsageRepository, RepositoryError};

#[derive(Debug, FromRow)]
struct AiModelConfigRow {
    model_id: String,
    display_name: String,
    provider_type: String,
    base_url: String,
    api_key_env: String,
    quality_tier: String,
    input_cost_per_1k_tokens: f64,
    output_cost_per_1k_tokens: f64,
    max_output_tokens: i64,
    max_input_tokens: i64,
    timeout: i64,
    temperature: f64,
    /// SQLite hands booleans back as integers.
    enabled: i64,
    is_default: i64,
    sort_order: i64,
}

impl From<AiModelConfigRow> for AiModelData {
    fn from(row: AiModelConfigRow) -> Self {
        Self {
            model_id: row.model_id,
            display_name: row.display_name,
            provider_type: row.provider_type,
            base_url: row.base_url,
            api_key_env: row.api_key_env,
            quality_tier: row.quality_tier,
            input_cost_per_1k_tokens: row.input_cost_per_1k_tokens,
            output_cost_per_1k_tokens: row.output_cost_per_1k_tokens,
            max_output_tokens: row.max_output_tokens,
            max_input_tokens: row.max_input_tokens,
            timeout: row.timeout,
            temperature: row.temperature,
            enabled: row.enabled != 0,
            is_default: row.is_default != 0,
            sort_order: row.sort_order,
        }
    }
}

#[async_trait::async_trait]
impl AiModelRepository for SqliteStore {
    async fn list_configs(&self) -> Result<Vec<AiModelData>, RepositoryError> {
        let rows: Vec<AiModelConfigRow> = sqlx::query_as(
            "SELECT model_id, display_name, provider_type, base_url, api_key_env, \
             quality_tier, input_cost_per_1k_tokens, output_cost_per_1k_tokens, \
             max_output_tokens, max_input_tokens, timeout, temperature, enabled, \
             is_default, sort_order FROM ai_model_configs ORDER BY sort_order",
        )
        .fetch_all(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn routing_overrides(&self) -> Result<Vec<(String, String)>, RepositoryError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT message_type, tier FROM ai_model_routing")
                .fetch_all(self.pool())
                .await
                .map_err(repo_error)?;
        Ok(rows)
    }

    async fn replace_routing_override(
        &self,
        message_type: &str,
        tier: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        match tier {
            None => {
                sqlx::query("DELETE FROM ai_model_routing WHERE message_type = ?")
                    .bind(message_type)
                    .execute(&mut *tx)
                    .await
                    .map_err(repo_error)?;
            }
            Some(tier) => {
                let now = to_sql_timestamp(&chrono::Utc::now());
                let outcome = sqlx::query(
                    "UPDATE ai_model_routing SET tier = ?, updated_at = ? \
                     WHERE message_type = ?",
                )
                .bind(tier)
                .bind(&now)
                .bind(message_type)
                .execute(&mut *tx)
                .await
                .map_err(repo_error)?;
                if outcome.rows_affected() == 0 {
                    sqlx::query(concat!(
                        "INSERT INTO ai_model_routing (message_type, tier, id,",
                        " created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
                    ))
                    .bind(message_type)
                    .bind(tier)
                    .bind(new_uuid())
                    .bind(&now)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(repo_error)?;
                }
            }
        }
        tx.commit().await.map_err(repo_error)
    }
}

#[derive(Debug, FromRow)]
struct AiUsageRowRaw {
    usage_date: String,
    model_id: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    estimated_cost_cents: i64,
    request_count: i64,
}

impl From<AiUsageRowRaw> for AiUsageRow {
    fn from(row: AiUsageRowRaw) -> Self {
        Self {
            usage_date: row.usage_date,
            model_id: row.model_id,
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            estimated_cost_cents: row.estimated_cost_cents,
            request_count: row.request_count,
        }
    }
}

#[async_trait::async_trait]
impl AiUsageRepository for SqliteStore {
    async fn list_rows(&self) -> Result<Vec<AiUsageRow>, RepositoryError> {
        let rows: Vec<AiUsageRowRaw> = sqlx::query_as(
            "SELECT usage_date, model_id, prompt_tokens, completion_tokens, \
             estimated_cost_cents, request_count FROM ai_usage_daily \
             ORDER BY usage_date",
        )
        .fetch_all(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
