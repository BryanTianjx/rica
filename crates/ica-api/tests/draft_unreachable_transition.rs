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
