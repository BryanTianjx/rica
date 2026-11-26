//! Rust-owned storage for structured authorization decisions (P1 plan §10).
//!
//! This table is intentionally separate from `audit_events`: the latter is a
//! frozen Python-baseline table compared byte-for-byte by the contract gates,
//! while authorization decisions are defined only by this repository. It is
//! created by canonical migration v2 and never contains a token, API key,
//! cookie, request body or unwrapped personal data.

use async_trait::async_trait;
use ica_application::ports::{AuthzDecision, AuthzDecisionRecorder};
use ica_application::RepositoryError;
use sqlx::SqlitePool;

use crate::store::{repo_error, to_sql_timestamp};

/// SQLite recorder over the same pool as the business adapters.
#[derive(Clone)]
pub struct SqliteAuthzDecisionRecorder {
    pool: SqlitePool,
}

impl SqliteAuthzDecisionRecorder {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthzDecisionRecorder for SqliteAuthzDecisionRecorder {
    async fn record(&self, decision: &AuthzDecision) -> Result<(), RepositoryError> {
        sqlx::query(concat!(
            "INSERT INTO authz_decisions (request_id, actor_id, tenant_id, action,",
            " resource_type, decision, reason_code, entrypoint, scope_kind, occurred_at)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&decision.request_id)
        .bind(&decision.actor_id)
        .bind(&decision.tenant_id)
        .bind(&decision.action)
        .bind(&decision.resource_type)
        .bind(&decision.decision)
        .bind(&decision.reason_code)
        .bind(&decision.entrypoint)
        .bind(&decision.scope_kind)
        .bind(to_sql_timestamp(&decision.occurred_at))
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }
}
