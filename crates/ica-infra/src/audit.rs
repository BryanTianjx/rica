//! The one audit writer.
//!
//! `app/application/common.py::record_audit` is the Python counterpart. The
//! observable rules it encodes:
//!
//! * `actor` is `local_user` in local mode and `actor_id` stays NULL;
//! * `tenant_id` falls back to the default tenant when no real user acts;
//! * only small metadata is stored (`{"source": "manual"}`, the send-record
//!   identifiers), never a message body or a secret.
//!
//! Every write path calls this with the same transaction that changes the
//! business row, so an audit event can never describe a change that rolled back.

use chrono::Utc;
use serde_json::Value;

use crate::json_fmt::python_json;
use sqlx::SqliteConnection;

use crate::store::{new_uuid, repo_error, to_sql_timestamp, DEFAULT_TENANT_ID, LOCAL_ACTOR};

/// One audit event to append.
///
/// `entity_type`/`entity_id` are optional: the follow-up sweep
/// (`follow_up.tasks_expired`) writes a storage-level event with no entity,
/// exactly like the baseline's bare `record_audit(event_type=..., summary=...)`.
pub struct AuditEntry<'a> {
    pub event_type: &'a str,
    pub entity_type: Option<&'a str>,
    pub entity_id: Option<&'a str>,
    pub summary: String,
    pub metadata: Option<Value>,
}

impl<'a> AuditEntry<'a> {
    pub fn new(
        event_type: &'a str,
        entity_type: &'a str,
        entity_id: &'a str,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            entity_type: Some(entity_type),
            entity_id: Some(entity_id),
            summary: summary.into(),
            metadata: None,
        }
    }

    /// An event with an entity TYPE but no id.
    ///
    /// The baseline's export audit is `record_audit(event_type="data.exported",
    /// entity_type="export", summary=...)` — no `entity_id`, so the column is
    /// NULL. Passing a placeholder id would put a string there that the
    /// baseline leaves empty.
    pub fn with_entity_type(
        event_type: &'a str,
        entity_type: &'a str,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            entity_type: Some(entity_type),
            entity_id: None,
            summary: summary.into(),
            metadata: None,
        }
    }

    /// An event with no entity (summary only), like the baseline's sweep audit.
    pub fn without_entity(event_type: &'a str, summary: impl Into<String>) -> Self {
        Self {
            event_type,
            entity_type: None,
            entity_id: None,
            summary: summary.into(),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Append `entry` through `connection` (a pool or an open transaction).
pub async fn insert_audit(
    connection: &mut SqliteConnection,
    entry: AuditEntry<'_>,
) -> Result<(), sqlx::Error> {
    let now = to_sql_timestamp(&Utc::now());
    // `json.dumps(metadata, ensure_ascii=False)` in Python. The exact byte
    // layout is normalised by the contract diff (both sides are parsed back to
    // JSON), but the value must be identical.
    // The baseline stores `json.dumps(metadata, ensure_ascii=False)`: the
    // DEFAULT separators (`", "` and `": "`) and the INSERTION order of the
    // keys — it does NOT sort them. `serde_json`'s compact output would put
    // `{"a":1}` where the baseline puts `{"a": 1}`, and `preserve_order` plus
    // an explicit separator walk reproduces the baseline byte for byte.
    let metadata_json = entry.metadata.map(|value| python_json(&value));

    sqlx::query(concat!(
        "INSERT INTO audit_events (id, event_type, entity_type, entity_id, actor,",
        " summary, metadata_json, tenant_id, actor_id, created_at, updated_at)",
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)"
    ))
    .bind(new_uuid())
    .bind(entry.event_type)
    .bind(entry.entity_type)
    .bind(entry.entity_id)
    .bind(LOCAL_ACTOR)
    .bind(entry.summary)
    .bind(metadata_json)
    .bind(DEFAULT_TENANT_ID)
    .bind(&now)
    .bind(&now)
    .execute(connection)
    .await?;

    Ok(())
}

// ── The read side (W5 `GET /api/audit-events`) ─────────────────────────────

use crate::store::SqliteStore;
use ica_application::ports::{AuditTrailRepository, RepositoryError};
use ica_domain::{AuditEvent, AuditEventFilter};

/// One raw `audit_events` row.
#[derive(sqlx::FromRow)]
struct AuditEventRow {
    id: String,
    event_type: String,
    entity_type: Option<String>,
    entity_id: Option<String>,
    actor: String,
    summary: Option<String>,
    metadata_json: Option<String>,
    created_at: String,
}

impl AuditEventRow {
    fn into_event(self) -> Result<AuditEvent, RepositoryError> {
        let created_at = crate::time::parse_sqlite_datetime(&self.created_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "audit_events.created_at for id '{}': {error}",
                self.id
            ))
        })?;
        Ok(AuditEvent {
            id: self.id,
            event_type: self.event_type,
            entity_type: self.entity_type,
            entity_id: self.entity_id,
            actor: self.actor,
            summary: self.summary,
            metadata_json: self.metadata_json,
            created_at,
        })
    }
}

#[async_trait::async_trait]
impl AuditTrailRepository for SqliteStore {
    async fn recent_events(
        &self,
        scope: Option<&ica_application::Scope>,
        filter: AuditEventFilter,
    ) -> Result<Vec<AuditEvent>, RepositoryError> {
        // Same shape as the baseline's `audit_recent`: equality filters only
        // when present, `ORDER BY created_at DESC`, `LIMIT ?`. The tenant
        // predicate is added first and cannot be overridden by a filter value.
        let mut sql = String::from(
            "SELECT a.id, a.event_type, a.entity_type, a.entity_id, a.actor, a.summary, \
             a.metadata_json, a.created_at FROM audit_events a WHERE 1=1",
        );
        let mut binds: Vec<String> = Vec::new();
        if let Some(scope) = scope {
            sql.push_str(" AND a.tenant_id = ?");
            binds.push(scope.tenant_id.clone());
            if !scope.is_admin {
                // Members see only their own audit events by default (plan §5.2).
                sql.push_str(" AND a.actor_id = ?");
                binds.push(scope.actor_id.clone());
            }
        }
        if let Some(value) = &filter.event_type {
            sql.push_str(" AND a.event_type = ?");
            binds.push(value.clone());
        }
        if let Some(value) = &filter.entity_type {
            sql.push_str(" AND a.entity_type = ?");
            binds.push(value.clone());
        }
        if let Some(value) = &filter.entity_id {
            sql.push_str(" AND a.entity_id = ?");
            binds.push(value.clone());
        }
        sql.push_str(" ORDER BY a.created_at DESC LIMIT ?");

        let mut query = sqlx::query_as::<_, AuditEventRow>(sqlx::AssertSqlSafe(sql.to_owned()));
        for value in &binds {
            query = query.bind(value.clone());
        }
        let rows = query
            .bind(filter.limit as i64)
            .fetch_all(self.pool())
            .await
            .map_err(repo_error)?;
        rows.into_iter().map(AuditEventRow::into_event).collect()
    }
}
