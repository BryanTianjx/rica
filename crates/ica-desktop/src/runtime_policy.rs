//! Desktop runtime launch policy (ARCH-P3 AC2).
//!
//! The P2 `AppRuntime` is the only business assembly. What this module decides
//! is *how the process is wired*: whether the local HTTP server is resident,
//! on demand, or disabled, and the timeouts the Sidecar Manager applies to
//! child processes. It is pure configuration (no Tauri, no `Command`), so the
//! policy can be tested on a host that cannot compile the shell.

use std::time::Duration;

/// How the optional local Rust HTTP server is launched.
///
/// `Embedded` is the accepted review decision (RICA-ARCH-001 §19 question 1):
/// the desktop assembles `AppRuntime` in-process and does **not** keep a
/// resident HTTP listener. The `ica://` bridge starts the listener lazily on
/// the first Rust-routed `/api` request. `Always` preserves the legacy
/// resident mode for the §6.4 scenarios (third-party tools, local web
/// clients, automation, debugging, future remote service); `Never` is the
/// pure-IPC mode where a Rust-routed request returns 502 instead of starting
/// anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpServerMode {
    Embedded,
    Always,
    Never,
}

impl HttpServerMode {
    /// Accept the documented lowercase names plus a few harmless aliases;
    /// anything unrecognised falls back to the accepted default (Embedded)
    /// rather than aborting the desktop.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "always" | "on" | "legacy" | "resident" => Self::Always,
            "never" | "off" | "disabled" | "ipc" => Self::Never,
            _ => Self::Embedded,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    /// True only for the legacy resident mode: the host spawns ica-server
    /// during startup.
    pub fn starts_at_boot(&self) -> bool {
        matches!(self, Self::Always)
    }

    /// True for the accepted default: no listener at boot, spawn on the first
    /// request that needs the Rust backend.
    pub fn starts_on_demand(&self) -> bool {
        matches!(self, Self::Embedded)
    }
}

/// All host-injected runtime knobs in one testable value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePolicy {
    pub http_mode: HttpServerMode,
    pub handshake_timeout_ms: u64,
    pub health_timeout_ms: u64,
    pub graceful_shutdown_timeout_ms: u64,
    pub max_restarts: u32,
    pub diagnostics_enabled: bool,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            http_mode: HttpServerMode::Embedded,
            handshake_timeout_ms: 60_000,
            health_timeout_ms: 20_000,
            graceful_shutdown_timeout_ms: 5_000,
            max_restarts: 2,
            diagnostics_enabled: true,
        }
    }
}

impl RuntimePolicy {
    /// Parse from any `(key, value)` source; the host passes `std::env::vars()`
    /// and tests pass a slice, so no environment access is hidden here.
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut policy = Self::default();
        for (key, value) in pairs {
            let value = value.as_ref();
            match key.as_ref() {
                "ICA_HTTP_SERVER_MODE" => policy.http_mode = HttpServerMode::parse(value),
                "ICA_HANDSHAKE_TIMEOUT_MS" => {
                    policy.handshake_timeout_ms = positive_or(value, policy.handshake_timeout_ms);
                }
                "ICA_HEALTH_TIMEOUT_MS" => {
                    policy.health_timeout_ms = positive_or(value, policy.health_timeout_ms);
                }
                "ICA_GRACEFUL_SHUTDOWN_TIMEOUT_MS" => {
                    policy.graceful_shutdown_timeout_ms =
                        positive_or(value, policy.graceful_shutdown_timeout_ms);
                }
                "ICA_MAX_RESTARTS" => {
                    policy.max_restarts = positive_or(value, policy.max_restarts as u64) as u32;
                }
                "ICA_DIAGNOSTICS" => {
                    policy.diagnostics_enabled = !value.trim().eq_ignore_ascii_case("off");
                }
                _ => {}
            }
        }
        policy
    }

    pub fn from_env() -> Self {
        Self::from_pairs(std::env::vars())
    }

    pub fn handshake_timeout(&self) -> Duration {
        Duration::from_millis(self.handshake_timeout_ms)
    }

    pub fn health_timeout(&self) -> Duration {
        Duration::from_millis(self.health_timeout_ms)
    }

    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_millis(self.graceful_shutdown_timeout_ms)
    }

    /// True only for the legacy resident mode: the host spawns ica-server
    /// during startup.
    pub fn starts_http_at_boot(&self) -> bool {
        self.http_mode.starts_at_boot()
    }

    /// True for the accepted default: no listener at boot, spawn on the first
    /// request that needs the Rust backend.
    pub fn starts_http_on_demand(&self) -> bool {
        self.http_mode.starts_on_demand()
    }
}

fn positive_or(raw: &str, default: u64) -> u64 {
    raw.trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_mode_is_the_default_and_never_resident() {
        let policy = RuntimePolicy::default();
        assert_eq!(policy.http_mode, HttpServerMode::Embedded);
        assert!(!policy.starts_http_at_boot());
        assert!(policy.starts_http_on_demand());
    }

    #[test]
    fn always_and_never_are_explicit_opt_ins() {
        let always = RuntimePolicy::from_pairs([("ICA_HTTP_SERVER_MODE", "always")]);
        assert!(always.starts_http_at_boot());
        assert!(!always.starts_http_on_demand());
        let never = RuntimePolicy::from_pairs([("ICA_HTTP_SERVER_MODE", "never")]);
        assert!(!never.starts_http_at_boot());
        assert!(!never.starts_http_on_demand());
    }

    #[test]
    fn an_unknown_mode_falls_back_to_embedded() {
        for raw in ["", "bogus", "EMBEDDED", "1"] {
            let policy = RuntimePolicy::from_pairs([("ICA_HTTP_SERVER_MODE", raw)]);
            assert_eq!(policy.http_mode, HttpServerMode::Embedded, "{raw}");
        }
    }

    #[test]
    fn timeouts_are_positive_and_invalid_values_keep_the_default() {
        let policy = RuntimePolicy::from_pairs([
            ("ICA_HANDSHAKE_TIMEOUT_MS", "1234"),
            ("ICA_HEALTH_TIMEOUT_MS", "oops"),
            ("ICA_GRACEFUL_SHUTDOWN_TIMEOUT_MS", "0"),
            ("ICA_MAX_RESTARTS", "3"),
        ]);
        assert_eq!(policy.handshake_timeout(), Duration::from_millis(1234));
        assert_eq!(policy.health_timeout(), Duration::from_millis(20_000));
        assert_eq!(
            policy.graceful_shutdown_timeout(),
            Duration::from_millis(5_000)
        );
        assert_eq!(policy.max_restarts, 3);
    }
}
