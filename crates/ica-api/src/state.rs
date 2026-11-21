//! Shared application state handed to every handler.

use std::sync::Arc;

use ica_application::{
    AiBudgetSettings, AiModelRepository, AiUsageRepository, AppSettingsRepository,
    AuditTrailRepository, AuthRepository, AuthSecurity, BackupStore, BrowserExtractionRepository,
    CandidateRepository, ConversationRepository, DashboardRepository, DataSnapshotRepository,
    DraftGenerator, DraftRepository, ExportSealer, FollowUpRepository, ImportRepository,
    MessageRepository, OcrEngine, PositionRepository, SecretStore, SendRecordRepository,
};

/// Settings that shape a response body.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Reported by `GET /health` as `app` (`settings.app_name` in Python).
    pub app_name: String,
    /// Reported by `GET /health` as `ai_enabled`
    /// (`bool(settings.ai_api_key.strip())` in Python).
    pub ai_enabled: bool,
    /// The only directory `POST /api/imports/ocr` will read an image from
    /// (`OCR_UPLOAD_DIR`, i.e. `<data dir>/ocr` in Python). The path guard
    /// resolves against this before touching the file.
    pub ocr_upload_dir: std::path::PathBuf,
    /// W5 `GET /settings`: the effective AI configuration, echoed minus any
    /// secret (`settings.ai_provider` etc. in Python; defaults match).
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub ai_include_candidate_name: bool,
    /// Echoed as `database_url` (`settings.database_url`).
    pub database_url: String,
    /// Echoed as `backup_dir` (`settings.backup_dir`, resolved to
    /// `<data dir>/backups` when the default).
    pub backup_dir: String,
    pub backup_keep_count: i64,
    pub data_retention_days: i64,
    /// W5 AI endpoints: the V1.0 fallback model parameters and the budget
    /// pair (defaults match `app/config.py`).
    pub ai_max_tokens: i64,
    pub ai_timeout: i64,
    pub ai_temperature: f64,
    pub ai_daily_budget_cents: i64,
    pub ai_budget_warning_threshold: i64,
    /// W5 backups: the file path the backup API copies in and out of
    /// (parsed from `DATABASE_URL`; an in-memory database refuses backups).
    pub database_path: String,
    /// W6 auth: the JWT/refresh parameters (defaults match `app/config.py`).
    pub jwt_secret: String,
    pub jwt_access_token_expire_minutes: i64,
    pub jwt_refresh_token_expire_days: i64,
    pub bcrypt_cost: u32,
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
    follow_ups: Arc<dyn FollowUpRepository>,
    dashboard: Arc<dyn DashboardRepository>,
    drafts: Arc<dyn DraftRepository>,
    draft_generator: Arc<dyn DraftGenerator>,
    imports: Arc<dyn ImportRepository>,
    extractions: Arc<dyn BrowserExtractionRepository>,
    snapshots: Arc<dyn DataSnapshotRepository>,
    audit_trail: Arc<dyn AuditTrailRepository>,
    settings: Arc<dyn AppSettingsRepository>,
    ai_models: Arc<dyn AiModelRepository>,
    ai_usage: Arc<dyn AiUsageRepository>,
    /// W5 ai-key: the OS credential store seam.
    secrets: Arc<dyn SecretStore>,
    /// W5 backups: the file-subsystem seam.
    backups: Arc<dyn BackupStore>,
    /// W6: the auth data seam + the crypto seam.
    auth_repo: Arc<dyn AuthRepository>,
    auth_security: Arc<dyn AuthSecurity>,
    /// W5 ai-key: the runtime AI-enabled flag. Python derives
    /// `settings.ai_enabled` from `bool(settings.ai_api_key.strip())`, so a
    /// `PUT /settings/ai-key` turns it on and an installation-wide
    /// `DELETE` turns it off without a restart.
    ai_key_present: Arc<std::sync::RwLock<bool>>,
    /// `PATCH /ai/budget` mutates this in process; `GET` reads it. The
    /// `app_settings` rows it also writes are never read back by the
    /// baseline, so a restart resets to the configured default (reproduced
    /// quirk, not a cache).
    ai_budget: Arc<std::sync::RwLock<AiBudgetSettings>>,
    export_sealer: Arc<dyn ExportSealer>,
    ocr: Arc<dyn OcrEngine>,
    config: AppConfig,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        positions: Arc<dyn PositionRepository>,
        candidates: Arc<dyn CandidateRepository>,
        conversations: Arc<dyn ConversationRepository>,
        messages: Arc<dyn MessageRepository>,
        send_records: Arc<dyn SendRecordRepository>,
        follow_ups: Arc<dyn FollowUpRepository>,
        dashboard: Arc<dyn DashboardRepository>,
        drafts: Arc<dyn DraftRepository>,
        draft_generator: Arc<dyn DraftGenerator>,
        imports: Arc<dyn ImportRepository>,
        extractions: Arc<dyn BrowserExtractionRepository>,
        snapshots: Arc<dyn DataSnapshotRepository>,
        audit_trail: Arc<dyn AuditTrailRepository>,
        settings: Arc<dyn AppSettingsRepository>,
        ai_models: Arc<dyn AiModelRepository>,
        ai_usage: Arc<dyn AiUsageRepository>,
        secrets: Arc<dyn SecretStore>,
        backups: Arc<dyn BackupStore>,
        auth_repo: Arc<dyn AuthRepository>,
        auth_security: Arc<dyn AuthSecurity>,
        ai_key_present: Arc<std::sync::RwLock<bool>>,
        ai_budget: Arc<std::sync::RwLock<AiBudgetSettings>>,
        export_sealer: Arc<dyn ExportSealer>,
        ocr: Arc<dyn OcrEngine>,
        config: AppConfig,
    ) -> Self {
        Self {
            positions,
            candidates,
            conversations,
            messages,
            send_records,
            follow_ups,
            dashboard,
            drafts,
            draft_generator,
            imports,
            extractions,
            snapshots,
            audit_trail,
            settings,
            ai_models,
            ai_usage,
            secrets,
            backups,
            auth_repo,
            auth_security,
            ai_key_present,
            ai_budget,
            export_sealer,
            ocr,
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

    pub fn follow_ups(&self) -> &dyn FollowUpRepository {
        self.follow_ups.as_ref()
    }

    pub fn dashboard(&self) -> &dyn DashboardRepository {
        self.dashboard.as_ref()
    }

    pub fn drafts(&self) -> &dyn DraftRepository {
        self.drafts.as_ref()
    }

    pub fn draft_generator(&self) -> &dyn DraftGenerator {
        self.draft_generator.as_ref()
    }

    pub fn extractions(&self) -> &dyn BrowserExtractionRepository {
        self.extractions.as_ref()
    }

    pub fn imports(&self) -> &dyn ImportRepository {
        self.imports.as_ref()
    }

    pub fn snapshots(&self) -> &dyn DataSnapshotRepository {
        self.snapshots.as_ref()
    }

    /// W5: the audit trail read port.
    pub fn audit_trail(&self) -> &dyn AuditTrailRepository {
        self.audit_trail.as_ref()
    }

    /// W5: the `app_settings` port.
    pub fn settings(&self) -> &dyn AppSettingsRepository {
        self.settings.as_ref()
    }

    /// W5: the AI registry read port.
    pub fn ai_models(&self) -> &dyn AiModelRepository {
        self.ai_models.as_ref()
    }

    /// W5: the AI usage read port.
    pub fn ai_usage(&self) -> &dyn AiUsageRepository {
        self.ai_usage.as_ref()
    }

    /// W5: the OS credential store seam.
    pub fn secrets(&self) -> &dyn SecretStore {
        self.secrets.as_ref()
    }

    /// W5: the backup/restore seam.
    pub fn backups(&self) -> &dyn BackupStore {
        self.backups.as_ref()
    }

    /// W6: the auth data seam.
    pub fn auth_repo(&self) -> &dyn AuthRepository {
        self.auth_repo.as_ref()
    }

    /// W6: the crypto seam (bcrypt + JWT + refresh generation).
    pub fn auth_security(&self) -> &dyn AuthSecurity {
        self.auth_security.as_ref()
    }

    /// W5: the runtime AI flag (see the field comment for the Python
    /// derivation it mirrors).
    pub fn ai_key_present(&self) -> bool {
        *self.ai_key_present.read().expect("ai key lock")
    }

    /// W5: apply the runtime AI flag change.
    pub fn set_ai_key_present(&self, present: bool) {
        *self.ai_key_present.write().expect("ai key lock") = present;
    }

    /// W5: the process-local budget pair (a snapshot; PATCH writes through).
    pub fn ai_budget(&self) -> AiBudgetSettings {
        *self.ai_budget.read().expect("ai budget lock")
    }

    /// W5: apply the budget patch to the in-process pair.
    pub fn set_ai_budget(&self, budget: AiBudgetSettings) {
        *self.ai_budget.write().expect("ai budget lock") = budget;
    }

    /// W5: the AI-side config view for the registry fallback.
    pub fn ai_settings_view(&self) -> ica_application::AiSettingsView {
        ica_application::AiSettingsView {
            ai_enabled: self.ai_key_present(),
            ai_provider: self.config.ai_provider.clone(),
            ai_model: self.config.ai_model.clone(),
            ai_base_url: self.config.ai_base_url.clone(),
            ai_max_tokens: self.config.ai_max_tokens,
            ai_timeout: self.config.ai_timeout,
            ai_temperature: self.config.ai_temperature,
        }
    }

    /// W5: the `GET /settings` / `ai-egress` config view, built from the
    /// injected configuration (the adapter never reads the environment).
    pub fn settings_config_view(&self) -> ica_application::SettingsConfigView {
        ica_application::SettingsConfigView {
            app_name: self.config.app_name.clone(),
            ai_enabled: self.ai_key_present(),
            ai_provider: self.config.ai_provider.clone(),
            ai_model: self.config.ai_model.clone(),
            ai_base_url: self.config.ai_base_url.clone(),
            database_url: self.config.database_url.clone(),
            backup_dir: self.config.backup_dir.clone(),
            backup_keep_count: self.config.backup_keep_count,
            data_retention_days: self.config.data_retention_days,
            ai_include_candidate_name: self.config.ai_include_candidate_name,
        }
    }

    pub fn export_sealer(&self) -> &dyn ExportSealer {
        self.export_sealer.as_ref()
    }

    pub fn ocr(&self) -> &dyn OcrEngine {
        self.ocr.as_ref()
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}
