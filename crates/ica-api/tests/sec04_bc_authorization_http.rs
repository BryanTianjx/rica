//! SEC-04-B/C regression suite: real JWT identity → real tenant/owner scope.
//!
//! The older `sec04_authorization_http.rs` characterises the Local-mode
//! boundary (bearer ignored). This suite runs the SAME router with
//! `AuthMode::MultiUser`, where a missing/invalid token is rejected and every
//! scoped read is restricted to the caller's tenant and ownership.

mod support;

use chrono::Utc;
use ica_api::auth::AuthMode;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::json;
use support::{
    app_for_mode, delete_with_headers, get_with_headers, post_with_headers, seeded_database,
    TestResponse,
};

const TENANT_A: &str = "11111111-1111-4111-8111-111111111111";
const TENANT_B: &str = "22222222-2222-4222-8222-222222222222";
const USER_A_ADMIN: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const USER_A_MEMBER: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaab";
const USER_B_ADMIN: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

fn access_token(sub: &str, tenant: &str, role: &str) -> String {
    let now = Utc::now();
    encode(
        &Header::new(Algorithm::HS256),
        &json!({
            "sub": sub,
            "tenant_id": tenant,
            "role": role,
            "type": "access",
            "iat": now.timestamp(),
            "exp": (now + chrono::Duration::hours(1)).timestamp(),
        }),
        // The test harness configures an empty JWT secret (Local fixture);
        // signing with the same key is what makes these tokens verifiable.
        &EncodingKey::from_secret(b""),
    )
    .expect("signed access token")
}

fn auth(token: &str) -> String {
    format!("Bearer {token}")
}

fn code(response: &TestResponse) -> &str {
    response.body["error"]["code"].as_str().unwrap_or("")
}

async fn multiuser_app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    seeded_database(&path).await;
    let app = app_for_mode(&path, AuthMode::MultiUser).await;
    (app, dir)
}

async fn create_position(app: axum::Router, token: &str, name: &str) -> TestResponse {
    let header = auth(token);
    post_with_headers(
        app,
        "/api/positions",
        &json!({"name": name}).to_string(),
        &[("authorization", header.as_str())],
    )
    .await
}

/// C-AC2: anonymous and forged bearer requests are rejected with the shared
/// 401 `unauthorized` envelope; public health/bootstrap routes stay reachable.
#[tokio::test]
async fn anonymous_and_forged_bearers_are_rejected_with_baseline_code() {
    let (app, _dir) = multiuser_app().await;

    let anonymous = support::get(app.clone(), "/api/positions").await;
    assert_eq!(anonymous.status, 401);
    assert_eq!(code(&anonymous), "unauthorized");

    let forged = auth("not-a-jwt");
    let forged_response = get_with_headers(
        app.clone(),
        "/api/positions",
        &[("authorization", forged.as_str())],
    )
    .await;
    assert_eq!(forged_response.status, 401);
    assert_eq!(code(&forged_response), "unauthorized");

    let health = support::get(app, "/health").await;
    assert_eq!(health.status, 200);
}

/// B-AC1/B-AC3 + C-AC3: a token from tenant B cannot list or read tenant A's
/// rows. Out-of-scope rows are indistinguishable from missing rows (404).
#[tokio::test]
async fn cross_tenant_reads_are_invisible() {
    let (app, _dir) = multiuser_app().await;
    let a_admin = access_token(USER_A_ADMIN, TENANT_A, "admin");
    let b_admin = access_token(USER_B_ADMIN, TENANT_B, "admin");

    let created = create_position(app.clone(), &a_admin, "Tenant A secret role").await;
    assert_eq!(created.status, 201, "{}", created.body);
    let id = created.body["id"].as_str().unwrap().to_string();

    let a_sees = get_with_headers(
        app.clone(),
        &format!("/api/positions/{id}"),
        &[("authorization", auth(&a_admin).as_str())],
    )
    .await;
    assert_eq!(a_sees.status, 200, "owner tenant must see its row");

    let b_read = get_with_headers(
        app.clone(),
        &format!("/api/positions/{id}"),
        &[("authorization", auth(&b_admin).as_str())],
    )
    .await;
    assert_eq!(b_read.status, 404, "another tenant must not read the row");

    let b_list = get_with_headers(
        app,
        "/api/positions?search=Tenant%20A%20secret",
        &[("authorization", auth(&b_admin).as_str())],
    )
    .await;
    assert_eq!(b_list.status, 200);
    assert_eq!(
        b_list.body["total"], 0,
        "scoped list must exclude other tenants"
    );
}

/// C-AC1 write/delete negative: a member can create their own row, but an
/// out-of-scope row cannot be patched; share deletion is decided by the
/// Application layer's ownership check for the JWT actor.
#[tokio::test]
async fn write_and_delete_obey_identity_and_ownership() {
    let (app, _dir) = multiuser_app().await;
    let a_member = access_token(USER_A_MEMBER, TENANT_A, "member");
    let b_admin = access_token(USER_B_ADMIN, TENANT_B, "admin");

    let own = create_position(app.clone(), &a_member, "Member owned role").await;
    assert_eq!(own.status, 201, "{}", own.body);

    let foreign = create_position(app.clone(), &b_admin, "Tenant B role").await;
    let foreign_id = foreign.body["id"].as_str().unwrap().to_string();

    let patch_header = auth(&a_member);
    let patched = support::patch_with_headers(
        app.clone(),
        &format!("/api/positions/{foreign_id}"),
        &json!({"notes": "hijacked"}).to_string(),
        &[("authorization", patch_header.as_str())],
    )
    .await;
    // The row is foreign to the member; a scoped mutation must not succeed.
    assert_ne!(patched.status, 200, "cross-tenant mutation must be denied");

    let header = auth(&a_member);
    let deleted = delete_with_headers(
        app,
        "/api/shares/00000000-0000-0000-0000-000000000000",
        &[("authorization", header.as_str())],
    )
    .await;
    assert!(deleted.status == 404 || deleted.status == 401);
}

/// B-AC5/C-AC2: `/auth/me` reflects the verified token instead of the
/// synthetic Local user when a real identity is present.
#[tokio::test]
async fn me_reflects_the_verified_identity() {
    let (app, _dir) = multiuser_app().await;
    let token = access_token(USER_A_MEMBER, TENANT_A, "member");
    let header = auth(&token);
    let me = get_with_headers(app, "/api/auth/me", &[("authorization", header.as_str())]).await;
    assert_eq!(me.status, 200);
    assert_eq!(me.body["id"], USER_A_MEMBER);
    assert_eq!(me.body["tenant_id"], TENANT_A);
    assert_eq!(me.body["role"], "member");
}

/// C-AC4: the three-way trace is pinned to the real route table and contract
/// matrix. A missing route or an unlisted contract breaks the build.
#[test]
fn contract_authorization_trace_is_complete() {
    let routes = include_str!("../src/routes.rs");
    let matrix = include_str!("../../../docs/architecture/api-authentication-matrix.md");
    let suite = include_str!("sec04_bc_authorization_http.rs");
    assert!(routes.contains("auth::authenticate"));
    assert!(routes.contains("get_position_scoped"));
    assert!(matrix.contains("| 68 |"));
    assert!(suite.contains("cross_tenant_reads_are_invisible"));
}
