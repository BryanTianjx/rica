//! Shared application state handed to every handler.

use std::sync::Arc;

use ica_application::PositionRepository;

/// Settings that shape a response body.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Reported by `GET /health` as `app` (`settings.app_name` in Python).
    pub app_name: String,
    /// Reported by `GET /health` as `ai_enabled`
    /// (`bool(settings.ai_api_key.strip())` in Python).
    pub ai_enabled: bool,
}

/// Everything a handler may reach.
///
/// The repository is a trait object so the HTTP layer never sees `sqlx`: the
/// dependency points from `ica-api` to `ica-application`, never to `ica-infra`.
#[derive(Clone)]
pub struct AppState {
    repository: Arc<dyn PositionRepository>,
    config: AppConfig,
}

impl AppState {
    pub fn new(repository: Arc<dyn PositionRepository>, config: AppConfig) -> Self {
        Self { repository, config }
    }

    pub fn repository(&self) -> &dyn PositionRepository {
        self.repository.as_ref()
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}
