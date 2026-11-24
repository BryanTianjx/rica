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
    error.application_error()
}

/// One page of drafts for the approval centre.
pub async fn list_drafts(
    drafts: &dyn DraftRepository,
    filter: DraftFilter,
    offset: usize,
    limit: usize,
) -> Result<(Vec<GeneratedDraft>, usize), ApplicationError> {
    list_drafts_scoped(drafts, filter, offset, limit, None).await
}

/// SEC-04: list with the authenticated tenant/owner predicate in SQL.
pub async fn list_drafts_scoped(
    drafts: &dyn DraftRepository,
    filter: DraftFilter,
    offset: usize,
    limit: usize,
    scope: Option<&crate::Scope>,
) -> Result<(Vec<GeneratedDraft>, usize), ApplicationError> {
    match scope {
        Some(scope) => drafts.list_scoped(filter, offset, limit, scope).await,
        None => drafts.list(filter, offset, limit).await,
    }
    .map_err(unavailable)
}

/// Fetch one draft or report it missing with the baseline's message.
pub async fn get_draft(
    drafts: &dyn DraftRepository,
    id: &str,
) -> Result<GeneratedDraft, ApplicationError> {
    get_draft_scoped(drafts, id, None).await
}

/// SEC-04: out-of-scope drafts are indistinguishable from missing ones.
pub async fn get_draft_scoped(
    drafts: &dyn DraftRepository,
    id: &str,
    scope: Option<&crate::Scope>,
) -> Result<GeneratedDraft, ApplicationError> {
    let found = match scope {
        Some(scope) => drafts.find_scoped(id, scope).await,
        None => drafts.find(id).await,
    };
    found
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
    generate_impl(
        drafts, follow_ups, candidates, positions, generator, request, None,
    )
    .await
}

/// SEC-04: every parent (candidate, position, task) is resolved in the
/// caller's scope, and the generator stamps the new drafts with it.
pub async fn generate_scoped(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    candidates: &dyn crate::ports::CandidateRepository,
    positions: &dyn crate::ports::PositionRepository,
    generator: &dyn DraftGenerator,
    request: NewDraftGeneration,
    scope: Option<&crate::Scope>,
) -> Result<GenerateOutcome, ApplicationError> {
    generate_impl(
        drafts, follow_ups, candidates, positions, generator, request, scope,
    )
    .await
}

async fn generate_impl(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    candidates: &dyn crate::ports::CandidateRepository,
    positions: &dyn crate::ports::PositionRepository,
    generator: &dyn DraftGenerator,
    request: NewDraftGeneration,
    scope: Option<&crate::Scope>,
) -> Result<GenerateOutcome, ApplicationError> {
    let candidate =
        crate::candidates::get_candidate_scoped(candidates, &request.candidate_id, scope).await?;

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
        Some(position_id) => {
            Some(crate::positions::get_position_scoped(positions, position_id, scope).await?)
        }
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
        let found = match scope {
            Some(scope) => follow_ups.find_scoped(task_id, scope).await,
            None => follow_ups.find(task_id).await,
        };
        let task = found
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
    let outcome = generator
        .generate(crate::ai_context::GenerateDraftRequest {
            candidate_id: request.candidate_id.clone(),
            task_id: request.task_id.clone(),
            message_type: request.message_type,
            tone: request.tone,
            user_instruction: request.user_instruction,
            include_alternatives: request.include_alternatives,
            max_length: request.max_length,
            model_id: request.model_id,
            comparison_models: request.comparison_models,
            // SEC-04: the generator stamps the persisted draft(s) with this
            // actor/tenant; Local mode passes None and keeps the defaults.
            scope: scope.cloned(),
        })
        .await?;

    // The draft now exists and awaits review, so the task moves to Reviewing
    // — the single-model path only (the baseline advances the task after the
    // commit; the A/B group path leaves the task untouched until one member
    // is selected).
    let outcome = match outcome {
        crate::ports::DraftGeneration::Single(draft) => {
            if let Some(task_id) = &draft.task_id {
                advance_task_scoped(
                    follow_ups,
                    task_id,
                    ica_domain::FollowUpTaskStatus::Reviewing,
                    "A draft was generated",
                    scope,
                )
                .await?;
            }
            GenerateOutcome::Created(draft)
        }
        crate::ports::DraftGeneration::Group(drafts) => GenerateOutcome::Group(drafts),
    };
    Ok(outcome)
}

/// What `generate` produced: a single draft, or the whole A/B comparison
/// group (one draft per requested model under one `draft_group_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateOutcome {
    Created(Box<GeneratedDraft>),
    Group(Vec<GeneratedDraft>),
}

/// Patch a draft's wording. `approved`/`copied` can no longer be edited (409);
/// a text edit marks `is_edited` and advances `generated → edited`; absent and
/// null fields are both ignored (`exclude_unset + exclude_none`).
pub async fn update_draft(
    drafts: &dyn DraftRepository,
    id: &str,
    patch: DraftPatch,
) -> Result<GeneratedDraft, ApplicationError> {
    update_draft_scoped(drafts, id, patch, None).await
}

/// SEC-04: edit only a draft the caller can see.
pub async fn update_draft_scoped(
    drafts: &dyn DraftRepository,
    id: &str,
    patch: DraftPatch,
    scope: Option<&crate::Scope>,
) -> Result<GeneratedDraft, ApplicationError> {
    let draft = get_draft_scoped(drafts, id, scope).await?;
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
    let updated = match scope {
        Some(scope) => {
            drafts
                .update_scoped(id, patch.message_text, patch.objective, scope)
                .await
        }
        None => drafts.update(id, patch.message_text, patch.objective).await,
    };
    updated
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
    approve_draft_scoped(drafts, follow_ups, id, note, None).await
}

/// SEC-04: approve only a draft the caller can see; its linked task must be
/// in the same scope too.
pub async fn approve_draft_scoped(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    note: Option<String>,
    scope: Option<&crate::Scope>,
) -> Result<GeneratedDraft, ApplicationError> {
    let draft = get_draft_scoped(drafts, id, scope).await?;
    if draft.status == DraftStatus::Rejected {
        return Err(ApplicationError::conflict(
            "A rejected draft cannot be approved",
            Default::default(),
        ));
    }
    // Guard before the write: the baseline rolls the draft change back when
    // the task cannot reach `Approved`.
    if let Some(task_id) = &draft.task_id {
        ensure_transition_reachable_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Approved,
            scope,
        )
        .await?;
    }
    let updated = match scope {
        Some(scope) => drafts.approve_scoped(id, Utc::now(), note, scope).await,
        None => drafts.approve(id, Utc::now(), note).await,
    }
    .map_err(unavailable)?
    .ok_or_else(|| ApplicationError::not_found("Draft", id))?;

    if let Some(task_id) = &updated.task_id {
        advance_task_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Approved,
            "The draft was approved by the user",
            scope,
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
    reject_draft_scoped(drafts, follow_ups, id, None).await
}

/// SEC-04: reject only a draft the caller can see.
pub async fn reject_draft_scoped(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    scope: Option<&crate::Scope>,
) -> Result<GeneratedDraft, ApplicationError> {
    let draft = get_draft_scoped(drafts, id, scope).await?;
    if let Some(task_id) = &draft.task_id {
        ensure_transition_reachable_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Rejected,
            scope,
        )
        .await?;
    }
    let updated = match scope {
        Some(scope) => drafts.reject_scoped(id, Utc::now(), scope).await,
        None => drafts.reject(id, Utc::now()).await,
    }
    .map_err(unavailable)?
    .ok_or_else(|| ApplicationError::not_found("Draft", id))?;

    if let Some(task_id) = &updated.task_id {
        advance_task_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Rejected,
            "The draft was rejected by the user",
            scope,
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
    copy_draft_scoped(drafts, follow_ups, id, None).await
}

/// SEC-04: copy only a draft the caller can see.
pub async fn copy_draft_scoped(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    scope: Option<&crate::Scope>,
) -> Result<crate::payload::DraftCopyOutcome, ApplicationError> {
    let draft = get_draft_scoped(drafts, id, scope).await?;
    if draft.status == DraftStatus::Rejected {
        return Err(ApplicationError::conflict(
            "A rejected draft cannot be copied",
            Default::default(),
        ));
    }
    if let Some(task_id) = &draft.task_id {
        ensure_transition_reachable_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::AwaitingResult,
            scope,
        )
        .await?;
    }
    let copied_at = Utc::now();
    let updated = match scope {
        Some(scope) => drafts.copy_scoped(id, copied_at, scope).await,
        None => drafts.copy(id, copied_at).await,
    }
    .map_err(unavailable)?
    .ok_or_else(|| ApplicationError::not_found("Draft", id))?;

    if let Some(task_id) = &updated.task_id {
        advance_task_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::AwaitingResult,
            "The draft was copied; waiting for the user to record the result",
            scope,
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
    get_draft_group_scoped(drafts, group_id, None).await
}

/// SEC-04: a foreign group is indistinguishable from a missing one.
pub async fn get_draft_group_scoped(
    drafts: &dyn DraftRepository,
    group_id: &str,
    scope: Option<&crate::Scope>,
) -> Result<Vec<GeneratedDraft>, ApplicationError> {
    let rows = match scope {
        Some(scope) => drafts.find_group_scoped(group_id, scope).await,
        None => drafts.find_group(group_id).await,
    }
    .map_err(unavailable)?;
    if rows.is_empty() {
        return Err(ApplicationError::not_found("Draft group", group_id));
    }
    let mut ordered = rows;
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
    select_draft_from_group_scoped(drafts, follow_ups, group_id, selection, None).await
}

/// SEC-04: the whole group is resolved in the caller's scope before it can be
/// selected or superseded.
pub async fn select_draft_from_group_scoped(
    drafts: &dyn DraftRepository,
    follow_ups: &dyn crate::ports::FollowUpRepository,
    group_id: &str,
    selection: NewDraftGroupSelection,
    scope: Option<&crate::Scope>,
) -> Result<Vec<GeneratedDraft>, ApplicationError> {
    let group = get_draft_group_scoped(drafts, group_id, scope).await?;
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
        ensure_transition_reachable_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Reviewing,
            scope,
        )
        .await?;
    }

    // The others are marked rejected WITHOUT touching the shared task: only
    // the selected draft owns the task transition.
    match scope {
        Some(scope) => {
            drafts
                .supersede_others_scoped(group_id, &selection.draft_id, Utc::now(), scope)
                .await
        }
        None => {
            drafts
                .supersede_others(group_id, &selection.draft_id, Utc::now())
                .await
        }
    }
    .map_err(unavailable)?;

    if let Some(task_id) = &selected.task_id {
        advance_task_scoped(
            follow_ups,
            task_id,
            ica_domain::FollowUpTaskStatus::Reviewing,
            "A draft was selected from a comparison group",
            scope,
        )
        .await?;
    }

    // Re-read: the superseded rows changed; the returned group reflects it.
    get_draft_group_scoped(drafts, group_id, scope).await
}

/// The shared `advance_task`: walk the shortest legal path to `target` and
/// write the single `follow_up.task_advanced` audit (via the repository).
pub async fn advance_task(
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    target: ica_domain::FollowUpTaskStatus,
    reason: &str,
) -> Result<(), ApplicationError> {
    advance_task_scoped(follow_ups, id, target, reason, None).await
}

/// SEC-04: the same transition for a task inside the caller's scope.
pub async fn advance_task_scoped(
    follow_ups: &dyn crate::ports::FollowUpRepository,
    id: &str,
    target: ica_domain::FollowUpTaskStatus,
    reason: &str,
    scope: Option<&crate::Scope>,
) -> Result<(), ApplicationError> {
    let advanced = match scope {
        Some(scope) => {
            follow_ups
                .advance_scoped(id, target, Some(reason.to_string()), scope)
                .await
        }
        None => {
            follow_ups
                .advance(id, target, Some(reason.to_string()))
                .await
        }
    };
    advanced.map_err(unavailable)?;
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
/// SEC-04: the pre-write guard with the caller's scope on the task read.
async fn ensure_transition_reachable_scoped(
    follow_ups: &dyn crate::ports::FollowUpRepository,
    task_id: &str,
    target: ica_domain::FollowUpTaskStatus,
    scope: Option<&crate::Scope>,
) -> Result<(), ApplicationError> {
    let found = match scope {
        Some(scope) => follow_ups.find_scoped(task_id, scope).await,
        None => follow_ups.find(task_id).await,
    };
    let Some(task) = found.map_err(unavailable)? else {
        // The row is gone (a dangling `task_id` on the draft). The baseline's
        // `get_task` raises `NotFoundError` here, so the request is a 404 and
        // nothing is written. `advance` alone would not catch it: it treats a
        // missing row as "nothing to update" and reports success, which made
        // the port answer 200 for a draft that points at nothing.
        return Err(ApplicationError::not_found("Follow-up task", task_id));
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
    preview_context_scoped(candidates, positions, messages, candidate_id, None).await
}

/// SEC-04: the AI-context preview is scoped exactly like the candidate read.
pub async fn preview_context_scoped(
    candidates: &dyn crate::ports::CandidateRepository,
    positions: &dyn crate::ports::PositionRepository,
    messages: &dyn crate::ports::MessageRepository,
    candidate_id: &str,
    scope: Option<&crate::Scope>,
) -> Result<AiContext, ApplicationError> {
    crate::ai_context::build_context_scoped(candidates, positions, messages, candidate_id, scope)
        .await
}
