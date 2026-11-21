//! Smoke tests for the IPC command layer: dispatch real commands through
//! `DesktopState` and assert the contract shapes — including that `IpcError`
//! serialises to the same `{"error": …}` envelope the HTTP side emits.

use ica_desktop::{commands, DesktopConfig, DesktopState};
use serde_json::json;

/// A throwaway database with the baseline schema, wired into a `DesktopState`.
async fn desktop_app() -> (DesktopState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    let pool = ica_infra::connect(&format!("sqlite:///{}?mode=rwc", path.display()))
        .await
        .expect("open temp database");
    sqlx::raw_sql(include_str!("../../ica-infra/schema/ica-baseline.sql"))
        .execute(&pool)
        .await
        .expect("apply baseline schema");
    pool.close().await;

    let state = DesktopState::connect(
        &format!("sqlite:///{}", path.display()),
        DesktopConfig {
            app_name: "ICA Personal".to_string(),
            ai_enabled: false,
            // A directory inside the throwaway temp dir, so the OCR path guard
            // has a real root to resolve against.
            ocr_upload_dir: dir.path().join("ocr"),
            ai_provider: "openai_compatible".to_string(),
            ai_model: "glm-4-flash".to_string(),
            ai_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            ai_include_candidate_name: false,
            database_url: format!("sqlite:///{}", path.display()),
            backup_dir: dir.path().join("backups").display().to_string(),
            backup_keep_count: 10,
            data_retention_days: 180,
            ai_max_tokens: 1500,
            ai_timeout: 60,
            ai_temperature: 0.4,
            ai_daily_budget_cents: 0,
            ai_budget_warning_threshold: 90,
            database_path: format!("{}", path.display()),
            jwt_secret: String::new(),
            jwt_access_token_expire_minutes: 15,
            jwt_refresh_token_expire_days: 7,
            bcrypt_cost: 4,
        },
    )
    .await
    .expect("assemble desktop state");
    (state, dir)
}

/// The health command reflects the injected config, not the environment
/// (Tauri-audit G-4: the host owns configuration).
#[tokio::test]
async fn health_reports_the_injected_config() {
    let (state, _dir) = desktop_app().await;

    let health = commands::health(&state).await.expect("health");

    assert_eq!(health.status, "ok");
    assert_eq!(health.app, "ICA Personal");
    assert!(!health.ai_enabled);
}

/// Create → read through the command layer, exercising the shared body
/// validators (same JSON shape the HTTP side parses).
#[tokio::test]
async fn positions_create_and_get_round_trip() {
    let (state, _dir) = desktop_app().await;

    let payload = json!({"name": "Desktop Engineer", "location": "Wuhan", "status": "Open"});
    let created = commands::positions_create(&state, &payload)
        .await
        .expect("create");
    assert_eq!(created.name, "Desktop Engineer");
    assert_eq!(created.status, "Open");

    let id = created.id.clone();
    let fetched = commands::positions_get(&state, &id).await.expect("get");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.candidate_count, 0);

    // The list command carries the real total through the typed page window.
    let page = commands::positions_list(&state, commands::PositionListArgs::default())
        .await
        .expect("list");
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);
}

/// An unknown id renders the contract error as the IPC envelope — the same
/// code/message the HTTP side emits, no status code involved.
#[tokio::test]
async fn an_unknown_position_renders_the_ipc_error_envelope() {
    let (state, _dir) = desktop_app().await;

    let error = commands::positions_get(&state, "does-not-exist")
        .await
        .expect_err("expected a not_found");

    let rendered = serde_json::to_value(&error).expect("serialise IpcError");
    assert_eq!(
        rendered,
        json!({
            "error": {
                "code": "not_found",
                "message": "Position 'does-not-exist' was not found",
                "details": {},
            }
        })
    );
}

/// The follow-up policy command creates the default row on first use, and the
/// typed page-window checks keep the baseline's ranges (`limit = 0` rejects).
#[tokio::test]
async fn policy_defaults_and_typed_page_ranges_hold() {
    let (state, _dir) = desktop_app().await;

    let policy = commands::follow_ups_policy_get(&state)
        .await
        .expect("policy");
    assert_eq!(policy.first_follow_up_delay_hours, 24);
    assert_eq!(policy.max_active_follow_ups, 2);

    let error = commands::follow_ups_list(
        &state,
        commands::FollowUpListArgs {
            limit: Some(0),
            ..Default::default()
        },
    )
    .await
    .expect_err("limit = 0 must reject");
    assert_eq!(error.error.code, "validation_error");
    assert_eq!(error.error.message, "Request payload failed validation");
}

/// W5 (first slice): the audit-trail command mirrors the HTTP contract — a
/// bare array newest first, a business write lands in it, and the typed page
/// window keeps the baseline's bounds.
#[tokio::test]
async fn audit_events_list_mirrors_the_http_contract() {
    let (state, dir) = desktop_app().await;

    let empty = commands::audit_events_list(&state, commands::AuditEventsListArgs::default())
        .await
        .expect("empty trail");
    assert!(empty.is_empty());

    // A business write through the command layer lands in the trail, newest
    // first.
    let payload = json!({"name": "Trail Probe"});
    commands::positions_create(&state, &payload)
        .await
        .expect("create");
    let trail = commands::audit_events_list(
        &state,
        commands::AuditEventsListArgs {
            event_type: Some("position.created".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("trail");
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].event_type, "position.created");
    assert_eq!(trail[0].actor, "local_user");
    assert!(trail[0].metadata.is_object());

    // `limit = 0` rejects with the same envelope the HTTP side emits.
    let error = commands::audit_events_list(
        &state,
        commands::AuditEventsListArgs {
            limit: Some(0),
            ..Default::default()
        },
    )
    .await
    .expect_err("limit = 0 must reject");
    assert_eq!(error.error.code, "validation_error");

    // The event row was written with the local actor and default tenant, so
    // reading it back through the pool confirms the write path end to end.
    let pool = ica_infra::connect(&format!(
        "sqlite:///{}",
        dir.path().join("ica.db").display()
    ))
    .await
    .expect("reopen");
    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_events")
        .fetch_one(&pool)
        .await
        .expect("count audit rows");
    assert_eq!(total, 1);
}

/// W5 settings slice: the config echo and the AI-egress switch through the
/// command layer, with the audit cross-check through the trail command.
#[tokio::test]
async fn settings_and_ai_egress_mirrors_the_http_contract() {
    let (state, _dir) = desktop_app().await;

    let settings = commands::settings_read(&state).await.expect("settings");
    assert_eq!(settings.app_name, "ICA Personal");
    assert_eq!(settings.ai_provider, "openai_compatible");
    assert_eq!(settings.ai_model, "glm-4-flash");
    assert_eq!(settings.backup_keep_count, 10);
    assert_eq!(settings.data_retention_days, 180);

    // Consent defaults to granted when nothing is recorded.
    let disclosure = commands::ai_egress_read(&state).await.expect("egress");
    assert!(disclosure.consent_granted);
    assert_eq!(disclosure.masked_fields.len(), 5);
    assert_eq!(disclosure.masked_fields[0], "name");

    // Revoke: the fresh disclosure reflects the switch, and the trail command
    // sees the audited change.
    let revoked = commands::ai_egress_consent(
        &state,
        commands::AiEgressConsentArgs {
            consent_granted: Some(false),
        },
    )
    .await
    .expect("revoke");
    assert!(!revoked.consent_granted);

    let trail = commands::audit_events_list(
        &state,
        commands::AuditEventsListArgs {
            event_type: Some("ai.egress_consent.revoked".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("trail");
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entity_type.as_deref(), Some("setting"));
    assert_eq!(trail[0].entity_id.as_deref(), Some("ai.egress_consent"));

    // An omitted switch is the same missing-field rejection the HTTP body
    // contract emits.
    let error = commands::ai_egress_consent(&state, commands::AiEgressConsentArgs::default())
        .await
        .expect_err("the switch is required");
    assert_eq!(error.error.code, "validation_error");
}
