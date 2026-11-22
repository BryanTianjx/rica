//! SEC-03 — authentication state machine + desktop session recovery policy.
//!
//! The audit (§5.3 / RICA-SEC-001 §5) requires one canonical state machine
//! shared by every UI surface instead of each component re-implementing
//! "what happens on a 401". This module is that canonical, *pure* definition:
//! no I/O, no runtime, no adapter dependency (the crate manifest forbids
//! axum/sqlx/tokio/hyper). `ica-contracts` re-exports it to adapters, and the
//! webview mirrors the same six phases in `frontend/src/auth/auth-state.ts`;
//! a source-level test in `ica-contracts` pins the two name lists together.
//!
//! The recovery chain fixed here is:
//!
//! ```text
//! probe /auth/me → 401 → (one) refresh → retry /auth/me → authenticated
//!                              └─ failure / second 401 → SessionExpired
//! ```
//!
//! `retried` is the loop guard: after one refresh+retry cycle a second 401 can
//! no longer schedule another refresh. Refresh *deduplication* (single-flight)
//! lives in the API client, where the in-flight request actually exists; this
//! policy is the state-level half of the same guarantee.

/// The six phases mandated by the remediation plan §5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPhase {
    Unauthenticated,
    Authenticating,
    Authenticated,
    Refreshing,
    SessionExpired,
    AuthenticationError,
}

impl AuthPhase {
    /// Wire/UI spelling. Keep in lockstep with `AUTH_PHASES` in
    /// `ica-contracts::auth_session` and the frontend union type.
    pub const fn as_str(self) -> &'static str {
        match self {
            AuthPhase::Unauthenticated => "Unauthenticated",
            AuthPhase::Authenticating => "Authenticating",
            AuthPhase::Authenticated => "Authenticated",
            AuthPhase::Refreshing => "Refreshing",
            AuthPhase::SessionExpired => "SessionExpired",
            AuthPhase::AuthenticationError => "AuthenticationError",
        }
    }
}

/// Why a session ended. The value is surfaced to the UI, never used as a
/// secret or as an authorization decision by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEndReason {
    /// The refresh HTTP call failed (network / 5xx) — SEC03-T08.
    RefreshFailed,
    /// The refresh endpoint rejected the token (expired, revoked, replayed).
    RefreshRejected,
    /// Access token rejected with no usable refresh token left — SEC03-T05.
    Expired,
    /// The refresh token was revoked by logout or by the server — SEC03-T06.
    Revoked,
    /// A still-invalid access token after the single retry — SEC03-T11.
    InvalidToken,
}

impl SessionEndReason {
    pub const fn as_str(&self) -> &'static str {
        match self {
            SessionEndReason::RefreshFailed => "refresh_failed",
            SessionEndReason::RefreshRejected => "refresh_rejected",
            SessionEndReason::Expired => "expired",
            SessionEndReason::Revoked => "revoked",
            SessionEndReason::InvalidToken => "invalid_token",
        }
    }
}

/// The whole authentication state, deliberately small enough to live in one
/// reducer. `user` is carried by the UI layer (it is a transport DTO); the
/// policy only owns the phase, the retry guard and diagnostic metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSessionState {
    pub phase: AuthPhase,
    /// `true` once a refresh+retry cycle has been consumed for this attempt.
    pub retried: bool,
    pub reason: Option<SessionEndReason>,
    pub message: Option<String>,
}

impl AuthSessionState {
    pub const fn new(phase: AuthPhase) -> Self {
        Self {
            phase,
            retried: false,
            reason: None,
            message: None,
        }
    }

    fn ended(reason: SessionEndReason) -> Self {
        Self {
            phase: AuthPhase::SessionExpired,
            retried: false,
            reason: Some(reason),
            message: None,
        }
    }

    fn errored(message: &str) -> Self {
        Self {
            phase: AuthPhase::AuthenticationError,
            retried: false,
            reason: None,
            message: Some(message.to_string()),
        }
    }
}

/// Events fed into the reducer by the auth provider and the API client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthEvent {
    /// App start (a persisted token pair may exist).
    RestoreStarted,
    /// Login form submitted.
    LoginStarted,
    /// `/auth/me` (or the login probe) answered 200.
    ProbeSucceeded,
    /// `/auth/me` answered 401.
    ProbeUnauthorized,
    /// Non-401 failure while probing.
    ProbeFailed(String),
    /// The (deduplicated) refresh returned a new pair.
    RefreshSucceeded,
    /// The refresh failed for the given reason.
    RefreshFailed(SessionEndReason),
    /// The API client reported a dead session out-of-band (F2-1).
    SessionExpired(SessionEndReason),
    /// Credentials accepted.
    LoginSucceeded,
    /// Credentials rejected.
    LoginFailed(String),
    /// Intentional sign-out.
    SignOut,
    /// The user acknowledged an authentication error and may retry.
    DismissError,
}

/// The one transition function. Illegal transitions are no-ops — that is what
/// makes a duplicated 401 harmless instead of starting a refresh storm.
pub fn transition(state: &AuthSessionState, event: &AuthEvent) -> AuthSessionState {
    use AuthEvent as E;

    if matches!(event, E::SignOut) {
        return AuthSessionState::new(AuthPhase::Unauthenticated);
    }

    match event {
        E::RestoreStarted | E::LoginStarted
            if matches!(
                state.phase,
                AuthPhase::Unauthenticated
                    | AuthPhase::SessionExpired
                    | AuthPhase::AuthenticationError
            ) =>
        {
            AuthSessionState::new(AuthPhase::Authenticating)
        }

        E::ProbeSucceeded | E::LoginSucceeded if state.phase == AuthPhase::Authenticating => {
            AuthSessionState::new(AuthPhase::Authenticated)
        }

        E::ProbeUnauthorized if state.phase == AuthPhase::Authenticating => {
            if state.retried {
                // A second 401 after the single allowed refresh is proof the
                // stored pair cannot recover the session. Never refresh again.
                AuthSessionState::ended(SessionEndReason::InvalidToken)
            } else {
                AuthSessionState {
                    phase: AuthPhase::Refreshing,
                    ..state.clone()
                }
            }
        }

        E::ProbeFailed(message)
            if matches!(
                state.phase,
                AuthPhase::Authenticating | AuthPhase::Refreshing
            ) =>
        {
            AuthSessionState::errored(message)
        }

        E::RefreshSucceeded if state.phase == AuthPhase::Refreshing => AuthSessionState {
            phase: AuthPhase::Authenticating,
            retried: true,
            reason: None,
            message: None,
        },

        E::RefreshFailed(reason) if state.phase == AuthPhase::Refreshing => {
            AuthSessionState::ended(reason.clone())
        }

        E::SessionExpired(reason)
            if matches!(
                state.phase,
                AuthPhase::Authenticated
                    | AuthPhase::Authenticating
                    | AuthPhase::Refreshing
                    | AuthPhase::SessionExpired
            ) =>
        {
            AuthSessionState::ended(reason.clone())
        }

        E::LoginFailed(message) if state.phase == AuthPhase::Authenticating => {
            AuthSessionState::errored(message)
        }

        E::DismissError if state.phase == AuthPhase::AuthenticationError => {
            AuthSessionState::new(AuthPhase::Unauthenticated)
        }

        // Duplicate/unexpected events: stay put. This is the refresh-loop and
        // single-flight state-level guard (SEC03-T07 / SEC03-T08).
        _ => state.clone(),
    }
}

/// Stable list used by the cross-layer name assertion in `ica-contracts`.
pub const ALL_AUTH_PHASES: [AuthPhase; 6] = [
    AuthPhase::Unauthenticated,
    AuthPhase::Authenticating,
    AuthPhase::Authenticated,
    AuthPhase::Refreshing,
    AuthPhase::SessionExpired,
    AuthPhase::AuthenticationError,
];

#[cfg(test)]
mod tests {
    use super::*;
    use AuthEvent as E;

    fn apply(states: &[AuthEvent]) -> AuthSessionState {
        let mut state = AuthSessionState::new(AuthPhase::Unauthenticated);
        for event in states {
            state = transition(&state, event);
        }
        state
    }

    #[test]
    fn sec03_t01_login_enters_authenticated() {
        let state = apply(&[E::LoginStarted, E::LoginSucceeded]);
        assert_eq!(state.phase, AuthPhase::Authenticated);
        assert!(!state.retried);
    }

    #[test]
    fn sec03_t02_unexpired_access_token_restores() {
        let state = apply(&[E::RestoreStarted, E::ProbeSucceeded]);
        assert_eq!(state.phase, AuthPhase::Authenticated);
    }

    #[test]
    fn sec03_t03_expired_access_token_refreshes_and_retries_once() {
        let state = apply(&[
            E::RestoreStarted,
            E::ProbeUnauthorized,
            E::RefreshSucceeded,
            E::ProbeSucceeded,
        ]);
        assert_eq!(state.phase, AuthPhase::Authenticated);
        assert!(
            !state.retried,
            "the guard is per attempt: a fresh session may refresh once later"
        );
    }

    #[test]
    fn sec03_t04_restart_then_restore_is_the_same_transition() {
        // A restart is just a fresh provider instance over persisted tokens;
        // the policy is stateless across process boundaries.
        let before = apply(&[E::LoginStarted, E::LoginSucceeded]);
        let after_restart = transition(
            &AuthSessionState::new(AuthPhase::Unauthenticated),
            &E::RestoreStarted,
        );
        let restored = transition(&after_restart, &E::ProbeSucceeded);
        assert_eq!(before.phase, restored.phase);
    }

    #[test]
    fn sec03_t05_refresh_token_expired_requires_relogin() {
        let state = apply(&[
            E::RestoreStarted,
            E::ProbeUnauthorized,
            E::RefreshFailed(SessionEndReason::RefreshRejected),
        ]);
        assert_eq!(state.phase, AuthPhase::SessionExpired);
        assert_eq!(state.reason, Some(SessionEndReason::RefreshRejected));
    }

    #[test]
    fn sec03_t06_revoked_refresh_token_requires_relogin() {
        let state = apply(&[
            E::RestoreStarted,
            E::ProbeUnauthorized,
            E::SessionExpired(SessionEndReason::Revoked),
        ]);
        assert_eq!(state.phase, AuthPhase::SessionExpired);
        assert_eq!(state.reason, Some(SessionEndReason::Revoked));
    }

    #[test]
    fn sec03_t07_duplicate_401_never_starts_a_second_refresh() {
        let refreshing = apply(&[E::RestoreStarted, E::ProbeUnauthorized]);
        assert_eq!(refreshing.phase, AuthPhase::Refreshing);
        let duplicate = transition(&refreshing, &E::ProbeUnauthorized);
        assert_eq!(duplicate, refreshing, "401 while Refreshing is ignored");

        let retried = transition(&refreshing, &E::RefreshSucceeded);
        let second_401 = transition(&retried, &E::ProbeUnauthorized);
        assert_eq!(second_401.phase, AuthPhase::SessionExpired);
        assert_eq!(second_401.reason, Some(SessionEndReason::InvalidToken));
    }

    #[test]
    fn sec03_t08_refresh_500_does_not_retry_forever() {
        let state = apply(&[
            E::RestoreStarted,
            E::ProbeUnauthorized,
            E::RefreshFailed(SessionEndReason::RefreshFailed),
        ]);
        assert_eq!(state.phase, AuthPhase::SessionExpired);
        // Even a stray later success cannot resurrect a dead session.
        assert_eq!(transition(&state, &E::RefreshSucceeded), state);
    }

    #[test]
    fn sec03_t09_sign_out_clears_everything() {
        for from in [
            E::RestoreStarted,
            E::ProbeUnauthorized,
            E::RefreshSucceeded,
            E::ProbeSucceeded,
        ] {
            let mut state = AuthSessionState::new(AuthPhase::Unauthenticated);
            state = transition(&state, &E::LoginStarted);
            state = transition(&state, &from);
            let signed_out = transition(&state, &E::SignOut);
            assert_eq!(signed_out.phase, AuthPhase::Unauthenticated);
            assert!(!signed_out.retried);
            assert!(signed_out.reason.is_none());
            assert!(signed_out.message.is_none());
        }
    }

    #[test]
    fn sec03_t11_stale_token_cannot_be_accepted_after_retry() {
        let state = apply(&[
            E::RestoreStarted,
            E::ProbeUnauthorized,
            E::RefreshSucceeded,
            E::ProbeUnauthorized,
        ]);
        assert_eq!(state.phase, AuthPhase::SessionExpired);
        assert_eq!(state.reason, Some(SessionEndReason::InvalidToken));
    }

    #[test]
    fn non_probe_failure_is_an_authentication_error_with_a_retry_path() {
        let failed = apply(&[E::RestoreStarted, E::ProbeFailed("offline".into())]);
        assert_eq!(failed.phase, AuthPhase::AuthenticationError);
        assert_eq!(failed.message.as_deref(), Some("offline"));
        let retry = transition(&failed, &E::DismissError);
        assert_eq!(retry.phase, AuthPhase::Unauthenticated);
        assert_eq!(
            transition(&retry, &E::RestoreStarted).phase,
            AuthPhase::Authenticating
        );
    }

    #[test]
    fn the_six_phase_names_are_stable() {
        let names: Vec<&str> = ALL_AUTH_PHASES.iter().map(|p| p.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Unauthenticated",
                "Authenticating",
                "Authenticated",
                "Refreshing",
                "SessionExpired",
                "AuthenticationError",
            ]
        );
    }
}
