//! The authentication and sharing use cases (W6).
//!
//! Baseline: `app/auth/service.py` + `app/api/auth.py`. Local mode is the
//! default for this port: `me` answers the synthetic local user (role admin,
//! tenant `…0000`, id `…0001`), and share mutations act as that user — the
//! JWT middleware itself is W6's second half and stays off until a shell
//! sends credentials.
//!
//! Security properties carried over:
//! * refresh tokens are stored as SHA-256 hashes, never plaintext;
//! * rotation is **single-use**: the presented token is revoked atomically
//!   (a guarded UPDATE) before the new pair is issued, so two concurrent
//!   refreshes cannot both win;
//! * logout is idempotent: an unknown token is a 204, not an error.

use crate::error::ApplicationError;
use crate::ports::AuthRepository;
use std::collections::BTreeMap;

/// The synthetic local user's id (`settings.default_user_id`).
pub const LOCAL_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
/// The default tenant (`settings.tenant_id_default`).
pub const LOCAL_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";
/// The synthetic local user's display name.
pub const LOCAL_DISPLAY_NAME: &str = "Local User";

fn unauthorized(message: impl Into<String>) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 401,
        code: "unauthorized",
        message: message.into(),
        details: serde_json::json!({}),
    }
}

fn email_conflict() -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: 409,
        code: "email_conflict",
        message: "Email is already registered".to_string(),
        details: serde_json::json!({}),
    }
}

fn validation(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Validation {
        message: message.into(),
        details: BTreeMap::new(),
    }
}

/// One `users` row (the read model the use cases see).
#[derive(Debug, Clone, PartialEq)]
pub struct UserData {
    pub id: String,
    pub tenant_id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub role: String,
    pub is_active: bool,
}

/// One `refresh_tokens` row (the use case's view).
#[derive(Debug, Clone, PartialEq)]
pub struct RefreshTokenRow {
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One `shares` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ShareData {
    pub id: String,
    pub owner_id: String,
    pub recipient_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub permission: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `TokenResponse` payload minus the token_type constant.
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedTokens {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub user: UserData,
}

/// `UserRead` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct UserRead {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub tenant_id: String,
    pub is_active: bool,
}

impl From<&UserData> for UserRead {
    fn from(user: &UserData) -> Self {
        Self {
            id: user.id.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id.clone(),
            is_active: user.is_active,
        }
    }
}

/// The synthetic local user (`auth/deps.py::local_user`).
pub fn local_user() -> UserData {
    UserData {
        id: LOCAL_USER_ID.to_string(),
        tenant_id: LOCAL_TENANT_ID.to_string(),
        email: String::new(),
        password_hash: String::new(),
        display_name: LOCAL_DISPLAY_NAME.to_string(),
        role: "admin".to_string(),
        is_active: true,
    }
}

/// `GET /auth/me` — the current user; local mode answers the synthetic one.
pub fn me() -> UserRead {
    UserRead::from(&local_user())
}

/// `POST /auth/register` — first sign-up bootstraps the tenant as admin,
/// later registrations join the oldest tenant as member.
///
/// Validations run in the baseline's order: email shape, password length,
/// then the (optional) tenant lookup, then the email conflict.
pub async fn register(
    repository: &dyn AuthRepository,
    security: &dyn crate::ports::AuthSecurity,
    email: &str,
    password: &str,
    display_name: Option<&str>,
    tenant_id: Option<&str>,
) -> Result<UserData, ApplicationError> {
    let email = email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(validation("A valid email is required"));
    }
    if password.chars().count() < 8 {
        return Err(validation("Password must be at least 8 characters"));
    }
    let display_name = display_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| email.split('@').next().unwrap_or_default().to_string());

    let (tenant_id, role) = match tenant_id {
        Some(tenant_id) => {
            let tenant = repository
                .get_tenant(tenant_id)
                .await
                .map_err(|error| error.application_error())?;
            if tenant.is_none() {
                return Err(ApplicationError::NotFound {
                    message: format!("Tenant '{tenant_id}' was not found"),
                });
            }
            (tenant_id.to_string(), "member".to_string())
        }
        None => {
            let existing_tenant = repository
                .get_default_tenant()
                .await
                .map_err(|error| error.application_error())?;
            match existing_tenant {
                Some(tenant) => (tenant, "member".to_string()),
                None => {
                    let id = repository
                        .insert_tenant("Default Tenant")
                        .await
                        .map_err(|error| error.application_error())?;
                    (id, "admin".to_string())
                }
            }
        }
    };

    if repository
        .find_user_by_email(&email)
        .await
        .map_err(|error| error.application_error())?
        .is_some()
    {
        return Err(email_conflict());
    }

    let password_hash = security.hash_password(password).await;
    let user = repository
        .insert_user(&UserData {
            id: String::new(),
            tenant_id,
            email,
            password_hash,
            display_name,
            role,
            is_active: true,
        })
        .await
        .map_err(|error| error.application_error())?;
    Ok(user)
}

/// `POST /auth/login` — verify credentials, issue the pair.
pub async fn login(
    repository: &dyn AuthRepository,
    security: &dyn crate::ports::AuthSecurity,
    email: &str,
    password: &str,
) -> Result<IssuedTokens, ApplicationError> {
    let email = email.trim().to_lowercase();
    let user = repository
        .find_user_by_email(&email)
        .await
        .map_err(|error| error.application_error())?;
    let authenticated = match &user {
        Some(user) if user.is_active => {
            security
                .verify_password(password, &user.password_hash)
                .await
        }
        _ => false,
    };
    if !authenticated {
        return Err(unauthorized("Invalid email or password"));
    }
    issue_pair(repository, security, user.as_ref().expect("checked above")).await
}

/// `POST /auth/refresh` — single-use rotation.
pub async fn refresh(
    repository: &dyn AuthRepository,
    security: &dyn crate::ports::AuthSecurity,
    refresh_token: &str,
) -> Result<IssuedTokens, ApplicationError> {
    let token_hash = security.hash_refresh_token(refresh_token).await;
    let row = repository
        .find_refresh_token(&token_hash)
        .await
        .map_err(|error| error.application_error())?;
    let Some(row) = row else {
        return Err(unauthorized("Invalid refresh token"));
    };
    let now = chrono::Utc::now();
    if row.revoked_at.is_some() || row.expires_at < now {
        return Err(unauthorized("Refresh token expired or revoked"));
    }
    let user = repository
        .get_user(&row.user_id)
        .await
        .map_err(|error| error.application_error())?;
    let Some(user) = user.filter(|user| user.is_active) else {
        return Err(unauthorized("User is disabled"));
    };

    // Single-use, concurrency-safe: the guarded UPDATE wins for exactly one
    // caller; a racing second caller sees zero rows and gets the revoked
    // message.
    let revoked = repository
        .revoke_refresh_token_if_active(&token_hash, now)
        .await
        .map_err(|error| error.application_error())?;
    if !revoked {
        return Err(unauthorized("Refresh token expired or revoked"));
    }
    issue_pair(repository, security, &user).await
}

/// `POST /auth/logout` — revoke one refresh token, idempotently (204).
pub async fn logout(
    repository: &dyn AuthRepository,
    security: &dyn crate::ports::AuthSecurity,
    refresh_token: &str,
) -> Result<(), ApplicationError> {
    let token_hash = security.hash_refresh_token(refresh_token).await;
    repository
        .revoke_refresh_token_by_hash(&token_hash, chrono::Utc::now())
        .await
        .map_err(|error| error.application_error())?;
    Ok(())
}

/// Issue an access token plus a fresh persisted refresh token.
async fn issue_pair(
    repository: &dyn AuthRepository,
    security: &dyn crate::ports::AuthSecurity,
    user: &UserData,
) -> Result<IssuedTokens, ApplicationError> {
    let (access_token, expires_in) = security
        .create_access_token(&user.id, &user.tenant_id, &user.role)
        .await;
    let refresh_token = security.new_refresh_token().await;
    let token_hash = security.hash_refresh_token(&refresh_token).await;
    repository
        .insert_refresh_token(
            &user.id,
            &token_hash,
            chrono::Utc::now() + chrono::Duration::days(security.refresh_token_days()),
        )
        .await
        .map_err(|error| error.application_error())?;
    Ok(IssuedTokens {
        access_token,
        expires_in,
        refresh_token,
        user: user.clone(),
    })
}

/// `POST /shares` — grant (or upgrade) one recipient's access.
///
/// An existing (recipient, resource_type, resource_id) row is UPDATED to the
/// new permission, not a conflict — the baseline's upsert.
pub async fn create_share(
    repository: &dyn AuthRepository,
    actor_id: &str,
    recipient_id: &str,
    resource_type: &str,
    resource_id: &str,
    permission: &str,
) -> Result<ShareData, ApplicationError> {
    if resource_type != "candidate" && resource_type != "position" {
        return Err(validation(
            "resource_type must be one of ['candidate', 'position']",
        ));
    }
    if permission != "read" && permission != "write" {
        return Err(validation("permission must be one of ['read', 'write']"));
    }
    let recipient = repository
        .get_user(recipient_id)
        .await
        .map_err(|error| error.application_error())?;
    if recipient.is_none() {
        return Err(ApplicationError::NotFound {
            message: format!("User '{recipient_id}' was not found"),
        });
    }
    if recipient_id == actor_id {
        return Err(validation("Cannot share with yourself"));
    }
    repository
        .upsert_share(
            actor_id,
            recipient_id,
            resource_type,
            resource_id,
            permission,
        )
        .await
        .map_err(|error| error.application_error())
}

/// `GET /shares` — the user's shares (granted or received), newest first.
pub async fn list_shares(
    repository: &dyn AuthRepository,
    user_id: &str,
) -> Result<Vec<ShareData>, ApplicationError> {
    repository
        .list_shares_for_user(user_id)
        .await
        .map_err(|error| error.application_error())
}

/// `DELETE /shares/{id}` — owner or admin only (the local user is admin).
pub async fn delete_share(
    repository: &dyn AuthRepository,
    share_id: &str,
    actor_id: &str,
    is_admin: bool,
) -> Result<(), ApplicationError> {
    let share = repository
        .get_share(share_id)
        .await
        .map_err(|error| error.application_error())?;
    let Some(share) = share else {
        return Err(ApplicationError::NotFound {
            message: format!("Share '{share_id}' was not found"),
        });
    };
    if share.owner_id != actor_id && !is_admin {
        return Err(unauthorized("Only the owner can revoke this share"));
    }
    repository
        .delete_share(share_id)
        .await
        .map_err(|error| error.application_error())
}
