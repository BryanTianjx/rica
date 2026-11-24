//! The auth adapters (W6): users/tenants/refresh-tokens/shares over the
//! same pool, bcrypt password hashing, HS256 access tokens and random
//! refresh tokens.
//!
//! The token semantics mirror `app/auth/security.py`: claims
//! `{sub, tenant_id, role, type:"access", iat, exp}` signed HS256, a refresh
//! token of 48 random bytes URL-safe-base64 (no padding), stored as a
//! SHA-256 hash, expiring after `jwt_refresh_token_expire_days` days.
//! Rotation is a guarded UPDATE so exactly one concurrent caller wins.

use chrono::Utc;
use rand::RngCore;
use sqlx::FromRow;

use crate::store::{new_uuid, repo_error, to_sql_timestamp, SqliteStore};
use ica_application::auth::{RefreshTokenRow, ShareData, UserData};
use ica_application::ports::{AuthRepository, AuthSecurity, RepositoryError};

#[derive(Debug, FromRow)]
struct UserRow {
    id: String,
    tenant_id: String,
    email: String,
    password_hash: String,
    display_name: String,
    role: String,
    is_active: i64,
}

impl UserRow {
    fn into_data(self) -> UserData {
        UserData {
            id: self.id,
            tenant_id: self.tenant_id,
            email: self.email,
            password_hash: self.password_hash,
            display_name: self.display_name,
            role: self.role,
            is_active: self.is_active != 0,
        }
    }
}

const USER_COLUMNS: &str = "id, tenant_id, email, password_hash, display_name, role, is_active";

#[async_trait::async_trait]
impl AuthRepository for SqliteStore {
    async fn get_user(&self, id: &str) -> Result<Option<UserData>, RepositoryError> {
        let row: Option<UserRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {USER_COLUMNS} FROM users WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(row.map(UserRow::into_data))
    }

    async fn find_user_by_email(&self, email: &str) -> Result<Option<UserData>, RepositoryError> {
        let row: Option<UserRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {USER_COLUMNS} FROM users WHERE email = ?"
        )))
        .bind(email)
        .fetch_optional(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(row.map(UserRow::into_data))
    }

    async fn insert_user(&self, user: &UserData) -> Result<UserData, RepositoryError> {
        let id = new_uuid();
        let now = to_sql_timestamp(&Utc::now());
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO users (id, tenant_id, email, password_hash, display_name,",
            " role, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&user.tenant_id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.display_name)
        .bind(&user.role)
        .bind(user.is_active)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;
        Ok(UserData { id, ..user.clone() })
    }

    async fn get_default_tenant(&self) -> Result<Option<String>, RepositoryError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT id FROM tenants ORDER BY created_at ASC LIMIT 1")
                .fetch_optional(self.pool())
                .await
                .map_err(repo_error)?;
        Ok(row.map(|(id,)| id))
    }

    async fn get_tenant(&self, id: &str) -> Result<Option<String>, RepositoryError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT id FROM tenants WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(repo_error)?;
        Ok(row.map(|(id,)| id))
    }

    async fn insert_tenant(&self, name: &str) -> Result<String, RepositoryError> {
        let id = new_uuid();
        let now = to_sql_timestamp(&Utc::now());
        sqlx::query("INSERT INTO tenants (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(&now)
            .bind(&now)
            .execute(self.pool())
            .await
            .map_err(repo_error)?;
        Ok(id)
    }

    async fn insert_refresh_token(
        &self,
        user_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        sqlx::query(concat!(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, revoked_at,",
            " created_at) VALUES (?, ?, ?, ?, NULL, ?)"
        ))
        .bind(new_uuid())
        .bind(user_id)
        .bind(token_hash)
        .bind(to_sql_timestamp(&expires_at))
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRow>, RepositoryError> {
        let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT user_id, token_hash, expires_at, revoked_at FROM refresh_tokens \
             WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(
            row.map(|(user_id, hash, expires_at, revoked_at)| RefreshTokenRow {
                user_id,
                token_hash: hash,
                expires_at: crate::time::parse_sqlite_datetime(&expires_at)
                    .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC),
                revoked_at: revoked_at
                    .and_then(|raw| crate::time::parse_sqlite_datetime(&raw).ok()),
            }),
        )
    }

    async fn revoke_refresh_token_if_active(
        &self,
        token_hash: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        let outcome = sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = ? \
             WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?",
        )
        .bind(to_sql_timestamp(&now))
        .bind(token_hash)
        .bind(to_sql_timestamp(&now))
        .execute(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn revoke_refresh_token_by_hash(
        &self,
        token_hash: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = ? \
             WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(to_sql_timestamp(&now))
        .bind(token_hash)
        .execute(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn upsert_share(
        &self,
        owner_id: &str,
        recipient_id: &str,
        resource_type: &str,
        resource_id: &str,
        permission: &str,
    ) -> Result<ShareData, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        let now = to_sql_timestamp(&Utc::now());
        let updated = sqlx::query(
            "UPDATE shares SET permission = ? \
             WHERE recipient_id = ? AND resource_type = ? AND resource_id = ?",
        )
        .bind(permission)
        .bind(recipient_id)
        .bind(resource_type)
        .bind(resource_id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;
        if updated.rows_affected() == 0 {
            sqlx::query(concat!(
                "INSERT INTO shares (id, owner_id, recipient_id, resource_type,",
                " resource_id, permission, created_at)",
                " VALUES (?, ?, ?, ?, ?, ?, ?)"
            ))
            .bind(new_uuid())
            .bind(owner_id)
            .bind(recipient_id)
            .bind(resource_type)
            .bind(resource_id)
            .bind(permission)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;
        }
        tx.commit().await.map_err(repo_error)?;

        let row: Option<(String, String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, owner_id, recipient_id, resource_type, resource_id, permission, created_at \
             FROM shares WHERE recipient_id = ? AND resource_type = ? AND resource_id = ?",
        )
        .bind(recipient_id)
        .bind(resource_type)
        .bind(resource_id)
        .fetch_optional(self.pool())
        .await
        .map_err(repo_error)?;
        let (id, owner, recipient, rtype, rid, perm, created_at) =
            row.ok_or_else(|| repo_error(sqlx::Error::RowNotFound))?;
        Ok(ShareData {
            id,
            owner_id: owner,
            recipient_id: recipient,
            resource_type: rtype,
            resource_id: rid,
            permission: perm,
            created_at: crate::time::parse_sqlite_datetime(&created_at)
                .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC),
        })
    }

    async fn list_shares_for_user(&self, user_id: &str) -> Result<Vec<ShareData>, RepositoryError> {
        let rows: Vec<(String, String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, owner_id, recipient_id, resource_type, resource_id, permission, created_at \
             FROM shares WHERE owner_id = ? OR recipient_id = ? ORDER BY created_at DESC",
        )
        .bind(user_id)
        .bind(user_id)
        .fetch_all(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    owner_id,
                    recipient_id,
                    resource_type,
                    resource_id,
                    permission,
                    created_at,
                )| {
                    ShareData {
                        id,
                        owner_id,
                        recipient_id,
                        resource_type,
                        resource_id,
                        permission,
                        created_at: crate::time::parse_sqlite_datetime(&created_at)
                            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC),
                    }
                },
            )
            .collect())
    }

    async fn get_share(&self, id: &str) -> Result<Option<ShareData>, RepositoryError> {
        let rows: Vec<(String, String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, owner_id, recipient_id, resource_type, resource_id, permission, created_at \
             FROM shares WHERE id = ?",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    owner_id,
                    recipient_id,
                    resource_type,
                    resource_id,
                    permission,
                    created_at,
                )| {
                    ShareData {
                        id,
                        owner_id,
                        recipient_id,
                        resource_type,
                        resource_id,
                        permission,
                        created_at: crate::time::parse_sqlite_datetime(&created_at)
                            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC),
                    }
                },
            )
            .next())
    }

    async fn delete_share(&self, id: &str) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM shares WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(repo_error)?;
        Ok(())
    }
}

/// The crypto adapter: bcrypt (cost configurable, matching
/// `settings.bcrypt_cost`), HS256 access tokens via `jsonwebtoken`, and
/// 48-byte URL-safe refresh tokens.
pub struct AuthCrypto {
    pub jwt_secret: String,
    pub access_token_minutes: i64,
    pub refresh_token_days: i64,
    pub bcrypt_cost: u32,
}

#[async_trait::async_trait]
impl AuthSecurity for AuthCrypto {
    async fn hash_password(&self, plain: &str) -> String {
        let cost = self.bcrypt_cost;
        let plain = plain.to_string();
        tokio::task::spawn_blocking(move || bcrypt::hash(plain, cost).unwrap_or_default())
            .await
            .unwrap_or_default()
    }

    async fn verify_password(&self, plain: &str, hash: &str) -> bool {
        let plain = plain.to_string();
        let hash = hash.to_string();
        tokio::task::spawn_blocking(move || bcrypt::verify(plain, &hash).unwrap_or(false))
            .await
            .unwrap_or(false)
    }

    async fn create_access_token(
        &self,
        user_id: &str,
        tenant_id: &str,
        role: &str,
    ) -> (String, i64) {
        let now = Utc::now();
        let expires_in = self.access_token_minutes * 60;
        let claims = serde_json::json!({
            "sub": user_id,
            "tenant_id": tenant_id,
            "role": role,
            "type": "access",
            "iat": now.timestamp(),
            "exp": (now + chrono::Duration::seconds(expires_in)).timestamp(),
        });
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )
        .unwrap_or_default();
        (token, expires_in)
    }

    async fn new_refresh_token(&self) -> String {
        use base64::Engine;
        let mut bytes = [0u8; 48];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    async fn hash_refresh_token(&self, token: &str) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn refresh_token_days(&self) -> i64 {
        self.refresh_token_days
    }
}
