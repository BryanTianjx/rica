//! Backend routing decision, moved out of the Tauri host (ARCH-P3 AC1).
//!
//! It is a pure function over `(method, path)` with no Tauri types, so the P1
//! dual-backend disposition (browser/v1 + AI-EXT model CRUD to Python, the
//! 68-route migration contract to Rust) is testable on a Linux host that
//! cannot compile the shell. The host keeps only the thin `use` + call.

/// Which backend a `/api` request belongs to (P1 disposition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The Rust `AppRuntime` (HTTP adapter when enabled, IPC otherwise).
    Rust,
    /// The Python sidecar: browser/v1 (B-series) and AI-EXT model CRUD.
    Python,
}

/// The P1 routing rule as a pure function. `path` is the `/api`-prefixed
/// request path WITHOUT the query string.
pub fn route_backend(method: &str, path: &str) -> Backend {
    if path.starts_with("api/browser/v1") {
        return Backend::Python;
    }
    let method = method.to_ascii_uppercase();
    if path == "api/ai/models" && method == "POST" {
        return Backend::Python;
    }
    if path.starts_with("api/ai/models/") && matches!(method.as_str(), "PUT" | "DELETE") {
        return Backend::Python;
    }
    Backend::Rust
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_v1_routes_to_python() {
        for (method, path) in [
            ("GET", "api/browser/v1/runtime/status"),
            ("POST", "api/browser/v1/runtime/start"),
            ("POST", "api/browser/v1/sessions"),
            ("DELETE", "api/browser/v1/sessions/abc"),
            ("POST", "api/browser/v1/sessions/abc/actions"),
            ("GET", "api/browser/v1/candidate-extractions"),
            ("POST", "api/browser/v1/permissions"),
        ] {
            assert_eq!(route_backend(method, path), Backend::Python, "{path}");
        }
        assert_eq!(route_backend("GET", "api/browser/v1"), Backend::Python);
    }

    #[test]
    fn ai_ext_model_crud_routes_to_python() {
        assert_eq!(route_backend("POST", "api/ai/models"), Backend::Python);
        assert_eq!(
            route_backend("PUT", "api/ai/models/glm-4-flash"),
            Backend::Python
        );
        assert_eq!(
            route_backend("DELETE", "api/ai/models/glm-4-flash"),
            Backend::Python
        );
        assert_eq!(
            route_backend("PUT", "api/ai/models/glm-4-flash/default"),
            Backend::Python
        );
    }

    #[test]
    fn implemented_w5_routes_stay_on_rust() {
        for (method, path) in [
            ("GET", "api/ai/models"),
            ("GET", "api/ai/routing"),
            ("PATCH", "api/ai/routing"),
            ("GET", "api/ai/usage"),
            ("PATCH", "api/ai/budget"),
            ("POST", "api/imports/abc/confirm-candidate"),
        ] {
            assert_eq!(route_backend(method, path), Backend::Rust, "{path}");
        }
    }

    #[test]
    fn the_68_route_contract_stays_on_rust() {
        for (method, path) in [
            ("GET", "api/positions"),
            ("POST", "api/positions"),
            ("DELETE", "api/positions/p-1"),
            ("GET", "api/candidates"),
            ("GET", "api/candidates/c-1/timeline"),
            ("GET", "api/candidates/c-1/context"),
            ("POST", "api/conversations"),
            ("POST", "api/messages"),
            ("POST", "api/follow-ups/evaluate"),
            ("POST", "api/follow-ups/t-1/record-result"),
            ("GET", "api/dashboard"),
            ("GET", "api/drafts"),
            ("POST", "api/drafts/generate"),
            ("GET", "api/draft-groups/g-1"),
            ("POST", "api/imports/preview"),
            ("POST", "api/imports/ocr"),
            ("POST", "api/export"),
            ("POST", "api/import"),
            ("GET", "api/settings"),
            ("PUT", "api/settings/ai-key"),
            ("DELETE", "api/settings/ai-key"),
            ("GET", "api/settings/ai-egress"),
            ("POST", "api/settings/ai-egress/consent"),
            ("GET", "api/audit-events"),
            ("GET", "api/backups"),
            ("POST", "api/backups/n.db/restore"),
            ("POST", "api/auth/register"),
            ("POST", "api/auth/login"),
            ("POST", "api/auth/refresh"),
            ("POST", "api/auth/logout"),
            ("GET", "api/auth/me"),
            ("POST", "api/shares"),
            ("GET", "api/shares"),
            ("DELETE", "api/shares/s-1"),
        ] {
            assert_eq!(route_backend(method, path), Backend::Rust, "{path}");
        }
        assert_eq!(route_backend("GET", "api/browser"), Backend::Rust);
        assert_eq!(route_backend("PUT", "api/ai/models-default"), Backend::Rust);
    }
}
