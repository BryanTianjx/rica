//! HTTP contract tests for the W2 follow-up endpoints: the 12-state machine
//! (measured against the baseline in `docs/contract-baseline/w2-python-facts.md`
//! §1), the decision engine's stable branches, the policy ranges, the expiry
//! sweep and the dashboard counts.

mod support;

use serde_json::json;
use support::{get, patch, post, w1_app};

/// The manual-settable statuses a raw PATCH may write.
const MANUAL: [&str; 6] = [
    "Pending",
    "Eligible",
    "Drafting",
    "Reviewing",
    "Rejected",
    "Cancelled",
];

const STATUSES: [&str; 12] = [
    "Pending",
    "Eligible",
    "Drafting",
    "Reviewing",
    "Rejected",
    "Approved",
    "Copied",
    "AwaitingResult",
    "Completed",
    "Cancelled",
    "Expired",
    "Blocked",
];

/// Craft a fresh task row in exactly `status`, overwriting any prior row with
/// the same id — the same trick the baseline probe used, because the API
/// cannot set `Approved`/`Copied`/`AwaitingResult` by hand.
async fn craft_task(dir: &tempfile::TempDir, id: &str, status: &str) {
    let pool = support::pool_for(dir).await;
    sqlx::query("DELETE FROM follow_up_tasks WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .expect("delete probe row");
    sqlx::query(concat!(
        "INSERT INTO follow_up_tasks (id, candidate_id, position_id, status, objective,",
        " reason, block_reason, recommended_at, expires_at, last_follow_up_at,",
        " follow_up_count, completed_at, result_type, result_note, result_recorded_at,",
        " created_at, updated_at) VALUES (?, 'c-1', NULL, ?, NULL, NULL, NULL, NULL,",
        " NULL, NULL, 0, NULL, NULL, NULL, NULL, '2026-11-10 09:00:00.000000',",
        " '2026-11-10 09:00:00.000000')"
    ))
    .bind(id)
    .bind(status)
    .execute(&pool)
    .await
    .expect("insert probe row");
    pool.close().await;
}

/// Card W2 AC2: every `(current, target)` of the state machine, driven through
/// real HTTP, matching the baseline's measured answers cell by cell.
#[tokio::test]
async fn the_state_machine_walks_like_the_baseline() {
    let (app, dir) = w1_app().await;

    for current in STATUSES {
        for target in MANUAL {
            craft_task(&dir, "mat-probe", current).await;
            let body = format!(r#"{{"status": "{target}"}}"#);
            let allowed = current == target
                || matches!(
                    (current, target),
                    ("Pending", "Eligible")
                        | ("Pending", "Cancelled")
                        | ("Eligible", "Drafting")
                        | ("Eligible", "Cancelled")
                        | ("Drafting", "Reviewing")
                        | ("Drafting", "Rejected")
                        | ("Drafting", "Cancelled")
                        | ("Reviewing", "Drafting")
                        | ("Reviewing", "Rejected")
                        | ("Reviewing", "Cancelled")
                        | ("Rejected", "Cancelled")
                        | ("Approved", "Cancelled")
                        | ("Copied", "Cancelled")
                        | ("AwaitingResult", "Cancelled")
                        | ("Blocked", "Cancelled")
                );
            let response = patch(app.clone(), "/api/follow-ups/mat-probe", &body).await;
            if allowed {
                assert_eq!(response.status, 200, "{current} PATCH {target}");
                assert_eq!(response.body["status"], target, "{current} PATCH {target}");
            } else {
                assert_eq!(response.status, 409, "{current} PATCH {target}");
                assert_eq!(
                    response.body["error"]["message"],
                    format!("Illegal follow-up transition {current} → {target}"),
                    "{current} PATCH {target}"
                );
            }
        }

        craft_task(&dir, "mat-probe", current).await;
        let response = post(app.clone(), "/api/follow-ups/mat-probe/cancel", "").await;
        if matches!(current, "Completed" | "Expired") {
            assert_eq!(response.status, 409, "{current} CANCEL");
            assert_eq!(
                response.body["error"]["message"],
                format!("A task in state {current} cannot be cancelled")
            );
        } else {
            assert_eq!(response.status, 200, "{current} CANCEL");
            assert_eq!(response.body["status"], "Cancelled");
        }

        craft_task(&dir, "mat-probe", current).await;
        let response = post(app.clone(), "/api/follow-ups/mat-probe/complete", "").await;
        if matches!(current, "Completed" | "Copied" | "AwaitingResult") {
            assert_eq!(response.status, 200, "{current} COMPLETE");
            assert_eq!(response.body["status"], "Completed");
        } else {
            assert_eq!(response.status, 409, "{current} COMPLETE");
            assert_eq!(
                response.body["error"]["message"],
                format!("A task in state {current} cannot be completed")
            );
        }

        // record-result answers 200 from every state; a task that may legally
        // complete becomes Completed, everything else keeps its status.
        craft_task(&dir, "mat-probe", current).await;
        let response = post(
            app.clone(),
            "/api/follow-ups/mat-probe/record-result",
            r#"{"result_type": "sent"}"#,
        )
        .await;
        assert_eq!(response.status, 200, "{current} RESULT");
        let completable = matches!(
            current,
            "Pending"
                | "Eligible"
                | "Drafting"
                | "Reviewing"
                | "Approved"
                | "Copied"
                | "AwaitingResult"
        );
        if completable {
            assert_eq!(response.body["status"], "Completed", "{current} RESULT");
        } else {
            assert_eq!(response.body["status"], current, "{current} RESULT");
        }
        assert_eq!(response.body["follow_up_count"], 1, "{current} RESULT");
        assert!(response.body["result_recorded_at"].is_string());
    }
}

/// The semantic states cannot be written by hand — the 409 ships the
/// `settable` list as a JSON array.
#[tokio::test]
async fn a_semantic_state_cannot_be_set_by_a_raw_patch() {
    let (app, _dir) = w1_app().await;

    let response = patch(app, "/api/follow-ups/t-2", r#"{"status": "Approved"}"#).await;

    assert_eq!(response.status, 409);
    assert_eq!(
        response.body["error"]["message"],
        "'Approved' cannot be set directly; it is recorded by the action it represents"
    );
    let details = &response.body["error"]["details"];
    assert_eq!(details["current"], "Pending");
    assert_eq!(details["requested"], "Approved");
    assert_eq!(
        details["settable"],
        json!([
            "Cancelled",
            "Drafting",
            "Eligible",
            "Pending",
            "Rejected",
            "Reviewing"
        ])
    );
}

/// An unknown status string is a 422 enum error.
#[tokio::test]
async fn an_unknown_status_is_a_422_enum_error() {
    let (app, _dir) = w1_app().await;

    let response = patch(app, "/api/follow-ups/t-2", r#"{"status": "Nonsense"}"#).await;

    assert_eq!(response.status, 422);
    assert_eq!(response.body["error"]["code"], "validation_error");
}

/// record-result is idempotent: the same outcome twice counts one attempt and
/// appends one send-record row; a different outcome corrects without counting
/// again but appends a corrected row.
#[tokio::test]
async fn recording_results_is_idempotent_and_logs_sends() {
    let (app, dir) = w1_app().await;

    let first = post(
        app.clone(),
        "/api/follow-ups/t-2/record-result",
        r#"{"result_type": "sent"}"#,
    )
    .await;
    assert_eq!(first.status, 200);
    assert_eq!(first.body["status"], "Completed");
    assert_eq!(first.body["follow_up_count"], 1);

    let repeat = post(
        app.clone(),
        "/api/follow-ups/t-2/record-result",
        r#"{"result_type": "sent"}"#,
    )
    .await;
    assert_eq!(repeat.status, 200);
    assert_eq!(
        repeat.body["follow_up_count"], 1,
        "same outcome: no recount"
    );

    let correction = post(
        app,
        "/api/follow-ups/t-2/record-result",
        r#"{"result_type": "not_sent", "note": "typo"}"#,
    )
    .await;
    assert_eq!(correction.status, 200);
    assert_eq!(
        correction.body["follow_up_count"], 1,
        "correction: no recount"
    );
    assert_eq!(correction.body["result_type"], "not_sent");
    // The task keeps the raw note; the `correction:` prefix belongs to the
    // send log only (baseline: `task.result_note = note`, and the record's
    // note is prefixed separately).
    assert_eq!(correction.body["result_note"], "typo");

    let pool = support::pool_for(&dir).await;
    let send_rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT note FROM send_records WHERE task_id = 't-2' ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("read send records");
    pool.close().await;
    assert_eq!(send_rows.len(), 2, "first + correction, never the replay");
    assert_eq!(send_rows[0].0.as_deref(), None, "first record: raw note");
    assert_eq!(
        send_rows[1].0.as_deref(),
        Some("correction: typo"),
        "correction: prefixed note on the send log only"
    );
}

/// The decision engine's stable branches (fixed seed timestamps, so no
/// wall-clock dependence).
#[tokio::test]
async fn evaluate_answers_the_stable_branches() {
    let (app, dir) = w1_app().await;

    // Alice: last message is the recruiter's (2026-11-12 18:00), one contact
    // task, both older than 48h — the ≥48h branch with a fixed string.
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        r#"{"candidate_id": "c-1"}"#,
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["outcome"], "eligible");
    assert_eq!(
        response.body["reasons"],
        json!(["At least 48h since the last follow-up attempt."])
    );
    assert_eq!(response.body["suggested_objective"], "polite_follow_up");
    assert_eq!(response.body["risks"], json!([]));

    // Bob: his last message came from the candidate — the reply branch.
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        r#"{"candidate_id": "c-2"}"#,
    )
    .await;
    assert_eq!(
        response.body["reasons"],
        json!(["The last message came from the candidate; a reply is expected."])
    );
    assert_eq!(response.body["suggested_objective"], "reply");

    // A candidate with no messages at all — the greeting branch, plus the
    // no-position risk.
    let created = post(app.clone(), "/api/candidates", r#"{"name": "Probe Fresh"}"#).await;
    let candidate_id = created.body["id"].as_str().unwrap().to_string();
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        &format!(r#"{{"candidate_id": "{candidate_id}"}}"#),
    )
    .await;
    assert_eq!(
        response.body["reasons"],
        json!(["No prior communication recorded."])
    );
    assert_eq!(response.body["suggested_objective"], "greeting");
    assert_eq!(
        response.body["risks"],
        json!(["The candidate is not linked to a position, so the message cannot reference position requirements."])
    );

    // A rejected candidate blocks before any timing is considered.
    let created = post(
        app.clone(),
        "/api/candidates",
        r#"{"name": "Probe Rejected", "status": "Rejected"}"#,
    )
    .await;
    let rejected_id = created.body["id"].as_str().unwrap().to_string();
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        &format!(r#"{{"candidate_id": "{rejected_id}"}}"#),
    )
    .await;
    assert_eq!(response.body["outcome"], "blocked");
    assert_eq!(
        response.body["block_reasons"],
        json!(["Candidate status is Rejected; ordinary follow-up is not allowed."])
    );

    // An interview-stage candidate is a distinct block reason.
    let created = post(
        app.clone(),
        "/api/candidates",
        r#"{"name": "Probe Interview", "status": "Interview"}"#,
    )
    .await;
    let interview_id = created.body["id"].as_str().unwrap().to_string();
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        &format!(r#"{{"candidate_id": "{interview_id}"}}"#),
    )
    .await;
    assert_eq!(
        response.body["block_reasons"],
        json!(["Candidate entered the interview stage; use interview follow-up instead."])
    );

    // The count limit blocks before timing: give Bob a second contact task.
    let pool = support::pool_for(&dir).await;
    sqlx::query(concat!(
        "INSERT INTO follow_up_tasks (id, candidate_id, status, follow_up_count,",
        " created_at, updated_at) VALUES ('mat-limit-1', 'c-2', 'Completed', 1,",
        " '2026-11-10 09:00:00.000000', '2026-11-10 09:00:00.000000')"
    ))
    .execute(&pool)
    .await
    .expect("insert limit task");
    pool.close().await;
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        r#"{"candidate_id": "c-2"}"#,
    )
    .await;
    assert_eq!(
        response.body["block_reasons"],
        json!(["Active follow-up count 2 reached the limit of 2."])
    );

    // A position override can block an otherwise eligible candidate.
    let response = post(
        app.clone(),
        "/api/follow-ups/evaluate",
        r#"{"candidate_id": "c-1", "position_id": "p-4"}"#,
    )
    .await;
    assert_eq!(
        response.body["block_reasons"],
        json!(["Position status is Closed; ordinary follow-up is not allowed."])
    );

    // Unknown candidate: the baseline's 404 wording.
    let response = post(
        app,
        "/api/follow-ups/evaluate",
        r#"{"candidate_id": "does-not-exist"}"#,
    )
    .await;
    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Candidate 'does-not-exist' was not found"
    );
}

/// Policy: the default row is created on first use, ranges are enforced, and
/// an explicit null leaves the field alone.
#[tokio::test]
async fn policy_creates_the_default_row_and_enforces_the_ranges() {
    let (app, dir) = w1_app().await;

    let response = get(app.clone(), "/api/follow-ups/policy/current").await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["name"], "default");
    assert_eq!(response.body["first_follow_up_delay_hours"], 24);
    assert_eq!(response.body["max_active_follow_ups"], 2);
    assert_eq!(response.body["min_interval_hours"], 48);
    assert_eq!(response.body["task_validity_days"], 7);
    assert_eq!(response.body["is_active"], true);

    let response = patch(
        app.clone(),
        "/api/follow-ups/policy/current",
        r#"{"max_active_follow_ups": 3}"#,
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["max_active_follow_ups"], 3);

    for body in [
        r#"{"max_active_follow_ups": 0}"#,
        r#"{"max_active_follow_ups": 21}"#,
        r#"{"first_follow_up_delay_hours": -1}"#,
        r#"{"first_follow_up_delay_hours": 721}"#,
        r#"{"min_interval_hours": -1}"#,
        r#"{"task_validity_days": 0}"#,
        r#"{"task_validity_days": 366}"#,
    ] {
        let response = patch(app.clone(), "/api/follow-ups/policy/current", body).await;
        assert_eq!(response.status, 422, "{body}");
    }

    // An explicit null is dropped, not written (exclude_unset + exclude_none).
    let response = patch(
        app,
        "/api/follow-ups/policy/current",
        r#"{"task_validity_days": null}"#,
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["task_validity_days"], 7);

    let pool = support::pool_for(&dir).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM follow_up_policies")
        .fetch_one(&pool)
        .await
        .expect("count policies");
    pool.close().await;
    assert_eq!(count, 1, "GET and PATCH reuse the one default row");
}

/// Creating a task derives its status from the decision engine and reuses an
/// open task instead of piling up duplicates.
#[tokio::test]
async fn creating_tasks_reuses_open_rows() {
    let (app, _dir) = w1_app().await;

    // Bob is eligible (reply branch); the first create returns a new row.
    let first = post(app.clone(), "/api/follow-ups", r#"{"candidate_id": "c-2"}"#).await;
    assert_eq!(first.status, 201);
    assert_eq!(first.body["status"], "Eligible");
    assert_eq!(first.body["candidate_name"], "Bob");
    assert_eq!(first.body["objective"], "reply");
    let task_id = first.body["id"].as_str().unwrap().to_string();

    // The second create returns the SAME open task, not a duplicate.
    let second = post(app, "/api/follow-ups", r#"{"candidate_id": "c-2"}"#).await;
    assert_eq!(second.status, 201);
    assert_eq!(second.body["id"], json!(task_id));
}

/// A rejected candidate records a Blocked task; unknown candidates 404; the
/// objective cap is enforced.
#[tokio::test]
async fn creating_tasks_records_blocks_and_validates() {
    let (app, _dir) = w1_app().await;

    let created = post(
        app.clone(),
        "/api/candidates",
        r#"{"name": "Probe Rejected", "status": "Rejected"}"#,
    )
    .await;
    let rejected = created.body["id"].as_str().unwrap().to_string();
    let blocked = post(
        app.clone(),
        "/api/follow-ups",
        &format!(r#"{{"candidate_id": "{rejected}"}}"#),
    )
    .await;
    assert_eq!(blocked.status, 201);
    assert_eq!(blocked.body["status"], "Blocked");
    assert_eq!(
        blocked.body["block_reason"],
        "Candidate status is Rejected; ordinary follow-up is not allowed."
    );

    let response = post(
        app.clone(),
        "/api/follow-ups",
        r#"{"candidate_id": "nope"}"#,
    )
    .await;
    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Candidate 'nope' was not found"
    );

    let long = "x".repeat(65);
    let response = post(
        app,
        "/api/follow-ups",
        &format!(r#"{{"candidate_id": "c-2", "objective": "{long}"}}"#),
    )
    .await;
    assert_eq!(response.status, 422);
}

/// The list sweeps stale rows: an expired `Eligible` becomes `Expired`, while
/// `Copied`/`AwaitingResult`/`Completed` are left alone.
#[tokio::test]
async fn the_list_sweeps_stale_tasks() {
    let (app, dir) = w1_app().await;

    let pool = support::pool_for(&dir).await;
    for (id, status) in [
        ("mat-sweep-eligible", "Eligible"),
        ("mat-sweep-copied", "Copied"),
        ("mat-sweep-awaiting", "AwaitingResult"),
        ("mat-sweep-completed", "Completed"),
    ] {
        sqlx::query(concat!(
            "INSERT INTO follow_up_tasks (id, candidate_id, status, follow_up_count,",
            " expires_at, created_at, updated_at) VALUES (?, 'c-1', ?, 0,",
            " '2026-11-09 09:00:00.000000', '2026-11-10 09:00:00.000000',",
            " '2026-11-10 09:00:00.000000')"
        ))
        .bind(id)
        .bind(status)
        .execute(&pool)
        .await
        .expect("insert sweep row");
    }
    pool.close().await;

    let response = get(app, "/api/follow-ups?candidate_id=c-1").await;
    assert_eq!(response.status, 200);
    let mut statuses: std::collections::BTreeMap<String, String> = response.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["id"].as_str().unwrap().starts_with("mat-sweep"))
        .map(|item| {
            (
                item["id"].as_str().unwrap().to_string(),
                item["status"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    // Equal `created_at` values leave the SQL order undefined among these four
    // rows, so the assertion is per id, not per position.
    assert_eq!(
        statuses.remove("mat-sweep-eligible").as_deref(),
        Some("Expired")
    );
    assert_eq!(
        statuses.remove("mat-sweep-copied").as_deref(),
        Some("Copied")
    );
    assert_eq!(
        statuses.remove("mat-sweep-awaiting").as_deref(),
        Some("AwaitingResult")
    );
    assert_eq!(
        statuses.remove("mat-sweep-completed").as_deref(),
        Some("Completed")
    );
    assert!(statuses.is_empty());
}

/// The PATCH field semantics: absent keeps, null clears, and an unknown task
/// 404s with the baseline wording.
#[tokio::test]
async fn patch_field_semantics_match_the_baseline() {
    let (app, _dir) = w1_app().await;

    // Absent leaves the field alone.
    let response = patch(
        app.clone(),
        "/api/follow-ups/t-2",
        r#"{"reason": "why now"}"#,
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["reason"], "why now");

    let response = patch(
        app.clone(),
        "/api/follow-ups/t-2",
        r#"{"status": "Eligible"}"#,
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["reason"], "why now",
        "absent key keeps the value"
    );

    // Explicit null clears a nullable column.
    let response = patch(app.clone(), "/api/follow-ups/t-2", r#"{"reason": null}"#).await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["reason"], serde_json::Value::Null);

    // Unknown task id.
    let response = patch(app, "/api/follow-ups/nope", r#"{"reason": "x"}"#).await;
    assert_eq!(response.status, 404);
    assert_eq!(
        response.body["error"]["message"],
        "Follow-up task 'nope' was not found"
    );
}

/// Dashboard: counts over the W1 fixture (3 live candidates, 3 live
/// positions, one open; one pending task; one generated draft).
#[tokio::test]
async fn the_dashboard_counts_the_fixture() {
    let (app, _dir) = w1_app().await;

    let response = get(app, "/api/dashboard").await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["candidate_total"], 3);
    assert_eq!(
        response.body["candidate_status_counts"],
        json!({
            "New": 3, "Contacted": 0, "ReadNoReply": 0, "Replied": 0, "Active": 0,
            "Interview": 0, "Rejected": 0, "Withdrawn": 0, "Closed": 0,
        })
    );
    assert_eq!(response.body["active_follow_up_count"], 1, "t-2 is Pending");
    assert_eq!(response.body["awaiting_result_count"], 0);
    assert_eq!(response.body["pending_draft_count"], 1, "d-2 is generated");
    assert_eq!(response.body["position_total"], 3);
    assert_eq!(response.body["open_position_count"], 1);
    let recent = response.body["recent_messages"].as_array().unwrap();
    assert_eq!(recent.len(), 6, "the fixture has six messages");
    let pending = response.body["pending_follow_ups"].as_array().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["id"], "t-2");
    assert_eq!(pending[0]["candidate_name"], "Alice");
}
