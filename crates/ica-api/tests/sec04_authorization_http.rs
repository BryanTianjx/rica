//! SEC-04 (AUD-P1-04) — authorization / IDOR characterization tests.
//!
//! These tests do NOT introduce a multi-user mode. They pin what the current
//! Local Mode actually enforces in the *trusted backend* (the Application
//! use cases), so the API authentication matrix and the SEC-04 report can cite
//! executable evidence instead of prose.
//!
//! Local Mode is single-user and loopback-only by default (`HOST=127.0.0.1`).
//! A business route that answers without a token is a documented Local Mode
//! boundary here — NOT an approval of unauthenticated access in Multi-user
//! Mode. The `local_mode_does_not_gate_business_routes_on_a_bearer_token`
//! characterization test is expected to FAIL (and must be replaced) when a
//! real authentication gate is introduced.

mod support;

use support::{get, init_test_logging, pool_for, post, seed_local_identity, w1_app, TestResponse};
use tower::ServiceExt;

const LOCAL_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// SEC04 self-share: the Application layer rejects recipient == trusted actor.
///
/// The existing `auth_shares_http.rs` probe tolerates 201 because it uses a
/// *different* user as recipient. This test closes that gap by presenting the
/// synthetic local actor's own id, which must be refused by
/// `ica_application::auth::create_share` (auth.rs:353-355).
#[tokio::test]
async fn self_share_is_rejected_by_the_application_layer() {
    init_test_logging();
    let (app, dir) = w1_app().await;
    seed_local_identity(&pool_for(&dir).await).await;

    let body = format!(
        r#"{{"recipient_id":"{LOCAL_USER_ID}","resource_type":"candidate","resource_id":"c-1"}}"#
    );
    let response = post(app, "/api/shares", &body).await;
    assert_eq!(response.status, 422, "body: {}", response.body);
    assert_eq!(
        response.body["error"]["message"], "Cannot share with yourself",
        "body: {}",
        response.body
    );
}

/// SEC04 missing resource: a non-existent object must be a 404, never a 200
/// with another tenant's row and never a 500.
#[tokio::test]
async fn missing_resources_return_404() {
    init_test_logging();
    let (app, _dir) = w1_app().await;

    let candidate = get(app.clone(), "/api/candidates/does-not-exist").await;
    assert_eq!(candidate.status, 404, "body: {}", candidate.body);

    let position = get(app.clone(), "/api/positions/does-not-exist").await;
    assert_eq!(position.status, 404, "body: {}", position.body);

    let share = send_delete(app, "/api/shares/does-not-exist").await;
    assert_eq!(share.status, 404, "body: {}", share.body);
}

/// SEC04 invalid token: the auth endpoints that consume a credential must
/// reject an unparseable one with 401 `unauthorized`.
#[tokio::test]
async fn an_invalid_token_is_rejected_with_401() {
    init_test_logging();
    let (app, _dir) = w1_app().await;

    let refresh = post(
        app.clone(),
        "/api/auth/refresh",
        r#"{"refresh_token":"not-a-token"}"#,
    )
    .await;
    assert_eq!(refresh.status, 401, "body: {}", refresh.body);
    assert_eq!(refresh.body["error"]["code"], "unauthorized");
    assert_eq!(refresh.body["error"]["message"], "Invalid refresh token");

    let logout = post(
        app,
        "/api/auth/logout",
        r#"{"refresh_token":"not-a-token"}"#,
    )
    .await;
    // Logout is idempotent by contract (auth.rs:284-295): an unknown token is
    // still a 204, NOT a 401. This documents the deliberate exception to the
    // "invalid token -> 401" rule.
    assert_eq!(logout.status, 204, "body: {:?}", logout.body);
}

/// SEC04 Local Mode boundary, characterized honestly.
///
/// No middleware gates business routes in this build (routes.rs:226-227 and
/// the only two `.layer(...)` calls at 243/246 are HTTP parity middlewares).
/// A forged bearer token is therefore ignored and the request succeeds. This
/// is the accepted single-user loopback boundary; it is a hard blocker for
/// Multi-user Mode or any non-loopback exposure, and must NOT be reported as
/// "authentication implemented".
#[tokio::test]
async fn local_mode_does_not_gate_business_routes_on_a_bearer_token() {
    init_test_logging();
    let (app, _dir) = w1_app().await;

    let ordinary = get(app.clone(), "/api/positions").await;
    let forged = get_with_header(
        app,
        "/api/positions",
        "authorization",
        "Bearer forged-token",
    )
    .await;
    assert_eq!(ordinary.status, 200);
    assert_eq!(forged.status, 200);
    assert_eq!(
        forged.body, ordinary.body,
        "the forged bearer token changes nothing: there is no authentication gate"
    );
}

/// SEC04 AC3 source assertion: HTTP and IPC adapters call the SAME
/// `ica_application` use cases for the auth/share surface. Neither transport
/// owns its own authorization rules.
#[test]
fn http_and_ipc_share_and_auth_paths_use_the_same_use_cases() {
    const HTTP: &str = include_str!("../src/routes.rs");
    const IPC: &str = include_str!("../../ica-desktop/src/commands.rs");

    for call in [
        "ica_application::create_share(",
        "ica_application::list_shares(",
        "ica_application::delete_share(",
        "ica_application::register(",
        "ica_application::login(",
        "ica_application::refresh(",
        "ica_application::logout(",
        "ica_application::local_user(",
    ] {
        assert!(HTTP.contains(call), "HTTP adapter must call {call}");
        assert!(IPC.contains(call), "IPC adapter must call {call}");
    }

    // The actor passed to every share use case comes from the trusted local
    // identity, never from a request parameter, on BOTH transports.
    assert_eq!(
        HTTP.matches("&local.id").count(),
        3,
        "HTTP: create/list/delete share pass the trusted actor"
    );
    assert_eq!(
        IPC.matches("&local.id").count(),
        3,
        "IPC: create/list/delete share pass the trusted actor"
    );
}

async fn get_with_header(app: axum::Router, uri: &str, name: &str, value: &str) -> TestResponse {
    let request = axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .header("accept", "application/json")
        .header(name, value)
        .body(axum::body::Body::empty())
        .expect("build request");
    send(app, request).await
}

async fn send_delete(app: axum::Router, uri: &str) -> TestResponse {
    let request = axum::http::Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("accept", "application/json")
        .body(axum::body::Body::empty())
        .expect("build request");
    send(app, request).await
}

async fn send(app: axum::Router, request: axum::http::Request<axum::body::Body>) -> TestResponse {
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
    TestResponse { status, body }
}
