//! SEC-P1 AUTH-08: signed identity assertion for the internal Sidecar channel.
//!
//! The Python browser sidecar listens on loopback and is reached through the
//! Tauri host proxy. A client must never be able to mint its own identity for
//! that channel: the host verifies the caller's access token, then attaches a
//! short-lived HS256 assertion that binds the verified actor/tenant to the
//! exact bearer token presented on this request. The sidecar verifies the
//! assertion (signature, audience, issuer, expiry, nonce and bearer binding)
//! before it trusts any identity claim. Direct anonymous loopback access is
//! rejected by the sidecar independently.
//!
//! This module deliberately does not reuse either service's long-lived JWT
//! signing key: the assertion key is generated per host process and handed to
//! the sidecar through its process environment.

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Header carrying the signed assertion. Added to the proxy request allowlist
/// in `ica-contracts::proxy_headers`.
pub const ASSERTION_HEADER: &str = "x-ica-internal-assertion";
pub const ISSUER: &str = "ica-desktop-host";
pub const AUDIENCE: &str = "ica-sidecar";
/// Assertions are single-request credentials; a minute is ample for a proxied
/// call and keeps the replay window small.
pub const TTL_SECONDS: i64 = 60;

/// Access-token claims the host needs in order to build an assertion. This is
/// the same shape `ica-api::auth` verifies; the desktop adapter must not depend
/// on `ica-api`, so the verification primitive lives here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessIdentity {
    pub sub: String,
    pub tenant_id: String,
    pub role: String,
    #[serde(rename = "type")]
    pub token_type: String,
}

/// The signed assertion claims. `bearer_fp` binds the assertion to one bearer
/// token; replaying it with a different token fails verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionClaims {
    pub sub: String,
    pub tenant_id: String,
    pub role: String,
    pub bearer_fp: String,
    pub request_id: String,
    pub iss: String,
    pub aud: String,
    pub jti: String,
    pub iat: i64,
    pub exp: i64,
}

/// A fresh 256-bit-equivalent process-local secret. It is not persisted: a
/// restart invalidates outstanding assertions, which is the desired behaviour
/// for single-request internal credentials.
pub fn generate_secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// SHA-256 fingerprint of a bearer token. Python uses
/// `hashlib.sha256(token.encode()).hexdigest()` for the same value.
pub fn bearer_fingerprint(bearer: &str) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let digest = Sha256::digest(bearer.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Verify an access bearer at the host boundary. `type == "access"` is
/// mandatory so a refresh token can never be delegated to the sidecar.
pub fn verify_access_bearer(token: &str, secret: &str) -> Result<AccessIdentity, String> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["exp", "sub"]);
    let decoded = decode::<AccessIdentity>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| "Invalid or expired access token".to_string())?;
    if decoded.claims.token_type != "access" {
        return Err("A refresh token cannot be used as a bearer token".to_string());
    }
    Ok(decoded.claims)
}

/// Mint the internal assertion. `request_id` may be empty when the webview did
/// not send one; the assertion still carries a unique `jti` nonce.
pub fn mint_assertion(
    bearer: &str,
    identity: &AccessIdentity,
    request_id: &str,
    secret: &str,
    now: DateTime<Utc>,
) -> Result<String, String> {
    let claims = AssertionClaims {
        sub: identity.sub.clone(),
        tenant_id: identity.tenant_id.clone(),
        role: identity.role.clone(),
        bearer_fp: bearer_fingerprint(bearer),
        request_id: request_id.to_string(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(TTL_SECONDS)).timestamp(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|error| format!("cannot sign internal assertion: {error}"))
}

/// Convenience wrapper for callers that do not own a clock (the Tauri host).
pub fn mint_assertion_now(
    bearer: &str,
    identity: &AccessIdentity,
    request_id: &str,
    secret: &str,
) -> Result<String, String> {
    mint_assertion(bearer, identity, request_id, secret, Utc::now())
}

/// Verify an assertion and its binding to the presented bearer token. The
/// caller (sidecar) additionally enforces `jti` single-use in its own process.
pub fn verify_assertion(
    assertion: &str,
    bearer: &str,
    secret: &str,
    now: DateTime<Utc>,
) -> Result<AssertionClaims, String> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["exp", "iat", "sub", "jti", "iss", "aud"]);
    validation.set_audience(&[AUDIENCE]);
    validation.set_issuer(&[ISSUER]);
    let decoded = decode::<AssertionClaims>(
        assertion,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| "Invalid internal identity assertion".to_string())?;
    let claims = decoded.claims;
    if claims.iat > now.timestamp() + 5 {
        return Err("Internal assertion is not yet valid".to_string());
    }
    if claims.exp <= now.timestamp() {
        return Err("Internal assertion has expired".to_string());
    }
    if claims.bearer_fp != bearer_fingerprint(bearer) {
        return Err("Internal assertion is bound to a different bearer".to_string());
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> AccessIdentity {
        AccessIdentity {
            sub: "u-1".into(),
            tenant_id: "t-1".into(),
            role: "admin".into(),
            token_type: "access".into(),
        }
    }

    fn bearer(secret: &str, claims: serde_json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("bearer")
    }

    #[test]
    fn a_minted_assertion_verifies_and_binds_identity() {
        let now = Utc::now();
        let secret = generate_secret();
        let token = bearer(
            "jwt",
            serde_json::json!({
                "sub": "u-1", "tenant_id": "t-1", "role": "admin",
                "type": "access", "exp": (now + Duration::hours(1)).timestamp()
            }),
        );
        let verified = verify_access_bearer(&token, "jwt").expect("bearer");
        let assertion =
            mint_assertion(&token, &verified, "req-1", &secret, now).expect("assertion");
        let claims = verify_assertion(&assertion, &token, &secret, now).expect("verify");
        assert_eq!(claims.sub, "u-1");
        assert_eq!(claims.tenant_id, "t-1");
        assert_eq!(claims.request_id, "req-1");
        assert_eq!(claims.iss, ISSUER);
        assert_eq!(claims.aud, AUDIENCE);
    }

    #[test]
    fn forged_tampered_and_mismatched_assertions_are_rejected() {
        let now = Utc::now();
        let secret = generate_secret();
        let token = bearer(
            "jwt",
            serde_json::json!({
                "sub": "u-1", "tenant_id": "t-1", "role": "admin",
                "type": "access", "exp": (now + Duration::hours(1)).timestamp()
            }),
        );
        let assertion =
            mint_assertion(&token, &identity(), "req-1", &secret, now).expect("assertion");

        assert!(verify_assertion(&assertion, &token, "other-secret", now).is_err());
        assert!(verify_assertion("not-a-jwt", &token, &secret, now).is_err());
        // A different bearer must not be able to reuse this assertion.
        assert!(verify_assertion(&assertion, "different-bearer", &secret, now).is_err());
        // Expired assertions are rejected even with a valid signature.
        assert!(verify_assertion(
            &assertion,
            &token,
            &secret,
            now + Duration::seconds(TTL_SECONDS + 1)
        )
        .is_err());
    }

    #[test]
    fn refresh_tokens_cannot_be_delegated() {
        let now = Utc::now();
        let refresh = bearer(
            "jwt",
            serde_json::json!({
                "sub": "u-1", "tenant_id": "t-1", "role": "admin",
                "type": "refresh", "exp": (now + Duration::hours(1)).timestamp()
            }),
        );
        assert!(verify_access_bearer(&refresh, "jwt").is_err());
    }
}
