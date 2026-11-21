//! The audit trail use case (W5 `GET /api/audit-events`).
//!
//! The baseline's `app/api/settings.py::read_audit_events` plus
//! `app/application/common.py::audit_recent`: newest first, equality filters
//! on `event_type`/`entity_type`/`entity_id`, and a validated `limit`
//! (default 50, `1..=500`). The trail is what makes the send boundary
//! verifiable — "the draft was copied but never sent" is a fact you can check.

use crate::error::ApplicationError;
use crate::ports::{AuditTrailRepository, RepositoryError};
use ica_domain::{AuditEvent, AuditEventFilter};

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// The audit trail, newest first.
pub async fn recent_audit_events(
    trail: &dyn AuditTrailRepository,
    filter: AuditEventFilter,
) -> Result<Vec<AuditEvent>, ApplicationError> {
    trail.recent_events(filter).await.map_err(unavailable)
}
