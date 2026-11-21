//! `GET /settings` + the AI-egress pair (W5 settings slice).
//!
//! The contract is the baseline's `read_settings` (config echo, minus any
//! secret), `read_ai_egress` (`PRIV-07` disclosure, consent defaults to
//! **true**) and `write_ai_egress_consent` (required explicit switch,
//! audited in the same transaction as the row).

mod support;

use support::{get, pool_for, post, w1_app};

#[tokio::test]
async fn settings_echoes_the_effective_configuration_without_secrets() {
    let (app, dir) = w1_app().await;
    let response = get(app, "/api/settings").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(
        response.body,
        serde_json::json!({
            "app_name": "ICA Personal",
            "ai_enabled": false,
            "ai_provider": "openai_compatible",
            "ai_model": "glm-4-flash",
            "ai_base_url": "https://open.bigmodel.cn/api/paas/v4",
            "database_url": format!("sqlite:///{}", dir.path().join("ica.db").display()),
            "backup_dir": format!("{}", dir.path().join("backups").display()),
            "backup_keep_count": 10,
            "data_retention_days": 180,
        }),
        "the echo is exact and carries no key material: {}",
        response.body
    );
}

#[tokio::test]
async fn ai_egress_discloses_the_pipeline_and_defaults_to_consent_granted() {
    let (app, _dir) = w1_app().await;
    let response = get(app, "/api/settings/ai-egress").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(
        response.body,
        serde_json::json!({
            "enabled": false,
            "consent_granted": true,
            "provider": "openai_compatible",
            "model": "glm-4-flash",
            "base_url": "https://open.bigmodel.cn/api/paas/v4",
            "masked_fields": ["name", "phone", "email", "id_number", "card_number"],
            "include_candidate_name": false,
            "notice": "生成草稿时，只有下方列出的脱敏后上下文会发送至所配置的第三方 AI 供应商；本次会话的聊天记录、草稿和联系方式不会随之外发。姓名默认以 [NAME] 占位符代替。",
        }),
        "the disclosure is the baseline's PRIV-07 shape: {}",
        response.body
    );
}

#[tokio::test]
async fn revoking_consent_persists_the_switch_and_audits_it() {
    let (app, dir) = w1_app().await;
    let response = post(
        app.clone(),
        "/api/settings/ai-egress/consent",
        r#"{"consent_granted":false}"#,
    )
    .await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(response.body["consent_granted"], false);
    assert_eq!(response.body["masked_fields"][0], "name");

    // The switch survives in `app_settings`.
    let pool = pool_for(&dir).await;
    let (value, value_type): (String, String) = sqlx::query_as(
        "SELECT value, value_type FROM app_settings WHERE \"key\" = 'ai.egress_consent'",
    )
    .fetch_one(&pool)
    .await
    .expect("the consent row");
    assert_eq!((value.as_str(), value_type.as_str()), ("false", "bool"));

    // The change is audited — and the trail cross-checks it (W5 first slice).
    let trail = get(
        app,
        "/api/audit-events?event_type=ai.egress_consent.revoked",
    )
    .await;
    let items = trail.body.as_array().expect("a bare array");
    assert_eq!(items.len(), 1, "the revocation is audited: {}", trail.body);
    assert_eq!(items[0]["entity_type"], "setting");
    assert_eq!(items[0]["entity_id"], "ai.egress_consent");
    assert_eq!(items[0]["metadata"]["consent_granted"], false);
}

#[tokio::test]
async fn granting_again_flips_the_switch_back_and_audits_the_grant() {
    let (app, dir) = w1_app().await;
    let revoke = post(
        app.clone(),
        "/api/settings/ai-egress/consent",
        r#"{"consent_granted":false}"#,
    )
    .await;
    assert_eq!(revoke.status, 200);

    let grant = post(
        app.clone(),
        "/api/settings/ai-egress/consent",
        r#"{"consent_granted":true}"#,
    )
    .await;
    assert_eq!(grant.status, 200);
    assert_eq!(grant.body["consent_granted"], true);

    let pool = pool_for(&dir).await;
    let (value,): (String,) =
        sqlx::query_as("SELECT value FROM app_settings WHERE \"key\" = 'ai.egress_consent'")
            .fetch_one(&pool)
            .await
            .expect("the consent row");
    assert_eq!(value, "true");

    let trail = get(
        app,
        "/api/audit-events?event_type=ai.egress_consent.granted",
    )
    .await;
    let items = trail.body.as_array().expect("a bare array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["metadata"]["consent_granted"], true);
}

#[tokio::test]
async fn an_omitted_or_null_switch_is_a_pydantic_shaped_422() {
    let (app, _dir) = w1_app().await;

    let missing = post(app.clone(), "/api/settings/ai-egress/consent", "{}").await;
    assert_eq!(missing.status, 422, "body: {}", missing.body);
    let issue = &missing.body["error"]["details"]["errors"][0];
    assert_eq!(issue["type"], "missing");
    assert_eq!(issue["loc"], serde_json::json!(["body", "consent_granted"]));

    let null = post(
        app,
        "/api/settings/ai-egress/consent",
        r#"{"consent_granted":null}"#,
    )
    .await;
    assert_eq!(null.status, 422, "body: {}", null.body);
    let issue = &null.body["error"]["details"]["errors"][0];
    assert_eq!(issue["type"], "bool_type");
    assert_eq!(issue["msg"], "Input should be a valid boolean");
}
