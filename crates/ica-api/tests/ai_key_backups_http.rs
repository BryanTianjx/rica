//! `PUT/DELETE /settings/ai-key` + the backups trio (W5).
//!
//! The key is write-only: responses carry the enabled flag and a masked
//! tail, and every change is audited masked. Backups go through the SQLite
//! online-backup API, the restore takes a pre-restore safety copy first,
//! and the drill in `docs/testing/backup-restore-drill.md` is the manual.

mod support;

use support::{count, get, pool_for, post, put, w1_app};

#[tokio::test]
async fn writing_the_installation_key_turns_ai_on_and_audits_masked() {
    let (app, dir) = w1_app().await;

    let response = put(
        app.clone(),
        "/api/settings/ai-key",
        r#"{"ai_api_key":"sk-live-1234567890"}"#,
    )
    .await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    assert_eq!(
        response.body,
        serde_json::json!({
            "ai_enabled": true,
            "key_masked": "…7890",
            "model_id": null,
        }),
        "the key never comes back: {}",
        response.body
    );

    // The registry fallback now reports the key as configured.
    let models = get(app.clone(), "/api/ai/models").await;
    assert_eq!(models.body["models"][0]["api_key_configured"], true);

    // The audit trail carries the masked tail only.
    let trail = get(
        app.clone(),
        "/api/audit-events?event_type=ai.key_configured",
    )
    .await;
    let items = trail.body.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["entity_type"], "settings");
    assert_eq!(items[0]["metadata"]["key_masked"], "…7890");
    assert_eq!(
        items[0]["metadata"]["model_id"],
        serde_json::Value::Null,
        "the installation-wide write has no model"
    );

    // The credential slot is in the store, not the database.
    let pool = pool_for(&dir).await;
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM ai_model_configs", &[]).await,
        0
    );
}

#[tokio::test]
async fn an_empty_key_is_a_coded_422_and_a_model_slot_stays_independent() {
    let (app, _dir) = w1_app().await;

    let empty = put(
        app.clone(),
        "/api/settings/ai-key",
        r#"{"ai_api_key":"  "}"#,
    )
    .await;
    assert_eq!(empty.status, 422, "body: {}", empty.body);
    assert_eq!(empty.body["error"]["code"], "ai_key_empty");
    assert_eq!(
        empty.body["error"]["message"],
        "AI_API_KEY must not be empty; use DELETE /settings/ai-key to clear it."
    );

    // A model-scoped key targets its own slot and leaves the runtime flag
    // alone (the fixture seeds no installation key).
    let scoped = put(
        app.clone(),
        "/api/settings/ai-key",
        r#"{"ai_api_key":"sk-model-9999","model_id":"glm-4-flash"}"#,
    )
    .await;
    assert_eq!(scoped.status, 200, "body: {}", scoped.body);
    assert_eq!(scoped.body["model_id"], "glm-4-flash");
    assert_eq!(scoped.body["key_masked"], "…9999");

    let clear = delete_with_query(app, "/api/settings/ai-key?model_id=glm-4-flash").await;
    assert_eq!(clear.status, 200, "body: {}", clear.body);
    assert_eq!(clear.body["key_masked"], serde_json::Value::Null);
    assert_eq!(clear.body["model_id"], "glm-4-flash");
    // The installation-wide flag was never on.
    assert_eq!(clear.body["ai_enabled"], false);
}

#[tokio::test]
async fn clearing_the_installation_key_turns_ai_back_off() {
    let (app, _dir) = w1_app().await;
    let write = put(
        app.clone(),
        "/api/settings/ai-key",
        r#"{"ai_api_key":"sk-live-abcdef"}"#,
    )
    .await;
    assert_eq!(write.body["ai_enabled"], true);

    let clear = delete_with_query(app, "/api/settings/ai-key").await;
    assert_eq!(clear.status, 200, "body: {}", clear.body);
    assert_eq!(
        clear.body,
        serde_json::json!({
            "ai_enabled": false,
            "key_masked": null,
            "model_id": null,
        })
    );
}

#[tokio::test]
async fn the_backup_lifecycle_round_trips_the_data() {
    support::init_test_logging();
    let (app, dir) = w1_app().await;

    // Business data, then a backup that must contain it.
    let created = post(app.clone(), "/api/positions", r#"{"name":"Backup Probe"}"#).await;
    assert_eq!(created.status, 201);
    let total_before = get(app.clone(), "/api/positions?limit=500").await.body["total"].clone();
    let backup = post(app.clone(), "/api/backups", "{}").await;
    assert_eq!(backup.status, 200, "body: {}", backup.body);
    let name = backup.body["name"].as_str().expect("name").to_string();
    assert!(name.starts_with("ica-backup-"));
    assert_eq!(backup.body["kind"], "backup");
    assert!(backup.body["size_bytes"].as_i64().expect("size") > 0);

    // Delete the business data, then restore: the probe comes back.
    let listed = get(app.clone(), "/api/positions?limit=50").await;
    let position_id = listed.body["items"][0]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let deleted = send_delete(app.clone(), &format!("/api/positions/{position_id}")).await;
    assert_eq!(deleted.status, 204);

    let restore = post(
        app.clone(),
        &format!("/api/backups/{name}/restore"),
        r#"{"confirm":true}"#,
    )
    .await;
    assert_eq!(restore.status, 200, "body: {}", restore.body);
    assert!(restore.body["restored_from"]
        .as_str()
        .expect("from")
        .contains(&name));
    assert!(restore.body["previous_copy"]
        .as_str()
        .expect("prev")
        .contains("pre-restore-"));
    assert!(restore.body["message"]
        .as_str()
        .expect("msg")
        .contains("safety copy"));

    let after = get(app.clone(), "/api/positions?limit=500").await;
    assert_eq!(
        after.body["total"], total_before,
        "the backup-time state is fully restored"
    );
    let probe = get(app.clone(), "/api/positions?search=Backup%20Probe").await;
    assert_eq!(probe.body["total"], 1, "the restored position is back");

    // The restore is audited with its safety copy.
    let trail = get(app.clone(), "/api/audit-events?event_type=backup.restored").await;
    let items = trail.body.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["entity_type"], "backup");
    assert_eq!(items[0]["entity_id"], name);
    assert!(items[0]["metadata"]["previous_copy"]
        .as_str()
        .expect("prev")
        .contains("pre-restore-"));

    // The listing shows both families.
    let backups = get(app, "/api/backups").await;
    let items = backups.body.as_array().expect("array");
    assert!(
        items.len() >= 2,
        "the backup and the undo point: {}",
        backups.body
    );
    assert!(items
        .iter()
        .all(|item| { item["kind"] == "backup" || item["kind"] == "pre_restore" }));

    let _ = dir;
}

#[tokio::test]
async fn restore_guards_run_in_the_baseline_order() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;

    // No confirm: a 409 before anything is touched.
    let no_confirm = post(
        app.clone(),
        "/api/backups/ica-backup-x.db/restore",
        r#"{"confirm":false}"#,
    )
    .await;
    assert_eq!(no_confirm.status, 409, "body: {}", no_confirm.body);
    assert_eq!(no_confirm.body["error"]["code"], "business_rule_violation");
    assert_eq!(
        no_confirm.body["error"]["message"],
        "Restoring a backup replaces the current data. Send confirm=true once you are sure."
    );

    // A path, not a name.
    for name in ["..%2Fica.db", "sub%2Ffile.db"] {
        let error = post(
            app.clone(),
            &format!("/api/backups/{name}/restore"),
            r#"{"confirm":true}"#,
        )
        .await;
        assert_eq!(error.status, 422, "{name}");
        assert_eq!(
            error.body["error"]["message"],
            "A backup is addressed by its file name, not by a path."
        );
    }

    // A missing (but well-shaped) name: 404 with the not-found envelope.
    let missing = post(
        app.clone(),
        "/api/backups/ica-backup-missing.db/restore",
        r#"{"confirm":true}"#,
    )
    .await;
    assert_eq!(missing.status, 404, "body: {}", missing.body);
    assert_eq!(missing.body["error"]["code"], "not_found");

    // Nothing was audited by the refused calls.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'backup.restored'",
            &[]
        )
        .await,
        0
    );
}

// The support helpers only cover GET/POST/PATCH/PUT/DELETE without queries.
async fn send_delete(app: axum::Router, uri: &str) -> support::TestResponse {
    use tower::ServiceExt;
    let request = axum::http::Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("accept", "application/json")
        .body(axum::body::Body::empty())
        .expect("build request");
    let response = app.oneshot(request).await.expect("router responded");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    support::TestResponse { status, body }
}

async fn delete_with_query(app: axum::Router, uri: &str) -> support::TestResponse {
    send_delete(app, uri).await
}
