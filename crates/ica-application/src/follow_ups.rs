//! Follow-up use cases: the decision engine's application wiring and the
//! task state machine's API-facing operations.
//!
//! Everything that decides *behaviour* lives in `ica_domain::follow_up`; this
//! module gathers the inputs the engine needs, chooses the codes/messages the
//! baseline emits, and keeps the audit trail identical — including the
//! `record-result` → send-log linkage the baseline performs in one transaction.

use std::collections::BTreeMap;

use chrono::Utc;
use ica_domain::{
    can_transition, evaluate_follow_up, task_expiry, DecisionOutcome, EvaluateInput,
    FollowUpDecision, FollowUpPolicy, FollowUpTask, FollowUpTaskStatus, Position, ResultType,
};

use crate::error::ApplicationError;
use crate::payload::{
    Field, FollowUpPolicyPatch, FollowUpResultPatch, FollowUpTaskFilter, FollowUpTaskPatch,
    NewFollowUpEvaluation, NewFollowUpResult, NewFollowUpTask, NewSendRecord,
};
use crate::ports::{
    CandidateRepository, FollowUpPolicyRecord, FollowUpRepository, NewFollowUpTaskRow,
    PositionRepository, RepositoryError,
};

/// `settings.data_retention_days` in the baseline: candidates `archived` past
/// this window are hard-deleted by the dashboard's maintenance tick.
pub const DATA_RETENTION_DAYS: i64 = 180;
/// `RAW_TEXT_TTL_DAYS`: orphaned import preview text is blanked after this.
pub const RAW_TEXT_TTL_DAYS: i64 = 7;
/// Content-summary cap for the send log (ICA-G5-06): the record keeps a short
/// label, never the message body. Counted in characters, like Python.
pub const SEND_RECORD_SUMMARY_MAX: usize = 200;

const DEFAULT_POLICY_NAME: &str = "default";

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// The active policy, falling back to the built-in defaults.
pub async fn active_policy(
    follow_ups: &dyn FollowUpRepository,
) -> Result<FollowUpPolicy, ApplicationError> {
    match follow_ups.active_policy().await.map_err(unavailable)? {
        Some(record) => Ok(record.policy),
        None => Ok(FollowUpPolicy::default()),
    }
}

/// The default policy row, creating it on first use (`ensure_default_policy`).
pub async fn ensure_default_policy(
    follow_ups: &dyn FollowUpRepository,
) -> Result<FollowUpPolicyRecord, ApplicationError> {
    match follow_ups
        .policy_by_name(DEFAULT_POLICY_NAME)
        .await
        .map_err(unavailable)?
    {
        Some(record) => Ok(record),
        None => follow_ups
            .create_default_policy()
            .await
            .map_err(unavailable),
    }
}

/// Patch the active policy thresholds and audit `follow_up.policy_updated`.
pub async fn update_policy(
    follow_ups: &dyn FollowUpRepository,
    patch: FollowUpPolicyPatch,
) -> Result<FollowUpPolicyRecord, ApplicationError> {
    let record = ensure_default_policy(follow_ups).await?;
    follow_ups
        .update_policy(&record.id, &patch)
        .await
        .map_err(unavailable)
}

/// Decide whether a follow-up is appropriate for a candidate right now.
///
/// `position_id` in the request overrides the candidate's current link so the
/// user can ask "what if this candidate were attached to that role?".
pub async fn evaluate(
    follow_ups: &dyn FollowUpRepository,
    candidates: &dyn CandidateRepository,
    positions: &dyn PositionRepository,
    evaluation: NewFollowUpEvaluation,
) -> Result<FollowUpDecision, ApplicationError> {
    let candidate = candidates
        .find(&evaluation.candidate_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Candidate", &evaluation.candidate_id))?;
    let position = match evaluation
        .position_id
        .as_deref()
        .or(candidate.position_id.as_deref())
    {
        Some(position_id) => positions.find(position_id).await.map_err(unavailable)?,
        None => None,
    };
    evaluate_for_candidate(follow_ups, positions, &candidate, position).await
}

/// Run the decision engine for one candidate, using current data — shared by
/// the evaluate endpoint and by task creation, so both agree on what "a
/// follow-up is appropriate" means.
async fn evaluate_for_candidate(
    follow_ups: &dyn FollowUpRepository,
    positions: &dyn PositionRepository,
    candidate: &ica_domain::Candidate,
    position_override: Option<Position>,
) -> Result<FollowUpDecision, ApplicationError> {
    let position = match position_override {
        Some(position) => Some(position),
        None => match &candidate.position_id {
            Some(position_id) => positions.find(position_id).await.map_err(unavailable)?,
            None => None,
        },
    };
    let last_message = follow_ups
        .latest_message_for_candidate(&candidate.id)
        .await
        .map_err(unavailable)?;
    let active_count = follow_ups
        .contact_attempt_count(&candidate.id)
        .await
        .map_err(unavailable)?;
    // The interval reference is `last_follow_up_at`, falling back to the
    // contact task's `updated_at` when it was never stamped (measured on the
    // baseline: the seeded `Completed` task walks the ≥48h branch this way).
    let last_follow_up_at = follow_ups
        .latest_contact_task(&candidate.id)
        .await
        .map_err(unavailable)?
        .and_then(|task| task.last_follow_up_at.or(Some(task.updated_at)));
    let policy = active_policy(follow_ups).await?;

    let mut decision = evaluate_follow_up(EvaluateInput {
        candidate_status: candidate.status,
        position_status: position.as_ref().map(|position| position.status),
        last_message_at: last_message
            .as_ref()
            .and_then(|message| message.message_time),
        last_message_from_candidate: last_message
            .as_ref()
            .map(|message| message.sender_type.as_deref() == Some("Candidate")),
        active_follow_up_count: active_count,
        last_follow_up_at,
        now: Utc::now(),
        policy: Some(policy),
    });

    if decision.eligible() && position.is_none() {
        decision.risks.push(
            "The candidate is not linked to a position, so the message cannot reference \
             position requirements."
                .to_string(),
        );
    }
    Ok(decision)
}

/// One page of tasks. Stale tasks are swept first so the list never shows a
/// task whose validity window has passed — the expiry rule runs on the
/// workbench read, exactly like the baseline.
pub async fn list_tasks(
    follow_ups: &dyn FollowUpRepository,
    filter: FollowUpTaskFilter,
    offset: usize,
    limit: usize,
) -> Result<(Vec<FollowUpTask>, usize), ApplicationError> {
    follow_ups
        .expire_stale(Utc::now())
        .await
        .map_err(unavailable)?;
    follow_ups
        .list(filter, offset, limit)
        .await
        .map_err(unavailable)
}

/// Fetch one task or report it missing with the baseline's message.
pub async fn get_task(
    follow_ups: &dyn FollowUpRepository,
    id: &str,
) -> Result<FollowUpTask, ApplicationError> {
    follow_ups
        .find(id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Follow-up task", id))
}

/// Create a follow-up task in the state the decision engine reports.
///
/// A blocked task is recorded rather than dropped — the blocking condition
/// stays visible on the task. Creation is idempotent per open task: an
/// existing non-terminal, non-blocked task is returned instead of a duplicate.
pub async fn create_task(
    follow_ups: &dyn FollowUpRepository,
    candidates: &dyn CandidateRepository,
    positions: &dyn PositionRepository,
    new: NewFollowUpTask,
) -> Result<FollowUpTask, ApplicationError> {
    let candidate = candidates
        .find(&new.candidate_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Candidate", &new.candidate_id))?;
    let policy = active_policy(follow_ups).await?;
    let position = match new
        .position_id
        .as_deref()
        .or(candidate.position_id.as_deref())
    {
        Some(position_id) => positions.find(position_id).await.map_err(unavailable)?,
        None => None,
    };
    let decision = evaluate_for_candidate(follow_ups, positions, &candidate, position).await?;

    let (status, block_reason) = match decision.outcome {
        DecisionOutcome::Blocked => (
            FollowUpTaskStatus::Blocked,
            Some(decision.block_reasons.join(" ")),
        ),
        DecisionOutcome::Eligible => (FollowUpTaskStatus::Eligible, None),
        DecisionOutcome::Wait => (FollowUpTaskStatus::Pending, None),
    };

    // Reuse an open task rather than piling up duplicates: a double click must
    // not leave duplicate work in the workbench.
    if let Some(existing) = follow_ups
        .find_open_for_candidate(&candidate.id)
        .await
        .map_err(unavailable)?
    {
        let status = existing.status;
        return follow_ups
            .mark_reused(&existing.id, status)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| ApplicationError::not_found("Follow-up task", &existing.id));
    }

    let now = Utc::now();
    follow_ups
        .create(NewFollowUpTaskRow {
            candidate_id: candidate.id.clone(),
            position_id: new
                .position_id
                .clone()
                .or_else(|| candidate.position_id.clone()),
            status,
            outcome: decision.outcome.as_str().to_string(),
            objective: new
                .objective
                .clone()
                .or_else(|| decision.suggested_objective.clone()),
            reason: new
                .reason
                .clone()
                .or_else(|| reason_from_decision(&decision)),
            block_reason,
            recommended_at: new.recommended_at.or(decision.recommended_at),
            expires_at: task_expiry(now, Some(policy)),
        })
        .await
        .map_err(unavailable)
}

/// One-line explanation of why a follow-up is (not) due.
fn reason_from_decision(decision: &FollowUpDecision) -> Option<String> {
    if !decision.reasons.is_empty() {
        return Some(decision.reasons.join(" "));
    }
    if !decision.block_reasons.is_empty() {
        return Some(decision.block_reasons.join(" "));
    }
    None
}

/// Patch a task, validating any status change against the state machine.
///
/// A status may only be set by hand if it is manual-settable. The semantic
/// states — `Approved`, `Copied` and `Completed` — are owned by the
/// draft-approval, copy and result-recording flows; letting a raw patch write
/// them would break the "only a recorded action implies the action" guarantee.
pub async fn update_task(
    follow_ups: &dyn FollowUpRepository,
    id: &str,
    patch: FollowUpTaskPatch,
) -> Result<FollowUpTask, ApplicationError> {
    let task = get_task(follow_ups, id).await?;

    if let Field::Value(target) = patch.status {
        let current = task.status;
        if !target.is_manual_settable() {
            return Err(ApplicationError::conflict_value(
                format!(
                    "'{}' cannot be set directly; it is recorded by the action it represents",
                    target.as_str()
                ),
                serde_json::json!({
                    "current": current.as_str(),
                    "requested": target.as_str(),
                    "settable": MANUAL_SETTABLE_SORTED,
                }),
            ));
        }
        if !can_transition(current, target) {
            let mut details = BTreeMap::new();
            details.insert("current".to_string(), current.as_str().to_string());
            details.insert("requested".to_string(), target.as_str().to_string());
            return Err(ApplicationError::conflict(
                &format!(
                    "Illegal follow-up transition {} → {}",
                    current.as_str(),
                    target.as_str()
                ),
                details,
            ));
        }
    }

    follow_ups
        .update(id, patch)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Follow-up task", id))
}

/// The manual-settable statuses in the order the baseline renders them
/// (`sorted(s.value for s in MANUAL_SETTABLE_STATUSES)`).
const MANUAL_SETTABLE_SORTED: [&str; 6] = [
    "Cancelled",
    "Drafting",
    "Eligible",
    "Pending",
    "Rejected",
    "Reviewing",
];

/// Cancel a task. An already-`Cancelled` task is a legal self-transition and
/// answers 200 unchanged (measured on the baseline).
pub async fn cancel_task(
    follow_ups: &dyn FollowUpRepository,
    id: &str,
    reason: Option<String>,
) -> Result<FollowUpTask, ApplicationError> {
    let task = get_task(follow_ups, id).await?;
    let current = task.status;
    if !can_transition(current, FollowUpTaskStatus::Cancelled) {
        let mut details = BTreeMap::new();
        details.insert("current".to_string(), current.as_str().to_string());
        return Err(ApplicationError::conflict(
            &format!("A task in state {} cannot be cancelled", current.as_str()),
            details,
        ));
    }
    // Python: `reason or task.reason` — an empty string keeps the old reason.
    let reason = reason.filter(|value| !value.is_empty());
    follow_ups
        .cancel(id, reason)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Follow-up task", id))
}

/// Mark a task as completed (the user handled it).
pub async fn complete_task(
    follow_ups: &dyn FollowUpRepository,
    id: &str,
) -> Result<FollowUpTask, ApplicationError> {
    let task = get_task(follow_ups, id).await?;
    let current = task.status;
    if !can_transition(current, FollowUpTaskStatus::Completed) {
        let mut details = BTreeMap::new();
        details.insert("current".to_string(), current.as_str().to_string());
        return Err(ApplicationError::conflict(
            &format!("A task in state {} cannot be completed", current.as_str()),
            details,
        ));
    }
    follow_ups
        .complete(id, Utc::now())
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Follow-up task", id))
}

/// Record the manually observed communication outcome.
///
/// This is the only operation that can mark a message as actually sent.
/// Recording is idempotent: the same outcome twice counts one contact attempt,
/// a different outcome is a correction and replaces the earlier one.
pub async fn record_result(
    follow_ups: &dyn FollowUpRepository,
    id: &str,
    result: NewFollowUpResult,
) -> Result<FollowUpTask, ApplicationError> {
    let task = get_task(follow_ups, id).await?;
    let now = Utc::now();

    let already_counted = task.result_recorded_at.is_some() && task.result_type.is_some();
    let previous_indicates_contact = already_counted
        && task
            .result_type
            .as_deref()
            .and_then(ResultType::parse)
            .is_some_and(|value| value.indicates_contact());
    let previous_result = task.result_type.clone();

    let bump_count =
        result.result_type.indicates_contact() && !(already_counted && previous_indicates_contact);

    // The send log needs the *previous* outcome before it is overwritten; a
    // row is appended exactly when the outcome is new or a correction.
    let send_record = if previous_result.as_deref() != Some(result.result_type.as_str()) {
        let correction = previous_result.is_some();
        let note = match (&result.note, correction) {
            (Some(note), true) => Some(format!("correction: {note}")),
            (None, true) => Some("correction:".to_string()),
            (note, false) => note.clone(),
        };
        let content_summary = task
            .objective
            .as_deref()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .map(|summary| {
                if summary.chars().count() > SEND_RECORD_SUMMARY_MAX {
                    summary.chars().take(SEND_RECORD_SUMMARY_MAX).collect()
                } else {
                    summary.to_string()
                }
            });
        let draft_id = follow_ups
            .sole_sendable_draft_id(&task.id)
            .await
            .map_err(unavailable)?;
        Some(NewSendRecord {
            candidate_id: task.candidate_id.clone(),
            task_id: Some(task.id.clone()),
            draft_id,
            channel: "boss".to_string(),
            target: None,
            content_summary,
            status: result.result_type,
            sent_at: None,
            note,
        })
    } else {
        None
    };

    let status = if task.status.is_result_completable() {
        Some(FollowUpTaskStatus::Completed)
    } else {
        None
    };
    let completed_at = status.map(|_| now);
    let last_follow_up_at = if result.result_type.indicates_contact() {
        Some(now)
    } else {
        None
    };

    follow_ups
        .record_result(
            &task.id,
            FollowUpResultPatch {
                result_type: result.result_type.as_str().to_string(),
                result_note: result.note,
                result_recorded_at: now,
                status,
                completed_at,
                bump_count,
                last_follow_up_at,
            },
            send_record,
        )
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Follow-up task", id))
}
