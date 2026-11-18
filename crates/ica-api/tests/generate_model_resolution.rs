//! `POST /api/drafts/generate` with a model the registry cannot resolve.
//!
//! The baseline resolves the requested model against its registry before it
//! looks for an API key (`app/ai/registry.py::get_model`,
//! `app/ai/service.py`): an unknown id is a 503 `ai_provider_error` naming it
//! in `details.model_id`. The port ignored `model_id` / `comparison_models`
//! entirely and always reported the default model's missing key, so a client
//! asking for `gpt-4` was told about `glm-4-flash`.
//!
//! `comparison_models` wins over `model_id`, and its ids resolve in order, so
//! the first unknown one is the one reported.

mod support;

use support::{post, w1_app};

fn generate_body(extra: &str) -> String {
    format!(r#"{{"candidate_id": "c-2"{extra}}}"#)
}

#[tokio::test]
async fn an_unknown_model_id_is_503_naming_that_model() {
    let (app, _guard) = w1_app().await;

    let response = post(
        app,
        "/api/drafts/generate",
        &generate_body(r#", "model_id": "gpt-4""#),
    )
    .await;

    assert_eq!(response.status, 503, "body: {}", response.body);
    assert_eq!(response.body["error"]["code"], "ai_provider_error");
    assert_eq!(
        response.body["error"]["message"],
        "Unknown model 'gpt-4'. No matching configuration found."
    );
    assert_eq!(response.body["error"]["details"]["model_id"], "gpt-4");
}

#[tokio::test]
async fn an_unknown_comparison_model_is_503_naming_the_first_unknown() {
    let (app, _guard) = w1_app().await;

    let response = post(
        app,
        "/api/drafts/generate",
        &generate_body(r#", "comparison_models": ["gpt-4", "gpt-4o"]"#),
    )
    .await;

    assert_eq!(response.status, 503, "body: {}", response.body);
    assert_eq!(
        response.body["error"]["message"],
        "Unknown model 'gpt-4'. No matching configuration found.",
        "the first unknown id is the one reported"
    );
    assert_eq!(response.body["error"]["details"]["model_id"], "gpt-4");
}

#[tokio::test]
async fn a_known_model_falls_through_to_the_missing_key() {
    let (app, _guard) = w1_app().await;

    let response = post(
        app,
        "/api/drafts/generate",
        &generate_body(r#", "model_id": "glm-4-flash""#),
    )
    .await;

    assert_eq!(response.status, 503, "body: {}", response.body);
    assert_eq!(response.body["error"]["code"], "ai_provider_error");
    assert_eq!(
        response.body["error"]["message"],
        "No API key found for model 'glm-4-flash' in env var 'AI_API_KEY'"
    );
    assert_eq!(
        response.body["error"]["details"],
        serde_json::json!({}),
        "the no-key error carries no details, so the envelope has {{}}"
    );
}
