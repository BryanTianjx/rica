//! Shared application state handed to every handler.

use std::sync::Arc;

use ica_application::{
    CandidateRepository, ConversationRepository, DashboardRepository, DataSnapshotRepository,
    DraftGenerator, DraftRepository, ExportSealer, FollowUpRepository, ImportRepository,
    MessageRepository, OcrEngine, PositionRepository, SendRecordRepository,
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
    snapshots: Arc<dyn DataSnapshotRepository>,
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
        snapshots: Arc<dyn DataSnapshotRepository>,
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
            snapshots,
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

    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}
