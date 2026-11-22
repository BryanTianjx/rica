//! HTTP adapter state (ARCH-P2 phase B).
//!
//! `AppState` is a thin wrapper around [`AppRuntime`], the single Application
//! Services assembly shared with the IPC adapter. It holds no business state
//! of its own: every port, lock and config value comes from the runtime, so
//! HTTP and IPC cannot drift into different application services.

use std::ops::Deref;
use std::sync::Arc;

use ica_application::ports::{
    AiModelRepository, AiUsageRepository, AppSettingsRepository, AuditTrailRepository,
    AuthRepository, AuthSecurity, BackupStore, BrowserExtractionRepository, CandidateRepository,
    ConversationRepository, DashboardRepository, DataSnapshotRepository, DraftGenerator,
    DraftRepository, ExportSealer, FollowUpRepository, ImportRepository, MessageRepository,
    OcrEngine, PositionRepository, SecretStore, SendRecordRepository,
};
use ica_application::{AiBudgetSettings, AppRuntime};

/// Transport-neutral configuration, formerly defined twice (once per
/// adapter). Both names now alias the same struct in `ica-application`.
pub type AppConfig = ica_application::RuntimeConfig;

/// The state handed to every axum handler.
#[derive(Clone)]
pub struct AppState {
    runtime: AppRuntime,
}

impl AppState {
    /// Historical constructor: port objects are injected by the composition
    /// root and forwarded to the shared runtime.
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
        Self::from_runtime(AppRuntime::new(
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
        ))
    }

    /// Wrap an already assembled runtime (the two transports can share the
    /// very same instance inside one process).
    pub fn from_runtime(runtime: AppRuntime) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &AppRuntime {
        &self.runtime
    }
}

/// Handlers reach Application Services through the shared runtime.
impl Deref for AppState {
    type Target = AppRuntime;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}
