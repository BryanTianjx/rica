//! The IPC command layer: one command per endpoint, the same validation and
//! use cases `ica-api` routes through — no contract logic duplicated here.
//!
//! Two argument shapes, mirroring what each transport naturally receives:
//!
//! * **Body-carrying commands** take the request JSON as a `serde_json::Value`
//!   and run it through `ica_contracts::body::parse` + the same validators the
//!   HTTP side uses — the omitted/null distinction and every length/rule come
//!   along for free.
//! * **List commands** take typed filter args; the enum/`search` rules reuse
//!   the contract's query validators via the `Raw*Query` structs, and the
//!   numeric `offset`/`limit` checks go through the typed variants.
//!
//! Wrapping one of these functions with `#[tauri::command]` is mechanical and
//! belongs to the shell host; this crate stays tauri-free so it builds (and
//! tests) on any machine.

use serde::Deserialize;
use serde_json::Value;

use ica_contracts::{
    body, dto, query,
    query::{
        RawListCandidatesQuery, RawListConversationsQuery, RawListFollowUpsQuery,
        RawListMessagesQuery, RawListSendRecordsQuery, DEFAULT_LIMIT, DEFAULT_MESSAGE_LIMIT,
        MAX_LIMIT, MAX_MESSAGE_LIMIT,
    },
};

use crate::error::IpcError;
use crate::state::DesktopState;

fn parse_body<T>(payload: &Value) -> Result<T, IpcError>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = serde_json::to_vec(payload).map_err(|error| {
        IpcError::from(ContractError::internal(format!(
            "cannot serialise command args: {error}"
        )))
    })?;
    body::parse(&bytes).map_err(IpcError::from)
}

use ica_contracts::error::ContractError;

fn fail(error: ContractError) -> IpcError {
    IpcError::from(error)
}

// ── Health ─────────────────────────────────────────────────────────────────

/// `health` — the liveness probe the shell polls.
pub async fn health(state: &DesktopState) -> Result<dto::HealthResponse, IpcError> {
    Ok(dto::HealthResponse {
        status: "ok",
        app: state.config().app_name.clone(),
        ai_enabled: state.config().ai_enabled,
    })
}

// ── Positions ──────────────────────────────────────────────────────────────

/// Typed args for `positions_list`.
#[derive(Debug, Default, Deserialize)]
pub struct PositionListArgs {
    pub include_archived: Option<bool>,
    pub search: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `positions_list` — one page of positions, newest first.
pub async fn positions_list(
    state: &DesktopState,
    args: PositionListArgs,
) -> Result<dto::PageResponse<dto::PositionRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let query = query::validate(query::RawListPositionsQuery {
        include_archived: args.include_archived.map(|value| value.to_string()),
        search: args.search,
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let page = ica_application::list_positions(
        state.positions(),
        ica_application::ListPositions {
            include_archived: query.include_archived,
            search: query.search,
            offset,
            limit,
        },
    )
    .await?;
    Ok(dto::PageResponse::from_page(page, dto::PositionRead::from))
}

/// `positions_get` — one position, archived or not.
pub async fn positions_get(state: &DesktopState, id: &str) -> Result<dto::PositionRead, IpcError> {
    let position = ica_application::get_position(state.positions(), id).await?;
    Ok(dto::PositionRead::from(position))
}

/// `positions_create` — the created row.
pub async fn positions_create(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::PositionRead, IpcError> {
    let raw: body::RawPositionCreate = parse_body(payload)?;
    let new = body::position_create(raw)?;
    let position = ica_application::create_position(state.positions(), new).await?;
    Ok(dto::PositionRead::from(position))
}

/// `positions_patch` — apply the present fields only.
pub async fn positions_patch(
    state: &DesktopState,
    id: &str,
    payload: &Value,
) -> Result<dto::PositionRead, IpcError> {
    let raw: body::RawPositionPatch = parse_body(payload)?;
    let patch = body::position_patch(raw)?;
    let position =
        ica_application::positions::update_position(state.positions(), id, patch).await?;
    Ok(dto::PositionRead::from(position))
}

/// `positions_delete` — candidates keep their row.
pub async fn positions_delete(state: &DesktopState, id: &str) -> Result<(), IpcError> {
    ica_application::delete_position(state.positions(), id)
        .await
        .map_err(IpcError::from)
}

// ── Candidates ─────────────────────────────────────────────────────────────

/// Typed args for `candidates_list`.
#[derive(Debug, Default, Deserialize)]
pub struct CandidateListArgs {
    pub search: Option<String>,
    pub status: Option<String>,
    pub position_id: Option<String>,
    pub include_archived: Option<bool>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `candidates_list` — one page with the real total.
pub async fn candidates_list(
    state: &DesktopState,
    args: CandidateListArgs,
) -> Result<dto::PageResponse<dto::CandidateRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let query = query::validate_candidates(RawListCandidatesQuery {
        search: args.search,
        status: args.status,
        position_id: args.position_id,
        include_archived: args.include_archived.map(|value| value.to_string()),
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let page = ica_application::list_candidates(
        state.candidates(),
        ica_application::ListCandidates {
            search: query.search,
            status: query.status,
            position_id: query.position_id,
            include_archived: query.include_archived,
            offset,
            limit,
        },
    )
    .await?;
    Ok(dto::PageResponse::from_page(page, dto::CandidateRead::from))
}

/// `candidates_get` — the candidate plus its position.
pub async fn candidates_get(
    state: &DesktopState,
    id: &str,
) -> Result<dto::CandidateDetailRead, IpcError> {
    let detail = ica_application::get_candidate_detail(state.candidates(), id).await?;
    Ok(dto::CandidateDetailRead {
        candidate: dto::CandidateRead::from(detail.candidate),
        position: detail.position.map(dto::PositionRead::from),
    })
}

/// `candidates_create`.
pub async fn candidates_create(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::CandidateRead, IpcError> {
    let raw: body::RawCandidateCreate = parse_body(payload)?;
    let new = body::candidate_create(raw)?;
    let candidate =
        ica_application::create_candidate(state.positions(), state.candidates(), new).await?;
    Ok(dto::CandidateRead::from(candidate))
}

/// `candidates_patch` — `position_id: null` detaches.
pub async fn candidates_patch(
    state: &DesktopState,
    id: &str,
    payload: &Value,
) -> Result<dto::CandidateRead, IpcError> {
    let raw: body::RawCandidatePatch = parse_body(payload)?;
    let patch = body::candidate_patch(raw)?;
    let candidate =
        ica_application::update_candidate(state.positions(), state.candidates(), id, patch).await?;
    Ok(dto::CandidateRead::from(candidate))
}

/// `candidates_delete` — FK cascade removes children.
pub async fn candidates_delete(state: &DesktopState, id: &str) -> Result<(), IpcError> {
    ica_application::delete_candidate(state.candidates(), id)
        .await
        .map_err(IpcError::from)
}

/// `candidate_timeline` — an array, not a `Page`.
pub async fn candidate_timeline(
    state: &DesktopState,
    candidate_id: &str,
    limit: Option<i64>,
) -> Result<Vec<dto::TimelineEntryRead>, IpcError> {
    let limit =
        query::typed_limit(limit, DEFAULT_MESSAGE_LIMIT, MAX_MESSAGE_LIMIT).map_err(fail)?;
    let entries = ica_application::candidate_timeline(
        state.candidates(),
        state.messages(),
        candidate_id,
        limit,
    )
    .await?;
    Ok(entries
        .into_iter()
        .map(dto::TimelineEntryRead::from)
        .collect())
}

// ── Conversations and messages ─────────────────────────────────────────────

/// Typed args for `conversations_list`.
#[derive(Debug, Default, Deserialize)]
pub struct ConversationListArgs {
    pub candidate_id: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `conversations_list` — `last_message_at DESC NULLS LAST`.
pub async fn conversations_list(
    state: &DesktopState,
    args: ConversationListArgs,
) -> Result<dto::PageResponse<dto::ConversationRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let query = query::validate_conversations(RawListConversationsQuery {
        candidate_id: args.candidate_id,
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let page = ica_application::list_conversations(
        state.conversations(),
        ica_application::ListConversations {
            candidate_id: query.candidate_id,
            offset,
            limit,
        },
    )
    .await?;
    Ok(dto::PageResponse::from_page(
        page,
        dto::ConversationRead::from,
    ))
}

/// `conversations_create` — open a thread and audit `conversation.created`.
pub async fn conversations_create(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::ConversationRead, IpcError> {
    let raw: body::RawConversationCreate = parse_body(payload)?;
    let new = body::conversation_create(raw)?;
    let conversation =
        ica_application::create_conversation(state.candidates(), state.conversations(), new)
            .await?;
    Ok(dto::ConversationRead::from(conversation))
}

/// Typed args for `messages_list`.
#[derive(Debug, Default, Deserialize)]
pub struct MessageListArgs {
    pub candidate_id: Option<String>,
    pub conversation_id: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `messages_list` — time order, real `total` (IMP-1).
pub async fn messages_list(
    state: &DesktopState,
    args: MessageListArgs,
) -> Result<dto::PageResponse<dto::MessageRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit =
        query::typed_limit(args.limit, DEFAULT_MESSAGE_LIMIT, MAX_MESSAGE_LIMIT).map_err(fail)?;
    let query = query::validate_messages(RawListMessagesQuery {
        candidate_id: args.candidate_id,
        conversation_id: args.conversation_id,
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let page = ica_application::list_messages(
        state.messages(),
        ica_application::ListMessages {
            candidate_id: query.candidate_id,
            conversation_id: query.conversation_id,
            offset,
            limit,
        },
    )
    .await?;
    Ok(dto::PageResponse::from_page(page, dto::MessageRead::from))
}

/// `messages_create` — record one, opening or reusing the conversation.
pub async fn messages_create(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::MessageRead, IpcError> {
    let raw: body::RawMessageCreate = parse_body(payload)?;
    let new = body::message_create(raw)?;
    let message = ica_application::create_message(
        state.candidates(),
        state.conversations(),
        state.messages(),
        new,
    )
    .await?;
    Ok(dto::MessageRead::from(message))
}

// ── Send records ───────────────────────────────────────────────────────────

/// Typed args for `send_records_list`.
#[derive(Debug, Default, Deserialize)]
pub struct SendRecordListArgs {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub status: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `send_records_list` — newest send first.
pub async fn send_records_list(
    state: &DesktopState,
    args: SendRecordListArgs,
) -> Result<dto::PageResponse<dto::SendRecordRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let query = query::validate_send_records(RawListSendRecordsQuery {
        candidate_id: args.candidate_id,
        task_id: args.task_id,
        draft_id: args.draft_id,
        status: args.status,
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let page = ica_application::list_send_records(
        state.send_records(),
        ica_application::ListSendRecords {
            candidate_id: query.candidate_id,
            task_id: query.task_id,
            draft_id: query.draft_id,
            status: query.status,
            offset,
            limit,
        },
    )
    .await?;
    Ok(dto::PageResponse::from_page(
        page,
        dto::SendRecordRead::from,
    ))
}

/// `send_records_create` — append one manual send, audited.
pub async fn send_records_create(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::SendRecordRead, IpcError> {
    let raw: body::RawSendRecordCreate = parse_body(payload)?;
    let new = body::send_record_create(raw)?;
    let record =
        ica_application::record_send(state.candidates(), state.send_records(), new).await?;
    Ok(dto::SendRecordRead::from(record))
}

// ── Follow-ups ─────────────────────────────────────────────────────────────

/// `follow_ups_evaluate` — the decision engine's answer.
pub async fn follow_ups_evaluate(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::FollowUpDecisionRead, IpcError> {
    let raw: body::RawFollowUpEvaluate = parse_body(payload)?;
    let evaluation = body::follow_up_evaluate(raw)?;
    let decision = ica_application::evaluate(
        state.follow_ups(),
        state.candidates(),
        state.positions(),
        evaluation,
    )
    .await?;
    Ok(dto::FollowUpDecisionRead::from(decision))
}

/// Typed args for `follow_ups_list`.
#[derive(Debug, Default, Deserialize)]
pub struct FollowUpListArgs {
    pub candidate_id: Option<String>,
    pub status: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `follow_ups_list` — stale rows swept first.
pub async fn follow_ups_list(
    state: &DesktopState,
    args: FollowUpListArgs,
) -> Result<dto::PageResponse<dto::FollowUpTaskRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let query = query::validate_follow_ups(RawListFollowUpsQuery {
        candidate_id: args.candidate_id,
        status: args.status,
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let (tasks, total) = ica_application::list_tasks(
        state.follow_ups(),
        ica_application::FollowUpTaskFilter {
            candidate_id: query.candidate_id,
            status: query.status,
        },
        offset,
        limit,
    )
    .await?;
    let page = ica_domain::Page {
        items: tasks,
        total,
        offset,
        limit,
    };
    Ok(dto::PageResponse::from_page(
        page,
        dto::FollowUpTaskRead::from,
    ))
}

/// `follow_ups_create` — create (or reuse) a task.
pub async fn follow_ups_create(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::FollowUpTaskRead, IpcError> {
    let raw: body::RawFollowUpCreate = parse_body(payload)?;
    let new = body::follow_up_create(raw)?;
    let task = ica_application::create_task(
        state.follow_ups(),
        state.candidates(),
        state.positions(),
        new,
    )
    .await?;
    Ok(dto::FollowUpTaskRead::from(task))
}

/// `follow_ups_get`.
pub async fn follow_ups_get(
    state: &DesktopState,
    task_id: &str,
) -> Result<dto::FollowUpTaskRead, IpcError> {
    let task = ica_application::get_task(state.follow_ups(), task_id).await?;
    Ok(dto::FollowUpTaskRead::from(task))
}

/// `follow_ups_patch` — the state machine validates any status change.
pub async fn follow_ups_patch(
    state: &DesktopState,
    task_id: &str,
    payload: &Value,
) -> Result<dto::FollowUpTaskRead, IpcError> {
    let raw: body::RawFollowUpPatch = parse_body(payload)?;
    let patch = body::follow_up_patch(raw)?;
    let task = ica_application::update_task(state.follow_ups(), task_id, patch).await?;
    Ok(dto::FollowUpTaskRead::from(task))
}

/// `follow_ups_cancel`.
pub async fn follow_ups_cancel(
    state: &DesktopState,
    task_id: &str,
    reason: Option<String>,
) -> Result<dto::FollowUpTaskRead, IpcError> {
    let task = ica_application::cancel_task(state.follow_ups(), task_id, reason).await?;
    Ok(dto::FollowUpTaskRead::from(task))
}

/// `follow_ups_complete`.
pub async fn follow_ups_complete(
    state: &DesktopState,
    task_id: &str,
) -> Result<dto::FollowUpTaskRead, IpcError> {
    let task = ica_application::complete_task(state.follow_ups(), task_id).await?;
    Ok(dto::FollowUpTaskRead::from(task))
}

/// `follow_ups_record_result` — the only path that marks a message as sent.
pub async fn follow_ups_record_result(
    state: &DesktopState,
    task_id: &str,
    payload: &Value,
) -> Result<dto::FollowUpTaskRead, IpcError> {
    let raw: body::RawFollowUpResult = parse_body(payload)?;
    let result = body::follow_up_result(raw)?;
    let task = ica_application::record_result(state.follow_ups(), task_id, result).await?;
    Ok(dto::FollowUpTaskRead::from(task))
}

/// `follow_ups_policy_get` — creates the default row on first use.
pub async fn follow_ups_policy_get(
    state: &DesktopState,
) -> Result<dto::FollowUpPolicyRead, IpcError> {
    let record = ica_application::ensure_default_policy(state.follow_ups()).await?;
    Ok(dto::FollowUpPolicyRead::from(record))
}

/// `follow_ups_policy_patch` — thresholds with the pydantic ranges.
pub async fn follow_ups_policy_patch(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::FollowUpPolicyRead, IpcError> {
    let raw: body::RawFollowUpPolicyPatch = parse_body(payload)?;
    let patch = body::follow_up_policy_patch(raw)?;
    let record = ica_application::update_policy(state.follow_ups(), patch).await?;
    Ok(dto::FollowUpPolicyRead::from(record))
}

// ── Dashboard ──────────────────────────────────────────────────────────────

/// `dashboard` — the aggregated workbench, after the maintenance tick.
pub async fn dashboard(state: &DesktopState) -> Result<dto::DashboardRead, IpcError> {
    use ica_domain::CandidateStatus;
    let snapshot = ica_application::dashboard(state.dashboard()).await?;
    let mut status_counts = std::collections::BTreeMap::new();
    for status in [
        CandidateStatus::New,
        CandidateStatus::Contacted,
        CandidateStatus::ReadNoReply,
        CandidateStatus::Replied,
        CandidateStatus::Active,
        CandidateStatus::Interview,
        CandidateStatus::Rejected,
        CandidateStatus::Withdrawn,
        CandidateStatus::Closed,
    ] {
        status_counts.insert(status.as_str().to_string(), 0);
    }
    for (status, count) in snapshot.candidate_status_counts {
        status_counts.insert(status, count);
    }
    Ok(dto::DashboardRead {
        candidate_total: snapshot.candidate_total,
        candidate_status_counts: status_counts,
        active_follow_up_count: snapshot.active_follow_up_count,
        pending_follow_ups: snapshot
            .pending_follow_ups
            .into_iter()
            .map(dto::FollowUpTaskRead::from)
            .collect(),
        awaiting_result_count: snapshot.awaiting_result_count,
        pending_draft_count: snapshot.pending_draft_count,
        recent_messages: snapshot
            .recent_messages
            .into_iter()
            .map(dto::MessageRead::from)
            .collect(),
        position_total: snapshot.position_total,
        open_position_count: snapshot.open_position_count,
    })
}

// ── Drafts and draft groups (W3) ───────────────────────────────────────────

/// Typed args for `drafts_list` (the `status` alias is the Python wire name).
#[derive(Debug, Default, Deserialize)]
pub struct DraftListArgs {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// `drafts_list` — one page, `created_at DESC`, true total.
pub async fn drafts_list(
    state: &DesktopState,
    args: DraftListArgs,
) -> Result<dto::PageResponse<dto::DraftRead>, IpcError> {
    let offset = query::typed_offset(args.offset).map_err(fail)?;
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let validated = query::validate_drafts(ica_contracts::query::RawListDraftsQuery {
        candidate_id: args.candidate_id,
        task_id: args.task_id,
        status: args.status,
        offset: None,
        limit: None,
    })
    .map_err(fail)?;
    let (drafts, total) =
        ica_application::list_drafts(state.drafts(), validated.filter, offset, limit).await?;
    let page = ica_domain::Page {
        items: drafts,
        total,
        offset,
        limit,
    };
    Ok(dto::PageResponse::from_page(page, dto::DraftRead::from))
}

/// `drafts_get` — one draft.
pub async fn drafts_get(state: &DesktopState, draft_id: &str) -> Result<dto::DraftRead, IpcError> {
    let draft = ica_application::get_draft(state.drafts(), draft_id).await?;
    Ok(dto::DraftRead::from(draft))
}

/// `drafts_generate` — the guard chain, then the AI port's 503.
pub async fn drafts_generate(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::DraftRead, IpcError> {
    let raw: body::RawGenerateRequest = parse_body(payload)?;
    let request = body::generate_request(raw).map_err(fail)?;
    match ica_application::generate(
        state.drafts(),
        state.follow_ups(),
        state.candidates(),
        state.positions(),
        state.draft_generator(),
        request,
    )
    .await?
    {
        ica_application::drafts::GenerateOutcome::Created(draft) => {
            Ok(dto::DraftRead::from(*draft))
        }
    }
}

/// `candidate_context` — exactly what the model would see.
pub async fn candidate_context(
    state: &DesktopState,
    candidate_id: &str,
) -> Result<dto::ContextPreviewRead, IpcError> {
    let context = ica_application::preview_context(
        state.candidates(),
        state.positions(),
        state.messages(),
        candidate_id,
    )
    .await?;
    Ok(dto::ContextPreviewRead::from(context))
}

/// `drafts_patch` — edit the wording (409 once approved/copied).
pub async fn drafts_patch(
    state: &DesktopState,
    draft_id: &str,
    payload: &Value,
) -> Result<dto::DraftRead, IpcError> {
    let raw: body::RawDraftPatch = parse_body(payload)?;
    let patch = body::draft_patch(raw).map_err(fail)?;
    let draft = ica_application::update_draft(state.drafts(), draft_id, patch).await?;
    Ok(dto::DraftRead::from(draft))
}

/// `drafts_approve` — approval only, never a send.
pub async fn drafts_approve(
    state: &DesktopState,
    draft_id: &str,
    payload: &Value,
) -> Result<dto::DraftRead, IpcError> {
    let raw: body::RawDraftApprove = parse_body(payload)?;
    let note = body::draft_approve(raw).map_err(fail)?;
    let draft =
        ica_application::approve_draft(state.drafts(), state.follow_ups(), draft_id, note).await?;
    Ok(dto::DraftRead::from(draft))
}

/// `drafts_reject` — no guard, every state answers success.
pub async fn drafts_reject(
    state: &DesktopState,
    draft_id: &str,
) -> Result<dto::DraftRead, IpcError> {
    let draft = ica_application::reject_draft(state.drafts(), state.follow_ups(), draft_id).await?;
    Ok(dto::DraftRead::from(draft))
}

/// `drafts_copy` — copy is not a send.
pub async fn drafts_copy(
    state: &DesktopState,
    draft_id: &str,
) -> Result<dto::DraftCopyResponse, IpcError> {
    let outcome = ica_application::copy_draft(state.drafts(), state.follow_ups(), draft_id).await?;
    Ok(dto::DraftCopyResponse::from(outcome))
}

/// `draft_groups_get` — the group, `created_at ASC`.
pub async fn draft_groups_get(
    state: &DesktopState,
    group_id: &str,
) -> Result<dto::DraftGroupRead, IpcError> {
    let drafts = ica_application::get_draft_group(state.drafts(), group_id).await?;
    Ok(build_draft_group(drafts))
}

/// `draft_groups_select` — pick one, reject the rest.
pub async fn draft_groups_select(
    state: &DesktopState,
    group_id: &str,
    payload: &Value,
) -> Result<dto::DraftGroupRead, IpcError> {
    let raw: body::RawDraftGroupSelect = parse_body(payload)?;
    let selection = body::draft_group_select(raw).map_err(fail)?;
    let drafts = ica_application::select_draft_from_group(
        state.drafts(),
        state.follow_ups(),
        group_id,
        selection,
    )
    .await?;
    Ok(build_draft_group(drafts))
}

/// `_build_draft_group`, shared with the HTTP adapter's mapping.
fn build_draft_group(drafts: Vec<ica_domain::GeneratedDraft>) -> dto::DraftGroupRead {
    let draft_group_id = drafts
        .first()
        .and_then(|draft| draft.draft_group_id.clone())
        .unwrap_or_default();
    let created_at = drafts.iter().map(|draft| draft.created_at).max();
    dto::DraftGroupRead {
        draft_group_id,
        created_at,
        drafts: drafts.into_iter().map(dto::DraftRead::from).collect(),
    }
}

// ── W4: imports, export, import, OCR ────────────────────────────────────────

/// `imports_preview` — parse pasted text into an import session.
///
/// The session and its records are persisted; no message is written — that is
/// `imports_confirm`'s job.
pub async fn imports_preview(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::ImportSessionRead, IpcError> {
    let raw: body::RawImportPreview = parse_body(payload)?;
    let request = body::import_preview(raw).map_err(fail)?;
    let session = ica_application::preview_import(state.imports(), request).await?;
    Ok(dto::ImportSessionRead::from(session))
}

/// `imports_get` — one session with its parsed records.
pub async fn imports_get(
    state: &DesktopState,
    session_id: &str,
) -> Result<dto::ImportSessionRead, IpcError> {
    let session = ica_application::get_import(state.imports(), session_id).await?;
    Ok(dto::ImportSessionRead::from(session))
}

/// `imports_confirm` — write the parsed records into the timeline.
pub async fn imports_confirm(
    state: &DesktopState,
    session_id: &str,
    payload: &Value,
) -> Result<dto::ImportConfirmResult, IpcError> {
    let raw: body::RawImportConfirm = parse_body(payload)?;
    let request = body::import_confirm(raw).map_err(fail)?;
    let outcome = ica_application::confirm_import(state.imports(), session_id, request).await?;
    Ok(dto::ImportConfirmResult {
        session_id: outcome.session_id,
        saved_message_ids: outcome.saved_message_ids,
        skipped_count: outcome.skipped_count,
        duplicate_count: outcome.duplicate_count,
    })
}

/// `imports_reject` — close a session without importing.
pub async fn imports_reject(
    state: &DesktopState,
    session_id: &str,
) -> Result<dto::ImportSessionRead, IpcError> {
    let session = ica_application::reject_import(state.imports(), session_id).await?;
    Ok(dto::ImportSessionRead::from(session))
}

/// `export_data` — the whole snapshot, optionally sealed.
///
/// `encrypt_key` is the base64 AES-256 key the caller wants the snapshot
/// sealed with; `None` returns the plaintext document. The key is never logged
/// or echoed, and it is never persisted.
pub async fn export_data(
    state: &DesktopState,
    encrypt_key: Option<&str>,
) -> Result<Value, IpcError> {
    let outcome =
        ica_application::export_data(state.snapshots(), state.export_sealer(), encrypt_key).await?;

    Ok(match outcome {
        ica_application::sync::ExportOutcome::Plain(snapshot) => serde_json::json!({
            "version": snapshot.version,
            "exported_at": snapshot.exported_at,
            "data": snapshot.data,
        }),
        ica_application::sync::ExportOutcome::Sealed(envelope) => serde_json::json!({
            // The sealer is AES-256-GCM by contract, so the marker is fixed.
            "alg": "A256GCM",
            "iv": envelope.iv,
            "tag": envelope.tag,
            "ciphertext": envelope.ciphertext,
        }),
    })
}

/// `import_data` — merge a snapshot, last-write-wins per row.
pub async fn import_data(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::ImportResult, IpcError> {
    let raw: body::RawImportPayload = parse_body(payload)?;
    let data = body::import_payload(raw).map_err(fail)?;
    let outcome = ica_application::import_data(state.snapshots(), data).await?;
    Ok(dto::ImportResult {
        imported: outcome.imported,
        updated: outcome.updated,
        skipped: outcome.skipped,
        conflicts: outcome.conflicts,
    })
}

/// `imports_ocr` — read a screenshot from the staging directory.
///
/// The path guard runs before any I/O, so a path outside the staging area is
/// refused without disclosing whether the file exists.
pub async fn imports_ocr(
    state: &DesktopState,
    payload: &Value,
) -> Result<dto::ImportOcrResult, IpcError> {
    let raw: body::RawImportOcr = parse_body(payload)?;
    let request = body::import_ocr(raw).map_err(fail)?;
    let candidate_id = request.candidate_id.clone();
    let outcome = ica_application::ocr_screenshot(
        state.imports(),
        state.ocr(),
        request,
        &state.config().ocr_upload_dir,
    )
    .await?;
    Ok(dto::ImportOcrResult {
        session_id: outcome.session.id,
        text: outcome.text,
        language: outcome.language,
        candidate_id,
        created_at: outcome.session.created_at,
    })
}
