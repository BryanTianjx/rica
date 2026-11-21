//! `GET /api/audit-events` (W5 first slice).
//!
//! The contract is the baseline's `read_audit_events`: a **bare array**
//! (like the timeline), newest first, equality filters that ignore an empty
//! query value, `limit` default 50 with `gt=0 le=500`, and pydantic-shaped
//! 422s for the violations. The metadata blob degrades to `{}` rather than
//! hiding the event.

mod support;

use support::{get, pool_for, post, w1_app};

/// Seed audit rows with distinct, controlled timestamps so the ordering
/// assertions are deterministic.
async fn seed_events(
    pool: &sqlx::SqlitePool,
    events: &[(&str, &str, Option<&str>, &str)], // (id, event_type, entity_id, created_at)
) {
    for (id, event_type, entity_id, created_at) in events {
        sqlx::query(concat!(
            "INSERT INTO audit_events (id, event_type, entity_type, entity_id, actor,",
            " summary, metadata_json, tenant_id, actor_id, created_at, updated_at)",
            " VALUES (?, ?, 'candidate', ?, 'local_user', NULL, NULL,",
            " '00000000-0000-0000-0000-000000000000', NULL, ?, ?)"
        ))
        .bind(id)
        .bind(event_type)
        .bind(*entity_id)
        .bind(created_at)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("seed audit event");
    }
}

#[tokio::test]
async fn the_trail_is_a_bare_array_of_newest_first_entries() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;
    seed_events(
        &pool,
        &[
            (
                "e-1",
                "candidate.created",
                Some("c-1"),
                "2026-11-20 09:00:00",
            ),
            (
                "e-2",
                "candidate.updated",
                Some("c-1"),
                "2026-11-20 10:00:00",
            ),
            (
                "e-3",
                "candidate.deleted",
                Some("c-2"),
                "2026-11-20 11:00:00",
            ),
        ],
    )
    .await;

    let response = get(app, "/api/audit-events").await;
    assert_eq!(response.status, 200, "body: {}", response.body);
    let items = response.body.as_array().expect("a bare array");
    let ids: Vec<&str> = items
        .iter()
        .map(|event| event["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, vec!["e-3", "e-2", "e-1"], "newest first: {:?}", items);

    let first = &items[0];
    assert_eq!(first["event_type"], "candidate.deleted");
    assert_eq!(first["entity_type"], "candidate");
    assert_eq!(first["entity_id"], "c-2");
    assert_eq!(first["actor"], "local_user");
    assert!(first["summary"].is_null());
    assert!(
        first["metadata"].is_object(),
        "metadata is always an object"
    );
    assert!(first["created_at"].is_string());
}

#[tokio::test]
async fn an_empty_trail_is_an_empty_array_not_a_page() {
    let (app, _dir) = w1_app().await;
    let response = get(app, "/api/audit-events").await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, serde_json::json!([]));
}

#[tokio::test]
async fn a_business_write_lands_in_the_trail() {
    let (app, _dir) = w1_app().await;
    let response = post(app.clone(), "/api/positions", r#"{"name":"Trail Probe"}"#).await;
    assert_eq!(response.status, 201, "body: {}", response.body);

    let trail = get(app, "/api/audit-events?event_type=position.created").await;
    let items = trail.body.as_array().expect("a bare array");
    assert_eq!(items.len(), 1, "the create is audited: {}", trail.body);
    assert_eq!(items[0]["event_type"], "position.created");
    assert_eq!(items[0]["entity_type"], "position");
}

#[tokio::test]
async fn filters_narrow_to_one_entity_and_empty_values_are_ignored() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;
    seed_events(
        &pool,
        &[
            (
                "e-1",
                "candidate.created",
                Some("c-1"),
                "2026-11-20 09:00:00",
            ),
            (
                "e-2",
                "candidate.updated",
                Some("c-2"),
                "2026-11-20 10:00:00",
            ),
        ],
    )
    .await;

    // The entity filter narrows the trail.
    let filtered = get(app.clone(), "/api/audit-events?entity_id=c-2").await;
    let items = filtered.body.as_array().expect("a bare array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "e-2");

    // An EMPTY value means "no filter" (the baseline's truthiness), not
    // "filter for the empty string".
    let empty = get(app, "/api/audit-events?event_type=&entity_id=").await;
    let items = empty.body.as_array().expect("a bare array");
    assert_eq!(items.len(), 2, "empty filters are ignored: {}", empty.body);
}

#[tokio::test]
async fn limit_bounds_follow_the_shared_page_contract() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;
    seed_events(
        &pool,
        &[
            ("e-1", "candidate.created", None, "2026-11-20 09:00:00"),
            ("e-2", "candidate.updated", None, "2026-11-20 10:00:00"),
        ],
    )
    .await;

    let one = get(app.clone(), "/api/audit-events?limit=1").await;
    assert_eq!(one.body.as_array().expect("array").len(), 1);

    for (uri, kind, message) in [
        (
            "/api/audit-events?limit=0",
            "greater_than",
            "Input should be greater than 0",
        ),
        (
            "/api/audit-events?limit=501",
            "less_than_equal",
            "Input should be less than or equal to 500",
        ),
    ] {
        let response = get(app.clone(), uri).await;
        assert_eq!(response.status, 422, "{uri}: {}", response.body);
        let issue = &response.body["error"]["details"]["errors"][0];
        assert_eq!(issue["type"], kind, "{uri}");
        assert_eq!(issue["msg"], message, "{uri}");
        assert_eq!(issue["loc"], serde_json::json!(["query", "limit"]));
    }
}

#[tokio::test]
async fn a_filter_longer_than_the_pydantic_max_is_a_422() {
    let (app, _dir) = w1_app().await;
    let long = "x".repeat(65);
    let response = get(app.clone(), &format!("/api/audit-events?event_type={long}")).await;
    assert_eq!(response.status, 422, "body: {}", response.body);
    let issue = &response.body["error"]["details"]["errors"][0];
    assert_eq!(issue["type"], "string_too_long");
    assert_eq!(issue["loc"], serde_json::json!(["query", "event_type"]));

    // 64 characters exactly is accepted.
    let edge = "x".repeat(64);
    let ok = get(app, &format!("/api/audit-events?event_type={edge}")).await;
    assert_eq!(ok.status, 200, "64 chars pass: {}", ok.body);
}

#[tokio::test]
async fn a_broken_metadata_blob_degrades_to_an_empty_object() {
    let (app, dir) = w1_app().await;
    let pool = pool_for(&dir).await;
    sqlx::query(concat!(
        "INSERT INTO audit_events (id, event_type, entity_type, actor,",
        " summary, metadata_json, tenant_id, created_at, updated_at)",
        " VALUES ('e-broken', 'candidate.created', 'candidate', 'local_user',",
        " NULL, 'not-json-at-all', '00000000-0000-0000-0000-000000000000',",
        " '2026-11-20 09:00:00', '2026-11-20 09:00:00')"
    ))
    .execute(&pool)
    .await
    .expect("seed the broken row");

    let response = get(app, "/api/audit-events").await;
    let items = response.body.as_array().expect("a bare array");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["metadata"],
        serde_json::json!({}),
        "a broken blob must not hide the event: {}",
        response.body
    );
    assert_eq!(items[0]["id"], "e-broken");
}
