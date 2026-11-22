//! Desktop adapter state: the same port-object assembly `ica-server` performs,
//! minus the HTTP server. The shell host calls [`DesktopState::connect`] with
//! the platform data directory's database path (Tauri-audit G-4: the host owns
//! the data directory — `%LOCALAPPDATA%\ICA\data` / `~/Library/Application
//! Support` — and injects the URL; the adapter never reads the environment).

use std::sync::Arc;

use ica_application::{
    AiBudgetSettings, AiModelRepository, AiUsageRepository, AppSettingsRepository,
    AuditTrailRepository, AuthRepository, AuthSecurity, BackupStore, BrowserExtractionRepository,
    CandidateRepository, ConversationRepository, DashboardRepository, DataSnapshotRepository,
    DraftGenerator, DraftRepository, ExportSealer, FollowUpRepository, ImportRepository,
    MessageRepository, OcrEngine, PositionRepository, SecretStore, SendRecordRepository,
};
use ica_infra::export_crypto::AesGcmSealer;
use ica_infra::ocr_rapid::RapidOcrEngine;
use ica_infra::SqliteStore;
use std::path::PathBuf;

/// Settings that shape response bodies (mirrors the server's `AppConfig`).
#[derive(Debug, Clone)]
pub struct DesktopConfig {
    /// Reported by the `health` command as `app` (`settings.app_name`).
    pub app_name: String,
    /// Reported by the `health` command as `ai_enabled`
    /// (`bool(settings.ai_api_key.strip())` in Python).
    pub ai_enabled: bool,
    /// The only directory `imports_ocr` will read an image from
    /// (`<data dir>/ocr` in Python). The path guard resolves against this
    /// before touching the file.
    pub ocr_upload_dir: std::path::PathBuf,
    /// W5 settings echo (mirrors the server's `AppConfig`; defaults match).
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub ai_include_candidate_name: bool,
    pub database_url: String,
    pub backup_dir: String,
    pub backup_keep_count: i64,
    pub data_retention_days: i64,
    /// W5 AI endpoints (defaults match `app/config.py`).
    pub ai_max_tokens: i64,
    pub ai_timeout: i64,
    pub ai_temperature: f64,
    pub ai_daily_budget_cents: i64,
    pub ai_budget_warning_threshold: i64,
    /// W5 backups: the file path the backup API copies in and out of.
    pub database_path: String,
    /// W6 auth (defaults match `app/config.py`).
    pub jwt_secret: String,
    pub jwt_access_token_expire_minutes: i64,
    pub jwt_refresh_token_expire_days: i64,
    pub bcrypt_cost: u32,
}

/// Every port the command layer may reach — several views of one adapter, so
/// a write plus its audit event share the same pool and transaction.
#[derive(Clone)]
pub struct DesktopState {
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
    config: DesktopConfig,
}

impl DesktopState {
    /// Open the SQLite database at `database_url`, run the startup self-check
    /// and assemble the port views (the `ica-server/main.rs` assembly, minus
    /// the HTTP listener).
    pub async fn connect(database_url: &str, config: DesktopConfig) -> Result<Self, String> {
        let pool = ica_infra::connect(database_url)
            .await
            .map_err(|error| format!("cannot open database {database_url}: {error}"))?;
        ica_infra::table_names(&pool)
            .await
            .map_err(|error| format!("cannot list tables in {database_url}: {error}"))?;
        let store = SqliteStore::new(pool);
        Ok(Self::from_store(store, config))
    }

    /// Assemble the views over an existing store — what the integration tests
    /// and (later) the Tauri shell builder use when the pool already exists.
    pub fn from_store(store: SqliteStore, config: DesktopConfig) -> Self {
        // W5 ai-key: the OS credential store seam (same ruling as the server).
        // Windows/macOS go through the platform vault; Linux (dev host) uses
        // the baseline's 0600-file fallback under `<data root>/secrets` —
        // derived from the database path the host injected.
        let secrets: Arc<dyn SecretStore> = Arc::new(
            ica_infra::secret_store::KeyringSecretStore::with_fallback_dir(
                PathBuf::from(&config.database_path)
                    .parent()
                    .map(|root| root.join("secrets"))
                    .unwrap_or_else(|| PathBuf::from("secrets")),
            ),
        );
        // The process-local budget pair, shared between the settings patch
        // and the generator's gate.
        let ai_budget = Arc::new(std::sync::RwLock::new(ica_application::AiBudgetSettings {
            daily_budget_cents: config.ai_daily_budget_cents,
            warning_threshold: config.ai_budget_warning_threshold,
        }));
        // W5's real AI port (the same adapter the server wires), fed from the
        // injected configuration.
        let draft_generator = ica_infra::ai_client::AiDraftGenerator::new(
            store.clone(),
            secrets.clone(),
            ica_application::AiSettingsView {
                ai_enabled: config.ai_enabled,
                ai_provider: config.ai_provider.clone(),
                ai_model: config.ai_model.clone(),
                ai_base_url: config.ai_base_url.clone(),
                ai_max_tokens: config.ai_max_tokens,
                ai_timeout: config.ai_timeout,
                ai_temperature: config.ai_temperature,
            },
            ai_budget.clone(),
        );
        Self {
            positions: Arc::new(store.clone()),
            candidates: Arc::new(store.clone()),
            conversations: Arc::new(store.clone()),
            messages: Arc::new(store.clone()),
            send_records: Arc::new(store.clone()),
            follow_ups: Arc::new(store.clone()),
            dashboard: Arc::new(store.clone()),
            drafts: Arc::new(store.clone()),
            draft_generator: Arc::new(draft_generator),
            // W4 ports. The same store serves the import and snapshot
            // repositories, so a write and its audit event share one
            // transaction.
            imports: Arc::new(store.clone()),
            extractions: Arc::new(store.clone()),
            snapshots: Arc::new(store.clone()),
            // W5 (first slice): the audit trail read port — same store.
            audit_trail: Arc::new(store.clone()),
            // W5 settings slice: the non-sensitive `app_settings` port.
            settings: Arc::new(store.clone()),
            // W5 AI slice: registry/usage ports plus the process-local budget
            // pair (same semantics as the server).
            ai_models: Arc::new(store.clone()),
            ai_usage: Arc::new(store.clone()),
            secrets,
            backups: Arc::new(ica_infra::backup::BackupManager::new(
                PathBuf::from(&config.database_path),
                PathBuf::from(&config.backup_dir),
                config.backup_keep_count,
            )),
            auth_repo: Arc::new(store.clone()),
            auth_security: Arc::new(ica_infra::auth_store::AuthCrypto {
                jwt_secret: config.jwt_secret.clone(),
                access_token_minutes: config.jwt_access_token_expire_minutes,
                refresh_token_days: config.jwt_refresh_token_expire_days,
                bcrypt_cost: config.bcrypt_cost,
            }),
            ai_key_present: Arc::new(std::sync::RwLock::new(config.ai_enabled)),
            ai_budget,
            export_sealer: Arc::new(AesGcmSealer),
            ocr: Arc::new(RapidOcrEngine),
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

    /// W6: the crypto seam.
    pub fn auth_security(&self) -> &dyn AuthSecurity {
        self.auth_security.as_ref()
    }

    /// W5: the runtime AI flag (mirrors `bool(settings.ai_api_key.strip())`).
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
            ai_enabled: self.config.ai_enabled,
            ai_provider: self.config.ai_provider.clone(),
            ai_model: self.config.ai_model.clone(),
            ai_base_url: self.config.ai_base_url.clone(),
            ai_max_tokens: self.config.ai_max_tokens,
            ai_timeout: self.config.ai_timeout,
            ai_temperature: self.config.ai_temperature,
        }
    }

    /// W5: the settings config view, built from the injected configuration.
    pub fn settings_config_view(&self) -> ica_application::SettingsConfigView {
        ica_application::SettingsConfigView {
            app_name: self.config.app_name.clone(),
            ai_enabled: self.config.ai_enabled,
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

    pub fn config(&self) -> &DesktopConfig {
        &self.config
    }
}
