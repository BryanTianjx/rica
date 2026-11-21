//! The audit trail read model.
//!
//! `audit_events` is written by every business write inside its own
//! transaction (see `ica-infra::audit`); the W5 `GET /api/audit-events`
//! endpoint reads it back. The wire shape and the "newest first" rule come
//! from the baseline's `app/application/common.py::audit_recent`.

/// One audit trail entry, as stored.
///
/// `metadata_json` is the raw text column: the transport layer (not the
/// domain) parses it, because a broken blob must degrade to an empty object
/// rather than hide the event — `app.application.common.load_json_dict`.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub id: String,
    pub event_type: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub actor: String,
    pub summary: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The `GET /api/audit-events` filters.
///
/// The baseline ignores an *empty* query value (`if event_type:` is falsy for
/// `""`), so an empty string means "no filter" — the transport layer
/// normalises that before it reaches here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditEventFilter {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub limit: usize,
}
