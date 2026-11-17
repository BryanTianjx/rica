//! Router-level behaviours where axum's defaults differ from the Python
//! baseline's router (Starlette), measured on the baseline in the R0 audit
//! (finding F-05): a `HEAD` on a served route must answer the 405 envelope
//! instead of a body-stripped 200, and a path with trailing slashes must be
//! redirected instead of 404ing.
//!
//! Both middlewares match requests against the route patterns declared in
//! `routes.rs`, mirroring what Starlette's `Router.app` does: it redirects a
//! path whose stripped form matches any route *regardless of the method*
//! (`Match.PARTIAL` counts), and it 405s a `HEAD` on any served route because
//! FastAPI never declares `HEAD`.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{HOST, LOCATION};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;

/// Route patterns the parity middlewares test against.
pub type RoutePatterns = Arc<[&'static str]>;

/// `HEAD` on a served route answers the 405 envelope, like the baseline: no
/// route declares `HEAD`, so Starlette treats the request as a method mismatch
/// while axum would serve the `GET` handler with a stripped body. Unknown paths
/// fall through to the router and keep their 404 envelope.
pub async fn reject_head(patterns: RoutePatterns, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if request.method() == Method::HEAD
        && patterns
            .iter()
            .any(|pattern| matches_pattern(pattern, path))
    {
        return method_not_allowed();
    }
    next.run(request).await
}

/// A path with trailing slash(es) is redirected — method and query preserved —
/// to the stripped path when that path matches a route, exactly like
/// Starlette's `redirect_slashes`: `rstrip("/")`, a 307 with an absolute
/// `Location`. Starlette's append branch (a path that would only match with a
/// slash *added*) is never reachable here, because no route ends in a slash.
pub async fn redirect_trailing_slash(
    patterns: RoutePatterns,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let stripped = path.trim_end_matches('/');
    if stripped.len() == path.len() || stripped.is_empty() {
        // No trailing slash to strip, or the path is just "/": the baseline
        // answers 404 for both (its `route_path != "/"` guard).
        return next.run(request).await;
    }
    if !patterns
        .iter()
        .any(|pattern| matches_pattern(pattern, stripped))
    {
        return next.run(request).await;
    }
    let Some(location) = absolute_location(&request, stripped) else {
        // No usable host in the request: the baseline would still redirect, but
        // an absolute Location cannot be built, so serve the stripped path
        // directly instead of emitting a relative one it never emits.
        return next.run(request).await;
    };
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
    response.headers_mut().insert(LOCATION, location);
    response
}

/// The 405 envelope the baseline emits for any method a route does not serve
/// (`str(HTTPException(405).detail)`): `{"error":{"code":"http_error",
/// "message":"Method Not Allowed","details":{}}}`. For `HEAD` requests hyper
/// suppresses the body but keeps the headers, which is exactly what uvicorn
/// does — and the baseline's handler drops Starlette's `Allow` header, so the
/// envelope is sent bare as well.
pub fn method_not_allowed() -> Response {
    ApiError::Http {
        status: 405,
        message: "Method Not Allowed".to_string(),
    }
    .into_response()
}

/// Build the absolute `Location` value the way Starlette's `RedirectResponse`
/// renders `URL(scope)`: scheme, host, stripped path, query. The host comes
/// from the `Host` header (the same value clients see echoed back) and falls
/// back to the request's authority, so `None` only happens for a client that
/// sent neither.
fn absolute_location(request: &Request, stripped_path: &str) -> Option<HeaderValue> {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            request
                .uri()
                .authority()
                .map(|authority| authority.as_str().to_owned())
        })?;
    let scheme = request.uri().scheme_str().unwrap_or("http");
    let mut location = format!("{scheme}://{host}{stripped_path}");
    if let Some(query) = request.uri().query() {
        location.push('?');
        location.push_str(query);
    }
    // URI-derived strings are header-safe by construction (RFC 3986); the
    // fallback keeps the impossible case panic-free without `.unwrap()`.
    Some(HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")))
}

/// Whether `path` matches a route pattern: static segments must be equal, a
/// `{param}` segment matches one non-empty segment. Mirrors the path syntax of
/// the patterns in `routes.rs` (`{position_id}`, axum 0.8).
fn matches_pattern(pattern: &str, path: &str) -> bool {
    let mut pattern_segments = pattern.split('/');
    for segment in path.split('/') {
        match pattern_segments.next() {
            Some(param) if param.starts_with('{') && param.ends_with('}') => {
                if segment.is_empty() {
                    return false;
                }
            }
            Some(static_segment) => {
                if static_segment != segment {
                    return false;
                }
            }
            None => return false,
        }
    }
    pattern_segments.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns() -> RoutePatterns {
        Arc::from(&["/health", "/api/positions", "/api/positions/{position_id}"][..])
    }

    #[test]
    fn patterns_match_static_and_parametrised_paths() {
        let patterns = patterns();
        assert!(patterns.iter().any(|p| matches_pattern(p, "/health")));
        assert!(patterns
            .iter()
            .any(|p| matches_pattern(p, "/api/positions")));
        assert!(patterns
            .iter()
            .any(|p| matches_pattern(p, "/api/positions/abc-123")));
        // Empty segments never match a parameter, and extra segments never match.
        assert!(!patterns
            .iter()
            .any(|p| matches_pattern(p, "/api/positions/")));
        assert!(!patterns
            .iter()
            .any(|p| matches_pattern(p, "/api/positions/a/b")));
        assert!(!patterns.iter().any(|p| matches_pattern(p, "/api")));
        assert!(!patterns.iter().any(|p| matches_pattern(p, "/unknown")));
    }
}
