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
