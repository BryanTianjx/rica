//! The desktop GUI must be able to tell that an AI key is configured.
//!
//! Two defects hid behind the same symptom ("API key 不可用" after the key
//! was saved):
//!
//! 1. `GET /api/settings` reported the *config flag* only, so the UI kept
//!    saying "not configured" even with a key in the credential store — and
//!    the flag is empty after every restart.
//! 2. The credential store itself never persisted anything (the platform
//!    backend was missing). That one is guarded in
//!    `crates/ica-infra/tests/secret_store_backend.rs`; this suite covers the
//!    HTTP surface the GUI reads.

mod support;

use serde_json::json;
use support::{
    app_for_mode, get_with_headers, post_with_headers, put_with_headers, seeded_database,
};

#[tokio::test]
async fn a_configured_key_is_reported_by_settings_and_models() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    seeded_database(&path).await;
    let app = app_for_mode(&path, ica_api::auth::AuthMode::Local).await;

    let before = get_with_headers(app.clone(), "/api/settings", &[]).await;
    assert_eq!(before.status, 200, "{}", before.body);
    assert_eq!(
        before.body["ai_enabled"], false,
        "no key is configured yet: {}",
        before.body
    );

    let written = put_with_headers(
        app.clone(),
        "/api/settings/ai-key",
        &json!({"ai_api_key": "sk-regression-dummy"}).to_string(),
        &[],
    )
    .await;
    assert_eq!(written.status, 200, "{}", written.body);
    assert_eq!(written.body["ai_enabled"], true, "{}", written.body);

    // The GUI reads the settings endpoint for the installation-wide state.
    let after = get_with_headers(app.clone(), "/api/settings", &[]).await;
    assert_eq!(after.status, 200, "{}", after.body);
    assert_eq!(
        after.body["ai_enabled"], true,
        "a configured key must be reported: {}",
        after.body
    );

    // …and the model list for the per-model state.
    let models = get_with_headers(app.clone(), "/api/ai/models", &[]).await;
    assert_eq!(models.status, 200, "{}", models.body);
    assert_eq!(
        models.body["models"][0]["api_key_configured"], true,
        "the model must see the installation key: {}",
        models.body
    );

    // A draft request must get past key resolution (it fails at the provider
    // instead, because the dummy key is not a real one).
    let position = post_with_headers(
        app.clone(),
        "/api/positions",
        &json!({"name": "Regression Position"}).to_string(),
        &[],
    )
    .await;
    let position_id = position.body["id"].as_str().unwrap_or_default().to_string();
    let candidate = post_with_headers(
        app.clone(),
        "/api/candidates",
        &json!({"name": "Regression Candidate", "position_id": position_id}).to_string(),
        &[],
    )
    .await;
    let candidate_id = candidate.body["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let generated = post_with_headers(
        app.clone(),
        "/api/drafts/generate",
        &json!({"candidate_id": candidate_id, "message_type": "greeting", "tone": "formal"})
            .to_string(),
        &[],
    )
    .await;
    let message = generated.body["error"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !message.contains("No API key found"),
        "the configured key must be found by the generator: {message}"
    );
}
