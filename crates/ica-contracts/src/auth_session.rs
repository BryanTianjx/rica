//! SEC-03 — the transport-neutral view of the authentication state machine.
//!
//! The policy itself is pure application logic (`ica-application::auth_recovery`,
//! re-exported here so both HTTP and IPC adapters see one definition). This
//! module adds the cross-layer guarantees that a machine can actually check in
//! this environment: the six phase names are mirrored byte-for-byte in the
//! webview reducer, the recovery call is centralised in `AuthContext`, and the
//! Tauri host stays a thin wiring layer over the persisted JWT key (P1-01).

pub use ica_application::auth_recovery::{
    transition as auth_transition, AuthEvent, AuthPhase, AuthSessionState, SessionEndReason,
    ALL_AUTH_PHASES,
};

/// The stable wire spelling of the six phases (RICA-SEC-001 §5.2).
pub const AUTH_PHASE_NAMES: [&str; 6] = [
    "Unauthenticated",
    "Authenticating",
    "Authenticated",
    "Refreshing",
    "SessionExpired",
    "AuthenticationError",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(relative)
    }

    fn read(relative: &str) -> String {
        std::fs::read_to_string(repo_path(relative))
            .unwrap_or_else(|error| panic!("missing {relative}: {error}"))
    }

    /// Compile-time pin: the webview file must exist and its phase union must
    /// declare every canonical name as a quoted string literal.
    const FRONTEND_STATE: &str = include_str!("../../../frontend/src/auth/auth-state.ts");

    #[test]
    fn sec03_c01_frontend_mirrors_all_six_phases() {
        assert_eq!(AUTH_PHASE_NAMES.len(), 6);
        assert_eq!(ALL_AUTH_PHASES.len(), 6);
        assert!(
            FRONTEND_STATE.contains("export const AUTH_PHASES"),
            "frontend must publish the same frozen phase list"
        );
        for (phase, name) in ALL_AUTH_PHASES.iter().zip(AUTH_PHASE_NAMES.iter()) {
            assert_eq!(phase.as_str(), *name);
            assert!(
                FRONTEND_STATE.contains(&format!("'{name}'"))
                    || FRONTEND_STATE.contains(&format!("\"{name}\"")),
                "frontend auth-state.ts is missing phase {name}"
            );
        }
    }

    fn scan_refresh_callers(dir: &std::path::Path, scanned: &mut usize) {
        for entry in std::fs::read_dir(dir).expect("read frontend/src") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                scan_refresh_callers(&path, scanned);
                continue;
            }
            let is_ts = path
                .extension()
                .map(|ext| ext == "ts" || ext == "tsx")
                .unwrap_or(false);
            if !is_ts {
                continue;
            }
            *scanned += 1;
            let is_test = path.to_string_lossy().contains(".test.");
            let is_refresh_owner =
                path.ends_with("services/api.ts") || path.ends_with("services/endpoints.ts");
            if !is_test && !is_refresh_owner {
                let text = std::fs::read_to_string(&path).expect("read source");
                assert!(
                    !text.contains("/auth/refresh"),
                    "{} calls /auth/refresh directly; route it through api.ts",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn sec03_c02_recovery_is_centralised_in_auth_context() {
        let context = read("frontend/src/auth/AuthContext.tsx");
        assert!(
            context.contains("reduceAuth"),
            "AuthContext must own the state-machine reducer"
        );
        assert!(
            context.contains("refreshSession"),
            "AuthContext must drive the one deduplicated refresh path"
        );
        assert!(
            context.contains("PROBE_UNAUTHORIZED") && context.contains("REFRESH_SUCCEEDED"),
            "AuthContext must implement /auth/me 401 -> refresh -> retry"
        );

        // No page/component may call the refresh endpoint directly: doing so
        // would bypass single-flight and start a rotation race.
        let src = repo_path("frontend/src");
        let mut scanned = 0;
        scan_refresh_callers(&src, &mut scanned);
        assert!(scanned > 20, "frontend source scan must cover every module");
    }

    #[test]
    fn sec03_c03_tauri_host_is_thin_wiring_over_the_persisted_key() {
        let host = read("desktop/tauri-host/src/main.rs");
        assert!(
            host.contains("JwtKeyProvider"),
            "host must delegate key management to the provider"
        );
        assert!(
            host.contains("load_or_create"),
            "restart must reuse the persisted CSPRNG key (SEC-01)"
        );
        assert!(
            !host.contains("SessionExpired") && !host.contains("refreshSession"),
            "host must not own authentication state or refresh logic"
        );
        let start = host
            .find("fn generate_jwt_secret()")
            .expect("host key generator exists");
        let body = &host[start..];
        let body = &body[..body.find("\n}").map(|i| i + 2).unwrap_or(body.len())];
        assert!(!body.contains("SystemTime"));
        assert!(!body.contains("process::id"));
    }

    #[test]
    fn sec03_c04_lifecycle_tests_cover_the_plan_matrix() {
        let state_tests = read("frontend/src/auth/auth-state.test.ts");
        for tag in [
            "SEC03-T01",
            "SEC03-T03",
            "SEC03-T07",
            "SEC03-T09",
            "SEC03-T11",
        ] {
            assert!(
                state_tests.contains(tag),
                "lifecycle matrix is missing {tag}"
            );
        }
        let mut context_tests = read("frontend/src/auth/AuthContext.test.tsx");
        context_tests.push_str(&read("frontend/src/services/api.refresh.test.ts"));
        assert!(
            context_tests.contains("restart"),
            "need a restart lifecycle test"
        );
        assert!(
            context_tests.contains("concurrent"),
            "need a concurrent-refresh lifecycle test"
        );
    }
}
