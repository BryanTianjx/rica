//! The unified Application Runtime (ARCH-P2 §4.2 phase B).
//!
//! `AppRuntime` owns the port-object assembly and the process-local runtime
//! state that every transport needs. It lives in `ica-application`, which is
//! already the crate both adapters depend on, so **no new `ica-core` crate is
//! needed** (phase C's four conditions have not been met). The HTTP adapter
//! (`ica-api::AppState`) and the IPC adapter (`ica-desktop::DesktopState`) are
//! thin wrappers around this one type: they parse their own wire format, then
//! obtain identical Application Services from `AppRuntime`.
//!
//! Nothing here knows about axum, Tauri, SQLite or the environment: the host
//! injects adapters and configuration (Tauri-audit G-4). The same runtime can
//! therefore be assembled once per process and shared by both transports.

use std::sync::{Arc, RwLock};

use crate::ports::{
    AiModelRepository, AiUsageRepository, AppSettingsRepository, AuditTrailRepository,
    AuthRepository, AuthSecurity, BackupStore, BrowserExtractionRepository, CandidateRepository,
    ConversationRepository, DashboardRepository, DataSnapshotRepository, DraftGenerator,
    DraftRepository, ExportSealer, FollowUpRepository, ImportRepository, MessageRepository,
    OcrEngine, PositionRepository, SecretStore, SendRecordRepository,
};

/// Configuration that shapes responses, independent of the transport that
/// delivered it. Formerly duplicated as `ica_api::AppConfig` and
/// `ica_desktop::DesktopConfig`; both names now alias this one struct.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Reported by `GET /health` as `app` (`settings.app_name` in Python).
    pub app_name: String,
    /// Reported by `GET /health` as `ai_enabled`.
    pub ai_enabled: bool,
    /// The only directory the OCR use case will read an image from.
    pub ocr_upload_dir: std::path::PathBuf,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub ai_include_candidate_name: bool,
    pub database_url: String,
    pub backup_dir: String,
    pub backup_keep_count: i64,
    pub data_retention_days: i64,
    pub ai_max_tokens: i64,
    pub ai_timeout: i64,
    pub ai_temperature: f64,
    pub ai_daily_budget_cents: i64,
    pub ai_budget_warning_threshold: i64,
    pub database_path: String,
    /// W6 auth/JWT parameters (defaults match `app/config.py`).
    pub jwt_secret: String,
    pub jwt_access_token_expire_minutes: i64,
    pub jwt_refresh_token_expire_days: i64,
    pub bcrypt_cost: u32,
}

/// The explicit init/destroy lifecycle of the one `AppRuntime` (ARCH-P3 AC3).
/// Shared by every clone and all transports; `ica-application` stays free of
/// tokio/sqlx/axum/hyper (std locks only). The desktop assembles the runtime
/// through `AppRuntime::new` and reaches it through `runtime()`; shutdown
/// calls `begin_drain()` before the Sidecar Manager stops child processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePhase {
    Uninitialized,
    Ready,
    Draining,
    Stopped,
}

impl RuntimePhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uninitialized => "uninitialized",
            Self::Ready => "ready",
            Self::Draining => "draining",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Clone)]
pub struct RuntimeLifecycle {
    phase: Arc<RwLock<RuntimePhase>>,
    transitions: Arc<RwLock<Vec<&'static str>>>,
}

impl RuntimeLifecycle {
    pub fn new() -> Self {
        Self {
            phase: Arc::new(RwLock::new(RuntimePhase::Uninitialized)),
            transitions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn phase(&self) -> RuntimePhase {
        *self.phase.read().expect("runtime lifecycle lock")
    }

    pub fn transitions(&self) -> Vec<&'static str> {
        self.transitions
            .read()
            .expect("runtime lifecycle lock")
            .clone()
    }

    fn transition(&self, from: RuntimePhase, to: RuntimePhase, label: &'static str) -> bool {
        let mut phase = self.phase.write().expect("runtime lifecycle lock");
        if *phase != from {
            return false;
        }
        *phase = to;
        self.transitions
            .write()
            .expect("runtime lifecycle lock")
            .push(label);
        true
    }

    /// Called at the end of `AppRuntime::new`, once every port is assembled.
    pub fn mark_ready(&self) -> bool {
        self.transition(
            RuntimePhase::Uninitialized,
            RuntimePhase::Ready,
            "initialized",
        )
    }

    /// Stop taking new work; the Sidecar Manager drains children after this.
    pub fn begin_drain(&self) -> bool {
        self.transition(RuntimePhase::Ready, RuntimePhase::Draining, "drain")
    }

    /// Final destroy step, after all children are reaped.
    pub fn mark_stopped(&self) -> bool {
        self.transition(RuntimePhase::Draining, RuntimePhase::Stopped, "stopped")
    }
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

/// One runtime's Application Services. Clone shares the same port objects and
/// the same process-local locks; it never clones a database or an adapter.
#[derive(Clone)]
pub struct AppRuntime {
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
    ai_key_present: Arc<RwLock<bool>>,
    ai_budget: Arc<RwLock<crate::AiBudgetSettings>>,
    export_sealer: Arc<dyn ExportSealer>,
    ocr: Arc<dyn OcrEngine>,
    config: RuntimeConfig,
    lifecycle: RuntimeLifecycle,
}

impl AppRuntime {
    /// Assemble the services from injected ports. The adapters call this once
    /// during startup; the argument order is the historical `AppState::new`
    /// order, so no call site has to be rewritten for the unification.
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
        ai_key_present: Arc<RwLock<bool>>,
        ai_budget: Arc<RwLock<crate::AiBudgetSettings>>,
        export_sealer: Arc<dyn ExportSealer>,
        ocr: Arc<dyn OcrEngine>,
        config: RuntimeConfig,
    ) -> Self {
        let lifecycle = RuntimeLifecycle::new();
        let runtime = Self {
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
            lifecycle: lifecycle.clone(),
        };
        lifecycle.mark_ready();
        runtime
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
    pub fn imports(&self) -> &dyn ImportRepository {
        self.imports.as_ref()
    }
    pub fn extractions(&self) -> &dyn BrowserExtractionRepository {
        self.extractions.as_ref()
    }
    pub fn snapshots(&self) -> &dyn DataSnapshotRepository {
        self.snapshots.as_ref()
    }
    pub fn audit_trail(&self) -> &dyn AuditTrailRepository {
        self.audit_trail.as_ref()
    }
    pub fn settings(&self) -> &dyn AppSettingsRepository {
        self.settings.as_ref()
    }
    pub fn ai_models(&self) -> &dyn AiModelRepository {
        self.ai_models.as_ref()
    }
    pub fn ai_usage(&self) -> &dyn AiUsageRepository {
        self.ai_usage.as_ref()
    }
    pub fn secrets(&self) -> &dyn SecretStore {
        self.secrets.as_ref()
    }
    pub fn backups(&self) -> &dyn BackupStore {
        self.backups.as_ref()
    }
    pub fn auth_repo(&self) -> &dyn AuthRepository {
        self.auth_repo.as_ref()
    }
    pub fn auth_security(&self) -> &dyn AuthSecurity {
        self.auth_security.as_ref()
    }
    pub fn export_sealer(&self) -> &dyn ExportSealer {
        self.export_sealer.as_ref()
    }
    pub fn ocr(&self) -> &dyn OcrEngine {
        self.ocr.as_ref()
    }

    pub fn ai_key_present(&self) -> bool {
        *self.ai_key_present.read().expect("ai key lock")
    }
    pub fn set_ai_key_present(&self, present: bool) {
        *self.ai_key_present.write().expect("ai key lock") = present;
    }
    pub fn ai_budget(&self) -> crate::AiBudgetSettings {
        *self.ai_budget.read().expect("ai budget lock")
    }
    pub fn set_ai_budget(&self, budget: crate::AiBudgetSettings) {
        *self.ai_budget.write().expect("ai budget lock") = budget;
    }

    /// The AI registry fallback view, assembled from injected config only.
    pub fn ai_settings_view(&self) -> crate::AiSettingsView {
        crate::AiSettingsView {
            ai_enabled: self.ai_key_present(),
            ai_provider: self.config.ai_provider.clone(),
            ai_model: self.config.ai_model.clone(),
            ai_base_url: self.config.ai_base_url.clone(),
            ai_max_tokens: self.config.ai_max_tokens,
            ai_timeout: self.config.ai_timeout,
            ai_temperature: self.config.ai_temperature,
        }
    }

    /// The settings echo view, assembled from injected config only.
    pub fn settings_config_view(&self) -> crate::SettingsConfigView {
        crate::SettingsConfigView {
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

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// The shared init/destroy state; clones observe the same phase.
    pub fn lifecycle(&self) -> &RuntimeLifecycle {
        &self.lifecycle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_is_send_sync_and_cloneable() {
        // The shared runtime must be shareable across HTTP tasks and IPC
        // command invocations without copying any adapter state.
        fn assert_send_sync_clone<T: Send + Sync + Clone>() {}
        assert_send_sync_clone::<AppRuntime>();
    }

    #[test]
    fn lifecycle_moves_ready_drain_stopped_and_is_shared_by_clones() {
        let lifecycle = RuntimeLifecycle::new();
        assert_eq!(lifecycle.phase(), RuntimePhase::Uninitialized);
        // A second transition out of Uninitialized is rejected: no double init.
        assert!(!lifecycle.begin_drain());
        assert!(lifecycle.mark_ready());
        let clone = lifecycle.clone();
        assert!(!lifecycle.mark_ready());
        assert!(clone.begin_drain());
        assert_eq!(lifecycle.phase(), RuntimePhase::Draining);
        assert!(lifecycle.mark_stopped());
        assert_eq!(clone.phase(), RuntimePhase::Stopped);
        assert_eq!(
            lifecycle.transitions(),
            vec!["initialized", "drain", "stopped"]
        );
    }

    #[test]
    fn both_adapters_wrap_the_same_application_runtime() {
        // Source-level assertion: neither adapter keeps its own port-object
        // assembly. This runs on hosts without GTK/WebKit, where the Tauri
        // host itself cannot be type-checked.
        const HTTP: &str = include_str!("../../ica-api/src/state.rs");
        const IPC: &str = include_str!("../../ica-desktop/src/state.rs");
        for source in [HTTP, IPC] {
            assert!(
                source.contains("AppRuntime"),
                "adapter must wrap AppRuntime"
            );
            assert!(
                source.contains("fn runtime("),
                "adapter must expose runtime()"
            );
            assert!(
                source.contains("AppRuntime::new("),
                "adapter must assemble services through AppRuntime::new"
            );
        }
        assert!(HTTP.contains("pub type AppConfig = ica_application::RuntimeConfig"));
        assert!(IPC.contains("pub type DesktopConfig = ica_application::RuntimeConfig"));
    }
}
