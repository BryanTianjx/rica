//! HTTP tests for the router-level parity behaviours (R0 audit F-02..F-05):
//! repeated query keys, the Python integer grammar, boolean whitespace, the
//! 405 envelope for unserved methods and `HEAD`, and trailing-slash redirects.
//!
//! The two-sided versions of these cases live in `scripts/contract_capture.py`
//! (`positions_duplicate_limit`, `positions_trailing_slash`, …); these tests
//! make `cargo test` alone catch a regression, without the Python baseline.

mod support;

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, LOCATION};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

/// One request against the router, keeping status, headers and decoded body.
struct RawResponse {
    status: u16,
    content_type: String,
    location: Option<String>,
    body: serde_json::Value,
}

async fn request(app: axum::Router, method: &str, uri: &str, body: Option<&str>) -> RawResponse {
    let mut builder = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("accept", "application/json")
        .header("host", "127.0.0.1:8055");
    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    let request = builder
        .body(Body::from(body.map(str::to_string).unwrap_or_default()))
        .expect("build request");
    let response = app.oneshot(request).await.expect("router responded");
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .map(|value| value.to_str().expect("ascii header").to_string())
        .unwrap_or_default();
    let location = response
        .headers()
        .get(LOCATION)
        .map(|value| value.to_str().expect("ascii header").to_string());
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("body is JSON")
    };
    RawResponse {
        status,
        content_type,
        location,
        body,
    }
}

/// The seed has four positions, one of them archived.
async fn seeded_app() -> axum::Router {
    let (app, _guard) = support::seeded_app().await;
    // The guard lives as long as the router is used; leaking it is fine in a
    // test process and keeps the helper's signature simple.
    std::mem::forget(_guard);
    app
}

/// F-02: a repeated key must not break the envelope — Starlette keeps the
/// last occurrence, so `limit=1&limit=2` means `limit=2`.
#[tokio::test]
async fn a_repeated_query_key_keeps_the_last_value() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/positions?limit=1&limit=2", None).await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["limit"], 2);
    assert_eq!(response.body["total"], 3);
}

/// F-03: an offset past `i64::MAX` is still an integer to pydantic; the page
/// comes back empty with the huge offset echoed, not 422 and not page one.
#[tokio::test]
async fn an_offset_past_i64_returns_the_empty_page_with_the_offset_echoed() {
    let app = seeded_app().await;

    let response = request(
        app,
        "GET",
        "/api/positions?offset=9223372036854775808",
        None,
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["offset"], 9223372036854775808u64);
    assert_eq!(response.body["total"], 3);
    assert_eq!(response.body["items"], json!([]));
    assert_eq!(response.body["has_more"], false);
}

/// IMP-4: past `u64::MAX` the offset cannot be echoed, so the port answers the
/// validation envelope instead of the baseline's empty page.
#[tokio::test]
async fn an_offset_beyond_u64_is_a_validation_envelope() {
    let app = seeded_app().await;

    let response = request(
        app,
        "GET",
        "/api/positions?offset=18446744073709551616",
        None,
    )
    .await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
    assert_eq!(response.content_type, "application/json");
}

/// F-04: pydantic strips whitespace for ints but not for bools.
#[tokio::test]
async fn a_padded_boolean_is_rejected_with_bool_parsing() {
    let app = seeded_app().await;

    let response = request(
        app,
        "GET",
        "/api/positions?include_archived=%20true%20",
        None,
    )
    .await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
    assert_eq!(response.content_type, "application/json");
}

/// F-05: any method a route does not serve answers the JSON envelope, never
/// axum's empty 405.
#[tokio::test]
async fn an_unserved_method_answers_the_405_envelope() {
    let app = seeded_app().await;

    for method in ["PUT", "OPTIONS", "DELETE"] {
        let response = request(app.clone(), method, "/api/positions", Some("{}")).await;
        assert_eq!(response.status, 405, "{method}");
        assert_eq!(response.body["error"]["code"], "http_error", "{method}");
        assert_eq!(
            response.body["error"]["message"], "Method Not Allowed",
            "{method}"
        );
    }
}

/// F-05: no endpoint declares `HEAD`, so it is a 405 like any other unserved
/// method. The envelope headers survive (uvicorn behaves the same); the body
/// is stripped for HEAD — by axum's `Route` here and by hyper in transport.
#[tokio::test]
async fn head_on_a_served_route_answers_the_405_envelope() {
    let app = seeded_app().await;

    let response = request(app, "HEAD", "/api/positions", None).await;

    assert_eq!(response.status, 405);
    assert_eq!(response.content_type, "application/json");
    assert_eq!(response.body, serde_json::Value::Null);
}

/// F-05: a `HEAD` on an unknown path keeps its 404 envelope, again with the
/// body stripped for the method.
#[tokio::test]
async fn head_on_an_unknown_path_answers_404() {
    let app = seeded_app().await;

    let response = request(app, "HEAD", "/api/no-such-route", None).await;

    assert_eq!(response.status, 404);
    assert_eq!(response.content_type, "application/json");
    assert_eq!(response.body, serde_json::Value::Null);
}

/// F-05: trailing slashes redirect (307) with method, path and query
/// preserved, like Starlette's `redirect_slashes` — never a hard 404.
#[tokio::test]
async fn a_trailing_slash_redirects_with_the_query_preserved() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/positions/?limit=2&search=abc", None).await;

    assert_eq!(response.status, 307);
    assert_eq!(
        response.location.as_deref(),
        Some("http://127.0.0.1:8055/api/positions?limit=2&search=abc")
    );
}

/// A bare trailing slash redirects without a query part in the `Location`.
#[tokio::test]
async fn a_bare_trailing_slash_redirects() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/positions/", None).await;

    assert_eq!(response.status, 307);
    assert_eq!(
        response.location.as_deref(),
        Some("http://127.0.0.1:8055/api/positions")
    );
}

/// Card W1.5 A6, row 4: Starlette `rstrip("/")` strips ALL trailing slashes,
/// so `/api/positions//` redirects to the bare path, not to a 404.
#[tokio::test]
async fn multiple_trailing_slashes_redirect_to_the_stripped_path() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/positions//", None).await;

    assert_eq!(response.status, 307);
    assert_eq!(
        response.location.as_deref(),
        Some("http://127.0.0.1:8055/api/positions")
    );
}

/// Card W1.5 A6, row 7 — the middleware-order guard: the baseline redirects
/// `HEAD /api/positions/` (307) because `redirect_slashes` matches paths
/// *before* considering the method. If the 405 guard ever moves outside the
/// redirect, this turns into a 405 and diverges from the baseline.
#[tokio::test]
async fn a_head_with_a_trailing_slash_redirects() {
    let app = seeded_app().await;

    let response = request(app, "HEAD", "/api/positions/", None).await;

    assert_eq!(response.status, 307);
    assert_eq!(
        response.location.as_deref(),
        Some("http://127.0.0.1:8055/api/positions")
    );
}

/// Card W1.5 A6, row 6: an unserved method with a trailing slash also
/// redirects (307 preserves the method for the client's retry), it is not a
/// 405 — same path-before-method ordering as above.
#[tokio::test]
async fn an_unserved_method_with_a_trailing_slash_redirects() {
    let app = seeded_app().await;

    let response = request(app, "DELETE", "/api/positions/", None).await;

    assert_eq!(response.status, 307);
    assert_eq!(
        response.location.as_deref(),
        Some("http://127.0.0.1:8055/api/positions")
    );
}

/// Card W1.5 A6, row 9: stripping `/api/` leaves an empty path, and an empty
/// path matches nothing — the baseline answers 404 here, not a redirect.
#[tokio::test]
async fn a_trailing_slash_on_the_bare_api_prefix_answers_404() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/", None).await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "http_error");
}

/// The pre-existing §7 declared delta: the baseline serves `GET /`, the port
/// does not implement it (`GET /` is not part of the API surface). This test
/// pins that declaration so a future root route cannot appear silently.
#[tokio::test]
async fn the_root_path_stays_unimplemented_as_declared() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/", None).await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "http_error");
}

/// The redirect also covers parameterised routes: the stripped path must match
/// a route pattern, which `/api/positions/{id}` does.
#[tokio::test]
async fn a_trailing_slash_on_a_parameterised_route_redirects() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/positions/p-1/", None).await;

    assert_eq!(response.status, 307);
    assert_eq!(
        response.location.as_deref(),
        Some("http://127.0.0.1:8055/api/positions/p-1")
    );
}

/// A trailing slash on a path matching nothing keeps the 404 envelope — the
/// redirect only fires when the stripped path actually matches a route.
#[tokio::test]
async fn a_trailing_slash_on_an_unknown_path_answers_404() {
    let app = seeded_app().await;

    let response = request(app, "GET", "/api/no-such-route/", None).await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "http_error");
}
