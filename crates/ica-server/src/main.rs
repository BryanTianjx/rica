//! ICA backend binary: configuration, tracing, startup self-check, graceful
//! shutdown.
//!
//! Configuration is environment-only, like the Python service: nothing
//! sensitive is written to disk, and the defaults are the loopback host and
//! port the desktop shell already probes.

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
        // W3's AI port: no provider is configured, so every call answers the
        // baseline's 503 `ai_provider_error`. The real provider is W5's job.
        Arc::new(ica_infra::drafts::NotConfiguredDraftGenerator),
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
        // W5 ai-key: the OS credential store (project ruling) — and the
        // runtime flag seeded from the env key, then overlaid by the stored
        // installation-wide slot (the baseline's apply_stored_secrets).
        Arc::new(ica_infra::secret_store::KeyringSecretStore),
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
        Arc::new(std::sync::RwLock::new(AiBudgetSettings {
            daily_budget_cents: ai_daily_budget_cents,
            warning_threshold: ai_budget_warning_threshold,
        })),
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

/// Resolve on Ctrl-C or SIGTERM so an in-flight request is not cut in half.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
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
    }
}
