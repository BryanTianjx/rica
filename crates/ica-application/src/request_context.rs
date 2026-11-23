//! Request context and Application-layer authorization (ARCH-P2 §4.4 / §5.2).
//!
//! A transport must not decide who the caller is or whether an action is
//! allowed. It builds a [`RequestContext`] from its own authentication
//! handshake and hands it to the use case; the use case calls [`authorize`]
//! before touching a repository. Because the type lives in `ica-application`,
//! HTTP, IPC and the sidecar gate all execute the *same* rule.
//!
//! Local Mode (the accepted single-user boundary, see SEC-04) has no token
//! middleware yet: [`RequestContext::local`] is the synthetic local identity
//! both adapters use today. That is an honest description of the current
//! authentication boundary, not a claim that multi-user authorization has
//! been implemented. Multi-user identity extraction and end-to-end security
//! validation remain a human verification task.

use crate::error::ApplicationError;

/// The identity and correlation data a use case is allowed to see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    pub actor_id: String,
    pub tenant_id: String,
    pub role: String,
    /// A disabled user never passes authorization, even with a valid role.
    pub active: bool,
    /// Correlates every write and audit event in one operation chain.
    pub correlation_id: String,
}

/// Repository-level resource scope derived from an authenticated context.
/// `None` in a filter means the documented Local mode (no filtering), so all
/// existing single-user call sites keep their behaviour. A `Some` scope forces
/// tenant isolation and owner filtering in the SQL adapter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    pub tenant_id: String,
    pub actor_id: String,
    pub is_admin: bool,
}

impl Scope {
    /// A real (non-synthetic) identity must always carry a repository scope.
    /// The Local user is the sole exception.
    pub fn from_context(ctx: &RequestContext) -> Option<Self> {
        if ctx.actor_id == crate::auth::LOCAL_USER_ID
            && ctx.tenant_id == crate::auth::LOCAL_TENANT_ID
        {
            return None;
        }
        Some(Self {
            tenant_id: ctx.tenant_id.clone(),
            actor_id: ctx.actor_id.clone(),
            is_admin: ctx.is_admin(),
        })
    }
}

impl RequestContext {
    /// Local Mode: both adapters currently synthesize the same local actor.
    pub fn local() -> Self {
        Self::from_identity(
            crate::auth::LOCAL_USER_ID,
            crate::auth::LOCAL_TENANT_ID,
            "admin",
            true,
            "local-mode",
        )
    }

    /// The one constructor for real identity sources (JWT claims, a trusted
    /// host-provided identity, or the local-mode synthetic user).
    pub fn from_identity(
        actor_id: impl Into<String>,
        tenant_id: impl Into<String>,
        role: impl Into<String>,
        active: bool,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            actor_id: actor_id.into(),
            tenant_id: tenant_id.into(),
            role: role.into(),
            active,
            correlation_id: correlation_id.into(),
        }
    }

    /// Attach a per-operation correlation id (the caller owns its generation).
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = correlation_id.into();
        self
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// True when this actor owns the object or administers all objects.
    pub fn can_administer(&self, owner_id: &str) -> bool {
        self.is_admin() || self.actor_id == owner_id
    }
}

/// The actions an Application use case may gate. The enum is deliberately
/// small: adding a permission here is the single place a new protected
/// operation has to declare its authorization need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    SharesRead,
    SharesCreate,
    SharesDelete,
    /// SEC-04-B: the five contract-level action classes used by the C suite.
    ResourceRead,
    ResourceWrite,
    ResourceDelete,
    ResourceShare,
    AdminOperation,
}

fn denied(message: &str) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 401,
        code: "unauthorized",
        message: message.to_string(),
        details: serde_json::json!({}),
    }
}

fn forbidden(message: &str) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 403,
        code: "forbidden",
        message: message.to_string(),
        details: serde_json::json!({}),
    }
}

impl Permission {
    /// Role gate for the action itself. Object ownership is a separate check
    /// (`RequestContext::can_administer`) performed by the use case that can
    /// see the target row.
    fn allowed_for_role(self, role: &str) -> bool {
        match self {
            Self::AdminOperation => role == "admin",
            Self::SharesRead
            | Self::SharesCreate
            | Self::SharesDelete
            | Self::ResourceRead
            | Self::ResourceWrite
            | Self::ResourceDelete
            | Self::ResourceShare => role == "admin" || role == "member",
        }
    }
}

/// The single authorization gate. Identity checks are uniform; the permission
/// is no longer ignored: an unknown/inactive actor is 401, while a known actor
/// without the required role is 403 `forbidden`.
pub fn authorize(ctx: &RequestContext, permission: Permission) -> Result<(), ApplicationError> {
    if !ctx.active || ctx.actor_id.is_empty() {
        return Err(denied("Authentication is required"));
    }
    if ctx.tenant_id.is_empty() {
        return Err(denied("Tenant context is required"));
    }
    if !permission.allowed_for_role(&ctx.role) {
        return Err(forbidden("Permission denied"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mode_context_carries_the_trusted_identity() {
        let ctx = RequestContext::local();
        assert_eq!(ctx.actor_id, crate::auth::LOCAL_USER_ID);
        assert_eq!(ctx.tenant_id, crate::auth::LOCAL_TENANT_ID);
        assert!(ctx.active);
        assert!(ctx.is_admin());
        assert!(authorize(&ctx, Permission::SharesDelete).is_ok());
    }

    #[test]
    fn an_inactive_or_anonymous_context_cannot_authorize_any_action() {
        for ctx in [
            RequestContext::from_identity("", crate::auth::LOCAL_TENANT_ID, "admin", true, "x"),
            RequestContext::from_identity(crate::auth::LOCAL_USER_ID, "", "admin", true, "x"),
            RequestContext::from_identity(
                crate::auth::LOCAL_USER_ID,
                crate::auth::LOCAL_TENANT_ID,
                "admin",
                false,
                "x",
            ),
        ] {
            let error = authorize(&ctx, Permission::SharesRead).expect_err("must be denied");
            assert_eq!(ApplicationError::code(&error), "unauthorized");
            assert_eq!(ApplicationError::http_status(&error), 401);
        }
    }

    #[test]
    fn permission_is_enforced_not_ignored() {
        let admin = RequestContext::local();
        let member = RequestContext::from_identity("u-2", "t-1", "member", true, "x");
        for action in [
            Permission::ResourceRead,
            Permission::ResourceWrite,
            Permission::ResourceDelete,
            Permission::ResourceShare,
            Permission::SharesRead,
            Permission::SharesCreate,
            Permission::SharesDelete,
        ] {
            assert!(authorize(&admin, action).is_ok());
            assert!(authorize(&member, action).is_ok());
        }
        assert!(authorize(&admin, Permission::AdminOperation).is_ok());
        let error = authorize(&member, Permission::AdminOperation).expect_err("member denied");
        assert_eq!(ApplicationError::code(&error), "forbidden");
        assert_eq!(ApplicationError::http_status(&error), 403);
    }

    #[test]
    fn ownership_is_a_separate_application_layer_check() {
        let admin = RequestContext::local();
        assert!(admin.can_administer("someone-else"));
        let member = RequestContext::from_identity("u-2", "t-1", "member", true, "x");
        assert!(member.can_administer("u-2"));
        assert!(!member.can_administer("someone-else"));
    }
}
