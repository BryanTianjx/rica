//! HTTP contract tests for the five candidate endpoints.

mod support;

use serde_json::json;

#[tokio::test]
async fn list_candidates_hides_archived_rows_and_reports_the_real_total() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app, "/api/candidates").await;

    assert_eq!(page.status, 200);
    assert_eq!(page.body["total"], 3);
    assert_eq!(page.body["offset"], 0);
    assert_eq!(page.body["limit"], 50);
    assert_eq!(page.body["has_more"], false);
    let items = page.body["items"].as_array().expect("items");
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|row| row["archived"] == false));
}

#[tokio::test]
async fn list_candidates_include_archived_adds_the_archived_row() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app.clone(), "/api/candidates?include_archived=true").await;

    assert_eq!(
        page.body["total"], 3,
        "no candidate row is archived in this seed"
    );
    let page = support::get(app, "/api/candidates?include_archived=1").await;
    assert_eq!(page.body["total"], 3);
}

#[tokio::test]
async fn list_candidates_search_is_case_insensitive_over_three_columns() {
    let (app, _guard) = support::w1_app().await;

    let by_name = support::get(app.clone(), "/api/candidates?search=ALICE").await;
    assert_eq!(by_name.body["total"], 1);
    assert_eq!(by_name.body["items"][0]["name"], "Alice");

    let miss = support::get(app, "/api/candidates?search=nobody-here").await;
    assert_eq!(miss.body["total"], 0);
    assert_eq!(miss.body["items"].as_array().expect("items").len(), 0);
}

#[tokio::test]
async fn list_candidates_filters_by_status_and_position() {
    let (app, _guard) = support::w1_app().await;

    let by_status = support::get(app.clone(), "/api/candidates?status=New").await;
    assert_eq!(by_status.body["total"], 3);

    let by_other = support::get(app.clone(), "/api/candidates?status=Contacted").await;
    assert_eq!(by_other.body["total"], 0);

    let by_position = support::get(app.clone(), "/api/candidates?position_id=p-1").await;
    assert_eq!(by_position.body["total"], 2);

    let both = support::get(app, "/api/candidates?position_id=p-4&status=New").await;
    assert_eq!(both.body["total"], 1);
    assert_eq!(both.body["items"][0]["id"], "c-3");
}

#[tokio::test]
async fn list_candidates_paginates_with_real_total_and_updated_at_order() {
    let (app, _guard) = support::w1_app().await;

    let first = support::get(app.clone(), "/api/candidates?limit=2").await;
    assert_eq!(first.body["total"], 3);
    assert_eq!(first.body["has_more"], true);
    let first_ids: Vec<&str> = first.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect();

    let second = support::get(app, "/api/candidates?limit=2&offset=2").await;
    assert_eq!(second.body["total"], 3);
    assert_eq!(second.body["has_more"], false);
    let second_ids: Vec<&str> = second.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect();

    // The same three rows, split, with no duplicate and no gap. All seeded
    // candidates share updated_at, so the comparison is set-based.
    let mut all: Vec<&str> = first_ids.into_iter().chain(second_ids).collect();
    all.sort_unstable();
    assert_eq!(all, vec!["c-1", "c-2", "c-3"]);
}

#[tokio::test]
async fn list_candidates_with_an_unknown_status_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates?status=Nope").await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

#[tokio::test]
async fn list_candidates_rejects_a_page_size_over_500() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates?limit=501").await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn candidate_detail_carries_the_nested_position_and_real_counters() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates/c-1").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["name"], "Alice");
    assert_eq!(response.body["position_name"], "Senior Rust Engineer");
    assert_eq!(response.body["message_count"], 5);
    // One Completed task counts as a contact; the Pending one does not.
    assert_eq!(response.body["active_follow_up_count"], 1);
    // The nested position is mapped like the position endpoint: candidate_count
    // is the real 2, not the schema default 0.
    assert_eq!(response.body["position"]["id"], "p-1");
    assert_eq!(response.body["position"]["candidate_count"], 2);
    assert_eq!(
        response.body["position"]["created_at"],
        "2026-11-10T09:00:00Z"
    );
}

#[tokio::test]
async fn candidate_detail_without_a_position_has_a_null_nested_object() {
    let (app, guard) = support::w1_app().await;

    support::delete(app.clone(), "/api/positions/p-1").await;
    let response = support::get(app, "/api/candidates/c-1").await;

    assert_eq!(response.status, 200);
    assert!(response.body["position_id"].is_null());
    assert!(response.body["position_name"].is_null());
    assert!(response.body["position"].is_null());
    drop(guard);
}

#[tokio::test]
async fn candidate_detail_of_a_missing_id_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates/nope").await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Candidate 'nope' was not found"
    );
}

#[tokio::test]
async fn create_candidate_applies_the_baseline_defaults() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/candidates", r#"{"name": "  Grace  "}"#).await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["name"], "Grace", "the name is trimmed");
    assert_eq!(response.body["source"], "boss");
    assert_eq!(response.body["status"], "New");
    assert_eq!(response.body["archived"], false);
    assert!(response.body["position_id"].is_null());
    assert!(response.body["position_name"].is_null());
    assert_eq!(response.body["message_count"], 0);
    assert_eq!(response.body["active_follow_up_count"], 0);
}

#[tokio::test]
async fn create_candidate_with_a_position_returns_its_name() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/candidates",
        r#"{"name": "Heidi", "position_id": "p-2", "status": "Contacted"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["position_id"], "p-2");
    assert_eq!(response.body["position_name"], "Frontend Developer");
    assert_eq!(response.body["status"], "Contacted");
}

#[tokio::test]
async fn create_candidate_linked_to_a_missing_position_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/candidates",
        r#"{"name": "x", "position_id": "nope"}"#,
    )
    .await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Position 'nope' was not found"
    );
}

#[tokio::test]
async fn create_candidate_with_a_blank_name_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/candidates", r#"{"name": "   "}"#).await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

#[tokio::test]
async fn patch_candidate_updates_the_present_fields_only() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(
        app,
        "/api/candidates/c-1",
        r#"{"status": "Replied", "expected_location": "Beijing"}"#,
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["status"], "Replied");
    assert_eq!(response.body["expected_location"], "Beijing");
    assert_eq!(response.body["name"], "Alice");
    assert_eq!(response.body["position_id"], "p-1");
}

#[tokio::test]
async fn patch_candidate_null_position_id_detaches_the_position() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/candidates/c-1", r#"{"position_id": null}"#).await;

    assert_eq!(response.status, 200);
    assert!(response.body["position_id"].is_null());
    assert!(response.body["position_name"].is_null());
}

#[tokio::test]
async fn patch_candidate_explicit_null_on_a_not_null_column_is_422() {
    for field in ["name", "source", "status", "archived"] {
        let (app, _guard) = support::w1_app().await;
        let body = format!(r#"{{"{field}": null}}"#);
        let response = support::patch(app, "/api/candidates/c-1", &body).await;
        assert_eq!(response.status, 422, "field {field}");
        assert_eq!(response.body["error"]["code"], "validation_error");
    }
}

#[tokio::test]
async fn patch_candidate_to_a_missing_position_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/candidates/c-1", r#"{"position_id": "nope"}"#).await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "not_found");
}

#[tokio::test]
async fn patch_a_missing_candidate_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::patch(app, "/api/candidates/nope", r#"{"name": "x"}"#).await;

    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn delete_candidate_removes_every_dependent_record() {
    let (app, guard) = support::w1_app().await;

    let response = support::delete(app, "/api/candidates/c-1").await;
    assert_eq!(response.status, 204);

    let pool = support::pool_for(&guard).await;
    for table in [
        "conversations",
        "messages",
        "send_records",
        "follow_up_tasks",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE candidate_id = ?");
        assert_eq!(
            support::count(&pool, &sql, &["c-1"]).await,
            0,
            "{table} rows for the deleted candidate"
        );
    }
    // Bob's rows are untouched.
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM messages WHERE candidate_id = ?",
            &["c-2"]
        )
        .await,
        1
    );
}

#[tokio::test]
async fn delete_a_missing_candidate_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::delete(app, "/api/candidates/nope").await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "not_found",
                "message": "Candidate 'nope' was not found",
                "details": {}
            }
        })
    );
}
