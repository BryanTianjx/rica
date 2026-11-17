//! HTTP contract tests for the three read-only endpoints.
//!
//! These pin the wire format the Python baseline produces, not the internal
//! shape of the Rust code: field sets, the `Page` envelope, the UTC rendering
//! and the unified error envelope.

mod support;

use serde_json::json;

#[tokio::test]
async fn health_reports_status_app_name_and_ai_flag() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/health").await;

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({"status": "ok", "app": "ICA Personal", "ai_enabled": false})
    );
}

#[tokio::test]
async fn health_has_exactly_the_three_baseline_fields() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/health").await;

    let object = response.body.as_object().expect("object body");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["ai_enabled", "app", "status"]);
}

#[tokio::test]
async fn position_detail_returns_every_field_with_a_utc_offset() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions/p-1").await;

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body,
        json!({
            "id": "p-1",
            "name": "Senior Rust Engineer",
            "description": "Senior Rust Engineer description",
            "location": "Beijing",
            "skill_requirements": null,
            "experience_requirements": null,
            "education_requirements": null,
            "salary_min": null,
            "salary_max": null,
            "notes": null,
            "status": "Open",
            "archived": false,
            "created_at": "2026-11-10T09:00:00Z",
            "updated_at": "2026-11-10T09:30:00Z",
            "candidate_count": 2
        })
    );
}

#[tokio::test]
async fn position_detail_keeps_microseconds_from_the_stored_timestamp() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions/p-4").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["created_at"], "2026-11-13T12:00:00.500000Z");
    assert_eq!(response.body["archived"], true);
    assert_eq!(response.body["status"], "Closed");
    assert_eq!(response.body["candidate_count"], 1);
}

#[tokio::test]
async fn position_detail_reports_missing_rows_with_the_baseline_envelope() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions/does-not-exist").await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "not_found",
                "message": "Position 'does-not-exist' was not found",
                "details": {}
            }
        })
    );
}

#[tokio::test]
async fn position_detail_returns_an_archived_row() {
    // The detail endpoint never filters on `archived`; only the list does.
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions/p-4").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["id"], "p-4");
}

#[tokio::test]
async fn list_hides_archived_rows_and_orders_newest_first() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total"], 3);
    assert_eq!(response.body["offset"], 0);
    assert_eq!(response.body["limit"], 50);
    assert_eq!(response.body["has_more"], false);
    assert_eq!(ids(&response.body), vec!["p-3", "p-2", "p-1"]);
}

#[tokio::test]
async fn list_reports_has_more_when_a_further_page_exists() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?limit=2").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total"], 3);
    assert_eq!(response.body["limit"], 2);
    assert_eq!(response.body["has_more"], true);
    assert_eq!(ids(&response.body), vec!["p-3", "p-2"]);
}

#[tokio::test]
async fn list_second_page_reports_no_more_rows() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?limit=2&offset=2").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["offset"], 2);
    assert_eq!(response.body["has_more"], false);
    assert_eq!(ids(&response.body), vec!["p-1"]);
}

#[tokio::test]
async fn list_offset_past_the_end_returns_an_empty_page_with_the_real_total() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?offset=100").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total"], 3);
    assert_eq!(response.body["offset"], 100);
    assert_eq!(response.body["has_more"], false);
    assert!(response.body["items"]
        .as_array()
        .expect("items array")
        .is_empty());
}

#[tokio::test]
async fn list_include_archived_returns_the_archived_row_too() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?include_archived=true").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total"], 4);
    assert_eq!(ids(&response.body), vec!["p-4", "p-3", "p-2", "p-1"]);
}

#[tokio::test]
async fn list_accepts_the_boolean_spellings_fastapi_accepts() {
    // `include_archived=1` must keep working: the desktop shell sends 0/1.
    for value in ["1", "true", "True", "yes", "on"] {
        let (app, _guard) = support::seeded_app().await;
        let response = support::get(app, &format!("/api/positions?include_archived={value}")).await;
        assert_eq!(response.status, 200, "include_archived={value}");
        assert_eq!(response.body["total"], 4, "include_archived={value}");
    }
}

#[tokio::test]
async fn list_search_matches_name_and_location_case_insensitively() {
    let (app, _guard) = support::seeded_app().await;

    let by_location = support::get(app.clone(), "/api/positions?search=beijing").await;
    assert_eq!(by_location.status, 200);
    assert_eq!(ids(&by_location.body), vec!["p-1"]);

    let by_name = support::get(app, "/api/positions?search=RUST").await;
    assert_eq!(by_name.status, 200);
    assert_eq!(ids(&by_name.body), vec!["p-1"]);
}

#[tokio::test]
async fn list_search_respects_the_archive_filter() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?search=beijing&include_archived=true").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total"], 2);
    assert_eq!(ids(&response.body), vec!["p-4", "p-1"]);
}

#[tokio::test]
async fn list_search_that_matches_nothing_is_an_empty_page() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?search=zzz-nothing").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total"], 0);
    assert_eq!(response.body["has_more"], false);
    assert!(response.body["items"]
        .as_array()
        .expect("items array")
        .is_empty());
}

#[tokio::test]
async fn list_search_trims_surrounding_whitespace() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?search=%20beijing%20").await;

    assert_eq!(response.status, 200);
    assert_eq!(ids(&response.body), vec!["p-1"]);
}

#[tokio::test]
async fn list_search_survives_a_row_with_a_null_location() {
    // p-3 has no location; searching by its name must still return it.
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?search=Data").await;

    assert_eq!(response.status, 200);
    assert_eq!(ids(&response.body), vec!["p-3"]);
    assert_eq!(
        response.body["items"][0]["location"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn list_items_carry_the_same_fields_as_the_detail_endpoint() {
    let (app, _guard) = support::seeded_app().await;

    let list = support::get(app.clone(), "/api/positions?limit=1").await;
    let detail = support::get(app, "/api/positions/p-3").await;

    assert_eq!(list.status, 200);
    assert_eq!(list.body["items"][0], detail.body);
}

#[tokio::test]
async fn list_envelope_has_exactly_the_five_baseline_fields() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions").await;

    let object = response.body.as_object().expect("object body");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["has_more", "items", "limit", "offset", "total"]);
}

/// Ids of the rows in `items`, in the order the API returned them.
fn ids(body: &serde_json::Value) -> Vec<&str> {
    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item["id"].as_str().expect("id string"))
        .collect()
}

#[tokio::test]
async fn limit_zero_is_a_422_with_the_validation_envelope() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?limit=0").await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
    assert_eq!(
        response.body["error"]["message"],
        "Request payload failed validation"
    );
    let issue = &response.body["error"]["details"]["errors"][0];
    assert_eq!(issue["loc"], serde_json::json!(["query", "limit"]));
    assert_eq!(issue["type"], "greater_than");
}

#[tokio::test]
async fn limit_above_500_is_a_422() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?limit=501").await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
    assert_eq!(
        response.body["error"]["details"]["errors"][0]["type"],
        "less_than_equal"
    );
}

#[tokio::test]
async fn a_non_numeric_limit_is_a_422_not_a_500() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?limit=abc").await;

    assert_eq!(response.status, 422);
    assert_eq!(
        response.body["error"]["details"]["errors"][0]["type"],
        "int_parsing"
    );
}

#[tokio::test]
async fn a_negative_offset_is_a_422() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?offset=-1").await;

    assert_eq!(response.status, 422);
    assert_eq!(
        response.body["error"]["details"]["errors"][0]["type"],
        "greater_than_equal"
    );
}

#[tokio::test]
async fn a_search_term_over_200_characters_is_a_422() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, &format!("/api/positions?search={}", "a".repeat(201))).await;

    assert_eq!(response.status, 422);
    assert_eq!(
        response.body["error"]["details"]["errors"][0]["type"],
        "string_too_long"
    );
}

#[tokio::test]
async fn an_unparseable_boolean_is_a_422() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?include_archived=maybe").await;

    assert_eq!(response.status, 422);
    assert_eq!(
        response.body["error"]["details"]["errors"][0]["type"],
        "bool_parsing"
    );
}

#[tokio::test]
async fn several_bad_parameters_are_reported_together() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions?limit=0&offset=-1").await;

    assert_eq!(response.status, 422);
    let errors = response.body["error"]["details"]["errors"]
        .as_array()
        .expect("errors array");
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[tokio::test]
async fn an_unknown_path_keeps_the_project_error_envelope() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/nope").await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body,
        serde_json::json!({
            "error": {"code": "http_error", "message": "Not Found", "details": {}}
        })
    );
}

#[tokio::test]
async fn an_unknown_position_is_a_404_not_an_empty_page() {
    let (app, _guard) = support::seeded_app().await;

    let response = support::get(app, "/api/positions/p-404").await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "not_found");
    assert_eq!(
        response.body["error"]["message"],
        "Position 'p-404' was not found"
    );
}
