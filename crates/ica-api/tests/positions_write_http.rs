//! HTTP contract tests for the three position write endpoints.
//!
//! Two behaviours are pinned that the Python baseline gets *wrong* on purpose
//! (declared improvements, see docs/rust-migration-contract.md §7):
//!
//! * IMP-2: `PATCH {"name": null}` / `{"archived": null}` answer 422, not 500.
//! * IMP-3: the list endpoints report the real total (unchanged for positions,
//!   pinned here so the shared `Page` semantics cannot drift).

mod support;

use serde_json::json;

#[tokio::test]
async fn create_position_returns_201_and_the_stored_row() {
    let (app, guard) = support::w1_app().await;

    let response = support::post(
        app.clone(),
        "/api/positions",
        r#"{"name": "Backend Engineer"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["name"], "Backend Engineer");
    // Defaults from the baseline schema.
    assert_eq!(response.body["status"], "Open");
    assert_eq!(response.body["archived"], false);
    assert_eq!(response.body["candidate_count"], 0);
    assert!(response.body["description"].is_null());
    assert!(response.body["salary_min"].is_null());
    // A fresh row: the timestamps are rendered as UTC with a `Z`.
    assert!(response.body["created_at"]
        .as_str()
        .expect("created_at is a string")
        .ends_with('Z'));
    assert_eq!(response.body["created_at"], response.body["updated_at"]);
    assert_eq!(
        response.body["id"].as_str().expect("id").len(),
        36,
        "a UUID4 string primary key"
    );

    let pool = support::pool_for(&guard).await;
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM positions WHERE name = ?",
            &["Backend Engineer"]
        )
        .await,
        1
    );
}

#[tokio::test]
async fn create_position_stores_every_optional_field() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/positions",
        r#"{
            "name": "Platform Engineer",
            "description": "Own the deploy path.",
            "location": "Hangzhou",
            "skill_requirements": "Kubernetes",
            "experience_requirements": "5 years",
            "education_requirements": "BSc",
            "salary_min": 30000,
            "salary_max": 45000,
            "notes": "urgent",
            "status": "OnHold"
        }"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["location"], "Hangzhou");
    assert_eq!(response.body["salary_min"], 30000);
    assert_eq!(response.body["salary_max"], 45000);
    assert_eq!(response.body["status"], "OnHold");
    assert_eq!(response.body["notes"], "urgent");
}

#[tokio::test]
async fn create_position_without_a_name_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/positions", "{}").await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
    assert_eq!(
        response.body["error"]["message"],
        "Request payload failed validation"
    );
}

#[tokio::test]
async fn create_position_with_a_blank_name_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/positions", r#"{"name": ""}"#).await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn create_position_with_an_unknown_status_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/positions",
        r#"{"name": "x", "status": "Paused"}"#,
    )
    .await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

#[tokio::test]
async fn create_position_with_a_negative_salary_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/positions", r#"{"name": "x", "salary_min": -1}"#).await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn create_position_with_broken_json_is_422_not_a_plain_text_body() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/positions", "{not json").await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

#[tokio::test]
async fn patch_position_changes_only_the_present_fields() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(
        app,
        "/api/positions/p-1",
        r#"{"location": "Chengdu", "salary_max": 70000}"#,
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["location"], "Chengdu");
    assert_eq!(response.body["salary_max"], 70000);
    // Untouched columns keep their stored value.
    assert_eq!(response.body["name"], "Senior Rust Engineer");
    assert_eq!(
        response.body["description"],
        "Senior Rust Engineer description"
    );
    assert_eq!(response.body["status"], "Open");
    // updated_at moves off the seeded value, created_at does not.
    assert_ne!(response.body["updated_at"], "2026-11-10T09:30:00Z");
    assert_eq!(response.body["created_at"], "2026-11-10T09:00:00Z");
}

#[tokio::test]
async fn patch_position_null_status_is_ignored_like_the_baseline() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/positions/p-1", r#"{"status": null}"#).await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["status"], "Open");
}

#[tokio::test]
async fn patch_position_null_location_clears_the_column() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/positions/p-1", r#"{"location": null}"#).await;

    assert_eq!(response.status, 200);
    assert!(response.body["location"].is_null());
}

/// IMP-2: the baseline answers 500 here (an unhandled IntegrityError).
#[tokio::test]
async fn patch_position_explicit_null_name_is_422_not_500() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/positions/p-1", r#"{"name": null}"#).await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
    assert_eq!(
        response.body["error"]["message"],
        "Request payload failed validation"
    );
}

/// IMP-2: same for the boolean NOT NULL column.
#[tokio::test]
async fn patch_position_explicit_null_archived_is_422_not_500() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/positions/p-1", r#"{"archived": null}"#).await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

#[tokio::test]
async fn patch_position_archives_and_unarchives() {
    let (app, _guard) = support::w1_app().await;

    let archived = support::patch(app.clone(), "/api/positions/p-1", r#"{"archived": true}"#).await;
    assert_eq!(archived.status, 200);
    assert_eq!(archived.body["archived"], true);

    let visible = support::get(app.clone(), "/api/positions?search=Senior%20Rust").await;
    assert_eq!(
        visible.body["total"], 0,
        "an archived row leaves the default list"
    );

    let restored = support::patch(app, "/api/positions/p-1", r#"{"archived": false}"#).await;
    assert_eq!(restored.body["archived"], false);
}

#[tokio::test]
async fn patch_a_missing_position_is_404_with_the_baseline_message() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/positions/nope", r#"{"name": "x"}"#).await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "not_found",
                "message": "Position 'nope' was not found",
                "details": {}
            }
        })
    );
}

#[tokio::test]
async fn delete_position_returns_204_and_detaches_candidates() {
    let (app, guard) = support::w1_app().await;

    let response = support::delete(app.clone(), "/api/positions/p-1").await;

    assert_eq!(response.status, 204);
    assert!(response.body.is_null(), "204 carries no body");

    // The candidate rows survive; only the link is cleared (FK SET NULL).
    let pool = support::pool_for(&guard).await;
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM candidates WHERE position_id IS NULL",
            &[]
        )
        .await,
        2
    );
    assert_eq!(
        support::count(&pool, "SELECT COUNT(*) FROM candidates", &[]).await,
        3
    );

    let detail = support::get(app, "/api/candidates/c-1").await;
    assert_eq!(detail.status, 200);
    assert!(detail.body["position_id"].is_null());
    assert!(detail.body["position"].is_null());
}

#[tokio::test]
async fn delete_a_missing_position_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::delete(app, "/api/positions/nope").await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "not_found");
}

#[tokio::test]
async fn the_position_list_total_stays_the_real_count_after_a_write() {
    let (app, _guard) = support::w1_app().await;

    support::post(app.clone(), "/api/positions", r#"{"name": "New Role"}"#).await;
    let page = support::get(app, "/api/positions?limit=2").await;

    assert_eq!(page.body["total"], 4, "3 live seeded rows + 1 created");
    assert_eq!(page.body["items"].as_array().expect("items").len(), 2);
    assert_eq!(page.body["has_more"], true);
}
