//! Desktop adapter state: the same port-object assembly `ica-server` performs,
//! minus the HTTP server. The shell host calls [`DesktopState::connect`] with
//! the platform data directory's database path (Tauri-audit G-4: the host owns
//! the data directory — `%LOCALAPPDATA%\ICA\data` / `~/Library/Application
//! Support` — and injects the URL; the adapter never reads the environment).

use std::sync::Arc;

use ica_application::{
    CandidateRepository, ConversationRepository, DashboardRepository, DataSnapshotRepository,
    DraftGenerator, DraftRepository, ExportSealer, FollowUpRepository, ImportRepository,
    MessageRepository, OcrEngine, PositionRepository, SendRecordRepository,
};
use ica_infra::drafts::NotConfiguredDraftGenerator;
use ica_infra::export_crypto::AesGcmSealer;
use ica_infra::ocr::NotConfiguredOcrEngine;
use ica_infra::SqliteStore;

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
    snapshots: Arc<dyn DataSnapshotRepository>,
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
        Self {
            positions: Arc::new(store.clone()),
            candidates: Arc::new(store.clone()),
            conversations: Arc::new(store.clone()),
            messages: Arc::new(store.clone()),
            send_records: Arc::new(store.clone()),
            follow_ups: Arc::new(store.clone()),
            dashboard: Arc::new(store.clone()),
            drafts: Arc::new(store.clone()),
            // W3: no provider configured, so `generate` answers the baseline's
            // 503 `ai_provider_error`. W5 supplies the real provider.
            draft_generator: Arc::new(NotConfiguredDraftGenerator),
            // W4 ports. The same store serves the import and snapshot
            // repositories, so a write and its audit event share one
            // transaction.
            imports: Arc::new(store.clone()),
            snapshots: Arc::new(store.clone()),
            export_sealer: Arc::new(AesGcmSealer),
            ocr: Arc::new(NotConfiguredOcrEngine),
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

    pub fn imports(&self) -> &dyn ImportRepository {
        self.imports.as_ref()
    }

    pub fn snapshots(&self) -> &dyn DataSnapshotRepository {
        self.snapshots.as_ref()
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
