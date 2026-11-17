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
use sqlx::SqliteConnection;

use crate::store::{new_uuid, to_sql_timestamp, DEFAULT_TENANT_ID, LOCAL_ACTOR};

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
    let metadata_json = entry.metadata.map(|value| value.to_string());

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
