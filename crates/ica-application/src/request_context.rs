//! Request context and Application-layer authorization (ARCH-P2 §4.4 / §5.2).
//!
//! A transport must not decide who the caller is or whether an action is
//! allowed. It builds a [`RequestContext`] from its own authentication
//! handshake and hands it to the use case; the use case calls [`authorize`]
//! before touching a repository. Because the type lives in `ica-application`,
//! HTTP, IPC and the sidecar gate all execute the *same* rule.
//!
//! P1 multi-tenant hardening (RICA-SEC-P1-TENANT-AUTH-R1.0, AUTH-01):
//! authorization no longer runs on a bare role string. A context carries a
//! typed [`PrincipalKind`], [`TenantRole`], a platform permission set and a
//! membership validity flag. Role strings are mapped **once** at the boundary
//! and every downstream gate uses the typed values. In particular
//! `role == "admin"` is *never* upgraded to a platform administrator: only
//! the explicit `platform_admin` role grants [`PlatformPermission`]s.
//!
//! Local Mode (the accepted single-user boundary, see SEC-04) has no token
//! middleware yet: [`RequestContext::local`] is the synthetic local identity
//! both adapters use today. That is an honest description of the current
//! authentication boundary, not a claim that multi-user authorization has
//! been implemented. Multi-user identity extraction and end-to-end security
//! validation remain a human verification task.

use crate::error::ApplicationError;

/// What kind of actor is making the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalKind {
    /// An interactive user (or the documented Local-mode synthetic user).
    User,
    /// A background job or a trusted service-to-service caller.
    Service,
}

impl PrincipalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Service => "service",
        }
    }
}

/// The role a principal holds inside the current tenant. This is deliberately
/// distinct from platform authority: a tenant administrator is **not** a
/// platform administrator (P1 plan §3 / §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantRole {
    Admin,
    Member,
}

impl TenantRole {
    /// Map a validated role string exactly once, at the authentication
    /// boundary. Unknown roles are rejected by returning `None`.
    pub fn from_role(role: &str) -> Option<Self> {
        match role {
            "admin" | "tenant_admin" | "platform_admin" => Some(Self::Admin),
            "member" | "tenant_member" => Some(Self::Member),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

/// Installation-level permissions. A tenant role never implies any of these;
/// they are granted only by the explicit `platform_admin` role or by a trusted
/// service identity constructed with [`RequestContext::with_platform_permissions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlatformPermission {
    DashboardReadAll,
    AuditReadAll,
    BackupManage,
    SnapshotManage,
    AiSettingsManage,
    BrowserPolicyManage,
}

impl PlatformPermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DashboardReadAll => "dashboard:read_all",
            Self::AuditReadAll => "audit:read_all",
            Self::BackupManage => "backup:manage",
            Self::SnapshotManage => "snapshot:manage",
            Self::AiSettingsManage => "ai_settings:manage",
            Self::BrowserPolicyManage => "browser_policy:manage",
        }
    }
}

/// How the identity was established; captured for structured audit records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Jwt,
    DesktopSession,
    InternalService,
    Local,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jwt => "jwt",
            Self::DesktopSession => "desktop_session",
            Self::InternalService => "internal_service",
            Self::Local => "local",
        }
    }
}

/// Stable denial reasons (P1 plan §10). They are returned in `details` so the
/// transport can audit a decision without leaking whether a resource in
/// another tenant exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    Unauthenticated,
    PrincipalInactive,
    MembershipInactive,
    PermissionDenied,
    TenantScopeMismatch,
    ResourceOwnerMismatch,
    PlatformPermissionRequired,
    AssertionInvalid,
    AssertionExpired,
}

impl DenyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unauthenticated => "AUTH_UNAUTHENTICATED",
            Self::PrincipalInactive => "AUTH_PRINCIPAL_INACTIVE",
            Self::MembershipInactive => "AUTH_MEMBERSHIP_INACTIVE",
            Self::PermissionDenied => "AUTH_PERMISSION_DENIED",
            Self::TenantScopeMismatch => "AUTH_TENANT_SCOPE_MISMATCH",
            Self::ResourceOwnerMismatch => "AUTH_RESOURCE_OWNER_MISMATCH",
            Self::PlatformPermissionRequired => "AUTH_PLATFORM_PERMISSION_REQUIRED",
            Self::AssertionInvalid => "AUTH_ASSERTION_INVALID",
            Self::AssertionExpired => "AUTH_ASSERTION_EXPIRED",
        }
    }
}

/// The identity and correlation data a use case is allowed to see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    pub actor_id: String,
    pub tenant_id: String,
    /// Original validated role string, retained for audit/me responses.
    pub role: String,
    pub principal_kind: PrincipalKind,
    pub tenant_role: Option<TenantRole>,
    pub platform_permissions: Vec<PlatformPermission>,
    pub authn_method: AuthMethod,
    /// A disabled user never passes authorization, even with a valid role.
    pub active: bool,
    /// Tenant membership must still be valid at request time (AUTH-02).
    pub membership_active: bool,
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

/// All installation-level permissions. Only two trusted sources may hold
/// these: the explicit `platform_admin` role and the Local-mode synthetic
/// installation owner (which is constructed server-side, never from a token).
pub fn all_platform_permissions() -> Vec<PlatformPermission> {
    vec![
        PlatformPermission::DashboardReadAll,
        PlatformPermission::AuditReadAll,
        PlatformPermission::BackupManage,
        PlatformPermission::SnapshotManage,
        PlatformPermission::AiSettingsManage,
        PlatformPermission::BrowserPolicyManage,
    ]
}

impl RequestContext {
    /// Local Mode: both adapters synthesize the same installation owner.
    /// It is a trusted host-provided identity, not a role string: platform
    /// permissions are granted explicitly here so single-user operation keeps
    /// working while every MultiUser caller must carry a real platform grant.
    pub fn local() -> Self {
        Self::from_identity(
            crate::auth::LOCAL_USER_ID,
            crate::auth::LOCAL_TENANT_ID,
            "admin",
            true,
            "local-mode",
        )
        .with_authn_method(AuthMethod::Local)
        .with_platform_permissions(all_platform_permissions())
    }

    /// The one constructor for real identity sources (JWT claims, a trusted
    /// host-provided identity, or the local-mode synthetic user).
    ///
    /// The role string is mapped to typed values here and never re-parsed by
    /// an authorization decision. `platform_admin` is the only role that
    /// grants platform permissions; the Local synthetic `admin` and ordinary
    /// tenant `admin` do not.
    pub fn from_identity(
        actor_id: impl Into<String>,
        tenant_id: impl Into<String>,
        role: impl Into<String>,
        active: bool,
        correlation_id: impl Into<String>,
    ) -> Self {
        let role = role.into();
        let tenant_role = TenantRole::from_role(&role);
        let platform_permissions = if role == "platform_admin" {
            all_platform_permissions()
        } else {
            Vec::new()
        };
        Self {
            actor_id: actor_id.into(),
            tenant_id: tenant_id.into(),
            role,
            principal_kind: PrincipalKind::User,
            tenant_role,
            platform_permissions,
            authn_method: AuthMethod::Jwt,
            active,
            membership_active: active,
            correlation_id: correlation_id.into(),
        }
    }

    /// Attach a per-operation correlation id (the caller owns its generation).
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = correlation_id.into();
        self
    }

    pub fn with_authn_method(mut self, method: AuthMethod) -> Self {
        self.authn_method = method;
        self
    }

    /// Explicitly grant platform permissions to a trusted service identity.
    /// Never call this from a client-controlled path.
    pub fn with_platform_permissions(mut self, permissions: Vec<PlatformPermission>) -> Self {
        self.platform_permissions = permissions;
        self
    }

    /// Mark the context as a service identity (background job / internal call).
    pub fn as_service(mut self) -> Self {
        self.principal_kind = PrincipalKind::Service;
        self.authn_method = AuthMethod::InternalService;
        self
    }

    /// Refresh the membership validity after a per-request lookup (AUTH-02).
    pub fn with_membership_active(mut self, active: bool) -> Self {
        self.membership_active = active;
        self
    }

    /// Replace the role with the authoritative value read from the database.
    /// This is what makes a role downgrade effective without re-login.
    pub fn with_authoritative_role(mut self, role: &str) -> Self {
        self.tenant_role = TenantRole::from_role(role);
        self.platform_permissions = if role == "platform_admin" {
            all_platform_permissions()
        } else {
            Vec::new()
        };
        self.role = role.to_string();
        self
    }

    pub fn is_admin(&self) -> bool {
        self.tenant_role == Some(TenantRole::Admin)
    }

    pub fn has_platform_permission(&self, permission: PlatformPermission) -> bool {
        self.platform_permissions.contains(&permission)
    }

    /// True when this actor owns the object or administers all objects.
    pub fn can_administer(&self, owner_id: &str) -> bool {
        self.is_admin() || self.actor_id == owner_id
    }

    /// A client-supplied tenant id must never widen the server-side scope.
    pub fn tenant_matches(&self, requested: &str) -> bool {
        self.tenant_id == requested
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

fn denied_with(message: &str, reason: DenyReason) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 401,
        code: "unauthorized",
        message: message.to_string(),
        details: serde_json::json!({ "reason_code": reason.as_str() }),
    }
}

fn denied(message: &str) -> ApplicationError {
    denied_with(message, DenyReason::Unauthenticated)
}

fn forbidden_with(message: &str, reason: DenyReason) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 403,
        code: "forbidden",
        message: message.to_string(),
        details: serde_json::json!({ "reason_code": reason.as_str() }),
    }
}

fn forbidden(message: &str) -> ApplicationError {
    forbidden_with(message, DenyReason::PermissionDenied)
}

impl Permission {
    /// Role gate for the action itself. Object ownership is a separate check
    /// (`RequestContext::can_administer`) performed by the use case that can
    /// see the target row.
    fn allowed_for_role(self, role: TenantRole) -> bool {
        match self {
            Self::AdminOperation => role == TenantRole::Admin,
            Self::SharesRead
            | Self::SharesCreate
            | Self::SharesDelete
            | Self::ResourceRead
            | Self::ResourceWrite
            | Self::ResourceDelete
            | Self::ResourceShare => true,
        }
    }
}

fn check_identity(ctx: &RequestContext) -> Result<(), ApplicationError> {
    if !ctx.active || ctx.actor_id.is_empty() {
        return Err(denied_with(
            "Authentication is required",
            DenyReason::PrincipalInactive,
        ));
    }
    if !ctx.membership_active {
        return Err(denied_with(
            "Tenant membership is no longer active",
            DenyReason::MembershipInactive,
        ));
    }
    if ctx.tenant_id.is_empty() {
        return Err(denied("Tenant context is required"));
    }
    Ok(())
}

/// The single authorization gate. Identity checks are uniform; the permission
/// is no longer ignored: an unknown/inactive actor is 401, while a known actor
/// without the required role is 403 `forbidden`.
pub fn authorize(ctx: &RequestContext, permission: Permission) -> Result<(), ApplicationError> {
    check_identity(ctx)?;
    let role = ctx
        .tenant_role
        .ok_or_else(|| forbidden("Permission denied"))?;
    if !permission.allowed_for_role(role) {
        return Err(forbidden("Permission denied"));
    }
    Ok(())
}

/// Gate for installation-level operations (P1 plan §3.2). A tenant role,
/// including `tenant_admin` / old `admin`, can never satisfy this gate.
pub fn authorize_platform(
    ctx: &RequestContext,
    permission: PlatformPermission,
) -> Result<(), ApplicationError> {
    check_identity(ctx)?;
    if !ctx.has_platform_permission(permission) {
        return Err(forbidden_with(
            "Platform permission required",
            DenyReason::PlatformPermissionRequired,
        ));
    }
    Ok(())
}

/// Reject a client-supplied tenant id that disagrees with the authenticated
/// context (P1 plan §1 principle 2 and §8 TA-016).
pub fn require_tenant_scope(ctx: &RequestContext, requested: &str) -> Result<(), ApplicationError> {
    check_identity(ctx)?;
    if !ctx.tenant_matches(requested) {
        return Err(forbidden_with(
            "Tenant scope mismatch",
            DenyReason::TenantScopeMismatch,
        ));
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
        assert_eq!(ctx.authn_method, AuthMethod::Local);
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

    #[test]
    fn a_tenant_admin_is_never_a_platform_admin() {
        let tenant_admin = RequestContext::from_identity("u-1", "t-1", "admin", true, "x");
        assert!(tenant_admin.is_admin());
        assert!(tenant_admin.platform_permissions.is_empty());
        for permission in [
            PlatformPermission::DashboardReadAll,
            PlatformPermission::AuditReadAll,
            PlatformPermission::BackupManage,
            PlatformPermission::SnapshotManage,
            PlatformPermission::AiSettingsManage,
            PlatformPermission::BrowserPolicyManage,
        ] {
            let error = authorize_platform(&tenant_admin, permission)
                .expect_err("tenant admin must not hold platform permission");
            assert_eq!(ApplicationError::http_status(&error), 403);
            assert_eq!(
                error_details_reason(&error),
                DenyReason::PlatformPermissionRequired.as_str()
            );
        }
    }

    #[test]
    fn an_explicit_platform_admin_holds_every_platform_permission() {
        let platform = RequestContext::from_identity("u-p", "t-1", "platform_admin", true, "x");
        assert!(platform.is_admin());
        assert!(authorize_platform(&platform, PlatformPermission::BackupManage).is_ok());
        assert!(authorize_platform(&platform, PlatformPermission::AuditReadAll).is_ok());
        assert!(authorize(&platform, Permission::AdminOperation).is_ok());
    }

    #[test]
    fn downgraded_or_removed_membership_is_denied() {
        let downgraded = RequestContext::from_identity("u-1", "t-1", "admin", true, "x")
            .with_authoritative_role("member");
        assert!(!downgraded.is_admin());
        assert_eq!(
            authorize(&downgraded, Permission::AdminOperation)
                .expect_err("downgraded role must lose admin gate")
                .details_reason(),
            DenyReason::PermissionDenied.as_str()
        );

        let removed = RequestContext::from_identity("u-1", "t-1", "admin", true, "x")
            .with_membership_active(false);
        let error = authorize(&removed, Permission::ResourceRead).expect_err("membership removed");
        assert_eq!(
            error.details_reason(),
            DenyReason::MembershipInactive.as_str()
        );
    }

    #[test]
    fn client_supplied_tenant_id_cannot_widen_scope() {
        let ctx = RequestContext::from_identity("u-1", "t-a", "admin", true, "x");
        assert!(require_tenant_scope(&ctx, "t-a").is_ok());
        let error =
            require_tenant_scope(&ctx, "t-b").expect_err("cross-tenant scope must be rejected");
        assert_eq!(
            error.details_reason(),
            DenyReason::TenantScopeMismatch.as_str()
        );
    }

    fn error_details_reason(error: &ApplicationError) -> &str {
        error.details_reason()
    }
}
