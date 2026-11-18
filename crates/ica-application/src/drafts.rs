//! Draft use cases: the approval-centre flows and the generate guard chain.
//!
//! The three approval actions are deliberately separate, and their semantics
//! are enforced server-side (baseline docstring preserved): **approve** means
//! the human accepted the wording, **copy** means the text reached the
//! clipboard — neither sends anything; only the follow-up result recording can
//! mark a message as sent.

use chrono::Utc;

use ica_domain::{DraftStatus, GeneratedDraft};

use crate::ai_context::AiContext;
use crate::error::ApplicationError;
use crate::payload::{DraftFilter, DraftPatch, NewDraftGeneration, NewDraftGroupSelection};
use crate::ports::{DraftGenerator, DraftRepository, RepositoryError};

fn unavailable(error: RepositoryError) -> ApplicationError {
    // A business-rule failure is a 409 `business_rule_violation`, not a 500:
    // the follow-up state machine refusing an unreachable transition is the
    // baseline's `BusinessRuleError`, and its message plus details are
    // contract text.
    if let Some(details) = error.conflict_details() {
        return ApplicationError::Conflict {
            message: error.message().to_string(),
            details: details.clone(),
        };
    }
    ApplicationError::Unavailable {
        message: error.message().to_string(),
    }
}

/// One page of drafts for the approval centre.
pub async fn list_drafts(
    drafts: &dyn DraftRepository,
    filter: DraftFilter,
    offset: usize,
    limit: usize,
) -> Result<(Vec<GeneratedDraft>, usize), ApplicationError> {
    drafts
        .list(filter, offset, limit)
        .await
        .map_err(unavailable)
}

/// Fetch one draft or report it missing with the baseline's message.
pub async fn get_draft(
    drafts: &dyn DraftRepository,
    id: &str,
) -> Result<GeneratedDraft, ApplicationError> {
    drafts
        .find(id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Draft", id))
}

/// Generate a draft (or a comparison group) for a candidate.
///
/// W3 wires the **guard chain** and the AI port; the no-key implementation of
/// the port answers with the baseline's 503, so no draft is ever persisted on
/// this path (the provider call and persistence belong to W5). The guards run
/// in the baseline's order, each with its verbatim message:
///
/// 1. unknown candidate (404);
/// 2. egress consent disabled (409);
/// 3. a blocking candidate status (409);
/// 4. the linked position closed (409);
/// 5. the follow-up task cannot be drafted / belongs to another candidate (409).
pub async fn generate(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    candidates: &dyn crate::ports::CandidateRepository,
    positions: &dyn crate::ports::PositionRepository,
    generator: &dyn DraftGenerator,
    request: NewDraftGeneration,
) -> Result<GenerateOutcome, ApplicationError> {
    let candidate = candidates
        .find(&request.candidate_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Candidate", &request.candidate_id))?;

    // Egress gate (PRIV-07): the operator can stop every outbound AI call from
    // the settings page; the default (no recorded switch) is allowed.
    if !drafts.egress_consent().await.map_err(unavailable)? {
        let mut details = std::collections::BTreeMap::new();
        details.insert("consent_granted".to_string(), "False".to_string());
        return Err(ApplicationError::conflict(
            "AI egress is disabled in settings; no candidate context may be sent to a \
             provider until it is re-enabled.",
            details,
        ));
    }

    if ica_domain::follow_up::candidate_blocks_follow_up(candidate.status) {
        let mut details = std::collections::BTreeMap::new();
        details.insert(
            "candidate_status".to_string(),
            candidate.status.as_str().to_string(),
        );
        return Err(ApplicationError::conflict(
            &format!(
                "Candidate status '{}' does not allow generating a follow-up message",
                candidate.status.as_str()
            ),
            details,
        ));
    }

    let position = match &candidate.position_id {
        Some(position_id) => positions.find(position_id).await.map_err(unavailable)?,
        None => None,
    };
    if let Some(position) = &position {
        if position.status == ica_domain::PositionStatus::Closed {
            let mut details = std::collections::BTreeMap::new();
            details.insert(
                "position_status".to_string(),
                position.status.as_str().to_string(),
            );
            return Err(ApplicationError::conflict(
                "The linked position is closed; no message can be generated for it",
                details,
            ));
        }
    }

    // The draft belongs to a follow-up task; validate before anything else so
    // a blocked or closed task cannot be drafted.
    if let Some(task_id) = &request.task_id {
        let task = follow_ups
            .find(task_id)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| ApplicationError::not_found("Follow-up task", task_id))?;
        if task.status == ica_domain::FollowUpTaskStatus::Blocked {
            let mut details = std::collections::BTreeMap::new();
            if let Some(block_reason) = &task.block_reason {
                details.insert("block_reason".to_string(), block_reason.clone());
            }
            return Err(ApplicationError::conflict(
                "This task is blocked and cannot be drafted",
                details,
            ));
        }
        if task.status.is_closed() {
            let mut details = std::collections::BTreeMap::new();
            details.insert("current".to_string(), task.status.as_str().to_string());
            return Err(ApplicationError::conflict(
                &format!("A task in state {} cannot be drafted", task.status.as_str()),
                details,
            ));
        }
        if task.candidate_id != candidate.id {
            let mut details = std::collections::BTreeMap::new();
            details.insert("task_id".to_string(), task.id.clone());
            details.insert("task_candidate_id".to_string(), task.candidate_id.clone());
            details.insert("candidate_id".to_string(), candidate.id.clone());
            return Err(ApplicationError::conflict(
                "The follow-up task belongs to a different candidate",
                details,
            ));
        }
    }

    // The AI port: with no configured key the adapter answers the baseline's
    // 503 (`ai_provider_error`), which propagates unchanged.
    generator
        .generate(crate::ai_context::GenerateDraftRequest {
            candidate_id: request.candidate_id,
            task_id: request.task_id,
            message_type: request.message_type,
            tone: request.tone,
            user_instruction: request.user_instruction,
            include_alternatives: request.include_alternatives,
            max_length: request.max_length,
        })
        .await
        .map(|draft| GenerateOutcome::Created(Box::new(draft)))
}

/// What `generate` produced: W3 only ever observes the not-configured error,
/// but the shape is the W5 success path's contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateOutcome {
    Created(Box<GeneratedDraft>),
}

/// Patch a draft's wording. `approved`/`copied` can no longer be edited (409);
/// a text edit marks `is_edited` and advances `generated → edited`; absent and
/// null fields are both ignored (`exclude_unset + exclude_none`).
pub async fn update_draft(
    drafts: &dyn DraftRepository,
    id: &str,
    patch: DraftPatch,
) -> Result<GeneratedDraft, ApplicationError> {
    let draft = get_draft(drafts, id).await?;
    if matches!(draft.status, DraftStatus::Approved | DraftStatus::Copied) {
        let mut details = std::collections::BTreeMap::new();
        details.insert("status".to_string(), draft.status.as_str().to_string());
        return Err(ApplicationError::conflict(
            &format!(
                "A draft in state '{}' can no longer be edited",
                draft.status.as_str()
            ),
            details,
        ));
    }
    drafts
        .update(id, patch.message_text, patch.objective)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Draft", id))
}

/// Approve a draft: the human accepted the wording. Records no send and
/// creates no message; only a rejected draft is refused (everything else
/// answers 200, including an already-approved one).
pub async fn approve_draft(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    note: Option<String>,
) -> Result<GeneratedDraft, ApplicationError> {
    let draft = get_draft(drafts, id).await?;
    if draft.status == DraftStatus::Rejected {
        return Err(ApplicationError::conflict(
            "A rejected draft cannot be approved",
            Default::default(),
        ));
    }
    // Guard before the write: the baseline rolls the draft change back when
    // the task cannot reach `Approved`.
    if let Some(task_id) = &draft.task_id {
        ensure_transition_reachable(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Approved,
        )
        .await?;
    }
    let updated = drafts
        .approve(id, Utc::now(), note)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Draft", id))?;

    if let Some(task_id) = &updated.task_id {
        advance_task(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Approved,
            "The draft was approved by the user",
        )
        .await?;
    }
    Ok(updated)
}

/// Reject a draft — no guard: every state answers 200 with `rejected`.
pub async fn reject_draft(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
) -> Result<GeneratedDraft, ApplicationError> {
    let draft = get_draft(drafts, id).await?;
    if let Some(task_id) = &draft.task_id {
        ensure_transition_reachable(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Rejected,
        )
        .await?;
    }
    let updated = drafts
        .reject(id, Utc::now())
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Draft", id))?;

    if let Some(task_id) = &updated.task_id {
        advance_task(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Rejected,
            "The draft was rejected by the user",
        )
        .await?;
    }
    Ok(updated)
}

/// Mark a draft as copied — the text reached the clipboard, which is **not**
/// evidence that the message was sent.
pub async fn copy_draft(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
) -> Result<crate::payload::DraftCopyOutcome, ApplicationError> {
    let draft = get_draft(drafts, id).await?;
    if draft.status == DraftStatus::Rejected {
        return Err(ApplicationError::conflict(
            "A rejected draft cannot be copied",
            Default::default(),
        ));
    }
    if let Some(task_id) = &draft.task_id {
        ensure_transition_reachable(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::AwaitingResult,
        )
        .await?;
    }
    let copied_at = Utc::now();
    let updated = drafts
        .copy(id, copied_at)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Draft", id))?;

    if let Some(task_id) = &updated.task_id {
        advance_task(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::AwaitingResult,
            "The draft was copied; waiting for the user to record the result",
        )
        .await?;
    }
    Ok(crate::payload::DraftCopyOutcome {
        draft_id: updated.id,
        message_text: updated.message_text,
        copied_at,
    })
}

/// Fetch one comparison group, `created_at ASC`.
pub async fn get_draft_group(
    drafts: &dyn DraftRepository,
    group_id: &str,
) -> Result<Vec<GeneratedDraft>, ApplicationError> {
    let drafts = drafts.find_group(group_id).await.map_err(unavailable)?;
    if drafts.is_empty() {
        return Err(ApplicationError::not_found("Draft group", group_id));
    }
    let mut ordered = drafts;
    ordered.sort_by_key(|draft| draft.created_at);
    Ok(ordered)
}

/// Select one draft from a comparison group; the rest are automatically
/// rejected. The selected draft keeps its state and owns the task transition
/// (to `Reviewing`); the others are marked rejected with the supersession
/// audit, one per row.
pub async fn select_draft_from_group(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    group_id: &str,
    selection: NewDraftGroupSelection,
) -> Result<Vec<GeneratedDraft>, ApplicationError> {
    let group = get_draft_group(drafts, group_id).await?;
    if !group.iter().any(|draft| draft.id == selection.draft_id) {
        // The baseline raises `BusinessRuleError` with no `details` at all, so
        // the envelope must carry `{}` — adding `draft_id` here is a shape
        // difference, not a helpful extra.
        return Err(ApplicationError::conflict(
            "The selected draft does not belong to this draft group",
            Default::default(),
        ));
    }
    let selected = group
        .iter()
        .find(|draft| draft.id == selection.draft_id)
        .expect("checked membership above");
    if matches!(
        selected.status,
        DraftStatus::Rejected | DraftStatus::Approved | DraftStatus::Copied
    ) {
        return Err(ApplicationError::conflict(
            "This draft can no longer be selected",
            Default::default(),
        ));
    }

    // Guard before the write, like approve / reject / copy: the baseline
    // rejects the siblings and advances the task in one transaction, so a
    // refused transition must leave the whole group untouched. Without this,
    // `supersede_others` would commit and the siblings would stay `rejected`
    // even though the request failed.
    if let Some(task_id) = &selected.task_id {
        ensure_transition_reachable(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Reviewing,
        )
        .await?;
    }

    // The others are marked rejected WITHOUT touching the shared task: only
    // the selected draft owns the task transition.
    drafts
        .supersede_others(group_id, &selection.draft_id, Utc::now())
        .await
        .map_err(unavailable)?;

    if let Some(task_id) = &selected.task_id {
        advance_task(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Reviewing,
            "A draft was selected from a comparison group",
        )
        .await?;
    }

    // Re-read: the superseded rows changed; the returned group reflects it.
    get_draft_group(drafts, group_id).await
}

/// The shared `advance_task`: walk the shortest legal path to `target` and
/// write the single `follow_up.task_advanced` audit (via the repository).
pub async fn advance_task(
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    target: ica_domain::FollowUpTaskStatus,
    reason: &str,
) -> Result<(), ApplicationError> {
    follow_ups
        .advance(id, target, Some(reason.to_string()))
        .await
        .map_err(unavailable)?;
    Ok(())
}

/// Refuse an unreachable transition *before* anything is written.
///
/// The baseline mutates the draft and advances its task inside **one**
/// transaction: when the state machine refuses the target, the whole thing
/// rolls back and the draft keeps its previous status. The port's two writes
/// are separate transactions, so an advance that fails *after* the draft
/// write would leave the row mutated — the API would still answer 409, but
/// the stored draft would claim `approved` for a request that failed.
///
/// Checking reachability up front makes the failure happen before the first
/// write, which reproduces the baseline's roll-back outcome. The check and the
/// write are not atomic with respect to a concurrent task update, but the
/// desktop app drives one candidate's workflow at a time; the baseline's own
/// guarantee only has to hold against *its* writes, and `advance` still
/// re-validates the path inside its transaction.
async fn ensure_transition_reachable(
    follow_ups: &dyn crate::ports::FollowUpRepository,
    task_id: &str,
    target: ica_domain::FollowUpTaskStatus,
) -> Result<(), ApplicationError> {
    let Some(task) = follow_ups.find(task_id).await.map_err(unavailable)? else {
        // A missing task is `advance`'s own error to report.
        return Ok(());
    };
    if ica_domain::find_transition_path(task.status, target).is_some() {
        return Ok(());
    }
    let mut details = std::collections::BTreeMap::new();
    details.insert("current".to_string(), task.status.as_str().to_string());
    details.insert("requested".to_string(), target.as_str().to_string());
    Err(ApplicationError::conflict(
        &format!(
            "A task in state {} cannot reach {}",
            task.status.as_str(),
            target.as_str()
        ),
        details,
    ))
}

/// The AI context for one candidate (the `/context` preview surface).
pub async fn preview_context(
    candidates: &dyn crate::ports::CandidateRepository,
    positions: &dyn crate::ports::PositionRepository,
    messages: &dyn crate::ports::MessageRepository,
    candidate_id: &str,
) -> Result<AiContext, ApplicationError> {
    crate::ai_context::build_context(candidates, positions, messages, candidate_id).await
}
