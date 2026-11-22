//! Desktop adapter state: the same [`AppRuntime`] the HTTP adapter uses,
//! assembled over an injected SQLite store. The shell host owns the data
//! directory and hands the adapter a URL/config (Tauri-audit G-4); this
//! adapter never reads the environment.

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use ica_application::ports::{
    AiModelRepository, AiUsageRepository, AppSettingsRepository, AuditTrailRepository,
    AuthRepository, AuthSecurity, BackupStore, BrowserExtractionRepository, CandidateRepository,
    ConversationRepository, DashboardRepository, DataSnapshotRepository, DraftGenerator,
    DraftRepository, ExportSealer, FollowUpRepository, ImportRepository, MessageRepository,
    OcrEngine, PositionRepository, SecretStore, SendRecordRepository,
};
use ica_application::{AiBudgetSettings, AppRuntime};
use ica_infra::export_crypto::AesGcmSealer;
use ica_infra::ocr_rapid::RapidOcrEngine;
use ica_infra::SqliteStore;

/// Transport-neutral configuration, aliased to the one shared struct.
pub type DesktopConfig = ica_application::RuntimeConfig;

/// The state handed to every IPC command. Thin wrapper: the Application
/// Services come from the same `AppRuntime` type HTTP uses.
#[derive(Clone)]
pub struct DesktopState {
    runtime: AppRuntime,
}

impl DesktopState {
    /// Open the SQLite database at `database_url`, run the startup self-check
    /// and assemble the port views (the server's assembly, minus HTTP).
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

    /// Assemble the views over an existing store — what integration tests and
    /// the shell host use when the pool already exists.
    pub fn from_store(store: SqliteStore, config: DesktopConfig) -> Self {
        // The OS credential store seam; Linux dev uses a 0600 file under the
        // directory the host injected (derived from the database path).
        let secrets: Arc<dyn SecretStore> = Arc::new(
            ica_infra::secret_store::KeyringSecretStore::with_fallback_dir(
                PathBuf::from(&config.database_path)
                    .parent()
                    .map(|root| root.join("secrets"))
                    .unwrap_or_else(|| PathBuf::from("secrets")),
            ),
        );
        let ai_budget = Arc::new(std::sync::RwLock::new(AiBudgetSettings {
            daily_budget_cents: config.ai_daily_budget_cents,
            warning_threshold: config.ai_budget_warning_threshold,
        }));
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
        let backups: Arc<dyn BackupStore> = Arc::new(ica_infra::backup::BackupManager::new(
            PathBuf::from(&config.database_path),
            PathBuf::from(&config.backup_dir),
            config.backup_keep_count,
        ));
        let auth_security: Arc<dyn AuthSecurity> = Arc::new(ica_infra::auth_store::AuthCrypto {
            jwt_secret: config.jwt_secret.clone(),
            access_token_minutes: config.jwt_access_token_expire_minutes,
            refresh_token_days: config.jwt_refresh_token_expire_days,
            bcrypt_cost: config.bcrypt_cost,
        });
        let ocr: Arc<dyn OcrEngine> = Arc::new(RapidOcrEngine);
        let sealer: Arc<dyn ExportSealer> = Arc::new(AesGcmSealer);

        Self::from_runtime(AppRuntime::new(
            Arc::new(store.clone()) as Arc<dyn PositionRepository>,
            Arc::new(store.clone()) as Arc<dyn CandidateRepository>,
            Arc::new(store.clone()) as Arc<dyn ConversationRepository>,
            Arc::new(store.clone()) as Arc<dyn MessageRepository>,
            Arc::new(store.clone()) as Arc<dyn SendRecordRepository>,
            Arc::new(store.clone()) as Arc<dyn FollowUpRepository>,
            Arc::new(store.clone()) as Arc<dyn DashboardRepository>,
            Arc::new(store.clone()) as Arc<dyn DraftRepository>,
            Arc::new(draft_generator) as Arc<dyn DraftGenerator>,
            Arc::new(store.clone()) as Arc<dyn ImportRepository>,
            Arc::new(store.clone()) as Arc<dyn BrowserExtractionRepository>,
            Arc::new(store.clone()) as Arc<dyn DataSnapshotRepository>,
            Arc::new(store.clone()) as Arc<dyn AuditTrailRepository>,
            Arc::new(store.clone()) as Arc<dyn AppSettingsRepository>,
            Arc::new(store.clone()) as Arc<dyn AiModelRepository>,
            Arc::new(store.clone()) as Arc<dyn AiUsageRepository>,
            secrets,
            backups,
            Arc::new(store.clone()) as Arc<dyn AuthRepository>,
            auth_security,
            Arc::new(std::sync::RwLock::new(config.ai_enabled)),
            ai_budget,
            sealer,
            ocr,
            config,
        ))
    }

    pub fn from_runtime(runtime: AppRuntime) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &AppRuntime {
        &self.runtime
    }
}

impl Deref for DesktopState {
    type Target = AppRuntime;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}
