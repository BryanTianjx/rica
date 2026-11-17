//! HTTP contract tests for conversations, messages and the candidate timeline.
//!
//! IMP-1 is pinned here: `GET /api/messages` reports the real number of matching
//! rows, independent of `limit`/`offset`, where the Python baseline truncates it
//! to `min(total, limit + offset)`.

mod support;

use serde_json::json;

#[tokio::test]
async fn list_conversations_is_ordered_by_last_message_desc_with_message_counts() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app, "/api/conversations").await;

    assert_eq!(page.status, 200);
    assert_eq!(page.body["total"], 2);
    let items = page.body["items"].as_array().expect("items");
    // k-1 was active on 11-12, k-2 on 11-11: newest activity first.
    assert_eq!(items[0]["id"], "k-1");
    assert_eq!(items[1]["id"], "k-2");
    assert_eq!(items[0]["message_count"], 5);
    assert_eq!(items[1]["message_count"], 1);
    // The position comes from the candidate, not from the conversation row.
    assert_eq!(items[0]["position_id"], "p-1");
    assert_eq!(items[0]["position_name"], "Senior Rust Engineer");
    assert_eq!(items[0]["is_active"], true);
    assert_eq!(items[1]["is_active"], false);
    assert_eq!(items[0]["last_message_at"], "2026-11-12T18:00:00Z");
}

#[tokio::test]
async fn list_conversations_filters_by_candidate() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app.clone(), "/api/conversations?candidate_id=c-2").await;

    assert_eq!(page.body["total"], 1);
    assert_eq!(page.body["items"][0]["id"], "k-2");

    let empty = support::get(app, "/api/conversations?candidate_id=c-3").await;
    assert_eq!(empty.status, 200);
    assert_eq!(empty.body["total"], 0);
    assert_eq!(empty.body["items"].as_array().expect("items").len(), 0);
}

#[tokio::test]
async fn create_conversation_returns_201_with_the_candidate_position() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/conversations",
        r#"{"candidate_id": "c-3", "title": "Intro"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["candidate_id"], "c-3");
    assert_eq!(response.body["channel"], "boss");
    assert_eq!(response.body["title"], "Intro");
    assert_eq!(response.body["is_active"], true);
    assert!(response.body["last_message_at"].is_null());
    assert_eq!(response.body["message_count"], 0);
    assert_eq!(response.body["position_id"], "p-4");
    assert_eq!(response.body["position_name"], "Archived Recruiter");
}

#[tokio::test]
async fn create_conversation_for_a_missing_candidate_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/conversations", r#"{"candidate_id": "nope"}"#).await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Candidate 'nope' was not found"
    );
}

#[tokio::test]
async fn create_conversation_without_a_candidate_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(app, "/api/conversations", "{}").await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn list_messages_is_ordered_by_message_time_asc_with_nulls_last() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app, "/api/messages?candidate_id=c-1").await;

    assert_eq!(page.status, 200);
    let ids: Vec<&str> = page.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect();
    // m-4 has no message_time: it sorts last, not first.
    assert_eq!(ids, ["m-1", "m-2", "m-3", "m-5", "m-4"]);
    assert!(page.body["items"][4]["message_time"].is_null());
}

/// IMP-1: the baseline reports `total = min(real, limit + offset)`.
#[tokio::test]
async fn messages_total_is_the_real_count_not_the_window() {
    let (app, _guard) = support::w1_app().await;

    let page = support::get(app.clone(), "/api/messages?candidate_id=c-1&limit=2").await;
    assert_eq!(page.body["items"].as_array().expect("items").len(), 2);
    assert_eq!(page.body["total"], 5, "IMP-1: real total, not limit");
    assert_eq!(page.body["has_more"], true, "IMP-1: pages are computable");

    let offset = support::get(
        app.clone(),
        "/api/messages?candidate_id=c-1&limit=2&offset=1",
    )
    .await;
    assert_eq!(offset.body["total"], 5);
    let ids: Vec<&str> = offset.body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["m-2", "m-3"]);

    let past_end = support::get(app, "/api/messages?candidate_id=c-1&offset=100").await;
    assert_eq!(past_end.body["total"], 5);
    assert_eq!(
        past_end.body["items"].as_array().expect("items").len(),
        0,
        "an offset past the end is an empty page, never an error"
    );
}

#[tokio::test]
async fn messages_reject_a_page_size_over_1000() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/messages?limit=1001").await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn create_message_reuses_the_candidates_active_conversation() {
    let (app, guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": "msg 0", "sender_type": "Recruiter"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["conversation_id"], "k-1");
    assert_eq!(response.body["channel"], "boss");
    assert_eq!(response.body["source"], "manual");
    assert_eq!(response.body["sender_type"], "Recruiter");
    assert!(response.body["message_time"].is_null());
    assert!(response.body["created_at"]
        .as_str()
        .expect("created_at")
        .ends_with('Z'));

    let pool = support::pool_for(&guard).await;
    // `sha256("msg 0")` — the fingerprint the Python baseline stores.
    let hash: String = sqlx::query_scalar("SELECT content_hash FROM messages WHERE id = ?")
        .bind(response.body["id"].as_str().expect("id"))
        .fetch_one(&pool)
        .await
        .expect("read the stored hash");
    assert_eq!(
        hash,
        "bf40f3ea98f97bcfdc40cec1c291497e2dd089d05653bd389189c57b6ece0aed"
    );
    // A message with no time still marks the conversation as just used.
    let last: String =
        sqlx::query_scalar("SELECT last_message_at FROM conversations WHERE id = 'k-1'")
            .fetch_one(&pool)
            .await
            .expect("last_message_at");
    assert!(!last.is_empty());
    assert_ne!(last, "2026-11-12 18:00:00.000000");
}

#[tokio::test]
async fn create_message_with_an_explicit_conversation_uses_it() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "conversation_id": "k-1", "content": "hello",
            "message_time": "2026-11-14T09:30:00Z"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    assert_eq!(response.body["conversation_id"], "k-1");
    assert_eq!(response.body["message_time"], "2026-11-14T09:30:00Z");
}

#[tokio::test]
async fn create_message_opens_a_conversation_when_none_is_active() {
    let (app, guard) = support::w1_app().await;

    // Bob's only conversation is inactive, so the baseline opens a new one.
    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-2", "content": "fresh"}"#,
    )
    .await;

    assert_eq!(response.status, 201);
    let conversation_id = response.body["conversation_id"].as_str().expect("id");
    assert_ne!(conversation_id, "k-2");

    let pool = support::pool_for(&guard).await;
    let channel: String = sqlx::query_scalar("SELECT channel FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .expect("the opened conversation");
    assert_eq!(channel, "boss");
    // The implicit open is not audited: only `POST /api/conversations` is.
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'conversation.created'",
            &[]
        )
        .await,
        0
    );
}

#[tokio::test]
async fn create_message_with_another_candidates_conversation_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "conversation_id": "k-2", "content": "wrong thread"}"#,
    )
    .await;

    assert_eq!(response.status, 422);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "validation_error",
                "message": "Conversation does not belong to the given candidate",
                "details": {"conversation_id": "k-2", "candidate_id": "c-1"}
            }
        })
    );
}

#[tokio::test]
async fn create_message_with_a_missing_conversation_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "conversation_id": "nope", "content": "x"}"#,
    )
    .await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Conversation 'nope' was not found"
    );
}

#[tokio::test]
async fn create_message_for_a_missing_candidate_is_404() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "nope", "content": "x"}"#,
    )
    .await;

    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Candidate 'nope' was not found"
    );
}

#[tokio::test]
async fn create_message_with_empty_content_is_422() {
    let (app, _guard) = support::w1_app().await;

    let response = support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": ""}"#,
    )
    .await;

    assert_eq!(response.status, 422);
}

#[tokio::test]
async fn timeline_is_an_array_of_message_entries() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates/c-1/timeline").await;

    assert_eq!(response.status, 200);
    let entries = response.body.as_array().expect("a bare array");
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0]["kind"], "message");
    assert_eq!(entries[0]["id"], "m-1");
    assert_eq!(entries[0]["content"], "msg 0");
    assert_eq!(entries[0]["source"], "manual");
    assert!(entries[0]["status"].is_null());
    assert_eq!(entries[0]["created_at"], "2026-11-10T10:00:00Z");
    assert_eq!(entries[4]["id"], "m-4", "the NULL-time row sorts last");
}

#[tokio::test]
async fn timeline_honours_limit() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates/c-1/timeline?limit=2").await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body.as_array().expect("array").len(), 2);
}

#[tokio::test]
async fn timeline_of_a_missing_candidate_is_404_not_an_empty_list() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates/nope/timeline").await;

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"]["code"], "not_found");
}

#[tokio::test]
async fn timeline_rejects_a_page_size_over_1000() {
    let (app, _guard) = support::w1_app().await;

    let response = support::get(app, "/api/candidates/c-1/timeline?limit=1001").await;

    assert_eq!(response.status, 422);
}
