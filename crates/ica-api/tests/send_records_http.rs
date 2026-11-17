//! HTTP contract tests for the two manual send-record endpoints.

mod support;

use chrono::{DateTime, Utc};
use serde_json::json;

#[tokio::test]
async fn list_send_records_is_newest_send_first_with_the_real_total() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app, "/api/send-records").await;

    assert_eq!(page.status, 200);
    assert_eq!(page.body["total"], 4);
    let ids: Vec<&str> = page.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["s-4", "s-3", "s-2", "s-1"]);
    assert_eq!(page.body["items"][0]["sent_at"], "2026-11-13T10:00:00Z");
}

#[tokio::test]
async fn list_send_records_filters_by_candidate_and_status() {
    let (app, _guard) = support::w1_app().await;

    let by_candidate = support::get(app.clone(), "/api/send-records?candidate_id=c-1").await;
    assert_eq!(by_candidate.body["total"], 3);

    let by_status = support::get(app.clone(), "/api/send-records?status=sent").await;
    assert_eq!(by_status.body["total"], 2);

    let both = support::get(app, "/api/send-records?candidate_id=c-1&status=sent").await;
    assert_eq!(both.body["total"], 1);
    assert_eq!(both.body["items"][0]["id"], "s-1");
}

#[tokio::test]
async fn list_send_records_paginates() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app, "/api/send-records?limit=2&offset=1").await;

    assert_eq!(page.body["total"], 4);
    assert_eq!(page.body["limit"], 2);
    assert_eq!(page.body["offset"], 1);
    let ids: Vec<&str> = page.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["s-3", "s-2"]);
}

#[tokio::test]
async fn list_send_records_rejects_an_unknown_status() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/send-records?status=maybe").await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

#[tokio::test]
async fn record_send_appends_a_row_with_the_local_actor_and_a_default_time() {
    let (app, guard) = support::w1_app().await;

    let before = Utc::now();
    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "target": "boss-42",
            "content_summary": "intro message"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["candidate_id"], "c-1");
    assert_eq!(response.body["channel"], "boss");
    assert_eq!(response.body["status"], "sent");
    assert_eq!(response.body["actor"], "local_user");
    assert_eq!(response.body["target"], "boss-42");
    assert!(response.body["task_id"].is_null());
    assert!(response.body["draft_id"].is_null());

    // `sent_at` defaults to `now(UTC)`.
    let sent_at: DateTime<Utc> = response.body["sent_at"]
        .as_str()
        .expect("sent_at")
        .parse()
        .expect("RFC 3339");
    assert!(sent_at >= before - chrono::Duration::seconds(5));
    assert!(sent_at <= Utc::now() + chrono::Duration::seconds(5));

    let pool = support::pool_for(&guard).await;
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM send_records WHERE actor = ?",
            &["local_user"]
        )
        .await,
        5,
        "the seeded four plus the new one"
    );
}

#[tokio::test]
async fn record_send_keeps_an_explicit_sent_at() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-2", "status": "candidate_replied",
            "sent_at": "2026-11-14T08:15:00Z"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["sent_at"], "2026-11-14T08:15:00Z");
}

#[tokio::test]
async fn record_send_with_an_approved_draft_and_its_task_is_201() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "task_id": "t-1", "draft_id": "d-1"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["task_id"], "t-1");
    assert_eq!(response.body["draft_id"], "d-1");
}

#[tokio::test]
async fn record_send_with_a_task_of_another_candidate_is_409() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "task_id": "t-3"}"#,
    )
    .await;

    assert_eq!(response.status, 409);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "business_rule_violation",
                "message": "The follow-up task belongs to a different candidate",
                "details": {"task_id": "t-3", "candidate_id": "c-1"}
            }
        })
    );
}

#[tokio::test]
async fn record_send_with_a_missing_task_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "task_id": "nope"}"#,
    )
    .await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Follow-up task 'nope' was not found"
    );
}

#[tokio::test]
async fn record_send_with_an_unapproved_draft_is_409() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "draft_id": "d-2"}"#,
    )
    .await;

    assert_eq!(response.status, 409);
    assert_eq!(
        response.body["error"]["message"],
        "Only an approved or copied draft can be recorded as sent"
    );
    assert_eq!(
        response.body["error"]["details"]["draft_status"],
        "generated"
    );
}

#[tokio::test]
async fn record_send_with_another_candidates_draft_is_409() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "draft_id": "d-3"}"#,
    )
    .await;

    assert_eq!(response.status, 409);
    assert_eq!(
        response.body["error"]["message"],
        "The draft belongs to a different candidate"
    );
}

#[tokio::test]
async fn record_send_with_a_missing_draft_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "draft_id": "nope"}"#,
    )
    .await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Draft 'nope' was not found"
    );
}

#[tokio::test]
async fn record_send_for_a_missing_candidate_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "nope", "status": "sent"}"#,
    )
    .await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Candidate 'nope' was not found"
    );
}

#[tokio::test]
async fn record_send_without_a_status_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/send-records", r#"{"candidate_id": "c-1"}"#).await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn record_send_with_an_unknown_status_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "delivered"}"#,
    )
    .await;

    assert_eq!(response.status, 422);
}
