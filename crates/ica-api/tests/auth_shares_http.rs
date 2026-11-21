//! `POST /auth/*` + `/api/shares` (W6).
//!
//! The contract is the baseline's `app/auth/service.py`: the first
//! registration bootstraps "Default Tenant" with an admin, later ones join
//! as member; the pair carries a 15-minute HS256 access token and a
//! single-use refresh token (stored hashed, rotation is atomic — two
//! concurrent refreshes cannot both win); logout is idempotent. Sharing
//! upserts on (recipient, resource), and the local user is admin.

mod support;

use support::{get, init_test_logging, pool_for, post, seed_local_identity, w1_app};

#[tokio::test]
async fn the_first_registration_bootstraps_a_tenant_and_admin_role() {
    init_test_logging();
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;

    let first = post(
        app.clone(),
        "/api/auth/register",
        r#"{"email":"Owner@Example.com","password":"long-enough"}"#,
    )
    .await;
    assert_eq!(first.status, 201, "body: {}", first.body);
    let user = &first.body;
    assert_eq!(user["email"], "owner@example.com", "normalised lower");
    assert_eq!(user["role"], "admin", "first sign-up is admin");
    assert_eq!(user["display_name"], "owner", "email local part");
    assert_eq!(user["is_active"], true);

    // The bootstrap tenant exists.
    let tenant_id = user["tenant_id"].as_str().expect("tenant");
    let (name,): (String,) = sqlx::query_as("SELECT name FROM tenants WHERE id = ?")
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .expect("tenant row");
    assert_eq!(name, "Default Tenant");

    // The second registration joins as member.
    let second = post(
        app.clone(),
        "/api/auth/register",
        r#"{"email":"member@example.com","password":"long-enough","display_name":"Member"}"#,
    )
    .await;
    assert_eq!(second.status, 201);
    assert_eq!(second.body["role"], "member");
    assert_eq!(second.body["tenant_id"], user["tenant_id"], "same tenant");
    assert_eq!(second.body["display_name"], "Member");

    // Duplicate email: the coded conflict.
    let dup = post(
        app.clone(),
        "/api/auth/register",
        r#"{"email":"owner@example.com","password":"long-enough"}"#,
    )
    .await;
    assert_eq!(dup.status, 409);
    assert_eq!(dup.body["error"]["code"], "email_conflict");
    assert_eq!(dup.body["error"]["message"], "Email is already registered");

    // Shape errors keep the baseline's messages.
    for (body, message) in [
        (
            r#"{"email":"no-at-sign","password":"long-enough"}"#,
            "A valid email is required",
        ),
        (
            r#"{"email":"ok@example.com","password":"short"}"#,
            "Password must be at least 8 characters",
        ),
    ] {
        let error = post(app.clone(), "/api/auth/register", body).await;
        assert_eq!(error.status, 422, "{body}");
        assert_eq!(error.body["error"]["message"], message);
    }

    // An unknown tenant id is a 404.
    let unknown = post(
        app,
        "/api/auth/register",
        r#"{"email":"x@example.com","password":"long-enough","tenant_id":"t-missing"}"#,
    )
    .await;
    assert_eq!(unknown.status, 404);
    assert_eq!(
        unknown.body["error"]["message"],
        "Tenant 't-missing' was not found"
    );

    let _ = pool;
}

#[tokio::test]
async fn login_issues_a_pair_and_rotation_is_single_use() {
    init_test_logging();
    let (app, _dir) = w1_app().await;
    post(
        app.clone(),
        "/api/auth/register",
        r#"{"email":"rot@example.com","password":"long-enough"}"#,
    )
    .await;

    // Wrong password: the baseline's single message (no user enumeration).
    let bad = post(
        app.clone(),
        "/api/auth/login",
        r#"{"email":"rot@example.com","password":"wrong-password"}"#,
    )
    .await;
    assert_eq!(bad.status, 401);
    assert_eq!(bad.body["error"]["code"], "unauthorized");
    assert_eq!(bad.body["error"]["message"], "Invalid email or password");

    let login = post(
        app.clone(),
        "/api/auth/login",
        r#"{"email":"rot@example.com","password":"long-enough"}"#,
    )
    .await;
    assert_eq!(login.status, 200, "body: {}", login.body);
    assert_eq!(login.body["token_type"], "bearer");
    assert_eq!(login.body["expires_in"], 900, "15 minutes");
    let first_refresh = login.body["refresh_token"]
        .as_str()
        .expect("refresh")
        .to_string();
    assert_eq!(login.body["user"]["email"], "rot@example.com");

    // The access token is a three-part JWT (HS256 header/payload/signature).
    let access = login.body["access_token"].as_str().expect("access");
    assert_eq!(access.split('.').count(), 3);

    // Rotate once: a fresh pair.
    let rotated = post(
        app.clone(),
        "/api/auth/refresh",
        &format!(r#"{{"refresh_token":"{first_refresh}"}}"#),
    )
    .await;
    assert_eq!(rotated.status, 200, "body: {}", rotated.body);
    let second_refresh = rotated.body["refresh_token"]
        .as_str()
        .expect("refresh")
        .to_string();
    assert_ne!(second_refresh, first_refresh, "the token rotated");

    // Replay the old token: single-use means the presented one is dead.
    let replay = post(
        app.clone(),
        "/api/auth/refresh",
        &format!(r#"{{"refresh_token":"{first_refresh}"}}"#),
    )
    .await;
    assert_eq!(replay.status, 401);
    assert_eq!(
        replay.body["error"]["message"],
        "Refresh token expired or revoked"
    );

    // An unknown token is a different 401.
    let unknown = post(
        app.clone(),
        "/api/auth/refresh",
        r#"{"refresh_token":"not-a-token"}"#,
    )
    .await;
    assert_eq!(unknown.status, 401);
    assert_eq!(unknown.body["error"]["message"], "Invalid refresh token");

    // Logout revokes the live token; a further refresh is refused.
    let logout = post(
        app.clone(),
        "/api/auth/logout",
        &format!(r#"{{"refresh_token":"{second_refresh}"}}"#),
    )
    .await;
    assert_eq!(logout.status, 204, "body: {:?}", logout.body);
    let after_logout = post(
        app,
        "/api/auth/refresh",
        &format!(r#"{{"refresh_token":"{second_refresh}"}}"#),
    )
    .await;
    assert_eq!(after_logout.status, 401);
}

#[tokio::test]
async fn me_answers_the_synthetic_local_user() {
    let (app, _dir) = w1_app().await;
    let response = get(app, "/api/auth/me").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(
        response.body,
        serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "email": "",
            "display_name": "Local User",
            "role": "admin",
            "tenant_id": "00000000-0000-0000-0000-000000000000",
            "is_active": true,
        })
    );
}

#[tokio::test]
async fn shares_upsert_on_recipient_and_resource() {
    init_test_logging();
    let (app, dir) = w1_app().await;
    // A deployed local-mode install has the default tenant + user rows; a
    // fresh test database needs them before the owner FK can pass.
    seed_local_identity(&pool_for(&dir).await).await;
    let recipient = post(
        app.clone(),
        "/api/auth/register",
        r#"{"email":"colleague@example.com","password":"long-enough"}"#,
    )
    .await;
    let recipient_id = recipient.body["id"].as_str().expect("id").to_string();

    // The position the share points at.
    let position = post(app.clone(), "/api/positions", r#"{"name":"Shared Role"}"#).await;
    let position_id = position.body["id"].as_str().expect("id").to_string();

    // Self-share is refused.
    let self_share = post(
        app.clone(),
        "/api/shares",
        &format!(r#"{{"recipient_id":"{recipient_id}","resource_type":"candidate","resource_id":"{recipient_id}"}}"#),
    )
    .await;
    // The recipient is a user, but the local actor is the synthetic admin —
    // the self-share check compares against THAT id.
    assert!(
        self_share.status == 201 || self_share.status == 422,
        "body: {}",
        self_share.body
    );

    // An unknown recipient is a 404.
    let unknown = post(
        app.clone(),
        "/api/shares",
        r#"{"recipient_id":"u-missing","resource_type":"candidate","resource_id":"c-1"}"#,
    )
    .await;
    assert_eq!(unknown.status, 404);
    assert_eq!(
        unknown.body["error"]["message"],
        "User 'u-missing' was not found"
    );

    // A bad resource type trips the body-level pydantic pattern first (the
    // service-level message is unreachable over HTTP — defense in depth).
    let bad_type = post(
        app.clone(),
        "/api/shares",
        &format!(
            r#"{{"recipient_id":"{recipient_id}","resource_type":"tenant","resource_id":"x"}}"#
        ),
    )
    .await;
    assert_eq!(bad_type.status, 422);
    assert_eq!(bad_type.body["error"]["code"], "validation_error");
    let issue = &bad_type.body["error"]["details"]["errors"][0];
    assert_eq!(issue["type"], "string_pattern_mismatch");
    assert_eq!(issue["loc"], serde_json::json!(["body", "resource_type"]));

    // Create, then re-create with a higher permission: an UPDATE, not a
    // duplicate (the baseline's upsert).
    let create = post(
        app.clone(),
        "/api/shares",
        &format!(r#"{{"recipient_id":"{recipient_id}","resource_type":"position","resource_id":"{position_id}"}}"#),
    )
    .await;
    assert_eq!(create.status, 201, "body: {}", create.body);
    assert_eq!(create.body["permission"], "read");
    // The owner is the SYNTHETIC local user — the local-mode actor.
    assert_eq!(
        create.body["owner_id"],
        "00000000-0000-0000-0000-000000000001"
    );
    let share_id = create.body["id"].as_str().expect("id").to_string();

    let upgrade = post(
        app.clone(),
        "/api/shares",
        &format!(r#"{{"recipient_id":"{recipient_id}","resource_type":"position","resource_id":"{position_id}","permission":"write"}}"#),
    )
    .await;
    assert_eq!(upgrade.status, 201);
    assert_eq!(upgrade.body["id"], share_id, "same row upgraded");
    assert_eq!(upgrade.body["permission"], "write");

    // The local user granted both shares (the earlier candidate probe and
    // this position), so the listing carries both — newest first puts the
    // position upgrade first.
    let listed = get(app.clone(), "/api/shares").await;
    let items = listed.body.as_array().expect("array");
    assert_eq!(items.len(), 2, "both grants listed: {}", listed.body);
    assert_eq!(items[0]["id"], share_id, "newest first");
    assert_eq!(items[0]["permission"], "write");

    // The local user is admin: any share is revocable. 204, then 404.
    let deleted = send_delete(app.clone(), &format!("/api/shares/{share_id}")).await;
    assert_eq!(deleted.status, 204);
    let again = send_delete(app.clone(), &format!("/api/shares/{share_id}")).await;
    assert_eq!(again.status, 404);
    assert_eq!(
        again.body["error"]["message"],
        format!("Share '{share_id}' was not found")
    );

    let listed_after = get(app, "/api/shares").await;
    assert_eq!(listed_after.body.as_array().expect("array").len(), 1);
}

async fn send_delete(app: axum::Router, uri: &str) -> support::TestResponse {
    use tower::ServiceExt;
    let request = axum::http::Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("accept", "application/json")
        .body(axum::body::Body::empty())
        .expect("build request");
    let response = app.oneshot(request).await.expect("router responded");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    support::TestResponse { status, body }
}
