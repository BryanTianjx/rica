//! ICA backend binary: configuration, tracing, startup self-check, graceful
//! shutdown.
//!
//! Configuration is environment-only, like the Python service: nothing
//! sensitive is written to disk, and the defaults are the loopback host and
//! port the desktop shell already probes.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use ica_api::{AppConfig, AppState};
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
    let port: u16 = env_or("PORT", "8000")
        .parse()
        .map_err(|error| format!("PORT is not a valid port number: {error}"))?;

    let pool = connect(&database_url)
        .await
        .map_err(|error| format!("cannot open database {database_url}: {error}"))?;
    self_check(&pool).await?;

    // Log the path, not the URL: a URL could carry credentials if the scheme
    // ever changes, and the log is a file a user may share.
    let logged_path = DatabaseUrl::parse(&database_url)
        .map(|url| url.path)
        .unwrap_or_else(|_| database_url.clone());

    let repository = SqliteStore::new(pool);
    // One adapter, seven port views: the HTTP layer sees only the traits, and a
    // write plus its audit event always share the same pool/transaction.
    let state = AppState::new(
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository.clone()),
        Arc::new(repository),
        AppConfig {
            app_name,
            ai_enabled,
        },
    );

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|error| format!("invalid listen address {host}:{port}: {error}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
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
