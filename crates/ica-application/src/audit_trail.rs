//! The audit trail use case (W5 `GET /api/audit-events`).
//!
//! The baseline's `app/api/settings.py::read_audit_events` plus
//! `app/application/common.py::audit_recent`: newest first, equality filters
//! on `event_type`/`entity_type`/`entity_id`, and a validated `limit`
//! (default 50, `1..=500`). The trail is what makes the send boundary
//! verifiable — "the draft was copied but never sent" is a fact you can check.

use crate::error::ApplicationError;
use crate::ports::{AuditTrailRepository, RepositoryError};
use crate::request_context::{authorize, authorize_platform, Permission, PlatformPermission};
use crate::{RequestContext, Scope};
use ica_domain::{AuditEvent, AuditEventFilter};

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// The tenant audit trail, newest first (P1 plan §5.2). Only events whose
/// `tenant_id` matches the authenticated context can be returned; the scope
/// is derived here so no handler or query parameter can widen it.
pub async fn recent_audit_events(
    trail: &dyn AuditTrailRepository,
    ctx: &RequestContext,
    filter: AuditEventFilter,
) -> Result<Vec<AuditEvent>, ApplicationError> {
    authorize(ctx, Permission::AdminOperation)?;
    let scope = Scope::from_context(ctx);
    trail
        .recent_events(scope.as_ref(), filter)
        .await
        .map_err(unavailable)
}

/// The installation-wide audit trail (P1 plan §5.2). Requires an explicit
/// platform permission; a tenant admin can never reach this use case.
pub async fn platform_audit_events(
    trail: &dyn AuditTrailRepository,
    ctx: &RequestContext,
    filter: AuditEventFilter,
) -> Result<Vec<AuditEvent>, ApplicationError> {
    authorize_platform(ctx, PlatformPermission::AuditReadAll)?;
    trail.recent_events(None, filter).await.map_err(unavailable)
}
