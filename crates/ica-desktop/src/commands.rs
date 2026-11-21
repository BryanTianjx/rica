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
        calendar: dto::DashboardCalendar {
            month: snapshot.calendar_month,
            messages_by_day: snapshot.calendar_messages_by_day.into_iter().collect(),
            followups_by_day: snapshot.calendar_followups_by_day.into_iter().collect(),
        },
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

/// `imports_confirm_candidate` — write a REVIEWED browser extraction draft
/// into the candidate archive (the only path from a page into `candidates`).
pub async fn imports_confirm_candidate(
    state: &DesktopState,
    session_id: &str,
    payload: &Value,
) -> Result<Value, IpcError> {
    let raw: body::RawCandidateConfirm = parse_body(payload)?;
    let request = body::candidate_confirm(raw).map_err(fail)?;
    let outcome = ica_application::confirm_candidate(
        state.imports(),
        state.extractions(),
        session_id,
        ica_application::CandidateConfirm {
            position_id: request.position_id,
            status: request.status,
            confirm_duplicate: request.confirm_duplicate,
            note: request.note,
        },
    )
    .await?;
    Ok(serde_json::json!({
        "session_id": outcome.session_id,
        "extraction_id": outcome.extraction_id,
        "candidate_id": outcome.candidate_id,
        "duplicate_kind": outcome.duplicate_kind,
        "duplicate_override": outcome.duplicate_override,
        "fields_written": outcome.fields_written,
    }))
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

// ── W5 (first slice): audit events ─────────────────────────────────────────

/// `audit_events_list` — the audit trail, newest first.
///
/// The IPC layer mirrors the HTTP query: the filters are typed, and the
/// empty-string-means-absent rule lives in the shared `ica-contracts`
/// validator, so both transports behave identically.
#[derive(Debug, Default, serde::Deserialize)]
pub struct AuditEventsListArgs {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub limit: Option<i64>,
}

pub async fn audit_events_list(
    state: &DesktopState,
    args: AuditEventsListArgs,
) -> Result<Vec<dto::AuditEventRead>, IpcError> {
    let limit = query::typed_limit(args.limit, DEFAULT_LIMIT, MAX_LIMIT).map_err(fail)?;
    let list = query::validate_audit_events(query::RawListAuditEventsQuery {
        event_type: args.event_type,
        entity_type: args.entity_type,
        entity_id: args.entity_id,
        limit: None,
    })
    .map_err(fail)?;
    let events = ica_application::recent_audit_events(
        state.audit_trail(),
        query::ListAuditEvents {
            event_type: list.event_type,
            entity_type: list.entity_type,
            entity_id: list.entity_id,
            limit,
        }
        .into(),
    )
    .await?;
    Ok(events.into_iter().map(dto::AuditEventRead::from).collect())
}

// ── W5 settings slice ──────────────────────────────────────────────────────

/// `settings_read` — the effective configuration, minus any secret.
pub async fn settings_read(state: &DesktopState) -> Result<dto::SettingsRead, IpcError> {
    let snapshot = state.settings_config_view().settings_snapshot();
    Ok(dto::SettingsRead {
        app_name: snapshot.app_name,
        ai_enabled: snapshot.ai_enabled,
        ai_provider: snapshot.ai_provider,
        ai_model: snapshot.ai_model,
        ai_base_url: snapshot.ai_base_url,
        database_url: snapshot.database_url,
        backup_dir: snapshot.backup_dir,
        backup_keep_count: snapshot.backup_keep_count,
        data_retention_days: snapshot.data_retention_days,
    })
}

/// `ai_egress_read` — where AI context would go and what is masked.
pub async fn ai_egress_read(state: &DesktopState) -> Result<dto::AIEgressRead, IpcError> {
    let disclosure =
        ica_application::read_egress_disclosure(state.settings(), &state.settings_config_view())
            .await?;
    Ok(disclosure.into())
}

/// Typed args of `ai_egress_consent`: the switch is required and explicit,
/// mirroring the HTTP body contract.
#[derive(Debug, Default, serde::Deserialize)]
pub struct AiEgressConsentArgs {
    pub consent_granted: Option<bool>,
}

/// `ai_egress_consent` — persist the switch (audited in the same
/// transaction) and return the fresh disclosure.
pub async fn ai_egress_consent(
    state: &DesktopState,
    args: AiEgressConsentArgs,
) -> Result<dto::AIEgressRead, IpcError> {
    let consent = body::ai_egress_consent(body::RawAiEgressConsent {
        consent_granted: match args.consent_granted {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    let disclosure = ica_application::write_egress_consent(
        state.settings(),
        &state.settings_config_view(),
        consent.consent_granted,
    )
    .await?;
    Ok(disclosure.into())
}

// == W5 AI slice ==

/// `ai_models_list` — the registry with the V1.0 fallback, key presence only.
pub async fn ai_models_list(state: &DesktopState) -> Result<dto::AiModelsRead, IpcError> {
    let overview =
        ica_application::list_ai_models(state.ai_models(), &state.ai_settings_view()).await?;
    Ok(dto::AiModelsRead {
        models: overview.models.into_iter().map(Into::into).collect(),
        routing: overview.routing,
    })
}

/// `ai_routing_get` — the effective table (defaults layered with overrides).
pub async fn ai_routing_get(state: &DesktopState) -> Result<dto::AiRoutingRead, IpcError> {
    let routing = ica_application::get_ai_routing(state.ai_models()).await?;
    Ok(dto::AiRoutingRead { routing })
}

/// Typed args of `ai_routing_patch`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct AiRoutingPatchArgs {
    pub routing: Option<std::collections::BTreeMap<String, String>>,
}

/// `ai_routing_patch` — persist overrides for known message types only.
pub async fn ai_routing_patch(
    state: &DesktopState,
    args: AiRoutingPatchArgs,
) -> Result<dto::AiRoutingRead, IpcError> {
    let patch = body::ai_routing_patch(body::RawAiRoutingPatch {
        routing: match args.routing {
            Some(map) => body::Incoming::Value(map),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    let routing = ica_application::update_ai_routing(state.ai_models(), patch.routing).await?;
    Ok(dto::AiRoutingRead { routing })
}

/// Typed args of `ai_usage`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct AiUsageArgs {
    pub days: Option<i64>,
}

/// `ai_usage` — today, `days` of history, all-time per model.
pub async fn ai_usage(
    state: &DesktopState,
    args: AiUsageArgs,
) -> Result<dto::AiUsageRead, IpcError> {
    let raw = query::RawAiUsageQuery {
        days: args.days.map(|value| value.to_string()),
    };
    let days = query::validate_ai_usage_days(raw).map_err(fail)?;
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    let overview =
        ica_application::get_ai_usage(state.ai_usage(), &state.ai_budget(), days, &today).await?;
    Ok(overview.into())
}

/// `ai_budget_get` — the current budget config and spend state.
pub async fn ai_budget_get(state: &DesktopState) -> Result<dto::AiBudgetRead, IpcError> {
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    let spend = ica_application::today_spend_cents(state.ai_usage(), &today).await?;
    Ok(ica_application::budget_read(&state.ai_budget(), spend).into())
}

/// Typed args of `ai_budget_patch` (both fields required, like the body).
#[derive(Debug, Default, serde::Deserialize)]
pub struct AiBudgetPatchArgs {
    pub daily_budget_cents: Option<i64>,
    pub warning_threshold: Option<i64>,
}

/// `ai_budget_patch` — persist rows, mutate the in-process pair, answer with
/// the fresh spend state.
pub async fn ai_budget_patch(
    state: &DesktopState,
    args: AiBudgetPatchArgs,
) -> Result<dto::AiBudgetRead, IpcError> {
    let patch = body::ai_budget_patch(body::RawAiBudgetPatch {
        daily_budget_cents: match args.daily_budget_cents {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        warning_threshold: match args.warning_threshold {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    state
        .settings()
        .upsert_values(&[
            (
                "ai_daily_budget_cents",
                patch.daily_budget_cents.to_string(),
                "int",
            ),
            (
                "ai_budget_warning_threshold",
                patch.warning_threshold.to_string(),
                "int",
            ),
        ])
        .await
        .map_err(|error| IpcError::from(error.application_error()))?;
    state.set_ai_budget(ica_application::AiBudgetSettings {
        daily_budget_cents: patch.daily_budget_cents,
        warning_threshold: patch.warning_threshold,
    });
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    let spend = ica_application::today_spend_cents(state.ai_usage(), &today).await?;
    Ok(ica_application::budget_read(&state.ai_budget(), spend).into())
}

// == W5 ai-key + backups ==

/// Typed args of `ai_key_write` (the key is write-only, never echoed back).
#[derive(Debug, Default, serde::Deserialize)]
pub struct AiKeyWriteArgs {
    pub ai_api_key: Option<String>,
    pub model_id: Option<String>,
}

/// `ai_key_write` — persist through the OS credential store, audit masked.
pub async fn ai_key_write(
    state: &DesktopState,
    args: AiKeyWriteArgs,
) -> Result<dto::AiKeyStatus, IpcError> {
    let write = body::ai_key_write(body::RawAiKeyWrite {
        ai_api_key: match args.ai_api_key {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        model_id: match args.model_id {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    let (installation, masked, model_id) = ica_application::write_ai_key(
        state.secrets(),
        state.settings(),
        &write.ai_api_key,
        write.model_id.as_deref(),
    )
    .await?;
    if installation {
        state.set_ai_key_present(true);
    }
    Ok(dto::AiKeyStatus {
        ai_enabled: state.ai_key_present(),
        key_masked: masked,
        model_id,
    })
}

/// Typed args of `ai_key_clear`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct AiKeyClearArgs {
    pub model_id: Option<String>,
}

/// `ai_key_clear` — clear the chosen slot and audit the change.
pub async fn ai_key_clear(
    state: &DesktopState,
    args: AiKeyClearArgs,
) -> Result<dto::AiKeyStatus, IpcError> {
    let model_id = query::validate_ai_key_clear(query::RawAiKeyClearQuery {
        model_id: args.model_id,
    })
    .map_err(fail)?;
    let (installation, masked, model_id) =
        ica_application::clear_ai_key(state.secrets(), state.settings(), model_id.as_deref())
            .await?;
    if installation && model_id.is_none() {
        state.set_ai_key_present(false);
    }
    Ok(dto::AiKeyStatus {
        ai_enabled: state.ai_key_present(),
        key_masked: masked,
        model_id,
    })
}

/// `backups_list` — both backup families, newest first.
pub async fn backups_list(state: &DesktopState) -> Result<Vec<dto::BackupRead>, IpcError> {
    let records = ica_application::list_backups(state.backups()).await?;
    Ok(records.into_iter().map(Into::into).collect())
}

/// `backups_create` — one consistent online-backup copy.
pub async fn backups_create(state: &DesktopState) -> Result<dto::BackupRead, IpcError> {
    let record = ica_application::create_backup(state.backups()).await?;
    Ok(record.into())
}

/// Typed args of `backups_restore` (`confirm` must be explicit).
#[derive(Debug, Default, serde::Deserialize)]
pub struct BackupsRestoreArgs {
    pub name: Option<String>,
    pub confirm: Option<bool>,
}

/// `backups_restore` — replace the live data with the backup's contents.
pub async fn backups_restore(
    state: &DesktopState,
    args: BackupsRestoreArgs,
) -> Result<dto::RestoreRead, IpcError> {
    let name = args.name.unwrap_or_default();
    if !args.confirm.unwrap_or(false) {
        return Err(IpcError::from(ica_contracts::error::ContractError::Conflict {
            message: "Restoring a backup replaces the current data. Send confirm=true once you are sure.".to_string(),
            details: serde_json::json!({ "backup": name }),
        }));
    }
    if name.contains('/') || name.contains('\\') || name.is_empty() || name == "." || name == ".." {
        return Err(IpcError::from(
            ica_contracts::error::ContractError::Validation {
                message: "A backup is addressed by its file name, not by a path.".to_string(),
                details: serde_json::json!({ "backup": name }),
            },
        ));
    }
    let outcome = ica_application::restore_backup(state.backups(), &name).await?;
    Ok(dto::RestoreRead {
        restored_from: outcome.restored_from,
        database: outcome.database,
        previous_copy: outcome.previous_copy,
        message: "The database was restored. The previous state was kept as a safety copy next to the backup.".to_string(),
    })
}

// == W6 auth + shares ==

/// `auth_register` — first sign-up bootstraps the tenant as admin.
#[derive(Debug, Default, serde::Deserialize)]
pub struct AuthRegisterArgs {
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<String>,
    pub tenant_id: Option<String>,
}

pub async fn auth_register(
    state: &DesktopState,
    args: AuthRegisterArgs,
) -> Result<dto::UserRead, IpcError> {
    let write = body::auth_register(body::RawAuthRegister {
        email: match args.email {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        password: match args.password {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        display_name: match args.display_name {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        tenant_id: match args.tenant_id {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    let user = ica_application::register(
        state.auth_repo(),
        state.auth_security(),
        &write.email,
        &write.password,
        write.display_name.as_deref(),
        write.tenant_id.as_deref(),
    )
    .await?;
    Ok(dto::UserRead::from(&user))
}

/// `auth_login` — verify credentials, issue the pair.
pub async fn auth_login(
    state: &DesktopState,
    args: AuthRegisterArgs,
) -> Result<dto::TokenResponse, IpcError> {
    let login = body::auth_login(body::RawAuthLogin {
        email: match args.email {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        password: match args.password {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    let issued = ica_application::login(
        state.auth_repo(),
        state.auth_security(),
        &login.email,
        &login.password,
    )
    .await?;
    Ok(dto::TokenResponse {
        access_token: issued.access_token,
        token_type: "bearer".to_string(),
        expires_in: issued.expires_in,
        refresh_token: issued.refresh_token,
        user: dto::UserRead::from(&issued.user),
    })
}

/// `auth_refresh` — single-use rotation.
pub async fn auth_refresh(
    state: &DesktopState,
    refresh_token: String,
) -> Result<dto::TokenResponse, IpcError> {
    let token = body::auth_refresh(body::RawAuthRefresh {
        refresh_token: body::Incoming::Value(refresh_token),
    })
    .map_err(fail)?;
    let issued = ica_application::refresh(state.auth_repo(), state.auth_security(), &token).await?;
    Ok(dto::TokenResponse {
        access_token: issued.access_token,
        token_type: "bearer".to_string(),
        expires_in: issued.expires_in,
        refresh_token: issued.refresh_token,
        user: dto::UserRead::from(&issued.user),
    })
}

/// `auth_logout` — revoke one refresh token (idempotent, 204 semantics).
pub async fn auth_logout(state: &DesktopState, refresh_token: String) -> Result<(), IpcError> {
    let token = body::auth_refresh(body::RawAuthRefresh {
        refresh_token: body::Incoming::Value(refresh_token),
    })
    .map_err(fail)?;
    ica_application::logout(state.auth_repo(), state.auth_security(), &token).await?;
    Ok(())
}

/// `auth_me` — the current user (local mode answers the synthetic one).
pub async fn auth_me() -> dto::UserRead {
    dto::UserRead::from(&ica_application::local_user())
}

/// Typed args of `shares_create`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ShareCreateArgs {
    pub recipient_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub permission: Option<String>,
}

/// `shares_create` — grant (or upgrade) one recipient's access.
pub async fn shares_create(
    state: &DesktopState,
    args: ShareCreateArgs,
) -> Result<dto::ShareRead, IpcError> {
    let create = body::share_create(body::RawShareCreate {
        recipient_id: match args.recipient_id {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        resource_type: match args.resource_type {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        resource_id: match args.resource_id {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
        permission: match args.permission {
            Some(value) => body::Incoming::Value(value),
            None => body::Incoming::Absent,
        },
    })
    .map_err(fail)?;
    let local = ica_application::local_user();
    let share = ica_application::create_share(
        state.auth_repo(),
        &local.id,
        &create.recipient_id,
        &create.resource_type,
        &create.resource_id,
        &create.permission,
    )
    .await?;
    Ok(dto::ShareRead::from(&share))
}

/// `shares_list` — the local user's shares, newest first.
pub async fn shares_list(state: &DesktopState) -> Result<Vec<dto::ShareRead>, IpcError> {
    let local = ica_application::local_user();
    let shares = ica_application::list_shares(state.auth_repo(), &local.id).await?;
    Ok(shares.iter().map(dto::ShareRead::from).collect())
}

/// `shares_delete` — owner or admin (the local user is admin).
pub async fn shares_delete(state: &DesktopState, share_id: String) -> Result<(), IpcError> {
    let local = ica_application::local_user();
    ica_application::delete_share(state.auth_repo(), &share_id, &local.id, true).await?;
    Ok(())
}
