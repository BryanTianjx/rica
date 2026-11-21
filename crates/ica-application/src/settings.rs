//! Settings use cases (W5): the configuration echo and the AI-egress switch.
//!
//! The baseline's `app/api/settings.py` + `app/application/ai_consent.py`:
//! `GET /settings` reports the effective configuration minus any secret, and
//! the AI-egress pair discloses where candidate context would go (`PRIV-07`)
//! and lets the operator stop it without deleting the key. The switch lives
//! in `app_settings` (key `ai.egress_consent`), its changes are audited in
//! the same transaction, and every provider call later emits an `ai.egress`
//! event — so a send is a fact that can be checked, not a claim.

use crate::error::ApplicationError;
use crate::ports::AppSettingsRepository;
use ica_domain::AuditEventFilter;

/// `app_settings` row key for the outbound-AI switch.
pub const AI_EGRESS_CONSENT_KEY: &str = "ai.egress_consent";

/// The categories the context builder masks before text leaves the process.
/// Kept as constants (not derived from a live context) so the disclosure can
/// state the guarantee even when no candidate is open.
pub const MASKED_CATEGORIES: [&str; 5] = ["name", "phone", "email", "id_number", "card_number"];

/// The disclosure notice, verbatim from the baseline.
pub const EGRESS_NOTICE: &str = "生成草稿时，只有下方列出的脱敏后上下文会发送至所配置的第三方 AI 供应商；本次会话的聊天记录、草稿和联系方式不会随之外发。姓名默认以 [NAME] 占位符代替。";

/// The effective configuration, minus any secret. The api/desktop layers
/// build this from their own config object; the use cases only carry it.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsConfigView {
    pub app_name: String,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub database_url: String,
    pub backup_dir: String,
    pub backup_keep_count: i64,
    pub data_retention_days: i64,
    pub ai_include_candidate_name: bool,
}

impl SettingsConfigView {
    /// `GET /settings` — the wire shape minus masked_fields/notice.
    pub fn settings_snapshot(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            app_name: self.app_name.clone(),
            ai_enabled: self.ai_enabled,
            ai_provider: self.ai_provider.clone(),
            ai_model: self.ai_model.clone(),
            ai_base_url: self.ai_base_url.clone(),
            database_url: self.database_url.clone(),
            backup_dir: self.backup_dir.clone(),
            backup_keep_count: self.backup_keep_count,
            data_retention_days: self.data_retention_days,
        }
    }
}

/// `GET /settings` payload (pure config echo — no repository involved).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub app_name: String,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub database_url: String,
    pub backup_dir: String,
    pub backup_keep_count: i64,
    pub data_retention_days: i64,
}

/// `AIEgressRead` payload — everything the UI needs to tell the user where
/// the data goes.
#[derive(Debug, Clone, PartialEq)]
pub struct EgressDisclosure {
    pub enabled: bool,
    pub consent_granted: bool,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub masked_fields: Vec<String>,
    pub include_candidate_name: bool,
    pub notice: String,
}

/// `GET /settings/ai-egress` — read the switch and disclose the pipeline.
pub async fn read_egress_disclosure(
    settings: &dyn AppSettingsRepository,
    view: &SettingsConfigView,
) -> Result<EgressDisclosure, ApplicationError> {
    let consent_granted = settings
        .egress_consent_granted()
        .await
        .map_err(|error| error.application_error())?;
    Ok(EgressDisclosure {
        enabled: view.ai_enabled,
        consent_granted,
        provider: view.ai_provider.clone(),
        model: view.ai_model.clone(),
        base_url: view.ai_base_url.clone(),
        masked_fields: MASKED_CATEGORIES
            .iter()
            .map(|category| (*category).to_string())
            .collect(),
        include_candidate_name: view.ai_include_candidate_name,
        notice: EGRESS_NOTICE.to_string(),
    })
}

/// `POST /settings/ai-egress/consent` — persist the switch (with its audit
/// event, inside the adapter's transaction) and return the fresh disclosure.
///
/// A revocation takes effect immediately for every later generation: the
/// service refuses before building a context, so nothing is assembled for a
/// provider that is no longer authorised. The switch is process-wide, which
/// matches ICA's single-operator local-first deployment.
pub async fn write_egress_consent(
    settings: &dyn AppSettingsRepository,
    view: &SettingsConfigView,
    consent_granted: bool,
) -> Result<EgressDisclosure, ApplicationError> {
    settings
        .set_egress_consent(consent_granted)
        .await
        .map_err(|error| error.application_error())?;
    let mut disclosure = read_egress_disclosure(settings, view).await?;
    // The read-back must reflect the write that just happened (both sides hit
    // the same store, so this only guards against a future cache).
    disclosure.consent_granted = consent_granted;
    Ok(disclosure)
}

/// The audit-trail keys the egress switch writes, for callers that want to
/// cross-check the trail (`GET /audit-events?event_type=…`).
pub const EGRESS_GRANTED_EVENT: &str = "ai.egress_consent.granted";
pub const EGRESS_REVOKED_EVENT: &str = "ai.egress_consent.revoked";

/// The `AuditEventFilter` an audit-trail check for egress changes uses.
pub fn egress_event_filter() -> AuditEventFilter {
    AuditEventFilter {
        event_type: Some(EGRESS_GRANTED_EVENT.to_string()),
        entity_type: None,
        entity_id: None,
        limit: 50,
    }
}
