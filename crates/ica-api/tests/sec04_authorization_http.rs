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
/// `ica_application` use cases for the auth/share surface, and neither
/// transport decides authorization itself. ARCH-P2 moved the trusted actor
/// into a `RequestContext` and the permission checks into the use cases.
#[test]
fn http_and_ipc_share_and_auth_paths_use_the_same_use_cases() {
    const HTTP: &str = include_str!("../src/routes.rs");
    const IPC: &str = include_str!("../../ica-desktop/src/commands.rs");
    const AUTH: &str = include_str!("../../ica-application/src/auth.rs");

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

    // SEC-04-B: HTTP now receives the verified identity from the auth
    // middleware (a real JWT when present, the synthetic local actor in Local
    // mode); IPC still constructs the trusted local actor in its own adapter.
    assert_eq!(HTTP.matches("RequestContext::local()").count(), 0);
    assert_eq!(IPC.matches("RequestContext::local()").count(), 3);
    assert!(HTTP.contains("Extension<ica_application::RequestContext>"));
    assert!(HTTP.contains("auth::authenticate"));
    for source in [HTTP, IPC] {
        assert!(!source.contains("is_admin"));
        assert!(!source.contains("&local.id"));
    }

    // Authorization itself is executed in the Application layer.
    assert!(AUTH.contains("authorize(ctx, Permission::SharesCreate)?"));
    assert!(AUTH.contains("authorize(ctx, Permission::SharesRead)?"));
    assert!(AUTH.contains("authorize(ctx, Permission::SharesDelete)?"));
    assert!(AUTH.contains("ctx.can_administer(&share.owner_id)"));
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

// ── SEC-04-A AC3: the matrix's public whitelist is executable evidence ────

const SEC04_A_MATRIX: &str =
    include_str!("../../../docs/architecture/api-authentication-matrix.md");
const SEC04_A_ROUTES: &str = include_str!("../src/routes.rs");
const SEC04_A_SERVER_MAIN: &str = include_str!("../../../crates/ica-server/src/main.rs");

/// Parse the machine-readable whitelist fenced by the SEC04-A markers in the
/// authentication matrix. The markers are the single source of truth shared
/// between the document and this assertion.
fn documented_public_endpoints() -> Vec<(String, String)> {
    let after = SEC04_A_MATRIX
        .split("SEC04-A-PUBLIC-ENDPOINTS-BEGIN")
        .nth(1)
        .expect("matrix must carry the SEC04-A public-endpoint marker");
    let block = after
        .split("SEC04-A-PUBLIC-ENDPOINTS-END")
        .next()
        .expect("matrix must close the SEC04-A public-endpoint marker");
    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("<!--"))
        .map(|line| {
            let (method, path) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("malformed whitelist entry: {line}"));
            (method.to_string(), path.to_string())
        })
        .collect()
}

/// The public endpoints in the matrix must be exactly the liveness probe and
/// the two credential-minting auth endpoints, each really registered in
/// `routes.rs` with the matching method and with NO authentication layer.
/// Every other contract is `Local 放行 / 未来需认证`: a Local Mode boundary,
/// never a claim that authentication is implemented.
#[test]
fn sec04_a_public_whitelist_matches_unauthenticated_routes() {
    let mut documented = documented_public_endpoints();
    documented.sort();
    let mut expected = vec![
        ("GET".to_string(), "/health".to_string()),
        ("POST".to_string(), "/api/auth/register".to_string()),
        ("POST".to_string(), "/api/auth/login".to_string()),
    ];
    expected.sort();
    assert_eq!(documented, expected, "matrix whitelist drifted");

    for (method, path) in &expected {
        let registration = format!(".route(\"{path}\", {}(", method.to_lowercase());
        assert!(
            SEC04_A_ROUTES.contains(&registration),
            "whitelisted endpoint {method} {path} is not registered as {registration}"
        );
    }

    // The only two layers in the router are HTTP parity middlewares. If an
    // authentication layer is added by SEC-04-B/C, this test intentionally
    // fails until the matrix and this whitelist are updated with it.
    assert_eq!(
        SEC04_A_ROUTES
            .matches(".layer(middleware::from_fn(")
            .count(),
        2,
        "routes.rs has an unexpected middleware layer"
    );
    assert!(SEC04_A_ROUTES.contains("http_parity::reject_head"));
    assert!(SEC04_A_ROUTES.contains("http_parity::redirect_trailing_slash"));
    assert!(
        !SEC04_A_ROUTES.contains("auth_layer")
            && !SEC04_A_ROUTES.contains("require_auth")
            && !SEC04_A_ROUTES.contains("jwt_middleware"),
        "this build must not claim an HTTP authentication gate"
    );

    // The document must state the release rule, and the startup code must
    // still be honest about the missing multi-user authentication.
    assert!(SEC04_A_MATRIX.contains("SEC-04 完成前禁止远程暴露"));
    assert!(SEC04_A_MATRIX.contains("docs/architecture/remote-exposure-gate.md"));
    assert!(SEC04_A_SERVER_MAIN.contains("NETWORK_AUTHENTICATION_READY: bool = false"));
    assert!(SEC04_A_SERVER_MAIN.contains("多用户认证尚未实现，不能启用远程模式"));
    assert!(SEC04_A_SERVER_MAIN.contains("ICA_AUTH_REQUIRED"));
}

/// The matrix's new policy column is derived from the whitelist above: the two
/// anonymous contract rows are public, the other 66 are Local-only for now.
#[test]
fn sec04_a_matrix_policy_column_matches_whitelist() {
    let public: Vec<String> = documented_public_endpoints()
        .into_iter()
        .map(|(_method, path)| path)
        .filter(|path| path != "/health")
        .collect();
    let mut ids = Vec::new();
    for line in SEC04_A_MATRIX.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 7 || cells[0].parse::<u32>().is_err() {
            continue;
        }
        ids.push(cells[0].parse::<u32>().unwrap());
        let path = cells[2].trim_matches('`');
        let full_path = format!("/api{path}");
        let policy = cells[6];
        if public.iter().any(|public_path| public_path == &full_path) {
            assert_eq!(
                policy, "公开端点白名单",
                "contract {} ({path}) must be public",
                cells[0]
            );
        } else {
            assert_eq!(
                policy, "Local 放行 / 未来需认证",
                "contract {} ({path}) must not be public",
                cells[0]
            );
        }
    }
    assert_eq!(ids.len(), 68, "matrix must have exactly 68 contract rows");
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(*id, index as u32 + 1, "contract ids must be 1..68");
    }
    assert_eq!(public.len(), 2, "only register/login are public contracts");
}
