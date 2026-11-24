//! ICA backend binary: configuration, tracing, startup self-check, graceful
//! shutdown.
//!
//! Configuration is environment-only, like the Python service: nothing
//! sensitive is written to disk, and the defaults are the loopback host and
//! port the desktop shell already probes.

// P2-02: this binary crate has no public items to document and no production
// unwrap/expect sites, so it can be held to the strict target lint baseline
// without touching the workspace-wide deferred allowances.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use ica_api::{AiBudgetSettings, AppConfig, AppState};
use ica_infra::{connect, table_names, DatabaseUrl, SqlitePool, SqliteStore};
use tracing_subscriber::EnvFilter;

/// Tables the served endpoints read. A database without them is not "empty",
/// it is the wrong database, and the process must say so at startup instead of
/// failing on the first request.
const REQUIRED_TABLES: [&str; 2] = ["candidates", "positions"];

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Install the tracing subscriber once, honouring `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,ica_api=info,ica_server=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Only loopback hosts may be bound until network authentication exists.
///
/// The Python baseline and the desktop shell both use a local sidecar; there
/// is no auth middleware in this build (that work belongs to SEC-04-B/C).
/// Binding anything else would expose the whole HTTP surface unauthenticated.
fn validate_bind_host(host: &str) -> Result<(), String> {
    let trimmed = host.trim();
    if trimmed.eq_ignore_ascii_case("localhost") || trimmed == "127.0.0.1" || trimmed == "::1" {
        return Ok(());
    }
    Err(format!(
        "refusing to bind to non-loopback host '{host}': this build has no network authentication; set HOST=127.0.0.1 (or localhost) to start"
    ))
}

/// SEC-04-A: the explicit opt-in for *attempting* remote mode.
///
/// Absence of the variable, an empty value, or `false` all keep the Local-only
/// fail-fast policy. Only the literal `true` (case-insensitive) is an opt-in;
/// `1` / `yes` / other values are rejected rather than guessed at.
const AUTH_REQUIRED_ENV: &str = "ICA_AUTH_REQUIRED";

/// SEC-04-A: honest readiness flag. SEC-04-B/C must flip this to `true` only
/// when a real authentication gate AND resource authorization exist; until
/// then an opt-in cannot make a non-loopback bind safe.
const NETWORK_AUTHENTICATION_READY: bool = false;

/// The message an explicit opt-in receives while real auth is absent. It is
/// deliberately explicit so the flag can never be mistaken for a security
/// control (no "authentication enabled" claim anywhere in this build).
const AUTH_NOT_IMPLEMENTED_MESSAGE: &str = "多用户认证尚未实现，不能启用远程模式";

/// Parse the explicit opt-in. Pure (no environment reads) so the semantics are
/// unit-testable without mutating process-global state.
fn parse_auth_required(value: Option<&str>) -> Result<bool, String> {
    match value.map(str::trim) {
        None | Some("") => Ok(false),
        Some(v) if v.eq_ignore_ascii_case("false") => Ok(false),
        Some(v) if v.eq_ignore_ascii_case("true") => Ok(true),
        Some(other) => Err(format!(
            "{AUTH_REQUIRED_ENV} must be true or false, got '{other}'"
        )),
    }
}

/// SEC-04-A startup exposure policy: Local-only by default, explicit opt-in
/// required for a remote bind, and still blocked until real auth ships.
///
/// Loopback is always allowed. For a non-loopback host:
///   * no explicit opt-in -> the original fail-fast error (`validate_bind_host`);
///   * opt-in but `NETWORK_AUTHENTICATION_READY == false` -> the dedicated
///     "multi-user authentication is not implemented" error;
///   * opt-in and readiness (future SEC-04-B/C) -> allowed, with the real auth
///     middleware installed by that card, not by this one.
fn validate_bind_policy(host: &str, auth_required: bool) -> Result<(), String> {
    if validate_bind_host(host).is_ok() {
        return Ok(());
    }
    if !auth_required {
        // Keep the exact wording the three bas002 fail-fast tests pin.
        return validate_bind_host(host);
    }
    if !NETWORK_AUTHENTICATION_READY {
        return Err(format!(
            "refusing to bind to non-loopback host '{host}': {AUTH_NOT_IMPLEMENTED_MESSAGE} \
             ({AUTH_REQUIRED_ENV}=true was set, but this build has no real multi-user authentication / JWT middleware; \
             SEC-04-B/C are not implemented; set HOST=127.0.0.1 to start in Local-only mode)"
        ));
    }
    // Reachable only after SEC-04-B/C flip the readiness flag and install the
    // real gate. `run` still parses and binds the address below.
    Ok(())
}

/// Read an environment variable, falling back to `default` when it is unset.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env_or("DATABASE_URL", "sqlite:///./ica.db");
    let app_name = env_or("ICA_APP_NAME", "ICA Personal");
    // Python: `bool(settings.ai_api_key.strip())` — a whitespace-only key means
    // "no key", so AI features stay off rather than failing on first use.
    let ai_enabled = std::env::var("AI_API_KEY")
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false);
    let host = env_or("HOST", "127.0.0.1");
    // SEC-04-A: environment-only, explicitly opt-in. The policy refuses every
    // non-loopback bind unless BOTH the switch is set and real auth exists —
    // before the database is touched or a listener is opened.
    let auth_required_raw = std::env::var(AUTH_REQUIRED_ENV).ok();
    let auth_required = parse_auth_required(auth_required_raw.as_deref())?;
    validate_bind_policy(&host, auth_required)?;
    if auth_required && !NETWORK_AUTHENTICATION_READY {
        tracing::warn!(
            "{AUTH_REQUIRED_ENV}=true is set but real multi-user authentication is not implemented; server stays Local-only on {host}"
        );
    }
    let data_dir = env_or("ICA_DATA_DIR", "./data");
    let ai_daily_budget_cents: i64 = env_or("AI_DAILY_BUDGET_CENTS", "0")
        .parse()
        .map_err(|error| format!("AI_DAILY_BUDGET_CENTS is not an integer: {error}"))?;
    let ai_budget_warning_threshold: i64 = env_or("AI_BUDGET_WARNING_THRESHOLD", "90")
        .parse()
        .map_err(|error| format!("AI_BUDGET_WARNING_THRESHOLD is not an integer: {error}"))?;

    // The effective database file path and backup directory (both echoed by
    // GET /settings and used by the backup adapter).
    let database_path = DatabaseUrl::parse(&database_url)
        .map(|url| url.path.clone())
        .unwrap_or_else(|_| "ica.db".to_string());
    let backup_keep_count: i64 = env_or("BACKUP_KEEP_COUNT", "10")
        .parse()
        .map_err(|error| format!("BACKUP_KEEP_COUNT is not an integer: {error}"))?;
    let backup_dir = {
        let configured = std::env::var("BACKUP_DIR").unwrap_or_default();
        if configured.is_empty() || configured == "./backups" {
            std::path::Path::new(&data_dir)
                .join("backups")
                .display()
                .to_string()
        } else {
            configured
        }
    };

    // The startup AI flag: the env key alone decides the seed here; the
    // stored installation-wide slot is overlaid right after the state exists
    // (apply_stored_secrets), through the same port the endpoints use.
    let ai_key_present = std::env::var("AI_API_KEY")
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false);
    let port: u16 = env_or("PORT", "8000")
        .parse()
        .map_err(|error| format!("PORT is not a valid port number: {error}"))?;

    let pool = connect(&database_url)
        .await
        .map_err(|error| format!("cannot open database {database_url}: {error}"))?;
    self_check(&pool).await?;

    // Log the path, not the URL: a URL could carry credentials if the scheme
    // ever changes, and the log is a file a user may share.
    let url = DatabaseUrl::parse(&database_url)
        .map_err(|error| format!("cannot parse database URL: {error}"))?;
    let logged_path = url.path.clone();

    // DEF-002: evolve an installed database before serving. A read-only or
    // immutable connection never migrates (opening the baseline database must
    // leave it byte-identical), so the runner only runs when the URL allows
    // writes.
    if url.read_only || url.immutable {
        tracing::info!("read-only database: schema migrations skipped");
    } else {
        let mut connection = pool
            .acquire()
            .await
            .map_err(|error| format!("cannot acquire a database connection: {error}"))?;
        let applied = ica_infra::migrations::apply_pending_migrations(&mut connection)
            .await
            .map_err(|error| format!("schema migration failed: {error}"))?;
        if applied.is_empty() {
            tracing::info!("schema is up to date");
        } else {
            tracing::info!("applied schema migrations: {applied:?}");
        }
    }

    let repository = SqliteStore::new(pool);
    // W5 ai-key: the OS credential store (project ruling). Windows/macOS go
    // through the platform vault; Linux (a dev host) mirrors the baseline's
    // 0600-file fallback under `<data dir>/secrets`.
    let secrets: Arc<dyn ica_application::ports::SecretStore> = Arc::new(
        ica_infra::secret_store::KeyringSecretStore::with_fallback_dir(
            std::path::Path::new(&data_dir).join("secrets"),
        ),
    );
    // The process-local budget pair, shared between the PATCH endpoint and
    // the generator's gate (the baseline mutates one singleton).
    let ai_budget = Arc::new(std::sync::RwLock::new(AiBudgetSettings {
        daily_budget_cents: ai_daily_budget_cents,
        warning_threshold: ai_budget_warning_threshold,
    }));
    // The AI-side view the generator's V1.0 fallback needs (the same fields
    // the registry fallback uses).
    let ai_view = ica_application::AiSettingsView {
        ai_enabled,
        ai_provider: env_or("AI_PROVIDER", "openai_compatible"),
        ai_model: env_or("AI_MODEL", "glm-4-flash"),
        ai_base_url: env_or("AI_BASE_URL", "https://open.bigmodel.cn/api/paas/v4"),
        ai_max_tokens: env_or("AI_MAX_TOKENS", "1500")
            .parse()
            .map_err(|error| format!("AI_MAX_TOKENS is not an integer: {error}"))?,
        ai_timeout: env_or("AI_TIMEOUT", "60")
            .parse()
            .map_err(|error| format!("AI_TIMEOUT is not an integer: {error}"))?,
        ai_temperature: env_or("AI_TEMPERATURE", "0.4")
            .parse()
            .map_err(|error| format!("AI_TEMPERATURE is not a number: {error}"))?,
    };
    // W5's real AI port: the OpenAI-compatible provider (registry routing,
    // budget gate, OS-store key resolution, the HTTP call, validation and the
    // draft+usage+audit transaction).
    let draft_generator = ica_infra::ai_client::AiDraftGenerator::new(
        repository.clone(),
        secrets.clone(),
        ai_view,
        ai_budget.clone(),
    );
    // One adapter, nine port views: the HTTP layer sees only the traits, and a
    // write plus its audit event always share the same pool/transaction.
    let state = AppState::new(
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(draft_generator),
        // W4 ports. The same pool-backed store serves the import repository and
        // the snapshot repository, so a write and its audit event share one
        // transaction; sealing and OCR are separate adapters.
        Arc::new(repository.clone()),
        // W7: the browser-extraction confirm path — same store, same pool.
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        // W5 (first slice): the audit trail read port — same store, same pool.
        Arc::new(repository.clone()),
        // W5 settings slice: the non-sensitive `app_settings` port.
        Arc::new(repository.clone()),
        // W5 AI slice: the registry/usage read ports plus the process-local
        // budget pair (the baseline mutates it in memory, writes rows it
        // never reads back, and resets on restart).
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        // W5 ai-key: the OS credential store shared with the generator — and
        // the runtime flag seeded from the env key, then overlaid by the
        // stored installation-wide slot (the baseline's apply_stored_secrets).
        secrets,
        Arc::new(ica_infra::backup::BackupManager::new(
            std::path::PathBuf::from(&database_path),
            std::path::PathBuf::from(&backup_dir),
            backup_keep_count,
        )),
        Arc::new(repository.clone()),
        Arc::new(ica_infra::auth_store::AuthCrypto {
            jwt_secret: env_or("JWT_SECRET", ""),
            access_token_minutes: env_or("JWT_ACCESS_TOKEN_EXPIRE_MINUTES", "15")
                .parse()
                .map_err(|error| {
                    format!("JWT_ACCESS_TOKEN_EXPIRE_MINUTES is not an integer: {error}")
                })?,
            refresh_token_days: env_or("JWT_REFRESH_TOKEN_EXPIRE_DAYS", "7")
                .parse()
                .map_err(|error| {
                    format!("JWT_REFRESH_TOKEN_EXPIRE_DAYS is not an integer: {error}")
                })?,
            bcrypt_cost: env_or("BCRYPT_COST", "12")
                .parse()
                .map_err(|error| format!("BCRYPT_COST is not an integer: {error}"))?,
        }),
        Arc::new(std::sync::RwLock::new(ai_key_present)),
        ai_budget,
        Arc::new(ica_infra::export_crypto::AesGcmSealer),
        Arc::new(ica_infra::ocr_rapid::RapidOcrEngine),
        AppConfig {
            app_name,
            ai_enabled,
            // `OCR_UPLOAD_DIR` in Python: `<data dir>/ocr`. Only paths under
            // here are readable by the OCR route.
            ocr_upload_dir: std::path::PathBuf::from(data_dir.clone()).join("ocr"),
            // W5 settings echo, defaults matching the Python configuration.
            ai_provider: env_or("AI_PROVIDER", "openai_compatible"),
            ai_model: env_or("AI_MODEL", "glm-4-flash"),
            ai_base_url: env_or("AI_BASE_URL", "https://open.bigmodel.cn/api/paas/v4"),
            ai_include_candidate_name: env_or("AI_INCLUDE_CANDIDATE_NAME", "false")
                .trim()
                .eq_ignore_ascii_case("true"),
            database_url: database_url.clone(),
            // Python's model_post_init: the default `./backups` resolves to
            // `<data dir>/backups`.
            backup_dir: {
                let configured = std::env::var("BACKUP_DIR").unwrap_or_default();
                if configured.is_empty() || configured == "./backups" {
                    std::path::Path::new(&data_dir)
                        .join("backups")
                        .display()
                        .to_string()
                } else {
                    configured
                }
            },
            backup_keep_count: env_or("BACKUP_KEEP_COUNT", "10")
                .parse()
                .map_err(|error| format!("BACKUP_KEEP_COUNT is not an integer: {error}"))?,
            data_retention_days: env_or("DATA_RETENTION_DAYS", "180")
                .parse()
                .map_err(|error| format!("DATA_RETENTION_DAYS is not an integer: {error}"))?,
            ai_max_tokens: env_or("AI_MAX_TOKENS", "1500")
                .parse()
                .map_err(|error| format!("AI_MAX_TOKENS is not an integer: {error}"))?,
            ai_timeout: env_or("AI_TIMEOUT", "60")
                .parse()
                .map_err(|error| format!("AI_TIMEOUT is not an integer: {error}"))?,
            ai_temperature: env_or("AI_TEMPERATURE", "0.4")
                .parse()
                .map_err(|error| format!("AI_TEMPERATURE is not a number: {error}"))?,
            ai_daily_budget_cents,
            ai_budget_warning_threshold,
            database_path: database_path.clone(),
            jwt_secret: env_or("JWT_SECRET", ""),
            jwt_access_token_expire_minutes: env_or("JWT_ACCESS_TOKEN_EXPIRE_MINUTES", "15")
                .parse()
                .map_err(|error| {
                    format!("JWT_ACCESS_TOKEN_EXPIRE_MINUTES is not an integer: {error}")
                })?,
            jwt_refresh_token_expire_days: env_or("JWT_REFRESH_TOKEN_EXPIRE_DAYS", "7")
                .parse()
                .map_err(|error| {
                    format!("JWT_REFRESH_TOKEN_EXPIRE_DAYS is not an integer: {error}")
                })?,
            bcrypt_cost: env_or("BCRYPT_COST", "12")
                .parse()
                .map_err(|error| format!("BCRYPT_COST is not an integer: {error}"))?,
        },
    );

    // apply_stored_secrets: a UI-configured installation-wide key overrides
    // the env-only state without a restart.
    if !ai_key_present {
        if let Ok(Some(value)) = state.secrets().load(ica_api::AI_KEY_SLOT).await {
            if !value.trim().is_empty() {
                state.set_ai_key_present(true);
            }
        }
    }

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|error| format!("invalid listen address {host}:{port}: {error}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // The desktop host's listening handshake (the same `{"listening": true,
    // "port": N}` line the bundled Python sidecar emits on stdout): with
    // PORT=0 the bound port is ephemeral, and the host learns it here instead
    // of parsing a tracing line. stdout only — the HTTP surface is untouched.
    let bound_port = listener.local_addr()?.port();
    println!("{{\"listening\": true, \"port\": {bound_port}}}");
    tracing::info!("ICA server listening on http://{addr} (database: {logged_path})");

    axum::serve(listener, ica_api::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}

/// Refuse to serve a database that cannot answer the endpoints.
async fn self_check(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    let names = table_names(pool).await?;
    let missing: Vec<&str> = REQUIRED_TABLES
        .iter()
        .copied()
        .filter(|required| !names.iter().any(|name| name == required))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "database is missing required tables: {} (found {} tables)",
            missing.join(", "),
            names.len()
        )
        .into());
    }
    tracing::info!("startup self-check passed: {} tables present", names.len());
    Ok(())
}

/// Resolve on Ctrl-C, SIGTERM, or stdin EOF so an in-flight request is not
/// cut in half. The Sidecar Manager holds the child's stdin open and drops it
/// for graceful shutdown (REL-01); Windows has no SIGTERM listener, so EOF is
/// the cross-platform graceful-stop trigger the host can actually use.
async fn shutdown_signal() {
    use tokio::io::AsyncReadExt;

    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    let mut stdin = tokio::io::stdin();
    let stdin_eof = async move {
        let mut byte = [0u8; 1];
        loop {
            match stdin.read(&mut byte).await {
                Ok(0) => {
                    tracing::info!("stdin closed, shutting down gracefully");
                    break;
                }
                Ok(_) => continue, // a stray byte is not a stop request
                Err(error) => {
                    tracing::warn!("cannot read stdin for shutdown: {error}");
                    break;
                }
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::warn!("cannot listen for SIGTERM: {error}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl-C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
        _ = stdin_eof => tracing::info!("shutting down on stdin EOF"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn bas002_loopback_hosts_are_accepted() {
        for host in ["127.0.0.1", "localhost", "::1", " LOCALHOST "] {
            assert!(validate_bind_host(host).is_ok(), "{host} must be allowed");
        }
    }

    #[test]
    fn bas002_non_loopback_hosts_are_rejected() {
        for host in [
            "0.0.0.0",
            "::",
            "192.168.1.10",
            "example.com",
            "",
            "127.0.0.2",
        ] {
            let error = validate_bind_host(host).unwrap_err();
            assert!(
                error.contains("refusing to bind to non-loopback host"),
                "{error}"
            );
            assert!(error.contains("no network authentication"), "{error}");
            assert!(error.contains(host), "{error}");
        }
    }

    #[tokio::test]
    async fn bas002_loopback_host_can_actually_bind() {
        validate_bind_host("127.0.0.1").unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }

    // ── SEC-04-A: independent verification of the AC1 wording ──────────────

    #[test]
    fn sec04_a_fail_fast_wording_is_stable() {
        for host in ["0.0.0.0", "::", "192.168.1.10", "example.com"] {
            let error = validate_bind_host(host).unwrap_err();
            assert!(
                error.contains("refusing to bind to non-loopback host"),
                "{error}"
            );
            assert!(error.contains("no network authentication"), "{error}");
        }
    }

    // ── SEC-04-A AC2: explicit opt-in, still honest about missing auth ─────

    #[test]
    fn sec04_a_non_loopback_is_refused_without_explicit_opt_in() {
        for host in ["0.0.0.0", "::", "192.168.1.10", "example.com"] {
            let error = validate_bind_policy(host, false).unwrap_err();
            assert!(
                error.contains("refusing to bind to non-loopback host"),
                "{error}"
            );
            assert!(error.contains("no network authentication"), "{error}");
        }
    }

    #[test]
    fn sec04_a_opt_in_without_real_auth_is_refused_with_a_clear_message() {
        for host in ["0.0.0.0", "::", "192.168.1.10"] {
            let error = validate_bind_policy(host, true).unwrap_err();
            assert!(
                error.contains("多用户认证尚未实现，不能启用远程模式"),
                "{error}"
            );
            assert!(
                error.contains("refusing to bind to non-loopback host"),
                "{error}"
            );
            // The opt-in must be honoured: no generic "no network
            // authentication" fail-fast text once it has been set.
            assert!(
                !error.contains("this build has no network authentication"),
                "{error}"
            );
            assert!(error.contains("SEC-04-B/C"), "{error}");
        }
    }

    #[test]
    fn sec04_a_auth_required_flag_is_explicit_only() {
        assert_eq!(parse_auth_required(None), Ok(false));
        assert_eq!(parse_auth_required(Some("")), Ok(false));
        assert_eq!(parse_auth_required(Some(" false ")), Ok(false));
        assert_eq!(parse_auth_required(Some("FALSE")), Ok(false));
        assert_eq!(parse_auth_required(Some(" true ")), Ok(true));
        assert_eq!(parse_auth_required(Some("TRUE")), Ok(true));
        for other in ["1", "yes", "on", "0", "maybe"] {
            let error = parse_auth_required(Some(other)).unwrap_err();
            assert!(
                error.contains("ICA_AUTH_REQUIRED must be true or false"),
                "{error}"
            );
        }
    }

    #[test]
    fn sec04_a_loopback_stays_allowed_with_or_without_opt_in() {
        for host in ["127.0.0.1", "localhost", "::1", " LOCALHOST "] {
            for auth_required in [false, true] {
                assert!(
                    validate_bind_policy(host, auth_required).is_ok(),
                    "{host} / {auth_required}"
                );
            }
        }
    }

    #[test]
    fn sec04_a_does_not_pretend_authentication_is_implemented() {
        // A const block keeps the "auth is not implemented" state checkable
        // while satisfying clippy::assertions_on_constants under -D warnings.
        const {
            assert!(
                !NETWORK_AUTHENTICATION_READY,
                "flip this only in SEC-04-B/C together with a real auth gate"
            );
        };
    }
}
