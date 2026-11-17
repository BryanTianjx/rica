//! Shared application state handed to every handler.

use std::sync::Arc;

use ica_application::{
    CandidateRepository, ConversationRepository, MessageRepository, PositionRepository,
    SendRecordRepository,
};

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
/// Each port is a trait object so the HTTP layer never sees `sqlx`: the
/// dependency points from `ica-api` to `ica-application`, never to `ica-infra`.
/// The binary hands in several views of the *same* adapter, so one pool serves
/// every aggregate and a write plus its audit event can share a transaction.
#[derive(Clone)]
pub struct AppState {
    positions: Arc<dyn PositionRepository>,
    candidates: Arc<dyn CandidateRepository>,
    conversations: Arc<dyn ConversationRepository>,
    messages: Arc<dyn MessageRepository>,
    send_records: Arc<dyn SendRecordRepository>,
    config: AppConfig,
}

impl AppState {
    pub fn new(
        positions: Arc<dyn PositionRepository>,
        candidates: Arc<dyn CandidateRepository>,
        conversations: Arc<dyn ConversationRepository>,
        messages: Arc<dyn MessageRepository>,
        send_records: Arc<dyn SendRecordRepository>,
        config: AppConfig,
    ) -> Self {
        Self {
            positions,
            candidates,
            conversations,
            messages,
            send_records,
            config,
        }
    }

    pub fn positions(&self) -> &dyn PositionRepository {
        self.positions.as_ref()
    }

    pub fn candidates(&self) -> &dyn CandidateRepository {
        self.candidates.as_ref()
    }

    pub fn conversations(&self) -> &dyn ConversationRepository {
        self.conversations.as_ref()
    }

    pub fn messages(&self) -> &dyn MessageRepository {
        self.messages.as_ref()
    }

    pub fn send_records(&self) -> &dyn SendRecordRepository {
        self.send_records.as_ref()
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}
