//! Database side effects of the W1 write paths.
//!
//! The HTTP bodies are covered by the per-endpoint suites; this file asserts what
//! the *rows* look like afterwards, because a response can be right while the
//! audit trail, the content fingerprint or the ownership stamp is wrong.
//!
//! Every assertion here mirrors a Python behaviour the migration contract pins:
//! nine audit event types, `actor='local_user'` with `actor_id` NULL, the default
//! tenant, the deterministic `content_hash`, and the two FK actions
//! (`SET NULL` from position to candidate, `CASCADE` from candidate to children).

mod support;

use serde_json::Value;

/// The audit columns the contract pins, read back for assertion.
#[derive(sqlx::FromRow)]
struct AuditRow {
    event_type: String,
    entity_type: Option<String>,
    actor: String,
    actor_id: Option<String>,
    tenant_id: String,
    summary: Option<String>,
}

/// Perform one write of every kind the nine audit events describe.
async fn exercise_every_write(app: axum::Router) {
    support::post(app.clone(), "/api/positions", r#"{"name": "Audited Role"}"#).await;
    support::patch(
        app.clone(),
        "/api/positions/p-2",
        r#"{"location": "Nanjing"}"#,
    )
    .await;
    support::delete(app.clone(), "/api/positions/p-3").await;
    support::post(app.clone(), "/api/candidates", r#"{"name": "Ivan"}"#).await;
    support::patch(
        app.clone(),
        "/api/candidates/c-2",
        r#"{"status": "Contacted"}"#,
    )
    .await;
    support::post(
        app.clone(),
        "/api/conversations",
        r#"{"candidate_id": "c-3"}"#,
    )
    .await;
    support::post(
        app.clone(),
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": "audited message"}"#,
    )
    .await;
    support::post(
        app.clone(),
        "/api/send-records",
        r#"{"candidate_id": "c-2", "status": "sent", "channel": "wx"}"#,
    )
    .await;
    support::delete(app, "/api/candidates/c-3").await;
}

#[tokio::test]
async fn every_write_leaves_the_declared_audit_event() {
    let (app, guard) = support::w1_app().await;
    exercise_every_write(app).await;

    let pool = support::pool_for(&guard).await;
    let expected = [
        (
            "position.created",
            "position",
            Some("Created position 'Audited Role'"),
        ),
        (
            "position.updated",
            "position",
            Some("Updated position 'Frontend Developer'"),
        ),
        (
            "position.deleted",
            "position",
            Some("Deleted position 'Data Analyst'"),
        ),
        ("candidate.created", "candidate", Some("Created candidate")),
        ("candidate.updated", "candidate", Some("Updated candidate")),
        (
            "conversation.created",
            "conversation",
            Some("Opened conversation"),
        ),
        ("message.created", "message", Some("Recorded a message")),
        (
            "send.recorded",
            "send_record",
            Some("Recorded manual send with status 'sent'"),
        ),
        (
            "candidate.deleted",
            "candidate",
            Some("Deleted candidate and all dependent records"),
        ),
    ];

    for (event_type, entity_type, summary) in expected {
        let row: Option<AuditRow> = sqlx::query_as(concat!(
            "SELECT event_type, entity_type, actor, actor_id, tenant_id, summary",
            " FROM audit_events WHERE event_type = ?"
        ))
        .bind(event_type)
        .fetch_optional(&pool)
        .await
        .expect("read the audit event");

        let row = row.unwrap_or_else(|| panic!("no audit event of type {event_type}"));
        assert_eq!(row.event_type, event_type);
        assert_eq!(
            row.entity_type.as_deref(),
            Some(entity_type),
            "{event_type}"
        );
        assert_eq!(row.actor, "local_user", "{event_type} actor");
        assert!(
            row.actor_id.is_none(),
            "{event_type} keeps actor_id NULL in local mode"
        );
        assert_eq!(
            row.tenant_id, "00000000-0000-0000-0000-000000000000",
            "{event_type} tenant"
        );
        assert_eq!(row.summary.as_deref(), summary, "{event_type} summary");
    }
}

#[tokio::test]
async fn message_created_metadata_carries_the_source() {
    let (app, guard) = support::w1_app().await;

    support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": "hello", "source": "paste_import"}"#,
    )
    .await;

    let pool = support::pool_for(&guard).await;
    let raw: String = sqlx::query_scalar(
        "SELECT metadata_json FROM audit_events WHERE event_type = 'message.created'",
    )
    .fetch_one(&pool)
    .await
    .expect("metadata");
    let metadata: Value = serde_json::from_str(&raw).expect("metadata is JSON");
    assert_eq!(metadata, serde_json::json!({"source": "paste_import"}));
}

#[tokio::test]
async fn send_recorded_metadata_carries_the_six_declared_fields() {
    let (app, guard) = support::w1_app().await;

    support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "candidate_replied", "task_id": "t-1",
            "draft_id": "d-1", "channel": "wx"}"#,
    )
    .await;

    let pool = support::pool_for(&guard).await;
    let raw: String = sqlx::query_scalar(
        "SELECT metadata_json FROM audit_events WHERE event_type = 'send.recorded'",
    )
    .fetch_one(&pool)
    .await
    .expect("metadata");
    let metadata: Value = serde_json::from_str(&raw).expect("metadata is JSON");
    assert_eq!(
        metadata,
        serde_json::json!({
            "candidate_id": "c-1",
            "task_id": "t-1",
            "draft_id": "d-1",
            "channel": "wx",
            "status": "candidate_replied",
            "indicates_contact": true
        })
    );
}

#[tokio::test]
async fn a_send_record_that_means_no_contact_says_so_in_its_metadata() {
    let (app, guard) = support::w1_app().await;

    support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "not_sent"}"#,
    )
    .await;

    let pool = support::pool_for(&guard).await;
    let raw: String = sqlx::query_scalar(
        "SELECT metadata_json FROM audit_events WHERE event_type = 'send.recorded'",
    )
    .fetch_one(&pool)
    .await
    .expect("metadata");
    let metadata: Value = serde_json::from_str(&raw).expect("metadata is JSON");
    assert_eq!(metadata["indicates_contact"], false);
}

#[tokio::test]
async fn created_rows_carry_the_local_owner_and_default_tenant() {
    let (app, guard) = support::w1_app().await;

    support::post(app.clone(), "/api/positions", r#"{"name": "Owned Role"}"#).await;
    support::post(app.clone(), "/api/candidates", r#"{"name": "Judy"}"#).await;
    support::post(
        app.clone(),
        "/api/conversations",
        r#"{"candidate_id": "c-1"}"#,
    )
    .await;
    support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": "owned"}"#,
    )
    .await;

    let pool = support::pool_for(&guard).await;
    for (table, condition) in [
        ("positions", "name = 'Owned Role'"),
        ("candidates", "name = 'Judy'"),
        ("messages", "content = 'owned'"),
    ] {
        let sql = format!(
            "SELECT COUNT(*) FROM {table} WHERE {condition} AND owner_id = ? AND tenant_id = ?"
        );
        assert_eq!(
            support::count(
                &pool,
                &sql,
                &[
                    "00000000-0000-0000-0000-000000000001",
                    "00000000-0000-0000-0000-000000000000"
                ]
            )
            .await,
            1,
            "{table} ownership stamps"
        );
    }
}

#[tokio::test]
async fn the_message_content_hash_matches_the_python_normalisation() {
    let (app, guard) = support::w1_app().await;

    // Leading/trailing space, an inner run of whitespace and mixed case.
    support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": "  Hello   World  "}"#,
    )
    .await;

    let pool = support::pool_for(&guard).await;
    let hash: String =
        sqlx::query_scalar("SELECT content_hash FROM messages WHERE content LIKE '%Hello%'")
            .fetch_one(&pool)
            .await
            .expect("stored hash");
    // sha256("hello world")
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[tokio::test]
async fn a_message_moves_its_conversations_last_message_at() {
    let (app, guard) = support::w1_app().await;

    support::post(
        app,
        "/api/messages",
        r#"{"candidate_id": "c-1", "content": "later", "message_time": "2026-11-20T08:00:00Z"}"#,
    )
    .await;

    let pool = support::pool_for(&guard).await;
    let last: String =
        sqlx::query_scalar("SELECT last_message_at FROM conversations WHERE id = 'k-1'")
            .fetch_one(&pool)
            .await
            .expect("last_message_at");
    // Stored the way SQLAlchemy writes a naive UTC datetime.
    assert_eq!(last, "2026-11-20 08:00:00.000000");
}

#[tokio::test]
async fn deleting_a_candidate_cascades_to_conversations_and_messages() {
    let (app, guard) = support::w1_app().await;

    support::delete(app, "/api/candidates/c-1").await;

    let pool = support::pool_for(&guard).await;
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM conversations WHERE candidate_id = ?",
            &["c-1"]
        )
        .await,
        0
    );
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM messages WHERE candidate_id = ?",
            &["c-1"]
        )
        .await,
        0
    );
}

#[tokio::test]
async fn deleting_a_position_nulls_the_candidates_link_and_keeps_the_row() {
    let (app, guard) = support::w1_app().await;

    support::delete(app, "/api/positions/p-1").await;

    let pool = support::pool_for(&guard).await;
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM candidates WHERE id IN ('c-1', 'c-2')")
            .fetch_one(&pool)
            .await
            .expect("count candidates");
    assert_eq!(left, 2, "the candidate rows survive the delete");
    let linked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM candidates WHERE position_id = 'p-1'")
            .fetch_one(&pool)
            .await
            .expect("count links");
    assert_eq!(linked, 0, "ON DELETE SET NULL cleared the link");
}

#[tokio::test]
async fn a_failed_write_leaves_no_row_and_no_audit_event() {
    let (app, guard) = support::w1_app().await;

    // 409: the draft is not sendable. Nothing may be appended.
    let response = support::post(
        app,
        "/api/send-records",
        r#"{"candidate_id": "c-1", "status": "sent", "draft_id": "d-2"}"#,
    )
    .await;
    assert_eq!(response.status, 409);

    let pool = support::pool_for(&guard).await;
    assert_eq!(
        support::count(&pool, "SELECT COUNT(*) FROM send_records", &[]).await,
        4,
        "the seeded four, unchanged"
    );
    assert_eq!(
        support::count(
            &pool,
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'send.recorded'",
            &[]
        )
        .await,
        0
    );
}
