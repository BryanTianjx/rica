//! The AI-key credential use cases (W5 ai-key, project ruling: OS
//! credential store).
//!
//! Baseline: `app/api/settings.py::write_ai_key` / `clear_ai_key` +
//! `app/ai/registry.py::resolve_api_key`. The key is write-only: responses
//! carry the enabled flag and a masked tail, never the key; every change is
//! audited in masked form; a legacy plaintext `.env` line is stripped when
//! the host tells the adapter where that file lives (the desktop host owns
//! paths — Tauri-audit G-4).

use crate::error::ApplicationError;
use crate::ports::{AppSettingsRepository, SecretStore};

/// The installation-wide credential slot (`secret_store.AI_API_KEY`).
pub const AI_KEY_SLOT: &str = "ai_api_key";

/// The per-model slot prefix (`secret_store.AI_KEY_SLOT_PREFIX`).
pub const AI_KEY_SLOT_PREFIX: &str = "ai_key:";

/// The audit event types, exported for trail cross-checks.
pub const AI_KEY_CONFIGURED_EVENT: &str = "ai.key_configured";
pub const AI_KEY_CLEARED_EVENT: &str = "ai.key_cleared";

/// The masked tail for status display (`_mask_key`): `…` + the last four
/// characters, never the full key.
pub fn mask_key(key: &str) -> Option<String> {
    let stripped = key.trim();
    if stripped.is_empty() {
        None
    } else {
        let tail: String = stripped
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Some(format!("…{tail}"))
    }
}

/// The credential slot for one credential choice: a `model_id` targets that
/// model's own slot; anything falsy targets the installation-wide slot.
pub fn slot_for(model_id: Option<&str>) -> String {
    match model_id {
        Some(model_id) if !model_id.is_empty() => {
            format!("{AI_KEY_SLOT_PREFIX}{}", percent_encode_reserved(model_id))
        }
        _ => AI_KEY_SLOT.to_string(),
    }
}

/// `urllib.parse.quote(model_id, safe='')` — every reserved byte becomes
/// `%XX` (uppercase hex), so the slot stays usable as a file name and a
/// Keychain account on every platform.
fn percent_encode_reserved(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let unreserved = byte.is_ascii_alphanumeric()
            || byte == b'-'
            || byte == b'_'
            || byte == b'.'
            || byte == b'~';
        if unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// `PUT /settings/ai-key` — persist the key in the chosen slot, strip the
/// legacy plaintext line, audit the change, and report the masked status.
pub async fn write_ai_key(
    secrets: &dyn SecretStore,
    settings: &dyn AppSettingsRepository,
    ai_api_key: &str,
    model_id: Option<&str>,
) -> Result<(bool, Option<String>, Option<String>), ApplicationError> {
    let key = ai_api_key.trim();
    if key.is_empty() {
        // The baseline's ValidationError with a code: a 422 whose message is
        // contract text, telling the caller how to clear instead.
        return Err(ApplicationError::ValidationWithCode {
            code: "ai_key_empty",
            message: "AI_API_KEY must not be empty; use DELETE /settings/ai-key to clear it."
                .to_string(),
            details: serde_json::json!({}),
        });
    }
    let slot = slot_for(model_id);
    secrets
        .store(&slot, key)
        .await
        .map_err(|error| error.application_error())?;
    let masked = mask_key(key);
    let summary = match (model_id, &masked) {
        (Some(model_id), Some(masked)) if !model_id.is_empty() => {
            format!("AI API key configured for model '{model_id}' (masked: {masked})")
        }
        (_, Some(masked)) => format!("AI API key configured (masked: {masked})"),
        (_, None) => "AI API key configured".to_string(),
    };
    let metadata =
        serde_json::json!({ "key_masked": masked, "model_id": model_id.filter(|m| !m.is_empty()) });
    settings
        .record_ai_key_audit(AI_KEY_CONFIGURED_EVENT, summary, metadata)
        .await
        .map_err(|error| error.application_error())?;
    // Only the installation-wide write flips the runtime flag (the model
    // branch leaves the V1.0 key alone).
    let installation = model_id.is_none() || model_id == Some("");
    Ok((installation, masked, model_id.map(str::to_string)))
}

/// `DELETE /settings/ai-key` — clear the chosen slot and audit it.
pub async fn clear_ai_key(
    secrets: &dyn SecretStore,
    settings: &dyn AppSettingsRepository,
    model_id: Option<&str>,
) -> Result<(bool, Option<String>, Option<String>), ApplicationError> {
    let slot = slot_for(model_id);
    secrets
        .delete(&slot)
        .await
        .map_err(|error| error.application_error())?;
    match model_id.filter(|m| !m.is_empty()) {
        Some(model_id) => {
            settings
                .record_ai_key_audit(
                    AI_KEY_CLEARED_EVENT,
                    format!("AI API key cleared for model '{model_id}'"),
                    serde_json::json!({ "model_id": model_id }),
                )
                .await
                .map_err(|error| error.application_error())?;
            Ok((false, None, Some(model_id.to_string())))
        }
        None => {
            settings
                .record_ai_key_audit(
                    AI_KEY_CLEARED_EVENT,
                    "AI API key cleared".to_string(),
                    serde_json::json!({}),
                )
                .await
                .map_err(|error| error.application_error())?;
            // The installation-wide key was cleared: the runtime flag goes off.
            Ok((true, None, None))
        }
    }
}

/// Key presence for the registry (`resolve_api_key`, minus the deferred
/// credential-store model slots): the model's env var, then the V1.0 slot,
/// then the installation-wide env key.
pub fn api_key_resolved(
    secrets: &dyn SecretStore,
    api_key_env: &str,
    model_id: &str,
    v1_key_present: bool,
) -> bool {
    let _ = secrets;
    if !api_key_env.is_empty()
        && std::env::var(api_key_env)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    {
        return true;
    }
    // The model-slot credential lookup belongs to the keyring-backed store;
    // the synchronous resolution here only covers the env paths.
    let _ = model_id;
    v1_key_present
}

/// The three-level key resolution (`app/ai/registry.py::resolve_api_key`):
/// the model's own credential-store slot → the `api_key_env` environment
/// variable → the installation-wide `AI_API_KEY` slot. `None` means "not
/// configured" and the caller answers the baseline's 503.
///
/// A broken store must not block resolution: the adapter degrades an
/// unreadable slot to absent and the fall-through continues (the baseline's
/// `load_secret`).
pub async fn resolve_model_api_key(
    secrets: &dyn SecretStore,
    api_key_env: &str,
    model_id: &str,
) -> Result<Option<String>, crate::ports::RepositoryError> {
    let slot_value = secrets.load(&slot_for(Some(model_id))).await?;
    if let Some(value) = slot_value {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }

    if !api_key_env.is_empty() {
        let from_env = std::env::var(api_key_env).unwrap_or_default();
        let trimmed = from_env.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }

    let installation = secrets.load(AI_KEY_SLOT).await?;
    Ok(installation
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}
