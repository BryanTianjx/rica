//! SEC-04-B/C: HTTP authentication boundary.
//!
//! The Python baseline issued access tokens but the Rust adapter never verified
//! them: every business route ran with `RequestContext::local()`. This module
//! closes that gap. It verifies the HS256 access token, turns its claims into a
//! real [`RequestContext`] and inserts it into request extensions so handlers
//! and Application use cases see the same identity over HTTP and IPC.
//!
//! `AuthMode::Local` keeps the documented single-user boundary: a missing or
//! invalid bearer is tolerated and the synthetic Local identity is injected.
//! `AuthMode::MultiUser` requires a valid access token on every non-public
//! route; it is selected with `ICA_AUTH_MODE=multiuser` and is the mode the
//! IDOR/authorization regression suite exercises.

use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use ica_application::ports::AuthzDecision;
use ica_application::{ApplicationError, DenyReason, RequestContext};
use ica_contracts::error::code;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use crate::state::AppState;

/// How identity is established for non-public routes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    /// Single-user desktop default: synthetic local actor when no valid token.
    Local,
    /// Multi-user mode: every non-public route requires a valid access token.
    MultiUser,
}

impl AuthMode {
    pub fn from_env() -> Self {
        match std::env::var("ICA_AUTH_MODE").as_deref() {
            Ok("multiuser") | Ok("multi-user") | Ok("true") => Self::MultiUser,
            _ => Self::Local,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::MultiUser => "multiuser",
        }
    }
}

/// Routes that must work without an access token (the login/bootstrap chain
/// plus the liveness probe). Everything else goes through [`authenticate`].
pub const PUBLIC_PATHS: &[&str] = &[
    "/health",
    "/api/auth/register",
    "/api/auth/login",
    "/api/auth/refresh",
    "/api/auth/logout",
];

pub fn is_public(path: &str) -> bool {
    PUBLIC_PATHS.contains(&path)
}

/// High-risk installation-level surfaces whose successful use must be
/// auditable (P1 plan §8.4). Kept here so the middleware and tests agree on
/// the same set without duplicating router knowledge.
pub fn is_platform_operation(path: &str) -> bool {
    path == "/api/dashboard/platform"
        || path == "/api/export"
        || path == "/api/import"
        || path.starts_with("/api/backups")
        || path.starts_with("/api/settings")
        || path.starts_with("/api/ai/")
}

#[derive(Debug, Deserialize)]
struct AccessClaims {
    sub: String,
    tenant_id: String,
    role: String,
    #[serde(rename = "type")]
    token_type: String,
}

fn status_error(status: u16, code_name: &'static str, message: &str) -> crate::ApiError {
    status_error_with_reason(status, code_name, message, None)
}

/// A denial carrying a stable reason code for structured audit logging
/// (P1 plan §10). The response body never reveals which tenant owns a resource.
fn status_error_with_reason(
    status: u16,
    code_name: &'static str,
    message: &str,
    reason: Option<DenyReason>,
) -> crate::ApiError {
    let details = match reason {
        Some(reason) => serde_json::json!({ "reason_code": reason.as_str() }),
        None => serde_json::json!({}),
    };
    ApplicationError::StatusWithCode {
        status,
        code: code_name,
        message: message.to_string(),
        details,
    }
    .into()
}

fn unauthorized(message: &str) -> crate::ApiError {
    status_error(401, code::UNAUTHORIZED, message)
}

/// An authenticated actor reached a resource or action they may not use.
/// The wire code is `forbidden` (403), distinct from a missing identity (401).
pub fn forbidden(message: &str) -> crate::ApiError {
    status_error(403, code::FORBIDDEN, message)
}

/// Verify an access token and extract the transport-neutral identity.
/// `exp` and the `access` token type are mandatory; a refresh token can never
/// be used as a bearer credential.
pub fn verify_access_token(token: &str, secret: &str) -> Result<RequestContext, crate::ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["exp", "sub"]);
    let decoded = decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| unauthorized("Invalid or expired access token"))?;
    if decoded.claims.token_type != "access" {
        return Err(unauthorized(
            "A refresh token cannot be used as a bearer token",
        ));
    }
    Ok(RequestContext::from_identity(
        decoded.claims.sub,
        decoded.claims.tenant_id,
        decoded.claims.role,
        true,
        "bearer-jwt",
    ))
}

/// AUTH-02: a signed token is necessary but not sufficient. Reload the principal
/// from the authoritative store on every MultiUser request so that disabling an
/// account, removing tenant membership or downgrading a role takes effect
/// without waiting for token expiry (plan §6 / TA-017..TA-019).
async fn revalidate_principal(
    state: &AppState,
    ctx: RequestContext,
) -> Result<RequestContext, crate::ApiError> {
    let user = state
        .auth_repo()
        .get_user(&ctx.actor_id)
        .await
        .map_err(|_| {
            status_error_with_reason(
                401,
                code::UNAUTHORIZED,
                "Unable to verify the current principal",
                Some(DenyReason::Unauthenticated),
            )
        })?
        .ok_or_else(|| {
            status_error_with_reason(
                401,
                code::UNAUTHORIZED,
                "Principal no longer exists",
                Some(DenyReason::PrincipalInactive),
            )
        })?;

    if !user.is_active {
        return Err(status_error_with_reason(
            401,
            code::UNAUTHORIZED,
            "Principal is disabled",
            Some(DenyReason::PrincipalInactive),
        ));
    }
    if user.tenant_id != ctx.tenant_id {
        return Err(status_error_with_reason(
            403,
            code::FORBIDDEN,
            "Tenant membership is no longer active",
            Some(DenyReason::MembershipInactive),
        ));
    }

    // The database row is authoritative for the role. A stale token carrying
    // `admin` after a downgrade cannot keep using the admin gate.
    Ok(ctx
        .with_authoritative_role(&user.role)
        .with_membership_active(true))
}

/// Axum middleware: inject the authenticated identity into request extensions.
pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, crate::ApiError> {
    let mode = request
        .extensions()
        .get::<AuthMode>()
        .copied()
        .unwrap_or_else(AuthMode::from_env);
    let path = request.uri().path().to_owned();
    let method = request.method().to_string();
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let ctx = if is_public(&path) {
        RequestContext::local()
    } else {
        let bearer = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        match bearer {
            Some(token) => match verify_access_token(token, &state.config().jwt_secret) {
                Ok(ctx) => revalidate_principal(&state, ctx).await?,
                // Local mode preserves the documented single-user boundary:
                // a bogus bearer cannot escalate or deny the local actor.
                Err(error) if mode == AuthMode::MultiUser => return Err(error),
                Err(_) => RequestContext::local(),
            },
            None if mode == AuthMode::MultiUser => {
                return Err(unauthorized("Authentication is required"));
            }
            None => RequestContext::local(),
        }
    };

    // SEC-P1 §10: record the denial as a structured decision. The recorder is
    // a side table, so the frozen audit_events contract stays byte-identical.
    let actor_id = ctx.actor_id.clone();
    let tenant_id = ctx.tenant_id.clone();
    let scope_kind =
        if ctx.has_platform_permission(ica_application::PlatformPermission::AuditReadAll) {
            "platform".to_string()
        } else {
            "tenant".to_string()
        };
    request.extensions_mut().insert(ctx);
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let denied = status == 401 || status == 403;
    // Successful high-risk platform operations also need a structured record
    // (plan §8.4). Only the side table is written; audit_events keeps the
    // frozen baseline shape so contract capture stays byte-identical.
    let allowed_platform_operation = (200..300).contains(&status) && is_platform_operation(&path);
    if !denied && !allowed_platform_operation {
        return Ok(response);
    }

    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 16 * 1024)
        .await
        .unwrap_or_default();
    let reason = if denied {
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| {
                value["error"]["details"]["reason_code"]
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                if status == 401 {
                    "AUTH_UNAUTHENTICATED"
                } else {
                    "AUTH_PERMISSION_DENIED"
                }
                .to_string()
            })
    } else {
        "ALLOWED".to_string()
    };
    let decision = AuthzDecision {
        request_id,
        actor_id,
        tenant_id,
        action: format!("{method} {path}"),
        resource_type: "http_route".to_string(),
        decision: if denied { "deny" } else { "allow" }.to_string(),
        reason_code: reason,
        entrypoint: "http".to_string(),
        scope_kind,
        occurred_at: chrono::Utc::now(),
    };
    // Recording must never turn a denial into a 500.
    let _ = state.authz_decisions().record(&decision).await;
    Ok(Response::from_parts(parts, axum::body::Body::from(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    fn token(secret: &str, claims: serde_json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("token")
    }

    fn access(secret: &str, sub: &str, tenant: &str, role: &str) -> String {
        let now = chrono::Utc::now();
        token(
            secret,
            json!({
                "sub": sub,
                "tenant_id": tenant,
                "role": role,
                "type": "access",
                "iat": now.timestamp(),
                "exp": (now + chrono::Duration::hours(1)).timestamp(),
            }),
        )
    }

    #[test]
    fn a_valid_access_bearer_becomes_a_real_context() {
        let ctx = verify_access_token(&access("s", "u-2", "t-2", "member"), "s").unwrap();
        assert_eq!(ctx.actor_id, "u-2");
        assert_eq!(ctx.tenant_id, "t-2");
        assert_eq!(ctx.role, "member");
        assert!(ctx.active);
    }

    #[test]
    fn refresh_tokens_and_bad_signatures_are_rejected() {
        let refresh = token(
            "s",
            json!({"sub":"u","tenant_id":"t","role":"admin","type":"refresh","exp":9999999999i64}),
        );
        assert_eq!(
            verify_access_token(&refresh, "s")
                .unwrap_err()
                .0
                .http_status(),
            401
        );
        let other_key = access("attacker", "u", "t", "admin");
        assert_eq!(
            verify_access_token(&other_key, "s")
                .unwrap_err()
                .0
                .http_status(),
            401
        );
    }

    #[test]
    fn public_paths_are_exactly_the_bootstrap_chain() {
        assert!(is_public("/health"));
        assert!(is_public("/api/auth/login"));
        assert!(!is_public("/api/positions"));
        assert!(!is_public("/api/auth/me"));
    }
}
