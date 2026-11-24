//! ARCH-P2 AC5 — Adapter Contract Test.
//!
//! The same inputs are fed to the HTTP adapter (axum router + Application
//! Services) and the IPC adapter (desktop commands over the SAME AppRuntime
//! type). Success bodies and error envelopes must be identical; HTTP status
//! is reported alongside the body for evidence, not compared as a value.
//!
//! Scope note: the Tauri host registers only the five positions commands, so
//! the executable parity set covers health, positions and the shared
//! Application-level guards (backup restore, shares) reachable through the
//! command layer. Other contract surfaces are asserted at the source level
//! (same use case calls, no adapter business rules), not executed over IPC.

mod support;

use axum::body::Body;
use axum::http::Request;
use axum::Router;
use ica_desktop::{commands, DesktopConfig, DesktopState};
use ica_infra::SqliteStore;
use serde_json::{json, Value};
use std::path::Path;
use tower::ServiceExt;

fn config_for(path: &Path) -> DesktopConfig {
    DesktopConfig {
        app_name: "ICA Personal".to_string(),
        ai_enabled: false,
        ocr_upload_dir: path.parent().unwrap_or(Path::new(".")).join("ocr-uploads"),
        ai_provider: "openai_compatible".to_string(),
        ai_model: "glm-4-flash".to_string(),
        ai_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
        ai_include_candidate_name: false,
        database_url: format!("sqlite:///{}", path.display()),
        backup_dir: path
            .parent()
            .unwrap_or(Path::new("."))
            .join("backups")
            .display()
            .to_string(),
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
    }
}

/// Two throwaway databases with the same baseline schema and fixture: one
/// behind the HTTP router, one behind the IPC command layer.
async fn paired_adapters() -> (Router, DesktopState, tempfile::TempDir, tempfile::TempDir) {
    let http_dir = tempfile::tempdir().expect("http temp dir");
    let http_path = http_dir.path().join("ica.db");
    support::seeded_database(&http_path).await;
    let app = support::app_for(&http_path).await;

    let ipc_dir = tempfile::tempdir().expect("ipc temp dir");
    let ipc_path = ipc_dir.path().join("ica.db");
    support::seeded_database(&ipc_path).await;
    let pool = ica_infra::connect(&format!("sqlite:///{}", ipc_path.display()))
        .await
        .expect("open ipc database");
    let state = DesktopState::from_store(SqliteStore::new(pool), config_for(&ipc_path));

    (app, state, http_dir, ipc_dir)
}

async fn http(app: Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let has_body = body.is_some();
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("accept", "application/json");
    if has_body {
        builder = builder.header("content-type", "application/json");
    }
    let request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .expect("build request");
    let response = app.oneshot(request).await.expect("router responded");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, value)
}

fn assert_parity(label: &str, http_status: u16, http_body: Value, ipc: Value) {
    assert_eq!(
        http_body, ipc,
        "{label}: HTTP {http_status} body must equal the IPC result"
    );
}

#[tokio::test]
async fn health_and_positions_reads_produce_identical_results() {
    let (app, state, _hd, _id) = paired_adapters().await;

    let (status, body) = http(app.clone(), "GET", "/health", None).await;
    assert_eq!(status, 200);
    assert_parity(
        "health",
        status,
        body,
        serde_json::to_value(commands::health(&state).await.expect("health")).unwrap(),
    );

    let (status, body) = http(app.clone(), "GET", "/api/positions", None).await;
    assert_eq!(status, 200);
    assert_parity(
        "positions_list",
        status,
        body,
        serde_json::to_value(
            commands::positions_list(&state, commands::PositionListArgs::default())
                .await
                .expect("list"),
        )
        .unwrap(),
    );

    let (status, body) = http(app.clone(), "GET", "/api/positions/p-1", None).await;
    assert_eq!(status, 200);
    assert_parity(
        "positions_get",
        status,
        body,
        serde_json::to_value(commands::positions_get(&state, "p-1").await.expect("get")).unwrap(),
    );
}

#[tokio::test]
async fn error_envelopes_are_identical_across_transports() {
    let (app, state, _hd, _id) = paired_adapters().await;

    let (status, body) = http(app.clone(), "GET", "/api/positions/missing", None).await;
    let ipc = commands::positions_get(&state, "missing")
        .await
        .expect_err("must be a not_found");
    assert_eq!(status, 404);
    assert_parity(
        "not_found",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );

    let invalid = json!({});
    let (status, body) = http(app.clone(), "POST", "/api/positions", Some(invalid.clone())).await;
    let ipc = commands::positions_create(&state, &invalid)
        .await
        .expect_err("name is required");
    assert_eq!(status, 422);
    assert_parity(
        "validation_error",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );

    let (status, body) = http(
        app.clone(),
        "PATCH",
        "/api/positions/missing",
        Some(json!({"name": "x"})),
    )
    .await;
    let ipc = commands::positions_patch(&state, "missing", &json!({"name": "x"}))
        .await
        .expect_err("missing position");
    assert_eq!(status, 404);
    assert_parity(
        "patch_not_found",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );

    let (status, body) = http(app.clone(), "DELETE", "/api/positions/missing", None).await;
    let ipc = commands::positions_delete(&state, "missing")
        .await
        .expect_err("missing position");
    assert_eq!(status, 404);
    assert_parity(
        "delete_not_found",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );
}

#[tokio::test]
async fn a_write_and_its_generated_fields_are_equivalent() {
    let (app, state, _hd, _id) = paired_adapters().await;
    let payload = json!({"name": "Parity Engineer", "location": "Wuhan", "status": "Open"});

    let (status, body) = http(app.clone(), "POST", "/api/positions", Some(payload.clone())).await;
    assert_eq!(status, 201);
    let ipc = commands::positions_create(&state, &payload)
        .await
        .expect("create");
    let mut http_value = body;
    let mut ipc_value = serde_json::to_value(ipc).unwrap();
    // Generated UUIDs/timestamps differ by design; both sides must still
    // produce them and the remaining fields must match byte-for-byte.
    for value in [&mut http_value, &mut ipc_value] {
        let object = value.as_object_mut().expect("position object");
        for volatile in ["id", "created_at", "updated_at"] {
            assert!(object.contains_key(volatile), "missing {volatile}");
            object.remove(volatile);
        }
    }
    assert_eq!(
        http_value, ipc_value,
        "create DTOs differ after normalisation"
    );

    // Read back through each transport with its own generated id.
    let http_id = http(
        app.clone(),
        "POST",
        "/api/positions",
        Some(json!({"name":"Second"})),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_string();
    let ipc_id = commands::positions_create(&state, &json!({"name":"Second"}))
        .await
        .expect("create")
        .id;
    assert_eq!(
        http(
            app.clone(),
            "GET",
            &format!("/api/positions/{http_id}"),
            None
        )
        .await
        .1["name"],
        json!("Second")
    );
    assert_eq!(
        commands::positions_get(&state, &ipc_id)
            .await
            .expect("get")
            .name,
        "Second"
    );
}

#[tokio::test]
async fn shared_application_guards_render_identically_over_both_transports() {
    let (app, state, _hd, _id) = paired_adapters().await;

    // Confirm guard (409) — the rule used to live in each adapter.
    let (status, body) = http(
        app.clone(),
        "POST",
        "/api/backups/nope/restore",
        Some(json!({"confirm": false})),
    )
    .await;
    let ipc = commands::backups_restore(
        &state,
        commands::BackupsRestoreArgs {
            name: Some("nope".to_string()),
            confirm: Some(false),
        },
    )
    .await
    .expect_err("confirm is required");
    assert_eq!(status, 409);
    assert_parity(
        "restore_confirm",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );

    // Existence pre-check (404): IPC used to skip it; the shared use case now
    // executes it for both transports before touching any file.
    let (status, body) = http(
        app.clone(),
        "POST",
        "/api/backups/nope/restore",
        Some(json!({"confirm": true})),
    )
    .await;
    let ipc = commands::backups_restore(
        &state,
        commands::BackupsRestoreArgs {
            name: Some("nope".to_string()),
            confirm: Some(true),
        },
    )
    .await
    .expect_err("unknown backup");
    assert_eq!(status, 404);
    assert_parity(
        "restore_missing",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );

    // Path guard (422).
    let (status, body) = http(
        app.clone(),
        "POST",
        "/api/backups/..%2Fx/restore",
        Some(json!({"confirm": true})),
    )
    .await;
    let ipc = commands::backups_restore(
        &state,
        commands::BackupsRestoreArgs {
            name: Some("../x".to_string()),
            confirm: Some(true),
        },
    )
    .await
    .expect_err("path is not a file name");
    assert_eq!(status, 422);
    assert_parity(
        "restore_path",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );

    // Authorization is executed in the Application layer for both adapters.
    let (status, body) = http(app.clone(), "GET", "/api/shares", None).await;
    assert_eq!(status, 200);
    assert_parity(
        "shares_list",
        status,
        body,
        serde_json::to_value(commands::shares_list(&state).await.expect("shares")).unwrap(),
    );
    let (status, body) = http(app.clone(), "DELETE", "/api/shares/missing", None).await;
    let ipc = commands::shares_delete(&state, "missing".to_string())
        .await
        .expect_err("missing share");
    assert_eq!(status, 404);
    assert_parity(
        "shares_delete",
        status,
        body,
        serde_json::to_value(ipc).unwrap(),
    );
}

/// Extract a function body by brace matching (string literals skipped), so
/// the source assertions below cannot be fooled by comments or other
/// functions in the same file.
fn body_of<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("missing function {signature}"));
    let open = start
        + source[start..]
            .find('{')
            .unwrap_or_else(|| panic!("no body for {signature}"));
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = open;
    for (offset, ch) in source[open..].char_indices() {
        let index = open + offset;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = index;
                    break;
                }
            }
            _ => {}
        }
    }
    assert_eq!(depth, 0, "unbalanced braces in {signature}");
    &source[open..=end]
}

#[test]
fn adapter_bodies_only_parse_validate_and_call_use_cases() {
    const HTTP: &str = include_str!("../src/routes.rs");
    const IPC: &str = include_str!("../../ica-desktop/src/commands.rs");

    let http_fns = [
        "async fn list_positions(",
        "async fn create_position(",
        "async fn get_position(",
        "async fn update_position(",
        "async fn delete_position(",
        "async fn restore_backup_endpoint(",
        "async fn get_import(",
        "async fn create_share_endpoint(",
        "async fn list_shares_endpoint(",
        "async fn delete_share_endpoint(",
    ];
    let ipc_fns = [
        "pub async fn positions_list(",
        "pub async fn positions_create(",
        "pub async fn positions_get(",
        "pub async fn positions_patch(",
        "pub async fn positions_delete(",
        "pub async fn backups_restore(",
        "pub async fn imports_get(",
        "pub async fn shares_create(",
        "pub async fn shares_list(",
        "pub async fn shares_delete(",
    ];
    for (source, signatures) in [(HTTP, &http_fns[..]), (IPC, &ipc_fns[..])] {
        for signature in signatures {
            let raw = body_of(source, signature);
            let code: String = raw
                .lines()
                .map(|line| line.split("//").next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n");
            for forbidden in [
                "ContractError::",
                ".find(",
                "was not found",
                " if ",
                "\nif ",
                "is_admin",
            ] {
                assert!(
                    !code.contains(forbidden),
                    "{signature} still contains adapter business logic: {forbidden}\n{code}"
                );
            }
            assert!(
                code.contains("ica_application::"),
                "{signature} must call a shared use case"
            );
        }
    }

    // The business rules moved out of the adapters are now owned by the
    // Application layer, reachable from either transport.
    assert!(HTTP.contains("ica_application::restore_backup_checked("));
    assert!(IPC.contains("ica_application::restore_backup_checked("));
    assert!(HTTP.contains("ica_application::get_import_scoped("));
    assert!(!IPC.contains("state.imports().find("));
}
