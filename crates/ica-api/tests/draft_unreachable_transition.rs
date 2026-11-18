//! A draft whose follow-up task cannot reach the target status.
//!
//! The baseline mutates the draft and advances its task inside **one**
//! transaction. When the state machine refuses the target, the whole thing
//! rolls back: the client gets 409 `business_rule_violation` and the draft
//! keeps its previous status.
//!
//! The port's two writes are separate transactions, so an advance that failed
//! *after* the draft write left the row mutated — the response was right but
//! the stored draft claimed `approved` for a request that never succeeded.
//! These tests pin the observable outcome: 409, and an untouched draft row.

mod support;

use support::{pool_for, post, w1_app};

/// Insert a fresh `generated` draft bound to Bob's `Completed` task `t-3`.
///
/// `Completed` is terminal for the workflow: it cannot reach `Approved`,
/// `Rejected` or `AwaitingResult`, so every one of the three actions has to
/// be refused by the state machine.
async fn draft_on_a_completed_task(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query(concat!(
        "INSERT INTO generated_drafts (id, candidate_id, task_id, message_type, tone,",
        " status, objective, message_text, alternatives_json, context_references_json,",
        " uncertainties_json, warnings_json, validation_status, validation_messages_json,",
        " quality_status, provider, model, prompt_version, is_edited, approved_at,",
        " copied_at, rejected_at, draft_group_id, cost_cents, prompt_tokens,",
        " completion_tokens, routing_decision_json, created_at, updated_at)",
        " VALUES (?, 'c-2', 't-3', 'reply', 'friendly', 'generated', NULL, 'draft body',",
        " NULL, NULL, NULL, NULL, 'Passed', NULL, NULL, NULL, NULL, NULL, 0, NULL,",
        " NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?, ?)"
    ))
    .bind(id)
    .bind("2026-11-10 09:00:00.000000")
    .bind("2026-11-10 09:00:00.000000")
    .execute(pool)
    .await
    .expect("insert draft on the completed task");
}

/// The draft's status and the three action timestamps, or `None` if absent.
async fn draft_row(pool: &sqlx::SqlitePool, id: &str) -> Option<(String, bool, bool, bool)> {
    sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT status, approved_at, copied_at, rejected_at FROM generated_drafts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("read draft")
    .map(|(status, approved, copied, rejected)| {
        (
            status,
            approved.is_some(),
            copied.is_some(),
            rejected.is_some(),
        )
    })
}

#[tokio::test]
async fn approve_on_an_unreachable_task_is_409_and_leaves_the_draft_alone() {
    let (app, guard) = w1_app().await;
    let pool = pool_for(&guard).await;
    draft_on_a_completed_task(&pool, "d-unreachable-approve").await;

    let response = post(
        app,
        "/api/drafts/d-unreachable-approve/approve",
        r#"{"note": "looks good"}"#,
    )
    .await;

    assert_eq!(response.status, 409);
    assert_eq!(
        response.body["error"]["code"], "business_rule_violation",
        "body: {}",
        response.body
    );
    assert_eq!(
        response.body["error"]["message"],
        "A task in state Completed cannot reach Approved"
    );
    assert_eq!(response.body["error"]["details"]["current"], "Completed");
    assert_eq!(response.body["error"]["details"]["requested"], "Approved");

    assert_eq!(
        draft_row(&pool, "d-unreachable-approve").await,
        Some(("generated".to_string(), false, false, false)),
        "the draft must keep its status when the transition is refused"
    );
}

#[tokio::test]
async fn reject_on_an_unreachable_task_is_409_and_leaves_the_draft_alone() {
    let (app, guard) = w1_app().await;
    let pool = pool_for(&guard).await;
    draft_on_a_completed_task(&pool, "d-unreachable-reject").await;

    let response = post(app, "/api/drafts/d-unreachable-reject/reject", "").await;

    assert_eq!(response.status, 409);
    assert_eq!(response.body["error"]["code"], "business_rule_violation");
    assert_eq!(
        response.body["error"]["message"],
        "A task in state Completed cannot reach Rejected"
    );

    assert_eq!(
        draft_row(&pool, "d-unreachable-reject").await,
        Some(("generated".to_string(), false, false, false)),
        "the draft must keep its status when the transition is refused"
    );
}

#[tokio::test]
async fn copy_on_an_unreachable_task_is_409_and_leaves_the_draft_alone() {
    let (app, guard) = w1_app().await;
    let pool = pool_for(&guard).await;
    draft_on_a_completed_task(&pool, "d-unreachable-copy").await;

    let response = post(app, "/api/drafts/d-unreachable-copy/copy", "").await;

    assert_eq!(response.status, 409);
    assert_eq!(response.body["error"]["code"], "business_rule_violation");
    assert_eq!(
        response.body["error"]["message"],
        "A task in state Completed cannot reach AwaitingResult"
    );

    assert_eq!(
        draft_row(&pool, "d-unreachable-copy").await,
        Some(("generated".to_string(), false, false, false)),
        "the draft must keep its status when the transition is refused"
    );
}

/// Insert two `generated` drafts in one comparison group, both bound to Bob's
/// `Completed` task `t-3`, so selecting one would advance the task to
/// `Reviewing`.
async fn group_on_a_completed_task(pool: &sqlx::SqlitePool, group_id: &str, ids: [&str; 2]) {
    for id in ids {
        sqlx::query(concat!(
            "INSERT INTO generated_drafts (id, candidate_id, task_id, message_type, tone,",
            " status, objective, message_text, alternatives_json, context_references_json,",
            " uncertainties_json, warnings_json, validation_status, validation_messages_json,",
            " quality_status, provider, model, prompt_version, is_edited, approved_at,",
            " copied_at, rejected_at, draft_group_id, cost_cents, prompt_tokens,",
            " completion_tokens, routing_decision_json, created_at, updated_at)",
            " VALUES (?, 'c-2', 't-3', 'reply', 'friendly', 'generated', NULL, 'draft body',",
            " NULL, NULL, NULL, NULL, 'Passed', NULL, NULL, NULL, NULL, NULL, 0, NULL,",
            " NULL, NULL, ?, NULL, NULL, NULL, NULL, ?, ?)"
        ))
        .bind(id)
        .bind(group_id)
        .bind("2026-11-10 09:00:00.000000")
        .bind("2026-11-10 09:00:00.000000")
        .execute(pool)
        .await
        .expect("insert grouped draft");
    }
}

#[tokio::test]
async fn select_from_a_group_on_an_unreachable_task_is_409_and_touches_no_sibling() {
    let (app, guard) = w1_app().await;
    let pool = pool_for(&guard).await;
    group_on_a_completed_task(&pool, "g-unreachable", ["d-group-a", "d-group-b"]).await;

    let response = post(
        app,
        "/api/draft-groups/g-unreachable/select",
        r#"{"draft_id": "d-group-a"}"#,
    )
    .await;

    assert_eq!(response.status, 409);
    assert_eq!(response.body["error"]["code"], "business_rule_violation");
    assert_eq!(
        response.body["error"]["message"],
        "A task in state Completed cannot reach Reviewing"
    );

    // The baseline rejects the siblings and advances the task in one
    // transaction; a refused transition must leave the whole group alone.
    for id in ["d-group-a", "d-group-b"] {
        assert_eq!(
            draft_row(&pool, id).await,
            Some(("generated".to_string(), false, false, false)),
            "{id} must keep its status when the transition is refused"
        );
    }
}

/// A draft whose `task_id` points at a row that does not exist.
///
/// The baseline's `get_task` raises `NotFoundError` before anything is
/// written, so the request is a 404 and the draft keeps its status. The port
/// used to answer 200 because `advance` treats a missing row as "nothing to
/// update" and reports success — the draft was mutated for a request that
/// should have failed.
async fn draft_on_a_missing_task(pool: &sqlx::SqlitePool, id: &str) {
    // `generated_drafts.task_id` is guarded by a foreign key, so a dangling
    // value cannot be inserted under enforcement. It can still exist in a
    // database written with `PRAGMA foreign_keys = OFF` (the Python baseline
    // runs with SQLite's default, which is off) or left behind when a task
    // row disappears, so the endpoint has to cope with it. Disable the
    // pragma on this one connection to build that fixture, then restore it.
    let mut connection = pool.acquire().await.expect("acquire a connection");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .expect("disable foreign keys for the fixture");
    sqlx::query(concat!(
        "INSERT INTO generated_drafts (id, candidate_id, task_id, message_type, tone,",
        " status, objective, message_text, alternatives_json, context_references_json,",
        " uncertainties_json, warnings_json, validation_status, validation_messages_json,",
        " quality_status, provider, model, prompt_version, is_edited, approved_at,",
        " copied_at, rejected_at, draft_group_id, cost_cents, prompt_tokens,",
        " completion_tokens, routing_decision_json, created_at, updated_at)",
        " VALUES (?, 'c-2', 'task-that-does-not-exist', 'reply', 'friendly', 'generated',",
        " NULL, 'draft body', NULL, NULL, NULL, NULL, 'Passed', NULL, NULL, NULL, NULL,",
        " NULL, 0, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?, ?)"
    ))
    .bind(id)
    .bind("2026-11-10 09:00:00.000000")
    .bind("2026-11-10 09:00:00.000000")
    .execute(&mut *connection)
    .await
    .expect("insert draft on a missing task");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await
        .expect("restore foreign keys");
}

#[tokio::test]
async fn approve_with_a_dangling_task_id_is_404_and_leaves_the_draft_alone() {
    let (app, guard) = w1_app().await;
    let pool = pool_for(&guard).await;
    draft_on_a_missing_task(&pool, "d-dangling").await;

    let response = post(app, "/api/drafts/d-dangling/approve", r#"{"note": "ok"}"#).await;

    assert_eq!(response.status, 404, "body: {}", response.body);
    assert_eq!(response.body["error"]["code"], "not_found");
    assert_eq!(
        response.body["error"]["message"],
        "Follow-up task 'task-that-does-not-exist' was not found"
    );
    assert_eq!(
        draft_row(&pool, "d-dangling").await,
        Some(("generated".to_string(), false, false, false)),
        "nothing may be written when the task is missing"
    );
}

#[tokio::test]
async fn copy_with_a_dangling_task_id_is_404_and_leaves_the_draft_alone() {
    let (app, guard) = w1_app().await;
    let pool = pool_for(&guard).await;
    draft_on_a_missing_task(&pool, "d-dangling-copy").await;

    let response = post(app, "/api/drafts/d-dangling-copy/copy", "").await;

    assert_eq!(response.status, 404, "body: {}", response.body);
    assert_eq!(response.body["error"]["code"], "not_found");
    assert_eq!(
        draft_row(&pool, "d-dangling-copy").await,
        Some(("generated".to_string(), false, false, false))
    );
}
