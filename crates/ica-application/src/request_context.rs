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
}

fn denied(message: &str) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 401,
        code: "unauthorized",
        message: message.to_string(),
        details: serde_json::json!({}),
    }
}

/// The single authorization gate. Identity checks are uniform; object-level
/// ownership is checked by the use case that can see the object's owner.
pub fn authorize(ctx: &RequestContext, _permission: Permission) -> Result<(), ApplicationError> {
    if !ctx.active || ctx.actor_id.is_empty() {
        return Err(denied("Authentication is required"));
    }
    if ctx.tenant_id.is_empty() {
        return Err(denied("Tenant context is required"));
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
    fn ownership_is_a_separate_application_layer_check() {
        let admin = RequestContext::local();
        assert!(admin.can_administer("someone-else"));
        let member = RequestContext::from_identity("u-2", "t-1", "member", true, "x");
        assert!(member.can_administer("u-2"));
        assert!(!member.can_administer("someone-else"));
    }
}
