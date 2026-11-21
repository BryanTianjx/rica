//! The `app_settings` adapter (W5 settings slice).
//!
//! The switch lives in one row (key `ai.egress_consent`, `value_type` bool);
//! reading it applies the baseline's truthy set, and writing it shares one
//! transaction with its audit event so the trail can never describe a change
//! that rolled back.

use sqlx::SqliteConnection;

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{new_uuid, repo_error, to_sql_timestamp, SqliteStore, DEFAULT_TENANT_ID};
use ica_application::ports::{AppSettingsRepository, RepositoryError};

/// The baseline's `_TRUTHY` set (lowercased comparison).
const TRUTHY: [&str; 4] = ["1", "true", "yes", "on"];

async fn read_consent_value(
    connection: &mut SqliteConnection,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT value FROM app_settings WHERE \"key\" = ?")
            .bind(ica_application::AI_EGRESS_CONSENT_KEY)
            .fetch_optional(&mut *connection)
            .await?;
    Ok(row.and_then(|(value,)| value))
}

async fn upsert_consent(
    connection: &mut SqliteConnection,
    granted: bool,
) -> Result<(), sqlx::Error> {
    let now = to_sql_timestamp(&chrono::Utc::now());
    let value = if granted { "true" } else { "false" };
    let outcome = sqlx::query(
        "UPDATE app_settings SET value = ?, value_type = 'bool', updated_at = ? \
         WHERE \"key\" = ?",
    )
    .bind(value)
    .bind(&now)
    .bind(ica_application::AI_EGRESS_CONSENT_KEY)
    .execute(&mut *connection)
    .await?;
    if outcome.rows_affected() == 0 {
        sqlx::query(concat!(
            "INSERT INTO app_settings (\"key\", value, value_type, id,",
            " created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, ?, 'bool', ?, ?, ?, ?, ?)"
        ))
        .bind(ica_application::AI_EGRESS_CONSENT_KEY)
        .bind(value)
        .bind(new_uuid())
        .bind(&now)
        .bind(&now)
        .bind("00000000-0000-0000-0000-000000000001")
        .bind(DEFAULT_TENANT_ID)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

#[async_trait::async_trait]
impl AppSettingsRepository for SqliteStore {
    async fn egress_consent_granted(&self) -> Result<bool, RepositoryError> {
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let value = read_consent_value(&mut connection)
            .await
            .map_err(repo_error)?;
        // No row (or a NULL value) defaults to allowed: configuring a
        // provider key is itself an explicit operator action.
        let Some(value) = value else {
            return Ok(true);
        };
        Ok(TRUTHY.contains(&value.trim().to_lowercase().as_str()))
    }

    async fn set_egress_consent(&self, granted: bool) -> Result<(), RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        upsert_consent(&mut tx, granted).await.map_err(repo_error)?;
        let summary = if granted {
            "AI 出网已开启：脱敏后的候选人上下文可发送至所配置的供应商"
        } else {
            "AI 出网已关闭：所有 AI 草稿生成被拒绝"
        };
        insert_audit(
            &mut tx,
            AuditEntry {
                event_type: if granted {
                    ica_application::EGRESS_GRANTED_EVENT
                } else {
                    ica_application::EGRESS_REVOKED_EVENT
                },
                entity_type: Some("setting"),
                entity_id: Some(ica_application::AI_EGRESS_CONSENT_KEY),
                summary: summary.to_string(),
                metadata: Some(serde_json::json!({ "consent_granted": granted })),
            },
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)
    }

    async fn upsert_values(&self, entries: &[(&str, String, &str)]) -> Result<(), RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        for (key, value, value_type) in entries {
            let now = to_sql_timestamp(&chrono::Utc::now());
            let outcome = sqlx::query(
                "UPDATE app_settings SET value = ?, value_type = ?, updated_at = ?                  WHERE \"key\" = ?",
            )
            .bind(value)
            .bind(value_type)
            .bind(&now)
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
            if outcome.rows_affected() == 0 {
                sqlx::query(concat!(
                    "INSERT INTO app_settings (\"key\", value, value_type, id,",
                    " created_at, updated_at, owner_id, tenant_id)",
                    " VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
                ))
                .bind(key)
                .bind(value)
                .bind(value_type)
                .bind(new_uuid())
                .bind(&now)
                .bind(&now)
                .bind("00000000-0000-0000-0000-000000000001")
                .bind(DEFAULT_TENANT_ID)
                .execute(&mut *tx)
                .await
                .map_err(repo_error)?;
            }
        }
        tx.commit().await.map_err(repo_error)
    }

    async fn record_ai_key_audit(
        &self,
        event_type: &str,
        summary: String,
        metadata: serde_json::Value,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        crate::audit::insert_audit(
            &mut tx,
            crate::audit::AuditEntry {
                event_type,
                entity_type: Some("settings"),
                entity_id: None,
                summary,
                metadata: Some(metadata),
            },
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)
    }
}
